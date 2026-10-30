//! Independent stash archives (ADR-0012).
//!
//! A dropped/cleared stash must remain restorable after the original repository
//! has garbage-collected the objects.  Every archive is a throw-away bare Git
//! repository under Revia's own data directory. The archive contains the full
//! reachable commit/tree/blob closure so a later fetch never depends on old
//! ancestry still being available in the original repository.
//!
//! No reference is ever written to the user's repository.  Restoring fetches
//! from the archive into `FETCH_HEAD` and then stores the commit through
//! `git stash store`, which only changes `refs/stash` and its reflog.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};
use crate::git::Git;
use crate::recovery::{self, WorktreeIdentity};
use crate::session::WorktreeSession;

static NEXT_ARCHIVE: AtomicU64 = AtomicU64::new(1);

/// Stash archives are kept for the same 14 days as ordinary recovery records.
pub const STASH_ARCHIVE_RETENTION_MS: u64 = recovery::RETENTION_MS;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashArchiveRecord {
    pub id: String,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub identity: WorktreeIdentity,
    pub stash_oid: String,
    pub message: String,
    pub branch: Option<String>,
    /// Refs inside the archive repository.  Never created in the user's repo.
    pub archive_ref: String,
}

impl StashArchiveRecord {
    fn belongs_to_repository(&self, current: &WorktreeIdentity) -> bool {
        // Stash is shared by worktrees. The canonical commonDir and its known
        // filesystem identity must still match, even if the source worktree
        // has since been removed. Ordinary file recovery keeps its own guard.
        self.identity.common_dir == current.common_dir
            && matches!((self.identity.filesystem_ids.get(2), current.filesystem_ids.get(2)),
                (Some(Some(saved)), Some(Some(actual))) if saved == actual)
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StashArchiveSummary {
    pub id: String,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub stash_oid: String,
    pub message: String,
    pub worktree_path: String,
    pub identity_confirmed: bool,
}

#[derive(Debug, Clone)]
pub struct StashArchiveStore {
    directory: PathBuf,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl StashArchiveStore {
    pub fn new(data_directory: &Path) -> Self {
        Self {
            directory: data_directory.join("stash-archive"),
        }
    }

    fn ensure_outside(&self, git: &Git, identity: &WorktreeIdentity) -> Result<()> {
        crate::recovery::ensure_outside_git(&self.directory, git, identity)
    }

    fn archive_repo(&self, id: &str) -> PathBuf {
        self.directory.join(id).join("archive.git")
    }

    /// Create an archive.  The caller has already verified the stash identity.
    pub fn archive(
        &self,
        git: &Git,
        identity: WorktreeIdentity,
        stash_oid: &str,
        message: &str,
        branch: Option<String>,
    ) -> Result<StashArchiveRecord> {
        if !stash_oid.bytes().all(|byte| byte.is_ascii_hexdigit()) || stash_oid.len() < 7 {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.stashArchive.invalidStashObjectId",
                "stash 对象 ID 无效",
                &[],
            ));
        }
        self.ensure_outside(git, &identity)?;
        fs::create_dir_all(&self.directory)?;
        self.ensure_outside(git, &identity)?;
        let id = format!(
            "stash-{}-{}-{}",
            now_ms(),
            std::process::id(),
            NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed)
        );
        let root = self.directory.join(&id);
        let repo = root.join("archive.git");
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&root)?;
        let result = self.build_archive(git, &repo, stash_oid, &id);
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&root);
            return Err(error);
        }
        let created_at_ms = now_ms();
        let record = StashArchiveRecord {
            id,
            created_at_ms,
            expires_at_ms: created_at_ms + STASH_ARCHIVE_RETENTION_MS,
            identity,
            stash_oid: stash_oid.to_string(),
            message: message.to_string(),
            branch,
            archive_ref: format!(
                "refs/revia/stash/{}",
                root.file_name().unwrap_or_default().to_string_lossy()
            ),
        };
        write_new(&root.join("metadata.json"), &record)?;
        self.ensure_outside(git, &record.identity)?;
        Ok(record)
    }

    fn build_archive(&self, git: &Git, repo: &Path, stash_oid: &str, id: &str) -> Result<()> {
        // Object identity must not depend on local replace refs. Shallow roots
        // are part of the source graph and must travel with the owned archive.
        let git = git.with_env_vars(&[("GIT_NO_REPLACE_OBJECTS", "1")]);
        let shallow_path = crate::operation::marker_path(&git, "shallow")?;
        let shallow = read_optional(&shallow_path)?;
        // `git init --bare` into a path we allocated ourselves.  No user config
        // is modified: the repository-local config is the archive's own.
        let init_dir = repo.parent().unwrap_or(repo);
        let init = git.in_isolated_directory(init_dir).run(&[
            "init",
            "--bare",
            "--quiet",
            repo.to_string_lossy().as_ref(),
        ])?;
        init.into_success("创建 stash 归档仓库")?;

        let objects = self.collect_objects(&git, stash_oid)?;
        let archive = git.in_isolated_directory(repo);
        let mut input = Vec::new();
        for oid in &objects {
            input.extend_from_slice(oid.as_bytes());
            input.push(b'\n');
        }
        // `pack-objects --stdout` must run in the source repository: the
        // archive database is still empty and cannot resolve these objects.
        // Feed the pack to `index-pack --stdin` in the archive, which writes
        // the matching .pack and .idx into the archive's own object database.
        let pack = git.run_with_input(&["pack-objects", "--stdout"], input)?;
        let pack = pack.into_success("生成 stash 归档包")?;
        let indexed = archive.run_with_input(&["index-pack", "--stdin"], pack.stdout)?;
        indexed.into_success("写入 stash 归档对象库")?;

        let boundaries: Vec<_> = std::str::from_utf8(&shallow).map_err(|_| incomplete_archive())?
            .lines().filter(|oid| objects.iter().any(|object| object == oid)).collect();
        if !boundaries.is_empty() {
            fs::write(repo.join("shallow"), boundaries.join("\n") + "\n")?;
        }
        if read_optional(&shallow_path)? != shallow { return Err(crate::write_guard::stale()); }

        let archive_ref = format!("refs/revia/stash/{id}");
        let update = archive.run(&["update-ref", &archive_ref, stash_oid])?;
        update.into_success("登记 stash 归档引用")?;
        archive.run_ok(&["fsck", "--connectivity-only", "--no-dangling"], "核对 stash 独立归档")?;
        Ok(())
    }

    /// The archive is imported with fetch, which validates commit ancestry.
    /// Include the full reachable object closure, not just the stash/base trees.
    fn collect_objects(&self, git: &Git, stash_oid: &str) -> Result<Vec<String>> {
        let resolved = git.run_ok(&["rev-parse", "--verify", &format!("{stash_oid}^{{commit}}")], "解析 stash 对象")?;
        let oid = resolved.stdout_lossy().trim().to_string();
        let output = git.run_ok(
            &["rev-list", "--objects", "--no-object-names", &oid],
            "读取 stash 完整对象闭包",
        )?;
        let mut objects: Vec<String> = output.stdout_lossy().lines().map(str::to_owned).collect();
        if !objects.iter().any(|object| object == &oid)
            || objects.iter().any(|object| object.len() != oid.len() || !object.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(AppError::coded(
                ErrorKind::Internal,
                "errors.stashArchive.failedReadStashCommits",
                "无法读取 stash 提交关系",
                &[],
            ));
        }
        objects.sort();
        objects.dedup();
        Ok(objects)
    }

    fn read_metadata(&self, id: &str) -> Result<StashArchiveRecord> {
        let path = self.record_directory(id)?.join("metadata.json");
        let bytes = fs::read(&path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn record_directory(&self, id: &str) -> Result<PathBuf> {
        if !id.starts_with("stash-")
            || !id
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || value == b'-')
        {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.stashArchive.invalidArchiveId",
                "stash 归档 ID 无效",
                &[],
            ));
        }
        Ok(self.directory.join(id))
    }

    pub fn list(&self, current: Option<&WorktreeIdentity>) -> Result<Vec<StashArchiveSummary>> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let now = now_ms();
        let mut records = Vec::new();
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("stash-") {
                continue;
            }
            let record = match self.read_metadata(&name) {
                Ok(record) => record,
                Err(_) => continue,
            };
            if record.expires_at_ms <= now {
                continue;
            }
            let identity_confirmed = current.is_some_and(|current| record.belongs_to_repository(current));
            records.push(StashArchiveSummary {
                id: record.id,
                created_at_ms: record.created_at_ms,
                expires_at_ms: record.expires_at_ms,
                stash_oid: record.stash_oid,
                message: record.message,
                worktree_path: record.identity.worktree_path.display().to_string(),
                identity_confirmed,
            });
        }
        records.sort_by(|a, b| {
            b.created_at_ms
                .cmp(&a.created_at_ms)
                .then_with(|| b.id.cmp(&a.id))
        });
        Ok(records)
    }

    /// Restore an archive into the repository's stash list.  It never touches
    /// the index or working tree.  The caller supplies the session so the
    /// shared stash reflog is protected by the same write lock and stash lock
    /// as every other stash mutation.
    pub fn restore(&self, session: &WorktreeSession, id: &str) -> Result<String> {
        self.restore_controlled(session, id, None, || Ok(()))
    }

    pub fn restore_confirmed(&self, session: &WorktreeSession, id: &str, expected_state: &str) -> Result<String> {
        self.restore_controlled(session, id, Some(expected_state), || Ok(()))
    }

    /// Observe the real fetch/store boundary in isolated integration tests.
    pub fn restore_confirmed_with_after_fetch(&self, session: &WorktreeSession, id: &str,
        expected_state: &str, after_fetch: impl FnOnce() -> Result<()>) -> Result<String> {
        self.restore_controlled(session, id, Some(expected_state), after_fetch)
    }

    fn restore_controlled(&self, session: &WorktreeSession, id: &str,
        expected_state: Option<&str>, after_fetch: impl FnOnce() -> Result<()>) -> Result<String> {
        let _write = session.acquire_write_lock()?;
        let _stash = crate::stash::lock_guard();
        let git = &session.git;
        let identity = session.require_identity()?;
        let record = self.read_metadata(id)?;
        if !record.belongs_to_repository(&identity) {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.stashArchive.archiveBelongsAnotherRepository",
                "该归档属于另一个仓库或 worktree，当前身份无法确认",
                &[],
            ));
        }
        let require_target = || -> Result<()> {
            session.require_identity()?;
            if let Some(expected) = expected_state {
                if crate::stash::preflight(session, None)? != expected { return Err(crate::write_guard::stale()); }
            }
            Ok(())
        };
        require_target()?;
        if record.expires_at_ms <= now_ms() {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.stashArchive.archiveExpired",
                "该 stash 归档已超过 14 天保留期",
                &[],
            ));
        }
        let repo = self.archive_repo(id);
        if !repo.is_dir() {
            return Err(AppError::coded(
                ErrorKind::Environment,
                "errors.stashArchive.archiveRepositoryMissing",
                "stash 归档仓库不存在",
                &[],
            ));
        }
        let archive = git.in_isolated_directory(&repo);
        let exists = archive.run(&["cat-file", "-e", &record.stash_oid]);
        match exists {
            Ok(output) if output.exit_code == 0 => {}
            _ => {
                return Err(AppError::coded(
                    ErrorKind::NeedsVerification,
                    "errors.stashArchive.archivedObjectsIncomplete",
                    "归档中的对象不完整，无法恢复",
                    &[],
                ));
            }
        }
        require_target()?;
        let fetch = git.run_write(&[
            "fetch",
            "--quiet",
            "--no-tags",
            "--update-shallow",
            repo.to_string_lossy().as_ref(),
            &record.archive_ref,
        ])?;
        fetch.into_success("从归档取回 stash 对象")?;
        after_fetch()?;
        require_target()?;
        // `FETCH_HEAD` now points at the archived commit.  `stash store` only
        // updates refs/stash and its reflog, never the index or worktree.
        let store_args = [
            "stash",
            "store",
            "--message",
            &record.message,
            &record.stash_oid,
        ];
        let stored = git.run_write(&store_args)?;
        stored.into_success("恢复 stash 条目")?;
        Ok(record.stash_oid)
    }

    /// Remove expired archives.  Errors are returned for the maintenance panel;
    /// one unreadable archive must not prevent the others from being purged.
    pub fn purge_expired(&self) -> Result<u64> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        let now = now_ms();
        let mut removed = 0;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("stash-") {
                continue;
            }
            let expired = self
                .read_metadata(&name)
                .map(|record| record.expires_at_ms <= now)
                .unwrap_or(false);
            if expired && fs::remove_dir_all(entry.path()).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

fn read_optional(path: &Path) -> Result<Vec<u8>> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn incomplete_archive() -> AppError {
    AppError::coded(ErrorKind::Rejected, "errors.stashArchive.archivedObjectsIncomplete", "归档中的对象不完整，无法恢复", &[])
}

fn write_new(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    let pending = path.with_extension("pending");
    fs::write(&pending, bytes)?;
    fs::rename(pending, path)?;
    Ok(())
}
