# M7 Liquid Glass：方向与实现核对

更新日期：2026-11-01。前半保留 M7 第 1 步的设计与预览记录；当前产品实现以末节 Q200 为准。原生实现已进入合并验证，不是桌面验收通过报告。

## 用户已确认

- M7 外观改为 Liquid Glass，可以使用 [React Bits](https://github.com/DavidHDev/react-bits)（Q189）。
- 偏强透明、折射与悬浮感，同时代码必须清晰（Q190）。
- 在设计阶段同时考虑 macOS 和 Windows，不把 macOS 外观原样移到 Windows；用户认为第一版“很不协调”，该稿未被采用（Q191）。
- Rebased 的布局和功能基准、现有颜色主题、字体、图标与 Git 安全边界继续沿用；详见 [UI 方向](../../ui-direction.md#视觉风格)。

第一版将玻璃分别放在顶栏、侧栏和浮层，又根据全界面玻璃的提问增加了低透明代码底层对比；这两版都未被认可，保留为历史稿。第二版提案将材质连成一个窗口，以细分隔线、统一尺寸与同色系代码背景表达层次，临时浮层单独悬起；同时显示 macOS 和 Windows 外壳。随后用户回复“大体可以”，要求修正提交区并先查看最终稿。第三版补齐提交区后，用户指出按钮颜色不协调并询问能否全部使用 React Bits。第四版接入实际组件并修正配色；最终候选及组件使用范围已确认（Q199）。

## 实现边界

| 部分 | 设计处理 |
| --- | --- |
| 顶栏、侧栏、按钮、浮层 | 强玻璃背景、边缘高光与悬浮阴影；文字和控件在清晰的独立内容层 |
| 代码与 diff | 稳定底色、语法色、增删底色与行号；第二版保持不透明，背景与周边取同一色系，文字不施加模糊或折射 |
| 窄窗 | 保留代码字号，使用滚动或已选的换行方式；不靠缩小代码换取空间 |
| 交互 | 现有组件继续负责焦点、快捷键、弹层定位、拖动及写前校验；材质层不拦截指针 |
| 效果验证 | macOS WKWebView、Windows WebView2 分别验浅深色、滚动、弹层、缩放及降级状态；仅磨砂不等于强折射已实现 |

## Windows 同步设计

沿 Q60、Q61 的共享设计与平台外壳，第二版 Windows 保留右侧最小化 / 最大化 / 关闭区、原生菜单位置、Ctrl 修饰键和 Windows 路径样例；macOS 保留左侧红绿灯与 ⌘ 提示。两套使用同一套主区域布局、代码字号、信息密度和颜色语义。

2026-10-30 核对 [Microsoft 材质说明](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/materials)：Mica 是随桌面取色的不透明基底，Acrylic 是用于临时浮层的半透明材质。其设计分层可作为 Windows 的参照，但本提案未选择或接入 WinUI、Mica 或 Acrylic API。[Windows 标题栏规范](https://learn.microsoft.com/en-us/windows/apps/design/basics/titlebar-design) 还要求保留拖动、系统菜单、双击最大化等行为，标题被截断时窗口按钮仍完整可见。

官方 [WebView2 说明](https://learn.microsoft.com/en-us/microsoft-edge/webview2/) 确认其使用 Microsoft Edge 渲染引擎。本地 `src/lib/platform.ts` 已让非 macOS 使用原生标题栏并集中处理修饰键；锁定的 wry 具备 Windows WebView2 实现。该事实不等于 React Bits 在真实 Windows 上已验证。网页原件保存在 `/private/tmp/revia-m7-glass-v2-kfb5dh4o/windows-web-sources.json`；本轮没有 GrokSearch 入口，使用可用网页工具读取官方页面。

实施验收应覆盖 100% / 125% / 150% 系统缩放、窄窗、浅深色、Windows 高对比度、透明效果关闭及原生窗口操作。预览只通过 CSS 缩放和浏览器媒体模拟检查布局；在 macOS 渲染 Windows 外观也不会获得真实 Windows 字体、DPI、标题栏行为或 WebView2 性能。

## React Bits 核对

官方 [Glass Surface](https://reactbits.dev/components/glass-surface) 提供 liquid glass 材质。[安装说明](https://reactbits.dev/get-started/installation) 按组件交付源码，可选择 JS / TS 与 CSS / Tailwind 形式，手动复制或通过 shadcn CLI 引入。无需因此替换现有 Base UI 功能控件。

当前 [许可](https://github.com/DavidHDev/react-bits/blob/main/LICENSE.md) 是 **MIT + Commons Clause**，包含作为应用或网站一部分使用和分发的条款，同时限制单独销售、转售或重新分发组件本身；引入时保留原版权和完整许可声明，不标为普通 MIT。前三版预览未复制 React Bits 组件源码；第四版实际接入组件，完整许可随预览保留。

对 2026-10-30 取得的 `main` 源码核对如下；这些是已保存字节的事实，不代表已固定上游 commit：

- [TypeScript + CSS 组件](https://github.com/DavidHDev/react-bits/blob/main/src/ts-default/Components/GlassSurface/GlassSurface.tsx) 有 SVG、WebGL2 和磨砂三条路径。SVG 探测排除匹配 Safari 且不含 Chrome 的 UA 及 Firefox；其余只检查 CSS 属性是否接受，非像素结果验证。
- WebGL2 需要 `backdrop`、开启折射且上下文可用；`backdrop` 仅接受 `img`、`video` 或 `canvas`，不能把普通 DOM 面板自动截成折射纹理。未提供有效源时不能承诺 WebGL 折射。
- 每个 WebGL 实例独立创建上下文和动画循环，实施时应控制材质实例数量，不在每个文件行或 diff 行创建一个。
- 默认尺寸、padding、居中和裁剪不能直接套在 Revia Shell 上；应适配装饰层，保留已有布局和弹层行为。

早期原件在 `/private/tmp/revia-m7-glass-source.8JI4U0/`，manifest 记录 URL、取得时间与 SHA256：

| 原件 | SHA256 |
| --- | --- |
| `ts-default.tsx` | `3cfd551b02c48aab38418a1de724ebba35c276c27bd11dcf032197f5c6d56888` |
| `ts-default.css` | `90cfc3ad54ad7aa92bb2b700696e2d9f2d8704162ba1b94ebf0b6f8dcecc4e61` |
| `ts-tailwind.tsx` | `5475561bc5a62ce9476e7e7b8f4fd4c5b741b6dd1bc17f6f6fe76f021ba2428e` |

根代理先读取了用户提供的仓库 URL，官网、安装与许可页面的工具原件保存在 `/private/tmp/revia-m7-round3-n3ww6iju/m7_react_bits_*.json`。根代理没有 GrokSearch 入口，使用可用网页读取工具；独立源码核对保留了三个版本的原始字节。

第四版追加核对并固定 `e12c22192ac7b4b452c5e2feb9b586d74765d696`，实际使用 Specular Button、Glass Icons、Glass Surface 的 TS + CSS 源码；当前 GlassSurface 字节与上表一致。组件目录、图标传入契约、适配与许可见 [第四版来源说明](./mockups/react-bits/)。这回答了用户对按钮、图标也采用玻璃组件的提议；Q199 已确认优先复用这三个组件，缺少的业务控件沿用交互并统一材质。

## Revia 的实际起点

本轮基点 `4f9517ceeff84938686e17bf976ccd512be5fea8`：`Cargo.lock` 锁定 `wry 0.57.0`，本机 crate 源码的 macOS 实现使用 WKWebView；`src-tauri/tauri.conf.json` 的标题栏 Overlay 没有设置透明窗口或原生玻璃。不能从 Overlay 推断能够透出桌面壁纸。

`src/index.css` 管理共享样式，`src/lib/theme.ts` 设定颜色主题，`Shell.tsx` 与 App 侧栏是外壳接入点。弹层、通知、对话框有现成集中样式。`--editor-background` 同时支撑 diff 背景与代码行，不应通过把它改透明来开启整窗效果。

本轮没有取得真实 Revia 的 UA，也没有在 WKWebView 中验证 React Bits 三条渲染路径。不要写成“Safari 只能磨砂”或“所有效果已经兼容”。

## 预览与检查

[打开预览说明](./mockups/)；第四版最终候选 HTML 可切 macOS / Windows、浅深色、缩放及降低透明度，并查看浮层。它实际使用 React Bits 组件及示例代码，复用已有 M4 图标资源和工程已有 JetBrains Mono 字体，保留各方许可；没有连接 Git、账号或产品 IPC。代码示例用于呈现颜色和密度，不是 M7 实现片段。前三版文件保持原样，保留此前反馈对应的稿件。最终候选与访谈汇总见 [开始前确认清单](./confirmation.md)。

预览检查与截图记录在说明中；它们只证明独立设计稿的显示与控制，不代替真实 Revia 的布局、性能、折射或桌面验收。

## 原生 Liquid Glass（Q200，2026-10-31 起）

macOS 26 起系统提供公开 API `NSGlassEffectView` / `NSGlassEffectContainerView`（`objc2-app-kit 0.3.2` 已有绑定）。网页里的 CSS 只能模糊网页自己的内容，折射桌面、边缘高光和随系统外观、减少透明度设置的变化都做不到，所以外壳的玻璃交给 AppKit 绘制。

**外观结构**

- 窗口透明，整窗垫一块玻璃折射窗口后面的桌面；外壳、侧栏、日志和 diff 代码都放在这同一块玻璃上，不再有 VS Code 的编辑器底色、标题条底色和分栏实色块。
- 增删行用主题里本就带透明度的颜色，透出玻璃；语法高亮颜色照旧。代码面（diff、合并、编辑器）整块垫一层中性的半透明底（深色 52% 黑、浅色 62% 白，`--code-veil`），只加在外层容器上一次：直接放在玻璃上时，窗口后面的桌面和别的窗口会在空白栏里透成一层幽灵字（用户 2026-11-01 截图指出），这层底按 Q190 把它压到看不出字形。
- 工具栏横跨顶部，胶囊紧跟红绿灯靠左：仓库 / worktree / 分支、更新 / 推送、工具各合成一块，设置单独一块。侧栏和编辑区排在工具栏下面。
- 终端（xterm 开了 `allowTransparency`、主题底色透明）同样放在玻璃上，只用一条细线和编辑区分开。PR 面板里的卡片、代码块与提示条是玻璃上的淡底加细边。
- 工具栏的下拉弹层（仓库 / worktree / 分支等 Popover）在 macOS 上是一块原生玻璃：弹层登记为区域，网页这层透明，只用网页里的模糊把弹层下方的字糊掉，不淡出整个界面。
- 克隆、诊断、更新、丢弃确认等模态对话框是一块浮在窗口上的原生玻璃面板（22pt 圆角）：打开时 `useModalDialog()` 把对话框的位置交给原生层，工具栏胶囊的玻璃暂时撤掉；后面的界面由样式淡出并模糊，透过面板看像被玻璃折射（玻璃画在网页下面，后面的内容不让开就会盖住它）。
- 设置页铺满整窗，同样放在玻璃上：打开时藏起工具栏、主界面和状态栏，标题挨着红绿灯，左侧分类、右侧设置项；隐藏的工具栏不再下发玻璃胶囊（`useModalDialog(ref, open, "page")`）。仍是原来的模态对话框，Esc 与关闭按钮照旧。
- 52pt 工具栏、36pt 胶囊、侧栏行选中用内缩圆角、编辑区页签是圆角内衬、主提交按钮用系统强调色。

**Windows（Q126、Q191；接线已写，验证在 M8）**

样式分两层：`html.window-material` 是窗口底为系统材质时的共用规则，包括布局、透明外壳、内容放在材质上；`html.native-glass` 只在 macOS，表示胶囊、对话框面板、弹层也由 AppKit 玻璃按区域绘制。Windows 11 只开 `window-material`：

- `support()` 在 Windows 11（build ≥ 22000，`windows-version`）返回 `Mica`；`apply()` 用 Tauri 的 `set_effects(Mica)`，并把 WebView2 背景设为 alpha 0。
- Tauri 文档要求窗口特效配合透明窗口，所以 `src-tauri/tauri.windows.conf.json` 只在 Windows 上把主窗口设为 `transparent`。这是 JSON 合并补丁，数组整体替换，窗口尺寸要和 `tauri.conf.json` 保持一致。Windows 10 走 CSS 材质，页面自己画不透明底。
- 胶囊由网页画一层淡底，用 Fluent 的 6px 圆角；对话框和弹层是网页画的 Acrylic 风格面板，8px 圆角。
- 标题栏仍是系统原生标题栏（Q60），Mica 一并铺上。

本机没有 Windows 工具链：`apply_mica` 用的是 Tauri 跨平台接口，已在 macOS 上编译检查；`windows_version` 那一行只在 Windows 下编译，没有检查过。M8 要在真机上验证 Mica 是否透出、深浅色跟随，以及透明窗口缩放时有没有闪烁。

**代码位置**

- `src-tauri/src/platform/window_glass.rs`：跨平台类型与入口（`GlassLayout`、`GlassBackground`、`GlassRegion`、`support()`、`apply()`）。
- `src-tauri/src/platform/macos_glass.rs`：在窗口 contentView（wry 的 `WryWebViewParent`）里、WKWebView 下方放一个 `NSGlassEffectContainerView`，胶囊各一块 `NSGlassEffectView`；整窗玻璃放在容器外、垫在容器下面（同一个容器里的玻璃会合成一层，胶囊就看不出来）。关掉 WKWebView 的 `drawsBackground`（与 wry `transparent` 用同一个私有 KVC 键），窗口 `setOpaque(false)`、底色透明。
- `src/features/m7/glass/nativeGlass.ts`：外壳元素用 `glassPane()` 登记，按元素矩形和 CSS 圆角合成布局交给 Rust（命令 `m7_glass_support` / `m7_glass_layout`）。窗口底用 CSS 变量 `--glass-window`（`glass` / `solid`）选择。窗口缩放时原生层先按贴边规则（autoresizing mask）跟随，前端测到新矩形后再精确对齐。
- `html.window-material` / `html.native-glass` 下的样式在 `src/index.css` 末尾；中间那一行在这里 `display: contents`，工具栏、侧栏、编辑区和状态栏直接排进外壳网格。

**为什么不用 tauri-plugin-liquid-glass（0.1.6）**

它只能给每个窗口铺一整块玻璃，不能按区域放；工具栏胶囊这类形状又得回到 CSS 去模拟。它还另带 `cocoa` / `objc 0.2` 一套绑定，并通过私有 selector 设置 variant。它的做法（窗口与 webview 透明、玻璃插在 webview 下方）已经照搬。

**实测与限制**

- 调度用定时器而不是 `requestAnimationFrame`：窗口被遮挡时 WebKit 暂停 rAF，玻璃会停在启动时的空布局（实测复现过）。
- 不设置窗口外观：网页的 `prefers-color-scheme` 跟随窗口外观，颜色主题又跟随系统外观（`resolveTheme`）；钉死外观会让系统切换深浅色时主题不再跟着变。
- 给玻璃设 `tintColor` 往主题色染，玻璃反而更浅更实（实测侧栏从 (63,67,76) 变成 (72,77,83)），已放弃。
- 原生模式下提交按钮不渲染 WebGL 高光（`SpecularButton` 在透明 webview 里留下一块偏色矩形）。
- 玻璃在 webview 下方，网页接收全部输入；系统的交互高光（`effectIsInteractive`）用不上，悬停、按下仍由 CSS 画。
- 浮层、对话框、通知仍是网页元素，改为系统菜单的圆角、半透明与阴影，模糊的是网页内容。
- 单窗口截图拿不到窗口后面的桌面，透明窗口在里面显示为黑底；核对玻璃要看全屏截图。
- `REVIA_GLASS=css` 强制走 CSS 材质，便于对照；macOS 26 以下与 Windows 10 自动走这条路径，Windows 11 使用 Mica。
- 参考应用那种移动的“玻璃镜片”（镜片下文字放大、边缘弯折、红蓝色散）能否实现、各平台怎么做，以及 Glass Surface 在 WKWebView 上的降级判断问题，见 [玻璃镜片与窗口材质：事实核对](glass-lens-facts.md)。

## 玻璃镜片与布局块（Q202，2026-11-01）

参照用户提供的 Loop 截图。可行性与平台事实见 [glass-lens-facts.md](glass-lens-facts.md)。

**选中项玻璃镜片**（`src/features/m7/glass/glassLens.ts`，`useGlassLens`）

- 用在编辑器页签（`EditorTabs`，键是 `data-tab-key`）和设置页导航（`SettingsDialog`，键是 `data-lens-key`）。别的列表要用，给列表外层加 `glass-lens-host`，再在 CSS 里写一条规则：带 `data-glass-lens` 时藏起自己的选中底。
- 选中项变化时，镜片从旧选中项滑到新选中项，结束后淡出，露出真实的选中项。镜片的底色和圆角取自新选中项本身的样式，所以落定时能接上。只在 `html.window-material` 下出现；减少动态效果、减少透明度、高对比度时不出现。
- 做法：镜片里放一份列表快照（`cloneNode`），对快照用 `filter: url()`。滤镜有三层：边缘一圈向外取样的位移图、放大，以及垫在原图下面的红、蓝两道位移量不同的淡影（色散）。原列表在镜片位置用 `mask` 挖空，免得叠出两份文字。macOS 与 Windows 共用这一份实现。
- 实测踩过的坑：
  - WKWebView 能解析 `backdrop-filter: url()`，但不渲染。
  - 快照若用 transform 动画，会被提升成独立的合成层，光学层上的软件滤镜就作用不到它。所以镜片和快照都改用 `left` / `top` 移动。
  - 窗口被遮挡时 WebKit 暂停 rAF，位移强度要用定时器推进。
  - 透明底上按通道拆分再合成出不了彩边，所以改用红、蓝两道淡影。
- 连续切换时，先收掉上一块镜片，并在这一帧绘制前藏起新的选中底，避免闪现。

**布局块**（`index.css` 的 `--block-*` 变量）

- 系统材质下，侧栏（`.sidebar-pane`）和编辑区（`.shell-body > main`）各是一块圆角卡片，加亮边和淡阴影，卡片之间露出玻璃。工具栏仍横跨顶部（Q200）。
- 圆角与内距成套（用户指出“圆角的到地方 还有别的圆角”）：
  - 卡片圆角是 `--block-radius`（18px）；
  - 贴着卡片边的元素至少内缩 `--block-inset`（8px），例如页签、侧栏的行；
  - 这些元素的圆角取 `--block-inner-radius`（10px），与卡片圆角同心；
  - 活动栏按钮不在卡片角上放方块底，按下只换图标颜色，图标与“提交 / Stash / 回收区”那一行对齐。
- 底色分两种：
  - 编辑区与设置项卡片用 `--block-fill`，是 82% 的主题底色，压住背后的窗口，不透出幽灵字（用户实拍截图里在日志空白处看到过）；
  - 侧栏卡片用 `--block-side-fill`，是 50% 的主题底色，透出玻璃的颜色。
  - 代码面原来那层压底色（`--code-veil`）在卡片里不再叠加，默认是透明，留给辅助功能回退改成实色。
- 状态栏收进编辑区卡片底部：卡片向下占住状态栏那一行，状态栏叠在卡片底部，上面有一条分隔线。

**设置页**（`SettingsDialog`）

- 导航改为平铺，每行带图标（`PAGE_ICONS`）；多页的分类留一行小标题，单页的分类与上一组之间留间距。原来的可折叠树和面包屑栏去掉了。
- 页面顶部是分类小字、页面大标题和关闭按钮。
- 系统材质下：左侧是浮动侧栏卡片，红绿灯落在卡片左上角；选中项是白色胶囊；每页的设置项装进一张卡片。CSS 材质下仍是居中的对话框，结构相同。

**验证情况**

- 已在 macOS 27 的调试版上用窗口录屏和无损连拍逐帧核对：页签与设置导航的镜片（放大、边缘弯折、红蓝彩边）、快速连点时不出现两块选中底、主窗口卡片布局、设置页新版式（系统材质与 `REVIA_GLASS=css` 两种）。
- 没有核对：
  - 深色外观；
  - 真实桌面合成画面（单窗口截图拿不到窗口后面的玻璃）；
  - Windows 上的镜片与卡片效果（M8）。

### Codex 合并补充（2026-11-01）

第二批已在m7合入，保留Q201引用独立列和原有辅助功能实色回退。GlassLayer在macOS显式使用不含SVG引用的磨砂回退，避免默认WKWebView UA不含Safari而误选无效的 `backdrop-filter: url()`；Windows继续由既有能力选择路径。此改动不影响独立的 `filter: url()` 镜片。实际构建与产品验证结论见 [第二批集成验证](glass-round2-integration.md)，未验证部分保持未验。
