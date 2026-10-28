//! 提交编排：把提交预期核对、提交前快照、真实 `git commit`、hooks 对照、
//! 草稿收尾串成一次安全执行（M3 F01/F03/F04/F05/F06，Q21–Q25、Q90–Q96）。
//!
//! 时序（规格 F03/F05）：
//! 1. 写门（同 worktree 非排队写锁，Q90）；
//! 2. 重新读取真实提交预期，与前端带来的预期逐项比较，不一致拒绝（F03.2）；
//! 3. 门禁校验：空白信息、无暂存内容（无冲突 merge 收尾例外）、冲突（含无操作标记
//!    但 index unmerged）、禁提交操作、amend 边界、本地上游包含的二次确认
//!    （F01.2/F01.3/F02.2/F03.4/F06.2）；
//! 4. 部分暂存文件先存完整工作区快照（失败则不启动 commit，F05.1）；
//! 5. 捕获 hooks 基准（全部 tracked + 当时 dirty/untracked 的摘要，F04.3）；
//! 6. **再次**核对提交预期与快照路径工作区状态（F05.2：保存后到启动前再核对）；
//! 7. 真实执行 git commit（登录 shell 环境、实时输出事件、不杀进程、不重跑）；
//! 8. 收尾：
//!    - 成功：读新 HEAD、hooks 对照、快照 Completed（真实 after）、条件清草稿；
//!    - **已确认的非零退出**：同样返回结构化结果（success=false + 输出 +
//!      hooks 改动 + 快照 ID + 操作后预期），快照保存**真实 after** 基准，
//!      因为 hook 可能已经改过工作区，必须留下可安全恢复的基线（F05.3）；
//!    - 启动失败（命令从未运行）：快照记 NotApplied；
//!    - 结果不确定：快照保持无回执（回收区显示待核对）、草稿保留、结果待核对。

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commit::{
    self, CommitDraft, CommitExpectation, CommitRun, DraftClear, DraftStore, UpstreamStatus,
};
use crate::commit_snapshot::{self, HookBaseline, SnapshotBaseline};
use crate::error::{AppError, AppErrorPayload, Result};
use crate::operation;
use crate::recovery::{DiscardOutcome, RecoveryRecord, RecoveryStore, RecordUse, WorktreeIdentity};
use crate::session::WorktreeSession;

/// 客户端 nonce 的长度上限（实时输出事件的归属校验）。
const MAX_NONCE_LEN: usize = 64;

/// amend 的**二次确认凭据**（F06.2）。
///
/// 不只是上游读数：它还绑定当时看到的整组提交预期、提交信息原文与选项。
/// 期间刷新出新 index、切换 worktree、修改输入或选项，任何一项变化都让旧凭据失效，
/// 后端在写前逐项比对，不会拿旧确认去改一份新的提交内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AmendConfirmation {
    /// 上游读数版本（HEAD + 分支 + 上游名 + 上游对象名）。
    pub upstream_version: String,
    /// 用户确认时看到的提交预期指纹。
    pub expectation_fingerprint: String,
    /// 用户确认时的提交信息原文（逐字比较，不用哈希以免前端另算一份）。
    pub message: String,
    /// 确认时的 amend 选项。
    pub amend: bool,
    /// 确认时的跳过 hooks 选项。
    pub skip_hooks: bool,
}

/// 一次提交的固定选项（前端只传这些，不传任何 Git 参数，F03.1）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitOptions {
    /// 用户输入的原始提交信息（保留原样，用于草稿逐字比较）。
    pub message: String,
    #[serde(default)]
    pub amend: bool,
    /// 跳过 hooks：仅本次命令生效（命令局部全跳过），默认 false（F04.2）。
    #[serde(default)]
    pub skip_hooks: bool,
    /// 用户所见提交预期（`commit_context` 的返回值，原样带回）。
    pub expected: CommitExpectation,
    /// amend 且 HEAD 已包含于本地上游时的**第二次**确认凭据（绑定预期/信息/选项）。
    #[serde(default)]
    pub amend_confirmation: Option<AmendConfirmation>,
    /// 前端在**发起请求前**生成的 nonce：后端原样带回实时输出事件，
    /// 前端只接收属于当前这次操作的事件（F04.1 归属校验）。
    pub client_nonce: String,
    #[serde(default)]
    pub draft_revision: String,
}

/// hooks 改动文件列表（提交后报告用）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommitHookChanges {
    pub worktree: Vec<String>,
    pub index: Vec<String>,
}

/// 一次提交的最终结果。
///
/// **已确认的 Git 非零退出也走这里**（`success = false` + `error`），
/// 前端据此既能显示 Git 类错误，又能保留输出、hooks 改动与快照信息（F04.3/F05.3）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommitOutcome {
    /// git 是否成功退出（退出码 0 且信息确认写入）。
    pub success: bool,
    /// 真实退出码；结果不确定为 None。
    pub exit_code: Option<i32>,
    /// 提交创建（或 amend 改写）后的新 HEAD；成功时必有。
    pub new_head: Option<String>,
    /// 提交前的 HEAD。
    pub previous_head: Option<String>,
    /// 合并输出尾部（实时事件已另行交付，这里是聚合副本）。
    pub output: String,
    /// hooks 修改过的文件（提交成功或已确认失败后重新读取对照；不确定时为 None）。
    pub hook_changes: Option<CommitHookChanges>,
    /// 本次提交前保存的快照记录 ID（有部分暂存文件时）。
    pub snapshot_record_id: Option<String>,
    /// 快照记录保存的文件路径（没有快照时为空）。
    pub snapshot_paths: Vec<String>,
    /// 草稿是否已清除（成功且仍是本次输入时清除）。
    pub draft_cleared: bool,
    /// 草稿收尾失败的原因（持久化失败必须可见，F02.5）。
    pub draft_error: Option<String>,
    /// 结果是否需要用户核对真实状态（F03.5）。
    pub needs_verification: bool,
    /// 操作结束后重新读到的提交预期（前端据此刷新列表与审阅进度）。
    pub expectation_after: Option<CommitExpectation>,
    /// 已确认失败时的错误（Git 类）；成功或结果不确定时为 None。
    pub error: Option<AppErrorPayload>,
    /// 结果已确定但收尾需要核对（例如快照收尾落盘失败）时的说明。
    pub verification_note: Option<String>,
    /// 本次操作 ID（与实时输出事件里的 operationId 对应）。
    pub operation_id: String,
    /// 前端发起时提供的 nonce（原样带回，供前端确认事件归属）。
    pub client_nonce: String,
}

/// 提交执行期间的实时输出事件负载（事件名 `revia://commit-output`）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommitOutputEvent {
    /// 本次提交操作 ID。
    pub operation_id: String,
    /// 前端发起时提供的 nonce：只有当前这次请求能认领这些事件。
    pub client_nonce: String,
    pub worktree_id: String,
    pub line: String,
    /// 该行来自 stderr（前端可据此着色，不改变顺序）。
    #[serde(default)]
    pub stderr: bool,
}

/// 提交上下文（F02/F03/F06）：预期、模板、最近信息、草稿、上游状态、门禁。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommitContext {
    pub draft_identity: Option<String>,
    pub worktree_id: String,
    pub expectation: CommitExpectation,
    /// `commit.template` 的内容；未设置或读取失败为 None。
    pub template: Option<String>,
    /// 模板读取失败的原因（可见，但不阻断读草稿与提交，F02.3）。
    pub template_error: Option<String>,
    /// 最近 N 条提交信息（完整多行原文，最多 20 条）。
    pub recent_messages: Vec<String>,
    /// 该 worktree 的持久草稿；没有为 None。
    pub draft: Option<CommitDraft>,
    /// 草稿读取失败的原因（持久化失败必须可见，F02.5）；成功为 None。
    pub draft_error: Option<String>,
    /// 本地上游包含状态（截至上次 fetch，F06.3）。
    pub upstream: UpstreamStatus,
    /// 本地上游跟踪分支名；未配置为 None。
    pub upstream_name: Option<String>,
    /// 上游读数版本：变化使旧的 amend 二次确认失效。
    pub upstream_version: String,
    /// 门禁摘要（展示用；执行时重新校验，不只信 UI，F01.3）。
    pub gates: CommitGates,
}

/// 门禁摘要（前端禁用/提示用；真实门禁在执行时重新校验）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommitGates {
    /// 是否有未解决冲突（含没有进行中操作标记、但 index unmerged 的情况）。
    pub has_conflicts: bool,
    /// 是否有禁用提交的进行中操作（rebase/cherry-pick/revert）。
    pub operation_blocks_commit: bool,
    /// 是否有任意进行中操作（禁 amend，F01.3）。
    pub operation_blocks_amend: bool,
    /// 是否允许 amend（无 HEAD 或进行中操作时为 false）。
    pub amend_available: bool,
    /// 普通提交是否有已暂存内容（暂存删除算内容，intent-to-add 不算）。
    pub has_staged_changes: bool,
    /// 是否允许「没有暂存差异」的普通提交（无冲突 merge 收尾，F03.4）。
    pub allows_empty_diff_commit: bool,
    /// 进行中操作的类型（提示条文案用）；无操作时为 None。
    pub operation_kind: Option<operation::OperationKind>,
    /// 是否处于多提交序列中间阶段。
    pub operation_sequencer_pending: bool,
    /// 未解决冲突文件数。
    pub conflict_count: usize,
    /// HEAD 是否已包含于本地上游（amend 需二次确认）。
    pub upstream_contains_head: bool,
    /// 上游是否无法判断（本地没有上游引用）。
    pub upstream_unknown: bool,
}

impl CommitGates {
    /// 普通提交当前是否被硬门禁挡住（不看输入与选项）。
    pub fn blocks_commit(&self) -> bool {
        self.has_conflicts || self.operation_blocks_commit
    }

    /// 有信息时，当前选项下是否真的可以提交（前端按钮用；后端会重新校验）。
    pub fn can_submit(&self, has_message: bool, amend: bool) -> bool {
        if !has_message || self.blocks_commit() {
            return false;
        }
        if amend {
            return self.amend_available;
        }
        self.has_staged_changes || self.allows_empty_diff_commit
    }
}

impl WorktreeSession {
    /// 读取提交上下文（只读；草稿按 worktree 身份绑定，F02.5）。
    pub fn commit_context(&self, drafts: &DraftStore) -> Result<CommitContext> {
        let git = &self.git;
        let expectation = commit::capture(git)?;
        // 模板读取失败不能连带挡住草稿与提交：把原因作为可见字段返回（F02.3）。
        let (template, template_error) = match commit::read_template(git) {
            Ok(template) => (template, None),
            Err(error) => (None, Some(error.message)),
        };
        let recent = commit::read_recent_messages(git, commit::RECENT_COMMIT_LIMIT)?;
        let upstream = commit::upstream_read(git)?;
        let gates = commit_gates(&expectation, &upstream);
        let (draft_identity, draft, draft_error) = self.load_draft(drafts);
        Ok(CommitContext {
            draft_identity,
            worktree_id: self.id.clone(),
            expectation,
            template,
            template_error,
            recent_messages: recent,
            draft,
            draft_error,
            upstream: upstream.status,
            upstream_name: upstream.upstream,
            upstream_version: upstream.version,
            gates,
        })
    }

    /// 只读取用户所见 HEAD 的完整信息；读取期间 HEAD 变化即拒绝旧预填。
    pub fn amend_head_message(&self, expected_head: &str) -> Result<Option<String>> {
        if commit::read_head(&self.git)?.as_deref() != Some(expected_head) {
            return Err(AppError::rejected("HEAD 已变化，请刷新后重新选择修改上一次提交"));
        }
        let message = commit::read_last_message(&self.git, expected_head)?;
        if commit::read_head(&self.git)?.as_deref() != Some(expected_head) {
            return Err(AppError::rejected("读取期间 HEAD 已变化，请刷新后重新选择修改上一次提交"));
        }
        Ok(Some(message))
    }

    /// 保存提交草稿（F02.5）：身份绑定 + 原子落盘；失败如实上报。
    pub fn save_commit_draft(&self, drafts: &DraftStore, draft: &CommitDraft) -> Result<()> {
        let identity = self
            .captured_identity()?
            .ok_or_else(|| AppError::environment("worktree 身份不可用，不能保存提交草稿"))?;
        drafts.save(&identity, draft)
    }

    /// 清除提交草稿（前端明确丢弃输入时）。不存在视为已清除。
    pub fn clear_commit_draft(&self, drafts: &DraftStore) -> Result<()> {
        match self.captured_identity() {
            Ok(Some(identity)) => drafts.clear(&identity),
            // 身份在打开时就没能捕获：不可能有绑定了身份的草稿，视为已清除。
            Ok(None) | Err(_) => Ok(()),
        }
    }

    /// 读取该 worktree 的草稿：身份/读取失败时给出可见原因，不阻断提交。
    fn load_draft(&self, drafts: &DraftStore) -> (Option<String>, Option<CommitDraft>, Option<String>) {
        let identity = match self.captured_identity() {
            Ok(Some(identity)) => identity,
            Ok(None) => return (None, None, Some("worktree 身份已变化，本次不读取也不保存提交草稿".to_string())),
            Err(error) => return (None, None, Some(error.message)),
        };
        let key = match DraftStore::identity_key(&identity) {
            Ok(key) => key,
            Err(error) => return (None, None, Some(error.message)),
        };
        let loaded = drafts.load(&identity);
        // 身份键与正文来自同一次读取，结束前再确认，不返回失效身份的旧正文。
        match self.captured_identity() {
            Ok(Some(current)) if current == identity => {}
            Ok(_) => return (None, None, Some("读取期间 worktree 身份已变化，未载入旧草稿".to_string())),
            Err(error) => return (None, None, Some(error.message)),
        }
        match loaded {
            Ok(draft) => (Some(key), draft, None),
            Err(error) => (Some(key), None, Some(error.message)),
        }
    }

    /// 执行一次提交（完整时序见模块注释）。
    pub fn execute_commit_flow(
        self: &Arc<Self>,
        drafts: &DraftStore,
        recovery_store: &RecoveryStore,
        hooks_off_root: PathBuf,
        options: CommitOptions,
        operation_id: String,
        emit: Arc<dyn Fn(CommitOutputEvent) + Send + Sync>,
    ) -> Result<CommitOutcome> {
        // 1. 写门：同 worktree 非排队互斥（Q90）。占用时立即拒绝，不排队。
        let _write = self.acquire_write_lock()?;
        self.run_commit_flow(drafts, recovery_store, hooks_off_root, options, operation_id, emit)
    }

    /// 写门内的实际流程（`execute_commit_flow` 已持有写锁）。
    #[allow(clippy::too_many_lines)]
    fn run_commit_flow(
        &self,
        drafts: &DraftStore,
        recovery_store: &RecoveryStore,
        hooks_off_root: PathBuf,
        options: CommitOptions,
        operation_id: String,
        emit: Arc<dyn Fn(CommitOutputEvent) + Send + Sync>,
    ) -> Result<CommitOutcome> {
        let git = &self.git;
        // nonce 是前端发起时生成的归属凭据：只校验形状，不参与语义判断。
        if options.client_nonce.is_empty()
            || options.client_nonce.len() > MAX_NONCE_LEN
            || !options
                .client_nonce
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(AppError::rejected("提交请求缺少有效的操作凭据，请重试"));
        }
        let client_nonce = options.client_nonce.clone();
        // 交给 Git 的信息去掉尾部换行（与 Git 自己的 cleanup 语义一致），
        // 但草稿逐字比较用用户原始输入。
        let raw_message = options.message.clone();
        let git_message = raw_message.trim_end_matches('\n').to_string();

        // 3a. 空白信息不能提交（F02.2）。
        if git_message.trim().is_empty() {
            return Err(AppError::rejected("提交信息不能为空"));
        }

        // 2. 写前重新读取真实提交预期并逐项比较（F03.2、F01.3）。
        let current = commit::capture(git)?;
        if !commit::expectations_match(&current, &options.expected) {
            return Err(AppError::rejected(
                "提交内容已变化（暂存区、HEAD 或进行中操作与确认时不一致），请重新确认",
            ));
        }

        // 3b. 门禁：冲突、禁提交操作、空差异提交边界、amend 边界、上游包含确认。
        if current.has_unmerged() {
            // index 的 unmerged 条目独立于操作标记：`git stash pop` 之类会留下
            // 冲突却没有 MERGE_HEAD，同样必须禁用提交（F01.2）。
            return Err(AppError::rejected(format!(
                "有 {} 个文件存在未解决冲突，不能提交",
                current.unmerged_paths().len()
            )));
        }
        if let Some(state) = &current.operation {
            if state.has_conflicts() {
                return Err(AppError::rejected("存在未解决冲突，不能提交"));
            }
            if !state.commit_allowed() {
                return Err(AppError::rejected(format!(
                    "有进行中的 {} 操作，请到终端处理后再提交",
                    state.kind.label()
                )));
            }
        }
        if options.amend {
            // amend：无 HEAD（unborn）不可用；任意进行中操作禁 amend（F01.3、F06.1）。
            if current.head.is_none() {
                return Err(AppError::rejected("仓库还没有任何提交，不能修改上一次提交"));
            }
            if current.operation.is_some() {
                return Err(AppError::rejected("有进行中的操作，不能修改上一次提交"));
            }
            // F06.2：HEAD 已包含于本地上游，或本地无法判断是否已发布（Unknown）
            // 时，都必须有绑定预期/信息/选项的二次确认——不能把「无法判断」当成
            // 「未发布」来放行。
            let upstream = commit::upstream_read(git)?;
            if upstream.status == UpstreamStatus::Contained || upstream.status == UpstreamStatus::Unknown {
                let confirmation = options.amend_confirmation.as_ref().ok_or_else(|| {
                    AppError::rejected(
                        "上一次提交已包含于上游（按本地上游记录），或本地无法判断是否已发布，修改它需要再次确认",
                    )
                })?;
                verify_amend_confirmation(
                    confirmation,
                    &upstream.version,
                    &current,
                    &raw_message,
                    options.amend,
                    options.skip_hooks,
                )?;
            }
        } else if !current.has_staged_changes() && !current.allows_empty_diff_commit() {
            // 普通提交必须有真实暂存内容；无冲突 merge 收尾允许 index 与 HEAD 相同
            // （Git 仍会写出真正的 merge commit）。不自动暂存、不提供 allow-empty（F03.4）。
            return Err(AppError::rejected(
                "暂存区没有已暂存的改动，不能提交（无冲突 merge 收尾除外）",
            ));
        }

        // 4. 部分暂存文件先存完整工作区快照（F05.1：失败则不启动 commit）。
        let partial_paths = commit_snapshot::partial_staged_paths(git)?;
        let identity = self.captured_identity()?
            .ok_or_else(|| AppError::rejected("worktree 身份已变化，请重新打开后提交"))?;
        recovery_store.ensure_outside(&identity)?;
        let snapshot: Option<(RecoveryRecord, RecordUse, SnapshotBaseline)> =
            if partial_paths.is_empty() {
                None
            } else {
                Some(commit_snapshot::save_snapshot(
                    git,
                    recovery_store,
                    &identity,
                    &partial_paths,
                )?)
            };
        let snapshot_record_id = snapshot.as_ref().map(|(record, _, _)| record.id.clone());
        let snapshot_paths = snapshot
            .as_ref()
            .map(|(record, _, _)| record.before.iter().map(|file| file.path.clone()).collect())
            .unwrap_or_default();

        // 已保存快照后的所有启动前错误，统一记为「未执行」，不伪装成 Git 已启动。
        let preflight = (|| -> Result<(HookBaseline, Option<PathBuf>)> {
            let baseline = commit_snapshot::capture_hook_baseline(git, &current)?;
            let recheck = commit::capture(git)?;
            if !commit::expectations_match(&recheck, &current)
                || !identity.confirmed_match(&WorktreeIdentity::capture(git)?)
            {
                return Err(AppError::rejected("保存快照后提交内容或 worktree 身份发生变化，请重新确认"));
            }
            if let Some((_, _, snapshot_baseline)) = &snapshot {
                if !snapshot_baseline.matches_now(git)? {
                    return Err(AppError::rejected("保存快照后工作区内容发生变化，已保留快照，请重新确认"));
                }
            }
            let hooks_off_dir = if options.skip_hooks {
                Some(commit::prepare_hooks_off_dir(&hooks_off_root)?)
            } else { None };
            Ok((baseline, hooks_off_dir))
        })();
        let (baseline, hooks_off_dir) = match preflight {
            Ok(prepared) => prepared,
            Err(error) => {
                if let Some((record, _, _)) = &snapshot {
                    recovery_store.finish(record, &DiscardOutcome::NotApplied).map_err(|receipt| {
                        AppError::needs_verification(format!("Git 未启动，但快照回执写入失败，记录待核对：{receipt}；原错误：{error}"))
                    })?;
                }
                return Err(error);
            }
        };

        let sink = EmitSink {
            operation_id: operation_id.clone(),
            client_nonce: client_nonce.clone(),
            worktree_id: self.id.clone(),
            emit,
        };
        let run = commit::execute_commit(
            git,
            commit::CommitCommand {
                message: git_message.as_str(),
                amend: options.amend,
                skip_hooks: options.skip_hooks,
                hooks_off_dir: hooks_off_dir.as_deref(),
            },
            Arc::new(sink),
        );
        if let Some(directory) = &hooks_off_dir {
            commit::release_hooks_off_dir(directory);
        }

        let (success, exit_code, output) = match run {
            CommitRun::Finished { success, exit_code, output } => (success, Some(exit_code), output),
            CommitRun::SpawnFailed(reason) => {
                // 命令从未运行：快照记「未执行」，草稿保留（F03.5/F05.3）。
                if let Some((record, _, _)) = &snapshot {
                    recovery_store.finish(record, &DiscardOutcome::NotApplied)?;
                }
                return Err(AppError::environment(format!(
                    "{reason}；提交没有启动，草稿已保留"
                )));
            }
            CommitRun::Uncertain(reason) => {
                // 已启动但结果不确定：不杀进程、不重跑；快照保持无回执（待核对），
                // 草稿保留，阻止静默退出（F03.5、F05.4）。
                return Err(AppError::needs_verification(format!(
                    "{reason}；已保留提交草稿与提交前快照，请核对真实状态后再继续"
                )));
            }
        };

        let observed_head = commit::read_head(git).map_err(|error| {
            AppError::needs_verification(format!("提交进程已结束，但无法核对 HEAD，草稿与快照已保留：{error}"))
        })?;
        if !success && observed_head != current.head {
            return Err(AppError::needs_verification(format!("git commit 非零退出但 HEAD 已变化，不能当作未提交；请核对真实状态，草稿与快照已保留：\n{output}")));
        }

        // 8. 收尾：先落真实快照基准，再汇总结果。
        //
        // F05.3：**已确认的失败也要保存真实 after 基准**。hook 可能已经改过工作区，
        // 此时若只记「未执行」，快照就无法在后续恢复中安全比对，等于丢掉唯一的备份。
        // 只有「命令从未启动」才是 NotApplied。
        let mut needs_verification = false;
        let mut verification_note: Option<String> = None;
        if let Some((record, _, snapshot_baseline)) = &snapshot {
            match snapshot_baseline.capture_now(git) {
                Ok(after) => {
                    if let Err(error) = recovery_store
                        .finish(record, &DiscardOutcome::Completed { after })
                    {
                        needs_verification = true;
                        verification_note = Some(format!(
                            "快照收尾记录失败，结果待核对：{error}"
                        ));
                    }
                }
                Err(error) => {
                    // 读不到真实操作后状态：不伪造基准，保留无回执（待核对）。
                    needs_verification = true;
                    verification_note = Some(format!(
                        "无法读取操作后的真实状态，快照保持待核对（不伪造基准）：{error}"
                    ));
                }
            }
        }

        // hooks 对照：成功时有新 tree 作为正常收尾的对照基准。
        let new_head = if success {
            Some(observed_head.ok_or_else(|| AppError::needs_verification(
                "Git 成功退出但 HEAD 不存在，草稿与快照已保留，请核对真实状态"
            ))?)
        } else { None };
        let hook_changes = match baseline.diff_against(git, new_head.as_deref()) {
            Ok(changes) => Some(to_hook_changes(changes)),
            Err(error) => {
                if success {
                    // 提交已成功：不冒称提交失败，但对照读失败必须待核对。
                    return Err(AppError::needs_verification(format!(
                        "提交已成功（新 HEAD {}），但 hooks 改动对照读取失败（结果待核对）：{error}",
                        new_head.as_deref().unwrap_or("<未知>")
                    )));
                }
                // 提交已确认失败：没有 hook 报告也要如实说明读取失败。
                needs_verification = true;
                verification_note = Some(format!(
                    "提交失败，且 hooks 改动对照读取失败（结果待核对）：{error}"
                ));
                None
            }
        };
        let expectation_after = match commit::capture(git) {
            Ok(after) => Some(after),
            Err(error) => {
                needs_verification = true;
                verification_note.get_or_insert_with(|| format!("提交后的真实状态读取失败，需要核对：{error}"));
                None
            }
        };

        if success {
            // F02.5：只清除**逐字仍是本次输入**的草稿；期间被重新编辑过的草稿不清。
            let (draft_cleared, draft_error) = if needs_verification {
                (false, None)
            } else {
                self.clear_draft_if_matches(drafts, &raw_message, options.amend, &options.draft_revision)
            };
            return Ok(CommitOutcome {
                success: true,
                exit_code,
                new_head,
                previous_head: current.head.clone(),
                output,
                hook_changes,
                snapshot_record_id,
                snapshot_paths,
                draft_cleared,
                draft_error,
                needs_verification,
                expectation_after,
                error: None,
                verification_note: verification_note.clone(),
                operation_id,
                client_nonce,
            });
        }

        // 已确认的 Git 非零退出：返回**结构化**结果，草稿保留，错误按 Git 类上报。
        let detail = hook_changes
            .as_ref()
            .filter(|changes| !(changes.worktree.is_empty() && changes.index.is_empty()))
            .map(|changes| {
                format!(
                    "\nhooks 修改过的文件（未自动撤销）：工作区 {}；暂存区 {}",
                    join_or_none(&changes.worktree),
                    join_or_none(&changes.index)
                )
            })
            .unwrap_or_default();
        let message = format!(
            "git commit 失败（退出码 {}）：{}{detail}",
            exit_code.unwrap_or(-1),
            if output.trim().is_empty() {
                "（Git 未输出内容）".to_string()
            } else {
                output.trim().to_string()
            }
        );
        let mut error = AppError::git(message).payload();
        if let Some(note) = verification_note {
            // 快照/hook 对照收尾失败时，失败本身已确认，但收尾仍需核对。
            needs_verification = true;
            error.message = format!("{}（{}）", error.message, note);
        }
        Ok(CommitOutcome {
            success: false,
            exit_code,
            new_head: None,
            previous_head: current.head.clone(),
            output,
            hook_changes,
            snapshot_record_id,
            snapshot_paths,
            draft_cleared: false,
            draft_error: None,
            needs_verification,
            expectation_after,
            error: Some(error),
            verification_note: None,
            operation_id,
            client_nonce,
        })
    }

    /// 成功后只清除**逐字仍是本次输入**的草稿（F02.5）。
    ///
    /// 比较与删除在 `DraftStore` 的同一把锁内完成，因此与并发保存不会交错；
    /// 用原文而不是 trim 后的文本，避免把「提交后又补了一句」的新稿删掉。
    fn clear_draft_if_matches(
        &self,
        drafts: &DraftStore,
        raw_message: &str,
        amend: bool,
        revision: &str,
    ) -> (bool, Option<String>) {
        let identity = match self.captured_identity() {
            Ok(Some(identity)) => identity,
            Ok(None) => return (false, None),
            Err(error) => return (false, Some(error.message)),
        };
        match drafts.clear_if_matches(&identity, raw_message, amend, revision) {
            Ok(DraftClear::Cleared) => (true, None),
            Ok(DraftClear::Absent) => (false, None),
            Ok(DraftClear::Changed) => (false, None),
            Err(error) => (false, Some(error.message)),
        }
    }
}

/// 校验 amend 二次确认凭据：上游读数、整组预期、信息与选项都必须与当前一致。
fn verify_amend_confirmation(
    confirmation: &AmendConfirmation,
    upstream_version: &str,
    current: &CommitExpectation,
    raw_message: &str,
    amend: bool,
    skip_hooks: bool,
) -> Result<()> {
    if confirmation.upstream_version != upstream_version {
        return Err(AppError::rejected(
            "上游读数已变化，修改上一次提交的确认已失效，请重新确认",
        ));
    }
    if confirmation.expectation_fingerprint != current.fingerprint {
        return Err(AppError::rejected(
            "提交内容已变化，修改上一次提交的确认已失效，请重新确认",
        ));
    }
    if confirmation.message != raw_message {
        return Err(AppError::rejected(
            "提交信息已变化，修改上一次提交的确认已失效，请重新确认",
        ));
    }
    if confirmation.amend != amend || confirmation.skip_hooks != skip_hooks {
        return Err(AppError::rejected(
            "提交选项已变化，修改上一次提交的确认已失效，请重新确认",
        ));
    }
    Ok(())
}

fn to_hook_changes(changes: commit_snapshot::HookChanges) -> CommitHookChanges {
    CommitHookChanges { worktree: changes.worktree, index: changes.index }
}

fn join_or_none(paths: &[String]) -> String {
    if paths.is_empty() {
        "（无）".to_string()
    } else {
        paths.join(", ")
    }
}

/// 门禁摘要（纯函数：预期 + 上游读数 → 前端展示用）。
pub fn commit_gates(
    expectation: &CommitExpectation,
    upstream: &commit::UpstreamRead,
) -> CommitGates {
    let operation = expectation.operation.as_ref();
    let has_conflicts =
        expectation.has_unmerged() || operation.is_some_and(|state| state.has_conflicts());
    let operation_blocks_commit =
        operation.is_some_and(|state| !state.commit_allowed() && !state.has_conflicts());
    let operation_blocks_amend = operation.is_some();
    CommitGates {
        has_conflicts,
        operation_blocks_commit,
        operation_blocks_amend,
        amend_available: expectation.head.is_some() && operation.is_none(),
        has_staged_changes: expectation.has_staged_changes(),
        allows_empty_diff_commit: expectation.allows_empty_diff_commit(),
        operation_kind: operation.map(|state| state.kind),
        operation_sequencer_pending: operation.is_some_and(|state| state.sequencer_pending),
        conflict_count: expectation.unmerged_paths().len(),
        upstream_contains_head: upstream.status == UpstreamStatus::Contained,
        upstream_unknown: upstream.status == UpstreamStatus::Unknown,
    }
}

/// 实时输出 → 事件的桥（在读取线程上逐片调用，emit 必须非阻塞）。
struct EmitSink {
    operation_id: String,
    client_nonce: String,
    worktree_id: String,
    emit: Arc<dyn Fn(CommitOutputEvent) + Send + Sync>,
}

impl commit::CommitOutputSink for EmitSink {
    fn on_output(&self, line: &str, stderr: bool) {
        (self.emit)(CommitOutputEvent {
            operation_id: self.operation_id.clone(),
            client_nonce: self.client_nonce.clone(),
            worktree_id: self.worktree_id.clone(),
            line: line.to_string(),
            stderr,
        });
    }
}
