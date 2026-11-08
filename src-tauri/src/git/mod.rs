//! Git 调用：只走命令行（ADR-0002），统一使用 worktree 解析出的环境（Q21）。
//!
//! 约束：
//! - 前端不能传入任意 Git 参数，也不能传入未登记的路径；
//! - 固定参数 `--no-color --no-ext-diff --no-textconv --src-prefix=a/ --dst-prefix=b/`；
//! - 路径参数一律放在 `--` 之后；
//! - 返回路径的命令固定带 `-c core.quotePath=false`，避免非 ASCII 路径被引号转义；
//! - 遇到 `index.lock` 被占用只重试和上报，绝不删除锁文件（Q26）；
//! - 每条命令都记入 Git 命令日志（Q36），M1 只记录，面板在 M4。

pub mod branch;
pub mod diff;
pub mod highlight;
pub mod history;
pub mod rows;
pub mod segments;
pub mod status;
pub(crate) mod stash_lock;

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::git::segments::IgnoreWhitespace;
use sha2::{Digest, Sha256};
use wait_timeout::ChildExt;

use crate::error::{AppError, ErrorKind, Result};
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
/// 网络操作（fetch / pull / push / 检查远端）单独的上限（Q96、规格 B01）。
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(600);
const LOCK_RETRIES: u32 = 3;
const LOCK_RETRY_DELAY: Duration = Duration::from_millis(120);
/// 命令日志只保留最近 1000 条（规格 B07）；只在内存里，不写磁盘。
const LOG_CAPACITY: usize = 1000;
/// 单条命令保留的输出上限（64 KB，规格 B07），超出时标明截断。
pub const LOG_OUTPUT_LIMIT: usize = 64 * 1024;

/// 一条 Git 命令日志（只保存在本机，Q36、Q42、B07）。
///
/// 记录时间、所属 worktree、命令与参数、退出码、耗时与输出；参数与输出都做过
/// 凭据打码（URL 里的用户名 / 密码 / 令牌、携带认证信息的参数值）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRecord {
    pub sequence: u64,
    /// 发起这条命令的 worktree（不属于任何 worktree 的探测命令为 null）。
    pub worktree_id: Option<String>,
    /// 命令开始时的 Unix 毫秒时间。
    pub started_at_ms: u64,
    pub program: String,
    /// 打码后的参数。
    pub args: Vec<String>,
    pub directory: String,
    pub exit_code: i32,
    pub elapsed_ms: u64,
    pub timed_out: bool,
    pub stdout_bytes: usize,
    pub stderr_excerpt: String,
    /// 合并后的标准输出与错误输出（打码、最多 64 KB）。
    pub output: String,
    /// 输出是否因为超过上限被截断。
    pub output_truncated: bool,
}

#[derive(Debug, Default)]
pub struct CommandLog {
    inner: Mutex<LogInner>,
    diagnostics: Option<Arc<crate::m7::diagnostics::DiagnosticsSummaryStore>>,
}

#[derive(Debug, Default)]
struct LogInner {
    next_sequence: u64,
    records: VecDeque<CommandRecord>,
}

impl CommandLog {
    pub(crate) fn with_diagnostics_snapshot<T>(&self, read: impl FnOnce(&[CommandRecord]) -> Result<T>) -> Result<T> {
        let inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let records: Vec<_> = inner.records.iter().cloned().collect();
        read(&records)
    }

    pub fn with_diagnostics(store: Arc<crate::m7::diagnostics::DiagnosticsSummaryStore>) -> Self {
        Self { inner: Mutex::new(LogInner::default()), diagnostics: Some(store) }
    }

    pub fn clear_diagnostics(&self, worktree_id: Option<&str>) -> Result<usize> {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let removed = match &self.diagnostics {
            Some(store) => store.clear(worktree_id)?,
            None => 0,
        };
        inner.records.retain(|record| record.worktree_id.as_deref() != worktree_id);
        Ok(removed)
    }

    pub fn record(&self, record: CommandRecord) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.next_sequence += 1;
            let mut record = record;
            record.sequence = inner.next_sequence;
            if let Some(store) = &self.diagnostics { store.observe(&record); }
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
    pub fn into_success(self, _what: &str) -> Result<Self> {
        if self.exit_code == 0 {
            Ok(self)
        } else {
            // `what` used to be free-form, often Chinese, text.  Keeping it
            // out of the payload lets the frontend translate the whole error
            // and leaves stderr (untrusted Git output) as the only detail.
            Err(AppError::coded(
                ErrorKind::Git,
                "errors.mod.failedExitCode",
                "Git 命令失败（退出码 {0}）：{1}",
                &[
                    ("0", format!("{:}", self.exit_code)),
                    ("1", format!("{:}", self.stderr.trim())),
                ],
            ))
        }
    }
}

/// 一次 Git 调用的上下文：程序、工作目录、环境、命令日志。
#[derive(Debug, Clone)]
pub struct Git {
    pub program: PathBuf,
    pub directory: PathBuf,
    /// 子进程环境；打开 worktree 后可能在后台换成完整登录环境（Q212），
    /// 因此用读写锁而不是一次性快照。
    pub env: Arc<std::sync::RwLock<Vec<(String, String)>>>,
    pub log: Arc<CommandLog>,
    /// 发起调用的 worktree（命令日志归属，规格 B07）；不属于 worktree 的探测为 None。
    pub worktree_id: Option<String>,
}

impl Git {
    pub fn new(directory: &Path, shell_env: &ShellEnv, log: Arc<CommandLog>) -> Result<Self> {
        Self::with_program(directory, shell_env, log, None)
    }

    /// 用设置中手动指定的 Git 程序（Q27），未指定时使用登录 shell `PATH` 中的 git。
    pub fn with_program(
        directory: &Path,
        shell_env: &ShellEnv,
        log: Arc<CommandLog>,
        program: Option<&Path>,
    ) -> Result<Self> {
        let program = match program {
            Some(program) => program.to_path_buf(),
            None => shell_env.which("git").ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Environment,
                    "errors.mod.gitFoundLoginShellPath",
                    "登录 shell 的 PATH 中找不到 git",
                    &[],
                )
            })?,
        };
        Ok(Self {
            program,
            directory: directory.to_path_buf(),
            env: Arc::new(std::sync::RwLock::new(
                shell_env
                    .vars
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            )),
            log,
            worktree_id: None,
        })
    }

    /// 标注命令日志的 worktree 归属（会话打开时一次）。
    pub fn with_worktree_id(&self, worktree_id: &str) -> Self {
        let mut next = self.clone();
        next.worktree_id = Some(worktree_id.to_string());
        next
    }

    /// 用同一套环境在别的目录执行（例如对文件直接做 `--no-index` 比较）。
    pub fn in_directory(&self, directory: &Path) -> Self {
        Self {
            program: self.program.clone(),
            directory: directory.to_path_buf(),
            env: Arc::clone(&self.env),
            log: self.log.clone(),
            worktree_id: self.worktree_id.clone(),
        }
    }

    /// 读取当前子进程环境的快照（消除 Shotgun Surgery）。
    pub fn env_snapshot(&self) -> Vec<(String, String)> {
        self.env
            .read()
            .map(|env| env.clone())
            .unwrap_or_default()
    }

    /// 返回一个带额外环境变量的副本；同名变量以新值为准，不重复追加。
    ///
    /// 用于只读历史读取的两个 Git 行为开关（不改工作区读取）：
    /// - `GIT_NO_LAZY_FETCH=1`（Git 2.44+）：部分克隆里缺失对象不再自动向远端补取；
    /// - `GIT_ATTR_SOURCE=<treeish>`（Git 2.40+）：`.gitattributes` 取自指定提交的树，
    ///   而不是当前工作区。
    ///
    /// 低版本 Git 会忽略不认识的 `GIT_*` 变量，行为退回旧版默认（见报告已知限制）。
    pub fn with_env_vars(&self, vars: &[(&str, &str)]) -> Self {
        let mut env = self.env_snapshot();
        for (key, value) in vars {
            match env
                .iter_mut()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
            {
                Some(slot) => slot.1 = (*value).to_string(),
                None => env.push(((*key).to_string(), (*value).to_string())),
            }
        }
        Self {
            program: self.program.clone(),
            directory: self.directory.clone(),
            env: Arc::new(std::sync::RwLock::new(env)),
            log: self.log.clone(),
            worktree_id: self.worktree_id.clone(),
        }
    }

    /// 在**完全隔离**的目录里执行：去掉所有把 Git 定位或配置重定向到别处的环境变量。
    ///
    /// 只改工作目录不够：从设置了 `GIT_DIR` / `GIT_WORK_TREE` 的终端启动时（Windows
    /// 分支会继承进程全部环境），`git init` 仍会作用在原仓库上，等于修改用户的 Git 配置。
    /// 这里按大小写不敏感的方式剔除（Windows 环境变量名不区分大小写）。
    pub fn in_isolated_directory(&self, directory: &Path) -> Self {
        const REDIRECT_VARS: &[&str] = &[
            "GIT_DIR",
            "GIT_WORK_TREE",
            // 配置重定向：`GIT_CONFIG` 指向单个配置文件，`GIT_CONFIG_COUNT` /
            // `GIT_CONFIG_PARAMETERS` / `GIT_CONFIG_KEY_*` 可在环境里注入配置。
            // 保留任一都会让 scratch 的 `config` 写进用户配置。
            "GIT_CONFIG",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_COMMON_DIR",
            "GIT_NAMESPACE",
            "GIT_PREFIX",
            "GIT_CEILING_DIRECTORIES",
            "GIT_DISCOVERY_ACROSS_FILESYSTEM",
            "GIT_SHALLOW_FILE",
            "GIT_GRAFT_FILE",
            "GIT_REPLACE_REF_BASE",
        ];
        let current = self.env_snapshot();
        let env = current
            .iter()
            .filter(|(key, _)| {
                if key.to_ascii_uppercase().starts_with("GIT_CONFIG_KEY_")
                    || key.to_ascii_uppercase().starts_with("GIT_CONFIG_VALUE_")
                {
                    return false;
                }
                !REDIRECT_VARS
                    .iter()
                    .any(|name| key.eq_ignore_ascii_case(name))
            })
            .cloned()
            .collect();
        Self {
            program: self.program.clone(),
            directory: directory.to_path_buf(),
            env: Arc::new(std::sync::RwLock::new(env)),
            log: self.log.clone(),
            worktree_id: self.worktree_id.clone(),
        }
    }

    /// 换成完整登录环境（Q212）：只替换环境变量，Git 程序路径变化由会话层换会话。
    pub fn set_env(&self, shell_env: &ShellEnv) {
        let mut vars: Vec<(String, String)> = shell_env
            .vars
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        // PATH 以 `ShellEnv::path` 为准：`vars` 里可能还是打开时的旧值。
        match vars.iter_mut().find(|(key, _)| key == "PATH") {
            Some(slot) => slot.1 = shell_env.path_string(),
            None => vars.push(("PATH".into(), shell_env.path_string())),
        }
        if let Ok(mut env) = self.env.write() {
            *env = vars;
        }
    }

    pub fn version(&self) -> Result<String> {
        let output = self.run_ok(&["--version"], "读取 git 版本")?;
        Ok(output.stdout_lossy().trim().to_string())
    }

    /// 执行一次 Git 调用，遇到 `index.lock` 时短暂重试。
    pub fn run(&self, args: &[&str]) -> Result<GitOutput> {
        self.run_with_timeout(args, GIT_TIMEOUT)
    }

    /// 通过 stdin 传入数据的只读调用（例如 `check-ignore --stdin`，M9 任务 04）。
    pub fn run_stdin(&self, args: &[&str], input: Vec<u8>) -> Result<GitOutput> {
        self.run_with_options(args, Some(input), GIT_TIMEOUT, None, true)
    }

    /// 带显式超时的一次调用（网络操作用 10 分钟上限，规格 B01）。
    pub fn run_with_timeout(&self, args: &[&str], timeout: Duration) -> Result<GitOutput> {
        self.run_with_options(args, None, timeout, None, true)
    }

    /// Mutations run once: an abnormal exit may follow a committed write.
    /// Positive exits remain available to callers handling real conflict stops.
    pub fn run_write(&self, args: &[&str]) -> Result<GitOutput> {
        self.run_write_with_timeout(args, GIT_TIMEOUT)
    }

    pub fn run_write_with_timeout(&self, args: &[&str], timeout: Duration) -> Result<GitOutput> {
        let output = self.run_with_options(args, None, timeout, None, false)
            .map_err(crate::recovery::pending_write)?;
        if output.timed_out || output.exit_code < 0 {
            let cause = AppError::coded(ErrorKind::Git, "errors.mod.failedExitCode",
                "Git 命令失败（退出码 {0}）：{1}",
                &[("0", output.exit_code.to_string()), ("1", output.stderr.trim().to_string())]);
            return Err(crate::recovery::pending_write(cause));
        }
        Ok(output)
    }

    /// 运行可能长时间停留的外部工具（T03）。
    ///
    /// 复用同一套环境、命令日志和超时回收路径，但把 stdout/stderr 在读取时
    /// 就限制在上限内，避免外部工具刷屏时把整个输出驻留内存。
    pub fn run_external_tool(
        &self,
        args: &[&str],
        timeout: Duration,
        output_limit: usize,
    ) -> Result<GitOutput> {
        self.run_with_options(args, None, timeout, Some(output_limit), false)
    }

    fn run_with_options(
        &self,
        args: &[&str],
        input: Option<Vec<u8>>,
        timeout: Duration,
        output_limit: Option<usize>,
        retry_lock: bool,
    ) -> Result<GitOutput> {
        let mut last: Option<GitOutput> = None;
        let attempts = if retry_lock { LOCK_RETRIES + 1 } else { 1 };
        for attempt in 0..attempts {
            let output = self.run_once_with_timeout(args, input.clone(), timeout, output_limit, true)?;
            if !retry_lock || !is_lock_error(&output) {
                return Ok(output);
            }
            last = Some(output);
            if attempt + 1 < attempts {
                sleep(LOCK_RETRY_DELAY * (attempt + 1));
            }
        }
        Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.mod.theGitIndexLockedAnotherProcess",
            "Git 索引被其他进程占用（index.lock），已重试 {LOCK_RETRIES} 次：{0}",
            &[
                (
                    "0",
                    format!(
                        "{:}",
                        last.map(|out| out.stderr.trim().to_string())
                            .unwrap_or_default()
                    ),
                ),
                ("LOCK_RETRIES", format!("{}", LOCK_RETRIES)),
            ],
        ))
    }

    /// 网络命令（fetch / pull / push / 检查远端）：10 分钟上限，且不等待任何输入。
    ///
    /// 凭据打码与「不进入交互」在这里统一处理（Q153、规格 B01）：
    /// - 关闭 Git 自己的终端提示（`GIT_TERMINAL_PROMPT=0`），标准输入为空；
    /// - `SSH_ASKPASS_REQUIRE=never` 防止 SSH 弹出图形化询问；
    /// - **不**覆盖用户在环境或配置里的 ssh 命令与 askpass 程序，也不改 Git 配置。
    pub fn run_network(&self, args: &[&str], timeout: Duration) -> Result<GitOutput> {
        self.with_env_vars(&[
            ("GIT_TERMINAL_PROMPT", "0"),
            ("SSH_ASKPASS_REQUIRE", "never"),
        ])
        .run_with_timeout(args, timeout)
    }

    /// 敏感调用（例如 AI 发送内容生成）：执行命令，但**绝不把输出正文记录到日志**（Q180、Q214）。
    pub fn run_private_ok(&self, args: &[&str], what: &str) -> Result<GitOutput> {
        let output = self.run_once_with_timeout(args, None, GIT_TIMEOUT, None, false)?;
        output.into_success(what)
    }

    pub fn run_ok(&self, args: &[&str], what: &str) -> Result<GitOutput> {
        self.run(args)?.into_success(what)
    }

    /// 带 stdin 的一次调用（只用于只读计算，例如按仓库哈希算法求空 tree）。
    pub fn run_with_input(&self, args: &[&str], input: Vec<u8>) -> Result<GitOutput> {
        self.run_once_with_timeout(args, Some(input), GIT_TIMEOUT, None, true)
    }

    /// 允许非零退出码（例如 `git diff --no-index` 在有差异时返回 1）。
    pub fn run_allow_difference(&self, args: &[&str], _what: &str) -> Result<GitOutput> {
        let output = self.run(args)?;
        if output.exit_code == 0 || output.exit_code == 1 {
            Ok(output)
        } else {
            Err(AppError::coded(
                ErrorKind::Git,
                "errors.mod.failedExitCode2",
                "Git 命令失败（退出码 {0}）：{1}",
                &[
                    ("0", format!("{:}", output.exit_code)),
                    ("1", format!("{:}", output.stderr.trim())),
                ],
            ))
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
        self.run_once_with_timeout(args, None, GIT_TIMEOUT, None, true)
    }

    fn run_once_with_timeout(
        &self,
        args: &[&str],
        input: Option<Vec<u8>>,
        timeout: Duration,
        output_limit: Option<usize>,
        capture_log_output: bool,
    ) -> Result<GitOutput> {
        let mut timing = crate::commands::MeasureSpan::new("git.run");
        let started = Instant::now();
        let started_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_millis() as u64)
            .unwrap_or_default();
        let has_input = input.is_some();
        let (stdout_pipe, stdout_writer) = crate::platform::output_pipe::OutputPipe::new()?;
        let (stderr_pipe, stderr_writer) = crate::platform::output_pipe::OutputPipe::new()?;
        let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut command = Command::new(&self.program);
        command
            .current_dir(&self.directory)
            .args(args)
            .stdin(if has_input {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(stdout_writer)
            .stderr(stderr_writer)
            .env_clear();
        let env = self.env_snapshot();
        for (key, value) in env.iter() {
            command.env(key, value);
        }
        crate::platform::hide_console(&mut command);

        let mut child = command.spawn().map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => AppError::coded(
                ErrorKind::Environment,
                "errors.mod.gitProgramFound",
                "找不到 git 程序：{0}",
                &[("0", format!("{:}", self.program.display()))],
            ),
            _ => AppError::coded(
                ErrorKind::Environment,
                "errors.mod.failedRunGit",
                "执行 git 失败：{error}",
                &[("error", format!("{}", error))],
            ),
        })?;

        // 三条管道并发处理；stdin 大补丁也必须受同一超时约束。
        drop(command);
        let stdout_finished = finished.clone(); let stderr_finished = finished.clone();
        let stdout_reader = std::thread::spawn(move || read_pipe(stdout_pipe, output_limit, stdout_finished));
        let stderr_reader = std::thread::spawn(move || read_pipe(stderr_pipe, output_limit, stderr_finished));
        let stdin_writer = input.map(|bytes| {
            let pipe = child.stdin.take();
            std::thread::spawn(move || -> Result<()> {
                let mut pipe = pipe.ok_or_else(|| AppError::coded(ErrorKind::Internal, "errors.mod.stdinUnavailable", "Git stdin 管道不可用", &[]))?;
                pipe.write_all(&bytes).map_err(AppError::from)
            })
        });

        let waited = child.wait_timeout(timeout.saturating_sub(started.elapsed()));
        let timed_out = matches!(&waited, Ok(None));
        if timed_out || waited.is_err() {
            let _ = child.kill();
        }
        let status = child.wait();
        finished.store(true, std::sync::atomic::Ordering::Release);
        // 包括超时/中断路径：先结束子进程，再回收全部管道线程。
        let input_error = stdin_writer.and_then(|writer| match writer.join() {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error),
            Err(_) => Some(AppError::coded(ErrorKind::Internal, "errors.mod.stdinInterrupted", "Git stdin 写入线程中断", &[])),
        });
        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr_bytes = stderr_reader.join().unwrap_or_default();
        let elapsed = started.elapsed();
        let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
        let exit_code = status
            .as_ref()
            .ok()
            .and_then(|value| value.code())
            .unwrap_or(-1);
        let (output, output_truncated) = if capture_log_output {
            command_output(&stdout, &stderr_bytes)
        } else {
            (String::new(), false)
        };

        timing.mark("process_collected");
        self.log.record(CommandRecord {
            sequence: 0, // 完成入口 record 持锁分配；管道读取前不提前等待日志锁。
            worktree_id: self.worktree_id.clone(),
            started_at_ms,
            program: self.program.display().to_string(),
            args: args.iter().map(|arg| mask_credentials(arg)).collect(),
            directory: self.directory.display().to_string(),
            exit_code,
            elapsed_ms: elapsed.as_millis() as u64,
            timed_out,
            stdout_bytes: stdout.len(),
            stderr_excerpt: truncate(&mask_credentials(stderr.trim()), 2000),
            output,
            output_truncated,
        });

        timing.mark("command_recorded");
        if let Some(error) = waited.err().or_else(|| status.err()) {
            return Err(if has_input {
                AppError::coded(
                    ErrorKind::NeedsVerification,
                    "errors.mod.failedWhileWaitingGitWriteResult",
                    "等待 Git 写入结果失败，结果待核对：{error}",
                    &[("error", format!("{}", error))],
                )
            } else {
                AppError::coded(
                    ErrorKind::Environment,
                    "errors.mod.failedWhileWaitingGitExit",
                    "等待 git 结束失败：{error}",
                    &[("error", format!("{}", error))],
                )
            });
        }
        if exit_code == 0 {
            if let Some(error) = input_error {
                return Err(AppError::coded(
                    ErrorKind::NeedsVerification,
                    "errors.mod.couldConfirmPatchFullyWrittenResult",
                    "无法确认补丁完整写入，结果待核对：{cause}",
                    &[],
                ).cause(error));
            }
        }
        // Git 因锁占用等先退出时，stdin 的 BrokenPipe 不覆盖真实退出原因。
        Ok(GitOutput {
            stdout,
            stderr,
            exit_code,
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
                &[
                    "-c",
                    "core.quotePath=false",
                    "worktree",
                    "list",
                    "--porcelain",
                ],
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
        // 空范围 / "index" / ":" 是暂存区 stage 0；纯数字（"0"/"1"/"2"/"3"）是
        // 合并场景的 stage N（返修 A1/Q143 用）；其余按引用处理（HEAD、分支名）。
        let spec = if scope.is_empty() || scope == "index" || scope == ":" {
            format!(":0:{path}")
        } else if scope.chars().all(|ch| ch.is_ascii_digit()) && !scope.is_empty() {
            format!(":{scope}:{path}")
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

    /// 读取某个前缀下的全部配置项，返回（键, 值）。
    ///
    /// 必须使用 `--null`：配置值可以合法包含换行（例如 `smudge = ":\ncat"`），
    /// 按行拆分会把多行命令截断，复制到预览仓库后与原仓库语义不同。
    pub fn config_entries(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        let pattern = format!("^{prefix}");
        let output = self.run(&["config", "--null", "--get-regexp", &pattern])?;
        if output.exit_code == 1 {
            return Ok(Vec::new());
        }
        let output = output.into_success("读取 Git 配置")?;
        Ok(parse_config_null(&output.stdout))
    }

    /// Git 自己解析出的真实路径（例如 `info/attributes` 在 linked worktree 下位于 commonDir）。
    /// 返回值可能是相对路径，调用方按工作目录解析。
    pub fn git_path(&self, name: &str) -> Option<PathBuf> {
        let text = self.try_run(&["rev-parse", "--git-path", name])?;
        let path = text.trim();
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    /// `git config --get`；未设置时返回 None。
    pub fn config_get(&self, key: &str) -> Option<String> {
        let text = self.try_run(&["config", "--get", key])?;
        let value = text.trim();
        (!value.is_empty()).then(|| value.to_string())
    }

    pub fn changelist_base(&self) -> Result<String> {
        if let Some(head) = crate::commit::read_head(self)? { return Ok(head); }
        Ok(self.run_with_input(&["hash-object", "-t", "tree", "--stdin"], Vec::new())?.into_success("读取空树标识")?.stdout_lossy().trim().to_string())
    }

    /// 更改列表的固定比较基线；所有预览、选择与指纹复用同一组 Git 参数。
    pub fn diff_changelist(&self, path: Option<&str>, ignore: IgnoreWhitespace) -> Result<Vec<u8>> {
        let base = self.changelist_base()?;
        let mut args = vec!["--literal-pathspecs", "-c", "core.quotePath=false", "diff"];
        args.extend_from_slice(DIFF_FLAGS);
        args.extend_from_slice(ignore.diff_args());
        if ignore.ignore_blank_lines() { args.push("--ignore-blank-lines"); }
        args.extend_from_slice(&["--no-renames", "--binary", base.as_str(), "--"]);
        if let Some(path) = path { args.push(path); }
        Ok(self.run_ok(&args, "读取更改列表 Git diff")?.stdout)
    }

    /// 工作区相对 HEAD 的 diff（冲突文件的只读对照，设计 3.2）。
    pub fn diff_head(&self, path: &str) -> Result<Vec<u8>> {
        self.diff(Some("HEAD"), Some(path))
    }

    /// 工作区相对暂存区的 diff（未暂存改动）。
    pub fn diff_worktree(&self, path: Option<&str>) -> Result<Vec<u8>> {
        self.diff(None, path)
    }

    /// 带忽略空白档位的未暂存 diff（返修 Q140）：结果只用于显示与段计算，
    /// 指纹仍由普通 diff 计算，两者分开。
    pub fn diff_worktree_with_ignore(
        &self,
        path: Option<&str>,
        ignore: IgnoreWhitespace,
    ) -> Result<Vec<u8>> {
        self.diff_with_extra(None, path, ignore.diff_args(), ignore.ignore_blank_lines())
    }

    /// 带忽略空白档位的已暂存 diff（返修 Q140）。
    pub fn diff_index_with_ignore(
        &self,
        path: &str,
        orig_path: Option<&str>,
        ignore: IgnoreWhitespace,
    ) -> Result<Vec<u8>> {
        let mut args: Vec<&str> = vec!["--literal-pathspecs", "-c", "core.quotePath=false", "diff"];
        args.extend_from_slice(DIFF_FLAGS);
        args.extend_from_slice(ignore.diff_args());
        if ignore.ignore_blank_lines() {
            args.push("--ignore-blank-lines");
        }
        args.push("--staged");
        args.push("--");
        if let Some(orig) = orig_path {
            args.push(orig);
        }
        args.push(path);
        Ok(self.run_ok(&args, "读取已暂存 diff")?.stdout)
    }

    fn diff_with_extra(
        &self,
        extra: Option<&str>,
        path: Option<&str>,
        ignore_args: &[&str],
        ignore_blank_lines: bool,
    ) -> Result<Vec<u8>> {
        let mut args: Vec<&str> = Vec::new();
        if path.is_some() {
            args.push("--literal-pathspecs");
        }
        args.extend_from_slice(&["-c", "core.quotePath=false", "diff"]);
        args.extend_from_slice(DIFF_FLAGS);
        args.extend_from_slice(ignore_args);
        if ignore_blank_lines {
            args.push("--ignore-blank-lines");
        }
        if let Some(extra) = extra {
            args.push(extra);
        }
        args.push("--");
        if let Some(path) = path {
            args.push(path);
        }
        Ok(self.run_ok(&args, "读取 diff")?.stdout)
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
        let mut args = vec![
            "--literal-pathspecs",
            "-c",
            "core.quotePath=false",
            "add",
            "-A",
            "--",
        ];
        if let Some(orig) = orig_path {
            args.push(orig);
        }
        args.push(path);
        self.run_once(&args)
    }

    /// 整文件取消暂存：调用 git --literal-pathspecs reset -- <paths>（Q8、Q79、T04、审查 1）。
    pub fn unstage_file(&self, path: &str, orig_path: Option<&str>) -> Result<GitOutput> {
        let mut args = vec![
            "--literal-pathspecs",
            "-c",
            "core.quotePath=false",
            "reset",
            "--",
        ];
        if let Some(orig) = orig_path {
            args.push(orig);
        }
        args.push(path);
        self.run_once(&args)
    }

    /// 单次应用已校验的原始 hunk，只写暂存区；锁重试由会话层重新校验后决定。
    /// `ignore_whitespace`：忽略空白档位下按段写时补丁来自 -w diff，应用加 --ignore-whitespace（返修 Q140）。
    pub fn apply_index_patch(
        &self,
        patch: Vec<u8>,
        reverse: bool,
        ignore_whitespace: bool,
    ) -> Result<GitOutput> {
        let mut args = vec![
            "-c",
            "apply.ignoreWhitespace=no",
            "apply",
            "--cached",
            "--whitespace=nowarn",
        ];
        if ignore_whitespace {
            args.push("--ignore-whitespace");
        }
        if reverse {
            args.push("--reverse");
        }
        args.push("-");
        self.run_once_with_timeout(&args, Some(patch), GIT_TIMEOUT, None, true)
    }

    pub fn discard_patch(&self, path: &str) -> Result<Vec<u8>> {
        let mut args = vec!["--literal-pathspecs", "-c", "core.quotePath=false", "diff"];
        args.extend_from_slice(DIFF_FLAGS);
        args.extend_from_slice(&["--binary", "--full-index", "--", path]);
        Ok(self.run_ok(&args, "读取完整丢弃补丁")?.stdout)
    }

    /// No --index/--cached/--3way: discarding must not change the index.
    pub fn apply_worktree_patch(&self, patch: Vec<u8>) -> Result<GitOutput> {
        self.run_once_with_timeout(
            &[
                "-c",
                "apply.ignoreWhitespace=no",
                "apply",
                "--reverse",
                "--whitespace=nowarn",
                "-",
            ],
            Some(patch),
            GIT_TIMEOUT,
            None,
            true,
        )
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
    /// 两个目录外文件之间的只读 diff（用于回收区「保存版本 vs 当前内容」）。
    ///
    /// 走 `git diff --no-index`，因此 hunk 仍由 Git 生成（ADR-0004）；
    /// 两个文件都在临时目录里，不往用户仓库写任何东西。
    pub fn diff_no_index(&self, old: &Path, new: &Path) -> Result<Vec<u8>> {
        let old_text = old.display().to_string();
        let new_text = new.display().to_string();
        let args: Vec<&str> = vec![
            "--no-pager",
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
            &old_text,
            &new_text,
        ];
        Ok(self
            .run_allow_difference(&args, "生成回收内容对比 diff")?
            .stdout)
    }

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

fn read_pipe(mut pipe: crate::platform::output_pipe::OutputPipe, limit: Option<usize>, finished: Arc<std::sync::atomic::AtomicBool>) -> Vec<u8> {
    let mut buffer = Vec::new(); let mut chunk = [0u8; 16 * 1024]; let mut exited = None;
    loop {
        if finished.load(std::sync::atomic::Ordering::Acquire) {
            // Drain bytes already written by Git, but never wait indefinitely
            // for a hook/tool descendant retaining a copy of the output pipe.
            let at = exited.get_or_insert_with(Instant::now);
            if at.elapsed() >= Duration::from_millis(100) { break; }
        }
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                let take = limit.map_or(read, |limit| read.min(limit.saturating_sub(buffer.len())));
                buffer.extend_from_slice(&chunk[..take]);
            }
            Err(error) if matches!(error.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => {
                if finished.load(std::sync::atomic::Ordering::Acquire) { break; }
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(_) => break,
        }
    }
    buffer
}

/// 解析 `git config --null --get-regexp` 的输出：每条记录是 `key\nvalue\0`。
/// 值内部可以包含换行与空白，必须原样保留。
fn parse_config_null(bytes: &[u8]) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    for chunk in bytes.split(|byte| *byte == 0) {
        if chunk.is_empty() {
            continue;
        }
        match chunk.iter().position(|byte| *byte == b'\n') {
            Some(index) => entries.push((
                String::from_utf8_lossy(&chunk[..index]).into_owned(),
                String::from_utf8_lossy(&chunk[index + 1..]).into_owned(),
            )),
            // 没有分隔符时是「值为真」的布尔配置。
            None => entries.push((String::from_utf8_lossy(chunk).into_owned(), "true".into())),
        }
    }
    entries
}

pub fn is_lock_error(output: &GitOutput) -> bool {
    output.exit_code != 0 && output.stderr.contains("index.lock")
}

/// 凭据打码（规格 B07、Q153）：URL 中的用户名、密码与令牌，以及携带认证信息的参数值。
///
/// 只处理可识别的形状，不改动其他文字（对象 ID、普通路径、分支名不动），
/// 否则命令日志就失去诊断价值。
pub fn mask_credentials(text: &str) -> String {
    // 按行处理：一行里的 `key=value` 只影响这一行，不会把后面的输出当成值吞掉。
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let masked = mask_url_userinfo(line);
        match masked.find('=') {
            Some(position) => {
                let (key, value) = masked.split_at(position);
                if is_sensitive_key(key) && !value.is_empty() {
                    out.push_str(key);
                    out.push_str("=***");
                } else {
                    out.push_str(&masked);
                }
            }
            None => out.push_str(&masked),
        }
    }
    out
}

/// 参数 / 配置键是否携带认证信息（`http.extraheader`、`--password` 等）。
fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    const NEEDLES: &[&str] = &[
        "password",
        "passwd",
        "token",
        "secret",
        "authorization",
        "extraheader",
        "credential",
        "oauth",
        "apikey",
        "api-key",
        "private-key",
    ];
    NEEDLES.iter().any(|needle| lower.contains(needle))
}

/// 把 URL 里的 userinfo（用户名、密码、令牌）换成 `***`，主机与路径保留。
fn mask_url_userinfo(text: &str) -> String {
    if !text.contains("://") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("://") {
        let (before, after_scheme) = rest.split_at(start);
        out.push_str(before);
        out.push_str("://");
        let authority = &after_scheme[3..];
        let end = authority
            .find(['/', ' ', '\n', '\t', '"', '\''])
            .unwrap_or(authority.len());
        let segment = &authority[..end];
        match segment.rfind('@') {
            Some(at) => {
                out.push_str("***");
                out.push_str(&segment[at..]);
            }
            None => out.push_str(segment),
        }
        rest = &authority[end..];
    }
    out.push_str(rest);
    out
}

/// 合并标准输出与错误输出，打码后最多保留 64 KB，并标明是否截断。
fn command_output(stdout: &[u8], stderr: &[u8]) -> (String, bool) {
    // 先限定原始读取量，再打码，最后按字符边界截断：先截断会把 URL 截半，
    // 留下半个令牌看起来像普通文字。
    const RAW_LIMIT: usize = 512 * 1024;
    let stdout_text = String::from_utf8_lossy(&stdout[..stdout.len().min(RAW_LIMIT)]);
    let stderr_text = String::from_utf8_lossy(&stderr[..stderr.len().min(RAW_LIMIT)]);
    let mut text = String::new();
    if !stdout_text.is_empty() {
        text.push_str(&mask_credentials(&stdout_text));
    }
    if !stderr_text.is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&mask_credentials(&stderr_text));
    }
    let dropped_raw = stdout.len() > RAW_LIMIT || stderr.len() > RAW_LIMIT;
    cap_output_text(&text, dropped_raw)
}

/// 把一段输出截到单条命令的上限；`force_truncated` 用于原始读取本身已被截断的情况。
pub fn cap_output_text(text: &str, force_truncated: bool) -> (String, bool) {
    if text.len() > LOG_OUTPUT_LIMIT || force_truncated {
        let mut end = text.len().min(LOG_OUTPUT_LIMIT);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let mut capped = text[..end].to_string();
        capped.push_str(&format!(
            "\n…（输出已截断：单条最多保留 {} KB）",
            LOG_OUTPUT_LIMIT / 1024
        ));
        (capped, true)
    } else {
        (text.to_string(), false)
    }
}

/// Read-only patches must preserve text exactly, including at a byte cap.
/// Unsupported bytes are reported by the caller rather than replaced.
pub(crate) fn readonly_patch(bytes: &[u8], limit: usize) -> Option<(String, bool)> {
    let text = std::str::from_utf8(bytes).ok().filter(|text| !text.contains('\0'))?;
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) { end -= 1; }
    Some((text[..end].to_string(), text.len() > limit))
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
    fn mask_credentials_hides_url_userinfo_and_secret_values() {
        assert_eq!(
            mask_credentials("https://alice:s3cret@gitea.example/repo.git"),
            "https://***@gitea.example/repo.git"
        );
        assert_eq!(
            mask_credentials("git@github.com:owner/repo.git"),
            "git@github.com:owner/repo.git",
            "SSH 的 user@host 不是 URL userinfo，不当成凭据"
        );
        assert_eq!(mask_credentials("--password=hunter2"), "--password=***");
        assert_eq!(
            mask_credentials("-c http.extraHeader=Authorization: Bearer abc"),
            "-c http.extraHeader=***"
        );
        // 对象 ID 与普通路径保持原样，命令日志才有诊断价值。
        assert_eq!(
            mask_credentials("0123456789abcdef0123456789abcdef01234567"),
            "0123456789abcdef0123456789abcdef01234567"
        );
        assert_eq!(mask_credentials("fetch origin main"), "fetch origin main");
        // 多行输出按行打码：后面的行不会被当成前一行的值。
        assert_eq!(
            mask_credentials("Password=abc\nfatal: could not read"),
            "Password=***\nfatal: could not read"
        );
    }

    #[test]
    fn command_log_keeps_recent_records_and_caps_output() {
        let log = CommandLog::default();
        for index in 0..(LOG_CAPACITY + 5) {
            log.record(CommandRecord {
                sequence: 0,
                worktree_id: Some("w".into()),
                started_at_ms: index as u64,
                program: "git".into(),
                args: vec!["status".into()],
                directory: "/repo".into(),
                exit_code: 0,
                elapsed_ms: 1,
                timed_out: false,
                stdout_bytes: 0,
                stderr_excerpt: String::new(),
                output: String::new(),
                output_truncated: false,
            });
        }
        let snapshot = log.snapshot();
        assert_eq!(snapshot.len(), LOG_CAPACITY);
        assert_eq!(
            snapshot.last().map(|record| record.sequence),
            Some(LOG_CAPACITY as u64 + 5)
        );
        assert_eq!(snapshot.first().map(|record| record.sequence), Some(6));

        let big = "x".repeat(LOG_OUTPUT_LIMIT + 100);
        let (capped, truncated) = cap_output_text(&big, false);
        assert!(truncated);
        assert!(capped.len() < big.len());
        assert!(capped.contains("输出已截断"));
        let (small, truncated) = cap_output_text("ok", false);
        assert!(!truncated);
        assert_eq!(small, "ok");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        let text = "中文中文";
        let cut = truncate(text, 7);
        assert!(cut.ends_with('…'));
        assert!(cut.chars().count() <= 4);
    }
}
