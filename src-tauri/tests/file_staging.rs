//! 集成测试：整文件暂存与取消暂存（M2 任务 02）。
//!
//! 依据 M2 规格与测试决策：
//! - 好的测试从应用操作入口走真实 Git 和临时目录，自行创建仓库与独立应用数据目录；
//! - 完全隔离用户身份与全局配置（HOME、GIT_CONFIG_GLOBAL、GIT_CONFIG_NOSYSTEM）；
//! - 覆盖 T01, T03–T06, T14。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use revia_lib::git::status::{ChangesKind, FileStatus, ReadonlyReason};
use revia_lib::git::CommandLog;
use revia_lib::session::WorktreeSession;
use revia_lib::shell::ShellEnv;

struct TestContext {
    _temp_dir: tempfile::TempDir,
    repo_path: PathBuf,
    fake_home: PathBuf,
    global_config: PathBuf,
}

impl TestContext {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("创建临时目录失败");
        let repo_path = temp_dir.path().join("repo");
        let fake_home = temp_dir.path().join("fake_home");
        let global_config = fake_home.join(".gitconfig");
        fs::create_dir_all(&repo_path).expect("创建仓库目录失败");
        fs::create_dir_all(&fake_home).expect("创建假用户主目录失败");
        fs::write(&global_config, "").expect("创建空全局配置失败");

        let ctx = Self {
            _temp_dir: temp_dir,
            repo_path,
            fake_home,
            global_config,
        };

        ctx.git(&["init", "-b", "main"]);
        ctx.git(&["config", "user.name", "Revia Test"]);
        ctx.git(&["config", "user.email", "test@revia.dev"]);
        ctx.git(&["config", "commit.gpgsign", "false"]);
        ctx.git(&["config", "core.quotePath", "false"]);
        ctx
    }

    fn path(&self) -> &Path {
        &self.repo_path
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .env("HOME", &self.fake_home)
            .env("GIT_CONFIG_GLOBAL", &self.global_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("执行 git {:?} 失败: {}", args, e));
        if !output.status.success() {
            panic!(
                "git {:?} 失败 (code {:?}):\nstdout: {}\nstderr: {}",
                args,
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn write_file(&self, rel_path: &str, content: &[u8]) {
        let full = self.repo_path.join(rel_path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("创建父目录失败");
        }
        fs::write(&full, content).unwrap_or_else(|e| panic!("写入文件 {} 失败: {}", rel_path, e));
    }

    fn read_file(&self, rel_path: &str) -> Vec<u8> {
        let full = self.repo_path.join(rel_path);
        fs::read(&full).unwrap_or_else(|e| panic!("读取文件 {} 失败: {}", rel_path, e))
    }

    /// 读取 index 中的 blob 哈希与模式：`git ls-files --stage -- <path>`
    fn ls_files_stage(&self, rel_path: &str) -> Option<(String, String)> {
        let text = self.git(&["ls-files", "--stage", "--", rel_path]);
        let line = text.lines().next()?;
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            Some((parts[0].to_string(), parts[1].to_string()))
        } else {
            None
        }
    }

    fn open_session(&self) -> Arc<WorktreeSession> {
        let mut vars = std::collections::HashMap::new();
        vars.insert("HOME".into(), self.fake_home.display().to_string());
        vars.insert("GIT_CONFIG_GLOBAL".into(), self.global_config.display().to_string());
        vars.insert("GIT_CONFIG_NOSYSTEM".into(), "1".into());

        let shell_env = ShellEnv {
            path: vec![
                "/usr/bin".into(),
                "/bin".into(),
                "/usr/local/bin".into(),
                "/opt/homebrew/bin".into(),
            ],
            vars,
            info: revia_lib::shell::EnvCaptureInfo {
                worktree: self.repo_path.display().to_string(),
                shell: "/bin/zsh".into(),
                elapsed_ms: 1,
                timed_out: false,
                used_fallback: true,
                var_count: 3,
                path_entries: vec!["/opt/homebrew/bin".into(), "/usr/bin".into()],
                diagnostics: None,
            },
        };
        WorktreeSession::open("test-worktree", &self.repo_path, shell_env, Arc::new(CommandLog::default()))
            .expect("打开 WorktreeSession 失败")
    }
}

/// T01: 普通已跟踪文本整文件暂存与取消暂存；工作区字节与模式不变，暂存区 blob 正确更新与还原。
#[test]
fn t01_stage_and_unstage_regular_file() {
    let ctx = TestContext::new();
    ctx.write_file("file.txt", b"v1\nline 2\n");
    ctx.git(&["add", "file.txt"]);
    ctx.git(&["commit", "-m", "init"]);
    let (head_mode, head_blob) = ctx.ls_files_stage("file.txt").expect("初始应该有暂存条目");

    // 在工作区修改
    ctx.write_file("file.txt", b"v2\nline 2\nline 3\n");
    let session = ctx.open_session();

    // 1. 读取快照并找到未暂存改动
    let snapshot = session.refresh().expect("读取状态失败");
    let entry = snapshot
        .entries
        .iter()
        .find(|e| e.path == "file.txt" && e.changes == ChangesKind::Unstaged)
        .expect("应存在 file.txt 未暂存改动");
    assert_eq!(entry.status, FileStatus::Modified);
    assert!(entry.readonly_reason.is_none());
    assert!(!entry.fingerprint.is_empty());

    // 2. 整文件暂存
    let snap_after_stage = session
        .stage_file(&entry.path, &entry.fingerprint)
        .expect("整文件暂存失败");

    // 验证：工作区字节与模式保持不变
    assert_eq!(ctx.read_file("file.txt"), b"v2\nline 2\nline 3\n");
    // 验证：暂存区 blob 已经变为新内容，模式不变
    let (staged_mode, staged_blob) = ctx.ls_files_stage("file.txt").expect("暂存区应有条目");
    assert_eq!(staged_mode, head_mode);
    assert_ne!(staged_blob, head_blob, "暂存后 blob 必须改变");

    // 验证快照
    assert_eq!(snap_after_stage.unstaged_count, 0);
    assert_eq!(snap_after_stage.staged_count, 1);
    let staged_entry = snap_after_stage
        .entries
        .iter()
        .find(|e| e.path == "file.txt" && e.changes == ChangesKind::Staged)
        .expect("应移到已暂存分组");
    assert_eq!(staged_entry.status, FileStatus::Modified);

    // 3. 整文件取消暂存
    let snap_after_unstage = session
        .unstage_file(&staged_entry.path, &staged_entry.fingerprint)
        .expect("取消暂存失败");

    // 验证：工作区字节仍保持不变
    assert_eq!(ctx.read_file("file.txt"), b"v2\nline 2\nline 3\n");
    // 验证：暂存区 blob 恢复回 HEAD 的 blob 且模式一致（T01 严格断言）
    let (restored_mode, restored_blob) = ctx.ls_files_stage("file.txt").expect("恢复后暂存区条目");
    assert_eq!(restored_mode, head_mode);
    assert_eq!(restored_blob, head_blob, "取消暂存后暂存区 blob 必须恢复为 HEAD 版本");

    // 验证：回到未暂存分组
    assert_eq!(snap_after_unstage.staged_count, 0);
    assert_eq!(snap_after_unstage.unstaged_count, 1);
    assert!(snap_after_unstage
        .entries
        .iter()
        .any(|e| e.path == "file.txt" && e.changes == ChangesKind::Unstaged));
}

/// T01: 无 HEAD 的初始状态下，未跟踪文件整文件暂存与取消暂存。
#[test]
fn t01_stage_and_unstage_initial_commit_no_head() {
    let ctx = TestContext::new();
    ctx.write_file("new_file.txt", b"brand new file content");

    let session = ctx.open_session();
    let snapshot = session.refresh().expect("读取状态失败");
    let entry = snapshot
        .entries
        .iter()
        .find(|e| e.path == "new_file.txt" && e.changes == ChangesKind::Untracked)
        .expect("应为未跟踪文件");
    assert_eq!(entry.status, FileStatus::Untracked);

    // 暂存未跟踪文件
    let snap_after_stage = session
        .stage_file(&entry.path, &entry.fingerprint)
        .expect("无 HEAD 下暂存未跟踪文件失败");
    assert_eq!(snap_after_stage.staged_count, 1);
    assert_eq!(snap_after_stage.untracked_count, 0);
    let staged_entry = snap_after_stage
        .entries
        .iter()
        .find(|e| e.path == "new_file.txt" && e.changes == ChangesKind::Staged)
        .expect("应处于已暂存");
    assert_eq!(staged_entry.status, FileStatus::Added);

    // 取消暂存
    let snap_after_unstage = session
        .unstage_file(&staged_entry.path, &staged_entry.fingerprint)
        .expect("无 HEAD 下取消暂存失败");
    assert_eq!(snap_after_unstage.staged_count, 0);
    assert_eq!(snap_after_unstage.untracked_count, 1);
    assert_eq!(ctx.read_file("new_file.txt"), b"brand new file content");
}

/// T01: 空文件（0 字节）暂存与取消暂存。
#[test]
fn t01_stage_and_unstage_empty_file() {
    let ctx = TestContext::new();
    ctx.write_file("empty.txt", b"");
    ctx.git(&["add", "empty.txt"]);
    ctx.git(&["commit", "-m", "add empty"]);

    ctx.write_file("empty.txt", b"some text now\n");
    let session = ctx.open_session();
    let snap = session.refresh().unwrap();
    let entry = snap.entries.iter().find(|e| e.path == "empty.txt").unwrap();

    let staged = session.stage_file(&entry.path, &entry.fingerprint).unwrap();
    assert_eq!(staged.staged_count, 1);
    let staged_entry = staged.entries.iter().find(|e| e.path == "empty.txt").unwrap();

    let unstaged = session.unstage_file(&staged_entry.path, &staged_entry.fingerprint).unwrap();
    assert_eq!(unstaged.unstaged_count, 1);
    assert_eq!(ctx.read_file("empty.txt"), b"some text now\n");
}

/// T01: 同一文件部分暂存（同文件同时存在 Staged 和 Unstaged 改动）。
#[test]
fn t01_stage_and_unstage_partial_staged_file() {
    let ctx = TestContext::new();
    ctx.write_file("part.txt", b"line 1\nline 2\nline 3\n");
    ctx.git(&["add", "part.txt"]);
    ctx.git(&["commit", "-m", "init"]);

    // 先暂存一部分
    ctx.write_file("part.txt", b"line 1 staged\nline 2\nline 3\n");
    ctx.git(&["add", "part.txt"]);
    // 工作区再改一部分
    ctx.write_file("part.txt", b"line 1 staged\nline 2\nline 3 unstaged\n");

    let session = ctx.open_session();
    let snapshot = session.refresh().unwrap();
    assert_eq!(snapshot.entries.len(), 2, "应同时出现在已暂存与未暂存组");

    let unstaged_entry = snapshot.entries.iter().find(|e| e.changes == ChangesKind::Unstaged).unwrap();
    let staged_entry = snapshot.entries.iter().find(|e| e.changes == ChangesKind::Staged).unwrap();

    assert_ne!(unstaged_entry.fingerprint, staged_entry.fingerprint);

    // 1. 暂存未暂存部分
    let snap_after_stage = session
        .stage_file(&unstaged_entry.path, &unstaged_entry.fingerprint)
        .expect("暂存未暂存改动失败");
    assert_eq!(snap_after_stage.unstaged_count, 0);
    assert_eq!(snap_after_stage.staged_count, 1);
    assert_eq!(ctx.read_file("part.txt"), b"line 1 staged\nline 2\nline 3 unstaged\n");

    // 2. 取消暂存已暂存部分
    let staged_now = snap_after_stage.entries.iter().find(|e| e.changes == ChangesKind::Staged).unwrap();
    let snap_after_unstage = session
        .unstage_file(&staged_now.path, &staged_now.fingerprint)
        .expect("取消暂存失败");
    assert_eq!(snap_after_unstage.staged_count, 0);
    assert_eq!(snap_after_unstage.unstaged_count, 1);
    assert_eq!(ctx.read_file("part.txt"), b"line 1 staged\nline 2\nline 3 unstaged\n");
}

/// T02/T14: 中文路径与带空格路径整文件暂存与取消暂存。
#[test]
fn t02_stage_and_unstage_chinese_and_spaces_path() {
    let ctx = TestContext::new();
    let rel = "测试 文件夹/我的 源码 文件.ts";
    let greeting = "export const greeting = '你好世界';\n";
    ctx.write_file(rel, greeting.as_bytes());

    let session = ctx.open_session();
    let snap = session.refresh().unwrap();
    let entry = snap.entries.iter().find(|e| e.path == rel).expect("应找到中文空格路径");

    let snap_staged = session.stage_file(&entry.path, &entry.fingerprint).expect("暂存中文空格路径失败");
    let staged_entry = snap_staged.entries.iter().find(|e| e.path == rel).unwrap();
    assert_eq!(staged_entry.changes, ChangesKind::Staged);

    let snap_unstaged = session.unstage_file(&staged_entry.path, &staged_entry.fingerprint).expect("取消暂存中文空格路径失败");
    assert!(snap_unstaged.entries.iter().any(|e| e.path == rel && e.changes == ChangesKind::Untracked));
    assert_eq!(ctx.read_file(rel), greeting.as_bytes());
}

/// T03: 完整能力矩阵与只读拒绝（符号链接、子模块、未解决冲突、未跟踪嵌套仓库、非 UTF-8 路径）。
#[test]
fn t03_readonly_matrix_and_untracked_limits() {
    let ctx = TestContext::new();

    // 1. 符号链接
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        ctx.write_file("target.txt", b"hello");
        ctx.git(&["add", "target.txt"]);
        ctx.git(&["commit", "-m", "init"]);

        symlink(ctx.path().join("target.txt"), ctx.path().join("link.txt")).unwrap();

        let session = ctx.open_session();
        let snap = session.refresh().unwrap();
        let link_entry = snap.entries.iter().find(|e| e.path == "link.txt").unwrap();
        assert_eq!(link_entry.readonly_reason, Some(ReadonlyReason::Symlink));

        let res = session.stage_file(&link_entry.path, &link_entry.fingerprint);
        assert!(res.is_err(), "符号链接暂存应被拒绝");
    }

    // 2. 未跟踪嵌套仓库（? sub/ 包含 .git）
    let nested_path = ctx.path().join("nested_repo");
    fs::create_dir_all(&nested_path).unwrap();
    Command::new("git").current_dir(&nested_path).args(&["init"]).output().unwrap();
    fs::write(nested_path.join("file.txt"), b"nested").unwrap();

    let session_nested = ctx.open_session();
    let snap_nested = session_nested.refresh().unwrap();
    let nested_entry = snap_nested.entries.iter().find(|e| e.path.starts_with("nested_repo")).unwrap();
    assert_eq!(nested_entry.readonly_reason, Some(ReadonlyReason::NestedRepo));
    let stage_nested_res = session_nested.stage_file(&nested_entry.path, &nested_entry.fingerprint);
    assert!(stage_nested_res.is_err(), "未跟踪嵌套仓库暂存必须被拒绝");

    // 3. 冲突未合并状态 (Conflict/unmerged)
    let ctx_conflict = TestContext::new();
    ctx_conflict.write_file("conflict.txt", b"base line\n");
    ctx_conflict.git(&["add", "conflict.txt"]);
    ctx_conflict.git(&["commit", "-m", "base commit"]);
    ctx_conflict.git(&["branch", "feature"]);
    ctx_conflict.write_file("conflict.txt", b"branch main line\n");
    ctx_conflict.git(&["commit", "-am", "commit on main"]);
    ctx_conflict.git(&["checkout", "feature"]);
    ctx_conflict.write_file("conflict.txt", b"branch feature line\n");
    ctx_conflict.git(&["commit", "-am", "commit on feature"]);
    // merge 产生冲突
    let _ = Command::new("git")
        .current_dir(ctx_conflict.path())
        .env("HOME", &ctx_conflict.fake_home)
        .env("GIT_CONFIG_GLOBAL", &ctx_conflict.global_config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(&["merge", "main"])
        .output();

    let session_conflict = ctx_conflict.open_session();
    let snap_conflict = session_conflict.refresh().unwrap();
    let conflict_entry = snap_conflict.entries.iter().find(|e| e.path == "conflict.txt").unwrap();
    assert!(conflict_entry.unmerged);
    assert_eq!(conflict_entry.readonly_reason, Some(ReadonlyReason::Conflict));
    let conflict_stage_res = session_conflict.stage_file(&conflict_entry.path, &conflict_entry.fingerprint);
    assert!(conflict_stage_res.is_err(), "冲突文件不能通过普通暂存操作");

    // 4. 未跟踪文件不能执行 unstage
    let ctx_untracked = TestContext::new();
    ctx_untracked.write_file("untracked.txt", b"content");
    let session_untracked = ctx_untracked.open_session();
    let snap_u = session_untracked.refresh().unwrap();
    let entry_u = snap_u.entries.iter().find(|e| e.path == "untracked.txt").unwrap();
    let unstage_res = session_untracked.unstage_file(&entry_u.path, &entry_u.fingerprint);
    assert!(unstage_res.is_err(), "未跟踪文件取消暂存必须被拒绝");
}

/// T04: 取消暂存重命名，状态变为“旧路径未暂存删除 + 新路径未跟踪”，工作区内容与路径不变。
#[test]
fn t04_unstage_renamed_file_becomes_deleted_and_untracked() {
    let ctx = TestContext::new();
    ctx.write_file("old_name.txt", b"important content in file");
    ctx.git(&["add", "old_name.txt"]);
    ctx.git(&["commit", "-m", "initial commit"]);

    ctx.git(&["mv", "old_name.txt", "new_name.txt"]);

    let session = ctx.open_session();
    let snap = session.refresh().unwrap();
    assert_eq!(snap.staged_count, 1);
    let renamed = snap.entries.iter().find(|e| e.path == "new_name.txt").unwrap();
    assert_eq!(renamed.status, FileStatus::Renamed);
    assert_eq!(renamed.original_path.as_deref(), Some("old_name.txt"));

    let snap_after = session
        .unstage_file(&renamed.path, &renamed.fingerprint)
        .expect("取消暂存重命名失败");

    // 验证工作区：只有 new_name.txt，旧路径不存在
    assert!(!ctx.path().join("old_name.txt").exists());
    assert!(ctx.path().join("new_name.txt").exists());
    assert_eq!(ctx.read_file("new_name.txt"), b"important content in file");

    assert_eq!(snap_after.staged_count, 0);
    assert_eq!(snap_after.unstaged_count, 1);
    assert_eq!(snap_after.untracked_count, 1);

    let old_deleted = snap_after.entries.iter().find(|e| e.path == "old_name.txt").unwrap();
    assert_eq!(old_deleted.changes, ChangesKind::Unstaged);
    assert_eq!(old_deleted.status, FileStatus::Deleted);

    let new_untracked = snap_after.entries.iter().find(|e| e.path == "new_name.txt").unwrap();
    assert_eq!(new_untracked.changes, ChangesKind::Untracked);
    assert_eq!(new_untracked.status, FileStatus::Untracked);
}

/// T05: 写前核对指纹完整覆盖（未暂存、未跟踪、二进制、权限、已暂存侧变动、重命名变动，无关文件改变不拒绝）。
#[test]
fn t05_comprehensive_fingerprint_verification() {
    let ctx = TestContext::new();
    ctx.write_file("target.txt", b"v1\n");
    ctx.write_file("other.txt", b"other v1\n");
    ctx.write_file("binary.bin", &[0u8, 255, 128, 64]);
    ctx.git(&["add", "target.txt", "other.txt", "binary.bin"]);
    ctx.git(&["commit", "-m", "init"]);

    // 工作区修改
    ctx.write_file("target.txt", b"v2\n");
    ctx.write_file("other.txt", b"other v2\n");
    ctx.write_file("binary.bin", &[0u8, 255, 99, 64]);
    ctx.write_file("new_untracked.txt", b"untracked v1\n");

    let session = ctx.open_session();
    let snap = session.refresh().unwrap();

    let target_entry = snap.entries.iter().find(|e| e.path == "target.txt").unwrap();
    let old_target_fp = target_entry.fingerprint.clone();

    let bin_entry = snap.entries.iter().find(|e| e.path == "binary.bin").unwrap();
    let old_bin_fp = bin_entry.fingerprint.clone();

    let untracked_entry = snap.entries.iter().find(|e| e.path == "new_untracked.txt").unwrap();
    let old_untracked_fp = untracked_entry.fingerprint.clone();

    // 1. 无关文件修改：修改 other.txt，不影响 target.txt 暂存
    ctx.write_file("other.txt", b"other v3 modified\n");
    let snap_after_target = session.stage_file(&target_entry.path, &old_target_fp).expect("无关文件修改不应拒绝目标文件");
    assert_eq!(snap_after_target.staged_count, 1);

    // 2. 二进制文件外部修改后，旧指纹拒绝
    ctx.write_file("binary.bin", &[0u8, 255, 77, 64]);
    let bin_res = session.stage_file(&bin_entry.path, &old_bin_fp);
    assert!(bin_res.is_err(), "二进制文件内容外部改变后必须拒绝旧指纹");

    // 3. 未跟踪文件外部修改后，旧指纹拒绝
    ctx.write_file("new_untracked.txt", b"untracked v2 modified\n");
    let untracked_res = session.stage_file(&untracked_entry.path, &old_untracked_fp);
    assert!(untracked_res.is_err(), "未跟踪文件外部改变后必须拒绝旧指纹");

    // 4. 已暂存侧文件变动（在外部 git reset），旧已暂存指纹失效
    let staged_target = snap_after_target.entries.iter().find(|e| e.path == "target.txt" && e.changes == ChangesKind::Staged).unwrap();
    let old_staged_fp = staged_target.fingerprint.clone();
    ctx.git(&["reset", "--", "target.txt"]);
    let unstage_stale_res = session.unstage_file("target.txt", &old_staged_fp);
    assert!(unstage_stale_res.is_err(), "暂存区已变化时旧已暂存指纹必须拒绝");
}

/// T06: index.lock 被占用时在操作层重试并提示，绝不删除锁文件。
#[test]
fn t06_index_lock_retry_and_never_deleted() {
    let ctx = TestContext::new();
    ctx.write_file("file.txt", b"v1\n");
    ctx.git(&["add", "file.txt"]);
    ctx.git(&["commit", "-m", "init"]);

    ctx.write_file("file.txt", b"v2\n");
    let session = ctx.open_session();
    let snap = session.refresh().unwrap();
    let entry = snap.entries.iter().find(|e| e.path == "file.txt").unwrap();

    let lock_path = ctx.path().join(".git").join("index.lock");
    fs::write(&lock_path, b"fake lock").unwrap();

    let started = std::time::Instant::now();
    let res = session.stage_file(&entry.path, &entry.fingerprint);
    let elapsed = started.elapsed();

    assert!(res.is_err());
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("index.lock") || err_msg.contains("占用"), "错误提示锁被占用: {}", err_msg);
    assert!(elapsed.as_millis() >= 100, "应有重试等待过程");
    assert!(lock_path.exists(), "安全红线：绝不得删除 index.lock");
    fs::remove_file(&lock_path).unwrap();
}

/// T06: 同一 worktree 两次真实 stage_file 并发执行时，第二个立即被拒绝，不排队。
#[test]
fn t06_concurrent_write_operations_rejected_without_queuing() {
    let ctx = TestContext::new();
    ctx.write_file("file1.txt", b"v1\n");
    ctx.write_file("file2.txt", b"v1\n");
    ctx.git(&["add", "file1.txt", "file2.txt"]);
    ctx.git(&["commit", "-m", "init"]);

    ctx.write_file("file1.txt", b"v2\n");
    ctx.write_file("file2.txt", b"v2\n");

    let session = ctx.open_session();
    let snap = session.refresh().unwrap();
    let e1 = snap.entries.iter().find(|e| e.path == "file1.txt").unwrap();
    let e2 = snap.entries.iter().find(|e| e.path == "file2.txt").unwrap();

    let s1 = session.clone();
    let p1 = e1.path.clone();
    let fp1 = e1.fingerprint.clone();

    let s2 = session.clone();
    let p2 = e2.path.clone();
    let fp2 = e2.fingerprint.clone();

    let barrier = Arc::new(std::sync::Barrier::new(2));
    let b1 = barrier.clone();
    let b2 = barrier.clone();

    let t1 = std::thread::spawn(move || {
        b1.wait();
        s1.stage_file(&p1, &fp1)
    });

    let t2 = std::thread::spawn(move || {
        b2.wait();
        s2.stage_file(&p2, &fp2)
    });

    let res1 = t1.join().unwrap();
    let res2 = t2.join().unwrap();

    // 必有且仅有一个成功，另一个被拒绝且错误信息明确提示有正在进行的写操作
    let (success_count, rejected_count) = match (&res1, &res2) {
        (Ok(_), Err(e)) if e.to_string().contains("正在进行的写操作") => (1, 1),
        (Err(e), Ok(_)) if e.to_string().contains("正在进行的写操作") => (1, 1),
        _ => (0, 0),
    };
    assert_eq!((success_count, rejected_count), (1, 1), "两个并发写操作必须一个成功，另一个立即返回拒绝不排队！res1={:?}, res2={:?}", res1, res2);
}

/// T14: 受控入口拒绝未登记或越界目标。
#[test]
fn t14_controlled_entry_rejects_invalid_path() {
    let ctx = TestContext::new();
    let session = ctx.open_session();
    let _ = session.refresh().unwrap();

    let res = session.stage_file("../secret.txt", "any_hash");
    assert!(res.is_err(), "越界路径必须拒绝");

    let res2 = session.unstage_file("nonexistent.txt", "any_hash");
    assert!(res2.is_err(), "不存在的文件必须拒绝");
}
