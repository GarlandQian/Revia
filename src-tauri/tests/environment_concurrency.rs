//! M9 Q222：打开 worktree 时完整环境后台加载的回归测试。
//!
//! 全部经过生产入口 `AppState::worktree_session` 与会话的 `acquire_write_lock`。
//! 完整环境由替身解析器提供，不运行用户真实的登录 shell / PowerShell；
//! 临时仓库与应用数据目录都由 tempfile 独占分配。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, Instant};

use revia_lib::commands::{AppState, ShellResolver};
use revia_lib::environment::EnvCoordinator;
use revia_lib::shell::ShellEnv;

const MARKER: &str = "REVIA_TEST_MARKER";

fn make_repo(parent: &Path, name: &str) -> PathBuf {
    let repo = parent.join(name);
    fs::create_dir_all(&repo).unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.name", "Test"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "commit.gpgsign", "false"],
    ] {
        assert!(Command::new("git").args(&args).current_dir(&repo).status().unwrap().success());
    }
    fs::canonicalize(repo).unwrap()
}

/// 测试进程 PATH 中的 git（测试本身也用它建仓库）。
fn real_git(any_dir: &Path) -> PathBuf {
    ShellEnv::minimal(any_dir).which("git").expect("git on PATH")
}

/// 替身解析器给出的“完整环境”：PATH 只含真实 git 所在目录，并带一个标记变量。
fn full_env(path: &Path, marker: &str) -> ShellEnv {
    let mut env = ShellEnv::minimal(path);
    let git_dir = real_git(path).parent().unwrap().display().to_string();
    env.path = vec![git_dir.clone()];
    env.vars.insert("PATH".into(), git_dir);
    env.vars.insert(MARKER.into(), marker.into());
    env.info.used_fallback = false;
    env.info.fallback_reason = None;
    env
}

/// 预先写一条“上次运行记住的 Git”。
fn remember(data: &Path, repo: &Path, git_path: &Path) {
    let git_dir = real_git(repo).parent().unwrap().display().to_string();
    let mut memory = HashMap::new();
    memory.insert(
        repo.display().to_string(),
        serde_json::json!({
            "gitPath": git_path.display().to_string(),
            "shell": "test",
            "pathEntries": [git_dir],
            "capturedAtMs": 0,
        }),
    );
    fs::write(data.join("env-memory.json"), serde_json::to_vec(&memory).unwrap()).unwrap();
}

fn register(state: &AppState, repo: &Path) -> String {
    let entry = state.registry.lock().unwrap().add(repo.to_str().unwrap()).unwrap();
    entry
        .worktrees
        .first()
        .map(|worktree| worktree.id.clone())
        .unwrap_or_else(|| entry.id.clone())
}

fn app(data: &Path, repo: &Path, resolver: ShellResolver) -> (AppState, String) {
    let state = AppState::new(data.to_path_buf()).with_shell_resolver(resolver);
    let id = register(&state, repo);
    (state, id)
}

fn marker_of(env: &ShellEnv) -> Option<String> {
    env.vars.get(MARKER).cloned()
}

fn wait_until(deadline: Duration, condition: impl Fn() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    condition()
}

fn counting_resolver(calls: &Arc<AtomicUsize>) -> ShellResolver {
    let counter = Arc::clone(calls);
    Arc::new(move |path| {
        counter.fetch_add(1, Ordering::SeqCst);
        full_env(path, "full")
    })
}

/// S2：记住的 Git 已被删除——打开不失败，只同步解析一次完整环境，并更新记忆。
#[test]
fn remembered_missing_git_falls_back_to_single_capture() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path(), "repo");
    let data = dir.path().join("data");
    fs::create_dir_all(&data).unwrap();
    remember(&data, &repo, &dir.path().join("deleted-git"));

    let calls = Arc::new(AtomicUsize::new(0));
    let (state, id) = app(&data, &repo, counting_resolver(&calls));

    let session = state.worktree_session(&id).expect("open falls back instead of failing");
    assert_eq!(session.git.program, real_git(&repo));
    assert_eq!(marker_of(&session.shell_env_snapshot()).as_deref(), Some("full"));
    // 不能再在后台重复跑一次登录 shell。
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let memory = EnvCoordinator::new(data.join("env-memory.json"));
    assert_eq!(memory.provisional(&repo).git_path, Some(real_git(&repo)));
}

/// S2：记住的 Git 版本低于下限——同样丢弃记忆，退回同步解析。
#[cfg(unix)]
#[test]
fn remembered_low_version_git_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path(), "repo");
    let data = dir.path().join("data");
    fs::create_dir_all(&data).unwrap();
    let low_git = dir.path().join("low-git");
    fs::write(&low_git, "#!/bin/sh\necho 'git version 2.10.0'\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&low_git, fs::Permissions::from_mode(0o755)).unwrap();
    }
    remember(&data, &repo, &low_git);

    let calls = Arc::new(AtomicUsize::new(0));
    let (state, id) = app(&data, &repo, counting_resolver(&calls));

    let session = state.worktree_session(&id).unwrap();
    assert_eq!(session.git.program, real_git(&repo));
    assert!(session.git_version_ok());
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

/// Q222/S9：有可用记忆时立即打开；写操作在 `acquire_write_lock` 里等完整环境装好后才开始。
#[test]
fn writes_wait_until_full_environment_is_installed() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path(), "repo");
    let data = dir.path().join("data");
    fs::create_dir_all(&data).unwrap();
    remember(&data, &repo, &real_git(&repo));

    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let (entered_r, release_r) = (Arc::clone(&entered), Arc::clone(&release));
    let resolver: ShellResolver = Arc::new(move |path| {
        entered_r.wait();
        release_r.wait();
        full_env(path, "full")
    });
    let (state, id) = app(&data, &repo, resolver);

    let session = state.worktree_session(&id).expect("opens with remembered git");
    assert_ne!(marker_of(&session.shell_env_snapshot()).as_deref(), Some("full"));
    entered.wait(); // 后台解析已经开始

    let acquired = Arc::new(AtomicBool::new(false));
    let seen = Arc::new(Mutex::new(None));
    let writer = {
        let session = Arc::clone(&session);
        let acquired = Arc::clone(&acquired);
        let seen = Arc::clone(&seen);
        std::thread::spawn(move || {
            let guard = session.acquire_write_lock().expect("write lock after env ready");
            acquired.store(true, Ordering::SeqCst);
            *seen.lock().unwrap() = marker_of(&session.shell_env_snapshot());
            drop(guard);
        })
    };
    std::thread::sleep(Duration::from_millis(300));
    assert!(!acquired.load(Ordering::SeqCst), "write started before the full environment");

    release.wait();
    writer.join().unwrap();
    assert_eq!(seen.lock().unwrap().as_deref(), Some("full"));
}

/// 重新加载环境后，旧的后台结果不能装进新会话。
#[test]
fn late_capture_after_reload_is_discarded() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path(), "repo");
    let data = dir.path().join("data");
    fs::create_dir_all(&data).unwrap();
    remember(&data, &repo, &real_git(&repo));

    let calls = Arc::new(AtomicUsize::new(0));
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let (counter, entered_r, release_r) =
        (Arc::clone(&calls), Arc::clone(&entered), Arc::clone(&release));
    let resolver: ShellResolver = Arc::new(move |path| {
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            entered_r.wait();
            release_r.wait();
            full_env(path, "stale")
        } else {
            full_env(path, "fresh")
        }
    });
    let (state, id) = app(&data, &repo, resolver);

    let first = state.worktree_session(&id).unwrap();
    entered.wait();
    state.reload_sessions().unwrap();
    // 重新加载清掉了记忆：再次打开同步解析出新的环境。
    let second = state.worktree_session(&id).unwrap();
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(marker_of(&second.shell_env_snapshot()).as_deref(), Some("fresh"));

    release.wait();
    assert!(wait_until(Duration::from_secs(2), || calls.load(Ordering::SeqCst) == 2));
    std::thread::sleep(Duration::from_millis(300));
    let current = state.worktree_session(&id).unwrap();
    assert!(Arc::ptr_eq(&current, &second));
    assert_eq!(marker_of(&current.shell_env_snapshot()).as_deref(), Some("fresh"));
}

/// 两个 worktree 同时打开，各自的完整环境只装进自己的会话。
#[test]
fn concurrent_opens_install_environment_per_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let repo_a = make_repo(dir.path(), "a");
    let repo_b = make_repo(dir.path(), "b");
    let data = dir.path().join("data");
    fs::create_dir_all(&data).unwrap();
    remember(&data, &repo_a, &real_git(&repo_a));

    let resolver: ShellResolver = Arc::new(move |path| {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        full_env(path, &format!("env-{name}"))
    });
    let state = Arc::new(AppState::new(data.clone()).with_shell_resolver(resolver));
    let ids = [register(&state, &repo_a), register(&state, &repo_b)];

    let start = Arc::new(Barrier::new(2));
    let handles: Vec<_> = ids
        .iter()
        .cloned()
        .map(|id| {
            let state = Arc::clone(&state);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                state.worktree_session(&id).unwrap()
            })
        })
        .collect();
    let sessions: Vec<_> = handles.into_iter().map(|handle| handle.join().unwrap()).collect();

    for (session, expected) in sessions.iter().zip(["env-a", "env-b"]) {
        assert!(
            wait_until(Duration::from_secs(3), || {
                marker_of(&session.shell_env_snapshot()).as_deref() == Some(expected)
            }),
            "{} did not get {expected}",
            session.path.display()
        );
    }
}

/// S13：正常退出的收尾入口把内存里的诊断摘要写盘。
#[test]
fn exit_flushes_diagnostics_summary() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path(), "repo");
    let data = dir.path().join("data");
    fs::create_dir_all(&data).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (state, id) = app(&data, &repo, counting_resolver(&calls));
    // 打开会话会运行 Git（读取版本、状态），这些命令只进内存，不立即写盘。
    let session = state.worktree_session(&id).unwrap();
    session.git.run_ok(&["status", "--porcelain"], "status").unwrap();
    let summary = data.join("diagnostics-summary.json");
    let before = fs::read(&summary).ok().map(|bytes| bytes.len()).unwrap_or(0);

    state.on_exit();
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&fs::read(&summary).unwrap()).unwrap();
    assert!(!entries.is_empty(), "exit must write pending entries");
    assert!(fs::read(&summary).unwrap().len() > before);
}
