//! 集成测试：工作区编辑、保存核验与草稿（M7 任务 03）。
//!
//! 依据 M7 规格与既有测试约定：
//! - 从应用操作入口（`m7::edit::open` / `save` / `EditDraftStore`）走真实 Git 与临时目录；
//! - 完全隔离用户身份与全局配置（HOME、GIT_CONFIG_GLOBAL、GIT_CONFIG_NOSYSTEM）；
//! - 覆盖验收项中的“真实自建 Git 验证保存、外部修改、重启恢复、权限与跨目标失败”。

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use revia_lib::git::CommandLog;
use revia_lib::m7::edit::{self, EditDraft, EditDraftStore, EditNewline, EditReadonlyReason, EditSaveRequest};
use revia_lib::session::WorktreeSession;
use revia_lib::shell::ShellEnv;

struct TestContext {
    _temp_dir: tempfile::TempDir,
    repo_path: PathBuf,
    fake_home: PathBuf,
    global_config: PathBuf,
    data_dir: PathBuf,
}

impl TestContext {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("创建临时目录失败");
        let repo_path = temp_dir.path().join("repo");
        let fake_home = temp_dir.path().join("fake_home");
        let global_config = fake_home.join(".gitconfig");
        let data_dir = temp_dir.path().join("appdata");
        fs::create_dir_all(&repo_path).expect("创建仓库目录失败");
        fs::create_dir_all(&fake_home).expect("创建假用户主目录失败");
        fs::create_dir_all(&data_dir).expect("创建应用数据目录失败");
        fs::write(&global_config, "").expect("创建空全局配置失败");

        let ctx = Self {
            _temp_dir: temp_dir,
            repo_path,
            fake_home,
            global_config,
            data_dir,
        };

        ctx.git(&["init", "-b", "main"]);
        ctx.git(&["config", "user.name", "Revia Test"]);
        ctx.git(&["config", "user.email", "revia-test@example.invalid"]);
        ctx.git(&["config", "commit.gpgsign", "false"]);
        ctx.git(&["config", "core.quotePath", "false"]);
        ctx
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
        fs::write(&full, content).unwrap_or_else(|e| panic!("写入文件 {rel_path} 失败: {e}"));
    }

    fn read_file(&self, rel_path: &str) -> Vec<u8> {
        fs::read(self.repo_path.join(rel_path))
            .unwrap_or_else(|e| panic!("读取文件 {rel_path} 失败: {e}"))
    }

    fn commit_all(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-m", message]);
    }

    fn open_session(&self) -> Arc<WorktreeSession> {
        self.open_session_at(&self.repo_path)
    }

    fn open_session_at(&self, path: &Path) -> Arc<WorktreeSession> {
        let mut vars = std::collections::HashMap::new();
        vars.insert("HOME".into(), self.fake_home.display().to_string());
        vars.insert(
            "GIT_CONFIG_GLOBAL".into(),
            self.global_config.display().to_string(),
        );
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
                worktree: path.display().to_string(),
                shell: shell_name.into(),
                elapsed_ms: 1,
                timed_out: false,
                used_fallback: true,
                var_count: 3,
                path_entries,
                diagnostics: None,
            },
        };
        WorktreeSession::open(
            "test-worktree",
            path,
            shell_env,
            Arc::new(CommandLog::default()),
        )
        .expect("打开 WorktreeSession 失败")
    }
}

/// 普通文本文件可以打开，返回正文与基线指纹；保存后磁盘内容更新且指纹前进。
#[test]
fn open_and_save_round_trip_on_real_worktree() {
    let ctx = TestContext::new();
    ctx.write_file("src/hello.txt", b"line one\nline two\n");
    ctx.commit_all("initial");

    let session = ctx.open_session();
    let opened = edit::open(&session, "src/hello.txt").expect("打开文件失败");
    assert_eq!(opened.readonly_reason, None, "普通文本不应只读");
    assert_eq!(opened.text.as_deref(), Some("line one\nline two\n"));
    assert_eq!(opened.newline, EditNewline::Lf);
    assert!(!opened.has_bom);
    let base = opened.base_sha256.clone().expect("应有基线指纹");

    let saved = edit::save(
        &session,
        &EditSaveRequest {
            worktree_id: session.id.clone(),
            path: "src/hello.txt".into(),
            base_sha256: base.clone(),
            text: "line one\nline two changed\n".into(),
        },
    )
    .expect("保存失败");

    assert_eq!(ctx.read_file("src/hello.txt"), b"line one\nline two changed\n");
    assert_ne!(saved.sha256, base, "保存后指纹必须前进");
    assert_eq!(saved.bytes, 26, "保存后字节数应与新正文一致");

    // 保存后的新指纹可以再次保存；旧指纹则被拒绝。
    let reopened = edit::open(&session, "src/hello.txt").expect("重新打开失败");
    assert_eq!(reopened.base_sha256.as_deref(), Some(saved.sha256.as_str()));

    let stale = edit::save(
        &session,
        &EditSaveRequest {
            worktree_id: session.id.clone(),
            path: "src/hello.txt".into(),
            base_sha256: base,
            text: "overwrite from a stale tab\n".into(),
        },
    );
    let stale_error = stale.expect_err("旧指纹保存必须被拒绝");
    assert_eq!(stale_error.code, Some("errors.m7.editFileChangedOnDisk"));
    assert_eq!(
        ctx.read_file("src/hello.txt"),
        b"line one\nline two changed\n",
        "拒绝保存时磁盘内容不得变化"
    );
}

/// 外部修改后保存被拒绝，且错误里带磁盘当前指纹，便于界面比较两版。
#[test]
fn external_change_rejects_save_and_reports_disk_sha() {
    let ctx = TestContext::new();
    ctx.write_file("notes.md", b"original\n");
    ctx.commit_all("initial");

    let session = ctx.open_session();
    let opened = edit::open(&session, "notes.md").expect("打开文件失败");
    let base = opened.base_sha256.expect("应有基线指纹");

    ctx.write_file("notes.md", b"changed outside revia\n");

    let error = edit::save(
        &session,
        &EditSaveRequest {
            worktree_id: session.id.clone(),
            path: "notes.md".into(),
            base_sha256: base,
            text: "draft text\n".into(),
        },
    )
    .expect_err("外部修改后保存必须被拒绝");
    assert_eq!(error.code, Some("errors.m7.editFileChangedOnDisk"));
    assert_eq!(
        ctx.read_file("notes.md"),
        b"changed outside revia\n",
        "外部内容必须保留"
    );

    // 按刷新后的基线重试即可保存：这正是界面上“重新加载后继续”的路径。
    let refreshed = edit::open(&session, "notes.md").expect("重新打开失败");
    edit::save(
        &session,
        &EditSaveRequest {
            worktree_id: session.id.clone(),
            path: "notes.md".into(),
            base_sha256: refreshed.base_sha256.expect("应有基线指纹"),
            text: "draft text\n".into(),
        },
    )
    .expect("刷新基线后保存应成功");
    assert_eq!(ctx.read_file("notes.md"), b"draft text\n");
}

/// 既有换行风格、BOM 与可执行位在保存后保持不变。
#[test]
fn save_preserves_crlf_bom_and_executable_bit() {
    let ctx = TestContext::new();
    ctx.write_file("win/crlf.txt", b"\xEF\xBB\xBFalpha\r\nbeta\r\n");
    ctx.commit_all("initial");

    let session = ctx.open_session();
    let opened = edit::open(&session, "win/crlf.txt").expect("打开文件失败");
    assert_eq!(opened.newline, EditNewline::Crlf, "应识别 CRLF");
    assert!(opened.has_bom, "应识别 BOM");
    assert_eq!(opened.text.as_deref(), Some("alpha\r\nbeta\r\n"));

    let saved = edit::save(
        &session,
        &EditSaveRequest {
            worktree_id: session.id.clone(),
            path: "win/crlf.txt".into(),
            base_sha256: opened.base_sha256.expect("应有基线指纹"),
            text: "alpha\r\ngamma\r\n".into(),
        },
    )
    .expect("保存失败");
    assert_eq!(saved.newline, EditNewline::Crlf);
    let raw = ctx.read_file("win/crlf.txt");
    assert_eq!(raw, b"\xEF\xBB\xBFalpha\r\ngamma\r\n", "BOM 与 CRLF 必须保留");

    #[cfg(unix)]
    {
        let script = ctx.repo_path.join("run.sh");
        fs::write(&script, b"#!/bin/sh\necho one\n").expect("写入脚本失败");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("设置权限失败");
        ctx.commit_all("script");

        let opened = edit::open(&session, "run.sh").expect("打开脚本失败");
        assert!(opened.executable, "应识别可执行位");
        let saved = edit::save(
            &session,
            &EditSaveRequest {
                worktree_id: session.id.clone(),
                path: "run.sh".into(),
                base_sha256: opened.base_sha256.expect("应有基线指纹"),
                text: "#!/bin/sh\necho two\n".into(),
            },
        )
        .expect("保存脚本失败");
        assert!(saved.executable, "保存后应保留可执行位");
        let mode = fs::metadata(&script).expect("读取元数据失败").permissions().mode();
        assert_eq!(mode & 0o777, 0o755, "权限位不得被保存改写");
    }
}

/// 各种非安全文本给出对应只读原因，且不写入磁盘。
#[test]
fn readonly_reasons_match_file_kind() {
    let ctx = TestContext::new();
    ctx.write_file("ok.txt", b"text\n");
    ctx.write_file("binary.bin", b"a\0b\n");
    ctx.write_file("big.txt", &vec![b'x'; (edit::MAX_EDIT_BYTES + 1) as usize]);
    ctx.commit_all("initial");

    let session = ctx.open_session();

    let binary = edit::open(&session, "binary.bin").expect("打开二进制失败");
    assert_eq!(binary.readonly_reason, Some(EditReadonlyReason::Binary));
    assert!(binary.text.is_none());

    let big = edit::open(&session, "big.txt").expect("打开大文件失败");
    assert_eq!(big.readonly_reason, Some(EditReadonlyReason::TooLarge));

    let missing = edit::open(&session, "not-there.txt").expect("打开缺失文件失败");
    assert_eq!(missing.readonly_reason, Some(EditReadonlyReason::Missing));

    let outside = edit::open(&session, "../escape.txt").expect("打开越界路径失败");
    assert_eq!(outside.readonly_reason, Some(EditReadonlyReason::NotInWorktree));

    #[cfg(unix)]
    {
        let target = ctx.repo_path.join("ok.txt");
        let link = ctx.repo_path.join("link.txt");
        std::os::unix::fs::symlink(&target, &link).expect("创建符号链接失败");
        let symlink = edit::open(&session, "link.txt").expect("打开符号链接失败");
        assert_eq!(symlink.readonly_reason, Some(EditReadonlyReason::Symlink));

        // 越界与符号链接都不能被写穿。
        let error = edit::save(
            &session,
            &EditSaveRequest {
                worktree_id: session.id.clone(),
                path: "link.txt".into(),
                base_sha256: "0".repeat(64),
                text: "written through symlink\n".into(),
            },
        )
        .expect_err("符号链接保存必须被拒绝");
        assert_eq!(error.code, Some("errors.m7.editSymlinkReadonly"));
        assert_eq!(ctx.read_file("ok.txt"), b"text\n", "链接目标不得被改写");
    }
}

/// 草稿按 worktree + 路径唯一，跨重启（新 store 实例）仍能取回，也能单独清除。
#[test]
fn draft_store_survives_restart_and_is_scoped() {
    let ctx = TestContext::new();
    let store = EditDraftStore::new(&ctx.data_dir);

    let draft = EditDraft {
        worktree_id: "test-worktree".into(),
        path: "src/hello.txt".into(),
        base_sha256: "a".repeat(64),
        text: "unsaved text\n".into(),
        updated_at_ms: 1_700_000_000_000,
    };
    store.save(&draft).expect("保存草稿失败");

    let other = EditDraft {
        worktree_id: "other-worktree".into(),
        path: "src/hello.txt".into(),
        base_sha256: "b".repeat(64),
        text: "other worktree\n".into(),
        updated_at_ms: 1_700_000_000_500,
    };
    store.save(&other).expect("保存另一 worktree 草稿失败");

    // 模拟重启：新的 store 实例指向同一数据目录。
    let restarted = EditDraftStore::new(&ctx.data_dir);
    let loaded = restarted
        .load("test-worktree", "src/hello.txt")
        .expect("读取草稿失败")
        .expect("重启后草稿必须还在");
    assert_eq!(loaded.text, "unsaved text\n");
    assert_eq!(loaded.base_sha256, "a".repeat(64));

    let listed = restarted.list("test-worktree").expect("列出草稿失败");
    assert_eq!(listed.len(), 1, "草稿必须按 worktree 隔离");
    assert_eq!(listed[0].path, "src/hello.txt");

    restarted
        .clear("test-worktree", "src/hello.txt")
        .expect("清除草稿失败");
    assert!(
        restarted
            .load("test-worktree", "src/hello.txt")
            .expect("读取草稿失败")
            .is_none(),
        "清除后不得再取回"
    );
    assert!(
        restarted
            .load("other-worktree", "src/hello.txt")
            .expect("读取草稿失败")
            .is_some(),
        "清除只影响同一 worktree 的同一路径"
    );
}
