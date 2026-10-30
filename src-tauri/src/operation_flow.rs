//! Continue / abort / skip for in-progress Git operations (M6 O01).
//!
//! The operation and its metadata version are re-detected before every action.
//! A stale confirmation is rejected instead of acting on a different conflict
//! set. Every action captures paths it may overwrite in the recovery store;
//! Git itself remains the only executor.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::autostash::{AutostashRestoreOutcome, AutostashStore};
use crate::error::{AppError, ErrorKind, Result};
use crate::operation::{self, OperationKind, OperationState};
use crate::recovery::{self, RecoverySource, RecoveryStore};
use crate::session::WorktreeSession;
use crate::stash_archive::StashArchiveStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OperationAction {
    Continue,
    Abort,
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationActionRequest {
    pub kind: OperationKind,
    /// The `metadata_version` from the state the user saw.
    pub metadata_version: String,
    pub action: OperationAction,
    pub expected_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OperationOutcome {
    pub kind: OperationKind,
    pub action: OperationAction,
    /// No operation remains after the command.
    pub completed: bool,
    /// New state when Git stopped at the next step.
    pub state: Option<OperationState>,
    pub recovery_id: Option<String>,
    /// Present when an automatic stash was saved before this operation.
    pub autostash: Option<AutostashRestoreOutcome>,
    #[serde(default)]
    #[ts(optional)]
    pub continuation: Option<crate::rewrite::RewriteOutcome>,
}

pub fn state(session: &WorktreeSession, data: &std::path::Path) -> Result<Option<OperationState>> {
    let native = operation::detect(&session.git, operation::conflict_count(&session.git)?)?;
    let mut state = crate::history_sequence::state(data, session, native)?;
    if let Some(state) = &mut state {
        state.confirmation_state = crate::write_guard::bind_confirmation(session, &state.confirmation_state);
    }
    Ok(state)
}

pub fn execute(
    session: &WorktreeSession,
    recovery_store: &RecoveryStore,
    autostash_store: &AutostashStore,
    archive_store: &StashArchiveStore,
    request: &OperationActionRequest,
) -> Result<OperationOutcome> {
    let _write = session.acquire_write_lock()?;
    let state = state(session, autostash_store.data_directory())?
        .ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.operationFlow.noOperationInProgress",
                "当前没有进行中的操作",
                &[],
            )
        })?;
    if state.kind != request.kind {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.operationFlow.operationChanged",
            "进行中的操作已变化，请刷新后重试",
            &[],
        ));
    }
    if state.metadata_version != request.metadata_version || state.confirmation_state != request.expected_state {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.operationFlow.operationStateChanged",
            "操作状态已变化，请刷新后重试",
            &[],
        ));
    }
    if state.continuation_only {
        let mut continuation = None;
        match request.action {
            OperationAction::Abort => crate::history_sequence::end_pending(autostash_store.data_directory(), session)?,
            OperationAction::Continue if !state.needs_verification => {
                continuation = Some(crate::history_sequence::resume_pending(session, recovery_store, autostash_store, archive_store)?);
            }
            _ => return Err(AppError::coded(ErrorKind::NeedsVerification, "errors.rewrite.sequencePending", "上次历史操作的计划尚未结束，请先核对并处理该计划", &[])),
        }
        let after = self::state(session, autostash_store.data_directory()).map_err(recovery::pending_write)?;
        return Ok(OperationOutcome { kind: state.kind, action: request.action, completed: after.is_none(), state: after,
            recovery_id: None, autostash: continuation.as_ref().and_then(|result| result.autostash.clone()), continuation });
    }
    if request.action == OperationAction::Continue && state.has_conflicts() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.operationFlow.unresolvedConflicts",
            "仍有未解决的冲突，不能继续",
            &[("count", state.conflict_count.to_string())],
        ));
    }
    if request.action == OperationAction::Skip && state.kind == OperationKind::Merge {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.operationFlow.mergeCannotSkip",
            "合并操作不能跳过",
            &[],
        ));
    }

    let recovery_record = {
        let source = match request.action {
            OperationAction::Continue => RecoverySource::OperationContinue,
            OperationAction::Abort => RecoverySource::OperationAbort,
            OperationAction::Skip => RecoverySource::OperationSkip,
        };
        let targets = crate::write_guard::operation_targets(&session.git, state.kind)?;
        let mut paths = crate::write_guard::paths(&session.git, &targets.iter().map(String::as_str).collect::<Vec<_>>())?;
        if let Some(plan) = crate::history_sequence::read(autostash_store.data_directory(), session)? {
            paths.extend(crate::history_sequence::paths(session, &plan)?); paths.sort(); paths.dedup();
        }
        crate::write_guard::protect(session, recovery_store, source,
            &format!("{} {}", state.kind.label(), action_label(request.action)), &paths)?
    };
    let recovery_id = recovery_record.as_ref().map(|record| record.id.clone());

    let args: Vec<&str> = match (state.kind, request.action) {
        (_, OperationAction::Continue) => match state.kind {
            OperationKind::Merge => vec!["merge", "--continue"],
            OperationKind::Rebase => vec!["rebase", "--continue"],
            OperationKind::CherryPick => vec!["cherry-pick", "--continue"],
            OperationKind::Revert => vec!["revert", "--continue"],
        },
        (_, OperationAction::Abort) => match state.kind {
            OperationKind::Merge => vec!["merge", "--abort"],
            OperationKind::Rebase => vec!["rebase", "--abort"],
            OperationKind::CherryPick => vec!["cherry-pick", "--abort"],
            OperationKind::Revert => vec!["revert", "--abort"],
        },
        (OperationKind::Rebase, OperationAction::Skip) => vec!["rebase", "--skip"],
        (OperationKind::CherryPick, OperationAction::Skip) => vec!["cherry-pick", "--skip"],
        (OperationKind::Revert, OperationAction::Skip) => vec!["revert", "--skip"],
        (OperationKind::Merge, OperationAction::Skip) => unreachable!(),
    };
    // Continue must never block on an editor.  An interactive rebase that
    // Revia started keeps its persisted plan and OID-bound messages: continuing must
    // select the current reword by its Git action/OID instead of consuming a queue.
    let data_directory = autostash_store.data_directory();
    let mut env = if state.kind == OperationKind::Rebase {
        crate::rewrite::rebase_editor_env(data_directory, session)?.unwrap_or_default()
    } else {
        Vec::new()
    };
    if env.is_empty() {
        env.push(("GIT_EDITOR".to_string(), "true".to_string()));
        env.push(("GIT_SEQUENCE_EDITOR".to_string(), "true".to_string()));
    }
    let env_refs: Vec<(&str, &str)> = env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let git = session.git.with_env_vars(&env_refs);
    let checked = self::state(session, autostash_store.data_directory())?
        .ok_or_else(crate::write_guard::stale)?;
    if checked.metadata_version != request.metadata_version || checked.confirmation_state != request.expected_state {
        return Err(crate::write_guard::stale());
    }
    let plan = crate::history_sequence::before_action(data_directory, session, &state)?;
    let mut output = git.run_write(&args).map_err(recovery::pending_write)?;
    let empty_skipped = if request.action == OperationAction::Continue {
        match &plan { Some(plan) => crate::history_sequence::finish_empty_continue(&git, plan, &mut output).map_err(recovery::pending_write)?, None => false }
    } else { false };
    let after_conflicts = operation::conflict_count(&session.git).map_err(recovery::pending_write)?;
    let after = operation::detect(&session.git, after_conflicts).map_err(recovery::pending_write)?;
    let completed = after.is_none();
    let continuation = if let Some(plan) = plan {
        crate::history_sequence::after_action(session, recovery_store, autostash_store, archive_store,
            plan, request.action, output.exit_code == 0, after.as_ref(), empty_skipped).map_err(recovery::pending_write)?
    } else { None };
    if let Some(result) = continuation {
        let result = crate::rewrite::finish_sequence_snapshot(recovery_store, recovery_record.as_ref(), result);
        return Ok(OperationOutcome { kind: state.kind, action: request.action, completed: result.completed,
            state: result.state.clone(), recovery_id, autostash: result.autostash.clone(), continuation: Some(result) });
    }
    if completed {
        crate::rewrite::clear_rebase_state(data_directory, session);
    }

    let mut autostash = None;
    if completed {
        autostash = autostash_store.restore_if_ready(session, recovery_store, archive_store).map_err(recovery::pending_write)?;
    }

    // Include the remaining commits, whole-selection abort and automatic
    // restoration in the final recovery baseline, not just the first Git call.
    if let Some(record) = &recovery_record { recovery_store.finish_snapshot(record)?; }
    // Stopping at the next conflict of the same operation is a normal result:
    // Git exits non-zero, but the state below drives the banner. A failure
    // that left the operation where it was, or any other non-zero exit, still
    // needs verification.
    let next_conflict = after.as_ref().is_some_and(|next| next.kind == state.kind && next.conflict_count > 0
        && next.metadata_version != state.metadata_version);
    if output.exit_code != 0 && !next_conflict {
        return Err(AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.operationFlow.gitActionNeedsVerification",
            "{action} 未成功完成，请检查当前操作状态：{detail}",
            &[
                ("action", action_label(request.action).to_string()),
                ("detail", output.stderr.trim().to_string()),
            ],
        ));
    }

    Ok(OperationOutcome {
        kind: state.kind,
        action: request.action,
        completed,
        state: after,
        recovery_id,
        autostash,
        continuation: None,
    })
}

pub fn action_label(action: OperationAction) -> &'static str {
    match action {
        OperationAction::Continue => "continue",
        OperationAction::Abort => "abort",
        OperationAction::Skip => "skip",
    }
}
