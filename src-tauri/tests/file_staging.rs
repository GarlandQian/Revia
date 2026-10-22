//! 集成测试：整文件暂存与取消暂存（M2 任务 02，第二轮审查修改）。
//!
//! 依据 M2 规格与测试决策：
//! - 好的测试从应用操作入口走真实 Git 和临时目录，自行创建仓库与独立应用数据目录；
//! - 完全隔离用户身份与全局配置（HOME、GIT_CONFIG_GLOBAL、GIT_CONFIG_NOSYSTEM）；
//! - 覆盖 T01, T03–T06, T14。

use std::fs;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
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
        let text = self.git(&["--literal-pathspecs", "ls-files", "--stage", "--", rel_path]);
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
            for extra in ["/usr/bin", "/bin", "/usr/local/bin", "/opt/homebrew/bin"] {
                if !path_entries.iter().any(|p| p == extra) {
                    path_entries.push(extra.into());
                }
            }
        }

        let shell_name = if cfg!(windows) { "cmd.exe" } else { "/bin/zsh" };

        let shell_env = ShellEnv {
            path: path_entries.clone(),
            vars,
            info: revia_lib::shell::EnvCaptureInfo {
                worktree: self.repo_path.display().to_string(),
                shell: shell_name.into(),
                elapsed_ms: 1,
                timed_out: false,
                used_fallback: true,
                var_count: 3,
                path_entries,
                diagnostics: None,
            },
        };
        WorktreeSession::open("test-worktree", &self.repo_path, shell_env, Arc::new(CommandLog::default()))
            .expect("打开 WorktreeSession 失败")
    }
}

/// 审查意见 1：路径按字面处理，避免通配符误匹配导致非目标文件被暂存。
/// `x*.txt` 与 `xy.txt` 同时存在时，只暂存 `x*.txt`。
#[test]
fn t01_literal_pathspecs_wildcard_safety() {
    let ctx = TestContext::new();
    ctx.write_file("x*.txt", b"content of star file\n");
    ctx.write_file("xy.txt", b"content of xy file\n");

    let session = ctx.open_session();
    let snapshot = session.refresh().expect("读取状态失败");

    let star_entry = snapshot
        .entries
        .iter()
        .find(|e| e.path == "x*.txt" && e.changes == ChangesKind::Untracked)
        .expect("应存在 x*.txt 未跟踪条目");

    // 暂存 x*.txt
    let snap_after = session
        .stage_file(&star_entry.path, &star_entry.fingerprint)
        .expect("暂存带星号文件失败");

    // 关键安全断言：xy.txt 绝对不能被误带入暂存区！必须仍是未跟踪！
    assert!(
        snap_after
            .entries
            .iter()
            .any(|e| e.path == "xy.txt" && e.changes == ChangesKind::Untracked),
        "xy.txt 必须保持未跟踪状态，不得被 x*.txt 通配暂存！"
    );
    assert!(
        !snap_after
            .entries
            .iter()
            .any(|e| e.path == "xy.txt" && e.changes == ChangesKind::Staged),
        "xy.txt 绝不得进入已暂存状态！"
    );

    // 验证 x*.txt 正常进入已暂存
    let staged_star = snap_after
        .entries
        .iter()
        .find(|e| e.path == "x*.txt" && e.changes == ChangesKind::Staged)
        .expect("x*.txt 应当已暂存");
    assert_eq!(staged_star.status, FileStatus::Added);

    // 取消暂存 x*.txt 也必须按字面路径执行
    let snap_unstage = session
        .unstage_file(&staged_star.path, &staged_star.fingerprint)
        .expect("取消暂存带星号文件失败");
    assert!(snap_unstage
        .entries
        .iter()
        .any(|e| e.path == "x*.txt" && e.changes == ChangesKind::Untracked));
    assert!(snap_unstage
        .entries
        .iter()
        .any(|e| e.path == "xy.txt" && e.changes == ChangesKind::Untracked));
}

/// 审查意见 2：非 UTF-8 内容（如 GBK 编码文本）字节口径一致，暂存与取消暂存成功。
#[test]
fn t02_non_utf8_gbk_content_staging_and_unstaging() {
    let ctx = TestContext::new();
    // GBK 编码字节序列（包含非法 UTF-8 字节）
    let gbk_v1: &[u8] = b"\xd6\xd0\xce\xc4\xb2\xe2\xca\xd4\nv1\n";
    let gbk_v2: &[u8] = b"\xd6\xd0\xce\xc4\xb2\xe2\xca\xd4\nv2 modified\n";

    ctx.write_file("gbk.txt", gbk_v1);
    ctx.git(&["add", "gbk.txt"]);
    ctx.git(&["commit", "-m", "init gbk"]);

    // 在工作区修改为新的 GBK 内容
    ctx.write_file("gbk.txt", gbk_v2);

    let session = ctx.open_session();
    let snap = session.refresh().expect("读取快照失败");
    let entry = snap
        .entries
        .iter()
        .find(|e| e.path == "gbk.txt" && e.changes == ChangesKind::Unstaged)
        .expect("应找到 gbk.txt 未暂存改动");

    // 关键断言：非 UTF-8 内容在刷新与写前校验时使用同一份原始字节，不得误拒为“文件已变化”！
    let snap_staged = session
        .stage_file(&entry.path, &entry.fingerprint)
        .expect("非 UTF-8 内容暂存应当成功，不应因字节口径不一致被拒");
    assert_eq!(ctx.read_file("gbk.txt"), gbk_v2);

    let staged_entry = snap_staged
        .entries
        .iter()
        .find(|e| e.path == "gbk.txt" && e.changes == ChangesKind::Staged)
        .unwrap();

    let snap_unstaged = session
        .unstage_file(&staged_entry.path, &staged_entry.fingerprint)
        .expect("非 UTF-8 内容取消暂存应当成功");
    assert_eq!(ctx.read_file("gbk.txt"), gbk_v2);
    assert_eq!(snap_unstaged.unstaged_count, 1);
}

/// T01: 多文件仓库断言：操作目标文件时，其他文件的工作区字节、模式和暂存区 blob 保持不变。
#[test]
fn t01_stage_and_unstage_regular_file_multi_file_invariants() {
    let ctx = TestContext::new();
    ctx.write_file("target.txt", b"target v1\n");
    ctx.write_file("sibling_clean.txt", b"clean content\n");
    ctx.write_file("sibling_dirty.txt", b"dirty v1\n");
    ctx.git(&["add", "target.txt", "sibling_clean.txt", "sibling_dirty.txt"]);
    ctx.git(&["commit", "-m", "init"]);

    let (_, clean_blob_init) = ctx.ls_files_stage("sibling_clean.txt").unwrap();
    let (dirty_mode_init, dirty_blob_init) = ctx.ls_files_stage("sibling_dirty.txt").unwrap();

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    let clean_worktree_mode_init = fs::metadata(ctx.path().join("sibling_clean.txt")).unwrap().permissions().mode();
    #[cfg(unix)]
    let dirty_worktree_mode_init = fs::metadata(ctx.path().join("sibling_dirty.txt")).unwrap().permissions().mode();

    // 在工作区修改 target 和 sibling_dirty
    ctx.write_file("target.txt", b"target v2\n");
    ctx.write_file("sibling_dirty.txt", b"dirty v2 modified\n");

    let session = ctx.open_session();
    let snapshot = session.refresh().expect("读取状态失败");
    let target_entry = snapshot
        .entries
        .iter()
        .find(|e| e.path == "target.txt" && e.changes == ChangesKind::Unstaged)
        .expect("应存在 target.txt 未暂存改动");

    // 1. 暂存 target.txt
    let snap_after_stage = session
        .stage_file(&target_entry.path, &target_entry.fingerprint)
        .expect("整文件暂存失败");

    // 验证：其他文件工作区字节不变
    assert_eq!(ctx.read_file("sibling_clean.txt"), b"clean content\n");
    assert_eq!(ctx.read_file("sibling_dirty.txt"), b"dirty v2 modified\n");
    // 验证：其他文件工作区模式不变（审查意见 5）
    #[cfg(unix)]
    {
        assert_eq!(
            fs::metadata(ctx.path().join("sibling_clean.txt")).unwrap().permissions().mode(),
            clean_worktree_mode_init,
            "暂存 target 后 sibling_clean 工作区模式不变"
        );
        assert_eq!(
            fs::metadata(ctx.path().join("sibling_dirty.txt")).unwrap().permissions().mode(),
            dirty_worktree_mode_init,
            "暂存 target 后 sibling_dirty 工作区模式不变"
        );
    }
    // 验证：其他文件暂存区 blob 与 mode 不变
    let (_clean_mode_now, clean_blob_now) = ctx.ls_files_stage("sibling_clean.txt").unwrap();
    assert_eq!(clean_blob_now, clean_blob_init);
    let (dirty_mode_now, dirty_blob_now) = ctx.ls_files_stage("sibling_dirty.txt").unwrap();
    assert_eq!(dirty_blob_now, dirty_blob_init);
    assert_eq!(dirty_mode_now, dirty_mode_init);

    // 2. 取消暂存 target.txt
    let staged_target = snap_after_stage
        .entries
        .iter()
        .find(|e| e.path == "target.txt" && e.changes == ChangesKind::Staged)
        .unwrap();
    let snap_after_unstage = session
        .unstage_file(&staged_target.path, &staged_target.fingerprint)
        .expect("取消暂存失败");

    // 验证：其他文件依然保持不变
    assert_eq!(ctx.read_file("sibling_clean.txt"), b"clean content\n");
    assert_eq!(ctx.read_file("sibling_dirty.txt"), b"dirty v2 modified\n");
    #[cfg(unix)]
    {
        assert_eq!(
            fs::metadata(ctx.path().join("sibling_clean.txt")).unwrap().permissions().mode(),
            clean_worktree_mode_init,
            "取消暂存 target 后 sibling_clean 工作区模式不变"
        );
        assert_eq!(
            fs::metadata(ctx.path().join("sibling_dirty.txt")).unwrap().permissions().mode(),
            dirty_worktree_mode_init,
            "取消暂存 target 后 sibling_dirty 工作区模式不变"
        );
    }
    let (_, clean_blob_final) = ctx.ls_files_stage("sibling_clean.txt").unwrap();
    assert_eq!(clean_blob_final, clean_blob_init);
    let (_, dirty_blob_final) = ctx.ls_files_stage("sibling_dirty.txt").unwrap();
    assert_eq!(dirty_blob_final, dirty_blob_init);
    assert_eq!(snap_after_unstage.unstaged_count, 2);
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

/// 审查意见 1：含空格路径的二进制、仅权限变化与重命名的整文件暂存与取消暂存。
#[test]
fn t02_stage_and_unstage_spaced_paths_binary_mode_and_rename() {
    let ctx = TestContext::new();

    // 1. 初始化含空格的二进制文件、脚本文件和将被重命名的文件
    let bin_path = "my bin file.bin";
    let bin_data = vec![0u8, 255, 128, 64, 99];
    ctx.write_file(bin_path, &bin_data);

    let script_path = "my script with space.sh";
    ctx.write_file(script_path, b"#!/bin/sh\n");

    let old_rename_path = "old path with spaces.txt";
    ctx.write_file(old_rename_path, b"rename space content\n");

    ctx.git(&["add", bin_path, script_path, old_rename_path]);
    ctx.git(&["commit", "-m", "init spaced files"]);

    // 2. 工作区变动：
    // - 二进制文件内容修改
    let mut modified_bin = bin_data.clone();
    modified_bin[2] = 42;
    ctx.write_file(bin_path, &modified_bin);

    // - 脚本文件仅权限变化（644 -> 755）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script_full = ctx.path().join(script_path);
        fs::set_permissions(&script_full, fs::Permissions::from_mode(0o755)).unwrap();
    }

    // - 重命名带空格路径
    let new_rename_path = "new path with spaces.txt";
    ctx.git(&["mv", old_rename_path, new_rename_path]);

    let session = ctx.open_session();
    let snap = session.refresh().expect("刷新包含空格特殊变动的快照失败");

    // 3. 验证并暂存含空格二进制文件
    let bin_entry = snap
        .entries
        .iter()
        .find(|e| e.path == bin_path && e.changes == ChangesKind::Unstaged)
        .expect("应找到含空格二进制未暂存条目");
    assert!(bin_entry.binary, "含空格二进制文件应正确识别为 binary");
    let snap_after_bin_stage = session
        .stage_file(&bin_entry.path, &bin_entry.fingerprint)
        .expect("暂存含空格二进制文件应当成功");
    assert!(snap_after_bin_stage.entries.iter().any(|e| e.path == bin_path && e.changes == ChangesKind::Staged));

    // 取消暂存含空格二进制文件
    let staged_bin = snap_after_bin_stage
        .entries
        .iter()
        .find(|e| e.path == bin_path && e.changes == ChangesKind::Staged)
        .unwrap();
    let snap_after_bin_unstage = session
        .unstage_file(&staged_bin.path, &staged_bin.fingerprint)
        .expect("取消暂存含空格二进制文件应当成功");
    assert!(snap_after_bin_unstage.entries.iter().any(|e| e.path == bin_path && e.changes == ChangesKind::Unstaged));
    assert_eq!(ctx.read_file(bin_path), modified_bin);

    // 4. 验证并暂存含空格仅权限变化文件
    #[cfg(unix)]
    {
        let mode_entry = snap_after_bin_unstage
            .entries
            .iter()
            .find(|e| e.path == script_path && e.changes == ChangesKind::Unstaged)
            .expect("应找到含空格脚本未暂存条目");
        let snap_after_mode_stage = session
            .stage_file(&mode_entry.path, &mode_entry.fingerprint)
            .expect("暂存含空格仅权限文件应当成功");
        let staged_mode = snap_after_mode_stage
            .entries
            .iter()
            .find(|e| e.path == script_path && e.changes == ChangesKind::Staged)
            .unwrap();

        // 取消暂存仅权限文件
        let snap_after_mode_unstage = session
            .unstage_file(&staged_mode.path, &staged_mode.fingerprint)
            .expect("取消暂存含空格仅权限文件应当成功");
        assert!(snap_after_mode_unstage.entries.iter().any(|e| e.path == script_path && e.changes == ChangesKind::Unstaged));
    }

    // 5. 验证并取消暂存含空格重命名文件
    let rename_entry = snap
        .entries
        .iter()
        .find(|e| e.path == new_rename_path && e.changes == ChangesKind::Staged)
        .expect("应找到含空格重命名条目");
    assert_eq!(rename_entry.status, FileStatus::Renamed);
    assert_eq!(rename_entry.original_path.as_deref(), Some(old_rename_path));

    let snap_after_rename_unstage = session
        .unstage_file(&rename_entry.path, &rename_entry.fingerprint)
        .expect("取消暂存含空格重命名文件应当成功");
    // 取消暂存重命名后：暂存区恢复为 HEAD 状态，原路径在工作区缺失表现为未暂存的删除（Unstaged），新路径在工作区存在表现为未跟踪（Untracked）
    let old_deleted = snap_after_rename_unstage
        .entries
        .iter()
        .find(|e| e.path == old_rename_path)
        .expect("应找到原路径删除条目");
    assert_eq!(old_deleted.changes, ChangesKind::Unstaged);
    assert_eq!(old_deleted.status, FileStatus::Deleted);

    let new_untracked = snap_after_rename_unstage
        .entries
        .iter()
        .find(|e| e.path == new_rename_path)
        .expect("应找到新路径未跟踪条目");
    assert_eq!(new_untracked.changes, ChangesKind::Untracked);
}

/// T03: 恢复被删掉的二进制文件暂存与取消暂存测试。
#[test]
fn t03_stage_and_unstage_binary_file() {
    let ctx = TestContext::new();
    let bin_data = vec![0u8, 255, 128, 64, 99, 10, 0, 1];
    ctx.write_file("asset.bin", &bin_data);
    ctx.git(&["add", "asset.bin"]);
    ctx.git(&["commit", "-m", "add binary"]);

    let mut modified_bin = bin_data.clone();
    modified_bin[2] = 42;
    ctx.write_file("asset.bin", &modified_bin);

    let session = ctx.open_session();
    let snap = session.refresh().unwrap();
    let bin_entry = snap.entries.iter().find(|e| e.path == "asset.bin").unwrap();
    assert!(bin_entry.binary);

    let snap_staged = session.stage_file(&bin_entry.path, &bin_entry.fingerprint).unwrap();
    assert_eq!(snap_staged.staged_count, 1);
    assert_eq!(ctx.read_file("asset.bin"), modified_bin);

    let staged_bin = snap_staged.entries.iter().find(|e| e.path == "asset.bin" && e.changes == ChangesKind::Staged).unwrap();
    let snap_unstaged = session.unstage_file(&staged_bin.path, &staged_bin.fingerprint).unwrap();
    assert_eq!(snap_unstaged.unstaged_count, 1);
    assert_eq!(ctx.read_file("asset.bin"), modified_bin);
}

/// T03: 仅权限变化（mode change）暂存与取消暂存测试（POSIX 专有）。
#[cfg(unix)]
#[test]
fn t03_stage_and_unstage_mode_change() {
    let ctx = TestContext::new();
    use std::os::unix::fs::PermissionsExt;
    ctx.write_file("script.sh", b"#!/bin/sh\necho ok\n");
    ctx.git(&["add", "script.sh"]);
    ctx.git(&["commit", "-m", "add script"]);

    let full_script = ctx.path().join("script.sh");
    fs::set_permissions(&full_script, fs::Permissions::from_mode(0o755)).unwrap();

    let session = ctx.open_session();
    let snap_mode = session.refresh().unwrap();
    let mode_entry = snap_mode.entries.iter().find(|e| e.path == "script.sh").unwrap();

    let snap_mode_staged = session.stage_file(&mode_entry.path, &mode_entry.fingerprint).unwrap();
    let staged_mode = snap_mode_staged.entries.iter().find(|e| e.path == "script.sh" && e.changes == ChangesKind::Staged).unwrap();

    let snap_mode_unstage = session.unstage_file(&staged_mode.path, &staged_mode.fingerprint).unwrap();
    assert!(snap_mode_unstage.entries.iter().any(|e| e.path == "script.sh" && e.changes == ChangesKind::Unstaged));
}

/// T03: 完整能力矩阵只读拒绝：符号链接、子模块、未解决冲突、未跟踪嵌套仓库、非 UTF-8 路径及无权限读取文件。
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

    // 2. 子模块（Submodule, 模式 160000）
    let dummy_oid = "0123456789012345678901234567890123456789";
    ctx.git(&["update-index", "--add", "--cacheinfo", "160000", dummy_oid, "my_submodule"]);
    let session_sub = ctx.open_session();
    let snap_sub = session_sub.refresh().unwrap();
    let sub_entry = snap_sub.entries.iter().find(|e| e.path == "my_submodule").expect("应检测到子模块");
    assert_eq!(sub_entry.readonly_reason, Some(ReadonlyReason::Submodule));
    let sub_res = session_sub.unstage_file(&sub_entry.path, &sub_entry.fingerprint);
    assert!(sub_res.is_err(), "子模块取消暂存必须被拒绝");

    // 3. 未跟踪嵌套仓库（? nested_repo/ 包含 .git，审查意见 7 要求使用隔离环境）
    let nested_path = ctx.path().join("nested_repo");
    fs::create_dir_all(&nested_path).unwrap();
    let _ = Command::new("git")
        .current_dir(&nested_path)
        .env("HOME", &ctx.fake_home)
        .env("GIT_CONFIG_GLOBAL", &ctx.global_config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(&["init", "-b", "main"])
        .output()
        .unwrap();
    fs::write(nested_path.join("file.txt"), b"nested").unwrap();

    let session_nested = ctx.open_session();
    let snap_nested = session_nested.refresh().unwrap();
    let nested_entry = snap_nested.entries.iter().find(|e| e.path.starts_with("nested_repo")).unwrap();
    assert_eq!(nested_entry.readonly_reason, Some(ReadonlyReason::NestedRepo));
    let stage_nested_res = session_nested.stage_file(&nested_entry.path, &nested_entry.fingerprint);
    assert!(stage_nested_res.is_err(), "未跟踪嵌套仓库暂存必须被拒绝");

    // 4. 冲突未合并状态 (Conflict/unmerged)
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

    // 5. 非 UTF-8 路径（通过 Git 索引注入非 UTF-8 字节路径）
    #[cfg(unix)]
    {
        use std::ffi::OsStr;
        let blob_oid = ctx.git(&["hash-object", "-w", "--stdin"]);
        let blob_oid = blob_oid.trim();
        let raw_non_utf8_path = OsStr::from_bytes(b"bad_\x80\x81_path.txt");
        let mut cmd = Command::new("git");
        cmd.current_dir(ctx.path())
            .env("HOME", &ctx.fake_home)
            .env("GIT_CONFIG_GLOBAL", &ctx.global_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .arg("update-index")
            .arg("--add")
            .arg("--cacheinfo")
            .arg("100644")
            .arg(blob_oid)
            .arg(raw_non_utf8_path);
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "git update-index 注入非 UTF-8 路径必须成功");
        let session_non_utf8 = ctx.open_session();
        let snap_nu = session_non_utf8.refresh().expect("读取非 UTF-8 路径快照失败");
        let entry_nu = snap_nu
            .entries
            .iter()
            .find(|e| e.path.contains('\u{FFFD}'))
            .expect("状态快照中必须包含非 UTF-8 路径条目");
        assert_eq!(entry_nu.readonly_reason, Some(ReadonlyReason::NonUtf8Path));
        assert!(
            session_non_utf8.unstage_file(&entry_nu.path, &entry_nu.fingerprint).is_err(),
            "非 UTF-8 路径取消暂存必须被拒绝"
        );
    }

    // 6. 无权限访问的文件（chmod 000）：标记为 Unreadable，且不可写（审查意见 3）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ctx.write_file("unreadable.txt", b"secret\n");
        let unreadable_path = ctx.path().join("unreadable.txt");
        fs::set_permissions(&unreadable_path, fs::Permissions::from_mode(0o000)).unwrap();

        let session_unreadable = ctx.open_session();
        let snap_unread = session_unreadable.refresh().unwrap();
        let entry_unread = snap_unread.entries.iter().find(|e| e.path == "unreadable.txt").unwrap();
        assert_eq!(entry_unread.readonly_reason, Some(ReadonlyReason::Unreadable));
        assert!(session_unreadable.stage_file(&entry_unread.path, &entry_unread.fingerprint).is_err());

        // 恢复权限以便后续清理
        fs::set_permissions(&unreadable_path, fs::Permissions::from_mode(0o644)).unwrap();
    }

    // 7. 未跟踪文件不能执行 unstage
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

/// T05: 写前核对指纹完整覆盖并断言文案“文件已变化，请重新确认”（审查意见 7）。
/// 包含：目标文件内容变动、仅权限变动、重命名相关变动、未跟踪变动、二进制变动、已暂存侧变动，无关文件改变不拒绝。
#[test]
fn t05_comprehensive_fingerprint_verification() {
    let ctx = TestContext::new();
    ctx.write_file("target.txt", b"v1\n");
    ctx.write_file("other.txt", b"other v1\n");
    ctx.write_file("binary.bin", &[0u8, 255, 128, 64]);
    ctx.write_file("script.sh", b"#!/bin/sh\n");
    let mut rename_content = Vec::new();
    for i in 0..20 {
        rename_content.extend_from_slice(format!("line {}\n", i).as_bytes());
    }
    ctx.write_file("old_renamed.txt", &rename_content);
    ctx.git(&["add", "target.txt", "other.txt", "binary.bin", "script.sh", "old_renamed.txt"]);
    ctx.git(&["commit", "-m", "init"]);

    // 重命名文件
    ctx.git(&["mv", "old_renamed.txt", "new_renamed.txt"]);

    // 工作区修改
    ctx.write_file("target.txt", b"v2\n");
    ctx.write_file("other.txt", b"other v2\n");
    ctx.write_file("binary.bin", &[0u8, 255, 99, 64]);
    ctx.write_file("new_untracked.txt", b"untracked v1\n");
    ctx.write_file("script.sh", b"#!/bin/sh\necho script v2\n");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script_full = ctx.path().join("script.sh");
        fs::set_permissions(&script_full, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let session = ctx.open_session();
    let snap = session.refresh().unwrap();

    let target_entry = snap.entries.iter().find(|e| e.path == "target.txt").unwrap();
    let old_target_fp = target_entry.fingerprint.clone();

    let bin_entry = snap.entries.iter().find(|e| e.path == "binary.bin").unwrap();
    let old_bin_fp = bin_entry.fingerprint.clone();

    let untracked_entry = snap.entries.iter().find(|e| e.path == "new_untracked.txt").unwrap();
    let old_untracked_fp = untracked_entry.fingerprint.clone();

    let script_entry = snap.entries.iter().find(|e| e.path == "script.sh").unwrap();
    let old_script_fp = script_entry.fingerprint.clone();

    let renamed_entry = snap.entries.iter().find(|e| e.path == "new_renamed.txt").unwrap();
    let old_renamed_fp = renamed_entry.fingerprint.clone();

    // 1. 无关文件修改：修改 other.txt，不影响 target.txt 暂存
    ctx.write_file("other.txt", b"other v3 modified\n");
    let snap_after_target = session.stage_file(&target_entry.path, &old_target_fp).expect("无关文件修改不应拒绝目标文件");
    assert!(snap_after_target.entries.iter().any(|e| e.path == "target.txt" && e.changes == ChangesKind::Staged));

    // 2. 目标文件自身内容外部改变：旧指纹必须被拒绝，并提示“文件已变化，请重新确认”！
    ctx.write_file("other.txt", b"other v4 changed again\n");
    let other_stale_entry = snap_after_target.entries.iter().find(|e| e.path == "other.txt" && e.changes == ChangesKind::Unstaged).unwrap();
    let other_stale_fp = other_stale_entry.fingerprint.clone();
    ctx.write_file("other.txt", b"other v5 changed yet again\n");
    let res_content = session.stage_file(&other_stale_entry.path, &other_stale_fp);
    assert!(res_content.is_err());
    assert_eq!(res_content.unwrap_err().to_string(), "文件已变化，请重新确认");

    // 3. 仅权限改变：条目仍存在时只改变权限位（755 -> 644，内容不变），旧指纹被拒绝并提示“文件已变化，请重新确认”（审查意见 3）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script_full = ctx.path().join("script.sh");
        fs::set_permissions(&script_full, fs::Permissions::from_mode(0o644)).unwrap();
        let res_mode = session.stage_file(&script_entry.path, &old_script_fp);
        assert!(res_mode.is_err());
        assert_eq!(res_mode.unwrap_err().to_string(), "文件已变化，请重新确认");
    }

    // 4. 二进制文件外部修改后，旧指纹拒绝并提示“文件已变化，请重新确认”
    ctx.write_file("binary.bin", &[0u8, 255, 77, 64]);
    let bin_res = session.stage_file(&bin_entry.path, &old_bin_fp);
    assert!(bin_res.is_err());
    assert_eq!(bin_res.unwrap_err().to_string(), "文件已变化，请重新确认");

    // 5. 未跟踪文件外部修改后，旧指纹拒绝并提示“文件已变化，请重新确认”
    ctx.write_file("new_untracked.txt", b"untracked v2 modified\n");
    let untracked_res = session.stage_file(&untracked_entry.path, &old_untracked_fp);
    assert!(untracked_res.is_err());
    assert_eq!(untracked_res.unwrap_err().to_string(), "文件已变化，请重新确认");

    // 6. 重命名条目外部修改：在暂存区追加内容（Git 判定为 R85 相似度，仍为重命名条目），旧指纹拒绝并提示“文件已变化，请重新确认”
    let mut updated_rename_content = rename_content.clone();
    updated_rename_content.extend_from_slice(b"extra line to alter blob\n");
    ctx.write_file("new_renamed.txt", &updated_rename_content);
    ctx.git(&["add", "new_renamed.txt"]);

    // 断言外部修改后条目在快照中依然保持为重命名条目（更正要求）
    let snap_after_edit = session.refresh().unwrap();
    let renamed_still = snap_after_edit
        .entries
        .iter()
        .find(|e| e.path == "new_renamed.txt" && e.changes == ChangesKind::Staged)
        .expect("应找到修改后的重命名条目");
    assert_eq!(renamed_still.status, FileStatus::Renamed, "外部修改后条目在快照中依然保持为重命名");
    assert_eq!(renamed_still.original_path.as_deref(), Some("old_renamed.txt"));

    let renamed_res = session.unstage_file(&renamed_entry.path, &old_renamed_fp);
    assert!(renamed_res.is_err());
    assert_eq!(renamed_res.unwrap_err().to_string(), "文件已变化，请重新确认");
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

/// 审查意见 3：带冒号文件名（如 `0:x.txt`）读取暂存区版本，使用带阶段号 `:0:<path>` 确保字面路径准确。
#[test]
fn t03_staged_file_at_literal_colon_paths() {
    let ctx = TestContext::new();
    let file_name = "0:x.txt";
    ctx.write_file(file_name, b"content of 0:x\n");
    ctx.git(&["add", file_name]);
    ctx.git(&["commit", "-m", "init 0:x"]);

    ctx.write_file(file_name, b"content of 0:x v2\n");
    ctx.git(&["add", file_name]);

    let session = ctx.open_session();
    let content_index = session.git.file_at("", file_name);
    assert_eq!(content_index.as_deref(), Some("content of 0:x v2\n"));

    let content_named_index = session.git.file_at("index", file_name);
    assert_eq!(content_named_index.as_deref(), Some("content of 0:x v2\n"));

    let content_head = session.git.file_at("HEAD", file_name);
    assert_eq!(content_head.as_deref(), Some("content of 0:x\n"));
}

/// 审查意见 4：含 `"` 或 `\` 的路径暂存与取消暂存，先反引号再去前缀，保证路径字面量与指纹 100% 匹配。
#[test]
fn t04_stage_and_unstage_quoted_and_backslash_paths() {
    let ctx = TestContext::new();
    let q_name = "q\"x.txt";
    let backslash_name = "back\\slash.txt";
    ctx.write_file(q_name, b"quote content\n");
    ctx.write_file(backslash_name, b"backslash content\n");

    let session = ctx.open_session();
    let snap1 = session.refresh().expect("刷新快照失败");

    let q_entry = snap1
        .entries
        .iter()
        .find(|e| e.path == q_name && e.changes == ChangesKind::Untracked)
        .expect("应找到引号文件名未跟踪条目");
    let bs_entry = snap1
        .entries
        .iter()
        .find(|e| e.path == backslash_name && e.changes == ChangesKind::Untracked)
        .expect("应找到反斜杠文件名未跟踪条目");

    // 暂存含引号的文件
    let snap2 = session
        .stage_file(&q_entry.path, &q_entry.fingerprint)
        .expect("暂存含引号路径文件失败");
    let q_staged = snap2
        .entries
        .iter()
        .find(|e| e.path == q_name && e.changes == ChangesKind::Staged)
        .expect("应移至已暂存");

    // 暂存含反斜杠的文件
    let snap3 = session
        .stage_file(&bs_entry.path, &bs_entry.fingerprint)
        .expect("暂存含反斜杠路径文件失败");
    let bs_staged = snap3
        .entries
        .iter()
        .find(|e| e.path == backslash_name && e.changes == ChangesKind::Staged)
        .expect("应移至已暂存");

    // 验证暂存区内容
    assert_eq!(ctx.read_file(q_name), b"quote content\n");
    assert_eq!(ctx.read_file(backslash_name), b"backslash content\n");

    // 取消暂存
    let snap4 = session
        .unstage_file(&q_staged.path, &q_staged.fingerprint)
        .expect("取消暂存含引号路径文件失败");
    assert!(snap4.entries.iter().any(|e| e.path == q_name && e.changes == ChangesKind::Untracked));

    let snap5 = session
        .unstage_file(&bs_staged.path, &bs_staged.fingerprint)
        .expect("取消暂存含反斜杠路径文件失败");
    assert!(snap5.entries.iter().any(|e| e.path == backslash_name && e.changes == ChangesKind::Untracked));
}
