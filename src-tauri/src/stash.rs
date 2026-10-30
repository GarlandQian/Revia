//! Stash listing, inspection, save/apply/drop/clear (M6 S01–S03).
//!
//! Stash contents use full commit OIDs. A reflog version and occurrence key
//! additionally identify individual entries, including repeated OIDs. Entries
//! are archived to an independent bare repository (ADR-0012) before any drop
//! or clear; a failed archive aborts the destructive step.
//!
//! Only deletion uses ADR-0014's locked refs/stash transaction; all other Git
//! calls use the CLI executor. No hidden refs are added.

use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppErrorPayload, ErrorKind, Result};
use crate::git::Git;
use crate::git::status;
use crate::recovery::{self, RecoveryStore, WorktreeIdentity};
use crate::session::WorktreeSession;
use crate::stash_archive::{StashArchiveRecord, StashArchiveStore};

/// Stash writes touch a repository-wide resource shared by all worktrees.
/// A process-local lock prevents our own commands from racing; every entry is
/// additionally re-identified by OID to reject external changes.
static STASH_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn stash_write_lock() -> std::sync::MutexGuard<'static, ()> {
    STASH_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Acquire the repository-wide stash lock.  Callers that already hold the
/// session write lock use this for multi-step flows (autostash save/restore)
/// so the whole apply/drop sequence is serialized with manual stash writes.
pub(crate) fn lock_guard() -> std::sync::MutexGuard<'static, ()> {
    stash_write_lock()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StashEntry {
    /// Opaque credential for this occurrence in a fixed reflog version.
    pub key: String,
    /// Position is trusted only together with the complete version in `key`.
    pub index: u32,
    pub oid: String,
    pub message: String,
    pub branch: Option<String>,
    /// Unix milliseconds from the reflog entry.
    pub time_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StashList {
    pub entries: Vec<StashEntry>,
    /// Identity of the worktree whose common Git directory holds these stashes.
    pub worktree_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StashFile {
    pub path: String,
    pub status: String,
    /// Added/deleted line counts when Git's patch contains them.
    pub added: u32,
    pub deleted: u32,
    pub binary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StashInspection {
    pub entry: StashEntry,
    pub files: Vec<StashFile>,
    /// Read-only patch text straight from Git.
    pub patch: String,
    pub truncated: bool,
    #[serde(default)]
    #[ts(optional)]
    pub patch_error: Option<AppErrorPayload>,
    /// False compares with the stash's base commit (the default); true
    /// compares the stash content with the current working tree.
    pub compared_with_local: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashSaveRequest {
    pub message: String,
    /// Keep the staged changes in the index (`git stash push --keep-index`).
    pub keep_index: bool,
    pub expected_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashApplyRequest {
    /// Full OID from the list the user confirmed.
    pub oid: String,
    #[serde(default)]
    pub entry_key: Option<String>,
    /// Recreate the index as it was when the stash was created.
    pub restore_index: bool,
    /// Delete only after a successful, conflict-free apply.
    pub delete_after: bool,
    /// Apply on a new branch created from the stash base.
    #[serde(default)]
    pub new_branch: Option<String>,
    pub expected_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StashApplyOutcome {
    pub oid: String,
    pub applied: bool,
    /// Conflicts mean the entry is intentionally kept.
    pub conflicted: bool,
    pub deleted: bool,
    pub archive_id: Option<String>,
    pub recovery_id: Option<String>,
    pub branch: Option<String>,
    /// The stash was applied but its archive/delete cleanup failed.  This is
    /// reported separately so the UI never tells the user to apply it again.
    #[serde(default)]
    #[ts(optional)]
    pub cleanup_error: Option<AppErrorPayload>,
    #[serde(default)]
    #[ts(optional)]
    pub error: Option<AppErrorPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StashDropOutcome {
    pub oid: String,
    pub deleted: bool,
    pub archive_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StashClearOutcome {
    pub archived: Vec<String>,
    pub deleted: u32,
}

impl StashEntry {
    fn from_reflog(index: u32, oid: String, subject: String, time_ms: u64) -> Self {
        // `%gs` is either `On <branch>: <message>` or `WIP on <branch>: ...`.
        let branch = subject
            .strip_prefix("On ")
            .or_else(|| subject.strip_prefix("WIP on "))
            .and_then(|rest| rest.split(':').next())
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "(no branch)")
            .map(str::to_string);
        Self {
            key: String::new(),
            index,
            oid,
            message: subject,
            branch,
            time_ms,
        }
    }
}

/// Read the real stash list.  Reflog order is the same order `git stash list`
/// uses; the index is only a display convenience and is never trusted alone.
pub fn list(git: &Git) -> Result<Vec<StashEntry>> {
    let output = git.run_ok(
        &["stash", "list", "--date=raw", "--format=%H%x00%gs%x00%ct%x00%gD"],
        "读取 stash 列表",
    )?;
    let version: String = Sha256::digest(&output.stdout).iter().map(|b| format!("{b:02x}")).collect();
    let mut entries = Vec::new();
    for line in output.stdout_lossy().lines() {
        let mut fields = line.split('\0');
        let oid = fields.next().unwrap_or_default();
        let subject = fields.next().unwrap_or_default();
        let seconds: u64 = fields.next().unwrap_or("0").parse().unwrap_or(0);
        if oid.is_empty() {
            continue;
        }
        let mut entry = StashEntry::from_reflog(
            entries.len() as u32,
            oid.to_string(),
            subject.to_string(),
            seconds.saturating_mul(1000),
        );
        entry.key = format!("{version}:{}", entry.index);
        entries.push(entry);
    }
    Ok(entries)
}

fn find_entry(git: &Git, oid: &str) -> Result<Option<StashEntry>> {
    find_selected(git, oid, None)
}

fn find_selected(git: &Git, oid: &str, key: Option<&str>) -> Result<Option<StashEntry>> {
    let entries = list(git)?;
    let mut matches = entries.into_iter().filter(|entry| entry.oid == oid
        && key.is_none_or(|key| entry.key == key));
    let found = matches.next();
    // Internal OID-only callers cannot choose between identical objects.
    if matches.next().is_some() { return Err(crate::stash_transaction::changed()); }
    Ok(found)
}

/// Read one entry, including a patch and a file summary.  Stash patches are
/// generated by Git; nothing here fabricates a hunk.
pub fn inspect(git: &Git, oid: &str, compare_local: bool) -> Result<StashInspection> {
    inspect_selected(git, oid, None, compare_local)
}

pub fn inspect_selected(git: &Git, oid: &str, key: Option<&str>, compare_local: bool) -> Result<StashInspection> {
    let entry = find_selected(git, oid, key)?.ok_or_else(|| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.stash.stashEntryNoLongerExists",
            "stash 条目已不存在，请刷新列表",
            &[],
        )
    })?;
    // A stash made with -u has a separate third-parent tree. Use an owned
    // temporary index to compare those paths with the real working files;
    // the user's index is never changed by inspection.
    let third = if compare_local {
        let output = git.run(&["rev-parse", "--verify", "--quiet", &format!("{oid}^3")])?;
        (output.exit_code == 0).then(|| output.stdout_lossy().trim().to_string())
    } else { None };
    let temporary_index = third.as_ref().map(|_| tempfile::tempdir()).transpose()?;
    let untracked_git = if let (Some(third), Some(directory)) = (&third, &temporary_index) {
        let index = std::fs::canonicalize(directory.path())?.join("index");
        let isolated = git.with_env_vars(&[("GIT_INDEX_FILE", index.to_string_lossy().as_ref())]);
        isolated.run_ok(&["read-tree", third], "读取 stash 未跟踪树")?;
        Some(isolated)
    } else { None };
    let mut exclusions = Vec::new();
    if let Some(third) = &third {
        let output = git.run_ok(&["ls-tree", "-r", "--name-only", "-z", third], "读取 stash 未跟踪路径")?;
        for path in output.stdout.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
            let path = std::str::from_utf8(path).map_err(|_| crate::write_guard::stale())?;
            exclusions.push(format!(":(exclude,literal){path}"));
        }
    }
    let mut files_args: Vec<&str> = if compare_local {
        vec!["diff", "--name-status", "--no-renames", "-z", oid]
    } else {
        vec![
            "stash",
            "show",
            "--include-untracked",
            "--name-status",
            "-z",
            "--no-renames",
            oid,
        ]
    };
    if !exclusions.is_empty() { files_args.push("--"); files_args.extend(exclusions.iter().map(String::as_str)); }
    let mut files_output = git.run_ok(&files_args, "读取 stash 文件列表")?;
    if let (Some(third), Some(untracked)) = (&third, &untracked_git) {
        files_output.stdout.extend(untracked.run_ok(&["diff", "--name-status", "--no-renames", "-z", third], "比较 stash 未跟踪路径")?.stdout);
    }
    let mut files = Vec::new();
    let text = std::str::from_utf8(&files_output.stdout).map_err(|_| AppError::coded(
        ErrorKind::Rejected, "errors.readonly.nonUtf8Path", "非 UTF-8 路径无法无损处理，仅供只读查看", &[]))?;
    let mut fields = text.split('\0');
    while let Some(status_code) = fields.next() {
        let path = fields.next().unwrap_or_default().to_string();
        if path.is_empty() {
            continue;
        }
        let status = match status_code.chars().next() {
            Some('A') => "added",
            Some('D') => "deleted",
            Some('R') => "renamed",
            Some('C') => "copied",
            Some('T') => "typeChanged",
            _ => "modified",
        }
        .to_string();
        files.push(StashFile {
            path,
            status,
            added: 0,
            deleted: 0,
            binary: false,
        });
    }

    const MAX_PATCH: usize = 8 * 1024 * 1024;
    let mut patch_args: Vec<&str> = if compare_local {
        vec![
            "-c",
            "core.quotePath=false",
            "diff",
            "-p",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            oid,
        ]
    } else {
        vec![
            "-c",
            "core.quotePath=false",
            "stash",
            "show",
            "--include-untracked",
            "-p",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            oid,
        ]
    };
    if !exclusions.is_empty() { patch_args.push("--"); patch_args.extend(exclusions.iter().map(String::as_str)); }
    let mut output = git.run_ok(&patch_args, "读取 stash diff")?;
    if let (Some(third), Some(untracked)) = (&third, &untracked_git) {
        output.stdout.extend(untracked.run_ok(&["-c", "core.quotePath=false", "diff", "-p", "--no-color", "--no-ext-diff", "--no-textconv",
            "--src-prefix=a/", "--dst-prefix=b/", third], "比较 stash 未跟踪内容")?.stdout);
    }
    let (patch, truncated, patch_error) = match crate::git::readonly_patch(&output.stdout, MAX_PATCH) {
        Some((patch, truncated)) => (patch, truncated, None),
        None => (String::new(), false, Some(AppError::coded(ErrorKind::Rejected,
            "errors.stash.patchUnavailable", "该 stash 差异不是可无损显示的 UTF-8 文本，仅保留文件列表", &[]).payload())),
    };

    // Obtain counts from Git's NUL machine format too; patch headers can be
    // C-quoted and are not a path identity for Unicode/tab/newline names.
    let stat_args: Vec<_> = files_args.iter().map(|arg| if *arg == "--name-status" { "--numstat" } else { *arg }).collect();
    let mut counts = git.run_ok(&stat_args, "读取 stash 文件统计")?;
    if let (Some(third), Some(untracked)) = (&third, &untracked_git) {
        counts.stdout.extend(untracked.run_ok(&["diff", "--numstat", "--no-renames", "-z", third], "读取 stash 未跟踪统计")?.stdout);
    }
    let counts_text = std::str::from_utf8(&counts.stdout).map_err(|_| AppError::coded(
        ErrorKind::Rejected, "errors.readonly.nonUtf8Path", "非 UTF-8 路径无法无损处理，仅供只读查看", &[]))?;
    for token in counts_text.split('\0') {
        let mut parts = token.splitn(3, '\t');
        let added = parts.next().unwrap_or_default(); let deleted = parts.next().unwrap_or_default();
        let path = parts.next().unwrap_or_default();
        if let Some(file) = files.iter_mut().find(|file| file.path == path) {
            file.binary = added == "-" || deleted == "-";
            file.added = added.parse().unwrap_or(0); file.deleted = deleted.parse().unwrap_or(0);
        }
    }
    Ok(StashInspection {
        entry,
        files,
        patch,
        truncated,
        patch_error,
        compared_with_local: compare_local,
    })
}

fn ensure_identity(git: &Git) -> Result<WorktreeIdentity> {
    WorktreeIdentity::capture(git)
}

/// Actual stash changes plus existing dirty paths; the untracked tree is a
/// partial tree and must never be compared to the complete HEAD tree.
pub(crate) fn apply_paths(git: &Git, oid: &str) -> Result<Vec<String>> {
    let mut paths = crate::write_guard::paths(git, &[])?;
    for target in [oid.to_string(), format!("{oid}^2")] {
        let output = git.run_ok(&["diff", "--name-only", "--no-renames", "-z", &format!("{oid}^1"), &target, "--"], "读取 stash 覆盖路径")?;
        paths.extend(output.stdout.split(|byte| *byte == 0).filter(|path| !path.is_empty())
            .map(|path| std::str::from_utf8(path).map(str::to_string).map_err(|_| crate::write_guard::stale())).collect::<Result<Vec<_>>>()?);
    }
    if git.run(&["rev-parse", "--verify", "--quiet", &format!("{oid}^3")])?.exit_code == 0 {
        let output = git.run_ok(&["ls-tree", "-r", "--name-only", "-z", &format!("{oid}^3")], "读取 stash 未跟踪路径")?;
        paths.extend(output.stdout.split(|byte| *byte == 0).filter(|path| !path.is_empty())
            .map(|path| std::str::from_utf8(path).map(str::to_string).map_err(|_| crate::write_guard::stale())).collect::<Result<Vec<_>>>()?);
    }
    paths.sort(); paths.dedup(); Ok(paths)
}

/// The dialog may choose a new branch after it opens. Bind its possible switch
/// paths too; ordinary apply only snapshots the actual apply paths above.
pub(crate) fn confirmation_paths(git: &Git, oid: &str) -> Result<Vec<String>> {
    let mut paths = apply_paths(git, oid)?;
    paths.extend(crate::write_guard::paths(git, &[&format!("{oid}^1")])?);
    paths.sort(); paths.dedup(); Ok(paths)
}

pub fn preflight(session: &WorktreeSession, oid: Option<&str>) -> Result<String> {
    preflight_selected(session, oid, None)
}

pub fn preflight_selected(session: &WorktreeSession, oid: Option<&str>, key: Option<&str>) -> Result<String> {
    session.require_identity()?;
    let paths = match oid {
        Some(oid) => {
            if find_selected(&session.git, oid, key)?.is_none() { return Err(crate::write_guard::stale()); }
            confirmation_paths(&session.git, oid)?
        }
        None => crate::write_guard::paths(&session.git, &[])?,
    };
    let files = crate::write_guard::confirmation(session, &paths)?;
    // Saving appends without deleting any existing entry; another serialized
    // stash write does not invalidate an unchanged worktree save confirmation.
    if oid.is_none() { return Ok(files); }
    let mut hash = Sha256::new();
    hash.update(files);
    hash.update(serde_json::to_vec(&list(&session.git)?)?);
    hash.update(oid.unwrap_or_default());
    hash.update(key.unwrap_or_default());
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub(crate) fn require_preflight(session: &WorktreeSession, oid: &str, key: Option<&str>, expected: &str) -> Result<()> {
    if preflight_selected(session, Some(oid), key)? != expected { return Err(crate::write_guard::stale()); }
    Ok(())
}

pub fn collection_preflight(session: &WorktreeSession) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(preflight(session, None)?);
    hash.update(serde_json::to_vec(&list(&session.git)?)?);
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Save a stash.  The dialog has only a message and “keep index”; we do not
/// add `--include-untracked` or `--all` here (Q156).
pub fn save(session: &WorktreeSession, request: &StashSaveRequest) -> Result<StashEntry> {
    save_with_timeout(session, request, std::time::Duration::from_secs(120))
}

/// Short real-Git deadlines are used by the existing integration regressions.
pub fn save_with_timeout(session: &WorktreeSession, request: &StashSaveRequest, timeout: std::time::Duration) -> Result<StashEntry> {
    save_controlled(session, request, timeout, || Ok(()))
}

/// Observe the real write boundary without replacing Git in concurrency tests.
pub fn save_with_after_write(session: &WorktreeSession, request: &StashSaveRequest,
    after_write: impl FnOnce() -> Result<()>) -> Result<StashEntry> {
    save_controlled(session, request, std::time::Duration::from_secs(120), after_write)
}

fn saved_trees(git: &Git, oid: &str) -> Result<Vec<String>> {
    let output = git.run_ok(&["cat-file", "-p", oid], "核对 stash 内容身份")?;
    let text = output.stdout_lossy();
    let headers: Vec<&str> = text.lines().take_while(|line| !line.is_empty()).collect();
    let tree = headers.iter().find_map(|line| line.strip_prefix("tree ")).ok_or_else(crate::write_guard::stale)?;
    let parents: Vec<&str> = headers.iter().filter_map(|line| line.strip_prefix("parent ")).collect();
    if parents.len() < 2 { return Err(crate::write_guard::stale()); }
    let index = git.run_ok(&["rev-parse", "--verify", &format!("{}^{{tree}}", parents[1])], "核对 stash 暂存区身份")?;
    let mut values = vec![tree.to_string(), parents[0].to_string(), index.stdout_lossy().trim().to_string()];
    if values.iter().any(|value| !matches!(value.len(), 40 | 64)
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())) { return Err(crate::write_guard::stale()); }
    // A third-parent stash includes other content and cannot be our manual save.
    values.push(parents.len().to_string());
    Ok(values)
}

fn matches_save_message(entry: &StashEntry, message: &str) -> bool {
    if message.is_empty() { return entry.message.starts_with("WIP on "); }
    entry.message.strip_prefix("On ").and_then(|text| text.split_once(": "))
        .is_some_and(|(_, saved)| saved == message)
}

fn require_save_writable(git: &Git) -> Result<()> {
    // Manual save cleans tracked changes only. Do not broaden this guard to
    // untouched untracked files or to the read/delete collection preflight.
    for entry in status::read(git)?.entries {
        if entry.changes != status::ChangesKind::Untracked {
            if let Some(reason) = entry.readonly_reason { return Err(reason.error()); }
        }
    }
    Ok(())
}

fn save_controlled(session: &WorktreeSession, request: &StashSaveRequest, timeout: std::time::Duration,
    after_write: impl FnOnce() -> Result<()>) -> Result<StashEntry> {
    let _write = session.acquire_write_lock()?;
    let _stash = stash_write_lock();
    require_save_writable(&session.git)?;
    if preflight(session, None)? != request.expected_state { return Err(crate::write_guard::stale()); }
    let before = list(&session.git)?;
    // `create` fixes the expected worktree/index trees without changing refs or
    // cleaning files. A later external push must not become this save's receipt.
    let candidate = session.git.run_write_with_timeout(&["stash", "create"], timeout)?;
    if candidate.timed_out || candidate.exit_code != 0 {
        return Err(recovery::pending_write(AppError::coded(ErrorKind::Git,
            "errors.stash.failedSaveStash", "保存 stash 失败：{detail}", &[("detail", candidate.stderr.trim().to_string())])));
    }
    let oid = candidate.stdout_lossy().trim().to_string();
    if oid.is_empty() {
        return Err(AppError::coded(ErrorKind::Rejected, "errors.stash.noChangesToStash", "没有可保存的改动", &[]));
    }
    let expected_trees = saved_trees(&session.git, &oid)?;
    let mut args: Vec<&str> = vec!["stash", "push", "--quiet"];
    let message = request.message.trim();
    if !message.is_empty() {
        args.extend_from_slice(&["--message", message]);
    }
    if request.keep_index {
        args.push("--keep-index");
    }
    require_save_writable(&session.git)?;
    if preflight(session, None)? != request.expected_state { return Err(crate::write_guard::stale()); }
    let output = session.git.run_write_with_timeout(&args, timeout).map_err(recovery::pending_write)?;
    // Git can append the stash and clean some paths before returning nonzero
    // (for example, when a required smudge filter fails during cleanup).
    if output.timed_out || output.exit_code != 0 {
        return Err(recovery::pending_write(AppError::coded(ErrorKind::Git,
            "errors.stash.failedSaveStash", "保存 stash 失败：{detail}", &[("detail", output.stderr.trim().to_string())])));
    }
    after_write().map_err(recovery::pending_write)?;
    let after = list(&session.git).map_err(recovery::pending_write)?;
    let mut matches = Vec::new();
    let mut previous = before.clone();
    let mut added = Vec::new();
    // Match old occurrences from oldest to newest. The remaining prefix can
    // include both our save and an external push, even if OIDs repeat.
    for entry in after.iter().rev() {
        if let Some(index) = previous.iter().rposition(|old| old.oid == entry.oid
            && old.message == entry.message && old.time_ms == entry.time_ms) {
            previous.remove(index);
        } else { added.push(entry.clone()); }
    }
    // Git does not append a reflog row if the saved object is already the tip.
    if added.is_empty() && after == before {
        if let Some(entry) = after.first() {
            if matches_save_message(entry, message)
                && saved_trees(&session.git, &entry.oid).map_err(recovery::pending_write)? == expected_trees {
                return Ok(entry.clone());
            }
        }
    }
    for entry in added {
        if saved_trees(&session.git, &entry.oid).map_err(recovery::pending_write)? == expected_trees {
            matches.push(entry);
        }
    }
    if matches.len() == 1 && matches_save_message(&matches[0], message) { return Ok(matches.remove(0)); }
    Err(AppError::coded(ErrorKind::NeedsVerification, "errors.stash.savedIdentityUncertain",
        "保存命令已执行，但无法唯一确认新 stash 条目，请核对列表；未删除任何条目", &[]))
}

fn validate_new_branch(git: &Git, name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.stash.branchNameEmpty",
            "分支名不能为空",
            &[],
        ));
    }
    let output = git.run(&["check-ref-format", "--branch", trimmed])?;
    if output.exit_code != 0 {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.stash.invalidBranchName",
            "分支名不合法：{name}",
            &[("name", trimmed.to_string())],
        ));
    }
    let exists = git.run(&[
        "show-ref",
        "--verify",
        "--quiet",
        &format!("refs/heads/{trimmed}"),
    ])?;
    if exists.exit_code == 0 {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.stash.branchAlreadyExists",
            "分支已存在：{name}",
            &[("name", trimmed.to_string())],
        ));
    }
    Ok(())
}

/// Apply a stash.  Existing dirty paths are captured first.  A conflict keeps
/// the entry, even when delete-after was selected.
pub fn apply(
    session: &WorktreeSession,
    recovery_store: &RecoveryStore,
    archive_store: &StashArchiveStore,
    request: &StashApplyRequest,
) -> Result<StashApplyOutcome> {
    let _write = session.acquire_write_lock()?;
    let _stash = stash_write_lock();
    let entry = find_selected(&session.git, &request.oid, request.entry_key.as_deref())?.ok_or_else(|| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.stash.stashEntryNoLongerExists2",
            "stash 条目已不存在，请刷新列表",
            &[],
        )
    })?;
    let identity = session.require_identity()?;
    let context = crate::write_guard::context(&session.git)?;
    let mut paths = apply_paths(&session.git, &entry.oid)?;
    if request.new_branch.as_deref().is_some_and(|name| !name.trim().is_empty()) {
        paths.extend(crate::write_guard::paths(&session.git, &[&format!("{}^1", entry.oid)])?);
        paths.sort(); paths.dedup();
    }
    require_preflight(session, &entry.oid, request.entry_key.as_deref(), &request.expected_state)?;
    let mut prepared = crate::write_guard::PreparedState::capture(session, &paths)?;
    let recovery_record = crate::write_guard::protect(session, recovery_store,
        recovery::RecoverySource::StashApply, &format!("stash {}", entry.oid), &paths)?;
    require_preflight(session, &entry.oid, request.entry_key.as_deref(), &request.expected_state)?;
    crate::write_guard::require_context(session, &context)?;
    let recovery_id = recovery_record.as_ref().map(|record| record.id.clone());

    let mut branch = None;
    let mut branch_base = None;
    let mut branch_confirmation = None;
    if let Some(name) = request
        .new_branch
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        validate_new_branch(&session.git, name)?;
        let base = session
            .git
            .run_ok(
                &["rev-parse", &format!("{}^1", entry.oid)],
                "读取 stash 基准提交",
            )?
            .stdout_lossy()
            .trim()
            .to_string();
        let expected = prepared.checkout_expected(session, &base)?;
        require_preflight(session, &entry.oid, request.entry_key.as_deref(), &request.expected_state)?;
        prepared.require(session)?;
        let switch = session
            .git
            .run_write(&["switch", "--quiet", "-c", name.trim(), &base])?;
        if switch.exit_code != 0 {
            return Err(AppError::coded(
                ErrorKind::NeedsVerification,
                "errors.stash.failedCreateBranchFromStash",
                "从 stash 创建分支失败：{detail}",
                &[("detail", switch.stderr.trim().to_string())],
            ));
        }
        branch = Some(name.trim().to_string());
        branch_base = Some(base);
        branch_confirmation = Some(expected);
    }

    let result = (|| -> Result<StashApplyOutcome> {
        // Re-read immediately before the mutation, like drop/clear.  Creating a
        // branch above may have taken time while another process changed the list.
        find_selected(&session.git, &request.oid, Some(&entry.key))?.ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.stash.stashEntryChangedBeforeApply",
                "stash 条目已变化，请刷新后重试",
                &[],
            )
        })?;

        // A new branch was created by us; otherwise the original confirmation must
        // still match immediately before apply (including HEAD and branch name).
        if let (Some(name), Some(base)) = (&branch, &branch_base) {
            prepared.after_checkout(session, name, base, branch_confirmation.take().ok_or_else(crate::write_guard::stale)?)
                .map_err(recovery::pending_write)?;
        } else {
            require_preflight(session, &entry.oid, request.entry_key.as_deref(), &request.expected_state)?;
        }
        let branch_record = if branch.is_some() {
            let paths = apply_paths(&session.git, &entry.oid)?;
            let record = crate::write_guard::protect(session, recovery_store,
                recovery::RecoverySource::StashApply, "stash apply after branch switch", &paths)?;
            prepared.require(session).map_err(recovery::pending_write)?;
            record
        } else { None };
        let mut args = vec!["stash", "apply", "--quiet"];
        if request.restore_index {
            args.push("--index");
        }
        args.push(&entry.oid);
        prepared.require(session).map_err(recovery::pending_write)?;
        let output = session.git.run_write(&args).map_err(recovery::pending_write)?;
        if let Some(record) = &recovery_record {
            recovery_store.finish_snapshot(record)?;
        }
        if let Some(record) = &branch_record { recovery_store.finish_snapshot(record)?; }
        let state = status::read(&session.git).map_err(recovery::pending_write)?;
        let conflicted = state.entries.iter().any(|entry| entry.unmerged);
        let applied = output.exit_code == 0 && !conflicted;
        if output.exit_code != 0 && !conflicted {
            return Err(AppError::coded(
                ErrorKind::NeedsVerification,
                "errors.stash.failedApplyStash",
                "应用 stash 失败：{detail}",
                &[("detail", output.stderr.trim().to_string())],
            ));
        }

        let mut deleted = false;
        let mut archive_id = None;
        let mut cleanup_error = None;
        if applied && request.delete_after {
            match archive_and_drop(session, archive_store, identity, &entry, |_| Ok(())) {
                Ok((record, dropped)) => {
                    archive_id = Some(record.id);
                    deleted = dropped;
                }
                Err(error) => {
                    // The worktree already contains the applied stash.  Report the
                    // cleanup boundary instead of returning a bare error that makes
                    // the user apply the same stash a second time.
                    cleanup_error = Some(error.payload());
                }
            }
        }

        Ok(StashApplyOutcome {
            oid: entry.oid,
            applied,
            conflicted,
            deleted,
            archive_id,
            recovery_id: recovery_id.clone(),
            branch: branch.clone(),
            cleanup_error,
            error: None,
        })
    })();
    match result {
        Err(error) if branch.is_some() => {
            let branch_name = branch.unwrap();
            let error = AppError::coded(ErrorKind::NeedsVerification, "errors.stash.branchCreatedApplyIncomplete",
                "已创建并切换分支 {branch}，stash 应用尚未确认完成：{cause}", &[("branch", branch_name.clone())]).cause(error);
            Ok(StashApplyOutcome { oid: request.oid.clone(), applied: false, conflicted: false, deleted: false,
                archive_id: None, recovery_id, branch: Some(branch_name), cleanup_error: None, error: Some(error.payload()) })
        }
        other => other,
    }
}

fn archive_and_drop(
    session: &WorktreeSession,
    archive_store: &StashArchiveStore,
    identity: WorktreeIdentity,
    entry: &StashEntry,
    mut hook: impl FnMut(crate::stash_transaction::StashDeletePhase) -> Result<()>,
) -> Result<(StashArchiveRecord, bool)> {
    let confirmation = session.confirmation_version();
    let record = archive_store.archive(
        &session.git,
        identity.clone(),
        &entry.oid,
        &entry.message,
        entry.branch.clone(),
    )?;
    crate::stash_transaction::delete(session, &identity,
        crate::stash_transaction::Deletion::One(entry), |phase| {
            hook(phase)?;
            if phase != crate::stash_transaction::StashDeletePhase::AfterCommit
                && session.confirmation_version() != confirmation { return Err(crate::write_guard::stale()); }
            Ok(())
        })?;
    Ok((record, true))
}

/// Drop one entry after archiving it.  The caller already holds both the
/// session write lock and the repository-wide stash lock.
pub(crate) fn drop_locked(
    session: &WorktreeSession,
    archive_store: &StashArchiveStore,
    oid: &str,
) -> Result<StashDropOutcome> {
    let entry = find_entry(&session.git, oid)?.ok_or_else(|| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.stash.stashEntryNoLongerExists3",
            "stash 条目已不存在，请刷新列表",
            &[],
        )
    })?;
    let identity = ensure_identity(&session.git)?;
    let (record, deleted) = archive_and_drop(session, archive_store, identity, &entry, |_| Ok(()))?;
    Ok(StashDropOutcome {
        oid: entry.oid,
        deleted,
        archive_id: Some(record.id),
    })
}

/// Drop one entry after archiving it.
pub fn drop(
    session: &WorktreeSession,
    archive_store: &StashArchiveStore,
    oid: &str,
) -> Result<StashDropOutcome> {
    drop_with_hook(session, archive_store, oid, |_| Ok(()))
}

pub fn drop_with_hook(
    session: &WorktreeSession, archive_store: &StashArchiveStore, oid: &str,
    hook: impl FnMut(crate::stash_transaction::StashDeletePhase) -> Result<()>,
) -> Result<StashDropOutcome> {
    drop_selected_controlled(session, archive_store, oid, None, None, hook)
}

pub fn drop_selected(session: &WorktreeSession, archive_store: &StashArchiveStore,
    oid: &str, key: &str, expected_state: &str) -> Result<StashDropOutcome> {
    drop_selected_controlled(session, archive_store, oid, Some(key), Some(expected_state), |_| Ok(()))
}

pub fn drop_selected_with_hook(session: &WorktreeSession, archive_store: &StashArchiveStore,
    oid: &str, key: &str, expected_state: &str,
    hook: impl FnMut(crate::stash_transaction::StashDeletePhase) -> Result<()>) -> Result<StashDropOutcome> {
    drop_selected_controlled(session, archive_store, oid, Some(key), Some(expected_state), hook)
}

fn drop_selected_controlled(session: &WorktreeSession, archive_store: &StashArchiveStore,
    oid: &str, key: Option<&str>, expected_state: Option<&str>,
    mut hook: impl FnMut(crate::stash_transaction::StashDeletePhase) -> Result<()>) -> Result<StashDropOutcome> {
    let _write = session.acquire_write_lock()?;
    let _stash = stash_write_lock();
    if let Some(expected) = expected_state { require_preflight(session, oid, key, expected)?; }
    let entry = find_selected(&session.git, oid, key)?.ok_or_else(|| AppError::coded(
        ErrorKind::Rejected, "errors.stash.stashEntryNoLongerExists3", "stash 条目已不存在，请刷新列表", &[]))?;
    let identity = ensure_identity(&session.git)?;
    let (record, deleted) = archive_and_drop(session, archive_store, identity, &entry, |phase| {
        hook(phase)?;
        if phase != crate::stash_transaction::StashDeletePhase::AfterCommit {
            if let Some(expected) = expected_state { require_preflight(session, oid, key, expected)?; }
        }
        Ok(())
    })?;
    Ok(StashDropOutcome { oid: entry.oid, deleted, archive_id: Some(record.id) })
}

/// Clear every entry.  The complete set is archived first; if the list changes
/// between confirmation and execution, nothing is deleted.
pub fn clear(
    session: &WorktreeSession,
    archive_store: &StashArchiveStore,
    expected_oids: &[String],
) -> Result<StashClearOutcome> {
    clear_with_hook(session, archive_store, expected_oids, |_| Ok(()))
}

pub fn clear_with_hook(
    session: &WorktreeSession, archive_store: &StashArchiveStore, expected_oids: &[String],
    hook: impl FnMut(crate::stash_transaction::StashDeletePhase) -> Result<()>,
) -> Result<StashClearOutcome> {
    clear_controlled(session, archive_store, expected_oids, None, hook)
}

pub fn clear_confirmed(session: &WorktreeSession, archive_store: &StashArchiveStore,
    expected_oids: &[String], expected_state: &str) -> Result<StashClearOutcome> {
    clear_controlled(session, archive_store, expected_oids, Some(expected_state), |_| Ok(()))
}

pub fn clear_confirmed_with_hook(session: &WorktreeSession, archive_store: &StashArchiveStore,
    expected_oids: &[String], expected_state: &str,
    hook: impl FnMut(crate::stash_transaction::StashDeletePhase) -> Result<()>) -> Result<StashClearOutcome> {
    clear_controlled(session, archive_store, expected_oids, Some(expected_state), hook)
}

fn clear_controlled(session: &WorktreeSession, archive_store: &StashArchiveStore,
    expected_oids: &[String], expected_state: Option<&str>,
    mut hook: impl FnMut(crate::stash_transaction::StashDeletePhase) -> Result<()>) -> Result<StashClearOutcome> {
    let _write = session.acquire_write_lock()?;
    let _stash = stash_write_lock();
    let current = list(&session.git)?;
    if let Some(expected) = expected_state {
        if collection_preflight(session)? != expected { return Err(crate::write_guard::stale()); }
    }
    let current_oids: Vec<String> = current.iter().map(|entry| entry.oid.clone()).collect();
    if current_oids != expected_oids {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.stash.stashListChangedRefresh",
            "stash 列表已变化，请刷新后重试",
            &[],
        ));
    }
    let identity = ensure_identity(&session.git)?;
    let confirmation = session.confirmation_version();
    let mut archived = Vec::new();
    for entry in &current {
        let record = archive_store.archive(
            &session.git,
            identity.clone(),
            &entry.oid,
            &entry.message,
            entry.branch.clone(),
        )?;
        archived.push(record.id);
    }
    if current.is_empty() { return Ok(StashClearOutcome { archived, deleted: 0 }); }
    crate::stash_transaction::delete(session, &identity,
        crate::stash_transaction::Deletion::All(&current), |phase| {
            hook(phase)?;
            if phase != crate::stash_transaction::StashDeletePhase::AfterCommit {
                if session.confirmation_version() != confirmation { return Err(crate::write_guard::stale()); }
                if let Some(expected) = expected_state {
                    if collection_preflight(session)? != expected { return Err(crate::write_guard::stale()); }
                }
            }
            Ok(())
        })?;
    Ok(StashClearOutcome { archived, deleted: expected_oids.len() as u32 })
}

/// Exposed for the recovery/notification flow and tests.
pub fn entry_at(git: &Git, index: u32) -> Result<Option<StashEntry>> {
    Ok(list(git)?.into_iter().find(|entry| entry.index == index))
}

/// System time helper used by tests that need to age an archive.
pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// The common git directory is the identity anchor for a stash list.
pub fn common_dir(session: &WorktreeSession) -> Result<String> {
    let output = session.git.run_ok(
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        "读取 stash 归属",
    )?;
    let path = output.stdout_lossy();
    Ok(Path::new(path.trim()).display().to_string())
}
