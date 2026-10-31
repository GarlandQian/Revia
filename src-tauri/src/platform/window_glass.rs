//! 窗口外壳的原生 Liquid Glass（macOS 26+，平台代码集中在这里，Q55）。
//!
//! 玻璃由 AppKit 的 NSGlassEffectView 绘制，webview 只画内容：前端把需要玻璃的外壳区域
//! （浮动侧栏、工具栏胶囊）的矩形同步过来，原生层在 webview 下方逐块放玻璃；
//! 网页在这些区域保持透明，代码与 diff 仍由网页画不透明底色（Q190）。
//! Windows 11 用 Mica 铺满整窗（含原生标题栏），不按区域放玻璃，胶囊与对话框由网页画（Q191）。
//! 其他平台与系统版本返回 `Unsupported`，前端继续用 CSS 材质。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 区域在一个方向上贴住哪条边：窗口缩放时原生层先按它跟随，再等前端送来新矩形。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GlassEdge {
    Start,
    End,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GlassStyle {
    Regular,
    Clear,
}

/// 一块玻璃。坐标是窗口内容区里的 CSS 像素（左上角为原点），与 AppKit 点一致。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GlassRegion {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
    pub horizontal: GlassEdge,
    pub vertical: GlassEdge,
    pub style: GlassStyle,
}

/// 窗口底：实色，或整窗一块玻璃。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GlassBackground {
    /// 窗口不透明，底色为 sRGB 0–255（编辑器底色）。
    Solid([u8; 3]),
    /// 窗口透明，整窗铺一块玻璃折射窗口后面的桌面；圆角与系统窗口一致。
    Glass { radius: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GlassLayout {
    pub background: GlassBackground,
    /// 浮在窗口底上的玻璃（工具栏胶囊等）。
    pub regions: Vec<GlassRegion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GlassSupport {
    /// macOS 26+：整窗系统玻璃，工具栏胶囊、对话框面板按区域放系统玻璃。
    Native,
    /// Windows 11：整窗 Mica；区域上的材质由网页画。
    Mica,
    Unsupported,
}

/// `REVIA_GLASS=css` 强制走 CSS 材质，便于对照。
pub fn support() -> GlassSupport {
    if std::env::var("REVIA_GLASS").is_ok_and(|value| value == "css") {
        return GlassSupport::Unsupported;
    }
    #[cfg(target_os = "macos")]
    if super::macos_glass::supported() {
        return GlassSupport::Native;
    }
    // Mica 从 Windows 11（build 22000）起才有。
    #[cfg(windows)]
    if windows_version::OsVersion::current().build >= 22_000 {
        return GlassSupport::Mica;
    }
    GlassSupport::Unsupported
}

/// 更新窗口底与玻璃区域。macOS 在主线程上安装玻璃容器、关掉 webview 底色并逐块放玻璃；
/// Windows 只切换整窗 Mica。
pub fn apply(window: &tauri::WebviewWindow, layout: GlassLayout) -> tauri::Result<()> {
    match support() {
        #[cfg(target_os = "macos")]
        GlassSupport::Native => {
            window.with_webview(move |webview| super::macos_glass::apply(webview.inner(), &layout))?;
        }
        GlassSupport::Mica => apply_mica(window, layout.background)?,
        _ => {}
    }
    Ok(())
}

/// Windows 11：窗口底用 Mica，WebView2 不画底色（alpha 0 是 WebView2 唯一接受的透明值）。
/// 窗口本身必须是透明窗口才看得到特效，见 `tauri.windows.conf.json`。
/// 用的是 Tauri 的跨平台接口，macOS 上也能编译检查（那里 `support()` 不会返回 Mica）。
fn apply_mica(window: &tauri::WebviewWindow, background: GlassBackground) -> tauri::Result<()> {
    use tauri::utils::config::WindowEffectsConfig;
    use tauri::window::{Color, Effect, EffectsBuilder};
    match background {
        GlassBackground::Glass { .. } => {
            window.set_effects(EffectsBuilder::new().effect(Effect::Mica).build())?;
            window.set_background_color(Some(Color(0, 0, 0, 0)))?;
        }
        GlassBackground::Solid([red, green, blue]) => {
            window.set_effects(None::<WindowEffectsConfig>)?;
            window.set_background_color(Some(Color(red, green, blue, 255)))?;
        }
    }
    Ok(())
}
