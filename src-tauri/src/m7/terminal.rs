//! 内置终端（M7 任务 04、Q178 / Q196）。
//!
//! 每个终端是一个真实 PTY 里的用户登录 shell：输入只来自用户按键，不拼接命令字符串；
//! 不读取、不写入用户的 shell 配置与密钥；输出经 `revia://terminal-output` 事件流式送达，
//! 同时保留有上限的回滚缓冲，供界面重新挂载时恢复。
//!
//! 进程不随应用重启恢复：重启后只恢复 tab 列表（见 `TerminalTabStore`）。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, Child, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};

/// 每个终端保留的输出上限（字符）；超出后从头部丢弃。
const SCROLLBACK_CHARS: usize = 200 * 1024;
/// 单次输入上限（一次按键/粘贴远超这个值说明不是用户输入）。
const MAX_INPUT_BYTES: usize = 8 * 1024;
/// 终端尺寸的合理范围。
const MIN_ROWS: u16 = 2;
const MAX_ROWS: u16 = 200;
const MIN_COLS: u16 = 10;
const MAX_COLS: u16 = 400;

/// 终端会话的可见状态（不含进程句柄）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TerminalSessionInfo {
    pub session_id: u64,
    pub worktree_id: String,
    pub worktree_path: String,
    pub shell: String,
    pub cwd: String,
    pub title: String,
    pub rows: u16,
    pub cols: u16,
    pub running: bool,
    /// 退出码（进程结束后才有）。
    pub exit_code: Option<i32>,
}

/// 输出事件负载。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TerminalOutput {
    pub session_id: u64,
    pub data: String,
}

/// 退出事件负载。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TerminalExit {
    pub session_id: u64,
    pub exit_code: Option<i32>,
}

/// 打开终端的请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOpenRequest {
    pub worktree_id: String,
    /// 工作目录（必须仍在 worktree 内；`None` 用 worktree 根）。
    pub cwd: Option<String>,
    pub rows: u16,
    pub cols: u16,
}

/// 重启后恢复的 tab 记录（不含进程）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TerminalTabRecord {
    pub worktree_id: String,
    pub cwd: String,
    pub title: String,
    /// 排序用；不表示运行状态。
    pub order: u32,
}

/// tab 列表存储（Revia 数据目录，跨重启保留）。
pub struct TerminalTabStore {
    path: PathBuf,
}

impl TerminalTabStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("terminal-tabs.json"),
        }
    }

    pub fn load(&self) -> Result<Vec<TerminalTabRecord>> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save(&self, mut tabs: Vec<TerminalTabRecord>) -> Result<()> {
        tabs.sort_by_key(|tab| tab.order);
        tabs.truncate(64);
        super::atomic_write(&self.path, &serde_json::to_vec_pretty(&tabs)?)
    }
}

struct Scrollback {
    text: String,
}

impl Scrollback {
    fn push(&mut self, chunk: &str) {
        self.text.push_str(chunk);
        let excess = self.text.chars().count().saturating_sub(SCROLLBACK_CHARS);
        if excess > 0 {
            let cut = self
                .text
                .char_indices()
                .nth(excess)
                .map(|(index, _)| index)
                .unwrap_or(self.text.len());
            self.text.drain(..cut);
        }
    }
}

struct TerminalHandle {
    info: TerminalSessionInfo,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    buffer: Arc<Mutex<Scrollback>>,
    exited: Arc<AtomicBool>,
    exit_code: Arc<Mutex<Option<i32>>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

/// 所有终端会话（进程内）。
pub struct TerminalManager {
    sessions: Mutex<HashMap<u64, TerminalHandle>>,
    counter: AtomicU64,
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            counter: AtomicU64::new(1),
        }
    }

    pub fn list(&self) -> Vec<TerminalSessionInfo> {
        let sessions = self.sessions.lock().unwrap_or_else(|error| error.into_inner());
        let mut list: Vec<_> = sessions
            .values()
            .map(|handle| {
                let mut info = handle.info.clone();
                info.running = !handle.exited.load(Ordering::Relaxed);
                info.exit_code = *handle.exit_code.lock().unwrap_or_else(|e| e.into_inner());
                info
            })
            .collect();
        list.sort_by_key(|info| info.session_id);
        list
    }

    /// 打开终端：在 worktree 里启动用户的登录 shell。
    pub fn open<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        worktree_id: &str,
        worktree_path: &Path,
        shell_env: &crate::shell::ShellEnv,
        cwd: Option<&str>,
        rows: u16,
        cols: u16,
    ) -> Result<TerminalSessionInfo> {
        let cwd = match cwd {
            Some(relative) if !relative.trim().is_empty() => resolve_cwd(worktree_path, relative)?,
            _ => worktree_path.to_path_buf(),
        };
        let shell = shell_program(shell_env);
        let shell_path = shell.to_string_lossy().into_owned();
        let rows = rows.clamp(MIN_ROWS, MAX_ROWS);
        let cols = cols.clamp(MIN_COLS, MAX_COLS);

        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| {
                AppError::coded(
                    ErrorKind::Environment,
                    "errors.m7.terminalOpenFailed",
                    "打开终端失败：{error}",
                    &[("error", error.to_string())],
                )
            })?;

        let mut command = CommandBuilder::new(&shell);
        command.cwd(&cwd);
        // 只传登录环境解析出的变量；不额外读取用户配置或密钥。
        for (key, value) in &shell_env.vars {
            if key == "PATH" || key == "Path" || key == "path" {
                continue;
            }
            command.env(key, value);
        }
        command.env("PATH", shell_env.path_string());
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env("TERM_PROGRAM", "Revia");
        // 交互式登录 shell：与用户在系统终端里得到的环境一致。
        if cfg!(windows) {
            // cmd.exe / PowerShell 由用户默认 shell 决定，不额外加参数。
        } else {
            command.arg("-l");
        }

        let child = pair.slave.spawn_command(command).map_err(|error| {
            AppError::coded(
                ErrorKind::Environment,
                "errors.m7.terminalSpawnFailed",
                "启动终端进程失败：{error}",
                &[("error", error.to_string())],
            )
        })?;
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(|error| {
            AppError::coded(
                ErrorKind::Environment,
                "errors.m7.terminalStreamFailed",
                "无法读取终端输出：{error}",
                &[("error", error.to_string())],
            )
        })?;
        let writer = pair.master.take_writer().map_err(|error| {
            AppError::coded(
                ErrorKind::Environment,
                "errors.m7.terminalStreamFailed",
                "无法写入终端：{error}",
                &[("error", error.to_string())],
            )
        })?;

        let session_id = self.counter.fetch_add(1, Ordering::Relaxed);
        let buffer = Arc::new(Mutex::new(Scrollback {
            text: String::new(),
        }));
        let exited = Arc::new(AtomicBool::new(false));
        let exit_code = Arc::new(Mutex::new(None));
        let info = TerminalSessionInfo {
            session_id,
            worktree_id: worktree_id.to_string(),
            worktree_path: worktree_path.to_string_lossy().into_owned(),
            shell: shell_path,
            cwd: cwd.to_string_lossy().into_owned(),
            title: cwd
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Terminal".into()),
            rows,
            cols,
            running: true,
            exit_code: None,
        };

        let killer = child.clone_killer();
        spawn_reader(
            app.clone(),
            session_id,
            reader,
            Arc::clone(&buffer),
            Arc::clone(&exited),
        );
        spawn_waiter(
            app.clone(),
            session_id,
            child,
            Arc::clone(&exited),
            Arc::clone(&exit_code),
        );

        let mut sessions = self.sessions.lock().unwrap_or_else(|error| error.into_inner());
        sessions.insert(
            session_id,
            TerminalHandle {
                info: info.clone(),
                master: pair.master,
                writer,
                buffer,
                exited,
                exit_code,
                killer,
            },
        );
        Ok(info)
    }

    /// 输入/错误/忙碌提示按真实会话归属，不根据当前显示面板猜worktree。
    pub fn owner(&self, session_id: u64) -> Result<String> {
        let sessions = self.sessions.lock().unwrap_or_else(|error| error.into_inner());
        Ok(sessions.get(&session_id).ok_or_else(unknown_session)?.info.worktree_id.clone())
    }

    pub fn write_input(&self, session_id: u64, data: &str) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        if data.len() > MAX_INPUT_BYTES {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.m7.terminalInputTooLarge",
                "终端单次输入过大",
                &[],
            ));
        }
        let mut sessions = self.sessions.lock().unwrap_or_else(|error| error.into_inner());
        let handle = sessions.get_mut(&session_id).ok_or_else(unknown_session)?;
        if handle.exited.load(Ordering::Relaxed) {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.m7.terminalExited",
                "终端进程已结束，无法写入",
                &[],
            ));
        }
        handle.writer.write_all(data.as_bytes()).map_err(|error| {
            AppError::coded(
                ErrorKind::Environment,
                "errors.m7.terminalWriteFailed",
                "写入终端失败：{error}",
                &[("error", error.to_string())],
            )
        })?;
        handle.writer.flush().ok();
        Ok(())
    }

    pub fn resize(&self, session_id: u64, rows: u16, cols: u16) -> Result<()> {
        let rows = rows.clamp(MIN_ROWS, MAX_ROWS);
        let cols = cols.clamp(MIN_COLS, MAX_COLS);
        let mut sessions = self.sessions.lock().unwrap_or_else(|error| error.into_inner());
        let handle = sessions.get_mut(&session_id).ok_or_else(unknown_session)?;
        handle
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| {
                AppError::coded(
                    ErrorKind::Environment,
                    "errors.m7.terminalResizeFailed",
                    "调整终端尺寸失败：{error}",
                    &[("error", error.to_string())],
                )
            })?;
        handle.info.rows = rows;
        handle.info.cols = cols;
        Ok(())
    }

    /// 重新挂载时取回已有输出（有上限）。
    pub fn snapshot(&self, session_id: u64) -> Result<String> {
        let sessions = self.sessions.lock().unwrap_or_else(|error| error.into_inner());
        let handle = sessions.get(&session_id).ok_or_else(unknown_session)?;
        let buffer = handle.buffer.lock().unwrap_or_else(|error| error.into_inner());
        Ok(buffer.text.clone())
    }

    /// 关闭终端：结束进程并丢弃会话（调用前由界面确认）。
    pub fn close(&self, session_id: u64) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|error| error.into_inner());
        let handle = sessions.get_mut(&session_id).ok_or_else(unknown_session)?;
        // kill失败时保留原句柄与页签，由用户核对/重试，不能返回假成功。
        if !handle.exited.load(Ordering::Relaxed) {
            handle.killer.kill().map_err(|error| AppError::coded(
                ErrorKind::Environment, "errors.m7.terminalCloseFailed",
                "结束终端进程失败：{cause}", &[],
            ).cause(error.into()))?;
        }
        let handle = sessions.remove(&session_id).ok_or_else(unknown_session)?;
        drop(handle.master);
        handle.exited.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// 关闭所有属于某个 worktree 的终端（切换仓库时由界面确认后调用）。
    pub fn close_worktree(&self, worktree_id: &str) -> Vec<u64> {
        let ids: Vec<u64> = self
            .list()
            .into_iter()
            .filter(|info| info.worktree_id == worktree_id)
            .map(|info| info.session_id)
            .collect();
        for id in &ids {
            let _ = self.close(*id);
        }
        ids
    }

    pub fn has_running(&self, worktree_id: &str) -> bool {
        self.list()
            .iter()
            .any(|info| info.worktree_id == worktree_id && info.running)
    }
}

#[cfg(test)]
mod cwd_tests {
    use super::*;

    #[test]
    fn resolve_cwd_accepts_absolute_path_inside_worktree() {
        // F05：持久化页签恢复用绝对 cwd；worktree 内的绝对路径必须被接受。
        let root = tempfile::tempdir().unwrap();
        let worktree = root.path().join("repo");
        let nested = worktree.join("src");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(
            resolve_cwd(&worktree, nested.to_string_lossy().as_ref()).unwrap(),
            nested.canonicalize().unwrap()
        );
        // 相对路径仍然可用。
        assert_eq!(
            resolve_cwd(&worktree, "src").unwrap(),
            nested.canonicalize().unwrap()
        );
        // worktree 外的绝对路径仍然拒绝。
        let outside = root.path().join("elsewhere");
        std::fs::create_dir_all(&outside).unwrap();
        assert!(resolve_cwd(&worktree, outside.to_string_lossy().as_ref()).is_err());
        // .. 逃逸仍然拒绝。
        assert!(resolve_cwd(&worktree, "../elsewhere").is_err());
    }
}

fn unknown_session() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.m7.terminalSessionUnknown",
        "终端会话已不存在",
        &[],
    )
}

fn resolve_cwd(worktree_path: &Path, relative: &str) -> Result<PathBuf> {
    let candidate = Path::new(relative);
    // F05：持久化页签里存的是会话返回的绝对 cwd；恢复时同样是绝对路径。
    // 只要求最终位置在 worktree 内，相对/绝对都接受。
    let full = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        for component in candidate.components() {
            if !matches!(component, std::path::Component::Normal(_) | std::path::Component::CurDir) {
                return Err(AppError::coded(
                    ErrorKind::Rejected,
                    "errors.m7.terminalCwdOutsideWorktree",
                    "终端工作目录必须在当前 worktree 内",
                    &[],
                ));
            }
        }
        worktree_path.join(candidate)
    };
    let canonical = std::fs::canonicalize(&full).map_err(|_| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.terminalCwdMissing",
            "终端工作目录不存在",
            &[],
        )
    })?;
    let root = std::fs::canonicalize(worktree_path).map_err(|_| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.terminalCwdOutsideWorktree",
            "终端工作目录必须在当前 worktree 内",
            &[],
        )
    })?;
    if !canonical.starts_with(&root) || !canonical.is_dir() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.terminalCwdOutsideWorktree",
            "终端工作目录必须在当前 worktree 内",
            &[],
        ));
    }
    Ok(canonical)
}

fn shell_program(env: &crate::shell::ShellEnv) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(shell) = env.vars.get("ComSpec") {
            let candidate = PathBuf::from(shell);
            if candidate.is_file() {
                return candidate;
            }
        }
        PathBuf::from("cmd.exe")
    }
    #[cfg(not(windows))]
    {
        if let Some(shell) = env.vars.get("SHELL") {
            let candidate = PathBuf::from(shell);
            if candidate.is_file() {
                return candidate;
            }
        }
        let zsh = PathBuf::from("/bin/zsh");
        if zsh.is_file() {
            return zsh;
        }
        PathBuf::from("/bin/sh")
    }
}

/// 读线程：把 PTY 输出按 UTF-8 边界解码后发事件，并写入回滚缓冲。
fn spawn_reader<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    session_id: u64,
    mut reader: Box<dyn Read + Send>,
    buffer: Arc<Mutex<Scrollback>>,
    exited: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let mut pending: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => {
                    pending.extend_from_slice(&chunk[..count]);
                    let valid = match std::str::from_utf8(&pending) {
                        Ok(_) => pending.len(),
                        Err(error) => error.valid_up_to(),
                    };
                    if valid == 0 {
                        if pending.len() > 4 {
                            // 无法解码的字节按替换字符处理，避免无限累积。
                            let lossy = String::from_utf8_lossy(&pending).into_owned();
                            pending.clear();
                            emit_output(&app, session_id, &lossy);
                            push_buffer(&buffer, &lossy);
                        }
                        continue;
                    }
                    let text = String::from_utf8_lossy(&pending[..valid]).into_owned();
                    pending.drain(..valid);
                    emit_output(&app, session_id, &text);
                    push_buffer(&buffer, &text);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        if !pending.is_empty() {
            let text = String::from_utf8_lossy(&pending).into_owned();
            emit_output(&app, session_id, &text);
            push_buffer(&buffer, &text);
        }
        exited.store(true, Ordering::Relaxed);
    });
}

fn push_buffer(buffer: &Arc<Mutex<Scrollback>>, text: &str) {
    if let Ok(mut buffer) = buffer.lock() {
        buffer.push(text);
    }
}

fn emit_output<R: tauri::Runtime>(app: &tauri::AppHandle<R>, session_id: u64, data: &str) {
    use tauri::Emitter;
    let _ = app.emit(
        "revia://terminal-output",
        TerminalOutput {
            session_id,
            data: data.to_string(),
        },
    );
}

/// 等待线程：进程结束时发退出事件（退出码只作为观察结果）。
fn spawn_waiter<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    session_id: u64,
    mut child: Box<dyn Child + Send + Sync>,
    exited: Arc<AtomicBool>,
    exit_code: Arc<Mutex<Option<i32>>>,
) {
    std::thread::spawn(move || {
        let status = child.wait();
        let code = status.ok().map(|status| status.exit_code() as i32);
        if let Ok(mut slot) = exit_code.lock() {
            *slot = code;
        }
        exited.store(true, Ordering::Relaxed);
        use tauri::Emitter;
        let _ = app.emit(
            "revia://terminal-exit",
            TerminalExit {
                session_id,
                exit_code: code,
            },
        );
    });
}
