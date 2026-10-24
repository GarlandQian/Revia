//! Filesystem identity and permissions stay in the platform shell (Q55).

use std::fs;
#[cfg(any(unix, windows))]
use std::fs::File;
use std::path::{Path, PathBuf};

use crate::error::Result;

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
        let text = path.as_os_str().to_string_lossy().into_owned();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
        path.to_path_buf()
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}
