//! Tauri 命令层：前端只能调用这里固定、带类型的操作。
//!
//! 路径一律用 Rust 层登记过的 ID（评审调整 1）：除 `add_repo` 外，不接收任意目录路径；
//! 文件路径必须是当前 `list_changes` 结果里的条目。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::State;

use crate::error::{AppError, Result};
use crate::git::highlight::HighlightResult;
use crate::git::rows::ViewMode;
use crate::git::status::ChangesKind;
use crate::git::{CommandLog, CommandRecord};
use crate::session::{ChangeEntry, ChangesSnapshot, FileVersion, RepoEntry, RowSlice, WorktreeSession, WorktreeSessionInfo};
use crate::settings::{Registry, Settings};

pub struct AppState {
    pub registry: Mutex<Registry>,
    pub sessions: Mutex<HashMap<String, Arc<WorktreeSession>>>,
    pub command_log: Arc<CommandLog>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let command_log = Arc::new(CommandLog::default());
        Self {
            registry: Mutex::new(Registry::load(data_dir, command_log.clone())),
            sessions: Mutex::new(HashMap::new()),
            command_log,
        }
    }

    /// 取出（必要时创建）某个 worktree 的会话。
    fn session(&self, worktree_id: &str) -> Result<Arc<WorktreeSession>> {
        if let Ok(sessions) = self.sessions.lock() {
            if let Some(session) = sessions.get(worktree_id) {
                return Ok(session.clone());
            }
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
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| AppError::internal("会话锁失败"))?;
        sessions.insert(worktree_id.to_string(), session.clone());
        Ok(session)
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

/// 写入测量日志文件（只在设置 REVIA_MEASURE_LOG 时生效）。
pub fn log_measure(message: &str) {
    let Some(path) = std::env::var("REVIA_MEASURE_LOG").ok().map(std::path::PathBuf::from) else {
        return;
    };
    if is_soak_run() && std::env::var("REVIA_SOAK_DIAG_LOGS").as_deref() == Ok("minimal") {
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
    let inner = result?;
    log_measure(&format!("list_changes 完成：{} 个文件，{} ms", inner.entries.len(), inner.elapsed_ms));
    Ok(inner)
}

#[tauri::command]
pub async fn get_rows(state: State<'_, AppState>, request: RowsRequest) -> Result<RowSlice> {
    log_measure(&format!(
        "get_rows 收到：{} {}..{}",
        request.path, request.from, request.to
    ));
    let session = state.session(&request.worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        session.rows(&request.path, request.changes, request.view, request.from, request.to)
    })
    .await
    .map_err(|error| AppError::internal(format!("取显示行失败：{error}")))?
}

#[tauri::command]
pub async fn highlight_range(state: State<'_, AppState>, request: HighlightRequestPayload) -> Result<HighlightResult> {
    log_measure(&format!(
        "highlight_range 收到：{} {:?} {:?} {}..{}",
        request.path, request.changes, request.version, request.from_line, request.to_line
    ));
    let session = state.session(&request.worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        session.highlight(
            &request.path,
            request.changes,
            request.version,
            request.from_line,
            request.to_line,
        )
    })
    .await
    .map_err(|error| AppError::internal(format!("高亮失败：{error}")))?
}

#[tauri::command]
pub async fn row_index_for_line(state: State<'_, AppState>, request: RowIndexRequest) -> Result<u32> {
    let session = state.session(&request.worktree_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        session.row_index_for_line(
            &request.path,
            request.changes,
            request.view,
            request.line,
            request.version,
        )
    })
    .await
    .map_err(|error| AppError::internal(format!("换算行号失败：{error}")))?
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoakDiagnosticConfig {
    pub logging_mode: String,
    pub idle_seconds: u64,
    pub cycles: Option<u64>,
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
    // 自动测量、冒烟检查或长测结束写入结果后退出应用进程，使 `open -n -W` 能正常阻塞并返回。
    if is_measure_auto_run() || is_soak_run() || std::env::var("REVIA_SMOKE").is_ok() {
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
