//! 克隆仓库（M7 任务 05、Q179）。
//!
//! 克隆是长时间操作：真实调用系统 Git CLI，把 stderr 的进度行流式发给界面，
//! 支持取消；成功后才登记到仓库列表。
//!
//! 失败或取消都**不删除**目标目录：残留内容可能包含用户想保留的部分克隆，
//! 界面显示实际目标与状态，由用户决定是否清理（Q179）。
//!
//! 所有参数都做形状校验并放进独立 argv，不拼 shell 字符串（ADR-0002）。

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};

/// 克隆参数（已经过界面确认）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneRequest {
    /// 远端 URL（HTTPS / SSH / 本地路径都允许，但必须是单个参数）。
    pub url: String,
    /// 目标目录：绝对路径；不存在时其父目录必须存在。
    pub target: String,
    /// 只克隆某个分支。
    pub branch: Option<String>,
    /// 浅克隆深度（`None` 表示完整克隆）。
    pub depth: Option<u32>,
    /// 是否递归克隆子模块。
    pub recurse_submodules: bool,
}

/// 克隆进度事件。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CloneProgress {
    pub job_id: u64,
    /// Git 输出的一行（已去掉控制字符与进度回车）。
    pub line: String,
}

/// 克隆结束事件。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CloneFinished {
    pub job_id: u64,
    /// 是否由用户取消。
    pub cancelled: bool,
    /// 是否成功（成功才登记）。
    pub success: bool,
    /// 实际目标目录；失败或取消时也给出，方便用户查看残留内容。
    pub target: String,
    /// 成功时是登记用的目标目录。
    pub path: Option<String>,
    /// 失败时的 Git 输出尾部（用于展示原因）。
    pub message: Option<String>,
}

/// 一个进行中的克隆任务。
struct CloneJob {
    cancelled: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

/// 克隆任务表（进程内）。
pub struct CloneManager {
    jobs: Mutex<HashMap<u64, CloneJob>>,
    counter: AtomicU64,
    /// H8：设置里手动指定的 Git 程序（Q27）；None 时与统一执行器一致用 PATH。
    git_program: Option<std::path::PathBuf>,
    /// H8：统一命令日志。
    log: Arc<crate::git::CommandLog>,
}

impl Default for CloneManager {
    fn default() -> Self {
        Self::new(None, Arc::new(crate::git::CommandLog::default()))
    }
}

impl CloneManager {
    pub fn new(git_program: Option<std::path::PathBuf>, log: Arc<crate::git::CommandLog>) -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
            counter: AtomicU64::new(1),
            git_program,
            log,
        }
    }

    /// 启动克隆：立即返回 job id，进度与结果走事件。
    pub fn start(
        self: &Arc<Self>,
        app: tauri::AppHandle,
        request: CloneRequest,
        shell_env: crate::shell::ShellEnv,
    ) -> Result<u64> {
        let validated = validate(&request)?;
        let job_id = self.counter.fetch_add(1, Ordering::Relaxed);
        let cancelled = Arc::new(AtomicBool::new(false));
        let child = Arc::new(Mutex::new(None));
        self.jobs.lock().unwrap_or_else(|error| error.into_inner()).insert(
            job_id,
            CloneJob {
                cancelled: Arc::clone(&cancelled),
                child: Arc::clone(&child),
            },
        );
        let manager = Arc::clone(self);
        let git_program = self.git_program.clone();
        let log = Arc::clone(&self.log);
        std::thread::spawn(move || {
            run(app, job_id, validated, shell_env, cancelled, child, git_program, log);
            manager
                .jobs
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&job_id);
        });
        Ok(job_id)
    }

    pub fn cancel(&self, job_id: u64) -> Result<()> {
        let jobs = self.jobs.lock().unwrap_or_else(|error| error.into_inner());
        let job = jobs.get(&job_id).ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.m7.cloneJobUnknown",
                "克隆任务已结束或不存在",
                &[],
            )
        })?;
        job.cancelled.store(true, Ordering::Relaxed);
        if let Some(child) = job
            .child
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_mut()
        {
            let _ = child.kill();
        }
        Ok(())
    }

    pub fn active(&self) -> Vec<u64> {
        let jobs = self.jobs.lock().unwrap_or_else(|error| error.into_inner());
        let mut ids: Vec<u64> = jobs.keys().copied().collect();
        ids.sort_unstable();
        ids
    }
}

struct ValidatedClone {
    url: String,
    target: PathBuf,
    branch: Option<String>,
    depth: Option<u32>,
    recurse_submodules: bool,
}

fn validate(request: &CloneRequest) -> Result<ValidatedClone> {
    let url = request.url.trim();
    if url.is_empty() {
        return Err(rejected("errors.m7.cloneUrlEmpty", "请填写仓库地址"));
    }
    if url.len() > 2048 {
        return Err(rejected("errors.m7.cloneUrlTooLong", "仓库地址过长"));
    }
    // 以 `-` 开头会被 Git 当成选项；控制字符会破坏输出与参数边界。
    if url.starts_with('-') || url.chars().any(|ch| ch.is_control()) {
        return Err(rejected(
            "errors.m7.cloneUrlInvalid",
            "仓库地址含有非法字符",
        ));
    }
    let target_text = request.target.trim();
    if target_text.is_empty() {
        return Err(rejected("errors.m7.cloneTargetEmpty", "请选择克隆目标目录"));
    }
    let target = PathBuf::from(target_text);
    if !target.is_absolute() {
        return Err(rejected(
            "errors.m7.cloneTargetNotAbsolute",
            "克隆目标必须是绝对路径",
        ));
    }
    if target.exists() {
        if !target.is_dir() {
            return Err(rejected(
                "errors.m7.cloneTargetNotDirectory",
                "克隆目标已存在且不是目录",
            ));
        }
        let mut entries = std::fs::read_dir(&target)?;
        if entries.next().is_some() {
            return Err(rejected(
                "errors.m7.cloneTargetNotEmpty",
                "克隆目标目录必须为空",
            ));
        }
    } else {
        let parent = target.parent().ok_or_else(|| {
            rejected("errors.m7.cloneTargetParentMissing", "克隆目标的父目录不存在")
        })?;
        if !parent.is_dir() {
            return Err(rejected(
                "errors.m7.cloneTargetParentMissing",
                "克隆目标的父目录不存在",
            ));
        }
    }
    let branch = match request.branch.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(name) => {
            if name.starts_with('-')
                || name.len() > 255
                || name.chars().any(|ch| ch.is_control() || ch.is_whitespace())
            {
                return Err(rejected(
                    "errors.m7.cloneBranchInvalid",
                    "分支名含有非法字符",
                ));
            }
            Some(name.to_string())
        }
    };
    let depth = match request.depth {
        None | Some(0) => None,
        Some(depth) if depth > 1000 => {
            return Err(rejected(
                "errors.m7.cloneDepthInvalid",
                "浅克隆深度必须在 1 到 1000 之间",
            ))
        }
        Some(depth) => Some(depth),
    };
    Ok(ValidatedClone {
        url: url.to_string(),
        target,
        branch,
        depth,
        recurse_submodules: request.recurse_submodules,
    })
}

fn rejected(code: &'static str, message: &'static str) -> AppError {
    AppError::coded(ErrorKind::Rejected, code, message, &[])
}

fn run(
    app: tauri::AppHandle,
    job_id: u64,
    request: ValidatedClone,
    shell_env: crate::shell::ShellEnv,
    cancelled: Arc<AtomicBool>,
    child_slot: Arc<Mutex<Option<Child>>>,
    git_program: Option<std::path::PathBuf>,
    log: Arc<crate::git::CommandLog>,
) {
    use tauri::Emitter;
    // H8：程序选择与统一 Git 执行器同源（设置路径优先，否则 PATH 里的 git）。
    let program = git_program
        .clone()
        .or_else(|| shell_env.which("git"))
        .unwrap_or_else(|| std::path::PathBuf::from("git"));
    let program_text = program.to_string_lossy().into_owned();
    let started = std::time::Instant::now();
    let mut command = Command::new(&program);
    command
        .arg("clone")
        .arg("--progress")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(branch) = &request.branch {
        command.arg("--branch").arg(branch);
    }
    if let Some(depth) = request.depth {
        command.arg("--depth").arg(depth.to_string());
    }
    if request.recurse_submodules {
        command.arg("--recurse-submodules");
    }
    command.arg("--").arg(&request.url).arg(&request.target);
    command.env("PATH", shell_env.path_string());
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GCM_INTERACTIVE", "never");
    for (key, value) in &shell_env.vars {
        if key == "PATH" || key == "Path" || key == "path" {
            continue;
        }
        command.env(key, value);
    }
    command.current_dir(
        request
            .target
            .parent()
            .unwrap_or_else(|| Path::new(".")),
    );

    let spawned = command.spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            let _ = app.emit(
                "revia://clone-finished",
                CloneFinished {
                    job_id,
                    cancelled: false,
                    success: false,
                    target: request.target.to_string_lossy().into_owned(),
                    path: None,
                    message: Some(error.to_string()),
                },
            );
            return;
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    *child_slot.lock().unwrap_or_else(|error| error.into_inner()) = Some(child);

    let tail = Arc::new(Mutex::new(Vec::<String>::new()));
    let stderr_app = app.clone();
    let stderr_tail = Arc::clone(&tail);
    let stderr_thread = std::thread::spawn(move || {
        pump_stream(stderr, &stderr_app, job_id, &stderr_tail)
    });
    pump_stream(stdout, &app, job_id, &tail);
    let _ = stderr_thread.join();

    let status = child_slot
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_mut()
        .and_then(|child| child.wait().ok());
    let was_cancelled = cancelled.load(Ordering::Relaxed);
    let success = status.as_ref().map(|status| status.success()).unwrap_or(false) && !was_cancelled;
    // H8：克隆进入统一 Git 命令日志（参数打码、耗时、退出码；输出只留尾部摘要）。
    {
        let exit_code = status
            .as_ref()
            .and_then(|status| status.code())
            .unwrap_or(if was_cancelled { -1 } else { -1 });
        log.record(crate::git::CommandRecord {
            sequence: 0,
            worktree_id: None,
            started_at_ms: crate::m7::now_ms().saturating_sub(started.elapsed().as_millis() as u64),
            program: program_text.clone(),
            args: ["clone".to_string(), request.url.clone(), request.target.to_string_lossy().into_owned()]
                .into_iter()
                .chain(
                    request
                        .branch
                        .as_ref()
                        .map(|b| format!("--branch={b}"))
                        .into_iter()
                        .chain(request.depth.map(|d| format!("--depth={d}")).into_iter()),
                )
                .collect(),
            directory: request
                .target
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            exit_code,
            elapsed_ms: started.elapsed().as_millis() as u64,
            timed_out: false,
            stdout_bytes: 0,
            stderr_excerpt: tail
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .last()
                .cloned()
                .unwrap_or_default(),
            output: tail
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .join("
"),
            output_truncated: false,
        });
    }
    // 取消或失败都保留目标目录里的残留内容，由用户在界面上决定是否清理。
    let message = if success {
        None
    } else if was_cancelled {
        Some("cancelled".to_string())
    } else {
        Some(
            tail.lock()
                .unwrap_or_else(|error| error.into_inner())
                .join("\n"),
        )
    };
    let _ = app.emit(
        "revia://clone-finished",
        CloneFinished {
            job_id,
            cancelled: was_cancelled,
            success,
            target: request.target.to_string_lossy().into_owned(),
            path: success.then(|| request.target.to_string_lossy().into_owned()),
            message,
        },
    );
}

fn pump_stream<R: std::io::Read + Send>(
    stream: Option<R>,
    app: &tauri::AppHandle,
    job_id: u64,
    tail: &Arc<Mutex<Vec<String>>>,
) {
    use tauri::Emitter;
    let Some(stream) = stream else { return };
    for line in BufReader::new(stream).lines().map_while(|line| line.ok()) {
        for part in split_progress(&line) {
            if let Ok(mut tail) = tail.lock() {
                if tail.len() >= 40 {
                    tail.remove(0);
                }
                tail.push(part.clone());
            }
            let _ = app.emit(
                "revia://clone-progress",
                CloneProgress {
                    job_id,
                    line: part,
                },
            );
        }
    }
}

/// Git 的进度用 `\r` 刷新同一行，按 `\r` 与 `\n` 拆开，并去掉 ANSI 转义序列。
fn split_progress(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in line.split(['\r', '\n']) {
        let cleaned = strip_ansi(part);
        let trimmed = cleaned.trim();
        if !trimmed.is_empty() {
            out.push(trimmed.to_string());
        }
    }
    out
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                // CSI：参数与中间字节，直到终止字节 @–~。
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            } else {
                chars.next();
            }
            continue;
        }
        if ch.is_control() && ch != '\t' {
            continue;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(url: &str, target: &str) -> CloneRequest {
        CloneRequest {
            url: url.into(),
            target: target.into(),
            branch: None,
            depth: None,
            recurse_submodules: false,
        }
    }

    #[test]
    fn rejects_option_like_urls_and_relative_targets() {
        assert!(validate(&request("--upload-pack=evil", "/tmp/x")).is_err());
        assert!(validate(&request("https://example.com/a.git", "relative")).is_err());
        assert!(validate(&request("", "/tmp/x")).is_err());
    }

    #[test]
    fn rejects_non_empty_target_and_bad_branch() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file"), "x").unwrap();
        let mut req = request("https://example.com/a.git", &dir.path().to_string_lossy());
        assert!(validate(&req).is_err());
        req.target = dir.path().join("new").to_string_lossy().into_owned();
        req.branch = Some("-b".into());
        assert!(validate(&req).is_err());
        req.branch = None;
        req.depth = Some(5000);
        assert!(validate(&req).is_err());
    }

    #[test]
    fn progress_lines_are_split_and_cleaned() {
        let lines = split_progress("Receiving objects: 10%\rReceiving objects: 20%\n\u{1b}[Kdone");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[2], "done");
    }
}
