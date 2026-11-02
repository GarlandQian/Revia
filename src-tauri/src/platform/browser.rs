//! Open only the fixed GitHub device authorization page in the system browser.

use std::process::{Command, Stdio};

use crate::error::{AppError, ErrorKind, Result};

fn device_page_command() -> std::io::Result<Command> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("/usr/bin/open");
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("cmd.exe");
        command.args(["/d", "/c", "start", ""]);
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        command
    };
    #[cfg(not(any(target_os = "macos", windows)))]
    return Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "browser launch is not supported on this platform",
    ));

    #[cfg(any(target_os = "macos", windows))]
    {
        // No URL, code or token supplied by the frontend or remote response is executed.
        command.arg("https://github.com/login/device");
        command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        Ok(command)
    }
}

pub fn open_github_device_page() -> Result<()> {
    let outcome = device_page_command().and_then(|mut command| {
        let status = command.status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(status.to_string()))
        }
    });
    outcome.map_err(|error| AppError::coded(
        ErrorKind::Environment,
        "errors.m7.githubBrowserOpenFailed",
        "无法打开浏览器：{detail}",
        &[("detail", error.to_string())],
    ))
}
