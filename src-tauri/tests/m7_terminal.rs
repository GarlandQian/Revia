//! 集成测试：内置终端的真实 PTY 读写、resize、退出与关闭（M7 任务 04）。
//!
//! 依据 M7 规格与既有测试约定：
//! - 从应用入口（`TerminalManager::open` / `write_input` / `resize` / `close`）走真实 PTY；
//! - 只用测试自己的 shell 子进程与临时 worktree，不触碰用户进程与用户配置；
//! - Tauri 的 mock app 只用来满足事件发射，不注册任何命令或窗口。

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use revia_lib::m7::terminal::TerminalManager;
use revia_lib::shell::{EnvCaptureInfo, ShellEnv};

/// 构造只含测试变量的登录环境：HOME 指向临时目录，PATH 沿用当前进程。
fn shell_env(home: &PathBuf, worktree: &PathBuf) -> ShellEnv {
    let mut path_entries = Vec::new();
    if let Some(ambient) = std::env::var_os("PATH") {
        for p in std::env::split_paths(&ambient) {
            let s = p.display().to_string();
            if !s.is_empty() {
                path_entries.push(s);
            }
        }
    }
    #[cfg(unix)]
    {
        for extra in ["/usr/bin", "/bin"] {
            if !path_entries.iter().any(|p| p == extra) {
                path_entries.push(extra.into());
            }
        }
    }

    let mut vars = std::collections::HashMap::new();
    vars.insert("HOME".into(), home.display().to_string());
    vars.insert("PATH".into(), path_entries.join(":"));

    ShellEnv {
        path: path_entries.clone(),
        vars,
        info: EnvCaptureInfo {
            worktree: worktree.display().to_string(),
            shell: if cfg!(windows) { "cmd.exe" } else { "/bin/zsh" }.into(),
            elapsed_ms: 1,
            timed_out: false,
            used_fallback: true,
            var_count: 2,
            path_entries,
            diagnostics: None,
        },
    }
}

/// 轮询回滚缓冲直到出现标记；超时返回当前内容，由调用方断言。
fn wait_for_marker(manager: &Arc<TerminalManager>, session_id: u64, marker: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut latest = String::new();
    while Instant::now() < deadline {
        if let Ok(text) = manager.snapshot(session_id) {
            latest = text;
            if latest.contains(marker) {
                return latest;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    latest
}

#[cfg(unix)]
#[test]
fn pty_round_trip_resize_and_close() {
    let temp = tempfile::tempdir().expect("创建临时目录失败");
    let worktree = temp.path().join("repo");
    let home = temp.path().join("home");
    fs::create_dir_all(&worktree).expect("创建 worktree 失败");
    fs::create_dir_all(&home).expect("创建 HOME 失败");

    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("构建 mock app 失败");
    let handle = app.handle().clone();

    let manager = Arc::new(TerminalManager::new());
    let env = shell_env(&home, &worktree);
    let info = manager
        .open(&handle, "wt-1", &worktree, &env, None, 24, 80)
        .expect("打开终端失败");
    assert!(info.running, "新开的终端应在运行");
    assert_eq!(info.worktree_id, "wt-1");
    assert_eq!((info.rows, info.cols), (24, 80));

    // 写入 + 读回：PTY 双向可用，中文与 UTF-8 不被截断。
    manager
        .write_input(info.session_id, "printf 'M7-OK-中文\\n'\n")
        .expect("写入终端失败");
    let text = wait_for_marker(&manager, info.session_id, "M7-OK-中文");
    assert!(
        text.contains("M7-OK-中文"),
        "应能读回终端输出，实际内容：{text:?}"
    );

    // resize 之后仍可继续写入与读取。
    manager
        .resize(info.session_id, 40, 120)
        .expect("调整终端尺寸失败");
    let resized = manager
        .list()
        .into_iter()
        .find(|s| s.session_id == info.session_id)
        .expect("会话应仍在列表中");
    assert_eq!((resized.rows, resized.cols), (40, 120));

    manager
        .write_input(info.session_id, "printf 'AFTER-RESIZE\\n'\n")
        .expect("resize 后写入失败");
    let text = wait_for_marker(&manager, info.session_id, "AFTER-RESIZE");
    assert!(text.contains("AFTER-RESIZE"), "resize 后仍应读到输出");

    // 关闭：会话被移除，之后的读写按“未知会话”拒绝，不残留进程句柄。
    manager.close(info.session_id).expect("关闭终端失败");
    assert!(manager.list().is_empty(), "关闭后不应还有会话");
    let write_error = manager
        .write_input(info.session_id, "echo still-there\n")
        .expect_err("关闭后写入必须被拒绝");
    assert_eq!(write_error.code, Some("errors.m7.terminalSessionUnknown"));
    assert!(manager.snapshot(info.session_id).is_err());
    assert!(!manager.has_running("wt-1"), "关闭后该 worktree 不应有运行中的终端");
}

/// 两个 worktree 的会话互相独立：关掉一个不影响另一个。
#[cfg(unix)]
#[test]
fn sessions_are_independent_per_worktree() {
    let temp = tempfile::tempdir().expect("创建临时目录失败");
    let first = temp.path().join("one");
    let second = temp.path().join("two");
    let home = temp.path().join("home");
    for dir in [&first, &second, &home] {
        fs::create_dir_all(dir).expect("创建目录失败");
    }

    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("构建 mock app 失败");
    let handle = app.handle().clone();

    let manager = Arc::new(TerminalManager::new());
    let env = shell_env(&home, &first);
    let a = manager
        .open(&handle, "wt-a", &first, &env, None, 24, 80)
        .expect("打开 A 失败");
    let b = manager
        .open(&handle, "wt-b", &second, &env, None, 24, 80)
        .expect("打开 B 失败");

    manager.write_input(a.session_id, "pwd\n").expect("A 写入失败");
    wait_for_marker(&manager, a.session_id, first.file_name().unwrap().to_str().unwrap());

    manager.close(a.session_id).expect("关闭 A 失败");
    assert!(manager.has_running("wt-b"), "B 必须仍在运行");
    manager
        .write_input(b.session_id, "printf 'B-ALIVE\\n'\n")
        .expect("关闭 A 后 B 仍应可写");
    let text = wait_for_marker(&manager, b.session_id, "B-ALIVE");
    assert!(text.contains("B-ALIVE"), "B 的输出不应受影响");

    manager.close_worktree("wt-b");
    assert!(manager.list().is_empty(), "两个会话都应被关闭");
}
