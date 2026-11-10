//! 真实 Git 输出与子进程退出的受控交错；所有文件、HOME 和配置属于本例 fixture。
#![cfg(unix)]

#[path = "support/git_environment.rs"]
mod git_environment;

use revia_lib::git::{CommandLog, Git, GitOutputEvent};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, mpsc};
use std::time::Duration;

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    git: Git,
    expected: Vec<u8>,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let repo = root.join("repo");
        let home = root.join("home");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(&home).unwrap();
        let config = home.join("gitconfig");
        fs::write(&config, "").unwrap();
        let env = git_environment::shell_env(&home, &repo, &config);
        let git = Git::new(&repo, &env, Arc::new(CommandLog::default())).unwrap();
        let init = Command::new(&git.program)
            .current_dir(&repo)
            .env_clear()
            .envs(git.env_snapshot())
            .args(["init", "-b", "main"])
            .output()
            .unwrap();
        assert!(init.status.success(), "{init:?}");
        // 独立 Command 输出证明该真实 Git 命令产生的完整路径，避免用被测收集器算期望。
        let expected = Command::new(&git.program)
            .current_dir(&repo)
            .env_clear()
            .envs(git.env_snapshot())
            .args([
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "sequencer",
            ])
            .output()
            .unwrap();
        assert!(expected.status.success(), "{expected:?}");
        assert!(!expected.stdout.is_empty());
        Self {
            _temp: temp,
            root,
            git,
            expected: expected.stdout,
        }
    }
}

fn output_arriving_after_idle_read_is_retained(event: GitOutputEvent) {
    let fixture = Fixture::new();
    let fifo = fixture.root.join("release-git.fifo");
    let mkfifo = Command::new("mkfifo").arg(&fifo).output().unwrap();
    assert!(mkfifo.status.success(), "{mkfifo:?}");
    let git = fixture.git.with_env_vars(&[
        ("REVIA_TEST_RELEASE_FIFO", fifo.to_str().unwrap()),
        ("REVIA_TEST_GIT", fixture.git.program.to_str().unwrap()),
    ]);
    // FIFO 只控制何时执行真实 Git；数据来自 rev-parse，不伪造 Git 响应。
    let redirect = if event == GitOutputEvent::StderrIdle {
        " >&2"
    } else {
        ""
    };
    let alias = format!(
        "alias.controlled-path=!f() {{ IFS= read -r release < \"$REVIA_TEST_RELEASE_FIFO\"; \
         exec \"$REVIA_TEST_GIT\" rev-parse --path-format=absolute --git-path sequencer{redirect}; }}; f"
    );
    let once = AtomicBool::new(false);
    let release_read = Arc::new(Barrier::new(2));
    let reader_barrier = Arc::clone(&release_read);
    let (idle_tx, idle_rx) = mpsc::channel();
    let (exit_tx, exit_rx) = mpsc::channel();
    let observer = Arc::new(move |observed| {
        if observed == event && !once.swap(true, Ordering::AcqRel) {
            idle_tx.send(()).unwrap();
            reader_barrier.wait();
        } else if observed == GitOutputEvent::ProcessExited {
            let _ = exit_tx.send(());
        }
    });
    let worker = std::thread::spawn(move || {
        git.run_with_output_observer(
            &["-c", &alias, "controlled-path"],
            Duration::from_secs(10),
            observer,
        )
    });
    idle_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("reader reached a real idle read");
    // 在辅助线程中打开 FIFO，异常时主线程仍能释放 reader 并报告真实退出状态。
    let (written_tx, written_rx) = mpsc::channel();
    let writer = std::thread::spawn(move || {
        let _ = written_tx.send(fs::write(fifo, "go\n"));
    });
    let exited = exit_rx.recv_timeout(Duration::from_secs(10));
    release_read.wait();
    let output = worker.join().unwrap().unwrap();
    exited.expect("real Git finished before the paused reader resumed");
    written_rx
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    writer.join().unwrap();
    assert_eq!(output.exit_code, 0, "{output:?}");
    assert!(!output.timed_out, "{output:?}");
    let records = fixture.git.log.snapshot();
    eprintln!(
        "expected path: {:?}; collected: {output:?}; command records: {records:?}",
        String::from_utf8_lossy(&fixture.expected)
    );
    if event == GitOutputEvent::StdoutIdle {
        assert_eq!(
            output.stdout, fixture.expected,
            "successful Git output was lost: {records:?}"
        );
    } else {
        assert_eq!(
            output.stderr.as_bytes(),
            fixture.expected,
            "successful Git stderr was lost: {records:?}"
        );
    }
}

#[test]
fn stdout_written_before_exit_survives_an_earlier_idle_read() {
    output_arriving_after_idle_read_is_retained(GitOutputEvent::StdoutIdle);
}

#[test]
fn stderr_written_before_exit_survives_an_earlier_idle_read() {
    output_arriving_after_idle_read_is_retained(GitOutputEvent::StderrIdle);
}

#[test]
fn inherited_writer_does_not_hold_git_output_after_exit() {
    let fixture = Fixture::new();
    let git = fixture
        .git
        .with_env_vars(&[("REVIA_TEST_GIT", fixture.git.program.to_str().unwrap())]);
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        // 有限后代保留输出描述符；测试失败时也会自行结束，不杀外部进程。
        tx.send(git.run_with_timeout(&[
            "-c", "alias.retained-writer=!f() { (sleep 3) & exec \"$REVIA_TEST_GIT\" rev-parse --path-format=absolute --git-path sequencer; }; f",
            "retained-writer",
        ], Duration::from_secs(10))).unwrap();
    });
    let result = rx.recv_timeout(Duration::from_secs(2));
    worker.join().unwrap();
    let output = result
        .expect("Git exited but an inherited writer kept output collection waiting")
        .unwrap();
    assert_eq!(output.exit_code, 0, "{output:?}");
    assert_eq!(output.stdout, fixture.expected);
}
