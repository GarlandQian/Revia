//! 测量结果落盘：让 GUI 里的测量可以被脚本读取，而不是只在窗口里看。

use std::path::PathBuf;

use crate::error::{AppError, Result};

/// 结果写到哪里：
/// - `REVIA_M0_MEASURE_LOG` 直接指定文件；
/// - `REVIA_M0_MEASURE_DIR` 指定目录，文件名按时间戳生成。
pub fn target_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("REVIA_M0_MEASURE_LOG") {
        return Some(PathBuf::from(path));
    }
    let dir = std::env::var_os("REVIA_M0_MEASURE_DIR")?;
    let dir = PathBuf::from(dir);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    Some(dir.join(format!("gui-measure-{stamp}.json")))
}

/// GUI 在启动时把 `REVIA_M0_MEASURE=1` 当作“自动跑一轮测量”的开关。
pub fn auto_run_requested() -> bool {
    matches!(
        std::env::var("REVIA_M0_MEASURE").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

/// 允许只用环境变量指定仓库路径，避免依赖界面里的默认值。
pub fn measure_repo() -> Option<String> {
    std::env::var("REVIA_M0_REPO").ok().filter(|value| !value.is_empty())
}

/// 自动测量的目标文件；设置为空字符串表示整仓（不限定文件）。
pub fn measure_path() -> Option<String> {
    std::env::var("REVIA_M0_PATH").ok()
}

/// 只跑指定的组合，形如 `shiki:unified,syntect:split`，便于单独排查某个组合。
pub fn measure_only() -> Option<Vec<String>> {
    let value = std::env::var("REVIA_M0_ONLY").ok()?;
    if value.trim().is_empty() {
        return None;
    }
    Some(value.split(',').map(|item| item.trim().to_string()).filter(|item| !item.is_empty()).collect())
}

pub fn write_json(path: &std::path::Path, payload: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| {
                AppError::internal(format!("创建输出目录失败（{}）：{error}", parent.display()))
            })?;
        }
    }
    let text = serde_json::to_string_pretty(payload)?;
    std::fs::write(path, text)
        .map_err(|error| AppError::internal(format!("写入 {} 失败：{error}", path.display())))?;
    Ok(())
}

/// 诊断日志路径：与测量结果同一目录，便于一起收集。
pub fn log_path() -> Option<PathBuf> {
    let path = target_path()?;
    Some(path.with_extension("log"))
}

pub fn append_log(path: &std::path::Path, message: &str) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).ok();
        }
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| AppError::internal(format!("打开日志失败（{}）：{error}", path.display())))?;
    writeln!(file, "{message}")
        .map_err(|error| AppError::internal(format!("写日志失败：{error}")))?;
    Ok(())
}
