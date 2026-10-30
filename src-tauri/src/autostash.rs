//! Persistent association between an automatic stash and a history operation
//! (M6 O02).  Automatic saves always use a real Git stash; there is no Shelf.
//!
//! The association is written to Revia's data directory, not to the user's
//! repository.  If the operation stops on a conflict the link stays; the stash
//! is only removed after an apply with no conflicts.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};
use crate::git::status;
use crate::operation;
use crate::recovery::{ProtectedRecovery, RecoverySource, RecoveryStore, WorktreeIdentity};
use crate::session::WorktreeSession;
use crate::stash;
use crate::stash_archive::StashArchiveStore;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AutostashLink {
    pub id: String,
    pub worktree_path: String,
    pub common_dir: String,
    pub stash_oid: String,
    pub operation: String,
    pub message: String,
    pub created_at_ms: u64,
    pub recovery_id: Option<String>,
    /// Older path-only links remain readable, but cannot authorize a restore.
    #[serde(default)]
    pub identity: Option<WorktreeIdentity>,
    #[serde(default)]
    pub restoration: AutostashRestoration,
    /// Branch identity at save time. Automatic cleanup restores only there.
    #[serde(default)]
    #[ts(optional)]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AutostashRestoration {
    #[default]
    Pending,
    Saving,
    /// Persisted before Git starts; interruption must not apply the stash twice.
    Applying,
    Restored,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AutostashRestoreOutcome {
    pub link: AutostashLink,
    pub restored: bool,
    pub conflicted: bool,
    /// The stash was archived and removed after a clean apply.
    pub removed: bool,
    pub recovery_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub cleanup_error: Option<crate::error::AppErrorPayload>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AutostashPending {
    pub link: AutostashLink,
    pub confirmation_state: Option<String>,
    pub error: Option<crate::error::AppErrorPayload>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Clone)]
pub struct AutostashStore {
    directory: PathBuf,
}

static LINK_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn link_lock() -> std::sync::MutexGuard<'static, ()> {
    LINK_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn same_worktree(link: &AutostashLink, current: &WorktreeIdentity) -> bool {
    link.identity
        .as_ref()
        .is_some_and(|identity| identity.confirmed_match(current))
}

impl AutostashStore {
    pub fn new(data_directory: &Path) -> Self {
        Self {
            directory: data_directory.join("autostash"),
        }
    }

    fn file(&self) -> PathBuf {
        self.directory.join("links.json")
    }

    /// Application data directory shared by Revia's operation state.
    pub fn data_directory(&self) -> &Path {
        self.directory.parent().unwrap_or_else(|| Path::new("."))
    }

    fn read_all(&self) -> Result<Vec<AutostashLink>> {
        match fs::read(self.file()) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
                AppError::coded(
                    ErrorKind::Environment,
                    "errors.autostash.failedReadAutostashLinks",
                    "读取自动保存关联失败：{cause}",
                    &[],
                ).cause(error.into())
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    fn write_all(&self, links: &[AutostashLink]) -> Result<()> {
        fs::create_dir_all(&self.directory)?;
        let pending = self.file().with_extension("pending");
        let mut file = fs::File::create(&pending)?;
        file.write_all(&serde_json::to_vec(links)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(pending, self.file())?;
        crate::platform::sync_directory(&self.directory)?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<AutostashLink>> {
        let _guard = link_lock();
        self.read_all()
    }

    pub fn for_worktree(&self, session: &WorktreeSession) -> Result<Option<AutostashLink>> {
        let _guard = link_lock();
        let identity = WorktreeIdentity::capture(&session.git)?;
        Ok(self
            .read_all()?
            .into_iter()
            .find(|link| same_worktree(link, &identity)))
    }

    fn upsert(&self, link: AutostashLink) -> Result<()> {
        let mut links = self.read_all()?;
        links.retain(|item| !link.identity.as_ref().is_some_and(|identity| same_worktree(item, identity)));
        links.push(link);
        self.write_all(&links)
    }

    fn remove(&self, id: &str) -> Result<()> {
        let mut links = self.read_all()?;
        links.retain(|item| item.id != id);
        self.write_all(&links)
    }

    /// Save all local changes (including untracked paths) before a history
    /// operation.  Returns the created link, or None when the tree is clean.
    /// The caller must already hold the session write lock.
    pub fn save_before(
        &self,
        session: &WorktreeSession,
        recovery_store: &RecoveryStore,
        operation: &str,
    ) -> Result<Option<AutostashLink>> {
        self.save_before_impl(session, recovery_store, operation, None)
    }

    pub(crate) fn save_before_confirmed(&self, session: &WorktreeSession, recovery_store: &RecoveryStore,
        operation: &str, confirmation: &crate::write_guard::PreparedState) -> Result<Option<AutostashLink>> {
        self.save_before_impl(session, recovery_store, operation, Some(confirmation))
    }

    fn save_before_impl(&self, session: &WorktreeSession, recovery_store: &RecoveryStore,
        operation: &str, confirmation: Option<&crate::write_guard::PreparedState>) -> Result<Option<AutostashLink>> {
        let _guard = link_lock();
        let _stash = stash::lock_guard();
        if let Some(confirmation) = confirmation { confirmation.require_stash_save(session)?; }
        let status = status::read(&session.git)?;
        if status.entries.is_empty() {
            return Ok(None);
        }
        // A previous link must not be orphaned silently.  If it is still
        // present, refuse rather than overwriting its association.
        let identity = WorktreeIdentity::capture(&session.git)?;
        let canonical = fs::canonicalize(&session.path).unwrap_or_else(|_| session.path.clone());
        if self
            .read_all()?
            .iter()
            .any(|link| same_worktree(link, &identity))
        {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.autostash.previousAutostashStillPending",
                "上一次自动保存的本地改动仍未恢复，请先处理",
                &[],
            ));
        }
        // O02: recovery completes before the save.  The stash is not itself a
        // substitute for the recovery record.
        let saved_paths = crate::write_guard::paths(&session.git, &[])?;
        let expected = crate::write_guard::fingerprint(&session.git, &saved_paths)?;
        let recovery_record = protect_local_changes(
            session,
            recovery_store,
            identity.clone(),
            RecoverySource::HistoryAutostash,
            &format!("autostash before {operation}"),
        )?;
        let recovery_id = recovery_record.as_ref().map(|record| record.id.clone());
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let id = format!("autostash-{}-{nonce}", std::process::id());
        let message = format!("Revia autostash before {operation} [{id}]");
        // Persist an intent before Git can clean any files. An interrupted save
        // remains visible for verification and can never trigger an auto-apply.
        let mut link = AutostashLink {
            id,
            worktree_path: canonical.display().to_string(),
            common_dir: identity.common_dir.display().to_string(),
            stash_oid: String::new(),
            operation: operation.to_string(),
            message: message.clone(),
            created_at_ms: now_ms(),
            recovery_id,
            identity: Some(identity),
            restoration: AutostashRestoration::Saving,
            branch: Some(crate::write_guard::branch_context(&session.git)?),
        };
        crate::write_guard::require(&session.git, &crate::write_guard::paths(&session.git, &[])?, &expected)?;
        self.upsert(link.clone())?;
        if let Some(confirmation) = confirmation {
            if let Err(error) = confirmation.require_stash_save(session) {
                return match self.remove(&link.id) {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(crate::recovery::pending_write(error.related(&cleanup))),
                };
            }
        }
        let push = session.git.run_write(&[
            "stash",
            "push",
            "--include-untracked",
            "--quiet",
            "--message",
            &message,
        ]).map_err(crate::recovery::pending_write)?;
        if push.exit_code != 0 {
            if let Some(record) = &recovery_record {
                recovery_store.finish_snapshot(record)?;
            }
            return Err(AppError::coded(
                ErrorKind::NeedsVerification,
                "errors.autostash.failedSaveLocalChanges",
                "操作前自动保存本地改动失败：{detail}",
                &[("detail", push.stderr.trim().to_string())],
            ));
        }
        // Another process can push a newer stash after ours. Resolve our
        // unique reflog message, never claim the current refs/stash value.
        let entry = stash::list(&session.git).map_err(crate::recovery::pending_write)?
            .into_iter().find(|entry| entry.message.ends_with(&message))
            .ok_or_else(|| AppError::coded(ErrorKind::NeedsVerification,
                "errors.autostash.autostashEntryMissing",
                "自动保存的 stash 条目已不存在，无法恢复", &[]))?;
        let oid = entry.oid;
        link.stash_oid = oid.clone();
        link.restoration = AutostashRestoration::Pending;
        if let Err(error) = self.upsert(link.clone()) {
            // The save already wrote files. Keep the prior Saving association
            // and exact stash for inspection; a persistence failure cannot
            // authorize an unprotected second write to the worktree.
            let receipt_error = recovery_record.as_ref()
                .and_then(|record| recovery_store.finish_snapshot(record).err());
            let mut failure = AppError::coded(
                ErrorKind::NeedsVerification,
                "errors.autostash.failedPersistAutostashAssociation",
                "自动保存已执行但关联写入失败；stash {oid} 已保留，未自动恢复工作区：{cause}",
                &[("oid", oid)],
            ).cause(error);
            if let Some(error) = receipt_error { failure = failure.related(&error); }
            return Err(failure);
        }
        if let Some(record) = &recovery_record {
            recovery_store.finish_snapshot(record)?;
        }
        Ok(Some(link))
    }

    /// Restore a pending automatic stash once no operation is in progress.
    /// A conflict leaves both the local changes and the stash in place.  The
    /// caller must already hold the session write lock; internal stash removal
    /// uses the non-locking variant to avoid a self-deadlock.
    pub fn restore_if_ready(
        &self,
        session: &WorktreeSession,
        recovery_store: &RecoveryStore,
        archive_store: &StashArchiveStore,
    ) -> Result<Option<AutostashRestoreOutcome>> {
        let _guard = link_lock();
        // Hold the repository-wide stash lock across apply and archive-before-
        // drop so a manual stash operation cannot change the list in between.
        let _stash = stash::lock_guard();
        self.restore_locked(session, recovery_store, archive_store, None)
    }

    pub fn pending(&self, session: &WorktreeSession) -> Result<Option<AutostashPending>> {
        let Some(link) = self.for_worktree(session)? else { return Ok(None); };
        let confirmation = if link.restoration == AutostashRestoration::Pending {
            stash::preflight(session, Some(&link.stash_oid))
        } else { crate::write_guard::confirmation(session, &[]) };
        let (confirmation_state, error) = match confirmation {
            Ok(state) => (Some(state), None),
            Err(error) => (None, Some(error.payload())),
        };
        // Reading an association must not require its stash to remain usable.
        // Keep it visible for inspection/forgetting without authorizing restore.
        Ok(Some(AutostashPending { link, confirmation_state, error }))
    }

    /// Explicit restoration is bound to the association and files actually
    /// shown by the UI. Automatic operation cleanup uses restore_if_ready.
    pub fn restore_confirmed(&self, session: &WorktreeSession, recovery_store: &RecoveryStore,
        archive_store: &StashArchiveStore, expected: &AutostashLink, state: &str) -> Result<Option<AutostashRestoreOutcome>> {
        let _guard = link_lock(); let _stash = stash::lock_guard();
        self.restore_locked(session, recovery_store, archive_store, Some((expected, state)))
    }

    fn restore_locked(&self, session: &WorktreeSession, recovery_store: &RecoveryStore,
        archive_store: &StashArchiveStore, expected: Option<(&AutostashLink, &str)>) -> Result<Option<AutostashRestoreOutcome>> {
        let state = operation::detect(&session.git, operation::conflict_count(&session.git)?)?;
        if state.is_some() {
            return Ok(None);
        }
        let identity = WorktreeIdentity::capture(&session.git)?;
        let Some(mut link) = self
            .read_all()?
            .into_iter()
            .find(|link| same_worktree(link, &identity))
        else {
            if expected.is_some() { return Err(crate::write_guard::stale()); }
            return Ok(None);
        };
        if let Some((expected, state)) = expected {
            if serde_json::to_value(&link)? != serde_json::to_value(expected)? { return Err(crate::write_guard::stale()); }
            crate::stash::require_preflight(session, &link.stash_oid, None, state)?;
        }
        // A hook or another process may switch branches before cleanup. Keep
        // the stash for an explicit restore instead of applying it elsewhere.
        if expected.is_none()
            && link.branch.as_deref() != Some(crate::write_guard::branch_context(&session.git)?.as_str()) {
            return Err(AppError::coded(ErrorKind::NeedsVerification, "errors.autostash.restoreTargetChanged",
                "自动保存时所在的分支已变化，未自动恢复；stash 已保留，请核对后再恢复", &[]));
        }
        if matches!(link.restoration, AutostashRestoration::Applying | AutostashRestoration::Saving) {
            return Err(AppError::coded(ErrorKind::NeedsVerification, "errors.autostash.restoreAlreadyAttempted", "这些改动已尝试恢复，结果待核对；stash 已保留，不会重复应用", &[]));
        }
        if link.restoration == AutostashRestoration::Restored {
            return Err(AppError::coded(ErrorKind::NeedsVerification, "errors.autostash.restoreAlreadyAttempted", "这些改动已尝试恢复，结果待核对；stash 已保留，不会重复应用", &[]));
        }
        let exists =
            session
                .git
                .run(&["cat-file", "-e", &format!("{}^{{commit}}", link.stash_oid)])?;
        if exists.exit_code != 0 {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.autostash.autostashEntryMissing",
                "自动保存的 stash 条目已不存在，无法恢复",
                &[],
            ));
        }
        let paths = crate::stash::apply_paths(&session.git, &link.stash_oid)?;
        let file_state = crate::write_guard::fingerprint(&session.git, &paths)?;
        let recovery_record = crate::write_guard::protect(session, recovery_store,
            RecoverySource::HistoryAutostash, "autostash restore", &paths)?;
        let require_confirmation = || -> Result<()> {
            crate::write_guard::require(&session.git, &crate::stash::apply_paths(&session.git, &link.stash_oid)?, &file_state)?;
            if let Some((_, state)) = expected {
                crate::stash::require_preflight(session, &link.stash_oid, None, state)?;
            }
            Ok(())
        };
        require_confirmation()?;
        let recovery_id = recovery_record.as_ref().map(|record| record.id.clone());
        link.restoration = AutostashRestoration::Applying;
        self.upsert(link.clone())?;
        // Publishing the interruption receipt also takes time. Keep the shown
        // confirmation through that boundary; no Git apply has started yet.
        if let Err(error) = require_confirmation() {
            link.restoration = AutostashRestoration::Pending;
            return match self.upsert(link) {
                Ok(()) => Err(error),
                Err(receipt_error) => Err(crate::recovery::pending_write(error.related(&receipt_error))),
            };
        }
        let output = session
            .git
            .run_write(&["stash", "apply", "--index", "--quiet", &link.stash_oid]).map_err(crate::recovery::pending_write)?;
        if let Some(record) = &recovery_record {
            recovery_store.finish_snapshot(record)?;
        }
        let after = status::read(&session.git).map_err(crate::recovery::pending_write)?;
        let conflicted = after.entries.iter().any(|entry| entry.unmerged);
        if output.exit_code != 0 || conflicted {
            return Ok(Some(AutostashRestoreOutcome {
                link,
                restored: output.exit_code == 0,
                conflicted,
                removed: false,
                recovery_id,
                cleanup_error: if !conflicted && output.exit_code != 0 {
                    Some(crate::recovery::pending_write(AppError::coded(ErrorKind::Git,
                        "errors.mod.failedExitCode", "Git 命令失败（退出码 {0}）：{1}",
                        &[("0", output.exit_code.to_string()), ("1", output.stderr.trim().to_string())])).payload())
                } else { None },
            }));
        }
        link.restoration = AutostashRestoration::Restored;
        self.upsert(link.clone()).map_err(crate::recovery::pending_write)?;
        // Keep the Restored receipt on cleanup failure. It prevents another
        // apply after restart and retains the exact pending result for the UI.
        let (removed, cleanup_error) = match stash::drop_locked(session, archive_store, &link.stash_oid) {
            Ok(outcome) => (outcome.deleted, None),
            Err(error) => (false, Some(error.payload())),
        };
        if cleanup_error.is_none() {
            self.remove(&link.id).map_err(crate::recovery::pending_write)?;
        }
        Ok(Some(AutostashRestoreOutcome {
            link, restored: true, conflicted: false, removed, recovery_id, cleanup_error,
        }))
    }

    /// Drop the association after the user chose to keep the stash manually.
    pub fn forget(&self, session: &WorktreeSession, id: &str) -> Result<()> {
        let _guard = link_lock();
        let identity = WorktreeIdentity::capture(&session.git)?;
        if !self.read_all()?.iter().any(|link| link.id == id && same_worktree(link, &identity)) {
            return Err(AppError::coded(ErrorKind::Rejected, "errors.autostash.associationChanged", "自动保存关联已变化，请刷新后重试", &[]));
        }
        self.remove(id)
    }

}

pub(crate) fn protect_local_changes(
    session: &WorktreeSession,
    recovery_store: &RecoveryStore,
    identity: WorktreeIdentity,
    source: RecoverySource,
    target: &str,
) -> Result<Option<ProtectedRecovery>> {
    let paths = crate::write_guard::paths(&session.git, &[])?;
    let _ = identity; // the opened session identity is authoritative
    crate::write_guard::protect(session, recovery_store, source, target, &paths)
}
