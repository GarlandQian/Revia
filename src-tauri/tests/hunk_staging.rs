//! M2 task 03: hunk writes through the application boundary and real Git.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::time::{Duration, Instant};

use revia_lib::error::{AppError, ErrorKind};
use revia_lib::git::rows::{HunkAction, RowKind, ViewMode};
use revia_lib::git::status::{ChangesKind, FileStatus, ReadonlyReason};
use revia_lib::git::{CommandLog, CommandRecord};
use revia_lib::session::{ChangeEntry, ChangesSnapshot, WorktreeSession};
use revia_lib::shell::{EnvCaptureInfo, ShellEnv};

struct TestContext {
    _temp_dir: tempfile::TempDir,
    repo: PathBuf,
    env: HashMap<String, String>,
    paths: Vec<String>,
}

impl TestContext {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("create temporary repository");
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
            ("XDG_CONFIG_HOME".into(), home.display().to_string()),
            (
                "GIT_CONFIG_GLOBAL".into(),
                global_config.display().to_string(),
            ),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("PATH".into(), joined_path.to_string_lossy().into_owned()),
        ]);
        for key in ["SYSTEMROOT", "WINDIR", "TEMP", "TMP", "TMPDIR"] {
            if let Ok(value) = std::env::var(key) {
                env.insert(key.into(), value);
            }
        }
        let ctx = Self {
            _temp_dir: temp_dir,
            repo,
            env,
            paths,
        };
        ctx.git(&["init", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Revia Hunk Test"),
            ("user.email", "hunk-test@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.quotePath", "false"),
            ("core.autocrlf", "false"),
            ("core.safecrlf", "false"),
        ] {
            ctx.git(&["config", key, value]);
        }
        ctx
    }

    fn command(&self) -> Command {
        let mut command = Command::new("git");
        command.current_dir(&self.repo).env_clear().envs(&self.env);
        command
    }

    fn git_output(&self, args: &[&str]) -> Output {
        self.command().args(args).output().expect("execute Git")
    }

    fn git_bytes(&self, args: &[&str]) -> Vec<u8> {
        let output = self.git_output(args);
        assert!(
            output.status.success(),
            "git {args:?}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        output.stdout
    }

    fn git(&self, args: &[&str]) -> String {
        String::from_utf8(self.git_bytes(args)).expect("Git metadata is UTF-8")
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        let full = self.repo.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, bytes).unwrap();
    }

    fn read(&self, path: &str) -> Vec<u8> {
        fs::read(self.repo.join(path)).unwrap()
    }

    fn add(&self, path: &str) {
        self.git(&["--literal-pathspecs", "add", "--", path]);
    }

    fn commit(&self) {
        self.git(&["commit", "-m", "fixture"]);
    }

    fn track(&self, path: &str, bytes: &[u8]) {
        self.write(path, bytes);
        self.add(path);
        self.commit();
    }

    fn index_entry(&self, path: &str) -> Option<(String, String)> {
        let bytes = self.git_bytes(&[
            "--literal-pathspecs",
            "ls-files",
            "--stage",
            "-z",
            "--",
            path,
        ]);
        if bytes.is_empty() {
            return None;
        }
        let tab = bytes
            .iter()
            .position(|byte| *byte == b'\t')
            .expect("index metadata separator");
        let metadata = std::str::from_utf8(&bytes[..tab]).unwrap();
        let mut fields = metadata.split_whitespace();
        let mode = fields.next().unwrap().to_string();
        let oid = fields.next().unwrap().to_string();
        assert_eq!(
            fields.next(),
            Some("0"),
            "expected non-conflicted index entry"
        );
        Some((mode, oid))
    }

    fn index_bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.index_entry(path)
            .map(|(_, oid)| self.git_bytes(&["cat-file", "blob", &oid]))
    }

    fn index_state(&self) -> Vec<u8> {
        self.git_bytes(&["ls-files", "--stage", "-z"])
    }

    fn open(&self) -> Arc<WorktreeSession> {
        self.open_with_id("test-worktree")
    }

    fn open_with_id(&self, id: &str) -> Arc<WorktreeSession> {
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
        WorktreeSession::open(id, &self.repo, shell_env, Arc::new(CommandLog::default())).unwrap()
    }
}

fn entry<'a>(snapshot: &'a ChangesSnapshot, path: &str, changes: ChangesKind) -> &'a ChangeEntry {
    snapshot
        .entries
        .iter()
        .find(|entry| entry.path == path && entry.changes == changes)
        .unwrap_or_else(|| panic!("missing {changes:?} entry for {path}: {snapshot:?}"))
}

fn actions(
    session: &WorktreeSession,
    path: &str,
    changes: ChangesKind,
    view: ViewMode,
) -> Vec<HunkAction> {
    let slice = session.rows(path, changes, view, 0, u32::MAX).unwrap();
    let mut actions = Vec::new();
    for row in slice.rows {
        if row.kind == RowKind::Hunk {
            let action = row
                .hunk_action
                .expect("ordinary tracked text hunk must have an action");
            assert_eq!(action.worktree_id, slice.worktree_id);
            assert_eq!(action.path, path);
            assert_eq!(action.changes, changes);
            assert_eq!(action.fingerprint, slice.entry.fingerprint);
            assert_eq!(action.snapshot_revision, slice.entry.snapshot_revision);
            assert!(!action.hunk_id.is_empty());
            actions.push(action);
        } else {
            assert!(
                row.hunk_action.is_none(),
                "content rows are not hunk write targets"
            );
        }
    }
    assert!(
        !actions.is_empty(),
        "fixture must contain at least one actionable hunk"
    );
    actions
}

fn first_action(session: &WorktreeSession, path: &str, changes: ChangesKind) -> HunkAction {
    actions(session, path, changes, ViewMode::Unified).remove(0)
}

fn stage(session: &WorktreeSession, action: &HunkAction) -> ChangesSnapshot {
    session
        .stage_hunk(&action.path, &action.fingerprint, &action.hunk_id)
        .unwrap()
}

fn unstage(session: &WorktreeSession, action: &HunkAction) -> ChangesSnapshot {
    session
        .unstage_hunk(&action.path, &action.fingerprint, &action.hunk_id)
        .unwrap()
}

fn assert_rejected(error: AppError) {
    assert_eq!(error.kind, ErrorKind::Rejected, "{error}");
}

fn assert_stale(error: AppError) {
    assert_eq!(error.kind, ErrorKind::Rejected, "{error}");
    assert_eq!(error.to_string(), "文件已变化，请重新确认");
}

fn lines_with_edits(edits: &[usize]) -> Vec<u8> {
    (1..=90)
        .map(|line| {
            if edits.contains(&line) {
                format!("changed {line:03}\n")
            } else {
                format!("line {line:03}\n")
            }
        })
        .collect::<String>()
        .into_bytes()
}

fn assert_no_hunk_access(ctx: &TestContext, session: &WorktreeSession, target: &ChangeEntry) {
    let before = ctx.index_state();
    for view in [ViewMode::Split, ViewMode::Unified] {
        // Some readonly types intentionally refuse to load a diff at all.
        if let Ok(slice) = session.rows(&target.path, target.changes, view, 0, u32::MAX) {
            assert!(
                slice.rows.iter().all(|row| row.hunk_action.is_none()),
                "{target:?}"
            );
        }
    }
    assert_rejected(
        session
            .stage_hunk(&target.path, &target.fingerprint, "forged-hunk")
            .unwrap_err(),
    );
    assert_rejected(
        session
            .unstage_hunk(&target.path, &target.fingerprint, "forged-hunk")
            .unwrap_err(),
    );
    assert_eq!(
        ctx.index_state(),
        before,
        "unsupported target must not change index"
    );
}

#[test]
fn t02_first_middle_last_hunks_keep_partial_staging_and_other_files() {
    let ctx = TestContext::new();
    let base = lines_with_edits(&[]);
    let worktree = lines_with_edits(&[5, 45, 85]);
    ctx.write("target.txt", &base);
    ctx.write("other.txt", b"other base\n");
    ctx.write("clean.txt", b"clean\n");
    for path in ["target.txt", "other.txt", "clean.txt"] {
        ctx.add(path);
    }
    ctx.commit();
    ctx.write("other.txt", b"other staged\n");
    ctx.add("other.txt");
    ctx.write("other.txt", b"other staged and unstaged\n");
    ctx.write("target.txt", &worktree);
    let other_index = ctx.index_entry("other.txt");
    let clean_index = ctx.index_entry("clean.txt");
    let session = ctx.open();
    session.refresh().unwrap();

    let split = actions(
        &session,
        "target.txt",
        ChangesKind::Unstaged,
        ViewMode::Split,
    );
    let unified = actions(
        &session,
        "target.txt",
        ChangesKind::Unstaged,
        ViewMode::Unified,
    );
    assert_eq!(split.len(), 3);
    assert_eq!(
        split
            .iter()
            .map(|action| &action.hunk_id)
            .collect::<Vec<_>>(),
        unified
            .iter()
            .map(|action| &action.hunk_id)
            .collect::<Vec<_>>(),
        "both layouts must refer to the same Git hunks"
    );

    let mut staged = Vec::new();
    for (selected_index, line, view) in [
        (1, 45, ViewMode::Split),
        (0, 5, ViewMode::Unified),
        (0, 85, ViewMode::Split),
    ] {
        let targets = actions(&session, "target.txt", ChangesKind::Unstaged, view);
        let after = stage(&session, &targets[selected_index]);
        staged.push(line);
        assert_eq!(
            ctx.index_bytes("target.txt"),
            Some(lines_with_edits(&staged))
        );
        assert_eq!(ctx.read("target.txt"), worktree);
        assert_eq!(ctx.index_entry("other.txt"), other_index);
        assert_eq!(ctx.index_entry("clean.txt"), clean_index);
        assert_eq!(ctx.read("other.txt"), b"other staged and unstaged\n");
        assert_eq!(ctx.read("clean.txt"), b"clean\n");
        if staged.len() < 3 {
            entry(&after, "target.txt", ChangesKind::Staged);
            entry(&after, "target.txt", ChangesKind::Unstaged);
        }
    }

    for (selected_index, line, view) in [
        (1, 45, ViewMode::Unified),
        (0, 5, ViewMode::Split),
        (0, 85, ViewMode::Unified),
    ] {
        let targets = actions(&session, "target.txt", ChangesKind::Staged, view);
        unstage(&session, &targets[selected_index]);
        staged.retain(|value| *value != line);
        assert_eq!(
            ctx.index_bytes("target.txt"),
            Some(lines_with_edits(&staged))
        );
        assert_eq!(ctx.read("target.txt"), worktree);
        assert_eq!(ctx.index_entry("other.txt"), other_index);
        assert_eq!(ctx.index_entry("clean.txt"), clean_index);
    }
    assert_eq!(ctx.index_bytes("target.txt"), Some(base));
}

fn inserted_deleted_lines(first: bool, middle: bool, last: bool) -> Vec<u8> {
    let mut result = Vec::new();
    for line in 1..=90 {
        if first && line == 5 {
            result.extend_from_slice(b"inserted before five\n");
        }
        if !(middle && line == 45) {
            result.extend_from_slice(format!("line {line:03}\n").as_bytes());
        }
        if last && line == 85 {
            result.extend_from_slice(b"inserted after eighty-five\n");
        }
    }
    result
}

#[test]
fn t02_pure_additions_deletions_and_shifted_line_numbers() {
    let ctx = TestContext::new();
    ctx.track("offsets.txt", &inserted_deleted_lines(false, false, false));
    let worktree = inserted_deleted_lines(true, true, true);
    ctx.write("offsets.txt", &worktree);
    let session = ctx.open();
    session.refresh().unwrap();
    assert_eq!(
        actions(
            &session,
            "offsets.txt",
            ChangesKind::Unstaged,
            ViewMode::Split
        )
        .len(),
        3
    );

    for expected in [
        (true, false, false),
        (true, true, false),
        (true, true, true),
    ] {
        stage(
            &session,
            &first_action(&session, "offsets.txt", ChangesKind::Unstaged),
        );
        assert_eq!(
            ctx.index_bytes("offsets.txt"),
            Some(inserted_deleted_lines(expected.0, expected.1, expected.2))
        );
        assert_eq!(ctx.read("offsets.txt"), worktree);
    }
    for expected in [
        (false, true, true),
        (false, false, true),
        (false, false, false),
    ] {
        unstage(
            &session,
            &first_action(&session, "offsets.txt", ChangesKind::Staged),
        );
        assert_eq!(
            ctx.index_bytes("offsets.txt"),
            Some(inserted_deleted_lines(expected.0, expected.1, expected.2))
        );
        assert_eq!(ctx.read("offsets.txt"), worktree);
    }
}

#[test]
fn t02_deleted_file_hunk_changes_only_index_existence() {
    let ctx = TestContext::new();
    let original = b"a deleted file\nsecond line\n";
    ctx.track("deleted.txt", original);
    fs::remove_file(ctx.repo.join("deleted.txt")).unwrap();
    let session = ctx.open();
    session.refresh().unwrap();
    stage(
        &session,
        &first_action(&session, "deleted.txt", ChangesKind::Unstaged),
    );
    assert!(ctx.index_entry("deleted.txt").is_none());
    assert!(!ctx.repo.join("deleted.txt").exists());
    unstage(
        &session,
        &first_action(&session, "deleted.txt", ChangesKind::Staged),
    );
    assert_eq!(ctx.index_bytes("deleted.txt"), Some(original.to_vec()));
    assert!(!ctx.repo.join("deleted.txt").exists());
}

#[test]
fn t02_added_file_hunk_with_and_without_head() {
    for has_head in [false, true] {
        let ctx = TestContext::new();
        if has_head {
            ctx.track("anchor.txt", b"anchor\n");
        }
        ctx.write("new.txt", b"new file\n");
        ctx.add("new.txt");
        let worktree = b"new file\nadditional unstaged line\n";
        ctx.write("new.txt", worktree);
        let session = ctx.open();
        session.refresh().unwrap();
        stage(
            &session,
            &first_action(&session, "new.txt", ChangesKind::Unstaged),
        );
        assert_eq!(ctx.index_bytes("new.txt"), Some(worktree.to_vec()));
        let after = unstage(
            &session,
            &first_action(&session, "new.txt", ChangesKind::Staged),
        );
        assert!(ctx.index_entry("new.txt").is_none());
        assert_eq!(ctx.read("new.txt"), worktree);
        entry(&after, "new.txt", ChangesKind::Untracked);
        if has_head {
            assert_eq!(ctx.index_bytes("anchor.txt"), Some(b"anchor\n".to_vec()));
        }
    }
}

#[test]
fn t02_raw_bytes_survive_crlf_missing_eol_and_non_utf8_content() {
    let cases: &[(&str, &[u8], &[u8])] = &[
        ("crlf.txt", b"old\r\ncontext\r\n", b"new\r\ncontext\r\n"),
        ("no-eol.txt", b"context\nold", b"context\nnew"),
        ("remove-eol.txt", b"context\nold\n", b"context\nnew"),
        ("add-eol.txt", b"context\nold", b"context\nnew\n"),
        (
            "gbk.txt",
            b"\xd6\xd0\xce\xc4 old\ncontext\n",
            b"\xd6\xd0\xce\xc4 new\ncontext\n",
        ),
        ("empty.txt", b"", b"now nonempty\n"),
    ];
    for &(path, before, after) in cases {
        let ctx = TestContext::new();
        ctx.track(path, before);
        ctx.write(path, after);
        let session = ctx.open();
        session.refresh().unwrap();
        let target = first_action(&session, path, ChangesKind::Unstaged);
        stage(&session, &target);
        assert_eq!(
            ctx.index_bytes(path),
            Some(after.to_vec()),
            "raw index bytes after staging {path}"
        );
        assert_eq!(ctx.read(path), after);
        unstage(&session, &first_action(&session, path, ChangesKind::Staged));
        assert_eq!(
            ctx.index_bytes(path),
            Some(before.to_vec()),
            "raw index bytes after unstaging {path}"
        );
        assert_eq!(ctx.read(path), after);
    }
}

#[test]
fn t02_hunk_preserves_whitespace_despite_repository_apply_defaults() {
    let ctx = TestContext::new();
    ctx.git(&["config", "apply.whitespace", "fix"]);
    ctx.git(&["config", "apply.ignoreWhitespace", "change"]);
    let before = b"anchor\nold trailing \t  \ncontext\n";
    let after = b"anchor\nnew mixed \t \t  \n \tadded with trailing \t \ncontext\n";
    ctx.track("whitespace.txt", before);
    ctx.write("whitespace.txt", after);
    let session = ctx.open();
    session.refresh().unwrap();

    stage(
        &session,
        &first_action(&session, "whitespace.txt", ChangesKind::Unstaged),
    );
    assert_eq!(ctx.index_bytes("whitespace.txt"), Some(after.to_vec()));
    assert_eq!(ctx.read("whitespace.txt"), after);
    unstage(
        &session,
        &first_action(&session, "whitespace.txt", ChangesKind::Staged),
    );
    assert_eq!(ctx.index_bytes("whitespace.txt"), Some(before.to_vec()));
    assert_eq!(ctx.read("whitespace.txt"), after);
    assert_eq!(
        ctx.git(&["config", "--get", "apply.whitespace"]).trim(),
        "fix"
    );
    assert_eq!(
        ctx.git(&["config", "--get", "apply.ignoreWhitespace"])
            .trim(),
        "change"
    );
}

#[test]
fn t02_patch_header_lookalikes_are_body_content() {
    let ctx = TestContext::new();
    let before = b"intro\n-- old body, not a file path\ntrailer\n";
    let after = b"intro\n++ new body, not a file path\ntrailer\n";
    ctx.track("body.txt", before);
    ctx.write("body.txt", after);
    let session = ctx.open();
    session.refresh().unwrap();
    let slice = session
        .rows(
            "body.txt",
            ChangesKind::Unstaged,
            ViewMode::Unified,
            0,
            u32::MAX,
        )
        .unwrap();
    assert!(
        slice
            .rows
            .iter()
            .any(|row| row.text.as_deref() == Some("-- old body, not a file path"))
    );
    assert!(
        slice
            .rows
            .iter()
            .any(|row| row.text.as_deref() == Some("++ new body, not a file path"))
    );
    stage(
        &session,
        &first_action(&session, "body.txt", ChangesKind::Unstaged),
    );
    assert_eq!(ctx.index_bytes("body.txt"), Some(after.to_vec()));
    unstage(
        &session,
        &first_action(&session, "body.txt", ChangesKind::Staged),
    );
    assert_eq!(ctx.index_bytes("body.txt"), Some(before.to_vec()));
    assert_eq!(ctx.read("body.txt"), after);
}

#[test]
fn t02_patch_larger_than_a_pipe_buffer_preserves_all_bytes() {
    let ctx = TestContext::new();
    let before: Vec<u8> = (0..512)
        .map(|line| format!("old {line:04} {}\n", "x".repeat(256)))
        .collect::<String>()
        .into_bytes();
    let after: Vec<u8> = (0..512)
        .map(|line| format!("new {line:04} {}\n", "y".repeat(256)))
        .collect::<String>()
        .into_bytes();
    ctx.track("large.txt", &before);
    ctx.write("large.txt", &after);
    let session = ctx.open();
    session.refresh().unwrap();
    let targets = actions(
        &session,
        "large.txt",
        ChangesKind::Unstaged,
        ViewMode::Unified,
    );
    assert_eq!(targets.len(), 1);
    stage(&session, &targets[0]);
    assert_eq!(ctx.index_bytes("large.txt"), Some(after.clone()));
    unstage(
        &session,
        &first_action(&session, "large.txt", ChangesKind::Staged),
    );
    assert_eq!(ctx.index_bytes("large.txt"), Some(before));
    assert_eq!(ctx.read("large.txt"), after);
}

fn assert_literal_hunk_path(path: &str) {
    let ctx = TestContext::new();
    for name in [path, "a.txt", "b.txt", "xy.txt"] {
        ctx.write(name, b"base\n");
        ctx.add(name);
    }
    ctx.commit();
    for name in [path, "a.txt", "b.txt", "xy.txt"] {
        ctx.write(name, b"changed\n");
    }
    let decoys: Vec<_> = ["a.txt", "b.txt", "xy.txt"]
        .map(|name| (name, ctx.index_entry(name)))
        .into();
    let session = ctx.open();
    session.refresh().unwrap();
    stage(
        &session,
        &first_action(&session, path, ChangesKind::Unstaged),
    );
    assert_eq!(ctx.index_bytes(path), Some(b"changed\n".to_vec()));
    for (name, saved) in &decoys {
        assert_eq!(
            ctx.index_entry(name),
            *saved,
            "literal path must not stage {name}"
        );
        assert_eq!(ctx.read(name), b"changed\n");
    }
    unstage(&session, &first_action(&session, path, ChangesKind::Staged));
    assert_eq!(ctx.index_bytes(path), Some(b"base\n".to_vec()));
    assert_eq!(ctx.read(path), b"changed\n");
    for (name, saved) in &decoys {
        assert_eq!(ctx.index_entry(name), *saved);
    }
}

#[test]
fn t14_unicode_spaces_and_windows_valid_literal_pathspecs() {
    for path in ["目录/空 格.txt", "[ab].txt", "-leading.txt"] {
        assert_literal_hunk_path(path);
    }
}

#[cfg(unix)]
#[test]
fn t14_unix_only_literal_colon_quote_backslash_and_star_paths() {
    for path in [
        "0:x.txt",
        ":(glob)*.txt",
        "q\"x.txt",
        "back\\slash.txt",
        "x*.txt",
    ] {
        assert_literal_hunk_path(path);
    }
}

#[cfg(unix)]
#[test]
fn t02_content_hunk_does_not_stage_or_unstage_independent_mode_change() {
    use std::os::unix::fs::PermissionsExt;
    let ctx = TestContext::new();
    ctx.git(&["config", "core.filemode", "true"]);
    ctx.write("script.sh", b"#!/bin/sh\necho before\n");
    let path = ctx.repo.join("script.sh");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    ctx.add("script.sh");
    ctx.commit();
    let worktree = b"#!/bin/sh\necho after\n";
    ctx.write("script.sh", worktree);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    let worktree_mode = fs::metadata(&path).unwrap().permissions().mode();
    let session = ctx.open();
    session.refresh().unwrap();
    stage(
        &session,
        &first_action(&session, "script.sh", ChangesKind::Unstaged),
    );
    assert_eq!(ctx.index_entry("script.sh").unwrap().0, "100644");
    assert_eq!(ctx.index_bytes("script.sh"), Some(worktree.to_vec()));
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode(),
        worktree_mode
    );

    // A reverse hunk must preserve mode already staged independently.
    ctx.add("script.sh");
    session.refresh().unwrap();
    assert_eq!(ctx.index_entry("script.sh").unwrap().0, "100755");
    let snapshot = unstage(
        &session,
        &first_action(&session, "script.sh", ChangesKind::Staged),
    );
    assert_eq!(ctx.index_entry("script.sh").unwrap().0, "100755");
    assert_eq!(
        ctx.index_bytes("script.sh"),
        Some(b"#!/bin/sh\necho before\n".to_vec())
    );
    assert_eq!(ctx.read("script.sh"), worktree);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode(),
        worktree_mode
    );
    assert_eq!(
        entry(&snapshot, "script.sh", ChangesKind::Staged).hunk_count,
        0
    );
}

#[test]
fn t05_refresh_invalidates_tokens_even_when_file_fingerprint_is_unchanged() {
    let ctx = TestContext::new();
    ctx.track("file.txt", b"before\n");
    ctx.write("file.txt", b"after\n");
    let session = ctx.open();
    session.refresh().unwrap();
    let old = first_action(&session, "file.txt", ChangesKind::Unstaged);
    session.refresh().unwrap();
    let current = first_action(&session, "file.txt", ChangesKind::Unstaged);
    assert_eq!(old.fingerprint, current.fingerprint);
    assert_ne!(old.snapshot_revision, current.snapshot_revision);
    assert_ne!(old.hunk_id, current.hunk_id);
    let index = ctx.index_state();
    assert_stale(
        session
            .stage_hunk(&old.path, &old.fingerprint, &old.hunk_id)
            .unwrap_err(),
    );
    assert_eq!(ctx.index_state(), index);
    stage(&session, &current);
    assert_eq!(ctx.index_bytes("file.txt"), Some(b"after\n".to_vec()));
}

#[test]
fn t14_tokens_are_bound_to_path_group_fingerprint_and_session() {
    let ctx = TestContext::new();
    for path in ["a.txt", "b.txt"] {
        ctx.write(path, b"before\n");
        ctx.add(path);
    }
    ctx.commit();
    for path in ["a.txt", "b.txt"] {
        ctx.write(path, b"after\n");
    }
    let session = ctx.open();
    session.refresh().unwrap();
    let action = first_action(&session, "a.txt", ChangesKind::Unstaged);
    let b_action = first_action(&session, "b.txt", ChangesKind::Unstaged);
    let index = ctx.index_state();
    assert_rejected(
        session
            .stage_hunk("b.txt", &b_action.fingerprint, &action.hunk_id)
            .unwrap_err(),
    );
    assert_rejected(
        session
            .stage_hunk("../a.txt", &action.fingerprint, &action.hunk_id)
            .unwrap_err(),
    );
    assert_rejected(
        session
            .stage_hunk("a.txt", "incorrect-fingerprint", &action.hunk_id)
            .unwrap_err(),
    );
    assert_rejected(
        session
            .stage_hunk("a.txt", &action.fingerprint, "nonexistent-token")
            .unwrap_err(),
    );
    assert_rejected(
        session
            .unstage_hunk("a.txt", &action.fingerprint, &action.hunk_id)
            .unwrap_err(),
    );

    // Reopening the same worktree with the same public ID must not revive another session's token.
    let reopened = ctx.open();
    reopened.refresh().unwrap();
    let reopened_action = first_action(&reopened, "a.txt", ChangesKind::Unstaged);
    assert_eq!(action.fingerprint, reopened_action.fingerprint);
    assert_ne!(action.hunk_id, reopened_action.hunk_id);
    assert_rejected(
        reopened
            .stage_hunk("a.txt", &action.fingerprint, &action.hunk_id)
            .unwrap_err(),
    );

    let other = TestContext::new();
    other.track("a.txt", b"before\n");
    other.write("a.txt", b"after\n");
    let other_session = other.open_with_id("another-worktree");
    other_session.refresh().unwrap();
    let other_index = other.index_state();
    assert_rejected(
        other_session
            .stage_hunk("a.txt", &action.fingerprint, &action.hunk_id)
            .unwrap_err(),
    );
    assert_eq!(other.index_state(), other_index);
    assert_eq!(ctx.index_state(), index);
    assert_eq!(ctx.read("a.txt"), b"after\n");
    stage(&session, &action);
    let staged = first_action(&session, "a.txt", ChangesKind::Staged);
    let staged_index = ctx.index_state();
    assert_rejected(
        session
            .stage_hunk("a.txt", &staged.fingerprint, &staged.hunk_id)
            .unwrap_err(),
    );
    assert_eq!(ctx.index_state(), staged_index);
}

#[test]
fn t05_other_hunk_change_rejects_both_sides_but_unrelated_file_change_does_not() {
    for is_stage in [true, false] {
        let ctx = TestContext::new();
        ctx.track("target.txt", &lines_with_edits(&[]));
        ctx.write("target.txt", &lines_with_edits(&[5, 45]));
        if !is_stage {
            ctx.add("target.txt");
        }
        let session = ctx.open();
        session.refresh().unwrap();
        let group = if is_stage {
            ChangesKind::Unstaged
        } else {
            ChangesKind::Staged
        };
        let old = first_action(&session, "target.txt", group);
        let changed = lines_with_edits(&[5, 45, 85]);
        ctx.write("target.txt", &changed);
        if !is_stage {
            ctx.add("target.txt");
        }
        let index = ctx.index_state();
        let result = if is_stage {
            session.stage_hunk(&old.path, &old.fingerprint, &old.hunk_id)
        } else {
            session.unstage_hunk(&old.path, &old.fingerprint, &old.hunk_id)
        };
        assert_stale(result.unwrap_err());
        assert_eq!(ctx.index_state(), index);
        assert_eq!(ctx.read("target.txt"), changed);
    }

    let ctx = TestContext::new();
    ctx.write("target.txt", &lines_with_edits(&[]));
    ctx.write("other.txt", b"base\n");
    ctx.add("target.txt");
    ctx.add("other.txt");
    ctx.commit();
    ctx.write("target.txt", &lines_with_edits(&[5, 45]));
    let session = ctx.open();
    session.refresh().unwrap();
    let action = first_action(&session, "target.txt", ChangesKind::Unstaged);
    let other_index = ctx.index_entry("other.txt");
    ctx.write("other.txt", b"external modification\n");
    stage(&session, &action);
    assert_eq!(ctx.index_bytes("target.txt"), Some(lines_with_edits(&[5])));
    assert_eq!(ctx.index_entry("other.txt"), other_index);
    assert_eq!(ctx.read("other.txt"), b"external modification\n");
}

#[test]
fn t05_removed_or_replaced_target_rejects_old_hunk() {
    for replacement in [None, Some(b"binary\0replacement\n".as_slice())] {
        let ctx = TestContext::new();
        ctx.track("target.txt", b"base\n");
        ctx.write("target.txt", b"changed\n");
        let session = ctx.open();
        session.refresh().unwrap();
        let action = first_action(&session, "target.txt", ChangesKind::Unstaged);
        match replacement {
            Some(bytes) => ctx.write("target.txt", bytes),
            None => fs::rename(ctx.repo.join("target.txt"), ctx.repo.join("moved.txt")).unwrap(),
        }
        let index = ctx.index_state();
        assert_stale(
            session
                .stage_hunk(&action.path, &action.fingerprint, &action.hunk_id)
                .unwrap_err(),
        );
        assert_eq!(ctx.index_state(), index);
        match replacement {
            Some(bytes) => assert_eq!(ctx.read("target.txt"), bytes),
            None => {
                assert!(!ctx.repo.join("target.txt").exists());
                assert_eq!(ctx.read("moved.txt"), b"changed\n");
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn t05_mode_and_symlink_type_changes_reject_old_hunk() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for replace_with_link in [false, true] {
        let ctx = TestContext::new();
        ctx.git(&["config", "core.filemode", "true"]);
        ctx.write("target.txt", b"base\n");
        fs::set_permissions(
            ctx.repo.join("target.txt"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        ctx.add("target.txt");
        ctx.commit();
        ctx.write("target.txt", b"changed\n");
        let session = ctx.open();
        session.refresh().unwrap();
        let action = first_action(&session, "target.txt", ChangesKind::Unstaged);
        if replace_with_link {
            ctx.write("destination.txt", b"do not touch\n");
            fs::remove_file(ctx.repo.join("target.txt")).unwrap();
            symlink("destination.txt", ctx.repo.join("target.txt")).unwrap();
        } else {
            fs::set_permissions(
                ctx.repo.join("target.txt"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        let index = ctx.index_state();
        assert_rejected(
            session
                .stage_hunk(&action.path, &action.fingerprint, &action.hunk_id)
                .unwrap_err(),
        );
        assert_eq!(ctx.index_state(), index);
        if replace_with_link {
            assert_eq!(
                fs::read_link(ctx.repo.join("target.txt")).unwrap(),
                PathBuf::from("destination.txt")
            );
            assert_eq!(ctx.read("destination.txt"), b"do not touch\n");
        } else {
            assert_eq!(
                fs::metadata(ctx.repo.join("target.txt"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o755
            );
        }
    }
}

#[test]
fn t03_binary_untracked_and_rename_never_expose_hunk_actions() {
    let ctx = TestContext::new();
    ctx.write("binary.bin", b"base\0binary\n");
    ctx.write("rename-old.txt", &lines_with_edits(&[]));
    ctx.add("binary.bin");
    ctx.add("rename-old.txt");
    ctx.commit();
    ctx.write("binary.bin", b"modified\0binary\n");
    ctx.write("untracked.txt", b"ordinary untracked text\n");
    ctx.git(&["mv", "rename-old.txt", "rename-new.txt"]);
    ctx.write("rename-new.txt", &lines_with_edits(&[45]));
    ctx.add("rename-new.txt");
    let session = ctx.open();
    let snapshot = session.refresh().unwrap();
    let binary = entry(&snapshot, "binary.bin", ChangesKind::Unstaged);
    assert!(binary.binary);
    let renamed = entry(&snapshot, "rename-new.txt", ChangesKind::Staged);
    assert_eq!(renamed.status, FileStatus::Renamed);
    assert_eq!(renamed.original_path.as_deref(), Some("rename-old.txt"));
    for target in [
        binary,
        renamed,
        entry(&snapshot, "untracked.txt", ChangesKind::Untracked),
    ] {
        assert_no_hunk_access(&ctx, &session, target);
    }
    assert!(!ctx.repo.join("rename-old.txt").exists());
    assert_eq!(ctx.read("rename-new.txt"), lines_with_edits(&[45]));
    assert_eq!(ctx.read("binary.bin"), b"modified\0binary\n");
    assert_eq!(ctx.read("untracked.txt"), b"ordinary untracked text\n");
}

#[cfg(unix)]
#[test]
fn t03_mode_only_changes_have_no_hunk_actions_on_either_side() {
    use std::os::unix::fs::PermissionsExt;
    let ctx = TestContext::new();
    ctx.git(&["config", "core.filemode", "true"]);
    ctx.write("mode.sh", b"#!/bin/sh\n");
    fs::set_permissions(ctx.repo.join("mode.sh"), fs::Permissions::from_mode(0o644)).unwrap();
    ctx.add("mode.sh");
    ctx.commit();
    fs::set_permissions(ctx.repo.join("mode.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    let session = ctx.open();
    let snapshot = session.refresh().unwrap();
    let target = entry(&snapshot, "mode.sh", ChangesKind::Unstaged);
    assert_eq!(target.hunk_count, 0);
    assert_no_hunk_access(&ctx, &session, target);
    ctx.add("mode.sh");
    let snapshot = session.refresh().unwrap();
    assert_no_hunk_access(
        &ctx,
        &session,
        entry(&snapshot, "mode.sh", ChangesKind::Staged),
    );
    assert_eq!(ctx.read("mode.sh"), b"#!/bin/sh\n");
    assert_eq!(
        fs::metadata(ctx.repo.join("mode.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
}

#[test]
fn t03_submodule_nested_repo_and_conflict_are_readonly_hunk_targets() {
    let ctx = TestContext::new();
    ctx.track("base.txt", b"base\n");
    let commit = ctx.git(&["rev-parse", "HEAD"]);
    ctx.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        "160000",
        commit.trim(),
        "submodule",
    ]);
    let nested = ctx.repo.join("nested");
    fs::create_dir_all(&nested).unwrap();
    let init = ctx
        .command()
        .current_dir(&nested)
        .args(["init", "-b", "main"])
        .output()
        .unwrap();
    assert!(init.status.success(), "nested fixture init must succeed");
    fs::write(nested.join("nested.txt"), b"nested\n").unwrap();
    let session = ctx.open();
    let snapshot = session.refresh().unwrap();
    let submodule = snapshot
        .entries
        .iter()
        .find(|target| target.path == "submodule")
        .expect("submodule entry");
    assert_eq!(submodule.readonly_reason, Some(ReadonlyReason::Submodule));
    assert_no_hunk_access(&ctx, &session, submodule);
    let nested = snapshot
        .entries
        .iter()
        .find(|target| target.path.starts_with("nested"))
        .expect("nested entry");
    assert_eq!(nested.readonly_reason, Some(ReadonlyReason::NestedRepo));
    assert_no_hunk_access(&ctx, &session, nested);

    let conflict_ctx = TestContext::new();
    conflict_ctx.track("conflict.txt", b"base\n");
    conflict_ctx.git(&["branch", "feature"]);
    conflict_ctx.write("conflict.txt", b"main\n");
    conflict_ctx.add("conflict.txt");
    conflict_ctx.commit();
    conflict_ctx.git(&["checkout", "feature"]);
    conflict_ctx.write("conflict.txt", b"feature\n");
    conflict_ctx.add("conflict.txt");
    conflict_ctx.commit();
    let merge = conflict_ctx.git_output(&["merge", "--no-edit", "main"]);
    assert!(
        !merge.status.success(),
        "fixture must produce a real merge conflict"
    );
    let session = conflict_ctx.open();
    let snapshot = session.refresh().unwrap();
    let target = snapshot
        .entries
        .iter()
        .find(|target| target.path == "conflict.txt")
        .unwrap();
    assert!(target.unmerged);
    assert_eq!(target.readonly_reason, Some(ReadonlyReason::Conflict));
    let worktree = conflict_ctx.read("conflict.txt");
    assert_no_hunk_access(&conflict_ctx, &session, target);
    assert_eq!(conflict_ctx.read("conflict.txt"), worktree);
}

#[cfg(unix)]
#[test]
fn t03_symlink_non_utf8_path_and_unreadable_file_are_readonly() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{PermissionsExt, symlink};
    let ctx = TestContext::new();
    ctx.write("base.txt", b"base\n");
    ctx.write("other.txt", b"other\n");
    symlink("base.txt", ctx.repo.join("link.txt")).unwrap();
    ctx.add("base.txt");
    ctx.add("other.txt");
    ctx.add("link.txt");
    ctx.commit();
    fs::remove_file(ctx.repo.join("link.txt")).unwrap();
    symlink("other.txt", ctx.repo.join("link.txt")).unwrap();
    let oid = ctx.index_entry("base.txt").unwrap().1;
    let invalid_path = OsStr::from_bytes(b"invalid-\x80.txt");
    let output = ctx
        .command()
        .args(["update-index", "--add", "--cacheinfo", "100644", &oid])
        .arg(invalid_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "invalid UTF-8 path fixture must be present in index"
    );
    ctx.write("unreadable.txt", b"private\n");
    fs::set_permissions(
        ctx.repo.join("unreadable.txt"),
        fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    let session = ctx.open();
    let snapshot = session.refresh().unwrap();
    let link = entry(&snapshot, "link.txt", ChangesKind::Unstaged);
    assert_eq!(link.readonly_reason, Some(ReadonlyReason::Symlink));
    assert_no_hunk_access(&ctx, &session, link);
    let invalid = snapshot
        .entries
        .iter()
        .find(|target| target.path.contains('\u{FFFD}'))
        .expect("non-UTF-8 entry");
    assert_eq!(invalid.readonly_reason, Some(ReadonlyReason::NonUtf8Path));
    assert_no_hunk_access(&ctx, &session, invalid);
    let unreadable = entry(&snapshot, "unreadable.txt", ChangesKind::Untracked);
    assert_eq!(unreadable.readonly_reason, Some(ReadonlyReason::Unreadable));
    assert_no_hunk_access(&ctx, &session, unreadable);
    fs::set_permissions(
        ctx.repo.join("unreadable.txt"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert_eq!(ctx.read("unreadable.txt"), b"private\n");
    assert_eq!(
        fs::read_link(ctx.repo.join("link.txt")).unwrap(),
        PathBuf::from("other.txt")
    );
    assert_eq!(ctx.read("base.txt"), b"base\n");
    assert_eq!(ctx.read("other.txt"), b"other\n");
}

#[test]
fn t06_hunk_and_whole_file_operations_share_nonqueued_write_guard() {
    let ctx = TestContext::new();
    ctx.track("partial.txt", &lines_with_edits(&[]));
    ctx.write("partial.txt", &lines_with_edits(&[5]));
    ctx.add("partial.txt");
    let worktree = lines_with_edits(&[5, 45]);
    ctx.write("partial.txt", &worktree);
    let session = ctx.open();
    session.refresh().unwrap();
    let staged = first_action(&session, "partial.txt", ChangesKind::Staged);
    let unstaged = first_action(&session, "partial.txt", ChangesKind::Unstaged);
    let index = ctx.index_state();
    let guard = session.acquire_write_lock().unwrap();
    let logged_before = session.git.log.snapshot().len();
    for result in [
        session.stage_hunk(&unstaged.path, &unstaged.fingerprint, &unstaged.hunk_id),
        session.unstage_hunk(&staged.path, &staged.fingerprint, &staged.hunk_id),
        session.stage_file(&unstaged.path, &unstaged.fingerprint),
        session.unstage_file(&staged.path, &staged.fingerprint),
    ] {
        let error = result.unwrap_err();
        assert_eq!(error.kind, ErrorKind::Rejected);
        assert!(error.to_string().contains("正在进行的写操作"), "{error}");
    }
    assert_eq!(
        session.git.log.snapshot().len(),
        logged_before,
        "busy operations must not execute or queue Git"
    );
    assert_eq!(ctx.index_state(), index);
    assert_eq!(ctx.read("partial.txt"), worktree);
    drop(guard);
    stage(&session, &unstaged);
    assert_eq!(ctx.index_bytes("partial.txt"), Some(worktree));
}

fn apply_records(session: &WorktreeSession) -> Vec<CommandRecord> {
    session
        .git
        .log
        .snapshot()
        .into_iter()
        .filter(|record| record.args.iter().any(|arg| arg == "apply"))
        .collect()
}

fn wait_for_failed_apply(session: &WorktreeSession) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if apply_records(session)
            .iter()
            .any(|record| record.exit_code != 0 && record.stderr_excerpt.contains("index.lock"))
        {
            return true;
        }
        // This waits for an observed event, not a guessed Git execution duration.
        std::thread::sleep(Duration::from_millis(1));
    }
    false
}

#[test]
fn t06_index_lock_is_retried_without_deleting_or_overwriting_it() {
    let ctx = TestContext::new();
    ctx.track("target.txt", b"base\n");
    ctx.write("target.txt", b"changed\n");
    let session = ctx.open();
    session.refresh().unwrap();
    let target = first_action(&session, "target.txt", ChangesKind::Unstaged);
    let lock = ctx.repo.join(".git/index.lock");
    let sentinel = b"owned by an external Git operation";
    fs::write(&lock, sentinel).unwrap();
    let index = ctx.index_state();
    let error = session
        .stage_hunk(&target.path, &target.fingerprint, &target.hunk_id)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Rejected);
    assert!(error.to_string().contains("index.lock"), "{error}");
    assert_eq!(fs::read(&lock).unwrap(), sentinel);
    let attempts = apply_records(&session);
    assert!(attempts.len() >= 2, "the lock should cause a bounded retry");
    assert!(attempts.iter().all(|record| record.exit_code != 0));
    assert_eq!(ctx.index_state(), index);
    assert_eq!(ctx.read("target.txt"), b"changed\n");
}

#[test]
fn t06_lock_retry_rechecks_file_fingerprint_and_snapshot_token() {
    for refresh_only in [false, true] {
        let ctx = TestContext::new();
        ctx.track("target.txt", &lines_with_edits(&[]));
        ctx.write("target.txt", &lines_with_edits(&[5, 45]));
        let session = ctx.open();
        session.refresh().unwrap();
        let target = first_action(&session, "target.txt", ChangesKind::Unstaged);
        let lock = ctx.repo.join(".git/index.lock");
        let sentinel = b"external lock stays present";
        fs::write(&lock, sentinel).unwrap();
        let index = ctx.index_state();
        let writer_session = session.clone();
        let writer = std::thread::spawn(move || {
            writer_session.stage_hunk(&target.path, &target.fingerprint, &target.hunk_id)
        });
        let observed = wait_for_failed_apply(&session);
        if observed {
            if refresh_only {
                session.refresh().unwrap();
            } else {
                ctx.write("target.txt", &lines_with_edits(&[5, 45, 85]));
            }
        }
        let result = writer.join().expect("hunk writer thread must exit");
        assert!(
            observed,
            "must observe the first actual failed apply before modifying the fixture"
        );
        assert_stale(result.unwrap_err());
        assert_eq!(ctx.index_state(), index);
        assert_eq!(fs::read(&lock).unwrap(), sentinel);
        assert!(
            apply_records(&session)
                .iter()
                .all(|record| record.exit_code != 0)
        );
        let expected_edits: &[usize] = if refresh_only { &[5, 45] } else { &[5, 45, 85] };
        assert_eq!(ctx.read("target.txt"), lines_with_edits(expected_edits));
    }
}

/// R04-GUI1 / T03 / Q81：Git 识别的重命名只有整文件操作。
///
/// porcelain type-2 `RM`（已暂存改名 + 新路径有未暂存修改）会同时出现 staged `R`
/// 和 unstaged `M`，两者都保留 Git 给出的原路径；未暂存的 `M` 不能因此获得 hunk 能力。
/// 同时确认普通 `M` 文件的 hunk 仍然可用，且整文件操作不受影响。
#[test]
fn t03_git_rename_never_exposes_hunk_actions_on_either_side() {
    let ctx = TestContext::new();
    ctx.track("old.txt", b"alpha\nbeta\ngamma\n");
    ctx.track("plain.txt", b"one\ntwo\nthree\n");
    ctx.git(&["mv", "old.txt", "new.txt"]);
    // 新路径上再加未暂存文本修改：porcelain 变成 RM。
    ctx.write("new.txt", b"alpha\nBETA\ngamma\n");
    // 普通修改作为对照。
    ctx.write("plain.txt", b"one\nTWO\nthree\n");

    let session = ctx.open();
    let snapshot = session.refresh().unwrap();

    let staged = entry(&snapshot, "new.txt", ChangesKind::Staged);
    assert_eq!(staged.status, FileStatus::Renamed);
    assert_eq!(staged.original_path.as_deref(), Some("old.txt"));

    let unstaged = entry(&snapshot, "new.txt", ChangesKind::Unstaged);
    assert_eq!(unstaged.status, FileStatus::Modified, "用例前提：工作区侧是 M");
    assert_eq!(
        unstaged.original_path.as_deref(),
        Some("old.txt"),
        "用例前提：Git 的 rename 元数据保留在未暂存条目上"
    );

    // 两个变更组、两种视图都不得产生 hunk 动作，但 diff 本身仍然可见。
    for changes in [ChangesKind::Staged, ChangesKind::Unstaged] {
        for view in [ViewMode::Split, ViewMode::Unified] {
            let slice = session.rows("new.txt", changes, view, 0, u32::MAX).unwrap();
            assert!(
                slice.rows.iter().all(|row| row.hunk_action.is_none()),
                "rename 不得暴露 hunk 动作（{changes:?}/{view:?}）"
            );
            if changes == ChangesKind::Unstaged {
                assert!(
                    slice.rows.iter().any(|row| row.kind == RowKind::Hunk),
                    "未暂存侧的内容改动仍应显示为 diff（{view:?}）"
                );
            }
        }
    }

    // 后端必须拒绝伪造的 hunk 凭据，而不是只看前端是否渲染按钮。
    let stage_error = session
        .stage_hunk("new.txt", &unstaged.fingerprint, "forged-hunk")
        .expect_err("rename 的未暂存条目不得接受 hunk 暂存");
    assert_eq!(stage_error.to_string(), "这个文件不支持 hunk 操作");
    let unstage_error = session
        .unstage_hunk("new.txt", &staged.fingerprint, "forged-hunk")
        .expect_err("rename 的已暂存条目不得接受 hunk 取消暂存");
    assert_eq!(unstage_error.to_string(), "这个文件不支持 hunk 操作");

    // 普通 M 文件仍具备 hunk 能力。
    let plain = entry(&snapshot, "plain.txt", ChangesKind::Unstaged);
    let plain_actions = actions(&session, "plain.txt", ChangesKind::Unstaged, ViewMode::Split);
    assert_eq!(plain_actions.len(), 1, "普通修改必须仍有 hunk 动作");

    // 整文件操作对 rename 仍然可用：暂存未暂存侧的修改。
    let staged_after = session.stage_file("new.txt", &unstaged.fingerprint).unwrap();
    assert!(staged_after
        .entries
        .iter()
        .any(|entry| entry.path == "new.txt" && entry.changes == ChangesKind::Staged));
    let _ = plain;

    // 整文件取消暂存把改名退回，工作区文件仍在。
    let staged_entry = staged_after
        .entries
        .iter()
        .find(|entry| entry.path == "new.txt" && entry.changes == ChangesKind::Staged)
        .expect("暂存后应有 new.txt");
    session.unstage_file("new.txt", &staged_entry.fingerprint).unwrap();
    assert!(ctx.repo.join("new.txt").is_file(), "取消暂存不得删除工作区文件");
}
