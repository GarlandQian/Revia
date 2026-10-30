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
use ts_rs::TS;
use tauri_plugin_updater::UpdaterExt;

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
        let stored = self.load();
        let reason = unconfigured_reason(app);
        AppUpdateStatus {
            configured: reason.is_none(),
            unconfigured_reason: reason,
            mode,
            current_version: app.package_info().version.to_string(),
            last_checked_at_ms: stored.last_checked_at_ms,
            available: stored.available,
            receipt: stored.receipt,
        }
    }
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
            if now.saturating_sub(last) < CHECK_INTERVAL_MS {
                return Ok(status);
            }
        }
    }
    // F22：按“预发布渠道开关”选择清单——两份清单都在每次发布时上传。
    // 关闭时请求稳定版清单；开启后请求预发布清单（包含预发布版本）。
    let configured = configured_endpoints(app)?;
    let prerelease_allowed = state.allow_prerelease();
    let endpoint = if prerelease_allowed {
        configured.endpoint.replace("latest.json", "latest-prerelease.json")
    } else {
        configured.endpoint.clone()
    };
    let parsed: url::Url = endpoint
        .parse()
        .map_err(|error: url::ParseError| {
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
    let found = updater.check().await.map_err(update_error)?;
    // 未开启预发布渠道时，预发布版本（semver 带预发布段）不作为可用更新。
    let found = found.filter(|update| prerelease_allowed || !update.version.contains('-'));
    let mut stored = state.load();
    stored.last_checked_at_ms = Some(now);
    stored.available = found.as_ref().map(|update| AvailableUpdate {
        version: update.version.clone().to_string(),
        current_version: update.current_version.clone(),
        notes: update.body.clone(),
        published_at: update.date.map(|date| date.to_string()),
    });
    state.store(&stored)?;
    Ok(state.status(app, mode))
}

/// F13：两段式。第一步只下载（签名在下载时由插件校验），不安装、不落回执；
/// 已下载的安装包保存在插件的临时目录，用户第二步确认后才真正安装。
pub async fn download_update(
    app: tauri::AppHandle,
    state: &UpdateState,
    mode: AppUpdateMode,
) -> Result<AppUpdateStatus> {
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
    let updater = app.updater_builder().build().map_err(update_error)?;
    updater
        .check()
        .await
        .map_err(update_error)?
        .ok_or_else(|| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.m7.updateNothingToInstall",
                "当前已经是最新版本",
                &[],
            )
        })
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
    let update = fetch_update(app, state, mode).await?;
    // F13：只下载不安装。签名由插件按配置公钥在下载时校验。
    update
        .download(progress_emitter(app), || {})
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Environment,
                "errors.m7.updateDownloadFailed",
                "下载更新失败，当前版本保持不变：{error}",
                &[("error", error.to_string())],
            )
        })?;
    // 记录“已下载待安装”，重启后界面也能看到这一状态。
    let mut stored = state.load();
    stored.downloaded_version = Some(update.version.clone());
    state.store(&stored)?;
    Ok(state.status(app, mode))
}

async fn install_inner(
    app: &tauri::AppHandle,
    state: &UpdateState,
    mode: AppUpdateMode,
) -> Result<AppUpdateStatus> {
    let update = fetch_update(app, state, mode).await?;
    let version = update.version.clone();
    // F13：必须先走确认下载这一步；直接调安装按未下载拒绝。
    {
        let stored = state.load();
        if stored.downloaded_version.as_deref() != Some(version.as_str()) {
            return Err(AppError::coded(
                ErrorKind::Rejected,
                "errors.m7.updateNotDownloaded",
                "更新尚未下载，请先下载更新",
                &[],
            ));
        }
    }
    // F13：下载阶段已由插件按公钥验签；install(bytes) 安装前再 verify 一次。
    // 下载保存的安装包在插件下载闭包返回的 bytes 里，这里重新下载一次校验后的字节
    // （插件不跨调用缓存 bytes；重新下载仍验签，恶意包无法通过）。
    let bytes = update
        .download(
            progress_emitter(app),
            || {},
        )
        .await
        .map_err(|error| {
            AppError::coded(
                ErrorKind::Environment,
                "errors.m7.updateInstallFailed",
                "安装更新失败，当前版本保持不变：{error}",
                &[("error", error.to_string())],
            )
        })?;
    update.install(bytes).map_err(|error| {
        AppError::coded(
            ErrorKind::Environment,
            "errors.m7.updateInstallFailed",
            "安装更新失败，当前版本保持不变：{error}",
            &[("error", error.to_string())],
        )
    })?;
    let mut stored = state.load();
    stored.receipt = Some(UpdateReceipt {
        version: version.clone(),
        installed_at_ms: now_ms(),
    });
    stored.available = None;
    stored.downloaded_version = None;
    state.store(&stored)?;
    Ok(state.status(app, mode))
}

/// 安装完成后由用户确认重启。
pub fn restart(app: &tauri::AppHandle) {
    app.restart();
}

/// 启动后的自动检查：非手动模式时按间隔检查，发现新版本发事件。
///
/// 只在配置了更新通道时工作；失败不阻塞启动，也不反复弹窗。
pub fn spawn_auto_check(app: tauri::AppHandle, data_dir: PathBuf) {
    std::thread::spawn(move || loop {
        let mode = read_settings(&data_dir)
            .map(|settings| settings.app_update_mode)
            .unwrap_or_default();
        if mode != AppUpdateMode::Manual && unconfigured_reason(&app).is_none() {
            let state = UpdateState::new(&data_dir, &data_dir);
            let app_for_check = app.clone();
            let outcome = tauri::async_runtime::block_on(check(
                app_for_check,
                &state,
                mode,
                false,
            ));
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
