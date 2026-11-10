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
use std::sync::{mpsc, Arc, Barrier, Mutex};
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

/// 在独立目录里放一份 Git，用于模拟「解析出的 Git 路径变了」。
fn git_in(dir: &Path, repo: &Path) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let target = dir.join(name);
    fs::copy(real_git(repo), &target).unwrap();
    target
}

/// 只含指定 Git 目录的完整环境。
fn env_with_git(path: &Path, bin: &Path, marker: &str) -> ShellEnv {
    let mut env = ShellEnv::minimal(path);
    let bin = bin.display().to_string();
    env.path = vec![bin.clone()];
    env.vars.insert("PATH".into(), bin);
    env.vars.insert(MARKER.into(), marker.into());
    env.info.used_fallback = false;
    env.info.fallback_reason = None;
    env
}

fn marker_of(env: &ShellEnv) -> Option<String> {
    env.vars.get(MARKER).cloned()
}

/// 只用来**观察**生产接缝（`EnvCoordinator::waiters`）何时被进入，不驱动被测时序：
/// 断言仍然落在生产行为上，轮询不到就失败，不会因为等待而让测试变绿。
fn wait_until(deadline: Duration, condition: impl Fn() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < deadline {
        if condition() {
            return true;
        }
        std::thread::yield_now();
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

/// M10：环境重载会新建会话，但前端仍保留旧快照水位，新快照不能从 1 重新计数。
#[test]
fn reloaded_session_publishes_revisions_above_the_previous_session() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path(), "repo");
    let data = dir.path().join("data");
    fs::create_dir_all(&data).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (state, id) = app(&data, &repo, counting_resolver(&calls));
    let original = state.worktree_session(&id).unwrap();
    original.refresh().unwrap();
    original.refresh().unwrap();
    let previous = original.refresh().unwrap();

    state.reload_sessions().unwrap();
    fs::write(repo.join("after-reload.txt"), "visible after reload\n").unwrap();
    let replacement = state.worktree_session(&id).unwrap();
    assert_ne!(original.instance_id(), replacement.instance_id());
    let current = replacement.refresh().unwrap();
    assert!(current.revision > previous.revision, "新会话快照必须能通过前端已有水位");
    assert!(current.entries.iter().any(|entry| entry.path == "after-reload.txt"));

    fs::write(repo.join("after-watch.txt"), "visible after watch\n").unwrap();
    let watched = replacement.refresh().unwrap();
    assert!(watched.revision > current.revision);
    assert!(watched.entries.iter().any(|entry| entry.path == "after-watch.txt"));
}

/// 旧会话已完成真实读取但尚未发布时重载，新会话发布后旧内容不能获得更大的版本。
#[test]
fn superseded_read_cannot_publish_after_the_replacement_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path(), "repo");
    let data = dir.path().join("data");
    fs::create_dir_all(&data).unwrap();
    fs::write(repo.join("before.txt"), "before reload\n").unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (state, id) = app(&data, &repo, counting_resolver(&calls));
    let original = state.worktree_session(&id).unwrap();
    let captured = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let reader = {
        let session = Arc::clone(&original);
        let captured = Arc::clone(&captured);
        let release = Arc::clone(&release);
        std::thread::spawn(move || session.refresh_with_hook(|| {
            captured.wait();
            release.wait();
        }))
    };
    captured.wait();
    state.reload_sessions().unwrap();
    fs::write(repo.join("after.txt"), "after reload\n").unwrap();
    let replacement = state.worktree_session(&id).unwrap();
    let fresh = replacement.refresh().unwrap();
    assert!(fresh.entries.iter().any(|entry| entry.path == "after.txt"));
    release.wait();
    let late = reader.join().unwrap().expect_err("已失效会话的迟到读取不能发布");
    assert_eq!(late.payload().code.as_deref(), Some("errors.environment.reloaded"));
    let retry = original.refresh().expect_err("旧会话不能再次取得新版本");
    assert_eq!(retry.payload().code.as_deref(), Some("errors.environment.reloaded"));
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
    // 解析仍卡在 release 屏障上：等写者真正进入生产等待点（`EnvCoordinator::wait`），
    // 而不是只等到线程启动。此时它不可能拿到写锁。
    assert!(
        wait_until(Duration::from_secs(3), || state.env.waiters(&id) == 1),
        "writer never entered the environment wait"
    );
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

/// 后台解析换了 Git：旧会话的等待写入按结构化错误拒绝，不在被替换的对象上继续写。
#[test]
fn replaced_session_rejects_waiting_write() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path(), "repo");
    let data = dir.path().join("data");
    fs::create_dir_all(&data).unwrap();
    let old_git = git_in(&dir.path().join("bin-a"), &repo);
    let new_git = git_in(&dir.path().join("bin-b"), &repo);
    remember(&data, &repo, &old_git);

    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let (entered_r, release_r) = (Arc::clone(&entered), Arc::clone(&release));
    let bin_b = dir.path().join("bin-b");
    let resolver: ShellResolver = Arc::new(move |path| {
        entered_r.wait();
        release_r.wait();
        env_with_git(path, &bin_b, "replaced")
    });
    let (state, id) = app(&data, &repo, resolver);

    let session = state.worktree_session(&id).expect("opens with remembered git");
    assert_eq!(session.git.program, old_git);
    entered.wait(); // 后台解析已经开始，ready 尚未发布

    // 旧会话上的写操作在环境门里等待；会话被替换后必须被拒绝。
    let waiter = {
        let session = Arc::clone(&session);
        std::thread::spawn(move || {
            let result = session.acquire_write_lock().map(|guard| {
                drop(guard);
            });
            result
        })
    };
    // 等写者真正进入生产等待点，再放行后台解析。
    assert!(
        wait_until(Duration::from_secs(3), || state.env.waiters(&id) == 1),
        "writer never entered the environment wait"
    );
    release.wait();
    let error = waiter.join().unwrap().expect_err("superseded session must reject the write");
    assert_eq!(error.code, Some("errors.environment.reloaded"));

    let current = state.worktree_session(&id).unwrap();
    assert!(!Arc::ptr_eq(&current, &session), "new git must replace the session");
    assert_eq!(current.git.program, new_git);
    assert_eq!(marker_of(&current.shell_env_snapshot()).as_deref(), Some("replaced"));
}

/// 重新加载环境时，等待中的写入醒来看到失效会话，拿到结构化错误而不是旧对象。
#[test]
fn reload_rejects_write_waiting_for_environment() {
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
        // 只有第一次（后台解析）阻塞；重新打开时的同步解析立即返回。
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            entered_r.wait();
            release_r.wait();
            full_env(path, "late")
        } else {
            full_env(path, "fresh")
        }
    });
    let (state, id) = app(&data, &repo, resolver);

    let session = state.worktree_session(&id).unwrap();
    entered.wait();

    let waiter = {
        let session = Arc::clone(&session);
        std::thread::spawn(move || session.acquire_write_lock().map(|guard| drop(guard)))
    };
    // 等写者真正进入生产等待点：它没有持有写锁，重新加载应当成功并让旧会话失效。
    assert!(
        wait_until(Duration::from_secs(3), || state.env.waiters(&id) == 1),
        "writer never entered the environment wait"
    );
    state.reload_sessions().unwrap();
    let error = waiter.join().unwrap().expect_err("reloaded session must reject the write");
    assert_eq!(error.code, Some("errors.environment.reloaded"));
    release.wait();
    // 重新打开得到新会话；旧的后台结果在作废槽位上发布不了。
    let fresh = state.worktree_session(&id).unwrap();
    assert!(!Arc::ptr_eq(&fresh, &session));
    assert_eq!(marker_of(&fresh.shell_env_snapshot()).as_deref(), Some("fresh"));
}

/// SP2：有写操作进行中时重载被拒绝，**不能**部分失效——另一个 worktree 的会话照常可用。
#[test]
fn rejected_reload_leaves_every_session_usable() {
    let dir = tempfile::tempdir().unwrap();
    let repo_a = make_repo(dir.path(), "a");
    let repo_b = make_repo(dir.path(), "b");
    let data = dir.path().join("data");
    fs::create_dir_all(&data).unwrap();

    let calls = Arc::new(AtomicUsize::new(0));
    let state = AppState::new(data.clone()).with_shell_resolver(counting_resolver(&calls));
    let a_id = register(&state, &repo_a);
    let b_id = register(&state, &repo_b);
    let a = state.worktree_session(&a_id).unwrap();
    let b = state.worktree_session(&b_id).unwrap();

    // B 上有写操作进行中：重载必须整体拒绝。
    let held = b.acquire_write_lock().expect("write lock on b");
    let error = state
        .reload_sessions()
        .expect_err("reload must be rejected while a write is running");
    assert_eq!(
        error.code.as_deref(),
        Some("errors.commands.aWriteOperationProgressReloadEnvironment")
    );
    // A 没有被部分失效：仍是同一个会话，写入可以正常开始。
    assert!(Arc::ptr_eq(&state.worktree_session(&a_id).unwrap(), &a));
    let guard = a
        .acquire_write_lock()
        .expect("untouched session must still accept writes");
    drop(guard);
    assert!(state.has_session(&a_id), "rejected reload must not clear sessions");
    assert!(state.has_session(&b_id), "rejected reload must not clear sessions");

    // 写操作结束后重载成功，旧会话统一失效。
    drop(held);
    state.reload_sessions().unwrap();
    assert!(!state.has_session(&a_id));
    assert!(!state.has_session(&b_id));
    assert_eq!(
        a.acquire_write_lock().unwrap_err().code.as_deref(),
        Some("errors.environment.reloaded")
    );
}

/// SP4：在真实解析阻塞期间完成重载，旧打开随后必须被拒绝登记。
#[test]
fn reload_during_sync_resolve_does_not_resurrect_old_session() {
    let dir = tempfile::tempdir().unwrap();
    let repo = make_repo(dir.path(), "repo");
    let data = dir.path().join("data");
    fs::create_dir_all(&data).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let (counter, entered_r, release_r) =
        (Arc::clone(&calls), Arc::clone(&entered), Arc::clone(&release));
    let resolver: ShellResolver = Arc::new(move |path| {
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            entered_r.wait();
            release_r.wait();
            full_env(path, "old")
        } else {
            full_env(path, "fresh")
        }
    });
    let state = Arc::new(AppState::new(data).with_shell_resolver(resolver));
    let id = register(&state, &repo);
    let opener = {
        let state = Arc::clone(&state);
        let id = id.clone();
        std::thread::spawn(move || state.worktree_session(&id))
    };
    entered.wait();
    let (finished, completed) = mpsc::channel();
    let reloader = {
        let state = Arc::clone(&state);
        std::thread::spawn(move || { finished.send(state.reload_sessions()).unwrap(); })
    };
    // 观察实际完成，非“线程已启动”。即使回归导致等待也先放行并 join，避免挂住套件。
    let result = completed.recv_timeout(Duration::from_secs(2));
    release.wait();
    let opened = opener.join().unwrap();
    reloader.join().unwrap();
    result.expect("reload must finish while the shell resolver is blocked").unwrap();
    assert_eq!(opened.err().expect("old open must be rejected").code.as_deref(),
        Some("errors.environment.reloaded"));
    assert!(!state.has_session(&id));
    let fresh = state.worktree_session(&id).unwrap();
    assert_eq!(marker_of(&fresh.shell_env_snapshot()).as_deref(), Some("fresh"));
    drop(fresh.acquire_write_lock().expect("fresh session accepts writes"));
}

/// 一个 worktree 的同步回退不阻塞另一个已打开 worktree 的读取。
#[test]
fn slow_open_does_not_block_an_existing_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let existing_repo = make_repo(dir.path(), "existing");
    let slow_repo = make_repo(dir.path(), "slow");
    let data = dir.path().join("data");
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let (entered_r, release_r, slow) = (Arc::clone(&entered), Arc::clone(&release), slow_repo.clone());
    let resolver: ShellResolver = Arc::new(move |path| {
        if path == slow { entered_r.wait(); release_r.wait(); }
        full_env(path, "complete")
    });
    let state = Arc::new(AppState::new(data).with_shell_resolver(resolver));
    let existing_id = register(&state, &existing_repo);
    let existing = state.worktree_session(&existing_id).unwrap();
    // 仅清环境记忆，让新目标必须同步解析；现有会话保留。
    state.env.reset();
    let slow_id = register(&state, &slow_repo);
    let opener = {
        let state = Arc::clone(&state);
        std::thread::spawn(move || state.worktree_session(&slow_id))
    };
    entered.wait();
    let (finished, completed) = mpsc::channel();
    let reader = {
        let state = Arc::clone(&state);
        std::thread::spawn(move || { finished.send(state.worktree_session(&existing_id)).unwrap(); })
    };
    let result = completed.recv_timeout(Duration::from_secs(2));
    release.wait();
    opener.join().unwrap().unwrap();
    reader.join().unwrap();
    let loaded = result.expect("existing worktree must remain readable while another resolver waits").unwrap();
    assert!(Arc::ptr_eq(&existing, &loaded));
}
