//! M6 端到端接缝：真实 Git 仓库、真实归档目录、真实进程环境。
//!
//! 这些测试只使用系统 Git 命令行和临时 HOME，不 mock Git，也不把工作区写入
//! 伪装成单元测试。覆盖 stash 归档、自动保存、操作继续/中止、历史改写、
//! 文件历史与 blame、外部 diff/merge 工具，以及交互式 rebase edit 停点的提交门禁。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;

use revia_lib::autostash::AutostashStore;
use revia_lib::commit::DraftStore;
use revia_lib::external_tools::{self, ExternalToolKind, ExternalToolRequest};
use revia_lib::file_history::{self, BlameRequest, FileHistoryRequest};
use revia_lib::git::CommandLog;
use revia_lib::operation::{self, OperationKind};
use revia_lib::operation_flow::{self, OperationAction, OperationActionRequest};
use revia_lib::recovery::{RecoverySource, RecoveryStore, WorktreeIdentity};
use revia_lib::rewrite::{
    self, CherryPickRequest, DirtyStrategy, InteractiveRebaseRequest, RebaseAction,
    RebasePlanEntry, ResetMode, ResetRequest, RevertRequest,
};
use revia_lib::session::WorktreeSession;
use revia_lib::shell::{EnvCaptureInfo, ShellEnv};
use revia_lib::stash::{self, StashApplyRequest, StashSaveRequest};
use revia_lib::stash_archive::StashArchiveStore;

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
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        #[cfg(unix)]
        paths.extend(["/opt/homebrew/bin".into(), "/usr/bin".into(), "/bin".into()]);
        let mut vars = HashMap::from([
            ("HOME".into(), home.display().to_string()),
            ("USERPROFILE".into(), home.display().to_string()),
            ("XDG_CONFIG_HOME".into(), home.display().to_string()),
            (
                "GIT_CONFIG_GLOBAL".into(),
                home.join(".gitconfig").display().to_string(),
            ),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            (
                "PATH".into(),
                std::env::join_paths(paths.iter().map(Path::new))
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            ),
        ]);
        for key in ["SYSTEMROOT", "WINDIR", "TEMP", "TMP", "TMPDIR"] {
            if let Ok(value) = std::env::var(key) {
                vars.insert(key.into(), value);
            }
        }
        let fixture = Self {
            temp,
            repo,
            vars,
            paths,
        };
        fixture.git(&["init", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Revia M6 Fixture"),
            ("user.email", "m6@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
        ] {
            fixture.git(&["config", key, value]);
        }
        fixture
    }

    fn git(&self, args: &[&str]) -> Vec<u8> {
        let output = self.command(args, &[]);
        assert!(
            output.status.success(),
            "Git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    fn try_git(&self, args: &[&str]) -> Output {
        self.command(args, &[])
    }

    fn command(&self, args: &[&str], extra: &[(&str, &str)]) -> Output {
        let mut command = Command::new("git");
        command
            .current_dir(&self.repo)
            .env_clear()
            .envs(&self.vars)
            .args(args);
        for (key, value) in extra {
            command.env(key, value);
        }
        command.output().unwrap()
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        let path = self.repo.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    fn read(&self, name: &str) -> Vec<u8> {
        fs::read(self.repo.join(name)).unwrap()
    }

    fn commit_all(&self, message: &str) -> String {
        self.git(&["add", "--all"]);
        self.git(&["commit", "--quiet", "-m", message]);
        self.head()
    }

    fn head(&self) -> String {
        String::from_utf8(self.git(&["rev-parse", "HEAD"]))
            .unwrap()
            .trim()
            .to_string()
    }

    fn oid(&self, revision: &str) -> String {
        String::from_utf8(self.git(&["rev-parse", revision]))
            .unwrap()
            .trim()
            .to_string()
    }

    fn index(&self) -> Vec<u8> {
        self.git(&["ls-files", "--stage", "-z"])
    }

    fn index_clean(&self) -> bool {
        self.try_git(&["diff", "--cached", "--quiet"])
            .status
            .success()
    }

    fn branch(&self, name: &str) {
        self.git(&["checkout", "--quiet", "-b", name]);
    }

    fn checkout(&self, name: &str) {
        self.git(&["checkout", "--quiet", name]);
    }

    fn session(&self) -> Arc<WorktreeSession> {
        self.session_at(&self.repo)
    }

    fn session_at(&self, path: &Path) -> Arc<WorktreeSession> {
        let shell = ShellEnv {
            vars: self.vars.clone(),
            path: self.paths.clone(),
            info: EnvCaptureInfo {
                worktree: path.display().to_string(),
                shell: "fixture".into(),
                elapsed_ms: 0,
                timed_out: false,
                used_fallback: true,
                var_count: self.vars.len(),
                path_entries: self.paths.clone(),
                diagnostics: None,
            },
        };
        WorktreeSession::open(
            "m6-fixture",
            path,
            shell,
            Arc::new(CommandLog::default()),
        )
        .unwrap()
    }

    fn data(&self) -> PathBuf {
        self.temp.path().join("data")
    }

    fn recovery(&self) -> RecoveryStore {
        RecoveryStore::new(self.data())
    }

    fn archives(&self) -> StashArchiveStore {
        StashArchiveStore::new(&self.data())
    }

    fn autostash(&self) -> AutostashStore {
        AutostashStore::new(&self.data())
    }

    fn identity(&self, session: &WorktreeSession) -> WorktreeIdentity {
        WorktreeIdentity::capture(&session.git).unwrap()
    }
}

fn recovery_sources(store: &RecoveryStore) -> Vec<RecoverySource> {
    store
        .list(None)
        .unwrap()
        .into_iter()
        .map(|record| record.source)
        .collect()
}

#[test]
fn stash_archive_survives_gc_and_restores_into_stash_list_without_touching_worktree_or_index() {
    let f = Fixture::new();
    f.write("tracked.txt", b"base\n");
    f.commit_all("base");
    f.write("tracked.txt", b"stashed\n");
    f.write("untracked/new.txt", b"untracked\n");
    f.git(&[
        "stash",
        "push",
        "--include-untracked",
        "--quiet",
        "-m",
        "keep me",
    ]);
    let stash_oid = f.oid("refs/stash");
    let index_after_stash = f.index();
    let worktree_after_stash = f.read("tracked.txt");

    let session = f.session();
    let identity = f.identity(&session);
    // Exercise canonical application-data paths at both Git argv boundaries.
    fs::create_dir_all(f.data()).unwrap();
    let data = fs::canonicalize(f.data()).unwrap();
    let archives = StashArchiveStore::new(&data);
    let record = archives
        .archive(
            &session.git,
            identity.clone(),
            &stash_oid,
            "keep me",
            Some("main".into()),
        )
        .unwrap();
    assert!(record.id.starts_with("stash-"));
    assert_eq!(record.stash_oid, stash_oid);

    // 真正删除源 stash 并让 Git 回收对象；归档必须是唯一的对象来源。
    f.git(&["stash", "drop", "--quiet"]);
    f.git(&["reflog", "expire", "--expire=now", "--all"]);
    f.git(&["gc", "--prune=now", "--quiet"]);
    assert!(stash::list(&session.git).unwrap().is_empty());

    // 新实例模拟应用重启后重新打开同一个数据目录。
    let reopened = StashArchiveStore::new(&data);
    let summaries = reopened.list(Some(&identity)).unwrap();
    assert_eq!(summaries.len(), 1);
    assert!(summaries[0].identity_confirmed);
    assert_eq!(summaries[0].stash_oid, stash_oid);

    let restored = reopened.restore(&session, &record.id).unwrap();
    assert_eq!(restored, stash_oid);
    assert_eq!(f.index(), index_after_stash);
    assert_eq!(f.read("tracked.txt"), worktree_after_stash);
    let entries = stash::list(&session.git).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].oid, stash_oid);
    assert_eq!(entries[0].message, "keep me");

    // 归档只存在于应用数据目录；用户仓库不能出现 Revia 私有引用。
    let refs = String::from_utf8(f.git(&["for-each-ref", "--format=%(refname)"])).unwrap();
    assert!(!refs.contains("refs/revia/"), "unexpected refs: {refs}");
    assert!(refs.lines().any(|line| line == "refs/stash"));
}

#[test]
fn stash_archive_restores_from_a_surviving_worktree_without_relaxing_file_recovery() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    let source_path = f.temp.path().join("source"); let survivor_path = f.temp.path().join("survivor");
    f.git(&["worktree", "add", "-b", "source", source_path.to_str().unwrap()]);
    f.git(&["worktree", "add", "-b", "survivor", survivor_path.to_str().unwrap()]);
    let source = f.session_at(&source_path); let survivor = f.session_at(&survivor_path);
    fs::write(source_path.join("file"), b"saved in source\n").unwrap();
    let identity = f.identity(&source); let paths = vec!["file".to_string()];
    let files = revia_lib::recovery::capture_files(&source.path, &paths).unwrap();
    let context = revia_lib::recovery::context_fingerprint(&source.git, &paths).unwrap();
    let (_, ordinary_guard) = f.recovery().save(identity.clone(), RecoverySource::DiscardFile, "file", files, context).unwrap();
    drop(ordinary_guard);
    assert_eq!(source.git.run_write(&["stash", "push", "-qm", "from removed worktree"]).unwrap().exit_code, 0);
    let oid = stash::list(&source.git).unwrap()[0].oid.clone(); let archives = f.archives();
    let archived = stash::drop(&source, &archives, &oid).unwrap().archive_id.unwrap();
    f.git(&["worktree", "remove", source_path.to_str().unwrap()]); assert!(!source_path.exists());
    let current = f.identity(&survivor);
    assert_eq!(identity.common_dir, current.common_dir); assert_eq!(identity.filesystem_ids[2], current.filesystem_ids[2]);
    assert!(archives.list(Some(&current)).unwrap()[0].identity_confirmed);
    assert!(!f.recovery().list(Some(&current)).unwrap()[0].identity_confirmed);
    fs::write(survivor_path.join("file"), b"survivor staged\n").unwrap();
    assert_eq!(survivor.git.run_write(&["add", "--", "file"]).unwrap().exit_code, 0);
    fs::write(survivor_path.join("file"), b"survivor unstaged\n").unwrap();
    fs::write(survivor_path.join("untracked"), b"keep\n").unwrap();
    let index = survivor.git.run_ok(&["ls-files", "--stage", "-z"], "fixture index").unwrap().stdout;
    let head = survivor.git.run_ok(&["rev-parse", "HEAD"], "fixture HEAD").unwrap().stdout;
    let state = stash::preflight(&survivor, None).unwrap();
    assert_eq!(archives.restore_confirmed(&survivor, &archived, &state).unwrap(), oid);
    println!("shared restore fixture={} commands={:?}", f.temp.path().display(), survivor.git.log.snapshot());
    assert_eq!(survivor.git.run_ok(&["ls-files", "--stage", "-z"], "fixture index").unwrap().stdout, index);
    assert_eq!(survivor.git.run_ok(&["rev-parse", "HEAD"], "fixture HEAD").unwrap().stdout, head);
    assert_eq!(fs::read(survivor_path.join("file")).unwrap(), b"survivor unstaged\n");
    assert_eq!(fs::read(survivor_path.join("untracked")).unwrap(), b"keep\n");
    assert_eq!(f.read("file"), b"base\n"); assert_eq!(stash::list(&survivor.git).unwrap()[0].oid, oid);
    assert_eq!(stash::list(&f.session().git).unwrap()[0].oid, oid);
    assert_eq!(survivor.git.run_ok(&["show", &format!("{oid}:file")], "fixture saved file").unwrap().stdout, b"saved in source\n");
}

#[test]
fn stash_archive_repository_guard_rejects_clones_unknown_ids_and_recreated_common_dirs() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    let session = f.session(); f.write("file", b"saved\n"); f.git(&["stash", "push", "-qm", "saved"]);
    let oid = f.oid("refs/stash"); let archives = f.archives();
    let id = stash::drop(&session, &archives, &oid).unwrap().archive_id.unwrap();
    let clone_path = f.temp.path().join("clone");
    f.git(&["clone", "--quiet", f.repo.to_str().unwrap(), clone_path.to_str().unwrap()]);
    let cloned = f.session_at(&clone_path); let clone_identity = f.identity(&cloned);
    assert!(!archives.list(Some(&clone_identity)).unwrap()[0].identity_confirmed);
    let clone_index = cloned.git.run_ok(&["ls-files", "--stage", "-z"], "fixture index").unwrap().stdout;
    let error = archives.restore_confirmed(&cloned, &id, &stash::preflight(&cloned, None).unwrap()).unwrap_err();
    assert_eq!(error.code, Some("errors.stashArchive.archiveBelongsAnotherRepository"));
    assert!(stash::list(&cloned.git).unwrap().is_empty());
    assert_eq!(cloned.git.run_ok(&["ls-files", "--stage", "-z"], "fixture index").unwrap().stdout, clone_index);
    let metadata = f.data().join("stash-archive").join(&id).join("metadata.json");
    let saved = fs::read(&metadata).unwrap(); let mut unknown: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    unknown["identity"]["filesystemIds"][2] = serde_json::Value::Null;
    fs::write(&metadata, serde_json::to_vec(&unknown).unwrap()).unwrap();
    assert!(!archives.list(Some(&f.identity(&session))).unwrap()[0].identity_confirmed);
    assert_eq!(archives.restore(&session, &id).unwrap_err().code, Some("errors.stashArchive.archiveBelongsAnotherRepository"));
    fs::write(&metadata, saved).unwrap();
    let original = f.identity(&session); fs::rename(f.repo.join(".git"), f.temp.path().join("original-git")).unwrap();
    f.git(&["init", "-b", "replacement"]);
    f.git(&["config", "user.name", "Fixture"]); f.git(&["config", "user.email", "fixture@example.invalid"]);
    f.git(&["config", "commit.gpgsign", "false"]); f.commit_all("replacement");
    let replacement = f.session(); let current = f.identity(&replacement);
    assert_eq!(current.common_dir, original.common_dir); assert_ne!(current.filesystem_ids[2], original.filesystem_ids[2]);
    assert!(!archives.list(Some(&current)).unwrap()[0].identity_confirmed);
    assert_eq!(archives.restore(&replacement, &id).unwrap_err().code, Some("errors.stashArchive.archiveBelongsAnotherRepository"));
    assert_eq!(archives.restore(&session, &id).unwrap_err().code, Some("errors.session.worktreeIdentityChanged"));
    assert!(stash::list(&replacement.git).unwrap().is_empty()); assert_eq!(f.read("file"), b"base\n");
    assert!(metadata.is_file());
}

#[test]
fn stash_archive_restore_preserves_the_shown_confirmation_through_the_real_fetch() {
    for refresh_only in [true, false] {
        let f = Fixture::new(); f.write("file", b"base\n"); let head = f.commit_all("base");
        f.write("file", b"saved\n"); f.git(&["stash", "push", "-qm", "saved"]);
        let session = f.session(); let oid = f.oid("refs/stash"); let archives = f.archives();
        let id = stash::drop(&session, &archives, &oid).unwrap().archive_id.unwrap();
        let state = stash::preflight(&session, None).unwrap(); let index = f.index();
        let error = archives.restore_confirmed_with_after_fetch(&session, &id, &state, || {
            assert!(session.git.log.snapshot().iter().any(|record| record.args.first().is_some_and(|arg| arg == "fetch") && record.exit_code == 0));
            if refresh_only { session.refresh().unwrap(); } else { f.write("file", b"external change\n"); }
            Ok(())
        }).unwrap_err();
        println!("archive confirmation fixture={} refresh_only={refresh_only} error={error:?} commands={:?}", f.temp.path().display(), session.git.log.snapshot());
        assert_eq!(error.code, Some("errors.rewrite.stateChanged")); assert_eq!(error.kind, revia_lib::error::ErrorKind::Rejected);
        assert!(!session.git.log.snapshot().iter().any(|record| record.args.first().is_some_and(|arg| arg == "stash")
            && record.args.get(1).is_some_and(|arg| arg == "store")));
        assert!(stash::list(&session.git).unwrap().is_empty()); assert_eq!(f.index(), index); assert_eq!(f.head(), head);
        let content = if refresh_only { b"base\n".as_slice() } else { b"external change\n".as_slice() };
        assert_eq!(f.read("file"), content);
        let fresh = stash::preflight(&session, None).unwrap();
        assert_eq!(archives.restore_confirmed(&session, &id, &fresh).unwrap(), oid);
        assert_eq!(f.index(), index); assert_eq!(f.read("file"), content); assert_eq!(f.head(), head);
    }
}

#[test]
fn stash_archive_failure_leaves_entries_untouched() {
    let f = Fixture::new();
    f.write("tracked.txt", b"base\n");
    f.commit_all("base");
    f.write("tracked.txt", b"stashed\n");
    f.git(&["stash", "push", "--quiet", "-m", "do not lose"]);
    let stash_oid = f.oid("refs/stash");

    // 数据目录位置被普通文件占住，归档初始化必然失败。
    fs::write(f.data(), b"not a directory\n").unwrap();
    let session = f.session();
    let archives = f.archives();
    let error = stash::drop(&session, &archives, &stash_oid).unwrap_err();
    assert!(error.code.is_some());
    let entries = stash::list(&session.git).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].oid, stash_oid);

    let error = stash::clear(&session, &archives, &[stash_oid.clone()]).unwrap_err();
    assert!(error.code.is_some());
    let entries = stash::list(&session.git).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].oid, stash_oid);
}

#[test]
fn stash_apply_conflict_keeps_entry_and_snapshots_dirty_files() {
    let f = Fixture::new();
    f.write("conflict.txt", b"base\n");
    f.commit_all("base");
    f.write("conflict.txt", b"stashed\n");
    f.git(&["stash", "push", "--quiet", "-m", "conflicting stash"]);
    let stash_oid = f.oid("refs/stash");
    f.write("conflict.txt", b"main\n");
    f.write("notes.txt", b"base notes\n");
    f.commit_all("main");
    f.write("notes.txt", b"dirty notes\n");

    let session = f.session();
    let recovery = f.recovery();
    let archives = f.archives();
    let entry = stash::list(&session.git)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let outcome = stash::apply(
        &session,
        &recovery,
        &archives,
        &StashApplyRequest { entry_key: None,
            expected_state: stash::preflight(&session, Some(&entry.oid.clone())).unwrap(),
            oid: entry.oid.clone(),
            restore_index: false,
            delete_after: true,
            new_branch: None,
        },
    )
    .unwrap();
    assert!(!outcome.applied);
    assert!(outcome.conflicted);
    assert!(!outcome.deleted);
    assert!(outcome.recovery_id.is_some());
    assert_eq!(stash::list(&session.git).unwrap().len(), 1);
    assert_eq!(stash::list(&session.git).unwrap()[0].oid, stash_oid);
    assert!(
        recovery_sources(&recovery).contains(&RecoverySource::StashApply),
        "apply must snapshot dirty files before touching the worktree"
    );
    assert!(f.read("conflict.txt").starts_with(b"<<<<<<<"));
}

#[test]
fn autostash_roundtrip_restores_staged_and_untracked_changes_then_archives_entry() {
    let f = Fixture::new();
    f.write("tracked.txt", b"base\n");
    f.commit_all("base");
    let base_index = f.index();
    f.write("tracked.txt", b"staged\n");
    f.git(&["add", "tracked.txt"]);
    f.write("tracked.txt", b"worktree\n");
    f.write("new.txt", b"untracked\n");
    let before_index = f.index();

    let session = f.session();
    let recovery = f.recovery();
    let archives = f.archives();
    let auto = f.autostash();
    let link = auto
        .save_before(&session, &recovery, "rebase")
        .unwrap()
        .unwrap();
    assert_eq!(link.operation, "rebase");
    assert!(link.recovery_id.is_some());
    assert_eq!(f.index(), base_index);
    assert!(f.index_clean());
    assert_eq!(f.read("tracked.txt"), b"base\n");
    assert!(!f.repo.join("new.txt").exists());
    assert!(recovery_sources(&recovery).contains(&RecoverySource::HistoryAutostash));

    // 新建实例读取持久化的 links.json，模拟重启后的恢复。
    let reopened = AutostashStore::new(&f.data());
    let outcome = reopened
        .restore_if_ready(&session, &recovery, &archives)
        .unwrap()
        .unwrap();
    assert!(outcome.restored);
    assert!(!outcome.conflicted);
    assert!(outcome.removed);
    assert_eq!(f.index(), before_index);
    assert_eq!(f.read("tracked.txt"), b"worktree\n");
    assert_eq!(f.read("new.txt"), b"untracked\n");
    assert!(stash::list(&session.git).unwrap().is_empty());
    assert!(reopened.for_worktree(&session).unwrap().is_none());
    assert_eq!(archives.list(None).unwrap().len(), 1);
}

#[test]
fn operation_flow_rejects_stale_state_blocks_continue_and_skip_and_records_abort() {
    let f = Fixture::new();
    f.write("conflict.txt", b"base\n");
    f.commit_all("base");
    f.branch("feature");
    f.write("conflict.txt", b"feature\n");
    f.commit_all("feature");
    f.checkout("main");
    f.write("conflict.txt", b"main\n");
    f.commit_all("main");
    assert!(!f.try_git(&["merge", "feature"]).status.success());

    let session = f.session();
    let recovery = f.recovery();
    let archives = f.archives();
    let auto = f.autostash();
    let state = operation_flow::state(&session, &f.data()).unwrap().unwrap();
    assert_eq!(state.kind, OperationKind::Merge);
    assert!(state.conflict_count > 0);

    let stale = OperationActionRequest {
            expected_state: state.confirmation_state.clone(),
        kind: OperationKind::Merge,
        metadata_version: "stale".into(),
        action: OperationAction::Abort,
    };
    let error = operation_flow::execute(&session, &recovery, &auto, &archives, &stale).unwrap_err();
    assert_eq!(
        error.code,
        Some("errors.operationFlow.operationStateChanged")
    );

    let skip = OperationActionRequest {
            expected_state: state.confirmation_state.clone(),
        kind: OperationKind::Merge,
        metadata_version: state.metadata_version.clone(),
        action: OperationAction::Skip,
    };
    let error = operation_flow::execute(&session, &recovery, &auto, &archives, &skip).unwrap_err();
    assert_eq!(error.code, Some("errors.operationFlow.mergeCannotSkip"));

    let continue_request = OperationActionRequest {
            expected_state: state.confirmation_state.clone(),
        kind: OperationKind::Merge,
        metadata_version: state.metadata_version.clone(),
        action: OperationAction::Continue,
    };
    let error = operation_flow::execute(&session, &recovery, &auto, &archives, &continue_request)
        .unwrap_err();
    assert_eq!(error.code, Some("errors.operationFlow.unresolvedConflicts"));

    let abort = OperationActionRequest {
            expected_state: state.confirmation_state.clone(),
        kind: OperationKind::Merge,
        metadata_version: state.metadata_version,
        action: OperationAction::Abort,
    };
    let outcome = operation_flow::execute(&session, &recovery, &auto, &archives, &abort).unwrap();
    assert!(outcome.completed);
    assert!(outcome.state.is_none());
    assert!(outcome.recovery_id.is_some());
    assert!(
        operation::detect(
            &session.git,
            operation::conflict_count(&session.git).unwrap()
        )
        .unwrap()
        .is_none()
    );
    assert!(recovery_sources(&recovery).contains(&RecoverySource::OperationAbort));
    assert_eq!(f.read("conflict.txt"), b"main\n");
}

#[test]
fn interactive_rebase_edit_stop_allows_amend_but_other_operations_do_not() {
    let f = Fixture::new();
    f.write("file.txt", b"one\n");
    f.commit_all("one");
    f.write("file.txt", b"two\n");
    f.commit_all("two");
    f.write("file.txt", b"three\n");
    f.commit_all("three");

    let editor = f.temp.path().join("edit-sequence.sh");
    fs::write(&editor, "#!/bin/sh\nsed 's/^pick /edit /' \"$1\" > \"$1.revia-test\"\nmv \"$1.revia-test\" \"$1\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&editor, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let output = f.command(
        &["rebase", "--interactive", "--root"],
        &[
            ("GIT_SEQUENCE_EDITOR", revia_lib::platform::git_shell_argument(&editor).as_str()),
            ("GIT_EDITOR", "true"),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let session = f.session();
    let state = operation_flow::state(&session, &f.data()).unwrap().unwrap();
    assert_eq!(state.kind, OperationKind::Rebase);
    assert!(state.edit_stop);
    assert_eq!(state.conflict_count, 0);

    let drafts = DraftStore::new(f.data());
    let context = session.commit_context(&drafts).unwrap();
    assert!(!context.gates.operation_blocks_amend);
    assert!(!context.gates.operation_blocks_commit);
    assert!(context.gates.amend_available);

    // 用一个会被中止的 merge 验证普通进行中操作仍会挡住 amend。
    f.git(&["rebase", "--abort"]);
    f.branch("side");
    f.write("other.txt", b"side\n");
    f.commit_all("side");
    f.checkout("main");
    f.write("other.txt", b"main\n");
    f.commit_all("main");
    assert!(!f.try_git(&["merge", "side"]).status.success());
    let context = session.commit_context(&drafts).unwrap();
    assert!(context.gates.has_conflicts);
    assert!(context.gates.blocks_commit());
    assert!(context.gates.operation_blocks_amend);
    assert!(!context.gates.amend_available);
    f.git(&["merge", "--abort"]);
}

#[test]
fn cherry_pick_reword_and_reset_follow_native_git_semantics() {
    let f = Fixture::new();
    f.write("file.txt", b"base\n");
    f.commit_all("base");
    f.branch("feature");
    f.write("feature.txt", b"picked\n");
    let picked = f.commit_all("feature change");
    f.checkout("main");
    let session = f.session();
    let recovery = f.recovery();
    let archives = f.archives();
    let auto = f.autostash();

    let outcome = rewrite::cherry_pick(
        &session,
        &recovery,
        &auto,
        &archives,
        &CherryPickRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            expected_state: Some(rewrite::preflight(&session, &[picked.clone()]).unwrap().overwrite_fingerprint),
            oids: vec![picked],
            expected_head: f.head(),
            dirty_strategy: None,
            keep_empty: false,
            include_source: false,

        },
    )
    .unwrap();
    assert!(outcome.completed);
    assert!(outcome.error.is_none());
    assert_eq!(f.read("feature.txt"), b"picked\n");
    let pick_head = f.head();

    let reword = rewrite::reword(
        &session,
        &recovery,
        &auto,
        &archives,
        &f.data(),
        &pick_head,
        &pick_head,
        &revia_lib::write_guard::context(&session.git).unwrap(),
        &rewrite::preflight(&session, &[pick_head.clone()]).unwrap().overwrite_fingerprint,
        "reworded",
    )
    .unwrap();
    assert!(reword.completed);
    assert_eq!(
        String::from_utf8(f.git(&["show", "-s", "--format=%s", "HEAD"]))
            .unwrap()
            .trim(),
        "reworded"
    );

    let rewound = f.head();
    let outcome = rewrite::revert(
        &session,
        &recovery,
        &auto,
        &archives,
        &RevertRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            expected_state: Some(rewrite::preflight(&session, &[rewound.clone()]).unwrap().overwrite_fingerprint),
            oids: vec![rewound],
            expected_head: f.head(),
            dirty_strategy: None,

        },
    )
    .unwrap();
    assert!(outcome.completed);
    assert!(!f.repo.join("feature.txt").exists());

    // reset --soft 保留工作区与 index，reset --mixed 清空 index，reset --hard 覆盖工作区。
    f.write("file.txt", b"second\n");
    let second = f.commit_all("second");
    f.write("file.txt", b"second dirty\n");
    let recovery_id = rewrite::reset(
        &session,
        &recovery,
        &auto,
        &archives,
        &ResetRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            target: f.oid("HEAD^"),
            mode: ResetMode::Soft,
            expected_head: second.clone(),
            expected_state: Some(rewrite::preflight(&session, &[f.oid("HEAD^")]).unwrap().reset_fingerprint),
        },
    )
    .unwrap();
    assert!(recovery_id.completed);
    assert_eq!(f.head(), f.oid("HEAD"));
    assert_eq!(f.read("file.txt"), b"second dirty\n");
    assert!(!f.index_clean());

    let current = f.head();
    rewrite::reset(
        &session,
        &recovery,
        &auto,
        &archives,
        &ResetRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            target: current.clone(),
            mode: ResetMode::Mixed,
            expected_head: current.clone(),
            expected_state: Some(rewrite::preflight(&session, &[current.clone()]).unwrap().reset_fingerprint),
        },
    )
    .unwrap();
    assert!(f.index_clean());
    assert_eq!(f.read("file.txt"), b"second dirty\n");

    f.commit_all("second again");
    let hard_target = f.head();
    f.write("file.txt", b"changed again\n");
    rewrite::reset(
        &session,
        &recovery,
        &auto,
        &archives,
        &ResetRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            target: hard_target.clone(),
            mode: ResetMode::Hard,
            expected_head: f.head(),
            expected_state: Some(rewrite::preflight(&session, &[]).unwrap().reset_fingerprint),
        },
    )
    .unwrap();
    assert_eq!(f.read("file.txt"), b"second dirty\n");
    assert!(recovery_sources(&recovery).contains(&RecoverySource::HistoryReset));

    // reset --keep 在目标只改动其他文件时，保留无关工作区修改。
    let keep_target = f.head();
    let history_reset_before = recovery_sources(&recovery)
        .into_iter()
        .filter(|source| *source == RecoverySource::HistoryReset)
        .count();
    f.write("extra.txt", b"extra\n");
    let before_keep = f.commit_all("extra commit");
    f.write("file.txt", b"keep local edit\n");
    rewrite::reset(
        &session,
        &recovery,
        &auto,
        &archives,
        &ResetRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            target: keep_target.clone(),
            mode: ResetMode::Keep,
            expected_head: before_keep,
            expected_state: Some(rewrite::preflight(&session, &[keep_target.clone()]).unwrap().reset_fingerprint),
        },
    )
    .unwrap();
    assert_eq!(f.head(), keep_target);
    assert!(!f.repo.join("extra.txt").exists());
    assert_eq!(f.read("file.txt"), b"keep local edit\n");
    assert!(f.index_clean());
    let history_reset_after = recovery_sources(&recovery)
        .into_iter()
        .filter(|source| *source == RecoverySource::HistoryReset)
        .count();
    assert!(history_reset_after > history_reset_before);
}

#[test]
fn stash_drop_by_oid_targets_exact_entry_and_clear_rejects_stale_set() {
    let f = Fixture::new();
    f.write("a.txt", b"base a\n");
    f.write("b.txt", b"base b\n");
    f.commit_all("base");

    f.write("a.txt", b"stash a\n");
    f.git(&["stash", "push", "--quiet", "-m", "stash a"]);
    let oid_a = f.oid("refs/stash");
    f.write("b.txt", b"stash b\n");
    f.git(&["stash", "push", "--quiet", "-m", "stash b"]);
    let oid_b = f.oid("refs/stash");

    let session = f.session();
    let identity = f.identity(&session);
    let archives = f.archives();

    // 删除较旧的条目时，列表中还有后来追加的条目；必须按 OID 命中，
    // 不能因位置变化而删掉 stash@{0}。
    let dropped = stash::drop(&session, &archives, &oid_a).unwrap();
    assert!(dropped.deleted);
    let archive_id = dropped.archive_id.unwrap();
    let remaining = stash::list(&session.git).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].oid, oid_b);

    let restored = archives.restore(&session, &archive_id).unwrap();
    assert_eq!(restored, oid_a);
    let restored_list = stash::list(&session.git).unwrap();
    assert_eq!(restored_list.len(), 2);
    assert!(restored_list.iter().any(|entry| entry.oid == oid_a));
    assert!(restored_list.iter().any(|entry| entry.oid == oid_b));

    // 旧确认集合（缺少恢复出的条目）必须被拒绝，且不能删除任何条目。
    let error = stash::clear(&session, &archives, std::slice::from_ref(&oid_b)).unwrap_err();
    assert_eq!(error.code, Some("errors.stash.stashListChangedRefresh"));
    assert_eq!(stash::list(&session.git).unwrap().len(), 2);
    assert_eq!(archives.list(Some(&identity)).unwrap().len(), 1);
}

#[test]
fn cherry_pick_stashes_staged_changes_and_restores_them_after_success() {
    let f = Fixture::new();
    f.write("tracked.txt", b"base\n");
    f.write("notes.txt", b"base notes\n");
    f.commit_all("base");
    f.branch("feature");
    f.write("tracked.txt", b"feature\n");
    let source = f.commit_all("feature change");
    f.checkout("main");
    f.write("notes.txt", b"local staged notes\n");
    f.git(&["add", "notes.txt"]);

    let session = f.session();
    let recovery = f.recovery();
    let archives = f.archives();
    let auto = f.autostash();
    let outcome = rewrite::cherry_pick(
        &session,
        &recovery,
        &auto,
        &archives,
        &CherryPickRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            expected_state: Some(rewrite::preflight(&session, &[source.clone()]).unwrap().overwrite_fingerprint),
            oids: vec![source.clone()],
            expected_head: f.head(),
            dirty_strategy: Some(DirtyStrategy::Stash),
            keep_empty: false,
            include_source: false,

        },
    )
    .unwrap();
    assert!(outcome.completed);
    assert_eq!(outcome.applied, vec![source]);
    assert_eq!(f.read("tracked.txt"), b"feature\n");
    assert_eq!(f.read("notes.txt"), b"local staged notes\n");
    assert!(!f.index_clean());
    let staged = String::from_utf8(f.git(&["diff", "--cached", "--name-only"])).unwrap();
    assert_eq!(staged.trim(), "notes.txt");
    let autostash = outcome.autostash.unwrap();
    assert!(autostash.restored);
    assert!(!autostash.conflicted);
    assert!(autostash.removed);
    assert!(stash::list(&session.git).unwrap().is_empty());
    if let Ok(path) = std::env::var("REVIA_M6_ERROR_FIXTURE") {
        fs::write(format!("{path}.autostash-removed.json"), serde_json::to_vec_pretty(&autostash).unwrap()).unwrap();
    }
}

#[test]
fn stash_deletion_keeps_confirmation_through_archiving_and_transaction_boundaries() {
    use revia_lib::stash_transaction::StashDeletePhase;
    for count in [1, 2] {
        for phase in [StashDeletePhase::BeforeLock, StashDeletePhase::Locked, StashDeletePhase::BeforeCommit] {
            for clear in [false, true] {
                let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
                let session = f.session();
                for n in 0..count {
                    f.write("file", format!("saved {n}\n").as_bytes());
                    stash::save(&session, &StashSaveRequest { message: format!("item {n}"), keep_index: false,
                        expected_state: stash::preflight(&session, None).unwrap() }).unwrap();
                }
                let entries = stash::list(&session.git).unwrap(); let entry = &entries[0];
                let original_version = session.confirmation_version();
                let index = f.index(); let bytes = f.read("file"); let archives = f.archives();
                let mut refreshed = false;
                let mut hook = |at| {
                    if at == phase { session.refresh()?; refreshed = true; }
                    Ok(())
                };
                let failure = if clear {
                    let state = stash::collection_preflight(&session).unwrap();
                    stash::clear_confirmed_with_hook(&session, &archives,
                        &entries.iter().map(|entry| entry.oid.clone()).collect::<Vec<_>>(), &state, &mut hook).unwrap_err()
                } else {
                    let state = stash::preflight_selected(&session, Some(&entry.oid), Some(&entry.key)).unwrap();
                    stash::drop_selected_with_hook(&session, &archives, &entry.oid, &entry.key, &state, &mut hook).unwrap_err()
                };
                assert!(refreshed); assert_ne!(original_version, session.confirmation_version());
                assert_eq!(failure.kind, revia_lib::error::ErrorKind::Rejected);
                assert_eq!(stash::list(&session.git).unwrap(), entries);
                assert_eq!(f.index(), index); assert_eq!(f.read("file"), bytes);
                assert_eq!(archives.list(Some(&f.identity(&session))).unwrap().len(), if clear { count } else { 1 });
            }
        }
    }
}

#[test]
fn cherry_pick_empty_strategy_skips_or_creates_an_empty_commit() {
    let f = Fixture::new();
    f.write("file.txt", b"base\n");
    f.commit_all("base");
    f.branch("feature");
    f.write("extra.txt", b"same patch\n");
    let source = f.commit_all("same patch");
    f.checkout("main");
    f.write("extra.txt", b"same patch\n");
    let main_head = f.commit_all("already applied");

    let session = f.session();
    let recovery = f.recovery();
    let archives = f.archives();
    let auto = f.autostash();
    let skipped = rewrite::cherry_pick(
        &session,
        &recovery,
        &auto,
        &archives,
        &CherryPickRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            expected_state: Some(rewrite::preflight(&session, &[source.clone()]).unwrap().overwrite_fingerprint),
            oids: vec![source.clone()],
            expected_head: main_head.clone(),
            dirty_strategy: None,
            keep_empty: false,
            include_source: false,

        },
    )
    .unwrap();
    assert!(skipped.completed);
    assert_eq!(skipped.skipped_empty, vec![source.clone()]);
    assert!(skipped.applied.is_empty());
    assert_eq!(f.head(), main_head);

    let kept = rewrite::cherry_pick(
        &session,
        &recovery,
        &auto,
        &archives,
        &CherryPickRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            expected_state: Some(rewrite::preflight(&session, &[source.clone()]).unwrap().overwrite_fingerprint),
            oids: vec![source.clone()],
            expected_head: main_head.clone(),
            dirty_strategy: None,
            keep_empty: true,
            include_source: false,

        },
    )
    .unwrap();
    assert!(kept.completed);
    assert!(kept.skipped_empty.is_empty());
    assert_eq!(kept.applied, vec![source]);
    assert_ne!(f.head(), main_head);
    assert_eq!(f.oid("HEAD^"), main_head);
    assert!(
        f.git(&["diff", "--quiet", &format!("{main_head}..HEAD")])
            .is_empty()
    );
}

#[test]
fn cherry_pick_include_source_only_applies_to_local_remote_commits() {
    let f = Fixture::new();
    f.write("file.txt", b"base\n");
    let base = f.commit_all("base");
    f.branch("feature");
    f.write("feature.txt", b"feature\n");
    let source = f.commit_all("feature change");
    f.checkout("main");
    f.git(&[
        "update-ref",
        "refs/remotes/origin/feature",
        source.as_str(),
    ]);
    let session = f.session();
    let recovery = f.recovery();
    let archives = f.archives();
    let auto = f.autostash();
    let request = CherryPickRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            expected_state: Some(rewrite::preflight(&session, &[source.clone()]).unwrap().overwrite_fingerprint),
        oids: vec![source.clone()],
        expected_head: base.clone(),
        dirty_strategy: None,
        keep_empty: false,
        include_source: true,

    };
    let outcome = rewrite::cherry_pick(&session, &recovery, &auto, &archives, &request).unwrap();
    assert!(outcome.completed);
    let message = String::from_utf8(f.git(&["show", "-s", "--format=%B", "HEAD"])).unwrap();
    assert!(message.contains(&format!("(cherry picked from commit {source})")));

    // 引用变化使旧确认失效；重新确认后，即使 include_source 为真也不加来源后缀。
    f.git(&["reset", "--hard", "--quiet", base.as_str()]);
    f.git(&["update-ref", "-d", "refs/remotes/origin/feature"]);
    let request = CherryPickRequest {
        expected_head: base,
        ..request
    };
    assert_eq!(rewrite::cherry_pick(&session, &recovery, &auto, &archives, &request).unwrap_err().kind,
        revia_lib::error::ErrorKind::Rejected);
    let confirmed = rewrite::preflight(&session, &[source.clone()]).unwrap();
    assert!(!confirmed.published);
    let request = CherryPickRequest { expected_context: confirmed.context_version,
        expected_state: Some(confirmed.overwrite_fingerprint), ..request };
    let outcome = rewrite::cherry_pick(&session, &recovery, &auto, &archives, &request).unwrap();
    assert!(outcome.completed);
    let message = String::from_utf8(f.git(&["show", "-s", "--format=%B", "HEAD"])).unwrap();
    assert!(!message.contains("cherry picked from commit"));
}

#[test]
fn rebase_and_revert_reject_merge_commits_before_writing() {
    let f = Fixture::new();
    f.write("file.txt", b"base\n");
    f.commit_all("base");
    f.branch("side");
    f.write("side.txt", b"side\n");
    f.commit_all("side change");
    f.checkout("main");
    f.write("main.txt", b"main\n");
    f.commit_all("main change");
    f.git(&["merge", "--no-ff", "--quiet", "side", "-m", "merge side"]);
    let merge_oid = f.head();
    f.write("after.txt", b"after\n");
    let tip = f.commit_all("after merge");
    let upstream = f.oid(&format!("{merge_oid}^1"));
    let session = f.session();
    let recovery = f.recovery();
    let archives = f.archives();
    let auto = f.autostash();

    let request = InteractiveRebaseRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            expected_state: rebase_confirmation(&session, &upstream),
        upstream,
        expected_head: tip.clone(),
        entries: vec![
            RebasePlanEntry {
                oid: merge_oid.clone(),
                action: RebaseAction::Pick,
                message: None,
            },
            RebasePlanEntry {
                oid: tip.clone(),
                action: RebaseAction::Pick,
                message: None,
            },
        ],
    };
    let error = rewrite::interactive(&session, &recovery, &auto, &archives, &f.data(), &request)
        .unwrap_err();
    assert_eq!(error.code, Some("errors.rewrite.mergeCommitNotSupported"));

    let error = rewrite::revert(
        &session,
        &recovery,
        &auto,
        &archives,
        &RevertRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            expected_state: Some(rewrite::preflight(&session, &[merge_oid.clone()]).unwrap().overwrite_fingerprint),
            oids: vec![merge_oid],
            expected_head: tip.clone(),
            dirty_strategy: None,

        },
    )
    .unwrap_err();
    assert_eq!(error.code, Some("errors.rewrite.mergeCommitNotSupported"));
    assert_eq!(f.head(), tip);
}

#[test]
fn error_payload_from_real_git_stays_structured_for_english_presentation() {
    let mut f = Fixture::new();
    // Pin Git's own diagnostics so the raw detail parameter is deterministic;
    // production inherits the user's environment unchanged.
    f.vars.insert("LC_ALL".into(), "C".into());
    f.write("file.txt", b"base\n");
    f.commit_all("base");
    let session = f.session();
    let error = file_history::blame(
        &session.git,
        &BlameRequest {
            path: "file.txt".into(),
            revision: Some("0123456789abcdef0123456789abcdef01234567".into()),
            ignore_whitespace: false,
            detect_moves: false,
            detect_copies: false,
        },
    )
    .unwrap_err();
    let payload = error.payload();
    assert_eq!(
        payload.code.as_deref(),
        Some("errors.fileHistory.failedReadBlame")
    );
    let detail = payload
        .params
        .as_ref()
        .and_then(|params| params.get("detail"))
        .expect("the real Git stderr must be carried as a neutral parameter");
    assert!(detail.contains("fatal"), "unexpected Git detail: {detail}");
    if let Some(path) = std::env::var_os("REVIA_M6_ERROR_FIXTURE") {
        let related = revia_lib::error::AppError::coded(revia_lib::error::ErrorKind::NeedsVerification,
            "errors.commitFlow.gitDidStartButWritingSnapshot", "Git 未启动，但快照回执写入失败，记录待核对：{cause}", &[])
            .cause_payload(payload.clone()).related(&error);
        let nested = revia_lib::error::AppError::coded(revia_lib::error::ErrorKind::NeedsVerification,
            "errors.recovery.writeResultNeedsVerification", "操作可能已执行，结果待核对：{cause}", &[]).cause(error);
        f.write("docs/CON.txt", b"warning\r\n");
        f.git(&["add", "docs/CON.txt"]);
        f.git(&["checkout", "--detach", "--quiet"]);
        let warnings = revia_lib::commit_warnings::collect(&session.git,
            &revia_lib::commit_warnings::WarningOptions::default()).unwrap();
        assert!(warnings.len() >= 3, "expected actual staged CRLF, filename and detached warnings: {warnings:?}");
        let history_store = revia_lib::history::HistoryStore::new();
        let info = history_store.open(&session).unwrap();
        let log = history_store.target(&info.target_id).unwrap().log_page_filtered(
            Some(revia_lib::history::HistoryStart::Head), false, 10, None,
            &revia_lib::history::HistoryFilter { text: Some("[".into()), regex: true, ..Default::default() }).unwrap_err().payload();
        fs::write(&path, serde_json::to_vec_pretty(&serde_json::json!({"git": payload, "nested": nested.payload(), "related": related.payload(), "warnings": warnings, "log": log, "environment": session.info.checks})).unwrap()).unwrap();
    }
}

#[test]
fn file_history_stops_at_same_path_delete_recreate_boundary() {
    let f = Fixture::new();
    f.write("reused.txt", b"first life\n");
    let first = f.commit_all("first life");
    f.git(&["rm", "--quiet", "reused.txt"]);
    let deleted = f.commit_all("delete first life");
    f.write("reused.txt", b"second life\n");
    let second = f.commit_all("second life");

    let session = f.session();
    let page = file_history::history(
        &session.git,
        &FileHistoryRequest {
            path: "reused.txt".into(),
            all_branches: false,
            skip: 0,
            limit: 40,
            anchor: None,
        },
    )
    .unwrap();
    assert_eq!(
        page.entries.len(),
        1,
        "history must not stitch the previous file life"
    );
    assert_eq!(page.entries[0].oid, second);
    assert_ne!(page.entries[0].oid, deleted);
    assert_ne!(page.entries[0].oid, first);
    assert!(!page.has_more);

    // The log's old object and its deletion remain readable even after HEAD
    // reuses the name. Passing the selected OID must never select the new life.
    for anchor in [&first, &deleted] {
        let old = file_history::history(&session.git, &FileHistoryRequest {
            path: "reused.txt".into(), all_branches: false, skip: 0,
            limit: 1, anchor: Some(anchor.clone()),
        }).unwrap();
        assert_eq!(old.entries[0].oid, *anchor);
        assert!(!old.entries.iter().any(|entry| entry.oid == second));
        if old.has_more {
            let rest = file_history::history(&session.git, &FileHistoryRequest {
                path: "reused.txt".into(), all_branches: false, skip: old.next_skip,
                limit: 40, anchor: Some(old.anchor),
            }).unwrap();
            assert!(rest.entries.iter().any(|entry| entry.oid == first));
            assert!(!rest.entries.iter().any(|entry| entry.oid == second));
        }
    }
}

#[test]
fn file_history_keeps_control_characters_in_paths_and_commit_fields() {
    let f = Fixture::new();
    // Windows cannot store control characters in filenames; commit fields still exercise them.
    let paths = if cfg!(windows) {
        ["before after.txt".to_string(), format!("separator {}", "a".repeat(40)), "line name [tab].txt".to_string()]
    } else {
        ["before\u{1e}after.txt".to_string(), format!("\u{1e}{}", "a".repeat(40)), "line\nname\t.txt".to_string()]
    };
    f.git(&["config", "user.name", "Author\u{1e}Name"]);
    f.write("before", b"unrelated alias bytes\n");
    for path in &paths { f.write(path, b"real object\n"); }
    let root = f.commit_all("subject\u{1e}with separator");
    let renamed = if cfg!(windows) { "new name [line].txt" } else { "new\u{1e}name\n.txt" };
    f.git(&["mv", &paths[0], renamed]);
    let tip = f.commit_all("rename\u{1e}subject");
    let session = f.session();
    for path in [renamed, paths[1].as_str(), paths[2].as_str()] {
        let page = file_history::history(&session.git, &FileHistoryRequest {
            path: path.into(), all_branches: false, skip: 0, limit: 1, anchor: None,
        }).unwrap();
        let entry = &page.entries[0];
        assert_eq!(entry.path, path);
        assert_eq!(entry.author, "Author\u{1e}Name");
        assert!(entry.subject.contains('\u{1e}'));
        assert!(entry.patch_error.is_none());
        assert!(!entry.patch.is_empty());
        assert!(!entry.patch.contains("unrelated alias bytes"));
        if path == renamed {
            assert_eq!(entry.oid, tip);
            assert_eq!(entry.old_path.as_deref(), Some(paths[0].as_str()));
            assert!(page.has_more);
            let old = file_history::history(&session.git, &FileHistoryRequest {
                path: path.into(), all_branches: false, skip: page.next_skip,
                limit: 40, anchor: Some(page.anchor),
            }).unwrap();
            assert_eq!(old.entries[0].oid, root);
            assert_eq!(old.entries[0].path, paths[0]);
            assert!(old.entries[0].patch.contains("+real object"));
        }
    }
    assert_eq!(f.read("before"), b"unrelated alias bytes\n");
}

#[test]
fn all_branch_file_history_keeps_both_paths_when_commit_oids_match() {
    let f = Fixture::new();
    f.write("a.txt", b"content A\n"); f.write("b.txt", b"content B\n");
    let root = f.commit_all("both files");
    f.git(&["switch", "-c", "a"]); f.git(&["mv", "a.txt", "target.txt"]);
    f.commit_all("rename A"); f.git(&["switch", "main"]);
    f.git(&["switch", "-c", "b"]); f.git(&["mv", "b.txt", "target.txt"]);
    f.commit_all("rename B");
    let session = f.session();
    let page = file_history::history(&session.git, &FileHistoryRequest {
        path: "target.txt".into(), all_branches: true, skip: 0, limit: 40, anchor: None,
    }).unwrap();
    for (path, content) in [("a.txt", "+content A"), ("b.txt", "+content B")] {
        let entry = page.entries.iter().find(|entry| entry.oid == root && entry.path == path).unwrap();
        assert!(entry.patch.contains(content)); assert!(entry.patch_error.is_none());
    }
    let mut paged = Vec::new();
    let mut skip = 0;
    loop {
        let next = file_history::history(&session.git, &FileHistoryRequest {
            path: "target.txt".into(), all_branches: true, skip, limit: 1,
            anchor: Some(page.anchor.clone()),
        }).unwrap();
        paged.extend(next.entries.iter().map(|entry| (entry.oid.clone(), entry.path.clone())));
        assert!(paged.len() <= page.entries.len());
        if !next.has_more { break; }
        assert!(next.next_skip > skip); skip = next.next_skip;
    }
    assert_eq!(paged, page.entries.iter().map(|entry| (entry.oid.clone(), entry.path.clone())).collect::<Vec<_>>());
    if let Ok(path) = std::env::var("REVIA_M6_ERROR_FIXTURE") {
        fs::write(format!("{path}.history-collision.json"), serde_json::to_vec_pretty(&page).unwrap()).unwrap();
    }
}

#[test]
fn all_branch_file_history_pages_are_stable_with_nonmonotonic_commit_dates() {
    let f = Fixture::new();
    let path = "文件 space.txt";
    let commit_at = |message: &str, date: &str| {
        f.git(&["add", "--all"]);
        let output = f.command(&["commit", "--quiet", "-m", message],
            &[("GIT_AUTHOR_DATE", date), ("GIT_COMMITTER_DATE", date)]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr)); f.head()
    };
    f.write(path, b"base\n");
    let base = commit_at("base", "2025-01-01T00:00:00Z");
    f.git(&["branch", "other"]);
    let mut expected = std::collections::HashSet::from([base]);
    for n in 1..=45 {
        f.write(path, format!("main-{n}\n").as_bytes());
        // An ancestor is newer than every descendant; ties exercise the
        // deterministic head ordering too. Git's own stream remains fixed.
        let date = if n == 1 { "2030-01-01T00:00:00Z".into() }
            else { format!("2026-01-01T00:00:{:02}Z", n / 2) };
        expected.insert(commit_at(&format!("main-{n}"), &date));
    }
    f.checkout("other"); f.write(path, b"other\n");
    expected.insert(commit_at("other", "2025-01-02T00:00:00Z"));
    f.checkout("main");
    let session = f.session();
    let request = FileHistoryRequest { path: path.into(), all_branches: true, skip: 0, limit: 200, anchor: None };
    let original = file_history::history(&session.git, &request).unwrap();
    assert!(!original.has_more); assert_eq!(original.entries.len(), 47);
    assert_eq!(original.entries.iter().map(|entry| entry.oid.clone()).collect::<std::collections::HashSet<_>>(), expected);
    // Moving a ref must not change the pinned streams or their merge order.
    f.checkout("other"); f.write(path, b"late\n");
    let late = commit_at("late", "2031-01-01T00:00:00Z");
    let mut runs = Vec::new();
    for limit in [1, 7, 40] {
        let mut skip = 0; let mut pages = Vec::new(); let mut actual = Vec::new();
        loop {
            let page = file_history::history(&session.git, &FileHistoryRequest {
                skip, limit, anchor: Some(original.anchor.clone()), ..request.clone()
            }).unwrap();
            actual.extend(page.entries.iter().map(|entry| (entry.oid.clone(), entry.path.clone())));
            assert!(actual.len() <= expected.len(), "pagination repeated entries");
            let more = page.has_more; let next = page.next_skip; pages.push(page);
            if !more { break; }
            assert!(next > skip); skip = next;
        }
        assert_eq!(actual, original.entries.iter().map(|entry| (entry.oid.clone(), entry.path.clone())).collect::<Vec<_>>());
        assert!(!actual.iter().any(|entry| entry.0 == late));
        runs.push(pages);
    }
    let records = session.git.log.snapshot();
    for record in records.iter().filter(|record| record.args.iter().any(|arg| arg == "log")) {
        let count = record.args.iter().find_map(|arg| arg.strip_prefix("--max-count=")).unwrap().parse::<u32>().unwrap();
        assert!(count <= 201, "history read must stay bounded");
    }
    if let Ok(output) = std::env::var("REVIA_M6_ERROR_FIXTURE") {
        let fixture = f.temp.keep();
        fs::write(format!("{output}.history-pages.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "fixture": fixture, "original": original, "runs": runs, "git": records,
        })).unwrap()).unwrap();
    }
}

#[test]
fn file_history_follows_renames_across_all_branches_and_blame_reports_lines() {
    let f = Fixture::new();
    f.write("旧 文件.txt", b"one\ntwo\nthree\n");
    f.commit_all("add a");
    f.git(&["mv", "旧 文件.txt", "重命名 文件.txt"]);
    f.write("重命名 文件.txt", b"one\ntwo\nthree\nfour\n");
    let rename_commit = f.commit_all("rename a to b");

    f.branch("side");
    f.write("重命名 文件.txt", b"one\ntwo\nthree\nfour\nside\n");
    let side_commit = f.commit_all("side change");
    f.checkout("main");
    f.write("重命名 文件.txt", b"one\ntwo\nthree\nfour\nmain\n");
    let main_commit = f.commit_all("main change");

    let session = f.session();
    let page = file_history::history(
        &session.git,
        &FileHistoryRequest {
            path: "重命名 文件.txt".into(),
            all_branches: true,
            skip: 0,
            limit: 40,
            anchor: None,
        },
    )
    .unwrap();
    assert!(page.entries.iter().any(|entry| entry.oid == rename_commit));
    assert!(page.entries.iter().any(|entry| entry.oid == side_commit));
    assert!(page.entries.iter().any(|entry| entry.oid == main_commit));
    let rename_entry = page.entries.iter().find(|entry| entry.oid == rename_commit).unwrap();
    assert_eq!(rename_entry.path, "重命名 文件.txt");
    assert_eq!(rename_entry.old_path.as_deref(), Some("旧 文件.txt"));
    assert_eq!(rename_entry.status, "R");

    let previous = file_history::previous_revision(&session.git, &rename_commit, "重命名 文件.txt")
        .unwrap()
        .unwrap();
    assert_eq!(previous.path, "旧 文件.txt");
    assert_eq!(previous.oid, f.oid(&format!("{rename_commit}^")));

    let blame = file_history::blame(
        &session.git,
        &BlameRequest {
            path: "重命名 文件.txt".into(),
            revision: None,
            ignore_whitespace: true,
            detect_moves: true,
            detect_copies: false,
        },
    )
    .unwrap();
    assert!(blame.lines.iter().any(|line| line.content == "main"));
    assert!(blame.lines.iter().all(|line| !line.uncommitted));
}

#[test]
fn external_merge_tool_zero_exit_does_not_mean_resolved_and_choices_are_parsed() {
    let f = Fixture::new();
    f.write("conflict.txt", b"base\n");
    f.commit_all("base");
    f.branch("feature");
    f.write("conflict.txt", b"feature\n");
    f.commit_all("feature");
    f.checkout("main");
    f.write("conflict.txt", b"main\n");
    f.commit_all("main");
    assert!(!f.try_git(&["merge", "feature"]).status.success());
    f.git(&["config", "merge.tool", "fake"]);
    f.git(&["config", "mergetool.fake.cmd", "true"]);
    f.git(&["config", "mergetool.fake.trustExitCode", "true"]);

    let session = f.session();
    let recovery = f.recovery();
    let choices = external_tools::choices(&session.git, None, Some("fake"));
    assert_eq!(choices.configured_merge.as_deref(), Some("fake"));
    assert!(
        !choices.merge.is_empty(),
        "Git should report at least one mergetool"
    );

    let outcome = external_tools::launch(
        &session,
        &recovery,
        &ExternalToolRequest {
            expected_context: external_tools::confirmation_context(&session).unwrap(),
            kind: ExternalToolKind::Merge,
            path: "conflict.txt".into(),
            tool: Some("fake".into()),
            changes_kind: None,
            expected_fingerprint: Some(revia_lib::merge::fingerprint(&session.git, "conflict.txt").unwrap()),
            revision: None,
        },
        Some("fake"),
    )
    .unwrap();
    assert_eq!(outcome.exit_code, Some(0));
    assert!(outcome.still_conflicted);
    assert!(outcome.recovery_id.is_some());
    assert!(f.read("conflict.txt").starts_with(b"<<<<<<<"));
    assert!(recovery_sources(&recovery).contains(&RecoverySource::ExternalTool));
}

#[test]
fn external_diff_allows_historical_revision_without_current_worktree_change() {
    let f = Fixture::new();
    f.write("file.txt", b"old\n");
    let old = f.commit_all("old");
    f.write("file.txt", b"new\n");
    let new = f.commit_all("new");
    f.git(&["config", "diff.tool", "fake"]);
    f.git(&["config", "difftool.fake.cmd", "true"]);
    f.git(&["config", "difftool.fake.trustExitCode", "true"]);

    let session = f.session();
    let recovery = f.recovery();
    let outcome = external_tools::launch(
        &session,
        &recovery,
        &ExternalToolRequest {
            expected_context: external_tools::confirmation_context(&session).unwrap(),
            kind: ExternalToolKind::Diff,
            path: "file.txt".into(),
            tool: Some("fake".into()),
            changes_kind: None,
            expected_fingerprint: None,
            revision: Some(new.to_ascii_uppercase()),
        },
        Some("fake"),
    )
    .unwrap();
    assert_eq!(outcome.exit_code, Some(0));
    assert!(!outcome.still_conflicted);
    assert!(outcome.recovery_id.is_none());
    assert_eq!(f.read("file.txt"), b"new\n");
    assert_eq!(old.len(), 40);
    let commands = session.git.log.snapshot();
    let diff = commands.iter().find(|command| command.args.iter().any(|arg| arg == "difftool")).unwrap();
    assert!(diff.args.windows(2).any(|pair| pair[0] == old && pair[1] == new));
    let validation = commands.iter().find(|command| command.args.iter().any(|arg| arg == "diff-tree")).unwrap();
    assert!(validation.args.windows(2).any(|pair| pair[0] == old && pair[1] == new));
}

#[test]
fn external_diff_rejects_revision_options_and_non_commit_targets_before_validation() {
    let f = Fixture::new(); f.write("file", b"old\n"); let root = f.commit_all("root");
    f.write("file", b"new\n"); let head = f.commit_all("next");
    let blob = f.oid("HEAD:file"); let sentinel = f.temp.path().join("outside-sentinel");
    fs::write(&sentinel, b"must survive\n").unwrap();
    let session = f.session(); let index = f.index(); let recovery = f.recovery();
    for revision in [format!("--output={}", sentinel.display()), "HEAD".into(), blob, "0".repeat(40), root] {
        let before = session.git.log.snapshot().last().map_or(0, |command| command.sequence);
        let result = external_tools::launch(&session, &recovery, &ExternalToolRequest {
            expected_context: external_tools::confirmation_context(&session).unwrap(), kind: ExternalToolKind::Diff,
            path: "file".into(), tool: Some("unused".into()), revision: Some(revision.clone()),
            changes_kind: None, expected_fingerprint: None,
        }, None);
        assert!(result.is_err(), "invalid historical target: {revision}");
        let commands: Vec<_> = session.git.log.snapshot().into_iter().filter(|command| command.sequence > before).collect();
        assert!(commands.iter().all(|command| !command.args.iter().any(|arg| arg == "diff-tree" || arg == "difftool")));
        println!("revision={revision:?} result={result:?} commands={}", serde_json::to_string_pretty(&commands).unwrap());
        assert_eq!(fs::read(&sentinel).unwrap(), b"must survive\n");
        assert_eq!(f.read("file"), b"new\n"); assert_eq!(f.head(), head); assert_eq!(f.index(), index);
        assert!(recovery.list(Some(&f.identity(&session))).unwrap().is_empty());
    }
}

#[test]
fn stash_save_does_not_include_untracked_files_and_apply_restore_index_is_opt_in() {
    let f = Fixture::new();
    f.write("tracked.txt", b"base\n");
    f.commit_all("base");
    f.write("tracked.txt", b"staged\n");
    f.git(&["add", "tracked.txt"]);
    f.write("tracked.txt", b"worktree\n");
    f.write("new.txt", b"untracked\n");

    let session = f.session();
    let recovery = f.recovery();
    let archives = f.archives();
    let entry = stash::save(
        &session,
        &StashSaveRequest {
            expected_state: stash::preflight(&session, None).unwrap(),
            message: "only tracked".into(),
            keep_index: false,
        },
    )
    .unwrap();
    assert_eq!(f.read("new.txt"), b"untracked\n");
    let list = stash::list(&session.git).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].oid, entry.oid);

    let outcome = stash::apply(
        &session,
        &recovery,
        &archives,
        &StashApplyRequest { entry_key: None,
            expected_state: stash::preflight(&session, Some(&entry.oid.clone())).unwrap(),
            oid: entry.oid.clone(),
            restore_index: false,
            delete_after: false,
            new_branch: None,
        },
    )
    .unwrap();
    assert!(outcome.applied);
    assert!(!outcome.deleted);
    assert_eq!(f.read("tracked.txt"), b"worktree\n");
    assert_eq!(stash::list(&session.git).unwrap().len(), 1);
}

#[test]
fn stash_apply_new_branch_and_delete_after_archives_entry() {
    let f = Fixture::new();
    f.write("tracked.txt", b"base\n");
    let base = f.commit_all("base");
    f.write("tracked.txt", b"stashed\n");
    let session = f.session();
    let recovery = f.recovery();
    let archives = f.archives();
    let entry = stash::save(
        &session,
        &StashSaveRequest {
            expected_state: stash::preflight(&session, None).unwrap(),
            message: "branch me".into(),
            keep_index: false,
        },
    )
    .unwrap();

    let outcome = stash::apply(
        &session,
        &recovery,
        &archives,
        &StashApplyRequest { entry_key: None,
            expected_state: stash::preflight(&session, Some(&entry.oid.clone())).unwrap(),
            oid: entry.oid.clone(),
            restore_index: false,
            delete_after: true,
            new_branch: Some("stash-work".into()),
        },
    )
    .unwrap();
    assert!(outcome.applied);
    assert!(outcome.deleted);
    assert!(outcome.archive_id.is_some());
    assert_eq!(f.read("tracked.txt"), b"stashed\n");
    assert_eq!(
        String::from_utf8(f.git(&["branch", "--show-current"]))
            .unwrap()
            .trim(),
        "stash-work"
    );
    assert_eq!(f.head(), base);
    assert!(stash::list(&session.git).unwrap().is_empty());
    let identity = f.identity(&session);
    assert_eq!(archives.list(Some(&identity)).unwrap().len(), 1);
    assert!(outcome.recovery_id.is_some());
}


#[test]
fn autostash_conflict_keeps_link_and_stash_for_follow_up_resolution() {
    let f = Fixture::new();
    f.write("tracked.txt", b"base\n");
    f.write("other.txt", b"base\n");
    f.commit_all("base");
    f.write("tracked.txt", b"autostash\n");
    let session = f.session();
    let recovery = f.recovery();
    let archives = f.archives();
    let auto = f.autostash();
    auto.save_before(&session, &recovery, "rebase")
        .unwrap()
        .unwrap();

    // Advance HEAD with a competing edit, then keep an unrelated dirty file so
    // the restore has state to protect as well as a real merge conflict.
    f.write("tracked.txt", b"current\n");
    f.commit_all("current");
    f.write("other.txt", b"dirty\n");
    let outcome = auto
        .restore_if_ready(&session, &recovery, &archives)
        .unwrap()
        .unwrap();
    assert!(!outcome.restored);
    assert!(outcome.conflicted);
    assert!(!outcome.removed);
    assert!(outcome.recovery_id.is_some());
    assert!(f.read("tracked.txt").starts_with(b"<<<<<<<"));
    assert_eq!(f.read("other.txt"), b"dirty\n");
    assert_eq!(stash::list(&session.git).unwrap().len(), 1);
    assert!(auto.for_worktree(&session).unwrap().is_some());
    assert!(recovery_sources(&recovery).contains(&RecoverySource::HistoryAutostash));
}

#[test]
fn blame_options_keep_native_attribution_for_component_requests() {
    let f = Fixture::new(); let path = "中文 空格.txt";
    f.write(path, b"alpha\nbeta\n"); let base = f.commit_all("base");
    f.write(path, b"  alpha\nbeta\n"); let revision = f.commit_all("whitespace");
    let session = f.session(); let mut variants = Vec::new();
    for target in [Some(revision.clone()), Some(base.clone()), None] {
        for ignore_whitespace in [false, true] {
            for detect_moves in [false, true] {
                for detect_copies in [false, true] {
                    let request = BlameRequest { path: path.into(), revision: target.clone(),
                        ignore_whitespace, detect_moves, detect_copies };
                    let result = file_history::blame(&session.git, &request).unwrap();
                    let expected = if target.as_deref() == Some(base.as_str()) || ignore_whitespace { &base } else { &revision };
                    assert_eq!(&result.lines[0].oid, expected);
                    variants.push(serde_json::json!({ "request": request, "result": result }));
                }
            }
        }
    }
    let history = file_history::history(&session.git, &FileHistoryRequest {
        path: path.into(), all_branches: false, skip: 0, limit: 40, anchor: Some(revision.clone()),
    }).unwrap();
    let missing = file_history::blame(&session.git, &BlameRequest {
        path: "missing.txt".into(), revision: Some(revision.clone()),
        ignore_whitespace: true, detect_moves: false, detect_copies: false,
    }).unwrap_err().payload();
    let evidence = serde_json::json!({ "path": path, "base": base, "revision": revision,
        "history": history, "variants": variants, "missingError": missing,
        "commands": session.git.log.snapshot() });
    if let Ok(path) = std::env::var("REVIA_M6_ERROR_FIXTURE") {
        fs::write(format!("{path}.blame-options.json"), serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
    println!("blame options fixture={} evidence={evidence}", f.temp.path().display());
}

#[test]
fn file_history_paginates_and_handles_root_and_binary_patches() {
    let f = Fixture::new();
    f.write("paged.txt", b"one\n");
    let first = f.commit_all("one");
    f.write("paged.txt", b"one\ntwo\n");
    f.commit_all("two");
    f.write("paged.txt", b"one\ntwo\nthree\n");
    f.commit_all("three");
    f.write("paged.txt", b"one\ntwo\nthree\nfour\n");
    f.commit_all("four");
    f.write("blob.bin", &[0, 159, 146, 150, 0, 255, 10]);
    f.commit_all("binary");

    let session = f.session();
    let request = |skip| FileHistoryRequest {
        path: "paged.txt".into(),
        all_branches: false,
        skip,
        limit: 2,
            anchor: None,
    };
    let page1 = file_history::history(&session.git, &request(0)).unwrap();
    assert_eq!(page1.entries.len(), 2);
    assert!(page1.has_more);
    assert_eq!(page1.next_skip, 2);
    let page2 = file_history::history(&session.git, &request(page1.next_skip)).unwrap();
    assert_eq!(page2.entries.len(), 2);
    assert!(!page2.has_more);
    assert_eq!(page2.next_skip, 4);
    let ids: Vec<&str> = page1
        .entries
        .iter()
        .chain(page2.entries.iter())
        .map(|entry| entry.oid.as_str())
        .collect();
    assert_eq!(ids.len(), 4);
    assert!(ids.iter().all(|oid| !oid.is_empty()));
    assert!(ids.iter().any(|oid| *oid == first));

    // The root commit has no parent revision; this must be a normal end of
    // history rather than an error or a guessed parent.
    assert!(file_history::previous_revision(&session.git, &first, "paged.txt")
        .unwrap()
        .is_none());

    let binary = file_history::history(
        &session.git,
        &FileHistoryRequest {
            path: "blob.bin".into(),
            all_branches: false,
            skip: 0,
            limit: 10,
            anchor: None,
        },
    )
    .unwrap();
    assert_eq!(binary.entries.len(), 1);
    assert!(binary.entries[0].patch.to_lowercase().contains("binary"));
    assert!(binary.entries[0].patch.len() <= 2 * 1024 * 1024);
}

#[test]
fn interactive_rebase_plan_rejects_duplicate_or_missing_commits_before_running_git() {
    let f = Fixture::new();
    f.write("file.txt", b"one\n");
    f.commit_all("one");
    f.write("file.txt", b"two\n");
    let two = f.commit_all("two");
    f.write("file.txt", b"three\n");
    let three = f.commit_all("three");
    let session = f.session();
    let recovery = f.recovery();
    let archives = f.archives();
    let auto = f.autostash();

    let upstream = f.oid("HEAD~2");
    let duplicate = InteractiveRebaseRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            expected_state: rebase_confirmation(&session, &upstream.clone()),
        upstream: upstream.clone(),
        expected_head: f.head(),
        entries: vec![
            RebasePlanEntry {
                oid: two.clone(),
                action: RebaseAction::Pick,
                message: None,
            },
            RebasePlanEntry {
                oid: two.clone(),
                action: RebaseAction::Pick,
                message: None,
            },
            RebasePlanEntry {
                oid: three.clone(),
                action: RebaseAction::Pick,
                message: None,
            },
        ],
    };
    let error = rewrite::interactive(&session, &recovery, &auto, &archives, &f.data(), &duplicate)
        .unwrap_err();
    assert_eq!(
        error.code,
        Some("errors.rewrite.rebasePlanContainsDuplicate")
    );

    let missing = InteractiveRebaseRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            expected_state: rebase_confirmation(&session, &upstream),
        upstream,
        expected_head: f.head(),
        entries: vec![RebasePlanEntry {
            oid: three,
            action: RebaseAction::Pick,
            message: None,
        }],
    };
    let error = rewrite::interactive(&session, &recovery, &auto, &archives, &f.data(), &missing)
        .unwrap_err();
    assert_eq!(
        error.code,
        Some("errors.rewrite.rebasePlanMustContainEveryCommit")
    );
    assert_eq!(
        String::from_utf8(f.git(&["show", "-s", "--format=%s", "HEAD"]))
            .unwrap()
            .trim(),
        "three"
    );
}

#[test]
fn confirmed_hard_reset_rejects_later_file_or_index_changes_without_writing() {
    let f = Fixture::new();
    f.write("file.txt", b"base\n");
    f.commit_all("base");
    let session = f.session();
    for stage in [false, true] {
        f.write("file.txt", b"confirmed\n");
        let confirmed = rewrite::preflight(&session, &[]).unwrap();
        f.write("file.txt", b"later\n");
        if stage { f.git(&["add", "file.txt"]); }
        let index = f.index();
        let result = rewrite::reset(&session, &f.recovery(), &f.autostash(), &f.archives(), &ResetRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            target: f.head(), mode: ResetMode::Hard, expected_head: f.head(),
            expected_state: Some(confirmed.reset_fingerprint),
        });
        assert_eq!(result.unwrap_err().code, Some("errors.rewrite.stateChanged"));
        assert_eq!(f.read("file.txt"), b"later\n");
        assert_eq!(f.index(), index);
    }
}

#[test]
fn force_cherry_pick_treats_metacharacters_as_literal_paths_and_rejects_stale_confirmation() {
    let f = Fixture::new();
    let (literal_path, other_path) = if cfg!(windows) { ("a[bc]", "ab") } else { ("a*", "abc") };
    f.write(literal_path, b"base\n"); f.write(other_path, b"unrelated\n");
    let base = f.commit_all("base");
    f.branch("feature"); f.write(literal_path, b"feature\n"); let feature = f.commit_all("feature");
    f.checkout("main"); f.write(literal_path, b"local\n"); f.write(other_path, b"unrelated dirty\n");
    let session = f.session();
    let confirmed = rewrite::preflight(&session, &[feature.clone()]).unwrap();
    f.write(literal_path, b"later\n");
    let request = CherryPickRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(), oids: vec![feature.clone()], expected_head: base,
        dirty_strategy: Some(DirtyStrategy::Force), expected_state: Some(confirmed.overwrite_fingerprint),
        keep_empty: false, include_source: false };
    let index = f.index();
    let error = rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &request).unwrap_err();
    assert_eq!(error.code, Some("errors.rewrite.stateChanged"));
    assert_eq!(f.read(literal_path), b"later\n"); assert_eq!(f.index(), index);
    let confirmed = rewrite::preflight(&session, &[feature]).unwrap();
    let request = CherryPickRequest { expected_state: Some(confirmed.overwrite_fingerprint), ..request };
    let result = rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &request).unwrap();
    assert!(result.completed && result.error.is_none());
    assert_eq!(f.read(literal_path), b"feature\n");
    assert_eq!(f.read(other_path), b"unrelated dirty\n");
}

#[test]
fn protected_recovery_remains_occupied_until_write_and_receipt_finish() {
    use std::sync::Barrier;
    use revia_lib::recovery::{capture_files, context_fingerprint};
    let f = Fixture::new(); f.write("file", b"before\n"); f.commit_all("base");
    let session = f.session(); let store = f.recovery(); let paths = vec!["file".to_string()];
    let record = store.save_protected(f.identity(&session), RecoverySource::HistoryReset, "fixture",
        capture_files(&f.repo, &paths).unwrap(), context_fingerprint(&session.git, &paths).unwrap()).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let clearing = { let store = store.clone(); let barrier = barrier.clone(); std::thread::spawn(move || {
        barrier.wait(); store.clear().unwrap()
    })};
    barrier.wait(); assert_eq!(clearing.join().unwrap(), 0);
    f.write("file", b"after\n"); store.finish_snapshot(&record).unwrap();
    assert_eq!(store.list(None).unwrap().len(), 1);
    drop(record); assert_eq!(store.clear().unwrap(), 1);
}

#[test]
fn stash_archive_restores_after_entire_original_ancestry_is_pruned() {
    let f = Fixture::new(); f.write("file", b"one\n"); f.commit_all("one");
    f.write("file", b"two\n"); let base = f.commit_all("two");
    f.write("file", b"stash\n"); f.git(&["stash", "push", "-qm", "archived"]);
    let oid = f.oid("refs/stash"); let session = f.session(); let archives = f.archives();
    let record = archives.archive(&session.git, f.identity(&session), &oid, "archived", Some("main".into())).unwrap();
    f.git(&["stash", "clear"]); f.git(&["checkout", "--orphan", "survivor"]);
    f.git(&["rm", "-rf", "."]); f.write("other", b"independent\n"); f.commit_all("survivor");
    f.git(&["branch", "-D", "main"]); f.git(&["reflog", "expire", "--expire=now", "--all"]);
    f.git(&["gc", "--prune=now", "--quiet"]);
    assert!(!f.try_git(&["cat-file", "-e", &base]).status.success());
    let before = f.index(); let current = f.head();
    assert_eq!(archives.restore(&session, &record.id).unwrap(), oid);
    assert_eq!(f.index(), before); assert_eq!(f.head(), current);
    assert!(f.try_git(&["cat-file", "-e", &base]).status.success());
    assert_eq!(f.git(&["show", &format!("{oid}:file")]), b"stash\n");
}

#[test]
fn autostash_conflict_is_not_applied_again_after_restart_and_keep_only_clears_association() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    f.write("file", b"saved\n"); let session = f.session();
    let link = f.autostash().save_before(&session, &f.recovery(), "rebase").unwrap().unwrap();
    f.write("file", b"current\n"); f.commit_all("current");
    let result = f.autostash().restore_if_ready(&session, &f.recovery(), &f.archives()).unwrap().unwrap();
    assert!(result.conflicted);
    let bytes = f.read("file"); let index = f.index(); let stashes = f.git(&["reflog", "show", "refs/stash"]);
    let error = f.autostash().restore_if_ready(&session, &f.recovery(), &f.archives()).unwrap_err();
    assert_eq!(error.code, Some("errors.autostash.restoreAlreadyAttempted"));
    f.autostash().forget(&session, &link.id).unwrap();
    assert!(f.autostash().for_worktree(&session).unwrap().is_none());
    assert_eq!(f.read("file"), bytes); assert_eq!(f.index(), index);
    assert_eq!(f.git(&["reflog", "show", "refs/stash"]), stashes);
}

#[test]
fn autostash_does_not_recognize_a_recreated_repository_at_the_same_path() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    f.write("file", b"saved\n"); let session = f.session();
    f.autostash().save_before(&session, &f.recovery(), "rebase").unwrap().unwrap();
    fs::rename(&f.repo, f.temp.path().join("original-repo")).unwrap();
    fs::create_dir(&f.repo).unwrap(); f.git(&["init", "-b", "new"]);
    f.git(&["config", "user.name", "Fixture"]); f.git(&["config", "user.email", "fixture@example.invalid"]);
    f.write("file", b"replacement\n"); f.commit_all("replacement");
    let replacement = f.session();
    assert!(f.autostash().for_worktree(&replacement).unwrap().is_none());
    assert!(f.autostash().restore_if_ready(&replacement, &f.recovery(), &f.archives()).unwrap().is_none());
    assert_eq!(f.read("file"), b"replacement\n");
    assert_eq!(f.autostash().list().unwrap().len(), 1);
}

#[test]
fn all_branch_file_history_pages_keep_the_original_tips_when_refs_move() {
    let f = Fixture::new(); f.write("file", b"one\n"); f.commit_all("one");
    f.write("file", b"two\n"); f.commit_all("two"); f.branch("side");
    f.write("file", b"side\n"); f.commit_all("side"); f.checkout("main");
    let session = f.session();
    let request = FileHistoryRequest { path: "file".into(), all_branches: true, limit: 1, skip: 0, anchor: None };
    let original = file_history::history(&session.git, &FileHistoryRequest { limit: 100, ..request.clone() }).unwrap();
    let first = file_history::history(&session.git, &request).unwrap();
    f.checkout("side"); f.write("file", b"late\n"); let late = f.commit_all("late");
    let rest = file_history::history(&session.git, &FileHistoryRequest { skip: first.next_skip,
        limit: 100, anchor: Some(first.anchor.clone()), ..request }).unwrap();
    let oids: Vec<_> = first.entries.iter().chain(&rest.entries).map(|entry| &entry.oid).collect();
    assert_eq!(oids, original.entries.iter().map(|entry| &entry.oid).collect::<Vec<_>>());
    assert!(!oids.contains(&&late));
}

#[test]
fn rebase_pick_conflicts_do_not_consume_later_reword_messages_and_busy_plan_is_untouched() {
    let f = Fixture::new(); f.write("file", b"base\n"); let base = f.commit_all("base");
    f.write("file", b"a\n"); let a = f.commit_all("A");
    f.write("file", b"b\n"); let b = f.commit_all("B");
    f.write("other", b"c\n"); let c = f.commit_all("C");
    let session = f.session(); let data = f.temp.path().join("Application Support ' Revia");
    let recovery = RecoveryStore::new(data.clone()); let auto = AutostashStore::new(&data); let archives = StashArchiveStore::new(&data);
    let request = InteractiveRebaseRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            expected_state: rebase_confirmation(&session, &base), upstream: base, expected_head: f.head(), entries: vec![
        RebasePlanEntry { oid: b, action: RebaseAction::Pick, message: None },
        RebasePlanEntry { oid: c, action: RebaseAction::Reword, message: Some("C revised".into()) },
        RebasePlanEntry { oid: a, action: RebaseAction::Reword, message: Some("A revised".into()) },
    ]};
    let plan_dir = rewrite::rebase_state_dir(&data, &session); fs::create_dir_all(&plan_dir).unwrap();
    fs::write(plan_dir.join("sentinel"), b"keep").unwrap();
    let busy = session.acquire_write_lock().unwrap();
    assert!(rewrite::interactive(&session, &recovery, &auto, &archives, &data, &request).is_err());
    assert_eq!(fs::read(plan_dir.join("sentinel")).unwrap(), b"keep"); drop(busy);
    let result = rewrite::interactive(&session, &recovery, &auto, &archives, &data, &request).unwrap();
    assert!(!result.completed); assert!(result.state.unwrap().has_conflicts());
    f.write("file", b"b\n"); f.git(&["add", "file"]);
    let state = operation_flow::state(&session, &data).unwrap().unwrap();
    let stopped = operation_flow::execute(&session, &recovery, &auto, &archives, &OperationActionRequest {
            expected_state: state.confirmation_state.clone(),
        kind: state.kind, metadata_version: state.metadata_version, action: OperationAction::Continue,
    }).unwrap(); // A now conflicts after C's reword: a normal stop, not a failure.
    assert!(!stopped.completed); assert!(stopped.state.unwrap().has_conflicts());
    assert_eq!(f.git(&["show", "-s", "--format=%s", "HEAD"]), b"C revised\n");
    assert_eq!(f.git(&["show", "-s", "--format=%s", "HEAD^"]), b"B\n");
    f.write("file", b"a\n"); f.git(&["add", "file"]);
    let state = operation_flow::state(&session, &data).unwrap().unwrap();
    let result = operation_flow::execute(&session, &recovery, &auto, &archives, &OperationActionRequest {
            expected_state: state.confirmation_state.clone(),
        kind: state.kind, metadata_version: state.metadata_version, action: OperationAction::Continue,
    }).unwrap();
    assert!(result.completed); assert_eq!(f.git(&["show", "-s", "--format=%s", "HEAD"]), b"A revised\n");
}

#[test]
fn resolving_an_edit_command_conflict_does_not_enable_ordinary_commits() {
    let f = Fixture::new(); f.write("file", b"base\n"); let base = f.commit_all("base");
    f.write("file", b"a\n"); let a = f.commit_all("A");
    f.write("file", b"b\n"); let b = f.commit_all("B");
    let session = f.session();
    let result = rewrite::interactive(&session, &f.recovery(), &f.autostash(), &f.archives(), &f.data(), &InteractiveRebaseRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
            expected_state: rebase_confirmation(&session, &base),
        upstream: base, expected_head: f.head(), entries: vec![
            RebasePlanEntry { oid: b, action: RebaseAction::Edit, message: None },
            RebasePlanEntry { oid: a, action: RebaseAction::Pick, message: None },
        ],
    }).unwrap();
    assert!(result.state.unwrap().has_conflicts());
    f.write("file", b"resolved\n"); f.git(&["add", "file"]);
    let state = operation::detect(&session.git, 0).unwrap().unwrap(); assert!(!state.edit_stop);
    let context = session.commit_context(&DraftStore::new(f.data())).unwrap();
    assert!(context.gates.operation_blocks_commit); assert!(context.gates.operation_blocks_amend);
}

#[test]
fn persisted_autostash_restored_receipt_prevents_reapplying_after_restart() {
    use revia_lib::autostash::AutostashRestoration;
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    f.write("file", b"saved\n"); let session = f.session();
    let mut link = f.autostash().save_before(&session, &f.recovery(), "rebase").unwrap().unwrap();
    // A crash after the durable Restored receipt but before association removal.
    // Use real Git for the already-completed apply, then preserve that exact
    // production receipt shape in this fixture's private application directory.
    f.git(&["stash", "apply", "--index", "--quiet", &link.stash_oid]);
    link.restoration = AutostashRestoration::Restored;
    fs::write(f.data().join("autostash/links.json"), serde_json::to_vec(&vec![link]).unwrap()).unwrap();
    f.write("file", b"edited after restore\n"); let index = f.index();
    let error = f.autostash().restore_if_ready(&session, &f.recovery(), &f.archives()).unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
    assert_eq!(f.read("file"), b"edited after restore\n"); assert_eq!(f.index(), index);
    assert!(f.autostash().for_worktree(&session).unwrap().is_some());
    assert_eq!(stash::list(&session.git).unwrap().len(), 1);
}

#[test]
fn interrupted_autostash_save_intent_never_applies_an_unidentified_stash() {
    use revia_lib::autostash::AutostashRestoration;
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    f.write("file", b"saved\n"); let session = f.session();
    let mut link = f.autostash().save_before(&session, &f.recovery(), "rebase").unwrap().unwrap();
    link.restoration = AutostashRestoration::Saving; link.stash_oid.clear();
    fs::write(f.data().join("autostash/links.json"), serde_json::to_vec(&vec![link]).unwrap()).unwrap();
    let error = f.autostash().restore_if_ready(&session, &f.recovery(), &f.archives()).unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
    assert_eq!(error.code, Some("errors.autostash.restoreAlreadyAttempted"));
    assert_eq!(f.read("file"), b"base\n"); assert!(f.index_clean());
    assert_eq!(stash::list(&session.git).unwrap().len(), 1);
}

fn saved_stash(f: &Fixture, text: &[u8], message: &str) -> String {
    f.write("file", text);
    f.git(&["stash", "push", "--quiet", "-m", message]);
    f.oid("refs/stash")
}

#[test]
fn stash_transaction_selects_after_lock_and_preserves_reflog_metadata_for_middle_top_and_last() {
    use revia_lib::stash_transaction::StashDeletePhase;
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    let old = saved_stash(&f, b"old\n", "old");
    let middle = saved_stash(&f, b"middle\n", "middle");
    let top = saved_stash(&f, b"top\n", "top");
    let session = f.session();
    let metadata = |oid: &str| {
        let raw = String::from_utf8(f.git(&["reflog", "show", "--format=%H%x00%gs%x00%gN%x00%gE%x00%gD", "--date=raw", "refs/stash"])).unwrap();
        raw.lines().find(|line| line.starts_with(oid)).unwrap().to_string()
    };
    let old_meta = metadata(&old); let top_meta = metadata(&top);
    let mut late = None;
    let result = stash::drop_with_hook(&session, &f.archives(), &middle, |phase| {
        if phase == StashDeletePhase::BeforeLock {
            late = Some(saved_stash(&f, b"late\n", "late"));
        }
        Ok(())
    }).unwrap_err();
    assert_eq!(result.code, Some("errors.stashTransaction.changed"));
    assert_eq!(stash::list(&session.git).unwrap().len(), 4);
    let result = stash::drop(&session, &f.archives(), &middle).unwrap();
    assert!(result.deleted); assert_eq!(metadata(&old), old_meta); assert_eq!(metadata(&top), top_meta);
    let late = late.unwrap();
    assert_eq!(stash::list(&session.git).unwrap().iter().map(|entry| &entry.oid).collect::<Vec<_>>(), vec![&late, &top, &old]);
    stash::drop(&session, &f.archives(), &late).unwrap();
    assert_eq!(f.oid("refs/stash"), top);
    stash::drop(&session, &f.archives(), &top).unwrap();
    assert_eq!(f.oid("refs/stash"), old);
    stash::drop(&session, &f.archives(), &old).unwrap();
    assert!(!f.try_git(&["rev-parse", "--verify", "refs/stash"]).status.success());
    assert!(stash::list(&session.git).unwrap().is_empty());
    // A new entry after deleting the last one must not inherit old log entries.
    let fresh = saved_stash(&f, b"fresh\n", "fresh");
    assert_eq!(stash::list(&session.git).unwrap().iter().map(|entry| &entry.oid).collect::<Vec<_>>(), vec![&fresh]);
}

#[test]
fn stash_clear_transaction_rejects_new_entries_even_when_the_head_oid_returns_to_the_confirmed_value() {
    use revia_lib::stash_transaction::StashDeletePhase;
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    let first = saved_stash(&f, b"first\n", "first");
    let head = saved_stash(&f, b"head\n", "head");
    let session = f.session(); let expected = vec![head.clone(), first];
    let mut added = None;
    let error = stash::clear_with_hook(&session, &f.archives(), &expected, |phase| {
        if phase == StashDeletePhase::BeforeLock {
            added = Some(saved_stash(&f, b"unconfirmed\n", "unconfirmed"));
            f.git(&["stash", "store", "-m", "head restored externally", &head]);
        }
        Ok(())
    }).unwrap_err();
    assert_eq!(error.code, Some("errors.stashTransaction.changed"));
    assert_eq!(f.oid("refs/stash"), head);
    assert!(stash::list(&session.git).unwrap().iter().any(|entry| entry.oid == added.clone().unwrap()));
    let current = stash::list(&session.git).unwrap();
    let oids = current.iter().map(|entry| entry.oid.clone()).collect::<Vec<_>>();
    assert_eq!(stash::clear(&session, &f.archives(), &oids).unwrap().deleted, 4);
    assert!(stash::list(&session.git).unwrap().is_empty());
    assert!(!f.try_git(&["show-ref", "--verify", "refs/stash"]).status.success());
}

#[test]
fn stash_transaction_holds_the_real_shared_ref_lock_against_an_external_linked_worktree() {
    use std::sync::Barrier;
    use revia_lib::stash_transaction::StashDeletePhase;
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    let old = saved_stash(&f, b"old\n", "old");
    let top = saved_stash(&f, b"top\n", "top");
    let linked = f.temp.path().join("linked"); f.git(&["worktree", "add", "-b", "linked", linked.to_str().unwrap()]);
    let session = f.session_at(&linked);
    let barrier = Arc::new(Barrier::new(2));
    std::thread::scope(|scope| {
        let gate = barrier.clone(); let fixture = &f; let top_oid = &top;
        let worker = scope.spawn(move || {
            gate.wait();
            let run = |args: &[&str]| Command::new("git").current_dir(&fixture.repo)
                .env_clear().envs(&fixture.vars).args(args).output().unwrap();
            let append = run(&["-c", "core.filesRefLockTimeout=0", "stash", "store", "-m", "external", top_oid]);
            let drop = run(&["-c", "core.filesRefLockTimeout=0", "stash", "drop", "stash@{0}"]);
            gate.wait(); (append, drop)
        });
        let outcome = stash::drop_with_hook(&session, &f.archives(), &old, |phase| {
            if phase == StashDeletePhase::Locked { barrier.wait(); barrier.wait(); }
            Ok(())
        }).unwrap();
        assert!(outcome.deleted);
        let (append, drop) = worker.join().unwrap();
        assert!(!append.status.success()); assert!(!drop.status.success());
        assert!(String::from_utf8_lossy(&append.stderr).contains("lock"));
    });
    assert_eq!(stash::list(&session.git).unwrap().iter().map(|entry| &entry.oid).collect::<Vec<_>>(), vec![&top]);
    assert!(session.git.log.snapshot().iter().any(|record| record.program.starts_with("Revia stash transaction ") && record.args[0] == "stash-drop-transaction"));
}

#[test]
fn stash_transaction_preserves_foreign_locks_and_rejects_duplicate_oid_ambiguity() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    let oid = saved_stash(&f, b"stash\n", "stash"); let session = f.session();
    let before = f.git(&["reflog", "show", "refs/stash"]);
    let lock = f.repo.join(".git/refs/stash.lock"); fs::write(&lock, b"owned by another process\n").unwrap();
    let error = stash::drop(&session, &f.archives(), &oid).unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::Rejected);
    assert_eq!(fs::read(&lock).unwrap(), b"owned by another process\n");
    assert_eq!(f.git(&["reflog", "show", "refs/stash"]), before);
    fs::remove_file(&lock).unwrap(); // This fixture created the lock above.
    // Updating a ref to its current value need not append a reflog event.
    // Move to a distinct real stash first, then append the original OID.
    saved_stash(&f, b"another\n", "another");
    f.git(&["stash", "store", "-m", "duplicate", &oid]);
    assert_eq!(stash::list(&session.git).unwrap().len(), 3);
    let before = f.git(&["reflog", "show", "refs/stash"]);
    assert_eq!(stash::drop(&session, &f.archives(), &oid).unwrap_err().code, Some("errors.stashTransaction.changed"));
    assert_eq!(f.git(&["reflog", "show", "refs/stash"]), before);
}

#[test]
fn stash_transaction_interruption_and_commit_failure_keep_archive_and_do_not_retry() {
    use revia_lib::stash_transaction::StashDeletePhase;
    use revia_lib::error::{AppError, ErrorKind};
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    let old = saved_stash(&f, b"old\n", "old");
    let top = saved_stash(&f, b"top\n", "top"); let session = f.session();
    let before = f.git(&["reflog", "show", "refs/stash"]);
    let error = stash::drop_with_hook(&session, &f.archives(), &old, |phase| {
        if phase == StashDeletePhase::BeforeCommit { return Err(AppError::rejected("fixture interrupted before commit")); }
        Ok(())
    }).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected); assert_eq!(f.git(&["reflog", "show", "refs/stash"]), before);
    let log_lock = f.repo.join(".git/logs/refs/stash.lock"); fs::write(&log_lock, b"foreign log lock").unwrap();
    assert_eq!(stash::drop(&session, &f.archives(), &old).unwrap_err().kind, ErrorKind::NeedsVerification);
    assert_eq!(fs::read(&log_lock).unwrap(), b"foreign log lock");
    assert_eq!(f.git(&["reflog", "show", "refs/stash"]), before);
    fs::remove_file(&log_lock).unwrap();
    let error = stash::drop_with_hook(&session, &f.archives(), &old, |phase| {
        if phase == StashDeletePhase::AfterCommit { return Err(AppError::environment("fixture lost completion receipt")); }
        Ok(())
    }).unwrap_err();
    assert_eq!(error.kind, ErrorKind::NeedsVerification);
    assert_eq!(stash::list(&session.git).unwrap().iter().map(|entry| &entry.oid).collect::<Vec<_>>(), vec![&top]);
    let record = f.archives().list(Some(&f.identity(&session))).unwrap().into_iter().find(|record| record.stash_oid == old).unwrap();
    f.archives().restore(&session, &record.id).unwrap();
    assert!(stash::list(&session.git).unwrap().iter().any(|entry| entry.oid == old));
}

#[test]
fn stash_partial_backend_commit_is_needs_verification_and_archive_still_restores() {
    use revia_lib::stash_transaction::StashDeletePhase;
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    let old = saved_stash(&f, b"old\n", "old");
    let top = saved_stash(&f, b"top\n", "top"); let session = f.session();
    let reference = f.repo.join(".git/refs/stash");
    let original = f.temp.path().join("original-ref-for-fault-injection");
    let error = stash::drop_with_hook(&session, &f.archives(), &old, |phase| {
        if phase == StashDeletePhase::BeforeCommit {
            // Fault injection only in this fixture: make the final ref rename
            // fail after libgit2 has really committed the modified reflog.
            fs::rename(&reference, &original).unwrap();
            fs::create_dir(&reference).unwrap();
        }
        Ok(())
    }).unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
    fs::remove_dir(&reference).unwrap(); fs::rename(&original, &reference).unwrap();
    assert_eq!(f.oid("refs/stash"), top);
    assert!(!stash::list(&session.git).unwrap().iter().any(|entry| entry.oid == old));
    let record = f.archives().list(Some(&f.identity(&session))).unwrap().into_iter().find(|record| record.stash_oid == old).unwrap();
    f.archives().restore(&session, &record.id).unwrap();
    assert!(stash::list(&session.git).unwrap().iter().any(|entry| entry.oid == old));
}

#[test]
fn stash_transaction_deletes_a_packed_stash_without_rewriting_other_refs() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    let oid = saved_stash(&f, b"stash\n", "stash");
    let middle = saved_stash(&f, b"middle\n", "middle");
    let top = saved_stash(&f, b"top\n", "top");
    f.git(&["tag", "preserved"]); f.git(&["pack-refs", "--all"]);
    let session = f.session();
    let packed = fs::read(f.repo.join(".git/packed-refs")).unwrap();
    assert!(String::from_utf8_lossy(&packed).contains("refs/stash"));
    let head = f.head();
    assert!(stash::drop(&session, &f.archives(), &middle).unwrap().deleted);
    assert_eq!(f.oid("refs/stash"), top); assert_eq!(fs::read(f.repo.join(".git/packed-refs")).unwrap(), packed);
    assert!(stash::drop(&session, &f.archives(), &top).unwrap().deleted);
    assert_eq!(f.oid("refs/stash"), oid); assert_eq!(fs::read(f.repo.join(".git/packed-refs")).unwrap(), packed);
    f.git(&["pack-refs", "--all"]);
    assert!(stash::drop(&session, &f.archives(), &oid).unwrap().deleted);
    assert_eq!(f.head(), head); assert_eq!(f.oid("refs/tags/preserved"), head);
    assert!(stash::list(&session.git).unwrap().is_empty());
    assert!(!f.try_git(&["rev-parse", "--verify", "refs/stash"]).status.success());
}

#[test]
fn stash_clear_holds_core_git_locks_and_preserves_concurrent_other_refs() {
    use revia_lib::stash_transaction::StashDeletePhase;
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    let old = saved_stash(&f, b"old\n", "old");
    let top = saved_stash(&f, b"top\n", "top");
    f.git(&["pack-refs", "--all"]);
    let linked = f.temp.path().join("linked"); f.git(&["worktree", "add", "-b", "linked", linked.to_str().unwrap()]);
    let session = f.session_at(&linked); let head = f.head();
    let outcome = stash::clear_with_hook(&session, &f.archives(), &[top.clone(), old], |phase| {
        if phase == StashDeletePhase::Locked {
            let store = f.try_git(&["-c", "core.filesRefLockTimeout=0", "stash", "store", "-m", "concurrent", &top]);
            let drop = f.try_git(&["-c", "core.filesRefLockTimeout=0", "stash", "drop", "stash@{0}"]);
            let packed = f.try_git(&["-c", "core.packedRefsTimeout=0", "pack-refs", "--all"]);
            assert!(!store.status.success()); assert!(!drop.status.success()); assert!(!packed.status.success());
            f.git(&["update-ref", "refs/heads/parallel", &head]);
        }
        Ok(())
    }).unwrap();
    assert_eq!(outcome.deleted, 2); assert_eq!(f.oid("refs/heads/parallel"), head);
    assert_eq!(f.head(), head); assert!(stash::list(&session.git).unwrap().is_empty());
    assert!(session.git.log.snapshot().iter().any(|record| record.args == ["update-ref", "--no-deref", "--stdin"]));
}

#[cfg(unix)]
#[test]
fn stash_core_transaction_rejects_hook_spoofing_and_preserves_archive_on_rejection() {
    use std::os::unix::fs::PermissionsExt;
    use revia_lib::stash_transaction::StashDeletePhase;
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    let oid = saved_stash(&f, b"stash\n", "stash"); let session = f.session();
    let hook = f.repo.join(".git/hooks/reference-transaction");
    fs::write(&hook, b"#!/bin/sh\nif [ \"$1\" = prepared ]; then printf 'prepare: ok\\ncommit: ok\\n'; exit 1; fi\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let before = f.git(&["reflog", "show", "refs/stash"]); let mut reached_commit = false;
    let error = stash::drop_with_hook(&session, &f.archives(), &oid, |phase| {
        reached_commit |= phase == StashDeletePhase::BeforeCommit; Ok(())
    }).unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::Rejected); assert!(!reached_commit);
    assert_eq!(f.oid("refs/stash"), oid); assert_eq!(f.git(&["reflog", "show", "refs/stash"]), before);
    assert_eq!(f.archives().list(Some(&f.identity(&session))).unwrap().len(), 1);
    // Harmless hook output is retained in the log without corrupting protocol.
    fs::write(&hook, b"#!/bin/sh\nprintf 'prepare: ok\\ncommit: ok\\n'\n").unwrap();
    assert!(stash::drop(&session, &f.archives(), &oid).unwrap().deleted);
    assert!(stash::list(&session.git).unwrap().is_empty());
}

#[test]
fn stash_last_entry_abort_and_lost_receipt_do_not_repeat_the_write() {
    use revia_lib::stash_transaction::StashDeletePhase;
    use revia_lib::error::{AppError, ErrorKind};
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    let oid = saved_stash(&f, b"stash\n", "stash"); let session = f.session();
    let before = f.git(&["reflog", "show", "refs/stash"]);
    let error = stash::drop_with_hook(&session, &f.archives(), &oid, |phase| {
        if phase == StashDeletePhase::BeforeCommit { return Err(AppError::rejected("fixture interrupted")); } Ok(())
    }).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected); assert_eq!(f.git(&["reflog", "show", "refs/stash"]), before);
    let error = stash::drop_with_hook(&session, &f.archives(), &oid, |phase| {
        if phase == StashDeletePhase::AfterCommit { return Err(AppError::environment("fixture lost receipt")); } Ok(())
    }).unwrap_err();
    assert_eq!(error.kind, ErrorKind::NeedsVerification); assert!(stash::list(&session.git).unwrap().is_empty());
    let record = f.archives().list(Some(&f.identity(&session))).unwrap().into_iter().next().unwrap();
    f.archives().restore(&session, &record.id).unwrap();
    assert_eq!(stash::list(&session.git).unwrap().iter().map(|entry| &entry.oid).collect::<Vec<_>>(), vec![&oid]);
}

#[test]
fn stash_core_transaction_reports_partial_reflog_unlink_failure_as_uncertain() {
    use revia_lib::stash_transaction::StashDeletePhase;
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    let oid = saved_stash(&f, b"stash\n", "stash"); let session = f.session();
    let log = f.repo.join(".git/logs/refs/stash"); let original = f.temp.path().join("original-log-for-fault-injection");
    let error = stash::drop_with_hook(&session, &f.archives(), &oid, |phase| {
        if phase == StashDeletePhase::BeforeCommit {
            // Force Git's actual unlink warning after validation, only here.
            fs::rename(&log, &original).unwrap(); fs::create_dir(&log).unwrap();
            fs::write(log.join("fixture-sentinel"), b"keep").unwrap();
        }
        Ok(())
    }).unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
    assert!(!f.try_git(&["show-ref", "--verify", "refs/stash"]).status.success());
    assert_eq!(fs::read(log.join("fixture-sentinel")).unwrap(), b"keep");
    // Repair only the paths created by this fixture, then restore the archive.
    fs::remove_file(log.join("fixture-sentinel")).unwrap(); fs::remove_dir(&log).unwrap();
    let record = f.archives().list(Some(&f.identity(&session))).unwrap().into_iter().next().unwrap();
    f.archives().restore(&session, &record.id).unwrap();
    assert_eq!(stash::list(&session.git).unwrap().iter().map(|entry| &entry.oid).collect::<Vec<_>>(), vec![&oid]);
}

#[test]
fn review_hard_reset_must_protect_ignored_path_reintroduced_by_target() {
    let f = Fixture::new();
    f.write("generated.txt", b"tracked old output\n");
    let target = f.commit_all("track generated file");
    f.git(&["rm", "generated.txt"]);
    f.write(".gitignore", b"generated.txt\n");
    f.commit_all("ignore generated output after removing it");
    f.write("generated.txt", b"unique local ignored output\n");
    let session = f.session();
    let confirmed = rewrite::preflight(&session, &[target.clone()]).unwrap();
    let outcome = rewrite::reset(&session, &f.recovery(), &f.autostash(), &f.archives(), &ResetRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
        target, mode: ResetMode::Hard, expected_head: f.head(), expected_state: Some(confirmed.reset_fingerprint),
    });
    let bytes = f.read("generated.txt");
    let records = f.recovery().list(None).unwrap();
    println!("fixture={} outcome={outcome:?} after={bytes:?} recovery={records:?}", f.temp.path().display());
    assert!(outcome.is_ok(), "the ordinary ignored file can be protected: {outcome:?}");
    assert_eq!(bytes, b"tracked old output\n");
    let saved = records.iter().map(|record| f.recovery().read(&record.id).unwrap().0)
        .flat_map(|record| record.before).find(|file| file.path == "generated.txt").unwrap();
    assert_eq!(saved.content.as_deref(), Some(b"unique local ignored output\n".as_slice()));
}

#[test]
fn mixed_reset_rejects_an_index_changed_after_confirmation() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    f.write("file", b"confirmed\n"); f.git(&["add", "file"]);
    let session = f.session(); let target = f.head();
    let confirmed = rewrite::preflight(&session, &[target.clone()]).unwrap();
    f.write("file", b"new index\n"); f.git(&["add", "file"]); let index = f.index();
    let error = rewrite::reset(&session, &f.recovery(), &f.autostash(), &f.archives(), &ResetRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
        target: target.clone(), mode: ResetMode::Mixed, expected_head: target, expected_state: Some(confirmed.reset_fingerprint),
    }).unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::Rejected); assert_eq!(f.index(), index);
    assert_eq!(f.read("file"), b"new index\n");
}

#[test]
fn stash_inspection_preserves_unicode_tabs_and_newlines_in_git_paths() {
    let f = Fixture::new();
    let paths = if cfg!(windows) {
        ["说明 文件.txt", "tab file", "line file [brackets]"]
    } else {
        ["说明 文件.txt", "tab\tfile", "line\nfile"]
    };
    for path in paths { f.write(path, b"old\n"); } f.commit_all("base");
    for path in paths { f.write(path, b"new\n"); }
    f.git(&["stash", "push", "--quiet"]); let session = f.session();
    let view = stash::inspect(&session.git, &f.oid("refs/stash"), false).unwrap();
    assert_eq!(view.files.len(), 3);
    for path in paths {
        let file = view.files.iter().find(|file| file.path == path).unwrap();
        assert_eq!((file.added, file.deleted, file.binary), (1, 1, false));
    }
}

#[test]
fn external_tools_prefer_repository_config_and_reject_changed_merge_content() {
    let f = Fixture::new(); f.write("conflict", b"base\n"); f.commit_all("base");
    f.branch("feature"); f.write("conflict", b"feature\n"); f.commit_all("feature");
    f.checkout("main"); f.write("conflict", b"main\n"); f.commit_all("main");
    assert!(!f.try_git(&["merge", "feature"]).status.success());
    f.git(&["config", "merge.tool", "repository-tool"]); f.git(&["config", "diff.tool", "repository-diff"]);
    let session = f.session(); let choices = external_tools::choices(&session.git, Some("global-diff"), Some("global-merge"));
    assert_eq!(choices.configured_merge.as_deref(), Some("repository-tool"));
    assert_eq!(choices.configured_diff.as_deref(), Some("repository-diff"));
    let fingerprint = revia_lib::merge::fingerprint(&session.git, "conflict").unwrap();
    f.write("conflict", b"changed while confirmation was open\n"); let index = f.index();
    let error = external_tools::launch(&session, &f.recovery(), &ExternalToolRequest {
            expected_context: external_tools::confirmation_context(&session).unwrap(),
        kind: ExternalToolKind::Merge, path: "conflict".into(), tool: Some("repository-tool".into()),
        changes_kind: None, revision: None, expected_fingerprint: Some(fingerprint),
    }, Some("global-merge")).unwrap_err();
    assert_eq!(error.code, Some("errors.externalTools.staleTarget")); assert_eq!(f.index(), index);
    assert_eq!(f.read("conflict"), b"changed while confirmation was open\n"); assert!(f.recovery().list(None).unwrap().is_empty());
}

#[test]
fn rewrite_range_uses_the_exact_head_ancestry_and_rejects_another_branch() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    f.branch("side"); f.write("side", b"side\n"); let side = f.commit_all("side"); f.checkout("main");
    f.write("file", b"first\n"); let first = f.commit_all("visible filtered row");
    f.write("file", b"second\n"); let second = f.commit_all("hidden by filter");
    f.write("file", b"third\n"); let third = f.commit_all("visible newest");
    let session = f.session(); let range = rewrite::range(&session, &first, &third).unwrap();
    assert_eq!(range.iter().map(|entry| entry.oid.as_str()).collect::<Vec<_>>(), vec![third.as_str(), second.as_str(), first.as_str()]);
    assert_eq!(rewrite::range(&session, &side, &third).unwrap_err().code, Some("errors.rewrite.targetNotAncestor"));
}

#[cfg(unix)]
#[test]
fn real_git_stash_timeout_is_uncertain_and_does_not_wait_on_a_hook_writer() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); f.write("file", b"save me\n");
    let hook = f.repo.join(".git/hooks/reference-transaction");
    fs::write(&hook, b"#!/bin/sh\nsleep 0.3\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let session = f.session();
    let error = stash::save_with_timeout(&session, &StashSaveRequest {
            expected_state: stash::preflight(&session, None).unwrap(), message: "timeout".into(), keep_index: false }, Duration::from_millis(40)).unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
    // The command deadline excludes confirmation reads and unrelated test
    // threads waiting for the stash mutex. Include pipe drain in Git's receipt.
    let record = session.git.log.snapshot().into_iter().find(|record| record.timed_out).unwrap();
    assert!(record.elapsed_ms < 2000, "actual timed-out Git command: {record:?}");
    std::thread::sleep(Duration::from_millis(400)); // Only this fixture's hook is still exiting.
}

#[cfg(unix)]
#[test]
fn real_external_tool_timeout_is_uncertain_and_preserves_recovery() {
    use std::time::{Duration, Instant};
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); f.write("file", b"local\n");
    f.git(&["config", "difftool.slow.cmd", "printf 'fixture tool started\\n'; sleep 0.3"]);
    let session = f.session(); let fingerprint = session.refresh().unwrap().entries.iter().find(|entry| entry.path == "file").unwrap().fingerprint.clone();
    let start = Instant::now();
    let error = external_tools::launch_with_timeout(&session, &f.recovery(), &ExternalToolRequest {
            expected_context: external_tools::confirmation_context(&session).unwrap(),
        kind: ExternalToolKind::Diff, path: "file".into(), tool: Some("slow".into()), revision: None,
        changes_kind: Some(revia_lib::git::status::ChangesKind::Unstaged), expected_fingerprint: Some(fingerprint),
    }, None, Duration::from_millis(40)).unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
    assert!(start.elapsed() < Duration::from_secs(2)); assert!(!f.recovery().list(None).unwrap().is_empty());
    assert_eq!(f.read("file"), b"local\n");
    std::thread::sleep(Duration::from_millis(400)); // Only this fixture's tool is still exiting.
}

#[test]
fn review_file_history_must_treat_filename_metacharacters_literally() {
    let f = Fixture::new();
    let (literal_path, other_path) = if cfg!(windows) { ("a[bc]", "ab") } else { ("a*", "abc") };
    f.write(literal_path, b"literal name\n");
    let original = f.commit_all("literal a-star");
    f.write(other_path, b"unrelated path\n");
    let unrelated = f.commit_all("only other path changes");
    let session = f.session();
    let result = file_history::history(&session.git, &FileHistoryRequest {
        path: literal_path.into(), all_branches: false, skip: 0, limit: 40, anchor: None,
    }).unwrap();
    println!("fixture={} original={original} unrelated={unrelated} result={result:?}", f.temp.path().display());
    assert!(!result.entries.iter().any(|entry| entry.oid == unrelated), "History for literal {literal_path} includes unrelated {other_path} commit");
}

#[test]
fn review_reset_must_reject_a_replacement_repository_at_the_same_path() {
    let f = Fixture::new();
    f.write("file", b"base\n");
    f.commit_all("base");
    f.write("file", b"confirmed\n");
    let expected_head = f.head();
    let session = f.session();
    let confirmed = rewrite::preflight(&session, &[]).unwrap();
    let moved = f.temp.path().join("original-repository");
    fs::rename(&f.repo, &moved).unwrap();
    let cloned = Command::new("git").current_dir(f.temp.path()).env_clear().envs(&f.vars)
        .args(["clone", "--quiet", moved.to_str().unwrap(), f.repo.to_str().unwrap()]).output().unwrap();
    assert!(cloned.status.success(), "{}", String::from_utf8_lossy(&cloned.stderr));
    f.git(&["config", "core.autocrlf", "false"]);
    f.write("file", b"confirmed\n");
    let outcome = rewrite::reset(&session, &f.recovery(), &f.autostash(), &f.archives(), &ResetRequest {
            expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
        target: expected_head.clone(), mode: ResetMode::Hard, expected_head, expected_state: Some(confirmed.reset_fingerprint),
    });
    let bytes = f.read("file");
    println!("fixture={} outcome={outcome:?} replacement_after={bytes:?}", f.temp.path().display());
    assert!(outcome.is_err(), "Old confirmation wrote to a replacement repository at the same path");
}

fn rebase_confirmation(session: &WorktreeSession, upstream: &str) -> String {
    let text = session.git.run_ok(&["rev-list", &format!("{upstream}..HEAD")], "read range").unwrap().stdout_lossy();
    let oids = text.lines().map(str::to_string).collect::<Vec<_>>();
    rewrite::preflight(session, &oids).unwrap().overwrite_fingerprint
}

#[test]
fn same_oid_branch_switch_invalidates_all_reset_modes() {
    for mode in [ResetMode::Soft, ResetMode::Mixed, ResetMode::Hard, ResetMode::Keep] {
        let f = Fixture::new(); f.write("file", b"base\n"); let base = f.commit_all("base");
        f.write("file", b"tip\n"); let tip = f.commit_all("tip"); f.git(&["branch", "other"]);
        let session = f.session(); let confirmed = rewrite::preflight(&session, &[base.clone()]).unwrap();
        let request = ResetRequest { target: base, mode, expected_head: tip.clone(),
            expected_context: confirmed.context_version, expected_state: Some(confirmed.reset_fingerprint) };
        f.checkout("other");
        assert!(rewrite::reset(&session, &f.recovery(), &f.autostash(), &f.archives(), &request).is_err());
        assert_eq!(f.oid("main"), tip); assert_eq!(f.head(), tip); assert_eq!(f.read("file"), b"tip\n");
    }
}

#[test]
fn stash_strategy_rejects_changes_after_confirmation_and_keeps_bytes() {
    let f = Fixture::new(); f.write("file", b"base\n"); let base = f.commit_all("base");
    f.branch("source"); f.write("file", b"picked\n"); let picked = f.commit_all("source"); f.checkout("main");
    f.write("file", b"viewed\n"); let session = f.session();
    let confirmed = rewrite::preflight(&session, &[picked.clone()]).unwrap();
    let request = CherryPickRequest { oids: vec![picked], expected_head: base.clone(),
        expected_context: confirmed.context_version, expected_state: Some(confirmed.overwrite_fingerprint),
        dirty_strategy: Some(DirtyStrategy::Stash), keep_empty: false, include_source: false };
    f.write("file", b"new external text\n"); let index = f.index();
    let error = rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &request).unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::Rejected);
    assert_eq!(f.read("file"), b"new external text\n"); assert_eq!(f.head(), base); assert_eq!(f.index(), index);
    assert!(stash::list(&session.git).unwrap().is_empty());
}

#[test]
fn manual_stash_save_and_apply_reject_stale_confirmation() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); f.write("file", b"viewed\n");
    let session = f.session(); let save = StashSaveRequest { message: "viewed".into(), keep_index: false,
        expected_state: stash::preflight(&session, None).unwrap() };
    f.write("file", b"new external\n");
    assert!(stash::save(&session, &save).is_err()); assert_eq!(f.read("file"), b"new external\n");
    let entry = stash::save(&session, &StashSaveRequest { expected_state: stash::preflight(&session, None).unwrap(), ..save }).unwrap();
    let apply = StashApplyRequest { entry_key: None, expected_state: stash::preflight(&session, Some(&entry.oid)).unwrap(),
        oid: entry.oid, restore_index: false, delete_after: false, new_branch: None };
    f.write("file", b"after apply confirmation\n");
    assert!(stash::apply(&session, &f.recovery(), &f.archives(), &apply).is_err());
    assert_eq!(f.read("file"), b"after apply confirmation\n"); assert_eq!(stash::list(&session.git).unwrap().len(), 1);
}

#[test]
fn abort_rejects_new_conflict_edit_after_confirmation() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    f.branch("source"); f.write("file", b"source\n"); let picked = f.commit_all("source");
    f.checkout("main"); f.write("file", b"current\n"); f.commit_all("current");
    assert!(!f.try_git(&["cherry-pick", &picked]).status.success()); let session = f.session();
    let state = operation_flow::state(&session, &f.data()).unwrap().unwrap();
    let request = OperationActionRequest { kind: state.kind, metadata_version: state.metadata_version,
        action: OperationAction::Abort, expected_state: state.confirmation_state };
    f.write("file", b"new resolution after confirmation\n"); let index = f.index();
    assert!(operation_flow::execute(&session, &f.recovery(), &f.autostash(), &f.archives(), &request).is_err());
    assert_eq!(f.read("file"), b"new resolution after confirmation\n"); assert_eq!(f.index(), index);
}

#[test]
fn stash_branch_and_rebase_abort_protect_ignored_target_bytes() {
    for abort in [false, true] {
        let f = Fixture::new(); f.write("generated.txt", b"tracked output\n"); f.write("file", b"base\n"); f.commit_all("base");
        let oid = if abort {
            f.branch("topic"); f.write("file", b"topic\n"); f.commit_all("topic"); f.checkout("main"); String::new()
        } else {
            f.write("file", b"stash changes\n"); f.git(&["stash", "push", "-m", "stash changes"]); f.oid("refs/stash")
        };
        f.git(&["rm", "generated.txt"]); f.write(".gitignore", b"generated.txt\n"); f.write("file", b"main\n"); f.commit_all("main ignores output");
        if abort { f.checkout("topic"); assert!(!f.try_git(&["rebase", "main"]).status.success()); }
        f.write("generated.txt", b"unique ignored local bytes\n"); let session = f.session();
        if abort {
            let state = operation_flow::state(&session, &f.data()).unwrap().unwrap();
            operation_flow::execute(&session, &f.recovery(), &f.autostash(), &f.archives(), &OperationActionRequest {
                kind: state.kind, metadata_version: state.metadata_version, action: OperationAction::Abort,
                expected_state: state.confirmation_state }).unwrap();
        } else {
            let expected_state = stash::preflight(&session, Some(&oid)).unwrap();
            stash::apply(&session, &f.recovery(), &f.archives(), &StashApplyRequest { entry_key: None, oid, expected_state,
                restore_index: false, delete_after: false, new_branch: Some("restored".into()) }).unwrap();
        }
        assert_eq!(f.read("generated.txt"), b"tracked output\n");
        let saved = f.recovery().list(None).unwrap().iter().map(|summary| f.recovery().read(&summary.id).unwrap().0)
            .flat_map(|record| record.before).find(|file| file.path == "generated.txt").unwrap();
        assert_eq!(saved.content.as_deref(), Some(b"unique ignored local bytes\n".as_slice()));
    }
}

#[cfg(unix)]
#[test]
fn autostash_cleanup_error_keeps_restored_receipt_and_blocks_reapply() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); f.write("file", b"unique restored bytes\n");
    let session = f.session(); let recovery = f.recovery(); let auto = f.autostash(); let archive = f.archives();
    { let _write = session.acquire_write_lock().unwrap(); auto.save_before(&session, &recovery, "rebase").unwrap(); }
    let hook = f.repo.join(".git/hooks/reference-transaction");
    fs::write(&hook, b"#!/bin/sh\nif [ \"$1\" = committed ]; then\n while read old new ref; do\n if [ \"$ref\" = refs/stash ] && [ \"$new\" = 0000000000000000000000000000000000000000 ]; then mkdir -p .git/logs/refs/stash; fi\n done\nfi\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let outcome = { let _write = session.acquire_write_lock().unwrap(); auto.restore_if_ready(&session, &recovery, &archive).unwrap().unwrap() };
    assert!(outcome.restored && !outcome.removed);
    assert_eq!(outcome.cleanup_error.unwrap().kind, revia_lib::error::ErrorKind::NeedsVerification);
    assert_eq!(auto.list().unwrap().len(), 1); assert_eq!(f.read("file"), b"unique restored bytes\n");
    { let _write = session.acquire_write_lock().unwrap(); assert!(auto.restore_if_ready(&session, &recovery, &archive).is_err()); }
    assert_eq!(f.read("file"), b"unique restored bytes\n");
}

#[cfg(unix)]
#[test]
fn interrupted_archive_store_keeps_real_stash_and_uncertain_receipt() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); f.write("file", b"saved\n");
    f.git(&["stash", "push", "-m", "archive"]); let oid = f.oid("refs/stash"); let session = f.session();
    let archives = f.archives(); let record = archives.archive(&session.git, f.identity(&session), &oid, "archive", Some("main".into())).unwrap();
    f.git(&["stash", "drop"]); let hook = f.repo.join(".git/hooks/reference-transaction");
    fs::write(&hook, b"#!/bin/sh\nif [ \"$1\" = committed ]; then\n while read old new ref; do\n if [ \"$ref\" = refs/stash ] && [ \"$new\" != 0000000000000000000000000000000000000000 ]; then kill -TERM \"$PPID\"; fi\n done\nfi\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let error = archives.restore(&session, &record.id).unwrap_err();
    assert_eq!(f.oid("refs/stash"), oid); assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
}

#[test]
fn snapshot_rejection_retains_structured_product_cause() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); let session = f.session();
    fs::remove_file(f.repo.join("file")).unwrap(); fs::create_dir(f.repo.join("file")).unwrap();
    let error = revia_lib::commit_snapshot::save_snapshot(&session.git, &f.recovery(), &f.identity(&session), &["file".into()]).err().unwrap();
    assert!(error.cause.is_some()); assert!(!error.params.iter().any(|(key, _)| *key == "error"));
}

#[test]
fn ignored_overwrite_cannot_silently_use_stash_strategy() {
    let f = Fixture::new(); f.write(".gitignore", b"generated\n"); let base = f.commit_all("base");
    f.branch("source"); f.write("generated", b"tracked source\n"); f.git(&["add", "-f", "generated"]); let oid = f.commit_all("source"); f.checkout("main");
    f.write("generated", b"unique ignored bytes\n"); let session = f.session();
    let confirmed = rewrite::preflight(&session, &[oid.clone()]).unwrap(); assert!(confirmed.requires_choice);
    let request = CherryPickRequest { expected_context: confirmed.context_version, expected_state: Some(confirmed.overwrite_fingerprint),
        expected_head: base.clone(), oids: vec![oid], dirty_strategy: Some(DirtyStrategy::Stash), keep_empty: false, include_source: false };
    let error = rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &request).unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::Rejected); assert_eq!(f.head(), base);
    assert_eq!(f.read("generated"), b"unique ignored bytes\n"); assert!(stash::list(&session.git).unwrap().is_empty());
    let request = CherryPickRequest { dirty_strategy: Some(DirtyStrategy::Force), ..request };
    assert!(rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &request).unwrap().completed);
}

#[test]
fn multi_commit_selection_survives_continue_skip_abort_and_restart() {
    for action in [OperationAction::Continue, OperationAction::Skip, OperationAction::Abort] {
        let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); f.branch("source");
        f.write("a", b"A\n"); let a = f.commit_all("A"); f.write("file", b"source B\n"); let b = f.commit_all("B");
        f.write("c", b"C\n"); let c = f.commit_all("C"); f.checkout("main"); f.write("file", b"current main\n"); let original = f.commit_all("main");
        f.write("local", b"saved staged local\n"); f.git(&["add", "local"]);
        let session = f.session(); let auto = f.autostash(); let oids = vec![a.clone(), b.clone(), c.clone()];
        let confirmed = rewrite::preflight(&session, &oids).unwrap();
        let outcome = rewrite::cherry_pick(&session, &f.recovery(), &auto, &f.archives(), &CherryPickRequest {
            oids, expected_head: original.clone(), expected_context: confirmed.context_version,
            expected_state: Some(confirmed.overwrite_fingerprint), dirty_strategy: Some(DirtyStrategy::Stash), keep_empty: false, include_source: false }).unwrap();
        assert!(!outcome.completed); assert_eq!(outcome.applied, vec![a.clone()]); assert_eq!(outcome.pending, vec![c.clone()]);
        assert!(!f.repo.join("local").exists()); assert!(auto.for_worktree(&session).unwrap().is_some());
        if action == OperationAction::Continue { f.write("file", b"resolved B\n"); f.git(&["add", "file"]); }
        // Reopen the real session and stores; remaining selection is not an in-memory closure.
        let session = f.session(); let state = operation_flow::state(&session, &f.data()).unwrap().unwrap();
        let result = operation_flow::execute(&session, &f.recovery(), &f.autostash(), &f.archives(), &OperationActionRequest {
            kind: state.kind, metadata_version: state.metadata_version, expected_state: state.confirmation_state, action }).unwrap();
        assert!(result.completed); assert_eq!(f.read("local"), b"saved staged local\n");
        assert!(stash::list(&session.git).unwrap().is_empty());
        if action == OperationAction::Abort {
            assert_eq!(f.head(), original); assert!(!f.repo.join("a").exists()); assert!(!f.repo.join("c").exists());
            let record = f.recovery().list(None).unwrap().into_iter().find(|record| record.source == RecoverySource::OperationAbort).unwrap();
            let (_, outcome) = f.recovery().read(&record.id).unwrap();
            let revia_lib::recovery::DiscardOutcome::Completed { after } = outcome.unwrap() else { panic!("missing abort receipt") };
            assert_eq!(after.iter().find(|file| file.path == "a").unwrap().content, None);
            for file in &after { assert_eq!(file.content, fs::read(f.repo.join(&file.path)).ok(), "final baseline for {}", file.path); }
        }
        else {
            assert_eq!(f.read("c"), b"C\n"); let continuation = result.continuation.unwrap();
            if action == OperationAction::Skip { assert!(continuation.skipped_empty.is_empty()); assert_eq!(continuation.skipped, vec![b]); assert_eq!(continuation.applied, vec![a, c]); }
            else { assert_eq!(continuation.applied, vec![a, b, c]); assert_eq!(f.read("file"), b"resolved B\n"); }
        }
        assert!(operation_flow::state(&session, &f.data()).unwrap().is_none());
    }
}

#[cfg(unix)]
#[test]
fn multi_history_failure_without_native_stop_keeps_partial_receipts() {
    let mut cases = Vec::new();
    for revert in [false, true] {
        for failure in ["index-lock", "receipt-write"] {
            let mut f = Fixture::new(); f.vars.insert("LC_ALL".into(), "C".into());
            f.write("base", b"base\n"); let base = f.commit_all("base");
            f.branch("source");
            let mut oids = Vec::new();
            for name in ["a", "b", "c"] { f.write(name, format!("{name}\n").as_bytes()); oids.push(f.commit_all(name)); }
            if revert { oids.reverse(); } else { f.checkout("main"); }
            let original = f.head();
            f.vars.insert("REVIA_TEST_DATA".into(), f.data().display().to_string());
            let script = if failure == "index-lock" {
                "#!/bin/sh\nprintf 'owned fixture lock\\n' > .git/index.lock\n"
            } else {
                "#!/bin/sh\nstep=0\n[ ! -f .git/fixture-step ] || step=$(cat .git/fixture-step)\nstep=$((step+1))\nprintf '%s' \"$step\" > .git/fixture-step\n[ \"$step\" = 2 ] || exit 0\nfor entry in \"$REVIA_TEST_DATA\"/recovery/record-*; do\n if [ -f \"$entry/saved.json\" ] && [ ! -f \"$entry/outcome.json\" ]; then mkdir \"$entry/outcome.pending\"; fi\ndone\n"
            };
            install_m6_hook(&f, "post-commit", script);
            let session = f.session(); let checked = rewrite::preflight(&session, &oids).unwrap();
            let outcome = if revert {
                rewrite::revert(&session, &f.recovery(), &f.autostash(), &f.archives(), &RevertRequest {
                    oids: oids.clone(), expected_head: original.clone(), expected_context: checked.context_version,
                    expected_state: Some(checked.overwrite_fingerprint), dirty_strategy: None,
                })
            } else {
                rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &CherryPickRequest {
                    oids: oids.clone(), expected_head: base.clone(), expected_context: checked.context_version,
                    expected_state: Some(checked.overwrite_fingerprint), dirty_strategy: None, keep_empty: false, include_source: false,
                })
            }.expect("partial progress must survive an error without a native stop");
            assert!(!outcome.completed); assert_eq!(outcome.applied, vec![oids[0].clone()]);
            assert_eq!(outcome.failed_oid.as_deref(), Some(oids[1].as_str())); assert_eq!(outcome.pending, vec![oids[2].clone()]);
            assert!(outcome.skipped.is_empty() && outcome.skipped_empty.is_empty());
            assert_eq!(outcome.error.as_ref().unwrap().kind, revia_lib::error::ErrorKind::NeedsVerification);
            assert!(operation::detect(&session.git, 0).unwrap().is_none());
            assert_ne!(f.head(), original);
            assert!(f.repo.join("a").exists());
            assert_eq!(f.repo.join("b").exists(), if revert { failure == "index-lock" } else { failure == "receipt-write" });
            assert!(!f.repo.join("c").exists());
            if failure == "index-lock" { assert_eq!(fs::read(f.repo.join(".git/index.lock")).unwrap(), b"owned fixture lock\n"); }
            // The receipt does not authorize replay. Reopening the session
            // still exposes an uncertain plan and explicit Continue is refused.
            let current_head = f.head(); let index = f.index(); let reopened = f.session();
            let state = operation_flow::state(&reopened, &f.data()).unwrap().unwrap();
            assert!(state.needs_verification && state.continuation_only);
            let before = reopened.git.log.snapshot().len();
            assert!(operation_flow::execute(&reopened, &f.recovery(), &f.autostash(), &f.archives(), &OperationActionRequest {
                kind: state.kind, action: OperationAction::Continue, metadata_version: state.metadata_version,
                expected_state: state.confirmation_state,
            }).is_err());
            assert!(!reopened.git.log.snapshot()[before..].iter().any(|record| record.args.iter().any(|arg| matches!(arg.as_str(), "cherry-pick" | "revert"))));
            assert_eq!(f.head(), current_head); assert_eq!(f.index(), index);
            let commands = session.git.log.snapshot();
            if let Ok(path) = std::env::var("REVIA_M6_ERROR_FIXTURE") {
                let fixture = f.temp.keep();
                cases.push(serde_json::json!({"revert":revert,"failure":failure,"fixture":fixture,"outcome":outcome,"git":commands,"head":current_head}));
                fs::write(format!("{path}.partial-history.json"), serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_history_continuation_recovery_failure_keeps_completed_selection() {
    let mut cases = Vec::new();
    for revert in [false, true] {
        for action in [OperationAction::Continue, OperationAction::Skip] {
            let mut f = Fixture::new(); f.vars.insert("LC_ALL".into(), "C".into());
            f.write("file", b"base\n"); f.commit_all("base"); f.branch("source");
            f.write("file", b"source\n"); let a = f.commit_all("source one");
            f.write("second", b"second\n"); let b = f.commit_all("source two");
            if !revert { f.checkout("main"); }
            f.write("file", b"main\n"); let original = f.commit_all("main conflict");
            f.vars.insert("REVIA_TEST_DATA".into(), f.data().display().to_string());
            let session = f.session(); let oids = vec![a.clone(), b.clone()]; let pre = rewrite::preflight(&session, &oids).unwrap();
            let start = if revert {
                rewrite::revert(&session, &f.recovery(), &f.autostash(), &f.archives(), &RevertRequest {
                    oids: oids.clone(), expected_head: original, expected_context: pre.context_version,
                    expected_state: Some(pre.overwrite_fingerprint), dirty_strategy: None,
                })
            } else {
                rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &CherryPickRequest {
                    oids: oids.clone(), expected_head: original, expected_context: pre.context_version,
                    expected_state: Some(pre.overwrite_fingerprint), dirty_strategy: None, keep_empty: false, include_source: false,
                })
            }.unwrap();
            assert!(!start.completed); assert!(start.state.unwrap().has_conflicts());
            if action == OperationAction::Continue {
                f.write("file", if revert { b"base\n" } else { b"source\n" }); f.git(&["add", "file"]);
            }
            let source = if action == OperationAction::Continue { "operationContinue" } else { "operationSkip" };
            install_m6_hook(&f, "post-commit", &format!(
                "#!/bin/sh\nfor entry in \"$REVIA_TEST_DATA\"/recovery/record-*; do\n if [ -f \"$entry/saved.json\" ] && grep -q '\"source\":\"{source}\"' \"$entry/saved.json\"; then mkdir -p \"$entry/outcome.pending\"; fi\ndone\n"));
            let state = operation_flow::state(&session, &f.data()).unwrap().unwrap();
            let request = OperationActionRequest { kind: state.kind, action,
                metadata_version: state.metadata_version, expected_state: state.confirmation_state };
            let outcome = operation_flow::execute(&session, &f.recovery(), &f.autostash(), &f.archives(), &request)
                .expect("recovery failure must preserve the completed continuation receipt");
            assert!(!outcome.completed); assert!(outcome.state.is_none());
            let receipt = outcome.continuation.as_ref().unwrap();
            assert!(!receipt.completed); assert_eq!(receipt.applied, if action == OperationAction::Continue { oids.clone() } else { vec![b] });
            assert_eq!(receipt.skipped, if action == OperationAction::Skip { vec![a] } else { Vec::new() });
            assert!(receipt.pending.is_empty() && receipt.failed_oid.is_none());
            assert_eq!(receipt.error.as_ref().unwrap().kind, revia_lib::error::ErrorKind::NeedsVerification);
            assert!(operation_flow::state(&session, &f.data()).unwrap().is_none());
            assert_eq!(f.repo.join("second").exists(), !revert);
            assert_eq!(f.read("file"), if action == OperationAction::Skip { b"main\n".as_slice() }
                else if revert { b"base\n".as_slice() } else { b"source\n".as_slice() });
            let record = outcome.recovery_id.as_ref().unwrap();
            assert!(f.recovery().read(record).unwrap().1.is_none());
            let head = f.head(); let index = f.index();
            assert!(operation_flow::execute(&session, &f.recovery(), &f.autostash(), &f.archives(), &request).is_err());
            assert_eq!(f.head(), head); assert_eq!(f.index(), index);
            if let Ok(path) = std::env::var("REVIA_M6_ERROR_FIXTURE") {
                let git = session.git.log.snapshot(); let fixture = f.temp.keep();
                cases.push(serde_json::json!({"revert":revert,"action":action,"fixture":fixture,"outcome":outcome,"git":git}));
                fs::write(format!("{path}.continue-receipts.json"), serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
            }
        }
    }
}

#[test]
fn external_tool_settings_failure_has_a_real_error_and_does_not_prevent_this_launch() {
    use revia_lib::settings::{Registry, SettingsPatch, settings_path};
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); f.write("file", b"local\n");
    f.git(&["config", "difftool.fixturetool.cmd", "true"]);
    let data = f.data(); fs::create_dir_all(&data).unwrap();
    let mut registry = Registry::load(data.clone(), Arc::new(CommandLog::default()));
    // A directory at our own settings file makes the real persistence fail.
    fs::create_dir(settings_path(&data)).unwrap();
    let error = registry.update_settings(&SettingsPatch { diff_tool: Some("fixturetool".into()), ..Default::default() })
        .unwrap_err().payload();
    assert_eq!(error.code.as_deref(), Some("errors.settings.failedWriteSettingsCause"));
    assert!(Registry::load(data, Arc::new(CommandLog::default())).settings().diff_tool.is_none());
    let session = f.session(); let fingerprint = session.refresh().unwrap().entries.into_iter().find(|entry| entry.path == "file").unwrap().fingerprint;
    let outcome = external_tools::launch(&session, &f.recovery(), &ExternalToolRequest {
        expected_context: external_tools::confirmation_context(&session).unwrap(), kind: ExternalToolKind::Diff,
        path: "file".into(), tool: Some("fixturetool".into()), revision: None,
        changes_kind: Some(revia_lib::git::status::ChangesKind::Unstaged), expected_fingerprint: Some(fingerprint),
    }, None).unwrap();
    assert_eq!(outcome.exit_code, Some(0)); assert_eq!(f.read("file"), b"local\n");
    if let Ok(path) = std::env::var("REVIA_M6_ERROR_FIXTURE") {
        let git = session.git.log.snapshot(); let fixture = f.temp.keep();
        fs::write(format!("{path}.external-settings.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "error":error,"outcome":outcome,"fixture":fixture,"git":git,
        })).unwrap()).unwrap();
    }
}

#[test]
fn real_history_receipts_are_available_when_the_followup_git_read_fails() {
    let mut cases = Vec::new();
    for conflict in [false, true] {
        let mut f = Fixture::new(); f.vars.insert("LC_ALL".into(), "C".into());
        f.write("a", b"base\n"); f.write("b", b"base\n"); let base = f.commit_all("base"); f.branch("source");
        f.write("a", b"A\n"); let a = f.commit_all("A"); f.write("b", b"B\n"); let b = f.commit_all("B");
        f.write("c", b"C\n"); let c = f.commit_all("C"); f.checkout("main");
        let head = if conflict { f.write("b", b"main\n"); f.commit_all("diverge") } else { base };
        let session = f.session(); let oids = vec![a.clone(), b.clone(), c.clone()];
        let pre = rewrite::preflight(&session, &oids).unwrap();
        let outcome = rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &CherryPickRequest {
            oids: oids.clone(), expected_head: head, expected_context: pre.context_version,
            expected_state: Some(pre.overwrite_fingerprint), dirty_strategy: None, keep_empty: false, include_source: false,
        }).unwrap();
        assert_eq!(outcome.completed, !conflict);
        assert_eq!(outcome.applied, if conflict { vec![a] } else { oids });
        assert_eq!(outcome.failed_oid, if conflict { Some(b) } else { None });
        assert_eq!(outcome.pending, if conflict { vec![c] } else { Vec::new() });
        // Preserve the original fixture index, then make the next real Git
        // status read fail. No user repository or normal app data is touched.
        fs::rename(f.repo.join(".git/index"), f.repo.join(".git/index.preserved")).unwrap();
        fs::create_dir(f.repo.join(".git/index")).unwrap();
        let error = session.refresh().expect_err("the owned index is deliberately unreadable").payload();
        if let Ok(path) = std::env::var("REVIA_M6_ERROR_FIXTURE") {
            let git = session.git.log.snapshot(); let fixture = f.temp.keep();
            cases.push(serde_json::json!({"conflict":conflict,"fixture":fixture,"outcome":outcome,"readError":error,"git":git}));
            fs::write(format!("{path}.history-readbacks.json"), serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
        }
    }
}

#[test]
fn binary_blame_refuses_replacement_characters_and_nul_content() {
    for bytes in [b"bad byte: \xff\n".as_slice(), b"binary\0data\n".as_slice()] {
        let f = Fixture::new(); f.write("file", bytes); let oid = f.commit_all("binary"); let session = f.session();
        let result = file_history::blame(&session.git, &BlameRequest { path: "file".into(), revision: Some(oid),
            ignore_whitespace: true, detect_moves: false, detect_copies: false });
        assert_eq!(result.unwrap_err().kind, revia_lib::error::ErrorKind::Rejected); assert_eq!(f.read("file"), bytes);
    }
}

#[test]
fn stash_archive_rejects_data_inside_any_worktree_and_preserves_stash() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); f.write("file", b"stash\n");
    f.git(&["stash", "push", "-m", "preserve"]); let oid = f.oid("refs/stash"); let session = f.session();
    let archive = StashArchiveStore::new(&f.repo.join("owned-app-data"));
    assert!(stash::drop(&session, &archive, &oid).is_err()); assert_eq!(f.oid("refs/stash"), oid);
    let linked = f.temp.path().join("linked"); f.git(&["worktree", "add", "-b", "linked", linked.to_str().unwrap()]);
    let linked_session = f.session_at(&linked);
    assert!(stash::drop(&linked_session, &archive, &oid).is_err()); assert_eq!(f.oid("refs/stash"), oid);
    #[cfg(unix)] {
        let data = f.temp.path().join("outside-app-data"); fs::create_dir(&data).unwrap();
        let inside = f.repo.join("archive-destination"); fs::create_dir(&inside).unwrap();
        std::os::unix::fs::symlink(&inside, data.join("stash-archive")).unwrap();
        assert!(stash::drop(&session, &StashArchiveStore::new(&data), &oid).is_err()); assert_eq!(f.oid("refs/stash"), oid);
    }
}

#[cfg(unix)]
#[test]
fn signalled_external_tool_write_must_be_uncertain() {
    let f=Fixture::new();f.write("file",b"base\n");f.commit_all("base");f.write("file",b"local\n");
    let tool=f.temp.path().join("tool.py");
    fs::write(&tool,r#"import os,signal,subprocess,json,pathlib,sys
pathlib.Path(sys.argv[1]).write_text('tool already modified this file\n')
p=os.getpid(); rows=[]
for _ in range(12):
    p=int(subprocess.check_output(['/bin/ps','-o','ppid=','-p',str(p)]).strip())
    if p<=1:break
    cmd=subprocess.check_output(['/bin/ps','-o','command=','-p',str(p)]).decode().strip()
    rows.append([p,cmd])
    if 'difftool --no-prompt' in cmd:
        pathlib.Path('tool-ancestors.json').write_text(json.dumps(rows));os.kill(p,signal.SIGTERM);break
else:raise RuntimeError('owned difftool process not found')
"#).unwrap();
    let command=format!("/usr/bin/python3 '{}' \"$REMOTE\"",tool.display());f.git(&["config","difftool.review.cmd",&command]);
    let session=f.session();let fingerprint=session.refresh().unwrap().entries.into_iter().find(|entry|entry.path=="file").unwrap().fingerprint;
    let result=external_tools::launch(&session,&f.recovery(),&ExternalToolRequest{expected_context:external_tools::confirmation_context(&session).unwrap(),kind:ExternalToolKind::Diff,path:"file".into(),tool:Some("review".into()),revision:None,changes_kind:Some(revia_lib::git::status::ChangesKind::Unstaged),expected_fingerprint:Some(fingerprint)},None);
    println!("result={result:?} contents={:?} log={:?}",f.read("file"),session.git.log.snapshot());
    fs::write(f.temp.path().join("facts.json"),serde_json::to_vec_pretty(&serde_json::json!({"result":format!("{result:?}"),"after":f.read("file"),"log":session.git.log.snapshot()})).unwrap()).unwrap();
    assert!(matches!(result,Err(error) if error.kind==revia_lib::error::ErrorKind::NeedsVerification),"signal after tool write returned success-shaped outcome");
}

#[cfg(unix)]
#[test]
fn interrupted_history_intent_is_not_replayed_after_restart() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(); f.write("base", b"base\n"); let base = f.commit_all("base"); f.branch("source");
    f.write("a", b"A\n"); let a = f.commit_all("A"); f.write("b", b"B\n"); let b = f.commit_all("B"); f.checkout("main");
    let hook = f.repo.join(".git/hooks/reference-transaction");
    fs::write(&hook, b"#!/bin/sh\nif [ \"$1\" = committed ]; then\n while read old new ref; do\n if [ \"$ref\" = refs/heads/main ] && [ \"$new\" != 0000000000000000000000000000000000000000 ]; then kill -TERM \"$PPID\"; fi\n done\nfi\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let session = f.session(); let oids = vec![a.clone(),b.clone()]; let confirmed = rewrite::preflight(&session, &oids).unwrap();
    let result = rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &CherryPickRequest {
        oids, expected_head: base, expected_context: confirmed.context_version, expected_state: Some(confirmed.overwrite_fingerprint),
        dirty_strategy: None, keep_empty: false, include_source: false }).unwrap();
    assert!(!result.completed);
    assert_eq!(result.error.as_ref().unwrap().kind, revia_lib::error::ErrorKind::NeedsVerification);
    assert!(result.applied.is_empty(), "interrupted Git did not confirm success");
    assert_eq!(result.failed_oid.as_deref(), Some(a.as_str())); assert_eq!(result.pending, vec![b]);
    assert_eq!(f.read("a"), b"A\n"); assert!(!f.repo.join("b").exists()); let actual_head = f.head(); let actual_index = f.index();
    fs::remove_file(&hook).unwrap(); // Only this fixture's intentionally interrupting hook.
    let session = f.session(); let state = operation_flow::state(&session, &f.data()).unwrap().unwrap();
    assert!(state.needs_verification && state.continuation_only);
    let request = OperationActionRequest { kind: state.kind, metadata_version: state.metadata_version,
        expected_state: state.confirmation_state, action: OperationAction::Continue };
    assert!(operation_flow::execute(&session, &f.recovery(), &f.autostash(), &f.archives(), &request).is_err());
    assert_eq!(f.head(), actual_head); assert_eq!(f.index(), actual_index); assert!(!f.repo.join("b").exists());
    let request = OperationActionRequest { action: OperationAction::Abort, ..request };
    operation_flow::execute(&session, &f.recovery(), &f.autostash(), &f.archives(), &request).unwrap();
    assert_eq!(f.head(), actual_head); assert_eq!(f.index(), actual_index);
    assert!(operation_flow::state(&session, &f.data()).unwrap().is_none_or(|state| !state.continuation_only));
}

#[test]
fn multi_revert_continues_remaining_original_selection() {
    let f = Fixture::new(); for path in ["a", "b", "c"] { f.write(path, b"base\n"); } f.commit_all("base");
    f.write("a", b"A\n"); let a = f.commit_all("A"); f.write("b", b"B\n"); let b = f.commit_all("B");
    f.write("c", b"C\n"); let c = f.commit_all("C"); f.write("b", b"later B\n"); f.commit_all("later");
    let session = f.session(); let oids = vec![c.clone(), b.clone(), a.clone()]; let confirmed = rewrite::preflight(&session, &oids).unwrap();
    let result = rewrite::revert(&session, &f.recovery(), &f.autostash(), &f.archives(), &RevertRequest {
        oids, expected_head: f.head(), expected_context: confirmed.context_version, expected_state: Some(confirmed.overwrite_fingerprint), dirty_strategy: None }).unwrap();
    assert_eq!(result.applied, vec![c.clone()]); assert_eq!(result.pending, vec![a.clone()]);
    f.write("b", b"resolved revert B\n"); f.git(&["add", "b"]);
    let session = f.session(); let state = operation_flow::state(&session, &f.data()).unwrap().unwrap();
    let result = operation_flow::execute(&session, &f.recovery(), &f.autostash(), &f.archives(), &OperationActionRequest {
        kind: state.kind, metadata_version: state.metadata_version, expected_state: state.confirmation_state, action: OperationAction::Continue }).unwrap();
    assert!(result.completed); assert_eq!(result.continuation.unwrap().applied, vec![c,b,a]); assert_eq!(f.read("a"), b"base\n");
}

#[test]
fn partially_applied_autostash_retains_uncertain_cleanup_payload() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    f.write("file", b"stashed tracked\n"); f.write("new", b"stashed untracked\n");
    let session = f.session(); let auto = f.autostash();
    auto.save_before(&session, &f.recovery(), "rebase").unwrap(); f.write("new", b"new unique collision\n");
    let result = auto.restore_if_ready(&session, &f.recovery(), &f.archives()).unwrap().unwrap();
    assert!(!result.restored && !result.conflicted); assert_eq!(f.read("new"), b"new unique collision\n");
    assert_eq!(result.cleanup_error.unwrap().kind, revia_lib::error::ErrorKind::NeedsVerification);
    assert!(auto.for_worktree(&session).unwrap().is_some());
}

#[cfg(unix)]
#[test]
fn whole_selection_abort_must_not_reset_a_new_head_created_by_a_git_hook() {
 use std::os::unix::fs::PermissionsExt;
 let f=Fixture::new();f.write("file",b"base\n");f.commit_all("base");f.branch("source");
 f.write("a",b"A\n");let a=f.commit_all("A");f.write("file",b"source B\n");let b=f.commit_all("B");f.write("c",b"C\n");let c=f.commit_all("C");
 let foreign=String::from_utf8(f.git(&["commit-tree",&format!("{a}^{{tree}}"),"-p",&a,"-m","external commit during abort"])).unwrap().trim().to_string();
 f.checkout("main");f.write("file",b"main\n");let original=f.commit_all("main");let session=f.session();let oids=vec![a.clone(),b,c];
 let confirmed=rewrite::preflight(&session,&oids).unwrap();let result=rewrite::cherry_pick(&session,&f.recovery(),&f.autostash(),&f.archives(),&CherryPickRequest{oids,expected_head:original.clone(),expected_context:confirmed.context_version,expected_state:Some(confirmed.overwrite_fingerprint),dirty_strategy:None,keep_empty:false,include_source:false}).unwrap();assert!(!result.completed);
 let state=operation_flow::state(&session,&f.data()).unwrap().unwrap();
 let hook=f.repo.join(".git/hooks/reference-transaction");
 let script=format!(r#"#!/bin/sh
if [ "$1" = committed ] && [ -e .git/review06-trigger ]; then
 while read old new ref; do
  if [ "$ref" = refs/heads/main ]; then
   mv .git/review06-trigger .git/review06-fired
   git update-ref refs/heads/main {foreign}
   git rev-parse HEAD > .git/review06-hook-head
  fi
 done
fi
"#);
 fs::write(&hook,script).unwrap();fs::set_permissions(&hook,fs::Permissions::from_mode(0o755)).unwrap();fs::write(f.repo.join(".git/review06-trigger"),b"owned\n").unwrap();
 let result=operation_flow::execute(&session,&f.recovery(),&f.autostash(),&f.archives(),&OperationActionRequest{kind:state.kind,metadata_version:state.metadata_version,expected_state:state.confirmation_state,action:OperationAction::Abort});
 let actual=f.head();let hook_head=fs::read_to_string(f.repo.join(".git/review06-hook-head")).unwrap_or_default();
 let facts=serde_json::json!({"result":format!("{result:?}"),"original":original,"foreign":foreign,"actual":actual,"hook_head":hook_head,"fired":f.repo.join(".git/review06-fired").exists()});fs::write(f.temp.path().join("abort-facts.json"),serde_json::to_vec_pretty(&facts).unwrap()).unwrap();println!("abort_facts={facts}");
 assert_eq!(hook_head.trim(),foreign,"the owned hook must really change HEAD after native abort");
 assert_eq!(actual,foreign,"the second reset silently removed a new HEAD created after native abort");
}

#[test]
fn persisted_restore_errors_keep_their_production_payload_after_reload() {
    use revia_lib::recovery::RestoreExpectation;
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); f.write("file", b"user changes\n");
    let session = f.session(); let store = f.recovery(); let snapshot = session.refresh().unwrap();
    let entry = snapshot.entries.iter().find(|entry| entry.path == "file").unwrap();
    session.discard_file(&entry.path, &entry.fingerprint, &entry.snapshot_revision, &store).unwrap();
    let id = store.list(None).unwrap()[0].id.clone(); let viewed = session.inspect_record(&id, &store).unwrap();
    let expected = RestoreExpectation { version: viewed.version, paths: viewed.paths, state: viewed.state };
    fs::remove_file(f.repo.join("file")).unwrap(); fs::create_dir(f.repo.join("file")).unwrap();
    let failure = session.restore_record(&id, &expected, &store).unwrap_err();
    fs::remove_dir(f.repo.join("file")).unwrap(); f.write("file", b"base\n");
    let reloaded = f.session().inspect_record(&id, &f.recovery()).unwrap();
    let payload = reloaded.restore.unwrap().error.unwrap();
    assert_eq!(payload.code, failure.payload().code); assert!(payload.cause.is_some());
    if let Some(path) = std::env::var_os("REVIA_M6_ERROR_FIXTURE") {
        let output = format!("{}.recovery.json", PathBuf::from(path).display());
        fs::write(output, serde_json::to_vec_pretty(&payload).unwrap()).unwrap();
    }
}

#[test]
fn working_blame_previous_is_bound_to_head_despite_all_branch_history() {
    let mut f = Fixture::new();
    for key in ["GIT_AUTHOR_DATE", "GIT_COMMITTER_DATE"] { f.vars.insert(key.into(), "2020-01-01T00:00:00Z".into()); }
    f.write("file", b"base\n"); f.commit_all("base");
    for key in ["GIT_AUTHOR_DATE", "GIT_COMMITTER_DATE"] { f.vars.insert(key.into(), "2020-02-01T00:00:00Z".into()); }
    f.write("file", b"main data\n"); let main = f.commit_all("main");
    for key in ["GIT_AUTHOR_DATE", "GIT_COMMITTER_DATE"] { f.vars.insert(key.into(), "2020-03-01T00:00:00Z".into()); }
    f.branch("other"); f.write("file", b"other data\n"); let other = f.commit_all("other"); f.checkout("main");
    f.write("file", b"working copy data\n"); let session = f.session();
    let history = file_history::history(&session.git, &FileHistoryRequest { path: "file".into(), all_branches: true, skip: 0, limit: 100, anchor: None }).unwrap();
    assert_eq!(history.entries[0].oid, other);
    let request = BlameRequest { path: "file".into(), revision: None, ignore_whitespace: true, detect_moves: false, detect_copies: false };
    let working = file_history::blame(&session.git, &request).unwrap(); let previous = working.previous.unwrap();
    assert_eq!(previous.oid, main);
    let historical = file_history::blame(&session.git, &BlameRequest { path: previous.path, revision: Some(previous.oid), ..request }).unwrap();
    assert_eq!(historical.lines[0].content, "main data"); assert_eq!(f.read("file"), b"working copy data\n");
}


#[test]
fn shallow_stash_archive_remains_restorable_after_original_history_is_pruned() {
    let mut f = Fixture::new();
    for n in 0..3 { f.write("file", format!("base {n}\n").as_bytes()); f.commit_all("base"); }
    let origin = f.repo.clone(); let shallow = f.temp.path().join("shallow");
    f.git(&["clone", "--depth=1", &format!("file://{}", origin.display()), shallow.to_str().unwrap()]);
    f.repo = shallow;
    for (key, value) in [("user.name", "M6 fixture"), ("user.email", "m6@example.invalid"), ("commit.gpgsign", "false")] { f.git(&["config", key, value]); }
    let base = f.head(); f.write("file", b"stash bytes\n"); f.git(&["stash", "push", "-m", "shallow stash"]);
    let session = f.session(); let entry = stash::list(&session.git).unwrap().remove(0);
    let deleted = stash::drop(&session, &f.archives(), &entry.oid).unwrap(); assert!(deleted.deleted);
    f.git(&["checkout", "--orphan", "fresh"]); f.git(&["rm", "-f", "file"]);
    f.write("fresh", b"independent history\n"); f.commit_all("fresh"); f.git(&["remote", "remove", "origin"]);
    fs::remove_file(f.repo.join(".git/shallow")).unwrap(); f.git(&["branch", "-D", "main"]);
    if f.try_git(&["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"]).status.success() {
        f.git(&["symbolic-ref", "--delete", "refs/remotes/origin/HEAD"]);
    }
    f.git(&["reflog", "expire", "--expire=now", "--all"]); f.git(&["gc", "--prune=now"]);
    assert!(!f.command(&["cat-file", "-e", &base], &[]).status.success());
    f.git(&["fsck", "--connectivity-only", "--no-dangling"]);
    let before = f.read("fresh");
    let restored = f.archives().restore(&session, deleted.archive_id.as_deref().unwrap()).unwrap();
    assert_eq!(restored, entry.oid); assert_eq!(f.git(&["show", &format!("{}:file", entry.oid)]), b"stash bytes\n");
    assert_eq!(f.read("fresh"), before); f.git(&["fsck", "--connectivity-only", "--no-dangling"]);
}

#[test]
fn stash_local_comparison_includes_third_parent_without_touching_index() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    f.write("file", b"saved tracked\n"); f.write("未跟踪 文件.txt", b"saved untracked\n"); f.git(&["stash", "push", "-u"]);
    let session = f.session(); let entry = stash::list(&session.git).unwrap().remove(0);
    let before = f.git(&["ls-files", "--stage", "-z"]);
    let missing = stash::inspect(&session.git, &entry.oid, true).unwrap();
    assert!(missing.files.iter().any(|file| file.path == "未跟踪 文件.txt" && file.deleted == 1));
    assert!(missing.patch.contains("saved untracked"));
    f.write("未跟踪 文件.txt", b"local different\n"); f.git(&["add", "--", "未跟踪 文件.txt"]);
    let staged = f.git(&["ls-files", "--stage", "-z"]);
    let present = stash::inspect(&session.git, &entry.oid, true).unwrap();
    let files: Vec<_> = present.files.iter().filter(|file| file.path == "未跟踪 文件.txt").collect();
    assert_eq!(files.len(), 1); assert_eq!(files[0].added, 1); assert_eq!(files[0].deleted, 1);
    assert!(present.patch.contains("-saved untracked")); assert!(present.patch.contains("+local different"));
    assert_eq!(f.git(&["ls-files", "--stage", "-z"]), staged);
    assert_ne!(before, staged); assert_eq!(f.read("未跟踪 文件.txt"), b"local different\n");
}

#[test]
fn explicit_continue_honors_both_persisted_empty_policies() {
    for keep in [false, true] {
        let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); f.branch("source");
        f.write("file", b"source\n"); let source = f.commit_all("source"); f.checkout("main");
        f.write("file", b"target\n"); let head = f.commit_all("target"); let session = f.session();
        let pre = rewrite::preflight(&session, &[source.clone()]).unwrap();
        let start = rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &CherryPickRequest {
            oids: vec![source.clone()], expected_head: head.clone(), expected_context: pre.context_version,
            expected_state: Some(pre.overwrite_fingerprint), dirty_strategy: None, keep_empty: keep, include_source: false }).unwrap();
        assert!(!start.completed); f.git(&["checkout", "--ours", "--", "file"]); f.git(&["add", "file"]);
        let state = operation_flow::state(&session, &f.data()).unwrap().unwrap();
        let result = operation_flow::execute(&session, &f.recovery(), &f.autostash(), &f.archives(), &OperationActionRequest {
            kind: state.kind, metadata_version: state.metadata_version, expected_state: state.confirmation_state, action: OperationAction::Continue }).unwrap();
        assert!(result.completed); let continuation = result.continuation.unwrap();
        if keep { assert_ne!(f.head(), head); assert_eq!(continuation.applied, vec![source]); assert!(continuation.skipped_empty.is_empty()); }
        else { assert_eq!(f.head(), head); assert_eq!(continuation.skipped_empty, vec![source]); assert!(continuation.applied.is_empty()); }
        assert!(continuation.skipped.is_empty()); assert_eq!(f.read("file"), b"target\n");
    }
}

#[test]
fn ui_confirmation_expires_on_refresh_and_session_replacement() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); f.write("file", b"saved\n"); f.git(&["stash", "push"]);
    let session = f.session(); let entry = stash::list(&session.git).unwrap().remove(0);
    let state = stash::preflight(&session, Some(&entry.oid)).unwrap(); session.refresh().unwrap();
    let request = StashApplyRequest { entry_key: None, oid: entry.oid.clone(), restore_index: false, delete_after: false, new_branch: None, expected_state: state };
    assert!(stash::apply(&session, &f.recovery(), &f.archives(), &request).is_err());
    let state = stash::preflight(&session, Some(&entry.oid)).unwrap(); let reopened = f.session();
    assert!(stash::apply(&reopened, &f.recovery(), &f.archives(), &StashApplyRequest { expected_state: state, ..request }).is_err());
    assert_eq!(f.read("file"), b"base\n"); assert_eq!(stash::list(&session.git).unwrap()[0].oid, entry.oid);
}

#[test]
fn explicit_autostash_restore_rejects_a_different_shown_link_and_expired_view() {
    let a = Fixture::new(); a.write("file", b"base\n"); a.commit_all("base"); a.write("file", b"A saved\n"); let sa = a.session();
    let shown_a = a.autostash().save_before(&sa, &a.recovery(), "cherry-pick").unwrap().unwrap();
    let b = Fixture::new(); b.write("file", b"base\n"); b.commit_all("base"); b.write("file", b"B saved\n"); let sb = b.session();
    b.autostash().save_before(&sb, &b.recovery(), "cherry-pick").unwrap();
    let shown_b = b.autostash().pending(&sb).unwrap().unwrap();
    assert!(b.autostash().restore_confirmed(&sb, &b.recovery(), &b.archives(), &shown_a, shown_b.confirmation_state.as_deref().unwrap()).is_err());
    sb.refresh().unwrap();
    assert!(b.autostash().restore_confirmed(&sb, &b.recovery(), &b.archives(), &shown_b.link, shown_b.confirmation_state.as_deref().unwrap()).is_err());
    assert_eq!(b.read("file"), b"base\n"); assert_eq!(a.read("file"), b"base\n");
    let fresh = b.autostash().pending(&sb).unwrap().unwrap();
    assert!(b.autostash().restore_confirmed(&sb, &b.recovery(), &b.archives(), &fresh.link, fresh.confirmation_state.as_deref().unwrap()).unwrap().unwrap().restored);
    assert_eq!(b.read("file"), b"B saved\n"); assert!(a.autostash().for_worktree(&sa).unwrap().is_some());
}

#[cfg(unix)]
#[test]
fn continue_rebase_preserves_ignored_content_created_at_the_edit_stop() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.write(".gitignore", b"victim.txt\n"); f.write("regular.txt", b"base\n");
    let base = f.commit_all("base");
    f.write("regular.txt", b"first\n"); f.commit_all("edit stop");
    f.write("victim.txt", b"later committed bytes\n");
    f.git(&["add", "-f", "victim.txt"]); f.git(&["commit", "--quiet", "-m", "later addition"]);
    let editor = f.temp.path().join("sequence-editor.sh");
    fs::write(&editor, b"#!/bin/sh\nsed '1s/^pick /edit /' \"$1\" > \"$1.next\"\nmv \"$1.next\" \"$1\"\n").unwrap();
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(f.command(&["rebase", "--interactive", &base], &[("GIT_SEQUENCE_EDITOR", editor.to_str().unwrap()), ("GIT_EDITOR", "true")]).status.success());
    let user_bytes = b"USER IGNORED CONTENT AT STOP\n";
    f.write("victim.txt", user_bytes);
    let session = f.session(); let recovery = f.recovery();
    let state = operation_flow::state(&session, &f.data()).unwrap().unwrap();
    assert!(state.edit_stop);
    let outcome = operation_flow::execute(&session, &recovery, &f.autostash(), &f.archives(), &OperationActionRequest {
        kind: state.kind, metadata_version: state.metadata_version, expected_state: state.confirmation_state, action: OperationAction::Continue,
    }).unwrap();
    assert!(outcome.completed);
    assert_eq!(f.read("victim.txt"), b"later committed bytes\n");
    let (record, _) = recovery.read(outcome.recovery_id.as_deref().unwrap()).unwrap();
    assert_eq!(record.source, RecoverySource::OperationContinue);
    assert_eq!(record.before.iter().find(|file| file.path == "victim.txt").unwrap().content.as_deref(), Some(user_bytes.as_slice()));
}

#[test]
fn external_merge_protects_git_backup_companion_and_fails_closed_if_it_cannot_be_saved() {
    for block_recovery in [false, true] {
        let f = Fixture::new();
        f.write("conflict.txt", b"base\n"); f.commit_all("base");
        f.branch("feature"); f.write("conflict.txt", b"feature\n"); f.commit_all("feature");
        f.checkout("main"); f.write("conflict.txt", b"main\n"); f.commit_all("main");
        assert!(!f.try_git(&["merge", "feature"]).status.success());
        f.git(&["config", "merge.tool", "m6backup"]);
        f.git(&["config", "mergetool.m6backup.cmd", "printf 'resolved\\n' > \"$MERGED\""]);
        f.git(&["config", "mergetool.m6backup.trustExitCode", "true"]);
        f.write("conflict.txt.orig", b"original user backup\n");
        let before = f.read("conflict.txt"); let session = f.session(); let store = f.recovery();
        if block_recovery { fs::create_dir_all(f.data()).unwrap(); fs::write(f.data().join("recovery"), b"owned blocker").unwrap(); }
        let result = external_tools::launch(&session, &store, &ExternalToolRequest {
            expected_context: external_tools::confirmation_context(&session).unwrap(), kind: ExternalToolKind::Merge,
            path: "conflict.txt".into(), tool: None, changes_kind: None,
            expected_fingerprint: Some(revia_lib::merge::fingerprint(&session.git, "conflict.txt").unwrap()), revision: None,
        }, None);
        if block_recovery {
            assert!(result.is_err()); assert_eq!(f.read("conflict.txt"), before); assert_eq!(f.read("conflict.txt.orig"), b"original user backup\n");
        } else {
            let outcome = result.unwrap(); assert_eq!(outcome.exit_code, Some(0)); assert!(!outcome.still_conflicted);
            let (record, _) = store.read(outcome.recovery_id.as_deref().unwrap()).unwrap();
            assert_eq!(record.before.iter().find(|file| file.path == "conflict.txt.orig").unwrap().content.as_deref(), Some(b"original user backup\n".as_slice()));
            assert_eq!(f.read("conflict.txt.orig"), before); assert_eq!(f.read("conflict.txt"), b"resolved\n");
            assert!(!f.try_git(&["config", "--get", "mergetool.writeToTemp"]).status.success(), "process override must not persist in Git config");
        }
    }
}

#[test]
fn missing_autostash_keeps_the_association_and_real_operation_readable() {
    let f = Fixture::new();
    f.write("file", b"base\n"); f.write("local", b"local base\n"); f.commit_all("base");
    f.branch("other"); f.write("file", b"other\n"); f.commit_all("other");
    f.checkout("main"); f.write("file", b"main\n"); f.commit_all("main");
    f.write("local", b"saved local data\n"); let session = f.session(); let store = f.autostash();
    let link = store.save_before(&session, &f.recovery(), "rebase").unwrap().unwrap();
    assert!(!f.try_git(&["merge", "other"]).status.success()); f.git(&["stash", "drop", "stash@{0}"]);
    let pending = store.pending(&session).unwrap().unwrap();
    assert_eq!(pending.link.id, link.id); assert!(pending.confirmation_state.is_none()); assert!(pending.error.is_some());
    let operation = operation_flow::state(&session, &f.data()).unwrap().unwrap();
    assert_eq!(operation.kind, OperationKind::Merge); assert_eq!(operation.conflict_count, 1);
    assert_eq!(store.for_worktree(&session).unwrap().unwrap().stash_oid, link.stash_oid);
    if let Some(path) = std::env::var_os("REVIA_M6_ERROR_FIXTURE") {
        fs::write(format!("{}.pending.json", PathBuf::from(path).display()), serde_json::to_vec_pretty(&serde_json::json!({"pending":pending,"operation":operation})).unwrap()).unwrap();
    }
}

#[test]
fn manual_stash_save_partial_cleanup_failure_keeps_stashes_and_requires_verification() {
    use revia_lib::error::ErrorKind;
    let mut cases = Vec::new();
    for keep_index in [false, true] {
        let mut f = Fixture::new();
        if std::env::var_os("REVIA_M6_ERROR_FIXTURE").is_some() { f.temp.disable_cleanup(true); }
        f.vars.insert("LC_ALL".into(), "C".into());
        println!("partial stash save fixture={} keep_index={keep_index}", f.temp.path().display());
        f.write("a.txt", b"base a\n"); f.write("b.txt", b"base b\n");
        f.write(".gitattributes", b"b.txt filter=m6fail\n"); let head = f.commit_all("base");
        f.write("a.txt", b"earlier saved a\n"); f.git(&["stash", "push", "--quiet", "-m", "earlier stash"]);
        let earlier = f.oid("refs/stash");
        f.git(&["config", "filter.m6fail.clean", "cat"]);
        f.git(&["config", "filter.m6fail.smudge", "false"]);
        f.git(&["config", "filter.m6fail.required", "true"]);
        f.write("a.txt", b"changed a\n"); f.write("b.txt", b"changed b\n");
        let session = f.session();
        let request = StashSaveRequest { message: "partial cleanup".into(), keep_index,
            expected_state: stash::preflight(&session, None).unwrap() };
        let error = stash::save(&session, &request).unwrap_err();
        let payload = error.payload(); let entries = stash::list(&session.git).unwrap();
        let records = session.git.log.snapshot();
        let pushes: Vec<_> = records.iter().filter(|record| record.args.starts_with(&["stash".into(), "push".into()])).collect();
        assert_eq!(pushes.len(), 1); assert_ne!(pushes[0].exit_code, 0); assert!(!pushes[0].timed_out);
        assert_eq!(error.kind, ErrorKind::NeedsVerification);
        assert_eq!(error.code, Some("errors.recovery.writeResultNeedsVerification"));
        let cause = payload.cause.as_ref().unwrap();
        assert_eq!(cause.code.as_deref(), Some("errors.stash.failedSaveStash"));
        assert!(cause.params.as_ref().unwrap()["detail"].contains("smudge filter m6fail failed"));
        assert_eq!(entries.len(), 2); assert_eq!(entries[1].oid, earlier);
        assert_eq!(f.head(), head); assert_eq!(f.read("a.txt"), b"base a\n"); assert!(!f.repo.join("b.txt").exists());
        let saved = &entries[0].oid;
        assert_eq!(f.git(&["show", &format!("{saved}:a.txt")]), b"changed a\n");
        assert_eq!(f.git(&["show", &format!("{saved}:b.txt")]), b"changed b\n");
        // Even an explicit repeat of the old request cannot replay this write.
        assert_eq!(stash::save(&session, &request).unwrap_err().kind, ErrorKind::Rejected);
        assert_eq!(stash::list(&session.git).unwrap(), entries);
        assert_eq!(session.git.log.snapshot().iter().filter(|record| record.args.starts_with(&["stash".into(), "push".into()])).count(), 1);
        cases.push(serde_json::json!({ "keepIndex": keep_index, "root": f.temp.path(), "error": payload,
            "entries": entries, "commands": records, "head": head }));
    }
    if let Ok(path) = std::env::var("REVIA_M6_ERROR_FIXTURE") {
        fs::write(format!("{path}.stash-save-partial.json"), serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
    }
}

#[test]
fn manual_stash_save_receipt_follows_its_content_and_refuses_ambiguous_identity() {
    for duplicate in [false, true] {
        let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
        f.write("file", b"own requested content\n"); let session = f.session();
        let result = stash::save_with_after_write(&session, &StashSaveRequest {
            message: "own requested save".into(), keep_index: false, expected_state: stash::preflight(&session, None).unwrap(),
        }, || {
            if duplicate {
                // update-ref elides an identical OID. Use a distinct commit
                // with the same payload/base/index, as another writer can.
                let tree = f.oid("refs/stash^{tree}"); let base = f.oid("refs/stash^1"); let index = f.oid("refs/stash^2");
                let oid = String::from_utf8(f.git(&["commit-tree", &tree, "-p", &base, "-p", &index, "-m", "external copy"])).unwrap();
                f.git(&["stash", "store", "-m", "external copy of same data", oid.trim()]);
            } else {
                f.write("file", b"external content\n"); f.git(&["stash", "push", "--quiet", "-m", "external save"]);
            }
            Ok(())
        });
        let entries = stash::list(&session.git).unwrap(); assert_eq!(entries.len(), 2);
        if duplicate { assert_eq!(result.unwrap_err().kind, revia_lib::error::ErrorKind::NeedsVerification); }
        else { let returned = result.unwrap(); assert!(returned.message.ends_with("own requested save")); assert_eq!(returned.oid, entries[1].oid); assert_ne!(returned.oid, entries[0].oid); }
        assert_eq!(f.read("file"), b"base\n");
    }
}

#[cfg(unix)]
#[test]
fn stash_untracked_tree_does_not_claim_untouched_symlinks() {
    let f = Fixture::new(); f.write("file.txt", b"base\n");
    std::os::unix::fs::symlink("file.txt", f.repo.join("link")).unwrap(); f.commit_all("base");
    f.write("file.txt", b"saved text\n"); f.write("new.txt", b"saved untracked\n");
    f.git(&["stash", "push", "-u", "-m", "saved"]); let oid = f.oid("refs/stash"); let session = f.session();
    let result = stash::apply(&session, &f.recovery(), &f.archives(), &StashApplyRequest { entry_key: None,
        oid: oid.clone(), restore_index: false, delete_after: false, new_branch: None, expected_state: stash::preflight(&session, Some(&oid)).unwrap(),
    }).unwrap();
    assert!(result.applied); assert_eq!(f.read("new.txt"), b"saved untracked\n"); assert_eq!(f.read("file.txt"), b"saved text\n");
    assert_eq!(fs::read_link(f.repo.join("link")).unwrap(), PathBuf::from("file.txt"));
}

#[cfg(unix)]
#[test]
fn manual_stash_save_rejects_changed_tracked_readonly_paths() {
    use revia_lib::git::status::ReadonlyReason;
    for keep_index in [false, true] {
        for change in ["unstaged", "staged", "deleted", "regular"] {
            let f = Fixture::new();
            f.write("a", b"a\n"); f.write("b", b"b\n"); f.write("file", b"base\n");
            std::os::unix::fs::symlink("a", f.repo.join("link")).unwrap();
            let head = f.commit_all("base"); f.write("file", b"local\n");
            fs::remove_file(f.repo.join("link")).unwrap();
            match change {
                "deleted" => {},
                "regular" => f.write("link", b"replacement\n"),
                _ => std::os::unix::fs::symlink("b", f.repo.join("link")).unwrap(),
            }
            if change == "staged" { f.git(&["add", "--", "link"]); }
            let session = f.session(); let index = f.index();
            assert!(session.refresh().unwrap().entries.iter()
                .any(|entry| entry.path == "link" && entry.readonly_reason == Some(ReadonlyReason::Symlink)));
            let error = stash::save(&session, &StashSaveRequest { message: "readonly".into(), keep_index,
                expected_state: stash::preflight(&session, None).unwrap() }).unwrap_err();
            println!("readonly save fixture={} change={change} keep_index={keep_index} error={error:?} commands={:?}", f.temp.path().display(), session.git.log.snapshot());
            assert_eq!(error.kind, revia_lib::error::ErrorKind::Rejected);
            assert_eq!(error.code, Some("errors.readonly.symlink"));
            assert!(!session.git.log.snapshot().iter().any(|record| record.args.first().is_some_and(|arg| arg == "stash")
                && record.args.get(1).is_some_and(|arg| arg == "create" || arg == "push")));
            assert_eq!(f.head(), head); assert_eq!(f.index(), index); assert_eq!(f.read("file"), b"local\n");
            assert!(stash::list(&session.git).unwrap().is_empty());
            match change {
                "deleted" => assert!(!f.repo.join("link").exists()),
                "regular" => assert_eq!(f.read("link"), b"replacement\n"),
                _ => assert_eq!(fs::read_link(f.repo.join("link")).unwrap(), PathBuf::from("b")),
            }
            assert_eq!(f.read("a"), b"a\n"); assert_eq!(f.read("b"), b"b\n");
        }
    }
}

#[cfg(unix)]
#[test]
fn manual_stash_guard_preserves_untouched_symlinks_and_collection_clear() {
    for keep_index in [false, true] {
        let f = Fixture::new(); f.write("file", b"base\n");
        std::os::unix::fs::symlink("file", f.repo.join("tracked-link")).unwrap(); f.commit_all("base");
        std::os::unix::fs::symlink("missing", f.repo.join("untracked-link")).unwrap();
        f.write("file", b"saved\n"); let session = f.session();
        let entry = stash::save(&session, &StashSaveRequest { message: "regular only".into(), keep_index,
            expected_state: stash::preflight(&session, None).unwrap() }).unwrap();
        assert_eq!(f.read("file"), b"base\n");
        assert_eq!(fs::read_link(f.repo.join("tracked-link")).unwrap(), PathBuf::from("file"));
        assert_eq!(fs::read_link(f.repo.join("untracked-link")).unwrap(), PathBuf::from("missing"));
        // Clear writes the stash collection, not these unrelated dirty files.
        fs::remove_file(f.repo.join("tracked-link")).unwrap();
        std::os::unix::fs::symlink("missing", f.repo.join("tracked-link")).unwrap();
        let index = f.index(); let expected = stash::collection_preflight(&session).unwrap();
        let result = stash::clear_confirmed(&session, &f.archives(), &[entry.oid], &expected).unwrap();
        assert_eq!(result.deleted, 1); assert!(stash::list(&session.git).unwrap().is_empty());
        assert_eq!(f.index(), index);
        assert_eq!(fs::read_link(f.repo.join("tracked-link")).unwrap(), PathBuf::from("missing"));
        assert_eq!(fs::read_link(f.repo.join("untracked-link")).unwrap(), PathBuf::from("missing"));
    }
}

#[cfg(unix)]
#[test]
fn manual_stash_save_rejects_changed_gitlinks_and_non_utf8_paths() {
    let child = Fixture::new(); child.write("child", b"base\n"); child.commit_all("base");
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    f.git(&["-c", "protocol.file.allow=always", "submodule", "add", "--quiet", child.repo.to_str().unwrap(), "module"]);
    f.commit_all("submodule"); child.write("child", b"next\n"); let next = child.commit_all("next");
    f.git(&["update-index", "--cacheinfo", "160000", &next, "module"]);
    f.write("file", b"local\n"); let session = f.session(); let index = f.index(); let head = f.head();
    let error = stash::save(&session, &StashSaveRequest { message: "gitlink".into(), keep_index: false,
        expected_state: stash::preflight(&session, None).unwrap() }).unwrap_err();
    assert_eq!(error.code, Some("errors.readonly.submodule"));
    assert_eq!(f.index(), index); assert_eq!(f.head(), head); assert_eq!(f.read("file"), b"local\n");
    assert_eq!(f.read("module/child"), b"base\n"); assert!(stash::list(&session.git).unwrap().is_empty());
    // macOS cannot create an invalid UTF8 filename. Import its real Git index
    // entry instead, as in the existing non-UTF8 history regression.
    let f = Fixture::new(); f.write("bad-\u{fffd}", b"UTF8 alias\n"); f.commit_all("base");
    let session = f.session(); let blob = f.oid("HEAD:bad-\u{fffd}");
    let mut raw = format!("100644 {blob}\t").into_bytes(); raw.extend(b"bad-\xff\0");
    assert_eq!(session.git.run_with_input(&["update-index", "-z", "--index-info"], raw).unwrap().exit_code, 0);
    f.git(&["commit", "-m", "import non UTF8 path"]);
    let index = f.index(); let head = f.head();
    let error = stash::save(&session, &StashSaveRequest { message: "non UTF8".into(), keep_index: false,
        expected_state: "unavailable".into() }).unwrap_err();
    assert_eq!(error.code, Some("errors.readonly.nonUtf8Path"));
    assert_eq!(f.read("bad-\u{fffd}"), b"UTF8 alias\n"); assert_eq!(f.index(), index); assert_eq!(f.head(), head);
    assert!(!session.git.log.snapshot().iter().any(|record| record.args.first().is_some_and(|arg| arg == "stash")
        && record.args.get(1).is_some_and(|arg| arg == "create" || arg == "push")));
}

#[test]
fn explicit_autostash_restore_rechecks_confirmation_after_recovery_and_allows_fresh_retry() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use revia_lib::autostash::AutostashRestoration;
    for refresh_at in [0, 1] {
        let f = Fixture::new(); f.write("file", b"base\n"); let head = f.commit_all("base");
        f.write("file", b"saved local\n"); let session = f.session(); let auto = f.autostash();
        auto.save_before(&session, &f.recovery(), "cherry-pick").unwrap().unwrap();
        f.write("unrelated", b"keep\n");
        let shown = auto.pending(&session).unwrap().unwrap(); let before = session.confirmation_version();
        let index = f.index(); let entries = stash::list(&session.git).unwrap();
        let calls = Arc::new(AtomicUsize::new(0)); let clock_calls = calls.clone(); let target = session.clone();
        let recovery = RecoveryStore::with_clock(f.data(), Arc::new(move || {
            if clock_calls.fetch_add(1, Ordering::SeqCst) == refresh_at { target.refresh().unwrap(); }
            1_798_000_000_000
        }));
        let error = auto.restore_confirmed(&session, &recovery, &f.archives(), &shown.link,
            shown.confirmation_state.as_deref().unwrap()).unwrap_err();
        println!("restore fixture={} refresh_at={refresh_at} before={before:?} after={:?} error={error:?} commands={:?}",
            f.temp.path().display(), session.confirmation_version(), session.git.log.snapshot());
        assert!(calls.load(Ordering::SeqCst) >= 2); assert_ne!(session.confirmation_version(), before);
        assert_eq!(error.kind, revia_lib::error::ErrorKind::Rejected);
        assert_eq!(error.code, Some("errors.rewrite.stateChanged"));
        assert_eq!(f.read("file"), b"base\n"); assert_eq!(f.read("unrelated"), b"keep\n");
        assert_eq!(f.index(), index); assert_eq!(f.head(), head); assert_eq!(stash::list(&session.git).unwrap(), entries);
        let retained = auto.for_worktree(&session).unwrap().unwrap();
        assert_eq!(retained.restoration, AutostashRestoration::Pending);
        assert_eq!(serde_json::to_value(&retained).unwrap(), serde_json::to_value(&shown.link).unwrap());
        assert!(!session.git.log.snapshot().iter().any(|record| record.args.first().is_some_and(|arg| arg == "stash")
            && record.args.get(1).is_some_and(|arg| arg == "apply")));
        assert!(recovery.list(None).unwrap().iter().any(|record| record.target == "autostash restore"));
        let fresh = auto.pending(&session).unwrap().unwrap();
        let result = auto.restore_confirmed(&session, &f.recovery(), &f.archives(), &fresh.link,
            fresh.confirmation_state.as_deref().unwrap()).unwrap().unwrap();
        assert!(result.restored && result.removed); assert!(!result.conflicted);
        assert_eq!(f.read("file"), b"saved local\n"); assert_eq!(f.read("unrelated"), b"keep\n");
        assert!(auto.for_worktree(&session).unwrap().is_none()); assert!(stash::list(&session.git).unwrap().is_empty());
    }
}

#[cfg(unix)]
#[test]
fn selected_history_stops_before_accepting_a_new_ignored_file_between_steps() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(); f.write(".gitignore", b"victim.txt\n"); f.write("base", b"base\n"); let base = f.commit_all("base");
    f.branch("source"); f.write("a", b"a\n"); let a = f.commit_all("A");
    f.write("victim.txt", b"source data\n"); f.git(&["add", "-f", "victim.txt"]); f.git(&["commit", "--quiet", "-m", "B"]); let b = f.head(); f.checkout("main");
    let hook = f.repo.join(".git/hooks/reference-transaction");
    fs::write(&hook, b"#!/bin/sh\n[ \"$1\" = committed ] || exit 0\n[ ! -f .git/m6-fired ] || exit 0\nwhile read old new ref; do\n if [ \"$ref\" = refs/heads/main ]; then\n  touch .git/m6-fired\n  i=0\n  while [ ! -f .git/m6-release ] && [ \"$i\" -lt 100 ]; do sleep 0.1; i=$((i+1)); done\n fi\ndone\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
    let root = f.repo.clone(); let writer = std::thread::spawn(move || {
        let limit = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !root.join(".git/m6-fired").exists() { assert!(std::time::Instant::now() < limit); std::thread::sleep(std::time::Duration::from_millis(5)); }
        fs::write(root.join("victim.txt"), b"FOREIGN USER CONTENT\n").unwrap(); fs::write(root.join(".git/m6-release"), b"release").unwrap();
    });
    let session = f.session(); let p = rewrite::preflight(&session, &[a.clone(), b.clone()]).unwrap();
    let result = rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &CherryPickRequest {
        oids: vec![a.clone(), b.clone()], expected_head: base, expected_context: p.context_version, expected_state: Some(p.overwrite_fingerprint), dirty_strategy: None, keep_empty: false, include_source: false,
    }).unwrap(); writer.join().unwrap();
    assert!(!result.completed); assert_eq!(result.applied, vec![a]); assert_eq!(result.pending, vec![b]);
    assert_eq!(result.error.unwrap().code.as_deref(), Some("errors.rewrite.sequenceFilesChanged"));
    assert_eq!(f.read("victim.txt"), b"FOREIGN USER CONTENT\n");
    let state = operation_flow::state(&session, &f.data()).unwrap().unwrap(); assert!(state.continuation_only); assert!(!state.needs_verification);
    // Only an explicit new Continue accepts this now-visible file state.
    let outcome = operation_flow::execute(&session, &f.recovery(), &f.autostash(), &f.archives(), &OperationActionRequest {
        kind: state.kind, metadata_version: state.metadata_version, expected_state: state.confirmation_state, action: OperationAction::Continue,
    }).unwrap(); assert!(outcome.completed); assert_eq!(f.read("victim.txt"), b"source data\n");
    assert!(f.recovery().list(Some(&f.identity(&session))).unwrap().iter().any(|summary| f.recovery().read(&summary.id).unwrap().0.before.iter().any(|saved| saved.content.as_deref() == Some(b"FOREIGN USER CONTENT\n".as_slice()))));
}

#[test]
fn external_diff_accepts_a_deleted_historical_path_without_a_current_file() {
    let f = Fixture::new(); f.write("中文 deleted.txt", b"before delete\n"); f.commit_all("base");
    f.git(&["rm", "中文 deleted.txt"]); let revision = f.commit_all("delete");
    f.git(&["config", "diff.tool", "m6delete"]); f.git(&["config", "difftool.m6delete.cmd", "true"]);
    let session = f.session(); let outcome = external_tools::launch(&session, &f.recovery(), &ExternalToolRequest {
        expected_context: external_tools::confirmation_context(&session).unwrap(), kind: ExternalToolKind::Diff, path: "中文 deleted.txt".into(),
        tool: None, revision: Some(revision), changes_kind: None, expected_fingerprint: None,
    }, None).unwrap(); assert_eq!(outcome.exit_code, Some(0)); assert!(!f.repo.join("中文 deleted.txt").exists());
}

#[cfg(unix)]
#[test]
fn stash_branch_switch_partial_failure_retains_known_boundary_and_uncertainty() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); f.write("file", b"saved\n"); f.git(&["stash", "push", "-m", "saved"]);
    let oid = f.oid("refs/stash"); let hook = f.repo.join(".git/hooks/post-checkout");
    fs::write(&hook, b"#!/bin/sh\ngit stash drop --quiet stash@{0}\n").unwrap(); fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
    let session = f.session(); let result = stash::apply(&session, &f.recovery(), &f.archives(), &StashApplyRequest { entry_key: None,
        oid: oid.clone(), restore_index: false, delete_after: false, new_branch: Some("partial-applied".into()), expected_state: stash::preflight(&session, Some(&oid)).unwrap(),
    }).unwrap();
    assert!(!result.applied); assert_eq!(result.branch.as_deref(), Some("partial-applied")); assert!(result.recovery_id.is_some());
    let error = result.error.as_ref().unwrap(); assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification); assert_eq!(error.code.as_deref(), Some("errors.stash.branchCreatedApplyIncomplete"));
    assert_eq!(f.read("file"), b"base\n"); assert_eq!(String::from_utf8(f.git(&["symbolic-ref", "--short", "HEAD"])).unwrap().trim(), "partial-applied");
    if let Some(path) = std::env::var_os("REVIA_M6_ERROR_FIXTURE") { fs::write(format!("{}.branch.json", PathBuf::from(path).display()), serde_json::to_vec_pretty(&result).unwrap()).unwrap(); }
}

#[test]
#[cfg(target_os = "macos")]
fn system_language_uses_native_preferences_under_neutral_process_locale() {
    const CHILD: &str = "REVIA_NATIVE_LANGUAGE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let output = Command::new("/usr/bin/defaults").args(["read", "-g", "AppleLanguages"]).output().unwrap();
        assert!(output.status.success(), "{:?}", output.stderr);
        let raw = String::from_utf8(output.stdout).unwrap();
        let primary = raw.lines().map(str::trim).find(|line| !line.is_empty() && *line != "(" && *line != ")")
            .map(|line| line.trim_matches(['"', ',', ' '])).unwrap_or("");
        let expected = revia_lib::settings::language_from_locale(primary).unwrap_or("en");
        let actual = revia_lib::settings::system_language();
        println!("AppleLanguages={raw:?} expected={expected} production={actual} LC_ALL={:?}", std::env::var("LC_ALL"));
        assert_eq!(actual, expected);
        return;
    }
    let f = Fixture::new();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "system_language_uses_native_preferences_under_neutral_process_locale", "--nocapture"])
        .env_clear().envs(&f.vars).env(CHILD, "1")
        .env("LC_ALL", "C.UTF-8").env("LC_MESSAGES", "C").env("LANG", "C.UTF-8").env("LANGUAGE", "POSIX")
        .output().unwrap();
    println!("native language child exit={:?} stdout={} stderr={}", output.status.code(), String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success());
}

#[test]
fn native_watch_install_failure_keeps_a_structured_payload_for_the_status_bar() {
    use revia_lib::watch::{WatchController, WatchRoots};
    let f = Fixture::new(); f.write("file", b"fixture\n"); f.commit_all("base");
    let linked = f.temp.path().join("linked");
    f.git(&["worktree", "add", "-b", "watched", linked.to_str().unwrap()]);
    let session = f.session_at(&linked); let identity = session.require_identity().unwrap();
    let missing = identity.git_dir.display().to_string();
    fs::rename(&identity.git_dir, f.temp.path().join("held-gitdir")).unwrap();
    let controller = WatchController::new();
    let status = controller.activate(WatchRoots { worktree: session.path.clone(), git_dir: identity.git_dir,
        common_dir: Some(identity.common_dir) }, 1, |_| {});
    assert!(status.error.is_some());
    assert_eq!(status.error.as_ref().unwrap().code.as_deref(), Some("errors.watch.unavailable"));
    assert_eq!(status.target.as_deref(), Some(session.path.to_str().unwrap()));
    assert!(status.error.as_ref().unwrap().params.as_ref().unwrap()["detail"].contains(&missing));
    println!("actual watch status={}", serde_json::to_string_pretty(&status).unwrap());
    if let Ok(path) = std::env::var("REVIA_M6_ERROR_FIXTURE") {
        fs::write(format!("{path}.watch.json"), serde_json::to_vec_pretty(&status).unwrap()).unwrap();
    }
}

#[test]
fn remote_tracking_changes_invalidate_all_reset_modes_until_the_warning_is_reconfirmed() {
    for mode in [ResetMode::Soft, ResetMode::Mixed, ResetMode::Hard, ResetMode::Keep] {
        let f = Fixture::new(); f.write("file", b"base\n"); let base = f.commit_all("base");
        f.write("file", b"tip\n"); let tip = f.commit_all("tip");
        if mode != ResetMode::Soft { f.git(&["update-ref", "refs/remotes/local/main", &base]); }
        let session = f.session(); let shown = rewrite::reset_preflight(&session, &base).unwrap();
        assert!(!shown.published); let index = f.index();
        f.git(&["update-ref", "refs/remotes/local/main", &tip]);
        let changed = rewrite::reset_preflight(&session, &base).unwrap(); assert!(changed.published);
        assert_ne!(shown.context_version, changed.context_version);
        assert_ne!(shown.reset_fingerprint, changed.reset_fingerprint);
        let request = ResetRequest { target: base.clone(), mode, expected_head: tip.clone(),
            expected_context: shown.context_version, expected_state: Some(shown.reset_fingerprint) };
        let error = rewrite::reset(&session, &f.recovery(), &f.autostash(), &f.archives(), &request).unwrap_err();
        assert_eq!(error.kind, revia_lib::error::ErrorKind::Rejected);
        assert_eq!(f.head(), tip); assert_eq!(f.index(), index); assert_eq!(f.read("file"), b"tip\n");
        assert!(f.recovery().list(None).unwrap().is_empty());
        if let Ok(path) = std::env::var("REVIA_M6_ERROR_FIXTURE") {
            fs::write(format!("{path}.publication.json"), serde_json::to_vec_pretty(&error.payload()).unwrap()).unwrap();
        }
        println!("reset publication mode={mode:?} rejected={error:?} commands={}", serde_json::to_string_pretty(&session.git.log.snapshot()).unwrap());
        let confirmed = ResetRequest { expected_context: changed.context_version,
            expected_state: Some(changed.reset_fingerprint), ..request };
        assert!(rewrite::reset(&session, &f.recovery(), &f.autostash(), &f.archives(), &confirmed).unwrap().completed);
        assert_eq!(f.head(), base);
        assert_eq!(f.read("file"), if matches!(mode, ResetMode::Hard | ResetMode::Keep) { b"base\n".as_slice() } else { b"tip\n".as_slice() });
    }
}

#[test]
fn publication_changes_expire_cherry_pick_revert_reword_and_interactive_confirmations() {
    for action in ["cherry-pick", "revert", "reword", "interactive"] {
        let f = Fixture::new(); f.write("file", b"base\n"); let base = f.commit_all("base");
        f.write("file", b"tip\n"); let tip = f.commit_all("tip"); let session = f.session();
        let shown = rewrite::preflight(&session, &[tip.clone()]).unwrap(); assert!(!shown.published);
        let index = f.index(); f.git(&["update-ref", "refs/remotes/local/main", &tip]);
        let result = match action {
            "cherry-pick" => rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &CherryPickRequest {
                oids: vec![tip.clone()], expected_head: tip.clone(), expected_context: shown.context_version,
                expected_state: Some(shown.overwrite_fingerprint), dirty_strategy: None, keep_empty: false, include_source: true,
            }),
            "revert" => rewrite::revert(&session, &f.recovery(), &f.autostash(), &f.archives(), &RevertRequest {
                oids: vec![tip.clone()], expected_head: tip.clone(), expected_context: shown.context_version,
                expected_state: Some(shown.overwrite_fingerprint), dirty_strategy: None,
            }),
            "reword" => rewrite::reword(&session, &f.recovery(), &f.autostash(), &f.archives(), &f.data(), &tip,
                &tip, &shown.context_version, &shown.overwrite_fingerprint, "new message"),
            _ => rewrite::interactive(&session, &f.recovery(), &f.autostash(), &f.archives(), &f.data(), &InteractiveRebaseRequest {
                upstream: base, expected_head: tip.clone(), expected_context: shown.context_version,
                expected_state: shown.overwrite_fingerprint, entries: vec![RebasePlanEntry { oid: tip.clone(), action: RebaseAction::Edit, message: None }],
            }),
        };
        assert_eq!(result.unwrap_err().kind, revia_lib::error::ErrorKind::Rejected);
        assert_eq!(f.head(), tip); assert_eq!(f.index(), index); assert_eq!(f.read("file"), b"tip\n");
        assert!(f.recovery().list(None).unwrap().is_empty()); assert!(stash::list(&session.git).unwrap().is_empty());
        assert!(!session.git.log.snapshot().iter().any(|command| command.args.iter().any(|arg| matches!(arg.as_str(), "cherry-pick" | "revert" | "rebase"))));
    }
}

#[cfg(unix)]
#[test]
fn publication_change_during_autostash_keeps_the_save_and_stops_the_history_write() {
    for action in ["interactive", "reword"] {
        let f = Fixture::new(); f.write("file", b"base\n"); let base = f.commit_all("base");
        f.write("file", b"tip\n"); let tip = f.commit_all("tip"); f.write("file", b"saved local\n");
        install_m6_hook(&f, "reference-transaction", "#!/bin/sh\n[ \"$1\" = committed ] || exit 0\nwhile read old new ref; do\n if [ \"$ref\" = refs/stash ]; then git update-ref refs/remotes/local/main HEAD || exit 1; fi\ndone\n");
        let session = f.session(); let shown = rewrite::preflight(&session, &[tip.clone()]).unwrap(); assert!(!shown.published);
        let auto = f.autostash(); let archive = f.archives();
        let result = if action == "interactive" {
            rewrite::interactive(&session, &f.recovery(), &auto, &archive, &f.data(), &InteractiveRebaseRequest {
                upstream: base, expected_head: tip.clone(), expected_context: shown.context_version,
                expected_state: shown.overwrite_fingerprint, entries: vec![RebasePlanEntry { oid: tip.clone(), action: RebaseAction::Edit, message: None }],
            })
        } else {
            rewrite::reword(&session, &f.recovery(), &auto, &archive, &f.data(), &tip,
                &tip, &shown.context_version, &shown.overwrite_fingerprint, "new message")
        };
        assert_eq!(result.unwrap_err().kind, revia_lib::error::ErrorKind::NeedsVerification);
        assert_eq!(f.head(), tip); assert_eq!(f.oid("refs/remotes/local/main"), tip);
        assert!(operation::detect(&session.git, 0).unwrap().is_none());
        assert!(!session.git.log.snapshot().iter().any(|command| command.args.iter().any(|arg| arg == "rebase")));
        let link = auto.for_worktree(&session).unwrap().unwrap();
        assert_eq!(stash::list(&session.git).unwrap()[0].oid, link.stash_oid);
        assert_eq!(f.git(&["show", &format!("{}:file", link.stash_oid)]), b"saved local\n");
        assert_eq!(link.restoration, revia_lib::autostash::AutostashRestoration::Pending);
    }
}

#[test]
fn reset_publication_warning_only_covers_the_history_that_would_be_removed() {
    let f = Fixture::new(); f.write("file", b"base\n"); let base = f.commit_all("base"); f.write("file", b"published\n"); let published = f.commit_all("published");
    f.git(&["update-ref", "refs/remotes/local/main", &published]);
    f.write("file", b"unpublished\n"); let head = f.commit_all("unpublished"); let session = f.session();
    assert!(!rewrite::reset_preflight(&session, &published).unwrap().published, "keeping all published commits only removes unpublished history");
    assert!(rewrite::reset_preflight(&session, &base).unwrap().published);
    assert!(!rewrite::reset_preflight(&session, &head).unwrap().published);
    assert_eq!(f.head(), head); assert_eq!(f.read("file"), b"unpublished\n");
}

#[test]
fn all_branch_file_history_preserves_each_live_lineage_and_paginates() {
    let f = Fixture::new();
    let path = "文件 space.txt";
    f.write(path, b"base\n");
    f.git(&["add", "--all"]);
    let commit_at = |message: &str, date: &str| {
        let output = f.command(&["commit", "--quiet", "-m", message],
            &[("GIT_AUTHOR_DATE", date), ("GIT_COMMITTER_DATE", date)]);
        assert!(output.status.success()); f.head()
    };
    let base = commit_at("base", "2026-01-01T00:00:00Z");
    f.git(&["branch", "other"]);
    f.git(&["rm", path]);
    let deleted = commit_at("delete on main", "2026-01-02T00:00:00Z");
    f.checkout("other"); f.write(path, b"other survives\n"); f.git(&["add", "--all"]);
    let modified = commit_at("modify on other", "2026-01-03T00:00:00Z");
    f.checkout("main");
    let session = f.session();
    let first = file_history::history(&session.git, &FileHistoryRequest {
        path: path.into(), all_branches: true, skip: 0, limit: 1, anchor: None,
    }).unwrap();
    assert_eq!(first.entries[0].oid, modified); assert!(first.has_more);
    let next = file_history::history(&session.git, &FileHistoryRequest {
        path: path.into(), all_branches: true, skip: first.next_skip, limit: 10, anchor: Some(first.anchor),
    }).unwrap();
    assert_eq!(next.entries.iter().map(|entry| entry.oid.as_str()).collect::<Vec<_>>(), vec![deleted.as_str(), base.as_str()]);
    assert!(!next.has_more);
    let current = file_history::history(&session.git, &FileHistoryRequest {
        path: path.into(), all_branches: false, skip: 0, limit: 10, anchor: None,
    }).unwrap();
    assert_eq!(current.entries.iter().map(|entry| entry.oid.as_str()).collect::<Vec<_>>(), vec![deleted.as_str(), base.as_str()]);
}

#[test]
fn file_history_invalid_content_has_a_readonly_reason_and_valid_truncation_is_lossless() {
    let f = Fixture::new(); f.write("invalid.txt", b"bad byte: \xff\n"); f.commit_all("invalid content");
    let session = f.session();
    let request = |path: &str| FileHistoryRequest { path: path.into(), all_branches: false, skip: 0, limit: 10, anchor: None };
    let page = file_history::history(&session.git, &request("invalid.txt")).unwrap();
    assert_eq!(page.entries.len(), 1);
    let entry = &page.entries[0];
    assert!(entry.patch.is_empty() && !entry.patch_truncated);
    assert_eq!(entry.patch_error.as_ref().unwrap().code.as_deref(), Some("errors.fileHistory.patchUnavailable"));
    assert_eq!(f.read("invalid.txt"), b"bad byte: \xff\n");
    if let Some(path) = std::env::var_os("REVIA_M6_ERROR_FIXTURE") {
        fs::write(format!("{}.patch.json", PathBuf::from(path).display()), serde_json::to_vec_pretty(entry).unwrap()).unwrap();
    }
    f.write("large.txt", format!("x{}\n", "界".repeat(800_000)).as_bytes()); f.commit_all("large text");
    let page = file_history::history(&session.git, &request("large.txt")).unwrap();
    let entry = &page.entries[0];
    assert!(entry.patch_truncated && entry.patch_error.is_none());
    assert!(!entry.patch.contains('\u{fffd}')); assert!(entry.patch.len() <= 2 * 1024 * 1024);
}

#[cfg(unix)]
fn install_m6_hook(f: &Fixture, name: &str, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    let path = f.repo.join(".git/hooks").join(name);
    fs::write(&path, text).unwrap(); fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
#[cfg(unix)]
fn smart_checkout_coordinates_with_m6_stash_writes_and_archives_only_its_own_entry() {
    use revia_lib::remote_ops::{self, CheckoutTarget, CheckoutStrategy, CheckoutResult};
    let mut f = Fixture::new();
    f.write("one.txt", b"base one\n"); f.write("two.txt", b"base two\n"); f.commit_all("base");
    f.git(&["branch", "target"]);
    let linked = f.temp.path().join("linked");
    f.git(&["worktree", "add", "-b", "other", linked.to_str().unwrap(), "HEAD"]);
    fs::write(linked.join("two.txt"), b"B private\n").unwrap(); f.write("one.txt", b"A private\n");
    let ready = f.temp.path().join("checkout-ready"); let release = f.temp.path().join("checkout-release");
    f.vars.insert("REVIA_TEST_READY".into(), ready.display().to_string());
    f.vars.insert("REVIA_TEST_RELEASE".into(), release.display().to_string());
    install_m6_hook(&f, "post-checkout", "#!/bin/sh\n: > \"$REVIA_TEST_READY\"\ni=0\nwhile [ ! -f \"$REVIA_TEST_RELEASE\" ] && [ \"$i\" -lt 1500 ]; do sleep 0.01; i=$((i + 1)); done\n");
    let a = f.session(); let b = f.session_at(&linked); let store = f.recovery(); let archives = f.archives();
    let a_thread = a.clone(); let archive_thread = archives.clone();
    let checkout = std::thread::spawn(move || remote_ops::checkout(&a_thread, &store,
        &CheckoutTarget::Local { name: "target".into() }, CheckoutStrategy::Smart, &archive_thread));
    let start = std::time::Instant::now();
    while !ready.exists() { assert!(start.elapsed() < std::time::Duration::from_secs(15)); std::thread::sleep(std::time::Duration::from_millis(10)); }
    let expected_state = stash::preflight(&b, None).unwrap();
    let (done, received) = std::sync::mpsc::channel();
    let barrier = Arc::new(std::sync::Barrier::new(2)); let b_barrier = barrier.clone();
    let saving = std::thread::spawn(move || {
        b_barrier.wait();
        let result = stash::save(&b, &StashSaveRequest { message: "B via M6".into(), keep_index: false, expected_state });
        done.send(()).unwrap(); result
    });
    barrier.wait();
    let blocked = received.recv_timeout(std::time::Duration::from_millis(100)).is_err();
    fs::write(&release, b"resume").unwrap();
    let result = checkout.join().unwrap().unwrap(); let saved_b = saving.join().unwrap().unwrap();
    assert!(blocked, "a shared stash write must wait until smart checkout finishes");
    let CheckoutResult::Done(outcome) = result else { panic!("checkout not completed"); };
    assert!(outcome.stash_restored && !outcome.stash_kept);
    assert_eq!(f.read("one.txt"), b"A private\n"); assert_eq!(f.read("two.txt"), b"base two\n");
    assert_eq!(stash::list(&a.git).unwrap().iter().map(|entry| &entry.oid).collect::<Vec<_>>(), vec![&saved_b.oid]);
    let saved = archives.list(Some(&f.identity(&a))).unwrap();
    assert_eq!(saved.len(), 1); assert_ne!(saved[0].stash_oid, saved_b.oid);
}

#[test]
#[cfg(unix)]
fn smart_checkout_ignores_an_external_terminal_stash_and_reports_cleanup_failure() {
    use revia_lib::remote_ops::{self, CheckoutTarget, CheckoutStrategy, CheckoutResult};
    let mut f = Fixture::new();
    f.write("one.txt", b"base one\n"); f.write("two.txt", b"base two\n"); f.commit_all("base"); f.git(&["branch", "target"]);
    let linked = f.temp.path().join("linked");
    f.git(&["worktree", "add", "-b", "other", linked.to_str().unwrap(), "HEAD"]);
    fs::write(linked.join("two.txt"), b"terminal B private\n").unwrap(); f.write("one.txt", b"A private\n");
    f.vars.insert("REVIA_TEST_LINKED".into(), linked.display().to_string());
    install_m6_hook(&f, "post-checkout", "#!/bin/sh\ngit -C \"$REVIA_TEST_LINKED\" stash push --quiet -m 'terminal B'\n");
    let session = f.session(); let archives = f.archives();
    let CheckoutResult::Done(outcome) = remote_ops::checkout(&session, &f.recovery(),
        &CheckoutTarget::Local { name: "target".into() }, CheckoutStrategy::Smart, &archives).unwrap() else { panic!("checkout not completed"); };
    assert!(outcome.stash_restored && !outcome.stash_kept);
    assert_eq!(f.read("one.txt"), b"A private\n"); assert_eq!(f.read("two.txt"), b"base two\n");
    let remaining = stash::list(&session.git).unwrap(); assert_eq!(remaining.len(), 1);
    assert!(remaining[0].message.ends_with("terminal B"));
    assert_eq!(archives.list(Some(&f.identity(&session))).unwrap().len(), 1);

    // A second independent fixture verifies the already-restored/undeleted boundary.
    let g = Fixture::new(); g.write("file", b"base\n"); g.commit_all("base"); g.git(&["branch", "target"]);
    g.write("file", b"local\n"); fs::create_dir_all(g.data()).unwrap();
    fs::write(g.data().join("stash-archive"), b"fixture blocks archive\n").unwrap();
    let s = g.session();
    let error = remote_ops::checkout(&s, &g.recovery(), &CheckoutTarget::Local { name: "target".into() },
        CheckoutStrategy::Smart, &g.archives()).unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
    assert_eq!(g.read("file"), b"local\n"); assert_eq!(s.git.current_branch().as_deref(), Some("target"));
    assert_eq!(stash::list(&s.git).unwrap().len(), 1);
    assert_eq!(error.cause.as_ref().unwrap().code.as_deref(), Some("errors.remoteOps.checkoutRestoredCleanupFailed"));
    if let Some(path) = std::env::var_os("REVIA_M6_ERROR_FIXTURE") {
        fs::write(format!("{}.checkout.json", PathBuf::from(path).display()), serde_json::to_vec_pretty(&error.payload()).unwrap()).unwrap();
    }
}

#[test]
#[cfg(unix)]
fn failed_commit_keeps_the_structured_snapshot_receipt_failure() {
    use revia_lib::commit_flow::CommitOptions;
    let mut f = Fixture::new(); f.write("file.txt", b"base\n"); f.commit_all("base");
    f.write("file.txt", b"staged\n"); f.git(&["add", "file.txt"]); f.write("file.txt", b"unstaged bytes\n");
    f.vars.insert("REVIA_TEST_DATA".into(), f.data().display().to_string());
    install_m6_hook(&f, "pre-commit", "#!/bin/sh\nfor entry in \"$REVIA_TEST_DATA\"/recovery/record-*; do mkdir \"$entry/outcome.pending\"; done\necho 'fixture hook refuses commit' >&2\nexit 1\n");
    let session = f.session(); let drafts = DraftStore::new(f.data());
    let expected = session.commit_context(&drafts).unwrap().expectation;
    let result = session.execute_commit_flow(&drafts, &f.recovery(), f.temp.path().join("hooks-off"), CommitOptions {
        message: "rejected commit".into(), amend: false, skip_hooks: false, expected, amend_confirmation: None,
        client_nonce: "repair10".into(), draft_revision: "".into(),
    }, "repair10-receipt".into(), Arc::new(|_| {})).unwrap();
    assert!(!result.success && result.needs_verification && !result.draft_cleared);
    let reason = result.verification_note.as_ref().unwrap();
    assert_eq!(reason.code.as_deref(), Some("errors.commitFlow.snapshotFinishFailed")); assert!(reason.cause.is_some());
    assert_eq!(f.read("file.txt"), b"unstaged bytes\n");
    if let Some(path) = std::env::var_os("REVIA_M6_ERROR_FIXTURE") {
        fs::write(format!("{}.snapshot.json", PathBuf::from(path).display()), serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    }
}

#[test]
#[cfg(unix)]
fn autostash_association_receipt_failure_does_not_restore_without_protection() {
    let mut f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); f.write("file", b"saved local\n");
    f.vars.insert("REVIA_TEST_DATA".into(), f.data().display().to_string());
    install_m6_hook(&f, "reference-transaction", "#!/bin/sh\n[ \"$1\" = committed ] || exit 0\nwhile read old new ref; do\n if [ \"$ref\" = refs/stash ]; then mkdir \"$REVIA_TEST_DATA/autostash/links.pending\"; fi\ndone\n");
    let session = f.session(); let auto = f.autostash();
    let error = auto.save_before(&session, &f.recovery(), "rebase").unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
    assert_eq!(f.read("file"), b"base\n");
    let stashes = stash::list(&session.git).unwrap(); assert_eq!(stashes.len(), 1);
    let shown = auto.for_worktree(&session).unwrap().unwrap();
    assert_eq!(shown.restoration, revia_lib::autostash::AutostashRestoration::Saving);
    assert!(auto.restore_if_ready(&session, &f.recovery(), &f.archives()).is_err());
    assert!(!session.git.log.snapshot().iter().any(|entry| entry.args.windows(2).any(|args| args == ["stash", "apply"])));
    if let Some(path) = std::env::var_os("REVIA_M6_ERROR_FIXTURE") {
        fs::write(format!("{}.association.json", PathBuf::from(path).display()), serde_json::to_vec_pretty(&error.payload()).unwrap()).unwrap();
    }
}

#[test]
#[cfg(unix)]
fn smart_checkout_preserves_confirmation_across_save_switch_and_restore_boundaries() {
    use revia_lib::remote_ops::{self, CheckoutTarget, CheckoutStrategy};
    for phase in ["after-save", "after-switch-untracked", "after-switch-deleted"] {
        let mut f = Fixture::new();
        f.write("local.txt", b"base\n");
        if phase == "after-save" { f.write(".gitignore", b"victim.txt\n"); }
        if phase == "after-switch-deleted" {
            f.write(".gitignore", b"victim.txt\n"); f.write("victim.txt", b"tracked base\n");
        }
        f.commit_all("base");
        // The deleted-file fixture tracks an otherwise ignored path explicitly.
        if phase == "after-switch-deleted" {
            f.git(&["add", "--force", "victim.txt"]); f.commit_all("track victim");
        }
        f.branch("target");
        match phase {
            "after-save" => { f.write("victim.txt", b"target\n"); f.git(&["add", "--force", "victim.txt"]); }
            "after-switch-untracked" => f.write(".gitignore", b"victim.txt\n"),
            _ => { f.git(&["rm", "victim.txt"]); }
        }
        f.commit_all("target tree"); f.checkout("main"); f.write("local.txt", b"saved local\n");
        if phase != "after-save" { f.write("victim.txt", b"saved victim\n"); }
        f.vars.insert("REVIA_TEST_ROOT".into(), f.repo.display().to_string());
        if phase == "after-save" {
            install_m6_hook(&f, "reference-transaction", "#!/bin/sh\n[ \"$1\" = committed ] || exit 0\nwhile read old new ref; do\n if [ \"$ref\" = refs/stash ]; then printf 'foreign bytes\n' > \"$REVIA_TEST_ROOT/victim.txt\"; fi\ndone\n");
        } else {
            install_m6_hook(&f, "post-checkout", "#!/bin/sh\nprintf 'foreign bytes\n' > \"$REVIA_TEST_ROOT/victim.txt\"\n");
        }
        let session = f.session();
        let error = remote_ops::checkout(&session, &f.recovery(), &CheckoutTarget::Local { name: "target".into() },
            CheckoutStrategy::Smart, &f.archives()).unwrap_err();
        assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification, "{phase}");
        assert_eq!(f.read("victim.txt"), b"foreign bytes\n", "{phase}: outside bytes must not be re-confirmed");
        assert_eq!(session.git.current_branch().as_deref(), Some(if phase == "after-save" { "main" } else { "target" }));
        assert_eq!(stash::list(&session.git).unwrap().len(), 1, "the original saved changes must remain in their stash");
        assert!(!session.git.log.snapshot().iter().any(|entry| entry.args.windows(2).any(|args| args == ["stash", "apply"])));
        assert!(f.archives().list(Some(&f.identity(&session))).unwrap().is_empty());
    }
}

#[test]
fn stash_patch_encoding_is_explicit_for_both_comparisons_and_truncates_on_characters() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base"); f.write("file", b"bad byte: \xff\n");
    f.git(&["stash", "push", "--quiet", "-m", "invalid text"]);
    let session = f.session(); let oid = f.oid("refs/stash"); let original_index = f.index();
    for compare_local in [false, true] {
        let result = stash::inspect(&session.git, &oid, compare_local).unwrap();
        assert!(result.patch.is_empty() && !result.truncated);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.patch_error.as_ref().unwrap().code.as_deref(), Some("errors.stash.patchUnavailable"));
        if !compare_local {
            if let Some(path) = std::env::var_os("REVIA_M6_ERROR_FIXTURE") {
                fs::write(format!("{}.stash-patch.json", PathBuf::from(path).display()), serde_json::to_vec_pretty(&result).unwrap()).unwrap();
            }
        }
    }
    assert_eq!(f.read("file"), b"base\n"); assert_eq!(f.index(), original_index);
    f.write("large.txt", format!("x{}\n", "界".repeat(3_000_000)).as_bytes()); f.git(&["add", "large.txt"]);
    f.git(&["stash", "push", "--quiet", "-m", "large text"]);
    let result = stash::inspect(&session.git, &f.oid("refs/stash"), false).unwrap();
    assert!(result.truncated && result.patch_error.is_none());
    assert!(result.patch.len() <= 8 * 1024 * 1024 && !result.patch.contains('\u{fffd}'));
}

#[test]
#[cfg(unix)]
fn history_selection_does_not_reconfirm_ignored_files_created_during_autostash() {
    for action in ["cherry-pick", "revert"] {
        let mut f = Fixture::new();
        f.write(".gitignore", b"victim.txt\n"); f.write("local.txt", b"base\n"); f.commit_all("base");
        let oid = if action == "cherry-pick" {
            f.branch("source"); f.write("victim.txt", b"SOURCE\n");
            f.git(&["add", "--force", "victim.txt"]); let oid = f.commit_all("add victim");
            f.checkout("main"); oid
        } else {
            f.write("victim.txt", b"SOURCE\n"); f.git(&["add", "--force", "victim.txt"]); f.commit_all("add victim");
            f.git(&["rm", "victim.txt"]); f.commit_all("delete victim")
        };
        let head = f.head(); f.write("local.txt", b"saved local\n"); f.git(&["add", "local.txt"]);
        f.vars.insert("REVIA_TEST_ROOT".into(), f.repo.display().to_string());
        install_m6_hook(&f, "reference-transaction", "#!/bin/sh\n[ \"$1\" = committed ] || exit 0\nwhile read old new ref; do\n if [ \"$ref\" = refs/stash ]; then printf 'FOREIGN\\n' > \"$REVIA_TEST_ROOT/victim.txt\"; fi\ndone\n");
        let session = f.session(); let auto = f.autostash(); let archives = f.archives();
        let checked = rewrite::preflight(&session, &[oid.clone()]).unwrap();
        let result = if action == "cherry-pick" {
            rewrite::cherry_pick(&session, &f.recovery(), &auto, &archives, &CherryPickRequest {
                oids: vec![oid], expected_head: head.clone(), expected_context: checked.context_version,
                expected_state: Some(checked.overwrite_fingerprint), dirty_strategy: Some(DirtyStrategy::Stash),
                keep_empty: false, include_source: false,
            })
        } else {
            rewrite::revert(&session, &f.recovery(), &auto, &archives, &RevertRequest {
                oids: vec![oid], expected_head: head.clone(), expected_context: checked.context_version,
                expected_state: Some(checked.overwrite_fingerprint), dirty_strategy: Some(DirtyStrategy::Stash),
            })
        };
        let error = result.unwrap_err();
        assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification, "{action}");
        assert_eq!(f.read("victim.txt"), b"FOREIGN\n", "{action}"); assert_eq!(f.head(), head);
        let link = auto.for_worktree(&session).unwrap().unwrap();
        assert_eq!(link.restoration, revia_lib::autostash::AutostashRestoration::Pending);
        assert_eq!(stash::list(&session.git).unwrap()[0].oid, link.stash_oid);
        assert_eq!(f.git(&["show", &format!("{}:local.txt", link.stash_oid)]), b"saved local\n");
        assert!(operation::detect(&session.git, 0).unwrap().is_none());
        assert!(!session.git.log.snapshot().iter().any(|entry| entry.args.iter().any(|arg| arg == action)));
        assert!(archives.list(Some(&f.identity(&session))).unwrap().is_empty());
        if let Some(path) = std::env::var_os("REVIA_M6_ERROR_FIXTURE") {
            fs::write(format!("{}.prepare-{action}.json", PathBuf::from(path).display()), serde_json::to_vec_pretty(&error.payload()).unwrap()).unwrap();
        }
    }
}

#[test]
#[cfg(unix)]
fn interactive_and_reword_preserve_file_confirmation_through_automatic_stash() {
    for action in ["interactive", "reword"] {
        let mut f = Fixture::new(); f.write(".gitignore", b"victim.txt\n"); f.write("local.txt", b"base\n");
        let base = f.commit_all("base"); f.write("victim.txt", b"historical\n"); f.git(&["add", "--force", "victim.txt"]);
        let a = f.commit_all("add victim"); f.git(&["rm", "victim.txt"]); let b = f.commit_all("delete victim");
        f.write("local.txt", b"saved local\n"); f.vars.insert("REVIA_TEST_ROOT".into(), f.repo.display().to_string());
        install_m6_hook(&f, "reference-transaction", "#!/bin/sh\n[ \"$1\" = committed ] || exit 0\nwhile read old new ref; do\n if [ \"$ref\" = refs/stash ]; then printf 'FOREIGN AFTER CONFIRMATION\\n' > \"$REVIA_TEST_ROOT/victim.txt\"; fi\ndone\n");
        let session = f.session(); let checked = rewrite::preflight(&session, &[a.clone(), b.clone()]).unwrap();
        let auto = f.autostash(); let archive = f.archives();
        let result = if action == "interactive" {
            rewrite::interactive(&session, &f.recovery(), &auto, &archive, &f.data(), &InteractiveRebaseRequest {
                upstream: base, expected_head: b.clone(), expected_context: checked.context_version,
                expected_state: checked.overwrite_fingerprint,
                entries: vec![RebasePlanEntry { oid: a, action: RebaseAction::Reword, message: Some("rewritten A".into()) },
                    RebasePlanEntry { oid: b.clone(), action: RebaseAction::Edit, message: None }],
            })
        } else {
            rewrite::reword(&session, &f.recovery(), &auto, &archive, &f.data(), &a,
                &b, &checked.context_version, &checked.overwrite_fingerprint, "rewritten A")
        };
        let error = result.unwrap_err();
        assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
        assert_eq!(f.read("victim.txt"), b"FOREIGN AFTER CONFIRMATION\n"); assert_eq!(f.head(), b);
        assert!(operation::detect(&session.git, 0).unwrap().is_none());
        assert!(!session.git.log.snapshot().iter().any(|entry| entry.args.iter().any(|arg| arg == "rebase")));
        let link = auto.for_worktree(&session).unwrap().unwrap();
        assert_eq!(stash::list(&session.git).unwrap()[0].oid, link.stash_oid);
        assert_eq!(link.restoration, revia_lib::autostash::AutostashRestoration::Pending);
    }
}

#[test]
#[cfg(unix)]
fn user_refresh_during_autostash_invalidates_the_unstarted_history_write() {
    let mut f = Fixture::new(); f.write("local.txt", b"base\n"); let base = f.commit_all("base");
    f.write("history.txt", b"history\n"); let oid = f.commit_all("history"); f.write("local.txt", b"saved local\n");
    let ready = f.temp.path().join("ready"); let release = f.temp.path().join("release");
    f.vars.insert("REVIA_TEST_READY".into(), ready.display().to_string());
    f.vars.insert("REVIA_TEST_RELEASE".into(), release.display().to_string());
    install_m6_hook(&f, "reference-transaction", "#!/bin/sh\n[ \"$1\" = committed ] || exit 0\nwhile read old new ref; do\n if [ \"$ref\" = refs/stash ]; then\n  : > \"$REVIA_TEST_READY\"\n  i=0\n  while [ ! -f \"$REVIA_TEST_RELEASE\" ] && [ \"$i\" -lt 1500 ]; do sleep 0.01; i=$((i + 1)); done\n fi\ndone\n");
    let session = f.session(); let checked = rewrite::preflight(&session, &[oid.clone()]).unwrap();
    let initial_version = session.confirmation_version(); let target = session.clone();
    let store = f.recovery(); let auto = f.autostash(); let archives = f.archives(); let data = f.data(); let original_head = oid.clone();
    let running = std::thread::spawn(move || rewrite::interactive(&target, &store, &auto, &archives, &data,
        &InteractiveRebaseRequest { upstream: base, expected_head: oid.clone(), expected_context: checked.context_version,
            expected_state: checked.overwrite_fingerprint,
            entries: vec![RebasePlanEntry { oid, action: RebaseAction::Edit, message: None }] }));
    let start = std::time::Instant::now();
    while !ready.exists() { assert!(start.elapsed() < std::time::Duration::from_secs(15)); std::thread::sleep(std::time::Duration::from_millis(10)); }
    let _refresh = session.refresh();
    let refreshed = session.confirmation_version() != initial_version;
    fs::write(&release, b"resume").unwrap();
    let error = running.join().unwrap().unwrap_err();
    assert!(refreshed); assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
    assert_eq!(f.head(), original_head); assert!(operation::detect(&session.git, 0).unwrap().is_none());
    assert_eq!(stash::list(&session.git).unwrap().len(), 1);
}

#[test]
fn repeated_stash_objects_keep_distinct_occurrence_credentials() {
    let mut f = Fixture::new();
    f.vars.insert("GIT_AUTHOR_DATE".into(), "2026-10-29T00:00:00Z".into());
    f.vars.insert("GIT_COMMITTER_DATE".into(), "2026-10-29T00:00:00Z".into());
    f.write("file", b"base\n"); f.commit_all("base");
    f.write("file", b"staged\n"); f.git(&["add", "file"]);
    let session = f.session(); let index = f.index();
    let save = |message: &str| stash::save(&session, &StashSaveRequest {
        message: message.into(), keep_index: true, expected_state: stash::preflight(&session, None).unwrap(),
    }).unwrap();
    let first = save("A"); let unchanged = save("A");
    assert_eq!(first.oid, unchanged.oid);
    assert_eq!(stash::list(&session.git).unwrap().len(), 1);
    let middle = save("B"); let newest = save("A");
    let entries = stash::list(&session.git).unwrap();
    assert_eq!(entries.len(), 3); assert_eq!(entries[0].oid, entries[2].oid);
    assert_eq!(newest.oid, first.oid); assert_ne!(entries[0].key, entries[2].key);
    assert_eq!(f.index(), index); assert_eq!(f.read("file"), b"staged\n");
    let old = &entries[2];
    assert_eq!(stash::inspect_selected(&session.git, &old.oid, Some(&old.key), false).unwrap().entry.index, 2);
    let expected = stash::preflight_selected(&session, Some(&old.oid), Some(&old.key)).unwrap();
    let deleted = stash::drop_selected(&session, &f.archives(), &old.oid, &old.key, &expected).unwrap();
    assert!(deleted.deleted);
    let after = stash::list(&session.git).unwrap();
    assert_eq!(after.iter().map(|entry| &entry.oid).collect::<Vec<_>>(), vec![&newest.oid, &middle.oid]);
    assert!(stash::preflight_selected(&session, Some(&entries[0].oid), Some(&entries[0].key)).is_err());
    let top = after[0].clone();
    let collection_before_external_change = stash::collection_preflight(&session).unwrap();
    let expected = stash::preflight_selected(&session, Some(&top.oid), Some(&top.key)).unwrap();
    let error = stash::drop_selected_with_hook(&session, &f.archives(), &top.oid, &top.key, &expected, |phase| {
        if phase == revia_lib::stash_transaction::StashDeletePhase::BeforeLock {
            f.git(&["stash", "store", "-m", "external occurrence", &middle.oid]);
        }
        Ok(())
    }).unwrap_err();
    // The retained confirmation now rejects before the transaction's list
    // comparison. Its state-change code still means no entry was deleted.
    assert_eq!(error.code, Some("errors.rewrite.stateChanged"));
    let after = stash::list(&session.git).unwrap();
    assert_eq!(after.len(), 3); assert_eq!(after[0].message, "external occurrence");
    let current_oids: Vec<_> = after.iter().map(|entry| entry.oid.clone()).collect();
    assert!(stash::clear_confirmed(&session, &f.archives(), &current_oids, &collection_before_external_change).is_err());
    assert_eq!(stash::list(&session.git).unwrap(), after);
    f.git(&["restore", "--source=HEAD", "--staged", "--worktree", "--", "file"]);
    let selected = after[1].clone();
    let expected = stash::preflight_selected(&session, Some(&selected.oid), Some(&selected.key)).unwrap();
    let result = stash::apply(&session, &f.recovery(), &f.archives(), &StashApplyRequest {
        oid: selected.oid.clone(), entry_key: Some(selected.key), expected_state: expected,
        restore_index: false, delete_after: true, new_branch: None,
    }).unwrap();
    assert!(result.applied && result.deleted && result.cleanup_error.is_none());
    let remaining = stash::list(&session.git).unwrap();
    assert_eq!(remaining.len(), 2); assert!(remaining.iter().all(|entry| entry.oid == middle.oid));
    assert_eq!(f.read("file"), b"staged\n"); assert!(f.index_clean());
}

#[test]
fn force_history_clears_confirmed_staged_paths_and_preserves_unrelated_work_files() {
    for revert in [false, true] {
        let f = Fixture::new(); f.write("file", b"base\n"); f.write("other", b"other\n");
        f.write("old", b"rename bytes\n"); f.commit_all("base");
        f.branch("source"); f.write("file", b"source\n"); let source = f.commit_all("source");
        if !revert { f.checkout("main"); }
        f.write("file", b"staged local\n"); f.git(&["add", "file"]); f.write("file", b"working local\n");
        f.git(&["mv", "old", "new"]); f.write("added", b"added local\n"); f.git(&["add", "added"]);
        f.write("other", b"unrelated working bytes\n");
        let session = f.session(); let confirmed = rewrite::preflight(&session, &[source.clone()]).unwrap();
        assert!(confirmed.has_staged_changes && confirmed.requires_choice);
        let result = if revert {
            rewrite::revert(&session, &f.recovery(), &f.autostash(), &f.archives(), &RevertRequest {
                expected_head: f.head(), expected_context: confirmed.context_version,
                expected_state: Some(confirmed.overwrite_fingerprint), oids: vec![source], dirty_strategy: Some(DirtyStrategy::Force),
            })
        } else {
            rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &CherryPickRequest {
                expected_head: f.head(), expected_context: confirmed.context_version,
                expected_state: Some(confirmed.overwrite_fingerprint), oids: vec![source], dirty_strategy: Some(DirtyStrategy::Force),
                keep_empty: false, include_source: false,
            })
        }.unwrap();
        assert!(result.completed && result.error.is_none());
        assert_eq!(f.read("file"), if revert { b"base\n".as_slice() } else { b"source\n".as_slice() });
        assert_eq!(f.read("old"), b"rename bytes\n"); assert!(!f.repo.join("new").exists()); assert!(!f.repo.join("added").exists());
        assert_eq!(f.read("other"), b"unrelated working bytes\n"); assert!(f.index_clean());
        let saved: Vec<_> = f.recovery().list(None).unwrap().iter().flat_map(|summary| f.recovery().read(&summary.id).unwrap().0.before).collect();
        assert!(saved.iter().any(|file| file.path == "file" && file.content.as_deref() == Some(b"working local\n")));
        assert!(saved.iter().any(|file| file.path == "new" && file.content.as_deref() == Some(b"rename bytes\n")));
        assert!(saved.iter().any(|file| file.path == "added" && file.content.as_deref() == Some(b"added local\n")));
        assert!(stash::list(&session.git).unwrap().is_empty());
    }
}

#[cfg(unix)]
#[test]
fn force_history_stops_before_overwriting_later_hook_changes_in_files_or_index() {
    use std::os::unix::fs::PermissionsExt;
    for revert in [false, true] {
        for change in ["worktree", "index", "ignored"] {
            let f = Fixture::new(); f.write("a", b"base-a\n");
            if change == "ignored" { f.write(".gitignore", b"b\n"); }
            if change != "ignored" || revert {
                f.write("b", b"base-b\n"); f.git(&["add", "--force", "b"]);
            }
            f.commit_all("base"); f.branch("source"); f.write("a", b"topic-a\n");
            if change == "ignored" && revert { f.git(&["rm", "b"]); }
            else { f.write("b", b"topic-b\n"); f.git(&["add", "--force", "b"]); }
            let target = f.commit_all("target");
            if !revert { f.checkout("main"); }
            let head = f.head(); let index = f.index();
            f.write("a", b"dirty-a\n"); f.write("b", b"dirty-b\n");
            let hook = f.repo.join(".git/hooks/post-index-change");
            let cleaned_a = if revert { "topic-a" } else { "base-a" };
            let stage = if change == "index" { "git add b\nprintf 'dirty-b\\n' > b\n" } else { "" };
            fs::write(&hook, format!("#!/bin/sh\nif test -f .git/m6-force-arm && test \"$(cat a)\" = {cleaned_a}; then\n rm .git/m6-force-arm\n printf 'NEW-UNCONFIRMED-BYTES\\n' > b\n {stage} printf 'hook-wrote-b\\n' >> .git/m6-force-hook.log\nfi\n")).unwrap();
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
            let session = f.session(); let checked = rewrite::preflight(&session, &[target.clone()]).unwrap();
            assert_eq!(checked.paths, vec!["a", "b"]);
            fs::write(f.repo.join(".git/m6-force-arm"), b"armed").unwrap();
            let result = if revert {
                rewrite::revert(&session, &f.recovery(), &f.autostash(), &f.archives(), &RevertRequest {
                    oids: vec![target], expected_head: head.clone(), expected_context: checked.context_version,
                    expected_state: Some(checked.overwrite_fingerprint), dirty_strategy: Some(DirtyStrategy::Force),
                })
            } else {
                rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &CherryPickRequest {
                    oids: vec![target], expected_head: head.clone(), expected_context: checked.context_version,
                    expected_state: Some(checked.overwrite_fingerprint), dirty_strategy: Some(DirtyStrategy::Force),
                    keep_empty: false, include_source: false,
                })
            };
            println!("force revert={revert} change={change} result={result:?} commands={}", serde_json::to_string_pretty(&session.git.log.snapshot()).unwrap());
            let error = result.unwrap_err();
            assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
            assert_eq!(fs::read_to_string(f.repo.join(".git/m6-force-hook.log")).unwrap(), "hook-wrote-b\n");
            assert_eq!(f.head(), head); assert_eq!(f.read("a"), format!("{cleaned_a}\n").as_bytes());
            if change == "index" {
                assert_eq!(f.git(&["show", ":b"]), b"NEW-UNCONFIRMED-BYTES\n");
                assert_eq!(f.read("b"), b"dirty-b\n");
            } else {
                assert_eq!(f.read("b"), b"NEW-UNCONFIRMED-BYTES\n"); assert_eq!(f.index(), index);
            }
            assert!(session.git.log.snapshot().iter().all(|command| {
                !command.args.iter().any(|arg| arg == "cherry-pick" || arg == "revert")
                    && !(command.args.iter().any(|arg| arg == "restore" || arg == "clean") && command.args.last().is_some_and(|arg| arg == "b"))
            }));
            let records = f.recovery().list(None).unwrap(); assert!(!records.is_empty());
            for path in ["a", "b"] {
                assert!(records.iter().any(|summary| f.recovery().read(&summary.id).unwrap().0.before.iter()
                    .any(|file| file.path == path && file.content.as_deref() == Some(format!("dirty-{path}\n").as_bytes()))));
            }
            assert!(stash::list(&session.git).unwrap().is_empty());
        }
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_history_and_previous_paths_never_open_utf8_aliases() {
    use std::io::Write;
    use std::process::Stdio;
    let f = Fixture::new();
    let input = |args: &[&str], bytes: &[u8]| {
        let mut child = Command::new("git").args(args).current_dir(&f.repo).env_clear().envs(&f.vars)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(bytes).unwrap(); let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{args:?}: {:?}", output.stderr); output.stdout
    };
    f.write("old-\u{fffd}.txt", b"UNRELATED-UTF8-ALIAS\n"); f.git(&["add", "--all"]);
    let blob = String::from_utf8(input(&["hash-object", "-w", "--stdin"], b"ORIGINAL-LINEAGE\n")).unwrap();
    let blob = blob.trim(); let mut index = format!("100644 {blob}\t").into_bytes(); index.extend(b"old-\xff.txt\0");
    input(&["update-index", "-z", "--index-info"], &index); f.git(&["commit", "-m", "import byte paths"]);
    let mut rename = format!("0 {}\t", "0".repeat(40)).into_bytes(); rename.extend(b"old-\xff.txt\0");
    rename.extend(format!("100644 {blob}\tcurrent.txt\0").as_bytes()); input(&["update-index", "-z", "--index-info"], &rename);
    f.git(&["commit", "-m", "rename to UTF8"]); let session = f.session(); let before = f.index();
    let error = file_history::history(&session.git, &FileHistoryRequest {
        path: "current.txt".into(), all_branches: false, skip: 0, limit: 40, anchor: None,
    }).unwrap_err();
    assert_eq!(error.code, Some("errors.readonly.nonUtf8Path"));
    assert_eq!(file_history::previous_revision(&session.git, &f.head(), "current.txt").unwrap_err().code,
        Some("errors.readonly.nonUtf8Path"));
    assert_eq!(f.read("old-\u{fffd}.txt"), b"UNRELATED-UTF8-ALIAS\n"); assert_eq!(f.index(), before);
    assert!(!session.git.log.snapshot().iter().any(|record| record.args.iter().any(|arg| arg == "old-\u{fffd}.txt") && record.args.iter().any(|arg| arg == "show")));
}

#[test]
fn unchanged_keep_index_save_does_not_claim_an_external_same_content_entry() {
    let mut f = Fixture::new();
    f.vars.insert("GIT_AUTHOR_DATE".into(), "2026-10-29T00:00:00Z".into());
    f.vars.insert("GIT_COMMITTER_DATE".into(), "2026-10-29T00:00:00Z".into());
    f.write("file", b"base\n"); f.commit_all("base"); f.write("file", b"saved\n"); f.git(&["add", "file"]);
    let session = f.session();
    let request = || StashSaveRequest { message: "own unchanged save".into(), keep_index: true,
        expected_state: stash::preflight(&session, None).unwrap() };
    let own = stash::save(&session, &request()).unwrap();
    let outcome = stash::save_with_after_write(&session, &request(), || {
        let tree = f.oid("refs/stash^{tree}"); let base = f.oid("refs/stash^1"); let index = f.oid("refs/stash^2");
        let foreign = String::from_utf8(f.git(&["commit-tree", &tree, "-p", &base, "-p", &index, "-m", "external copy"])).unwrap();
        f.git(&["stash", "store", "-m", "external same data", foreign.trim()]); Ok(())
    });
    let listed = stash::list(&session.git).unwrap();
    println!("own={own:?} result={outcome:?} actual={listed:?}");
    assert!(outcome.as_ref().map_or(true, |entry| entry.oid == own.oid), "a deduplicated save must not claim the external occurrence");
    assert_eq!(listed.len(), 2); assert_eq!(f.read("file"), b"saved\n");
}

#[cfg(unix)]
#[test]
fn stash_new_branch_keeps_original_confirmation_through_checkout_hooks() {
    use std::os::unix::fs::PermissionsExt;
    for (changed_tree, hook_mode) in [(false, "worktree"), (false, "index"), (true, "both"), (true, "none")] {
        let f = Fixture::new();
        let original = (1..=30).map(|line| format!("line {line}\n")).collect::<String>();
        f.write("file.txt", original.as_bytes()); f.write("keep", b"keep base\n"); f.commit_all("base");
        let stashed = original.replace("line 2\n", "stashed line 2\n"); f.write("file.txt", stashed.as_bytes());
        f.git(&["stash", "push", "--quiet", "-m", "selected"]);
        if changed_tree { f.write("file.txt", original.replace("line 15\n", "newer line 15\n").as_bytes()); }
        f.write("main-only", b"newer main\n"); f.commit_all("newer main");
        f.write("keep", b"unrelated working bytes\n");
        if hook_mode != "none" {
            let hook = f.repo.join(".git/hooks/post-checkout");
            let mut script = "#!/bin/sh\n[ \"$3\" = 1 ] || exit 0\nsed 's/^line 28$/foreign line 28/' file.txt > hook-output\nmv hook-output file.txt\n".to_string();
            if hook_mode != "worktree" { script.push_str("git add -- file.txt\n"); }
            if hook_mode == "index" { script.push_str("git show HEAD:file.txt > file.txt\n"); }
            fs::write(&hook, script).unwrap(); fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let session = f.session(); let entry = stash::list(&session.git).unwrap()[0].clone();
        let expected = stash::preflight_selected(&session, Some(&entry.oid), Some(&entry.key)).unwrap();
        let result = stash::apply(&session, &f.recovery(), &f.archives(), &StashApplyRequest {
            oid: entry.oid.clone(), entry_key: Some(entry.key), expected_state: expected, restore_index: false,
            delete_after: false, new_branch: Some("from-stash".into()),
        }).unwrap();
        assert_eq!(result.branch.as_deref(), Some("from-stash")); assert_eq!(f.read("keep"), b"unrelated working bytes\n");
        assert_eq!(stash::list(&session.git).unwrap()[0].oid, entry.oid);
        if hook_mode == "none" {
            assert!(result.applied && result.error.is_none()); assert_eq!(f.read("file.txt"), stashed.as_bytes());
        } else {
            assert!(!result.applied); assert_eq!(result.error.unwrap().kind, revia_lib::error::ErrorKind::NeedsVerification);
            assert!(!session.git.log.snapshot().iter().any(|record| record.args.starts_with(&["stash".into(), "apply".into()])));
            if hook_mode == "index" {
                assert_eq!(f.read("file.txt"), original.as_bytes());
                assert!(String::from_utf8(f.git(&["show", ":file.txt"])).unwrap().contains("foreign line 28"));
            } else { assert!(String::from_utf8(f.read("file.txt")).unwrap().contains("foreign line 28")); }
        }
    }
}

#[test]
fn all_branch_file_history_excludes_stash_only_commits_and_keeps_other_branches() {
    let f = Fixture::new(); f.write("file", b"base\n"); let base = f.commit_all("base");
    f.branch("other"); f.write("file", b"other branch committed\n"); let other = f.commit_all("other branch");
    f.checkout("main"); f.write("file", b"stash-only content\n"); f.git(&["add", "file"]);
    f.git(&["stash", "push", "--quiet", "--keep-index", "-m", "not on a branch"]);
    let stash_oid = f.oid("refs/stash"); let stash_index = f.oid("refs/stash^2");
    let session = f.session(); let index = f.index(); let bytes = f.read("file");
    let request = FileHistoryRequest { path: "file".into(), all_branches: true, skip: 0, limit: 40, anchor: None };
    let history = file_history::history(&session.git, &request).unwrap();
    let mut actual = history.entries.iter().map(|entry| entry.oid.clone()).collect::<Vec<_>>(); actual.sort();
    let mut expected = vec![base, other]; expected.sort(); assert_eq!(actual, expected);
    assert!(history.entries.iter().all(|entry| entry.oid != stash_oid && entry.oid != stash_index && !entry.patch.contains("stash-only content")));
    assert_eq!(f.index(), index); assert_eq!(f.read("file"), bytes);
}

#[test]
fn automatic_autostash_restore_stays_on_the_branch_it_was_saved_from() {
    let f = Fixture::new(); f.write("file", b"base\n"); f.commit_all("base");
    f.git(&["branch", "other"]);
    f.write("file", b"main local\n"); f.git(&["add", "file"]);
    let session = f.session();
    let link = f.autostash().save_before(&session, &f.recovery(), "cherry-pick").unwrap().unwrap();
    f.checkout("other");
    let error = f.autostash().restore_if_ready(&session, &f.recovery(), &f.archives()).unwrap_err();
    assert_eq!(error.code, Some("errors.autostash.restoreTargetChanged"));
    assert_eq!(f.read("file"), b"base\n"); assert!(f.index_clean());
    assert_eq!(stash::list(&session.git).unwrap().len(), 1);
    assert_eq!(f.autostash().for_worktree(&session).unwrap().unwrap().id, link.id);
    f.checkout("main");
    let outcome = f.autostash().restore_if_ready(&session, &f.recovery(), &f.archives()).unwrap().unwrap();
    assert!(outcome.restored && outcome.removed);
    assert_eq!(f.read("file"), b"main local\n");
}

#[cfg(unix)]
#[test]
fn final_history_step_hook_branch_switch_keeps_the_plan_and_autostash() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(); f.write("local.txt", b"base local\n"); f.write("picked.txt", b"base pick\n");
    f.commit_all("base"); f.git(&["branch", "other"]);
    f.branch("source"); f.write("picked.txt", b"source pick\n"); let source = f.commit_all("source");
    f.checkout("main"); f.write("local.txt", b"main private local\n"); f.git(&["add", "local.txt"]);
    let hook = f.repo.join(".git/hooks/post-commit");
    fs::write(&hook, b"#!/bin/sh\nexec git checkout --quiet other\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
    let session = f.session(); let auto = f.autostash();
    let outcome = rewrite::cherry_pick(&session, &f.recovery(), &auto, &f.archives(), &CherryPickRequest {
        expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
        expected_state: Some(rewrite::preflight(&session, &[source.clone()]).unwrap().overwrite_fingerprint),
        oids: vec![source.clone()], expected_head: f.head(), dirty_strategy: Some(DirtyStrategy::Stash),
        keep_empty: false, include_source: false,
    }).unwrap();
    assert!(!outcome.completed); assert!(outcome.error.is_some()); assert!(outcome.autostash.is_none());
    assert_eq!(outcome.applied, vec![source]);
    assert_eq!(f.git(&["branch", "--show-current"]), b"other\n");
    assert_eq!(f.read("local.txt"), b"base local\n"); assert!(f.index_clean());
    assert_eq!(stash::list(&session.git).unwrap().len(), 1);
    assert!(auto.for_worktree(&session).unwrap().is_some());
    let state = operation_flow::state(&session, &f.data()).unwrap().unwrap();
    assert!(state.needs_verification);
}

#[test]
fn rebase_skip_into_the_next_conflict_is_a_normal_stop() {
    let f = Fixture::new(); f.write("a.txt", b"base\n"); f.write("b.txt", b"base\n"); f.commit_all("base");
    f.branch("topic"); f.write("a.txt", b"topic a\n"); f.commit_all("topic A");
    f.write("b.txt", b"topic b\n"); f.commit_all("topic B");
    f.checkout("main"); f.write("a.txt", b"main a\n"); f.write("b.txt", b"main b\n"); f.commit_all("main AB");
    f.checkout("topic"); assert!(!f.try_git(&["rebase", "main"]).status.success());
    let session = f.session(); let data = f.data();
    let state = operation_flow::state(&session, &data).unwrap().unwrap();
    let outcome = operation_flow::execute(&session, &f.recovery(), &f.autostash(), &f.archives(), &OperationActionRequest {
        expected_state: state.confirmation_state.clone(),
        kind: state.kind, metadata_version: state.metadata_version, action: OperationAction::Skip,
    }).unwrap();
    assert!(!outcome.completed);
    let next = outcome.state.unwrap();
    assert_eq!(next.kind, OperationKind::Rebase); assert!(next.has_conflicts());
    assert_eq!(f.git(&["diff", "--name-only", "--diff-filter=U"]), b"b.txt\n");
}

#[test]
fn history_selection_conflict_is_a_normal_stop_with_its_receipt() {
    let f = Fixture::new(); f.write("a.txt", b"base\n"); f.write("b.txt", b"base\n"); f.commit_all("base");
    f.branch("feature"); f.write("a.txt", b"feature a\n"); let a = f.commit_all("A");
    f.write("b.txt", b"feature b\n"); let b = f.commit_all("B");
    f.checkout("main"); f.write("b.txt", b"main b\n"); f.commit_all("main b");
    let session = f.session();
    let outcome = rewrite::cherry_pick(&session, &f.recovery(), &f.autostash(), &f.archives(), &CherryPickRequest {
        expected_context: revia_lib::write_guard::context(&session.git).unwrap(),
        expected_state: Some(rewrite::preflight(&session, &[a.clone(), b.clone()]).unwrap().overwrite_fingerprint),
        oids: vec![a.clone(), b.clone()], expected_head: f.head(), dirty_strategy: None,
        keep_empty: false, include_source: false,
    }).unwrap();
    assert!(!outcome.completed); assert!(outcome.error.is_none());
    assert_eq!(outcome.applied, vec![a]); assert_eq!(outcome.failed_oid, Some(b));
    assert!(outcome.state.unwrap().has_conflicts());
}

#[test]
fn file_history_and_stash_patches_keep_non_ascii_paths_readable() {
    let f = Fixture::new(); f.write("文档 目录/说明 文件.txt", b"one\n"); f.commit_all("base");
    f.write("文档 目录/说明 文件.txt", b"two\n"); f.commit_all("change");
    let session = f.session();
    let page = file_history::history(&session.git, &FileHistoryRequest {
        path: "文档 目录/说明 文件.txt".into(), all_branches: false, skip: 0, limit: 40, anchor: None,
    }).unwrap();
    assert!(page.entries[0].patch.contains("文档 目录/说明 文件.txt"), "{}", page.entries[0].patch);
    f.write("文档 目录/说明 文件.txt", b"three\n"); f.git(&["stash", "push", "--quiet"]);
    let oid = stash::list(&session.git).unwrap()[0].oid.clone();
    for compare_local in [false, true] {
        let inspection = stash::inspect(&session.git, &oid, compare_local).unwrap();
        // Require readable path text; Git may omit the a/ and b/ display prefixes.
        assert!(inspection.patch.contains("文档 目录/说明 文件.txt"), "{}", inspection.patch);
        assert!(!inspection.patch.contains("\\346"));
    }
}

#[test]
fn rebase_skip_that_does_not_advance_still_needs_verification() {
    let f = Fixture::new(); f.write("a.txt", b"base\n"); f.write("b.txt", b"base\n"); f.commit_all("base");
    f.branch("topic"); f.write("a.txt", b"topic a\n"); f.commit_all("topic A");
    f.write("b.txt", b"topic b\n"); f.commit_all("topic B");
    f.checkout("main"); f.write("a.txt", b"main a\n"); f.write("b.txt", b"main b\n"); f.commit_all("main AB");
    f.checkout("topic"); assert!(!f.try_git(&["rebase", "main"]).status.success());
    let session = f.session(); let data = f.data();
    let state = operation_flow::state(&session, &data).unwrap().unwrap();
    // Another process holds the index lock: Git cannot skip, the stop stays where it was.
    let lock = f.repo.join(".git/index.lock"); fs::write(&lock, b"").unwrap();
    let result = operation_flow::execute(&session, &f.recovery(), &f.autostash(), &f.archives(), &OperationActionRequest {
        expected_state: state.confirmation_state.clone(),
        kind: state.kind, metadata_version: state.metadata_version.clone(), action: OperationAction::Skip,
    });
    assert!(lock.exists(), "Revia must never delete index.lock");
    fs::remove_file(&lock).unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.kind, revia_lib::error::ErrorKind::NeedsVerification);
    let after = operation_flow::state(&session, &data).unwrap().unwrap();
    assert_eq!(after.metadata_version, state.metadata_version);
    assert_eq!(f.git(&["diff", "--name-only", "--diff-filter=U"]), b"a.txt\n");
}
