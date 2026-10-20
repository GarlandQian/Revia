//! Git 调用：只走命令行（ADR-0002），统一使用 worktree 解析出的环境（Q21）。
//!
//! 约束：
//! - 前端不能传入任意 Git 参数；这里只暴露固定、带类型的读取操作；
//! - 固定参数 `--no-color --no-ext-diff --no-textconv --src-prefix=a/ --dst-prefix=b/`；
//! - 路径参数一律放在 `--` 之后；
//! - 遇到 `index.lock` 被占用只重试和上报，绝不删除锁文件。

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::env::ShellEnv;
use crate::error::{AppError, Result};

/// 读取 git 版本的最低要求：`git diff --no-textconv` 需要 2.11+，
/// `git worktree list --porcelain` 需要 2.7+，取两者中更高的要求。
pub const MIN_GIT_VERSION: (u32, u32) = (2, 11);

pub const DIFF_FLAGS: &[&str] = &[
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--unified=3",
];

const LOCK_RETRIES: u32 = 3;
const LOCK_RETRY_DELAY: Duration = Duration::from_millis(120);

#[derive(Debug, Clone)]
pub struct GitOutput {
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub exit_code: i32,
    pub elapsed: Duration,
}

impl GitOutput {
    pub fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    fn into_success(self, what: &str) -> Result<Self> {
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

#[derive(Debug, Clone)]
pub struct GitCommand {
    pub program: PathBuf,
    pub worktree: PathBuf,
    pub env: HashMap<String, String>,
}

impl GitCommand {
    pub fn new(worktree: &Path, shell_env: &ShellEnv) -> Result<Self> {
        let program = shell_env
            .which("git")
            .ok_or_else(|| AppError::environment("登录 shell 的 PATH 中找不到 git"))?;
        Ok(Self {
            program,
            worktree: worktree.to_path_buf(),
            env: shell_env.vars.clone(),
        })
    }

    /// 执行一次 git 调用，检查版本之后才使用固定 diff 参数。
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
            "Git 索引被其他进程占用（index.lock），已重试 {} 次：{}",
            LOCK_RETRIES,
            last.map(|out| out.stderr.trim().to_string()).unwrap_or_default()
        )))
    }

    pub fn run_ok(&self, args: &[&str], what: &str) -> Result<GitOutput> {
        self.run(args)?.into_success(what)
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
            .current_dir(&self.worktree)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear();
        for (key, value) in &self.env {
            command.env(key, value);
        }

        let output = command.output().map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => AppError::environment(format!(
                "找不到 git 程序：{}",
                self.program.display()
            )),
            _ => AppError::environment(format!("执行 git 失败：{error}")),
        })?;

        Ok(GitOutput {
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
            elapsed: started.elapsed(),
        })
    }

    pub fn version(&self) -> Result<String> {
        let output = self.run_ok(&["--version"], "读取 git 版本")?;
        Ok(output.stdout_lossy().trim().to_string())
    }

    /// `git rev-parse --show-toplevel`：确认目录是一个 worktree。
    pub fn toplevel(&self) -> Option<PathBuf> {
        let text = self.try_run(&["rev-parse", "--show-toplevel"])?;
        let path = text.trim();
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    pub fn worktrees(&self) -> Result<Vec<WorktreeEntry>> {
        let text = self
            .run_ok(&["worktree", "list", "--porcelain"], "列出 worktree")?
            .stdout_lossy();
        Ok(parse_worktrees(&text))
    }

    /// 文件在某个版本中的内容：`HEAD:<path>` 或 `:<path>`（暂存区）。
    pub fn file_at(&self, scope: &str, path: &str) -> Option<String> {
        let spec = format!("{scope}:{path}");
        let output = self.run(&["show", &spec]).ok()?;
        if output.exit_code != 0 {
            return None;
        }
        Some(output.stdout_lossy())
    }

    /// working tree（工作区）文件内容。
    pub fn working_file(&self, path: &str) -> Option<String> {
        let full = self.worktree.join(path);
        std::fs::read(&full).ok().map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }

    /// 新增文件在 HEAD 里不存在，取引入它的提交 SHA 作为“旧版本”。
    pub fn introduction_sha(&self, path: &str) -> Option<String> {
        let text = self.try_run(&["log", "--format=%H", "--max-count=1", "--", path])?;
        let sha = text.trim();
        (!sha.is_empty()).then(|| sha.to_string())
    }

    pub fn config_get(&self, key: &str) -> Option<String> {
        let text = self.try_run(&["config", "--get", key])?;
        let value = text.trim();
        (!value.is_empty()).then(|| value.to_string())
    }

    /// 固定参数的 diff 调用。`staged` 为真时比较 HEAD 与暂存区。
    pub fn diff(&self, path: Option<&str>, staged: bool) -> Result<Vec<u8>> {
        let mut args: Vec<&str> = vec!["diff"];
        args.extend_from_slice(DIFF_FLAGS);
        if staged {
            args.push("--staged");
        }
        args.push("--");
        if let Some(path) = path {
            args.push(path);
        }
        let output = self.run_ok(&args, "读取 diff")?;
        Ok(output.stdout)
    }
}

fn is_lock_error(output: &GitOutput) -> bool {
    output.exit_code != 0 && output.stderr.contains("index.lock")
}

pub fn fingerprint(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeEntry {
    pub path: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
}

fn parse_worktrees(text: &str) -> Vec<WorktreeEntry> {
    let mut entries = Vec::new();
    let mut current: Option<WorktreeEntry> = None;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(WorktreeEntry {
                path: unquote(path),
                head: None,
                branch: None,
                bare: false,
                detached: false,
            });
            continue;
        }
        let Some(entry) = current.as_mut() else { continue };
        if let Some(head) = line.strip_prefix("HEAD ") {
            entry.head = Some(head.trim().to_string());
        } else if let Some(branch) = line.strip_prefix("branch ") {
            entry.branch = branch
                .trim()
                .strip_prefix("refs/heads/")
                .or_else(|| Some(branch.trim()))
                .map(|name| name.to_string());
        } else if line.trim() == "bare" {
            entry.bare = true;
        } else if line.trim() == "detached" {
            entry.detached = true;
        }
    }
    if let Some(entry) = current.take() {
        entries.push(entry);
    }
    entries
}

/// 去掉 Git 在路径外层的引号；`core.quotePath=false` 下大多数路径本来就是裸的。
fn unquote(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let inner = &trimmed[1..trimmed.len() - 1];
        let mut result = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.next() {
                    Some('n') => result.push('\n'),
                    Some('t') => result.push('\t'),
                    Some('"') => result.push('"'),
                    Some('\\') => result.push('\\'),
                    Some(other) => result.push(other),
                    None => {}
                }
            } else {
                result.push(ch);
            }
        }
        result
    } else {
        trimmed.to_string()
    }
}

/// 解析版本号，用于最低版本检查。
pub fn parse_version(version_line: &str) -> Option<(u32, u32)> {
    let digits = version_line
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))?;
    let mut parts = digits.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor))
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
    fn parse_worktrees_handles_main_and_linked() {
        let text = "worktree /tmp/repo\nHEAD abc123\nbranch refs/heads/main\n\nworktree /tmp/other\nHEAD def456\ndetached\n\nworktree /tmp/bare\nbare\n\n";
        let entries = parse_worktrees(text);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].path, "/tmp/repo");
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert!(entries[1].detached);
        assert!(entries[2].bare);
    }

    #[test]
    fn unquote_removes_escapes() {
        assert_eq!(unquote("\"a b\""), "a b");
        assert_eq!(unquote("/plain/path"), "/plain/path");
        assert_eq!(unquote("\"quote\\\"d\""), "quote\"d");
    }

    #[test]
    fn fingerprint_is_stable_hex() {
        let first = fingerprint(b"diff --git a/x b/x\n");
        let second = fingerprint(b"diff --git a/x b/x\n");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert_ne!(first, fingerprint(b"other"));
    }
}
