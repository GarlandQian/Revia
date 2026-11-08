//! 测量版 / 验收版构建（M9，Q216）。
//!
//! 只有打开 `measure` 编译特性时才存在对照开关与构建来源。正式构建里
//! [`IS_MEASURE`] 为 false，菜单入口与命令都不注册，窗口外观也不读这个开关。
//!
//! 对照外观（朴素外观）在下次启动时生效：窗口不透明、不用 Mica / 原生玻璃，
//! 前端也不挂 `window-material`，所以没有模糊、镜片和半透明。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// 本次构建是否打开了 `measure` 特性。
pub const IS_MEASURE: bool = cfg!(feature = "measure");

/// 构建来源的提交短 SHA；工作流通过 `REVIA_BUILD_COMMIT` 注入。
pub fn commit_sha() -> &'static str {
    option_env!("REVIA_BUILD_COMMIT").unwrap_or("unknown")
}

/// 对照外观（朴素外观）的持久化文件。
const PLAIN_FILE: &str = "measure-plain-appearance.json";

static PLAIN: AtomicBool = AtomicBool::new(false);
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 启动时调用一次：记住数据目录并读入上次保存的对照开关。
pub fn init(data_dir: &Path) {
    let _ = DATA_DIR.set(data_dir.to_path_buf());
    PLAIN.store(load(data_dir), Ordering::Relaxed);
}

/// 对照外观是否开启；正式构建永远为 false。
pub fn plain_appearance() -> bool {
    IS_MEASURE && PLAIN.load(Ordering::Relaxed)
}

/// 修改对照外观：立即写入文件，下次启动生效。
pub fn set_plain_appearance(enabled: bool) -> Result<(), crate::error::AppError> {
    PLAIN.store(enabled, Ordering::Relaxed);
    let Some(directory) = DATA_DIR.get() else {
        return Ok(());
    };
    let bytes = serde_json::to_vec_pretty(&serde_json::json!({ "plainAppearance": enabled }))
        .unwrap_or_default();
    crate::m7::atomic_write(&directory.join(PLAIN_FILE), &bytes)
}

fn load(data_dir: &Path) -> bool {
    let Ok(bytes) = std::fs::read(data_dir.join(PLAIN_FILE)) else {
        return false;
    };
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|value| value.get("plainAppearance").and_then(|flag| flag.as_bool()))
        .unwrap_or(false)
}

/// 前端读取的测量状态（只在 `measure` 构建里注册）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasureState {
    pub measure: bool,
    pub build: String,
    pub commit: String,
    pub plain_appearance: bool,
}

pub fn state() -> MeasureState {
    MeasureState {
        measure: IS_MEASURE,
        build: if IS_MEASURE { "m9" } else { "" }.to_string(),
        commit: commit_sha().to_string(),
        plain_appearance: plain_appearance(),
    }
}

/// 诊断信息里的构建来源与对照状态。
pub fn diagnostics_json() -> serde_json::Value {
    serde_json::json!({
        "measure": IS_MEASURE,
        "build": if IS_MEASURE { "m9" } else { "" },
        "commit": commit_sha(),
        "plainAppearance": plain_appearance(),
    })
}
