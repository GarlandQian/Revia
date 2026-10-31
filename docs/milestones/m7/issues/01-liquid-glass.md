# 01：采用统一玻璃外观与系统材质（Q200）

Status: ready-for-human
Blocked by: None

## What to build

让既有审阅与提交界面采用已确认的连续玻璃外观，覆盖窗口、按钮、工具入口、浮层与平台外壳。

依据：规格 A；Q189–Q192、Q199、Q200、Q201；Q200 为当前材质与结构要求，以下历史 Comments 保留当时依据。

## Acceptance criteria

- [ ] macOS 26+ 整窗原生 Liquid Glass，工具栏分组胶囊靠左；外壳、日志、diff、终端与 PR 面板材质统一
- [ ] diff 代码面的中性半透明底能压住背景字形；语法色、行号、选区、增删标记清晰，缩放与侧栏拖动后仍对齐
- [ ] 设置页铺满整窗，浮动对话框和工具栏下拉菜单使用原生玻璃，关闭后原生区域正确撤除 / 恢复
- [ ] 真实桌面核对浅深色与系统降低透明度 / 动态效果；保留焦点、点击、键盘导航、拖动与滚动
- [ ] CSS 回退仍可读可操作；React Bits 固定来源与完整许可保留；Windows 11 Mica / Windows 10 回退接线完成，真机在 M8 验证
- [x] 日志引用标签独立一列且完整显示；分支图所有泳道、圆点和连线完整绘制，窄窗横向滚动，其他文字列可省略（Q201）
- [x] 相关 build / 类型 / lint 与分支图回归通过；真实材质与 D06 交任务 14，不凭浏览器预览标通过

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

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F01 P1 | “覆盖窗口”：玻璃层仅200×80，SSR确认 | src/features/m7/glass/GlassLayer.tsx:28；src/components/m7/glass/GlassSurface.tsx:355 |

## F01 [P1] 玻璃装饰层只有 200×80

规格 S64：“Glass Surface 用于窗口和浮层”；任务01原文“覆盖窗口、按钮、工具入口、浮层与平台外壳”；用户纠正要求“整窗外壳、侧栏、工具条、页签/状态栏、设置、业务对话框、菜单/通知与操作按钮采用第四版同一套连贯玻璃材质”。

源码 `src/features/m7/glass/GlassLayer.tsx:28–44` 不传 width/height；`src/components/m7/glass/GlassSurface.tsx:355–356,809–824` 默认200/80并输出行内样式；`src/index.css:1782–1789` 只覆盖 position/inset，未清除width/height。触发：正常打开主窗口或超过200×80的任意对话框。结果：唯一材质层固定200×80，无法覆盖整窗/浮层。`ui-probes.stdout` 三种variant均真实输出 `style="width:200px;height:80px;..."`，并保存SSR原件。这不是仅缺桌面证据，而是确定的尺寸接线错误；其余强折射及视觉一致性仍未验。

补充静态覆盖缺口：`src/index.css:995–1003` 的非layer `.revia-popper` 不透明背景优先于1841处layer内透明背景；通用 `src/components/ui/button.tsx:10–16` 沿旧实色。此处仅记录源码，不冒充像素判断。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Pi 正式返修处理 — 2026-10-31

- **F01**：`GlassLayer` 显式传 `width/height="100%"`，并加 `.glass-layer.glass-surface { width/height: 100% !important }`（vendor 默认 200×80 行内样式被清除）；`.revia-popper` 圆角/边框改玻璃语义。隔离冒烟 `logs/smoke-repair.json` titlebar y=0、activityBar h=740、windowLayer=true。强折射真机证据仍留任务 14。
- **F02**：主仓库分支补挂 CloneDialog / DiagnosticsDialog / UpdatePanel，Settings 传 worktreeId/updateStatus/onUpdateStatus/onThemesChanged；欢迎分支重复的第二个 CloneDialog 已删（standards smell 自然消除）。


### Codex：真实桌面场景集中修复与验证（2026-10-31）

固定 Release `5ca9178` / 产品`0dc11c0`已完成四主题及中英文900px真实查看；系统深色测试在finally中恢复原浅色。手动编辑Save前磁盘未变、Save后实际字节正确，草稿跨文件切换保留，外部变化后拒绝覆盖并保留草稿；原生退出按未解决草稿确认，先保留原件再仅丢弃fixture编辑，启动03真实exit0。本地clone实际目标`repos/native-clone-c_0_lf38/checkout`的HEAD与自建bare一致并自动登记打开。诊断原生导出的JSON不含文件正文标记与fixture邮箱。PR未登录和更新无签名公钥边界已观察，正式账号/保护/更新接线未验，D06尚未执行，不能将上述局部结果称整包通过。

原生发现并集中修复：更改列表view把元数据读取记成仓库写入，导致PTY逐字输入被拒及提交后Busy不能自动恢复；默认组未命名input冻结旧语言；设置玻璃透底文字干扰阅读。原始截图/AX/文件在`desktop-acceptance-myntirt_`，终端命令实际残缺并parse error，不能称PTY命令通过。

修复后元数据观察不再设置writing标志；真实写入开始/结束递增代际，只在同一空闲代际内发布元数据，读取不把仓库标为writing；最终发布的Mutex区仍包含身份重验证及其系统Git调用，可能短暂等待输入IPC，新版真实输入结果另行验证，跨写入结果拒绝。原写互斥与终端真实写入暂停保持。前端仅对明确Busy读取作三次有界重读，绑定原worktree/epoch且卸载取消；失败不继续显示可操作旧凭据。默认名用真实空name和即时翻译placeholder，避免翻译持久化成用户名称。统一浮层保留玻璃装饰/折射路径并增加主题tint与blur遮蔽透底，减少透明度/高对比度仍无滤镜降级。

`task14-native-scene-rust-01`真实exit0，共44通过（更改列表26、PTY3、会话3、共享提交12），含实际会话写代际与Barrier并发发布拒绝。`task14-native-scene-node-01`20通过，前端build/lint/x18n均exit0，翻译0错误/76既有unused警告。源码hash、argv、真实exit、分流原输出与失败原件保留；只改复用既有键，未改locale/绑定。下一步只审本新增量、固定新Release，再真实复验PTY/浮层/默认名与D06；当前不标任务14 done。


### Codex / review10 Standards (2026-10-31)

Standards：固定 `5ca917889efa5141442d0b73d6e48931828acf93...039fd1a3dbea7cf7e0c7bac91cd648509943dd41`，仅本轮10路径新增量，未读Spec。

硬违反0项；判断2项，均P3、仅backlog：

- S10-ST01，证据口径不一致：`docs/milestones/m7/report.md:303`及任务01:88、04:126、13:292、14:507称“持锁区不运行Git”，实际新增`session.rs:1624–1628`持有writing Mutex调用require_identity，经recovery.rs:188、160、122执行git rev-parse。按`docs/agents/local-validation.md:16`“每项结论都能对应固定源码、生产入口和实际可观察结果”，建议修正文档；没有另立“锁内禁Git”硬规范，未复现新输入丢失或输入延迟，不能据此升格P2。
- S10-ST02，possible Duplicated Code：`src/index.css:1848–1850`、1857–1859、1866–1868重复同一tint与两条blur声明，可合并选择器共用材质规则；无功能失败证据。

其余11项固定smell已逐项判断，无新增发现；仓库规则优先，工具强制项跳过。

已核固定diff、33项源码/规则快照、106项相关原件散列。原记录Rust44、node20通过，build/lint/x18n exit0，翻译0错误/76警告。Rust受测源码一致（CSS之后另改，最终前端快照一致）。未重跑、未启动桌面；新Release原生PTY/Busy/浮层/默认名、D06、Windows仍待规划方验证。两项P3按Q174不阻断验收。



### Codex / review10 Spec (2026-10-31)

Spec：固定 `5ca9178…039fd1a`，仅原生场景修复增量，10路径、1提交（Q174）。**新增发现0：P1/P2=0，P3=0。**

(a) 遗漏／部分实现：未发现。列表观察不再占用仓库写标志，真实写入前后代际校验阻止跨写发布；真实PTY输入仍按所属worktree的写门暂停。Busy读取最多重试三次，原epoch与卸载门禁保留，终态失败清掉列表凭据。

(b) 范围外行为：未发现scope creep。

(c) 实现错误：未发现。默认名称以空name保存，翻译只作即时placeholder；浮层增加主题tint／blur，减少透明度与高对比度仍移除滤镜。对应规格 `spec.md:68,90–93,133,138`、`ui-direction.md:191`；生产接线见 `changelists.rs:177–204`、`session.rs:1617–1653`、`Changelists.tsx:58–80,113`、`index.css:1846–1868,2010–2015,2035–2051`。

已核38项相关源码／规范、106项本轮原件及固定diff；Rust44、Node20、build／lint／x18n真实exit0，翻译0错误／76既有警告，受测Rust与最终前端源码对应。未重跑检查、未改产品或启动桌面；新Release原生复验、D06及Windows仍未验证。详见 `spec-evidence.md`。


### Codex：Q200 合并准备（2026-11-01）

用户已在 Q200 确认 macOS 26+ 使用系统原生 Liquid Glass，并要求下拉菜单与 Windows Mica 接线；本次把 m7-glass 合入 m7 后验证。规格 A 和当前验收项按该决定同步，历史第四版与 Pi 返修记录保留；旧勾选不作为新原生材质已验收的证据。命令检查、合并源码与真实桌面结论另记本轮验证记录，未完成项保持未勾选。

### Codex：原生玻璃合并两轴审查（2026-11-01）

固定候选 `262bd47`，基准 `b13b6a7`（三点 diff 的 merge-base 为 `88320fa`），只审本轮玻璃增量。Standards 轴 1 项 P2：新系统材质样式覆盖减少透明度的实底 / 无模糊回退。Spec 轴 2 项 P2：同一辅助功能回退缺口，以及设置页开启时隐藏后来打开的草稿 / 终端退出确认，可能出现不可见模态。各轴原文分别保存在 `/private/tmp/revia-m7-glass-integration-z3i4cgvx/review-standards.md`、`review-spec.md`；两轴原始计数分别保留。

前次正式 Pi 返修已完成，本轮按 Q113 由 Codex 接手：设置页隐藏规则排除全部原生 dialog，后续确认显示时淡出设置内容；辅助功能媒体规则在系统材质之后应用，使用主题实底或 Canvas、恢复描边并去掉网页模糊；媒体变化重新同步布局，solid 模式撤掉整窗材质和区域玻璃。检查、修复增量复审及桌面结果另见本轮验证记录，尚未据静态修正勾选桌面验收项。

### 用户：日志标签与分支图完整显示（2026-11-01，Q201）

用户在本轮真实桌面截图中指出引用胶囊被切断，要求“标签单独一列”“必须完全渲染 不要省略 其他列可以省略”，并补充“标签也不省略”。按 Q201 实施：引用列依据已加载引用的真实自然宽度展开，不裁切名称或胶囊；表头和行共享列定义，分支图两端留边，选择标记不占布局宽度；窄窗保留完整图和标签，使用横向滚动。其他文字列用省略号并保留全文提示。

### Codex：合并验证结果（2026-11-01）

玻璃已在 `95d0448` 合入 m7；辅助功能 / 模态修复 `24f7410`，Q201 日志列 `6e29f63`。后两轮 Standards / Spec 增量均 0 发现。最终类型 / Vite / Rust Release / app 打包 exit 0，日志49项通过、相关lint零告警；初轮Rust玻璃绑定6项、Node72项通过。真实桌面已观察原生和CSS回退diff、仓库/worktree菜单、设置页上的终端退出确认及取消、日志完整图/长标签/横向滚动，100%与150%切换后恢复100%。详情、固定源码边界、截图和未验项见 [原生玻璃合并验证](../glass-integration.md)。整窗桌面折射、代码遮蔽、系统深色/辅助功能实机、全组浮层、resize、900px与D06仍未完，不据局部通过标done。原有6个未提交文件逐字节保留。
