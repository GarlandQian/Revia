//! M3 regressions use production sessions, real Git and isolated repositories/data.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use revia_lib::commit::{CommitDraft, DraftStore};
use revia_lib::commit_flow::{AmendConfirmation, CommitOptions, CommitOutcome, CommitOutputEvent};
use revia_lib::error::ErrorKind;
use revia_lib::git::CommandLog;
use revia_lib::recovery::{RecoverySource, RecoveryState, RecoveryStore, RestoreExpectation};
use revia_lib::session::WorktreeSession;
use revia_lib::shell::{EnvCaptureInfo, ShellEnv};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    repo: PathBuf,
    env: ShellEnv,
    drafts: DraftStore,
    recovery: RecoveryStore,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let repo = root.join("repo");
        let home = root.join("home");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(&home).unwrap();
        let config = home.join(".gitconfig");
        fs::write(&config, "").unwrap();
        let path: Vec<String> = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|entry| entry.to_string_lossy().into_owned()).collect();
        let vars = HashMap::from([
            ("HOME".into(), revia_lib::platform::external_path(&home).display().to_string()),
            ("GIT_CONFIG_GLOBAL".into(), revia_lib::platform::external_path(&config).display().to_string()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("PATH".into(), std::env::join_paths(path.iter().map(Path::new)).unwrap().to_string_lossy().into_owned()),
        ]);
        #[cfg(windows)]
        let vars = {
            let mut vars = vars;
            for name in ["SystemRoot", "WINDIR", "COMSPEC", "PATHEXT"] {
                if let Ok(value) = std::env::var(name) {
                    vars.insert(name.into(), value);
                }
            }
            vars
        };
        let env = ShellEnv {
            path: path.clone(), vars,
            info: EnvCaptureInfo {
                worktree: repo.display().to_string(), shell: "fixture".into(), elapsed_ms: 0,
                timed_out: false, used_fallback: false, var_count: 4, path_entries: path,
                diagnostics: None,
            },
        };
        let fixture = Self {
            _temp: temp, root: root.clone(), repo, env,
            drafts: DraftStore::new(root.join("data")),
            recovery: RecoveryStore::new(root.join("data")),
        };
        fixture.git(&["init", "-b", "main"]);
        fixture.git(&["config", "user.name", "Revia Fixture"]);
        fixture.git(&["config", "user.email", "fixture@revia.invalid"]);
        fixture.git(&["config", "commit.gpgsign", "false"]);
        fixture
    }

    fn git_output(&self, args: &[&str]) -> Output {
        Command::new("git").args(args.iter().map(|argument| revia_lib::platform::external_path(Path::new(argument)))).current_dir(&self.repo)
            .env_clear().envs(&self.env.vars).output().unwrap()
    }

    fn git(&self, args: &[&str]) -> String {
        let output = self.git_output(args);
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    }

    fn write(&self, path: &str, text: &str) { fs::write(self.repo.join(path), text).unwrap(); }

    fn seed(&self) {
        self.write("file.txt", "base\n");
        self.git(&["add", "file.txt"]);
        self.git(&["commit", "-m", "initial"]);
    }

    fn staged(&self) {
        self.write("file.txt", "staged\n");
        self.git(&["add", "file.txt"]);
    }

    fn session(&self) -> Arc<WorktreeSession> { self.open("worktree", &self.repo) }

    fn open(&self, id: &str, path: &Path) -> Arc<WorktreeSession> {
        WorktreeSession::open(id, path, self.env.clone(), Arc::new(CommandLog::default())).unwrap()
    }

    fn options(&self, session: &WorktreeSession) -> CommitOptions {
        CommitOptions {
            message: "subject\n\nUnicode: 提交内容\n".into(), amend: false, skip_hooks: false,
            expected: session.commit_context(&self.drafts).unwrap().expectation,
            amend_confirmation: None, client_nonce: "test-nonce".into(), draft_revision: "revision-1".into(),
        }
    }

    fn run(&self, session: &Arc<WorktreeSession>, options: CommitOptions) -> revia_lib::error::Result<CommitOutcome> {
        session.execute_commit_flow(&self.drafts, &self.recovery, self.root.join("hooks-off"),
            options, "operation".into(), Arc::new(|_| {}))
    }

    #[cfg(unix)]
    fn hook(&self, name: &str, script: &str) {
        use std::os::unix::fs::PermissionsExt;
        let path = self.repo.join(".git/hooks").join(name);
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn draft(&self, options: &CommitOptions, revision: &str) -> CommitDraft {
        CommitDraft { message: options.message.clone(), amend: options.amend,
            skip_hooks: true, saved_at_ms: 0, revision: revision.into() }
    }
}

#[test]
fn initial_commit_uses_only_the_index_and_clears_only_matching_draft() {
    let fixture = Fixture::new();
    fixture.staged();
    fixture.write("untracked.txt", "not staged\n");
    let session = fixture.session();
    let options = fixture.options(&session);
    assert!(options.expected.head.is_none());
    session.save_commit_draft(&fixture.drafts, &fixture.draft(&options, "revision-1")).unwrap();
    let outcome = fixture.run(&session, options).unwrap();
    assert!(outcome.success && outcome.draft_cleared && !outcome.needs_verification);
    assert_eq!(outcome.exit_code, Some(0));
    assert!(outcome.previous_head.is_none());
    assert_eq!(outcome.new_head.unwrap(), fixture.git(&["rev-parse", "HEAD"]).trim());
    assert_eq!(fixture.git(&["ls-tree", "--name-only", "HEAD"]), "file.txt\n");
    assert!(fixture.git(&["log", "-1", "--format=%B"]).contains("Unicode: 提交内容"));
    assert!(session.commit_context(&fixture.drafts).unwrap().draft.is_none());
}

#[test]
fn changed_index_and_operation_metadata_reject_stale_expectations() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.staged();
    let session = fixture.session();
    let old = fixture.options(&session);
    fixture.write("file.txt", "external staging\n");
    fixture.git(&["add", "file.txt"]);
    assert_eq!(fixture.run(&session, old).unwrap_err().kind, ErrorKind::Rejected);
    let old = fixture.options(&session);
    fs::write(fixture.repo.join(".git/CHERRY_PICK_HEAD"), fixture.git(&["rev-parse", "HEAD"])).unwrap();
    assert_eq!(fixture.run(&session, old).unwrap_err().kind, ErrorKind::Rejected);
    let context = session.commit_context(&fixture.drafts).unwrap();
    assert!(context.gates.operation_blocks_commit && context.gates.operation_blocks_amend);
    assert_eq!(fixture.git(&["rev-list", "--count", "HEAD"]).trim(), "1");
}

#[test]
fn staged_deletion_is_committable_but_intent_to_add_is_not() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.git(&["rm", "file.txt"]);
    let session = fixture.session();
    assert!(fixture.options(&session).expected.has_staged_changes());
    assert!(fixture.run(&session, fixture.options(&session)).unwrap().success);
    fixture.write("intent.txt", "not staged\n");
    fixture.git(&["add", "-N", "intent.txt"]);
    let options = fixture.options(&session);
    assert!(!options.expected.has_staged_changes());
    assert_eq!(fixture.run(&session, options).unwrap_err().kind, ErrorKind::Rejected);
}

#[test]
fn published_amend_requires_a_confirmation_bound_to_the_exact_input() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.git(&["branch", "upstream"]);
    fixture.git(&["branch", "--set-upstream-to=upstream"]);
    let session = fixture.session();
    let context = session.commit_context(&fixture.drafts).unwrap();
    assert!(context.gates.upstream_contains_head);
    let head = context.expectation.head.clone().unwrap();
    assert_eq!(session.amend_head_message(&head).unwrap().unwrap(), "initial\n");
    let mut options = fixture.options(&session);
    options.amend = true;
    assert_eq!(fixture.run(&session, options.clone()).unwrap_err().kind, ErrorKind::Rejected);
    options.amend_confirmation = Some(AmendConfirmation {
        upstream_version: context.upstream_version,
        expectation_fingerprint: context.expectation.fingerprint,
        message: options.message.clone(), amend: true, skip_hooks: false,
    });
    let mut changed = options.clone();
    changed.message.push_str("new edit");
    assert_eq!(fixture.run(&session, changed).unwrap_err().kind, ErrorKind::Rejected);
    let outcome = fixture.run(&session, options).unwrap();
    assert!(outcome.success);
    assert_ne!(outcome.new_head.unwrap(), head);
    assert_eq!(fixture.git(&["rev-list", "--count", "HEAD"]).trim(), "1");
    assert_eq!(session.amend_head_message(&head).unwrap_err().kind, ErrorKind::Rejected);
}

#[test]
fn conflict_free_merge_can_finish_without_a_staged_tree_change() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.git(&["checkout", "-b", "other"]);
    fixture.git(&["commit", "--allow-empty", "-m", "other"]);
    fixture.git(&["checkout", "main"]);
    fixture.git(&["merge", "--no-ff", "--no-commit", "other"]);
    let session = fixture.session();
    let context = session.commit_context(&fixture.drafts).unwrap();
    assert!(!context.gates.has_staged_changes);
    assert!(context.gates.allows_empty_diff_commit && context.gates.operation_blocks_amend);
    assert!(fixture.run(&session, fixture.options(&session)).unwrap().success);
    assert_eq!(fixture.git(&["log", "-1", "--format=%P"]).split_whitespace().count(), 2);
}

#[test]
fn a_new_revision_of_the_same_text_survives_success_and_skip_is_not_persisted() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.staged();
    let session = fixture.session();
    let options = fixture.options(&session);
    session.save_commit_draft(&fixture.drafts, &fixture.draft(&options, "revision-2")).unwrap();
    let outcome = fixture.run(&session, options).unwrap();
    assert!(outcome.success && !outcome.draft_cleared);
    let saved = session.commit_context(&fixture.drafts).unwrap().draft.unwrap();
    assert_eq!(saved.revision, "revision-2");
    assert!(!saved.skip_hooks);
}

#[test]
fn snapshot_storage_failure_blocks_git_start() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.staged();
    fixture.write("file.txt", "complete working copy\n");
    let session = fixture.session();
    let old_head = fixture.git(&["rev-parse", "HEAD"]);
    let bad_data = fixture.root.join("not-a-directory");
    fs::write(&bad_data, "blocked").unwrap();
    let bad_store = RecoveryStore::new(bad_data);
    assert!(session.execute_commit_flow(&fixture.drafts, &bad_store, fixture.root.join("hooks-off"),
        fixture.options(&session), "operation".into(), Arc::new(|_| {})).is_err());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), old_head);
    assert_eq!(fixture.git(&["show", ":file.txt"]), "staged\n");
    assert_eq!(fs::read(fixture.repo.join("file.txt")).unwrap(), b"complete working copy\n");
}

#[cfg(unix)]
#[test]
fn failed_hook_records_real_after_and_snapshot_restores_only_the_working_copy() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.staged();
    fixture.write("file.txt", "complete working copy\n");
    fixture.hook("pre-commit", "printf 'hook changed\\n' > file.txt\nprintf 'hook diagnostic' >&2\nexit 7");
    let session = fixture.session();
    let options = fixture.options(&session);
    session.save_commit_draft(&fixture.drafts, &fixture.draft(&options, "revision-1")).unwrap();
    let outcome = fixture.run(&session, options).unwrap();
    assert!(!outcome.success && !outcome.needs_verification && !outcome.draft_cleared);
    assert!(outcome.output.contains("hook diagnostic"));
    let payload = outcome.error.as_ref().unwrap();
    let detail = &payload.related.as_ref().unwrap()[0];
    assert_eq!(detail.code.as_deref(), Some("errors.commitFlow.hookChangedFiles"));
    assert_eq!(detail.params.as_ref().unwrap().get("worktree").unwrap(), "file.txt");
    assert!(detail.params.as_ref().unwrap().get("index").unwrap().is_empty());
    if let Some(path) = std::env::var_os("REVIA_M6_ERROR_FIXTURE") {
        fs::write(format!("{}.hook.json", PathBuf::from(path).display()), serde_json::to_vec_pretty(payload).unwrap()).unwrap();
    }

    assert!(outcome.hook_changes.unwrap().worktree.contains(&"file.txt".to_string()));
    let id = outcome.snapshot_record_id.unwrap();
    let inspection = session.inspect_record(&id, &fixture.recovery).unwrap();
    assert!(matches!(inspection.source, RecoverySource::CommitSnapshot));
    assert_eq!(inspection.state, RecoveryState::Ready);
    let index = fixture.git(&["show", ":file.txt"]);
    session.restore_record(&id, &RestoreExpectation {
        version: inspection.version, paths: inspection.paths, state: inspection.state,
    }, &fixture.recovery).unwrap();
    assert_eq!(fs::read(fixture.repo.join("file.txt")).unwrap(), b"complete working copy\n");
    assert_eq!(fixture.git(&["show", ":file.txt"]), index);
    assert!(session.commit_context(&fixture.drafts).unwrap().draft.is_some());
}

#[cfg(unix)]
#[test]
fn skip_hooks_is_command_local_and_does_not_modify_git_config() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.staged();
    for name in ["pre-commit", "prepare-commit-msg", "commit-msg", "post-commit", "post-rewrite"] {
        fixture.hook(name, "printf 'called\\n' >> hooks-called\nexit 9");
    }
    let config = fs::read(fixture.repo.join(".git/config")).unwrap();
    let session = fixture.session();
    let mut options = fixture.options(&session);
    options.skip_hooks = true;
    assert!(fixture.run(&session, options).unwrap().success);
    assert!(!fixture.repo.join("hooks-called").exists());
    assert_eq!(fs::read(fixture.repo.join(".git/config")).unwrap(), config);
    fixture.write("file.txt", "second change\n");
    fixture.git(&["add", "file.txt"]);
    let outcome = fixture.run(&session, fixture.options(&session)).unwrap();
    assert!(!outcome.success && outcome.exit_code == Some(1));
    assert_eq!(fs::read(fixture.repo.join("hooks-called")).unwrap(), b"called\n");
    assert_eq!(fs::read(fixture.repo.join(".git/config")).unwrap(), config);
}

#[cfg(unix)]
#[test]
fn inherited_output_does_not_hold_the_write_lock_after_git_exits() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.staged();
    // The finite descendant is not killed, even if this regression fails.
    fixture.hook("post-commit", "(sleep 3) &\nprintf 'git completed' >&2");
    let session = fixture.session();
    let options = fixture.options(&session);
    let drafts = fixture.drafts.clone();
    let recovery = fixture.recovery.clone();
    let hooks = fixture.root.join("hooks-off");
    let worker = session.clone();
    let (sender, receiver) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        let result = worker.execute_commit_flow(&drafts, &recovery, hooks, options,
            "operation".into(), Arc::new(|_| {}));
        sender.send(result).unwrap();
    });
    let result = receiver.recv_timeout(Duration::from_secs(2));
    thread.join().unwrap();
    let outcome = result.expect("Git exited but inherited stdout/stderr kept the write gate locked").unwrap();
    assert!(outcome.success && outcome.output.contains("git completed"));
    assert!(session.acquire_write_lock().is_ok());
}

#[cfg(unix)]
#[test]
fn unicode_streams_without_newlines_and_output_tails_are_bounded() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.staged();
    fixture.hook("pre-commit", "printf '\\344\\275'\nsleep 0.05\nprintf '\\240\\345\\245\\275'\n\
        dd if=/dev/zero bs=8192 count=96 2>/dev/null | tr '\\000' x\nprintf '尾部' >&2");
    let session = fixture.session();
    let events = Arc::new(Mutex::new((0usize, String::new())));
    let captured = events.clone();
    let outcome = session.execute_commit_flow(&fixture.drafts, &fixture.recovery,
        fixture.root.join("hooks-off"), fixture.options(&session), "operation".into(),
        Arc::new(move |event: CommitOutputEvent| {
            assert_eq!(event.worktree_id, "worktree");
            assert_eq!(event.operation_id, "operation");
            assert_eq!(event.client_nonce, "test-nonce");
            let mut events = captured.lock().unwrap();
            events.0 += event.line.len();
            if events.1.len() < 64 { events.1.push_str(&event.line); }
        })).unwrap();
    assert!(outcome.success);
    assert!(outcome.output.len() <= 256 * 1024);
    let events = events.lock().unwrap();
    assert!(events.0 > 700 * 1024);
    assert!(events.1.starts_with("你好"), "split UTF-8 must be reconstructed: {:?}", events.1);
}

#[test]
fn linked_worktrees_have_independent_drafts_and_nonqueued_write_gates() {
    let fixture = Fixture::new();
    fixture.seed();
    fixture.staged();
    let linked = fixture.root.join("linked");
    let external = revia_lib::platform::external_path(&linked);
    fixture.git(&["worktree", "add", "-b", "linked", external.to_str().unwrap()]);
    let first = fixture.session();
    let second = fixture.open("linked", &linked);
    let first_options = fixture.options(&first);
    first.save_commit_draft(&fixture.drafts, &fixture.draft(&first_options, "first")).unwrap();
    assert!(second.commit_context(&fixture.drafts).unwrap().draft.is_none());
    fs::write(linked.join("other.txt"), "linked content\n").unwrap();
    let output = Command::new("git").current_dir(&linked).args(["add", "other.txt"])
        .env_clear().envs(&fixture.env.vars).output().unwrap();
    assert!(output.status.success());
    let guard = first.acquire_write_lock().unwrap();
    assert_eq!(fixture.run(&first, first_options).unwrap_err().kind, ErrorKind::Rejected);
    assert!(fixture.run(&second, fixture.options(&second)).unwrap().success);
    drop(guard);
    assert!(first.acquire_write_lock().is_ok());
    assert_eq!(first.commit_context(&fixture.drafts).unwrap().draft.unwrap().revision, "first");
}
