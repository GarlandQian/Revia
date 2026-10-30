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
}

/// 更新状态的本地存储与并发保护。
pub struct UpdateState {
    path: PathBuf,
    busy: Mutex<bool>,
}

impl UpdateState {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("update-state.json"),
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
    let updater = app.updater_builder().build().map_err(update_error)?;
    let found = updater.check().await.map_err(update_error)?;
    let mut stored = state.load();
    stored.last_checked_at_ms = Some(now);
    stored.available = found.as_ref().map(|update| AvailableUpdate {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        notes: update.body.clone(),
        published_at: update.date.map(|date| date.to_string()),
    });
    state.store(&stored)?;
    Ok(state.status(app, mode))
}

/// 下载并安装（签名由 updater 插件按公钥验证），成功后写回执。
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

async fn install_inner(
    app: &tauri::AppHandle,
    state: &UpdateState,
    mode: AppUpdateMode,
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
    let updater = app.updater_builder().build().map_err(update_error)?;
    let update = updater.check().await.map_err(update_error)?.ok_or_else(|| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.updateNothingToInstall",
            "当前已经是最新版本",
            &[],
        )
    })?;
    let version = update.version.clone();
    let app_for_progress = app.clone();
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;
    update
        .download_and_install(
            move |chunk, total| {
                use tauri::Emitter;
                downloaded += chunk as u64;
                // 每 256 KB 发一次进度，避免事件淹没界面。
                if downloaded.saturating_sub(last_emit) >= 256 * 1024 {
                    last_emit = downloaded;
                    let _ = app_for_progress.emit(
                        "revia://update-progress",
                        UpdateProgress {
                            downloaded,
                            total: total.map(|value| value as u64),
                        },
                    );
                }
            },
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
    let mut stored = state.load();
    stored.receipt = Some(UpdateReceipt {
        version,
        installed_at_ms: now_ms(),
    });
    stored.available = None;
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
            let state = UpdateState::new(&data_dir);
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
        let state = UpdateState::new(dir.path());
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
        let state = UpdateState::new(dir.path());
        state.begin().unwrap();
        assert_eq!(
            state.begin().unwrap_err().code,
            Some("errors.m7.updateBusy")
        );
        state.end();
        state.begin().unwrap();
    }
}
