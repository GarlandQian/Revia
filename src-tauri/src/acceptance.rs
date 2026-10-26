//! 显式隔离实例的验收接缝。只延迟真实结果的交付，不替换 Git 或响应内容。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, Result};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct HoldRule {
    id: String,
    command: String,
    worktree_id: Option<String>,
    path: Option<String>,
}

pub struct Acceptance {
    directory: PathBuf,
    capture_lock: Mutex<()>,
}

fn timestamp() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn data_directory(normal: &Path) -> Result<Option<PathBuf>> {
    if std::env::var("REVIA_ACCEPTANCE").as_deref() != Ok("1") {
        return Ok(None);
    }
    let directory = std::env::var_os("REVIA_ACCEPTANCE_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| AppError::environment("验收实例必须指定独立数据目录"))?;
    if !directory.is_absolute() || !directory.is_dir() {
        return Err(AppError::environment("验收数据目录必须是已存在的绝对目录"));
    }
    let directory = fs::canonicalize(directory)?;
    let normal = fs::canonicalize(normal).unwrap_or_else(|_| normal.to_path_buf());
    if directory == normal {
        return Err(AppError::environment("验收实例不能使用正常应用数据目录"));
    }
    Ok(Some(directory))
}

impl Acceptance {
    pub fn new(directory: PathBuf) -> Result<Self> {
        for child in ["claimed", "captured", "release", "delivered"] {
            fs::create_dir_all(directory.join(child))?;
        }
        Ok(Self { directory, capture_lock: Mutex::new(()) })
    }

    fn capture<T: Serialize>(&self, command: &str, worktree_id: &str, path: Option<&str>, result: &Result<T>) -> Result<Option<String>> {
        let _guard = self.capture_lock.lock().map_err(|_| AppError::internal("验收接缝锁失败"))?;
        let control = self.directory.join("hold.json");
        let bytes = match fs::read(&control) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let rule: HoldRule = serde_json::from_slice(&bytes)?;
        if rule.command != command
            || rule.worktree_id.as_deref().is_some_and(|target| target != worktree_id)
            || rule.path.as_deref().is_some_and(|target| Some(target) != path)
        {
            return Ok(None);
        }
        if rule.id.is_empty() || rule.id.len() > 80
            || !rule.id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(AppError::rejected("验收请求 ID 只能包含字母、数字、横线与下划线"));
        }
        let mut claimed = OpenOptions::new().write(true).create_new(true)
            .open(self.directory.join("claimed").join(format!("{}.json", rule.id)))?;
        claimed.write_all(&bytes)?;
        fs::remove_file(control)?;
        let payload = serde_json::to_value(result)?;
        let encoded = serde_json::to_vec(&payload)?;
        let value = payload.get("Ok");
        let summary = serde_json::json!({
            "id": rule.id, "command": command, "worktreeId": worktree_id, "path": path,
            "pid": std::process::id(), "capturedAtMs": timestamp(),
            "outcome": if result.is_ok() { "ok" } else { "error" },
            "revision": value.and_then(|value| value.get("revision")),
            "contentVersion": value.and_then(|value| value.get("contentVersion")),
            "entryFingerprint": value.and_then(|value| value.get("entry")).and_then(|entry| entry.get("fingerprint")),
            "payloadSha256": sha256(&encoded),
        });
        let mut capture = OpenOptions::new().write(true).create_new(true)
            .open(self.directory.join("captured").join(format!("{}.json", rule.id)))?;
        capture.write_all(&serde_json::to_vec_pretty(&summary)?)?;
        Ok(Some(rule.id))
    }

    fn wait_for_release(&self, id: &str) -> Result<()> {
        let started = Instant::now();
        let release = self.directory.join("release").join(id);
        while !release.is_file() && started.elapsed() < Duration::from_secs(300) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let receipt = serde_json::json!({
            "id": id, "releasedAtMs": timestamp(), "explicit": release.is_file(),
        });
        let mut file = OpenOptions::new().write(true).create_new(true)
            .open(self.directory.join("delivered").join(format!("{id}.json")))?;
        file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
        Ok(())
    }

    fn record_error(&self, error: &dyn std::fmt::Display) {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(self.directory.join("errors.log")) {
            let _ = writeln!(file, "{} {error}", timestamp());
        }
    }

    pub fn watch_failure(&self) -> Option<String> {
        match fs::read_to_string(self.directory.join("watch-failure.txt")) {
            Ok(reason) => Some(format!("自动刷新不可用：验收注入：{}", reason.trim())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => Some(format!("自动刷新不可用：验收控制文件读取失败：{error}")),
        }
    }
}

/// 在真实操作及其所有锁释放后等待；正常运行没有目录读取或序列化开销。
pub async fn deliver<T: Serialize + Send>(
    acceptance: Option<&Arc<Acceptance>>,
    command: &str,
    worktree_id: &str,
    path: Option<&str>,
    result: Result<T>,
) -> Result<T> {
    if let Some(gate) = acceptance {
        match gate.capture(command, worktree_id, path, &result) {
            Ok(Some(id)) => {
                let waiting = gate.clone();
                match tauri::async_runtime::spawn_blocking(move || waiting.wait_for_release(&id)).await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => gate.record_error(&error),
                    Err(error) => gate.record_error(&error),
                }
            }
            Ok(None) => {}
            Err(error) => gate.record_error(&error),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_is_targeted_one_shot_and_preserves_real_result() {
        let dir = tempfile::tempdir().unwrap();
        let gate = Acceptance::new(dir.path().to_path_buf()).unwrap();
        fs::write(dir.path().join("hold.json"), r#"{"id":"one","command":"get_rows","worktreeId":"a","path":"x.ts"}"#).unwrap();
        let value: Result<_> = Ok(serde_json::json!({"revision": 7, "rows": ["real bytes"]}));
        assert_eq!(gate.capture("get_rows", "b", Some("x.ts"), &value).unwrap(), None);
        assert_eq!(gate.capture("get_rows", "a", Some("y.ts"), &value).unwrap(), None);
        assert_eq!(gate.capture("get_rows", "a", Some("x.ts"), &value).unwrap(), Some("one".into()));
        assert_eq!(gate.capture("get_rows", "a", Some("x.ts"), &value).unwrap(), None);
        let capture: serde_json::Value = serde_json::from_slice(&fs::read(dir.path().join("captured/one.json")).unwrap()).unwrap();
        assert_eq!(capture["revision"], 7);
        assert_eq!(capture["payloadSha256"], sha256(&serde_json::to_vec(&value).unwrap()));
        assert_eq!(value.unwrap()["rows"][0], "real bytes");
        fs::write(dir.path().join("release/one"), "").unwrap();
        gate.wait_for_release("one").unwrap();
        let receipt: serde_json::Value = serde_json::from_slice(&fs::read(dir.path().join("delivered/one.json")).unwrap()).unwrap();
        assert_eq!(receipt["explicit"], true);
    }

    #[test]
    fn invalid_capture_id_cannot_escape_directory_and_watch_failure_is_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let gate = Acceptance::new(dir.path().to_path_buf()).unwrap();
        fs::write(dir.path().join("hold.json"), r#"{"id":"../escape","command":"list_changes"}"#).unwrap();
        assert!(gate.capture("list_changes", "a", None, &Ok(1)).is_err());
        assert!(gate.watch_failure().is_none());
        fs::write(dir.path().join("watch-failure.txt"), "watch unavailable").unwrap();
        assert!(gate.watch_failure().unwrap().contains("watch unavailable"));
    }
}
