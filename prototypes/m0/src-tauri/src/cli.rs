//! 运行环境验证：在剥离环境（模拟从 Finder 启动）的进程里跑真实的 git 命令。
//!
//! 两个场景：
//! - `hook`：husky pre-commit hook 里比较 `node -v` 与 `.node-version`，证明拿到的是项目指定版本；
//! - `mergetool`：`git mergetool --tool=vscode` 能找到 `code`（用一个记录调用的替身脚本验证）。
//!
//! 配置通过环境变量：`REVIA_M0_FIXTURES`（fixture 根目录）、`REVIA_M0_WORKTREE`（指定单个 worktree）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::env::{self, ShellEnv};
use crate::error::{AppError, Result};
use crate::git::GitCommand;

pub fn run(mode: &str) -> i32 {
    let result = match mode {
        "hook" => hook_check(),
        "mergetool" => mergetool_check(),
        "env" => env_dump(),
        "highlight" => highlight_dump(),
        other => Err(AppError::rejected(format!(
            "未知的 env-check 模式：{other}（可用：hook、mergetool、env）"
        ))),
    };

    let report = match result {
        Ok(report) => report,
        Err(error) => Report::failed(error.message),
    };
    let text = serde_json::to_string_pretty(&report).unwrap_or_default();
    println!("{text}");
    // 从 Finder 启动时看不到 stdout：如果配置了输出路径，把报告写进文件。
    if let Some(path) = crate::measure::target_path() {
        let _ = crate::measure::write_json(&path, &serde_json::to_value(&report).unwrap_or_default());
    }
    if report.passed() { 0 } else { 1 }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub check: String,
    pub ok: bool,
    pub error: Option<String>,
    pub worktree: Option<String>,
    pub repo: Option<String>,
    pub git_path: Option<String>,
    pub git_version: Option<String>,
    pub shell: Option<String>,
    pub shell_elapsed_ms: Option<u64>,
    pub env_fallback: Option<bool>,
    pub node_path: Option<String>,
    pub node_version: Option<String>,
    pub node_expected: Option<String>,
    pub committed_lines: Option<Vec<String>>,
    pub commit_sha: Option<String>,
    pub passed: Option<bool>,
    pub commit_output: Option<String>,
    /// 启动本进程时继承到的 PATH（用来证明是从 Finder/launchd 启动的残缺环境）。
    pub host_path: Option<String>,
    pub merge_tool: Option<String>,
    pub code_path: Option<String>,
    pub tool_log: Option<Vec<String>>,
    pub merged_content: Option<String>,
}

/// `passed` 字段是场景自身的判定；`ok` 表示流程本身没出错。脚本读 `passed`。
impl Report {
    fn failed(message: String) -> Self {
        Self {
            check: "error".into(),
            ok: false,
            error: Some(message),
            worktree: None,
            repo: None,
            git_path: None,
            git_version: None,
            shell: None,
            shell_elapsed_ms: None,
            env_fallback: None,
            node_path: None,
            node_version: None,
            node_expected: None,
            committed_lines: None,
            commit_sha: None,
            passed: Some(false),
            commit_output: None,
            host_path: std::env::var("PATH").ok(),
            merge_tool: None,
            code_path: None,
            tool_log: None,
            merged_content: None,
        }
    }

    pub fn passed(&self) -> bool {
        self.ok && self.passed.unwrap_or(false)
    }
}

fn fixtures_root() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("REVIA_M0_FIXTURES") {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Ok(path);
        }
        return Err(AppError::rejected(format!(
            "REVIA_M0_FIXTURES 不是目录：{}",
            path.display()
        )));
    }
    Err(AppError::rejected(
        "请设置 REVIA_M0_FIXTURES 指向 fixture 目录（见 fixtures/setup.sh）",
    ))
}

/// 优先使用环境变量指定的 worktree，否则用 fixtures 下的默认仓库。
fn target_worktree(default_name: &str) -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("REVIA_M0_WORKTREE") {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Ok(path);
        }
        return Err(AppError::rejected(format!("worktree 不存在：{}", path.display())));
    }
    let path = fixtures_root()?.join(default_name);
    if !path.is_dir() {
        return Err(AppError::rejected(format!(
            "fixture 不存在：{}（先运行 fixtures/setup.sh）",
            path.display()
        )));
    }
    Ok(path)
}

fn resolve(worktree: &Path) -> Result<(ShellEnv, GitCommand)> {
    let home = env::home_dir()?;
    let shell_env = env::resolve(&home, worktree);
    let git = GitCommand::new(worktree, &shell_env)?;
    Ok((shell_env, git))
}

fn base_report(check: &str, worktree: &Path, shell_env: &ShellEnv, git: &GitCommand) -> Report {
    Report {
        check: check.into(),
        ok: true,
        error: None,
        worktree: Some(worktree.display().to_string()),
        repo: None,
        git_path: Some(git.program.display().to_string()),
        git_version: git.version().ok(),
        shell: Some(shell_env.info.shell.clone()),
        shell_elapsed_ms: Some(shell_env.info.elapsed_ms),
        env_fallback: Some(shell_env.info.used_fallback),
        node_path: None,
        node_version: None,
        node_expected: None,
        committed_lines: None,
        commit_sha: None,
        passed: None,
        commit_output: None,
        host_path: std::env::var("PATH").ok(),
        merge_tool: None,
        code_path: None,
        tool_log: None,
        merged_content: None,
    }
}

/// 输出指定文件的 token 表（每行前若干列的颜色），用于核对两套高亮的正确性。
/// 环境变量：`REVIA_M0_FILE`（文件路径）、`REVIA_M0_LANG`（扩展名）、`REVIA_M0_LINES`（行数，默认 40）。
fn highlight_dump() -> Result<Report> {
    use crate::highlight::{HighlightRequest, Highlighter, LineRange};

    let file = std::env::var("REVIA_M0_FILE")
        .map_err(|_| AppError::rejected("请用 REVIA_M0_FILE 指定要检查的文件"))?;
    let language = std::env::var("REVIA_M0_LANG").unwrap_or_else(|_| {
        file.rsplit('.').next().unwrap_or("txt").to_string()
    });
    let lines: usize = std::env::var("REVIA_M0_LINES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(40);
    let content = std::fs::read_to_string(&file)
        .map_err(|error| AppError::internal(format!("读取 {file} 失败：{error}")))?;

    let highlighter = Highlighter::start();
    let result = highlighter
        .highlight(HighlightRequest {
            language,
            content,
            theme: None,
            line_range: Some(LineRange::new(0, lines)),
            document_id: Some(file.clone()),
            generation: 0,
        })
        .map_err(|error| AppError::internal(format!("高亮失败：{error}")))?;

    let mut report = Report::failed(String::new());
    report.check = "highlight".into();
    report.ok = true;
    report.passed = Some(true);
    report.error = None;
    report.worktree = Some(file);
    report.merge_tool = Some(result.syntax.clone());
    report.tool_log = Some(
        result
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let text = line
                    .as_ref()
                    .map(|tokens| {
                        tokens
                            .iter()
                            .map(|token| format!("{}[{}]", token.text.replace('\n', ""), token.color))
                            .collect::<Vec<_>>()
                            .join("")
                    })
                    .unwrap_or_default();
                format!("{:4} {}", index + 1, text)
            })
            .collect(),
    );
    report.merged_content = Some(format!(
        "syntax={} theme={} elapsed={}ms",
        result.syntax, result.theme, result.elapsed_ms
    ));
    Ok(report)
}

/// 只报告环境解析结果，便于从 Finder 双击时人工核对。
fn env_dump() -> Result<Report> {
    let worktree = target_worktree("node-versioned")?;
    let worktree = std::fs::canonicalize(&worktree).unwrap_or(worktree);
    let (shell_env, git) = resolve(&worktree)?;
    let mut report = base_report("env", &worktree, &shell_env, &git);
    report.repo = git.toplevel().map(|path| path.display().to_string());
    report.passed = Some(true);
    report.node_path = shell_env
        .which("node")
        .map(|path| path.display().to_string());
    report.node_version = shell_env
        .which("node")
        .and_then(|node| run_probe(&node, &["-v"]));
    report.code_path = shell_env.which("code").map(|path| path.display().to_string());
    Ok(report)
}

/// fnm 的默认 Node 版本：无 `.node-version` 时应拿到它。
/// 原型机上的实际值来自 `fnm list` 的 default 标记（见报告）。
fn default_node_version() -> &'static str {
    "26.5.0"
}

fn run_probe(program: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn hook_check() -> Result<Report> {
    let worktree = target_worktree("node-versioned")?;
    let worktree = std::fs::canonicalize(&worktree).unwrap_or(worktree);
    let (shell_env, git) = resolve(&worktree)?;
    let mut report = base_report("hook", &worktree, &shell_env, &git);
    report.repo = git.toplevel().map(|path| path.display().to_string());

    let expected = std::fs::read_to_string(worktree.join(".node-version"))
        .ok()
        .map(|text| text.trim().trim_start_matches('v').to_string());
    report.node_expected = expected.clone();
    report.node_path = shell_env.which("node").map(|path| path.display().to_string());
    report.node_version = shell_env
        .which("node")
        .and_then(|node| run_probe(&node, &["-v"]))
        .map(|version| version.trim_start_matches('v').to_string());

    // 模拟 real commit：写入一个由暂存区内容决定的行，再提交。
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    std::fs::write(
        worktree.join("env-check.txt"),
        format!("env-check {stamp}\n"),
    )?;
    git.run_ok(&["add", "--", "env-check.txt"], "暂存 env-check.txt")?;

    let output = git.run(&[
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-m",
        "env-check",
    ])?;
    report.commit_output = Some(format!(
        "exit={}\n--- stdout ---\n{}--- stderr ---\n{}",
        output.exit_code,
        output.stdout_lossy(),
        output.stderr
    ));

    if output.exit_code == 0 {
        report.commit_sha = git
            .try_run(&["rev-parse", "HEAD"])
            .map(|text| text.trim().to_string());
        let body = git
            .try_run(&["show", "--format=", "-U0", "HEAD", "--", "env-check.txt"])
            .unwrap_or_default();
        report.committed_lines = Some(
            body.lines()
                .filter(|line| line.starts_with('+'))
                .map(|line| line.to_string())
                .collect(),
        );
    }

    // 判定：提交成功、hook 确实跑了、node 版本符合预期。
    let default_version = default_node_version();
    let hook_ran = report
        .commit_output
        .as_deref()
        .map(|text| text.contains("[husky] OK"))
        .unwrap_or(false);
    let version_ok = match expected.as_deref() {
        Some(expected) => report.node_version.as_deref() == Some(expected),
        // 无 .node-version：必须拿到 fnm 的默认版本。
        None => report.node_version.as_deref() == Some(default_version),
    };
    let content_ok = report
        .committed_lines
        .as_ref()
        .map(|lines| lines.iter().any(|line| line.starts_with("+env-check ")))
        .unwrap_or(false);
    report.passed = Some(output.exit_code == 0 && hook_ran && version_ok && content_ok);
    report.ok = true;
    Ok(report)
}

/// 验证 `git mergetool --tool=vscode` 能找到 `code`：
/// 在 PATH 最前面放一个记录调用的 `code` 替身，替身会写 git 期望的合并结果。
fn mergetool_check() -> Result<Report> {
    let fixture = target_worktree("mergetool")?;
    let fixture = std::fs::canonicalize(&fixture).unwrap_or(fixture);

    // 在克隆出来的仓库里制造冲突，不弄脏 fixture 本身；
    // 克隆也保证 `git mergetool` 面对的是一个全新的冲突现场。
    let keep_sandbox = std::env::var("REVIA_M0_KEEP_SANDBOX").is_ok();
    let sandbox = if keep_sandbox {
        tempfile::Builder::new()
            .prefix("revia-m0-mergetool-")
            .tempdir_in("/tmp")
            .map_err(|error| AppError::internal(format!("创建临时目录失败：{error}")))?
    } else {
        tempfile::tempdir().map_err(|error| AppError::internal(format!("创建临时目录失败：{error}")))?
    };
    let sandbox_path = sandbox.path().to_path_buf();
    let worktree = sandbox.path().join("repo");
    let clone = Command::new("/usr/bin/git")
        .args(["clone", "--quiet", "--local"])
        .arg(&fixture)
        .arg(&worktree)
        .env_clear()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| AppError::environment(format!("执行 git clone 失败：{error}")))?;
    if !clone.status.success() {
        return Err(AppError::git(format!(
            "克隆 fixture 失败：{}",
            String::from_utf8_lossy(&clone.stderr).trim()
        )));
    }

    let (shell_env, git) = resolve(&worktree)?;
    let mut report = base_report("mergetool", &worktree, &shell_env, &git);
    report.repo = git.toplevel().map(|path| path.display().to_string());
    report.code_path = shell_env.which("code").map(|path| path.display().to_string());

    // 克隆出来的仓库没有 fixture 的本地 user 配置，这里显式补上，避免依赖全局配置。
    for (key, value) in [("user.name", "Revia M0 Env Check"), ("user.email", "env-check@revia.local")] {
        git.run_ok(&["config", key, value], "写入克隆仓库的用户配置")?;
    }

    let tool = git.config_get("merge.tool").unwrap_or_else(|| "vscode".to_string());
    report.merge_tool = Some(tool.clone());

    let default_branch = git
        .try_run(&["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|text| text.trim().to_string())
        .filter(|name| !name.is_empty() && name != "HEAD")
        .ok_or_else(|| AppError::git("fixture 仓库没有可用的分支"))?;

    // 两边都改 README.txt，制造真实冲突。
    let branch = "env-check-other";
    write_file(&worktree, "README.txt", "base v2 line\n")?;
    git.run_ok(&["add", "--", "README.txt"], "暂存 README.txt")?;
    git.run_ok(
        &["-c", "commit.gpgsign=false", "commit", "-m", "base"],
        "提交基础版本",
    )?;
    git.run_ok(&["checkout", "-q", "-b", branch], "创建临时分支")?;
    write_file(&worktree, "README.txt", "other branch line\n")?;
    git.run_ok(&["add", "--", "README.txt"], "暂存分支改动")?;
    git.run_ok(
        &["-c", "commit.gpgsign=false", "commit", "-m", "other"],
        "提交分支改动",
    )?;
    git.run_ok(&["checkout", "-q", &default_branch], "回到默认分支")?;
    write_file(&worktree, "README.txt", "head line\n")?;
    git.run_ok(&["add", "--", "README.txt"], "暂存当前分支改动")?;
    git.run_ok(
        &["-c", "commit.gpgsign=false", "commit", "-m", "head"],
        "提交当前分支改动",
    )?;
    let merge = git.run(&["merge", "--no-commit", branch])?;
    if merge.exit_code == 0 {
        return Err(AppError::git("fixture 没有产生冲突，无法验证 mergetool"));
    }

    // 替身 code：记录调用参数，并把“合并结果”写到 git 给的 merged 文件。
    let shim_dir = sandbox.path().join("shim");
    std::fs::create_dir_all(&shim_dir)?;
    let log_path = shim_dir.join("tool.log");
    let shim_path = shim_dir.join("code");
    // 替身必须模拟一个真实合并工具的行为：把 LOCAL 版本写入 MERGED。
    // git 调用外部工具的约定是最后四个参数依次为 REMOTE / LOCAL / BASE / MERGED，
    // 前面可能有工具自己的选项（vscode 是 `--wait --merge`），所以从末尾数。
    // git 约定：最后四个参数依次是 REMOTE / LOCAL / BASE / MERGED，前面可能有工具选项
    // （vscode 是 `--wait --merge`）。用滚动记录取末位参数，避免依赖 eval / 位置参数展开。
    let script = format!(
        r#"#!/bin/sh
log={log}
echo "$@" >> "$log"
prev3=; prev2=; prev1=
for arg in "$@"; do
  prev3=$prev2; prev2=$prev1; prev1=$arg
done
merged=$prev1
local=$prev3
if [ -n "$local" ] && [ -f "$local" ]; then
  cp "$local" "$merged" 2>> "$log" || echo "cp 失败" >> "$log"
else
  echo "找不到 local：$local" >> "$log"
fi
printf '%s\n' "merged by env-check stub" >> "$merged" 2>> "$log"
# git 用 `MERGED -nt BACKUP` 判断工具是否改过文件，而 macOS 的 /bin/sh(bash 3.2)
# 只按整秒比较；真实 GUI 工具通常要一两秒，这里显式等待以模拟并跨过整秒边界。
sleep 1.2
touch "$merged"
echo "cwd=$(pwd) args=$# local=$local merged=$merged" >> "$log"
"#,
        log = log_path.display()
    );
    std::fs::write(&shim_path, &script)?;
    // 把替身脚本原文写进日志，便于核对它实际收到的参数处理方式。
    let _ = crate::measure::append_log(&log_path, &format!("--- shim 脚本原文 ---\n{script}"));
    let mut permissions = std::fs::metadata(&shim_path)?.permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&shim_path, permissions)?;

    let mut env_vars: HashMap<String, String> = shell_env.vars.clone();
    env_vars.insert(
        "PATH".into(),
        format!("{}:{}", shim_dir.display(), shell_env.path_string()),
    );

    let mut command = Command::new(&git.program);
    command
        .current_dir(&worktree)
        .args([
            "-c",
            "mergetool.prompt=false",
            "mergetool",
            "--no-prompt",
            &format!("--tool={tool}"),
        ])
        .env_clear()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in &env_vars {
        command.env(key, value);
    }
    let output = command
        .output()
        .map_err(|error| AppError::environment(format!("执行 git mergetool 失败：{error}")))?;

    if keep_sandbox {
        eprintln!("沙盒保留在 {}", sandbox_path.display());
    }
    report.tool_log = std::fs::read_to_string(&log_path)
        .ok()
        .map(|text| text.lines().map(|line| line.to_string()).collect());
    report.merged_content = std::fs::read_to_string(worktree.join("README.txt")).ok();
    report.commit_output = Some(format!(
        "exit={}\n--- stdout ---\n{}--- stderr ---\n{}",
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    ));

    let resolved = report
        .merged_content
        .as_deref()
        .map(|content| {
            content.contains("merged by env-check stub")
                && !content.contains("<<<<<<<")
                && !content.contains(">>>>>>>")
        })
        .unwrap_or(false);
    report.passed = Some(output.status.success() && resolved && report.tool_log.is_some());
    report.ok = true;
    Ok(report)
}

fn write_file(worktree: &Path, name: &str, content: &str) -> Result<()> {
    std::fs::write(worktree.join(name), content)
        .map_err(|error| AppError::internal(format!("写入 {name} 失败：{error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_node_version_is_pinned_to_the_fixture_default() {
        // 改动这个值必须同步 fixtures/setup.sh 的 NODE_DEFAULT 和报告。
        assert_eq!(default_node_version(), "26.5.0");
    }

    #[test]
    fn report_passed_requires_ok_and_passed() {
        let mut report = Report::failed("boom".into());
        assert!(!report.passed());
        report.ok = true;
        report.passed = Some(true);
        assert!(report.passed());
    }
}
