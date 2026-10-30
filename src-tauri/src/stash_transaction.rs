//! ADR-0014: the sole libgit2 write boundary. Lock refs/stash before selecting
//! reflog entries, retain that lock through the mutation, and never retry an
//! uncertain commit. Every removed entry has already been archived by stash.rs.

use std::fs;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use git2::Repository;

use crate::error::{AppError, ErrorKind, Result};
use crate::git::CommandRecord;
use crate::recovery::WorktreeIdentity;
use crate::session::WorktreeSession;
use crate::stash::StashEntry;

const STASH_REF: &str = "refs/stash";

/// Existing real-Git integration tests can stop the production path at these
/// boundaries. Normal calls pass a no-op; no fake Git implementation is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StashDeletePhase {
    BeforeLock,
    Locked,
    BeforeCommit,
    AfterCommit,
}

pub(crate) enum Deletion<'a> {
    One(&'a StashEntry),
    All(&'a [StashEntry]),
}

fn verify_occurrences(session: &WorktreeSession, selection: &Deletion<'_>) -> Result<()> {
    let current = crate::stash::list(&session.git)?;
    let matches = match selection {
        Deletion::One(expected) => current.get(expected.index as usize).is_some_and(|entry| entry == *expected),
        Deletion::All(expected) => current.as_slice() == *expected,
    };
    if !matches { return Err(changed()); }
    Ok(())
}

pub(crate) fn changed() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.stashTransaction.changed",
        "stash 列表或目标身份已变化，未删除任何条目，请刷新后重试",
        &[],
    )
}

fn backend(error: git2::Error) -> AppError {
    AppError::coded(
        if error.code() == git2::ErrorCode::Locked {
            ErrorKind::Rejected
        } else {
            ErrorKind::Environment
        },
        "errors.stashTransaction.backend",
        "无法安全更新 stash：{detail}",
        &[("detail", error.message().to_string())],
    )
}

fn uncertain(error: impl Into<AppError>) -> AppError {
    AppError::coded(
        ErrorKind::NeedsVerification,
        "errors.stashTransaction.uncertain",
        "stash 已归档，删除结果待核对；请刷新列表，不要重复执行：{cause}",
        &[],
    )
    .cause(error.into())
}

/// All non-CLI writes are logged under their actual backend, never disguised
/// as a Git command. Preparation failures and uncertain commits are both kept.
pub(crate) fn delete(
    session: &WorktreeSession,
    identity: &WorktreeIdentity,
    selection: Deletion<'_>,
    mut hook: impl FnMut(StashDeletePhase) -> Result<()>,
) -> Result<()> {
    let started = Instant::now();
    let started_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let args = match &selection {
        Deletion::One(entry) => vec!["stash-drop-transaction".into(), entry.oid.clone()],
        Deletion::All(entries) => std::iter::once("stash-clear-transaction".into())
            .chain(entries.iter().map(|entry| entry.oid.clone()))
            .collect(),
    };
    let result = execute(session, identity, selection, &mut hook);
    let version = git2::Version::get().libgit2_version();
    let diagnostic = result
        .as_ref()
        .err()
        .map(|error| crate::git::mask_credentials(&error.message))
        .unwrap_or_default();
    session.git.log.record(CommandRecord {
        sequence: 0,
        worktree_id: session.git.worktree_id.clone(),
        started_at_ms,
        program: format!(
            "Revia stash transaction (Git CLI + libgit2 {}.{}.{})",
            version.0, version.1, version.2
        ),
        args,
        directory: session.path.display().to_string(),
        exit_code: if result.is_ok() { 0 } else { -1 },
        elapsed_ms: started.elapsed().as_millis() as u64,
        timed_out: false,
        stdout_bytes: 0,
        stderr_excerpt: diagnostic.chars().take(2000).collect(),
        output: diagnostic,
        output_truncated: false,
    });
    result
}

fn execute(
    session: &WorktreeSession,
    identity: &WorktreeIdentity,
    selection: Deletion<'_>,
    hook: &mut impl FnMut(StashDeletePhase) -> Result<()>,
) -> Result<()> {
    // Current libgit2 build supports files/SHA-1. Reject other storage formats
    // before opening a write transaction; reads/apply/archive still use CLI.
    let format = session.git.run_ok(
        &["rev-parse", "--show-object-format"],
        "stash object format",
    )?;
    let storage = session.git.config_get("extensions.refStorage");
    if format.stdout_lossy().trim() != "sha1"
        || storage.as_deref().is_some_and(|value| value != "files")
        || session
            .git
            .env
            .iter()
            .any(|(key, value)| key == "GIT_NAMESPACE" && !value.is_empty())
    {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.stashTransaction.unsupported",
            "当前仓库格式不支持安全删除 stash，原条目和归档已保留",
            &[],
        ));
    }
    if identity.revalidate(&session.git).is_none() {
        return Err(changed());
    }
    let repo = Repository::open(&identity.git_dir).map_err(backend)?;
    if fs::canonicalize(repo.path())? != identity.git_dir
        || fs::canonicalize(repo.commondir())? != identity.common_dir
    {
        return Err(changed());
    }
    hook(StashDeletePhase::BeforeLock)?;
    // Last-entry/clear reference removal is owned by core Git. It handles
    // packed-refs under its own transaction locks and removes the log while
    // still locked. libgit2 only edits a reflog when entries will remain.
    let before = crate::stash::list(&session.git)?;
    if matches!(selection, Deletion::All(_)) || before.len() == 1 {
        let current = before.first().ok_or_else(changed)?;
        return crate::git::stash_lock::delete_reference(&session.git, &current.oid, || {
            hook(StashDeletePhase::Locked)?;
            if identity.revalidate(&session.git).is_none() {
                return Err(changed());
            }
            verify_occurrences(session, &selection)?;
            let reference = repo.find_reference(STASH_REF).map_err(backend)?;
            let reflog = repo.reflog(STASH_REF).map_err(backend)?;
            if reference.symbolic_target().map_err(backend)?.is_some() {
                return Err(changed());
            }
            let expected = match &selection {
                Deletion::One(entry) => std::slice::from_ref(*entry),
                Deletion::All(entries) => entries,
            };
            if reference.target().map(|oid| oid.to_string()).as_deref()
                != expected.first().map(|entry| entry.oid.as_str())
                || reflog.len() != expected.len()
                || !reflog.iter().zip(expected).all(|(actual, expected)| {
                    actual.id_new().to_string() == expected.oid
                        && actual.message_bytes().unwrap_or_default() == expected.message.as_bytes()
                })
            {
                return Err(changed());
            }
            hook(StashDeletePhase::BeforeCommit)
        })
        .and_then(|()| {
            // Git's reflog unlink only warns on failure. Require the path to
            // be absent too (including malformed directories or symlinks).
            match fs::symlink_metadata(identity.common_dir.join("logs/refs/stash")) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(uncertain(error)),
                Ok(_) => return Err(uncertain(changed())),
            }
            hook(StashDeletePhase::AfterCommit).map_err(uncertain)
        });
    }
    let mut transaction = repo.transaction().map_err(backend)?;
    transaction.lock_ref(STASH_REF).map_err(backend)?;
    hook(StashDeletePhase::Locked)?;
    if identity.revalidate(&session.git).is_none() {
        return Err(changed());
    }
    verify_occurrences(session, &selection)?;
    let reference = repo.find_reference(STASH_REF).map_err(backend)?;
    let mut reflog = repo.reflog(STASH_REF).map_err(backend)?;
    if reference.symbolic_target().map_err(backend)?.is_some()
        || reflog.is_empty()
        || reference.target() != reflog.get(0).map(|entry| entry.id_new())
    {
        return Err(changed());
    }
    let same_entry = |index: usize, expected: &StashEntry| {
        reflog.get(index).is_some_and(|entry| {
            entry.id_new().to_string() == expected.oid
                && entry.message_bytes().unwrap_or_default() == expected.message.as_bytes()
        })
    };
    match selection {
        Deletion::One(expected) => {
            // This ordinal is safe only after the complete reflog credential
            // was verified while holding refs/stash's transaction lock.
            let index = expected.index as usize;
            if !same_entry(index, expected) {
                return Err(changed());
            }
            reflog.remove(index, true).map_err(backend)?;
        }
        Deletion::All(expected) => {
            if reflog.len() != expected.len()
                || !expected
                    .iter()
                    .enumerate()
                    .all(|(i, entry)| same_entry(i, entry))
            {
                return Err(changed());
            }
            while !reflog.is_empty() {
                reflog.remove(0, true).map_err(backend)?;
            }
        }
    }
    if let Some(top) = reflog.get(0) {
        // Preserve the existing entry's identity and do not append a synthetic
        // reflog event. set_reflog overrides set_target's log arguments.
        transaction
            .set_target(STASH_REF, top.id_new(), Some(&top.committer()), "")
            .map_err(backend)?;
    } else {
        // The list shrank since the CLI read; require a fresh attempt through
        // core Git rather than entering libgit2's packed-reference deletion.
        return Err(changed());
    }
    // No reference deletion (and thus no packed-refs rewrite) occurs here.
    transaction.set_reflog(STASH_REF, reflog).map_err(backend)?;
    hook(StashDeletePhase::BeforeCommit)?;
    transaction
        .commit()
        .map_err(|error| uncertain(backend(error)))?;
    hook(StashDeletePhase::AfterCommit).map_err(uncertain)?;
    Ok(())
}
