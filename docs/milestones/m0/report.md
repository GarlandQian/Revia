# M0 原型报告

更新日期：2026-10-19。状态：M0 完成。实施按 [m0-plan.md](./plan.md) 与 [m0-review.md](./review.md)（用户已同意）执行。

## 结论摘要

| 项目 | 结果 |
| --- | --- |
| 首屏（1 万行改动量级） | **207–228 ms**，达标（标准 300 ms 以内） |
| 滚动 | 每重建一屏（853 px，约 47 行）的主线程耗时 **p50 0–1 ms、p95 1 ms、max 2 ms**；用户人工确认不卡顿，但快速滚动时上方或下方会短暂出现空白（见第六节） |
| 视图切换 | 并排 ↔ 统一：顶部锚点行号一致，滚动位置偏移 **0 px**，达标 |
| lock / 二进制 / 超大文件 | 默认收起已验证（见下文实测记录） |
| 语法高亮 | **推荐 syntect**（Rust 侧，分块返回）；Shiki 仅在改用 Oniguruma 引擎后可用 |
| 运行环境 | 登录 shell 解析 116–204 ms；husky hook 与 `git mergetool --tool=vscode` 在剥离环境与 Finder 启动两种方式下全部通过 |

一句建议：**首版用 Rust 侧 syntect，前端只做渲染**。Shiki 的默认（Oniguruma/WASM）能在 WebView 跑出可用速度，但它的 JS 正则引擎在同一 WebView 里对 8 千行文件 35 秒没有返回；Shiki 还要额外背 Web Worker 与约 1.9 MB 的 worker 产物。syntect 的代价是 Rust 侧解析状态缓存和分块协议，这部分原型已经跑通。

## 一、测量环境与方法

| 项目 | 值 |
| --- | --- |
| 机器 / 系统 | Apple Silicon（arm64），macOS 26.0.1（build 26A434） |
| Git | `/opt/homebrew/bin/git`，`git version 2.56.0`（来自 worktree 解析出的登录 shell PATH） |
| Rust / cargo | 1.99.1（Homebrew） |
| Node / pnpm | v26.5.0 / 12.14.0 |
| Tauri | `tauri` 2.12.4、`tauri-build` 2.7.3、`@tauri-apps/cli` 2.12.3 |
| 前端 | React 19.3、Vite 8.3.4、TypeScript 6.0.3 |
| 高亮 | Shiki 4.5.0（`createHighlighterCore` + `shiki/langs/*`），syntect 5.3 + two-face 0.5.2（bat 的语法与主题集），主题 `Solarized (light)` / `github-light` |
| 构建 | **release**（`cargo build --release`，`lto = true`）；窗口 1440×928 逻辑像素、`devicePixelRatio = 2` |
| 测量入口 | 应用内测量面板；`REVIA_M0_MEASURE=1` + `REVIA_M0_ONLY=<mode>:<view>` 让 GUI 自动跑指定组合并把结果写成 JSON |

测量样例：`fixtures/setup.sh` 生成的 `diffs` 仓库，`src/generated.ts` 为 8309 行的 hunk（8 207 行旧版本 + 8 308 行新版本，共 16 515 行参与高亮），整仓改动 10 844 行新增 / 10 736 行删除。diff 数据的 JSON 字符数 1.15 MB。

**测量限制（重要）**：本机当前会话没有可用的图形合成（`document.visibilityState = "hidden"`，`requestAnimationFrame` 回调数为 0/s），窗口内容不会真正上屏。因此：

- 首屏耗时以「发起请求 → 数据到达 → React 提交 → 强制同步布局读到视口尺寸」为准，**不含合成器上屏时间**；
- 滚动测量退化为「逐屏推进 + 同步布局」，测的是**每一屏重建与排版的主线程成本**，不是合成帧间隔；
- 「滚动不卡顿」这一条**尚未在真实合成帧下验证**，需要在有图形会话的 Mac 上人工确认（见第 6 节）。

测量面板会把窗口可见性、rAF 次数、滚动模式一起记录，便于区分这两种情况。

## 二、首屏耗时（三段拆分）

每个组合单独启动一次 release 应用，取自动测量结果。

| 组合 | Rust 解析 | IPC 传输 | 前端渲染 | 首屏合计 | 行数 | 高亮额外耗时 |
| --- | --- | --- | --- | --- | --- | --- |
| 无高亮 / 并排 | 13 ms | 168 ms | 43 ms | **211 ms** | 8 309 | — |
| Shiki / 并排 | 13 ms | 187 ms | 41 ms | **228 ms** | 8 309 | 822 ms |
| syntect / 并排 | 12 ms | 164 ms | 44 ms | **208 ms** | 8 309 | 1 721 ms |
| 无高亮 / 统一 | 12 ms | 164 ms | 44 ms | **208 ms** | 8 309 | — |
| Shiki / 统一 | 12 ms | 165 ms | 42 ms | **207 ms** | 8 309 | 823 ms |
| syntect / 统一 | 13 ms | 168 ms | 43 ms | **211 ms** | 8 309 | 1 930 ms |

- **验收标准「1 万行改动的 diff 在 300 毫秒内显示出第一屏」达标**：最慢 228 ms。
- 首屏的瓶颈是 **IPC**（占 73–82%）：Rust 解析只要 12–13 ms，前端渲染 41–44 ms，剩下 164–187 ms 花在「Rust 序列化 → WebView 反序列化」。1.15 MB 的 JSON 换来约 7 MB/s 的有效吞吐，M1 之后如果要压首屏，应该先压这个环节（更紧凑的编码，或按需分页返回文件）。
- 高亮**不进入首屏路径**：先提交纯文本，再异步着色，所以三档高亮的首屏时间几乎一致。着色到达后的重渲染耗时（`rerenderMs`）实测为 0 ms（可见区域只有约 47 行）。

## 三、滚动与视图切换

| 组合 | 推进屏数 | 每屏渲染 p50 / p95 / max | 推进总耗时 | 切换后顶部锚点行 | 滚动位置偏移 |
| --- | --- | --- | --- | --- | --- |
| 无高亮 / 并排 | 120 | 0 / 1 / 2 ms | 35 ms | 4153 → 4153 | 0 px |
| 无高亮 / 统一 | 120 | 0 / 1 / 1 ms | 26 ms | 4153 → 4153 | 0 px |
| Shiki / 并排 | 120 | 0 / 1 / 1 ms | 61 ms | 4153 → 4153 | 0 px |
| Shiki / 统一 | 120 | 0 / 1 / 1 ms | 39 ms | 4153 → 4153 | 0 px |
| syntect / 并排 | 120 | 1 / 1 / 2 ms | 62 ms | 4153 → 4153 | 0 px |
| syntect / 统一 | 120 | 0 / 1 / 1 ms | 40 ms | 4153 → 4153 | 0 px |

- 虚拟列表只渲染可见区域加 24 行 overscan（行高 18 px）。每推进一屏（853 px ≈ 47 行）重建 DOM 并强制布局，主线程成本 p95 = 1 ms、最大 2 ms，**高亮开着也不变**：token 是按行预先算好的，渲染时只做查表和内联样式。
- 视图切换用「先记住顶部有行号的行 → 切换到目标视图 → 把它放回顶部」的方式保持位置。实测切换前后锚点行号一致、`scrollTop` 偏移 0 px，**验收标准「并排和统一视图来回切换时停在同一位置」达标**。
- 切换到统一视图时行数会更替，因此切换实现里对统一视图先用顶部定位再由布局效应校正，避免一次性把两倍行数推到布局里（这一条是实现细节，已记录在代码注释里）。

### 默认收起（实测记录）

```
文件 assets/icon.png：hunk 0 / 行 0 / 二进制 true / 默认收起
文件 logs/app.log：hunk 1 / 行 10000 / 二进制 false / 默认收起
文件 pnpm-lock.yaml：hunk 1 / 行 3003 / 二进制 false / 默认收起
文件 src/App.vue：hunk 1 / 行 23 / 二进制 false / 默认展开
文件 src/Model.swift：hunk 1 / 行 22 / 二进制 false / 默认展开
文件 src/generated.ts：hunk 1 / 行 13434 / 二进制 false / 默认收起
```

规则：二进制、lock 文件（`*.lock` / `pnpm-lock.yaml` / `package-lock.json` / `yarn.lock`）、行数超过 2000 的文件默认收起；实测符合预期。

## 四、Shiki 与 syntect 对比

### 4.1 性能

| 维度 | Shiki（Worker + Oniguruma WASM） | syntect（Rust，分块） |
| --- | --- | --- |
| 16 515 行高亮总耗时（含 IPC） | 822 / 823 ms | 1 721 / 1 930 ms |
| 引擎自身耗时 | 753 ms（旧 404 + 新 427，日志：8 208 行 361 ms、8 309 行 393 ms） | 1 425 / 1 458 ms |
| 请求次数 | 2（每个版本一次） | 12（每版本 6 块，可见区域优先） |
| 首屏影响 | 无（异步着色） | 无（异步着色） |
| token 数 | 111 896 | 161 252 |
| 首次语言加载 | 需要动态加载语法 + WASM（约 14 ms，之后 0 ms） | 进程启动时加载 bat 语法集（已计入二进制体积） |

Shiki 的总耗时约为 syntect 的一半。差距的部分原因是 token 粒度：syntect 把 CJK 字符、CSS 数值单位等拆成更细的 token（161 k vs 112 k），也让 IPC 回传的 JSON 更大。

### 4.2 正确性（Vue 单文件组件、Swift）

用 `env-check highlight`（syntect）与 `tools/shiki-tokens.mjs`（同一套 Shiki 配置）逐行对照 `src/App.vue`、`src/Model.swift`：

- **Vue SFC 两者都正确**：`<template>` 内的标签、属性、`{{ }}` 插值，`<script setup lang="ts">` 内的 TS，`<style scoped>` 内的 CSS 都按各自语法着色；syntect 额外把 `{{`、`label`、`}}` 分成多个 token，Shiki 合成一个。
- **模板字符串内插值**：`${count.value}` 在 syntect 中 `count` 与 `.value` 颜色不同，Shiki 更接近整段字符串同色。
- **Swift 多行字符串**（`"""…"""`）两者都正确识别，不会被截断成代码。
- **跨行结构**：多行注释、模板字符串在语言模型里必须按完整文件解析。Rust 侧有对应单元测试（`multi_line_comment_keeps_coloring_inside_the_comment`、`template_string_spans_lines_as_one_token_family`），并额外用「分块请求 == 一次性全量请求」的等价测试锁定（`chunked_requests_match_a_single_full_request`）。
- 未知扩展名两者都退回纯文本，不报错。

结论：**正确性上没有决定性差异**，都属于「够用」。syntect 的 token 更细，Shiki 的语法定义来自 VS Code 生态、对新语法跟进更快。

### 4.3 体积与工程复杂度

| 维度 | Shiki | syntect |
| --- | --- | --- |
| 前端产物 | worker 1.9 MB（含 Oniguruma WASM；用 JS 引擎时 1.3 MB 但不可用，见 4.4） | 无前端开销，主包 247 KB |
| 应用包 | — | 未打包 Shiki 时 `.app` 7.20 MiB、`.dmg` 3.79 MiB（含 syntect 与 bat 语法集） |
| 线程模型 | Web Worker + 消息协议，需要处理分片/取消 | 专用线程 + 通道，解析状态由该线程独占 |
| 数据通道 | 一次性回传整份 token（约 4 MB 结构化克隆） | 分块回传，每块最多 2000 行 |
| 与 ADR-0004 的关系 | 不使用 Shiki 的 diff 能力，只做单文件高亮，不冲突 | 同在 Rust 侧，与 Git 解析共用一套模型 |

### 4.4 重要发现：Shiki 的 JS 正则引擎在 WKWebView 里不可用

同一份 178 KB / 8 208 行的 TypeScript，在同一个 WebView Worker 中：

- `createJavaScriptRegexEngine()`：worker 打印「高亮开始」后 **35 秒仍未返回**（无完成日志、主线程 rAF/定时器被拖住），多次复现；
- `createOnigurumaEngine()`（Shiki 默认）：**361 ms** 完成。

同一份文件在 Node（V8）里用 JS 引擎只要 913 ms。也就是说这不是 Shiki 的问题，而是 **JSC 上 JS 正则引擎的回溯代价**在真实 WebView 里被放大。要选 Shiki，就必须用 Oniguruma，不能沿用「用 JS 引擎省体积」的建议。

### 4.5 推荐

**首版采用 Rust 侧 syntect。** 理由：

1. 首屏、滚动、切换三项验收标准已达标，且与高亮引擎无关；syntect 不引入前端负担与额外线程模型；
2. 首屏瓶颈在 IPC，而 syntect 的分块协议已经把单次回传压到 2000 行以内，M2 之后按需取行范围是自然的延伸；
3. Shiki 的可用配置（Oniguruma WASM + Worker）在体积和线程模型上都不占优，且存在 4.4 的踩坑；
4. 若以后要换，前端已经通过 `Highlighter` 接口把两套实现隔离开，切换成本主要在删代码。

同时建议在 M1 保持现有的**分块 + 解析状态缓存**协议：只渲染可见区域是既定方向，高亮也应该按同一粒度取数据。

## 五、运行环境验证

实现方式：在每个 worktree 目录下启动一次用户的交互式登录 zsh（`-l -i`），把选定环境变量以 NUL 分隔转储到临时文件后读回；显式传入最小基线环境，不继承应用进程的 PATH；硬超时 5 秒，超时或失败回退到最小环境。结果按 worktree 缓存。

验证用一个真实的 husky pre-commit hook：hook 内比较 `node -v` 与 `.node-version`，不一致就失败。

### 5.1 剥离环境（`env -i`，PATH 只有 `/usr/bin:/bin:/usr/sbin:/sbin`）

| 场景 | 判定 | node | 期望 | git | 登录 shell 解析耗时 |
| --- | --- | --- | --- | --- | --- |
| `node-versioned`（`.node-version` = v24.11.1） | 通过 | 24.11.1 | 24.11.1 | `/opt/homebrew/bin/git` 2.56.0 | 163 ms |
| `node-default`（无 `.node-version`） | 通过 | 26.5.0 | 26.5.0（fnm 默认） | 同上 | 118 ms |
| `mergetool`（`merge.tool = vscode`） | 通过 | — | — | 同上 | 116 ms |

hook 输出（`node-versioned`）：

```
[husky] node path: /Users/garland/.local/state/fnm_multishells/…/bin/node
[husky] node version: v24.11.1
[husky] OK: node v24.11.1 matches .node-version
```

`node` 的路径是本次 shell 会话新生成的 fnm 目录（`fnm_multishells/<pid>_<时间戳>`），证明拿到的是按目录解析出的版本，而不是应用进程里的默认版本。

### 5.2 从 Finder 双击构建出的 `.app`（launchd 启动）

用 `launchctl setenv` 注入验证参数后 `open "Revia M0 Prototype.app" --args env-check <模式>`，与应用由 LaunchServices 启动的路径一致。

| 场景 | 判定 | node | 期望 | code | 登录 shell 解析耗时 |
| --- | --- | --- | --- | --- | --- |
| hook / `node-versioned` | 通过 | 24.11.1 | 24.11.1 | — | 204 ms |
| hook / `node-default` | 通过 | 26.5.0 | 26.5.0 | — | 162 ms |
| mergetool | 通过 | — | — | `/opt/homebrew/bin/code` | 131 ms |

mergetool 场景的实测：`git mergetool --tool=vscode` 通过解析出的 PATH 找到了 `code`，并按 git 约定传参（用一个会把 LOCAL 写入 MERGED 的 `code` 替身验证，替身记录到实际参数）：

```
--wait --merge ./README_REMOTE_….txt ./README_LOCAL_….txt ./README_BASE_….txt README.txt
```

替换后的工作区文件内容为 `head line` + 替身标记，**不再包含冲突标记**，`git mergetool` 退出码 0。

### 5.3 与验收目标的对应

- 「按 worktree 解析运行环境」：每个 worktree 解析一次并缓存，两个仓库解析出不同 node 版本；
- 「husky hook 能找到对应项目版本的 node」：通过（v24.11.1 与 `.node-version` 一致）；
- 「`git mergetool --tool=vscode` 能找到 code」：通过（Finder 启动下实测）。

耗时都远低于 5 秒超时（116–204 ms），没有触发回退（`envFallback = false`）。

## 六、未达标与待人工确认

1. **「滚动不卡顿」尚未在真实合成帧下验证。** 本机会话没有图形合成（`visibilityState = hidden`，rAF 0 次/秒），只能测到每屏重建的主线程成本（p95 = 1 ms）。请在正常桌面会话里打开原型，用测量面板确认「滚动模式 = 逐帧」时的帧间隔，或人工拖动确认。
   - **用户人工确认结果（2026-10-20）**：在正常桌面会话中打开原型并滚动 `src/generated.ts`，滚动**不卡顿**；但快速滚动时，可见区域的上方或下方会短暂出现**空白**，新行稍后才渲染出来。这说明当前 24 行的预渲染余量不够，渲染跟不上滚动速度。M1 需要解决这个问题，验收标准见 [技术方向](../../technical-direction.md#diff-传输与渲染m1)。
2. **连续跑多个组合时出现过卡住。** 在还没换成 Oniguruma 引擎的版本里，同一进程连续跑 6 个组合到第 5 个时卡住，日志显示卡在 Shiki 的 `codeToTokens`。换成 Oniguruma 后改为**每个组合单独启动一次应用**，六组全部通过；**没有重新验证同进程连续跑 6 组的稳定性**，M1 之后如果要做长时间会话，需要留意内存与 Worker 的状态累积。
3. **Shiki 默认主题与 syntect 主题不同**（`github-light` vs `Solarized (light)`），对比里的颜色差异部分是主题差异；换主题不影响性能结论。
4. **IPC 是首屏瓶颈**（占 73–82%）。M0 没有优化它，M1 若要压首屏，建议先处理 diff 数据的编码方式，而不是高亮。
5. **`git diff` 的性能没有单独立项测量。** 本次最大仓库只有 10k 行改动；大仓库（几十万行改动）下的 `git diff` 与解析耗时需要在 M1 用真实仓库补测。

## 七、遗留物与如何复现

```bash
# 1. 生成 fixture（生成物不入库，脚本入库）
prototypes/m0/fixtures/setup.sh /tmp/revia-m0-fixtures

# 2. 构建并运行原型
cd prototypes/m0
pnpm install
pnpm tauri dev                 # 开发

# 3. release 构建 + 测量（每个组合单独启动一次）
pnpm tauri build
REVIA_M0_MEASURE=1 REVIA_M0_ONLY=shiki:split \
REVIA_M0_REPO=/tmp/revia-m0-fixtures/diffs \
REVIA_M0_MEASURE_LOG=/tmp/m0-results/shiki-split.json \
  "src-tauri/target/release/bundle/macos/Revia M0 Prototype.app/Contents/MacOS/revia-m0"

# 4. 运行环境验证（剥离环境 / Finder 启动）
REVIA_M0_WORKTREE=/tmp/revia-m0-fixtures/node-versioned \
REVIA_M0_MEASURE_LOG=/tmp/m0-results/hook.json \
  …/revia-m0 env-check hook
open "…/Revia M0 Prototype.app" --args env-check mergetool
```

原始测量数据、验证结果与日志保存在 `/tmp/m0-results/`（`single-*.json`、`stripped-*.json`、`finder-*.json`），重新生成即可复现。

## 八、给设计评审的提示

以下三条需要用户或评审决定，M0 未擅自处理：

1. **syntect 作为首版高亮方案**（本报告推荐）——这属于 README 中「实现时确定」的项，需要确认；
2. **Shiki 的取舍**：如果希望保留「前端高亮」以减轻 Rust 侧负担，必须接受 Oniguruma WASM（约 1.9 MB worker 产物）与 Worker 线程模型；
3. **IPC 预算**：首屏 200 ms 里约 170 ms 是 IPC。若首版目标里「大 diff 快速打开」比「实现简单」优先，建议在 M1 先把 diff 传输格式（而不是渲染）作为一项独立任务。
