//! Open only https://github.com URLs in the system browser (device login, portable download).

use std::process::{Command, Stdio};

use crate::error::{AppError, ErrorKind, Result};

/// 只允许打开 GitHub 的 https 地址：URL 由 Rust 层构造，前端不能传入任意地址。
fn validate(url: &str) -> Result<()> {
    let parsed = url::Url::parse(url).map_err(|_| invalid())?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("github.com")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(invalid());
    }
    Ok(())
}

fn invalid() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.m7.browserUrlRejected",
        "只允许打开 GitHub 的 https 地址",
        &[],
    )
}

fn browser_command(url: &str) -> std::io::Result<Command> {
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
        // The URL is a single argument and was validated above; no shell interpolation.
        command.arg(url);
        command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        Ok(command)
    }
}

pub fn open_url(url: &str) -> Result<()> {
    validate(url)?;
    let outcome = browser_command(url).and_then(|mut command| {
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

pub fn open_github_device_page() -> Result<()> {
    open_url("https://github.com/login/device")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_github_urls_are_accepted() {
        assert!(validate("https://github.com/owner/repo/releases/download/v1.0.0/file.zip").is_ok());
        assert!(validate("http://github.com/owner/repo").is_err());
        assert!(validate("https://example.com/owner/repo").is_err());
        assert!(validate("https://github.com.evil.example/owner/repo").is_err());
        assert!(validate("https://user:pass@github.com/owner/repo").is_err());
    }
}
