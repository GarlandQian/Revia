//! Tauri 命令层：前端只能调用这里固定、带类型的操作，不能传任意 Git 参数。

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::diff::{self, FileChange};
use crate::env::{self, EnvCache, EnvCaptureInfo, ShellEnv};
use crate::error::{AppError, Result};
use crate::git::{self, GitCommand};
use crate::highlight::{self, HighlightRequest, HighlightResult, Highlighter};

pub struct AppState {
    pub cache: EnvCache,
    /// syntect 解析状态由专用线程独占。
    pub highlighter: Highlighter,
}

impl AppState {
    pub fn new() -> Self {
        Self { cache: EnvCache::new(), highlighter: Highlighter::start() }
    }

    fn shell_env(&self, home: &std::path::Path, worktree: &std::path::Path) -> ShellEnv {
        self.cache.get(home, worktree, false)
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRequest {
    pub worktree: String,
    #[serde(default)]
    pub reload_env: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfo {
    pub worktree: String,
    pub git_path: String,
    pub git_version: String,
    pub env: EnvCaptureInfo,
}

/// 解析一个 worktree 的运行环境，并报告实际生效的 git 程序与版本。
#[tauri::command]
pub async fn resolve_worktree(
    state: State<'_, AppState>,
    request: WorktreeRequest,
) -> Result<WorktreeInfo> {
    let home = env::home_dir()?;
    let worktree = canonical_dir(&request.worktree)?;
    let shell_env = state.cache.get(&home, &worktree, request.reload_env);
    let git = GitCommand::new(&worktree, &shell_env)?;
    let version = git.version()?;
    Ok(WorktreeInfo {
        worktree: worktree.display().to_string(),
        git_path: git.program.display().to_string(),
        git_version: version,
        env: shell_env.info.clone(),
    })
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRequest {
    pub worktree: String,
    /// 目标文件；为空时返回整个 worktree 的改动。
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub staged: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffResponse {
    pub worktree: String,
    pub staged: bool,
    /// 用户看到的这份 diff 的指纹；后续写操作必须携带（ADR-0005）。
    pub fingerprint: String,
    pub files: Vec<FileChange>,
    /// Rust 侧解析耗时（毫秒），首屏三段计时之一。
    pub parse_ms: u64,
    pub git_path: String,
    pub git_version: String,
    pub env: EnvCaptureInfo,
}

/// 读取并解析 diff：hunk 来自 Git（ADR-0004），这里的解析只是读取。
#[tauri::command]
pub async fn get_diff(state: State<'_, AppState>, request: DiffRequest) -> Result<DiffResponse> {
    let home = env::home_dir()?;
    let worktree = canonical_dir(&request.worktree)?;
    let shell_env = state.shell_env(&home, &worktree);
    let git = GitCommand::new(&worktree, &shell_env)?;
    let git_version = git.version()?;

    let started = std::time::Instant::now();
    let raw = git.diff(request.path.as_deref(), request.staged)?;
    let fingerprint = git::fingerprint(&raw);
    let patch = String::from_utf8_lossy(&raw).into_owned();
    let parsed = diff::parse(&patch);
    let parse_ms = started.elapsed().as_millis() as u64;

    Ok(DiffResponse {
        worktree: worktree.display().to_string(),
        staged: request.staged,
        fingerprint,
        files: parsed.files,
        parse_ms,
        git_path: git.program.display().to_string(),
        git_version,
        env: shell_env.info.clone(),
    })
}

const MAX_LINES_PER_FILE: usize = 20_000;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileVersionRequest {
    pub worktree: String,
    pub path: String,
    /// 相对 HEAD 修改过的文件；为真时也检查文件是否新增。
    #[serde(default)]
    pub check_introduced: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileVersionResponse {
    pub head: Option<String>,
    pub working: Option<String>,
    pub staged: Option<String>,
    pub truncated: bool,
}

/// 读取一个文件的三个版本（HEAD / 暂存区 / 工作区），供高亮使用。
#[tauri::command]
pub async fn get_file_versions(
    state: State<'_, AppState>,
    request: FileVersionRequest,
) -> Result<FileVersionResponse> {
    let home = env::home_dir()?;
    let worktree = canonical_dir(&request.worktree)?;
    let shell_env = state.shell_env(&home, &worktree);
    let git = GitCommand::new(&worktree, &shell_env)?;

    let mut head = git.file_at("HEAD", &request.path);
    if head.is_none() && request.check_introduced {
        if let Some(sha) = git.introduction_sha(&request.path) {
            head = git.file_at(&sha, &request.path);
        }
    }
    let staged = git.file_at("", &request.path);
    let working = git.working_file(&request.path);

    let mut truncated = false;
    let mut clamp = |content: Option<String>| -> Option<String> {
        let content = content?;
        let count = content.split_inclusive('\n').count();
        if count > MAX_LINES_PER_FILE {
            truncated = true;
            let kept: String = content.split_inclusive('\n').take(MAX_LINES_PER_FILE).collect();
            Some(kept)
        } else {
            Some(content)
        }
    };

    Ok(FileVersionResponse {
        head: clamp(head),
        working: clamp(working),
        staged: clamp(staged),
        truncated,
    })
}

/// Rust 侧高亮（syntect）：对完整文件内容高亮，按行范围分块返回，前端按行号映射。
#[tauri::command]
pub async fn highlight_syntect(
    state: State<'_, AppState>,
    request: HighlightRequest,
) -> Result<HighlightResult> {
    let highlighter = state.highlighter.clone_handle();
    let debug = std::env::var("REVIA_M0_MEASURE_LOG").ok().map(|path| std::path::PathBuf::from(path).with_extension("log"));
    let label = format!(
        "highlight_syntect 收到：lang={} lines={} range={:?}",
        request.language,
        request.content.split('\n').count(),
        request.line_range.as_ref().map(|range| range.0.clone())
    );
    if let Some(log) = &debug {
        let _ = crate::measure::append_log(log, &label);
    }
    let result = tauri::async_runtime::spawn_blocking(move || highlighter.highlight(request))
        .await
        .map_err(|error| AppError::internal(format!("高亮任务失败：{error}")))?;
    if let Some(log) = &debug {
        match &result {
            Ok(value) => {
                let _ = crate::measure::append_log(
                    log,
                    &format!("highlight_syntect 返回：from={} lines={} elapsed={}ms", value.from, value.lines.len(), value.elapsed_ms),
                );
            }
            Err(error) => {
                let _ = crate::measure::append_log(log, &format!("highlight_syntect 失败：{error}"));
            }
        }
    }
    result
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitSupport {
    pub syntax_extensions: Vec<String>,
    pub theme: String,
}

/// 报告 Rust 侧可用的语法扩展名，便于核对 Vue / Swift 的支持情况。
#[tauri::command]
pub async fn syntect_support() -> Result<GitSupport> {
    Ok(GitSupport {
        syntax_extensions: highlight::supported_extensions(),
        theme: "Solarized (light)".into(),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasureConfig {
    /// 是否由环境变量请求自动测量。
    pub auto_run: bool,
    /// 结果落盘路径；为空表示只显示在窗口里。
    pub output_path: Option<String>,
    /// 环境变量指定的仓库路径。
    pub repo: Option<String>,
    /// 只跑这些组合（`mode:view`），为空表示全部。
    pub only: Option<Vec<String>>,
    /// 自动测量的目标文件；空字符串表示整仓。
    pub path: Option<String>,
}

#[tauri::command]
pub async fn measure_config() -> Result<MeasureConfig> {
    Ok(MeasureConfig {
        auto_run: crate::measure::auto_run_requested(),
        output_path: crate::measure::target_path().map(|path| path.display().to_string()),
        repo: crate::measure::measure_repo(),
        only: crate::measure::measure_only(),
        path: crate::measure::measure_path(),
    })
}

/// 把 GUI 里跑出的测量结果写进文件，供脚本读取。
#[tauri::command]
pub async fn write_measurement(payload: serde_json::Value) -> Result<Option<String>> {
    let fallback = crate::measure::log_path();
    let Some(path) = crate::measure::target_path() else {
        if let Some(log) = fallback {
            let _ = crate::measure::append_log(&log, "write_measurement：没有配置输出路径");
        }
        return Ok(None);
    };
    if let Some(log) = fallback {
        let bytes = serde_json::to_string(&payload).map(|text| text.len()).unwrap_or(0);
        let _ = crate::measure::append_log(
            &log,
            &format!("write_measurement：payload {bytes} 字节 -> {}", path.display()),
        );
    }
    crate::measure::write_json(&path, &payload)?;
    Ok(Some(path.display().to_string()))
}

/// 前端把关键步骤写进日志文件，便于没有窗口时排查。
#[tauri::command]
pub async fn debug_log(message: String) -> Result<()> {
    if let Some(log) = crate::measure::log_path() {
        crate::measure::append_log(&log, &message)?;
    }
    Ok(())
}

/// 只允许既有目录，避免前端传入任意路径。
fn canonical_dir(path: &str) -> Result<PathBuf> {
    let candidate = PathBuf::from(path);
    if !candidate.is_dir() {
        return Err(AppError::rejected(format!("目录不存在：{path}")));
    }
    Ok(candidate)
}
