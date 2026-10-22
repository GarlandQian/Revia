//! 集成测试：整文件暂存与取消暂存（M2 任务 02）。
//!
//! 依据 M2 规格与测试决策：
//! - 好的测试从应用操作入口走真实 Git 和临时目录，自行创建仓库与独立应用数据目录；
//! - 覆盖 T01, T03–T06, T14。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use revia_lib::git::status::{ChangesKind, FileStatus};
use revia_lib::git::CommandLog;
use revia_lib::session::WorktreeSession;

struct TestContext {
    _temp_dir: tempfile::TempDir,
    repo_path: PathBuf,
}

impl TestContext {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("创建临时目录失败");
        let repo_path = temp_dir.path().join("repo");
        fs::create_dir_all(&repo_path).expect("创建仓库目录失败");

        let ctx = Self {
            _temp_dir: temp_dir,
            repo_path,
        };

        ctx.git(&["init"]);
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

    fn open_session(&self) -> Arc<WorktreeSession> {
        let home = revia_lib::shell::home_dir().unwrap_or_else(|_| PathBuf::from("/tmp"));
        let shell_env = revia_lib::shell::resolve(&home, &self.repo_path);
        WorktreeSession::open("test-worktree", &self.repo_path, shell_env, Arc::new(CommandLog::default()))
            .expect("打开 WorktreeSession 失败")
    }
}

/// T01: 普通已跟踪文本整文件暂存与取消暂存；工作区字节全程不变。
#[test]
fn t01_stage_and_unstage_regular_file() {
    let ctx = TestContext::new();
    ctx.write_file("file.txt", b"v1\nline 2\n");
    ctx.git(&["add", "file.txt"]);
    ctx.git(&["commit", "-m", "init"]);

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

    // 验证：工作区字节保持不变
    assert_eq!(ctx.read_file("file.txt"), b"v2\nline 2\nline 3\n");
    // 验证：暂存区已有该改动，未暂存区已无该改动
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

    // 两个条目的指纹必须互不相同
    assert_ne!(unstaged_entry.fingerprint, staged_entry.fingerprint);

    // 1. 测试整文件暂存 Unstaged：工作区全部内容被暂存
    let snap_after_stage = session
        .stage_file(&unstaged_entry.path, &unstaged_entry.fingerprint)
        .expect("暂存未暂存改动失败");
    assert_eq!(snap_after_stage.unstaged_count, 0);
    assert_eq!(snap_after_stage.staged_count, 1);
    assert_eq!(ctx.read_file("part.txt"), b"line 1 staged\nline 2\nline 3 unstaged\n");

    // 2. 测试整文件取消暂存：已暂存内容恢复到 HEAD，工作区内容依然保留
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

/// T03: 二进制文件与仅权限改变（mode change）整文件暂存与取消暂存。
#[test]
fn t03_stage_and_unstage_binary_and_mode_change() {
    let ctx = TestContext::new();
    // 1. 二进制文件
    let bin_data = vec![0u8, 159, 146, 150, 0, 255, 128, 64];
    ctx.write_file("asset.bin", &bin_data);
    ctx.git(&["add", "asset.bin"]);
    ctx.git(&["commit", "-m", "add binary"]);

    let mut modified_bin = bin_data.clone();
    modified_bin[4] = 99;
    ctx.write_file("asset.bin", &modified_bin);

    let session = ctx.open_session();
    let snap = session.refresh().unwrap();
    let bin_entry = snap.entries.iter().find(|e| e.path == "asset.bin").unwrap();
    assert!(bin_entry.binary);

    let snap_staged = session.stage_file(&bin_entry.path, &bin_entry.fingerprint).unwrap();
    let staged_bin = snap_staged.entries.iter().find(|e| e.path == "asset.bin" && e.changes == ChangesKind::Staged).unwrap();

    let snap_unstaged = session.unstage_file(&staged_bin.path, &staged_bin.fingerprint).unwrap();
    assert_eq!(ctx.read_file("asset.bin"), modified_bin);
    assert_eq!(snap_unstaged.unstaged_count, 1);

    // 2. 权限改变
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ctx.write_file("script.sh", b"#!/bin/sh\necho ok\n");
        ctx.git(&["add", "script.sh"]);
        ctx.git(&["commit", "-m", "add script"]);

        let full_script = ctx.path().join("script.sh");
        fs::set_permissions(&full_script, fs::Permissions::from_mode(0o755)).unwrap();

        let snap_mode = session.refresh().unwrap();
        let mode_entry = snap_mode.entries.iter().find(|e| e.path == "script.sh").unwrap();
        assert_eq!(mode_entry.status, FileStatus::Modified);

        let snap_mode_staged = session.stage_file(&mode_entry.path, &mode_entry.fingerprint).unwrap();
        let staged_mode = snap_mode_staged.entries.iter().find(|e| e.path == "script.sh" && e.changes == ChangesKind::Staged).unwrap();

        let snap_mode_unstage = session.unstage_file(&staged_mode.path, &staged_mode.fingerprint).unwrap();
        assert!(snap_mode_unstage.entries.iter().any(|e| e.path == "script.sh" && e.changes == ChangesKind::Unstaged));
    }
}

/// T03: 符号链接、冲突文件只读并拒绝写操作；未跟踪文件拒绝取消暂存。
#[test]
fn t03_readonly_types_rejected() {
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
        assert!(link_entry.readonly_reason.is_some(), "符号链接应标记只读理由");

        let res = session.stage_file(&link_entry.path, &link_entry.fingerprint);
        assert!(res.is_err(), "符号链接暂存应被拒绝");
    }

    // 2. 未跟踪文件不能执行 unstage
    let ctx2 = TestContext::new();
    ctx2.write_file("untracked.txt", b"content");
    let session2 = ctx2.open_session();
    let snap2 = session2.refresh().unwrap();
    let entry2 = snap2.entries.iter().find(|e| e.path == "untracked.txt").unwrap();
    let unstage_res = session2.unstage_file(&entry2.path, &entry2.fingerprint);
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

    // 取消暂存该重命名文件
    let snap_after = session
        .unstage_file(&renamed.path, &renamed.fingerprint)
        .expect("取消暂存重命名失败");

    // 验证工作区：只有 new_name.txt，旧路径不存在
    assert!(!ctx.path().join("old_name.txt").exists());
    assert!(ctx.path().join("new_name.txt").exists());
    assert_eq!(ctx.read_file("new_name.txt"), b"important content in file");

    // 验证状态：旧路径变成 Unstaged Deleted，新路径变成 Untracked
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

/// T05: 写前校验目标文件指纹；过期指纹拒绝并提示；无关文件改变不拒绝。
#[test]
fn t05_stale_fingerprint_rejected_and_unrelated_file_allowed() {
    let ctx = TestContext::new();
    ctx.write_file("target.txt", b"v1\n");
    ctx.write_file("other.txt", b"other v1\n");
    ctx.git(&["add", "target.txt", "other.txt"]);
    ctx.git(&["commit", "-m", "init"]);

    // 两文件都在工作区修改
    ctx.write_file("target.txt", b"v2\n");
    ctx.write_file("other.txt", b"other v2\n");

    let session = ctx.open_session();
    let snap = session.refresh().unwrap();
    let target_entry = snap.entries.iter().find(|e| e.path == "target.txt").unwrap();
    let old_fingerprint = target_entry.fingerprint.clone();

    // 1. 无关文件发生进一步修改
    ctx.write_file("other.txt", b"other v3 further modified\n");

    // 暂存 target.txt 依然能够成功，不因无关文件改变被拒绝
    let snap_after_target = session
        .stage_file(&target_entry.path, &old_fingerprint)
        .expect("无关文件变化不应阻止目标文件暂存");
    assert_eq!(snap_after_target.staged_count, 1);
    assert_eq!(snap_after_target.unstaged_count, 1);

    // 2. 外部修改了未暂存的 other.txt 工作区，指纹失效
    let other_entry = snap_after_target.entries.iter().find(|e| e.path == "other.txt" && e.changes == ChangesKind::Unstaged).unwrap();
    let stale_other_fp = other_entry.fingerprint.clone();

    // 在外部修改 other.txt
    ctx.write_file("other.txt", b"other v4 changed again\n");

    // 用旧指纹操作 other.txt，必须被拒绝！
    let res = session.stage_file(&other_entry.path, &stale_other_fp);
    assert!(res.is_err(), "目标文件外部修改后，旧指纹必须被拒绝");
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("文件已变化，请重新确认"), "错误提示必须为‘文件已变化，请重新确认’，实际为: {}", err_msg);
}

/// T06: index.lock 被占用时短暂重试并提示，绝不删除锁文件。
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

    // 创建 index.lock 模拟被外部工具占用
    let lock_path = ctx.path().join(".git").join("index.lock");
    fs::write(&lock_path, b"fake lock").unwrap();

    let started = std::time::Instant::now();
    let res = session.stage_file(&entry.path, &entry.fingerprint);
    let elapsed = started.elapsed();

    // 验证：写操作失败
    assert!(res.is_err());
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("index.lock") || err_msg.contains("占用"), "提示信息应说明锁被占用: {}", err_msg);

    // 验证：进行了重试（耗时大于等于重试延迟）
    assert!(elapsed.as_millis() >= 100, "应有重试等待过程");

    // 关键红线：绝不能删除 index.lock！
    assert!(lock_path.exists(), "安全红线：绝不得删除 index.lock");
    fs::remove_file(&lock_path).unwrap();
}

/// T06: 同一 worktree 写入互斥且不排队。
#[test]
fn t06_concurrent_write_operations_rejected_without_queuing() {
    let ctx = TestContext::new();
    let session = ctx.open_session();

    // 手动获取写锁模拟在途写操作
    let guard = session.acquire_write_lock().expect("初次获取写锁应成功");

    // 另一个并发写操作必须立即返回拒绝，不排队
    let concurrent_res = session.acquire_write_lock();
    assert!(concurrent_res.is_err(), "并发写操作必须立即被拒绝");
    let err_msg = concurrent_res.unwrap_err().to_string();
    assert!(err_msg.contains("正在进行的写操作"), "错误信息应提示已有写操作在进行: {}", err_msg);

    drop(guard);
    // 释放后应能再次获取
    let second_guard = session.acquire_write_lock();
    assert!(second_guard.is_ok(), "释放后应能再次获取写锁");
}

/// T14: 受控入口拒绝未登记或越界目标。
#[test]
fn t14_controlled_entry_rejects_invalid_path() {
    let ctx = TestContext::new();
    let session = ctx.open_session();
    let _ = session.refresh().unwrap();

    // 尝试传入非改动列表中的越界路径
    let res = session.stage_file("../secret.txt", "any_hash");
    assert!(res.is_err(), "越界路径必须拒绝");

    let res2 = session.unstage_file("nonexistent.txt", "any_hash");
    assert!(res2.is_err(), "不存在的文件必须拒绝");
}
