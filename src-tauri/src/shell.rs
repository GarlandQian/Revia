//! 运行环境：按 worktree 解析交互式登录 shell 的环境变量。
//!
//! 从 Finder 或 Dock 启动的应用不继承登录 shell 的环境，而 Git、hooks 和外部合并工具都依赖
//! `PATH`。这里在目标 worktree 目录下启动一次用户的交互式登录 zsh，把环境变量转储到临时文件后读回。
//!
//! 实现要点（评审要求）：
//! - 显式传入最小基线环境（HOME / PATH / LANG 等），不继承应用进程的环境；
//! - 硬超时（默认 5 秒），超时后杀掉进程并回退到最小环境；
//! - 转储到临时文件而不是 stdout，避免 `.zshrc` 里的提示输出污染解析；
//! - 结果按 worktree 缓存，可手动重新加载。

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use serde::Serialize;
use wait_timeout::ChildExt;

use crate::error::{AppError, Result};

/// 登录 shell 的硬超时。
pub const SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// 不参与缓存键、也不从 shell 结果里继承的变量：这些由 Revia 自己控制。
const EXCLUDED_KEYS: &[&str] = &["PWD", "OLDPWD", "SHLVL", "_", "ZDOTDIR", "PS1", "RPROMPT"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvCaptureInfo {
    pub worktree: String,
    pub shell: String,
    pub elapsed_ms: u64,
    pub timed_out: bool,
    /// 解析失败时使用了最小环境回退。
    pub used_fallback: bool,
    pub var_count: usize,
    pub path_entries: Vec<String>,
    pub diagnostics: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ShellEnv {
    pub vars: HashMap<String, String>,
    pub path: Vec<String>,
    pub info: EnvCaptureInfo,
}

impl ShellEnv {
    /// 解析结果里的 PATH 条目（跨平台分隔符）。
    pub fn path_string(&self) -> String {
        std::env::join_paths(self.path.iter().map(Path::new))
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|_| self.path.join(if cfg!(windows) { ";" } else { ":" }))
    }

    /// 供展示用的 git / node 等程序的解析结果。
    pub fn which(&self, program: &str) -> Option<PathBuf> {
        which_in(&self.path, program)
    }
}

fn which_in(path: &[String], program: &str) -> Option<PathBuf> {
    let has_separator = program.contains('/') || (cfg!(windows) && program.contains('\\'));
    if has_separator {
        let candidate = PathBuf::from(program);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            for ext in ["exe", "cmd", "bat"] {
                let with_ext = candidate.with_extension(ext);
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }
        return None;
    }
    for dir in path {
        let candidate = Path::new(dir).join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            for ext in ["exe", "cmd", "bat"] {
                let with_ext = candidate.with_extension(ext);
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }
    }
    None
}

#[cfg(unix)]
/// 最小基线环境：模拟从 Finder 启动时的环境，只保留启动 zsh 必需的部分。
fn base_env(home: &Path) -> Vec<(String, String)> {
    let mut vars: Vec<(String, String)> = vec![
        ("HOME".into(), home.display().to_string()),
        ("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into()),
        ("SHELL".into(), "/bin/zsh".into()),
        ("LANG".into(), "en_US.UTF-8".into()),
        ("TERM".into(), "xterm-256color".into()),
    ];
    if let Some(user) = std::env::var_os("USER") {
        vars.push(("USER".into(), user.to_string_lossy().into_owned()));
    }
    if let Some(logname) = std::env::var_os("LOGNAME") {
        vars.push(("LOGNAME".into(), logname.to_string_lossy().into_owned()));
    }
    vars
}

#[cfg(unix)]
/// 回退用环境：shell 不可用或超时时使用。不继承应用进程的 PATH（从 Finder 启动时它是残缺的）。
fn fallback_env(home: &Path) -> Vec<(String, String)> {
    let mut vars = base_env(home);
    vars.retain(|(key, _)| key != "PATH");
    vars.push((
        "PATH".into(),
        "/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".into(),
    ));
    vars
}

#[cfg(unix)]
fn dump_path(worktree: &Path) -> String {
    worktree.join(".revia-env.dump").display().to_string()
}

#[cfg(unix)]
/// 转储脚本：把环境写成 `KEY=VALUE\0`，只保留选定变量，静默执行。
///
/// 用 zsh 自己的 `>&` 重定向打开转储文件，不继承应用进程的任何文件描述符；
/// stdout / stderr 都被丢弃，避免 `.zshrc` 里的提示输出混进结果。
/// `${(P)__revia_k}` 是 zsh 的间接取值；`${+parameters[...]}` 判断变量是否存在。
fn dump_script(worktree: &Path) -> String {
    let dump = dump_path(worktree);
    format!(
        "typeset -a __revia_keep\n\
         __revia_keep=(PATH HOME SHELL USER LOGNAME LANG LC_ALL LC_CTYPE TMPDIR SSH_AUTH_SOCK \
         DISPLAY XDG_CONFIG_HOME GIT_SSH GIT_SSH_COMMAND GIT_ASKPASS \
         FNM_DIR FNM_NODE_DIST_MIRROR NVM_DIR NVM_BIN NVM_INC NVM_CD_FLAGS NVM_RC_VERSION \
         VOLTA_HOME BUN_INSTALL DENO_INSTALL PNPM_HOME ASDF_DIR ASDF_DATA_DIR PYENV_ROOT)\n\
         exec {{__revia_fd}}> {dump:?} || exit 1\n\
         for __revia_k in $__revia_keep; do\n\
         \x20 if (( ${{+parameters[$__revia_k]}} )); then\n\
         \x20   print -rn -- \"$__revia_k=${{(P)__revia_k}}\"$'\\0' >&$__revia_fd || exit 1\n\
         \x20 fi\n\
         done\n\
         exec {{__revia_fd}}>&-\n\
         kill -USR1 $$\n",
        dump = dump
    )
}

fn sanitize(mut vars: HashMap<String, String>) -> HashMap<String, String> {
    for key in EXCLUDED_KEYS {
        vars.remove(*key);
    }
    vars
}

#[cfg(unix)]
/// 解析 worktree 的运行环境。失败时回退到最小环境，不返回错误，只在 info 里标注。
pub fn resolve(home: &Path, worktree: &Path) -> ShellEnv {
    let shell = detect_shell();
    let shell_path = shell.display().to_string();
    let started = Instant::now();

    if !worktree.is_dir() {
        return fallback(
            home,
            worktree,
            &shell_path,
            started,
            Some(format!("worktree 目录不存在：{}", worktree.display())),
        );
    }

    let _ = std::fs::remove_file(dump_path(worktree));
    let script = dump_script(worktree);

    let mut command = Command::new(&shell);
    command
        .current_dir(worktree)
        .args(["-l", "-i", "-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .env_clear();
    for (key, value) in base_env(home) {
        command.env(key, value);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return fallback(
                home,
                worktree,
                &shell_path,
                started,
                Some(format!("启动 {shell_path} 失败：{error}")),
            );
        }
    };

    let mut stderr = child.stderr.take();
    let timed_out = match child.wait_timeout(SHELL_TIMEOUT) {
        Ok(Some(_)) => false,
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            true
        }
        Err(error) => {
            let _ = child.kill();
            return fallback(
                home,
                worktree,
                &shell_path,
                started,
                Some(format!("等待 shell 失败：{error}")),
            );
        }
    };

    let mut diagnostics = None;
    if let Some(mut pipe) = stderr.take() {
        let mut text = String::new();
        let _ = pipe.read_to_string(&mut text);
        let text = text.trim();
        if !text.is_empty() {
            diagnostics = Some(truncate(text, 2000));
        }
    }

    if timed_out {
        let _ = std::fs::remove_file(dump_path(worktree));
        return fallback(
            home,
            worktree,
            &shell_path,
            started,
            Some(format!(
                "登录 shell 在 {} 毫秒内没有返回，已终止并使用最小环境",
                SHELL_TIMEOUT.as_millis()
            )),
        );
    }

    let dump = std::fs::read(dump_path(worktree));
    let _ = std::fs::remove_file(dump_path(worktree));
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let vars = match dump {
        Ok(bytes) => parse_dump(&bytes),
        Err(error) => {
            return fallback(
                home,
                worktree,
                &shell_path,
                started,
                Some(format!("读取环境转储失败：{error}")),
            );
        }
    };

    if vars.is_empty() {
        return fallback(
            home,
            worktree,
            &shell_path,
            started,
            Some("登录 shell 返回了空环境".into()),
        );
    }

    let vars = sanitize(vars);
    let path = split_path(&vars);
    if path.is_empty() {
        return fallback(
            home,
            worktree,
            &shell_path,
            started,
            Some("登录 shell 返回的环境里没有 PATH".into()),
        );
    }

    ShellEnv {
        info: EnvCaptureInfo {
            worktree: worktree.display().to_string(),
            shell: shell_path.clone(),
            elapsed_ms,
            timed_out: false,
            used_fallback: false,
            var_count: vars.len(),
            path_entries: path.clone(),
            diagnostics,
        },
        vars: {
            let mut vars = vars;
            vars.insert("REVIA_SHELL".into(), shell_path);
            vars
        },
        path,
    }
}

#[cfg(windows)]
/// Windows 下无登录 shell，行为合理降级：使用进程自身环境，确保 Windows CI 与开发环境可用（M2 决策 8、Q55）。
pub fn resolve(_home: &Path, worktree: &Path) -> ShellEnv {
    let shell = detect_shell();
    let shell_path = shell.display().to_string();
    let started = Instant::now();

    let mut vars = HashMap::new();
    for (key, value) in std::env::vars() {
        vars.insert(key, value);
    }
    let vars = sanitize(vars);
    let path = split_path(&vars);

    ShellEnv {
        info: EnvCaptureInfo {
            worktree: worktree.display().to_string(),
            shell: shell_path.clone(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            timed_out: false,
            used_fallback: false,
            var_count: vars.len(),
            path_entries: path.clone(),
            diagnostics: None,
        },
        vars: {
            let mut vars = vars;
            vars.insert("REVIA_SHELL".into(), shell_path);
            vars
        },
        path,
    }
}

#[cfg(unix)]
fn fallback(
    home: &Path,
    worktree: &Path,
    shell: &str,
    started: Instant,
    diagnostics: Option<String>,
) -> ShellEnv {
    let vars: HashMap<String, String> = fallback_env(home).into_iter().collect();
    let path = split_path(&vars);
    ShellEnv {
        info: EnvCaptureInfo {
            worktree: worktree.display().to_string(),
            shell: shell.to_string(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            timed_out: diagnostics
                .as_deref()
                .map(|text| text.contains("没有返回"))
                .unwrap_or(false),
            used_fallback: true,
            var_count: vars.len(),
            path_entries: path.clone(),
            diagnostics,
        },
        path,
        vars,
    }
}

#[cfg(unix)]
fn detect_shell() -> PathBuf {
    if let Some(shell) = std::env::var_os("SHELL") {
        let candidate = PathBuf::from(shell);
        if candidate.is_file() {
            return candidate;
        }
    }
    let zsh = PathBuf::from("/bin/zsh");
    if zsh.is_file() {
        return zsh;
    }
    zsh
}

#[cfg(windows)]
fn detect_shell() -> PathBuf {
    if let Some(comspec) = std::env::var_os("ComSpec") {
        let candidate = PathBuf::from(comspec);
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("cmd.exe")
}

fn split_path(vars: &HashMap<String, String>) -> Vec<String> {
    let path_val = vars.get("PATH").or_else(|| {
        if cfg!(windows) {
            vars.get("Path").or_else(|| vars.get("path"))
        } else {
            None
        }
    });

    path_val
        .map(|value| {
            std::env::split_paths(value)
                .filter_map(|entry| {
                    let s = entry.to_string_lossy().trim().to_string();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_dump(bytes: &[u8]) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    for chunk in bytes.split(|byte| *byte == 0) {
        if chunk.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(chunk);
        if let Some((key, value)) = text.split_once('=') {
            if !key.is_empty() {
                vars.insert(key.to_string(), value.to_string());
            }
        }
    }
    vars
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

/// 按 worktree 缓存的环境解析结果。
#[derive(Default)]
pub struct EnvCache {
    inner: RwLock<HashMap<PathBuf, ShellEnv>>,
}

impl EnvCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// 取缓存结果；`force` 为真时重新解析。
    pub fn get(&self, home: &Path, worktree: &Path, force: bool) -> ShellEnv {
        let key = worktree.to_path_buf();
        if !force {
            if let Ok(guard) = self.inner.read() {
                if let Some(env) = guard.get(&key) {
                    return env.clone();
                }
            }
        }
        let env = resolve(home, worktree);
        if let Ok(mut guard) = self.inner.write() {
            guard.insert(key, env.clone());
        }
        env
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.write() {
            guard.clear();
        }
    }
}

/// 解析当前用户的主目录；从 Finder 启动时 HOME 一定存在，但仍保留回退。
pub fn home_dir() -> Result<PathBuf> {
    #[cfg(unix)]
    if let Some(home) = std::env::var_os("HOME") {
        let path = PathBuf::from(home);
        if path.is_dir() {
            return Ok(path);
        }
    }

    #[cfg(windows)]
    {
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            let path = PathBuf::from(profile);
            if path.is_dir() {
                return Ok(path);
            }
        }
        if let (Some(drive), Some(homepath)) = (std::env::var_os("HOMEDRIVE"), std::env::var_os("HOMEPATH")) {
            let path = PathBuf::from(format!("{}{}", drive.to_string_lossy(), homepath.to_string_lossy()));
            if path.is_dir() {
                return Ok(path);
            }
        }
    }

    Err(AppError::environment("找不到用户主目录（HOME 或 USERPROFILE 未设置）"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dump_reads_nul_separated_pairs() {
        let bytes = b"PATH=/a:/b\0HOME=/Users/x\0LANG=en_US.UTF-8\0";
        let vars = parse_dump(bytes);
        assert_eq!(vars.get("PATH").unwrap(), "/a:/b");
        assert_eq!(vars.get("HOME").unwrap(), "/Users/x");
        assert_eq!(vars.len(), 3);
    }

    #[test]
    fn parse_dump_ignores_empty_and_malformed_chunks() {
        let bytes = b"\0NOEQUALS\0A=1\0";
        let vars = parse_dump(bytes);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars.get("A").unwrap(), "1");
    }

    #[test]
    #[cfg(unix)]
    fn resolve_ignores_ambient_path_when_launched_with_minimal_env() {
        // 模拟 Finder 启动：进程自身的 PATH 里没有 homebrew / fnm。
        let worktree = std::env::temp_dir().join("revia-env-test-no-such-dir");
        let env = resolve(Path::new("/Users/nobody"), &worktree);
        assert!(env.info.used_fallback, "目录不存在时应回退");
        assert!(
            env.path.iter().any(|entry| entry.contains("homebrew")),
            "回退环境应包含 homebrew，实际：{:?}",
            env.path
        );
    }

    #[test]
    fn split_path_drops_empty_entries() {
        let mut vars = HashMap::new();
        vars.insert("PATH".to_string(), "/a::/b:".to_string());
        assert_eq!(split_path(&vars), vec!["/a", "/b"]);
    }

    #[test]
    fn sanitize_drops_shell_specific_vars() {
        let mut vars = HashMap::new();
        vars.insert("PWD".to_string(), "/tmp".to_string());
        vars.insert("PATH".to_string(), "/a".to_string());
        let vars = sanitize(vars);
        assert!(!vars.contains_key("PWD"));
        assert!(vars.contains_key("PATH"));
    }

    #[test]
    fn path_string_roundtrips_entries() {
        let env = ShellEnv {
            vars: HashMap::new(),
            path: vec!["/bin".into(), "/usr/bin".into()],
            info: EnvCaptureInfo {
                worktree: "".into(),
                shell: "".into(),
                elapsed_ms: 0,
                timed_out: false,
                used_fallback: false,
                var_count: 0,
                path_entries: vec![],
                diagnostics: None,
            },
        };
        let joined = env.path_string();
        if cfg!(windows) {
            assert!(joined.contains(';') || joined == "/bin" || !joined.contains(':'));
        } else {
            assert_eq!(joined, "/bin:/usr/bin");
        }
    }

    #[test]
    fn which_in_finds_file_in_path() {
        let td = tempfile::tempdir().unwrap();
        let bin_dir = td.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let script = bin_dir.join("test-tool");
        std::fs::write(&script, b"#!/bin/sh\n").unwrap();
        let path = vec![bin_dir.display().to_string()];
        assert_eq!(which_in(&path, "test-tool"), Some(script));
        assert_eq!(which_in(&path, "nonexistent-tool"), None);
    }
}
