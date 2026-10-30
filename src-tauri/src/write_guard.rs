//! Confirmation and recovery share the same paths, including ignored files
//! which a target tree can reintroduce. No write may silently narrow this set.
use std::fs;
use sha2::{Digest, Sha256};
use crate::error::{AppError, ErrorKind, Result};
use crate::git::{Git, status};
use crate::operation::{self, OperationKind};
use crate::recovery::{self, ProtectedRecovery, RecoverySource, RecoveryStore, WorktreeIdentity};
use crate::session::WorktreeSession;

/// Serializable file/index baseline shared by initial cleanup and later steps.
/// Keep these fields stable for persisted history plans.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct ConfirmedFiles {
    files: Vec<recovery::SavedFile>,
    index: Vec<Vec<u8>>,
}

impl ConfirmedFiles {
    pub(crate) fn capture(session: &WorktreeSession, paths: &[String]) -> Result<Self> {
        let output = session.git.run_ok(&["ls-files", "--stage", "-z"], "读取确认暂存状态")?;
        Ok(Self { files: recovery::capture_files(&session.path, paths)?,
            index: output.stdout.split(|byte| *byte == 0).filter(|row| !row.is_empty()).map(<[u8]>::to_vec).collect() })
    }

    fn paths(&self) -> Vec<String> { self.files.iter().map(|file| file.path.clone()).collect() }

    pub(crate) fn matches(&self, session: &WorktreeSession) -> Result<bool> {
        let after = Self::capture(session, &self.paths())?;
        Ok(self.files == after.files && self.index == after.index)
    }

    /// Only a completed Git step's declared paths may advance their baseline.
    pub(crate) fn advance(&self, session: &WorktreeSession, touched: &[String]) -> Result<Option<Self>> {
        let after = Self::capture(session, &self.paths())?;
        let owns = |path: &str| touched.iter().any(|item| item == path);
        let index_owned = |row: &&Vec<u8>| touched.iter().any(|path| path.as_bytes()
            == row.splitn(2, |byte| *byte == b'\t').nth(1).unwrap_or_default());
        if self.files.iter().zip(&after.files).any(|(before, after)| !owns(&before.path) && before != after)
            || self.index.iter().filter(|row| !index_owned(row)).ne(after.index.iter().filter(|row| !index_owned(row))) {
            return Ok(None);
        }
        Ok(Some(after))
    }

    fn checkout_expected(&self, session: &WorktreeSession, head: &str, target: &str) -> Result<Self> {
        let diff = session.git.run_ok(&["diff", "--name-only", "--no-renames", "-z", head, target, "--"], "checkout paths")?;
        let touched = diff.stdout.split(|b| *b == 0).filter(|p| !p.is_empty())
            .map(|p| std::str::from_utf8(p).map(str::to_string).map_err(|_| stale())).collect::<Result<Vec<_>>>()?;
        let mut expected = self.clone();
        for path in &touched {
            let file = expected.files.iter_mut().find(|file| &file.path == path).ok_or_else(stale)?;
            expected.index.retain(|row| row.splitn(2, |b| *b == b'\t').nth(1) != Some(path.as_bytes()));
            let tree = session.git.run_ok(&["--literal-pathspecs", "ls-tree", "-z", target, "--", path], "checkout tree")?;
            if tree.stdout.is_empty() {
                file.content = None; file.executable = false;
                continue;
            }
            let row = tree.stdout.strip_suffix(&[0]).ok_or_else(stale)?;
            let mut parts = row.splitn(2, |b| *b == b'\t');
            let metadata = std::str::from_utf8(parts.next().unwrap_or_default()).map_err(|_| stale())?;
            if parts.next() != Some(path.as_bytes()) { return Err(stale()); }
            let fields: Vec<_> = metadata.split(' ').collect();
            if fields.len() != 3 || !matches!(fields[0], "100644" | "100755") || fields[1] != "blob" {
                return Err(stale());
            }
            // Git itself materializes filters/line endings. Do not infer the
            // expected bytes from the worktree after post-checkout has run.
            let object = format!("{target}:{path}");
            let path_arg = format!("--path={path}");
            let blob = session.git.run_ok(&["cat-file", "--filters", &path_arg, &object], "checkout file")?;
            file.content = Some(blob.stdout);
            file.executable = crate::platform::git_executable(fields[0]);
            expected.index.push(format!("{} {} 0\t{path}", fields[0], fields[2]).into_bytes());
        }
        expected.index.sort_by(|a, b| a.splitn(2, |v| *v == b'\t').nth(1)
            .cmp(&b.splitn(2, |v| *v == b'\t').nth(1)));
        Ok(expected)
    }
}

/// A cleanup phase can change known files, but cannot issue a new confirmation.
/// This state stays in memory; a persisted plan stores only ConfirmedFiles and
/// requires an explicit fresh confirmation when continued after a stop.
pub(crate) struct PreparedState {
    confirmed: ConfirmedFiles,
    fingerprint: String,
    version: (u64, u64),
    context: String,
    head: Vec<u8>,
}

impl PreparedState {
    pub(crate) fn capture(session: &WorktreeSession, paths: &[String]) -> Result<Self> {
        let version = session.confirmation_version();
        let fingerprint = fingerprint(&session.git, paths)?;
        let confirmed = ConfirmedFiles::capture(session, paths)?;
        let state = Self { confirmed, fingerprint, version, context: context(&session.git)?,
            head: session.git.run(&["rev-parse", "--verify", "--quiet", "HEAD"])?.stdout };
        state.require(session)?;
        Ok(state)
    }

    pub(crate) fn require(&self, session: &WorktreeSession) -> Result<()> {
        if self.version != session.confirmation_version() { return Err(stale()); }
        require(&session.git, &self.confirmed.paths(), &self.fingerprint)?;
        if self.version != session.confirmation_version() { return Err(stale()); }
        Ok(())
    }

    pub(crate) fn require_stash_save(&self, session: &WorktreeSession) -> Result<()> {
        let confirmed_paths = self.confirmed.paths();
        if paths(&session.git, &[])?.iter().any(|path| !confirmed_paths.contains(path)) { return Err(stale()); }
        self.require(session)
    }

    pub(crate) fn after_cleanup(&mut self, session: &WorktreeSession, touched: &[String]) -> Result<()> {
        // Sample first, validate original ownership second, and verify the
        // same sample last. No edit may slip into a new authorized baseline.
        let next_fingerprint = fingerprint(&session.git, &self.confirmed.paths())?;
        if self.version != session.confirmation_version() || self.context != context(&session.git)?
            || self.head != session.git.run(&["rev-parse", "--verify", "--quiet", "HEAD"])?.stdout {
            return Err(stale());
        }
        let next = self.confirmed.advance(session, touched)?.ok_or_else(stale)?;
        require(&session.git, &self.confirmed.paths(), &next_fingerprint)?;
        self.confirmed = next; self.fingerprint = next_fingerprint;
        self.require(session)
    }

    pub(crate) fn into_confirmed(self, session: &WorktreeSession) -> Result<ConfirmedFiles> {
        self.require(session)?;
        Ok(self.confirmed)
    }

    pub(crate) fn checkout_expected(&self, session: &WorktreeSession, target: &str) -> Result<ConfirmedFiles> {
        let head = std::str::from_utf8(&self.head).map_err(|_| stale())?.trim();
        self.confirmed.checkout_expected(session, head, target)
    }

    pub(crate) fn after_checkout(&mut self, session: &WorktreeSession, branch: &str,
        target: &str, expected: ConfirmedFiles) -> Result<()> {
        let paths = self.confirmed.paths();
        let next = fingerprint(&session.git, &paths)?;
        session.require_identity()?;
        let actual_ref = session.git.run_ok(&["symbolic-ref", "--quiet", "HEAD"], "checkout branch")?;
        let actual_head = session.git.run_ok(&["rev-parse", "HEAD"], "checkout HEAD")?;
        if self.version != session.confirmation_version()
            || actual_ref.stdout_lossy().trim() != format!("refs/heads/{branch}")
            || actual_head.stdout_lossy().trim() != target || operation::probe(&session.git)?.is_some()
            || crate::write_guard::paths(&session.git, &[])?.iter().any(|path| !paths.contains(path))
            || !expected.matches(session)? {
            return Err(stale());
        }
        require(&session.git, &paths, &next)?;
        self.confirmed = expected; self.fingerprint = next;
        self.context = context(&session.git)?; self.head = actual_head.stdout;
        self.require(session)
    }
}

pub fn paths(git: &Git, targets: &[&str]) -> Result<Vec<String>> {
    let state = status::read(git)?;
    if state.entries.iter().any(|entry| entry.readonly_reason == Some(status::ReadonlyReason::NonUtf8Path)) {
        return Err(AppError::coded(ErrorKind::Rejected, "errors.readonly.nonUtf8Path", "非 UTF-8 路径无法无损处理，仅供只读查看", &[]));
    }
    let mut paths: Vec<_> = state.entries.iter().flat_map(|entry|
        std::iter::once(entry.path.clone()).chain(entry.original_path.iter().cloned())).collect();
    for target in targets {
        let output = git.run_ok(&["diff", "--name-only", "--no-renames", "-z", "HEAD", target, "--"], "读取覆盖路径")?;
        paths.extend(output.stdout.split(|byte| *byte == 0).filter(|path| !path.is_empty())
            .map(|path| std::str::from_utf8(path).map(str::to_string).map_err(|_| stale())).collect::<Result<Vec<_>>>()?);
    }
    paths.sort(); paths.dedup(); Ok(paths)
}

/// The branch name is independent of its OID: two refs may point at one commit.
pub fn branch_context(git: &Git) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(&WorktreeIdentity::capture(git)?)?);
    let symbolic = git.run(&["symbolic-ref", "--quiet", "HEAD"])?;
    match symbolic.exit_code {
        0 => hash.update(symbolic.stdout),
        1 => hash.update(b"<detached>"),
        _ => { symbolic.into_success("读取分支身份")?; }
    }
    Ok(hex(hash))
}

pub fn context(git: &Git) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(branch_context(git)?);
    // Any local remote-tracking ref can establish that a selected commit is
    // published. Keep warning/source decisions bound to the same ref names
    // and object identities through preparation and the final write.
    hash.update(b"remote-tracking-refs-v1\0");
    hash.update(git.run_ok(&["for-each-ref", "--format=%(refname)%00%(objectname)%00", "refs/remotes"], "读取已发布范围")?.stdout);
    if let Some((_, _, metadata)) = operation::probe(git)? { hash.update(metadata); }
    Ok(hex(hash))
}

pub fn require_context(session: &WorktreeSession, expected: &str) -> Result<()> {
    session.require_identity()?;
    if expected != context(&session.git)? { return Err(stale()); }
    Ok(())
}

pub fn fingerprint(git: &Git, paths: &[String]) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(b"revia-write-state-v2\0");
    hash.update(context(git)?);
    // Unborn HEAD is a valid stash-save preflight (Git will reject the save).
    let head = git.run(&["rev-parse", "--verify", "--quiet", "HEAD"])?;
    hash.update(head.stdout);
    hash.update(git.run_ok(&["ls-files", "--stage", "-z"], "读取暂存状态")?.stdout);
    for path in paths {
        hash.update((path.len() as u64).to_le_bytes()); hash.update(path);
        let full = git.directory.join(path);
        match fs::symlink_metadata(&full) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                hash.update(b"symlink\0"); hash.update(fs::read_link(&full)?.as_os_str().as_encoded_bytes());
            }
            Ok(metadata) if metadata.is_dir() => {
                // Reads remain usable; a mutation needing this path is rejected
                // by capture_files rather than claiming an incomplete backup.
                hash.update(b"directory\0");
                hash.update(serde_json::to_vec(&crate::platform::directory_identity(&full)?)?);
            }
            Ok(_) => hash.update(serde_json::to_vec(&recovery::capture_files(&git.directory, &[path.clone()])?)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => hash.update(b"absent\0"),
            Err(error) => return Err(error.into()),
        }
    }
    hash.update(recovery::context_fingerprint(git, paths)?);
    Ok(hex(hash))
}

/// A UI confirmation also belongs to the session and refresh that issued it.
/// Internal file protection still uses the stable fingerprint above.
pub fn confirmation(session: &WorktreeSession, paths: &[String]) -> Result<String> {
    let version = session.confirmation_version();
    let state = fingerprint(&session.git, paths)?;
    if version != session.confirmation_version() { return Err(stale()); }
    Ok(bind_confirmation(session, &state))
}

pub fn bind_confirmation(session: &WorktreeSession, state: &str) -> String {
    let (instance, epoch) = session.confirmation_version();
    let mut hash = Sha256::new();
    hash.update(std::process::id().to_le_bytes()); hash.update(instance.to_le_bytes());
    hash.update(epoch.to_le_bytes()); hash.update(state); hex(hash)
}

pub fn require_confirmation(session: &WorktreeSession, paths: &[String], expected: &str) -> Result<()> {
    if confirmation(session, paths)? != expected { return Err(stale()); } Ok(())
}

fn hex(hash: Sha256) -> String { hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect() }
pub fn stale() -> AppError { AppError::coded(ErrorKind::Rejected, "errors.rewrite.stateChanged", "相关文件或暂存状态已变化，请刷新后重新确认", &[]) }
pub fn require(git: &Git, paths: &[String], expected: &str) -> Result<()> {
    if expected != fingerprint(git, paths)? { return Err(stale()); } Ok(())
}

pub fn protect(session: &WorktreeSession, store: &RecoveryStore, source: RecoverySource,
    target: &str, paths: &[String]) -> Result<Option<ProtectedRecovery>> {
    let identity = session.require_identity()?;
    if paths.is_empty() { return Ok(None); }
    let before = recovery::capture_files(&session.path, paths)?;
    let context = recovery::context_fingerprint(&session.git, paths)?;
    Ok(Some(store.save_protected(identity, source, target, before, context)?))
}

pub fn operation_targets(git: &Git, kind: OperationKind) -> Result<Vec<String>> {
    let markers: &[&str] = match kind {
        OperationKind::Rebase => &["rebase-merge/orig-head", "rebase-apply/orig-head"],
        OperationKind::CherryPick | OperationKind::Revert => &["sequencer/head"],
        OperationKind::Merge => &[],
    };
    let mut targets = Vec::new();
    for marker in markers {
        match fs::read_to_string(operation::marker_path(git, marker)?) {
            Ok(value) => { targets.push(value.trim().to_string()); break; }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => return Err(error.into()),
        }
    }
    if targets.is_empty() && kind != OperationKind::Merge {
        // Single cherry-pick/revert has no sequencer/head and abort resets to
        // current HEAD. Rebase always has an orig-head marker; fail closed.
        if kind == OperationKind::Rebase { return Err(stale()); }
    }
    // Skip may immediately run later todo entries. A path added and removed
    // again before orig-head is absent from a simple HEAD..orig-head tree diff.
    for marker in ["rebase-merge/git-rebase-todo", "sequencer/todo"] {
        match fs::read_to_string(operation::marker_path(git, marker)?) {
            Ok(todo) => for line in todo.lines() {
                let mut words = line.split_whitespace();
                if matches!(words.next(), Some("pick" | "p" | "reword" | "r" | "edit" | "e" | "squash" | "s" | "fixup" | "f" | "revert")) {
                    if let Some(oid) = words.find(|word| word.len() >= 7 && word.bytes().all(|byte| byte.is_ascii_hexdigit())) {
                        targets.push(oid.to_string());
                    }
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => return Err(error.into()),
        }
    }
    targets.push("HEAD".into());
    targets.sort(); targets.dedup();
    Ok(targets)
}
