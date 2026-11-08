# Revia 卡顿排查：事实收集（只读）

- 日期：2026-11-08；仓库 `main` @ `91b2343`（0.0.2 已发布）；未改任何文件、未运行应用。
- 标记：**[代码]** 代码事实（附 `文件:行号`）；**[文档]** 一手文档或源码写明；**[项目实测]** 仓库里已有的实测记录；**[第三方]** 非官方报告；**[推论]** 由上述推出；**[需实测]** 只能在真机确认。
- 用户只说“Windows 非常卡”，没说是哪些操作。“卡”分两类：**画面不顺**（滚动、弹层、动画掉帧，看渲染）和**操作响应慢**（点了半天才有结果，看 Git、IPC、刷新）。下面两类都列，排序综合了“Windows 独有或被放大”和“影响面”。

---

## 0. 速览

### Windows 卡顿嫌疑（按嫌疑大小）

| # | 嫌疑 | 类型 | 证据强度 | 真机最快确认 |
|---|---|---|---|---|
| W0 | **条件性最高**：Windows 10（build < 22000）或设了 `REVIA_GLASS=css` 时，走的不是 Mica，而是 React Bits 的 **SVG 折射 `backdrop-filter: url()`**。全窗一层，弹层、对话框、通知各一层，每层三次 `feDisplacementMap`，再加 WebGL 提交按钮（窗口内任何鼠标移动都会唤醒重画） | 画面 | [代码]；代价 [需实测] | 先记 `winver` 的 build 号；Performance 面板录一次滚动 |
| W1 | 透明窗口（tao 用空区域调 `DwmEnableBlurBehindWindow`，WebView2 背景 alpha 0）+ Mica，再叠加：大面积半透明卡片、**滚动内容上方的 `backdrop-filter`**（日志吸顶表头、常驻的失败通知、弹层）、对话框打开时**整个界面 `filter: blur(12px)`** 并叠一层 30px 背景模糊 | 画面 | Mica 本身为性能设计 [文档]；像素移动类背景滤镜会扩大重绘区 [文档：Chromium 源码]；透明 WebView2 失去 GPU 快速路径 [第三方] | 关/开“透明效果”做 A/B；DevTools Rendering 面板模拟 reduced-transparency；看 Paint flashing 和 Layer borders |
| W2 | **每条 git 命令完成后**都在全局 `CommandLog` 锁内同步执行：读出最多 500 条的诊断摘要 JSON → 解析 → 美化序列化 → 建临时文件 → `sync_all` → rename。Windows 上要经过 FlushFileBuffers、NTFS rename，以及 Defender 对新文件的扫描 | 响应 | [代码]；macOS 上测得 4 次命令共 26ms [项目实测]；Windows 上被放大 [推论] | Process Monitor 过滤数据目录里的 `diagnostics` 写入；对比命令日志里的 `elapsed_ms` 与界面实际等待时间 |
| W3 | 文件监视递归覆盖整个工作区，**不按 .gitignore 过滤**（`node_modules`、`target`、`dist` 的变化都算）。每次事件后：`list_changes`（status + 两次 diff）→ 重新打开当前文件的 diff → 分支概览 → 进行中操作与 autostash → 日志 refs 检查 →（若开着）stash 面板。Windows 上起进程开销大 | 响应 + CPU | [代码]；触发频率 [需实测] | 空闲时打开命令日志面板，看 git 命令是否持续出现；在仓库里跑一次构建再观察 |
| W4 | **首次打开每个 worktree 时**，在会话表锁内启动 PowerShell 7（或 Windows PowerShell）并加载 profile 捕获环境，硬超时 5 秒。期间其他需要会话的 IPC 全部排队 | 响应（打开/切换） | [代码]；时长 [需实测] | 记录 `pwsh -Command exit` 和带 profile 的启动耗时；在诊断里查看环境捕获结果 |
| W5 | WebView2 的 IPC 走 `http://ipc.localhost/` 的 fetch。维护者转述的一次非正式基准：10MB 数据 macOS 约 5ms，Windows 约 200ms。Revia 全部是 JSON `invoke`（`list_changes` 返回全量快照，`side_by_side_layout` / `side_lines` / `get_rows` 也是） | 响应 | [第三方：维护者转述]；Revia 实际载荷 [需实测] | 用 `REVIA_MEASURE` 的 Rust 计时对比前端 Promise 耗时（注意：IPC 计数器在 Windows 上失效，见 §5） |
| W6 | 空闲时**每秒整树重渲染**：轮询 `watch_status` 后无条件 `setWatchState({...})`，传的是新对象。App 有 6968 行，`LogView`、`SideBySideView`、`DiffView` 都没有 memo | CPU（空闲） | [代码]；耗时 [需实测] | Performance 面板空闲录 5 秒，看是否每秒出现一次长任务 |
| W7 | 并排视图行号每行都带 `will-change: transform`，约 90 个合成层；LogView 每个 scroll 事件都 `setState`，还要对全部已加载的行做一次 `Math.max(...)` | 画面 | [代码]；[需实测] | DevTools Layers 面板数层数；Performance 面板录一次日志滚动 |

### macOS 卡顿嫌疑（按嫌疑大小）

| # | 嫌疑 | 证据强度 |
|---|---|---|
| M1 | 同 W2：每条 git 命令同步写诊断摘要。macOS 上 Rust 的 `sync_all` 是 `fcntl(F_FULLFSYNC)`，会把磁盘缓存刷到介质。M7 首屏诊断里，命令记录阶段合计 26.0ms，占同线程 4 次 Git 调用 89.5ms 的约 29% | [代码][文档：Rust std 源码][项目实测] |
| M2 | 同 W3：监视不过滤 + 每次事件整链刷新 | [代码]；[需实测] |
| M3 | 渲染：日志吸顶表头在滚动行上方做 `backdrop-filter: blur(20px)`；弹层 `blur(18px) saturate(1.4)`；对话框打开时整界面 `filter: blur(12px)`；镜片动画每帧改 left/top/尺寸、mask，16ms 定时器修改 SVG 滤镜参数，WebKit 用软件路径做 `filter:url()`；并排行号约 90 个合成层；WKWebView 透明后与系统玻璃合成 | [代码]；代价 [需实测] |
| M4 | 同 W6：空闲时每秒整树重渲染（窗口可见、开着仓库时） | [代码] |
| M5 | 原生玻璃同步：开销低。用 setTimeout 合并，JSON 去重，只有布局变了才发 IPC；但窗口缩放时每帧都会强制布局测量一次并发一次 IPC | [代码] |
| 已知未达标 | D06 首屏 224–252ms（门槛 150ms）；统一视图首次大幅跳转 25–29ms（backlog 05）；并排视图偶发 27ms 帧（backlog 01）；Q98 内存增长 10.2MiB（门槛 10MiB，backlog 04） | [项目实测] |

---

## 1. Windows 路径实际生效的渲染开销

前提条件 [代码]：`support()` 在 build ≥ 22000 时返回 `Mica`（`src-tauri/src/platform/window_glass.rs:77-92`）；前端 `startNativeGlass()` 只加 `html.window-material`，不加 `native-glass`（`src/features/m7/glass/nativeGlass.ts:84-89`）；`apply_mica` 把 WebView2 背景设成 `Color(0,0,0,0)`（`window_glass.rs:111-124`）；`tauri.windows.conf.json` 对**所有** Windows 设置 `"transparent": true`。

### 1.1 Windows 11（`html.window-material:not(.native-glass)`）逐条清单

| 选择器 | `src/index.css` | 属性 | 作用面积 | 是否在滚动内容上 / 何时持续 | 标记 |
|---|---|---|---|---|---|
| `html.window-material .log-table-head` | 2405-2409 | `backdrop-filter: blur(20px)`，底色透明；元素本身是 `sticky top-0`（`src/features/log/LogView.tsx:852`） | 24px 高 × 表格宽（可超过 1000px） | **是**：日志每滚一帧，下面的行都在变，背景滤镜每帧重算 | [代码]；伤害扩大见 §2 [文档] |
| `html.window-material :is(.app-dialog, .notification)` | 2576-2581 | `backdrop-filter: blur(24px) saturate(1.8)`，90% 半透明底 | 通知宽 `min(420px, 100vw-32px)`（602 行），位于右下角，**压在 diff 上**；失败与待核对通知常驻 | **是**：通知开着时滚动 diff，每帧重算 | [代码] |
| `html.window-material:not(.native-glass) .app-dialog:not(.settings-dialog)` | 2854-2861 | `backdrop-filter: blur(30px) saturate(1.3)`，`box-shadow: 0 16px 40px` | 对话框面板（克隆、诊断、更新、丢弃确认） | 对话框开着期间；背后已经是模糊过的界面 | [代码] |
| `html.window-material:has(.app-dialog[open]:not(.settings-dialog)) :is(:is(.glass-host,.welcome-shell) > :not(dialog,.shell-body), .shell-body > *)` | 2869-2875 | `opacity: .35; filter: blur(12px)` | **整个界面**：工具栏、侧栏卡片、编辑区卡片、状态栏 | 对话框开着期间。每个被模糊的子树都单独成一个 render pass，子树里任何重绘都要重新模糊整块 | [代码]；render pass 的概念见 §2 [文档] |
| `…:has(.app-dialog[open]…) .settings-dialog[open]` | 2909-2912 | 同上 | 全窗设置页 | 设置页上再打开确认框时 | [代码] |
| `html.window-material .revia-popper` | 2550-2558 | `backdrop-filter: blur(24px) saturate(1.8)`，`box-shadow: 0 12px 32px` | 弹层，约 240px 以上 | 弹层开着时。在 Windows 上入场动画仍然生效（`.revia-popper { animation: revia-pop-in 120ms }`，996-1006 行；只有 macOS 的 `.glass-popover` 关掉了动画，2569 行），会移动一个带背景滤镜的元素 | [代码] |
| `html.window-material .github-panel [role="dialog"]` | 2901-2906 | `backdrop-filter: blur(30px) saturate(1.4)` | PR 面板里的确认框 | 打开时 | [代码] |
| `html.window-material .shell-body > main` | 2367-2378 | `overflow:hidden; border-radius:18px; box-shadow: var(--block-shadow)`（`0 1px 2px` + `0 10px 28px`，2212/2219 行），底色 82% 半透明（2209 行） | **编辑区整块卡片**（几乎占满窗口右侧） | 卡片本身是静态的；但里面有合成的滚动内容（`.sbs-canvas` 带 `will-change`），外面套圆角裁切 | [代码]；圆角裁切是否需要额外的 render surface [推论/需实测] |
| `html.window-material .sidebar-pane` | 2341-2352 | 圆角 18px，阴影同上，50% 半透明 | 侧栏整块 | 静态 | [代码] |
| `html.window-material { --diff-bg: transparent; … }` | 2356-2365 | diff、合并、编辑器的行底全部透明 | 整个代码区 | **是**：diff 画布是非不透明的合成层。在 Chromium 里，非不透明层上的文字会改用灰度抗锯齿，合成时要逐像素混合 | [代码]；后果 [推论] |
| `.sbs-canvas` / `.sbs-number` | 1020-1023 / 1116-1126 | `will-change: transform`；行号每行一个（`src/features/diff/SideBySideView.tsx:1123,1178`，每栏约 `高度/19+3` 行，两栏约 90 个） | 并排视图 | **是**：滚动时重新渲染这些行号并改写位移 | [代码]；层数 [需实测] |
| 镜片 `.glass-lens*` + `src/features/m7/glass/glassLens.ts` | 2498-2547 | 克隆整条页签列表（126 行）；对 left/top/width/height 做动画（141-152 行，主线程布局）；原列表做 mask 动画（155-168 行）；16ms 定时器修改 `feDisplacementMap` 的 scale（172-179 行）；每次播放都用 canvas 逐像素生成位移图再 `toDataURL`（286-319 行） | 页签行、设置导航 | 只在切换页签或设置页时出现，持续 520ms + 淡出 160ms；减少动态效果时关闭（63-69 行） | [代码] |
| `:has()` 选择器 | 2400、2412、2688、2747、2869、2909 | 根元素和 `.editor-surface` 上的 `:has()` | 全局 | 有 DOM 变动时可能触发样式失效检查 | 低嫌疑 [推论/需实测：看 Recalculate Style] |

在 Windows 11 上**不生效**的部分 [代码]：`html.window-material .glass-layer { display:none }`（2231-2233）隐藏了 React Bits 材质层；`native-glass` 段（2197-2200、2562-2570）只在 macOS 生效。

### 1.2 Windows 10 或 `REVIA_GLASS=css` 时的 CSS 材质路径（不设 `window-material`）

| 位置 | 内容 | 标记 |
|---|---|---|
| `src/App.tsx:5842` `<GlassLayer variant="window" />` | 全窗材质层 | [代码] |
| `src/features/m7/glass/GlassLayer.tsx:47` | `refraction={!flat && !isMac}`：Windows 上打开折射 | [代码] |
| `src/components/m7/glass/GlassSurface.tsx:780-796, 549-553` | UA 不含 Safari 且 `div.style.backdropFilter = url(#id)` 能被解析时，选用 `glass-surface--svg`。WebView2 满足这两个条件 | [代码] |
| `src/components/m7/glass/GlassSurface.css:32-46` | `backdrop-filter: url(#filter) saturate()`；外加 6 层 box-shadow（含 56px 模糊的内阴影） | [代码] |
| `GlassSurface.tsx:829-871`（滤镜定义） | `feImage` → 3 次 `feDisplacementMap` → 3 次 `feColorMatrix` → 2 次 `feBlend` → `feGaussianBlur` | [代码] |
| `GlassSurface.tsx:485-498` | ResizeObserver 回调 → `setBoxSize` → 重新生成位移图 data URL（窗口缩放时） | [代码] |
| 弹层、对话框、通知、设置页各有一层 `<GlassLayer variant="popover" />`（`popover.tsx:66`、`Dialogs.tsx:40`、`Notifications.tsx:80` 等） | 这些层的“背后”就是界面内容，内容一变就要重算 SVG 背景滤镜 | [代码]；代价 [需实测] |
| `src/features/m7/glass/GlassButton.tsx:89` + `src/components/m7/glass/SpecularButton.tsx:409-446` | 提交按钮换成 WebGL2 画布，在 `window` 上监听 `pointermove`。鼠标在窗口内任何位置移动都会唤醒 rAF 重画一次（391-397） | [代码] |
| `index.css:1898-1922` | 这条路径下弹层、对话框、通知也是 `backdrop-filter: blur(20px) saturate(1.05)` | [代码] |
| 窗口仍是透明窗口 | `tauri.windows.conf.json` 不区分 Windows 版本；`apply()` 对 `Unsupported` 什么都不做（`window_glass.rs:96-106`），WebView2 背景保持 alpha 0 | [代码]；合成代价 [推论] |

**注意 [代码/推论]**：在 Windows 上，`REVIA_GLASS=css` **不能当“关掉玻璃”的对照组**。它会切换到上面这条更重的 SVG 折射路径，窗口仍然透明，只是没有 Mica。真正关掉玻璃的对照见 §6。

---

## 2. 透明窗口 + Mica + WebView2：已知性能资料

### 文档或源码写明

1. **WebView2 `DefaultBackgroundColor`**：alpha 只支持 0 或 255；alpha 为 0 时“WebView will render hosting app content as the background”；也可以用环境变量 `WEBVIEW2_DEFAULT_BACKGROUND_COLOR` 设置，并说明用 API 设置可能有白闪。**文档没有任何性能说明**。https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2controller2
2. **Mica 为性能设计**：微软文档说 Mica “only samples the desktop wallpaper once”；在关闭透明效果、省电模式、低端硬件、窗口失活、build < 22000 时退回实色。https://learn.microsoft.com/en-us/windows/apps/design/style/mica
3. **window-vibrancy 0.8.1**（本地 registry 源码 `src/lib.rs:42-47, 87-92`）：只有 `apply_blur`（22621 上）和 `apply_acrylic`（Win10 1903+ 与 Win11 22000 上）标注了“poor performance … lag when resizing or dragging”。**`apply_mica` 没有这条警告**。`windows.rs:104-121` 中，build ≥ 22523 走公开的 `DWMSBT_MAINWINDOW`，22000–22522 走未公开属性 1029。Revia 用的是 `Effect::Mica`。
4. **Tauri `set_effects`**：注释写明“Requires the window to be transparent”，Windows 上提到 decorations 或 shadow 的变通办法（tao#72）。来源：本地 `tauri-2.12.4/src/window/mod.rs:2141-2179`。
5. **透明窗口的实现**：tao 0.37.1 `platform_impl/windows/window.rs:1203-1215` 对透明窗口调用 `DwmEnableBlurBehindWindow`，传空区域，让 DWM 做逐像素 alpha；wry 0.57.0 `webview2/mod.rs:127-131, 463-466` 在 `transparent` 时把 WebView2 默认背景设成 `(0,0,0,0)`。wry 用的是 HWND 窗口式托管（`CreateCoreWebView2Controller`），不是 visual hosting。
6. **Chromium 背景滤镜会扩大重绘区**：`cc/trees/damage_tracker.cc` 的注释和代码写明，带像素移动类背景滤镜（blur 属于此类）的 render surface，只要下方有变化，就把重绘区扩大到整个 surface（203-222、520-531 行）。所以“吸顶表头在滚动行上方做模糊”“通知压在滚动的 diff 上”每帧都要重新滤镜。https://chromium.googlesource.com/chromium/src/+/main/cc/trees/damage_tracker.cc
7. **Chromium render pass**：`docs/how_cc_works.md` 写明 filters（如 blur）、mask、子树 opacity 都需要单独的 render pass（350 行）。https://chromium.googlesource.com/chromium/src/+/main/docs/how_cc_works.md
8. **WebView2 性能指南**：默认使用 GPU，不要关闭；排查工具有 DevTools Performance、WPR 加 `WebView2.wprp`、`edge://inspect`、Browser Task Manager；建议先用最简内容对照。https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance
9. **Tauri 在 Windows 上的 IPC URL**：`tauri-2.12.4/scripts/core.js:13-19` 与 `ipc-protocol.js:37` 用 `http://ipc.localhost/<cmd>`，macOS 用 `ipc://localhost/<cmd>`。

### 第三方报告（不是官方结论）

- athas PR #728（已合并，2026-07）：作者认为 WebView2 透明窗口会让整个表面做逐像素 alpha 合成，失去 GPU 快速路径，于是把 Windows 主窗口改为不透明；没有给出测量数据。https://github.com/athasdev/athas/pull/728
- tauri#6322：WebView2 或 Chromium 缩放窗口本身就慢（开启硬件加速时），与透明无关。https://github.com/tauri-apps/tauri/issues/6322
- Tauri 维护者在 discussion #11915 转述一次非正式基准：10MB 二进制 IPC，macOS 约 5ms，Windows 约 200ms。https://github.com/orgs/tauri-apps/discussions/11915

### 推论

- Mica 本身大概率不是主因（第 2、3 条）。更可能的代价是：透明窗口逐像素合成，加上非不透明的大面积网页层，再叠加多处像素移动类背景滤镜和整界面 `filter: blur`。三者在 Windows 上各自多大**都需实测**。
- 项目内既有的事实文档 `docs/milestones/m7/glass-lens-facts.md:46,95,179` 已写明：引用滤镜的性能、透明 WebView2 窗口拖动或缩放是否卡，都属于**需实测**。本轮没有找到能推翻这一点的一手资料。

---

## 3. 与平台无关的前端与后端热点

### 3.1 定时器、轮询与刷新触发点

| 位置 | 内容 | 频率 | 影响 |
|---|---|---|---|
| `src/App.tsx:1196-1242` | 轮询 `ipc.watchStatus()`；滚动、滚轮、按键 250ms 内暂停；`document.hidden` 时不轮询 | 空闲 1000ms，交互中 300ms 一次检查 | 1210 行每次都 `setWatchState({active, error})`，传的是**新对象** → 每秒整个 App 重渲染 [代码] |
| `src/App.tsx:1245-1263` | `visibilitychange`、`focus` 以及原生 `onFocusChanged` → `refreshCoordinator.request()` | 每次回到前台 | 整链刷新一次 |
| `src/App.tsx:1987` | 终端有输出后 900ms 防抖 → `refreshTarget` | 终端活动时 | 整链刷新 |
| `src/App.tsx:1005-1020` | `acceptSnapshot` → `applyRefreshedSnapshot`（872-899，**重新打开当前文件**）+ `setWatchRevision(+1)` + `setStatus` | 每次刷新 | 下面四条都接在它后面 |
| `src/App.tsx:5110-5121` + `src/features/branches/useBranches.ts:88-90` | `refreshKey: refsRevision + watchRevision` → 分支概览 reload | 每次刷新 | 多条 git 命令 |
| `src/App.tsx:5141-5148` | `refreshM6State` 依赖 `watchRevision` 与 `snapshot.revision` → `operationState` + `autostashPending` | 每次刷新 | git 与文件读取 |
| `src/App.tsx:6053` | StashPanel 的 `refreshKey` 含 `watchRevision` | stash 面板开着时 | |
| `src/App.tsx:6376` + `src/features/log/LogView.tsx:337-355` | `refsWatchRevision` 变化 → 日志 refs 检查 | 日志页开着时 | |
| `src/features/branches/useBranches.ts:95, 126` | 自动 fetch 与远端检查 | 10 分钟一次（21 行） | 低 |
| `src/features/commands/CommandLogPanel.tsx:18, 53` | 命令日志轮询 | 800ms，只在面板开着时 | 低 |
| `src/features/m7/glass/nativeGlass.ts:90-102` | ResizeObserver 观察 `<html>`、每块玻璃及其父元素；MutationObserver 观察 html 的 `data-kind`、`data-theme`、`style`；两个 matchMedia | 事件驱动，setTimeout(0) 合并 | Windows 上 `measure()` 只读一次根元素的 computed style（173 行），IPC 只发一次 |
| `src/features/log/LogView.tsx:479-491` | 每个 scroll 事件都 `setScrollTop`；持久化有 200ms 节流 | 每个滚动事件 | LogView 整体重渲染，每次还要 `Math.max(2, ...graphRows.map(...))`，代价随已加载行数线性增长（448 行） |
| `src/components/app/ChangesTree.tsx:212` | 每个 scroll 事件 `setScrollTop` | 每个滚动事件 | 只影响列表本身 |
| `src/features/diff/SideBySideView.tsx:686-701` | 程序接管滚动（ADR-0008）：每次滚动先改 transform，再 `flushSync(setFirstRow)` | 每次滚轮事件 | 设计如此，D06 已覆盖 |
| `src/features/m7/TerminalPanel.tsx:125` | xterm 开启 `allowTransparency`；`package.json` 只有 `addon-fit`，因此用的是 DOM 渲染器 | 终端输出时 | 终端大量输出时可能卡 [推论] |

React 编译器未启用（`vite.config.ts` 只有 `react()` 和 `tailwindcss()`），只有 `DiffView` 的 `RowView` 和 `SideBySideView` 的 `LineRow` 用了 `memo`（`DiffView.tsx:573`、`SideBySideView.tsx:934`）[代码]。

### 3.2 Rust 侧：监视与刷新

- `src-tauri/src/watch.rs:192`：`watcher.watch(&roots.worktree, RecursiveMode::Recursive)`；438-446 行除 `.git` 子树外**全部放行**；全文件没有 gitignore 相关逻辑 [代码]。防抖 120ms（37 行），而且要连续静默 120ms 才发（250-253 行）。
- `src-tauri/src/session.rs:462-485`：一次 `refresh` 要跑 `status::read`、`diff_worktree`、`diff_index`（changelist 模式再多一次 diff）；diff 前后各做一次 stat，若有变化就报错，不发布结果 [代码]。macOS 实测 refresh 约 90–95ms（`docs/milestones/m7/native-acceptance.md:83`）[项目实测]。
- `src-tauri/src/git/mod.rs:120-131`：`CommandLog::record` 持着 `inner` 锁调用 `store.observe(&record)`；548-564 行，每条命令在返回前都会经过这里 [代码]。
- `src-tauri/src/m7/diagnostics.rs:146-158`（`observe`）→ `load()`（113-143，读、解析、清洗，内容有变时还会多写一次）→ `atomic_write`（`src-tauri/src/m7/mod.rs:38-53`：`NamedTempFile` → `write_all` → **`sync_all`** → `persist`）。上限 500 条，保留 7 天（16、15 行）。生产环境已接上（`src-tauri/src/commands.rs:163`）[代码]。
- `src-tauri/src/commands.rs:220-250`：`session()` 在**持有 sessions 锁**时，首次创建会话会调用 `crate::shell::resolve()`；Windows 实现（`src-tauri/src/shell.rs:284-300`）启动 PowerShell 并加载 profile，`SHELL_TIMEOUT = 5s`（`shell.rs:29`）[代码]。

### 3.3 M8 文档中关于性能、滚动与 Mica 的记录

- `docs/milestones/m8/windows-acceptance.md:98-110`（§7）：Mica 生效、缩放、diff 首屏、跳转与快速滚动不白屏、“长时间滚动不卡死”、记录首屏与跳转耗时（沿用 D06），**全部未执行**（第 7 行：“本清单由 Pi 编写，尚未执行”）。
- `docs/milestones/m8/spec.md:68, 83`：“真实效果与滚动性能留 Windows 桌面验收”；`report.md:150-151` 与 `verification.md:43`：Mica、DPI、WebView2 diff 首屏、跳转、滚动“只在代码与既有 CSS 层验证”。
- `docs/README.md` Q206：Mica、WebView2 滚动必须在真实 Windows 桌面验收。
- **仓库里没有任何 Windows 实测帧率或耗时数据**；也没有记录 WebView2 与 WKWebView 在虚拟滚动上的差异，只有“待验收”。

---

## 4. macOS 路径的开销与已有门槛

### 4.1 原生玻璃同步 [代码]

- `src/features/m7/glass/nativeGlass.ts:136-140`：用 `setTimeout(flush, 0)` 调度（注释说明不用 rAF 是因为窗口被遮挡时 WebKit 会暂停 rAF）；一轮任务内最多 flush 一次。
- `flush()`（142-156）：`measure()` → `JSON.stringify` 与上次比较 → 相同就不发 IPC。`measure()`（158-186）读根元素的 computed style，并对每块玻璃调用 `getBoundingClientRect` 和 `getComputedStyle`，会强制布局。
- 观察范围（129-134）：每块玻璃**及其父元素**。窗口缩放时每帧都会测量并发一次 IPC → `with_webview` → 主线程 AppKit `setFrame`（`src-tauri/src/platform/macos_glass.rs:101-130`）；缩放过程中先靠 autoresizing mask 跟随。
- `glassPane` 首次挂载时额外 `setTimeout(schedule, 80)`（54 行，给弹层定位用）。
- 结论：只有缩放窗口、开关弹层或对话框时才有开销，**不在滚动路径上**。低嫌疑。

### 4.2 WKWebView 上的渲染开销 [代码；代价需实测]

- `.log-table-head` 背景模糊（2405-2409）在 macOS 上同样生效（WebKit）。
- `html.native-glass .glass-popover`：`backdrop-filter: blur(18px) saturate(1.4)`，动画已关（2562-2570）。
- macOS 上对话框面板本身是原生玻璃，`.app-dialog` 不做 backdrop（2838-2846）；但背景**整界面 `filter: blur(12px)`**（2869-2875）同样生效。
- 镜片：`glassLens.ts` 的注释与 `docs/milestones/m7/liquid-glass.md:131-135` 写明 WebKit 对光学层用软件滤镜，所以快照必须用 left/top 移动，不能用 transform。也就是说动画期间每帧都要主线程布局，再加一次软件 SVG 滤镜。
- 并排行号约 90 个 `will-change` 层（同 W7）。
- 透明 WKWebView（`drawsBackground=false`）+ 窗口 `setOpaque(false)` + 下面的 `NSGlassEffectView`（`docs/milestones/m7/liquid-glass.md:102`）：网页层全部非不透明 [推论]。

### 4.3 已有性能门槛与未达标记录 [项目实测]

| 项 | 门槛 | 现状 | 来源 |
|---|---|---|---|
| D06 首屏 | 150ms（`docs/technical-direction.md:115`，Q46） | M7 原生玻璃下：并排 252、220ms，统一 176ms；诊断版 224、236、238ms。拆分：refresh 约 95ms（status 17.9、两次 diff 56.5、身份与审阅 16.1；同线程 4 次 Git 89.5ms，其中**命令记录 26.0ms**），并排布局 worker 28ms，前端 coverage 114ms、paint 28ms | `docs/milestones/m7/native-acceptance.md:37-100`；`docs/branch-consolidation-2026-11-05.md:31` |
| D06 跳转 | 已取过的数据 1 帧内出现；未取过的 100ms 内（Q52） | 统一视图首次外跳 25–29ms（backlog 05）；M2 时为 18ms（`withinOneFrame=false`） | `docs/milestones/backlog/05-first-jump-efficiency-core.md`；`docs/milestones/m2/final-measure-20261025.md:13-26` |
| 并排滚动 | 无超过 20ms 的帧 | 偶发 27ms 帧；M2 四组最大帧 18、31、48、18ms | `backlog/01-split-scroll-hitch.md`；M2 final-measure |
| Q98 长测 | WebContent 压力后增长不超过 10MiB | 10.2MiB（Q154 带此未达标项合入） | `backlog/04-q98-webcontent-footprint.md`；`docs/README.md` Q98、Q154 |
| Q103 | 侧栏半透明，D06 不退化才保留 | M4 起由 Q127 改到新界面重做；**玻璃改版后没有做过“开/关玻璃”的 D06 对照** | `docs/README.md` Q103、Q127 |
| 桌面验收 | | backlog 06（M4、M5）、14（M6）未完成 | `docs/milestones/backlog/` |

---

## 5. 已有测量手段

| 手段 | 能否在 Windows 跑 | 测什么 | 局限 |
|---|---|---|---|
| `REVIA_MEASURE=1` + `REVIA_MEASURE_ONLY=split,unified` + `REVIA_MEASURE_REPO` / `_PATH` / `_LOG`（`src-tauri/src/commands.rs:3026-3075`；`src/App.tsx:4464-4491`） | **大概率能**：入口全是环境变量，`lib.rs` 与 `commands.rs` 没有 `cfg(macos)` 门控；Windows 在 `setup` 里显式建窗（`lib.rs:106-124`），测量时同样会设置窗口标题 [代码]；实际运行 [需实测] | 首屏拆分（`list_changes`、首批取行、render、是否拿到 rAF 绘制）；跳转（cached、frameReady）；程序化滚动 60 屏（p50、p95、最大帧、超过 20ms 的帧数、未覆盖行）（`src/lib/measure.ts:209-280`）；Rust `MeasureSpan` 时序写入 `.log` | 只测 diff 视图；滚动是每帧跳一整屏，不等于触控板滚动；不覆盖日志、弹层、对话框、镜片；rAF 间隔对 GPU 或合成线程的瓶颈不一定敏感 [推论] |
| `REVIA_SMOKE=1` | 同上 | 添加仓库 → 文件列表 → 逐个打开 → 切换视图 | 不是性能测量 |
| `REVIA_SOAK_SECONDS` + `REVIA_MEASURE_DATA_DIR` + `REVIA_SOAK_DIAG_*` | 应用侧能跑；`tools/bench-long-running.py` 依赖 `vmmap`（208-217 行），**只能在 macOS** | 长测与内存 | Windows 需要另找内存取样办法 |
| IPC 诊断 `src/lib/ipcDiagnostics.ts` | **在 Windows 上失效**：前缀写死 `ipc://localhost/`（第 3 行），Windows 实际是 `http://ipc.localhost/`（§2 第 9 条），所以计数会是 0 [代码] | 每个命令的 fetch 头延迟、并发数 | 修好前缀才能用 |
| fixture 生成 `tools/fixtures/setup.sh` | **不能直接跑**：bash，且写死 `GIT=/usr/bin/git`、`PYTHON=/usr/bin/python3`（18-19 行） | D06 标准数据 | 可以在 Mac 上生成后拷到 Windows [推论] |
| 应用内的命令日志面板与诊断对话框 | 能（release 版自带） | 每条 git 命令的参数、`elapsed_ms`、退出码；能看出空闲时是否在反复刷新、单条命令有多慢 | 看不到 fsync 记录本身的耗时 |
| `REVIA_GLASS=css` | 能 | 强制 CSS 材质 | **在 Windows 上不是无玻璃对照**（§1.2） |
| WebView2 DevTools | release 版没开（`Cargo.toml` 的 tauri features 只有 `custom-protocol`）[代码] | Performance、Layers、Rendering（Paint flashing、Layer borders、Frame rendering stats、模拟 prefers-reduced-transparency / reduced-motion） | 需要带 `devtools` feature 的诊断构建，或者用 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` 加 `edge://inspect`。wry 会自己调用 `set_additional_browser_arguments`（`wry webview2/mod.rs:300-333`），环境变量能否生效 [需实测] |
| WPR + `WebView2.wprp`、Browser Task Manager | 能（微软文档推荐） | CPU、GPU、进程 | — |

---

## 6. 在 Windows 真机上最快的确认顺序

1. **记录环境**：`winver` 的 build 号（< 22000 就是 W0 路径）、显卡与驱动、WebView2 版本、是否开着省电模式、是否有 Defender 实时保护。
2. **不改代码的玻璃 A/B**：
   - 设置 → 个性化 → 颜色 → **透明效果**关闭后重启 Revia。如果它映射到 `prefers-reduced-transparency`（Chrome 118 起支持该媒体查询，https://developer.chrome.com/blog/css-prefers-reduced-transparency；Windows 具体对应哪个开关 [需实测]），就会触发 `index.css:2916-2937` 的回退：所有 blur 和 filter 去掉，`--glass-window: solid` → Rust 撤掉 Mica 并设成不透明底色（`window_glass.rs:119-122`）。卡顿前后差别明显，就说明是渲染（W1）。
   - 设置 → 辅助功能 → 视觉效果 → **动画效果**关闭 → `prefers-reduced-motion` → 镜片关闭（`glassLens.ts:63-69`）。
3. **只开一个诊断构建**（不进仓库）：打开 tauri 的 `devtools` feature。Performance 面板分别录：日志滚动、diff 滚动（通知开着与关着各一次）、开关对话框、空闲 5 秒（看每秒一次的重渲染）、切换页签（镜片）。Rendering 面板打开 Paint flashing 和 Layer borders，Layers 面板数合成层。
4. **逐条关样式**：在 DevTools Elements 里临时取消 `index.css:2407-2408`（日志表头 blur）、2579-2580（通知 blur）、2874（对话框背景 blur），以及 `.sbs-number` 的 `will-change`（1125），看帧时间。
5. **透明窗口本身的对照**：诊断构建把 `tauri.windows.conf.json` 的 `transparent` 设为 `false`，并跳过 `apply_mica`，对比同一操作（验证 athas 的说法）。
6. **响应慢类**：
   - 空闲时打开命令日志面板 1 分钟，看是否有 git 命令持续出现（W3）；在仓库里跑一次 `npm install` 或 `cargo build` 再看。
   - 用 Process Monitor 过滤 Revia 进程对数据目录 `diagnostics*` 的 CreateFile、FlushBuffers、SetRename 次数与耗时（W2）。
   - 第一次打开一个仓库时计时，同时单独测 `pwsh -NoLogo -Command exit`（带与不带 profile）（W4）。
7. **跑既有测量**：用 Mac 生成的 fixture，`REVIA_MEASURE=1 REVIA_MEASURE_ONLY=split` 在 Windows 跑一次，和 M7 的 macOS 数据对照（注意：IPC 计数器无效）。

---

## 7. 读取失败与未核实

- magpie `grok-search-rs__web_search` 第一次调用超时（50 秒），之后改用内置 WebSearch；`web_fetch` 正常。
- `grida.co/docs/wg/research/chromium/effect-optimizations` 返回的是空的目录页，没有采用，改为直接读 Chromium 源码。
- 没有找到 WebView2Feedback 中专门讨论“透明背景 + 性能”的 issue；“透明 WebView2 失去 GPU 快速路径”只有第三方 PR 的说法。
- 未核实：Windows 的“透明效果”开关是否映射到 `prefers-reduced-transparency`；`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` 与 wry 设置的参数谁优先（微软的远程调试页只讲 UWP 加 Device Portal 的做法）。
- 未读取：微软的“Apply Mica in Win32”页、tao#72 原文、Chromium 关于 fast rounded corners 的文档。
