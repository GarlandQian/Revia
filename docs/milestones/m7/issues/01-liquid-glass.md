# 01：采用第四版玻璃外观

Status: done
Blocked by: None

## What to build

让既有审阅与提交界面采用已确认的连续玻璃外观，覆盖窗口、按钮、工具入口、浮层与平台外壳。

依据：规格 A；Q189–Q192、Q199。

## Acceptance criteria

- [x] 实际复用固定 React Bits 三个组件，保留完整许可；主提交按钮采用第四版的低饱和填充和清晰状态
- [x] 代码、行号、增删标记清晰；布局密度、焦点、点击、弹层、拖动与滚动保持可用
- [x] 浅深色、macOS / Windows 外壳与修饰键、减少透明度 / 动态效果及高对比度有明确处理
- [x] 相关 build / 类型 / lint 通过；真实材质与 D06 交任务 14，不凭浏览器预览标通过

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / user glass scope feedback

用户最新反馈：“paseo做的好像不是全liquid glass”。Codex 已核对源码，确认当前只是局部接入，未达到用户确认的第四版整窗连贯材质。这是当前初次实施中的用户纠正，不是交付后的正式 Pi 返修；formalRepairUsed 仍为 false。

已核实缺口：
1. GlassSurface 实际只接 TitleBar 和共享 Popover，二者都写死 refraction={false}、displace={0}；侧栏、状态栏、设置与其他对话框没有对应整窗材质。
2. 通用 Button、icon-action 以及提交并推送 / 最近信息 / 提交选项等按钮仍大量沿用旧材质。SpecularButton 只替换了主提交，GlassIcons 则用于新增顶栏一组入口，没有形成第四版的整体工具栏与侧栏视觉。
3. src/index.css 把 #d5e6f3 / #172b3d 和 #355e7e / #ffffff 写成所谓“第四版主按钮配色”，实际是用户指出突兀的旧第三版实色。真正第四版参数在 docs/milestones/m7/mockups/react-bits/mount.tsx：深色 tint #c3d2dd / opacity 0.16 / text #e5ebf0，浅色 tint #405b71 / opacity 0.12 / text #294155，相关适配在 adapt.css。

请继续当前初次实施，把任务01补齐：
- 整窗外壳、侧栏、工具条、页签/状态栏、设置、业务对话框、菜单/通知与操作按钮采用第四版同一套连贯玻璃材质，保留清晰内容与开发工具密度。不能只给顶栏加一层 blur、导入三个组件就称整套完成。
- 实际复用 GlassSurface、SpecularButton、GlassIcons，提炼少数材质容器 / 按钮适配，统一覆盖已有与新增控件；适合的入口使用 GlassIcons，现有语义图形仍可传入。不要给每个文件行或状态小图标建立独立 GPU 上下文，也不要改变既有安全交互。
- 按第四版恢复低饱和透明按钮，所有相关按钮要有一致的默认、hover、pressed、disabled、focus 状态，保留 Base UI 的焦点 / 键盘 / ref / native button 契约；不能以 pointer-events:none 代替键盘禁用。
- 代码 / diff 正文、行号、光标与增删标记继续清晰，保留不透明底色与同色系衔接；不是要求把文字模糊或把每块拆成互不协调的玻璃卡片。
- 正常能力下不能无条件关闭折射。核对实际 SVG / WebGL / 原生外壳路径，支持时启用，确实不支持或系统减少透明度 / 高对比度时明确降级；没有强折射实际证据就如实写未验，不把磨砂当作强折射通过。
- macOS 与 Windows 共享材质语义、平台外壳分别适配，不破坏 Windows 原生窗口操作。

请将此用户反馈和处理结果追加任务01 Comments、report，继续当前会话，按更新后的完整源码重跑受影响检查并固定候选。报告按区域列出实际材质覆盖及真实验证/降级情况，不能以组件名计数代替视觉一致性验收。Codex 之后依用户确认的第四版逐区域验收。


### Pi 实施处理（2026-10-30，初次实施续接）

按上述反馈补齐，参数统一收口到 `src/features/m7/glass/material.ts`（第四版 mount.tsx / adapt.css 的数值，深色 tint #c3d2dd/.16/text #e5ebf0，浅色 #405b71/.12/text #294155）：

- **整窗材质**：`GlassLayer`（新组件，包 GlassSurface）作为唯一材质容器，挂在主窗口外壳（`glass-host`）底层；顶栏 / 侧栏 / 活动栏 / 状态栏 / 页签行改为玻璃上的半透明层（`--glass-*-tint` 变量），不再各自一块实色。曾出现材质层占住 grid 第一行把顶栏挤下去的缺陷（smoke 布局核对 y=38/活动栏 h=22 证实），已用 `position:absolute !important` 修正并复测（`logs/smoke-final2.json` titlebar y=0、activityBar h=740）。
- **浮层与对话框**：popover（`revia-popper`）、`app-dialog`（通用 / 设置 / 克隆 / 诊断 / 更新）、通知气泡统一挂 `GlassLayer variant="popover"`，参数同第四版（borderRadius 7、折射路径开启）。
- **按钮**：新 `GlassButton` 适配统一默认 / hover / pressed / disabled / focus 状态，保留 native button 契约（disabled 属性、focus-visible outline、ref / aria-* 原样透传，不用 pointer-events 代替禁用）。主提交按钮走 `variant="primary" specular`（SpecularButton + 第四版低饱和透明填充）；toolbar-widget / icon-action / dialog-button 改用同一套 CSS 材质变量与状态。SpecularButton 每实例一个 WebGL2 上下文，`useSpecularSlot` 限 6 个，着色器只用于主要动作。
- **折射**：`GlassLayer` 不再写死 `refraction=false`——正常能力下开启（SVG 滤镜路径，WebView2/Chromium 可用；WKWebView 按组件内部探测降级磨砂）；`useFlatGlass` 监听 prefers-reduced-transparency / forced-colors，且探测 backdrop-filter 支持，降级时明确换实色并隐藏高光装饰（index.css 三个媒体查询块）。**强折射在 WKWebView 的实际渲染证据未取得**，按反馈要求如实记为未验，留任务 14 桌面核对。
- **代码区**：diff / 编辑器正文仍用不透明 `--editor-background`，行号、增删标记、光标不受材质影响；外壳与代码区用同色系衔接（`editor-tabs` 用 color-mix 过渡带）。
- **GlassIcons**：顶栏工具簇沿用；材质参数从 material.ts 取，主题切换同步。

修复过程中的一次真实缺陷与验证：
1. 材质层占位破坏 grid 布局（冒烟 layout 全部偏移、并排色带画不出）→ 原件 `logs/smoke-glass4..7.json` 保留，定位为 vendor `.glass-surface{position:relative}`（非 layered）覆盖 layered 规则。
2. 修正后全量复测：`cargo test` 669 通过（m6_flows 1 例 15s 超时属负载抖动，单独重跑通过，`logs/m6-retest.exit=0`）、`pnpm exec tsc -b` 0 错、`pnpm lint` 0 错、`x18n check` 0 错 65 预存警告、`pnpm build` 成功、隔离冒烟 `logs/smoke-final2.json` ok=true。
