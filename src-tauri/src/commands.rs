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
use crate::error::{AppError, Result};
use crate::git::highlight::HighlightResult;
use crate::git::rows::ViewMode;
use crate::git::status::ChangesKind;
use crate::git::{CommandLog, CommandRecord};
use crate::session::{ChangeEntry, ChangesSnapshot, FileVersion, RepoEntry, RowSlice, WorktreeSession, WorktreeSessionInfo};
use crate::settings::{Registry, Settings};
use crate::worktree_status::WorktreeStatusEntry;
use crate::recovery::{self, RecoveryInspection, RecoveryState, RecoveryStore, RecoverySummary, RestoreExpectation, WorktreeIdentity};
use crate::commit::{CommitDraft, DraftStore};
use crate::commit_flow::{self, CommitOptions, CommitOutcome, CommitOutputEvent};
use crate::watch::{resolve_roots, WatchController, WatchEvents, WatchStatus};

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
        let blocked = self.state.lock()
            .map_err(|_| AppError::internal("退出保护锁失败"))?.1;
        if blocked {
            return Err(AppError::rejected("仍有写操作或结果待核对，不能退出"));
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
    /// 只监视当前 worktree 的控制器（Q88）。
    pub watch: Arc<WatchController>,
    /// 合并事件的单调进度及激活归属。
    pub watch_events: Arc<Mutex<WatchEvents>>,
    pub acceptance: Option<Arc<Acceptance>>,
    pub recovery: RecoveryStore,
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
    /// 提交操作 ID 计数器（实时输出事件归属用）。
    commit_ops: std::sync::atomic::AtomicU64,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let command_log = Arc::new(CommandLog::default());
        Self {
            recovery: RecoveryStore::new(data_dir.clone()),
            review: Arc::new(crate::review::ReviewStore::new(data_dir.clone())),
            registry: Mutex::new(Registry::load(data_dir.clone(), command_log.clone())),
            sessions: Mutex::new(HashMap::new()),
            command_log,
            watch: Arc::new(WatchController::new()),
            watch_events: Arc::new(Mutex::new(WatchEvents::default())),
            acceptance: None,
            exit_guard: Arc::new(ExitGuard::default()),
            drafts: DraftStore::new(data_dir.clone()),
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
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| AppError::internal("会话锁失败"))?;
        if let Some(session) = sessions.get(worktree_id) {
            return Ok(session.clone());
        }
        let path = {
            let registry = self
                .registry
                .lock()
                .map_err(|_| AppError::internal("仓库列表锁失败"))?;
            registry
                .worktree_path(worktree_id)
                .ok_or_else(|| AppError::rejected("worktree 未登记，请先添加仓库"))?
        };
        let home = crate::shell::home_dir()?;
        let shell_env = crate::shell::resolve(&home, &path);
        // 设置里手动指定的 Git 程序（Q27）；未指定时使用登录 shell PATH 中的 git。
        let git_program = {
            let registry = self
                .registry
                .lock()
                .map_err(|_| AppError::internal("仓库列表锁失败"))?;
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
        let sessions = self.sessions.lock().map_err(|_| AppError::internal("会话锁失败"))?;
        for session in sessions.values() {
            if *session.writing.lock().map_err(|_| AppError::internal("写操作锁失败"))? {
                return Err(AppError::rejected(
                    "有正在进行的写操作，完成后再重新加载环境",
                ));
            }
        }
        drop(sessions);
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.clear();
        }
        Ok(())
    }

    /// 取得某个已登记 worktree 的会话（命令层与生产入口测试共用同一条路径）。
    pub fn worktree_session(&self, worktree_id: &str) -> Result<Arc<WorktreeSession>> {
        self.session(worktree_id)
    }

    /// 释放某个 worktree 的会话（切换离开时用）：停止其高亮线程引用与缓存。
    pub fn release_session(&self, worktree_id: &str) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(worktree_id);
        }
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
        .map_err(|error| AppError::internal(format!("读取冲突列表失败：{error}")))?
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
            let registry = state
                .registry
                .lock()
                .map_err(|_| AppError::internal("仓库列表锁失败"))?;
            registry.settings().merge_auto_apply
        }
    };
    let data_dir = state.data_dir.clone();
    let path = request.path.clone();
    let worktree_id = request.worktree_id.clone();
    tauri::async_runtime::spawn_blocking(move || session.merge_layout(&data_dir, &path, auto_apply))
        .await
        .map_err(|error| AppError::internal(format!("读取合并页签失败：{error}")))?
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
    .map_err(|error| AppError::needs_verification(format!("应用合并结果时中断，结果待核对：{error}")))?
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
        let targets: Vec<(String, String)> =
            request.targets.into_iter().map(|target| (target.path, target.fingerprint)).collect();
        session.merge_accept_side(&targets, request.side, &store)
    })
    .await
    .map_err(|error| AppError::needs_verification(format!("接受一侧时中断，结果待核对：{error}")))?
}

#[tauri::command]
pub async fn commit_warnings(
    state: State<'_, AppState>,
    request: WorktreeRequest,
) -> Result<Vec<crate::commit_warnings::CommitWarning>> {
    let session = state.session(&request.worktree_id)?;
    let settings = {
        let registry = state
            .registry
            .lock()
            .map_err(|_| AppError::internal("仓库列表锁失败"))?;
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
        .map_err(|error| AppError::internal(format!("提交前检查失败：{error}")))?
}

#[tauri::command]
pub async fn list_repos(state: State<'_, AppState>) -> Result<Vec<RepoEntry>> {
    let registry = state
        .registry
        .lock()
        .map_err(|_| AppError::internal("仓库列表锁失败"))?;
    Ok(registry.list())
}

#[tauri::command]
pub async fn list_settings(state: State<'_, AppState>) -> Result<Settings> {
    let registry = state
        .registry
        .lock()
        .map_err(|_| AppError::internal("仓库列表锁失败"))?;
    Ok(registry.settings().clone())
}

/// 唯一的例外：接收用户选择的目录路径，确认是 Git 仓库后登记。
#[tauri::command]
pub async fn add_repo(state: State<'_, AppState>, request: AddRepoRequest) -> Result<RepoEntry> {
    let path = request.path.clone();
    {
        let mut registry = state
            .registry
            .lock()
            .map_err(|_| AppError::internal("仓库列表锁失败"))?;
        registry.add(&path)
    }
}

#[tauri::command]
pub async fn select_repo(state: State<'_, AppState>, request: SelectRepoRequest) -> Result<RepoEntry> {
    let mut registry = state
        .registry
        .lock()
        .map_err(|_| AppError::internal("仓库列表锁失败"))?;
    registry.select(&request.repo_id, request.worktree_id.as_deref())
}

#[tauri::command]
pub async fn set_diff_font_size(state: State<'_, AppState>, size: u32) -> Result<Settings> {
    let mut registry = state
        .registry
        .lock()
        .map_err(|_| AppError::internal("仓库列表锁失败"))?;
    registry.update_font_size(size)
}

/// 修改设置（颜色主题、代码字体与字号、Git 程序路径、合并工具）。
#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    request: crate::settings::SettingsPatch,
) -> Result<Settings> {
    let mut registry = state
        .registry
        .lock()
        .map_err(|_| AppError::internal("仓库列表锁失败"))?;
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
        let registry = state
            .registry
            .lock()
            .map_err(|_| AppError::internal("仓库列表锁失败"))?;
        registry
            .find(&request.repo_id)
            .ok_or_else(|| AppError::rejected("仓库未登记，请先添加"))?
    };
    // 优先使用该仓库已有会话的 Git 上下文（同一套登录 shell 环境），
    // 否则按仓库路径解析一次；不在命令里重复为每个 worktree 启动登录 shell。
    let git = {
        let sessions = state
            .sessions
            .lock()
            .map_err(|_| AppError::internal("会话锁失败"))?;
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
                let registry = state
                    .registry
                    .lock()
                    .map_err(|_| AppError::internal("仓库列表锁失败"))?;
                registry
                    .settings()
                    .git_path
                    .clone()
                    .map(std::path::PathBuf::from)
            };
            crate::git::Git::with_program(&path, &shell_env, state.command_log.clone(), program.as_deref())?
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
    .map_err(|error| AppError::internal(format!("读取 worktree 状态失败：{error}")))
}

#[tauri::command]
pub async fn open_worktree(state: State<'_, AppState>, request: WorktreeRequest) -> Result<WorktreeSessionInfo> {
    log_measure(&format!("open_worktree 收到：id={}", request.worktree_id));
    let session = state.session(&request.worktree_id)?;
    log_measure(&format!("open_worktree 完成：{}", session.info.git_version));
    Ok(session.info.clone())
}

/// 切换当前监视目标：只监视这个 worktree，并释放旧监视（Q88、Q95）。
///
/// `activation` 是前端发起时的激活代号：后端用它拒绝迟到进入的旧激活请求。
#[tauri::command]
pub async fn watch_worktree(state: State<'_, AppState>, request: WatchWorktreeRequest) -> Result<WatchStatus> {
    let session = state.session(&request.worktree_id)?;
    apply_acceptance_watch_failure(&state);
    let events = state.watch_events.clone();
    let status = state.watch.activate(
        resolve_roots(&session.path, session.git_dir_path(), session.common_dir_path()),
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
    let events = state.watch_events.lock().map(|guard| *guard)
        .map_err(|_| AppError::internal("监视事件锁失败"))?;
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
    let Some(path) = std::env::var("REVIA_MEASURE_LOG").ok().map(std::path::PathBuf::from) else {
        return;
    };
    if minimal_soak_logging() {
        return;
    }
    let log = path.with_extension("log");
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
        let _ = writeln!(file, "{} {message}", chrono_like_stamp());
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
pub fn set_review_mark_impl(state: &AppState, request: &ReviewMarkRequest) -> Result<crate::review::ReviewMarks> {
    let session = state.session(&request.worktree_id)?;
    session.set_review_mark(&request.path, request.marked, Some(request.expected_versions.clone()))
}

#[tauri::command]
pub async fn set_review_mark(state: State<'_, AppState>, request: ReviewMarkRequest) -> Result<crate::review::ReviewMarks> {
    let session = state.session(&request.worktree_id)?;
    let path = request.path.clone();
    let marked = request.marked;
    let expected = request.expected_versions.clone();
    let worktree_id = request.worktree_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        session.set_review_mark(&path, marked, Some(expected))
    })
        .await
        .map_err(|error| AppError::internal(format!("保存审阅标记失败：{error}")))?;
    deliver(state.acceptance.as_ref(), "set_review_mark", &worktree_id, None, result).await
}

#[tauri::command]
pub async fn list_changes(state: State<'_, AppState>, request: WorktreeRequest) -> Result<ChangesSnapshot> {
    log_measure("list_changes 收到");
    let session = state.session(&request.worktree_id)?;
    log_measure("list_changes 会话就绪，开始读取");
    let result = tauri::async_runtime::spawn_blocking(move || session.refresh())
        .await
        .map_err(|error| AppError::internal(format!("读取状态失败：{error}")))?;
    if let Ok(inner) = &result {
        log_measure(&format!("list_changes 完成：{} 个文件，{} ms", inner.entries.len(), inner.elapsed_ms));
    }
    deliver(state.acceptance.as_ref(), "list_changes", &request.worktree_id, None, result).await
}

#[tauri::command]
pub async fn stage_file(
    state: State<'_, AppState>,
    request: StageFileRequest,
) -> Result<ChangesSnapshot> {
    log_measure(&format!("stage_file 收到：{} {}", request.worktree_id, request.path));
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        session.stage_file(&request.path, &request.fingerprint)
    })
    .await
    .map_err(|error| AppError::internal(format!("暂存文件失败：{error}")))?;
    deliver(state.acceptance.as_ref(), "stage_file", &context.worktree_id, Some(&context.path), result).await
}

#[tauri::command]
pub async fn unstage_file(
    state: State<'_, AppState>,
    request: UnstageFileRequest,
) -> Result<ChangesSnapshot> {
    log_measure(&format!("unstage_file 收到：{} {}", request.worktree_id, request.path));
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        session.unstage_file(&request.path, &request.fingerprint)
    })
    .await
    .map_err(|error| AppError::internal(format!("取消暂存文件失败：{error}")))?;
    deliver(state.acceptance.as_ref(), "unstage_file", &context.worktree_id, Some(&context.path), result).await
}

#[tauri::command]
pub async fn stage_hunk(state: State<'_, AppState>, request: HunkRequest) -> Result<ChangesSnapshot> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        session.stage_hunk(&request.path, &request.fingerprint, &request.hunk_id)
    })
    .await
    .map_err(|error| AppError::needs_verification(format!("暂存 hunk 任务中断，结果待核对：{error}")))?;
    deliver(state.acceptance.as_ref(), "stage_hunk", &context.worktree_id, Some(&context.path), result).await
}

#[tauri::command]
pub async fn unstage_hunk(state: State<'_, AppState>, request: HunkRequest) -> Result<ChangesSnapshot> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        session.unstage_hunk(&request.path, &request.fingerprint, &request.hunk_id)
    })
    .await
    .map_err(|error| AppError::needs_verification(format!("取消暂存 hunk 任务中断，结果待核对：{error}")))?;
    deliver(state.acceptance.as_ref(), "unstage_hunk", &context.worktree_id, Some(&context.path), result).await
}

async fn execute_discard(state: State<'_, AppState>, request: DiscardRequest, hunk: bool) -> Result<ChangesSnapshot> {
    if hunk != request.hunk_id.is_some() { return Err(AppError::rejected("丢弃目标范围不匹配")); }
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let store = state.recovery.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        if let Some(id) = request.hunk_id {
            session.discard_hunk(&request.path, &request.fingerprint, &request.snapshot_revision, &id, &store)
        } else {
            session.discard_file(&request.path, &request.fingerprint, &request.snapshot_revision, &store)
        }
    }).await.map_err(|error| AppError::needs_verification(format!("丢弃任务中断，结果待核对：{error}")))?;
    deliver(state.acceptance.as_ref(), if hunk { "discard_hunk" } else { "discard_file" }, &context.worktree_id, Some(&context.path), result).await
}

#[tauri::command]
pub async fn stage_files(
    state: State<'_, AppState>,
    request: BatchRequest,
) -> Result<BatchOutcome> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        session.stage_files(&request.targets, true)
    })
    .await
    .map_err(|error| AppError::internal(format!("批量暂存失败：{error}")))?;
    deliver(state.acceptance.as_ref(), "stage_files", &context.worktree_id, None, Ok(result)).await
}

#[tauri::command]
pub async fn unstage_files(
    state: State<'_, AppState>,
    request: BatchRequest,
) -> Result<BatchOutcome> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        session.stage_files(&request.targets, false)
    })
    .await
    .map_err(|error| AppError::internal(format!("批量取消暂存失败：{error}")))?;
    deliver(state.acceptance.as_ref(), "unstage_files", &context.worktree_id, None, Ok(result)).await
}

#[tauri::command]
pub async fn discard_files(
    state: State<'_, AppState>,
    request: BatchRequest,
) -> Result<BatchOutcome> {
    let session = state.session(&request.worktree_id)?;
    let store = state.recovery.clone();
    let revision = request
        .snapshot_revision
        .clone()
        .ok_or_else(|| AppError::rejected("批量丢弃缺少确认时的快照版本"))?;
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        session.discard_files(&request.targets, &revision, &store)
    })
    .await
    .map_err(|error| AppError::needs_verification(format!("批量丢弃任务中断，结果待核对：{error}")))?;
    deliver(state.acceptance.as_ref(), "discard_files", &context.worktree_id, None, Ok(result)).await
}

#[tauri::command]
pub async fn discard_file(state: State<'_, AppState>, request: DiscardRequest) -> Result<ChangesSnapshot> {
    execute_discard(state, request, false).await
}

#[tauri::command]
pub async fn discard_hunk(state: State<'_, AppState>, request: DiscardRequest) -> Result<ChangesSnapshot> {
    execute_discard(state, request, true).await
}

/// 列出回收记录的实际执行函数：命令与测试共用。
///
/// 原 worktree 可能已被移动或删除：取不到 session 时仍要能列出记录（Q87）。
pub fn list_recovery_impl(state: &AppState, worktree_id: Option<&str>) -> Result<Vec<RecoverySummary>> {
    let session = worktree_id.and_then(|id| state.session(id).ok());
    let identity = session.and_then(|session| WorktreeIdentity::capture(&session.git).ok());
    state.recovery.list(identity.as_ref())
}

#[tauri::command]
pub async fn list_recovery(state: State<'_, AppState>, worktree_id: Option<String>) -> Result<Vec<RecoverySummary>> {
    // 原 worktree 可能已被移动或删除：取不到 session 时仍要能列出记录（Q87）。
    let session = worktree_id.as_deref().and_then(|id| state.session(id).ok());
    let store = state.recovery.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Unavailable/replaced worktrees must not prevent viewing their saved records.
        let identity = session.and_then(|session| WorktreeIdentity::capture(&session.git).ok());
        store.list(identity.as_ref())
    }).await.map_err(|error| AppError::internal(format!("读取回收区失败：{error}")))?
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
pub fn inspect_recovery_impl(state: &AppState, request: &RecoveryInspectRequest) -> Result<RecoveryInspection> {
    match state.session(&request.worktree_id) {
        Ok(session) => session.inspect_record(&request.record_id, &state.recovery),
        Err(_) => state.recovery.inspect(&request.record_id, None, None),
    }
}

/// 只读检查一条记录能否直接恢复，并给出对比所需的两侧状态。
#[tauri::command]
pub async fn inspect_recovery(state: State<'_, AppState>, request: RecoveryInspectRequest) -> Result<RecoveryInspection> {
    let store = state.recovery.clone();
    let session = state.session(&request.worktree_id).ok();
    tauri::async_runtime::spawn_blocking(move || match session {
        Some(session) => session.inspect_record(&request.record_id, &store),
        // 当前 worktree 不可用：仍返回保存内容，当前侧标为不可用。
        None => store.inspect(&request.record_id, None, None),
    })
    .await
    .map_err(|error| AppError::internal(format!("读取回收记录失败：{error}")))?
}

/// 恢复的实际执行函数：命令与测试共用。
pub fn restore_recovery_impl(state: &AppState, request: &RecoveryRestoreRequest) -> Result<recovery::RestoreOutcome> {
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
pub async fn restore_recovery(state: State<'_, AppState>, request: RecoveryRestoreRequest) -> Result<recovery::RestoreOutcome> {
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
    .map_err(|error| AppError::needs_verification(format!("恢复任务中断，结果待核对：{error}")))?;
    deliver(state.acceptance.as_ref(), "restore_recovery", &context.worktree_id, None, result).await
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
pub fn resolve_save_as_owner(state: &AppState, destination: &std::path::Path) -> Result<SaveAsOwner> {
    let registry = state
        .registry
        .lock()
        .map_err(|_| AppError::internal("仓库列表锁失败"))?;
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
pub async fn recovery_save_as_owner(state: State<'_, AppState>, request: SaveAsOwnerRequest) -> Result<SaveAsOwner> {
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
        let registry = state
            .registry
            .lock()
            .map_err(|_| AppError::internal("仓库列表锁失败"))?;
        save_as_owner(&registry, &destination)
    };
    // 归属必须与前端解析到的一致：界面按 A 进的门禁，不能在 B 上落盘（Q90）。
    if owner != request.expected_owner {
        return Err(AppError::rejected(
            "另存目标的归属已变化，请重新选择路径后再试",
        ));
    }
    let session = match &owner {
        Some(worktree_id) => Some(state.session(worktree_id)?),
        None => None,
    };
    execute_save_as(session.as_ref(), &state.recovery, &request.record_id, &request.path, &destination)
}

/// 把保存版本另存到尚不存在的新路径。
///
/// 目标落在已登记 worktree 里时，写入归属**那个** worktree：取得它的非排队写锁，
/// 并把结果交回它的结果通道；外部新路径不强行加锁，但仍逐级校验路径边界（Q90、T14）。
#[tauri::command]
pub async fn save_recovery_as(state: State<'_, AppState>, request: RecoverySaveAsRequest) -> Result<u64> {
    let destination = std::path::PathBuf::from(&request.destination);
    let owner = {
        let registry = state
            .registry
            .lock()
            .map_err(|_| AppError::internal("仓库列表锁失败"))?;
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
            return Err(AppError::rejected("另存目标的归属已变化，请重新选择路径后再试"));
        }
        execute_save_as(session.as_ref(), &store, &context.record_id, &context.path, &destination)
    })
    .await
    .map_err(|error| AppError::needs_verification(format!("另存任务中断，结果待核对：{error}")))?;
    deliver(state.acceptance.as_ref(), "save_recovery_as", deliver_owner.as_deref().unwrap_or(""), None, result).await
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
pub async fn recovery_text_diff(state: State<'_, AppState>, request: RecoveryDiffRequest) -> Result<String> {
    let session = state.session(&request.worktree_id)?;
    let store = state.recovery.clone();
    tauri::async_runtime::spawn_blocking(move || session.recovery_text_diff(&request.record_id, &request.path, &store))
        .await
        .map_err(|error| AppError::internal(format!("生成对比失败：{error}")))?
}

/// 清空回收区（在途记录保留）。
#[tauri::command]
pub async fn clear_recovery(state: State<'_, AppState>) -> Result<u64> {
    let store = state.recovery.clone();
    tauri::async_runtime::spawn_blocking(move || store.clear())
        .await
        .map_err(|error| AppError::internal(format!("清空回收区失败：{error}")))?
}

/// 启动与周期自动清理：由 `lib.rs` 在 setup 与后台线程调用。
pub fn purge_expired_now(store: &RecoveryStore) -> Result<u64> {
    store.auto_purge()
}

/// 读取自动清理状态：普通模式下界面据此显示持久失败提示（Q9）。
#[tauri::command]
pub async fn recovery_maintenance(state: State<'_, AppState>) -> Result<recovery::RecoveryMaintenance> {
    Ok(state.recovery.maintenance())
}

/// 删除超过保留期的记录（启动时与手动清理共用）。
#[tauri::command]
pub async fn purge_recovery(state: State<'_, AppState>) -> Result<u64> {
    let store = state.recovery.clone();
    tauri::async_runtime::spawn_blocking(move || store.purge_expired())
        .await
        .map_err(|error| AppError::internal(format!("清理过期记录失败：{error}")))?
}

#[tauri::command]
pub async fn get_rows(state: State<'_, AppState>, request: RowsRequest) -> Result<RowSlice> {
    log_measure(&format!(
        "get_rows 收到：{} {}..{}",
        request.path, request.from, request.to
    ));
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
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
    .map_err(|error| AppError::internal(format!("取显示行失败：{error}")))?;
    deliver(state.acceptance.as_ref(), "get_rows", &context.worktree_id, Some(&context.path), result).await
}

/// 左右对照布局（返修 A1/Q138）：两侧行数、段、折叠区与段操作目标。
#[tauri::command]
pub async fn side_by_side_layout(
    state: State<'_, AppState>,
    request: SideLayoutRequest,
) -> Result<crate::session::SideBySideLayout> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        session.side_layout(
            &request.path,
            request.changes,
            request.ignore_whitespace,
            request.context_lines,
        )
    })
    .await
    .map_err(|error| AppError::internal(format!("读取左右对照布局失败：{error}")))?;
    deliver(state.acceptance.as_ref(), "side_by_side_layout", &context.worktree_id, Some(&context.path), result).await
}

/// 左右对照取行（返修 A1/Q138）：按需取一侧的行、改动标记与词级区间。
#[tauri::command]
pub async fn side_lines(
    state: State<'_, AppState>,
    request: SideLinesRequest,
) -> Result<Vec<crate::git::segments::SideLine>> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
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
    .map_err(|error| AppError::internal(format!("取左右对照的行失败：{error}")))?;
    deliver(state.acceptance.as_ref(), "side_lines", &context.worktree_id, Some(&context.path), result).await
}

/// 暂存一段（块）（返修 A1/Q142）：补丁由 Rust 从同一个 Git hunk 按段切开。
#[tauri::command]
pub async fn stage_segment(
    state: State<'_, AppState>,
    request: SegmentRequest,
) -> Result<ChangesSnapshot> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        session.execute_segment_op(
            &request.path,
            &request.credential,
            request.hunk_index,
            request.segment_index,
            true,
            request.ignore_whitespace,
        )
    })
    .await
    .map_err(|error| AppError::needs_verification(format!("暂存块任务中断，结果待核对：{error}")))?;
    deliver(state.acceptance.as_ref(), "stage_segment", &context.worktree_id, Some(&context.path), result).await
}

/// 取消暂存一段（块）（返修 A1/Q142）。
#[tauri::command]
pub async fn unstage_segment(
    state: State<'_, AppState>,
    request: SegmentRequest,
) -> Result<ChangesSnapshot> {
    let session = state.session(&request.worktree_id)?;
    let context = request.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        session.execute_segment_op(
            &request.path,
            &request.credential,
            request.hunk_index,
            request.segment_index,
            false,
            request.ignore_whitespace,
        )
    })
    .await
    .map_err(|error| AppError::needs_verification(format!("取消暂存块任务中断，结果待核对：{error}")))?;
    deliver(state.acceptance.as_ref(), "unstage_segment", &context.worktree_id, Some(&context.path), result).await
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
    let result = tauri::async_runtime::spawn_blocking(move || {
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
    .await
    .map_err(|error| AppError::needs_verification(format!("丢弃块任务中断，结果待核对：{error}")))?;
    deliver(state.acceptance.as_ref(), "discard_segment", &context.worktree_id, Some(&context.path), result).await
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
pub async fn highlight_text(state: State<'_, AppState>, request: HighlightTextRequest) -> Result<HighlightResult> {
    let session = state.session(&request.worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || session.highlight_text(request.request))
        .await
        .map_err(|error| AppError::internal(format!("高亮失败：{error}")))?
}

#[tauri::command]
pub async fn highlight_range(state: State<'_, AppState>, request: HighlightRequestPayload) -> Result<HighlightResult> {
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
    .map_err(|error| AppError::internal(format!("高亮失败：{error}")))?;
    deliver(state.acceptance.as_ref(), "highlight_range", &context.worktree_id, Some(&context.path), result).await
}

#[tauri::command]
pub async fn row_index_for_line(state: State<'_, AppState>, request: RowIndexRequest) -> Result<u32> {
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
    .map_err(|error| AppError::internal(format!("换算行号失败：{error}")))?;
    deliver(state.acceptance.as_ref(), "row_index_for_line", &context.worktree_id, Some(&context.path), result).await
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
            .ok().and_then(|value| value.parse().ok()).unwrap_or(90),
        cycles: std::env::var("REVIA_SOAK_DIAG_CYCLES")
            .ok().and_then(|value| value.parse::<u64>().ok()).filter(|value| *value > 0),
        focus: std::env::var("REVIA_SOAK_DIAG_FOCUS").ok()
            .filter(|value| matches!(value.as_str(), "idle" | "ipc" | "highlight" | "channel" | "open" | "remount" | "switch" | "scroll" | "jump" | "view")),
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
    is_soak_run() && (std::env::var("REVIA_SOAK_MINIMAL_LOGGING").as_deref() == Ok("1")
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
    let Some(path) = std::env::var("REVIA_MEASURE_LOG").ok().map(std::path::PathBuf::from) else {
        return Ok(None);
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let text = serde_json::to_string_pretty(&payload)?;
    std::fs::write(&path, text)
        .map_err(|error| AppError::internal(format!("写入测量结果失败：{error}")))?;
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
    let Some(path) = std::env::var("REVIA_MEASURE_LOG").ok().map(std::path::PathBuf::from) else {
        return Ok(());
    };
    let log = path.with_extension("log");
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
        let _ = writeln!(file, "{message}");
    }
    Ok(())
}

/// 供测试与诊断：返回一个文件的列表元数据（不渲染）。
#[tauri::command]
pub async fn change_entry(
    state: State<'_, AppState>,
    request: RowsRequest,
) -> Result<ChangeEntry> {
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
        .map_err(|error| AppError::internal(format!("读取提交上下文失败：{error}")))?
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
        .map_err(|error| AppError::internal(format!("读取最近提交信息失败：{error}")))?
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
        .map_err(|error| AppError::internal(format!("保存提交草稿失败：{error}")))?
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
        .map_err(|error| AppError::internal(format!("清除提交草稿失败：{error}")))?
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteCommitRequest {
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
    tauri::async_runtime::spawn_blocking(move || {
        let emit_app = app.clone();
        let emit: Arc<dyn Fn(CommitOutputEvent) + Send + Sync> =
            Arc::new(move |event: CommitOutputEvent| {
                use tauri::Emitter;
                // 事件发送失败（窗口已关）不影响提交本身；输出仍留在返回值里。
                let _ = emit_app.emit("revia://commit-output", &event);
            });
        session.execute_commit_flow(
            &drafts,
            &recovery,
            hooks_off_root,
            options,
            operation_id,
            emit,
        )
    })
    .await
    .map_err(|error| {
        AppError::needs_verification(format!("提交任务中断，结果待核对：{error}"))
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
