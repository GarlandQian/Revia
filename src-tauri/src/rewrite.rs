//! History rewriting (M6 H01–H04): cherry-pick, revert, reset, reword and
//! interactive rebase.
//!
//! O02 distinguishes the actions:
//! * rebase always saves dirty work with a real stash and restores it after the
//!   operation stops for good;
//! * cherry-pick/revert ask for stash / force / cancel when local changes would
//!   block or be overwritten.  Force never secretly stashes: recovery runs
//!   first and the blocking local changes are overwritten in place;
//! * reset keeps Git's Soft/Mixed/Hard/Keep effects.  Hard and Keep capture the
//!   worktree before writing; nothing is re-applied afterwards.
//!
//! All commits are bound by full OIDs and all interactive plans are validated
//! against the exact `upstream..HEAD` set before a single Git command runs.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::autostash::{AutostashRestoreOutcome, AutostashStore};
use crate::error::{AppError, AppErrorPayload, ErrorKind, Result};
use crate::git::Git;
use crate::git::status;
use crate::operation::{self, OperationKind, OperationState};
use crate::recovery::{self, ProtectedRecovery, RecoverySource, RecoveryStore, WorktreeIdentity};
use crate::session::WorktreeSession;
use crate::stash_archive::StashArchiveStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
    Keep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DirtyStrategy {
    /// Save with `git stash push -u`, then restore after the operation finishes.
    Stash,
    /// Recovery first, then overwrite only the local changes that block the
    /// operation.  Nothing is restored afterwards.
    Force,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RewritePreflight {
    /// Paths whose local changes would block or be overwritten by the action.
    pub paths: Vec<String>,
    /// A choice between stash / force / cancel is required before executing.
    pub requires_choice: bool,
    pub has_staged_changes: bool,
    /// At least one selected commit is contained in a local remote-tracking
    /// ref.  Reword / interactive rebase use this to warn before rewriting
    /// already published history; no network request is made.
    pub published: bool,
    /// 当前分支命中本地与当前账号GitHub缓存的并集；历史确认必须显示该保护提示。
    #[serde(default)]
    pub protected_branch: Option<String>,
    pub overwrite_fingerprint: String,
    pub reset_fingerprint: String,
    pub context_version: String,
    pub overwrite_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CherryPickRequest {
    /// Commits in oldest-first application order.  Merge commits are rejected.
    pub oids: Vec<String>,
    /// The HEAD the user saw when confirming.
    pub expected_head: String,
    pub expected_context: String,
    #[serde(default)]
    pub dirty_strategy: Option<DirtyStrategy>,
    #[serde(default)]
    pub expected_state: Option<String>,
    /// Empty commits are skipped by default; true creates them.
    #[serde(default)]
    pub keep_empty: bool,
    /// Add the `-x` source suffix only when the commit is already published on
    /// a local remote-tracking ref.
    #[serde(default)]
    pub include_source: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevertRequest {
    pub oids: Vec<String>,
    pub expected_head: String,
    pub expected_context: String,
    #[serde(default)]
    pub dirty_strategy: Option<DirtyStrategy>,
    #[serde(default)]
    pub expected_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetRequest {
    pub target: String,
    pub mode: ResetMode,
    pub expected_head: String,
    pub expected_context: String,
    #[serde(default)]
    pub expected_state: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RebaseAction {
    Pick,
    Reword,
    Edit,
    Squash,
    Fixup,
    Drop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebasePlanEntry {
    pub oid: String,
    pub action: RebaseAction,
    /// Required for each `Reword` entry; bound to that entry's full OID.
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractiveRebaseRequest {
    /// Parent of the oldest commit in the rewritten range.
    pub upstream: String,
    pub expected_head: String,
    pub expected_context: String,
    pub entries: Vec<RebasePlanEntry>,
    pub expected_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RewriteOutcome {
    pub action: String,
    /// True when the command completed and no operation remains.
    pub completed: bool,
    pub state: Option<OperationState>,
    pub created_oid: Option<String>,
    pub autostash: Option<AutostashRestoreOutcome>,
    /// Cherry-pick only: commits skipped because they were empty.
    #[serde(default)]
    pub skipped_empty: Vec<String>,
    #[serde(default)]
    pub skipped: Vec<String>,
    /// Requested commits that Git finished successfully, in execution order.
    #[serde(default)]
    pub applied: Vec<String>,
    /// The commit Git stopped on, when the sequence did not finish.
    #[serde(default)]
    pub failed_oid: Option<String>,
    /// Requested commits not attempted because the sequence stopped first.
    #[serde(default)]
    pub pending: Vec<String>,
    /// A conflict/failure or uncertain write result. Partial progress remains
    /// in the other fields even when no native operation exists to continue.
    #[serde(default)]
    pub error: Option<AppErrorPayload>,
}

fn normalize_oid(git: &Git, value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) || value.len() < 7 {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.invalidCommitId",
            "提交 ID 无效：{oid}",
            &[("oid", value.to_string())],
        ));
    }
    let output = git.run(&[
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("{value}^{{commit}}"),
    ])?;
    if output.exit_code != 0 {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.commitNotFound",
            "找不到提交：{oid}",
            &[("oid", value.to_string())],
        ));
    }
    let full = output.stdout_lossy().trim().to_string();
    if full.len() < 40 || !full.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::coded(
            ErrorKind::Internal,
            "errors.rewrite.gitDidNotReturnFullCommitId",
            "Git 没有返回完整提交 ID",
            &[],
        ));
    }
    Ok(full)
}

fn head(git: &Git) -> Result<String> {
    let output = git.run(&["rev-parse", "--verify", "HEAD"])?;
    if output.exit_code != 0 {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.headMissing",
            "当前分支还没有提交",
            &[],
        ));
    }
    Ok(output.stdout_lossy().trim().to_string())
}

fn require_expected_head(git: &Git, expected: &str) -> Result<String> {
    let actual = head(git)?;
    if actual != expected.trim() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.headChanged",
            "当前 HEAD 已变化，请刷新日志后重试",
            &[],
        ));
    }
    Ok(actual)
}

fn ensure_no_operation(git: &Git) -> Result<()> {
    if operation::detect(git, operation::conflict_count(git)?)?.is_some() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.anotherOperationInProgress",
            "已有进行中的操作，请先继续或中止该操作",
            &[],
        ));
    }
    Ok(())
}

fn parents(git: &Git, oid: &str) -> Result<Vec<String>> {
    let output = git.run_ok(&["rev-list", "--parents", "-n", "1", oid], "读取提交关系")?;
    let text = output.stdout_lossy();
    let mut fields = text.split_whitespace();
    let _ = fields.next();
    Ok(fields.map(str::to_string).collect())
}

/// Read the exact current-branch rewrite range, independently of log filters.
pub fn range(session: &WorktreeSession, target: &str, expected_head: &str) -> Result<Vec<crate::history::HistoryCommit>> {
    session.require_identity()?;
    let current = require_expected_head(&session.git, expected_head)?;
    let target = normalize_oid(&session.git, target)?;
    if session.git.run(&["merge-base", "--is-ancestor", &target, &current])?.exit_code != 0 {
        return Err(AppError::coded(ErrorKind::Rejected, "errors.rewrite.targetNotAncestor", "所选提交不在当前分支历史内，请重新选择", &[]));
    }
    let parent = parents(&session.git, &target)?.into_iter().next().ok_or_else(||
        AppError::coded(ErrorKind::Rejected, "errors.rewrite.rootCommitNotSupported", "不支持改写根提交", &[]))?;
    let entries = crate::git::history::read_range(&session.git, Some(&parent), &current, 5001)?;
    if entries.len() > 5000 {
        return Err(AppError::coded(ErrorKind::Rejected, "errors.rewrite.rangeTooLarge", "改写范围超过 5000 个提交，请选择较小的范围", &[]));
    }
    if entries.iter().any(|entry| entry.parents.len() > 1) {
        return Err(AppError::coded(ErrorKind::Rejected, "errors.rewrite.mergeCommitNotSupported", "不支持对合并提交执行此操作", &[]));
    }
    require_expected_head(&session.git, expected_head)?;
    Ok(entries.iter().map(crate::history::describe_commit_public).collect())
}

fn require_linear(git: &Git, oid: &str) -> Result<()> {
    if parents(git, oid)?.len() > 1 {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.mergeCommitNotSupported",
            "不支持对合并提交执行此操作",
            &[],
        ));
    }
    Ok(())
}

/// Paths changed by a commit relative to its first parent (or the empty tree
/// for a root commit).  `--no-renames` makes the intersection with status paths
/// exact; a rename is represented by both its old and new path.
pub(crate) fn changed_paths(git: &Git, oid: &str) -> Result<Vec<String>> {
    let output = git.run(&[
        "diff-tree",
        "--no-commit-id",
        "--name-only",
        "-r",
        "--no-renames",
        "--root",
        "-z",
        oid,
    ])?;
    if output.exit_code != 0 {
        return Err(AppError::coded(
            ErrorKind::Git,
            "errors.rewrite.failedReadCommitPaths",
            "读取提交涉及的文件失败：{detail}",
            &[("detail", output.stderr.trim().to_string())],
        ));
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| std::str::from_utf8(part).map(str::to_string).map_err(|_| crate::write_guard::stale()))
        .collect::<Result<Vec<_>>>()?)
}

/// Detect the local changes that would block or be overwritten by
/// cherry-pick/revert.  A dirty index always blocks the sequencer, even when
/// the staged path is unrelated to the selected commit.
pub fn preflight(session: &WorktreeSession, oids: &[String]) -> Result<RewritePreflight> {
    let state = status::read(&session.git)?;
    let mut changed = std::collections::BTreeSet::new();
    let mut published = false;
    for oid in oids {
        let full = normalize_oid(&session.git, oid)?;
        published |= published_on_local_remote(&session.git, &full)?;
        changed.extend(changed_paths(&session.git, &full)?);
    }
    let mut paths = std::collections::BTreeSet::new();
    let mut has_staged_changes = false;
    for entry in &state.entries {
        // The sequencer refuses any staged change, whether related or not.
        if entry.changes == status::ChangesKind::Staged {
            has_staged_changes = true;
            paths.insert(entry.path.clone());
            paths.extend(entry.original_path.iter().cloned());
        }
        if changed.contains(&entry.path) {
            paths.insert(entry.path.clone());
        }
        if let Some(original) = &entry.original_path {
            if changed.contains(original) {
                paths.insert(entry.path.clone());
                paths.insert(original.clone());
            }
        }
    }
    for path in &changed {
        if session.path.join(path).symlink_metadata().is_ok()
            && session.git.run(&["--literal-pathspecs", "ls-files", "--error-unmatch", "--", path])?.exit_code != 0 {
            paths.insert(path.clone());
        }
    }
    let paths: Vec<String> = paths.into_iter().collect();
    let mut overwrite_paths = crate::write_guard::paths(&session.git, &[])?;
    overwrite_paths.extend(changed);
    overwrite_paths.sort(); overwrite_paths.dedup();
    let overwrite_fingerprint = state_fingerprint(session, &overwrite_paths)?;
    let reset_fingerprint = state_fingerprint(session, &reset_paths(session, oids.first().map(String::as_str))?)?;
    Ok(RewritePreflight {
        requires_choice: !paths.is_empty(),
        has_staged_changes,
        paths,
        published,
        protected_branch: None,
        overwrite_fingerprint,
        reset_fingerprint,
        context_version: crate::write_guard::context(&session.git)?,
        overwrite_paths,
    })
}

/// Whether moving HEAD to target would remove history already represented by
/// local remote-tracking refs. No network request is made for this warning.
pub fn reset_preflight(session: &WorktreeSession, target: &str) -> Result<RewritePreflight> {
    let target = normalize_oid(&session.git, target)?;
    let mut result = preflight(session, &[target.clone()])?;
    result.published = false;
    let tips = session.git.run_ok(&["for-each-ref", "--format=%(objectname)", "refs/remotes"], "读取已发布范围")?;
    for tip in tips.stdout_lossy().lines() {
        let common = session.git.run(&["merge-base", "--all", "HEAD", tip])?;
        if common.exit_code == 1 { continue; }
        let common = common.into_success("读取已发布共同祖先")?;
        for base in common.stdout_lossy().lines() {
            let removed = session.git.run_ok(&["rev-list", "--max-count=1", base, "--not", &target], "检查 reset 已发布范围")?;
            if !removed.stdout.is_empty() { result.published = true; return Ok(result); }
        }
    }
    Ok(result)
}

fn protect(
    session: &WorktreeSession,
    store: &RecoveryStore,
    source: RecoverySource,
    target: &str,
    paths: &[String],
) -> Result<Option<ProtectedRecovery>> {
    crate::write_guard::protect(session, store, source, target, paths)
}

fn reset_paths(session: &WorktreeSession, target: Option<&str>) -> Result<Vec<String>> {
    crate::write_guard::paths(&session.git, &target.into_iter().collect::<Vec<_>>())
}

fn state_fingerprint(session: &WorktreeSession, paths: &[String]) -> Result<String> {
    session.require_identity()?;
    crate::write_guard::confirmation(session, paths)
}

fn require_state(session: &WorktreeSession, paths: &[String], expected: Option<&str>) -> Result<()> {
    let current = state_fingerprint(session, paths)?;
    if expected == Some(current.as_str()) {
        return Ok(());
    }
    Err(AppError::coded(
        ErrorKind::Rejected,
        "errors.rewrite.stateChanged",
        "相关文件或暂存状态已变化，请刷新后重新确认",
        &[],
    ))
}

/// Recovery first, then discard exactly the blocking paths.  This is the
/// "force overwrite" branch; the discarded state is available in the recovery
/// store, never in a hidden stash.
fn force_overwrite(
    session: &WorktreeSession,
    paths: &[String],
    prepared: &mut crate::write_guard::PreparedState,
) -> Result<()> {
    let state = status::read(&session.git)?;
    let mut wrote = false;
    for path in paths {
        prepared.require(session).map_err(|error| if wrote { recovery::pending_write(error) } else { error })?;
        let entry = state.entries.iter().find(|entry| &entry.path == path
            || entry.original_path.as_ref() == Some(path));
        if entry.is_none() || entry.is_some_and(|entry| entry.changes == status::ChangesKind::Untracked) {
            let output = session.git.run_write(&["--literal-pathspecs", "clean", "-f", "-x", "--", path]).map_err(recovery::pending_write)?;
            if output.exit_code != 0 {
                return Err(AppError::coded(
                    ErrorKind::NeedsVerification,
                    "errors.rewrite.failedOverwriteUntrackedPath",
                    "无法覆盖未跟踪文件：{path}",
                    &[("path", path.clone())],
                ));
            }
        } else {
            let output = session.git.run_write(&[
                "--literal-pathspecs",
                "restore",
                "--source=HEAD",
                "--staged",
                "--worktree",
                "--",
                path,
            ]).map_err(recovery::pending_write)?;
            if output.exit_code != 0 {
                return Err(AppError::coded(
                    ErrorKind::NeedsVerification,
                    "errors.rewrite.failedOverwriteLocalPath",
                    "无法覆盖本地改动：{path}",
                    &[("path", path.clone())],
                ));
            }
        }
        wrote = true;
        // A completed cleanup owns only this path. Hooks or terminal edits to
        // any later path/index must stop us before that path is overwritten.
        prepared.after_cleanup(session, std::slice::from_ref(path)).map_err(recovery::pending_write)?;
    }
    Ok(())
}

/// Returns the pre-write recovery snapshot (and creates an autostash link when
/// requested).  The caller must finish the snapshot once Git has run.
fn prepare_dirty(
    session: &WorktreeSession,
    recovery_store: &RecoveryStore,
    autostash_store: &AutostashStore,
    action: &str,
    preflight: &RewritePreflight,
    strategy: Option<DirtyStrategy>,
    expected_state: Option<&str>,
) -> Result<(Option<ProtectedRecovery>, bool, crate::write_guard::PreparedState)> {
    // Even a clean preflight is a confirmation. New dirty paths or ignored
    // obstructions introduced afterwards must invalidate it for every strategy.
    require_state(session, &preflight.overwrite_paths, expected_state)?;
    let mut prepared = crate::write_guard::PreparedState::capture(session, &preflight.overwrite_paths)?;
    if preflight.requires_choice && strategy.is_none() {
        return Err(AppError::coded(ErrorKind::Rejected,
            "errors.rewrite.localChangesWouldBeOverwritten",
            "本地改动会被覆盖，请选择先保存、强制覆盖或取消",
            &[("count", preflight.paths.len().to_string())]));
    }
    let record = protect(session, recovery_store, RecoverySource::HistoryOverwrite,
        action, &preflight.overwrite_paths)?;
    require_state(session, &preflight.overwrite_paths, expected_state)?;
    prepared.require(session)?;
    if !preflight.requires_choice { return Ok((record, false, prepared)); }
    match strategy {
        Some(DirtyStrategy::Stash) => {
            let state = status::read(&session.git)?;
            if preflight.paths.iter().any(|path| !state.entries.iter().any(|entry| &entry.path == path || entry.original_path.as_ref() == Some(path))) {
                return Err(AppError::coded(ErrorKind::Rejected,
                    "errors.rewrite.ignoredCannotStash",
                    "Git stash 不会保存这些被忽略的文件；请选择先回收再强制覆盖，或取消", &[]));
            }
            let touched: Vec<_> = state.entries.iter().flat_map(|entry|
                std::iter::once(entry.path.clone()).chain(entry.original_path.iter().cloned())).collect();
            autostash_store.save_before_confirmed(session, recovery_store, action, &prepared)?;
            prepared.after_cleanup(session, &touched).map_err(recovery::pending_write)?;
            Ok((record, true, prepared))
        }
        Some(DirtyStrategy::Force) => {
            force_overwrite(session, &preflight.paths, &mut prepared)?;
            Ok((record, false, prepared))
        }
        None => unreachable!(),
    }

}

pub(crate) fn finish(
    session: &WorktreeSession,
    recovery_store: &RecoveryStore,
    autostash_store: &AutostashStore,
    archive_store: &StashArchiveStore,
    action: &str,
    output_exit: i32,
    stderr: &str,
    restore_autostash: bool,
    skipped_empty: Vec<String>,
) -> Result<RewriteOutcome> {
    let conflicts = operation::conflict_count(&session.git).map_err(recovery::pending_write)?;
    let state = operation::detect(&session.git, conflicts).map_err(recovery::pending_write)?;
    let completed = state.is_none();
    let mut error = None;
    if output_exit != 0 {
        let app = AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.rewrite.actionNeedsVerification",
            "{action} 未成功完成，请检查当前状态：{detail}",
            &[
                ("action", action.to_string()),
                ("detail", stderr.trim().to_string()),
            ],
        );
        // A real stop (conflict or next edit stop) is representable.  If Git
        // failed without leaving an operation, the caller must not pretend it
        // succeeded.
        if state.is_none() {
            return Err(app);
        }
        // Stopping at a conflict of this action is the normal native stop;
        // the state and receipt drive the notice. Other stops still need checking.
        if conflicts == 0 {
            error = Some(app.payload());
        }
    }
    let autostash = if completed && restore_autostash {
        autostash_store.restore_if_ready(session, recovery_store, archive_store).map_err(recovery::pending_write)?
    } else {
        None
    };
    Ok(RewriteOutcome {
        action: action.to_string(),
        completed,
        state,
        created_oid: None,
        autostash,
        skipped_empty,
        skipped: Vec::new(),
        applied: Vec::new(),
        failed_oid: None,
        pending: Vec::new(),
        error,
    })
}

fn published_on_local_remote(git: &Git, oid: &str) -> Result<bool> {
    let output = git.run(&[
        "for-each-ref",
        "--contains",
        oid,
        "--format=%(refname)",
        "refs/remotes",
    ])?;
    Ok(output.exit_code == 0 && !output.stdout_lossy().trim().is_empty())
}

pub(crate) fn empty_cherry_pick(git: &Git) -> Result<bool> {
    if operation::conflict_count(git)? > 0 {
        return Ok(false);
    }
    let staged = git.run(&["diff", "--cached", "--quiet", "HEAD", "--"])?;
    // Index equal to HEAD and no unmerged entries means the sequencer failed
    // because the picked change is already present.
    Ok(staged.exit_code == 0)
}

pub fn cherry_pick(
    session: &WorktreeSession,
    recovery_store: &RecoveryStore,
    autostash: &AutostashStore,
    archive_store: &StashArchiveStore,
    request: &CherryPickRequest,
) -> Result<RewriteOutcome> {
    let _write = session.acquire_write_lock()?;
    if request.oids.is_empty() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.noCommitsSelected",
            "未选择提交",
            &[],
        ));
    }
    ensure_no_operation(&session.git)?;
    crate::history_sequence::ensure_empty(autostash.data_directory(), session)?;
    require_expected_head(&session.git, &request.expected_head)?;
    crate::write_guard::require_context(session, &request.expected_context)?;
    let oids: Vec<String> = request
        .oids
        .iter()
        .map(|oid| normalize_oid(&session.git, oid))
        .collect::<Result<_>>()?;
    for oid in &oids {
        require_linear(&session.git, oid)?;
    }
    let source_oids = if request.include_source {
        oids.iter().filter_map(|oid| match published_on_local_remote(&session.git, oid) {
            Ok(true) => Some(Ok(oid.clone())), Ok(false) => None, Err(error) => Some(Err(error)),
        }).collect::<Result<Vec<_>>>()?
    } else { Vec::new() };
    let preflight = preflight(session, &oids)?;
    let (recovery_record, stash_mode, prepared) = prepare_dirty(
        session,
        recovery_store,
        autostash,
        "cherry-pick",
        &preflight,
        request.dirty_strategy,
        request.expected_state.as_deref(),
    )?;
    let plan = (|| {
        require_expected_head(&session.git, &request.expected_head)?;
        crate::write_guard::require_context(session, &request.expected_context)?;
        crate::history_sequence::create(session, OperationKind::CherryPick,
            oids, source_oids, request.keep_empty, stash_mode, &request.expected_head, &request.expected_context, prepared)
    })().map_err(|error| if preflight.requires_choice { recovery::pending_write(error) } else { error })?;
    let outcome = crate::history_sequence::run(session, recovery_store, autostash, archive_store, plan)?;
    Ok(finish_sequence_snapshot(recovery_store, recovery_record.as_ref(), outcome))
}

pub fn revert(
    session: &WorktreeSession,
    recovery_store: &RecoveryStore,
    autostash: &AutostashStore,
    archive_store: &StashArchiveStore,
    request: &RevertRequest,
) -> Result<RewriteOutcome> {
    let _write = session.acquire_write_lock()?;
    if request.oids.is_empty() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.noCommitsSelected2",
            "未选择提交",
            &[],
        ));
    }
    ensure_no_operation(&session.git)?;
    crate::history_sequence::ensure_empty(autostash.data_directory(), session)?;
    require_expected_head(&session.git, &request.expected_head)?;
    crate::write_guard::require_context(session, &request.expected_context)?;
    let oids: Vec<String> = request
        .oids
        .iter()
        .map(|oid| normalize_oid(&session.git, oid))
        .collect::<Result<_>>()?;
    for oid in &oids {
        require_linear(&session.git, oid)?;
    }
    let preflight = preflight(session, &oids)?;
    let (recovery_record, stash_mode, prepared) = prepare_dirty(
        session,
        recovery_store,
        autostash,
        "revert",
        &preflight,
        request.dirty_strategy,
        request.expected_state.as_deref(),
    )?;
    let plan = (|| {
        require_expected_head(&session.git, &request.expected_head)?;
        crate::write_guard::require_context(session, &request.expected_context)?;
        crate::history_sequence::create(session, OperationKind::Revert,
            oids, Vec::new(), false, stash_mode, &request.expected_head, &request.expected_context, prepared)
    })().map_err(|error| if preflight.requires_choice { recovery::pending_write(error) } else { error })?;
    let outcome = crate::history_sequence::run(session, recovery_store, autostash, archive_store, plan)?;
    Ok(finish_sequence_snapshot(recovery_store, recovery_record.as_ref(), outcome))
}

pub(crate) fn finish_sequence_snapshot(store: &RecoveryStore, record: Option<&ProtectedRecovery>, mut outcome: RewriteOutcome) -> RewriteOutcome {
    if let Some(record) = record {
        if let Err(error) = store.finish_snapshot(record) {
            outcome.completed = false;
            let failure = recovery::pending_write(error).payload();
            if let Some(error) = &mut outcome.error {
                error.kind = ErrorKind::NeedsVerification;
                error.related.get_or_insert_with(Vec::new).push(failure);
            } else { outcome.error = Some(failure); }
        }
    }
    outcome
}

pub fn reset(
    session: &WorktreeSession,
    recovery_store: &RecoveryStore,
    autostash: &AutostashStore,
    archive_store: &StashArchiveStore,
    request: &ResetRequest,
) -> Result<RewriteOutcome> {
    let _write = session.acquire_write_lock()?;
    ensure_no_operation(&session.git)?;
    crate::history_sequence::ensure_empty(autostash.data_directory(), session)?;
    require_expected_head(&session.git, &request.expected_head)?;
    crate::write_guard::require_context(session, &request.expected_context)?;
    let target = normalize_oid(&session.git, &request.target)?;
    let paths = reset_paths(session, Some(&target))?;
    // Mixed replaces the index too; its confirmation must remain valid even
    // when HEAD did not move. Soft does not replace index/worktree content.
    require_state(session, &paths, request.expected_state.as_deref())?;
    // Soft/Mixed leave the worktree untouched; Hard/Keep write to it and a
    // recovery record must exist before Git starts.
    let recovery_record = if matches!(request.mode, ResetMode::Hard | ResetMode::Keep) {
        require_state(session, &paths, request.expected_state.as_deref())?;
        protect(
            session,
            recovery_store,
            RecoverySource::HistoryReset,
            &format!("reset {}", request.mode.mode_label()),
            &paths,
        )?
    } else {
        None
    };
    if matches!(request.mode, ResetMode::Hard | ResetMode::Keep) {
        let current_paths = reset_paths(session, Some(&target))?;
        if current_paths != paths {
            return Err(AppError::coded(ErrorKind::Rejected, "errors.rewrite.stateChanged", "相关文件或暂存状态已变化，请刷新后重新确认", &[]));
        }
        require_state(session, &current_paths, request.expected_state.as_deref())?;
    }
    require_expected_head(&session.git, &request.expected_head)?;
    crate::write_guard::require_context(session, &request.expected_context)?;
    let mode = request.mode.flag();
    let output = session.git.run_write(&["reset", mode, &target]).map_err(recovery::pending_write)?;
    let failed = output.exit_code != 0;
    let stderr = output.stderr.clone();
    if let Some(record) = &recovery_record {
        recovery_store.finish_snapshot(record)?;
    }
    if failed {
        return Err(AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.rewrite.resetNeedsVerification",
            "reset 未成功完成：{detail}",
            &[("detail", stderr.trim().to_string())],
        ));
    }
    // No autostash around reset: restoring after Hard would undo the reset the
    // user confirmed, and Soft/Mixed must retain their index/worktree effects.
    let mut outcome = finish(
        session,
        recovery_store,
        autostash,
        archive_store,
        "reset",
        0,
        "",
        false,
        Vec::new(),
    )?;
    outcome.created_oid = Some(target);
    Ok(outcome)
}

impl ResetMode {
    fn flag(self) -> &'static str {
        match self {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
            ResetMode::Keep => "--keep",
        }
    }

    fn mode_label(self) -> &'static str {
        match self {
            ResetMode::Soft => "soft",
            ResetMode::Mixed => "mixed",
            ResetMode::Hard => "hard",
            ResetMode::Keep => "keep",
        }
    }
}

fn range_commits(git: &Git, upstream: &str) -> Result<Vec<String>> {
    let output = git.run(&[
        "rev-list",
        "--reverse",
        "--topo-order",
        &format!("{upstream}..HEAD"),
    ])?;
    if output.exit_code != 0 {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.invalidRebaseRange",
            "无法读取 rebase 范围",
            &[],
        ));
    }
    let commits: Vec<String> = output
        .stdout_lossy()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    if commits.is_empty() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.emptyRebaseRange",
            "所选范围没有可改写的提交",
            &[],
        ));
    }
    Ok(commits)
}

fn subject(git: &Git, oid: &str) -> Result<String> {
    Ok(git
        .run_ok(&["show", "-s", "--format=%s", oid], "读取提交说明")?
        .stdout_lossy()
        .trim()
        .to_string())
}

fn write_state(path: &Path, content: &[u8]) -> Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(content)?;
    file.sync_all()?;
    Ok(())
}

fn write_script(path: &Path, content: &str) -> Result<()> {
    write_state(path, content.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Where an interactive rebase keeps its pre-validated plan and message queue.
///
/// This lives in Revia's data directory (never in the user's repository) so a
/// conflict or `edit` stop can resume with the remaining rewords.  The name is
/// derived from the canonical worktree path, which keeps concurrent worktrees
/// of the same repository isolated.
pub fn rebase_state_dir(data_directory: &Path, session: &WorktreeSession) -> PathBuf {
    let canonical = fs::canonicalize(&session.path).unwrap_or_else(|_| session.path.clone());
    let digest: String = Sha256::digest(canonical.to_string_lossy().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    data_directory.join("rebase-state").join(digest)
}

fn cleanup(directory: &Path) {
    let _ = fs::remove_dir_all(directory);
}

/// Editor environment for continuing an interactive rebase that stopped on a
/// conflict or an `edit` action.  `None` means this was not a Revia rebase (or
/// the state was already finished), so callers should let Git use its default.
pub fn rebase_editor_env(
    data_directory: &Path,
    session: &WorktreeSession,
) -> Result<Option<Vec<(String, String)>>> {
    let directory = rebase_state_dir(data_directory, session);
    let message_editor = directory.join("message-editor.sh");
    let sequence_editor = directory.join("sequence-editor.sh");
    if !message_editor.is_file() || !sequence_editor.is_file() {
        return Ok(None);
    }
    let identity: WorktreeIdentity = serde_json::from_slice(&fs::read(directory.join("identity.json"))?)?;
    let saved_directory_id: Option<String> = serde_json::from_slice(&fs::read(directory.join("rebase-directory.json"))?)?;
    let rebase_directory = operation::marker_path(&session.git, "rebase-merge")?;
    let current_directory_id = crate::platform::directory_identity(&rebase_directory)?;
    if identity.revalidate(&session.git).is_none()
        || saved_directory_id.is_none()
        || saved_directory_id != current_directory_id
    {
        return Err(AppError::coded(ErrorKind::Rejected, "errors.rewrite.rebaseIdentityChanged", "rebase 所属目标已变化，不能沿用旧计划", &[]));
    }
    Ok(Some(vec![
        (
            "GIT_EDITOR".to_string(),
            crate::platform::git_shell_argument(&message_editor),
        ),
        (
            "GIT_SEQUENCE_EDITOR".to_string(),
            crate::platform::git_shell_argument(&sequence_editor),
        ),
        (
            "REVIA_REBASE_STATE".to_string(),
            directory.to_string_lossy().into_owned(),
        ),
        (
            "REVIA_REBASE_TODO".to_string(),
            directory.join("todo.txt").to_string_lossy().into_owned(),
        ),
        (
            "REVIA_REBASE_DONE".to_string(),
            operation::marker_path(&session.git, "rebase-merge/done")?.to_string_lossy().into_owned(),
        ),
    ]))
}

pub fn clear_rebase_state(data_directory: &Path, session: &WorktreeSession) {
    cleanup(&rebase_state_dir(data_directory, session));
}

/// Reword one commit using a fixed generated plan.  The plan and message queue
/// are persisted in the data directory so a stop can be continued.
pub fn reword(
    session: &WorktreeSession,
    recovery_store: &RecoveryStore,
    autostash: &AutostashStore,
    archive_store: &StashArchiveStore,
    data_directory: &Path,
    target: &str,
    expected_head: &str,
    expected_context: &str,
    expected_state: &str,
    message: &str,
) -> Result<RewriteOutcome> {
    if message.trim().is_empty() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.commitMessageEmpty",
            "提交说明不能为空",
            &[],
        ));
    }
    ensure_no_operation(&session.git)?;
    crate::history_sequence::ensure_empty(autostash.data_directory(), session)?;
    require_expected_head(&session.git, expected_head)?;
    let target = normalize_oid(&session.git, target)?;
    require_linear(&session.git, &target)?;
    let parent = parents(&session.git, &target)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.rewrite.rootCommitCannotReword",
                "不能改写根提交的说明",
                &[],
            )
        })?;
    let commits = range_commits(&session.git, &parent)?;
    if !commits.contains(&target) {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.commitNotInCurrentBranch",
            "该提交不在当前分支上",
            &[],
        ));
    }
    let entries: Vec<RebasePlanEntry> = commits
        .iter()
        .map(|oid| RebasePlanEntry {
            oid: oid.clone(),
            action: if oid == &target {
                RebaseAction::Reword
            } else {
                RebaseAction::Pick
            },
            message: if oid == &target {
                Some(message.to_string())
            } else {
                None
            },
        })
        .collect();
    let request = InteractiveRebaseRequest {
        upstream: parent,
        expected_head: expected_head.to_string(),
        expected_context: expected_context.to_string(),
        expected_state: expected_state.to_string(),
        entries,
    };
    interactive(
        session,
        recovery_store,
        autostash,
        archive_store,
        data_directory,
        &request,
    )
}

pub fn interactive(
    session: &WorktreeSession,
    recovery_store: &RecoveryStore,
    autostash: &AutostashStore,
    archive_store: &StashArchiveStore,
    data_directory: &Path,
    request: &InteractiveRebaseRequest,
) -> Result<RewriteOutcome> {
    let _write = session.acquire_write_lock()?;
    ensure_no_operation(&session.git)?;
    crate::history_sequence::ensure_empty(autostash.data_directory(), session)?;
    require_expected_head(&session.git, &request.expected_head)?;
    crate::write_guard::require_context(session, &request.expected_context)?;
    let upstream = normalize_oid(&session.git, &request.upstream)?;
    let commits = range_commits(&session.git, &upstream)?;
    for oid in &commits {
        require_linear(&session.git, oid)?;
    }
    let mut requested = Vec::with_capacity(request.entries.len());
    for entry in &request.entries {
        let oid = normalize_oid(&session.git, &entry.oid)?;
        if !commits.contains(&oid) {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.rewrite.rebasePlanContainsCommitOutsideRange",
                "rebase 计划包含范围外的提交",
                &[],
            ));
        }
        if requested.contains(&oid) {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.rewrite.rebasePlanContainsDuplicate",
                "rebase 计划包含重复提交",
                &[],
            ));
        }
        requested.push(oid);
    }
    if requested.len() != commits.len() || commits.iter().any(|oid| !requested.contains(oid)) {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.rebasePlanMustContainEveryCommit",
            "rebase 计划必须包含范围内的每个提交",
            &[],
        ));
    }
    let reword_messages: Vec<String> = request
        .entries
        .iter()
        .filter(|entry| entry.action == RebaseAction::Reword)
        .map(|entry| entry.message.clone().unwrap_or_default())
        .collect();
    if reword_messages
        .iter()
        .any(|message| message.trim().is_empty())
    {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.rewordMessageMissing",
            "修改提交说明时必须填写新说明",
            &[],
        ));
    }
    if matches!(
        request.entries.first().map(|entry| entry.action),
        Some(RebaseAction::Squash | RebaseAction::Fixup)
    ) {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.rewrite.firstEntryCannotSquash",
            "第一项不能是 squash 或 fixup",
            &[],
        ));
    }
    for oid in &commits {
        require_linear(&session.git, oid)?;
    }

    let checked = preflight(session, &commits)?;
    require_state(session, &checked.overwrite_paths, Some(&request.expected_state))?;
    let mut prepared = crate::write_guard::PreparedState::capture(session, &checked.overwrite_paths)?;
    let dirty = status::read(&session.git)?;
    let touched: Vec<_> = dirty.entries.iter().flat_map(|entry|
        std::iter::once(entry.path.clone()).chain(entry.original_path.iter().cloned())).collect();
    let record = protect(session, recovery_store, RecoverySource::HistoryOverwrite,
        "interactive rebase", &checked.overwrite_paths)?;
    require_state(session, &preflight(session, &commits)?.overwrite_paths, Some(&request.expected_state))?;
    let mut todo = String::new();
    for entry in &request.entries {
        let action = action_label(entry.action);
        let oid = normalize_oid(&session.git, &entry.oid)?;
        let subject = subject(&session.git, &oid)?;
        todo.push_str(&format!("{action} {oid} {subject}\n"));
    }

    let directory = rebase_state_dir(data_directory, session);
    cleanup(&directory);
    fs::create_dir_all(directory.join("messages"))?;
    write_state(&directory.join("identity.json"), &serde_json::to_vec(&WorktreeIdentity::capture(&session.git)?)?)?;
    let todo_path = directory.join("todo.txt");
    let sequence_editor = directory.join("sequence-editor.sh");
    let message_editor = directory.join("message-editor.sh");
    write_script(&todo_path, &todo)?;
    for entry in &request.entries {
        if entry.action == RebaseAction::Reword {
            let oid = normalize_oid(&session.git, &entry.oid)?;
            write_state(&directory.join("messages").join(oid), format!("{}\n", entry.message.as_deref().unwrap_or_default()).as_bytes())?;
        }
    }
    // The sequence editor only copies our pre-validated plan.  No user text is
    // interpolated into an executable script.
    write_script(
        &sequence_editor,
        "#!/bin/sh\ncat \"$REVIA_REBASE_TODO\" > \"$1\"\n",
    )?;
    // Git invokes the editor for resolved pick/edit conflicts and squashes as
    // well as rewords. Read the current action and original OID, never consume
    // a positional queue. Message text stays in data files, outside the script.
    write_script(
        &message_editor,
        "#!/bin/sh\nset -eu\naction=$(awk 'NF && $1 !~ /^#/ { value=$1 } END { print value }' \"$REVIA_REBASE_DONE\")\ncase \"$action\" in reword|r) ;; *) exit 0 ;; esac\noid=$(awk 'NF && $1 !~ /^#/ { value=$2 } END { print value }' \"$REVIA_REBASE_DONE\")\ncase \"$oid\" in ''|*[!0-9a-fA-F]*) exit 1 ;; esac\ncat \"$REVIA_REBASE_STATE/messages/$oid\" > \"$1\"\n",
    )?;

    crate::platform::sync_directory(&directory.join("messages"))?;
    crate::platform::sync_directory(&directory)?;
    require_expected_head(&session.git, &request.expected_head)?;
    crate::write_guard::require_context(session, &request.expected_context)?;
    require_state(session, &preflight(session, &commits)?.overwrite_paths, Some(&request.expected_state))?;
    let saved = match autostash.save_before_confirmed(session, recovery_store, "rebase", &prepared) {
        Ok(saved) => saved.is_some(),
        Err(error) => { cleanup(&directory); return Err(error); }
    };
    if saved {
        if let Err(error) = prepared.after_cleanup(session, &touched) {
            cleanup(&directory);
            return Err(recovery::pending_write(error));
        }
    }
    let rebase_git = session
        .git
        .with_env_vars(&[
            (
                "GIT_SEQUENCE_EDITOR",
                crate::platform::git_shell_argument(&sequence_editor).as_str(),
            ),
            ("GIT_EDITOR", crate::platform::git_shell_argument(&message_editor).as_str()),
            ("REVIA_REBASE_TODO", todo_path.to_string_lossy().as_ref()),
            ("REVIA_REBASE_STATE", directory.to_string_lossy().as_ref()),
            ("REVIA_REBASE_DONE", operation::marker_path(&session.git, "rebase-merge/done")?.to_string_lossy().as_ref()),
        ]);
    // Environment construction above also reads Git metadata. Verify the same
    // original/cleanup-owned baseline after those reads, immediately at write.
    if let Err(error) = prepared.require(session) {
        cleanup(&directory);
        return Err(if saved { recovery::pending_write(error) } else { error });
    }
    let output = rebase_git.run_write(&["rebase", "--interactive", "--no-autostash", &upstream]);
    let output = output.map_err(recovery::pending_write)?;
    if let Some(record) = &record { recovery_store.finish_snapshot(record)?; }
    let active =
        operation::detect(&session.git, operation::conflict_count(&session.git).map_err(recovery::pending_write)?).map_err(recovery::pending_write)?.is_some();
    if !active {
        cleanup(&directory);
    } else {
        (|| {
            let id = crate::platform::directory_identity(&operation::marker_path(&session.git, "rebase-merge")?)?;
            write_state(&directory.join("rebase-directory.json"), &serde_json::to_vec(&id)?)?;
            crate::platform::sync_directory(&directory)?;
            Ok::<(), AppError>(())
        })().map_err(recovery::pending_write)?;
    }
    finish(
        session,
        recovery_store,
        autostash,
        archive_store,
        "interactiveRebase",
        output.exit_code,
        &output.stderr,
        true,
        Vec::new(),
    )
}

pub fn action_label(action: RebaseAction) -> &'static str {
    match action {
        RebaseAction::Pick => "pick",
        RebaseAction::Reword => "reword",
        RebaseAction::Edit => "edit",
        RebaseAction::Squash => "squash",
        RebaseAction::Fixup => "fixup",
        RebaseAction::Drop => "drop",
    }
}
