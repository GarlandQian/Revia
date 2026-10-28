//! 分支与远端的写操作（M5 任务 03、规格 B01–B06）。
//!
//! 全部写操作走同一条边界：
//! - 同 worktree 一次只执行一个（`WorktreeSession::acquire_write_lock`），不排队；
//! - 会覆盖或丢弃工作区内容的步骤**先**把内容存入回收区，并注明是哪种操作之前保存的；
//! - 网络操作（fetch / 更新 / 推送 / 检查远端）单独使用 10 分钟上限，不等待任何输入
//!   （`GIT_TERMINAL_PROMPT=0`、空标准输入、`SSH_ASKPASS_REQUIRE=never`）；
//! - 不修改用户的 Git 配置：更新方式、标签方式都用命令参数传递；只有 Git 自己在
//!   「推送新分支」「检出远端分支」时记录上游，属于这些操作本身的结果；
//! - 结果无法确定（超时、中断、输出与状态不一致）一律按「结果待核对」返回，不自动重试。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, Result};
use crate::git::branch;
use crate::git::history::{self as gh, is_full_oid};
use crate::git::status::{ChangesKind, StatusEntry};
use crate::git::{Git, NETWORK_TIMEOUT};
use crate::operation::{OperationKind, OperationState};
use crate::recovery::{
    self, capture_files, DiscardOutcome, RecoverySource, RecoveryStore, WorktreeIdentity,
};
use crate::session::WorktreeSession;
pub use crate::settings::UpdateMode;
use crate::settings::{matches_protected, FetchTagMode};

/// merge 的 `--autostash` 需要 Git 2.27（更早的版本只支持 rebase 的 autostash）。
pub const MERGE_AUTOSTASH_MIN: (u32, u32) = (2, 27);

// ---------- 检出与新建分支（B02） ----------

/// 检出目标：本地分支或远端分支。
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum CheckoutTarget {
    /// 本地分支名。
    Local { name: String },
    /// 远端分支：`remote` 是远端名，`branch` 是不带远端名的分支名。
    Remote { remote: String, branch: String },
}

impl CheckoutTarget {
    fn label(&self) -> String {
        match self {
            Self::Local { name } => name.clone(),
            Self::Remote { remote, branch } => format!("{remote}/{branch}"),
        }
    }
}

/// 检出策略（Q150）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CheckoutStrategy {
    /// 直接检出；会被覆盖时返回需要确认。
    Plain,
    /// 智能检出：先存回收区，再 stash、检出、恢复 stash。
    Smart,
    /// 强制检出：先存回收区，再丢弃全部已跟踪文件的本地改动。
    Force,
}

/// 检出的结果：完成，或需要用户确认（列出会被覆盖的文件）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum CheckoutResult {
    Done(CheckoutOutcome),
    NeedsConfirmation {
        /// 会被覆盖或丢弃的本地文件（工作区路径）。
        files: Vec<String>,
        target: String,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckoutOutcome {
    /// 检出后的分支名。
    pub branch: String,
    pub strategy: CheckoutStrategy,
    /// 检出的提交。
    pub commit: String,
    /// 这次操作前保存的回收记录（没有覆盖任何内容时为 null）。
    pub recovery_record_id: Option<String>,
    /// 智能检出后 stash 是否成功恢复。
    pub stash_restored: bool,
    /// stash 条目是否被保留（恢复冲突时保留，通知用户）。
    pub stash_kept: bool,
    /// 恢复 stash 后的冲突文件。
    pub conflicts: Vec<String>,
    /// 给用户看的说明（例如「已新建并跟踪 origin/main」）。
    pub notes: Vec<String>,
}

/// 新建分支的起点（B02：三种入口）。
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum BranchStart {
    /// 当前 HEAD。
    Head,
    /// 一个本地引用（分支 / 标签 / 远端分支的完整引用名）。
    Ref {
        #[serde(rename = "fullName")]
        full_name: String,
    },
    /// 一个完整提交 ID。
    Commit { oid: String },
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateBranchRequest {
    pub name: String,
    pub start: BranchStart,
    /// 是否同时检出（对话框默认勾选）。
    pub checkout: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateBranchOutcome {
    pub name: String,
    pub commit: String,
    pub checked_out: bool,
}

// ---------- fetch（B03） ----------

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FetchOutcome {
    pub remotes: Vec<String>,
    /// 传给 Git 的标签参数（空字符串表示没有额外参数）。
    pub tag_args: Vec<String>,
    /// Git 的原始输出（失败时给用户看原文）。
    pub output: String,
}

/// 按标签方式组合 fetch 的固定参数（B03）。
pub fn fetch_tag_args(mode: FetchTagMode) -> Vec<String> {
    match mode {
        FetchTagMode::Auto => Vec::new(),
        // 同步：删掉远端已删除的标签和分支。
        FetchTagMode::Sync => vec!["--prune".into(), "--prune-tags".into()],
        FetchTagMode::Always => vec!["--tags".into()],
        FetchTagMode::Never => vec!["--no-tags".into()],
    }
}

fn fetch_remote_args(mode: FetchTagMode) -> Vec<String> {
    let mut args = vec!["--no-optional-locks".to_string(), "fetch".to_string()];
    args.extend(fetch_tag_args(mode));
    args
}

// ---------- 更新（B04） ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum UpdateResolvedMode {
    Merge,
    Rebase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum UpdateStatus {
    /// 已经是最新。
    UpToDate,
    /// 成功并入。
    Updated,
    /// merge 冲突：进入进行中操作，交给合并页签。
    MergeConflict,
    /// rebase 冲突：到终端继续或中止（按钮在 M6）。
    RebaseConflict,
    /// 恢复 autostash 时冲突：Git 保留的 stash 条目没有删除。
    AutostashConflict,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateOutcome {
    pub mode: UpdateResolvedMode,
    pub status: UpdateStatus,
    /// 上游引用全名（`refs/remotes/origin/main`）。
    pub upstream: String,
    /// 并入的提交数。
    pub commits_merged: u32,
    /// 冲突文件（唯一化）。
    pub conflicts: Vec<String>,
    /// 有未提交改动时先保存的回收记录。
    pub recovery_record_id: Option<String>,
    /// 保留下来的 stash 条目（autostash 恢复冲突时为 Some）。
    pub stash_kept: Option<String>,
    pub output: String,
}

/// 按分支默认方式解析更新方式（只读读取配置，不改配置）。
///
/// 顺序与 Git 自己一致：`branch.<name>.rebase` 优先，其次 `pull.rebase`。
pub fn resolve_branch_default(git: &Git, branch: &str) -> UpdateResolvedMode {
    let value = git
        .config_get(&format!("branch.{branch}.rebase"))
        .or_else(|| git.config_get("pull.rebase"));
    match value.as_deref().map(str::trim) {
        Some("true") | Some("yes") | Some("on") | Some("1") => UpdateResolvedMode::Rebase,
        Some("interactive") | Some("i") => UpdateResolvedMode::Rebase,
        // `false` / `merges` / 未设置都按 merge：`merges` 仍以 merge 为方向。
        _ => UpdateResolvedMode::Merge,
    }
}

// ---------- 推送（B05） ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PushForce {
    None,
    /// `--force-with-lease`，并带上推送前读取的远端跟踪分支值。
    WithLease,
}

/// 推送对话框要显示的预览（B05）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PushPreview {
    pub branch: String,
    pub has_upstream: bool,
    /// 目标远端（没有上游时默认 origin，只有单个远端时就是它）。
    pub remote: String,
    /// 目标远端分支名。
    pub target_branch: String,
    /// 可选的远端列表（没有上游且存在多个远端时让用户选）。
    pub remotes: Vec<String>,
    /// 将要推送的提交（新到旧），最多 `MAX_PUSH_PREVIEW` 条。
    pub commits: Vec<crate::history::HistoryCommit>,
    pub commit_count: u32,
    pub truncated: bool,
    /// 是否允许强制推送；不允许时给出原因。
    pub force_allowed: bool,
    pub force_reason: Option<String>,
    /// 当前分支是否是受保护分支。
    pub protected: bool,
}

/// 推送对话框最多列出的提交数（工程限额）。
pub const MAX_PUSH_PREVIEW: u32 = 200;

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PushRequest {
    /// 目标远端；空表示按上游 / origin 解析。
    pub remote: Option<String>,
    /// 目标远端分支名；空表示与本地分支同名。
    pub branch: Option<String>,
    /// 是否推送标签（当前分支相关的带注释标签）。
    pub tags: bool,
    pub force: PushForce,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum PushResult {
    Pushed {
        remote: String,
        branch: String,
        commits: u32,
        /// 推送成功后是否记录了上游（Git 的 `--set-upstream` 结果）。
        upstream_recorded: bool,
        output: String,
    },
    /// 被远端拒绝（远端有新提交）：提供「更新后再推送」。
    Rejected {
        remote: String,
        branch: String,
        reason: String,
        output: String,
    },
}

// ---------- 检查远端（B03） ----------

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoteCheck {
    pub upstream: String,
    /// 远端当前的上游提交（`ls-remote` 的结果）。
    pub remote_commit: Option<String>,
    /// 本地记录的跟踪分支提交。
    pub known_commit: Option<String>,
    pub has_new_commits: bool,
}

// ---------- 实现 ----------

fn run_network(git: &Git, args: &[&str], what: &str, timeout: Duration) -> Result<crate::git::GitOutput> {
    let output = git.run_network(args, timeout)?;
    if output.timed_out {
        // 超时不是普通失败：结果无法确定，按待核对处理（Q96），不自动重试。
        return Err(AppError::needs_verification(format!(
            "{what}超时（{} 秒），结果待核对：请核对引用与工作区真实状态后再继续",
            timeout.as_secs()
        )));
    }
    Ok(output)
}

/// 网络命令的统一出口：超时按「结果待核对」，非零退出按 Git 原始输出报错。
fn network_expect(
    git: &Git,
    args: &[&str],
    what: &str,
    timeout: Duration,
) -> Result<crate::git::GitOutput> {
    let output = run_network(git, args, what, timeout)?;
    output.into_success(what)
}

/// 允许非零退出（merge / rebase / push 的冲突与拒绝都不是异常）。
fn network_allow_failure(
    git: &Git,
    args: &[&str],
    what: &str,
    timeout: Duration,
) -> Result<crate::git::GitOutput> {
    run_network(git, args, what, timeout)
}

fn non_network(git: &Git, args: &[&str], what: &str) -> Result<crate::git::GitOutput> {
    let output = git.run(args)?;
    if output.timed_out {
        return Err(AppError::needs_verification(format!(
            "{what}超时，结果待核对：请核对引用与工作区真实状态后再继续"
        )));
    }
    output.into_success(what)
}

fn non_network_allow_failure(git: &Git, args: &[&str]) -> Result<crate::git::GitOutput> {
    let output = git.run(args)?;
    if output.timed_out {
        return Err(AppError::needs_verification(
            "Git 命令超时，结果待核对：请核对引用与工作区真实状态后再继续",
        ));
    }
    Ok(output)
}

/// 已修改的已跟踪文件与未跟踪文件（检出可能覆盖的那些）。
fn dirty_paths(git: &Git) -> Result<Vec<StatusEntry>> {
    let status = crate::git::status::read(git)?;
    Ok(status.entries)
}

fn tracked_dirty(entries: &[StatusEntry]) -> Vec<String> {
    let mut paths: Vec<String> = entries
        .iter()
        .filter(|entry| entry.changes != ChangesKind::Untracked)
        .flat_map(|entry| {
            let mut both = vec![entry.path.clone()];
            if let Some(original) = &entry.original_path {
                both.push(original.clone());
            }
            both
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

/// 工作区里可能被检出覆盖的路径：已暂存、未暂存与未跟踪都算。
///
/// 只看未暂存会漏掉「已暂存的改动与目标分支冲突」的情况：那种情况下 Git 也会
/// 拒绝检出，用户应当看到同样的智能 / 强制检出选择。
fn unreliable_scope(entries: &[StatusEntry]) -> Vec<String> {
    let mut paths: Vec<String> = entries.iter().map(|entry| entry.path.clone()).collect();
    paths.sort();
    paths.dedup();
    paths
}

/// 两个提交之间不同的路径（字面比较，不读工作区外的东西）。
fn changed_between(git: &Git, from: &str, to: &str) -> Result<Vec<String>> {
    let output = git.run_ok(
        &[
            "--no-optional-locks",
            "--literal-pathspecs",
            "-c",
            "core.quotePath=false",
            "diff",
            "--name-only",
            "-z",
            from,
            to,
            "--",
        ],
        "读取两个提交之间的文件列表",
    )?;
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect())
}

fn unmerged_paths(git: &Git) -> Result<Vec<String>> {
    let entries = dirty_paths(git)?;
    let mut paths: Vec<String> = entries
        .iter()
        .filter(|entry| entry.unmerged)
        .map(|entry| entry.path.clone())
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn operation_state(git: &Git) -> Option<OperationState> {
    crate::operation::detect(git, 0).ok().flatten()
}

/// 保存工作区内容到回收区（覆盖或丢弃之前必须先做）。
fn save_recovery(
    session: &WorktreeSession,
    store: &RecoveryStore,
    source: RecoverySource,
    target: &str,
    paths: &[String],
    identity: &WorktreeIdentity,
) -> Result<Option<(recovery::RecoveryRecord, recovery::RecordUse)>> {
    if paths.is_empty() {
        return Ok(None);
    }
    store.ensure_outside(identity)?;
    let before = capture_files(&session.path, paths)?;
    let context = recovery::context_fingerprint(&session.git, paths)?;
    let (record, usage) = store.save(identity.clone(), source, target, before, context)?;
    Ok(Some((record, usage)))
}

fn finish_recovery(
    session: &WorktreeSession,
    store: &RecoveryStore,
    record: &(recovery::RecoveryRecord, recovery::RecordUse),
    paths: &[String],
) {
    // 写入后状态只用于回收记录的「已执行」回执；读取失败不改变操作结果。
    let after = capture_files(&session.path, paths).unwrap_or_default();
    let _ = store.finish(&record.0, &DiscardOutcome::Completed { after });
}

fn abandon_recovery(store: &RecoveryStore, record: &(recovery::RecoveryRecord, recovery::RecordUse)) {
    let _ = store.finish(&record.0, &DiscardOutcome::NotApplied);
}

/// 解析检出目标的真实引用与提交。
struct CheckoutPlan {
    branch: String,
    /// 传给 `git checkout` 的起点（本地分支名 / `-b` 的起点）。
    checkout_args: Vec<String>,
    commit: String,
    notes: Vec<String>,
}

fn plan_checkout(git: &Git, target: &CheckoutTarget) -> Result<CheckoutPlan> {
    match target {
        CheckoutTarget::Local { name } => {
            if name.trim().is_empty() {
                return Err(AppError::rejected("分支名不能为空"));
            }
            let full = format!("refs/heads/{name}");
            let refs = gh::list_refs(git)?;
            let commit = refs
                .iter()
                .find(|(full_name, _, _)| full_name == &full)
                .map(|(_, _, commit)| commit.clone())
                .ok_or_else(|| AppError::rejected(format!("本地分支不存在：{name}")))?;
            Ok(CheckoutPlan {
                branch: name.clone(),
                checkout_args: vec![name.clone()],
                commit,
                notes: Vec::new(),
            })
        }
        CheckoutTarget::Remote { remote, branch: name } => {
            let remote_ref = format!("refs/remotes/{remote}/{name}");
            let refs = gh::list_refs(git)?;
            let commit = refs
                .iter()
                .find(|(full_name, _, _)| full_name == &remote_ref)
                .map(|(_, _, commit)| commit.clone())
                .ok_or_else(|| AppError::rejected(format!("远端分支不存在：{remote}/{name}")))?;
            let local_ref = format!("refs/heads/{name}");
            if let Some((_, _, _)) = refs.iter().find(|(full_name, _, _)| full_name == &local_ref) {
                // 已有同名本地分支：跟踪它就直接用它，不跟踪就要求改名（不覆盖）。
                let tracking = gh::upstream_of(git, name)?;
                if tracking.as_deref() == Some(remote_ref.as_str()) {
                    return Ok(CheckoutPlan {
                        branch: name.clone(),
                        checkout_args: vec![name.clone()],
                        commit,
                        notes: vec![format!("已检出跟踪 {remote}/{name} 的本地分支 {name}")],
                    });
                }
                return Err(AppError::rejected(format!(
                    "本地分支 {name} 已存在但不跟踪 {remote}/{name}；请为新分支改名后重试"
                )));
            }
            Ok(CheckoutPlan {
                branch: name.clone(),
                checkout_args: vec![
                    "-b".to_string(),
                    name.clone(),
                    "--track".to_string(),
                    format!("{remote}/{name}"),
                ],
                commit,
                notes: vec![format!("已从 {remote}/{name} 新建本地分支 {name} 并跟踪它")],
            })
        }
    }
}

/// 检出会被覆盖或丢弃的本地文件。
fn overwrite_targets(git: &Git, plan: &CheckoutPlan, entries: &[StatusEntry]) -> Result<Vec<String>> {
    let head = gh::head_commit(git)?;
    let differing = match head {
        Some(head) => changed_between(git, &head, &plan.commit)?,
        // 空仓库：所有目标文件都是新出现的。
        None => Vec::new(),
    };
    let local = unreliable_scope(entries);
    let mut paths: Vec<String> = local
        .into_iter()
        .filter(|path| differing.iter().any(|candidate| candidate == path))
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// 检出分支（B02）。
pub fn checkout(
    session: &WorktreeSession,
    store: &RecoveryStore,
    target: &CheckoutTarget,
    strategy: CheckoutStrategy,
) -> Result<CheckoutResult> {
    let _write = session.acquire_write_lock()?;
    let git = &session.git;
    if operation_state(git).is_some() {
        return Err(AppError::rejected("有进行中的操作，请先完成或中止后再检出"));
    }
    let plan = plan_checkout(git, target)?;
    let entries = dirty_paths(git)?;
    let files = overwrite_targets(git, &plan, &entries)?;
    if strategy == CheckoutStrategy::Plain && !files.is_empty() {
        return Ok(CheckoutResult::NeedsConfirmation {
            files,
            target: target.label(),
        });
    }
    let identity = WorktreeIdentity::capture(git)?;
    let label = target.label();
    let source = match strategy {
        CheckoutStrategy::Force => RecoverySource::CheckoutForce,
        _ => RecoverySource::CheckoutSmart,
    };
    let recovery_record_id;
    let mut stash_restored = true;
    let mut stash_kept = false;
    let mut conflicts = Vec::new();
    let mut notes = plan.notes.clone();

    if strategy == CheckoutStrategy::Smart {
        // 智能检出：先存回收区，再 stash 本地改动，检出后恢复 stash。
        let paths: Vec<String> = if files.is_empty() { unreliable_scope(&entries) } else { files.clone() };
        let record = save_recovery(session, store, source, &label, &paths, &identity)?;
        recovery_record_id = record.as_ref().map(|(record, _)| record.id.clone());
        let stash_args = ["stash", "push", "--include-untracked", "-m", "Revia 智能检出"];
        let stash_output = non_network(git, &stash_args, "暂存本地改动")?;
        let no_changes = stash_output.stdout_lossy().contains("No local changes");
        // `--` 明确「后面没有路径」：否则与分支同名的文件会被当成 pathspec。
        let checkout_args: Vec<&str> = std::iter::once("checkout")
            .chain(plan.checkout_args.iter().map(String::as_str))
            .chain(std::iter::once("--"))
            .collect();
        let checkout_output = non_network(git, &checkout_args, "检出分支");
        let checkout_output = match checkout_output {
            Ok(output) => output,
            Err(error) => {
                // 检出失败：尽量把 stash 恢复回工作区，保留回收记录并如实报错。
                if !no_changes {
                    let _ = git.run(&["stash", "pop"]);
                }
                if let Some(record) = &record {
                    abandon_recovery(store, record);
                }
                return Err(error);
            }
        };
        if !no_changes {
            let pop = non_network_allow_failure(git, &["stash", "pop"])?;
            if pop.exit_code != 0 {
                stash_kept = true;
                stash_restored = false;
                conflicts = unmerged_paths(git)?;
                notes.push("恢复本地改动时发生冲突：Git 保留了 stash 条目，请解决冲突后手动丢弃它".into());
            }
        }
        if let Some(record) = &record {
            finish_recovery(session, store, record, &paths);
        }
        let commit = head_commit_or(git, &plan.commit)?;
        let _ = checkout_output;
        return Ok(CheckoutResult::Done(CheckoutOutcome {
            branch: plan.branch,
            strategy,
            commit,
            recovery_record_id,
            stash_restored,
            stash_kept,
            conflicts,
            notes,
        }));
    }

    // 直接检出 / 强制检出：都会被 Git 的工作区覆盖检查兜底。
    let mut paths: Vec<String> = files.clone();
    if strategy == CheckoutStrategy::Force {
        // 强制检出会丢弃全部已跟踪文件的本地改动，因此全部先存回收区。
        paths = tracked_dirty(&entries);
        paths.extend(files.iter().cloned());
        paths.sort();
        paths.dedup();
    }
    let record = save_recovery(session, store, source, &label, &paths, &identity)?;
    recovery_record_id = record.as_ref().map(|(record, _)| record.id.clone());
    let mut args: Vec<&str> = vec!["checkout"];
    if strategy == CheckoutStrategy::Force {
        args.push("--force");
    }
    let plan_args: Vec<&str> = plan.checkout_args.iter().map(String::as_str).collect();
    args.extend(plan_args);
    // 同智能检出：`--` 避免与同名文件 / 目录混淆。
    args.push("--");
    let output = non_network(git, &args, "检出分支");
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            if let Some(record) = &record {
                abandon_recovery(store, record);
            }
            // Git 拒绝检出（工作区会被覆盖）时，把真实原因原样返回。
            return Err(error);
        }
    };
    if let Some(record) = &record {
        finish_recovery(session, store, record, &paths);
    }
    Ok(CheckoutResult::Done(CheckoutOutcome {
        branch: plan.branch,
        strategy,
        commit: head_commit_or(git, &plan.commit)?,
        recovery_record_id,
        stash_restored,
        stash_kept,
        conflicts,
        notes: { let _ = output; notes },
    }))
}

fn head_commit_or(git: &Git, fallback: &str) -> Result<String> {
    Ok(gh::head_commit(git)?.unwrap_or_else(|| fallback.to_string()))
}

/// 新建分支（B02）。
pub fn create_branch(
    session: &WorktreeSession,
    request: &CreateBranchRequest,
) -> Result<CreateBranchOutcome> {
    let _write = session.acquire_write_lock()?;
    let git = &session.git;
    let name = request.name.trim();
    branch::validate_branch_name(git, name)?;
    if branch::branch_exists(git, name)? {
        return Err(AppError::rejected(format!("分支已存在：{name}")));
    }
    let start = match &request.start {
        BranchStart::Head => gh::head_commit(git)?
            .ok_or_else(|| AppError::rejected("仓库还没有提交，无法新建分支"))?,
        BranchStart::Ref { full_name } => {
            let refs = gh::list_refs(git)?;
            refs.iter()
                .find(|(candidate, _, _)| candidate == full_name)
                .map(|(_, _, commit)| commit.clone())
                .ok_or_else(|| AppError::rejected(format!("起点引用不存在或不是提交：{full_name}")))?
        }
        BranchStart::Commit { oid } => gh::verify_commit_oid(git, oid)?,
    };
    if request.checkout {
        non_network(git, &["checkout", "-b", name, &start], "新建并检出分支")?;
    } else {
        non_network(git, &["branch", name, &start], "新建分支")?;
    }
    Ok(CreateBranchOutcome {
        name: name.to_string(),
        commit: start,
        checked_out: request.checkout,
    })
}

/// 对当前仓库的所有远端执行 fetch（B03）。
pub fn fetch(
    session: &WorktreeSession,
    tag_mode: FetchTagMode,
    timeout: Duration,
) -> Result<FetchOutcome> {
    let _write = session.acquire_write_lock()?;
    let git = &session.git;
    let remotes = read_remotes(git)?;
    if remotes.is_empty() {
        return Err(AppError::rejected("这个仓库没有配置任何远端"));
    }
    let mut outputs = Vec::new();
    for remote in &remotes {
        let mut args = fetch_remote_args(tag_mode);
        args.push(remote.clone());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = network_expect(git, &refs, &format!("从 {remote} fetch"), timeout)?;
        outputs.push(output.stdout_lossy());
        outputs.push(output.stderr.clone());
    }
    Ok(FetchOutcome {
        remotes,
        tag_args: fetch_tag_args(tag_mode),
        output: outputs.join("").trim().to_string(),
    })
}

/// 读取远端名列表。
pub fn read_remotes(git: &Git) -> Result<Vec<String>> {
    let output = git.run_ok(&["remote"], "读取远端列表")?;
    Ok(output
        .stdout_lossy()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

/// 分支的上游：`(完整引用名, 远端名, 远端分支名)`。
fn upstream_details(git: &Git, branch: &str) -> Result<Option<(String, String, String)>> {
    let Some(full) = gh::upstream_of(git, branch)? else {
        return Ok(None);
    };
    let short = full
        .strip_prefix("refs/remotes/")
        .ok_or_else(|| AppError::rejected(format!("上游不是远端分支：{full}")))?
        .to_string();
    let (remote, name) = short
        .split_once('/')
        .ok_or_else(|| AppError::rejected(format!("无法解析上游分支名：{full}")))?;
    Ok(Some((full, remote.to_string(), name.to_string())))
}

/// 更新项目（B04）。
pub fn update(
    session: &WorktreeSession,
    store: &RecoveryStore,
    mode: UpdateMode,
    tag_mode: FetchTagMode,
    timeout: Duration,
) -> Result<UpdateOutcome> {
    let _write = session.acquire_write_lock()?;
    let git = &session.git;
    if let Some(state) = operation_state(git) {
        return Err(AppError::rejected(format!(
            "有进行中的操作（{}），请先完成或中止后再更新",
            state.kind.label()
        )));
    }
    let Some(branch_name) = git.current_branch() else {
        return Err(AppError::rejected("游离 HEAD 没有上游分支，不能更新"));
    };
    let Some((upstream, remote, _remote_branch)) = upstream_details(git, &branch_name)? else {
        return Err(AppError::rejected(format!(
            "分支 {branch_name} 没有配置上游，不能更新"
        )));
    };
    let resolved = match mode {
        UpdateMode::Merge => UpdateResolvedMode::Merge,
        UpdateMode::Rebase => UpdateResolvedMode::Rebase,
        UpdateMode::BranchDefault => resolve_branch_default(git, &branch_name),
    };
    // 先 fetch 上游所在远端（标签方式与 fetch 设置一致）。
    let mut fetch_args = fetch_remote_args(tag_mode);
    fetch_args.push(remote.clone());
    let fetch_refs: Vec<&str> = fetch_args.iter().map(String::as_str).collect();
    network_expect(git, &fetch_refs, &format!("从 {remote} fetch"), timeout)?;

    let upstream_commit = resolve_ref(git, &upstream)?
        .ok_or_else(|| AppError::rejected(format!("上游引用不存在：{upstream}")))?;
    let head = gh::head_commit(git)?
        .ok_or_else(|| AppError::rejected("仓库还没有提交，不能更新"))?;
    if head == upstream_commit {
        return Ok(UpdateOutcome {
            mode: resolved,
            status: UpdateStatus::UpToDate,
            upstream,
            commits_merged: 0,
            conflicts: Vec::new(),
            recovery_record_id: None,
            stash_kept: None,
            output: String::new(),
        });
    }
    let commits_merged = count_range(git, &head, &upstream_commit)?;
    let entries = dirty_paths(git)?;
    let dirty = tracked_dirty(&entries);

    let identity = WorktreeIdentity::capture(git)?;
    let mut recovery_record_id = None;
    let mut record = None;
    if !dirty.is_empty() {
        let version = git.version()?;
        let parsed = crate::git::parse_version(&version).unwrap_or((0, 0));
        if resolved == UpdateResolvedMode::Merge && parsed < MERGE_AUTOSTASH_MIN {
            return Err(AppError::rejected(format!(
                "当前 Git（{version}）不支持 merge 的 autostash；请先提交或丢弃本地改动再更新"
            )));
        }
        // 有未提交改动：先把已跟踪文件的改动存入回收区，再带 autostash 执行。
        let saved = save_recovery(
            session,
            store,
            RecoverySource::UpdateAutostash,
            &upstream,
            &dirty,
            &identity,
        )?;
        recovery_record_id = saved.as_ref().map(|(record, _)| record.id.clone());
        record = saved;
    }

    let mut args: Vec<String> = match resolved {
        UpdateResolvedMode::Merge => vec![
            "--no-optional-locks".into(),
            "merge".into(),
            "--no-edit".into(),
            "--autostash".into(),
            upstream.clone(),
        ],
        UpdateResolvedMode::Rebase => vec![
            "--no-optional-locks".into(),
            "rebase".into(),
            "--autostash".into(),
            upstream.clone(),
        ],
    };
    // 没有本地改动时不加 autostash：避免多余的 stash 行为。
    if dirty.is_empty() {
        args.retain(|arg| arg != "--autostash");
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = non_network_allow_failure(git, &refs)?;
    let combined = format!("{}{}", output.stdout_lossy(), output.stderr);

    let state = operation_state(git);
    let conflicts = unmerged_paths(git)?;
    let autostash_conflict = combined.contains("Applying autostash resulted in conflicts")
        || combined.contains("Autostash conflict");
    let status = if autostash_conflict {
        UpdateStatus::AutostashConflict
    } else if let Some(state) = &state {
        match state.kind {
            OperationKind::Rebase => UpdateStatus::RebaseConflict,
            OperationKind::Merge => UpdateStatus::MergeConflict,
            _ => UpdateStatus::MergeConflict,
        }
    } else if output.exit_code == 0 {
        UpdateStatus::Updated
    } else {
        // 退出码非零但没有留下进行中操作：仍按冲突报告，错误原文保留在 output 里。
        UpdateStatus::MergeConflict
    };
    if let Some(record) = &record {
        finish_recovery(session, store, record, &dirty);
    }
    let stash_kept = if autostash_conflict {
        latest_stash(git)
    } else {
        None
    };
    Ok(UpdateOutcome {
        mode: resolved,
        status,
        upstream,
        commits_merged,
        conflicts,
        recovery_record_id,
        stash_kept,
        output: combined.trim().to_string(),
    })
}

fn latest_stash(git: &Git) -> Option<String> {
    let output = git.run(&["stash", "list", "--format=%gd", "-n", "1"]).ok()?;
    if output.exit_code != 0 {
        return None;
    }
    let name = output.stdout_lossy().trim().to_string();
    (!name.is_empty()).then_some(name)
}

fn resolve_ref(git: &Git, full_name: &str) -> Result<Option<String>> {
    let output = git.run(&[
        "--no-optional-locks",
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("{full_name}^{{commit}}"),
    ])?;
    if output.exit_code != 0 {
        return Ok(None);
    }
    let oid = output.stdout_lossy().trim().to_string();
    Ok(is_full_oid(&oid).then_some(oid))
}

fn count_range(git: &Git, head: &str, upstream: &str) -> Result<u32> {
    if !is_full_oid(head) || !is_full_oid(upstream) {
        return Ok(0);
    }
    let range = format!("{head}..{upstream}");
    let output = git.run_ok(&["rev-list", "--count", &range], "统计更新的提交数")?;
    Ok(output.stdout_lossy().trim().parse::<u32>().unwrap_or(0))
}

/// 推送对话框的预览（B05）。
pub fn push_preview(session: &WorktreeSession, protected: &[String]) -> Result<PushPreview> {
    let git = &session.git;
    let Some(branch_name) = git.current_branch() else {
        return Err(AppError::rejected("游离 HEAD 没有可推送的分支"));
    };
    let remotes = read_remotes(git)?;
    if remotes.is_empty() {
        return Err(AppError::rejected("这个仓库没有配置任何远端"));
    }
    let upstream = upstream_details(git, &branch_name)?;
    let has_upstream = upstream.is_some();
    let (remote, target_branch) = match &upstream {
        Some((_, remote, name)) => (remote.clone(), name.clone()),
        None => {
            let remote = if remotes.iter().any(|name| name == "origin") {
                "origin".to_string()
            } else if remotes.len() == 1 {
                remotes[0].clone()
            } else {
                remotes[0].clone()
            };
            (remote, branch_name.clone())
        }
    };
    let remote_ref = format!("refs/remotes/{remote}/{target_branch}");
    let known = resolve_ref(git, &remote_ref)?;
    let head = gh::head_commit(git)?;
    let commits = match (&known, &head) {
        (Some(known), Some(head)) if known == head => Vec::new(),
        (Some(known), Some(head)) => gh::read_range(git, Some(known), head, MAX_PUSH_PREVIEW)?,
        // 没有远端跟踪分支：列出当前分支可达的提交（上限内）。
        (None, Some(head)) => gh::read_range(git, None, head, MAX_PUSH_PREVIEW)?,
        _ => Vec::new(),
    };
    let commit_count = match (&known, &head) {
        (Some(known), Some(head)) => {
            let range = format!("{known}..{head}");
            git.run_ok(&["rev-list", "--count", &range], "统计待推送提交")
                .map(|output| output.stdout_lossy().trim().parse::<u32>().unwrap_or(0))
                .unwrap_or(0)
        }
        (None, Some(head)) => git
            .run_ok(&["rev-list", "--count", &head], "统计待推送提交")
            .map(|output| output.stdout_lossy().trim().parse::<u32>().unwrap_or(0))
            .unwrap_or(0),
        _ => 0,
    };
    // 强制推送保护的是**被覆盖的远端分支**：目标分支与本地分支任一受保护都拒绝
    // （本地 `feature` 不能强制覆盖远端 `main`）。
    let protected_branch = matches_protected(&branch_name, protected);
    let protected_target = matches_protected(&target_branch, protected);
    let protected_name = if protected_target {
        target_branch.clone()
    } else {
        branch_name.clone()
    };
    let force_reason = (protected_branch || protected_target).then(|| {
        format!("{protected_name} 是受保护分支，不能强制推送")
    });
    Ok(PushPreview {
        branch: branch_name,
        has_upstream,
        remote,
        target_branch,
        remotes,
        truncated: commit_count > MAX_PUSH_PREVIEW,
        commit_count,
        commits: commits
            .iter()
            .map(|commit| crate::history::describe_commit_public(commit))
            .collect(),
        force_allowed: force_reason.is_none(),
        force_reason,
        protected: protected_branch,
    })
}

/// 推送（B05）。
pub fn push(
    session: &WorktreeSession,
    request: &PushRequest,
    protected: &[String],
    timeout: Duration,
) -> Result<PushResult> {
    let _write = session.acquire_write_lock()?;
    let git = &session.git;
    let Some(branch_name) = git.current_branch() else {
        return Err(AppError::rejected("游离 HEAD 没有可推送的分支"));
    };
    let remotes = read_remotes(git)?;
    let upstream = upstream_details(git, &branch_name)?;
    let remote = request
        .remote
        .clone()
        .or_else(|| upstream.as_ref().map(|(_, remote, _)| remote.clone()))
        .unwrap_or_else(|| {
            if remotes.iter().any(|name| name == "origin") {
                "origin".to_string()
            } else {
                remotes.first().cloned().unwrap_or_default()
            }
        });
    if remote.is_empty() || !remotes.iter().any(|name| name == &remote) {
        return Err(AppError::rejected(format!("远端不存在：{remote}")));
    }
    let target_branch = request.branch.clone().unwrap_or_else(|| {
        upstream
            .as_ref()
            .map(|(_, _, name)| name.clone())
            .unwrap_or_else(|| branch_name.clone())
    });
    if branch::validate_branch_name(git, &target_branch).is_err() {
        return Err(AppError::rejected(format!("目标分支名不合法：{target_branch}")));
    }
    // 只有「没有上游」或「用户把目标改成了不同远端 / 分支」时才记录上游；
    // 每次推送都带 `--set-upstream` 会重写 `branch.<name>.remote/merge`（B01、B05）。
    let set_upstream = match &upstream {
        None => true,
        Some((_, upstream_remote, upstream_branch)) => {
            upstream_remote != &remote || upstream_branch != &target_branch
        }
    };
    let mut args: Vec<String> = vec!["--no-optional-locks".into(), "push".into()];
    if request.tags {
        // 只推送当前分支相关的带注释标签。
        args.push("--follow-tags".into());
    }
    if set_upstream {
        args.push("--set-upstream".into());
    }
    let mut lease_note = None;
    match request.force {
        PushForce::None => {}
        PushForce::WithLease => {
            // 保护的是会被覆盖的远端分支：本地分支名与目标分支名任一受保护都拒绝。
            if matches_protected(&target_branch, protected) {
                return Err(AppError::rejected(format!(
                    "{target_branch} 是受保护分支，不能强制推送"
                )));
            }
            if matches_protected(&branch_name, protected) {
                return Err(AppError::rejected(format!(
                    "{branch_name} 是受保护分支，不能强制推送"
                )));
            }
            // 带上推送前读取的远端跟踪分支值：期间远端前进则 Git 自己拒绝（Q123）。
            let tracking = format!("refs/remotes/{remote}/{target_branch}");
            let expected = resolve_ref(git, &tracking)?.unwrap_or_default();
            lease_note = Some(expected.clone());
            args.push(format!("--force-with-lease={target_branch}:{expected}"));
        }
    }
    // 推送前统计：推送成功后 `@{upstream}..HEAD` 已经归零，通知里就数不出提交数了。
    let commits = count_unpushed(git, &remote, &target_branch)?;
    args.push(remote.clone());
    args.push(format!("{branch_name}:{target_branch}"));
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = network_allow_failure(git, &refs, "推送", timeout)?;
    let combined = format!("{}{}", output.stdout_lossy(), output.stderr);
    let _ = lease_note;
    if output.exit_code != 0 {
        let rejected = combined.contains("[rejected]")
            || combined.contains("non-fast-forward")
            || combined.contains("fetch first")
            || combined.contains("stale info");
        if rejected {
            return Ok(PushResult::Rejected {
                remote,
                branch: target_branch,
                reason: format!("远端有新提交，推送被拒绝：{}", combined.trim()),
                output: combined.trim().to_string(),
            });
        }
        return Err(AppError::git(format!(
            "推送失败（退出码 {}）：{}",
            output.exit_code,
            combined.trim()
        )));
    }
    Ok(PushResult::Pushed {
        remote,
        branch: target_branch,
        commits,
        upstream_recorded: set_upstream,
        output: combined.trim().to_string(),
    })
}

/// 相对目标远端分支还没有的提交数（没有远端跟踪分支时是当前分支可达的全部提交）。
fn count_unpushed(git: &Git, remote: &str, branch: &str) -> Result<u32> {
    let tracking = format!("refs/remotes/{remote}/{branch}");
    let runtime = match resolve_ref(git, &tracking)? {
        Some(known) => format!("{known}..HEAD"),
        None => "HEAD".to_string(),
    };
    let output = git.run_ok(&["rev-list", "--count", &runtime], "统计待推送提交")?;
    Ok(output.stdout_lossy().trim().parse::<u32>().unwrap_or(0))
}

/// 检查远端是否有新提交（B03，只比较引用，不取回对象）。
pub fn check_remote(
    session: &WorktreeSession,
    timeout: Duration,
) -> Result<RemoteCheck> {
    let git = &session.git;
    let Some(branch_name) = git.current_branch() else {
        return Err(AppError::rejected("游离 HEAD 没有上游分支"));
    };
    let Some((upstream, remote, name)) = upstream_details(git, &branch_name)? else {
        return Err(AppError::rejected(format!(
            "分支 {branch_name} 没有配置上游，无法检查远端"
        )));
    };
    let output = network_expect(
        git,
        &["ls-remote", "--heads", &remote, &format!("refs/heads/{name}")],
        "检查远端",
        timeout,
    )?;
    let remote_commit = output
        .stdout_lossy()
        .split_whitespace()
        .next()
        .filter(|value| is_full_oid(value))
        .map(str::to_string);
    let known_commit = resolve_ref(git, &upstream)?;
    let has_new_commits = match (&remote_commit, &known_commit) {
        (Some(remote_commit), Some(known)) => remote_commit != known,
        (Some(_), None) => true,
        _ => false,
    };
    Ok(RemoteCheck {
        upstream,
        remote_commit,
        known_commit,
        has_new_commits,
    })
}

/// 网络超时的默认值（命令层使用；测试可以传更短的值）。
pub fn default_network_timeout() -> Duration {
    NETWORK_TIMEOUT
}

/// 受保护分支模式的默认判断（命令层传入设置里的模式）。
pub fn is_protected(branch: &str, patterns: &[String]) -> bool {
    matches_protected(branch, patterns)
}
