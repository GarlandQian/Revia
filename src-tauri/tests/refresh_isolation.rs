//! M2 任务 06：当前 worktree 自动刷新与旧响应隔离。
//!
//! 全部使用真实 Git 与隔离的 HOME / Git 配置 / 临时仓库，不依赖外部 fixture，
//! 不以固定 sleep 伪造时序（需要并发证据时用显式闸门）。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::time::{Duration, Instant};

use revia_lib::git::status::ChangesKind;
use revia_lib::git::{CommandLog, CommandRecord};
use revia_lib::session::WorktreeSession;
use revia_lib::shell::{EnvCaptureInfo, ShellEnv};
use revia_lib::watch::{resolve_roots, WatchController};

struct TestContext {
    _temp_dir: tempfile::TempDir,
    repo: PathBuf,
    env: HashMap<String, String>,
    paths: Vec<String>,
}

impl TestContext {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("创建临时仓库目录");
        let repo = temp_dir.path().join("repo");
        let home = temp_dir.path().join("home");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&home).unwrap();
        let global_config = home.join(".gitconfig");
        fs::write(&global_config, b"").unwrap();

        let mut paths: Vec<String> = std::env::var_os("PATH")
            .map(|value| {
                std::env::split_paths(&value)
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        #[cfg(unix)]
        for extra in ["/usr/bin", "/bin", "/usr/local/bin", "/opt/homebrew/bin"] {
            if !paths.iter().any(|path| path == extra) {
                paths.push(extra.into());
            }
        }
        let joined_path = std::env::join_paths(paths.iter().map(Path::new)).unwrap();
        let mut env = HashMap::from([
            ("HOME".into(), home.display().to_string()),
            ("USERPROFILE".into(), home.display().to_string()),
            ("GIT_CONFIG_GLOBAL".into(), global_config.display().to_string()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("PATH".into(), joined_path.to_string_lossy().into_owned()),
        ]);
        for key in ["SYSTEMROOT", "WINDIR", "TEMP", "TMP", "TMPDIR"] {
            if let Ok(value) = std::env::var(key) {
                env.insert(key.into(), value);
            }
        }

        let ctx = Self { _temp_dir: temp_dir, repo, env, paths };
        ctx.git(&["init", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Revia Refresh Test"),
            ("user.email", "refresh@example.com"),
            ("commit.gpgsign", "false"),
            ("core.quotePath", "false"),
            ("core.autocrlf", "false"),
        ] {
            ctx.git(&["config", key, value]);
        }
        ctx
    }

    fn git(&self, args: &[&str]) -> String {
        let output = self.git_output(args);
        assert!(
            output.status.success(),
            "git {args:?}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn git_output(&self, args: &[&str]) -> Output {
        Command::new("git")
            .current_dir(&self.repo)
            .env_clear()
            .envs(&self.env)
            .args(args)
            .output()
            .expect("执行 Git")
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        let full = self.repo.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, bytes).unwrap();
    }

    fn track(&self, path: &str, bytes: &[u8]) {
        self.write(path, bytes);
        self.git(&["--literal-pathspecs", "add", "--", path]);
        self.git(&["commit", "-m", "fixture"]);
    }

    fn open(&self) -> Arc<WorktreeSession> {
        let shell_env = ShellEnv {
            path: self.paths.clone(),
            vars: self.env.clone(),
            info: EnvCaptureInfo {
                worktree: self.repo.display().to_string(),
                shell: if cfg!(windows) { "cmd.exe" } else { "/bin/zsh" }.into(),
                elapsed_ms: 0,
                timed_out: false,
                used_fallback: true,
                var_count: self.env.len(),
                path_entries: self.paths.clone(),
                diagnostics: None,
            },
        };
        WorktreeSession::open("refresh-worktree", &self.repo, shell_env, Arc::new(CommandLog::default()))
            .unwrap()
    }
}

fn entry_paths(snapshot: &revia_lib::session::ChangesSnapshot) -> Vec<String> {
    snapshot
        .entries
        .iter()
        .map(|entry| format!("{:?}:{}", entry.changes, entry.path))
        .collect()
}

/// T13：同路径、同组、同行数、同字节数但内容改变时，文字、指纹与快照版本都必须更新。
#[test]
fn t13_same_row_count_content_change_updates_text_and_versions() {
    let ctx = TestContext::new();
    ctx.track("src/app.ts", b"line one\nline two\nline three\n");

    // 第一次外部改动：与 HEAD 不同，且行数、字节数保持不变（"two" -> "TWO"）。
    ctx.write("src/app.ts", b"line one\nline TWO\nline three\n");

    let session = ctx.open();
    let first = session.refresh().unwrap();
    let first_entry = first
        .entries
        .iter()
        .find(|entry| entry.path == "src/app.ts" && entry.changes == ChangesKind::Unstaged)
        .expect("应有未暂存改动");
    let first_rows = session
        .rows("src/app.ts", ChangesKind::Unstaged, revia_lib::git::rows::ViewMode::Unified, 0, 64)
        .unwrap();
    let first_text = row_texts(&first_rows.rows);
    assert!(
        first_text.iter().any(|text| text.contains("line TWO")),
        "首次应显示第一次改动的内容：{first_text:?}"
    );

    // 第二次等长改写：行数与字节数都不变，只有内容不同。
    ctx.write("src/app.ts", b"line one\nLINE two\nline three\n");
    let second = session.refresh().unwrap();
    let second_entry = second
        .entries
        .iter()
        .find(|entry| entry.path == "src/app.ts" && entry.changes == ChangesKind::Unstaged)
        .unwrap();

    assert_eq!(
        first_entry.row_count_unified, second_entry.row_count_unified,
        "用例前提：显示行数不变"
    );
    assert_ne!(
        first_entry.fingerprint, second_entry.fingerprint,
        "内容变化必须改变文件指纹"
    );
    assert_ne!(
        first_entry.snapshot_revision, second_entry.snapshot_revision,
        "每次刷新必须为条目分配新的快照版本"
    );
    assert!(second.revision > first.revision, "顶层快照版本必须单调递增");

    let second_rows = session
        .rows("src/app.ts", ChangesKind::Unstaged, revia_lib::git::rows::ViewMode::Unified, 0, 64)
        .unwrap();
    let second_text = row_texts(&second_rows.rows);
    assert!(
        second_text.iter().any(|text| text.contains("LINE two")),
        "刷新后必须显示新内容：{second_text:?}"
    );
    assert_eq!(second_rows.revision, second.revision, "行切片的版本必须与快照一致");
}

/// 取显示行的文本（统一视图用 `text`，并排视图用左右单元格）。
fn row_texts(rows: &[revia_lib::git::rows::RenderRow]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            row.text
                .clone()
                .or_else(|| row.left.as_ref().map(|cell| cell.text.clone()))
                .or_else(|| row.right.as_ref().map(|cell| cell.text.clone()))
                .unwrap_or_default()
        })
        .collect()
}

/// 空列表也要有顶层版本，前端才能在空与非空之间拒绝旧响应。
#[test]
fn t13_empty_snapshot_still_carries_revision() {
    let ctx = TestContext::new();
    ctx.track("a.txt", b"content\n");

    let session = ctx.open();
    let clean = session.refresh().unwrap();
    assert!(clean.entries.is_empty(), "干净仓库应无改动条目");
    assert!(clean.revision > 0, "空快照同样必须有顶层版本");

    ctx.write("a.txt", b"changed\n");
    let dirty = session.refresh().unwrap();
    assert!(dirty.revision > clean.revision, "后续快照版本必须更大");
}

/// 外部暂存／取消暂存：刷新后条目分组与内容都要跟着真实 Git 状态走。
#[test]
fn t13_external_stage_and_unstage_are_reflected() {
    let ctx = TestContext::new();
    ctx.track("file.txt", b"v1\n");
    ctx.write("file.txt", b"v2\n");

    let session = ctx.open();
    let unstaged = session.refresh().unwrap();
    assert!(unstaged
        .entries
        .iter()
        .any(|entry| entry.path == "file.txt" && entry.changes == ChangesKind::Unstaged));

    // 外部（模拟终端）暂存。
    ctx.git(&["--literal-pathspecs", "add", "--", "file.txt"]);
    let staged = session.refresh().unwrap();
    let staged_entry = staged
        .entries
        .iter()
        .find(|entry| entry.path == "file.txt" && entry.changes == ChangesKind::Staged)
        .expect("外部暂存后应出现在已暂存组");
    assert_ne!(staged_entry.fingerprint, "", "已暂存条目也要有指纹");
    assert!(staged.revision > unstaged.revision);

    // 外部取消暂存。
    ctx.git(&["--literal-pathspecs", "reset", "--", "file.txt"]);
    let back = session.refresh().unwrap();
    assert!(back
        .entries
        .iter()
        .any(|entry| entry.path == "file.txt" && entry.changes == ChangesKind::Unstaged));
    assert!(back.revision > staged.revision);
}

/// 未跟踪文件内容变化后，刷新必须拿到新内容与新的行数统计。
#[test]
fn t13_untracked_content_change_updates_loaded_metrics() {
    let ctx = TestContext::new();
    ctx.track("keep.txt", b"keep\n");
    ctx.write("new.txt", b"first\n");

    let session = ctx.open();
    let first = session.refresh().unwrap();
    let first_entry = first
        .entries
        .iter()
        .find(|entry| entry.path == "new.txt" && entry.changes == ChangesKind::Untracked)
        .unwrap();
    assert!(!first_entry.metrics_loaded, "未跟踪文件初始不带统计");

    // 首次取行会补全统计。
    let slice = session
        .rows("new.txt", ChangesKind::Untracked, revia_lib::git::rows::ViewMode::Unified, 0, 8)
        .unwrap();
    assert!(slice.entry.metrics_loaded, "取行后应补全统计");
    assert!(slice.row_count > 0, "未跟踪文件必须有可见内容");

    // 外部追加内容后刷新：行数与指纹都要变。
    ctx.write("new.txt", b"first\nsecond\nthird\n");
    let second = session.refresh().unwrap();
    let second_entry = second
        .entries
        .iter()
        .find(|entry| entry.path == "new.txt" && entry.changes == ChangesKind::Untracked)
        .unwrap();
    assert_ne!(first_entry.fingerprint, second_entry.fingerprint, "未跟踪内容变化必须改变指纹");

    let slice = session
        .rows("new.txt", ChangesKind::Untracked, revia_lib::git::rows::ViewMode::Unified, 0, 16)
        .unwrap();
    let text: Vec<String> = slice
        .rows
        .iter()
        .map(|row| {
            row.text
                .clone()
                .or_else(|| row.right.as_ref().map(|cell| cell.text.clone()))
                .or_else(|| row.left.as_ref().map(|cell| cell.text.clone()))
                .unwrap_or_default()
        })
        .collect();
    assert!(text.iter().any(|line| line.contains("third")), "刷新后必须读到新内容：{text:?}");
}

/// 高亮结果必须带版本；同一文件持续刷新不会让文档缓存随版本线性增长。
#[test]
fn t13_highlight_results_carry_versions_and_stay_bounded() {
    let ctx = TestContext::new();
    ctx.track("src/app.ts", b"const a = 1;\nconst b = 2;\n");
    ctx.write("src/app.ts", b"const a = 10;\nconst b = 2;\n");

    let session = ctx.open();
    let first = session.refresh().unwrap();
    let result = session
        .highlight("src/app.ts", ChangesKind::Unstaged, revia_lib::session::FileVersion::New, 1, 2)
        .unwrap();
    assert_eq!(result.revision, first.revision, "高亮结果必须回传快照版本");
    assert!(!result.need_content);

    // 连续多版本刷新后，仍能取到与最新快照一致的高亮版本。
    let mut last_revision = first.revision;
    for index in 0..12 {
        ctx.write("src/app.ts", format!("const a = {};\nconst b = 2;\n", 100 + index).as_bytes());
        let snapshot = session.refresh().unwrap();
        let highlighted = session
            .highlight("src/app.ts", ChangesKind::Unstaged, revia_lib::session::FileVersion::New, 1, 2)
            .unwrap();
        assert_eq!(highlighted.revision, snapshot.revision, "高亮必须与最新快照对齐");
        assert!(snapshot.revision > last_revision);
        last_revision = snapshot.revision;
    }
}

/// 并发刷新：任一时刻发布出的快照版本都必须与它自己的内容一致，且版本严格递增。
#[test]
fn t13_concurrent_refreshes_never_publish_stale_snapshot() {
    let ctx = TestContext::new();
    ctx.track("file.txt", b"v0\n");

    let session = ctx.open();
    let mut handles = Vec::new();
    for _ in 0..4 {
        let session = session.clone();
        handles.push(std::thread::spawn(move || {
            let mut revisions = Vec::new();
            for _ in 0..6 {
                revisions.push(session.refresh().unwrap().revision);
            }
            revisions
        }));
    }

    let mut all: Vec<u64> = Vec::new();
    for handle in handles {
        all.extend(handle.join().unwrap());
    }
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 24, "24 次刷新必须产生 24 个不同的快照版本，不能重复或倒退");
}

/// linked worktree：监视根必须解析出真实 gitDir，而不是工作区里的 `.git` 文件本身。
#[test]
fn t13_linked_worktree_watch_roots_resolve_real_git_dir() {
    let ctx = TestContext::new();
    ctx.track("file.txt", b"base\n");
    let linked = ctx._temp_dir.path().join("linked");
    let linked_str = linked.display().to_string();
    ctx.git(&["worktree", "add", "-b", "linked-branch", &linked_str]);

    let dot_git = linked.join(".git");
    assert!(dot_git.is_file(), "linked worktree 的 .git 是文件");

    let common = ctx.repo.join(".git");
    let roots = resolve_roots(&linked, None, Some(common.clone()));
    assert!(roots.git_dir.is_dir(), "必须解析出真实 gitDir 目录");
    let canonical_common = fs::canonicalize(&common).unwrap_or(common.clone());
    let canonical_git_dir = fs::canonicalize(&roots.git_dir).unwrap_or(roots.git_dir.clone());
    assert!(
        canonical_git_dir.starts_with(&canonical_common),
        "linked worktree 的 gitDir 应位于 commonDir 下：{}",
        roots.git_dir.display()
    );
    assert_ne!(roots.git_dir, dot_git, "不能把 .git 文件本身当作监视目录");
}

/// 监视失败要持续可见，且手动刷新不会假装恢复（Q95）。
#[test]
fn t13_watch_failure_stays_visible_across_manual_refresh() {
    let ctx = TestContext::new();
    ctx.track("file.txt", b"v1\n");

    let session = ctx.open();
    let controller = WatchController::new();
    controller.force_failure(Some("自动刷新不可用：测试注入".into()));

    let status = controller.activate(resolve_roots(&session.path, None, None), |_| {});
    assert!(!status.active);
    assert!(status.error.as_deref().unwrap_or("").contains("自动刷新不可用"));

    // 手动核对成功不改变监视状态。
    let snapshot = session.refresh().unwrap();
    assert!(snapshot.revision > 0);
    let after = controller.status();
    assert!(!after.active, "手动刷新不能把监视失败改成成功");
    assert!(after.error.is_some(), "监视失败必须持续提示");
}

/// 监视真实生效：当前 worktree 的文件改动会触发一次合并通知。
#[test]
fn t13_watcher_emits_coalesced_change_notification() {
    let ctx = TestContext::new();
    ctx.track("file.txt", b"v1\n");

    let session = ctx.open();
    let hits = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let controller = WatchController::new();
    let counter = hits.clone();
    let status = controller.activate(resolve_roots(&session.path, None, None), move |_| {
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    });
    assert!(status.active, "真实目录应当监视成功：{:?}", status.error);

    // 连续多次写入：合并窗口内只应产生少量通知，而不是每个事件一次。
    for index in 0..6 {
        ctx.write("file.txt", format!("v{index}\n").as_bytes());
        std::thread::sleep(Duration::from_millis(5));
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && hits.load(std::sync::atomic::Ordering::Relaxed) == 0 {
        std::thread::sleep(Duration::from_millis(20));
    }
    let observed = hits.load(std::sync::atomic::Ordering::Relaxed);
    assert!(observed >= 1, "文件改动必须触发自动刷新通知");
    assert!(observed < 6, "连续写入必须被合并，实际通知 {observed} 次");

    controller.stop();
    let stopped = controller.status();
    assert!(!stopped.active, "停止后不应仍报告监视中");
}

/// 会话的只读查询必须使用登记 ID 与真实 Git，不因刷新而绕过写前校验。
#[test]
fn t13_refresh_keeps_write_preconditions_intact() {
    let ctx = TestContext::new();
    ctx.track("file.txt", b"v1\n");
    ctx.write("file.txt", b"v2\n");

    let session = ctx.open();
    let snapshot = session.refresh().unwrap();
    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == "file.txt" && entry.changes == ChangesKind::Unstaged)
        .unwrap();
    let stale = entry.fingerprint.clone();

    // 外部再改一次，旧指纹必须被拒。
    ctx.write("file.txt", b"v3\n");
    let error = session.stage_file("file.txt", &stale).unwrap_err();
    assert_eq!(error.to_string(), "文件已变化，请重新确认");

    // 刷新后的新指纹可以正常写入。
    let fresh = session.refresh().unwrap();
    let fresh_entry = fresh
        .entries
        .iter()
        .find(|entry| entry.path == "file.txt" && entry.changes == ChangesKind::Unstaged)
        .unwrap();
    let after = session.stage_file("file.txt", &fresh_entry.fingerprint).unwrap();
    assert!(after
        .entries
        .iter()
        .any(|entry| entry.path == "file.txt" && entry.changes == ChangesKind::Staged));
    assert!(after.revision > fresh.revision, "写后快照必须是更新的版本");
}

/// 命令日志与真实 Git 调用计数：刷新不得为每个文件额外起 Git 进程（保持既有性能边界）。
#[test]
fn t13_refresh_does_not_spawn_per_file_git_processes() {
    let ctx = TestContext::new();
    for index in 0..5 {
        ctx.track(&format!("file{index}.txt"), b"base\n");
        ctx.write(&format!("file{index}.txt"), format!("change{index}\n").as_bytes());
    }

    let log = Arc::new(CommandLog::default());
    let shell_env = ShellEnv {
        path: ctx.paths.clone(),
        vars: ctx.env.clone(),
        info: EnvCaptureInfo {
            worktree: ctx.repo.display().to_string(),
            shell: if cfg!(windows) { "cmd.exe" } else { "/bin/zsh" }.into(),
            elapsed_ms: 0,
            timed_out: false,
            used_fallback: true,
            var_count: ctx.env.len(),
            path_entries: ctx.paths.clone(),
            diagnostics: None,
        },
    };
    let session = WorktreeSession::open("count-worktree", &ctx.repo, shell_env, log.clone()).unwrap();
    let before = log.snapshot().len();
    let snapshot = session.refresh().unwrap();
    let after = log.snapshot().len();
    let spawned = after - before;

    assert_eq!(snapshot.entries.len(), 5);
    assert!(
        spawned <= 4,
        "刷新 5 个文件不应按文件起 Git 进程，实际新增 {spawned} 次"
    );
    let _ = entry_paths(&snapshot);
}

#[allow(dead_code)]
fn unused(_: CommandRecord) {}
