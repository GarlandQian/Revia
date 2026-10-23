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
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
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
