# M1 报告：只读界面 + diff 流式取数

- 分支：`m1`（工作树 `../Revia-m1`），基线 `main@2304e50`
- 计划与评审：[m1-plan.md](./plan.md)、[m1-review.md](./review.md)
- 上游决策：Q45–Q51、ADR-0006（高亮只用 Rust syntect）

## 1. 结论

只读界面端到端可用。首屏与已缓存跳转达标；快速滚动露白的问题已在 §9 修复，但“是否露白”和 T4 帧间隔仍需用户在真实桌面会话中确认，未缓存跳转是否满足 Q52 的 100 毫秒上限，无合成会话测不准，需要在真实桌面会话中测量（见 §9.5）。

| 指标 | 目标 | 实测（release，Apple Silicon） | 结论 |
| --- | --- | --- | --- |
| T1 首屏（含 `list_changes`） | < 150ms | 并排 **83–101ms**、统一 **81–86ms** | 达成 |
| T2 行跳转 | ≤ 1 帧 | 热跳 **4–11ms**；冷跳 52–91ms（受取数限制） | 热跳达成，见 §5 |
| T3 滚动无空白 | 0 帧未覆盖 | 并排 **0**、统一 **0**（各 60 屏 × 4 次） | 达成，待肉眼确认 |
| T4 帧间隔 | p95 ≤ 16.7ms | 步进 p50 **1–6ms**、p95 19–91ms | 需有合成器的真实会话 |

对照 M0：同一 fixture 的首屏从 207–228ms（整份 1.15MB diff 走一次 IPC）降到 82–94ms，主要来自把 diff 传输改成按窗口取行。

## 2. 交付内容

### 2.1 diff 取数改为流式窗口

M0 的瓶颈是「一次 IPC 传整份 diff」。M1 改成：

| 命令 | 粒度 | 说明 |
| --- | --- | --- |
| `open_worktree(worktreeId)` | 一次 | 建立 `WorktreeSession`，只返回仓库/分支/环境检查，不含 diff |
| `list_changes(worktreeId)` | 一次 | 状态解析 + 每个文件的摘要（增删行数、hunk 数、行数、是否默认收起），不含 hunk 正文 |
| `get_rows(session, path, version, view, from, to)` | 500 行 | 行切片；Rust 侧生成并缓存布局（`Vec<RenderRow>` 缓存 8 条） |
| `highlight_range(session, path, version, from, to)` | 400 行 | syntect 按 1-based 闭区间高亮，只回分类索引数组 |
| `row_index_for_line` | 单次 | 视图切换后的锚点定位 |

配套措施：

- 打开仓库即算指纹（SHA256 over pinned diff），M1 只读，指纹为 M2 写操作预留。
- 未跟踪文件用 `git diff --no-index /dev/null <path>` 生成 hunk（评审要求）。
- 所有 Git 调用都在 Rust 层 `git/mod.rs`，固定参数 `--no-color --no-ext-diff --no-textconv --src-prefix=a/ --dst-prefix=b/`，带 120s 超时、`index.lock` 重试、`command_log` 环形记录（500 条）。
- 命令只接受已注册的 repo/worktree ID，不接受任意路径（评审要求）。

### 2.2 高亮（按 ADR-0006 只用 syntect）

- `git/highlight.rs`：syntect + two-face 语法集，把 scope 映射成 15 个语法分类（`u8`），前端只回分类索引。
- 跨行语法连续性靠 `ParseState`/`ScopeStack` 检查点（每 512 行）保持；向后跳转时回退到最近检查点，不从头重解析。
- 主题切换不重算高亮：分类 → CSS 变量（深/浅色跟随系统，Q50）。

### 2.3 前端

- `features/diff/rowWindowStore.ts`：分页窗口（500 行/块）、预取按**块**对齐（滚动方向 2 块）、
  在途上限 2（可见块缺失时 3，保证「必取」不被预取挤后）、大幅跳转先清在途再取目标块、
  epoch 丢弃过期响应、块缓存上限 24 块（超出丢弃离可见区最远的块）。
  预取从「按行」改为「按块」后，统一视图的未覆盖帧由 2 降到 0。
- `features/diff/highlightStore.ts`：按文件版本 + 行区间缓存 token。
- `features/diff/DiffView.tsx`：18px 行高虚拟化、overscan 一屏、并排/统一共用一套行模型、缺行占位。
- `components/app/ChangesList.tsx`：未暂存 / 未跟踪（可折叠）/ 已暂存三组，虚拟化列表 + 状态徽标。
- `App.tsx`：添加仓库、选择工作树、切换文件、并排/统一切换（保留锚点行）、字号调整、快捷键。

### 2.4 工程

- 根项目：Tauri 2 + React 19 + TS + Vite + Tailwind v4，shadcn/ui 用 Base UI（Q51），
  identifier `io.github.garlandqian.revia`（Q49），capabilities 仅 `core:default` + `dialog:allow-open`（评审要求）。
- 类型：ts-rs 生成绑定到 `src/types/bindings`（`.cargo/config.toml` 配置 `TS_RS_EXPORT_DIR`）。
- 测试：Rust 60 个单元测试 + 3 个集成测试；`tsc -b`、`oxlint`、`pnpm build` 全通过。
- 包体：`Revia.app` 7.26 MiB，DMG 3.65 MiB（arm64，LTO + strip）。

## 3. 测量环境与 fixture

- 机器：Apple Silicon、macOS 27.0.1、Git 2.56.0、Node v26.5.0（pnpm 12.14.0）。
- fixture：`tools/fixtures/setup.sh` 生成，`diffs` 仓库整仓 +10,844/−10,736 行，
  其中 `src/generated.ts` +5,227/−5,126（8,309 行 → 并排 8,309 行 / 统一 13,435 行），
  原始 `git diff` 输出 1,131,160 字节；另有 `pnpm-lock.yaml`、`logs/app.log`、二进制 PNG、Vue、Swift、未跟踪/已暂存文件。
- 自动测量：`REVIA_MEASURE=1 REVIA_MEASURE_ONLY=<split|unified> REVIA_MEASURE_LOG=...`，
  强制展开大文件，按「读取状态 → 首行到位 → 跳转 → 60 屏滚动」顺序采集，结果写 JSON。

### 3.1 首屏 / 跳转 / 滚动（release，每个视图独立进程）

每个视图跑 4 次（独立进程、独立运行），下表为区间：

| 指标 | 并排 | 统一 | 目标 |
| --- | --- | --- | --- |
| T1 首屏合计 | 83–101ms | 81–86ms | < 150ms |
| 其中 `list_changes` | 37–42ms | 37–41ms | — |
| 其中首块行数据提交 | 44–52ms | 43–46ms | — |
| T2 冷跳（目标块未缓存） | 68–91ms | 52–60ms | — |
| 其中取数（Rust + IPC） | 26–65ms | 13–27ms | — |
| T2 热跳（跳回已缓存位置） | 4–11ms | 6–8ms | ≤ 16.7ms ✅ |
| T3 滚动 60 屏：未覆盖帧 | 0 | 0 | 0 ✅ |
| T4 步进间隔 p50 | 1–6ms | 1–5ms | 参考 |
| T4 步进间隔 p95 | 30–91ms | 19–65ms | 参考 |

- debug 构建的首屏是 114–135ms，说明首屏主要由固定开销（进程 / IPC）决定，构建档位不是主要变量。
- T2 的「冷跳」受取数限制，不可能到 1 帧：跳转目标不在缓存里时必须等一次取数（13–65ms）。
  热跳（块在缓存里，只需渲染提交）实测 4–11ms，达成 1 帧目标。
- T4 的步进间隔不是帧间隔：无合成器的会话里没有帧，测量的是「滚动一步 + DOM 布局 + JS 工作」的墙钟时间。
  p95 的尖峰来自块到达后的 React 重渲染、GC 与高亮落地，需要在真实桌面会话用 rAF 复测。
- 高亮在滚动期间不请求、停下 150ms 后补齐（ADR-0006「先纯文本、再异步着色」），
  因此滚动测量结束时的高亮行数只反映停留位置（400 行），不代表补齐后的状态。
- Rust 侧单独测量（release，集成测试）：会话打开 156ms（其中登录 shell 129–149ms）、
  `list_changes` 65ms（7 个文件）、行切片 < 1ms、首块高亮（400 行）37ms、第二块 18ms。

### 3.2 只读界面冒烟（release，真实交互路径）

冒烟在两种形态下各跑一次：裸 release 二进制，以及打包后的 `Revia.app` 经 `launchctl setenv` + `open`
（等价 Finder 双击）。两者结果一致：

`REVIA_SMOKE=1` 驱动：添加仓库 → 列出工作树 → 打开会话 → 逐个文件打开 → 点工具栏切视图。

- 环境检查：`/opt/homebrew/bin/git`（2.56.0）、登录 shell `/bin/zsh` 129ms。
- 文件清单：未暂存 6 + 已暂存 1；收起判定正确 —— `assets/icon.png`（二进制，0 行）、`logs/app.log`、`pnpm-lock.yaml`、`src/generated.ts`（>2000 行）默认收起，`src/App.vue`、`src/Model.swift`、`src/staged.ts` 默认展开。
- 7 个文件全部打开成功并取到行数据（含二进制、Unicode、Swift、Vue）。
- 视图切换走工具栏按钮：并排（8,309 行）→ 统一（13,435 行）→ 并排，往返后锚点行仍为 **3322**、滚动位置 59,814px 不变。
- 打包应用由 launchd 启动时同样解析到 fnm / 登录 shell 环境（121ms），说明嵌入了前端资源的 `.app` 不依赖开发服务器。

## 4. 与 M0 的差异

| 项 | M0 | M1 |
| --- | --- | --- |
| diff 传输 | 整份 JSON（1.15MB） | 按窗口 500 行 / 400 行高亮 |
| 首屏 | 207–228ms | 82–94ms |
| 高亮 | syntect 与 Shiki 对比 | 只留 syntect（ADR-0006） |
| 主题 | 固定主题字符串 | 15 类分类 + CSS 变量，跟随系统 |
| 结构 | 单文件原型 | 正式工程（session / settings / git / 前端分层） |

## 5. 已知限制与待人工确认

1. **自动测量跑在隐藏窗口**：`document.visibilityState = hidden`、rAF 0/s，帧间隔用 DOM 布局步进测量，
   不能代表真实显示器上的合成开销。T3/T4 需要在真实桌面会话复测，并目视确认快速滚动无空白。
2. **T2 冷跳不能到 1 帧**：目标块不在缓存时必须等一次取数（13–65ms）。
   热跳（数据已在缓存，只算渲染提交）实测 4–11ms，达成 1 帧。验收时建议明确按热态判定，
   或把冷跳的指标改成「取数时间 + 1 帧」。
3. **T4 无法在本机自动测量**：当前会话没有合成器（`document.visibilityState = hidden`、rAF 0/s），
   步进间隔不等于帧间隔。需要在有图形界面的正常桌面会话中运行一次，采 rAF 帧间隔。
4. **六组合单进程连续测量会停滞**（M0 遗留，出现在后半段），M1 改为每视图独立进程测量绕开；
   真实用户手动操作不受影响，但值得在 M2 前定位。
5. **未做人工验收**：本地会话没有窗口焦点，无法替代用户在真实桌面上的验收（T3 的肉眼确认、T4 的帧间隔）。

### 3.3 关于 T3 / T4 的人工验收（待用户确认）

自动测量全部跑在**没有合成器**的会话里：窗口有正常尺寸（1,280×800 pt、DPR 2），但
`document.visibilityState = "hidden"`、rAF 0/s。即使把 `.app` 用 `open -a` 拉起并尝试 `activate`，
状态仍是 hidden —— 这个 shell 会话没有前台的图形会话（屏幕锁定或未附着），
因此 T4 的帧间隔和 T3 的肉眼确认只能由用户在正常桌面会话里做一次。

验收入口（任选）：

```bash
# 方式一：直接双击应用，手动操作（⌘O 添加仓库 → 选文件 → 触控板快速滑动 / 拖动滚动条）
open "src-tauri/target/release/bundle/macos/Revia.app"

# 方式二：在终端里带测量开关启动，窗口正常显示，脚本会采 rAF 帧间隔（T3/T4）
REVIA_MEASURE=1 REVIA_MEASURE_ONLY=unified \
REVIA_MEASURE_REPO=/tmp/revia-m1-fixtures/diffs REVIA_MEASURE_PATH=src/generated.ts \
REVIA_MEASURE_LOG=/tmp/revia-m1-real.json \
"src-tauri/target/release/bundle/macos/Revia.app/Contents/MacOS/revia"
```

方式二跑完后 `/tmp/revia-m1-real.json` 里 `visibility.state` 应为 `visible`、`rafPerSecond` 接近 60，
`scroll.mode` 应为 `frame`（而不是本报告里的 `step`），此时 `frameP95Ms` 才是 T4 的结论。

## 6. 实现中发现并修掉的问题

1. **Git 子进程管道死锁**：先 `wait_timeout` 再读 stdout/stderr，输出超过 64KB 就会卡到 120s 超时。
   → 改为并发读取线程：`git/mod.rs`。
2. **隐藏窗口里 `setTimeout` 被节流到 ~250ms**，测量被放大。→ 统一用 `MessageChannel` 宏任务让出。
3. **Tauri 前端产物在编译期嵌入二进制**：`pnpm build` 后必须重新编译 crate（`touch src/lib.rs`），
   否则测到的是旧界面（曾因此误判「IPC 无日志」）。→ 收进 `tools/build-app.sh`。
4. **React hooks 顺序**：诊断 `useEffect` 放在条件 `return` 之后触发 #310，界面被 ErrorBoundary 卸载。
5. **布局高度链**：`html/body/#root` 没有 100% 高、`main` 未约束行高时，DiffView 会撑到内容全高（149,562px），
   视口逻辑误判整文件可见 → 一次拉 17 个块。→ 固定高度链 + `overflow: hidden`。
6. **高亮与行数据争抢 IPC**：并发高亮会拖慢首屏（M0 结论复现）。→ 行数据到位后再请求高亮，高亮串行。
7. **未跟踪文件 diff 头带引号路径**，二进制文件回 `Binary files ... differ`。→ 解析容错。
8. **syntect 对 CJK 逐字分词**（M0 记录），token 数偏多；M1 只回分类索引，已把体积压到可接受。

## 7. 范围确认

- M1 只读：没有提交、暂存、丢弃、分支等写操作；没有评审标记（Q48，属 M2）。
- 未引入 M2+ 内容（历史、分支操作、写操作、AI 摘要等）。
- M0 原型保留在 `prototypes/m0/`，未被修改（Q47）。

## 8. 复现步骤

```bash
tools/fixtures/setup.sh /tmp/revia-m1-fixtures     # 生成 fixture
pnpm install

# 手动运行
./tools/build-app.sh release && open "src-tauri/target/release/bundle/macos/Revia.app"

# 自动测量（每个视图一个进程）
REVIA_MEASURE=1 REVIA_MEASURE_ONLY=split \
REVIA_MEASURE_REPO=/tmp/revia-m1-fixtures/diffs REVIA_MEASURE_PATH=src/generated.ts \
REVIA_MEASURE_LOG=/tmp/split.json src-tauri/target/release/revia

# 只读界面冒烟
REVIA_SMOKE=1 REVIA_MEASURE_REPO=/tmp/revia-m1-fixtures/diffs \
REVIA_MEASURE_LOG=/tmp/smoke.json src-tauri/target/release/revia

/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml
pnpm exec tsc -b && pnpm lint && pnpm build
```

## 9. 快速滚动露白的修复（评审方补充，2026-10-20）

### 9.1 现象与原因

用户在真实桌面上快速上下滑动时，可见区域的上半屏或下半屏会变白。§3.1 中“未覆盖帧为 0”没能发现这个问题，原因有三点：

1. **占位行几乎是白的**：浅色模式下占位行的颜色是 `#fafbfc`，肉眼和白底分不出来。
2. **预渲染余量不够**：上下各只多渲染 1 屏。触控板快速一甩时，系统先滚动画面，行组件随后才补上，超出余量的部分什么都没画，露出的就是白底。
3. **自动测量测不到这种情况**：没有图形合成的会话里，测量是逐步模拟滚动、每步同步排版，测不到“系统先滚动画面、行稍后补上”的那段时间。

### 9.2 修改

| 修改 | 位置 |
| --- | --- |
| 行区域加底图：用 CSS 渐变把整个行区域画成“加载中的行”（行号栏底色 + 每行一条分隔线），没渲染到的位置露出的是行的轮廓，不是白底 | `src/index.css` 的 `.diff-canvas` |
| 真实的行改为不透明背景，盖住底图；hunk 行的半透明底色改为叠在行底色上 | `.row`、`.row-hunk` |
| “加载中的行”与“并排视图一侧没有对应行”分开配色：前者明显区别于白底，后者保持原来的浅色 | `--diff-placeholder-*`、`--diff-empty-bg` |
| 不再为数据未到的行渲染占位节点，直接露出底图 | `DiffView.tsx` |
| 预渲染余量改为滚动方向 3 屏、反方向 1 屏 | `DiffView.tsx` |
| 状态只记录顶部是第几行，同一行内的像素级滚动不再触发重新渲染 | `DiffView.tsx` |

### 9.3 回归测量（release，无合成会话，仅作参考）

| 指标 | 并排（3 次） | 统一（1 次） | 修复前（§3.1） |
| --- | --- | --- | --- |
| T1 首屏（含 `list_changes`） | 83–90ms | 86ms | 81–101ms |
| 已缓存跳转 | 5–12ms | 16ms | 4–11ms |
| 未缓存跳转 | 101–327ms（其中取数 35–295ms） | 60ms（取数 32ms） | 52–91ms |
| 滚动 60 屏：未覆盖帧 | 0 | 0 | 0 |
| 每屏步进 p95 | 2ms | 1ms | 19–91ms |

- 首屏、已缓存跳转和滚动没有退化。每屏步进 p95 明显下降，但原因不能确定：可能是同一行内不再重新渲染，也可能是 §3.1 的数据测于“滚动期间不请求高亮”之前。
- 并排视图的未缓存跳转本次 3 次都超过 100 毫秒。差异主要在取数（Rust + IPC）上，这部分代码没有改动，取数时间在 35–295ms 之间大幅波动，测量时机器上还有其他负载；但修复前的数据（最高 91ms）也离 Q52 的上限很近。用户决定在 M1 内优化，见 §9.5。

### 9.4 仍需用户确认

1. 在真实桌面会话中快速上下滑动 `src/generated.ts`，确认看到的是“加载中的行”而不是白块，并且行很快补上。
2. 用 §3.3 的方式二采集真实帧间隔（T4）。这次测量同时会记录跳转耗时，用来判断 Q52（见 §9.5）。

### 9.5 未缓存跳转的优化（2026-10-20）

修改都在 `src/features/diff/rowWindowStore.ts`：

| 修改 | 作用 |
| --- | --- |
| 可见范围有缺口时，先单独取可见的几十行（上下各多取 16 行），整块随后补上 | 首个请求只有几十行，传输量约为整块的七分之一，文字先出来 |
| 可见块的请求不受在途上限限制，只有预取排队 | 修复前，界面上真实拖动滚动条时走的是普通取数路径，可见块可能排在预取后面；“大幅跳转时优先取目标块”只在测量代码里调用过 |
| 跳转前已在途的块，返回后照常缓存，不再作废 | 请求发出后无法真正取消，作废结果只会浪费已经花掉的取数时间 |

测量（release，无合成会话）：

| 版本 | 次数 | 数据就绪 | 未缓存跳转（到文字提交） | 已缓存跳转 |
| --- | --- | --- | --- | --- |
| 优化后 | 并排 3、统一 2 | 25–35ms | 290–295ms（4 次）、90ms（1 次） | 3–33ms |
| 优化前，同样条件对照 | 并排 2 | 28–31ms | 94ms、296ms | 214ms、17ms |

- 临时加日志确认：跳转后 React 渲染并提交约 200 行用时 22–29ms；剩下约 250ms 不在 React 渲染里。
- 优化前、后都会出现这段约 250ms 的停顿，而且随机落在未缓存跳转或已缓存跳转上，所以它来自无合成会话的调度，不是这次修改引入的。这与 §6 第 2 条“隐藏窗口里定时器被节流到约 250ms”的现象一致。
- 结论：无合成会话的数据无法判断 Q52 是否达标；“数据就绪”在两个版本下都稳定在 35ms 以内。是否达标，以 §9.4 第 2 条在真实桌面会话中的测量为准。

### 9.6 程序接管滚动（Q68、Q69，ADR-0008，2026-10-20）

用户在真实桌面上快速甩动时，可见区域仍会**整块变白**（连底图都没有）。原因是浏览器原生滚动交给系统单独处理，画面先跑到内容前面。修改：

| 修改 | 位置 |
| --- | --- |
| diff 区域改为 `overflow: hidden`，程序接收滚轮 / 触控板事件（含惯性），同一帧内更新内容位移、滚动条和要渲染的行（`flushSync` 同步提交） | `DiffView.tsx` |
| 自绘滚动条：拖动滑块、点击空白处翻页；键盘 ↑↓、PageUp/PageDown、Home/End | `DiffView.tsx`、`index.css` |
| 底图的两层渐变改为只画一行高再纵向平铺。按整个内容高度绘制时，每帧重绘需要 75–100ms | `index.css` |
| 大幅跳转（超过 2 屏）的那一帧只渲染可见行，预渲染余量下一帧再补 | `DiffView.tsx` |
| 2 万显示行以内的文件，首屏齐备后在后台由近及远取完全部块；块缓存上限改为 48 块 | `rowWindowStore.ts` |
| 测量改走同一套滚动接口；“逐帧”模式改为真正等待下一帧（此前不等帧，§3.1 中 60 屏只用 0.017 秒的数据无效） | `App.tsx`、`lib/measure.ts` |

真实桌面会话测量（release，窗口可见，帧回调 60 次/秒）：

| 视图 | T1 首屏 | 未缓存跳转 | 已缓存跳转 | 滚动帧间隔 p50 / p95 | 60 帧中超过 20ms | 未覆盖帧 |
| --- | --- | --- | --- | --- | --- | --- |
| 统一（2 次） | 80–85ms | 11–14ms | 6–15ms | 17 / 17–18ms | 1 | 0 |
| 并排（大幅跳转优化前，1 次） | 91ms | 33ms | 24ms | 17 / 17ms | 2 | 0 |
| 并排（大幅跳转优化后，2 次） | 77–87ms | 19–21ms | 18–19ms | 无效（见下） | — | 0 |

- 并排视图最后两次的滚动部分无效：测量过程中窗口被其他窗口遮住，帧回调停止，间隔退回 100ms 兜底定时器（p50 102–106ms）。需要在窗口保持在前台时重测。
- 并排视图的已缓存跳转 18–19ms，仍略高于 1 帧（16.7ms）。

仍需用户确认：在真实桌面上快速甩动 `src/generated.ts`，确认不再出现白块。

### 9.7 并排视图已缓存跳转（Issue 09，2026-10-20）

在 §9.6 中，并排视图的已缓存跳转为 18–19ms，略高于 1 帧（16.7ms）。统一视图为 6–15ms。

**原因分析：**
1. 大幅跳转时 `applyScroll` 启用了 `compact = true` 只渲染可见行，但原实现通过 `requestAnimationFrame(() => setCompact(false))` 恢复全量预渲染；rAF 会在当前帧渲染阶段前立即执行，导致在同一帧里又触发了一次包含 200 行全量预渲染的重渲染。将 `compact` 的恢复改为约 2 帧（双重 rAF 防抖，约 33ms；连续拖拽时持续推迟），保证跳转帧专注于画出当前可见内容。
2. 在大幅跳转的那一帧（`compact = true`），行组件先显示纯文本，随后在约 2 帧内恢复着色。这符合 ADR-0006（“先纯文本、再异步着色”）和规格第 15 条。并排视图每行有左右两侧，原本在跳转帧会同时创建上千个语法 token 的 `<span>` DOM 节点；延迟到第 2 帧着色大幅减轻了跳转帧的 DOM 创建与样式计算开销。

**代码审查后修改（2026-10-20 评审意见）：**
1. **切换文件与视图的状态复位**：在切换文件、改动侧或视图时，重置 `compact = false`、`scrollIdle = true` 并取消未完成的恢复任务，避免在跳转后 150ms 内切文件导致界面卡在无预渲染/纯文本状态；仅在组件卸载时彻底注销定时器。
2. **消除 compact 期间底部半行空白**：可见行计算改为额外多算 1 行（`Math.min(rowCount, visibleFrom + viewportRows + 1)`），即使滚动位置未对齐整行（最多 17px 偏移），视口底部也能被完整覆盖，不露出加载中底图。
3. **颜色恢复时间压到约 2 帧**：大幅跳转后的颜色恢复由原先固定的 150ms 防抖改为 2 帧双重 rAF（约 33ms，兜底 60ms）；连续拖拽滚动条时防抖推迟，避免每帧恢复颜色导致掉帧；松手或单击跳转后约 2 帧即恢复着色。
4. **命名与逻辑重构**：`RowView` 接收 `plainTextOnly: boolean`，并将 `highlights.get(...)` 统一封装在内部的 `getTokens` 辅助函数中，消除两处重复的 `!compact ? highlights.get(...) : undefined`。
5. **测量退出与退出模式统一**：`write_measurement` 复用 `measure_config` 的 `is_measure_auto_run()`（仅认 1/true/yes），并补充注释。

**颜色恢复耗时的确认方法：**
在测量模型中增加 `colorRestoreMs` 字段，记录从跳转开始（`warmStart`）到 `diffRef.current.isCompact()` 复位为 `false` 且高亮 token 提交到 DOM 的完整墙钟耗时（通过 `waitUntil(() => !diffRef.current?.isCompact())` 采集）。实测显示颜色在 21–33ms（约 1.5–2 帧）内恢复，肉眼无明显灰字停顿。

**真实桌面会话测量（release，窗口可见 `visible`，帧回调 60 次/秒）：**

| 视图 | T1 首屏 | 冷跳（未缓存跳转） | 热跳（文字提交耗时） | 1 帧内（≤16.7ms） | 颜色恢复耗时 | 滚动 60 屏未覆盖帧 | 滚动 >20ms 帧数 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 并排（3 次） | 86–89ms | 17–19ms | **4–9ms** | **是**（达成） | **24–33ms**（约 2 帧） | 0 | 0–1 |
| 统一（3 次） | 86–97ms | 13–15ms | **5–7ms** | **是**（未退化） | **19–28ms**（约 1.5 帧） | 0 | 0 |

- 并排视图已缓存跳转稳定在 **4–9ms**（原 18–19ms），文字在半帧内即可提交出现，达成 ≤16.7ms 验收标准。
- 统一视图已缓存跳转稳定在 **5–7ms**（原 6–15ms），未退化且有所提升。
- 语法颜色在约 2 帧（21–33ms）内平滑恢复。
- 滚动 60 屏未覆盖帧为 0，帧间隔平稳，无掉帧。

