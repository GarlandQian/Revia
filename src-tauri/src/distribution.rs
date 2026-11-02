//! 分发形态与数据位置（M8 任务 01、Q57、Q58、Q204）。
//!
//! 安装版使用当前用户的标准应用数据目录；便携版的数据、缓存与 WebView2 用户数据
//! 都在 exe 旁的 `data`。形态由编译特性 `portable` 决定，不看 exe 名称、环境变量或
//! 旁边的标记文件，所以改名、移动目录不会改变语义。
//!
//! 便携数据目录在**创建第一个窗口之前**确定并检查可写性：WebView2 用户数据目录
//! 必须在建窗前交给 Tauri，数据目录不可写时直接拒绝启动，不回退到用户目录。
//! 测量 / 验收仍可通过既有环境变量显式指定独立目录（见 `lib.rs` 启动顺序）。

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};

/// 编译时选择的分发形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Distribution {
    Installed,
    Portable,
}

/// `portable` 特性只由便携构建开启（CI 与本地打包脚本显式传入）。
pub const PORTABLE_BUILD: bool = cfg!(feature = "portable");

impl Distribution {
    pub fn current() -> Self {
        if PORTABLE_BUILD {
            Self::Portable
        } else {
            Self::Installed
        }
    }

    pub fn is_portable(self) -> bool {
        matches!(self, Self::Portable)
    }
}

/// 凭据后端：两种 Windows 分发形态都用 Windows Credential Manager（Q204、ADR-0015）；
/// macOS 继续用系统钥匙串。凭据属于当前系统用户，不随便携目录移动。
pub fn credential_store() -> &'static str {
    #[cfg(windows)]
    {
        "windowsCredentialManager"
    }
    #[cfg(target_os = "macos")]
    {
        "macosKeychain"
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        "systemKeychain"
    }
}

/// 供界面与诊断查看的实际启动布局。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DistributionInfo {
    pub shape: Distribution,
    pub data_dir: String,
    /// 便携版：`data/webview2`；安装版交给系统默认位置，故为 None。
    pub webview_data_dir: Option<String>,
    pub credential_store: String,
}

/// 建窗前的启动布局。内部保留真实路径，避免非 Unicode 目录名在往返中丢失。
#[derive(Debug, Clone)]
pub struct StartupLayout {
    pub info: DistributionInfo,
    data_dir: PathBuf,
    webview_data_dir: Option<PathBuf>,
}

impl StartupLayout {
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn webview_data_dir(&self) -> Option<&Path> {
        self.webview_data_dir.as_deref()
    }

    /// 正常启动：只替换业务数据目录。便携版连带把 WebView2 放到 data 旁；
    /// 安装版保持 Tauri 的默认 WebView2 位置（app_local_data_dir），不改变既有行为。
    pub fn with_data_dir(mut self, data_dir: &Path) -> Self {
        self.data_dir = data_dir.to_path_buf();
        self.info.data_dir = data_dir.to_string_lossy().into_owned();
        if self.info.shape.is_portable() {
            let webview = data_dir.join("webview2");
            self.info.webview_data_dir = Some(webview.to_string_lossy().into_owned());
            self.webview_data_dir = Some(webview);
        }
        self
    }

    /// 显式验收 / 测量：两种形态都把 WebView2 用户数据放进覆盖目录，
    /// 避免安装版回退到真实 app_local_data_dir 破坏隔离（X3）。
    pub fn with_isolated_data_dir(mut self, data_dir: &Path) -> Self {
        self.data_dir = data_dir.to_path_buf();
        self.info.data_dir = data_dir.to_string_lossy().into_owned();
        let webview = data_dir.join("webview2");
        self.info.webview_data_dir = Some(webview.to_string_lossy().into_owned());
        self.webview_data_dir = Some(webview);
        self
    }
}

/// 按编译形态计算启动布局。
///
/// `standard_data_dir` 是 Tauri 解析出的当前用户标准数据目录；便携版不需要它，
/// 这样即使系统目录不可用，便携版仍能启动（或按自身目录的可写性明确失败）。
pub fn startup_layout(standard_data_dir: Option<PathBuf>) -> Result<StartupLayout> {
    let exe = std::env::current_exe().map_err(|error| {
        AppError::coded(
            ErrorKind::Environment,
            "errors.distribution.executableUnavailable",
            "找不到应用可执行文件：{error}",
            &[("error", error.to_string())],
        )
    })?;
    plan(Distribution::current(), &exe, standard_data_dir.as_deref())
}

/// 纯规划函数，测试可指定形态与路径。
pub fn plan(
    distribution: Distribution,
    executable: &Path,
    standard_data_dir: Option<&Path>,
) -> Result<StartupLayout> {
    match distribution {
        Distribution::Installed => {
            let standard = standard_data_dir.ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Environment,
                    "errors.distribution.standardDataDirUnavailable",
                    "找不到当前用户的标准应用数据目录",
                    &[],
                )
            })?;
            Ok(StartupLayout {
                info: DistributionInfo {
                    shape: distribution,
                    data_dir: standard.to_string_lossy().into_owned(),
                    webview_data_dir: None,
                    credential_store: credential_store().into(),
                },
                data_dir: standard.to_path_buf(),
                webview_data_dir: None,
            })
        }
        Distribution::Portable => {
            let parent = executable.parent().filter(|path| path.is_dir()).ok_or_else(|| {
                AppError::coded(
                    ErrorKind::Environment,
                    "errors.distribution.portableLocationUnavailable",
                    "便携版需要位于一个可访问的文件夹中，找不到程序所在目录",
                    &[],
                )
            })?;
            let data_dir = parent.join("data");
            let webview = data_dir.join("webview2");
            // 只计算路径；创建与可写探测由 `ensure_writable` 在选定最终目录后执行，
            // 这样显式验收覆盖时不会先触碰正常便携 data（X3）。
            Ok(StartupLayout {
                info: DistributionInfo {
                    shape: distribution,
                    data_dir: data_dir.to_string_lossy().into_owned(),
                    webview_data_dir: Some(webview.to_string_lossy().into_owned()),
                    credential_store: credential_store().into(),
                },
                data_dir,
                webview_data_dir: Some(webview),
            })
        }
    }
}

/// 在建窗前确认最终布局的数据目录（以及显式 WebView2 目录）可写。
///
/// 调用方只在便携版或显式验收 / 测量覆盖时调用；安装版正常启动沿用 Tauri 的目录准备，
/// 不在这里额外创建用户目录。
pub fn ensure_writable(layout: &StartupLayout) -> Result<()> {
    ensure_writable_directory(layout.data_dir())?;
    if let Some(webview) = layout.webview_data_dir() {
        // WebView2 目录同样要在建窗前确认可写；失败时给同一条明确启动错误，
        // 而不是让 Tauri 在创建 webview 时 panic。
        ensure_writable_directory(webview)?;
    }
    Ok(())
}

/// 便携数据目录必须真实可写；失败时明确拒绝启动，不静默改用用户目录。
fn ensure_writable_directory(directory: &Path) -> Result<()> {
    std::fs::create_dir_all(directory).map_err(|error| {
        AppError::coded(
            ErrorKind::Environment,
            "errors.distribution.portableDataDirUnwritable",
            "无法创建便携版数据目录：{error}",
            &[
                ("path", directory.to_string_lossy().into_owned()),
                ("error", error.to_string()),
            ],
        )
    })?;
    // 真实写入探测：目录存在但只读、被策略拦截或位于不可写介质时 create_dir_all 可能成功。
    let probe = directory.join(format!(
        ".revia-write-probe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default()
    ));
    let outcome = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .and_then(|mut file| {
            use std::io::Write;
            file.write_all(b"revia")?;
            file.sync_all()
        });
    let _ = std::fs::remove_file(&probe);
    outcome.map_err(|error| {
        AppError::coded(
            ErrorKind::Environment,
            "errors.distribution.portableDataDirUnwritable",
            "便携版数据目录不可写：{error}",
            &[
                ("path", directory.to_string_lossy().into_owned()),
                ("error", error.to_string()),
            ],
        )
    })
}

/// 便携版缺少 WebView2 运行时的明确指引（安装版由 Tauri 的运行时准备机制处理）。
pub fn webview2_missing_error(detail: &str) -> AppError {
    AppError::coded(
        ErrorKind::Environment,
        "errors.distribution.webview2Missing",
        "找不到 Microsoft Edge WebView2 运行时：{error}",
        &[("error", detail.to_string())],
    )
}

/// 启动失败时的提示：便携版在建窗前就可能失败，此时前端还不存在，只能由原生层显示。
pub fn startup_failure_message(error: &AppError) -> String {
    let hint = match error.code {
        Some("errors.distribution.portableDataDirUnwritable") => {
            "\n\n请把 Revia 放到有写入权限的文件夹，或先调整该文件夹权限后重试。\nPut Revia in a folder you can write to, or fix its permissions and try again."
        }
        Some("errors.distribution.webview2Missing") => {
            "\n\n请安装 Microsoft Edge WebView2 Runtime 后重试。\nInstall the Microsoft Edge WebView2 Runtime and try again."
        }
        _ => "",
    };
    format!("{}{hint}", error.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_layout_uses_standard_data_dir() {
        let root = tempfile::tempdir().unwrap();
        let standard = root.path().join("standard");
        let exe = root.path().join("Revia.exe");
        std::fs::write(&exe, b"fixture").unwrap();
        let layout = plan(Distribution::Installed, &exe, Some(&standard)).unwrap();
        assert_eq!(layout.data_dir(), standard);
        assert_eq!(layout.webview_data_dir(), None);
        assert_eq!(layout.info.credential_store, credential_store());
    }

    #[test]
    fn portable_layout_keeps_data_next_to_executable_after_rename() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        std::fs::create_dir_all(&first).unwrap();
        let exe = first.join("Revia.exe");
        std::fs::write(&exe, b"fixture").unwrap();
        let layout = plan(Distribution::Portable, &exe, None).unwrap();
        assert_eq!(layout.data_dir(), first.join("data"));
        assert_eq!(
            layout.webview_data_dir(),
            Some(first.join("data").join("webview2").as_path())
        );
        ensure_writable(&layout).unwrap();
        assert!(first.join("data").is_dir());
        assert!(first.join("data").join("webview2").is_dir());

        // 改名并移动 exe：新目录旁的 data 仍是数据位置，形态不变。
        let second = root.path().join("renamed and moved");
        std::fs::create_dir_all(&second).unwrap();
        let renamed = second.join("Anything.exe");
        std::fs::write(&renamed, b"fixture").unwrap();
        let moved = plan(Distribution::Portable, &renamed, None).unwrap();
        assert_eq!(moved.data_dir(), second.join("data"));
        assert_eq!(moved.info.shape, Distribution::Portable);
        assert_eq!(moved.info.credential_store, credential_store());
    }

    #[test]
    fn portable_layout_never_falls_back_when_data_dir_is_unwritable() {
        let root = tempfile::tempdir().unwrap();
        let exe = root.path().join("Revia.exe");
        std::fs::write(&exe, b"fixture").unwrap();
        let blocked = root.path().join("data");
        std::fs::write(&blocked, b"a file where the data directory should be").unwrap();
        let layout = plan(Distribution::Portable, &exe, None).unwrap();
        let error = ensure_writable(&layout).unwrap_err();
        assert_eq!(
            error.code,
            Some("errors.distribution.portableDataDirUnwritable")
        );
    }

    #[test]
    fn installed_layout_requires_standard_directory() {
        let root = tempfile::tempdir().unwrap();
        let exe = root.path().join("Revia.exe");
        std::fs::write(&exe, b"fixture").unwrap();
        let error = plan(Distribution::Installed, &exe, None).unwrap_err();
        assert_eq!(
            error.code,
            Some("errors.distribution.standardDataDirUnavailable")
        );
    }

    #[test]
    fn credential_backend_does_not_depend_on_distribution_shape() {
        // Q204：便携版与安装版共用系统凭据库，凭据不放进便携 data。
        let root = tempfile::tempdir().unwrap();
        let exe = root.path().join("Revia.exe");
        std::fs::write(&exe, b"fixture").unwrap();
        let portable = plan(Distribution::Portable, &exe, None).unwrap();
        let installed = plan(Distribution::Installed, &exe, Some(root.path())).unwrap();
        assert_eq!(portable.info.credential_store, credential_store());
        assert_eq!(installed.info.credential_store, credential_store());
    }

    #[test]
    fn explicit_acceptance_isolates_webview_for_installed_and_portable() {
        let root = tempfile::tempdir().unwrap();
        let standard = root.path().join("standard");
        let exe_dir = root.path().join("portable");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let exe = exe_dir.join("Revia.exe");
        std::fs::write(&exe, b"fixture").unwrap();
        let override_dir = root.path().join("acceptance data");

        for distribution in [Distribution::Installed, Distribution::Portable] {
            let layout = plan(distribution, &exe, Some(&standard))
                .unwrap()
                .with_isolated_data_dir(&override_dir);
            assert_eq!(layout.data_dir(), override_dir);
            assert_eq!(
                layout.webview_data_dir(),
                Some(override_dir.join("webview2").as_path())
            );
            ensure_writable(&layout).unwrap();
            assert!(override_dir.join("webview2").is_dir());
            // 显式覆盖不触碰标准目录或便携正常 data。
            assert!(!standard.exists());
            assert!(!exe_dir.join("data").exists());
            std::fs::remove_dir_all(&override_dir).unwrap();
        }
    }

    #[test]
    fn portable_override_is_chosen_before_touching_the_normal_data_dir() {
        let root = tempfile::tempdir().unwrap();
        let exe_dir = root.path().join("portable");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let exe = exe_dir.join("Revia.exe");
        std::fs::write(&exe, b"fixture").unwrap();
        // 正常便携 data 位置被一个文件占住：若先探测正常目录就会失败。
        std::fs::write(exe_dir.join("data"), b"not a directory").unwrap();
        let override_dir = root.path().join("isolated");

        let layout = plan(Distribution::Portable, &exe, None)
            .unwrap()
            .with_isolated_data_dir(&override_dir);
        ensure_writable(&layout).expect("显式覆盖应先于正常便携目录探测");
        assert!(override_dir.join("webview2").is_dir());
        assert_eq!(std::fs::read(exe_dir.join("data")).unwrap(), b"not a directory");
    }

    #[test]
    fn normal_installed_startup_does_not_create_directories() {
        let root = tempfile::tempdir().unwrap();
        let standard = root.path().join("standard");
        let exe = root.path().join("Revia.exe");
        std::fs::write(&exe, b"fixture").unwrap();
        let layout = plan(Distribution::Installed, &exe, Some(&standard)).unwrap();
        // 正常安装版由 Tauri 准备目录；这里不主动创建或探测。
        assert!(!standard.exists());
        assert_eq!(layout.webview_data_dir(), None);
    }
}
