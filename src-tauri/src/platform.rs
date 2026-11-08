//! Filesystem identity, permissions and native termination stay in the platform shell (Q55).

#[cfg(target_os = "macos")]
mod macos_exit;
#[cfg(target_os = "macos")]
mod macos_glass;
pub mod window_glass;

mod language;
pub use language::{language_from_locale, system_language};

mod browser;
pub use browser::{open_github_device_page, open_url};

pub(crate) mod output_pipe;
pub mod system;

use std::fs;
#[cfg(any(unix, windows))]
use std::fs::File;
use std::path::{Path, PathBuf};

use crate::error::Result;

/// Git evaluates GIT_EDITOR / GIT_SEQUENCE_EDITOR with its command shell.
/// Quote the executable as one argument, even inside Application Support or a
/// user-selected data directory containing spaces, quotes or metacharacters.
pub fn git_shell_argument(path: &Path) -> String {
    let value = external_path(path).to_string_lossy().into_owned();
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// 启动早期（窗口与前端尚不存在）的致命错误提示。
///
/// 便携版数据目录不可写或缺少 WebView2 时必须在建窗前拒绝启动；此时无法用界面文案，
/// Windows 上显示原生消息框，其他平台写入 stderr（便携版只在 Windows 构建）。
pub fn fatal_startup_error(message: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let encode = |value: &str| -> Vec<u16> {
            std::ffi::OsStr::new(value)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        };
        let text = encode(message);
        let title = encode("Revia");
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBoxW(
                window: *mut std::ffi::c_void,
                text: *const u16,
                caption: *const u16,
                flags: u32,
            ) -> i32;
        }
        // MB_OK | MB_ICONERROR：启动失败只需要用户确认，不提供会误导的选项。
        unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10) };
    }
    #[cfg(not(windows))]
    eprintln!("{message}");
}

/// Install after Tauri builds its native application, before its event loop starts.
pub fn install_exit_interceptor(app: &tauri::AppHandle) -> Result<()> {
    #[cfg(target_os = "macos")]
    macos_exit::install(app)?;
    #[cfg(not(target_os = "macos"))]
    let _ = app;
    Ok(())
}

pub fn directory_identity(path: &Path) -> Result<Option<String>> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_dir() {
        return Ok(None);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let birth = metadata.created().ok().and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok());
        Ok(birth.map(|birth| format!("{}:{}:{}", metadata.dev(), metadata.ino(), birth.as_nanos())))
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        #[repr(C)]
        #[derive(Default)]
        struct FileTime { low: u32, high: u32 }
        #[repr(C)]
        #[derive(Default)]
        struct FileInformation {
            attributes: u32, created: FileTime, accessed: FileTime, written: FileTime,
            volume: u32, size_high: u32, size_low: u32, links: u32, index_high: u32, index_low: u32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut FileInformation) -> i32;
        }
        let file = File::options().read(true).custom_flags(0x02000000).open(path)?;
        let mut info = FileInformation::default();
        // FILE_FLAG_BACKUP_SEMANTICS opens directories without changing their contents.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Some(format!("{}:{}:{}:{}:{}", info.volume, info.index_high, info.index_low, info.created.high, info.created.low)))
    }
    #[cfg(not(any(unix, windows)))]
    Ok(None)
}

pub fn executable(metadata: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    { let _ = metadata; false }
}

/// Expected worktree permission for a Git tree mode on this platform.
pub fn git_executable(mode: &str) -> bool {
    #[cfg(unix)]
    { mode == "100755" }
    #[cfg(not(unix))]
    { let _ = mode; false }
}

pub fn set_executable(path: &Path, executable: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }))?;
    }
    #[cfg(not(unix))]
    { let _ = (path, executable); }
    Ok(())
}

pub fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// 后台子进程不额外弹控制台窗口（Windows）。Git 与 hooks 的输出去向由调用方决定。
pub fn hide_console(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW：从 GUI 进程启动控制台程序时不创建控制台。
        command.creation_flags(0x08000000);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

/// 把内部路径转换成**外部进程**（Git CLI）能接受的形式。
///
/// Windows 上 `std::fs::canonicalize` 会返回 verbatim 路径（`\\?\C:\...`），
/// Git for Windows 不接受它作为 `worktree add` 的目标（`fatal: ... Invalid argument`）。
/// 这里去掉 verbatim 前缀：`\\?\C:\x` → `C:\x`，UNC 形式 `\\?\UNC\server\share` → `\\server\share`。
///
/// 只用于传给外部进程；Rust 内部仍使用 canonicalize 的结果比较身份。
pub fn external_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        return PathBuf::from(external_path_text(&path.as_os_str().to_string_lossy()));
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

/// verbatim 前缀处理的纯文本部分；与平台无关，便于在 macOS 上回归 UNC / 盘符路径。
pub fn external_path_text(text: &str) -> String {
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return rest.to_string();
    }
    text.to_string()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_path_text_strips_verbatim_prefixes_without_touching_unc_or_drive_paths() {
        assert_eq!(external_path_text(r"\\?\C:\work\revia"), r"C:\work\revia");
        assert_eq!(
            external_path_text(r"\\?\UNC\server\share\revia"),
            r"\\server\share\revia"
        );
        assert_eq!(external_path_text(r"C:\work\revia"), r"C:\work\revia");
        assert_eq!(
            external_path_text(r"\\server\share\revia"),
            r"\\server\share\revia"
        );
    }
}
