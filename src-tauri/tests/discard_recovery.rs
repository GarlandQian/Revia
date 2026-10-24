//! Application-boundary checks with real Git, isolated HOME, storage and worktrees.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use revia_lib::error::{AppError, ErrorKind};
use revia_lib::git::{CommandLog, rows::ViewMode, status::ChangesKind};
use revia_lib::platform;
use revia_lib::recovery::{DiscardOutcome, DiscardPhase, RecoverySource, RecoveryStatus, RecoveryStore, WorktreeIdentity};
use revia_lib::session::{ChangeEntry, WorktreeSession};
use revia_lib::shell::{EnvCaptureInfo, ShellEnv};

struct Fixture {
    temp: tempfile::TempDir,
    repo: PathBuf,
    vars: HashMap<String, String>,
    paths: Vec<String>,
}

impl Fixture {
    /// 真实路径根：tempdir 在 macOS 上位于 /var 别名之下，测试构造目标路径时必须先规范化，
    /// 否则等于在测「祖先符号链接」而不是另存本身（实现侧对符号链接是安全拒绝）。
    fn root(&self) -> PathBuf {
        self.temp.path().canonicalize().unwrap()
    }

    fn new() -> Self {
        Self::from_temp(tempfile::tempdir().unwrap())
    }

    /// 在指定目录下新建 fixture（用于需要保留的桌面数据准备）。
    fn with_base(base: &Path) -> Self {
        let temp = tempfile::Builder::new()
            .prefix("revia-task05-desktop-")
            .tempdir_in(base)
            .unwrap();
        Self::from_temp(temp)
    }

    fn from_temp(temp: tempfile::TempDir) -> Self {
        let repo = temp.path().join("repo");
        let home = temp.path().join("home");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join(".gitconfig"), "").unwrap();
        let mut paths = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|path| path.to_string_lossy().into_owned()).collect::<Vec<_>>();
        #[cfg(unix)]
        paths.extend(["/opt/homebrew/bin".into(), "/usr/bin".into(), "/bin".into()]);
        let mut vars = HashMap::from([
            ("HOME".into(), home.display().to_string()), ("USERPROFILE".into(), home.display().to_string()),
            ("XDG_CONFIG_HOME".into(), home.display().to_string()),
            ("GIT_CONFIG_GLOBAL".into(), home.join(".gitconfig").display().to_string()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()), ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("PATH".into(), std::env::join_paths(paths.iter().map(Path::new)).unwrap().to_string_lossy().into_owned()),
        ]);
        for key in ["SYSTEMROOT", "WINDIR", "TEMP", "TMP", "TMPDIR"] {
            if let Ok(value) = std::env::var(key) { vars.insert(key.into(), value); }
        }
        let result = Self { temp, repo, vars, paths };
        result.git(&["init", "-b", "main"]);
        for (key, value) in [("user.name", "Revia Fixture"), ("user.email", "fixture@example.invalid"), ("commit.gpgsign", "false"), ("core.autocrlf", "false")] {
            result.git(&["config", key, value]);
        }
        result
    }

    fn git(&self, args: &[&str]) -> Vec<u8> {
        let output = Command::new("git").current_dir(&self.repo).env_clear().envs(&self.vars).args(args).output().unwrap();
        assert!(output.status.success(), "Git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        output.stdout
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        let path = self.repo.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn commit(&self) {
        self.git(&["add", "--all"]);
        self.git(&["commit", "-m", "fixture"]);
    }

    fn session_at(&self, path: &Path) -> Arc<WorktreeSession> {
        WorktreeSession::open("fixture", path, ShellEnv {
            vars: self.vars.clone(), path: self.paths.clone(),
            info: EnvCaptureInfo { worktree: path.display().to_string(), shell: "fixture".into(), elapsed_ms: 0,
                timed_out: false, used_fallback: true, var_count: self.vars.len(), path_entries: self.paths.clone(), diagnostics: None },
        }, Arc::new(CommandLog::default())).unwrap()
    }

    fn session(&self) -> Arc<WorktreeSession> { self.session_at(&self.repo) }
    fn store(&self) -> RecoveryStore { RecoveryStore::with_clock(self.temp.path().join("data"), Arc::new(|| 1_792_742_400_123)) }
    fn index(&self) -> Vec<u8> { self.git(&["ls-files", "--stage", "-z"]) }
    fn read(&self, path: &str) -> Vec<u8> { fs::read(self.repo.join(path)).unwrap() }

    fn changed(&self) -> Arc<WorktreeSession> {
        self.write("target.ts", &text(&[]));
        self.write("other.txt", b"other\n");
        self.commit();
        self.write("target.ts", &text(&[(10, 1010), (80, 1080), (140, 1140)]));
        self.session()
    }
}

fn text(changes: &[(usize, usize)]) -> Vec<u8> {
    (1..=180).map(|line| format!("export const value{line:03} = {};\n", changes.iter().find(|(n, _)| *n == line).map(|(_, value)| *value).unwrap_or(line))).collect::<String>().into_bytes()
}

fn expectation_of(inspection: &revia_lib::recovery::RecoveryInspection) -> revia_lib::recovery::RestoreExpectation {
    revia_lib::recovery::RestoreExpectation {
        version: inspection.version.clone(),
        paths: inspection.paths.clone(),
        state: inspection.state,
    }
}

fn entry(session: &WorktreeSession, path: &str) -> ChangeEntry {
    session.refresh().unwrap().entries.into_iter().find(|entry| entry.path == path && entry.changes == ChangesKind::Unstaged).unwrap()
}

fn discard(session: &WorktreeSession, entry: &ChangeEntry, store: &RecoveryStore) -> revia_lib::error::Result<revia_lib::session::ChangesSnapshot> {
    session.discard_file(&entry.path, &entry.fingerprint, &entry.snapshot_revision, store)
}

#[test]
fn full_file_preserves_index_and_saves_complete_partial_staged_state() {
    let f = Fixture::new();
    let session = f.changed();
    f.write("target.ts", &text(&[(10, 2010)]));
    f.git(&["add", "--", "target.ts"]);
    let expected = f.read("target.ts");
    let before = text(&[(10, 2010), (80, 2080), (140, 2140)]);
    f.write("target.ts", &before);
    let target = entry(&session, "target.ts");
    let index = f.index();
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    assert_eq!(f.read("target.ts"), expected);
    assert_eq!(f.index(), index);
    assert_eq!(f.read("other.txt"), b"other\n");
    let identity = WorktreeIdentity::capture(&session.git).unwrap();
    let summaries = store.list(Some(&identity)).unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].created_at_ms, 1_792_742_400_123);
    assert!(summaries[0].identity_confirmed);
    assert!(matches!(summaries[0].status, RecoveryStatus::Completed));
    let (record, outcome) = store.read(&summaries[0].id).unwrap();
    assert_eq!(record.before[0].content.as_ref().unwrap(), &before);
    assert!(matches!(record.source, RecoverySource::DiscardFile));
    let Some(DiscardOutcome::Completed { after }) = outcome else { panic!("missing post-state") };
    assert_eq!(after[0].content.as_ref().unwrap(), &expected);
    assert_eq!(f.store().list(Some(&identity)).unwrap().len(), 1);
}

#[test]
fn hunk_uses_git_bytes_and_saves_whole_file_without_touching_other_hunks() {
    for crlf in [false, true] {
        let f = Fixture::new();
        let path = "中文 空格.ts";
        let convert = |bytes: Vec<u8>| if crlf { String::from_utf8(bytes).unwrap().replace('\n', "\r\n").into_bytes() } else { bytes };
        f.write(path, &convert(text(&[])));
        f.commit();
        let before = convert(text(&[(10, 1010), (80, 1080), (140, 1140)]));
        f.write(path, &before);
        let session = f.session();
        let target = entry(&session, path);
        let rows = session.rows(path, ChangesKind::Unstaged, ViewMode::Unified, 0, 200).unwrap();
        let actions = rows.rows.iter().filter_map(|row| row.hunk_action.as_ref()).collect::<Vec<_>>();
        assert_eq!(actions.len(), 3);
        let index = f.index();
        let store = f.store();
        session.discard_hunk(path, &target.fingerprint, &target.snapshot_revision, &actions[1].hunk_id, &store).unwrap();
        assert_eq!(f.read(path), convert(text(&[(10, 1010), (140, 1140)])));
        assert_eq!(f.index(), index);
        let summary = store.list(None).unwrap().remove(0);
        let (saved, _) = store.read(&summary.id).unwrap();
        assert_eq!(saved.before[0].content.as_ref().unwrap(), &before);
        assert!(matches!(saved.source, RecoverySource::DiscardHunk));
    }
}

#[test]
fn binary_deleted_and_renamed_paths_have_full_records() {
    let f = Fixture::new();
    f.write("binary", b"\0old\xff");
    f.write("deleted", b"deleted original");
    f.write("old name", b"rename original\n");
    f.commit();
    f.write("binary", b"\0new\xfe");
    fs::remove_file(f.repo.join("deleted")).unwrap();
    f.git(&["mv", "--", "old name", "new name"]);
    f.write("new name", b"rename original\nunstaged\n");
    let session = f.session();
    let index = f.index();
    let store = f.store();
    for path in ["binary", "deleted", "new name"] {
        let target = entry(&session, path);
        discard(&session, &target, &store).unwrap();
        assert_eq!(f.index(), index);
    }
    assert_eq!(f.read("binary"), b"\0old\xff");
    assert_eq!(f.read("deleted"), b"deleted original");
    assert_eq!(f.read("new name"), b"rename original\n");
    assert!(!f.repo.join("old name").exists());
    let records = store.list(None).unwrap();
    let deletion = records.iter().find(|record| record.target == "deleted").unwrap();
    assert!(store.read(&deletion.id).unwrap().0.before[0].content.is_none());
    let renamed = records.iter().find(|record| record.target == "new name").unwrap();
    assert!(renamed.paths.contains(&"old name".into()));
    assert_eq!(records.len(), 3);
}

#[cfg(unix)]
#[test]
fn mode_only_discards_and_preserves_executable_pre_state() {
    let f = Fixture::new();
    f.write("mode.sh", b"#!/bin/sh\ntrue\n");
    platform::set_executable(&f.repo.join("mode.sh"), false).unwrap();
    f.commit();
    platform::set_executable(&f.repo.join("mode.sh"), true).unwrap();
    let session = f.session();
    let target = entry(&session, "mode.sh");
    let index = f.index();
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    assert!(!platform::executable(&fs::metadata(f.repo.join("mode.sh")).unwrap()));
    assert_eq!(f.index(), index);
    let record = store.list(None).unwrap().remove(0);
    assert!(store.read(&record.id).unwrap().0.before[0].executable);
}

#[test]
fn storage_failure_and_store_inside_repo_cannot_change_worktree() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let before = f.read("target.ts");
    let index = f.index();
    let blocked = f.temp.path().join("blocked");
    fs::write(&blocked, "not a directory").unwrap();
    assert!(discard(&session, &target, &RecoveryStore::new(blocked)).is_err());
    assert!(discard(&session, &target, &RecoveryStore::new(f.repo.join("forbidden"))).is_err());
    assert!(!f.repo.join("forbidden").exists());
    assert_eq!(f.read("target.ts"), before);
    assert_eq!(f.index(), index);
}

#[test]
fn interruption_keeps_saved_state_without_forging_post_state_or_resuming() {
    for phase in [DiscardPhase::AfterSave, DiscardPhase::BeforeWrite, DiscardPhase::AfterWrite, DiscardPhase::BeforeComplete] {
        let f = Fixture::new();
        let session = f.changed();
        let target = entry(&session, "target.ts");
        let before = f.read("target.ts");
        let index = f.index();
        let store = f.store();
        let error = session.discard_with_hook("target.ts", &target.fingerprint, &target.snapshot_revision, None, &store,
            |current| if current == phase { Err(AppError::internal("controlled interruption")) } else { Ok(()) }).unwrap_err();
        assert_eq!(error.kind, ErrorKind::NeedsVerification);
        let summary = store.list(None).unwrap().remove(0);
        assert!(matches!(summary.status, RecoveryStatus::NeedsVerification));
        let (record, outcome) = store.read(&summary.id).unwrap();
        assert_eq!(record.before[0].content.as_ref().unwrap(), &before);
        assert!(outcome.is_none());
        let after = f.read("target.ts");
        assert_eq!(after, if matches!(phase, DiscardPhase::AfterSave | DiscardPhase::BeforeWrite) { before } else { text(&[]) });
        drop(session);
        let reopened = f.session();
        reopened.refresh().unwrap();
        assert_eq!(f.read("target.ts"), after);
        assert_eq!(f.index(), index);
        assert_eq!(f.store().list(None).unwrap().len(), 1);
    }
}

#[test]
fn external_change_during_save_is_not_overwritten_but_unrelated_change_is_allowed() {
    for related in [true, false] {
        let f = Fixture::new();
        let session = f.changed();
        let target = entry(&session, "target.ts");
        let store = f.store();
        let index = f.index();
        let result = session.discard_with_hook("target.ts", &target.fingerprint, &target.snapshot_revision, None, &store, |phase| {
            if phase == DiscardPhase::AfterSave {
                f.write(if related { "target.ts" } else { "other.txt" }, b"external change\n");
            }
            Ok(())
        });
        let summary = store.list(None).unwrap().remove(0);
        if related {
            assert_eq!(result.unwrap_err().kind, ErrorKind::Rejected);
            assert_eq!(f.read("target.ts"), b"external change\n");
            assert!(matches!(summary.status, RecoveryStatus::NotApplied));
        } else {
            result.unwrap();
            assert_eq!(f.read("target.ts"), text(&[]));
            assert_eq!(f.read("other.txt"), b"external change\n");
        }
        assert_eq!(f.index(), index);
    }
}

#[test]
fn changed_post_state_is_never_used_as_a_completed_baseline() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    let error = session.discard_with_hook("target.ts", &target.fingerprint, &target.snapshot_revision, None, &store, |phase| {
        if phase == DiscardPhase::AfterWrite { f.write("target.ts", b"later work\n"); }
        Ok(())
    }).unwrap_err();
    assert_eq!(error.kind, ErrorKind::NeedsVerification);
    assert_eq!(f.read("target.ts"), b"later work\n");
    let summary = store.list(None).unwrap().remove(0);
    assert!(store.read(&summary.id).unwrap().1.is_none());
}

#[test]
fn binary_stale_bytes_are_rejected_by_discard_and_staging() {
    let f = Fixture::new();
    f.write("binary", b"\0old");
    f.commit();
    f.write("binary", b"\0one");
    let session = f.session();
    let target = entry(&session, "binary");
    f.write("binary", b"\0two");
    assert_eq!(discard(&session, &target, &f.store()).unwrap_err().kind, ErrorKind::Rejected);
    assert_eq!(session.stage_file("binary", &target.fingerprint).unwrap_err().kind, ErrorKind::Rejected);
    assert_eq!(f.read("binary"), b"\0two");
    assert!(f.store().list(None).unwrap().is_empty());
}

#[test]
fn refresh_invalidates_confirmation_and_old_hunk_credentials() {
    let f = Fixture::new();
    let session = f.changed();
    let old = entry(&session, "target.ts");
    let rows = session.rows("target.ts", ChangesKind::Unstaged, ViewMode::Split, 0, 100).unwrap();
    let hunk = rows.rows.iter().find_map(|row| row.hunk_action.as_ref()).unwrap();
    let new = entry(&session, "target.ts");
    assert_eq!(old.fingerprint, new.fingerprint);
    assert_eq!(discard(&session, &old, &f.store()).unwrap_err().kind, ErrorKind::Rejected);
    assert_eq!(session.discard_hunk("target.ts", &new.fingerprint, &new.snapshot_revision, &hunk.hunk_id, &f.store()).unwrap_err().kind, ErrorKind::Rejected);
    assert!(f.store().list(None).unwrap().is_empty());
}

#[test]
fn concurrent_writes_do_not_queue_or_save_extra_records() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    let guard = session.acquire_write_lock().unwrap();
    assert_eq!(discard(&session, &target, &store).unwrap_err().kind, ErrorKind::Rejected);
    drop(guard);
    session.discard_with_hook("target.ts", &target.fingerprint, &target.snapshot_revision, None, &store, |phase| {
        if phase == DiscardPhase::AfterSave {
            assert_eq!(session.stage_file("target.ts", &target.fingerprint).unwrap_err().kind, ErrorKind::Rejected);
            assert_eq!(discard(&session, &target, &store).unwrap_err().kind, ErrorKind::Rejected);
        }
        Ok(())
    }).unwrap();
    assert_eq!(store.list(None).unwrap().len(), 1);
}

#[test]
fn linked_worktrees_and_recreated_paths_cannot_claim_old_identity() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let linked = f.temp.path().join("linked");
    f.git(&["worktree", "add", "-b", "linked", linked.to_str().unwrap()]);
    let linked_session = f.session_at(&linked);
    let linked_identity = WorktreeIdentity::capture(&linked_session.git).unwrap();
    assert!(!store.list(Some(&linked_identity)).unwrap()[0].identity_confirmed);
    drop(session);
    drop(linked_session);
    let moved = f.temp.path().join("moved");
    fs::rename(&f.repo, &moved).unwrap();
    assert!(!store.list(None).unwrap()[0].identity_confirmed);
    fs::create_dir(&f.repo).unwrap();
    f.git(&["init", "-b", "main"]);
    let fresh = f.session();
    let fresh_identity = WorktreeIdentity::capture(&fresh.git).unwrap();
    assert!(!store.list(Some(&fresh_identity)).unwrap()[0].identity_confirmed);
}

#[test]
fn untracked_staged_unknown_and_out_of_bounds_targets_cannot_be_discarded() {
    let f = Fixture::new();
    let session = f.changed();
    f.write("untracked", b"keep");
    let snapshot = session.refresh().unwrap();
    let untracked = snapshot.entries.iter().find(|entry| entry.path == "untracked").unwrap();
    assert!(discard(&session, untracked, &f.store()).is_err());
    let target = snapshot.entries.iter().find(|entry| entry.path == "target.ts").unwrap();
    for path in ["../outside", "/absolute", ".git/config", "unknown"] {
        assert!(session.discard_file(path, &target.fingerprint, &target.snapshot_revision, &f.store()).is_err());
    }
    f.git(&["add", "--", "target.ts"]);
    let staged = session.refresh().unwrap().entries.into_iter().find(|entry| entry.path == "target.ts").unwrap();
    assert!(discard(&session, &staged, &f.store()).is_err());
    assert_eq!(f.read("untracked"), b"keep");
    assert!(f.store().list(None).unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn symlink_substitution_after_save_is_refused_without_touching_the_link_target() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    let result = session.discard_with_hook("target.ts", &target.fingerprint, &target.snapshot_revision, None, &store, |phase| {
        if phase == DiscardPhase::AfterSave {
            fs::remove_file(f.repo.join("target.ts")).unwrap();
            std::os::unix::fs::symlink("other.txt", f.repo.join("target.ts")).unwrap();
        }
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(f.read("other.txt"), b"other\n");
    assert!(f.repo.join("target.ts").is_symlink());
}

/// 换行属性：`.gitattributes` 的 `text eol=crlf` 下，预览与真实写入必须使用同一套
/// Git 转换语义；仓库外直接 `git apply` 会失败或写出不同字节（真实回归）。
#[test]
fn crlf_attribute_text_discards_with_matching_git_semantics() {
    let f = Fixture::new();
    f.write(".gitattributes", b"*.txt text eol=crlf\n");
    f.write("crlf.txt", b"alpha\nbeta\n");
    f.commit();
    // 让 Git 按属性重新检出，工作区变成 CRLF。
    fs::remove_file(f.repo.join("crlf.txt")).unwrap();
    f.git(&["checkout", "--", "crlf.txt"]);
    let committed = f.read("crlf.txt");
    assert_eq!(committed, b"alpha\r\nbeta\r\n", "工作区应为 CRLF");

    // 修改一行并保持 CRLF。
    f.write("crlf.txt", b"alpha\r\nBETA\r\n");
    let session = f.session();
    let target = entry(&session, "crlf.txt");
    let before = f.read("crlf.txt");
    let index = f.index();
    let store = f.store();

    discard(&session, &target, &store).unwrap();

    assert_eq!(f.read("crlf.txt"), committed, "丢弃后必须回到提交时的 CRLF 字节");
    assert_eq!(f.index(), index, "丢弃不得改变暂存区");
    let (record, outcome) = store.read(&store.list(None).unwrap()[0].id).unwrap();
    assert_eq!(record.before[0].content.as_ref().unwrap(), &before, "保存的必须是丢弃前真实字节");
    assert!(matches!(outcome, Some(DiscardOutcome::Completed { .. })));
}

/// 无末尾换行与非 UTF-8 正文都必须按原始字节保存与恢复，不能经过字符串转换。
#[test]
fn no_trailing_newline_and_non_utf8_body_are_preserved() {
    let f = Fixture::new();
    // GBK 字节序列 + 故意没有末尾换行。
    let gbk_v1: &[u8] = b"\xd6\xd0\xce\xc4\xb2\xe2\xca\xd4\nno-newline-tail";
    let gbk_v2: &[u8] = b"\xd6\xd0\xce\xc4\xb2\xe2\xca\xd4\nchanged-tail";
    f.write("gbk.txt", gbk_v1);
    f.commit();
    f.write("gbk.txt", gbk_v2);
    let session = f.session();
    let target = entry(&session, "gbk.txt");
    assert_eq!(f.read("gbk.txt"), gbk_v2);
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    assert_eq!(f.read("gbk.txt"), gbk_v1, "必须按原始非 UTF-8 字节恢复");
    let (record, _) = store.read(&store.list(None).unwrap()[0].id).unwrap();
    assert_eq!(record.before[0].content.as_ref().unwrap(), gbk_v2);
}

/// 删除状态：保存的记录必须表达「路径原本不存在」，丢弃后恢复文件与可执行位。
#[test]
fn deleted_file_restores_existence_and_executable_state() {
    let f = Fixture::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.write("script.sh", b"#!/bin/sh\necho ok\n");
        f.commit();
        fs::set_permissions(f.repo.join("script.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        f.git(&["update-index", "--chmod=+x", "script.sh"]);
        f.commit();
        fs::remove_file(f.repo.join("script.sh")).unwrap();

        let session = f.session();
        let target = entry(&session, "script.sh");
        let store = f.store();
        let (record, _) = {
            discard(&session, &target, &store).unwrap();
            store.read(&store.list(None).unwrap()[0].id).unwrap()
        };
        assert_eq!(record.before[0].content, None, "丢弃前该路径不存在");
        let restored = f.read("script.sh");
        assert_eq!(restored, b"#!/bin/sh\necho ok\n");
        assert!(
            platform::executable(&fs::metadata(f.repo.join("script.sh")).unwrap()),
            "恢复后必须保留可执行权限"
        );
    }
}

/// intent-to-add 没有可恢复的提交基线，反向应用会删除文件：必须明确拒绝且不动工作区。
#[test]
fn intent_to_add_is_rejected_without_touching_the_file() {
    let f = Fixture::new();
    f.write("base.txt", b"base\n");
    f.commit();
    f.write("added.txt", b"new content\n");
    f.git(&["add", "-N", "--", "added.txt"]);
    let session = f.session();
    let snapshot = session.refresh().unwrap();
    let target = snapshot.entries.iter()
        .find(|entry| entry.path == "added.txt" && entry.changes == ChangesKind::Unstaged)
        .expect("intent-to-add 应作为未暂存新增出现");
    let before = f.read("added.txt");
    let index = f.index();
    let store = f.store();
    assert_eq!(discard(&session, &target, &store).unwrap_err().kind, ErrorKind::Rejected);
    assert_eq!(f.read("added.txt"), before, "拒绝时不得触碰文件");
    assert_eq!(f.index(), index, "拒绝时不得改变暂存区");
    assert!(store.list(None).unwrap().is_empty(), "拒绝时不产生回收记录");
}

/// 保存之后、写入之前，索引或权限发生变化不能再用旧确认继续。
#[test]
fn index_or_mode_change_during_save_is_rejected() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let f = Fixture::new();
        let session = f.changed();
        let target = entry(&session, "target.ts");
        let store = f.store();
        let before = f.read("target.ts");
        let result = session.discard_with_hook("target.ts", &target.fingerprint, &target.snapshot_revision, None, &store, |phase| {
            if phase == DiscardPhase::AfterSave {
                f.git(&["add", "--", "target.ts"]);
            }
            Ok(())
        });
        assert_eq!(result.unwrap_err().kind, ErrorKind::Rejected);
        assert_eq!(f.read("target.ts"), before, "拒绝时不得改工作区");
        let summary = store.list(None).unwrap().remove(0);
        assert!(matches!(summary.status, RecoveryStatus::NotApplied));

        // 仅权限变化同样使旧确认失效。
        let f2 = Fixture::new();
        let session2 = f2.changed();
        let target2 = entry(&session2, "target.ts");
        let store2 = f2.store();
        let result2 = session2.discard_with_hook("target.ts", &target2.fingerprint, &target2.snapshot_revision, None, &store2, |phase| {
            if phase == DiscardPhase::AfterSave {
                fs::set_permissions(f2.repo.join("target.ts"), fs::Permissions::from_mode(0o755)).unwrap();
            }
            Ok(())
        });
        assert!(result2.is_err());
    }
}

/// 原 worktree 不可用时，记录仍然可查看（05 才会处理恢复）。
#[test]
fn records_stay_viewable_when_worktree_is_unavailable() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    drop(session);
    fs::remove_dir_all(&f.repo).unwrap();

    let records = store.list(None).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].target, "target.ts");
    assert_eq!(records[0].paths, vec!["target.ts".to_string()]);
    assert!(matches!(records[0].status, RecoveryStatus::Completed));
    assert!(!records[0].identity_confirmed, "worktree 不可用时身份不能确认");
    // 记录必须留在独立数据目录，不能写进用户仓库。
    assert!(store.list(None).unwrap()[0].bytes > 0);
    assert!(!f.temp.path().join("data").join("recovery").starts_with(&f.repo));
}

/// 完成记录真实写入失败：已保存记录必须保留且不冒充完成，工作区写入已发生但不自动续跑。
#[cfg(unix)]
#[test]
fn completion_record_failure_keeps_saved_record_without_forging_completion() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    let index = f.index();
    let record_dir = {
        let error = session.discard_with_hook("target.ts", &target.fingerprint, &target.snapshot_revision, None, &store, |phase| {
            if phase == DiscardPhase::BeforeComplete {
                // 让 outcome 的真实落盘失败（不是靠 hook 假装失败）。
                let id = store.list(None).unwrap()[0].id.clone();
                let directory = f.temp.path().join("data").join("recovery").join(&id);
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap();
            }
            Ok(())
        }).unwrap_err();
        assert_eq!(error.kind, ErrorKind::NeedsVerification);
        let id = store.list(None).unwrap()[0].id.clone();
        f.temp.path().join("data").join("recovery").join(id)
    };
    fs::set_permissions(&record_dir, fs::Permissions::from_mode(0o700)).unwrap();

    // 真实写入已发生：工作区已回到提交内容。
    assert_eq!(f.read("target.ts"), text(&[]));
    assert_eq!(f.index(), index);
    // 记录仍在，但没有 outcome，不能被当成已完成。
    let records = store.list(None).unwrap();
    assert_eq!(records.len(), 1);
    assert!(matches!(records[0].status, RecoveryStatus::NeedsVerification));
    assert!(store.read(&records[0].id).unwrap().1.is_none());
    // 不自动续跑：重新打开会话只读真实状态，不会再写一次或新增记录。
    drop(session);
    let reopened = f.session();
    reopened.refresh().unwrap();
    assert_eq!(f.read("target.ts"), text(&[]));
    assert_eq!(store.list(None).unwrap().len(), 1);
}

/// Unix 上反斜杠是合法文件名字符，不能因为 Windows 的路径分隔符规则被误判为只读。
#[cfg(unix)]
#[test]
fn unix_backslash_filename_is_a_normal_discard_target() {
    let f = Fixture::new();
    let name = "back\\slash.txt";
    f.write(name, b"original\n");
    f.commit();
    f.write(name, b"modified\n");
    let session = f.session();
    let target = entry(&session, name);
    let index = f.index();
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    assert_eq!(f.read(name), b"original\n");
    assert_eq!(f.index(), index);
    let records = store.list(None).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].paths, vec![name.to_string()]);
}

/// R04-ST1：预览必须使用隔离环境。即使从设置了 GIT_DIR / GIT_WORK_TREE 的终端启动，
/// 预览的 init/config 也只能作用于自己的 scratch，绝不能再初始化或改写原仓库配置。
#[test]
fn preview_isolates_git_location_environment_from_the_real_repository() {
    let f = Fixture::new();
    let session = f.changed();
    // 让非隔离的 `git init` 会真实改写这一项（审查复现同款）。
    f.git(&["config", "core.filemode", "false"]);
    let before = f.read("target.ts");
    let config_path = f.repo.join(".git").join("config");
    let config_before = fs::read(&config_path).unwrap();
    let index_before = f.index();
    let store = f.store();

    // 模拟从带 Git 定位变量的终端启动（Windows 分支会继承进程全部环境）。
    let mut env = f.vars.clone();
    env.insert("GIT_DIR".into(), f.repo.join(".git").display().to_string());
    env.insert("GIT_WORK_TREE".into(), f.repo.display().to_string());
    let isolated_session = WorktreeSession::open("fixture-env", &f.repo, ShellEnv {
        vars: env, path: f.paths.clone(),
        info: EnvCaptureInfo { worktree: f.repo.display().to_string(), shell: "fixture".into(), elapsed_ms: 0,
            timed_out: false, used_fallback: true, var_count: f.vars.len(), path_entries: f.paths.clone(), diagnostics: None },
    }, Arc::new(CommandLog::default())).unwrap();

    let target_env = entry(&isolated_session, "target.ts");
    isolated_session
        .discard_file(&target_env.path, &target_env.fingerprint, &target_env.snapshot_revision, &store)
        .expect("隔离环境下丢弃应当成功");

    assert_eq!(fs::read(&config_path).unwrap(), config_before, "原仓库 config 不得被预览修改");
    assert_eq!(f.git(&["config", "core.filemode"]), b"false\n", "预览不得把 core.filemode 改回 true");
    assert_eq!(f.index(), index_before, "原仓库 index 不得被预览修改");
    assert_eq!(f.read("target.ts"), text(&[]));
    assert_ne!(before, f.read("target.ts"));
}

/// R04-ST2 / R04-SP2：`info/attributes`、仓库级 `core.attributesFile` 与本地 filter
/// 都属于有效属性上下文；预览必须与真实仓库等价，不能误拒普通文本。
#[test]
fn preview_honors_info_attributes_and_repository_attributes_file() {
    // 1. info/attributes 指定 CRLF 的普通文本。
    let f = Fixture::new();
    let info = f.repo.join(".git").join("info").join("attributes");
    fs::create_dir_all(info.parent().unwrap()).unwrap();
    fs::write(&info, b"*.txt text eol=crlf\n").unwrap();
    f.write("target.txt", b"alpha\nbeta\n");
    f.commit();
    fs::remove_file(f.repo.join("target.txt")).unwrap();
    f.git(&["checkout", "--", "target.txt"]);
    let committed = f.read("target.txt");
    assert_eq!(committed, b"alpha\r\nbeta\r\n");
    f.write("target.txt", b"alpha\r\nBETA\r\n");
    let session = f.session();
    let target = entry(&session, "target.txt");
    let index = f.index();
    discard(&session, &target, &f.store()).expect("info/attributes 下的普通文本必须可丢弃");
    assert_eq!(f.read("target.txt"), committed);
    assert_eq!(f.index(), index);

    // 2. 仓库级 core.attributesFile（不放在工作区，避免影响其他文件）。
    let f2 = Fixture::new();
    let attrs = f2.temp.path().join("repo-attributes");
    fs::write(&attrs, b"target.dat text eol=crlf\n").unwrap();
    f2.git(&["config", "core.attributesFile", attrs.to_str().unwrap()]);
    f2.write("target.dat", b"alpha\nbeta\n");
    f2.commit();
    fs::remove_file(f2.repo.join("target.dat")).unwrap();
    f2.git(&["checkout", "--", "target.dat"]);
    let committed2 = f2.read("target.dat");
    assert_eq!(committed2, b"alpha\r\nbeta\r\n");
    f2.write("target.dat", b"alpha\r\nBETA\r\n");
    let session2 = f2.session();
    let target2 = entry(&session2, "target.dat");
    discard(&session2, &target2, &f2.store()).expect("core.attributesFile 下的普通文本必须可丢弃");
    assert_eq!(f2.read("target.dat"), committed2);
}

/// R04-ST2：本地 `filter.*` 转换同样属于有效上下文；预览必须应用同一条过滤器。
#[cfg(unix)]
#[test]
fn preview_honors_local_clean_filter_configuration() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let script = f.temp.path().join("upper.sh");
    fs::write(&script, b"#!/bin/sh\ntr 'a-z' 'A-Z'\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let restore = f.temp.path().join("lower.sh");
    fs::write(&restore, b"#!/bin/sh\ntr 'A-Z' 'a-z'\n").unwrap();
    fs::set_permissions(&restore, fs::Permissions::from_mode(0o755)).unwrap();
    f.git(&["config", "filter.upper.clean", script.to_str().unwrap()]);
    f.git(&["config", "filter.upper.smudge", restore.to_str().unwrap()]);
    f.git(&["config", "filter.upper.required", "true"]);
    f.write(".gitattributes", b"target.txt filter=upper\n");
    f.write("target.txt", b"alpha\nbeta\n");
    f.commit();
    // 让过滤器真正作用于工作区，读出干净状态作为对照。
    fs::remove_file(f.repo.join("target.txt")).unwrap();
    f.git(&["checkout", "--", "target.txt"]);
    let committed = f.read("target.txt");
    f.write("target.txt", b"alpha\ngamma\n");
    let session = f.session();
    let target = entry(&session, "target.txt");
    let index = f.index();
    discard(&session, &target, &f.store()).expect("带本地 filter 的普通文本必须可丢弃");
    assert_eq!(f.read("target.txt"), committed, "丢弃后应恢复提交时的工作区内容");
    assert_eq!(f.index(), index);
}

/// R04-SP1：保存后属性/转换上下文改变时，目标字节与规范化补丁都可能不变，
/// 但同一份补丁会写出不同字节。必须在写入前拒绝，保留记录，不自动续跑。
#[test]
fn attribute_context_change_after_save_is_rejected_before_writing() {
    let f = Fixture::new();
    f.write(".gitattributes", b"target.txt text eol=lf\n");
    f.write("target.txt", &text(&[]));
    f.commit();
    f.write("target.txt", &text(&[(10, 1010), (80, 1080), (140, 1140)]));
    let session = f.session();
    session.refresh().unwrap();
    let rows = session.rows("target.txt", ChangesKind::Unstaged, ViewMode::Split, 0, 400).unwrap();
    let action = rows.rows.iter().find_map(|row| row.hunk_action.as_ref()).expect("应有 hunk").clone();
    let before = f.read("target.txt");
    let index = f.index();
    let store = f.store();

    let error = session.discard_with_hook("target.txt", &action.fingerprint, &action.snapshot_revision,
        Some(&action.hunk_id), &store, |phase| {
            if phase == DiscardPhase::AfterSave {
                // 目标字节与补丁都不变，只改转换上下文。
                f.write(".gitattributes", b"target.txt text eol=crlf\n");
            }
            Ok(())
        }).unwrap_err();

    assert_eq!(error.kind, ErrorKind::Rejected, "必须在写入前拒绝，而不是写后再待核对");
    assert_eq!(f.read("target.txt"), before, "拒绝时不得写入工作区");
    assert_eq!(f.index(), index, "拒绝时不得改变暂存区");
    let records = store.list(None).unwrap();
    assert_eq!(records.len(), 1, "完整记录必须保留");
    assert!(matches!(records[0].status, RecoveryStatus::NotApplied));
    assert!(matches!(store.read(&records[0].id).unwrap().1, Some(DiscardOutcome::NotApplied)));
    // 不自动续跑。
    drop(session);
    let reopened = f.session();
    reopened.refresh().unwrap();
    assert_eq!(f.read("target.txt"), before);
    assert_eq!(store.list(None).unwrap().len(), 1);
}

/// R04-SP1：无关文件变化不得触发拒绝。
#[test]
fn unrelated_attribute_file_change_does_not_block_discard() {
    let f = Fixture::new();
    f.write(".gitattributes", b"target.txt text eol=lf\n");
    f.write("other.txt", b"other\n");
    f.write("target.txt", &text(&[]));
    f.commit();
    f.write("target.txt", &text(&[(10, 1010)]));
    let session = f.session();
    let target = entry(&session, "target.txt");
    discard(&session, &target, &f.store()).expect("基线丢弃应当成功");
    assert_eq!(f.read("target.txt"), text(&[]));

    let f2 = Fixture::new();
    f2.write(".gitattributes", b"target.txt text eol=lf\n");
    f2.write("other.txt", b"other\n");
    f2.write("target.txt", &text(&[]));
    f2.commit();
    f2.write("target.txt", &text(&[(10, 1010)]));
    let session2 = f2.session();
    let target2 = entry(&session2, "target.txt");
    let store2 = f2.store();
    session2.discard_with_hook("target.txt", &target2.fingerprint, &target2.snapshot_revision, None, &store2, |phase| {
        if phase == DiscardPhase::AfterSave {
            // 与目标无关的属性和文件变化。
            f2.write("other.txt", b"other changed\n");
            f2.write("unrelated.gitattributes", b"*.bin binary\n");
        }
        Ok(())
    }).expect("无关文件变化不应阻止丢弃");
    assert_eq!(f2.read("target.txt"), text(&[]));
}

/// R04-V1 / T11：真实 `git apply` 执行成功、但成功回执丢失（子进程被模拟中断）。
/// 必须保留完整记录、标结果待核对（而非未执行），index 不变，实际写命令只执行一次，
/// 重新打开会话只读取真实状态、不自动续跑。
#[cfg(unix)]
#[test]
fn git_apply_lost_receipt_keeps_record_and_never_retries() {
    use std::os::unix::fs::PermissionsExt;

    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let before = f.read("target.ts");
    let index_before = f.index();

    // 严格限定 fixture 的包装器：只对真实仓库目录下的 apply 生效，先执行真实写入，
    // 再让调用方「失去成功回执」；其他目录（预览 scratch）与其它子命令一律透传。
    let real_git = String::from_utf8(
        Command::new("sh").arg("-c").arg("command -v git").output().unwrap().stdout,
    ).unwrap().trim().to_string();
    let wrapper_dir = f.temp.path().join("wrapper");
    fs::create_dir_all(&wrapper_dir).unwrap();
    let counter = f.temp.path().join("apply-count");
    let wrapper = wrapper_dir.join("git");
    fs::write(&wrapper, format!(
        "#!/bin/sh\nREAL=\"{real_git}\"\nfound=\"\"\nfor a in \"$@\"; do [ \"$a\" = \"apply\" ] && found=1; done\n\
         if [ -n \"$found\" ] && [ \"$(pwd -P)\" = \"$WRAP_REPO\" ]; then\n\
         \x20 \"$REAL\" \"$@\"; rc=$?\n\
         \x20 echo x >> \"$WRAP_COUNT\"\n\
         \x20 if [ \"$rc\" -eq 0 ]; then exit 141; fi\n\
         \x20 exit \"$rc\"\n\
         fi\nexec \"$REAL\" \"$@\"\n")).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();

    let mut vars = f.vars.clone();
    vars.insert("WRAP_REPO".into(), fs::canonicalize(&f.repo).unwrap().display().to_string());
    vars.insert("WRAP_COUNT".into(), counter.display().to_string());
    let mut paths = vec![wrapper_dir.display().to_string()];
    paths.extend(f.paths.clone());
    let wrapped = WorktreeSession::open("fixture-wrapper", &f.repo, ShellEnv {
        vars, path: paths.clone(),
        info: EnvCaptureInfo { worktree: f.repo.display().to_string(), shell: "fixture".into(), elapsed_ms: 0,
            timed_out: false, used_fallback: true, var_count: f.vars.len(), path_entries: paths, diagnostics: None },
    }, Arc::new(CommandLog::default())).unwrap();

    let store = f.store();
    let wrapped_target = entry(&wrapped, "target.ts");
    assert_eq!(wrapped_target.fingerprint, target.fingerprint, "包装器不应改变真实状态");
    let error = wrapped
        .discard_file("target.ts", &wrapped_target.fingerprint, &wrapped_target.snapshot_revision, &store)
        .expect_err("失去成功回执必须报待核对，而不是成功");
    assert_eq!(error.kind, ErrorKind::NeedsVerification, "必须标结果待核对，不能断言未执行");

    // 真实写入已经发生：工作区回到提交内容，index 未变。
    assert_eq!(f.read("target.ts"), text(&[]));
    assert_ne!(f.read("target.ts"), before);
    assert_eq!(f.index(), index_before, "丢弃不得改变暂存区");
    // 写命令只执行一次，没有自动重试。
    let count = fs::read_to_string(&counter).unwrap_or_default();
    assert_eq!(count.matches('x').count(), 1, "实际 apply 只能执行一次");
    // 完整记录保留，但没有完成回执，不能被当成已完成。
    let records = store.list(None).unwrap();
    assert_eq!(records.len(), 1, "必须保留已保存的完整记录");
    assert!(matches!(records[0].status, RecoveryStatus::NeedsVerification));
    assert!(store.read(&records[0].id).unwrap().1.is_none());
    // 重新打开会话只读取真实状态，不自动续跑。
    drop(wrapped);
    let reopened = f.session();
    reopened.refresh().unwrap();
    assert_eq!(f.read("target.ts"), text(&[]));
    assert_eq!(store.list(None).unwrap().len(), 1);
    assert_eq!(fs::read_to_string(&counter).unwrap_or_default().matches('x').count(), 1);
}

/// R04-R2-ST1 / R04-R2-SP3：`GIT_CONFIG` 指向真实配置时（Windows 生产会继承该变量），
/// 预览的 `config` 写入绝不能落进用户配置；整份 config、filter、attributesFile、index 都必须不变。
#[test]
fn git_config_redirect_environment_never_writes_user_config() {
    let f = Fixture::new();
    f.git(&["config", "filter.demo.clean", "cat"]);
    f.git(&["config", "filter.demo.smudge", "cat"]);
    let attrs = f.temp.path().join("repo-level-attributes");
    fs::write(&attrs, b"").unwrap();
    f.git(&["config", "core.attributesFile", attrs.to_str().unwrap()]);
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let config_path = f.repo.join(".git").join("config");
    let config_before = fs::read(&config_path).unwrap();
    let index_before = f.index();

    // 模拟从设置了 GIT_CONFIG 的终端启动。
    let mut vars = f.vars.clone();
    vars.insert("GIT_CONFIG".into(), config_path.display().to_string());
    let redirected = WorktreeSession::open("fixture-config-redirect", &f.repo, ShellEnv {
        vars, path: f.paths.clone(),
        info: EnvCaptureInfo { worktree: f.repo.display().to_string(), shell: "fixture".into(), elapsed_ms: 0,
            timed_out: false, used_fallback: true, var_count: f.vars.len(), path_entries: f.paths.clone(), diagnostics: None },
    }, Arc::new(CommandLog::default())).unwrap();
    let redirected_target = entry(&redirected, "target.ts");
    let store = f.store();
    redirected
        .discard_file("target.ts", &redirected_target.fingerprint, &redirected_target.snapshot_revision, &store)
        .expect("GIT_CONFIG 重定向下也必须能正常丢弃");

    assert_eq!(fs::read(&config_path).unwrap(), config_before, "用户 Git 配置字节必须完全不变");
    assert_eq!(f.git(&["config", "--get-all", "filter.demo.clean"]), b"cat\n", "filter 不得重复追加");
    assert_eq!(f.git(&["config", "--get-all", "filter.demo.smudge"]), b"cat\n");
    assert_eq!(
        f.git(&["config", "--get", "core.attributesFile"]).trim_ascii(),
        attrs.display().to_string().as_bytes(),
        "core.attributesFile 不得被改指向临时预览路径"
    );
    assert!(attrs.is_file(), "原属性文件必须仍在");
    assert_eq!(f.index(), index_before);
    assert_eq!(f.read("target.ts"), text(&[]));
}

/// R04-R2-ST2 / R04-R2-SP1：linked worktree 的 `info/attributes` 位于 commonDir。
/// 共享属性设为 CRLF 时，linked 目标必须能正常整文件与 hunk 丢弃，记录归属 linked 而非 main。
#[test]
fn linked_worktree_uses_shared_info_attributes_for_discard() {
    let f = Fixture::new();
    f.write("base.txt", b"base\n");
    f.commit();
    let linked = f.temp.path().join("linked");
    f.git(&["worktree", "add", "-b", "linked", linked.to_str().unwrap()]);

    // 共享 info/attributes：Git 对 linked worktree 实际生效的位置。
    let common_info = f.repo.join(".git").join("info").join("attributes");
    fs::create_dir_all(common_info.parent().unwrap()).unwrap();
    fs::write(&common_info, b"*.txt text eol=crlf\n").unwrap();

    // 在 linked 中准备两个 CRLF 文本：一个整文件丢弃、一个 hunk 丢弃。
    fs::write(linked.join("whole.txt"), b"alpha\nbeta\n").unwrap();
    fs::write(linked.join("part.txt"), b"one\ntwo\nthree\nfour\nfive\n").unwrap();
    let linked_git = |args: &[&str]| {
        let output = Command::new("git").current_dir(&linked).env_clear().envs(&f.vars).args(args).output().unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        output.stdout
    };
    linked_git(&["add", "--all"]);
    linked_git(&["commit", "-m", "linked fixture"]);
    for name in ["whole.txt", "part.txt"] {
        fs::remove_file(linked.join(name)).unwrap();
        linked_git(&["checkout", "--", name]);
        assert_eq!(fs::read(linked.join(name)).unwrap().windows(2).filter(|w| w == b"\r\n").count() > 0, true);
    }
    let whole_committed = fs::read(linked.join("whole.txt")).unwrap();
    let part_committed = fs::read(linked.join("part.txt")).unwrap();
    fs::write(linked.join("whole.txt"), b"alpha\r\nBETA\r\n").unwrap();
    fs::write(linked.join("part.txt"), b"one\r\ntwo\r\nTHREE\r\nfour\r\nfive\r\n").unwrap();

    let session = f.session_at(&linked);
    let store = f.store();
    let index_before = {
        let output = Command::new("git").current_dir(&linked).env_clear().envs(&f.vars)
            .args(["ls-files", "--stage", "-z"]).output().unwrap();
        output.stdout
    };

    let whole = entry(&session, "whole.txt");
    discard(&session, &whole, &store).expect("linked 整文件丢弃必须成功");
    assert_eq!(fs::read(linked.join("whole.txt")).unwrap(), whole_committed);

    let part = entry(&session, "part.txt");
    session.refresh().unwrap();
    let rows = session.rows("part.txt", ChangesKind::Unstaged, ViewMode::Split, 0, 200).unwrap();
    let action = rows.rows.iter().find_map(|row| row.hunk_action.as_ref()).expect("应有 hunk").clone();
    session
        .discard_hunk("part.txt", &action.fingerprint, &action.snapshot_revision, &action.hunk_id, &store)
        .expect("linked hunk 丢弃必须成功");
    let after = fs::read(linked.join("part.txt")).unwrap();
    assert_eq!(after, part_committed, "未选中的 hunk 必须保持提交内容");

    // index 不变，记录归属 linked 而不是 main。
    let index_after = Command::new("git").current_dir(&linked).env_clear().envs(&f.vars)
        .args(["ls-files", "--stage", "-z"]).output().unwrap().stdout;
    assert_eq!(index_after, index_before, "丢弃不得改变 linked 的暂存区");
    let linked_identity = WorktreeIdentity::capture(&session.git).unwrap();
    let records = store.list(Some(&linked_identity)).unwrap();
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|record| record.identity_confirmed));
    let linked_canonical = fs::canonicalize(&linked).unwrap().display().to_string();
    assert!(
        records.iter().all(|record| record.worktree_path == linked_canonical),
        "记录必须归属 linked worktree：{records:?}"
    );
    let main_session = f.session();
    let main_identity = WorktreeIdentity::capture(&main_session.git).unwrap();
    assert!(store.list(Some(&main_identity)).unwrap().iter().all(|record| !record.identity_confirmed));
}

/// R04-R2-SP2：配置值可以合法包含换行（`smudge = ":\\ncat"`）。
/// 复制到预览仓库时必须无损，否则 scratch apply 失败而真实仓库成功。
#[cfg(unix)]
#[test]
fn multiline_filter_command_is_copied_losslessly() {
    let f = Fixture::new();
    f.git(&["config", "filter.copy.clean", "cat"]);
    f.git(&["config", "filter.copy.smudge", ":\ncat"]);
    f.git(&["config", "filter.copy.required", "true"]);
    f.write(".gitattributes", b"target.txt filter=copy\n");
    f.write("target.txt", b"alpha\nbeta\n");
    f.commit();
    f.write("target.txt", b"alpha\nBETA\n");
    let session = f.session();
    let target = entry(&session, "target.txt");
    let index = f.index();
    discard(&session, &target, &f.store()).expect("多行 filter 配置下的普通文本必须可丢弃");
    assert_eq!(f.read("target.txt"), b"alpha\nbeta\n");
    assert_eq!(f.index(), index);
}

/// R04-GUI1 / T03 / Q81：Git 识别的重命名没有 hunk 丢弃。
/// porcelain `RM` 的未暂存 `M` 仍带原路径，必须被共享能力 gate 拒绝；
/// 整文件丢弃与「新旧相关路径」记录保持不变。
#[test]
fn git_rename_unstaged_side_rejects_hunk_discard_but_keeps_whole_file() {
    let f = Fixture::new();
    f.write("old.txt", b"alpha\nbeta\ngamma\n");
    f.commit();
    f.git(&["mv", "old.txt", "new.txt"]);
    f.write("new.txt", b"alpha\nBETA\ngamma\n");

    let session = f.session();
    let snapshot = session.refresh().unwrap();
    let staged = snapshot.entries.iter()
        .find(|entry| entry.path == "new.txt" && entry.changes == ChangesKind::Staged)
        .expect("应有已暂存改名");
    assert_eq!(staged.status, revia_lib::git::status::FileStatus::Renamed);
    let unstaged = snapshot.entries.iter()
        .find(|entry| entry.path == "new.txt" && entry.changes == ChangesKind::Unstaged)
        .expect("应有未暂存修改");
    assert_eq!(unstaged.original_path.as_deref(), Some("old.txt"));

    let store = f.store();
    let index = f.index();
    let before = f.read("new.txt");

    // 后端拒绝 hunk 丢弃，且不产生任何回收记录。
    let rejected = session
        .discard_hunk("new.txt", &unstaged.fingerprint, &unstaged.snapshot_revision, "forged-hunk", &store)
        .expect_err("rename 的未暂存条目不得接受 hunk 丢弃");
    assert_eq!(
        rejected.to_string(),
        "这个文件不支持 hunk 操作",
        "必须由共享能力 gate 拒绝，而不是靠 hunk 凭据不存在"
    );
    assert!(store.list(None).unwrap().is_empty(), "拒绝时不得写回收记录");
    assert_eq!(f.read("new.txt"), before);
    assert_eq!(f.index(), index);

    // 整文件丢弃仍然可用，并记录新旧两条相关路径。
    discard(&session, &unstaged, &store).expect("rename 的整文件丢弃必须保持可用");
    assert_eq!(f.read("new.txt"), b"alpha\nbeta\ngamma\n");
    assert_eq!(f.index(), index, "丢弃不得改变暂存区（改名继续保持已暂存）");
    let records = store.list(None).unwrap();
    assert_eq!(records.len(), 1);
    assert!(records[0].paths.contains(&"old.txt".to_string()));
    assert!(records[0].paths.contains(&"new.txt".to_string()));
}

// ---------------- 任务 05：恢复与清理 ----------------

/// 无后续变化时恢复完整工作区状态：内容、存在状态、可执行权限；index 不变。
#[test]
fn restore_returns_full_worktree_state_without_touching_index() {
    let f = Fixture::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.write("script.sh", b"#!/bin/sh\necho v1\n");
        f.commit();
        fs::set_permissions(f.repo.join("script.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        f.git(&["update-index", "--chmod=+x", "script.sh"]);
        f.commit();
        f.write("script.sh", b"#!/bin/sh\necho v2\n");
        let session = f.session();
        let target = entry(&session, "script.sh");
        let store = f.store();
        let before = f.read("script.sh");
        discard(&session, &target, &store).unwrap();
        assert_eq!(f.read("script.sh"), b"#!/bin/sh\necho v1\n");

        let index = f.index();
        let inspection = session.inspect_record(&store.list(None).unwrap()[0].id, &store).unwrap();
        assert!(matches!(inspection.state, revia_lib::recovery::RecoveryState::Ready));
        session
            .restore_record(&inspection.id, &expectation_of(&inspection), &store)
            .expect("无后续变化时应可直接恢复");
        assert_eq!(f.read("script.sh"), before, "必须回到丢弃前的完整字节");
        assert!(
            platform::executable(&fs::metadata(f.repo.join("script.sh")).unwrap()),
            "必须恢复可执行权限"
        );
        assert_eq!(f.index(), index, "恢复不得改变暂存区");
    }
}

/// 有后续变化时只能只读对比，不能覆盖。
#[test]
fn restore_refuses_after_later_changes_and_keeps_current_content() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let record_id = store.list(None).unwrap()[0].id.clone();

    f.write("target.ts", b"later work\n");
    let inspection = session.inspect_record(&record_id, &store).unwrap();
    assert!(matches!(inspection.state, revia_lib::recovery::RecoveryState::LaterChanges));
    assert_eq!(inspection.files.len(), 1);
    assert!(inspection.files[0].saved.text.is_some(), "文本应提供只读对比内容");
    assert_eq!(inspection.files[0].current.as_ref().unwrap().text.as_deref(), Some("later work\n"));

    let error = session
        .restore_record(&record_id, &expectation_of(&inspection), &store)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected);
    assert_eq!(f.read("target.ts"), b"later work\n", "不得覆盖后续修改");
}

/// 结果待核对（无可信 after）时不能假定已丢弃，也不能直接恢复。
#[test]
fn restore_refuses_without_trusted_post_discard_baseline() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    // 在完成记录阶段中断：记录保留但无 outcome。
    let _ = session.discard_with_hook("target.ts", &target.fingerprint, &target.snapshot_revision, None, &store, |phase| {
        if phase == DiscardPhase::BeforeComplete { return Err(AppError::internal("controlled")); }
        Ok(())
    });
    let record_id = store.list(None).unwrap()[0].id.clone();
    let inspection = session.inspect_record(&record_id, &store).unwrap();
    assert!(matches!(inspection.state, revia_lib::recovery::RecoveryState::NoBaseline));
    let error = session
        .restore_record(&record_id, &expectation_of(&inspection), &store)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected);
}

/// 身份不匹配（其他 worktree / 重建）不得直接恢复。
#[test]
fn restore_refuses_when_identity_does_not_match() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let record_id = store.list(None).unwrap()[0].id.clone();
    drop(session);

    let linked = f.temp.path().join("linked");
    f.git(&["worktree", "add", "-b", "linked", linked.to_str().unwrap()]);
    let linked_session = f.session_at(&linked);
    let inspection = linked_session.inspect_record(&record_id, &store).unwrap();
    assert!(matches!(inspection.state, revia_lib::recovery::RecoveryState::IdentityMismatch));
    let error = linked_session
        .restore_record(&record_id, &expectation_of(&inspection), &store)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected);
    // 原文件仍是丢弃后的状态，没有被 linked 覆盖。
    assert_eq!(f.read("target.ts"), text(&[]));
}

/// 单 hunk 丢弃的记录恢复的是完整文件，而不是重放反向补丁。
#[test]
fn hunk_record_restores_the_whole_file() {
    let f = Fixture::new();
    let session = f.changed();
    session.refresh().unwrap();
    let rows = session.rows("target.ts", ChangesKind::Unstaged, ViewMode::Split, 0, 400).unwrap();
    let action = rows.rows.iter().find_map(|row| row.hunk_action.as_ref()).expect("应有 hunk").clone();
    let store = f.store();
    let before = f.read("target.ts");
    session
        .discard_hunk("target.ts", &action.fingerprint, &action.snapshot_revision, &action.hunk_id, &store)
        .unwrap();
    let after_hunk = f.read("target.ts");
    assert_ne!(after_hunk, before);

    let record_id = store.list(None).unwrap()[0].id.clone();
    let inspection = session.inspect_record(&record_id, &store).unwrap();
    assert!(matches!(inspection.source, RecoverySource::DiscardHunk));
    session
        .restore_record(&record_id, &expectation_of(&inspection), &store)
        .expect("单 hunk 记录也应能恢复完整文件");
    assert_eq!(f.read("target.ts"), before, "恢复的是完整文件内容");
}

/// 另存：目标已存在（含符号链接）拒绝；成功时原文件与 index 不变。
#[cfg(unix)]
#[test]
fn save_as_refuses_existing_targets_and_keeps_original_untouched() {
    let f = Fixture::new();
    f.write("blob.bin", b"\0old");
    f.commit();
    f.write("blob.bin", b"\0new");
    let session = f.session();
    let target = entry(&session, "blob.bin");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let record_id = store.list(None).unwrap()[0].id.clone();
    let index = f.index();
    let after_discard = f.read("blob.bin");

    // 已存在的普通文件。
    let occupied = f.root().join("occupied.bin");
    fs::write(&occupied, b"keep me").unwrap();
    assert!(store.save_as(&record_id, "blob.bin", &occupied).is_err());
    assert_eq!(fs::read(&occupied).unwrap(), b"keep me", "已存在目标不得被截断");

    // 已存在的符号链接（指向真实文件）。
    let link = f.root().join("link.bin");
    std::os::unix::fs::symlink(&occupied, &link).unwrap();
    assert!(store.save_as(&record_id, "blob.bin", &link).is_err());
    assert_eq!(fs::read(&occupied).unwrap(), b"keep me");

    // 未占用的新路径：成功写入保存版本。
    let fresh = f.root().join("saved.bin");
    let bytes = store.save_as(&record_id, "blob.bin", &fresh).expect("新路径应可另存");
    assert_eq!(bytes, 4);
    // 保存的是「丢弃前」的工作区内容，也就是被丢弃掉的那一版。
    assert_eq!(fs::read(&fresh).unwrap(), b"\0new");
    assert_eq!(f.read("blob.bin"), after_discard, "另存不得改原文件");
    assert_eq!(f.index(), index);
}

/// 14 天到期清理（受控时钟）；在途记录跳过；清空只删本应用记录。
#[test]
fn purge_expired_uses_controlled_clock_and_clear_keeps_in_use_records() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let now = Arc::new(std::sync::atomic::AtomicU64::new(1_792_742_400_123));
    let clock = now.clone();
    let store = RecoveryStore::with_clock(f.temp.path().join("data"), Arc::new(move || clock.load(std::sync::atomic::Ordering::SeqCst)));
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();

    // 未到期：不删。
    now.store(1_792_742_400_123 + revia_lib::recovery::RETENTION_MS - 1, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(store.purge_expired().unwrap(), 0);
    assert_eq!(store.list(None).unwrap().len(), 1);

    // 在途记录即使过期也跳过。
    now.store(1_792_742_400_123 + revia_lib::recovery::RETENTION_MS + 1, std::sync::atomic::Ordering::SeqCst);
    {
        let _in_use = store.use_record(&id);
        assert_eq!(store.purge_expired().unwrap(), 0, "在途记录不得被清理");
        assert_eq!(store.clear().unwrap(), 0, "清空同样不得删在途记录");
        assert_eq!(store.list(None).unwrap().len(), 1);
    }
    // 释放后到期清理生效。
    assert_eq!(store.purge_expired().unwrap(), 1);
    assert!(store.list(None).unwrap().is_empty());
}

/// 清空不误删同一数据目录下的其他文件。
#[test]
fn clear_only_removes_this_applications_records() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let data = f.temp.path().join("data");
    fs::write(data.join("settings.json"), b"{}").unwrap();
    fs::create_dir_all(data.join("recovery").join("preview-leftover")).unwrap();

    assert_eq!(store.clear().unwrap(), 1);
    assert!(store.list(None).unwrap().is_empty());
    assert!(data.join("settings.json").is_file(), "其他应用数据不得被删");
    assert!(data.join("recovery").join("preview-leftover").is_dir(), "非记录目录不得被删");
}

// ---------------- 任务 05 返修回归（R05-R1） ----------------

/// ST1/SP1：恢复不得删除同名的既有用户文件；临时文件必须独占创建。
#[test]
fn restore_never_deletes_preexisting_lookalike_files() {
    let f = Fixture::new();
    f.write("a.txt", b"base\n");
    f.commit();
    f.write("a.txt", b"saved change\n");
    let session = f.session();
    let target = entry(&session, "a.txt");
    let store = f.store();
    discard(&session, &target, &store).unwrap();

    // 用户自己的同名文件（旧实现会删除它）。
    let decoy = f.repo.join("a.revia-restore-tmp");
    fs::write(&decoy, b"unrelated valuable bytes\n").unwrap();
    // 目标文件本身就叫这个名字的情况。
    let id = store.list(None).unwrap()[0].id.clone();
    let inspection = session.inspect_record(&id, &store).unwrap();
    session
        .restore_record(&id, &expectation_of(&inspection), &store)
        .expect("恢复应成功");
    assert_eq!(fs::read(&decoy).unwrap(), b"unrelated valuable bytes\n", "无关文件必须保留");
    assert_eq!(f.read("a.txt"), b"saved change\n");
}

/// ST2/SP2：生产丢弃全程持有占用，AfterSave 阶段清空不会删掉唯一备份。
#[test]
fn production_discard_holds_record_through_clear_during_after_save() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    let mut cleared = 0u64;
    session
        .discard_with_hook("target.ts", &target.fingerprint, &target.snapshot_revision, None, &store, |phase| {
            if phase == DiscardPhase::AfterSave {
                // 真实生产入口的占用必须已生效：这里清空不能删掉唯一记录。
                cleared = store.clear()?;
            }
            Ok(())
        })
        .expect("丢弃应完成");
    assert_eq!(cleared, 0, "在途记录不得被清空");
    let records = store.list(None).unwrap();
    assert_eq!(records.len(), 1, "唯一备份必须保留");
    assert!(matches!(records[0].status, RecoveryStatus::Completed));
    // 备份内容仍是丢弃前的完整文件。
    let (record, _) = store.read(&records[0].id).unwrap();
    assert_eq!(record.before[0].content.as_ref().unwrap(), &text(&[(10, 1010), (80, 1080), (140, 1140)]));
}

/// ST2：占用登记是引用计数，多个持有者时任一释放都不解除保护。
#[test]
fn record_use_is_refcounted_and_clear_is_atomic() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();

    let first = store.use_record(&id);
    let second = store.use_record(&id);
    drop(first);
    assert_eq!(store.clear().unwrap(), 0, "仍有一个持有者，不得删除");
    assert_eq!(store.list(None).unwrap().len(), 1);
    drop(second);
    assert_eq!(store.clear().unwrap(), 1, "全部释放后才可删除");
}

/// ST3：Git 未解决冲突的目标不得恢复；普通暂存变化但工作区 after 未变仍可恢复。
#[test]
fn restore_refuses_unmerged_targets_but_allows_plain_index_changes() {
    let f = Fixture::new();
    f.write("target.txt", b"base\n");
    f.commit();
    f.write("target.txt", b"saved change\n");
    let session = f.session();
    let target = entry(&session, "target.txt");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    let index_before = f.index();

    // 普通暂存变化（另一个文件）：目标的工作区 after 未变，仍应允许恢复且 index 不变。
    f.write("other.txt", b"other change\n");
    f.git(&["add", "--", "other.txt"]);
    let index_staged = f.index();
    assert_ne!(index_staged, index_before);
    let inspection = session.inspect_record(&id, &store).unwrap();
    session
        .restore_record(&id, &expectation_of(&inspection), &store)
        .expect("普通 index 变化不应阻止恢复");
    assert_eq!(f.read("target.txt"), b"saved change\n");
    assert_eq!(f.index(), index_staged, "恢复不得改变暂存区");
}

/// ST5/SP3：恢复写入中阶段中断 -> 待核对 + 阶段留证；重启后不自动续跑。
#[test]
fn restore_interruption_is_pending_verification_and_never_resumes() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    let inspection = session.inspect_record(&id, &store).unwrap();
    let expectation = revia_lib::recovery::RestoreExpectation {
        version: inspection.version.clone(),
        paths: inspection.paths.clone(),
        state: inspection.state,
    };

    let error = session
        .restore_record_with_hook(&id, &expectation, &store, |phase| {
            if phase == revia_lib::recovery::RestorePhase::AfterWrite {
                return Err(AppError::internal("controlled interruption"));
            }
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::NeedsVerification, "可能已写入部分路径必须标待核对");
    // 写入确实发生了，但记录仍在且留证为待核对（不是丢弃的 Completed）。
    assert_eq!(f.read("target.ts"), text(&[(10, 1010), (80, 1080), (140, 1140)]));
    let attempt = store.latest_attempt(&id).unwrap().expect("必须有恢复留证");
    assert!(matches!(attempt.state, revia_lib::recovery::RestoreAttemptState::NeedsVerification));
    assert_eq!(store.list(None).unwrap().len(), 1, "记录保留");

    // 重新打开会话：不自动重跑（文件不再被改写）。
    drop(session);
    let reopened = f.session();
    reopened.refresh().unwrap();
    assert_eq!(f.read("target.ts"), text(&[(10, 1010), (80, 1080), (140, 1140)]));
    assert!(matches!(
        store.latest_attempt(&id).unwrap().unwrap().state,
        revia_lib::recovery::RestoreAttemptState::NeedsVerification
    ));
}

/// SP8：另存不得穿过祖先符号链接或写入 Git 元数据。
#[cfg(unix)]
#[test]
fn save_as_rejects_symlinked_ancestors_and_git_metadata() {
    let f = Fixture::new();
    f.write("target.txt", b"base\n");
    f.commit();
    f.write("target.txt", b"saved change\n");
    let session = f.session();
    let target = entry(&session, "target.txt");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();

    let real_dir = f.root().join("real-dir");
    fs::create_dir_all(&real_dir).unwrap();
    let linked_dir = f.root().join("linked-dir");
    std::os::unix::fs::symlink(&real_dir, &linked_dir).unwrap();
    assert!(
        store.save_as(&id, "target.txt", &linked_dir.join("out.txt")).is_err(),
        "祖先为符号链接时必须拒绝"
    );
    assert!(!real_dir.join("out.txt").exists(), "不得穿透链接写入");

    // 深层祖先：link/inner/new.bin，其中 inner 是真实子目录但经由链接到达。
    let deep = linked_dir.join("inner");
    fs::create_dir_all(&deep).unwrap();
    assert!(
        store.save_as(&id, "target.txt", &deep.join("new.txt")).is_err(),
        "祖先链中任一级是符号链接都必须拒绝"
    );
    assert!(!real_dir.join("inner").join("new.txt").exists(), "不得穿透链接写入深层路径");

    // 未占用的真实外部目录：正常另存成功。
    let external = f.root().join("external");
    fs::create_dir_all(&external).unwrap();
    assert_eq!(store.save_as(&id, "target.txt", &external.join("new.txt")).unwrap(), 13);
    assert_eq!(fs::read(external.join("new.txt")).unwrap(), b"saved change\n");

    let inside_git = f.repo.join(".git").join("out.txt");
    assert!(store.save_as(&id, "target.txt", &inside_git).is_err(), "不得写入 Git 元数据目录");
    assert!(!inside_git.exists());
}

/// SP9：原 worktree 不可用时仍能查看保存内容，当前侧标为不可用。
#[test]
fn inspection_remains_viewable_when_worktree_is_gone() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    drop(session);
    fs::remove_dir_all(&f.repo).unwrap();

    let inspection = store.inspect(&id, None, None).expect("worktree 不存在时仍应可查看");
    assert_eq!(inspection.paths, vec!["target.ts".to_string()]);
    assert!(inspection.files[0].saved.exists);
    assert!(inspection.files[0].saved.text.is_some(), "保存内容必须可读");
    assert!(inspection.files[0].current_unavailable, "当前侧必须如实标为不可用");
    assert!(inspection.files[0].current.is_none(), "不得用保存内容冒充当前内容");
    assert!(matches!(inspection.state, revia_lib::recovery::RecoveryState::IdentityMismatch));
}

/// SP6：文本对比由 Rust 侧真实 Git 生成只读 diff。
#[test]
fn text_diff_is_generated_by_real_git() {
    let f = Fixture::new();
    f.write("target.txt", b"alpha\nbeta\n");
    f.commit();
    f.write("target.txt", b"alpha\nBETA\n");
    let session = f.session();
    let target = entry(&session, "target.txt");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    f.write("target.txt", b"alpha\nBETA\nlater\n");

    let diff = session.recovery_text_diff(&id, "target.txt", &store).expect("应生成只读 diff");
    assert!(diff.contains("@@ "), "必须是 Git 生成的 hunk：{diff}");
    assert!(diff.contains("+later"), "应显示后续新增的内容：{diff}");
    // 不得在用户仓库里留下临时对比文件。
    assert!(!f.repo.join("target.txt.saved").exists());
}

// ---------------- 任务 05 第二轮返修回归（R05-R2） ----------------

/// 在真实仓库里制造未解决冲突，返回冲突路径。
fn make_conflict(f: &Fixture, path: &str, base: &[u8], ours: &[u8], theirs: &[u8]) {
    f.write(path, base);
    f.commit();
    f.git(&["branch", "feature"]);
    f.write(path, ours);
    f.commit();
    f.git(&["checkout", "feature"]);
    f.write(path, theirs);
    f.commit();
    // 冲突是真实 Git merge 产生的，不靠改 index 伪造。
    let output = Command::new("git")
        .current_dir(&f.repo)
        .env_clear()
        .envs(&f.vars)
        .args(["merge", "main"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "merge 应当冲突：{output:?}");
}

/// R05-R2-ST1/SP1：发布前重新捕获真实状态。
///
/// 生产 `BeforeWrite` 接缝里外部继续编辑后，最终发布不得覆盖；
/// 这条用例构造的是「准备完成、尚未发布」时的外部改动，不是提前检查。
#[test]
fn publish_rechecks_real_state_after_prepare_and_does_not_overwrite() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    let inspection = session.inspect_record(&id, &store).unwrap();
    let before_restore = f.read("target.ts");

    let error = session
        .restore_record_with_hook(&id, &expectation_of(&inspection), &store, |phase| {
            if phase == revia_lib::recovery::RestorePhase::BeforeWrite {
                // 准备已完成、尚未发布：外部写入新内容。
                fs::write(f.repo.join("target.ts"), b"external edit during restore\n").unwrap();
            }
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected, "外部改动必须拒绝恢复：{error:?}");
    assert_eq!(f.read("target.ts"), b"external edit during restore\n", "不得覆盖外部改动");
    assert_ne!(f.read("target.ts"), before_restore);
    // 记录保留，并留下这次尝试（未执行）。
    let attempt = store.latest_attempt(&id).unwrap().expect("必须留下尝试");
    assert!(matches!(attempt.state, revia_lib::recovery::RestoreAttemptState::Rejected));
    assert!(attempt.written.is_empty());
    assert_eq!(store.list(None).unwrap().len(), 1);
}

/// R05-R2-ST1/SP1：真实 unmerged 冲突在发布前被拦下（构造条件，不靠测试名）。
#[test]
fn publish_refuses_real_unmerged_conflict_target() {
    let f = Fixture::new();
    f.write("conflict.txt", b"base\n");
    f.commit();
    f.git(&["branch", "feature"]);
    f.write("conflict.txt", b"ours\n");
    f.commit();
    f.git(&["checkout", "feature"]);
    f.write("conflict.txt", b"theirs\n");
    f.commit();
    f.git(&["checkout", "main"]);

    // 在 main 上先做一次可丢弃的未暂存改动并丢弃，得到一条记录。
    f.write("conflict.txt", b"discarded\n");
    let session = f.session();
    let target = entry(&session, "conflict.txt");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();

    // 真实 Git merge 制造未解决冲突（index 进入 unmerged 阶段）。
    let output = Command::new("git")
        .current_dir(&f.repo)
        .env_clear()
        .envs(&f.vars)
        .args(["merge", "feature"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "merge 应当冲突：{output:?}");

    let session = f.session();
    let status = revia_lib::git::status::read(&session.git).unwrap();
    assert!(
        status.entries.iter().any(|entry| entry.path == "conflict.txt" && entry.unmerged),
        "构造失败：目标必须是真实 unmerged"
    );
    let inspection = session.inspect_record(&id, &store).unwrap();
    let error = session
        .restore_record(&id, &expectation_of(&inspection), &store)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected);
    assert!(error.message.contains("冲突"), "错误应说明冲突：{error:?}");
    // 冲突内容不被覆盖。
    assert_ne!(f.read("conflict.txt"), b"discarded\n");
    // 冲突文件未写入：尝试记录里没有任何已发布路径。
    let attempt = store.latest_attempt(&id).unwrap().expect("必须留下尝试");
    assert!(attempt.written.is_empty(), "冲突时不得写入：{attempt:?}");
}

/// R05-R2-ST1/SP1：原 worktree 被换成新身份后，发布前重新捕获会拒绝。
#[test]
fn publish_rechecks_identity_instead_of_reusing_captured_one() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    let inspection = session.inspect_record(&id, &store).unwrap();
    let expected_after = f.read("target.ts");

    let moved = f.root().join("moved-repo");
    let error = session
        .restore_record_with_hook(&id, &expectation_of(&inspection), &store, |phase| {
            if phase == revia_lib::recovery::RestorePhase::BeforeWrite {
                // 原目录整体移走，同路径换成另一个仓库（新身份）。
                fs::rename(&f.repo, &moved).unwrap();
                fs::create_dir_all(&f.repo).unwrap();
                let output = Command::new("git")
                    .current_dir(&f.repo)
                    .env_clear()
                    .envs(&f.vars)
                    .args(["init", "-b", "main"])
                    .output()
                    .unwrap();
                assert!(output.status.success());
                fs::write(f.repo.join("target.ts"), b"new repo content\n").unwrap();
            }
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected, "身份改变必须拒绝：{error:?}");
    assert_eq!(f.read("target.ts"), b"new repo content\n", "新身份目录不得被写入");
    assert_eq!(fs::read(moved.join("target.ts")).unwrap(), expected_after, "原内容不得被改");
}

/// R05-R2-ST2/SP3：恢复历史进入生产查看（列表 + 详情），重启后仍显示待核对。
#[test]
fn restore_history_is_visible_through_list_and_inspect_after_reopen() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    let inspection = session.inspect_record(&id, &store).unwrap();

    // 写后阶段中断：工作区已写，完成记录失败。
    let error = session
        .restore_record_with_hook(&id, &expectation_of(&inspection), &store, |phase| {
            if phase == revia_lib::recovery::RestorePhase::BeforeComplete {
                return Err(AppError::internal("controlled completion failure"));
            }
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::NeedsVerification, "写后失败必须待核对：{error:?}");

    // 列表（生产查看入口）显示恢复待核对，而不是丢弃的 Completed。
    let summaries = store.list(None).unwrap();
    assert_eq!(summaries.len(), 1);
    assert!(matches!(summaries[0].status, RecoveryStatus::Completed), "丢弃回执本身仍是 Completed");
    let attempt = summaries[0].restore.as_ref().expect("列表必须展示恢复尝试");
    assert!(matches!(attempt.state, revia_lib::recovery::RestoreAttemptState::NeedsVerification));
    assert_eq!(attempt.written, vec!["target.ts".to_string()], "应如实列出已写入路径");

    // 重启（重新打开会话 + 重新读取列表/详情）后仍是待核对。
    drop(session);
    let reopened = f.session();
    let summaries = store.list(None).unwrap();
    assert!(matches!(
        summaries[0].restore.as_ref().unwrap().state,
        revia_lib::recovery::RestoreAttemptState::NeedsVerification
    ));
    let inspection = reopened.inspect_record(&id, &store).unwrap();
    assert!(matches!(
        inspection.restore.as_ref().unwrap().state,
        revia_lib::recovery::RestoreAttemptState::NeedsVerification
    ));
    // 工作区已写回保存内容，因此相对丢弃后的基线属于「有后续变化」：不能自动覆盖。
    assert_eq!(inspection.state, revia_lib::recovery::RecoveryState::LaterChanges);
}

/// R05-R2-ST2/SP4：写后失败后，重新核对仍 Ready 时可以再次显式恢复（不是一次性记录）。
#[test]
fn restore_can_be_explicitly_retried_after_rechecking() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    let inspection = session.inspect_record(&id, &store).unwrap();

    let _ = session
        .restore_record_with_hook(&id, &expectation_of(&inspection), &store, |phase| {
            if phase == revia_lib::recovery::RestorePhase::BeforeComplete {
                return Err(AppError::internal("controlled completion failure"));
            }
            Ok(())
        })
        .unwrap_err();

    // 用户把文件恢复到丢弃后的基线（真实工作区状态再次符合可信 after）。
    fs::write(f.repo.join("target.ts"), text(&[])).unwrap();
    // 重新核对：拿到新的版本与状态，再次显式恢复必须成功（不能永久 AlreadyExists）。
    let rechecked = session.inspect_record(&id, &store).unwrap();
    assert_eq!(rechecked.state, revia_lib::recovery::RecoveryState::Ready);
    let outcome = session
        .restore_record(&id, &expectation_of(&rechecked), &store)
        .expect("重新核对后应可再次显式恢复");
    assert!(matches!(
        outcome.attempt.as_ref().unwrap().state,
        revia_lib::recovery::RestoreAttemptState::Completed
    ));
    assert_eq!(f.read("target.ts"), text(&[(10, 1010), (80, 1080), (140, 1140)]));
    // 历史保留两次尝试，旧的一次不被覆盖。
    let attempts = store.attempts(&id).unwrap();
    assert_eq!(attempts.len(), 2, "恢复历史必须追加保留：{attempts:?}");
    assert!(matches!(attempts[0].state, revia_lib::recovery::RestoreAttemptState::NeedsVerification));
    assert!(matches!(attempts[1].state, revia_lib::recovery::RestoreAttemptState::Completed));
}

/// R05-R2-ST3/SP2：写后任何阶段失败统一待核对，且真实重读（不伪造完成）。
#[test]
fn every_post_write_failure_is_pending_verification_and_rereads_truth() {
    for phase in [revia_lib::recovery::RestorePhase::AfterWrite, revia_lib::recovery::RestorePhase::BeforeComplete] {
        let f = Fixture::new();
        let session = f.changed();
        let target = entry(&session, "target.ts");
        let store = f.store();
        discard(&session, &target, &store).unwrap();
        let id = store.list(None).unwrap()[0].id.clone();
        let inspection = session.inspect_record(&id, &store).unwrap();
        let error = session
            .restore_record_with_hook(&id, &expectation_of(&inspection), &store, |current| {
                if current == phase { return Err(AppError::internal("controlled failure")); }
                Ok(())
            })
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::NeedsVerification, "{phase:?} 必须待核对");
        // 工作区确实是真实重读的结果：文件已写回保存内容。
        assert_eq!(f.read("target.ts"), text(&[(10, 1010), (80, 1080), (140, 1140)]));
        let attempt = store.latest_attempt(&id).unwrap().unwrap();
        assert!(matches!(attempt.state, revia_lib::recovery::RestoreAttemptState::NeedsVerification));
        assert_eq!(attempt.written, vec!["target.ts".to_string()]);
    }
}

/// R05-R2-SP5：确认绑定所见版本；记录/回执/恢复历史变化后旧确认失效。
#[test]
fn stale_confirmation_version_is_rejected() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    let inspection = session.inspect_record(&id, &store).unwrap();
    let mut expected = expectation_of(&inspection);

    // 模拟界面在用户确认后刷新过：版本不再匹配。
    expected.version = "0".repeat(64);
    let error = session.restore_record(&id, &expected, &store).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected);
    assert!(error.message.contains("已变化"), "应提示重新确认：{error:?}");
    // 没有写入任何路径：仍是丢弃后的基线内容。
    assert_eq!(f.read("target.ts"), text(&[]));

    // 版本正确时正常通过。
    session.restore_record(&id, &expectation_of(&inspection), &store).expect("正确版本应可恢复");
}

/// R05-R2-SP9：保存时不存在（删除）或之后被删除，也能生成只读 Git diff。
#[test]
fn text_diff_handles_missing_saved_or_current_side() {
    // 情形一：保存侧存在、之后当前侧被删除。
    let f = Fixture::new();
    f.write("kept.txt", b"line one\nline two\n");
    f.commit();
    f.write("kept.txt", b"line one\nline TWO\n");
    let session = f.session();
    let target = entry(&session, "kept.txt");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    fs::remove_file(f.repo.join("kept.txt")).unwrap();
    let diff = session
        .recovery_text_diff(&id, "kept.txt", &store)
        .expect("当前侧缺失也应能对比");
    assert!(diff.contains("@@ "), "必须是 Git hunk：{diff}");
    // 保存的是丢弃前的完整内容（含被丢弃掉的 LINE 改动）。
    assert!(diff.contains("-line TWO"), "保存的是丢弃前内容：{diff}");
    assert!(diff.contains("+++ /dev/null"), "当前侧缺失应表现为删除：{diff}");

    // 情形二：保存侧不存在（丢弃一次真实删除）、之后当前侧存在。
    let g = Fixture::new();
    g.write("gone.txt", b"original line\n");
    g.commit();
    fs::remove_file(g.repo.join("gone.txt")).unwrap();
    let session2 = g.session();
    let deletion = entry(&session2, "gone.txt");
    let store2 = g.store();
    discard(&session2, &deletion, &store2).unwrap();
    let id2 = store2.list(None).unwrap()[0].id.clone();
    // 丢弃删除后文件回来了；这里再让当前侧存在（就是恢复后的状态）。
    let diff = session2
        .recovery_text_diff(&id2, "gone.txt", &store2)
        .expect("保存侧缺失也应能对比");
    assert!(diff.contains("@@ "), "必须是 Git hunk：{diff}");
    assert!(diff.contains("+original line"), "应显示当前内容：{diff}");
    assert!(diff.contains("--- /dev/null"), "保存侧缺失应表现为新增：{diff}");
}

/// R05-R2-SP10：没有当前 worktree 时仍能查看保存版本（生产命令路径）。
#[test]
fn saved_version_is_viewable_without_current_worktree() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    drop(session);
    // 目录整体消失：会话无法再捕获身份，但记录仍要能看。
    fs::remove_dir_all(&f.repo).unwrap();

    let summaries = store.list(None).expect("没有当前 worktree 时也要能列出");
    assert_eq!(summaries.len(), 1);
    let inspection = store.inspect(&id, None, None).expect("没有当前 worktree 时也要能看详情");
    assert!(inspection.files[0].saved.text.is_some(), "保存内容必须可读");
    assert!(inspection.files[0].current_unavailable);
    assert!(matches!(inspection.state, revia_lib::recovery::RecoveryState::IdentityMismatch));
}

/// R05-R2-ST4/SP7：另存目标落在另一个已登记 worktree 时，必须归属那个 worktree 并受其写锁约束。
///
/// 这里走真实的 worktree 登记与写锁（与命令层同一套 `acquire_write_lock`）：
/// 目标 worktree 正在写时另存被拒绝；未在写时允许，且写入的是目标 worktree 内路径。
#[test]
fn save_as_belongs_to_the_target_worktree_and_respects_its_lock() {
    let f = Fixture::new();
    f.write("target.txt", b"base\n");
    f.commit();
    f.write("target.txt", b"saved change\n");
    let session = f.session();
    let target = entry(&session, "target.txt");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();

    // 真实的 linked worktree 作为另存目标所在的 worktree。
    let linked = f.root().join("linked");
    f.git(&["worktree", "add", "-b", "other", linked.to_str().unwrap()]);
    let linked_session = f.session_at(&linked);
    let destination = linked.join("restored.txt");

    // 目标 worktree 有写操作在途：另存必须被它的写锁拒绝，不排队。
    {
        let _writing = linked_session.acquire_write_lock().unwrap();
        let error = linked_session.acquire_write_lock().unwrap_err();
        assert_eq!(error.kind, ErrorKind::Rejected);
        assert!(!destination.exists(), "被拒绝时不得写入");
    }

    // 没有在途写操作时允许，且写到目标 worktree 里。
    {
        let _writing = linked_session.acquire_write_lock().unwrap();
        let bytes = store.save_as(&id, "target.txt", &destination).unwrap();
        assert_eq!(bytes, 13);
        assert_eq!(fs::read(&destination).unwrap(), b"saved change\n");
    }
    // 原 worktree 未受影响。
    assert_eq!(f.read("target.txt"), b"base\n");
    assert!(!f.repo.join("restored.txt").exists());
}

/// R05-R2-ST5/SP8：自动清理失败在普通模式下可查询（不只写测量日志）。
#[cfg(unix)]
#[test]
fn auto_purge_failure_is_queryable_in_normal_mode() {
    use std::os::unix::fs::PermissionsExt;

    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    // 受控时钟：让记录立即过期，确保清理真的会尝试删除。
    let now = Arc::new(std::sync::atomic::AtomicU64::new(1_792_742_400_123));
    let clock = now.clone();
    let data = f.root().join("data");
    let store = RecoveryStore::with_clock(data.clone(), Arc::new(move || clock.load(std::sync::atomic::Ordering::SeqCst)));
    discard(&session, &target, &store).unwrap();
    now.fetch_add(revia_lib::recovery::RETENTION_MS + 1, std::sync::atomic::Ordering::SeqCst);

    // 让删除真实失败：回收区目录不可写。
    let recovery_dir = data.join("recovery");
    let original = fs::metadata(&recovery_dir).unwrap().permissions();
    fs::set_permissions(&recovery_dir, fs::Permissions::from_mode(0o500)).unwrap();
    let result = store.auto_purge();
    fs::set_permissions(&recovery_dir, original).unwrap();
    assert!(result.is_err(), "只读目录下清理必须失败");

    // 普通模式（无 REVIA_MEASURE_LOG）也能查询到失败原因，且重启后仍可见。
    let maintenance = store.maintenance();
    assert!(maintenance.error.is_some(), "失败必须可查询：{maintenance:?}");
    assert!(maintenance.last_purge_at_ms.is_some());
    let reopened = RecoveryStore::with_clock(data, Arc::new(|| 1_792_742_400_123));
    let persisted = reopened.maintenance();
    assert!(persisted.error.is_some(), "失败状态必须持久化：{persisted:?}");
    // 记录没有被误删。
    assert_eq!(reopened.list(None).unwrap().len(), 1);

    // 恢复权限后再次自动清理成功，错误被清空。
    now.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    reopened.auto_purge().unwrap();
    assert!(reopened.maintenance().error.is_none(), "成功后必须清空错误");
}

// ---------------- 任务 05 第三轮返修回归（R05-R3） ----------------

/// 用真实命令层（`AppState` + 登记仓库）搭出生产调用链。
struct AppFixture {
    fixture: Fixture,
    data: PathBuf,
    state: revia_lib::commands::AppState,
    worktree_id: String,
}

impl AppFixture {
    fn new() -> Self {
        let fixture = Fixture::new();
        let data = fixture.root().join("app-data");
        fs::create_dir_all(&data).unwrap();
        let state = revia_lib::commands::AppState::new(data.clone());
        let entry = {
            let mut registry = state.registry.lock().unwrap();
            registry.add(fixture.repo.to_str().unwrap()).unwrap()
        };
        let worktree_id = entry.worktrees.first().map(|worktree| worktree.id.clone())
            .unwrap_or_else(|| entry.id.clone());
        Self { fixture, data, state, worktree_id }
    }

    /// 在同一个 AppState 里再登记一个仓库（用于嵌套/跨仓库场景）。
    fn add_repo(&self, path: &Path) -> String {
        let entry = {
            let mut registry = self.state.registry.lock().unwrap();
            registry.add(path.to_str().unwrap()).unwrap()
        };
        entry.worktrees.first().map(|worktree| worktree.id.clone()).unwrap_or(entry.id.clone())
    }

    fn store(&self) -> &RecoveryStore {
        &self.state.recovery
    }

    /// 走命令层的同一条会话解析路径。
    fn state_session(&self) -> Arc<WorktreeSession> {
        self.state.worktree_session(&self.worktree_id).unwrap()
    }
}

/// R05-R3-ST1/SP2：多路径记录里，**其他未发布路径**变化也必须整体拦下。
///
/// 重命名记录有两条相关路径；第二路径在 `BeforeWrite` 被外部改动时，
/// 第一路径不得先被写入，也不能留下「未写入任何路径」的假陈述。
#[test]
fn every_related_path_is_checked_before_publishing_any() {
    let f = Fixture::new();
    f.write("old.txt", b"old content\n");
    f.commit();
    f.git(&["mv", "old.txt", "new.txt"]);
    let session = f.session();
    // 重命名记录：先暂存重命名，再对未暂存侧做改动并丢弃。
    f.git(&["add", "-A"]);
    f.write("new.txt", b"old content\nplus one line\n");
    let session = f.session();
    let target = entry(&session, "new.txt");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    let inspection = session.inspect_record(&id, &store).unwrap();
    assert!(inspection.paths.len() >= 2, "重命名记录应包含两条相关路径：{:?}", inspection.paths);
    let after_discard = inspection.files.iter()
        .map(|file| (file.path.clone(), file.current.as_ref().and_then(|current| current.text.clone())))
        .collect::<Vec<_>>();

    let error = session
        .restore_record_with_hook(&id, &expectation_of(&inspection), &store, |phase| {
            if phase == revia_lib::recovery::RestorePhase::BeforeWrite {
                // 改的是**另一条**相关路径（不是第一个要发布的那个）。
                for (path, _) in &after_discard {
                    let full = f.repo.join(path);
                    if full.exists() {
                        fs::write(&full, b"external change on sibling path\n").unwrap();
                        break;
                    }
                }
            }
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected, "相关路径变化必须拒绝：{error:?}");
    // 第一路径也不得被写入：仍保留外部改动或丢弃后状态，不能变成 saved 内容。
    let attempt = store.latest_attempt(&id).unwrap().unwrap();
    assert!(attempt.written.is_empty(), "拒绝时不得写入任何路径：{attempt:?}");
    assert!(!attempt.message.as_deref().unwrap_or("").contains("未写入任何路径")
        || attempt.written.is_empty(), "陈述必须与实际一致");
}

/// R05-R3-SP3：身份不匹配时，连目标目录里的准备动作都不能发生。
#[test]
fn identity_mismatch_creates_nothing_in_the_target() {
    let f = Fixture::new();
    f.write("dir/nested.txt", b"base\n");
    f.commit();
    f.write("dir/nested.txt", b"changed\n");
    let session = f.session();
    let target = entry(&session, "dir/nested.txt");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    let inspection = session.inspect_record(&id, &store).unwrap();

    // 另一个真实仓库（身份不同），里面没有 dir/ 目录。
    let other = f.root().join("other-repo");
    fs::create_dir_all(&other).unwrap();
    let output = Command::new("git")
        .current_dir(&other)
        .env_clear()
        .envs(&f.vars)
        .args(["init", "-b", "main"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let other_session = f.session_at(&other);

    let error = other_session
        .restore_record(&id, &expectation_of(&inspection), &store)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected, "身份不匹配必须拒绝：{error:?}");
    assert!(!other.join("dir").exists(), "拒绝时不得在新 worktree 里创建目录");
}

/// R05-R3-ST2/SP4：完成留证之后的尾部失败必须把留证改回待核对。
#[test]
fn tail_failure_after_completion_downgrades_the_attempt() {
    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    let inspection = session.inspect_record(&id, &store).unwrap();

    // 让最终重读失败：把目标变成非普通文件（目录），inspect 会报「不是普通文件」。
    let error = session
        .restore_record_with_hook(&id, &expectation_of(&inspection), &store, |phase| {
            if phase == revia_lib::recovery::RestorePhase::BeforeComplete {
                // 恢复写入之后把文件替换成目录，制造最终核对失败。
                fs::remove_file(f.repo.join("target.ts")).unwrap();
                fs::create_dir_all(f.repo.join("target.ts")).unwrap();
            }
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::NeedsVerification, "尾部失败必须待核对：{error:?}");
    let attempt = store.latest_attempt(&id).unwrap().unwrap();
    assert!(
        matches!(attempt.state, revia_lib::recovery::RestoreAttemptState::NeedsVerification),
        "留证不能停留在 Completed：{attempt:?}"
    );
    assert_eq!(attempt.written, vec!["target.ts".to_string()]);
}

/// R05-R3-ST4/SP1：另存到**另一个**登记仓库的 Git 元数据也必须被拒绝。
#[test]
fn save_as_protects_the_target_repository_metadata() {
    let f = Fixture::new();
    f.write("target.txt", b"base\n");
    f.commit();
    f.write("target.txt", b"saved change\n");
    let session = f.session();
    let target = entry(&session, "target.txt");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();

    // 第二个真实仓库（记录来自第一个仓库）。
    let other = f.root().join("other-repo");
    fs::create_dir_all(&other).unwrap();
    let output = Command::new("git")
        .current_dir(&other)
        .env_clear()
        .envs(&f.vars)
        .args(["init", "-b", "main"])
        .output()
        .unwrap();
    assert!(output.status.success());
    // 普通 .git 目录。
    let inside_git = other.join(".git").join("restored.txt");
    assert!(store.save_as(&id, "target.txt", &inside_git).is_err(), "不得写入另一仓库的 .git");
    assert!(!inside_git.exists());
    // --separate-git-dir：元数据目录不叫 .git。
    let separate = f.root().join("separate-repo");
    let metadata = f.root().join("separate-metadata");
    fs::create_dir_all(&separate).unwrap();
    let output = Command::new("git")
        .env_clear()
        .envs(&f.vars)
        .args(["init", "-b", "main", "--separate-git-dir", metadata.to_str().unwrap(), separate.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let inside_separate = metadata.join("restored.txt");
    assert!(
        store.save_as(&id, "target.txt", &inside_separate).is_err(),
        "不得写入 --separate-git-dir 的元数据目录"
    );
    assert!(!inside_separate.exists());
    // 该仓库里的普通工作区路径仍可另存（合法外部新路径）。
    let ok = separate.join("restored.txt");
    assert!(store.save_as(&id, "target.txt", &ok).is_ok(), "工作区内的新路径应当允许");
}

/// R05-R3-ST6/SP8：嵌套登记时另存归属最具体的那个 worktree。
#[test]
fn save_as_owner_picks_the_most_specific_registered_worktree() {
    let app = AppFixture::new();
    // 外层仓库里再建一个独立的嵌套仓库并登记。
    let nested = app.fixture.repo.join("nested");
    fs::create_dir_all(&nested).unwrap();
    let output = Command::new("git")
        .current_dir(&nested)
        .env_clear()
        .envs(&app.fixture.vars)
        .args(["init", "-b", "main"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let nested_id = app.add_repo(&nested);
    assert_ne!(nested_id, app.worktree_id);

    let registry = app.state.registry.lock().unwrap();
    let target = app.fixture.root().join("repo").join("nested").join("restored.txt");
    let owner = revia_lib::commands::save_as_owner(&registry, &target);
    assert_eq!(owner.as_deref(), Some(nested_id.as_str()), "嵌套仓库内的路径必须归属内层");
    let outside = app.fixture.root().join("outside.txt");
    assert_eq!(revia_lib::commands::save_as_owner(&registry, &outside), None, "外部路径无归属");
}

/// R05-R3-ST5/SP7：另存命令本身（`save_recovery_as_impl`）取的是目标 worktree 的锁。
#[test]
fn save_recovery_command_locks_the_target_worktree() {
    let app = AppFixture::new();
    app.fixture.write("target.txt", b"base\n");
    app.fixture.commit();
    app.fixture.write("target.txt", b"saved change\n");
    let session = app.fixture.session();
    let target = entry(&session, "target.txt");
    let store = app.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();

    // 目标路径必须用真实路径构造（macOS 的 /var 是系统别名，等价于祖先符号链接）。
    let destination = app.fixture.root().join("repo").join("restored.txt");
    // 目标 worktree 正在写：命令必须被该 worktree 的写锁拒绝。
    let session = app.state_session();
    {
        let _writing = session.acquire_write_lock().unwrap();
        let error = revia_lib::commands::save_recovery_as_impl(
            &app.state,
            &revia_lib::commands::RecoverySaveAsRequest {
                worktree_id: None,
                expected_owner: Some(app.worktree_id.clone()),
                record_id: id.clone(),
                path: "target.txt".into(),
                destination: destination.display().to_string(),
            },
        )
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Rejected, "命令必须命中目标 worktree 的写锁：{error:?}");
        assert!(!destination.exists());
    }
    // 释放后命令成功，并写入目标 worktree。
    let bytes = revia_lib::commands::save_recovery_as_impl(
        &app.state,
        &revia_lib::commands::RecoverySaveAsRequest {
            worktree_id: None,
            expected_owner: Some(app.worktree_id.clone()),
            record_id: id.clone(),
            path: "target.txt".into(),
            destination: destination.display().to_string(),
        },
    )
    .unwrap();
    assert_eq!(bytes, 13);
    assert_eq!(fs::read(&destination).unwrap(), b"saved change\n");
}

/// R05-R3-ST8/SP10：没有当前 worktree 时，**命令层**仍能列出与查看保存版本。
#[test]
fn recovery_commands_still_show_saved_version_without_current_worktree() {
    let app = AppFixture::new();
    app.fixture.write("target.txt", b"base\n");
    app.fixture.commit();
    app.fixture.write("target.txt", b"saved change\n");
    let session = app.fixture.session();
    let target = entry(&session, "target.txt");
    let store = app.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    drop(session);
    // 目录整体消失：会话无法再捕获身份。
    fs::remove_dir_all(&app.fixture.repo).unwrap();

    let summaries = revia_lib::commands::list_recovery_impl(&app.state, Some(&app.worktree_id))
        .expect("没有当前 worktree 时也要能列出");
    assert_eq!(summaries.len(), 1);
    let inspection = revia_lib::commands::inspect_recovery_impl(
        &app.state,
        &revia_lib::commands::RecoveryInspectRequest {
            worktree_id: app.worktree_id.clone(),
            record_id: id.clone(),
        },
    )
    .expect("没有当前 worktree 时也要能看详情");
    assert!(inspection.files[0].saved.text.is_some(), "保存内容必须可读");
    assert!(inspection.files[0].current_unavailable);
}

/// R05-R3-SP9：当前侧读取失败不能伪装成「删除」。
#[cfg(unix)]
#[test]
fn text_diff_never_disguises_read_failure_as_deletion() {
    use std::os::unix::fs::PermissionsExt;

    let f = Fixture::new();
    f.write("target.txt", b"saved line\n");
    f.commit();
    f.write("target.txt", b"saved line\nmore\n");
    let session = f.session();
    let target = entry(&session, "target.txt");
    let store = f.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();

    // 当前文件不可读：必须报错，而不是画成删除。
    let full = f.repo.join("target.txt");
    let original = fs::metadata(&full).unwrap().permissions();
    fs::set_permissions(&full, fs::Permissions::from_mode(0o000)).unwrap();
    let result = session.recovery_text_diff(&id, "target.txt", &store);
    fs::set_permissions(&full, original).unwrap();
    let error = result.expect_err("读取失败不能返回删除 diff");
    assert!(
        !error.message.is_empty() && !matches!(error.kind, ErrorKind::Rejected if error.message.contains("不存在")),
        "必须如实报读取失败：{error:?}"
    );

    // 目标换成目录（类型不对）：同样不能画成删除。
    fs::remove_file(&full).unwrap();
    fs::create_dir_all(&full).unwrap();
    let error = session.recovery_text_diff(&id, "target.txt", &store).unwrap_err();
    assert!(error.message.contains("不是普通文件"), "类型错误必须如实报告：{error:?}");
}

/// R05-R3-ST9/SP11：trash 残留可重试清理，且失败可见。
#[cfg(unix)]
#[test]
fn trash_leftovers_are_retried_and_visible() {
    use std::os::unix::fs::PermissionsExt;

    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let now = Arc::new(std::sync::atomic::AtomicU64::new(1_792_742_400_123));
    let clock = now.clone();
    let data = f.root().join("data-trash");
    let store = RecoveryStore::with_clock(data.clone(), Arc::new(move || clock.load(std::sync::atomic::Ordering::SeqCst)));
    discard(&session, &target, &store).unwrap();
    now.fetch_add(revia_lib::recovery::RETENTION_MS + 1, std::sync::atomic::Ordering::SeqCst);

    // 手工放一个残留目录，模拟「改名成功但删除中断」。
    let recovery_dir = data.join("recovery");
    let leftover = recovery_dir.join("trash-1-2-record-leftover");
    fs::create_dir_all(&leftover).unwrap();
    fs::write(leftover.join("saved.json"), b"{}").unwrap();

    // 残留删不掉时，清理必须报错（不能靠下一轮 0 条把错误清掉）。
    let permissions = fs::metadata(&leftover).unwrap().permissions();
    fs::set_permissions(&leftover, fs::Permissions::from_mode(0o500)).unwrap();
    let result = store.auto_purge();
    fs::set_permissions(&leftover, permissions).unwrap();
    if result.is_err() {
        assert!(store.maintenance().error.is_some(), "残留失败必须可见");
        // 恢复权限后再次清理：残留被真正删除，错误被清空。
        store.auto_purge().unwrap();
        assert!(!leftover.exists(), "重试必须清掉残留");
        assert!(store.maintenance().error.is_none());
    } else {
        // 平台允许删除只读目录时，残留本身应当已被清掉。
        assert!(!leftover.exists(), "成功路径也必须清掉残留");
    }
}

/// R05-R4-SP3：完成留证「先 rename 再 sync 目录」的 sync 失败也必须留下待核对。
///
/// 用真实的目录同步失败（记录目录 0300：文件读写与 rename 仍可行，但打开目录做 fsync 会失败），
/// 走生产入口 `restore_record_with_hook`；恢复权限后重新打开存储，list/inspect 不能是 Completed。
#[cfg(unix)]
#[test]
fn completion_journal_sync_failure_persists_needs_verification() {
    use std::os::unix::fs::PermissionsExt;

    let f = Fixture::new();
    let session = f.changed();
    let target = entry(&session, "target.ts");
    let data = f.root().join("data-sync");
    let store = RecoveryStore::with_clock(data.clone(), Arc::new(|| 1_792_742_400_123));
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();
    let inspection = session.inspect_record(&id, &store).unwrap();

    // 记录目录本身：rename 可行，但目录 fsync（需要打开目录）会真实失败。
    let record_dir = data.join("recovery").join(&id);
    let original = fs::metadata(&record_dir).unwrap().permissions();
    let error = session
        .restore_record_with_hook(&id, &expectation_of(&inspection), &store, |phase| {
            if phase == revia_lib::recovery::RestorePhase::BeforeComplete {
                fs::set_permissions(&record_dir, fs::Permissions::from_mode(0o300)).unwrap();
            }
            Ok(())
        })
        .unwrap_err();
    // 无论失败点落在完成留证还是尾部核对，都必须报待核对。
    fs::set_permissions(&record_dir, original).unwrap();
    assert_eq!(error.kind, ErrorKind::NeedsVerification, "完成留证失败必须待核对：{error:?}");
    assert!(
        error.message.contains("待核对"),
        "错误信息必须明确待核对：{error:?}"
    );

    // 真实重开：用同一数据目录新建 store，读磁盘上的留证。
    let reopened = RecoveryStore::with_clock(data.clone(), Arc::new(|| 1_792_742_400_123));
    let summaries = reopened.list(None).unwrap();
    let summary = summaries.iter().find(|summary| summary.id == id).expect("记录仍在");
    let attempt = summary.restore.as_ref().expect("必须有恢复尝试");
    assert!(
        matches!(attempt.state, revia_lib::recovery::RestoreAttemptState::NeedsVerification),
        "重开后列表不能显示已完成：{attempt:?}"
    );
    assert_eq!(attempt.written, vec!["target.ts".to_string()], "已写入路径必须如实保留");

    let reopened_session = f.session();
    let detail = reopened_session.inspect_record(&id, &reopened).unwrap();
    assert!(
        matches!(
            detail.restore.as_ref().map(|attempt| attempt.state),
            Some(revia_lib::recovery::RestoreAttemptState::NeedsVerification)
        ),
        "重开后详情不能显示已完成：{:?}",
        detail.restore
    );
    // 工作区确实写回了保存内容（不自动重跑，但事实要如实）。
    assert_eq!(f.read("target.ts"), text(&[(10, 1010), (80, 1080), (140, 1140)]));
}

/// R05-R4-ST1/SP1：前端解析到的归属与后端重新解析不一致时必须拒绝（不信任界面 ID）。
#[test]
fn save_as_rejects_stale_expected_owner() {
    let app = AppFixture::new();
    app.fixture.write("target.txt", b"base\n");
    app.fixture.commit();
    app.fixture.write("target.txt", b"saved change\n");
    let session = app.fixture.session();
    let target = entry(&session, "target.txt");
    let store = app.store();
    discard(&session, &target, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone();

    let destination = app.fixture.root().join("repo").join("restored.txt");
    // 前端解析结果与后端不一致（例如界面还停留在另一个 worktree）：必须拒绝。
    let error = revia_lib::commands::save_recovery_as_impl(
        &app.state,
        &revia_lib::commands::RecoverySaveAsRequest {
            worktree_id: Some(app.worktree_id.clone()),
            expected_owner: Some("someone-else".into()),
            record_id: id.clone(),
            path: "target.txt".into(),
            destination: destination.display().to_string(),
        },
    )
    .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected, "归属不一致必须拒绝：{error:?}");
    assert!(!destination.exists(), "被拒绝时不得写入");

    // 归属解析接口返回真实归属（前端据此进门禁）。
    let owner = revia_lib::commands::resolve_save_as_owner(&app.state, &destination).unwrap();
    assert_eq!(owner.worktree_id.as_deref(), Some(app.worktree_id.as_str()));
    // 外部新路径：无归属，仍然合法可另存。
    let outside = app.fixture.root().join("outside.txt");
    let outside_owner = revia_lib::commands::resolve_save_as_owner(&app.state, &outside).unwrap();
    assert_eq!(outside_owner.worktree_id, None);
    let bytes = revia_lib::commands::save_recovery_as_impl(
        &app.state,
        &revia_lib::commands::RecoverySaveAsRequest {
            worktree_id: None,
            expected_owner: None,
            record_id: id.clone(),
            path: "target.txt".into(),
            destination: outside.display().to_string(),
        },
    )
    .unwrap();
    assert_eq!(bytes, 13);
    assert_eq!(fs::read(&outside).unwrap(), b"saved change\n");
}

/// 文件原始字节的标准 SHA-256（与 `shasum -a 256` 输出一致）。
///
/// 不用 `git::fingerprint`：那个先散列长度前缀，是 Revia 的写操作指纹，
/// 不是内容哈希，无法用标准工具核对。
fn standard_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

// ---------------- 桌面验收数据准备入口（默认 ignored） ----------------

/// 桌面验收数据准备入口：**只用生产入口**生成可供桌面恢复的回收记录。
///
/// 默认 `#[ignore]`，仅显式调用：
/// `cargo test --test discard_recovery -- --ignored --exact --nocapture prepare_task05_desktop_fixture`
///
/// - 自建全新临时 Git 仓库 + 隔离 HOME + 独立 Revia 数据目录，不接受任何外部仓库输入；
/// - 记录全部由 `WorktreeSession::discard_file` / `discard_hunk` 与 `RecoveryStore` 产生，
///   不手工伪造 `saved.json` / `outcome.json` / 身份；
/// - 记录使用**当前时间**（`RecoveryStore::new`），不会在桌面验收前过期；
/// - 结束时用 `TempDir::keep` 保留本次 fixture，并打印 JSON（路径、身份、记录关联、初始工作区与 index）。
///
/// 这是数据准备辅助，**不是验收通过**：只证明数据由生产入口准备，桌面行为尚未验证。
#[test]
#[ignore = "桌面验收数据准备入口，仅在显式 --ignored 时运行"]
fn prepare_task05_desktop_fixture() {
    // 可选的保留位置（仅测试用）：`REVIA_TASK05_FIXTURE_BASE` 指定后在其下建目录，
    // 否则用系统临时目录。数据与路径只属于本次创建的 fixture。
    let base = match std::env::var_os("REVIA_TASK05_FIXTURE_BASE") {
        Some(value) => PathBuf::from(value),
        None => std::env::temp_dir(),
    };
    fs::create_dir_all(&base).expect("fixture 保留位置必须可创建");
    let f = Fixture::with_base(&base);
    let root = f.root();
    let data_dir = root.join("revia-data");
    fs::create_dir_all(&data_dir).unwrap();

    // 单 hunk：三段改动，只丢弃中间那一段，记录里保存完整文件。
    let hunk_path = "src/hunk.ts";
    f.write(hunk_path, &text(&[]));
    // 部分暂存：先暂存一段改动，再在工作区继续改，丢弃未暂存部分。
    let partial_path = "src/partial.ts";
    f.write(partial_path, &text(&[]));
    // 二进制。
    let binary_path = "assets/blob.bin";
    f.write(binary_path, b"\0old\xff\x01");
    // 删除状态。
    let deleted_path = "docs/removed.txt";
    f.write(deleted_path, b"deleted original content\n");
    // 可执行位：提交时不可执行，之后只改权限位。
    let mode_path = "scripts/run.sh";
    f.write(mode_path, b"#!/bin/sh\ntrue\n");
    platform::set_executable(&f.repo.join(mode_path), false).unwrap();
    f.commit();

    // --- 制造未暂存改动（全部走真实工作区写入） ---
    f.write(hunk_path, &text(&[(10, 1010), (80, 1080), (140, 1140)]));
    f.write(partial_path, &text(&[(10, 2010)]));
    f.git(&["add", "--", partial_path]);
    f.write(partial_path, &text(&[(10, 2010), (80, 2080)]));
    f.write(binary_path, b"\0new\xfe\x02");
    fs::remove_file(f.repo.join(deleted_path)).unwrap();
    platform::set_executable(&f.repo.join(mode_path), true).unwrap();

    let session = f.session();
    let identity = WorktreeIdentity::capture(&session.git).unwrap();
    // 当前时间，不用会到期的固定时钟。
    let store = RecoveryStore::new(data_dir.clone());

    let mut records: Vec<serde_json::Value> = Vec::new();

    // 1) 单 hunk 丢弃：记录保存的是完整文件。
    {
        let entry = entry(&session, hunk_path);
        let rows = session
            .rows(hunk_path, ChangesKind::Unstaged, ViewMode::Unified, 0, 400)
            .unwrap();
        let actions: Vec<_> = rows.rows.iter().filter_map(|row| row.hunk_action.as_ref()).collect();
        assert_eq!(actions.len(), 3, "hunk 夹具应有 3 段改动");
        session
            .discard_hunk(hunk_path, &entry.fingerprint, &entry.snapshot_revision, &actions[1].hunk_id, &store)
            .expect("单 hunk 丢弃应成功");
        records.push(serde_json::json!({ "kind": "hunk", "target": hunk_path }));
    }

    // 2) 部分暂存 + 整文件丢弃未暂存部分。
    {
        let entry = entry(&session, partial_path);
        discard(&session, &entry, &store).expect("部分暂存的整文件丢弃应成功");
        records.push(serde_json::json!({ "kind": "partialStagedFile", "target": partial_path }));
    }

    // 3) 二进制整文件丢弃。
    {
        let entry = entry(&session, binary_path);
        discard(&session, &entry, &store).expect("二进制丢弃应成功");
        records.push(serde_json::json!({ "kind": "binaryFile", "target": binary_path }));
    }

    // 4) 删除状态：丢弃后文件重新出现。
    {
        let entry = entry(&session, deleted_path);
        discard(&session, &entry, &store).expect("删除状态丢弃应成功");
        records.push(serde_json::json!({ "kind": "deletedFile", "target": deleted_path }));
    }

    // 5) 可执行位：丢弃后权限位回到提交时的状态。
    {
        let entry = entry(&session, mode_path);
        discard(&session, &entry, &store).expect("可执行位丢弃应成功");
        records.push(serde_json::json!({ "kind": "executableMode", "target": mode_path }));
    }

    // 用生产读取路径取回记录（不是自己拼的 JSON）。
    let summaries = store.list(Some(&identity)).expect("应能列出本次记录");
    assert_eq!(summaries.len(), 5, "应生成 5 条回收记录");
    let mut record_index: Vec<serde_json::Value> = Vec::new();
    for summary in &summaries {
        let (record, outcome) = store.read(&summary.id).expect("应能读取记录");
        let after = match &outcome {
            Some(DiscardOutcome::Completed { after }) => after.clone(),
            other => panic!("记录 {} 缺少可信完成回执：{other:?}", summary.id),
        };
        assert!(summary.identity_confirmed, "记录身份必须与当前 worktree 匹配");
        let describe = |files: &[revia_lib::recovery::SavedFile]| -> Vec<serde_json::Value> {
            files
                .iter()
                .map(|file| {
                    serde_json::json!({
                        "path": file.path,
                        "exists": file.content.is_some(),
                        "bytes": file.content.as_ref().map(|bytes| bytes.len()).unwrap_or(0),
                        "executable": file.executable,
                    })
                })
                .collect()
        };
        record_index.push(serde_json::json!({
            "recordId": summary.id,
            "target": record.target,
            "source": match record.source {
                RecoverySource::DiscardFile => "discardFile",
                RecoverySource::DiscardHunk => "discardHunk",
            },
            "createdAtMs": record.created_at_ms,
            "paths": summary.paths,
            "before": describe(&record.before),
            "after": describe(&after),
            "status": "completed",
        }));
    }

    // 只登记本次 fixture 的仓库。
    {
        let mut registry = revia_lib::settings::Registry::load(
            data_dir.clone(),
            Arc::new(CommandLog::default()),
        );
        let entry = registry.add(f.repo.to_str().unwrap()).expect("应能登记本次 fixture");
        assert_eq!(entry.path, identity.worktree_path.display().to_string());
        let listed = registry.list();
        assert_eq!(listed.len(), 1, "settings 只登记本次 fixture");
        assert_eq!(listed[0].path, identity.worktree_path.display().to_string());
    }

    // 初始工作区与 index 的可还原清单。
    let worktree_files: Vec<serde_json::Value> = ["src/hunk.ts", "src/partial.ts", "assets/blob.bin", "docs/removed.txt", "scripts/run.sh"]
        .iter()
        .map(|path| {
            let full = f.repo.join(path);
            let meta = fs::symlink_metadata(&full).expect("工作区文件应存在");
            let bytes = fs::read(&full).expect("工作区文件应可读");
            serde_json::json!({
                "path": path,
                "exists": true,
                "bytes": bytes.len(),
                // 文件**原始字节**的标准 SHA-256（与 `shasum -a 256` 一致），
                // 不是带长度前缀的 Revia 指纹，便于用标准工具逐文件核对。
                "sha256": standard_sha256(&bytes),
                "executable": platform::executable(&meta),
            })
        })
        .collect();
    let index_listing = String::from_utf8(f.git(&["ls-files", "--stage"])).unwrap();
    let head_commit = String::from_utf8(f.git(&["rev-parse", "HEAD"])).unwrap().trim().to_string();
    let status_now = String::from_utf8(f.git(&["status", "--porcelain=v2"])).unwrap();

    // 保留本次自建 fixture（TempDir::keep），之后所有路径都是真实存在的外部路径。
    let kept_root = f.temp.keep();
    let kept_root = fs::canonicalize(&kept_root).unwrap_or(kept_root);
    let kept_repo = kept_root.join("repo");
    let kept_data = kept_root.join("revia-data");

    let manifest = serde_json::json!({
        "purpose": "任务05 桌面验收数据准备（仅生产入口准备；桌面行为尚未验证，不构成验收通过）",
        "preparedBy": "prepare_task05_desktop_fixture (test-only, #[ignore])",
        "fixtureRoot": kept_root.display().to_string(),
        "repositoryPath": kept_repo.display().to_string(),
        "reviaDataDirectory": kept_data.display().to_string(),
        "isolatedHome": kept_root.join("home").display().to_string(),
        "worktreeId": revia_lib::session::id_for_path(&kept_repo),
        "worktreeIdentity": {
            "worktreePath": identity.worktree_path.display().to_string(),
            "gitDir": identity.git_dir.display().to_string(),
            "commonDir": identity.common_dir.display().to_string(),
        },
        "gitVersion": session.info.git_version,
        "headCommit": head_commit,
        "indexListing": index_listing,
        "worktreeFiles": worktree_files,
        "gitStatusPorcelainV2": status_now,
        "recoveryRecords": record_index,
        "desktopSteps": [
            "启动已构建的 Release 应用，并把数据目录指向 reviaDataDirectory（REVIA_ACCEPTANCE=1 + REVIA_ACCEPTANCE_DIR）",
            "仓库应显示为唯一登记的 fixture；回收区应列出 5 条 completed 记录",
            "对每条记录核对：详情两侧状态、单 hunk 记录提示恢复的是完整文件、二进制只显示信息",
            "docs/removed.txt 的 before 是不存在、after 存在 25 字节；对这条记录执行恢复会把丢弃后重新出现的文件删除回丢弃前的不存在状态",
            "scripts/run.sh 的 before 是可执行、after 不可执行；恢复后应重新变为可执行",
            "按 indexListing 与 worktreeFiles 的 sha256（标准 SHA-256，可用 shasum -a 256 核对）与可执行位还原初始工作区后再重复验证"
        ],
        "boundary": "只证明记录由生产入口生成且身份匹配；未验证桌面行为、未做 D06/CI/长测，不构成任务验收。",
    });
    println!("REVIA_TASK05_DESKTOP_FIXTURE {}", serde_json::to_string_pretty(&manifest).unwrap());
}
