//! 应用更新（M7 任务 10、Q199）。
//!
//! 走 Tauri 官方 updater：清单与安装包都由插件按配置的 minisign 公钥验签，
//! 这里只负责模式（手动 / 提示 / 自动下载）、频率限制、进度事件与本地回执。
//!
//! 未配置更新通道（没有公钥或接口）时，检查与安装都明确报“未配置”，
//! 不把未签名的包当成更新；安装失败或取消时保留当前版本并给出原因。

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri_plugin_updater::UpdaterExt;
use ts_rs::TS;

use crate::distribution::Distribution;
use crate::error::{AppError, ErrorKind, Result};
use crate::settings::{read_settings, AppUpdateMode};

use super::now_ms;

/// 自动检查的最小间隔（手动检查不受限制）。
const CHECK_INTERVAL_MS: u64 = 60 * 60 * 1000;
/// 自动检查的轮询间隔。
const AUTO_CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// 已知的可用更新（不含安装包内容）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AvailableUpdate {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    pub published_at: Option<String>,
}

/// 安装回执：即使界面重启也能核对装过什么。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateReceipt {
    pub version: String,
    pub installed_at_ms: u64,
}

/// 更新状态。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppUpdateStatus {
    /// 是否配置了更新通道（公钥与接口）。
    pub configured: bool,
    /// 未配置时的原因（界面直接显示）。
    pub unconfigured_reason: Option<String>,
    pub mode: AppUpdateMode,
    pub current_version: String,
    pub last_checked_at_ms: Option<u64>,
    pub available: Option<AvailableUpdate>,
    pub receipt: Option<UpdateReceipt>,
    pub downloaded_version: Option<String>,
    pub restart_required: bool,
    /// 便携版：只提示新版本与下载位置，不下载、不安装、不重启（Q58、M8 任务 05）。
    pub portable: bool,
    /// 便携版可用版本的下载地址（由配置的 Release 仓库与命名约定生成）。
    pub portable_download_url: Option<String>,
    /// 已交给安装器、尚未由新进程核对的版本。
    pub pending_install: Option<UpdatePendingInstall>,
}

/// 已调用安装器、尚未确认新进程版本的记录。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdatePendingInstall {
    pub version: String,
    pub started_at_ms: u64,
}

/// 下载进度事件。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct StoredState {
    last_checked_at_ms: Option<u64>,
    available: Option<AvailableUpdate>,
    receipt: Option<UpdateReceipt>,
    /// F13：已下载待安装的版本（None 表示没有待安装包）。
    downloaded_version: Option<String>,
    downloaded: Option<super::update_package::DownloadedUpdate>,
    checked_prerelease: bool,
    /// Windows 安装器会结束应用：先落“待安装”，下一次启动按真实版本核对。
    pending_install: Option<UpdatePendingInstall>,
}

/// 更新状态的本地存储与并发保护。
pub struct UpdateState {
    path: PathBuf,
    /// F22：设置目录（读 allow_prerelease 用）。
    settings_dir: PathBuf,
    busy: Mutex<bool>,
}

impl UpdateState {
    pub fn new(data_dir: &Path, settings_dir: &Path) -> Self {
        Self {
            path: data_dir.join("update-state.json"),
            settings_dir: settings_dir.to_path_buf(),
            busy: Mutex::new(false),
        }
    }

    fn load(&self) -> StoredState {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn store(&self, state: &StoredState) -> Result<()> {
        super::atomic_write(&self.path, &serde_json::to_vec_pretty(state)?)
    }

    fn begin(&self) -> Result<()> {
        let mut busy = self.busy.lock().unwrap_or_else(|error| error.into_inner());
        if *busy {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.m7.updateBusy",
                "更新正在下载或安装，请稍候",
                &[],
            ));
        }
        *busy = true;
        Ok(())
    }

    fn end(&self) {
        let mut busy = self.busy.lock().unwrap_or_else(|error| error.into_inner());
        *busy = false;
    }

    /// F22：预发布渠道开关（来自设置；默认稳定版）。
    fn allow_prerelease(&self) -> bool {
        crate::settings::read_settings(&self.settings_dir)
            .map(|settings| settings.allow_prerelease)
            .unwrap_or(false)
    }

    pub fn status(&self, app: &tauri::AppHandle, mode: AppUpdateMode) -> AppUpdateStatus {
        let mut stored = self.load();
        // 上一次启动把安装交给了安装器：新进程里按真实版本确认，失败则保留待安装状态。
        if reconcile_pending_install(&mut stored, &app.package_info().version) {
            let _ = self.store(&stored);
        }
        let reason = unconfigured_reason(app);
        let available = stored.available.filter(|available| {
            semver::Version::parse(&available.version).is_ok_and(|version| {
                version > effective_version(app, &stored.receipt)
                    && (self.allow_prerelease() || version.pre.is_empty())
            })
        });
        let portable = Distribution::current().is_portable();
        let portable_download_url = if portable {
            available.as_ref().and_then(|available| {
                configured_endpoints(app)
                    .ok()
                    .and_then(|configured| portable_download_url(&configured.endpoint, &available.version))
            })
        } else {
            None
        };
        AppUpdateStatus {
            configured: reason.is_none(),
            unconfigured_reason: reason,
            mode,
            current_version: app.package_info().version.to_string(),
            last_checked_at_ms: stored.last_checked_at_ms,
            portable_download_url,
            available,
            restart_required: stored.receipt.as_ref().is_some_and(|receipt| {
                match (
                    semver::Version::parse(&receipt.version),
                    semver::Version::parse(&app.package_info().version.to_string()),
                ) {
                    (Ok(installed), Ok(current)) => installed > current,
                    _ => false,
                }
            }),
            downloaded_version: stored
                .downloaded
                .as_ref()
                .filter(|package| {
                    super::update_package::exists(self.path.parent().unwrap(), package)
                        && semver::Version::parse(&package.version).is_ok_and(|version| {
                            version > effective_version(app, &stored.receipt)
                                && (self.allow_prerelease() || version.pre.is_empty())
                        })
                })
                .map(|package| package.version.clone()),
            pending_install: stored.pending_install,
            portable,
            receipt: stored.receipt,
        }
    }
}

/// 新进程启动后核对上一次安装：版本达到或超过待安装版本才算成功。
fn reconcile_pending_install(stored: &mut StoredState, current: &semver::Version) -> bool {
    let Some(pending) = stored.pending_install.clone() else {
        return false;
    };
    let confirmed = semver::Version::parse(&pending.version).is_ok_and(|version| version <= *current);
    if !confirmed {
        return false;
    }
    stored.receipt = Some(UpdateReceipt {
        version: pending.version,
        installed_at_ms: pending.started_at_ms,
    });
    stored.pending_install = None;
    stored.available = None;
    stored.downloaded = None;
    stored.downloaded_version = None;
    true
}

/// 便携版的下载地址：按 Q65 的命名约定从配置的 GitHub Release 仓库生成。
pub fn portable_download_url(endpoint: &str, version: &str) -> Option<String> {
    let endpoint = url::Url::parse(endpoint).ok()?;
    if endpoint.scheme() != "https" || endpoint.host_str() != Some("github.com") {
        return None;
    }
    let parts: Vec<_> = endpoint.path_segments()?.collect();
    if parts.len() < 5 || parts[0].is_empty() || parts[1].is_empty() || parts[2] != "releases" {
        return None;
    }
    Some(format!(
        "https://github.com/{}/{}/releases/download/v{version}/Revia-{version}-windows-x64-portable.zip",
        parts[0], parts[1]
    ))
}

/// 便携版即使直接调用命令也拒绝下载、安装和重启（Q58、M8 任务 05）。
fn ensure_install_allowed(distribution: Distribution) -> Result<()> {
    if distribution.is_portable() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.portableUpdateInstallBlocked",
            "便携版不提供自动更新，请下载新版本后自行替换",
            &[],
        ));
    }
    Ok(())
}

fn effective_version(app: &tauri::AppHandle, receipt: &Option<UpdateReceipt>) -> semver::Version {
    let current = app.package_info().version.clone();
    receipt
        .as_ref()
        .and_then(|receipt| semver::Version::parse(&receipt.version).ok())
        .map_or(current.clone(), |installed| installed.max(current))
}

/// 读取 updater 配置里的公钥；为空表示这个构建没有更新通道。
/// F22：读取构建时配置的更新地址（tauri.conf.json plugins.updater.endpoints[0]）。
fn configured_endpoints(app: &tauri::AppHandle) -> Result<ConfiguredUpdate> {
    let plugins = &app.config().plugins.0;
    let updater = plugins.get("updater").ok_or_else(|| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.updateReasonNoPlugin",
            "没有配置更新通道（缺少 updater 插件配置）",
            &[],
        )
    })?;
    let endpoint = updater
        .get("endpoints")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .to_string();
    let pubkey = updater_pubkey(app);
    Ok(ConfiguredUpdate { endpoint, pubkey })
}

struct ConfiguredUpdate {
    endpoint: String,
    #[allow(dead_code)]
    pubkey: Option<String>,
}

fn updater_pubkey(app: &tauri::AppHandle) -> Option<String> {
    let plugins = &app.config().plugins.0;
    let updater = plugins.get("updater")?;
    let pubkey = updater.get("pubkey")?.as_str()?.trim();
    if pubkey.is_empty() {
        None
    } else {
        Some(pubkey.to_string())
    }
}

fn unconfigured_reason(app: &tauri::AppHandle) -> Option<String> {
    // 返回的是翻译键而不是文案：界面按当前语言展示（Q168）。
    let plugins = &app.config().plugins.0;
    let Some(updater) = plugins.get("updater") else {
        return Some("errors.m7.updateReasonNoPlugin".into());
    };
    if updater_pubkey(app).is_none() {
        return Some("errors.m7.updateReasonNoPubkey".into());
    }
    let endpoints = updater
        .get("endpoints")
        .and_then(|value| value.as_array())
        .map(|items| items.iter().filter(|item| item.is_string()).count())
        .unwrap_or(0);
    if endpoints == 0 {
        return Some("errors.m7.updateReasonNoEndpoints".into());
    }
    None
}

/// 检查更新（带频率限制）；`force` 表示用户主动点击。
pub async fn check(
    app: tauri::AppHandle,
    state: &UpdateState,
    mode: AppUpdateMode,
    force: bool,
) -> Result<AppUpdateStatus> {
    let _ = state.begin()?;
    let result = check_inner(&app, state, mode, force).await;
    state.end();
    result
}

async fn check_inner(
    app: &tauri::AppHandle,
    state: &UpdateState,
    mode: AppUpdateMode,
    force: bool,
) -> Result<AppUpdateStatus> {
    let status = state.status(app, mode);
    // M9 测量版 / 验收版关闭应用更新检查（Q216）。
    if crate::measure::IS_MEASURE {
        return Ok(status);
    }
    if !status.configured {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.updateNotConfigured",
            "更新通道未配置",
            &[],
        ));
    }
    let now = now_ms();
    if !force {
        if let Some(last) = status.last_checked_at_ms {
            if now.saturating_sub(last) < CHECK_INTERVAL_MS
                && state.load().checked_prerelease == state.allow_prerelease()
            {
                return Ok(status);
            }
        }
    }
    let prerelease_allowed = state.allow_prerelease();
    let found = find_update(app, state).await?;
    // 未开启预发布渠道时，预发布版本（semver 带预发布段）不作为可用更新。
    let found = found.filter(|update| prerelease_allowed || !update.version.contains('-'));
    let mut stored = state.load();
    stored.last_checked_at_ms = Some(now);
    stored.checked_prerelease = prerelease_allowed;
    stored.available = found.as_ref().map(|update| AvailableUpdate {
        version: update.version.clone().to_string(),
        current_version: update.current_version.clone(),
        notes: update.body.clone(),
        published_at: update.date.map(|date| date.to_string()),
    });
    state.store(&stored)?;
    Ok(state.status(app, mode))
}

fn channel_updater(app: &tauri::AppHandle) -> Result<tauri_plugin_updater::Updater> {
    let configured = configured_endpoints(app)?;
    let endpoint = configured.endpoint.clone();
    let parsed: url::Url = endpoint.parse().map_err(|error: url::ParseError| {
        AppError::coded(
            ErrorKind::Internal,
            "errors.m7.updateEndpointInvalid",
            "更新地址无法解析：{error}",
            &[("error", error.to_string())],
        )
    })?;
    let updater = app
        .updater_builder()
        .endpoints(vec![parsed])
        .map_err(update_error)?
        .build()
        .map_err(update_error)?;
    Ok(updater)
}

/// F13：两段式。第一步只下载（签名在下载时由插件校验），不安装、不落回执；
/// 验签后的原字节与清单保存在Revia独占数据目录，第二步确认后才安装。
pub async fn download_update(
    app: tauri::AppHandle,
    state: &UpdateState,
    mode: AppUpdateMode,
) -> Result<AppUpdateStatus> {
    ensure_install_allowed(Distribution::current())?;
    let _ = state.begin()?;
    let result = download_inner(&app, state, mode).await;
    state.end();
    result
}

/// 第二步：安装已下载的更新（再次确认后调用），成功后写回执。
pub async fn download_and_install(
    app: tauri::AppHandle,
    state: &UpdateState,
    mode: AppUpdateMode,
) -> Result<AppUpdateStatus> {
    ensure_install_allowed(Distribution::current())?;
    let _ = state.begin()?;
    let result = install_inner(&app, state, mode).await;
    state.end();
    result
}

/// 取一次可用更新（内部共用）。
async fn fetch_update(
    app: &tauri::AppHandle,
    state: &UpdateState,
    mode: AppUpdateMode,
) -> Result<tauri_plugin_updater::Update> {
    let status = state.status(app, mode);
    if !status.configured {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.updateNotConfigured",
            "更新通道未配置",
            &[],
        ));
    }
    find_update(app, state).await?.ok_or_else(|| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.updateNothingToInstall",
            "当前已经是最新版本",
            &[],
        )
    })
}

async fn find_update(
    app: &tauri::AppHandle,
    state: &UpdateState,
) -> Result<Option<tauri_plugin_updater::Update>> {
    let configured = configured_endpoints(app)?;
    let allow = state.allow_prerelease();
    let current = effective_version(app, &state.load().receipt);
    let selected = if let Some((base, list)) =
        super::update_channels::github_repository(&configured.endpoint)
    {
        super::update_channels::discover(&base, list, allow, &current).await?
    } else {
        // 独立受控验收源和显式配置的自托管清单；不猜测另一个渠道URL。
        let updater = channel_updater(app)?;
        return Ok(updater.check().await.map_err(update_error)?.filter(|u| {
            semver::Version::parse(&u.version)
                .is_ok_and(|version| version > current && (allow || version.pre.is_empty()))
        }));
    };
    let Some(selected) = selected else {
        return Ok(None);
    };
    let updater = app
        .updater_builder()
        .endpoints(vec![selected.endpoint])
        .map_err(update_error)?
        .build()
        .map_err(update_error)?;
    let update = updater.check().await.map_err(update_error)?;
    if update
        .as_ref()
        .is_some_and(|u| u.version != selected.version.to_string())
    {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.updateReleaseIncomplete",
            "更新发布信息不完整，已保留上次状态",
            &[],
        ));
    }
    Ok(update)
}

fn progress_emitter(app: &tauri::AppHandle) -> impl FnMut(usize, Option<u64>) + Send + 'static {
    let app = app.clone();
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;
    move |chunk, total| {
        use tauri::Emitter;
        downloaded += chunk as u64;
        // 每 256 KB 发一次进度，避免事件淹没界面。
        if downloaded.saturating_sub(last_emit) >= 256 * 1024 {
            last_emit = downloaded;
            let _ = app.emit(
                "revia://update-progress",
                UpdateProgress {
                    downloaded,
                    total: total,
                },
            );
        }
    }
}

async fn download_inner(
    app: &tauri::AppHandle,
    state: &UpdateState,
    mode: AppUpdateMode,
) -> Result<AppUpdateStatus> {
    let expected = state
        .load()
        .available
        .ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.m7.updateNothingToInstall",
                "请先检查更新",
                &[],
            )
        })?
        .version;
    let update = fetch_update(app, state, mode).await?;
    if update.version != expected {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.updateReleaseIncomplete",
            "可用更新已变化，请重新核对",
            &[],
        ));
    }
    let bytes = update
        .download(progress_emitter(app), || {})
        .await
        .map_err(update_error)?;
    let updater = channel_updater(app)?;
    let package =
        super::update_package::save(&updater, &update, state.path.parent().unwrap(), &bytes)?;
    // 记录“已下载待安装”，重启后界面也能看到这一状态。
    let mut stored = state.load();
    stored.downloaded_version = None; // 旧版本只有标记、没有真实字节，不能认作缓存。
    stored.downloaded = Some(package);
    state.store(&stored)?;
    Ok(state.status(app, mode))
}

async fn install_inner(
    app: &tauri::AppHandle,
    state: &UpdateState,
    mode: AppUpdateMode,
) -> Result<AppUpdateStatus> {
    let stored = state.load();
    let saved = stored.downloaded.as_ref().ok_or_else(|| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.updateNotDownloaded",
            "更新尚未下载，请先下载更新",
            &[],
        )
    })?;
    // build/restore/verify均不联网；只安装已确认下载的原字节。
    let updater = channel_updater(app)?;
    let (update, bytes) = super::update_package::restore(
        &updater,
        state.path.parent().unwrap(),
        saved,
        state.allow_prerelease(),
    )?;
    let version = update.version.clone();
    // Windows 安装器会结束本进程：先落“待安装”，下一次启动按真实版本核对（M8 任务 05）。
    let mut stored = state.load();
    stored.pending_install = Some(UpdatePendingInstall {
        version: version.clone(),
        started_at_ms: now_ms(),
    });
    state.store(&stored)?;
    if let Err(error) = update.install(bytes) {
        // 安装调用失败时保留已下载字节，允许用户核对后重试。
        let mut stored = state.load();
        stored.pending_install = None;
        let _ = state.store(&stored);
        return Err(AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.m7.updateInstallNeedsVerification",
            "安装更新未完成，请核对应用状态后再操作：{error}",
            &[("error", error.to_string())],
        ));
    }
    let mut stored = state.load();
    stored.receipt = Some(UpdateReceipt {
        version: version.clone(),
        installed_at_ms: now_ms(),
    });
    stored.pending_install = None;
    stored.available = None;
    stored.downloaded_version = None;
    stored.downloaded = None;
    state.store(&stored).map_err(|error| {
        AppError::coded(
            ErrorKind::NeedsVerification,
            "errors.m7.updateInstallRecordFailed",
            "更新已安装，但本地回执写入失败，请先核对应用状态",
            &[],
        )
        .cause(error)
    })?;
    Ok(state.status(app, mode))
}

/// 安装完成后由用户确认重启；便携版没有安装，不提供重启入口。
pub fn restart(app: &tauri::AppHandle, distribution: Distribution) -> Result<()> {
    ensure_install_allowed(distribution)?;
    app.restart()
}

/// 便携版：打开对应版本的便携包下载地址（不调用安装器）。
pub fn open_portable_download(
    app: &tauri::AppHandle,
    state: &UpdateState,
    mode: AppUpdateMode,
) -> Result<()> {
    if !Distribution::current().is_portable() {
        return Err(AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.portableDownloadOnly",
            "只有便携版使用便携包下载入口",
            &[],
        ));
    }
    let status = state.status(app, mode);
    let url = status.portable_download_url.ok_or_else(|| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.portableDownloadUnavailable",
            "当前没有可下载的便携版本，请先检查更新",
            &[],
        )
    })?;
    crate::platform::open_url(&url)
}

/// 启动后的自动检查：非手动模式时按间隔检查，发现新版本发事件。
///
/// 只在配置了更新通道时工作；失败不阻塞启动，也不反复弹窗。
pub fn spawn_auto_check(
    app: tauri::AppHandle,
    data_dir: PathBuf,
    state: std::sync::Arc<UpdateState>,
) {
    std::thread::spawn(move || loop {
        let mode = read_settings(&data_dir)
            .map(|settings| settings.app_update_mode)
            .unwrap_or_default();
        if mode != AppUpdateMode::Manual && unconfigured_reason(&app).is_none() {
            let app_for_check = app.clone();
            let outcome = tauri::async_runtime::block_on(check(app_for_check, &state, mode, false));
            if let Ok(status) = outcome {
                if let Some(available) = status.available {
                    use tauri::Emitter;
                    let _ = app.emit("revia://update-available", available);
                }
            }
        }
        std::thread::sleep(AUTO_CHECK_INTERVAL);
    });
}

fn update_error(error: tauri_plugin_updater::Error) -> AppError {
    AppError::coded(
        ErrorKind::Environment,
        "errors.m7.updateCheckFailed",
        "检查更新失败：{error}",
        &[("error", error.to_string())],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_state_round_trips_and_ignores_broken_files() {
        let dir = tempfile::tempdir().unwrap();
        let state = UpdateState::new(&dir.path(), &dir.path());
        assert!(state.load().receipt.is_none());
        state
            .store(&StoredState {
                last_checked_at_ms: Some(5),
                available: Some(AvailableUpdate {
                    version: "1.2.3".into(),
                    current_version: "1.2.2".into(),
                    notes: Some("修复".into()),
                    published_at: None,
                }),
                receipt: Some(UpdateReceipt {
                    version: "1.2.3".into(),
                    installed_at_ms: 9,
                }),
                downloaded_version: None,
                downloaded: None,
                checked_prerelease: false,
                pending_install: None,
            })
            .unwrap();
        let loaded = state.load();
        assert_eq!(loaded.last_checked_at_ms, Some(5));
        assert_eq!(loaded.available.unwrap().version, "1.2.3");
        assert_eq!(loaded.receipt.unwrap().installed_at_ms, 9);
        std::fs::write(&state.path, b"{not json").unwrap();
        assert!(state.load().available.is_none());
    }

    #[test]
    fn pending_install_is_confirmed_only_when_new_process_reaches_version() {
        let mut stored = StoredState {
            pending_install: Some(UpdatePendingInstall {
                version: "1.2.3".into(),
                started_at_ms: 7,
            }),
            ..StoredState::default()
        };
        // 安装器被取消或失败：仍停在旧版本，保留待安装状态供核对。
        assert!(!reconcile_pending_install(
            &mut stored,
            &semver::Version::parse("1.2.2").unwrap()
        ));
        assert!(stored.pending_install.is_some());
        assert!(stored.receipt.is_none());

        // 新进程版本达到待安装版本：确认为安装回执。
        assert!(reconcile_pending_install(
            &mut stored,
            &semver::Version::parse("1.2.3").unwrap()
        ));
        assert!(stored.pending_install.is_none());
        assert_eq!(stored.receipt.unwrap().version, "1.2.3");
    }

    #[test]
    fn portable_distribution_refuses_download_install_and_restart() {
        assert!(ensure_install_allowed(Distribution::Installed).is_ok());
        assert_eq!(
            ensure_install_allowed(Distribution::Portable).unwrap_err().code,
            Some("errors.m7.portableUpdateInstallBlocked")
        );
    }

    #[test]
    fn portable_download_url_follows_the_release_naming_convention() {
        let endpoint = "https://github.com/owner/repo/releases/latest/download/latest.json";
        assert_eq!(
            portable_download_url(endpoint, "1.2.3").as_deref(),
            Some("https://github.com/owner/repo/releases/download/v1.2.3/Revia-1.2.3-windows-x64-portable.zip")
        );
        assert!(portable_download_url("https://example.com/latest.json", "1.2.3").is_none());
        assert!(portable_download_url("http://github.com/owner/repo/releases/latest/download/latest.json", "1.2.3").is_none());
    }

    #[test]
    fn busy_guard_rejects_parallel_installs() {
        let dir = tempfile::tempdir().unwrap();
        let state = UpdateState::new(&dir.path(), &dir.path());
        state.begin().unwrap();
        assert_eq!(
            state.begin().unwrap_err().code,
            Some("errors.m7.updateBusy")
        );
        state.end();
        state.begin().unwrap();
    }
}
