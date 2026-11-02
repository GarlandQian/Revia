//! Windows 环境捕获：在目标 worktree 中启动 PowerShell，加载用户 profile 后转储环境（M8 任务 02）。
//!
//! 与 macOS 的登录 shell 捕获对应（Q205）：
//! - 优先 PowerShell 7（`pwsh.exe`），没有时用 Windows PowerShell；
//! - 按 worktree 启动，加载该版本 / 宿主的用户 profile（fnm 的 use-on-cd 依赖它）；
//! - 环境写入临时文件而不是 stdout，profile 的提示输出不会污染协议；
//! - 硬超时；失败或超时回退进程环境，并把诊断交给界面；
//! - 不改用户 profile、执行策略或 Git 配置。

use std::collections::HashMap;

/// 转储哪些变量：与 unix 捕获相同的开发工具变量，加 Windows 必需的系统位置。
/// 变量名在 Windows 上不区分大小写，读取用 `[Environment]::GetEnvironmentVariable`。
pub const DUMP_KEYS: &[&str] = &[
    "PATH", "PATHEXT", "HOME", "USERPROFILE", "HOMEDRIVE", "HOMEPATH", "SHELL", "USER",
    "LOGNAME", "LANG", "LC_ALL", "LC_CTYPE", "TMPDIR", "TEMP", "TMP", "APPDATA",
    "LOCALAPPDATA", "PROGRAMDATA", "PROGRAMFILES", "PROGRAMFILES(X86)", "SYSTEMROOT", "WINDIR",
    "COMSPEC", "PSMODULEPATH", "SSH_AUTH_SOCK", "GIT_SSH", "GIT_SSH_COMMAND", "GIT_ASKPASS",
    "GIT_CONFIG_GLOBAL", "GIT_CONFIG_NOSYSTEM", "FNM_DIR", "FNM_NODE_DIST_MIRROR",
    "FNM_MULTISHELL_PATH", "NVM_DIR", "NVM_BIN", "NVM_INC", "NVM_CD_FLAGS", "NVM_RC_VERSION",
    "VOLTA_HOME", "BUN_INSTALL", "DENO_INSTALL", "PNPM_HOME", "ASDF_DIR", "ASDF_DATA_DIR",
    "PYENV_ROOT", "CARGO_HOME", "RUSTUP_HOME", "JAVA_HOME", "DOTNET_ROOT",
];

/// 捕获脚本。目标路径与 worktree 通过子进程环境变量传入，不拼接进脚本或命令行。
pub fn dump_script() -> String {
    let keys = DUMP_KEYS
        .iter()
        .map(|key| format!("'{key}'"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"# profile 的非终止错误也必须进入 catch，不能发布部分初始化环境。
$ErrorActionPreference = 'Stop'
try {{
  Set-Location -LiteralPath $env:REVIA_CAPTURE_WORKTREE -ErrorAction Stop
}} catch {{
  [Console]::Error.WriteLine('Revia: worktree: ' + $_.Exception.Message)
  exit 12
}}
$reviaProfiles = @($PROFILE.AllUsersAllHosts, $PROFILE.AllUsersCurrentHost, $PROFILE.CurrentUserAllHosts, $PROFILE.CurrentUserCurrentHost)
foreach ($reviaProfile in $reviaProfiles) {{
  if ([string]::IsNullOrEmpty($reviaProfile)) {{ continue }}
  if (-not (Test-Path -LiteralPath $reviaProfile)) {{ continue }}
  try {{
    . $reviaProfile
  }} catch {{
    [Console]::Error.WriteLine('Revia: profile ' + $reviaProfile + ': ' + $_.Exception.Message)
    # profile 抛错或被执行策略阻止：整次捕获失败，不能接受部分初始化环境。
    exit 13
  }}
}}
# profile 里的 fnm / nvm 目录钩子安装后再进入一次目标目录，确保按 worktree 选择版本。
try {{
  Set-Location -LiteralPath $env:REVIA_CAPTURE_WORKTREE -ErrorAction Stop
}} catch {{
  [Console]::Error.WriteLine('Revia: worktree after profile: ' + $_.Exception.Message)
  exit 14
}}
$reviaKeys = @({keys})
$reviaText = New-Object System.Text.StringBuilder
foreach ($reviaKey in $reviaKeys) {{
  $reviaValue = [Environment]::GetEnvironmentVariable($reviaKey)
  if ($null -ne $reviaValue) {{
    [void]$reviaText.Append($reviaKey).Append('=').Append($reviaValue).Append([char]0)
  }}
}}
[IO.File]::WriteAllText($env:REVIA_CAPTURE_DESTINATION, $reviaText.ToString(), (New-Object System.Text.UTF8Encoding($false)))
"#
    )
}

/// 按不区分大小写的变量名合并；同名变量保留最后一次出现的拼写与值。
pub fn merge_vars(pairs: impl IntoIterator<Item = (String, String)>) -> HashMap<String, String> {
    let mut merged: HashMap<String, String> = HashMap::new();
    let mut spellings: HashMap<String, String> = HashMap::new();
    for (key, value) in pairs {
        let folded = key.to_lowercase();
        if let Some(previous) = spellings.get(&folded) {
            merged.remove(previous);
        }
        spellings.insert(folded, key.clone());
        merged.insert(key, value);
    }
    merged
}

pub(super) struct CapturedEnvironment {
    pub environment: Vec<u8>,
    pub diagnostics: Option<String>,
}

/// 捕获失败的结构化原因：界面按错误键翻译，原始详情只作为参数/详情，不写中文诊断进翻译。
#[derive(Debug)]
pub(super) struct CaptureFailure {
    pub timed_out: bool,
    pub code: &'static str,
    pub detail: Option<String>,
}

impl CaptureFailure {
    fn new(code: &'static str, detail: Option<String>) -> Self {
        Self {
            timed_out: false,
            code,
            detail,
        }
    }

    fn timeout(detail: Option<String>) -> Self {
        Self {
            timed_out: true,
            code: "errors.environment.shellTimeout",
            detail,
        }
    }
}

#[cfg(windows)]
pub(super) fn detect_shell() -> std::path::PathBuf {
    use std::path::PathBuf;

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            candidates.push(directory.join("pwsh.exe"));
        }
    }
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(variable) {
            candidates.push(PathBuf::from(root).join("PowerShell").join("7").join("pwsh.exe"));
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local)
                .join("Microsoft")
                .join("WindowsApps")
                .join("pwsh.exe"),
        );
    }
    if let Some(found) = candidates.into_iter().find(|candidate| candidate.is_file()) {
        return found;
    }

    if let Some(root) = std::env::var_os("SystemRoot") {
        let legacy = std::path::Path::new(&root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        if legacy.is_file() {
            return legacy;
        }
    }
    PathBuf::from("powershell.exe")
}

#[cfg(windows)]
pub(super) fn run(
    home: &std::path::Path,
    worktree: &std::path::Path,
) -> Result<CapturedEnvironment, CaptureFailure> {
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::time::Duration;

    use wait_timeout::ChildExt;

    use crate::platform::output_pipe::OutputPipe;

    let directory = tempfile::Builder::new()
        .prefix("revia-env-")
        .tempdir()
        .map_err(|error| {
            CaptureFailure::new(
                "errors.environment.shellCaptureFailed",
                Some(format!("创建环境捕获临时目录失败：{error}")),
            )
        })?;
    let destination = directory.path().join("environment");
    let (stderr_pipe, stderr_writer) = OutputPipe::new().map_err(|error| {
        CaptureFailure::new(
            "errors.environment.shellCaptureFailed",
            Some(format!("创建 PowerShell 诊断通道失败：{error}")),
        )
    })?;
    let shell = detect_shell();
    let script = dump_script();
    let mut command = Command::new(&shell);
    command
        .current_dir(worktree)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr_writer)
        .env_clear()
        .envs(base_env(home))
        .env("REVIA_CAPTURE_WORKTREE", worktree)
        .env("REVIA_CAPTURE_DESTINATION", &destination);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // 捕获进程不需要控制台窗口；否则每次打开仓库都会闪一个窗口。
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|error| {
        CaptureFailure::new(
            "errors.environment.shellCaptureFailed",
            Some(format!("启动 {} 失败：{error}", shell.display())),
        )
    })?;

    // stderr 由有界 reader 持续排空；进程回收后最多再等 250ms，避免后代持有写端时卡死。
    let finished = Arc::new(AtomicBool::new(false));
    let reader_finished = Arc::clone(&finished);
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut pipe = stderr_pipe;
        let output = super::bounded::read_bounded(&mut pipe, 8192, &reader_finished);
        let _ = sender.send(output);
    });

    let waited = child.wait_timeout(super::SHELL_TIMEOUT);
    let timed_out = matches!(&waited, Ok(None));
    if timed_out || waited.is_err() {
        let _ = child.kill();
    }
    let status = child.wait();
    finished.store(true, Ordering::Release);
    let stderr = receiver
        .recv_timeout(super::bounded::DRAIN_GRACE + Duration::from_millis(750))
        .ok()
        .map(|output| {
            let mut text = String::from_utf8_lossy(&output.bytes).trim().to_string();
            if output.truncated && !text.is_empty() {
                text.push_str("\n[stderr truncated]");
            }
            text
        })
        .filter(|text| !text.is_empty());

    if timed_out {
        return Err(CaptureFailure::timeout(stderr));
    }
    let status = status.map_err(|error| {
        CaptureFailure::new(
            "errors.environment.shellCaptureFailed",
            Some(format!("等待 PowerShell 结束失败：{error}")),
        )
    })?;
    if !status.success() {
        return Err(CaptureFailure::new(failure_code(status.code()), stderr));
    }

    let environment = std::fs::read(&destination).map_err(|error| {
        CaptureFailure::new(
            "errors.environment.shellCaptureFailed",
            Some(format!("读取 PowerShell 环境转储失败：{error}")),
        )
    })?;
    if environment.is_empty() {
        return Err(CaptureFailure::new(
            "errors.environment.shellCaptureFailed",
            stderr.or_else(|| Some("PowerShell 返回了空环境".into())),
        ));
    }
    Ok(CapturedEnvironment {
        environment,
        diagnostics: stderr,
    })
}

/// PowerShell 脚本用退出码区分失败阶段；其他非零退出都按一般捕获失败处理。
fn failure_code(exit: Option<i32>) -> &'static str {
    match exit {
        Some(12 | 14) => "errors.environment.shellWorktreeMissing",
        Some(13) => "errors.environment.shellProfileFailed",
        _ => "errors.environment.shellCaptureFailed",
    }
}

#[cfg(windows)]
/// 最小基线环境：PowerShell 与 profile 需要系统位置，但不继承凭据或用户 Git 配置。
fn base_env(home: &std::path::Path) -> Vec<(String, String)> {
    let mut vars = vec![("HOME".to_string(), home.display().to_string())];
    for name in [
        "PATH",
        "PATHEXT",
        "SystemRoot",
        "windir",
        "ComSpec",
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
        "APPDATA",
        "LOCALAPPDATA",
        "PROGRAMDATA",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "PSModulePath",
        "TEMP",
        "TMP",
    ] {
        if let Some(value) = std::env::var_os(name) {
            vars.push((name.to_string(), value.to_string_lossy().into_owned()));
        }
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_vars_folds_windows_variable_names_case_insensitively() {
        let merged = merge_vars([
            ("Path".to_string(), "old".to_string()),
            ("PATH".to_string(), "new".to_string()),
            ("HOME".to_string(), "C:\\Users\\revia".to_string()),
        ]);
        assert_eq!(merged.get("PATH").map(String::as_str), Some("new"));
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_vars_keeps_unicode_equals_and_newlines_in_values() {
        let merged = merge_vars([(
            "REVIA_VALUE".to_string(),
            "中文=一\r\n二".to_string(),
        )]);
        assert_eq!(
            merged.get("REVIA_VALUE").map(String::as_str),
            Some("中文=一\r\n二")
        );
    }

    #[test]
    fn exit_codes_map_to_structured_reasons_and_timeout_keeps_its_flag() {
        assert_eq!(failure_code(Some(13)), "errors.environment.shellProfileFailed");
        assert_eq!(failure_code(Some(12)), "errors.environment.shellWorktreeMissing");
        assert_eq!(failure_code(Some(14)), "errors.environment.shellWorktreeMissing");
        assert_eq!(failure_code(Some(1)), "errors.environment.shellCaptureFailed");
        assert_eq!(failure_code(None), "errors.environment.shellCaptureFailed");
        let timeout = CaptureFailure::timeout(Some("timed out".into()));
        assert!(timeout.timed_out);
        assert_eq!(timeout.code, "errors.environment.shellTimeout");
        assert!(!CaptureFailure::new("errors.environment.shellCaptureFailed", None).timed_out);
    }

    #[cfg(windows)]
    #[test]
    fn profile_nonterminating_error_rejects_the_partial_environment() {
        use std::process::{Command, Stdio};
        use std::time::Duration;
        use wait_timeout::ChildExt;

        let fixture = tempfile::tempdir().unwrap();
        let home = fixture.path().join("home");
        let worktree = fixture.path().join("worktree with spaces");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&worktree).unwrap();
        let roaming = home.join("AppData/Roaming");
        let local = home.join("AppData/Local");
        let temp = home.join("Temp");
        for directory in [&roaming, &local, &temp] { std::fs::create_dir_all(directory).unwrap(); }
        let profile = home.join("fixture-profile.ps1");
        std::fs::write(
            &profile,
            "$env:PNPM_HOME = 'partial-fixture'; Write-Error 'fixture-profile-failure'\n",
        ).unwrap();
        let destination = fixture.path().join("environment");
        let diagnostics = fixture.path().join("stderr");

        // -NoProfile 避免读取真实配置；生产脚本只从 PROFILE 属性取得本测试的 profile。
        let started_file = fixture.path().join("script-started");
        let script = format!(
            "[IO.File]::WriteAllText($env:REVIA_TEST_STARTED, 'started')\n$PROFILE = [pscustomobject]@{{ AllUsersAllHosts = $env:REVIA_TEST_PROFILE; AllUsersCurrentHost = ''; CurrentUserAllHosts = ''; CurrentUserCurrentHost = '' }}\n{}",
            dump_script(),
        );
        let shell = detect_shell();
        let mut command = Command::new(&shell);
        command
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", &script])
            .current_dir(&worktree)
            .env_clear()
            .envs(base_env(&home))
            // HOME alone does not isolate Windows' module-analysis cache or user profile.
            .env("USERPROFILE", &home)
            .env("APPDATA", &roaming)
            .env("LOCALAPPDATA", &local)
            .env("TEMP", &temp)
            .env("TMP", &temp)
            .env_remove("HOMEDRIVE")
            .env_remove("HOMEPATH")
            .env("PSModulePath", shell.parent().unwrap().join("Modules"))
            .env("POWERSHELL_TELEMETRY_OPTOUT", "1")
            .env("POWERSHELL_UPDATECHECK", "Off")
            .env("REVIA_TEST_STARTED", &started_file)
            .env("REVIA_TEST_PROFILE", &profile)
            .env("REVIA_CAPTURE_WORKTREE", &worktree)
            .env("REVIA_CAPTURE_DESTINATION", &destination)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&diagnostics).unwrap());
        crate::platform::hide_console(&mut command);
        let mut child = command.spawn().expect("Windows PowerShell must be available");
        let status = child.wait_timeout(Duration::from_secs(15)).unwrap();
        if status.is_none() {
            let _ = child.kill();
        }
        let reaped = child.wait().unwrap();
        let detail = std::fs::read(&diagnostics).unwrap();
        assert!(status.is_some(), "fixture PowerShell did not finish within 15s; shell={}; script_started={}; stderr={}", shell.display(), started_file.exists(), String::from_utf8_lossy(&detail));
        assert_eq!(reaped.code(), Some(13));
        assert_eq!(failure_code(reaped.code()), "errors.environment.shellProfileFailed");
        assert!(!destination.exists(), "partial profile environment must not be published");
        assert!(String::from_utf8_lossy(&detail).contains("fixture-profile-failure"), "{}", String::from_utf8_lossy(&detail));
    }

    #[test]
    fn dump_script_loads_every_profile_and_writes_nul_separated_pairs() {
        let script = dump_script();
        for profile in [
            "$PROFILE.AllUsersAllHosts",
            "$PROFILE.AllUsersCurrentHost",
            "$PROFILE.CurrentUserAllHosts",
            "$PROFILE.CurrentUserCurrentHost",
        ] {
            assert!(script.contains(profile), "缺少 {profile}");
        }
        assert!(script.contains("Append([char]0)"));
        assert!(script.contains("$env:REVIA_CAPTURE_DESTINATION"));
        assert!(script.contains("Set-Location -LiteralPath $env:REVIA_CAPTURE_WORKTREE"));
        // 协议内容不经过 stdout，profile 提示不会污染环境。
        assert!(script.contains("[IO.File]::WriteAllText"));
    }
}
