//! Durable selection for a multi-commit cherry-pick/revert. Native Git owns
//! each conflict stop; Revia owns the remaining, originally confirmed OIDs.
//! Running is persisted before mutation and is never replayed after a crash.
use std::{fs, io::Write, path::{Path, PathBuf}, sync::atomic::{AtomicU64, Ordering}};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use crate::{autostash::AutostashStore, error::{AppError, ErrorKind, Result},
    operation::{self, OperationKind, OperationState}, operation_flow::OperationAction,
    recovery::{self, RecoverySource, RecoveryStore, WorktreeIdentity}, rewrite::{self, RewriteOutcome},
    session::WorktreeSession, stash_archive::StashArchiveStore, write_guard};

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Phase { Ready, Running, Stopped { metadata: String } }

use crate::write_guard::{ConfirmedFiles, PreparedState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Plan {
    identity: WorktreeIdentity,
    branch: String,
    head: String,
    original_head: String,
    pub kind: OperationKind,
    remaining: Vec<String>,
    applied: Vec<String>,
    skipped_empty: Vec<String>,
    skipped: Vec<String>,
    confirmed_context: String,
    source_oids: Vec<String>,
    keep_empty: bool,
    restore_autostash: bool,
    phase: Phase,
    #[serde(default)]
    confirmed: Option<ConfirmedFiles>,
}

fn pending() -> AppError { AppError::coded(ErrorKind::NeedsVerification,
    "errors.rewrite.sequencePending", "上次历史操作的计划尚未结束，请先核对并处理该计划", &[]) }

fn head(session: &WorktreeSession) -> Result<String> {
    Ok(session.git.run_ok(&["rev-parse", "--verify", "HEAD"], "读取序列 HEAD")?.stdout_lossy().trim().to_string())
}

fn file(data: &Path, session: &WorktreeSession) -> PathBuf {
    let digest = Sha256::digest(session.path.as_os_str().as_encoded_bytes());
    let key: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    data.join("history-sequences").join(key).join("plan.json")
}

pub(crate) fn read(data: &Path, session: &WorktreeSession) -> Result<Option<Plan>> {
    match fs::read(file(data, session)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|error| recovery::pending_write(AppError::from(error))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(recovery::pending_write(error)),
    }
}

fn save(data: &Path, session: &WorktreeSession, plan: &Plan) -> Result<()> {
    let path = file(data, session); let directory = path.parent().unwrap();
    recovery::ensure_outside_git(directory, &session.git, &plan.identity)?;
    fs::create_dir_all(directory)?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let pending = directory.join(format!("pending-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let mut output = fs::OpenOptions::new().write(true).create_new(true).open(&pending)?;
    output.write_all(&serde_json::to_vec(plan)?)?; output.sync_all()?; drop(output);
    fs::rename(pending, &path)?;
    crate::platform::sync_directory(directory)?;
    Ok(())
}

fn clear(data: &Path, session: &WorktreeSession) -> Result<()> {
    let path = file(data, session);
    fs::remove_file(&path).map_err(recovery::pending_write)?;
    crate::platform::sync_directory(path.parent().unwrap()).map_err(recovery::pending_write)
}

pub(crate) fn ensure_empty(data: &Path, session: &WorktreeSession) -> Result<()> {
    if read(data, session)?.is_some() { return Err(pending()); } Ok(())
}

pub(crate) fn create(session: &WorktreeSession, kind: OperationKind, oids: Vec<String>,
    source_oids: Vec<String>, keep_empty: bool, restore_autostash: bool,
    expected_head: &str, expected_context: &str, prepared: PreparedState) -> Result<Plan> {
    write_guard::require_context(session, expected_context)?;
    if head(session)? != expected_head { return Err(pending()); }
    let plan = Plan { identity: session.require_identity()?, branch: write_guard::branch_context(&session.git)?,
        head: expected_head.to_string(), original_head: expected_head.to_string(), kind, remaining: oids,
        applied: Vec::new(), skipped_empty: Vec::new(), skipped: Vec::new(), confirmed_context: expected_context.to_string(),
        source_oids, keep_empty, restore_autostash, phase: Phase::Ready,
        confirmed: Some(prepared.into_confirmed(session)?) };
    Ok(plan)
}

fn capture_confirmed(session: &WorktreeSession, paths: &[String]) -> Result<ConfirmedFiles> {
    ConfirmedFiles::capture(session, paths)
}

fn advance_confirmed(session: &WorktreeSession, plan: &mut Plan, touched: &[String]) -> Result<bool> {
    let Some(before) = &plan.confirmed else { return Ok(false); };
    let Some(after) = before.advance(session, touched)? else { return Ok(false); };
    plan.confirmed = Some(after); Ok(true)
}

fn confirmed_matches(session: &WorktreeSession, plan: &Plan) -> Result<bool> {
    match &plan.confirmed { Some(before) => before.matches(session), None => Ok(false) }
}

fn stop_changed(session: &WorktreeSession, auto: &AutostashStore, mut plan: Plan) -> Result<RewriteOutcome> {
    plan.phase = Phase::Ready;
    save(auto.data_directory(), session, &plan).map_err(recovery::pending_write)?;
    Ok(RewriteOutcome { action: plan.kind.label().to_string(), completed: false,
        state: pending_state(auto.data_directory(), session)?, created_oid: Some(plan.head), autostash: None,
        skipped_empty: plan.skipped_empty, skipped: plan.skipped, applied: plan.applied,
        failed_oid: None, pending: plan.remaining,
        error: Some(AppError::coded(ErrorKind::NeedsVerification, "errors.rewrite.sequenceFilesChanged",
            "批次期间其他文件或暂存状态已变化，已停止剩余提交；请核对后重新确认继续", &[]).payload()) })
}

fn validate(session: &WorktreeSession, plan: &Plan) -> Result<()> {
    if !plan.identity.confirmed_match(&session.require_identity()?)
        || plan.branch != write_guard::branch_context(&session.git)? || plan.head != head(session)? {
        return Err(pending());
    }
    Ok(())
}

pub(crate) fn paths(session: &WorktreeSession, plan: &Plan) -> Result<Vec<String>> {
    // Whole-selection abort also undoes already-applied commits. Include the
    // original tree even after those OIDs left the remaining queue.
    let mut targets = vec![plan.original_head.clone()];
    for oid in &plan.remaining {
        targets.push(oid.clone());
        let parent = session.git.run(&["rev-parse", "--verify", "--quiet", &format!("{oid}^")])?;
        if parent.exit_code == 0 { targets.push(parent.stdout_lossy().trim().to_string()); }
    }
    write_guard::paths(&session.git, &targets.iter().map(String::as_str).collect::<Vec<_>>())
}

/// The UI can explicitly continue a receipt-confirmed between-step stop, or
/// end an interrupted plan while keeping every Git/file result for inspection.
pub(crate) fn pending_state(data: &Path, session: &WorktreeSession) -> Result<Option<OperationState>> {
    let Some(plan) = read(data, session)? else { return Ok(None); };
    let version = Sha256::digest(serde_json::to_vec(&plan)?).iter().map(|b| format!("{b:02x}")).collect();
    let uncertain = !matches!(plan.phase, Phase::Ready) || validate(session, &plan).is_err();
    Ok(Some(OperationState { kind: plan.kind, sequencer_pending: true, edit_stop: false, conflict_count: 0,
        metadata_version: version, confirmation_state: write_guard::fingerprint(&session.git, &paths(session, &plan)?)?,
        continuation_only: true, needs_verification: uncertain }))
}

pub(crate) fn state(data: &Path, session: &WorktreeSession, native: Option<OperationState>) -> Result<Option<OperationState>> {
    let Some(mut native) = native else { return pending_state(data, session); };
    let Some(plan) = read(data, session)? else { return Ok(Some(native)); };
    let matches = matches!(&plan.phase, Phase::Stopped { metadata } if *metadata == native.metadata_version)
        && plan.kind == native.kind && validate(session, &plan).is_ok();
    let mut protected = paths(session, &plan)?;
    let targets = write_guard::operation_targets(&session.git, native.kind)?;
    protected.extend(write_guard::paths(&session.git, &targets.iter().map(String::as_str).collect::<Vec<_>>())?);
    protected.sort(); protected.dedup();
    native.confirmation_state = write_guard::fingerprint(&session.git, &protected)?;
    if !matches {
        // Keep the actual Git stop untouched while the user inspects/ends the
        // uncertain Revia association. Native controls return after that.
        native.continuation_only = true; native.needs_verification = true;
        native.metadata_version = Sha256::digest(serde_json::to_vec(&plan)?).iter().map(|b| format!("{b:02x}")).collect();
    }
    Ok(Some(native))
}

/// Called after the ordinary confirmation/recovery checks and before native
/// --continue/--skip/--abort. A failed write leaves Running on disk.
pub(crate) fn before_action(data: &Path, session: &WorktreeSession, state: &OperationState) -> Result<Option<Plan>> {
    let Some(mut plan) = read(data, session)? else { return Ok(None); };
    validate(session, &plan)?;
    match &plan.phase {
        Phase::Stopped { metadata } if !state.continuation_only && *metadata == state.metadata_version && plan.kind == state.kind => {},
        _ => return Err(pending()),
    }
    plan.phase = Phase::Running;
    // The explicit action has a fresh file confirmation, including the user's
    // conflict resolution. It is the sole authority to accept a new baseline.
    plan.confirmed = Some(capture_confirmed(session, &paths(session, &plan)?)?);
    save(data, session, &plan).map_err(recovery::pending_write)?;
    Ok(Some(plan))
}

pub(crate) fn end_pending(data: &Path, session: &WorktreeSession) -> Result<()> {
    let plan = read(data, session)?.ok_or_else(pending)?;
    // Ending the association does not authorize a new Git action or restore.
    if !plan.identity.confirmed_match(&session.require_identity()?) { return Err(pending()); }
    clear(data, session)
}

pub(crate) fn resume_pending(session: &WorktreeSession, recovery: &RecoveryStore, auto: &AutostashStore,
    archives: &StashArchiveStore) -> Result<RewriteOutcome> {
    let mut plan = read(auto.data_directory(), session)?.ok_or_else(pending)?;
    if !matches!(plan.phase, Phase::Ready) { return Err(pending()); }
    plan.confirmed = Some(capture_confirmed(session, &paths(session, &plan)?)?);
    run(session, recovery, auto, archives, plan)
}

pub(crate) fn after_action(session: &WorktreeSession, recovery: &RecoveryStore, auto: &AutostashStore,
    archives: &StashArchiveStore, mut plan: Plan, action: OperationAction, success: bool,
    state: Option<&OperationState>, empty_skipped: bool) -> Result<Option<RewriteOutcome>> {
    let data = auto.data_directory();
    if let Some(state) = state {
        plan.head = head(session)?; plan.phase = Phase::Stopped { metadata: state.metadata_version.clone() };
        save(data, session, &plan).map_err(recovery::pending_write)?;
        return Ok(None);
    }
    if !success { return Err(pending()); }
    if action == OperationAction::Abort {
        // Match a native multi-commit sequencer: abort returns to the head
        // before the whole selection, preserving unrelated local changes.
        if plan.branch != write_guard::branch_context(&session.git)? || head(session)? != plan.head { return Err(pending()); }
        if plan.head != plan.original_head {
            // Native abort can run hooks. Protect its actual file/index result
            // again before the separate reset, then reject any intervening edit.
            let paths = paths(session, &plan)?;
            let expected = write_guard::fingerprint(&session.git, &paths)?;
            let record = write_guard::protect(session, recovery, RecoverySource::OperationAbort,
                "abort history selection", &paths)?;
            write_guard::require(&session.git, &self::paths(session, &plan)?, &expected)?;
            validate(session, &plan)?;
            write_guard::require_context(session, &plan.confirmed_context)?;
            session.git.run_write(&["reset", "--merge", &plan.original_head])?
                .into_success("中止整个历史序列").map_err(recovery::pending_write)?;
            if let Some(record) = &record { recovery.finish_snapshot(record)?; }
        }
        clear(data, session)?; return Ok(None);
    }
    let current = plan.remaining.remove(0);
    let touched = rewrite::changed_paths(&session.git, &current)?;
    if empty_skipped { plan.skipped_empty.push(current); }
    else if action == OperationAction::Skip { plan.skipped.push(current); } else { plan.applied.push(current); }
    plan.head = head(session)?; plan.phase = Phase::Ready;
    if !advance_confirmed(session, &mut plan, &touched)? { return stop_changed(session, auto, plan).map(Some); }
    save(data, session, &plan).map_err(recovery::pending_write)?;
    run(session, recovery, auto, archives, plan).map(Some)
}

/// Only called after an explicit Continue on this persisted selection. Git's
/// resolved index determines emptiness; the originally confirmed policy owns it.
pub(crate) fn finish_empty_continue(git: &crate::git::Git, plan: &Plan,
    output: &mut crate::git::GitOutput) -> Result<bool> {
    if output.exit_code == 0 || plan.kind != OperationKind::CherryPick
        || !operation::detect(git, operation::conflict_count(git)?)?.is_some_and(|state| state.kind == plan.kind)
        || !rewrite::empty_cherry_pick(git)? { return Ok(false); }
    let args = if plan.keep_empty { vec!["commit", "--allow-empty", "--no-edit"] }
        else { vec!["cherry-pick", "--skip"] };
    *output = git.run_write(&args)?;
    Ok(!plan.keep_empty && output.exit_code == 0)
}

pub(crate) fn run(session: &WorktreeSession, recovery: &RecoveryStore, auto: &AutostashStore,
    archives: &StashArchiveStore, mut plan: Plan) -> Result<RewriteOutcome> {
    match run_confirmed_steps(session, recovery, auto, archives, &mut plan) {
        Err(error) if !matches!(plan.phase, Phase::Ready) || !plan.applied.is_empty()
            || !plan.skipped_empty.is_empty() || !plan.skipped.is_empty() => {
            // Git may have changed earlier steps, or the current write may
            // have started. Keep the last known receipt even when no native
            // stop exists, a process fails, or final persistence/readback fails.
            // The durable Running phase remains uncertain; never replay it.
            let mut error = recovery::pending_write(error);
            let state = match crate::operation_flow::state(session, auto.data_directory()) {
                Ok(state) => state,
                Err(failure) => { error = error.related(&failure); None }
            };
            let stopped = !matches!(plan.phase, Phase::Ready);
            Ok(RewriteOutcome {
                action: plan.kind.label().to_string(), completed: false, state,
                created_oid: None, autostash: None,
                skipped_empty: plan.skipped_empty, skipped: plan.skipped, applied: plan.applied,
                failed_oid: if stopped { plan.remaining.first().cloned() } else { None },
                pending: plan.remaining.into_iter().skip(usize::from(stopped)).collect(),
                error: Some(error.payload()),
            })
        }
        result => result,
    }
}

fn run_confirmed_steps(session: &WorktreeSession, recovery: &RecoveryStore, auto: &AutostashStore,
    archives: &StashArchiveStore, plan: &mut Plan) -> Result<RewriteOutcome> {
    let data = auto.data_directory();
    validate(session, &plan)?;
    while let Some(oid) = plan.remaining.first().cloned() {
        validate(session, &plan)?;
        if !confirmed_matches(session, &plan)? { return stop_changed(session, auto, plan.clone()); }
        let paths = paths(session, &plan)?;
        let expected = write_guard::fingerprint(&session.git, &paths)?;
        let record = write_guard::protect(session, recovery, RecoverySource::HistoryOverwrite, plan.kind.label(), &paths)?;
        write_guard::require(&session.git, &self::paths(session, &plan)?, &expected)?;
        plan.phase = Phase::Running;
        save(data, session, &plan).map_err(recovery::pending_write)?;
        validate(session, &plan)?;
        write_guard::require_context(session, &plan.confirmed_context)?;
        if !confirmed_matches(session, &plan)? { return stop_changed(session, auto, plan.clone()); }
        let mut args = vec![plan.kind.label(), "--no-edit"];
        if plan.kind == OperationKind::CherryPick && plan.source_oids.contains(&oid) { args.push("-x"); }
        args.push(&oid);
        let mut output = session.git.with_env_vars(&[("GIT_EDITOR", "true")]).run_write(&args)?;
        let mut skipped = false;
        if output.exit_code != 0 && plan.kind == OperationKind::CherryPick
            && operation::detect(&session.git, operation::conflict_count(&session.git)?)?.is_some_and(|state| state.kind == plan.kind)
            && rewrite::empty_cherry_pick(&session.git)? {
            let args = if plan.keep_empty { vec!["commit", "--allow-empty", "--no-edit"] } else { vec!["cherry-pick", "--skip"] };
            output = session.git.with_env_vars(&[("GIT_EDITOR", "true")]).run_write(&args)?;
            skipped = !plan.keep_empty && output.exit_code == 0;
        }
        if let Some(record) = &record { recovery.finish_snapshot(record)?; }
        if output.exit_code != 0 {
            let state = operation::detect(&session.git, operation::conflict_count(&session.git)?)?;
            if let Some(state) = &state {
                plan.head = head(session)?; plan.phase = Phase::Stopped { metadata: state.metadata_version.clone() };
                save(data, session, &plan).map_err(recovery::pending_write)?;
            }
            let mut outcome = rewrite::finish(session, recovery, auto, archives, plan.kind.label(),
                output.exit_code, &output.stderr, false, plan.skipped_empty.clone())?;
            outcome.skipped = plan.skipped.clone();
            outcome.applied = plan.applied.clone(); outcome.failed_oid = Some(oid); outcome.pending = plan.remaining[1..].to_vec();
            return Ok(outcome);
        }
        let touched = rewrite::changed_paths(&session.git, &oid)?;
        if skipped { plan.skipped_empty.push(oid); } else { plan.applied.push(oid); }
        plan.remaining.remove(0); plan.head = head(session)?; plan.phase = Phase::Ready;
        if !advance_confirmed(session, plan, &touched)? { return stop_changed(session, auto, plan.clone()); }
        save(data, session, &plan).map_err(recovery::pending_write)?;
    }
    // The last step has no next-iteration check; a hook may have switched
    // branches. Keep the plan and its autostash for verification.
    validate(session, &plan)?;
    clear(data, session)?;
    let mut outcome = rewrite::finish(session, recovery, auto, archives, plan.kind.label(), 0, "",
        plan.restore_autostash, plan.skipped_empty.clone())?;
    outcome.skipped = plan.skipped.clone();
    outcome.created_oid = Some(plan.head.clone()); outcome.applied = plan.applied.clone();
    Ok(outcome)
}
