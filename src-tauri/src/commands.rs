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
use crate::error::{AppError, Result};
use crate::git::highlight::HighlightResult;
use crate::git::rows::ViewMode;
use crate::git::status::ChangesKind;
use crate::git::{CommandLog, CommandRecord};
use crate::session::{ChangeEntry, ChangesSnapshot, FileVersion, RepoEntry, RowSlice, WorktreeSession, WorktreeSessionInfo};
use crate::settings::{Registry, Settings};
use crate::recovery::{self, RecoveryInspection, RecoveryState, RecoveryStore, RecoverySummary, RestoreExpectation, WorktreeIdentity};
use crate::watch::{resolve_roots, WatchController, WatchEvents, WatchStatus};

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
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let command_log = Arc::new(CommandLog::default());
        Self {
            recovery: RecoveryStore::new(data_dir.clone()),
            registry: Mutex::new(Registry::load(data_dir, command_log.clone())),
            sessions: Mutex::new(HashMap::new()),
            command_log,
            watch: Arc::new(WatchController::new()),
            watch_events: Arc::new(Mutex::new(WatchEvents::default())),
            acceptance: None,
        }
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
        let session = WorktreeSession::open(worktree_id, &path, shell_env, self.command_log.clone())?;
        sessions.insert(worktree_id.to_string(), session.clone());
        Ok(session)
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
    pub from: u32,
    pub to: u32,
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
pub struct DiscardRequest {
    pub worktree_id: String,
    pub path: String,
    pub fingerprint: String,
    pub snapshot_revision: String,
    pub hunk_id: Option<String>,
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
        session.rows(&request.path, request.changes, request.view, request.from, request.to)
    })
    .await
    .map_err(|error| AppError::internal(format!("取显示行失败：{error}")))?;
    deliver(state.acceptance.as_ref(), "get_rows", &context.worktree_id, Some(&context.path), result).await
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
        session.row_index_for_line(
            &request.path,
            request.changes,
            request.view,
            request.line,
            request.version,
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
    Ok(MeasureConfig {
        logging: std::env::var("REVIA_MEASURE_LOG").is_ok(),
        auto_run: is_measure_auto_run(),
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
        smoke: std::env::var("REVIA_SMOKE").is_ok(),
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
