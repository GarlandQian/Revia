//! Git 调用：只走命令行（ADR-0002），统一使用 worktree 解析出的环境（Q21）。
//!
//! 约束：
//! - 前端不能传入任意 Git 参数，也不能传入未登记的路径；
//! - 固定参数 `--no-color --no-ext-diff --no-textconv --src-prefix=a/ --dst-prefix=b/`；
//! - 路径参数一律放在 `--` 之后；
//! - 返回路径的命令固定带 `-c core.quotePath=false`，避免非 ASCII 路径被引号转义；
//! - 遇到 `index.lock` 被占用只重试和上报，绝不删除锁文件（Q26）；
//! - 每条命令都记入 Git 命令日志（Q36），M1 只记录，面板在 M4。

pub mod diff;
pub mod highlight;
pub mod rows;
pub mod status;

use std::collections::VecDeque;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde::Serialize;
use sha2::{Digest, Sha256};
use wait_timeout::ChildExt;

use crate::error::{AppError, Result};
use crate::shell::ShellEnv;

/// 使用到的最老参数（`--no-textconv`、`--porcelain=v2`）需要 2.11+。
pub const MIN_GIT_VERSION: (u32, u32) = (2, 11);

pub const DIFF_FLAGS: &[&str] = &[
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--unified=3",
];

/// 单个 Git 命令的超时。读取操作远小于这个值；写成上限是为了不挂死界面。
const GIT_TIMEOUT: Duration = Duration::from_secs(120);
const LOCK_RETRIES: u32 = 3;
const LOCK_RETRY_DELAY: Duration = Duration::from_millis(120);
const LOG_CAPACITY: usize = 500;

/// 一条 Git 命令日志（只保存在本机，Q36、Q42）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRecord {
    pub sequence: u64,
    pub program: String,
    pub args: Vec<String>,
    pub directory: String,
    pub exit_code: i32,
    pub elapsed_ms: u64,
    pub timed_out: bool,
    pub stdout_bytes: usize,
    pub stderr_excerpt: String,
}

#[derive(Debug, Default)]
pub struct CommandLog {
    inner: Mutex<LogInner>,
}

#[derive(Debug, Default)]
struct LogInner {
    next_sequence: u64,
    records: VecDeque<CommandRecord>,
}

impl CommandLog {
    pub fn record(&self, record: CommandRecord) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.next_sequence += 1;
            let mut record = record;
            record.sequence = inner.next_sequence;
            inner.records.push_back(record);
            while inner.records.len() > LOG_CAPACITY {
                inner.records.pop_front();
            }
        }
    }

    pub fn snapshot(&self) -> Vec<CommandRecord> {
        self.inner
            .lock()
            .map(|inner| inner.records.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn next_sequence(&self) -> u64 {
        self.inner
            .lock()
            .map(|inner| inner.next_sequence + 1)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub struct GitOutput {
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub exit_code: i32,
    pub elapsed: Duration,
    pub timed_out: bool,
}

impl GitOutput {
    pub fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// 要求退出码为 0，否则按 Git 命令失败上报。
    pub fn into_success(self, what: &str) -> Result<Self> {
        if self.exit_code == 0 {
            Ok(self)
        } else {
            Err(AppError::git(format!(
                "{what}失败（退出码 {}）：{}",
                self.exit_code,
                self.stderr.trim()
            )))
        }
    }
}

/// 一次 Git 调用的上下文：程序、工作目录、环境、命令日志。
#[derive(Debug, Clone)]
pub struct Git {
    pub program: PathBuf,
    pub directory: PathBuf,
    pub env: Arc<Vec<(String, String)>>,
    pub log: Arc<CommandLog>,
}

impl Git {
    pub fn new(directory: &Path, shell_env: &ShellEnv, log: Arc<CommandLog>) -> Result<Self> {
        let program = shell_env
            .which("git")
            .ok_or_else(|| AppError::environment("登录 shell 的 PATH 中找不到 git"))?;
        Ok(Self {
            program,
            directory: directory.to_path_buf(),
            env: Arc::new(
                shell_env
                    .vars
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            ),
            log,
        })
    }

    /// 用同一套环境在别的目录执行（例如对文件直接做 `--no-index` 比较）。
    pub fn in_directory(&self, directory: &Path) -> Self {
        Self {
            program: self.program.clone(),
            directory: directory.to_path_buf(),
            env: self.env.clone(),
            log: self.log.clone(),
        }
    }

    pub fn version(&self) -> Result<String> {
        let output = self.run_ok(&["--version"], "读取 git 版本")?;
        Ok(output.stdout_lossy().trim().to_string())
    }

    /// 执行一次 Git 调用，遇到 `index.lock` 时短暂重试。
    pub fn run(&self, args: &[&str]) -> Result<GitOutput> {
        let mut last: Option<GitOutput> = None;
        for attempt in 0..=LOCK_RETRIES {
            let output = self.run_once(args)?;
            if !is_lock_error(&output) {
                return Ok(output);
            }
            last = Some(output);
            if attempt < LOCK_RETRIES {
                sleep(LOCK_RETRY_DELAY * (attempt + 1));
            }
        }
        Err(AppError::rejected(format!(
            "Git 索引被其他进程占用（index.lock），已重试 {LOCK_RETRIES} 次：{}",
            last.map(|out| out.stderr.trim().to_string()).unwrap_or_default()
        )))
    }

    pub fn run_ok(&self, args: &[&str], what: &str) -> Result<GitOutput> {
        self.run(args)?.into_success(what)
    }

    /// 允许非零退出码（例如 `git diff --no-index` 在有差异时返回 1）。
    pub fn run_allow_difference(&self, args: &[&str], what: &str) -> Result<GitOutput> {
        let output = self.run(args)?;
        if output.exit_code == 0 || output.exit_code == 1 {
            Ok(output)
        } else {
            Err(AppError::git(format!(
                "{what}失败（退出码 {}）：{}",
                output.exit_code,
                output.stderr.trim()
            )))
        }
    }

    /// 只读取输出，失败时返回 None，用于探测类调用。
    pub fn try_run(&self, args: &[&str]) -> Option<String> {
        let output = self.run(args).ok()?;
        if output.exit_code == 0 {
            Some(output.stdout_lossy())
        } else {
            None
        }
    }

    fn run_once(&self, args: &[&str]) -> Result<GitOutput> {
        let started = Instant::now();
        let mut command = Command::new(&self.program);
        command
            .current_dir(&self.directory)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear();
        for (key, value) in self.env.iter() {
            command.env(key, value);
        }

        let mut child = command.spawn().map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => AppError::environment(format!(
                "找不到 git 程序：{}",
                self.program.display()
            )),
            _ => AppError::environment(format!("执行 git 失败：{error}")),
        })?;

        let sequence = self.log.next_sequence();
        // 必须并发读取管道：diff 等命令的输出可能远大于管道缓冲区，
        // 先等待子进程结束会死锁。
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        let stdout_reader = std::thread::spawn(move || read_pipe(stdout_pipe));
        let stderr_reader = std::thread::spawn(move || read_pipe(stderr_pipe));

        let timed_out = match child.wait_timeout(GIT_TIMEOUT) {
            Ok(Some(_)) => false,
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                true
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::environment(format!("等待 git 失败：{error}")));
            }
        };

        let status = child
            .wait()
            .map_err(|error| AppError::environment(format!("等待 git 结束失败：{error}")))?;
        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr_bytes = stderr_reader.join().unwrap_or_default();
        let elapsed = started.elapsed();
        let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();

        self.log.record(CommandRecord {
            sequence,
            program: self.program.display().to_string(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            directory: self.directory.display().to_string(),
            exit_code: status.code().unwrap_or(-1),
            elapsed_ms: elapsed.as_millis() as u64,
            timed_out,
            stdout_bytes: stdout.len(),
            stderr_excerpt: truncate(stderr.trim(), 2000),
        });

        Ok(GitOutput {
            stdout,
            stderr,
            exit_code: status.code().unwrap_or(-1),
            elapsed,
            timed_out,
        })
    }

    /// 目录是否是 Git 仓库（含 worktree）。
    pub fn toplevel(&self) -> Option<PathBuf> {
        let text = self.try_run(&["rev-parse", "--show-toplevel"])?;
        let path = text.trim();
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    /// 主仓库的 .git 目录（worktree 也会指回主仓库）。
    pub fn common_dir(&self) -> Option<PathBuf> {
        let text = self.try_run(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
        let path = text.trim();
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    pub fn worktrees(&self) -> Result<Vec<crate::session::WorktreeEntry>> {
        let text = self
            .run_ok(
                &["-c", "core.quotePath=false", "worktree", "list", "--porcelain"],
                "列出 worktree",
            )?
            .stdout_lossy();
        Ok(crate::session::parse_worktrees(&text))
    }

    pub fn current_branch(&self) -> Option<String> {
        let text = self.try_run(&["rev-parse", "--abbrev-ref", "HEAD"])?;
        let name = text.trim();
        (!name.is_empty() && name != "HEAD").then(|| name.to_string())
    }

    /// 某个版本中的文件内容：`HEAD:<path>`、`:0:<path>`（暂存区 stage 0，审查意见 3）。
    pub fn file_at(&self, scope: &str, path: &str) -> Option<String> {
        let spec = if scope.is_empty() || scope == "index" || scope == ":" {
            format!(":0:{path}")
        } else {
            format!("{scope}:{path}")
        };
        let output = self.run(&["show", &spec]).ok()?;
        if output.exit_code != 0 {
            return None;
        }
        Some(output.stdout_lossy())
    }

    /// 工作区文件内容。
    pub fn working_file(&self, path: &str) -> Option<String> {
        std::fs::read(self.directory.join(path))
            .ok()
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }

    /// `git config --get`；未设置时返回 None。
    pub fn config_get(&self, key: &str) -> Option<String> {
        let text = self.try_run(&["config", "--get", key])?;
        let value = text.trim();
        (!value.is_empty()).then(|| value.to_string())
    }

    /// 工作区相对暂存区的 diff（未暂存改动）。
    pub fn diff_worktree(&self, path: Option<&str>) -> Result<Vec<u8>> {
        self.diff(None, path)
    }

    /// 暂存区相对 HEAD 的 diff（已暂存改动）。
    pub fn diff_index(&self, path: Option<&str>) -> Result<Vec<u8>> {
        self.diff(Some("--staged"), path)
    }

    /// 暂存区相对 HEAD 的 diff（已暂存改动）。支持传入 original_path（重命名场景）。
    pub fn diff_index_with_orig(&self, path: &str, orig_path: Option<&str>) -> Result<Vec<u8>> {
        let mut args: Vec<&str> = vec!["--literal-pathspecs", "-c", "core.quotePath=false", "diff"];
        args.extend_from_slice(DIFF_FLAGS);
        args.push("--staged");
        args.push("--");
        if let Some(orig) = orig_path {
            args.push(orig);
        }
        args.push(path);
        Ok(self.run_ok(&args, "读取已暂存 diff")?.stdout)
    }

    /// 整文件暂存：调用 git --literal-pathspecs add -A -- <paths>（Q8、Q79、Q83、审查 1）。
    pub fn stage_file(&self, path: &str, orig_path: Option<&str>) -> Result<GitOutput> {
        let mut args = vec!["--literal-pathspecs", "-c", "core.quotePath=false", "add", "-A", "--"];
        if let Some(orig) = orig_path {
            args.push(orig);
        }
        args.push(path);
        self.run_once(&args)
    }

    /// 整文件取消暂存：调用 git --literal-pathspecs reset -- <paths>（Q8、Q79、T04、审查 1）。
    pub fn unstage_file(&self, path: &str, orig_path: Option<&str>) -> Result<GitOutput> {
        let mut args = vec!["--literal-pathspecs", "-c", "core.quotePath=false", "reset", "--"];
        if let Some(orig) = orig_path {
            args.push(orig);
        }
        args.push(path);
        self.run_once(&args)
    }

    fn diff(&self, extra: Option<&str>, path: Option<&str>) -> Result<Vec<u8>> {
        let mut args: Vec<&str> = Vec::new();
        if path.is_some() {
            args.push("--literal-pathspecs");
        }
        args.extend_from_slice(&["-c", "core.quotePath=false", "diff"]);
        args.extend_from_slice(DIFF_FLAGS);
        if let Some(extra) = extra {
            args.push(extra);
        }
        args.push("--");
        if let Some(path) = path {
            args.push(path);
        }
        Ok(self.run_ok(&args, "读取 diff")?.stdout)
    }

    /// 未跟踪文件：用 `git diff --no-index -- /dev/null <路径>` 生成 diff，
    /// 保证 hunk 也来自 Git（ADR-0004）。有差异时退出码是 1，不是失败。
    pub fn diff_untracked(&self, path: &str) -> Result<Vec<u8>> {
        let args: Vec<&str> = vec![
            "--literal-pathspecs",
            "-c",
            "core.quotePath=false",
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--unified=3",
            "--no-index",
            "--",
            "/dev/null",
            path,
        ];
        Ok(self
            .run_allow_difference(&args, "读取未跟踪文件的 diff")?
            .stdout)
    }
}

fn read_pipe(pipe: Option<impl Read>) -> Vec<u8> {
    let Some(mut pipe) = pipe else {
        return Vec::new();
    };
    let mut buffer = Vec::new();
    let _ = pipe.read_to_end(&mut buffer);
    buffer
}

pub fn is_lock_error(output: &GitOutput) -> bool {
    output.exit_code != 0 && output.stderr.contains("index.lock")
}

/// diff 指纹（ADR-0005）：对原始 diff 输出取 SHA-256。
pub fn fingerprint(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn parse_version(version_line: &str) -> Option<(u32, u32)> {
    let digits = version_line
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))?;
    let mut parts = digits.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor))
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_reads_major_and_minor() {
        assert_eq!(parse_version("git version 2.56.0"), Some((2, 56)));
        assert_eq!(parse_version("git version 2.11"), Some((2, 11)));
        assert_eq!(parse_version("nonsense"), None);
    }

    #[test]
    fn fingerprint_is_stable_and_order_sensitive() {
        let first = fingerprint(&[b"a", b"b"]);
        assert_eq!(first, fingerprint(&[b"a", b"b"]));
        assert_len(&first, 64);
        assert_ne!(first, fingerprint(&[b"b", b"a"]));
    }

    fn assert_len(text: &str, expected: usize) {
        assert_eq!(text.len(), expected);
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        let text = "中文中文";
        let cut = truncate(text, 7);
        assert!(cut.ends_with('…'));
        assert!(cut.chars().count() <= 4);
    }
}
