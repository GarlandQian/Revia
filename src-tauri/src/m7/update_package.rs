//! 已下载包的实际字节与签名清单；恢复安装完全不联网，版本由签名绑定。
use crate::error::{AppError, ErrorKind, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri_plugin_updater::{Update, Updater};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedUpdate {
    pub version: String,
    pub manifest: serde_json::Value,
    pub sha256: String,
}
fn invalid() -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.m7.updatePackageInvalid",
        "已下载更新包不完整、版本不匹配或签名无效；请重新下载",
        &[],
    )
}
fn plugin(error: tauri_plugin_updater::Error) -> AppError {
    AppError::coded(
        ErrorKind::Rejected,
        "errors.m7.updatePackageInvalid",
        "已下载更新包不完整、版本不匹配或签名无效；请重新下载",
        &[],
    )
    .cause(AppError::coded(
        ErrorKind::Environment,
        "errors.m7.updateDownloadFailed",
        "下载更新失败：{error}",
        &[("error", error.to_string())],
    ))
}
fn path(directory: &Path, sha: &str) -> Result<PathBuf> {
    if sha.len() != 64 || !sha.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    Ok(directory
        .join("update-packages")
        .join(format!("{sha}.package")))
}
pub fn save(
    updater: &Updater,
    update: &Update,
    directory: &Path,
    bytes: &[u8],
) -> Result<DownloadedUpdate> {
    // restore_update强制检查签名内的版本，不能把一个旧的合法包配上新版本号保存。
    let restored = updater
        .restore_update(update.raw_json.clone())
        .map_err(plugin)?
        .ok_or_else(invalid)?;
    if restored.version != update.version || restored.download_url != update.download_url {
        return Err(invalid());
    }
    restored.verify(bytes).map_err(plugin)?;
    let sha256 = crate::m7::sha256_hex(bytes);
    crate::m7::atomic_write(&path(directory, &sha256)?, bytes)?;
    Ok(DownloadedUpdate {
        version: update.version.clone(),
        manifest: update.raw_json.clone(),
        sha256,
    })
}
pub fn restore(
    updater: &Updater,
    directory: &Path,
    saved: &DownloadedUpdate,
    allow_prerelease: bool,
) -> Result<(Update, Vec<u8>)> {
    let version = semver::Version::parse(&saved.version).map_err(|_| invalid())?;
    if !allow_prerelease && !version.pre.is_empty() {
        return Err(invalid());
    }
    let update = updater
        .restore_update(saved.manifest.clone())
        .map_err(plugin)?
        .ok_or_else(invalid)?;
    if update.version != saved.version {
        return Err(invalid());
    }
    let bytes = std::fs::read(path(directory, &saved.sha256)?)?;
    if crate::m7::sha256_hex(&bytes) != saved.sha256 {
        return Err(invalid());
    }
    update.verify(&bytes).map_err(plugin)?;
    Ok((update, bytes))
}
pub fn exists(directory: &Path, saved: &DownloadedUpdate) -> bool {
    path(directory, &saved.sha256).is_ok_and(|path| path.is_file())
}
