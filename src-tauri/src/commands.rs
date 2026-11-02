//! Tauri 命令层：前端只能调用这里固定、带类型的操作。
//!
//! 路径一律用 Rust 层登记过的 ID（评审调整 1）：除 `add_repo` 外，不接收任意目录路径；
//! 文件路径必须是当前 `list_changes` 结果里的条目。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::State;

use crate::acceptance::{deliver, Acceptance};
use crate::batch::{BatchOutcome, BatchTarget};
use crate::commit::{CommitDraft, DraftStore};
use crate::commit_flow::{self, CommitOptions, CommitOutcome, CommitOutputEvent};
use crate::error::{AppError, ErrorKind, Result};
use crate::git::highlight::HighlightResult;
use crate::git::rows::ViewMode;
use crate::git::segments::IgnoreWhitespace;
use crate::git::status::ChangesKind;
use crate::git::{CommandLog, CommandRecord};
use crate::history::{
    HistoryCommitDetail, HistoryCommitFiles, HistoryFileTarget, HistoryPage, HistoryRowSlice,
    HistorySideLayout, HistoryStart, HistoryStore, HistoryTargetInfo,
};
use crate::recovery::{
    self, RecoveryInspection, RecoveryState, RecoveryStore, RecoverySummary, RestoreExpectation,
    WorktreeIdentity,
};
use crate::session::{
    ChangeEntry, ChangesSnapshot, FileVersion, RepoEntry, RowSlice, WorktreeSession,
    WorktreeSessionInfo,
};
use crate::settings::{Registry, Settings};
use crate::watch::{resolve_roots, WatchController, WatchEvents, WatchStatus};
use crate::worktree_status::WorktreeStatusEntry;

/// 分发形态与实际数据位置（M8 任务 01）：界面据此隐藏便携版不支持的更新入口。
#[tauri::command]
pub fn distribution_info(state: State<'_, AppState>) -> crate::distribution::DistributionInfo {
    state.distribution.clone()
}

/// 退出保护的**版本化接收端**（任务 08、ST-B05-08-2）。
///
/// 生产 `AppState` 与回归测试**共用这一份实现**，测试不再复制比较算法：
/// 一旦生产比较被改坏，测试会真实失败。
#[derive(Debug, Default)]
pub struct ExitGuard {
    state: std::sync::Mutex<(u64, bool)>,
}

impl ExitGuard {
    /// 应用一次带版本的更新；只有**更大的序号**才生效。
    ///
    /// 本地 Tauri 每个 invoke 是独立 fetch，旧请求可能晚到；靠这里的版本比较
    /// 保证旧 `false` 绝不会覆盖新 `true`（ST-B03-08-1）。
    pub fn apply(&self, seq: u64, blocked: bool) -> bool {
        let Ok(mut guard) = self.state.lock() else {
            return false;
        };
        if seq <= guard.0 {
            return false;
        }
        *guard = (seq, blocked);
        true
    }

    /// 当前是否需要阻止退出。
    pub fn blocked(&self) -> bool {
        self.state.lock().map(|guard| guard.1).unwrap_or(false)
    }

    /// 前端已锁住新写入后，最终复核退出保护再投递真正的应用退出。
    fn request_exit(&self, exit: impl FnOnce()) -> Result<()> {
        let blocked = self
            .state
            .lock()
            .map_err(|_| {
                AppError::coded(
                    ErrorKind::Internal,
                    "errors.commands.exitGuardLockFailed",
                    "退出保护锁失败",
                    &[],
                )
            })?
            .1;
        if blocked {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.commands.writeOperationsUnverifiedResultsRemainQuit",
                "仍有写操作或结果待核对，不能退出",
                &[],
            ));
        }
        exit();
        Ok(())
    }

    /// 已接受的最高序号（诊断与测试用）。
    pub fn accepted_seq(&self) -> u64 {
        self.state.lock().map(|guard| guard.0).unwrap_or(0)
    }
}

pub struct AppState {
    pub registry: Mutex<Registry>,
    pub sessions: Mutex<HashMap<String, Arc<WorktreeSession>>>,
    pub command_log: Arc<CommandLog>,
    /// 只读提交历史目标（M5 任务 01）：与工作区会话分开，关闭后请求与游标立即失效。
    pub history: Arc<HistoryStore>,
    /// 只监视当前 worktree 的控制器（Q88）。
    pub watch: Arc<WatchController>,
    /// 合并事件的单调进度及激活归属。
    pub watch_events: Arc<Mutex<WatchEvents>>,
    pub acceptance: Option<Arc<Acceptance>>,
    pub recovery: RecoveryStore,
    /// 被删除 stash 的独立归档（ADR-0012）；位于应用数据目录。
    pub stash_archive: crate::stash_archive::StashArchiveStore,
    /// 历史改写前的自动 stash（Q161–Q162）。
    pub autostash: crate::autostash::AutostashStore,
    /// 审阅标记存储（任务 07，持久化在 Revia 数据目录）。
    pub review: Arc<crate::review::ReviewStore>,
    /// 前端报告的「退出保护」状态与版本（任务 08、Q96）。
    ///
    /// 存 `(seq, blocked)`：**只有更大的 seq 才生效**。本地 Tauri 每个 invoke 是
    /// 独立 fetch，前端「旧 false 的调用晚到」若直接 store 会解除新建立的保护，
    /// 因此在 Rust 侧用版本号保证旧请求绝不能覆盖新状态（ST-B03-08-1）。
    pub exit_guard: Arc<ExitGuard>,
    /// 每 worktree 持久提交草稿（M3 F02.5）。
    pub drafts: DraftStore,
    /// Revia 数据目录（跳过 hooks 的临时空目录放在这里，F04.2）。
    pub data_dir: PathBuf,
    pub changelists: Arc<crate::m7::changelists::ChangelistStore>,
    /// M7：导入的颜色主题。
    pub m7_themes: Arc<crate::m7::theme::ThemeStore>,
    /// M7：工作区编辑草稿。
    pub m7_edit_drafts: Arc<crate::m7::edit::EditDraftStore>,
    /// M7：内置终端（PTY 进程）。
    pub m7_terminal: Arc<crate::m7::terminal::TerminalManager>,
    /// M7：终端 tab 列表（跨重启保留）。
    pub m7_terminal_tabs: Arc<crate::m7::terminal::TerminalTabStore>,
    /// M7：克隆任务。
    pub m7_clone: Arc<crate::m7::clone::CloneManager>,
    /// M7：GitHub 账号、草稿、回执与设备码流程。
    pub m7_github: Arc<crate::m7::github::GithubState>,
    /// M7：应用更新状态。
    pub m7_update: Arc<crate::m7::update::UpdateState>,
    /// M7：诊断摘要（F21：脱敏摘要持久化 7 天，可清空）。
    pub m7_diagnostics: Arc<crate::m7::diagnostics::DiagnosticsSummaryStore>,
    /// 提交操作 ID 计数器（实时输出事件归属用）。
    commit_ops: std::sync::atomic::AtomicU64,
    /// 分发形态与实际数据位置（M8 任务 01）；启动时由 `lib.rs` 按真实布局覆盖。
    pub distribution: crate::distribution::DistributionInfo,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let diagnostics = Arc::new(crate::m7::diagnostics::DiagnosticsSummaryStore::new(
            &data_dir,
        ));
        let command_log = Arc::new(CommandLog::with_diagnostics(Arc::clone(&diagnostics)));
        Self {
            recovery: RecoveryStore::new(data_dir.clone()),
            stash_archive: crate::stash_archive::StashArchiveStore::new(&data_dir.clone()),
            autostash: crate::autostash::AutostashStore::new(&data_dir.clone()),
            review: Arc::new(crate::review::ReviewStore::new(data_dir.clone())),
            registry: Mutex::new(Registry::load(data_dir.clone(), Arc::clone(&command_log))),
            sessions: Mutex::new(HashMap::new()),
            command_log: Arc::clone(&command_log),
            history: Arc::new(HistoryStore::new()),
            watch: Arc::new(WatchController::new()),
            watch_events: Arc::new(Mutex::new(WatchEvents::default())),
            acceptance: None,
            exit_guard: Arc::new(ExitGuard::default()),
            drafts: DraftStore::new(data_dir.clone()),
            m7_themes: Arc::new(crate::m7::theme::ThemeStore::new(&data_dir)),
            changelists: Arc::new(crate::m7::changelists::ChangelistStore::new(&data_dir)),
            m7_edit_drafts: Arc::new(crate::m7::edit::EditDraftStore::new(&data_dir)),
            m7_terminal: Arc::new(crate::m7::terminal::TerminalManager::new()),
            m7_terminal_tabs: Arc::new(crate::m7::terminal::TerminalTabStore::new(&data_dir)),
            m7_clone: Arc::new(crate::m7::clone::CloneManager::new(Arc::clone(
                &command_log,
            ))),
            m7_github: Arc::new(crate::m7::github::GithubState::new(&data_dir)),
            m7_update: Arc::new(crate::m7::update::UpdateState::new(&data_dir, &data_dir)),
            m7_diagnostics: diagnostics,
            distribution: crate::distribution::DistributionInfo {
                shape: crate::distribution::Distribution::current(),
                data_dir: data_dir.to_string_lossy().into_owned(),
                webview_data_dir: crate::distribution::Distribution::current()
                    .is_portable()
                    .then(|| data_dir.join("webview2").to_string_lossy().into_owned()),
                credential_store: crate::distribution::credential_store().into(),
            },
            data_dir,
            commit_ops: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// 当前是否需要阻止原生退出。
    ///
    /// 原生退出（macOS Quit / ⌘Q）不经过窗口关闭监听，Rust 侧据此决定是否
    /// 阻止退出并通知前端（任务 08、Q96）。只反映状态，不执行任何 Git 动作。
    pub fn exit_blocked(&self) -> bool {
        self.exit_guard.blocked()
    }

    /// 应用一次带版本的保护更新；返回是否被接受（旧版本会被忽略）。
    pub fn apply_exit_guard(&self, seq: u64, blocked: bool) -> bool {
        self.exit_guard.apply(seq, blocked)
    }

    /// 取出（必要时创建）某个 worktree 的会话。
    ///
    /// 创建全程持有会话表锁：首次并发请求只发布一个会话，避免两个会话各自维护
    /// 快照代际、高亮线程与写锁（Q88、Q90）。
    fn session(&self, worktree_id: &str) -> Result<Arc<WorktreeSession>> {
        let mut sessions = self.sessions.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.sessionLockFailed",
                "会话锁失败",
                &[],
            )
        })?;
        if let Some(session) = sessions.get(worktree_id) {
            return Ok(session.clone());
        }
        let path = {
            let registry = self.registry.lock().map_err(|_| {
                AppError::coded(
                    ErrorKind::Internal,
                    "errors.commands.repositoryListLockFailed",
                    "仓库列表锁失败",
                    &[],
                )
            })?;
            registry.worktree_path(worktree_id).ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Rejected,
                    "errors.commands.theWorktreeRegisteredAddRepositoryFirst",
                    "worktree 未登记，请先添加仓库",
                    &[],
                )
            })?
        };
        let home = crate::shell::home_dir()?;
        let shell_env = crate::shell::resolve(&home, &path);
        // 设置里手动指定的 Git 程序（Q27）；未指定时使用登录 shell PATH 中的 git。
        let git_program = {
            let registry = self.registry.lock().map_err(|_| {
                AppError::coded(
                    ErrorKind::Internal,
                    "errors.commands.repositoryListLockFailed2",
                    "仓库列表锁失败",
                    &[],
                )
            })?;
            registry
                .settings()
                .git_path
                .clone()
                .map(std::path::PathBuf::from)
        };
        let session = WorktreeSession::open_with_options(
            worktree_id,
            &path,
            shell_env,
            self.command_log.clone(),
            Some(self.review.clone()),
            git_program.as_deref(),
        )?;
        sessions.insert(worktree_id.to_string(), session.clone());
        Ok(session)
    }

    /// 清空全部会话缓存：重新解析登录 shell 环境与 Git 程序（设置里的“重新加载环境”）。
    ///
    /// 有任何会话正在写操作时拒绝：清缓存会让下一次打开换一个新会话，
    /// 绕过它的写锁，同一 worktree 可能并发写（返修 C3，Q90）。
    pub fn reload_sessions(&self) -> Result<()> {
        let sessions = self.sessions.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.sessionLockFailed2",
                "会话锁失败",
                &[],
            )
        })?;
        for session in sessions.values() {
            if *session.writing.lock().map_err(|_| {
                AppError::coded(
                    ErrorKind::Internal,
                    "errors.commands.writeOperationLockFailed",
                    "写操作锁失败",
                    &[],
                )
            })? {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.commands.aWriteOperationProgressReloadEnvironment",
                    "有正在进行的写操作，完成后再重新加载环境",
                    &[],
                ));
            }
        }
        // 清空缓存之前先让旧会话实例失效：`history_open` 可能在读 Git 期间被
        // 这里抢先完成，只清目标表拦不住它随后把旧会话登记成新目标。
        let instances: Vec<(String, u64)> = sessions
            .iter()
            .map(|(worktree_id, session)| (worktree_id.clone(), session.instance_id()))
            .collect();
        drop(sessions);
        for (worktree_id, instance_id) in instances {
            self.history.invalidate_session(&worktree_id, instance_id);
        }
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.clear();
        }
        // 环境重载后旧的历史目标（含其 Git 程序与 shell 环境）不再有效。
        self.history.clear();
        Ok(())
    }

    /// 取得某个已登记 worktree 的会话（命令层与生产入口测试共用同一条路径）。
    pub fn worktree_session(&self, worktree_id: &str) -> Result<Arc<WorktreeSession>> {
        self.session(worktree_id)
    }

    /// 释放某个 worktree 的会话（切换离开时用）：停止其高亮线程引用与缓存。
    pub fn release_session(&self, worktree_id: &str) {
        // 先标记实例失效，再关目标：正在读 Git 的 `history_open` 随后登记时必须被拒绝，
        // 不能把已释放的旧会话认成新目标。
        let released = self
            .sessions
            .lock()
            .ok()
            .and_then(|mut sessions| sessions.remove(worktree_id));
        if let Some(session) = released {
            self.history
                .invalidate_session(worktree_id, session.instance_id());
        }
        // 历史目标绑定这个 worktree 的会话实例，一起释放：旧 target_id 之后一律拒绝。
        self.history.close_worktree(worktree_id);
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddRepoRequest {
    pub path: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectRepoRequest {
    pub repo_id: String,
    #[serde(default)]
    pub worktree_id: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRequest {
    pub worktree_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchWorktreeRequest {
    pub worktree_id: String,
    /// 前端发起这次激活时的代号；后端据此拒绝迟到的旧请求。
    pub activation: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowsRequest {
    pub worktree_id: String,
    pub path: String,
    pub changes: ChangesKind,
    pub view: ViewMode,
    /// 忽略空白档位（Q140）；缺省为关闭。
    #[serde(default)]
    pub ignore_whitespace: crate::git::segments::IgnoreWhitespace,
    pub from: u32,
    pub to: u32,
}

/// 左右对照布局请求（返修 A1/Q138）。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SideLayoutRequest {
    pub worktree_id: String,
    pub path: String,
    pub changes: ChangesKind,
    #[serde(default)]
    pub ignore_whitespace: crate::git::segments::IgnoreWhitespace,
    #[serde(default = "default_context_lines")]
    pub context_lines: u32,
    /// 差异高亮粒度（按词、按字符、按行/不高亮）。
    #[serde(default)]
    pub highlight: String,
}

/// 左右对照取行请求（返修 A1/Q138）。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SideLinesRequest {
    pub worktree_id: String,
    pub path: String,
    pub changes: ChangesKind,
    #[serde(default)]
    pub ignore_whitespace: crate::git::segments::IgnoreWhitespace,
    #[serde(default = "default_context_lines")]
    pub context_lines: u32,
    #[serde(default)]
    pub highlight: String,
    pub version: FileVersion,
    pub from: u32,
    pub to: u32,
}

fn default_context_lines() -> u32 {
    3
}

/// 按段（块）操作请求（返修 A1/Q142）。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentRequest {
    pub worktree_id: String,
    pub path: String,
    /// 段的写入凭证（`SegmentAction.credential`，含签发时的忽略空白档位）。
    pub credential: String,
    pub hunk_index: u32,
    pub segment_index: u32,
    pub ignore_whitespace: crate::git::segments::IgnoreWhitespace,
}

/// 丢弃一段（块）的请求：比暂存多一个用户确认时的快照版本。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardSegmentRequest {
    pub worktree_id: String,
    pub path: String,
    pub credential: String,
    pub snapshot_revision: String,
    pub hunk_index: u32,
    pub segment_index: u32,
    pub ignore_whitespace: crate::git::segments::IgnoreWhitespace,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightRequestPayload {
    pub worktree_id: String,
    pub path: String,
    pub changes: ChangesKind,
    pub version: FileVersion,
    pub from_line: u32,
    pub to_line: u32,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowIndexRequest {
    pub worktree_id: String,
    pub path: String,
    pub changes: ChangesKind,
    pub view: ViewMode,
    pub line: u32,
    pub version: FileVersion,
    #[serde(default)]
    pub ignore_whitespace: crate::git::segments::IgnoreWhitespace,
    pub expected_fingerprint: Option<String>,
    pub expected_snapshot_revision: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageFileRequest {
    pub worktree_id: String,
    pub path: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnstageFileRequest {
    pub worktree_id: String,
    pub path: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HunkRequest {
    pub worktree_id: String,
    pub path: String,
    pub fingerprint: String,
    pub hunk_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRequest {
    pub worktree_id: String,
    pub targets: Vec<BatchTarget>,
    /// 丢弃时用户确认的原快照版本；暂存不需要。
    #[serde(default)]
    pub snapshot_revision: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListWorktreeStatusRequest {
    pub repo_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardRequest {
    pub worktree_id: String,
    pub path: String,
    pub fingerprint: String,
    pub snapshot_revision: String,
    pub hunk_id: Option<String>,
}

/// 提交时警告（返修 E.3，Q139）：CRLF、游离 HEAD 或 rebase、大文件、非法文件名。
// ---------- 内置三方合并（返修 F10，Q143） ----------

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOpenRequest {
    pub worktree_id: String,
    pub path: String,
    /// 覆盖“合并时自动应用无冲突改动”设置（“应用所有不冲突的改动”按钮用）。
    #[serde(default)]
    pub auto_apply: Option<bool>,
}

/// 未解决冲突的文件与两侧状态（冲突对话框）。
#[tauri::command]
pub async fn list_conflicts(
    state: State<'_, AppState>,
    request: WorktreeRequest,
) -> Result<Vec<crate::merge::ConflictFile>> {
    let session = state.session(&request.worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || session.list_conflicts())
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedReadConflictListCause",
                "读取冲突列表失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

/// 合并页签的数据（Q143）。
#[tauri::command]
pub async fn merge_open(
    state: State<'_, AppState>,
    request: MergeOpenRequest,
) -> Result<crate::merge::MergeLayout> {
    let session = state.session(&request.worktree_id)?;
    let auto_apply = match request.auto_apply {
        Some(value) => value,
        None => {
            let registry = state.registry.lock().map_err(|_| {
                AppError::coded(
                    ErrorKind::Internal,
                    "errors.commands.repositoryListLockFailed3",
                    "仓库列表锁失败",
                    &[],
                )
            })?;
            registry.settings().merge_auto_apply
        }
    };
    let data_dir = state.data_dir.clone();
    let path = request.path.clone();
    let worktree_id = request.worktree_id.clone();
    tauri::async_runtime::spawn_blocking(move || session.merge_layout(&data_dir, &path, auto_apply))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedReadMergeTabsCause",
                "读取合并页签失败：{error}",
                &[("error", error.to_string())],
            )
        })?
        .map(|mut layout| {
            layout.worktree_id = worktree_id;
            layout
        })
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeApplyRequest {
    pub worktree_id: String,
    pub path: String,
    pub result: String,
    pub fingerprint: String,
}

/// “应用”：写前校验、存回收区、写工作区、`git add`、写后核对。
#[tauri::command]
pub async fn merge_apply(
    state: State<'_, AppState>,
    request: MergeApplyRequest,
) -> Result<crate::merge::MergeOutcome> {
    let session = state.session(&request.worktree_id)?;
    let store = state.recovery.clone();
    tauri::async_runtime::spawn_blocking(move || {
        session.merge_apply(&request.path, &request.result, &request.fingerprint, &store)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.applyingMergeResultInterruptedResultNeeds",
            "应用合并结果时中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeAcceptRequest {
    pub worktree_id: String,
    /// 每个文件带它的指纹（冲突列表或合并页签签发），写前逐个核对。
    pub targets: Vec<MergeAcceptTarget>,
    pub side: crate::merge::MergeSide,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeAcceptTarget {
    pub path: String,
    pub fingerprint: String,
}

/// 整文件接受一侧（可多选）。
#[tauri::command]
pub async fn merge_accept_side(
    state: State<'_, AppState>,
    request: MergeAcceptRequest,
) -> Result<Vec<crate::merge::MergeOutcome>> {
    let session = state.session(&request.worktree_id)?;
    let store = state.recovery.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let targets: Vec<(String, String)> = request
            .targets
            .into_iter()
            .map(|target| (target.path, target.fingerprint))
            .collect();
        session.merge_accept_side(&targets, request.side, &store)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.acceptingOneSideInterruptedResultNeeds",
            "接受一侧时中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn commit_warnings(
    state: State<'_, AppState>,
    request: WorktreeRequest,
) -> Result<Vec<crate::commit_warnings::CommitWarning>> {
    let session = state.session(&request.worktree_id)?;
    let settings = {
        let registry = state.registry.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.repositoryListLockFailed4",
                "仓库列表锁失败",
                &[],
            )
        })?;
        registry.settings().clone()
    };
    let options = crate::commit_warnings::WarningOptions {
        crlf: settings.commit_warn_crlf,
        detached: settings.commit_warn_detached,
        large_file_bytes: settings.commit_warn_large_file_bytes,
        invalid_filename: settings.commit_warn_invalid_filename,
    };
    let git = session.git().clone();
    tauri::async_runtime::spawn_blocking(move || crate::commit_warnings::collect(&git, &options))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.preCommitChecksFailedCause",
                "提交前检查失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

#[tauri::command]
pub async fn list_repos(state: State<'_, AppState>) -> Result<Vec<RepoEntry>> {
    let registry = state.registry.lock().map_err(|_| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.repositoryListLockFailed5",
            "仓库列表锁失败",
            &[],
        )
    })?;
    Ok(registry.list())
}

#[tauri::command]
pub async fn list_settings(state: State<'_, AppState>) -> Result<Settings> {
    let registry = state.registry.lock().map_err(|_| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.repositoryListLockFailed6",
            "仓库列表锁失败",
            &[],
        )
    })?;
    Ok(registry.settings().clone())
}

/// 唯一的例外：接收用户选择的目录路径，确认是 Git 仓库后登记。
#[tauri::command]
pub async fn add_repo(state: State<'_, AppState>, request: AddRepoRequest) -> Result<RepoEntry> {
    let path = request.path.clone();
    {
        let mut registry = state.registry.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.repositoryListLockFailed7",
                "仓库列表锁失败",
                &[],
            )
        })?;
        registry.add(&path)
    }
}

#[tauri::command]
pub async fn select_repo(
    state: State<'_, AppState>,
    request: SelectRepoRequest,
) -> Result<RepoEntry> {
    let mut registry = state.registry.lock().map_err(|_| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.repositoryListLockFailed8",
            "仓库列表锁失败",
            &[],
        )
    })?;
    registry.select(&request.repo_id, request.worktree_id.as_deref())
}

#[tauri::command]
pub async fn set_diff_font_size(state: State<'_, AppState>, size: u32) -> Result<Settings> {
    let mut registry = state.registry.lock().map_err(|_| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.repositoryListLockFailed9",
            "仓库列表锁失败",
            &[],
        )
    })?;
    registry.update_font_size(size)
}

/// 修改设置（颜色主题、代码字体与字号、Git 程序路径、合并工具）。
#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    request: crate::settings::SettingsPatch,
) -> Result<Settings> {
    let mut registry = state.registry.lock().map_err(|_| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.repositoryListLockFailed10",
            "仓库列表锁失败",
            &[],
        )
    })?;
    registry.update_settings(&request)
}

/// 重新解析登录 shell 环境与 Git 程序：丢弃会话缓存，下一次打开会重新探测。
/// 有写操作进行中时拒绝，避免绕过写锁并发写（返修 C3，Q90）。
#[tauri::command]
pub async fn reload_environment(state: State<'_, AppState>) -> Result<()> {
    state.reload_sessions()
}

/// worktree 下拉的只读状态（M4 F09.2）。
#[tauri::command]
pub async fn list_worktree_status(
    state: State<'_, AppState>,
    request: ListWorktreeStatusRequest,
) -> Result<Vec<WorktreeStatusEntry>> {
    let repo = {
        let registry = state.registry.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.repositoryListLockFailed11",
                "仓库列表锁失败",
                &[],
            )
        })?;
        registry.find(&request.repo_id).ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.commands.theRepositoryRegisteredAddFirst",
                "仓库未登记，请先添加",
                &[],
            )
        })?
    };
    // 优先使用该仓库已有会话的 Git 上下文（同一套登录 shell 环境），
    // 否则按仓库路径解析一次；不在命令里重复为每个 worktree 启动登录 shell。
    let git = {
        let sessions = state.sessions.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.sessionLockFailed3",
                "会话锁失败",
                &[],
            )
        })?;
        repo.worktrees
            .iter()
            .find_map(|worktree| sessions.get(&worktree.id).cloned())
    };
    let git = match git {
        Some(session) => session.git().clone(),
        None => {
            let home = crate::shell::home_dir()?;
            let path = std::path::PathBuf::from(&repo.path);
            let shell_env = crate::shell::resolve(&home, &path);
            let program = {
                let registry = state.registry.lock().map_err(|_| {
                    AppError::coded(
                        ErrorKind::Internal,
                        "errors.commands.repositoryListLockFailed12",
                        "仓库列表锁失败",
                        &[],
                    )
                })?;
                registry
                    .settings()
                    .git_path
                    .clone()
                    .map(std::path::PathBuf::from)
            };
            crate::git::Git::with_program(
                &path,
                &shell_env,
                state.command_log.clone(),
                program.as_deref(),
            )?
        }
    };
    let entries = repo.worktrees;
    tauri::async_runtime::spawn_blocking(move || {
        entries
            .iter()
            .map(|entry| crate::worktree_status::read(&git, entry))
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedReadWorktreeStatusCause",
            "读取 worktree 状态失败：{error}",
            &[("error", error.to_string())],
        )
    })
}

#[tauri::command]
pub async fn open_worktree(
    state: State<'_, AppState>,
    request: WorktreeRequest,
) -> Result<WorktreeSessionInfo> {
    log_measure(&format!("open_worktree 收到：id={}", request.worktree_id));
    let session = state.session(&request.worktree_id)?;
    log_measure(&format!("open_worktree 完成：{}", session.info.git_version));
    Ok(session.info.clone())
}

/// 切换当前监视目标：只监视这个 worktree，并释放旧监视（Q88、Q95）。
///
/// `activation` 是前端发起时的激活代号：后端用它拒绝迟到进入的旧激活请求。
#[tauri::command]
pub async fn watch_worktree(
    state: State<'_, AppState>,
    request: WatchWorktreeRequest,
) -> Result<WatchStatus> {
    let session = state.session(&request.worktree_id)?;
    apply_acceptance_watch_failure(&state);
    let events = state.watch_events.clone();
    let status = state.watch.activate(
        resolve_roots(
            &session.path,
            session.git_dir_path(),
            session.common_dir_path(),
        ),
        request.activation,
        move |activation| {
            if let Ok(mut events) = events.lock() {
                events.record(activation);
            }
        },
    );
    Ok(status)
}

/// 停止监视（离开 worktree 或关闭窗口）。
#[tauri::command]
pub async fn stop_watch(state: State<'_, AppState>) -> Result<WatchStatus> {
    Ok(state.watch.stop())
}

/// 当前监视状态与变化代号（前端轮询；失败时持续返回错误）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchSnapshot {
    pub status: WatchStatus,
    /// 最近一次触发变化的激活代号；前端只接受属于自己的目标。
    pub changed_activation: Option<u64>,
    pub event_sequence: u64,
}

#[tauri::command]
pub async fn watch_status(state: State<'_, AppState>) -> Result<WatchSnapshot> {
    apply_acceptance_watch_failure(&state);
    let status = state.watch.status();
    let events = state.watch_events.lock().map(|guard| *guard).map_err(|_| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.watchEventLockFailed",
            "监视事件锁失败",
            &[],
        )
    })?;
    Ok(WatchSnapshot {
        status,
        changed_activation: events.activation,
        event_sequence: events.sequence,
    })
}

fn apply_acceptance_watch_failure(state: &AppState) {
    if let Some(acceptance) = &state.acceptance {
        let failure = acceptance.watch_failure();
        state.watch.force_failure(failure.clone());
        if let Some(reason) = failure {
            state.watch.fail_current(reason);
        }
    }
}

/// 写入测量日志文件（只在设置 REVIA_MEASURE_LOG 时生效）。
pub fn log_measure(message: &str) {
    let Some(path) = std::env::var("REVIA_MEASURE_LOG")
        .ok()
        .map(std::path::PathBuf::from)
    else {
        return;
    };
    if minimal_soak_logging() {
        return;
    }
    append_measure_line(&path.with_extension("log"), &format!("{} {message}", chrono_like_stamp()));
}

/// Rust 阶段日志和前端日志共用完整记录写入，避免并发 write_fmt 拼接字段。
fn append_measure_line(path: &std::path::Path, message: &str) {
    use std::io::Write;
    static WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _writer = WRITER.lock().unwrap_or_else(|error| error.into_inner());
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(format!("{message}\n").as_bytes());
    }
}

/// 测量模式的阶段观测；退出时一次写日志，阶段之间不插入磁盘 I/O。
/// 只记录固定名称、线程和时间，不记录参数、正文或凭据。不是验收通过判定。
pub(crate) struct MeasureSpan {
    name: &'static str,
    started: Option<(std::time::Instant, u128)>,
    marks: Vec<(&'static str, u128)>,
}

impl MeasureSpan {
    pub(crate) fn new(name: &'static str) -> Self {
        let started = (std::env::var_os("REVIA_MEASURE_LOG").is_some()
            && !minimal_soak_logging()).then(|| (
                std::time::Instant::now(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                    .map(|value| value.as_micros()).unwrap_or_default(),
            ));
        Self { name, started, marks: Vec::new() }
    }

    pub(crate) fn mark(&mut self, name: &'static str) {
        if let Some((started, _)) = self.started {
            self.marks.push((name, started.elapsed().as_micros()));
        }
    }
}

impl Drop for MeasureSpan {
    fn drop(&mut self) {
        if let Some((started, epoch_us)) = self.started {
            let elapsed_us = started.elapsed().as_micros();
            log_measure(&format!("[TIMING] {}", serde_json::json!({
                "span": self.name, "startedAtUs": epoch_us, "elapsedUs": elapsed_us,
                "thread": format!("{:?}", std::thread::current().id()), "marksUs": self.marks,
            })));
        }
    }
}

fn chrono_like_stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("[rust {now}]")
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewMarkRequest {
    pub worktree_id: String,
    pub path: String,
    /// true = 标记已审阅，false = 取消标记。
    pub marked: bool,
    /// 用户在界面上看到的 HEAD + 工作区版本；执行时必须一致，否则拒绝（Q20、Q89）。
    pub expected_versions: crate::review::ReviewVersions,
}

/// 切换某个文件的审阅标记（任务 07）。
///
/// 标记按 worktree 身份 + HEAD 版本 + 工作区版本记录；持久化失败如实返回错误。
pub fn set_review_mark_impl(
    state: &AppState,
    request: &ReviewMarkRequest,
) -> Result<crate::review::ReviewMarks> {
    let session = state.session(&request.worktree_id)?;
    session.set_review_mark(
        &request.path,
        request.marked,
        Some(request.expected_versions.clone()),
    )
}

#[tauri::command]
pub async fn set_review_mark(
    state: State<'_, AppState>,
    request: ReviewMarkRequest,
) -> Result<crate::review::ReviewMarks> {
    let session = state.session(&request.worktree_id)?;
    let path = request.path.clone();
    let marked = request.marked;
    let expected = request.expected_versions.clone();
    let worktree_id = request.worktree_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        session.set_review_mark(&path, marked, Some(expected))
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedSaveReviewMarksCause",
            "保存审阅标记失败：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "set_review_mark",
        &worktree_id,
        None,
        result,
    )
    .await
}

#[tauri::command]
pub async fn list_changes(
    state: State<'_, AppState>,
    request: WorktreeRequest,
) -> Result<ChangesSnapshot> {
    log_measure("list_changes 收到");
    let mut timing = MeasureSpan::new("ipc.list_changes");
    let session = state.session(&request.worktree_id)?;
    timing.mark("session_ready");
    let changelist_mode = state.registry.lock().map_err(|_| AppError::coded(ErrorKind::Internal,"errors.m7.taskFailed","后台任务失败：{error}",&[("error","settings lock".into())]))?.settings().changelist_mode;
    session.set_changelist_mode(changelist_mode);
    log_measure("list_changes 会话就绪，开始读取");
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _worker = MeasureSpan::new("worker.list_changes");
        session.refresh()
    })
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedReadStatusCause",
                "读取状态失败：{error}",
                &[("error", error.to_string())],
            )
        })?;
    if let Ok(inner) = &result {
        log_measure(&format!(
            "list_changes 完成：{} 个文件，{} ms",
            inner.entries.len(),
            inner.elapsed_ms
        ));
    }
    timing.mark("worker_joined");
    deliver(
        state.acceptance.as_ref(),
        "list_changes",
        &request.worktree_id,
        None,
        result,
    )
    .await
}

#[tauri::command]
pub async fn stage_file(
    state: State<'_, AppState>,
    request: StageFileRequest,
) -> Result<ChangesSnapshot> {
    log_measure(&format!(
        "stage_file 收到：{} {}",
        request.worktree_id, request.path
    ));
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let edit_drafts = Arc::clone(&state.m7_edit_drafts);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<String> = vec![request.path.clone()];
        edit_drafts.with_resolved(&session, &paths, || {
            session.stage_file(&request.path, &request.fingerprint)
        })
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedStageFilesCause",
            "暂存文件失败：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "stage_file",
        &context.worktree_id,
        Some(&context.path),
        result,
    )
    .await
}

#[tauri::command]
pub async fn unstage_file(
    state: State<'_, AppState>,
    request: UnstageFileRequest,
) -> Result<ChangesSnapshot> {
    log_measure(&format!(
        "unstage_file 收到：{} {}",
        request.worktree_id, request.path
    ));
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let edit_drafts = Arc::clone(&state.m7_edit_drafts);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<String> = vec![request.path.clone()];
        edit_drafts.with_resolved(&session, &paths, || {
            session.unstage_file(&request.path, &request.fingerprint)
        })
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedUnstageFilesCause",
            "取消暂存文件失败：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "unstage_file",
        &context.worktree_id,
        Some(&context.path),
        result,
    )
    .await
}

#[tauri::command]
pub async fn stage_hunk(
    state: State<'_, AppState>,
    request: HunkRequest,
) -> Result<ChangesSnapshot> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let edit_drafts = Arc::clone(&state.m7_edit_drafts);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<String> = vec![request.path.clone()];
        edit_drafts.with_resolved(&session, &paths, || {
            session.stage_hunk(&request.path, &request.fingerprint, &request.hunk_id)
        })
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.theStageHunkTaskInterruptedResult",
            "暂存 hunk 任务中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "stage_hunk",
        &context.worktree_id,
        Some(&context.path),
        result,
    )
    .await
}

#[tauri::command]
pub async fn unstage_hunk(
    state: State<'_, AppState>,
    request: HunkRequest,
) -> Result<ChangesSnapshot> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let edit_drafts = Arc::clone(&state.m7_edit_drafts);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<String> = vec![request.path.clone()];
        edit_drafts.with_resolved(&session, &paths, || {
            session.unstage_hunk(&request.path, &request.fingerprint, &request.hunk_id)
        })
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.theUnstageHunkTaskInterruptedResult",
            "取消暂存 hunk 任务中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "unstage_hunk",
        &context.worktree_id,
        Some(&context.path),
        result,
    )
    .await
}

async fn execute_discard(
    state: State<'_, AppState>,
    request: DiscardRequest,
    hunk: bool,
) -> Result<ChangesSnapshot> {
    if hunk != request.hunk_id.is_some() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.commands.theDiscardTargetRangeDoesMatch",
            "丢弃目标范围不匹配",
            &[],
        ));
    }
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let store = state.recovery.clone();
    let edit_drafts = Arc::clone(&state.m7_edit_drafts);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<String> = vec![request.path.clone()];
        edit_drafts.with_resolved(&session, &paths, || {
            if let Some(id) = request.hunk_id {
                session.discard_hunk(
                    &request.path,
                    &request.fingerprint,
                    &request.snapshot_revision,
                    &id,
                    &store,
                )
            } else {
                session.discard_file(
                    &request.path,
                    &request.fingerprint,
                    &request.snapshot_revision,
                    &store,
                )
            }
        })
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.theDiscardTaskInterruptedResultNeeds",
            "丢弃任务中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        if hunk { "discard_hunk" } else { "discard_file" },
        &context.worktree_id,
        Some(&context.path),
        result,
    )
    .await
}

#[tauri::command]
pub async fn stage_files(
    state: State<'_, AppState>,
    request: BatchRequest,
) -> Result<BatchOutcome> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let edit_drafts = Arc::clone(&state.m7_edit_drafts);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<_> = request
            .targets
            .iter()
            .map(|target| target.path.clone())
            .collect();
        edit_drafts.with_resolved(&session, &paths, || {
            Ok(session.stage_files(&request.targets, true))
        })
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.batchStagingFailedCause",
            "批量暂存失败：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "stage_files",
        &context.worktree_id,
        None,
        result,
    )
    .await
}

#[tauri::command]
pub async fn unstage_files(
    state: State<'_, AppState>,
    request: BatchRequest,
) -> Result<BatchOutcome> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let edit_drafts = Arc::clone(&state.m7_edit_drafts);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<_> = request
            .targets
            .iter()
            .map(|target| target.path.clone())
            .collect();
        edit_drafts.with_resolved(&session, &paths, || {
            Ok(session.stage_files(&request.targets, false))
        })
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.batchUnstagingFailedCause",
            "批量取消暂存失败：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "unstage_files",
        &context.worktree_id,
        None,
        result,
    )
    .await
}

#[tauri::command]
pub async fn discard_files(
    state: State<'_, AppState>,
    request: BatchRequest,
) -> Result<BatchOutcome> {
    let session = state.session(&request.worktree_id)?;
    let store = state.recovery.clone();
    let revision = request.snapshot_revision.clone().ok_or_else(|| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.commands.batchDiscardMissingSnapshotRevisionConfirma",
            "批量丢弃缺少确认时的快照版本",
            &[],
        )
    })?;
    let context = request.clone();
    let edit_drafts = Arc::clone(&state.m7_edit_drafts);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<String> = request
            .targets
            .iter()
            .map(|target| target.path.clone())
            .collect();
        edit_drafts.with_resolved(&session, &paths, || {
            Ok(session.discard_files(&request.targets, &revision, &store))
        })
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.theBatchDiscardTaskInterruptedResult",
            "批量丢弃任务中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "discard_files",
        &context.worktree_id,
        None,
        result,
    )
    .await
}

#[tauri::command]
pub async fn discard_file(
    state: State<'_, AppState>,
    request: DiscardRequest,
) -> Result<ChangesSnapshot> {
    execute_discard(state, request, false).await
}

#[tauri::command]
pub async fn discard_hunk(
    state: State<'_, AppState>,
    request: DiscardRequest,
) -> Result<ChangesSnapshot> {
    execute_discard(state, request, true).await
}

/// 列出回收记录的实际执行函数：命令与测试共用。
///
/// 原 worktree 可能已被移动或删除：取不到 session 时仍要能列出记录（Q87）。
pub fn list_recovery_impl(
    state: &AppState,
    worktree_id: Option<&str>,
) -> Result<Vec<RecoverySummary>> {
    let session = worktree_id.and_then(|id| state.session(id).ok());
    let identity = session.and_then(|session| WorktreeIdentity::capture(&session.git).ok());
    state.recovery.list(identity.as_ref())
}

#[tauri::command]
pub async fn list_recovery(
    state: State<'_, AppState>,
    worktree_id: Option<String>,
) -> Result<Vec<RecoverySummary>> {
    // 原 worktree 可能已被移动或删除：取不到 session 时仍要能列出记录（Q87）。
    let session = worktree_id.as_deref().and_then(|id| state.session(id).ok());
    let store = state.recovery.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Unavailable/replaced worktrees must not prevent viewing their saved records.
        let identity = session.and_then(|session| WorktreeIdentity::capture(&session.git).ok());
        store.list(identity.as_ref())
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedReadRecoveryAreaCause",
            "读取回收区失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryInspectRequest {
    pub worktree_id: String,
    pub record_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryRestoreRequest {
    pub worktree_id: String,
    pub record_id: String,
    /// 用户确认时看到的记录版本；记录/回执/恢复历史任何变化都会使确认失效。
    pub expected_version: String,
    /// 用户确认时看到的记录相关路径与状态；不一致即拒绝。
    pub expected_paths: Vec<String>,
    pub expected_state: RecoveryState,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySaveAsRequest {
    /// 当前界面选中的 worktree；仅用于诊断，**不**作为归属依据。
    #[serde(default)]
    pub worktree_id: Option<String>,
    /// 前端发起写入前解析到的归属（`recovery_save_as_owner` 的结果）。
    /// 后端重新解析后必须与它一致，否则拒绝，避免界面与真实归属脱节。
    #[serde(default)]
    pub expected_owner: Option<String>,
    pub record_id: String,
    /// 记录中的哪个路径（一条记录可能包含重命名的新旧两条）。
    pub path: String,
    /// 用户选择的新路径（绝对路径）；必须尚不存在。
    pub destination: String,
}

/// 只读检查的实际执行函数：命令与测试共用。
///
/// 没有当前 worktree 时回退为「当前侧不可用」，而不是直接报错（Q87）。
pub fn inspect_recovery_impl(
    state: &AppState,
    request: &RecoveryInspectRequest,
) -> Result<RecoveryInspection> {
    match state.session(&request.worktree_id) {
        Ok(session) => session.inspect_record(&request.record_id, &state.recovery),
        Err(_) => state.recovery.inspect(&request.record_id, None, None),
    }
}

/// 只读检查一条记录能否直接恢复，并给出对比所需的两侧状态。
#[tauri::command]
pub async fn inspect_recovery(
    state: State<'_, AppState>,
    request: RecoveryInspectRequest,
) -> Result<RecoveryInspection> {
    let store = state.recovery.clone();
    let session = state.session(&request.worktree_id).ok();
    tauri::async_runtime::spawn_blocking(move || match session {
        Some(session) => session.inspect_record(&request.record_id, &store),
        // 当前 worktree 不可用：仍返回保存内容，当前侧标为不可用。
        None => store.inspect(&request.record_id, None, None),
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedReadRecoveryRecordCause",
            "读取回收记录失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// 恢复的实际执行函数：命令与测试共用。
pub fn restore_recovery_impl(
    state: &AppState,
    request: &RecoveryRestoreRequest,
) -> Result<recovery::RestoreOutcome> {
    let session = state.session(&request.worktree_id)?;
    let expected = RestoreExpectation {
        version: request.expected_version.clone(),
        paths: request.expected_paths.clone(),
        state: request.expected_state,
    };
    session.restore_record(&request.record_id, &expected, &state.recovery)
}

/// 恢复保存时的完整工作区状态（不改变暂存区）。
#[tauri::command]
pub async fn restore_recovery(
    state: State<'_, AppState>,
    request: RecoveryRestoreRequest,
) -> Result<recovery::RestoreOutcome> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let store = state.recovery.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let expected = RestoreExpectation {
            version: request.expected_version,
            paths: request.expected_paths,
            state: request.expected_state,
        };
        session.restore_record(&request.record_id, &expected, &store)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.theRestoreTaskInterruptedResultNeeds",
            "恢复任务中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "restore_recovery",
        &context.worktree_id,
        None,
        result,
    )
    .await
}

/// 另存目标的归属：落在哪个已登记 worktree 里，以及解析出的真实路径。
///
/// 前端在**发起写入之前**通过 `recovery_save_as_owner` 拿到这个结果，
/// 用它进入对应 worktree 的全局写门禁与结果通道；后端执行时再解析一次并要求一致（Q90）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SaveAsOwner {
    /// 归属 worktree 的 ID；外部路径为 None。
    pub worktree_id: Option<String>,
    /// 后端解析出的真实目标路径（父目录已规范化）；前端只用它做展示，不做路径判断。
    pub resolved_destination: String,
}

/// 另存的归属解析：目标路径落在哪个已登记 worktree 里。
///
/// 命令与测试共用同一个函数，避免「测试自己挑锁」而命令挑错锁（Q90）。
pub fn save_as_owner(registry: &Registry, destination: &std::path::Path) -> Option<String> {
    registry.worktree_containing(destination.parent().unwrap_or(destination))
}

/// 解析另存归属（前端在写入前调用）。
///
/// 只解析、不写盘，也不取锁；真正的互斥在后端执行时按同一条规则再核对一次。
pub fn resolve_save_as_owner(
    state: &AppState,
    destination: &std::path::Path,
) -> Result<SaveAsOwner> {
    let registry = state.registry.lock().map_err(|_| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.repositoryListLockFailed13",
            "仓库列表锁失败",
            &[],
        )
    })?;
    let owner = save_as_owner(&registry, destination);
    Ok(SaveAsOwner {
        worktree_id: owner,
        resolved_destination: destination.display().to_string(),
    })
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAsOwnerRequest {
    pub destination: String,
}

/// 查询另存目标的真实归属（固定类型接口；前端不自己判断路径）。
#[tauri::command]
pub async fn recovery_save_as_owner(
    state: State<'_, AppState>,
    request: SaveAsOwnerRequest,
) -> Result<SaveAsOwner> {
    resolve_save_as_owner(&state, std::path::Path::new(&request.destination))
}

/// 另存的实际执行函数：命令与测试都调用它。
///
/// 归属存在时先取得**那个** worktree 的非排队写锁；外部路径不强行加锁。
pub fn execute_save_as(
    owner: Option<&std::sync::Arc<crate::session::WorktreeSession>>,
    store: &RecoveryStore,
    record_id: &str,
    path: &str,
    destination: &std::path::Path,
) -> Result<u64> {
    let _guard = match owner {
        Some(session) => Some(session.acquire_write_lock()?),
        None => None,
    };
    store.save_as(record_id, path, destination)
}

/// 另存的完整执行路径：解析归属 → 取该 worktree 的会话与写锁 → 写入。
///
/// 命令与测试共用这个函数，因此「命令是否挑对锁」由真实测试覆盖，
/// 而不是测试自己手工先拿锁再调用 store（Q90）。
pub fn save_recovery_as_impl(state: &AppState, request: &RecoverySaveAsRequest) -> Result<u64> {
    let destination = std::path::PathBuf::from(&request.destination);
    let owner = {
        let registry = state.registry.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.repositoryListLockFailed14",
                "仓库列表锁失败",
                &[],
            )
        })?;
        save_as_owner(&registry, &destination)
    };
    // 归属必须与前端解析到的一致：界面按 A 进的门禁，不能在 B 上落盘（Q90）。
    if owner != request.expected_owner {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.commands.theSaveAsTargetSOwnership",
            "另存目标的归属已变化，请重新选择路径后再试",
            &[],
        ));
    }
    let session = match &owner {
        Some(worktree_id) => Some(state.session(worktree_id)?),
        None => None,
    };
    execute_save_as(
        session.as_ref(),
        &state.recovery,
        &request.record_id,
        &request.path,
        &destination,
    )
}

/// 把保存版本另存到尚不存在的新路径。
///
/// 目标落在已登记 worktree 里时，写入归属**那个** worktree：取得它的非排队写锁，
/// 并把结果交回它的结果通道；外部新路径不强行加锁，但仍逐级校验路径边界（Q90、T14）。
#[tauri::command]
pub async fn save_recovery_as(
    state: State<'_, AppState>,
    request: RecoverySaveAsRequest,
) -> Result<u64> {
    let destination = std::path::PathBuf::from(&request.destination);
    let owner = {
        let registry = state.registry.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.repositoryListLockFailed15",
                "仓库列表锁失败",
                &[],
            )
        })?;
        save_as_owner(&registry, &destination)
    };
    let session = match &owner {
        Some(worktree_id) => Some(state.session(worktree_id)?),
        None => None,
    };
    let store = state.recovery.clone();
    let context = request.clone();
    let expected = request.expected_owner.clone();
    let deliver_owner = owner.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        if owner != expected {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.commands.theSaveAsTargetSOwnership2",
                "另存目标的归属已变化，请重新选择路径后再试",
                &[],
            ));
        }
        execute_save_as(
            session.as_ref(),
            &store,
            &context.record_id,
            &context.path,
            &destination,
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.theSaveAsTaskInterruptedResult",
            "另存任务中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "save_recovery_as",
        deliver_owner.as_deref().unwrap_or(""),
        None,
        result,
    )
    .await
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryDiffRequest {
    pub worktree_id: String,
    pub record_id: String,
    pub path: String,
}

/// 保存版本与当前内容的只读 diff（文本），由 Rust 侧真实 Git 生成。
#[tauri::command]
pub async fn recovery_text_diff(
    state: State<'_, AppState>,
    request: RecoveryDiffRequest,
) -> Result<String> {
    let session = state.session(&request.worktree_id)?;
    let store = state.recovery.clone();
    tauri::async_runtime::spawn_blocking(move || {
        session.recovery_text_diff(&request.record_id, &request.path, &store)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedGenerateComparisonCause",
            "生成对比失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// 清空回收区（在途记录保留）。
#[tauri::command]
pub async fn clear_recovery(state: State<'_, AppState>) -> Result<u64> {
    let store = state.recovery.clone();
    tauri::async_runtime::spawn_blocking(move || store.clear())
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedClearRecoveryAreaCause",
                "清空回收区失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

/// 启动与周期自动清理：由 `lib.rs` 在 setup 与后台线程调用。
pub fn purge_expired_now(store: &RecoveryStore) -> Result<u64> {
    store.auto_purge()
}

/// 读取自动清理状态：普通模式下界面据此显示持久失败提示（Q9）。
#[tauri::command]
pub async fn recovery_maintenance(
    state: State<'_, AppState>,
) -> Result<recovery::RecoveryMaintenance> {
    Ok(state.recovery.maintenance())
}

/// 删除超过保留期的记录（启动时与手动清理共用）。
#[tauri::command]
pub async fn purge_recovery(state: State<'_, AppState>) -> Result<u64> {
    let store = state.recovery.clone();
    tauri::async_runtime::spawn_blocking(move || store.purge_expired())
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedPurgeExpiredRecordsCause",
                "清理过期记录失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

#[tauri::command]
pub async fn get_rows(state: State<'_, AppState>, request: RowsRequest) -> Result<RowSlice> {
    let mut timing = MeasureSpan::new("ipc.get_rows");
    log_measure(&format!(
        "get_rows 收到：{} {}..{}",
        request.path, request.from, request.to
    ));
    let session = state.session(&request.worktree_id)?;
    timing.mark("session_ready");
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _worker = MeasureSpan::new("worker.get_rows");
        session.rows_with_ignore(
            &request.path,
            request.changes,
            request.view,
            request.ignore_whitespace,
            request.from,
            request.to,
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedGetDisplayLinesCause",
            "取显示行失败：{error}",
            &[("error", error.to_string())],
        )
    })?;
    timing.mark("worker_joined");
    deliver(
        state.acceptance.as_ref(),
        "get_rows",
        &context.worktree_id,
        Some(&context.path),
        result,
    )
    .await
}

/// 左右对照布局（返修 A1/Q138）：两侧行数、段、折叠区与段操作目标。
#[tauri::command]
pub async fn side_by_side_layout(
    state: State<'_, AppState>,
    request: SideLayoutRequest,
) -> Result<crate::session::SideBySideLayout> {
    let mut timing = MeasureSpan::new("ipc.side_by_side_layout");
    let session = state.session(&request.worktree_id)?;
    timing.mark("session_ready");
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _worker = MeasureSpan::new("worker.side_by_side_layout");
        session.side_layout(
            &request.path,
            request.changes,
            request.ignore_whitespace,
            request.context_lines,
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedReadSideSideLayoutCause",
            "读取左右对照布局失败：{error}",
            &[("error", error.to_string())],
        )
    })?;
    timing.mark("worker_joined");
    deliver(
        state.acceptance.as_ref(),
        "side_by_side_layout",
        &context.worktree_id,
        Some(&context.path),
        result,
    )
    .await
}

/// 左右对照取行（返修 A1/Q138）：按需取一侧的行、改动标记与词级区间。
#[tauri::command]
pub async fn side_lines(
    state: State<'_, AppState>,
    request: SideLinesRequest,
) -> Result<Vec<crate::git::segments::SideLine>> {
    let mut timing = MeasureSpan::new("ipc.side_lines");
    let session = state.session(&request.worktree_id)?;
    timing.mark("session_ready");
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _worker = MeasureSpan::new("worker.side_lines");
        session.side_lines(
            &request.path,
            request.changes,
            request.ignore_whitespace,
            request.context_lines,
            request.version,
            request.from,
            request.to,
            crate::git::segments::WordGranularity::from_setting(&request.highlight),
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedGetSideSideRowsCause",
            "取左右对照的行失败：{error}",
            &[("error", error.to_string())],
        )
    })?;
    timing.mark("worker_joined");
    deliver(
        state.acceptance.as_ref(),
        "side_lines",
        &context.worktree_id,
        Some(&context.path),
        result,
    )
    .await
}

/// 暂存一段（块）（返修 A1/Q142）：补丁由 Rust 从同一个 Git hunk 按段切开。
#[tauri::command]
pub async fn stage_segment(
    state: State<'_, AppState>,
    request: SegmentRequest,
) -> Result<ChangesSnapshot> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let edit_drafts = Arc::clone(&state.m7_edit_drafts);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<String> = vec![request.path.clone()];
        edit_drafts.with_resolved(&session, &paths, || {
            session.execute_segment_op(
                &request.path,
                &request.credential,
                request.hunk_index,
                request.segment_index,
                true,
                request.ignore_whitespace,
            )
        })
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.theStageBlockTaskInterruptedResult",
            "暂存块任务中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "stage_segment",
        &context.worktree_id,
        Some(&context.path),
        result,
    )
    .await
}

/// 取消暂存一段（块）（返修 A1/Q142）。
#[tauri::command]
pub async fn unstage_segment(
    state: State<'_, AppState>,
    request: SegmentRequest,
) -> Result<ChangesSnapshot> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let edit_drafts = Arc::clone(&state.m7_edit_drafts);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<String> = vec![request.path.clone()];
        edit_drafts.with_resolved(&session, &paths, || {
            session.execute_segment_op(
                &request.path,
                &request.credential,
                request.hunk_index,
                request.segment_index,
                false,
                request.ignore_whitespace,
            )
        })
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.theUnstageBlockTaskInterruptedResult",
            "取消暂存块任务中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "unstage_segment",
        &context.worktree_id,
        Some(&context.path),
        result,
    )
    .await
}

/// 丢弃一段（块）（返修 A1/Q142）：先存回收区，再反向应用补丁。
#[tauri::command]
pub async fn discard_segment(
    state: State<'_, AppState>,
    request: DiscardSegmentRequest,
) -> Result<ChangesSnapshot> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let store = state.recovery.clone();
    let edit_drafts = Arc::clone(&state.m7_edit_drafts);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<String> = vec![request.path.clone()];
        edit_drafts.with_resolved(&session, &paths, || {
            session.discard_segment(
                &request.path,
                &request.credential,
                &request.snapshot_revision,
                request.hunk_index,
                request.segment_index,
                request.ignore_whitespace,
                &store,
            )
        })
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.theDiscardBlockTaskInterruptedResult",
            "丢弃块任务中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "discard_segment",
        &context.worktree_id,
        Some(&context.path),
        result,
    )
    .await
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightTextRequest {
    pub worktree_id: String,
    #[serde(flatten)]
    pub request: crate::git::highlight::HighlightRequest,
}

/// 合并页签按内容取高亮（Q143）。
#[tauri::command]
pub async fn highlight_text(
    state: State<'_, AppState>,
    request: HighlightTextRequest,
) -> Result<HighlightResult> {
    let session = state.session(&request.worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || session.highlight_text(request.request))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.highlightingFailedCause",
                "高亮失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

#[tauri::command]
pub async fn highlight_range(
    state: State<'_, AppState>,
    request: HighlightRequestPayload,
) -> Result<HighlightResult> {
    log_measure(&format!(
        "highlight_range 收到：{} {:?} {:?} {}..{}",
        request.path, request.changes, request.version, request.from_line, request.to_line
    ));
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        session.highlight(
            &request.path,
            request.changes,
            request.version,
            request.from_line,
            request.to_line,
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.highlightingFailedCause2",
            "高亮失败：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "highlight_range",
        &context.worktree_id,
        Some(&context.path),
        result,
    )
    .await
}

#[tauri::command]
pub async fn row_index_for_line(
    state: State<'_, AppState>,
    request: RowIndexRequest,
) -> Result<u32> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        session.row_index_for_line_with_ignore(
            &request.path,
            request.changes,
            request.view,
            request.line,
            request.version,
            request.ignore_whitespace,
            request.expected_fingerprint.as_deref(),
            request.expected_snapshot_revision.as_deref(),
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedConvertLineNumbersCause",
            "换算行号失败：{error}",
            &[("error", error.to_string())],
        )
    })?;
    deliver(
        state.acceptance.as_ref(),
        "row_index_for_line",
        &context.worktree_id,
        Some(&context.path),
        result,
    )
    .await
}

// ---------- 分支与远端（M5 任务 03，规格 B01–B06） ----------
//
// 全部命令只接收已登记的 worktree ID 与结构化参数（分支名、远端名、选项）；
// Git 参数由 Rust 组合，前端不拼任何参数。写操作在 Rust 侧仍占同 worktree 写锁。

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRequestPayload {
    pub worktree_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutRequestPayload {
    pub worktree_id: String,
    pub target: crate::remote_ops::CheckoutTarget,
    pub strategy: crate::remote_ops::CheckoutStrategy,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBranchPayload {
    pub worktree_id: String,
    pub request: crate::remote_ops::CreateBranchRequest,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushBranchPayload {
    pub worktree_id: String,
    pub request: crate::remote_ops::PushRequest,
}

/// 读一份设置快照（命令参数与网络选项都从这里取，前端不传设置值）。
pub(crate) fn settings_snapshot(state: &State<'_, AppState>) -> Result<Settings> {
    let registry = state.registry.lock().map_err(|_| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.repositoryListLockFailed16",
            "仓库列表锁失败",
            &[],
        )
    })?;
    Ok(registry.settings().clone())
}

/// 当前 HEAD 树里的文件路径（日志的路径筛选候选用，只读）。
#[tauri::command]
pub async fn repo_files(
    state: State<'_, AppState>,
    request: WorktreeRequestPayload,
) -> Result<Vec<String>> {
    let session = state.worktree_session(&request.worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || crate::git::history::head_files(&session.git))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedReadFileTreeCause",
                "读取文件树失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

/// 分支组件的只读状态：本地 / 远端分支、领先落后、当前分支与提交身份。
#[tauri::command]
pub async fn branches_overview(
    state: State<'_, AppState>,
    request: WorktreeRequestPayload,
) -> Result<crate::git::branch::BranchOverview> {
    let session = state.worktree_session(&request.worktree_id)?;
    let worktree_id = request.worktree_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::git::branch::read_overview(&session.git, &worktree_id)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedReadBranchesCause",
            "读取分支失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// 检出本地 / 远端分支（B02）。会被覆盖时返回需要确认的文件列表。
#[tauri::command]
pub async fn checkout_branch(
    state: State<'_, AppState>,
    request: CheckoutRequestPayload,
) -> Result<crate::remote_ops::CheckoutResult> {
    let session = state.worktree_session(&request.worktree_id)?;
    let store = state.recovery.clone();
    let archive = state.stash_archive.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::remote_ops::checkout(
            &session,
            &store,
            &request.target,
            request.strategy,
            &archive,
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedCheckOutBranchCause",
            "检出分支失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// 新建分支（可选同时检出）。
#[tauri::command]
pub async fn create_branch(
    state: State<'_, AppState>,
    request: CreateBranchPayload,
) -> Result<crate::remote_ops::CreateBranchOutcome> {
    let session = state.worktree_session(&request.worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::remote_ops::create_branch(&session, &request.request)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedCreateBranchCause",
            "新建分支失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// 对当前仓库的所有远端执行 fetch（标签方式来自设置）。
#[tauri::command]
pub async fn fetch_remotes(
    state: State<'_, AppState>,
    request: WorktreeRequestPayload,
) -> Result<crate::remote_ops::FetchOutcome> {
    let settings = settings_snapshot(&state)?;
    let session = state.worktree_session(&request.worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::remote_ops::fetch(
            &session,
            settings.fetch_tag_mode,
            crate::remote_ops::default_network_timeout(),
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.fetchFailedCause",
            "fetch 失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// 更新项目（⌘T）：先 fetch 上游，再按设置的更新方式并入。
#[tauri::command]
pub async fn update_project(
    state: State<'_, AppState>,
    request: WorktreeRequestPayload,
) -> Result<crate::remote_ops::UpdateOutcome> {
    let settings = settings_snapshot(&state)?;
    let session = state.worktree_session(&request.worktree_id)?;
    let store = state.recovery.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::remote_ops::update(
            &session,
            &store,
            settings.update_mode,
            settings.fetch_tag_mode,
            crate::remote_ops::default_network_timeout(),
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.updateFailedCause",
            "更新失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// 推送对话框的预览：目标远端 / 分支、将要推送的提交与受保护分支判断。
/// F10：本地保护列表与 GitHub 保护缓存取并集（远端已保护而本地未配置的分支同样拦强推）。
fn protected_with_github(
    github: &Arc<crate::m7::github::GithubState>,
    session: &crate::session::WorktreeSession,
    remote: Option<&str>,
    local: &[String],
) -> Result<Vec<String>> {
    crate::m7::github::protection::effective(session, github, remote, local)
}

#[tauri::command]
pub async fn push_preview(
    state: State<'_, AppState>,
    request: WorktreeRequestPayload,
) -> Result<crate::remote_ops::PushPreview> {
    let settings = settings_snapshot(&state)?;
    let session = state.worktree_session(&request.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    tauri::async_runtime::spawn_blocking(move || {
        let preview = crate::remote_ops::push_preview(&session, &settings.protected_branches)?;
        let protected = protected_with_github(
            &github, &session, Some(&preview.remote), &settings.protected_branches,
        )?;
        crate::remote_ops::push_preview(&session, &protected)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedReadPushPreviewCause",
            "读取推送预览失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// 推送（B05）：被拒时返回 Rejected，由前端决定是否「更新后再推送」。
#[tauri::command]
pub async fn push_branch(
    state: State<'_, AppState>,
    request: PushBranchPayload,
) -> Result<crate::remote_ops::PushResult> {
    let settings = settings_snapshot(&state)?;
    let session = state.worktree_session(&request.worktree_id)?;
    let github = Arc::clone(&state.m7_github);
    tauri::async_runtime::spawn_blocking(move || {
        // F10：实际推送保护使用并集。
        let remote = match &request.request.remote {
            Some(remote) => remote.clone(),
            None => crate::remote_ops::push_preview(&session, &settings.protected_branches)?.remote,
        };
        let protected = protected_with_github(
            &github, &session, Some(&remote), &settings.protected_branches,
        )?;
        let mut push_request = request.request;
        push_request.remote = Some(remote);
        crate::remote_ops::push(
            &session,
            &push_request,
            &protected,
            crate::remote_ops::default_network_timeout(),
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.pushFailedCause",
            "推送失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// 检查远端是否有新提交（只比较引用，不取回对象）。
#[tauri::command]
pub async fn check_remote(
    state: State<'_, AppState>,
    request: WorktreeRequestPayload,
) -> Result<crate::remote_ops::RemoteCheck> {
    let session = state.worktree_session(&request.worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::remote_ops::check_remote(&session, crate::remote_ops::default_network_timeout())
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedCheckRemoteCause",
            "检查远端失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// 语法类别名（按索引顺序），前端用它建立 index → CSS 变量的映射（Q50）。
#[tauri::command]
pub async fn syntax_categories() -> Result<Vec<String>> {
    Ok(crate::git::highlight::SyntaxCategory::names()
        .into_iter()
        .map(|name| name.to_string())
        .collect())
}

/// Git 命令日志：M1 只记录，面板在 M4（Q36）。
#[tauri::command]
pub async fn command_log_snapshot(state: State<'_, AppState>) -> Result<Vec<CommandRecord>> {
    Ok(state.command_log.snapshot())
}

// ---------- 只读提交历史（M5 任务 01，规格 H01–H04） ----------
//
// 全部命令只接收已登记 worktree 打开的历史目标 ID；历史内容和对象 ID 一律来自 Git，
// 前端不传仓库目录、不拼 Git 参数，也不传工作区路径。

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryOpenRequest {
    pub worktree_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryTargetRequest {
    pub target_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPageRequest {
    pub target_id: String,
    /// 首次查询必须给起点；继续分页时用游标，不再重解析起点。
    #[serde(default)]
    pub start: Option<HistoryStart>,
    pub first_parent: bool,
    pub limit: u32,
    #[serde(default)]
    pub cursor: Option<String>,
    /// 结构化筛选条件（M5 第二批 L04）；省略等同不筛选。
    #[serde(default)]
    pub filter: Option<crate::history::HistoryFilter>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryMergeFilesRequest {
    pub target_id: String,
    pub commit: String,
    pub mode: crate::history::HistoryMergeMode,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryMergeViewRequest {
    pub target_id: String,
    pub commit: String,
    pub path: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCommitRequest {
    pub target_id: String,
    pub commit: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryFilesRequest {
    pub target_id: String,
    pub commit: String,
    /// 显式指定的真实父提交；根提交为 null（明确用空 tree）。
    #[serde(default)]
    pub parent: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRowsRequest {
    pub target_id: String,
    pub file: HistoryFileTarget,
    pub view: ViewMode,
    #[serde(default)]
    pub ignore_whitespace: Option<IgnoreWhitespace>,
    pub from: u32,
    pub to: u32,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySideLayoutRequest {
    pub target_id: String,
    pub file: HistoryFileTarget,
    #[serde(default)]
    pub ignore_whitespace: Option<IgnoreWhitespace>,
    #[serde(default = "default_context_lines")]
    pub context_lines: u32,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySideLinesRequest {
    pub target_id: String,
    pub file: HistoryFileTarget,
    #[serde(default)]
    pub ignore_whitespace: Option<IgnoreWhitespace>,
    #[serde(default = "default_context_lines")]
    pub context_lines: u32,
    pub version: FileVersion,
    #[serde(default)]
    pub highlight: String,
    pub from: u32,
    pub to: u32,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryHighlightRequest {
    pub target_id: String,
    pub file: HistoryFileTarget,
    #[serde(default)]
    pub ignore_whitespace: Option<IgnoreWhitespace>,
    pub version: FileVersion,
    pub from_line: u32,
    pub to_line: u32,
}

/// 打开只读历史目标：返回 HEAD、本地引用与工程限额。
#[tauri::command]
pub async fn history_open(
    state: State<'_, AppState>,
    request: HistoryOpenRequest,
) -> Result<HistoryTargetInfo> {
    let session = state.worktree_session(&request.worktree_id)?;
    let store = state.history.clone();
    tauri::async_runtime::spawn_blocking(move || store.open(&session))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedOpenCommitHistoryCause",
                "打开提交历史失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

/// 关闭历史目标（幂等）。关闭后该目标的请求与分页游标一律失效。
#[tauri::command]
pub async fn history_close(
    state: State<'_, AppState>,
    request: HistoryTargetRequest,
) -> Result<bool> {
    // 关闭要等目标自身的锁（它可能正在跑 Git），放到阻塞线程里做，
    // 不占用异步运行时线程。
    let store = state.history.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(store.close(&request.target_id)))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedCloseCommitHistoryCause",
                "关闭提交历史失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

/// 重新读取本地引用。
#[tauri::command]
pub async fn history_refs(
    state: State<'_, AppState>,
    request: HistoryTargetRequest,
) -> Result<Vec<crate::history::HistoryRef>> {
    let target = state.history.target(&request.target_id)?;
    tauri::async_runtime::spawn_blocking(move || target.refs())
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedReadReferencesCause",
                "读取引用失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

/// 读取一页提交：起点固定为查询开始时的完整提交对象。
#[tauri::command]
pub async fn history_log_page(
    state: State<'_, AppState>,
    request: HistoryPageRequest,
) -> Result<HistoryPage> {
    let target = state.history.target(&request.target_id)?;
    let start = request.start;
    let cursor = request.cursor;
    let first_parent = request.first_parent;
    let limit = request.limit;
    let filter = request.filter.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        target.log_page_filtered(start, first_parent, limit, cursor, &filter)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedReadCommitHistoryCause",
            "读取提交历史失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// merge 提交的改动文件（默认合并视图 / 按父提交分组）。
#[tauri::command]
pub async fn history_merge_files(
    state: State<'_, AppState>,
    request: HistoryMergeFilesRequest,
) -> Result<crate::history::HistoryMergeFiles> {
    let target = state.history.target(&request.target_id)?;
    let commit = request.commit;
    let mode = request.mode;
    tauri::async_runtime::spawn_blocking(move || target.merge_files(&commit, mode))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedReadChangedFilesMergeCommit",
                "读取合并提交的改动文件失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

/// merge 提交的三栏只读内容（父提交 1 | 合并结果 | 父提交 2）。
#[tauri::command]
pub async fn history_merge_view(
    state: State<'_, AppState>,
    request: HistoryMergeViewRequest,
) -> Result<crate::history::HistoryMergeView> {
    let target = state.history.target(&request.target_id)?;
    let commit = request.commit;
    let path = request.path;
    tauri::async_runtime::spawn_blocking(move || target.merge_view(&commit, &path))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedReadThreePaneContentMerge",
                "读取合并提交的三栏内容失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

/// 按固定提交对象读取完整提交信息与父提交列表。
#[tauri::command]
pub async fn history_commit(
    state: State<'_, AppState>,
    request: HistoryCommitRequest,
) -> Result<HistoryCommitDetail> {
    let target = state.history.target(&request.target_id)?;
    let commit = request.commit;
    tauri::async_runtime::spawn_blocking(move || target.commit_detail(&commit))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedReadCommitDetailsCause",
                "读取提交详情失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

/// 读取提交相对显式父提交的改动文件（根提交用空 tree）。
#[tauri::command]
pub async fn history_commit_files(
    state: State<'_, AppState>,
    request: HistoryFilesRequest,
) -> Result<HistoryCommitFiles> {
    let target = state.history.target(&request.target_id)?;
    let commit = request.commit;
    let parent = request.parent;
    tauri::async_runtime::spawn_blocking(move || target.commit_files(&commit, parent.as_deref()))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedReadChangedFilesCause",
                "读取改动文件失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

/// 提交 diff 的统一 / 左右布局显示行（只读，不签发任何写操作目标）。
#[tauri::command]
pub async fn history_file_rows(
    state: State<'_, AppState>,
    request: HistoryRowsRequest,
) -> Result<HistoryRowSlice> {
    let target = state.history.target(&request.target_id)?;
    let ignore = request.ignore_whitespace.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        target.file_rows(
            &request.file,
            request.view,
            ignore,
            request.from,
            request.to,
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedReadCommitDisplayLinesCause",
            "读取提交显示行失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// 提交 diff 的左右对照布局：两侧行数、段与折叠区（没有段操作目标）。
#[tauri::command]
pub async fn history_file_side_layout(
    state: State<'_, AppState>,
    request: HistorySideLayoutRequest,
) -> Result<HistorySideLayout> {
    let target = state.history.target(&request.target_id)?;
    let ignore = request.ignore_whitespace.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        target.file_side_layout(&request.file, ignore, request.context_lines)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedReadCommitSideSideComparison",
            "读取提交左右对照失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// 按需取提交左右对照一侧的行。
#[tauri::command]
pub async fn history_file_side_lines(
    state: State<'_, AppState>,
    request: HistorySideLinesRequest,
) -> Result<Vec<crate::git::segments::SideLine>> {
    let target = state.history.target(&request.target_id)?;
    let ignore = request.ignore_whitespace.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        target.file_side_lines(
            &request.file,
            ignore,
            request.context_lines,
            request.version,
            request.from,
            request.to,
            crate::git::segments::WordGranularity::from_setting(&request.highlight),
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedReadCommitSideSideRows",
            "读取提交左右行失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// 提交文件的语法高亮（内容由 Rust 按对象 ID 读取，前端不传文件内容或路径）。
#[tauri::command]
pub async fn history_file_highlight(
    state: State<'_, AppState>,
    request: HistoryHighlightRequest,
) -> Result<HighlightResult> {
    let target = state.history.target(&request.target_id)?;
    let ignore = request.ignore_whitespace.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        target.file_highlight(
            &request.file,
            ignore,
            request.version,
            request.from_line,
            request.to_line,
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.commitHighlightingFailedCause",
            "提交高亮失败：{error}",
            &[("error", error.to_string())],
        )
    })?
}

// ---------- 测量用命令（M1 验收需要；不上界面，只在这个里程碑使用） ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasureConfig {
    /// 是否启用前端诊断日志（只有设置 REVIA_MEASURE_LOG 时为真）。
    pub logging: bool,
    /// 是否处于测量/冒烟/长测模式（任务 12：普通启动不显示测量面板）。
    ///
    /// 普通隔离的 acceptance 实例仍是普通模式：它只设置 REVIA_ACCEPTANCE(_DIR)，
    /// 不设置下面这些测量变量，因此这里是 false。
    pub measurement_mode: bool,
    pub auto_run: bool,
    pub only: Option<Vec<String>>,
    pub output_path: Option<String>,
    /// 自动测量时登记并打开的仓库路径（走 `add_repo`）。
    pub repo_path: Option<String>,
    /// 自动测量的目标文件（相对仓库路径）。
    pub target_path: Option<String>,
    /// 只读界面的冒烟检查（打开仓库、切换文件、切换视图）。
    pub smoke: bool,
    /// 长期运行长测时长（秒）。设置了时长即进入连续循环长测模式（Q53、Q78）。
    pub soak_seconds: Option<u64>,
    pub soak_diagnostic: Option<SoakDiagnosticConfig>,
    pub minimal_soak_logging: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoakDiagnosticConfig {
    pub logging_mode: String,
    pub idle_seconds: u64,
    pub cycles: Option<u64>,
    /// 只重复一类操作的诊断（idle、ipc、highlight、channel、open、remount、switch、scroll、jump、view），用于定位压力后仍保留的内存增长。
    pub focus: Option<String>,
}

fn soak_diagnostic() -> Option<SoakDiagnosticConfig> {
    soak_seconds()?;
    let logging_mode = std::env::var("REVIA_SOAK_DIAG_LOGS").ok()?;
    if !matches!(logging_mode.as_str(), "verbose" | "minimal") {
        return None;
    }
    Some(SoakDiagnosticConfig {
        logging_mode,
        idle_seconds: std::env::var("REVIA_SOAK_IDLE_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(90),
        cycles: std::env::var("REVIA_SOAK_DIAG_CYCLES")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0),
        focus: std::env::var("REVIA_SOAK_DIAG_FOCUS").ok().filter(|value| {
            matches!(
                value.as_str(),
                "idle"
                    | "ipc"
                    | "highlight"
                    | "channel"
                    | "open"
                    | "remount"
                    | "switch"
                    | "scroll"
                    | "jump"
                    | "view"
            )
        }),
    })
}

pub(crate) fn is_measure_auto_run() -> bool {
    matches!(
        std::env::var("REVIA_MEASURE").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

pub(crate) fn soak_seconds() -> Option<u64> {
    std::env::var("REVIA_SOAK_SECONDS")
        .ok()?
        .parse::<u64>()
        .ok()
        .filter(|seconds| *seconds > 0)
}

fn is_soak_run() -> bool {
    soak_seconds().is_some()
}

fn minimal_soak_logging() -> bool {
    is_soak_run()
        && (std::env::var("REVIA_SOAK_MINIMAL_LOGGING").as_deref() == Ok("1")
            || std::env::var("REVIA_SOAK_DIAG_LOGS").as_deref() == Ok("minimal"))
}

#[tauri::command]
pub async fn measure_config() -> Result<MeasureConfig> {
    let logging = std::env::var("REVIA_MEASURE_LOG").is_ok();
    let auto_run = is_measure_auto_run();
    let smoke = std::env::var("REVIA_SMOKE").is_ok();
    let soak = soak_seconds().is_some();
    Ok(MeasureConfig {
        logging,
        // 三种测量入口任一开启即视为测量模式；普通启动三者都不成立。
        measurement_mode: logging || auto_run || smoke || soak,
        auto_run,
        only: std::env::var("REVIA_MEASURE_ONLY").ok().map(|value| {
            value
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        }),
        output_path: std::env::var("REVIA_MEASURE_LOG").ok(),
        repo_path: std::env::var("REVIA_MEASURE_REPO").ok(),
        target_path: std::env::var("REVIA_MEASURE_PATH").ok(),
        smoke,
        soak_seconds: soak_seconds(),
        soak_diagnostic: soak_diagnostic(),
        minimal_soak_logging: minimal_soak_logging(),
    })
}

#[tauri::command]
pub async fn write_measurement(payload: serde_json::Value) -> Result<Option<String>> {
    let Some(path) = std::env::var("REVIA_MEASURE_LOG")
        .ok()
        .map(std::path::PathBuf::from)
    else {
        return Ok(None);
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let text = serde_json::to_string_pretty(&payload)?;
    std::fs::write(&path, text).map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.failedWriteMeasurementResultsCause",
            "写入测量结果失败：{error}",
            &[("error", error.to_string())],
        )
    })?;
    // 自动测量可在写完结果后保留原进程供截图；不改变测量边界或数据。
    let keep_measurement_open = is_measure_auto_run()
        && !is_soak_run()
        && std::env::var_os("REVIA_SMOKE").is_none()
        && std::env::var("REVIA_MEASURE_KEEP_OPEN").as_deref() == Ok("1");
    // 默认仍自动退出，使 `open -n -W` 能正常阻塞并返回。
    if !keep_measurement_open
        && (is_measure_auto_run() || is_soak_run() || std::env::var("REVIA_SMOKE").is_ok())
    {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            std::process::exit(0);
        });
    }
    Ok(Some(path.display().to_string()))
}

/// 前端把关键步骤写进日志文件，便于在没有窗口时排查（测量用）。
#[tauri::command]
pub async fn debug_log(message: String) -> Result<()> {
    let Some(path) = std::env::var("REVIA_MEASURE_LOG")
        .ok()
        .map(std::path::PathBuf::from)
    else {
        return Ok(());
    };
    append_measure_line(&path.with_extension("log"), &message);
    Ok(())
}

/// 供测试与诊断：返回一个文件的列表元数据（不渲染）。
#[tauri::command]
pub async fn change_entry(state: State<'_, AppState>, request: RowsRequest) -> Result<ChangeEntry> {
    // 通过取一行来实现，保证校验逻辑一致。
    let slice = get_rows(
        state,
        RowsRequest {
            to: request.from + 1,
            ..request
        },
    )
    .await?;
    Ok(slice.entry)
}

/// 设置「退出保护」状态（任务 08、Q96）。
///
/// 请求带单调递增的 `seq`：只有更大的序号才生效，因此「旧 false 晚到」
/// 不会解除已经建立的新保护（ST-B03-08-1）。只记录状态：
/// 不取消写操作、不重跑任何命令。
#[tauri::command]
pub fn set_exit_blocked(state: State<'_, AppState>, seq: u64, blocked: bool) -> bool {
    state.apply_exit_guard(seq, blocked)
}

/// 仅执行已通过前端最终门禁的退出；不以关闭窗口替代原退出请求。
#[tauri::command]
pub fn request_exit(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<()> {
    state.exit_guard.request_exit(|| app.exit(0))
}

// ---------- 提交工作流（M3 F01–F06） ----------

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitContextRequest {
    pub worktree_id: String,
}

/// 读取提交上下文（只读；草稿按 worktree 身份绑定，F02.5）。
#[tauri::command]
pub async fn commit_context(
    state: State<'_, AppState>,
    request: CommitContextRequest,
) -> Result<commit_flow::CommitContext> {
    let session = state.session(&request.worktree_id)?;
    let drafts = state.drafts.clone();
    tauri::async_runtime::spawn_blocking(move || session.commit_context(&drafts))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedReadCommitContextCause",
                "读取提交上下文失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmendMessageRequest {
    pub worktree_id: String,
    pub expected_head: String,
}

/// amend 预填用户所见 HEAD 的完整信息（F06.1；HEAD 变化拒绝）。
#[tauri::command]
pub async fn amend_head_message(
    state: State<'_, AppState>,
    request: AmendMessageRequest,
) -> Result<Option<String>> {
    let session = state.session(&request.worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || session.amend_head_message(&request.expected_head))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedReadLatestCommitMessageCause",
                "读取最近提交信息失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDraftRequest {
    pub worktree_id: String,
    pub draft: CommitDraft,
}

/// 保存提交草稿（F02.5：身份绑定 + 原子落盘；失败如实上报，前端保留本地输入）。
#[tauri::command]
pub async fn save_commit_draft(
    state: State<'_, AppState>,
    request: SaveDraftRequest,
) -> Result<()> {
    let session = state.session(&request.worktree_id)?;
    let drafts = state.drafts.clone();
    tauri::async_runtime::spawn_blocking(move || session.save_commit_draft(&drafts, &request.draft))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedSaveCommitDraftCause",
                "保存提交草稿失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearDraftRequest {
    pub worktree_id: String,
}

/// 清除提交草稿（用户明确丢弃输入时；不存在视为已清除）。
#[tauri::command]
pub async fn clear_commit_draft(
    state: State<'_, AppState>,
    request: ClearDraftRequest,
) -> Result<()> {
    let session = state.session(&request.worktree_id)?;
    let drafts = state.drafts.clone();
    tauri::async_runtime::spawn_blocking(move || session.clear_commit_draft(&drafts))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.failedClearCommitDraftCause",
                "清除提交草稿失败：{error}",
                &[("error", error.to_string())],
            )
        })?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteCommitRequest {
    #[serde(default)]
    pub selection: Option<crate::m7::changelists::ChangelistSelection>,
    /// 登记的 worktree ID（前端不传任何路径或 Git 参数，F03.1）。
    pub worktree_id: String,
    pub options: CommitOptions,
}

/// 执行提交（F03/F04/F05 完整时序）。
///
/// 实时输出经 `revia://commit-output` 事件交付：事件同时带后端 operationId、
/// 前端提供的 clientNonce 与 worktree ID，前端只认领属于当前这次请求的事件
/// （F04.1）。写门、退出保护与待核对由前端 `WriteGuardOrchestrator` +
/// Rust 同 worktree 写锁共同保证（F03.3）。
#[tauri::command]
pub async fn execute_commit(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: ExecuteCommitRequest,
) -> Result<CommitOutcome> {
    let session = state.session(&request.worktree_id)?;
    let drafts = state.drafts.clone();
    let recovery = state.recovery.clone();
    let hooks_off_root = state.data_dir.clone();
    let operation_id = format!(
        "commit-{}",
        state
            .commit_ops
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let options = request.options;
    let selection = request.selection;
    let changelists = Arc::clone(&state.changelists);
    let edit_drafts = Arc::clone(&state.m7_edit_drafts);
    tauri::async_runtime::spawn_blocking(move || {
        let emit_app = app.clone();
        let emit: Arc<dyn Fn(CommitOutputEvent) + Send + Sync> =
            Arc::new(move |event: CommitOutputEvent| {
                use tauri::Emitter;
                // 事件发送失败（窗口已关）不影响提交本身；输出仍留在返回值里。
                let _ = emit_app.emit("revia://commit-output", &event);
            });
        let mut paths: Vec<String> = options
            .expected
            .index
            .iter()
            .filter(|entry| entry.status != "F")
            .flat_map(|entry| {
                std::iter::once(entry.path.clone()).chain(entry.original_path.clone())
            })
            .collect();
        if let Some(selection) = &selection {
            let view = changelists.view(&session)?;
            if view.identity != selection.identity || view.revision != selection.revision {
                return Err(AppError::coded(ErrorKind::Rejected,"errors.m7.changelistChanged","更改列表或文件已变化，请刷新并重新选择",&[]));
            }
            paths = view.items.iter().filter(|i|i.selected&&!i.stale).map(|i|i.path.clone()).collect();
        }
        edit_drafts.with_resolved(&session, &paths, || {
            if let Some(selection) = selection {
                return session.execute_changelist_commit_flow(&changelists, &selection, &drafts, &recovery, hooks_off_root, options, operation_id, emit);
            }
            session.execute_commit_flow(
                &drafts,
                &recovery,
                hooks_off_root,
                options,
                operation_id,
                emit,
            )
        })
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.theCommitTaskInterruptedResultNeeds",
            "提交任务中断，结果待核对：{error}",
            &[("error", format!("{}", error))],
        )
    })?
}

#[cfg(test)]
mod exit_guard_tests {
    use super::ExitGuard;

    #[test]
    fn explicit_exit_dispatches_only_after_guard_release() {
        let guard = ExitGuard::default();
        let calls = std::cell::Cell::new(0);
        assert!(guard.apply(1, true));
        let result = guard.request_exit(|| calls.set(calls.get() + 1));
        assert_eq!(result.unwrap_err().kind, crate::error::ErrorKind::Rejected);
        assert_eq!(calls.get(), 0, "保护未释放时不投递退出");
        assert!(guard.apply(2, false));
        guard.request_exit(|| calls.set(calls.get() + 1)).unwrap();
        assert_eq!(calls.get(), 1, "只投递实际应用退出，不销毁窗口冒充");
    }

    /// 新 true 建立后，旧 false（更小 seq）必须被拒绝。
    #[test]
    fn newer_true_rejects_late_older_false() {
        let guard = ExitGuard::default();
        assert!(guard.apply(1, false), "首个 false 生效");
        assert!(guard.apply(2, true), "新 true 生效");
        assert!(!guard.apply(1, false), "旧 false 必须被拒绝");
        assert!(guard.blocked(), "旧 false 晚到不得解除新 true");
        assert_eq!(guard.accepted_seq(), 2, "已接受序号不得倒退");
    }

    /// 更新的 false 正常释放。
    #[test]
    fn newer_false_releases() {
        let guard = ExitGuard::default();
        assert!(guard.apply(1, true));
        assert!(guard.apply(2, false), "更新的 false 正常生效");
        assert!(!guard.blocked());
    }

    /// 相同序号不重复生效（幂等且不倒退）。
    #[test]
    fn equal_seq_is_rejected() {
        let guard = ExitGuard::default();
        assert!(guard.apply(1, true));
        assert!(!guard.apply(1, false), "同序号不得改变状态");
        assert!(guard.blocked());
    }
}

// ---------------------------------------------------------------------------
// M6: stash, in-progress operations, history rewriting, file history/blame,
// external tools, and first-run language.
// ---------------------------------------------------------------------------

/// Stashes of the worktree's common Git directory (newest first).
#[tauri::command]
pub async fn stash_list(
    state: State<'_, AppState>,
    request: WorktreeRequest,
) -> Result<Vec<crate::stash::StashEntry>> {
    let session = state.session(&request.worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || crate::stash::list(&session.git))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.stashListInterrupted",
                "读取 stash 列表时中断：{error}",
                &[("error", error.to_string())],
            )
        })?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashOidRequest {
    pub worktree_id: String,
    pub oid: String,
    pub entry_key: String,
    pub expected_state: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashInspectCommand {
    pub worktree_id: String,
    pub oid: String,
    pub entry_key: String,
    #[serde(default)]
    pub compare_local: bool,
}

#[tauri::command]
pub async fn stash_inspect(
    state: State<'_, AppState>,
    request: StashInspectCommand,
) -> Result<crate::stash::StashInspection> {
    let session = state.session(&request.worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::stash::inspect_selected(
            &session.git,
            &request.oid,
            Some(&request.entry_key),
            request.compare_local,
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.stashInspectInterrupted",
            "读取 stash 内容时中断：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashPreflightCommand {
    pub worktree_id: String,
    pub oid: Option<String>,
    pub entry_key: Option<String>,
    #[serde(default)]
    pub collection: bool,
}

#[tauri::command]
pub async fn stash_preflight(
    state: State<'_, AppState>,
    request: StashPreflightCommand,
) -> Result<String> {
    let session = state.session(&request.worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        if request.collection {
            crate::stash::collection_preflight(&session)
        } else {
            crate::stash::preflight_selected(
                &session,
                request.oid.as_deref(),
                request.entry_key.as_deref(),
            )
        }
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.stashInspectInterrupted",
            "读取 stash 内容时中断：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashSaveCommand {
    pub worktree_id: String,
    pub expected_state: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub keep_index: bool,
}

#[tauri::command]
pub async fn stash_save(
    state: State<'_, AppState>,
    request: StashSaveCommand,
) -> Result<crate::stash::StashEntry> {
    let session = state.session(&request.worktree_id)?;
    let stash = crate::stash::StashSaveRequest {
        message: request.message,
        keep_index: request.keep_index,
        expected_state: request.expected_state,
    };
    tauri::async_runtime::spawn_blocking(move || crate::stash::save(&session, &stash))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::NeedsVerification,
                "errors.commands.stashSaveInterrupted",
                "保存 stash 时中断，结果待核对：{error}",
                &[("error", error.to_string())],
            )
        })?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashApplyCommand {
    pub worktree_id: String,
    pub expected_state: String,
    pub oid: String,
    pub entry_key: String,
    #[serde(default)]
    pub restore_index: bool,
    #[serde(default)]
    pub delete_after: bool,
    #[serde(default)]
    pub new_branch: Option<String>,
}

#[tauri::command]
pub async fn stash_apply(
    state: State<'_, AppState>,
    request: StashApplyCommand,
) -> Result<crate::stash::StashApplyOutcome> {
    let session = state.session(&request.worktree_id)?;
    let recovery = state.recovery.clone();
    let archive = state.stash_archive.clone();
    let apply = crate::stash::StashApplyRequest {
        oid: request.oid,
        entry_key: Some(request.entry_key),
        restore_index: request.restore_index,
        delete_after: request.delete_after,
        new_branch: request.new_branch,
        expected_state: request.expected_state,
    };
    tauri::async_runtime::spawn_blocking(move || {
        crate::stash::apply(&session, &recovery, &archive, &apply)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.stashApplyInterrupted",
            "应用 stash 时中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn stash_drop(
    state: State<'_, AppState>,
    request: StashOidRequest,
) -> Result<crate::stash::StashDropOutcome> {
    let session = state.session(&request.worktree_id)?;
    let archive = state.stash_archive.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::stash::drop_selected(
            &session,
            &archive,
            &request.oid,
            &request.entry_key,
            &request.expected_state,
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.stashDropInterrupted",
            "删除 stash 时中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashClearCommand {
    pub worktree_id: String,
    pub expected_oids: Vec<String>,
    pub expected_state: String,
}

#[tauri::command]
pub async fn stash_clear(
    state: State<'_, AppState>,
    request: StashClearCommand,
) -> Result<crate::stash::StashClearOutcome> {
    let session = state.session(&request.worktree_id)?;
    let archive = state.stash_archive.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::stash::clear_confirmed(
            &session,
            &archive,
            &request.expected_oids,
            &request.expected_state,
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.stashClearInterrupted",
            "清空 stash 时中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn stash_archives(
    state: State<'_, AppState>,
    request: WorktreeRequest,
) -> Result<Vec<crate::stash_archive::StashArchiveSummary>> {
    let session = state.session(&request.worktree_id)?;
    let archive = state.stash_archive.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let identity = WorktreeIdentity::capture(&session.git)?;
        archive.list(Some(&identity))
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.stashArchivesInterrupted",
            "读取 stash 归档时中断：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn stash_archive_restore(
    state: State<'_, AppState>,
    request: StashArchiveRestoreCommand,
) -> Result<String> {
    let session = state.session(&request.worktree_id)?;
    let archive = state.stash_archive.clone();
    tauri::async_runtime::spawn_blocking(move || {
        archive.restore_confirmed(&session, &request.id, &request.expected_state)
    })
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::NeedsVerification,
                "errors.commands.stashArchiveRestoreInterrupted",
                "恢复 stash 归档时中断，结果待核对：{error}",
                &[("error", error.to_string())],
            )
        })?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashArchiveRestoreCommand {
    pub worktree_id: String,
    pub id: String,
    pub expected_state: String,
}

#[tauri::command]
pub async fn autostash_pending(
    state: State<'_, AppState>,
    request: WorktreeRequest,
) -> Result<Option<crate::autostash::AutostashPending>> {
    let session = state.session(&request.worktree_id)?;
    let store = state.autostash.clone();
    tauri::async_runtime::spawn_blocking(move || store.pending(&session))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.autostashPendingInterrupted",
                "读取自动 stash 时中断：{error}",
                &[("error", error.to_string())],
            )
        })?
}

#[tauri::command]
pub async fn autostash_restore(
    state: State<'_, AppState>,
    request: AutostashRestoreCommand,
) -> Result<Option<crate::autostash::AutostashRestoreOutcome>> {
    let session = state.session(&request.worktree_id)?;
    let recovery = state.recovery.clone();
    let archive = state.stash_archive.clone();
    let store = state.autostash.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _write = session.acquire_write_lock()?;
        store.restore_confirmed(
            &session,
            &recovery,
            &archive,
            &request.link,
            &request.expected_state,
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.autostashRestoreInterrupted",
            "恢复自动 stash 时中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutostashRestoreCommand {
    pub worktree_id: String,
    pub link: crate::autostash::AutostashLink,
    pub expected_state: String,
}

#[tauri::command]
pub async fn autostash_forget(
    state: State<'_, AppState>,
    worktree_id: String,
    id: String,
) -> Result<()> {
    let session = state.session(&worktree_id)?;
    let store = state.autostash.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _write = session.acquire_write_lock()?;
        store.forget(&session, &id)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.autostashForgetInterrupted",
            "保留自动 stash 时中断：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn operation_state(
    state: State<'_, AppState>,
    request: WorktreeRequest,
) -> Result<Option<crate::operation::OperationState>> {
    let session = state.session(&request.worktree_id)?;
    let data = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || crate::operation_flow::state(&session, &data))
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.operationStateInterrupted",
            "读取进行中的操作时中断：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn operation_execute(
    state: State<'_, AppState>,
    worktree_id: String,
    request: crate::operation_flow::OperationActionRequest,
) -> Result<crate::operation_flow::OperationOutcome> {
    let session = state.session(&worktree_id)?;
    let recovery = state.recovery.clone();
    let archive = state.stash_archive.clone();
    let autostash = state.autostash.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::operation_flow::execute(&session, &recovery, &autostash, &archive, &request)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.operationExecuteInterrupted",
            "执行操作时中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn rewrite_range(
    state: State<'_, AppState>, worktree_id: String, target: String, expected_head: String,
) -> Result<Vec<crate::history::HistoryCommit>> {
    let session = state.session(&worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::rewrite::range(&session, &target, &expected_head)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.rewritePreflightInterrupted",
            "检查本地改动时中断：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn rewrite_preflight(
    state: State<'_, AppState>,
    worktree_id: String,
    oids: Vec<String>,
    reset_target: Option<String>,
) -> Result<crate::rewrite::RewritePreflight> {
    let session = state.session(&worktree_id)?;
    let settings = settings_snapshot(&state)?;
    let github = Arc::clone(&state.m7_github);
    tauri::async_runtime::spawn_blocking(move || {
        let mut result = match reset_target {
            Some(target) => crate::rewrite::reset_preflight(&session, &target)?,
            None => crate::rewrite::preflight(&session, &oids)?,
        };
        result.protected_branch = crate::m7::github::protection::history_branch(&session, &github, &settings.protected_branches)?;
        Ok(result)
    })
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.rewritePreflightInterrupted",
                "检查本地改动时中断：{error}",
                &[("error", error.to_string())],
            )
        })?
}

#[tauri::command]
pub async fn rewrite_cherry_pick(
    state: State<'_, AppState>,
    worktree_id: String,
    request: crate::rewrite::CherryPickRequest,
) -> Result<crate::rewrite::RewriteOutcome> {
    let session = state.session(&worktree_id)?;
    let recovery = state.recovery.clone();
    let archive = state.stash_archive.clone();
    let autostash = state.autostash.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::rewrite::cherry_pick(&session, &recovery, &autostash, &archive, &request)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.cherryPickInterrupted",
            "cherry-pick 时中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn rewrite_revert(
    state: State<'_, AppState>,
    worktree_id: String,
    request: crate::rewrite::RevertRequest,
) -> Result<crate::rewrite::RewriteOutcome> {
    let session = state.session(&worktree_id)?;
    let recovery = state.recovery.clone();
    let archive = state.stash_archive.clone();
    let autostash = state.autostash.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::rewrite::revert(&session, &recovery, &autostash, &archive, &request)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.revertInterrupted",
            "revert 时中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn rewrite_reset(
    state: State<'_, AppState>,
    worktree_id: String,
    request: crate::rewrite::ResetRequest,
) -> Result<crate::rewrite::RewriteOutcome> {
    let session = state.session(&worktree_id)?;
    let recovery = state.recovery.clone();
    let archive = state.stash_archive.clone();
    let autostash = state.autostash.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::rewrite::reset(&session, &recovery, &autostash, &archive, &request)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.resetInterrupted",
            "reset 时中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewordCommand {
    pub target: String,
    pub expected_context: String,
    pub expected_state: String,
    pub expected_head: String,
    pub message: String,
}

#[tauri::command]
pub async fn rewrite_reword(
    state: State<'_, AppState>,
    worktree_id: String,
    request: RewordCommand,
) -> Result<crate::rewrite::RewriteOutcome> {
    let session = state.session(&worktree_id)?;
    let recovery = state.recovery.clone();
    let archive = state.stash_archive.clone();
    let data_dir = state.data_dir.clone();
    let autostash = state.autostash.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::rewrite::reword(
            &session,
            &recovery,
            &autostash,
            &archive,
            &data_dir,
            &request.target,
            &request.expected_head,
            &request.expected_context,
            &request.expected_state,
            &request.message,
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.rewordInterrupted",
            "改写提交说明时中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn rewrite_interactive(
    state: State<'_, AppState>,
    worktree_id: String,
    request: crate::rewrite::InteractiveRebaseRequest,
) -> Result<crate::rewrite::RewriteOutcome> {
    let session = state.session(&worktree_id)?;
    let recovery = state.recovery.clone();
    let archive = state.stash_archive.clone();
    let data_dir = state.data_dir.clone();
    let autostash = state.autostash.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::rewrite::interactive(
            &session, &recovery, &autostash, &archive, &data_dir, &request,
        )
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.interactiveRebaseInterrupted",
            "交互式 rebase 时中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn file_history(
    state: State<'_, AppState>,
    worktree_id: String,
    request: crate::file_history::FileHistoryRequest,
) -> Result<crate::file_history::FileHistoryPage> {
    let session = state.session(&worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::file_history::history(&session.git, &request)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.fileHistoryInterrupted",
            "读取文件历史时中断：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn file_blame(
    state: State<'_, AppState>,
    worktree_id: String,
    request: crate::file_history::BlameRequest,
) -> Result<crate::file_history::BlameResult> {
    let session = state.session(&worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || crate::file_history::blame(&session.git, &request))
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.fileBlameInterrupted",
                "读取 blame 时中断：{error}",
                &[("error", error.to_string())],
            )
        })?
}

#[tauri::command]
pub async fn file_previous_revision(
    state: State<'_, AppState>,
    worktree_id: String,
    oid: String,
    path: String,
) -> Result<Option<crate::file_history::FileRevision>> {
    let session = state.session(&worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::file_history::previous_revision(&session.git, &oid, &path)
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.filePreviousRevisionInterrupted",
            "读取上一版本时中断：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn external_tool_choices(
    state: State<'_, AppState>,
    request: WorktreeRequest,
) -> Result<crate::external_tools::ExternalToolSetup> {
    let session = state.session(&request.worktree_id)?;
    let settings = {
        let registry = state.registry.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.repositoryListLockFailed11",
                "仓库列表锁失败",
                &[],
            )
        })?;
        registry.settings().clone()
    };
    let configured_diff = settings.diff_tool.clone();
    let configured_merge = settings.merge_tool.clone();
    tauri::async_runtime::spawn_blocking(
        move || -> Result<crate::external_tools::ExternalToolSetup> {
            let choices = crate::external_tools::choices(
                &session.git,
                configured_diff.as_deref(),
                configured_merge.as_deref(),
            );
            Ok(crate::external_tools::ExternalToolSetup {
                choices,
                context: crate::external_tools::confirmation_context(&session)?,
            })
        },
    )
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.commands.externalToolChoicesInterrupted",
            "读取外部工具列表时中断：{error}",
            &[("error", error.to_string())],
        )
    })?
}

#[tauri::command]
pub async fn external_tool_launch(
    state: State<'_, AppState>,
    worktree_id: String,
    request: crate::external_tools::ExternalToolRequest,
) -> Result<crate::external_tools::ExternalToolOutcome> {
    let session = state.session(&worktree_id)?;
    let recovery = state.recovery.clone();
    let settings = {
        let registry = state.registry.lock().map_err(|_| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.commands.repositoryListLockFailed12",
                "仓库列表锁失败",
                &[],
            )
        })?;
        registry.settings().clone()
    };
    let configured = match request.kind {
        crate::external_tools::ExternalToolKind::Diff => settings.diff_tool,
        crate::external_tools::ExternalToolKind::Merge => settings.merge_tool,
    };
    tauri::async_runtime::spawn_blocking(move || {
        crate::external_tools::launch(&session, &recovery, &request, configured.as_deref())
    })
    .await
    .map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.commands.externalToolInterrupted",
            "外部工具运行时中断，结果待核对：{error}",
            &[("error", error.to_string())],
        )
    })?
}

/// First-run language when settings has no explicit choice.
#[tauri::command]
pub async fn system_language() -> Result<String> {
    Ok(crate::settings::system_language())
}

#[cfg(test)]
mod measurement_log_tests {
    use super::append_measure_line;
    use std::sync::{Arc, Barrier};

    #[test]
    fn concurrent_measurement_records_remain_complete() {
        // 仅自建临时日志；不改进程环境，也不打开用户的测量文件。
        let directory = tempfile::tempdir().unwrap();
        let path = Arc::new(directory.path().join("measure.log"));
        let barrier = Arc::new(Barrier::new(8));
        let threads: Vec<_> = (0..8).map(|worker| {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                for index in 0..32 {
                    let payload = serde_json::json!({"worker": worker, "index": index, "text": "x".repeat(2048)});
                    append_measure_line(&path, &payload.to_string());
                    append_measure_line(&path, &format!("frontend-{worker}-{index}"));
                }
            })
        }).collect();
        for thread in threads { thread.join().unwrap(); }
        let text = std::fs::read_to_string(&*path).unwrap();
        let mut records = std::collections::HashSet::new();
        let mut frontend = std::collections::HashSet::new();
        for line in text.lines() {
            if line.starts_with('{') {
                let value: serde_json::Value = serde_json::from_str(line).unwrap();
                assert_eq!(value["text"].as_str().unwrap().len(), 2048);
                assert!(records.insert((value["worker"].as_u64().unwrap(), value["index"].as_u64().unwrap())));
            } else {
                assert!(frontend.insert(line.to_owned()));
            }
        }
        assert_eq!(records.len(), 8 * 32);
        assert_eq!(frontend.len(), 8 * 32);
        assert!(text.ends_with('\n'));
        for worker in 0..8 {
            for index in 0..32 {
                assert!(frontend.contains(&format!("frontend-{worker}-{index}")));
            }
        }
    }
}
