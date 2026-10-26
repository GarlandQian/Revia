# Rust 排版减分配修复与完整 Q98 复验

日期：2026-10-25。用户在要求“修好之后测试之前停下来”后，以“继续吧”放行验证。本轮由 Codex 直接修复、验证，不再派回 Pi。**本轮完整 Q98 三条内存指标均通过；不等于 M2 全部完成，不合 main、不启动 M3。** 上轮完整长测 Rust maxRise 72.9 MiB 的[失败记录](./q98-formal-20261025.md)及更早失败保留，不覆盖、不拼接。

## 修复与固定版本

- 产品提交 `9e5c7d1b7d30d06631ea53b63e1360fb45b4c063`，基线 `201df2a93074f18e5ed50a694d6575fa09dc86c7`。`src-tauri/src/git/rows.rs` 共用仅保存源行位置的 `LayoutRow` / `RowLayout` 遍历，计数、行号定位及 hunk 锚点不再生成全文件 `RenderRow` 与文本副本；会话缓存只存位置索引，按请求范围物化。
- `src-tauri/src/session.rs` 保持八份缓存上限，在同一快照锁内生成范围行并附加当前 hunk action；刷新整体替换快照。保留 Git hunk、`NoEol` 分界、连续删除/新增配对、精确行匹配优先、diff 指纹、写门禁及 IPC 类型。
- 未改 Q98 阈值、180 秒暖机、负载、压力或采样间隔；未更换分配器、强制 GC、增加缓存上限或猜测性清缓存。
- 文档更正提交 `59b925d846286441df4c66ce2074cba4d46299a8` 是真实 Release、D06、Q98 和双平台 CI 的受测 HEAD。`snapshot-before-build.json` 起初冻结于产品提交，随后只改文档；`snapshot-pre-release.json` 记录实际构建 HEAD。143 份源码/测试/配置/锁文件在本地检查、构建和长测前后 SHA 完全一致。
- `pnpm tauri build --bundles app` exit0；新 bundle 与同次裸 binary 的 SHA256 均为 `28da0f0dc4bf542af589c0e73cc6592520860adee0852fa0f0b5da899438e9fd`。没有沿用旧 `.app`。本报告及后续归档仅改文档/证据，不冒称这些后续提交重新构建或重测。

## 检查与独立双轴审查

独占现场 `/private/tmp/revia-rust-memory-verify.aC01we`；`commands/record.py` 在执行时记录精确 argv、cwd、显式非敏感环境、起止、真实退出码和完整 stdout/stderr。完整继承环境没有导出；`env -i` 的应用隔离参数保留在实际 argv 中。

| 检查 | 实际结果与边界 |
| --- | --- |
| 完整 Rust | harness 274 passed / 0 failed / 1 既有 ignored；首轮旧 `session_flow` 三项因默认 fixture 缺失提前返回，不能算其业务实测通过 |
| `session_flow` 补验 | 生成本轮隔离 fixture，显式 `REVIA_M1_FIXTURE` 补跑全部三项，读取/布局/高亮断言及耗时输出实际完成，exit0 |
| 前端 | `pnpm lint`、`pnpm build`（含 `tsc -b`）exit0；既有 lint/Vite warning 保留 |
| 绑定与源码 | Rust 自动导出绑定后只运行既有行尾空白规范化；143 份源码检查前后 SHA 相同，diff-check exit0 |
| 新布局回归 | 既有模块新增三项、补强两项；跨 hunk 范围、缺侧配对、`NoEol`、空差异、计数/锚点与行号缺口实际执行通过；未新增框架 |

正式双轴固定 `201df2a...9e5c7d1`：Standards 确定 **1 项 P3**、possible smell **0**；Spec **1 项 P3**、产品行为确定发现 **0**、范围扩张 **0**。两轴分别发现矩阵仍把已执行布局回归写成未执行（`ST-RM-1` / `SP-RM-1`），不合并计数、不回写为首轮零发现。`59b925d` 修正文档，两轴限定复核均确认闭合、新增 0。四份完整独立报告随原件归档；静态审查不替代运行效果。

## 新版双平台 CI

[Compile Check #40199770545](https://github.com/GarlandQian/Revia/actions/runs/40199770545)，固定 `59b925d`，最终 `completed / success`。按 Q97 仅经既有 SSH origin 正常推送 `m2`，没有推 main、强推、打标签、开 PR 或发布。

| 平台 | 类型检查 / lint / 前端 build | Rust harness | Release binary |
| --- | --- | --- | --- |
| macOS arm64 | 全部 success | 274 passed / 0 failed / 1 既有 ignored | success |
| Windows x64 | 全部 success | 246 passed / 0 failed / 1 既有 ignored | success |

三个新增布局测试在两平台真实日志中均为 `ok`。平台数量差异来自既有条件编译。CI 的旧 `session_flow` 仍是 fixture-optional，harness 总数不能单独当作其 fixture 业务执行证据；本轮本地已显式实际补验。完整 CI 元数据及原始日志保留；旧版 CI 成功/失败不倒填为本版。

## 完整正式 Q98

新建 HOME/TMPDIR、隔离 fixture 和驱动独立应用数据目录 `tmp/revia-soak-data-q9acz9fn`；全局/系统 Git 配置隔离，仅测自建仓库。原参数 `--minutes 30 --interval 10`，无 diagnostic/minimal/focus/keep-on-top。`caffeinate -di` 只防空闲睡眠，未改系统设置。

| 项目 | 实测 |
| --- | --- |
| 驱动记录起止（北京时间） | 21:45:16.589–22:18:22.749；1986.155 墙秒，exit0 |
| 主动操作起止（北京时间） | 21:45:18.237–22:18:21.013 |
| 主动 / 操作墙钟 / 阶段暂停 | **1801.023 / 1982.776 / 181.748 秒**；可见性暂停 0；毫秒舍入有约 0.005 秒差异 |
| Revia / WebContent / GPU / Networking PID | 93979 / 93982 / 93981 / 93983，归属来自该进程的 launchctl services |
| 循环 / 文件 / 视图 / 滚动 / 跳转 | 620（含末轮部分）/ 4337 / 7433 / 20446 / 3718；七个文件、两种视图 |
| 常规 / 阶段 / 暖机后压力样本 | 194 / 29 / 5；29 段最终全部 valid、authoritative，缺失/无效 0 |
| 可见性 | 182 个前端样本加最终观测，hidden/hiddenTransitions 均 0；rAF 约 10.93–61 次/秒，最大帧间隔 933ms，不宣称全程 60fps |
| 结束 | 应用自行 exit0；清理前后 poll 均 0，terminate/kill 均未尝试；退出后 CUA `listApps` 确认未运行 |

启动、中段和近结束截图/AX 是时点证据，不代替前端完整可见性监测。最终 frontend JSON、29 条已闭合增量 checkpoint、阶段 BEGIN/END 一致；常规/阶段 JSONL 与最终汇总一致，errors/partialCandidates 为空，`runCompleted=true`、`q98MemoryVerdictAuthoritative=true`。本轮未复现 vmmap255，不宣称已修复其系统根因。

独立派生审核按真实操作开始时间对齐 180 秒暖机，本轮入选样本与 driver 的 setupSeconds 口径相同，三条判定逐字段吻合；不拿驱动墙秒或 exit0 代替主动时长/内存判定。

单位 MiB，斜率 MiB/分钟；每条均须 `maxRise<=10` 且 `slope<=0.5`，压力样本至少 3 个。

| 指标 | 样本 | 首值 → 末值 | 最高 | maxRise | 斜率 | 结论 |
| --- | ---: | --- | ---: | ---: | ---: | --- |
| Rust 常规 footprint | 59 | 47.8 → 47.6 | 49.6 | **1.8** | **-0.0051** | 通过 |
| 压力后 WebContent footprint | 5 | 83.4 → 84.7 | 84.7 | **1.3** | **+0.0922** | 通过 |
| 压力后 WebKit malloc allocated | 5 | 21.7 → 26.9 | 27.1 | **5.4** | **+0.2639** | 通过 |

普通 RSS/footprint 序列完整保留，不因非判定指标上涨而隐藏。例如暖机后普通 WebContent RSS 首 303.84、末 423.67、斜率 +2.4405；按已确认 Q98，不单独作为失败依据，WebView 判定仍使用压力后两条指标。没有因此放宽规则或把 RSS 当 footprint。

**与上一轮的对照**：旧版 Rust maxRise 72.9，本轮为 1.8 MiB；本轮针对性消除了源码中确定的重复全量分配路径，并通过完整正式验收。已保存的 94 份 Rust vmmap 中，暖机后 `DefaultMallocZone` live allocated 为 24.3–26.7 MiB。这里只是区域级观测，没有分配调用栈归因，不能声称已证明全部历史波动来源或永不再现。

fixture 的 HEAD、工作文件、暂存区语义及原始 index 字节前后均一致；index SHA256 `57ccb3e267874ec397f028280eb7ad3ec3cdecb8838c9590552d3941ed3c5e8c`。没有对用户真实仓库试写。

## 新版 D06 与保留的不利结果

四组有效样本均可见/聚焦，focusLosses/hiddenTransitions 为 0；每组 60 个真实 rAF、0 fallback、0 uncovered。

| 场景 | 首屏 | 原 appear / warm | 独立文字出 / 回 | 返回严格全色 DOM | 滚动最大帧 / p95 |
| --- | ---: | --- | --- | ---: | --- |
| 标准并排 | 124 | **18 / 7** | 2 / 1 | 32 | 19 / 17 |
| 标准统一 r2 | 129 | **22 / 6** | 2 / 1 | 33 | **35 / 17** |
| 真冷并排 | 124* | 10 / 3 | 3 / 1 | 29 | 18 / 17 |
| 真冷统一 | 108* | **28 / 4** | 2 / 1 | 32 | 17 / 17 |

单位毫秒。* 冷负载为独立 30000 源行全改文件，不混入标准 150ms 首屏门槛。标准两组首屏达标；真冷外跳实际各缺 35 行，独立文字 3/2ms。八次独立首帧均 35/35 文本匹配、0 占位；返回首帧并非全色，29–33ms 才严格全色。

**原 appear 的 18/22/28ms 均 `withinOneFrame=false`，保留为未闭合的原跳转超帧记录，不将 D06 笼统判为全部通过。** 独立探针是另一目标，不能倒填原 appear。原 warm 四组均在一帧内；滚动 35ms 不利值及标准统一 droppedFrames=1 保留，不额外增设 Q84 慢帧清零门槛。

标准统一首次因未取得前台可见合成窗口，120 秒后失败（`ok=false, combos=[]`），不是有效性能样本；原 JSON/log/截图/AX 全保留。另以新输出 `-r2` 独立补测，不静默覆盖或挑最好一次。

## 原件与过程纠正

[原件清单](./evidence/q98-rust-memory-20261025/manifest.json)保留本地与 CI 的命令元数据/完整双流、四份审查报告、源码/产物/fixture 快照、正式 Q98 全部日志/JSONL、D06 所有尝试、CUA 时点证据及独立派生脚本/结果。gzip 逐件解压核对字节和 SHA；fixture、应用运行数据和二进制仍留独占现场，不冒称为命令原件。

- CUA 预检通过完整路径 `getApp` 曾自动启动未隔离的普通实例；立即 Cmd-Q 退出，未点击内容控件或执行 Git 写入，不计验收。没有普通持久数据前后快照，不宣称零元数据触达；事件见 `ui-preflight-note.md`。后续只绑定已由隔离环境启动的受测实例，退出确认用 `listApps`。
- CI 派生脚本首版发生 SyntaxError（`ci/07-*`），保留错误脚本及双流；修正后 `ci/08-*` 成功，是派生脚本错误，不是 CI/产品失败。
- Q98 派生分析首版按 footprint 值关联 native 选点；重复值会误关联更早同值的时间/区域数据，不影响三条 Q98 数值或判定。保留 `analyze-final-v1.py`、`analysis-v1.json` 和 `commands/26-*`；修正为选定暖机后常规样本，再以 `samplingStartedAt` 对应 Rust vmmap 的 `startedAt`，断言偏移小于 1 秒且数值相同，`commands/27-*` 仅重跑派生分析。正确末样本为 22:18:10.613，Rust vmmap 为 22:18:10.627；没有重跑或改写正式长测。

## 尚未完成

本轮内存复验关闭了当前 Rust 超幅门槛，D07 对最终产品源码的本地检查与双平台实际编译已完成；D06 原跳转超帧记录仍需收口，其余 D02/D03/D04/D05/D08 桌面补验及任务05查看/清空的自动化/桌面边界仍见[当前矩阵](./issues/10-integration-matrix.md)。旧桌面证据保留各自版本，不冒称全部在 `59b925d` 上重做。任务10/11/13及 M2 不标 done，不 squash main、不清理仍需的 worktree、不启动 M3。
