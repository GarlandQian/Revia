//! AppKit 侧的原生玻璃：在窗口 contentView（wry 的 WryWebViewParent）里、WKWebView 下方
//! 放一个 NSGlassEffectContainerView，每个外壳区域一块 NSGlassEffectView。
//! 整窗玻璃放在容器外、垫在容器下面：同一个容器里的玻璃会合成一层，胶囊就看不出来了。
//! webview 在最上面接收全部输入，玻璃只负责材质。

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSColor, NSGlassEffectContainerView, NSGlassEffectView,
    NSGlassEffectViewStyle, NSView, NSWindowOrderingMode,
};
use objc2_foundation::{NSNumber, NSObjectNSKeyValueCoding, NSPoint, NSRect, NSSize, ns_string};

use super::window_glass::{GlassBackground, GlassEdge, GlassLayout, GlassRegion, GlassStyle};

struct Host {
    container: Retained<NSGlassEffectContainerView>,
    canvas: Retained<NSView>,
    panes: HashMap<String, Retained<NSGlassEffectView>>,
    /// 整窗玻璃；窗口底是实色时没有。
    backdrop: Option<Retained<NSGlassEffectView>>,
}

thread_local! {
    // 只在主线程访问（with_webview 的回调在主线程上执行）。
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

pub(super) fn supported() -> bool {
    AnyClass::get(c"NSGlassEffectView").is_some()
        && AnyClass::get(c"NSGlassEffectContainerView").is_some()
}

pub(super) fn apply(webview: *mut c_void, layout: &GlassLayout) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    // SAFETY: with_webview 在主线程上交出存活的 WKWebView，它是 NSView 的子类。
    let webview: &NSView = unsafe { &*webview.cast::<NSView>() };
    // SAFETY: 主线程上读取视图层级。
    let parent = unsafe { webview.superview() };
    let (Some(parent), Some(window)) = (parent, webview.window()) else { return };

    HOST.with(|cell| {
        let mut slot = cell.borrow_mut();
        let host = slot.get_or_insert_with(|| install(mtm, &parent, webview));
        // 不设置窗口外观：网页的 prefers-color-scheme 跟随它，颜色主题又跟随系统外观；
        // 钉死外观会让系统切换深浅色时主题不再跟着变。玻璃随窗口外观自动换深浅。
        match layout.background {
            GlassBackground::Solid(color) => {
                if let Some(backdrop) = host.backdrop.take() {
                    backdrop.removeFromSuperview();
                }
                window.setOpaque(true);
                window.setBackgroundColor(Some(&srgb(color)));
            }
            GlassBackground::Glass { radius } => {
                if host.backdrop.is_none() {
                    let backdrop = NSGlassEffectView::initWithFrame(mtm.alloc(), parent.bounds());
                    backdrop.setAutoresizingMask(fill());
                    parent.addSubview_positioned_relativeTo(&backdrop, NSWindowOrderingMode::Below, Some(&host.container));
                    host.backdrop = Some(backdrop);
                }
                if let Some(backdrop) = &host.backdrop {
                    backdrop.setCornerRadius(radius);
                }
                window.setOpaque(false);
                window.setBackgroundColor(Some(&NSColor::clearColor()));
                // 不透明区域变了，窗口阴影要按新的形状重算。
                window.invalidateShadow();
            }
        }
        sync(mtm, host, &layout.regions);
    });
}

fn install(mtm: MainThreadMarker, parent: &NSView, webview: &NSView) -> Host {
    // drawsBackground 是 WKWebView 的私有 KVC 键，wry 的 transparent 也用它；
    // 关掉后网页透明的外壳区域才能透出下面的玻璃与窗口底色。
    let no = NSNumber::numberWithBool(false);
    // SAFETY: 值是 NSNumber，与该键期望的 BOOL 包装一致。
    unsafe { webview.setValue_forKey(Some(&no), ns_string!("drawsBackground")) };

    let bounds = parent.bounds();
    let container = NSGlassEffectContainerView::initWithFrame(mtm.alloc(), bounds);
    container.setAutoresizingMask(fill());
    let canvas = NSView::initWithFrame(mtm.alloc(), bounds);
    canvas.setAutoresizingMask(fill());
    container.setContentView(Some(&canvas));
    parent.addSubview_positioned_relativeTo(&container, NSWindowOrderingMode::Below, Some(webview));
    Host { container, canvas, panes: HashMap::new(), backdrop: None }
}

fn fill() -> NSAutoresizingMaskOptions {
    NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable
}

fn sync(mtm: MainThreadMarker, host: &mut Host, regions: &[GlassRegion]) {
    let Host { canvas, panes, .. } = host;
    panes.retain(|id, pane| {
        let keep = regions.iter().any(|region| &region.id == id);
        if !keep {
            pane.removeFromSuperview();
        }
        keep
    });
    // canvas 不翻转（原点在左下角），网页坐标的原点在左上角。
    let height = canvas.bounds().size.height;
    for region in regions {
        let pane = panes.entry(region.id.clone()).or_insert_with(|| {
            let pane = NSGlassEffectView::initWithFrame(mtm.alloc(), NSRect::ZERO);
            canvas.addSubview(&pane);
            pane
        });
        pane.setFrame(NSRect::new(
            NSPoint::new(region.x, height - region.y - region.height),
            NSSize::new(region.width, region.height),
        ));
        pane.setAutoresizingMask(follow(region));
        pane.setCornerRadius(region.radius);
        pane.setStyle(match region.style {
            GlassStyle::Regular => NSGlassEffectViewStyle::Regular,
            GlassStyle::Clear => NSGlassEffectViewStyle::Clear,
        });
    }
}

/// 窗口缩放时先按贴边规则跟随，前端测得新矩形后再精确对齐。
fn follow(region: &GlassRegion) -> NSAutoresizingMaskOptions {
    let horizontal = match region.horizontal {
        GlassEdge::Start => NSAutoresizingMaskOptions::ViewMaxXMargin,
        GlassEdge::End => NSAutoresizingMaskOptions::ViewMinXMargin,
        GlassEdge::Stretch => NSAutoresizingMaskOptions::ViewWidthSizable,
    };
    // 不翻转的坐标里，贴顶意味着下边距可变。
    let vertical = match region.vertical {
        GlassEdge::Start => NSAutoresizingMaskOptions::ViewMinYMargin,
        GlassEdge::End => NSAutoresizingMaskOptions::ViewMaxYMargin,
        GlassEdge::Stretch => NSAutoresizingMaskOptions::ViewHeightSizable,
    };
    horizontal | vertical
}

fn srgb([red, green, blue]: [u8; 3]) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(
        f64::from(red) / 255.0,
        f64::from(green) / 255.0,
        f64::from(blue) / 255.0,
        1.0,
    )
}
