# M7 玻璃镜片与窗口材质：事实核对

核对日期：2026-11-01。本文只记录事实与由事实得出的推论，不代替产品决定；现有决定是 [docs/README.md](../../README.md) 的 Q200（macOS 26+ 原生 Liquid Glass，Windows 11 Mica）。资料核对只读；“本机实测”一节用独立的小程序实验，没有改动 Revia 代码，也没有运行 Revia。

本机环境：macOS 27.0.1（26A434），Safari 27.0.1 / 系统 WebKit `22625.1.29.11.28`，Xcode SDK `MacOSX27.0.sdk`；`Cargo.lock` 中 tauri 2.12.4、tauri-utils 2.10.2、window-vibrancy 0.8.1、wry 0.57.0。

文中标记：**文档写明**＝一手来源原文；**推论**＝由一手来源推出、未实测；**需实测**＝只能在本机或 Windows 真机上确认。

## 参考图效果拆解

用户在对话中给了 6 张截图，没有存进仓库：两张浅色主题的整窗截图；两张侧栏放大图，镜片正经过“聚焦”“待办”；两张分段控件放大图（“已取消 / 全部”“未结束 / 待处理 / 进行中”）。截图里能看到：

- 静止时，选中项只是一个白色圆角底，文字清楚。
- 镜片经过时，镜片下的文字被放大、在边缘弯折，边缘有红蓝色散；镜片上下边缘还能看到被镜像翻转的字形碎片。
- 两张整窗截图的底色不同，一张偏绿白、一张偏橘粉，界面内容一样。

**推论**：镜像碎片与 Chromium 处理背景滤镜边缘的方式一致（裁到层边界、mirror 方式补边，见 Chromium 一节），参考应用多半是在 Chromium 内核上用 `backdrop-filter: url()` 做的镜片。底色不同可能是背后的窗口变了，也可能是换了壁纸，单凭截图不能确定。

每层所需的能力不同：

| 层 | 效果 | 需要的能力 |
| --- | --- | --- |
| 窗口底 | 整窗透出强模糊、带暖色调的桌面或背后窗口 | 系统窗口材质（macOS 玻璃 / Windows Acrylic 或 Mica）。网页本身取不到窗口外的像素，见 Chromium 一节的 Backdrop Root |
| 卡片、浮动侧栏 | 白色半透明、圆角、边缘亮边，静止 | 只需在材质上叠半透明底和描边，不必读取下方像素；CSS 即可 |
| 选中项 / 分段控件的移动镜片 | 切换时镜片下文字被放大、在边缘弯折、边缘红蓝色散；静止时只是白色圆角底 | 必须读取镜片下方**已渲染的内容**再做位移：网页里是 `backdrop-filter: url()` 引用 SVG 滤镜，原生里是叠在内容上方的系统玻璃 |

第三层是唯一的难点；前两层 Revia 现有结构已覆盖。

## Chromium / WebView2

**规范允许 `url()`。** Filter Effects Level 2（Editor's Draft，2026-10-02）中 `backdrop-filter` 的取值是 `none | <filter-value-list>`，正文写明列表里第一个“filter function or filter reference”以 Backdrop Root Image 为输入；滤镜函数在 sRGB 色彩空间运算；含 `blur()` 时以 mirror 方式处理边缘。Backdrop Root Image 只包含页面内从最近的 Backdrop Root（根元素、带 filter / opacity<1 / mask / clip-path / backdrop-filter / mix-blend-mode 的元素等）到该元素之间绘制的内容。[规范 §2–3](https://drafts.csswg.org/filter-effects-2/#BackdropFilterProperty)（`drafts.fxtf.org` 已跳转到此地址）。kube.io 文章称“SVG filters as backdrop-filter isn’t part of the CSS spec”，与上述语法不符，以规范为准。

**Chromium 实现了引用滤镜。** 在 Chromium main（`5a78bde`，2026-10-31）中，`PaintLayer::UpdateCompositorFilterOperationsForBackdropFilter` 调用 `FilterEffectBuilder::BuildFilterOperations(..., contains_backdrop=true)`，后者对 `kReference` 构建 SVG 滤镜并 `AppendReferenceFilter`。[paint_layer.cc](https://chromium.googlesource.com/chromium/src/+/5a78bde2656f359031b87b55d7be006dd191ce66/third_party/blink/renderer/core/paint/paint_layer.cc#2603)、[filter_effect_builder.cc](https://chromium.googlesource.com/chromium/src/+/5a78bde2656f359031b87b55d7be006dd191ce66/third_party/blink/renderer/core/paint/filter_effect_builder.cc#378)

**版本。** Chrome Platform Status 记录 `backdrop-filter` 在 Chrome 76 发布 [chromestatus 5679432723333120](https://chromestatus.com/feature/5679432723333120)；Chrome 129 改为 mirror 边缘 [chromestatus 5382638738341888](https://chromestatus.com/feature/5382638738341888)。MDN BCD 只记 Chrome 76、Edge 79，没有 `url()` 子特性，不能据它区分 [BCD backdrop-filter.json @0efedff](https://github.com/mdn/browser-compat-data/blob/0efedff8c8b6f131ad029443056bc74e00b1aca2/css/properties/backdrop-filter.json)。WPT 的 `backdrop-filter-reference-filter.html`（SVG 引用滤镜）由 Chromium CL [1393943](https://chromium-review.googlesource.com/c/1393943) 于 2019-01-04 加入，早于 76 发布。**推论**：`url()` 引用自 Chrome 76 发布起即可用；没有找到写明“从某版本起”的一手声明。

**当前互通结果。** wpt.fyi 2026-10-31 稳定版运行：Chrome 156（Linux）与 Edge 156（Windows 10）通过 `backdrop-filter-svg.html`、`backdrop-filter-reference-filter.html`、`backdrop-filter-svg-foreignObject.html`；Safari 27.2（macOS 27.2）全部失败；Firefox 159 部分通过。[wpt.fyi 结果](https://wpt.fyi/results/css/filter-effects?label=master&label=stable&q=backdrop-filter)

**限制：**

- **位移图不能取自背景。** Filter Effects 1 §15.2 规定，`feDisplacementMap` 的 `in2`（位移图）若是被标记为 tainted 的输入，滤镜必须原样透传 [规范](https://drafts.csswg.org/filter-effects/#fedisplacemnentmap-restrictions)。Chromium 提交 [4a0ef6b](https://chromium.googlesource.com/chromium/src/+/4a0ef6be08f440935e0a72fdc69806d830559491)（“Disable FEDisplacementMap acting as a backdrop-filter”）把“含背景”也算作这种情况：源码只在 `InputEffect(1)`（即 `in2`）tainted 或含背景时透传 [fe_displacement_map.cc](https://chromium.googlesource.com/chromium/src/+/5a78bde2656f359031b87b55d7be006dd191ce66/third_party/blink/renderer/platform/graphics/filters/fe_displacement_map.cc#107)。该提交首次进入 158.0.8113.0，2026-11-01 时 Beta 为 158.0.8127.2，Stable（156.0.8078.80）尚未包含 [chromiumdash](https://chromiumdash.appspot.com/commit/4a0ef6be08f440935e0a72fdc69806d830559491)。**推论**：kube.io 一类做法（`feImage` 预生成位移图作 `in2`，背景作 `in`）不受影响；用背景本身当位移图的写法从 158 起失效。`feImage` 用同源或 `data:` 图片是否被视为 tainted，在 Tauri 的自定义协议下**需实测**。
- **只能取元素边框盒内的像素。** Chromium 源码注释写明背景滤镜的输入由合成器裁到层边界并以 mirror 处理边缘 [paint_layer.cc](https://chromium.googlesource.com/chromium/src/+/5a78bde2656f359031b87b55d7be006dd191ce66/third_party/blink/renderer/core/paint/paint_layer.cc#2635)；滤镜的参照框是边框盒（`BackdropFilterReferenceBox`）。**推论**：镜片只能把自身范围内的像素向内放大，不能把外面的字“拉进来”；kube.io 也因此只用凸面轮廓。
- **`color-interpolation-filters`。** 规范中它的初始值是 `linearRGB`，且对 `feDisplacementMap` 只作用于 `in2`（位移图）[规范 feDisplacementMap](https://drafts.csswg.org/filter-effects/#feDisplacementMapElement)。**推论**：位移图按 128 为零点编码时，要在滤镜上设 `sRGB`，否则零点会被色彩空间转换偏移；kube.io 的示例也设了 `sRGB`。
- **滤镜区域与精度。** 滤镜区域默认 `x/y=-10%`、`width/height=120%`；规范说明 `feDisplacementMap` 可能需要与 `scale` 成正比的缓冲 [规范 §8、feDisplacementMap](https://drafts.csswg.org/filter-effects/#FilterEffectsRegion)。kube.io 写明位移图每通道 8 位（−128…127 像素），且 backdrop 滤镜尺寸不会随元素自动调整，改尺寸要重建位移图，只动画 `scale` 则不必 [kube.io](https://kube.io/blog/liquid-glass-css-svg/)。
- **色散。** 规范与浏览器没有专门的色散原语；`liquid-glass-react` 的 `aberrationIntensity` 是用标准滤镜原语组合出来的（README 写明 Safari 与 Firefox 只部分支持，位移不可见）[liquid-glass-react](https://github.com/rdev/liquid-glass-react)。**推论**：按通道拆分（`feColorMatrix`）、分别用不同 `scale` 位移、再合成（`feBlend` / `feComposite`）可得到红蓝边。
- **性能。** 规范的处理步骤要求把背景复制到临时缓冲再滤镜；没有找到 Chromium 对引用滤镜背景的一手性能数据。kube.io 自述该做法“实验性”、动态改形状代价高。镜片在大面积 diff 上移动时的帧率**需实测**。

**WebView2。** 微软文档写明 WebView2 Runtime 与 Edge Stable 提供相同的网页平台能力与发布节奏，Evergreen 模式自动接收与 Edge Stable 相同的更新；Fixed Version 模式则固定版本 [WebView2 分发](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)。Revia 未设置 `webviewInstallMode`，tauri-utils 2.10.2 的默认值是 `DownloadBootstrapper`（安装 Evergreen）。**推论**：WebView2 上 `backdrop-filter: url()` 的可用性与限制跟随同版本 Chromium / Edge，Edge 156 的 WPT 结果可作参照；M158 的位移图限制会随 Evergreen 自动到达。

**推论**：按 Backdrop Root 的定义，网页里的镜片只能折射网页自己画的内容；Mica / Acrylic 由 DWM 画在窗口下方，透明的页面区域在镜片下不会出现被折射的桌面。

## WebKit / WKWebView

**不支持 `backdrop-filter: url()`，截至本机 Safari 27.0.1 与 WebKit trunk（2026-10-31）。**

- [WebKit Bug 245510](https://bugs.webkit.org/show_bug.cgi?id=245510)“backdrop-filter: url(#some-svg-filter) doesn't work with SVG filters like feDisplacementMap”：2022-09-22 提交，状态 NEW，带 InRadar；评论确认 Chromium 生效、Safari 无效，`feColorMatrix` 作背景滤镜同样无效。2026-07-16 有外部贡献者提交 [PR 68614](https://github.com/WebKit/WebKit/pull/68614)（在 RenderLayer 内容绘制中加软件路径）、[PR 68613](https://github.com/WebKit/WebKit/pull/68613)（加速滤镜路径的前置修复）和 [PR 69566](https://github.com/WebKit/WebKit/pull/69566)（WPT）；2026-09-06 的评论称在 trunk 上该测试页会反复让 GPU 进程崩溃。经 GitHub API 核对，三个 PR 在 2026-11-01 均为 open、未合并。
- WebKit trunk `965b213` 源码：`RenderLayerBacking::updateBackdropFilters` 在 `hasReferenceFilter()` 为真时不把背景滤镜交给图层，`m_canCompositeBackdropFilters` 为 false [RenderLayerBacking.cpp L999–1002](https://github.com/WebKit/WebKit/blob/965b21346c31b0110663e2f01cad0a5d2b9a2aa1/Source/WebCore/rendering/RenderLayerBacking.cpp#L999-L1002)；`GraphicsLayerCA::setBackdropFilters` 不能合成时直接清除，注释写着“This would clear the backdrop filters if we had a software implementation”[GraphicsLayerCA.cpp L963–980](https://github.com/WebKit/WebKit/blob/965b21346c31b0110663e2f01cad0a5d2b9a2aa1/Source/WebCore/platform/graphics/ca/GraphicsLayerCA.cpp#L963-L980)。**推论**：只要列表里含 `url()`，同一条声明里的 `blur()` 等也一起失效；降级要用另一条不含 `url()` 的规则。`@supports` 能否区分**需实测**。
- Safari 26.0–27.2 发布说明中没有“支持 backdrop-filter url()”的条目；26.4 修了“CSS reference filters using `<feDisplacementMap>`”的分块缝隙、27.0 修了 `url(#id)` 滤镜内容变化不失效，这两条针对的是普通 `filter` [26.4](https://developer.apple.com/documentation/safari-release-notes/safari-26_4-release-notes)、[27.0](https://developer.apple.com/documentation/safari-release-notes/safari-27-release-notes)。wpt.fyi 上 Safari 27.2 的相关测试全部失败（见上一节）。MDN BCD 记录 Safari 18 起无前缀、9 起 `-webkit-` 前缀，同样没有 `url()` 子特性。

**相关的另两条事实：**

- Safari 26.0 发布说明的 WebKit API 一节：“Added support for applying `backdrop-filter` to content behind a transparent webview.”[Safari 26.0](https://developer.apple.com/documentation/safari-release-notes/safari-26-release-notes)。trunk 源码注释对应：主框架的 RenderView 不再作为 Backdrop Root，“so that clients can intentionally allow backdrop blending outside of the webview by setting it to transparent”[RenderLayerBacking.cpp L1042–1060](https://github.com/WebKit/WebKit/blob/965b21346c31b0110663e2f01cad0a5d2b9a2aa1/Source/WebCore/rendering/RenderLayerBacking.cpp#L1042-L1060)。Revia 的 webview 已关掉 `drawsBackground`。**需实测**：网页里的 `backdrop-filter: blur()` 能否模糊 webview 下方的原生玻璃、能否取到窗口外的桌面。这与 liquid-glass.md 中“CSS 只能模糊网页自己的内容”的表述可能不一致，只能作参考，不支持 `url()`。
- WebKit 有非标准属性 `-apple-visual-effect`，取值含 `-apple-system-glass-material`、`-apple-system-glass-material-clear`，受 `useSystemAppearance` 设置控制 [CSSProperties.json](https://github.com/WebKit/WebKit/blob/965b21346c31b0110663e2f01cad0a5d2b9a2aa1/Source/WebCore/css/CSSProperties.json)、[AppleVisualEffect.h](https://github.com/WebKit/WebKit/blob/965b21346c31b0110663e2f01cad0a5d2b9a2aa1/Source/WebCore/platform/cocoa/AppleVisualEffect.h)；macOS 上玻璃取值要求 `HAVE_MATERIAL_HOSTING`（macOS 26+）[PlatformHave.h L1735–1738](https://github.com/WebKit/WebKit/blob/965b21346c31b0110663e2f01cad0a5d2b9a2aa1/Source/WTF/wtf/PlatformHave.h#L1735-L1738)，开关在私有接口 `-[WKWebView _setUseSystemAppearance:]` [WKWebViewPrivate.h L326](https://github.com/WebKit/WebKit/blob/965b21346c31b0110663e2f01cad0a5d2b9a2aa1/Source/WebKit/UIProcess/API/Cocoa/WKWebViewPrivate.h#L326)。这不是公开 API，能否透镜、能否折射网页内容都未知。

## macOS 原生玻璃（NSGlassEffectView）

**接口（Xcode 27.0 SDK 头文件与 Apple 文档一致）：**

- `NSGlassEffectView`：“A view that embeds its content view in a dynamic glass effect.”macOS 26.0+。`contentView` 的说明：只保证 `contentView` 放在玻璃效果内，其他任意子视图相对内容和玻璃的层级没有保证 [contentView](https://developer.apple.com/documentation/appkit/nsglasseffectview/contentview)。
- `cornerRadius`；`tintColor`：玻璃把背景和玻璃效果染向的颜色 [tintColor](https://developer.apple.com/documentation/appkit/nsglasseffectview/tintcolor)；`style`：`Regular` / `Clear`，头文件只写“Standard / Clear glass effect style”。
- `effectIsInteractive`：**macOS 27.0+**，开启后玻璃对交互给出视觉反馈，默认 NO，适用于交互控件的背景或容器 [effectIsInteractive](https://developer.apple.com/documentation/appkit/nsglasseffectview/effectisinteractive)。对照 CommandLineTools 的 `MacOSX26.5.sdk`，26.5 头文件没有这个属性，27.0 头文件中它是唯一新增。SwiftUI 的 `Glass.interactive(_:)` 在 macOS 26.0 就有（SwiftUICore swiftinterface），文档称其提供与标准按钮相同的触摸与指针反应 [Applying Liquid Glass to custom views](https://developer.apple.com/documentation/swiftui/applying-liquid-glass-to-custom-views)。`objc2-app-kit 0.3.2` 没有该属性的绑定；window-vibrancy 0.8.1 自行调用，并注明 macOS 26 上忽略。
- `NSGlassEffectContainerView`：把距离在 `spacing` 内、足够相似的后代玻璃合并；把 `contentView` 的后代提到 `contentView` 之上；批量处理以减少渲染次数。`spacing` 默认 0，只做批处理、不产生合并变形 [容器](https://developer.apple.com/documentation/appkit/nsglasseffectcontainerview)、[spacing](https://developer.apple.com/documentation/appkit/nsglasseffectcontainerview/spacing)。

**它折射什么。** WWDC25《Build an AppKit app with the new design》写明：玻璃从比自身更大的区域采样附近内容来反射、折射；玻璃不能直接采样另一块玻璃，因此相邻玻璃要放进同一个容器共享采样区；侧栏玻璃浮在窗口内容之上，内容可以延伸到侧栏下方；用 `contentView` 放内容，AppKit 才能施加保持可读性的处理，“avoid placing the NSGlassEffectView behind your content as a sibling view”[WWDC25 310](https://developer.apple.com/videos/play/wwdc2025/310/)。HIG 写明 Regular 会模糊并调整背景亮度，Clear 高度透明、需要时加暗化层；不要在内容层使用玻璃，避免玻璃叠玻璃 [HIG Materials](https://developer.apple.com/design/human-interface-guidelines/materials)。Apple 文档没有说明玻璃是否采样窗口外的桌面；Revia 在透明窗口下已实测透出桌面与其他窗口（[liquid-glass.md](liquid-glass.md)“原生 Liquid Glass”一节）。

**透镜与交互。** WWDC25《Meet Liquid Glass》：玻璃主要通过 lensing（光的弯折）界定自身；出现和消失时逐渐调制弯折而不是淡入淡出；变大时折射与透镜更明显；交互时从内部发光；控件可以在交互时临时“升起”成玻璃，透过透镜观察下方的值（如滑块）[WWDC25 219](https://developer.apple.com/videos/play/wwdc2025/219/)。文档没有提到色散（红蓝边），参考图第三层在 macOS 玻璃上是否出现**需实测**。

**放在 WKWebView 上方（同一 superview、排在 webview 之后）：**

- **推论**：Apple 的设计模型就是玻璃浮在内容上方并采样下方内容；WKWebView 在 macOS 26 新增 `obscuredContentInsets`，用于“被标签栏、工具栏等浏览器 UI 遮住的区域”[obscuredContentInsets](https://developer.apple.com/documentation/webkit/wkwebview/obscuredcontentinsets)，说明原生 UI 叠在 WKWebView 上是预期用法。但 WKWebView 的内容由独立进程渲染，玻璃能否采样并折射这些内容、文字是放大还是被模糊（Regular 与 Clear 的差别），文档没有写，**需实测**。
- Revia 的整窗玻璃在 webview 下方、容器之外。按“玻璃不能采样玻璃”，上方镜片下若是网页透明区域，采到的是下方玻璃，结果可能不一致，**需实测**。
- 鼠标穿透：`hitTest(_:)` 文档写明 NSWindow 主要用它决定哪个视图接收 mouse-down，可以覆盖它来对子视图隐藏事件 [hitTest](https://developer.apple.com/documentation/appkit/nsview/hittest(_:))；头文件没有禁止子类化 `NSGlassEffectView`。**推论**：子类返回 `nil` 时点击与滚轮落到下方 webview；悬停、光标与 tracking area **需实测**；这样做后 `effectIsInteractive` 的反馈大概率不会被触发。

## Windows Acrylic 与 Mica

**微软的定义：**

- Mica：不透明，结合主题与**桌面壁纸**，只采样一次壁纸以保证性能；用于长期存在的窗口底层；窗口失活时回到中性色。Mica Alt 是加深着色的变体 [Mica](https://learn.microsoft.com/en-us/windows/apps/design/style/mica)。
- Acrylic：半透明、磨砂玻璃。Background acrylic“reveals the desktop wallpaper and other windows that are behind the currently active app”；In-app acrylic 只在应用内产生层次。Windows 11 中 Acrylic 只用于浮出层、右键菜单等临时表面；明确写“Don't put desktop acrylic on large background surfaces of your app”；渲染耗 GPU，省电模式、关闭透明效果、低端硬件时变实色，窗口失活时 background acrylic 变实色 [Acrylic](https://learn.microsoft.com/en-us/windows/apps/design/style/acrylic)、[Materials](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/materials)。
- `DWMWA_SYSTEMBACKDROP_TYPE` 从 Windows 11 Build 22621 起支持：`DWMSBT_MAINWINDOW`＝Mica，`DWMSBT_TRANSIENTWINDOW`＝Desktop Acrylic（最亮变体），`DWMSBT_TABBEDWINDOW`＝Mica Alt [DWM_SYSTEMBACKDROP_TYPE](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type)、[DWMWINDOWATTRIBUTE](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute)。WinUI 的 `SystemBackdropElement`（Windows App SDK 2.0+）可以把材质只放在某个区域 [system backdrops](https://learn.microsoft.com/en-us/windows/apps/develop/ui/system-backdrops)，Tauri 没有对应接口。Windows 10 没有上述 DWM 材质的公开接口。

**Tauri 2 与 window-vibrancy（本地 crate 源码）：**

- tauri-utils 2.10.2 的 `WindowEffect` 中 Windows 可用值：`Mica`、`MicaDark`、`MicaLight`、`Tabbed`、`TabbedDark`、`TabbedLight`（均“Windows 11 Only”），`Blur`（Windows 7/10/11 22H1，注明 build 22621 上缩放、拖动性能差），`Acrylic`（Windows 10/11，注明 Windows 10 v1903+ 与 Windows 11 build 22000 上缩放、拖动性能差）；macOS 另有 `LiquidGlassRegular` / `LiquidGlassClear`（26.0+）[tauri-utils lib.rs](https://docs.rs/crate/tauri-utils/2.10.2/source/src/lib.rs)。`set_effects` 的文档注释写明要求窗口透明 [tauri 2.12.4 window/mod.rs](https://docs.rs/crate/tauri/2.12.4/source/src/window/mod.rs)，Tauri 把 `Effect::Acrylic`、`Effect::Mica` 等直接转给 window-vibrancy [vibrancy/windows.rs](https://docs.rs/crate/tauri/2.12.4/source/src/vibrancy/windows.rs)。window-vibrancy 0.8.1 README 的函数表给出同样的已知问题 [README](https://docs.rs/crate/window-vibrancy/0.8.1/source/README.md)。
- window-vibrancy 0.8.1 `windows.rs`：`apply_acrylic` 在 build ≥ 22523 用 `DWMSBT_TRANSIENTWINDOW`，否则在 build ≥ 17763（Windows 10 1809）退回未公开的 `SetWindowCompositionAttribute`（`ACCENT_ENABLE_ACRYLICBLURBEHIND`）；`apply_mica` 在 ≥ 22523 用 `DWMSBT_MAINWINDOW`，22000–22522 用未公开属性 1029 [windows.rs](https://docs.rs/crate/window-vibrancy/0.8.1/source/src/windows.rs)。**推论**：已知性能问题标注的版本对应未公开接口这条回退路径；22621+ 走公开接口时没有官方性能说明，拖动、缩放是否流畅**需实测**。
- Electron 的 `win.setBackgroundMaterial` 取值 `auto/none/mica/acrylic/tabbed`，仅支持 Windows 11 22H2+，并指向同一份 DWM 文档 [Electron BrowserWindow](https://github.com/electron/electron/blob/main/docs/api/browser-window.md#winsetbackgroundmaterialmaterial-windows)。参考应用若是 Electron，多半用的是这一接口。

**参考图更像哪种。** 按上面的官方描述：能透出**背后其他窗口**的只有 background acrylic；Mica 只取壁纸，且不透明。用户描述“透出桌面 / 背后窗口”，若截图里确有背后窗口的轮廓，或移动其他窗口时底色跟着变，就是 Acrylic（`DWMSBT_TRANSIENTWINDOW`）；若只有壁纸色、不随其他窗口变化、失活时变中性色，就是 Mica。两者都有强模糊和着色，只看静态色调不能区分；原图未纳入本次核对。

## 本机实测（2026-11-01）

环境：macOS 27.0.1（26A434），SDK 27.0，WebKit.framework 22625.1.29.11.28。用独立的 Swift 小程序（AppKit 窗口 + 默认配置的 WKWebView）实验。源码、stdout、截图和命令在 `/private/tmp/revia-glass-lens-261ZlICR/`（见其中 `README.txt`）。

| 做法 | 结果 |
| --- | --- |
| WKWebView 里写 `backdrop-filter: url(#svg)`（`feTurbulence` + `feDisplacementMap`） | **没有效果**：镜片下的文字原样不动。`CSS.supports` 的两种写法都返回 true，计算样式也是 `url("#warp")`。WebKit 能解析但不渲染，特性检测判断不出来 |
| 同一条声明写 `url(#svg) blur(6px) saturate(1.4)` | **整条失效**，连模糊也没有，与上文 WebKit 源码的推论一致 |
| 单独写 `backdrop-filter: blur()` | 有效 |
| 元素自身的 `filter: url(#svg)` | 有效，位移明显 |
| **网页自绘镜片**：镜片里放一份同样的列表副本；副本加 `filter: url()`，用边缘位移图，RGB 三个通道用不同的位移量，并放大 4%；镜片和副本反向同步平移 | **能渲染**：镜片下的内容在边缘弯折。弯折图、色散强度和底色还要设计调校，实验截图里的效果仍很粗糙 |
| **原生 `NSGlassEffectView` 叠在 WKWebView 上方**（同一个 superview，排在 webview 之后） | **能折射网页内容**：白底上，Clear 样式在边缘弯折下面的文字，上下两行的文字被拉进镜片边缘；Regular 样式是磨砂。没有看到红蓝色散 |
| 原生镜片子类的 `hitTest` 返回 nil | 镜片中心的 `hitTest` 落在 WKWebView 上，点击能穿透。悬停和光标没有测 |
| 原生镜片用 `NSAnimationContext` 移动，同时网页镜片用 CSS transition 移动 | 两块镜片同时到达终点。没截到中途帧（`screencapture` 本身就要耗时），帧率没有测 |
| 原生 Clear 玻璃放在浅色渐变底加半透明面板上 | **外观会随背后内容变化**：约 1 秒时接近不透明的白色，约 4 秒时变成灰色磨砂，看不见下面的字，和白底上透亮的镜片完全不同。系统按背后内容自动调整，网页一侧控制不了折射强度、放大倍数和色散 |
| 默认 WKWebView 的 `navigator.userAgent` | `Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)`，**不含 `Safari`**。wry 0.57 只在配置了 `user_agent` 时才改写 UA（`wkwebview/mod.rs` 的 `set_user_agent`），Revia 没有配置。**推论**：Revia 在 macOS 上的 UA 与此相同 |

没有测：原生镜片叠在 Revia 现有的整窗玻璃上（Revia 的真实结构），以及 Safari 26 起“透明 webview 的 backdrop-filter 能作用到 webview 下方内容”在 Revia 里的效果。

### 实现时补充的实测（2026-11-01）

Q202 选了网页自绘镜片（实现见 [liquid-glass.md](liquid-glass.md) 的“玻璃镜片与布局块”）。实现过程中在 WKWebView 里又测出三条，原件同在 `/private/tmp/revia-glass-lens-261ZlICR/`（LensProbe5–9），以及会话临时目录里的连拍截图：

- 元素自身的 `filter: url()` 作用不到已经提升成独立合成层的后代。例如后代正在跑 transform 的 WAAPI 动画，它照常画出来，但不经过滤镜。把这个后代改成用 `left` / `top` 动画后，滤镜生效。
- 快照的底是透明的。这时按 R、G、B 拆开、分别位移再用 `screen` 合成，深色文字出不了彩边（三个通道都是黑的）。改为在原图下面垫红、蓝两道染色的位移影后，深色、浅色文字都出现彩边。
- `feColorMatrix` 的 `values` 和 `feDisplacementMap` 的 `scale` 在首次绘制之后改动，WebKit 会重画。自定义协议（`tauri://`）页面里，`feImage` 引用 `data:` 位移图同样生效，不会被当作跨源而跳过位移。

## 与已接入的 React Bits 组件的关系

Q199 接入的三个组件在 `src/components/m7/glass/`（来源与许可见 [第四版来源说明](./mockups/react-bits/)）：

- **Glass Surface**：它的 SVG 路径正是参考图镜片的做法：用 `feImage` 生成位移图，红、绿、蓝三个通道各做一次 `feDisplacementMap`（`redOffset` / `greenOffset` / `blueOffset`），再用 `feBlend` 合成，最后挂在 `backdrop-filter: url()` 上（`GlassSurface.tsx` 第 829–871 行、`GlassSurface.css` 的 `.glass-surface--svg`）。所以在 WebView2 上可以直接用来做镜片，在 WKWebView 上不行。它的 WebGL 路径只接受 `img`、`video`、`canvas` 作背景，折射不了普通 DOM（见 [liquid-glass.md](liquid-glass.md) 的“React Bits 核对”）。
- **Glass Surface 在 macOS 上的降级判断不可靠**：`supportsSVGFilters()` 只在 UA 含 `Safari` 且不含 `Chrome` 时判定为 WebKit 并降级为磨砂，否则只要 `style.backdropFilter = url(...)` 被接受就走 SVG 路径。默认 WKWebView 的 UA 不含 `Safari`，`url()` 写法又能通过解析（见上表）。**推论**：Revia 在 macOS 走 CSS 材质时（`REVIA_GLASS=css`，或 macOS 26 以前的系统），`GlassLayer` 的 `refraction` 为真时会选中 SVG 路径，`.glass-surface--svg` 那条 `backdrop-filter` 整条失效，面板与浮层没有模糊。`GlassLayer.tsx` 的注释“WKWebView 自动降级为磨砂”与此不符。需要在 Revia 里实际打开 CSS 路径确认。
- **Specular Button**：鼠标跟随的高光，不读取下方像素，两端都能用；与镜片无关。原生玻璃模式下 `GlassButton` 目前不渲染它。
- **Glass Icons**：图标入口的玻璃方块，与镜片无关。

### 用户提供的掘金文章与 Fluid Glass

用户提供了掘金文章[《苹果的液态玻璃咋做？》](https://juejin.cn/post/7678529068804554787)（2026-08-27）。GrokSearch 读取返回空内容，改用 curl 取得页面 HTML 读正文。文章比较了三种做法：

- [liquid-glass-studio](https://github.com/iyinchao/liquid-glass-studio)：多 pass 的 WebGL 着色器。文章称它只能渲染图片，处理不了 HTML。GitHub 仓库信息：MIT 许可，说明为 WebGL2 与 WebGPU 实现。
- Glass Surface 的 SVG 版：文章称它“平滑度不够，会出现毛边和颜色异常”。**推论**：这与 `feDisplacementMap` 位移图每通道只有 8 位精度有关（见 Chromium 一节的 kube.io 说明）；本机实验的网页镜片边缘也比较粗糙。
- 作者认为最好的是 React Bits 的 Fluid Glass，并逐段讲解了源码：three.js + `@react-three/fiber` + `@react-three/drei`。要折射的文字和图片放进一个独立的 three.js 场景，每帧先画到离屏缓冲，再由 `MeshTransmissionMaterial` 按折射率、厚度、色散采样。

核对 Revia 固定的 React Bits 提交 `e12c221` 后发现，文章里的代码不是现在这一版。现在的 [`FluidGlass.tsx`](https://github.com/DavidHDev/react-bits/blob/e12c22192ac7b4b452c5e2feb9b586d74765d696/src/ts-default/Components/FluidGlass/FluidGlass.tsx) 共 1525 行，没有引入 three.js，直接用 WebGL2 着色器。它能折射的内容只有两种来源：

- `text`：用 Canvas 2D 的 `fillText` 画成纹理；
- `src`：一张图片或一段视频。

不论哪一版，都只能折射组件自己画出来的东西，折射不了页面上的 DOM。要折射 Revia 的侧栏，得把标签和图标另外画进纹理。

据此，**WebGL 着色器镜片**是网页自绘之外的第三条路：

- 优点：WKWebView 和 WebView2 都支持 WebGL2；边缘平滑，放大和色散都能按物理效果调。
- 代价：镜片下的内容要在画布里重画一遍，字体、字重、图标要和 DOM 对齐，否则镜片进出时能看出两份不一样；每块镜片一个 WebGL 上下文，带一个逐帧循环。
- 不建议引入文章那一版的 three.js 全家桶，只为一个小镜片不值得（**推论**）。

参考图的移动镜片在 React Bits 这几个组件里没有现成实现，要另写。可以复用 Glass Surface 的位移图生成和三通道色散滤镜；在 macOS 上把滤镜挂到“内容副本”的 `filter` 上，而不是 `backdrop-filter`。

## 对 Revia 的含义

### macOS

- **能直接做**：卡片和浮动侧栏的白色半透明底与亮边（在现有原生玻璃上用 CSS 画，或沿用按区域放的原生玻璃）；选中项静止时的白色圆角底（CSS）；整窗材质（Q200 已实现）。
- **镜片有两条路，均已实测能出效果：**
  - **网页自绘**：镜片里放一份下方内容的副本，副本用 `filter: url()` 做边缘弯折、三通道色散和放大。外观完全可控，包括参考图的色散，同一份代码在 WebView2 上也能用。代价：只适合内容固定、可以复制的小控件（侧栏导航、分段控件、标签栏）；副本要和原内容同步，滚动列表或 diff 正文不适合（**推论**）。
  - **原生玻璃叠在 webview 上方**：系统真实的透镜，点击可穿透。代价：只有 macOS；外观由系统按背后内容调整（实测在浅色底上变成白色或灰色磨砂）；没有色散；静止时必须撤掉，否则盖住选中项的文字；动画要原生和网页两边同步。
- **需实测**：原生镜片叠在 Revia 整窗玻璃上的效果、悬停与光标、帧率；透明 webview 的 `backdrop-filter` 能否取到下方原生玻璃；macOS 27 上 `effectIsInteractive` 的反馈。
- **做不到或代价大**：
  - WKWebView 里的 `backdrop-filter: url()` 镜片：Safari 27.0.1、27.2 与 trunk 都不支持，相关 PR 未合并；合并后也只是软件路径。
  - `-apple-visual-effect` 是私有设置。
  - Apple 建议不要把玻璃作为兄弟视图垫在内容下面，Revia 现在正是这样做的，AppKit 对内容的可读性处理用不上（事实，不改变 Q200）。

### Windows

- **能直接做**：
  - 整窗材质：现有 `Effect::Mica`；需要透出背后窗口时换成 `Effect::Acrylic`。22621+ 走公开的 `DWMSBT_TRANSIENTWINDOW`，但微软不建议大面积背景使用 desktop acrylic。
  - 网页镜片：WebView2 上 `backdrop-filter: url()` 可用。位移图用 `feImage` 作 `in2`、背景作 `in`，设 `color-interpolation-filters="sRGB"`；色散用通道拆分组合；静止时去掉滤镜或把 `scale` 动画到 0。
- **需实测**：
  - Acrylic / Mica 在透明 WebView2 窗口拖动、缩放时是否卡顿或闪烁。
  - 镜片在大 diff 上的帧率。
  - `feImage` 位移图在 Tauri 自定义协议下是否被视为 tainted。
  - Chromium 158 的限制确实不影响上述写法。
- **做不到或代价大**：
  - 镜片折射 Mica / Acrylic 或桌面：CSS 只看得到页面。
  - Windows 10 的 Acrylic 只能走未公开接口，且有已知性能问题。
  - Mica 不能透出背后窗口。
- **跨平台**：网页自绘镜片（内容副本加 `filter: url()`）两端都能渲染，可以只写一份；Windows 上也可以改用 `backdrop-filter: url()`，省掉副本。

## 来源与读取失败记录

已读取的一手来源见正文链接。下列读取失败或改用替代来源，未当作已核实：

- GrokSearch `web_fetch` 读取 `https://drafts.fxtf.org/filter-effects-2/` 失败（Tavily 返回空内容）；改用 curl 读取它跳转到的 `drafts.csswg.org`。
- GrokSearch `web_search` 50 秒超时；改用内置搜索，只作线索（它指向 WebKit Bug 245510），结论都回到原页核对。
- `bugs.webkit.org` 网页经 GrokSearch 只返回附件表；改用 Bugzilla REST 接口，评论接口首次 503，重试成功。
- Chromium gitiles 的 `+log` 需要登录（403）；改用 cr-rev 与 chromiumdash 查提交所属版本。`issues.chromium.org` 的 crbug 983157、540025369、520585 未读取，正文只引用提交说明与源码。
- `raw.githubusercontent.com` 上 BCD 的 `main` 路径被 302 到 jsDelivr；改用固定提交 `0efedff` 的文件。
- apple-docs 工具对 NSGlassEffectView 页面只返回摘要；改读 Apple 文档的 JSON 接口与本机 SDK 头文件。
- 未读取：Fluent 2 Material 页、微软“Apply Mica in Win32”页、WebView2“Evergreen vs fixed”页（已用分发页代替）。

本轮下载的原件（Apple 文档 JSON、Safari 发布说明 JSON、WebKit / Chromium 源码片段、BCD、wpt.fyi 结果、Bugzilla 数据）保存在会话临时目录 `/private/tmp/claude-501/-Volumes-T7-Shield-project-Revia/8c9c05c5-f1c6-4177-a6ec-d6b776f512c9/scratchpad/src/`，会话结束后可能被清理。
