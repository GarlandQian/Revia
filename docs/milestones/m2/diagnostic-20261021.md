# M2 诊断过程记录：2026-10-21

本文仅归档任务 01 的 IPC、测量日志与呈现诊断，不替代 [阶段报告](./report.md)，不表示长期运行或 D06 已验收通过；不新增产品决策、不修改既定门槛。

## 证据身份

- A/B 两组及四份 `frame-*` 均来自 `d71ca5b`，二进制 SHA-256：`ce01397a75ecd615254b3271ade6e9169d3b0ea4123cbbf2e7f182cc58b6a2df`；均在高亮通知修复之前。
- 原始报告、时序、有效及无效阶段样本、压缩日志和分析脚本均在 [ipc-20261021](./evidence/ipc-20261021/)；[archive-04-manifest.json](./evidence/archive-04-manifest.json) 记录 23 个文件的源文件及存储校验值，归档校验与分析复算一致。
- 数值摘要可复核 [analysis-ab-01.json](./evidence/ipc-20261021/analysis-ab-01.json)；原始 stdout 日志为空，仍以压缩文件保留。桌面截图在会话中，不虚构本地 PNG 路径。

## 同负载 A/B

[verbose](./evidence/ipc-20261021/verbose-01.json) 与 [minimal](./evidence/ipc-20261021/minimal-01.json) 分别运行 120 轮，随后在 `src/staged.ts` / `split` 静置 90 秒。两组均为 840 次文件切换、1440 次视图切换、3960 次滚动、720 次跳转；这是短诊断，不是 30 分钟基线。

| 指标 | verbose | minimal |
| --- | ---: | ---: |
| Rust / WebContent PID | 18064 / 18067 | 19326 / 19329 |
| 主动操作 / 操作墙钟（秒） | 416.830 / 441.775 | 376.873 / 401.760 |
| 扣除的阶段暂停 / 末尾静置（秒） | 24.945 / 90.010 | 24.887 / 90.007 |
| 负载期 `debug_log` fetch | 28,121 | 50 |
| 负载期全部 fetch | 48,597 | 20,512 |
| `get_rows` / `highlight_range` | 17,715 / 1,321 | 17,721 / 1,301 |
| `row_index_for_line` | 1,440 | 1,440 |
| 最大 fetch active / failed | 17 / 0 | 6 / 0 |
| 最长响应头耗时（毫秒） | 815 | 808 |
| 最长合成帧间隔（毫秒） | 831 | 831 |

两组 `runCompleted`、`frontend.ok` 均为 true，`hiddenTransitions=0`。verbose 末次 `focused=false`，但窗口可见且持续收到 rAF，不将失焦等同隐藏，也不宣称全程稳定 60fps。

逐步日志减少 99.82%，总 fetch 减少约 57.8%，主动操作时间少约 9.6%；足以说明测量日志是显著附加负载。只有一组 A→B，未随机交叉重复，不能把所有时间或内存差异都归因于日志。

### 观测边界

- 所有保存的 callback 观测（66 / 62 次）均为 registrySize=4、baselineStillRegistered=4、新 ID=0，无溢出或观测错误；这是采样事实，不是实时全程零在途。事件监听也在该 Map 中，callback 数不能换算成请求数。
- 保存的 fetch 快照 active 均为 0，但最大 active 为 17 / 6。hook 只观察字符串 `ipc://localhost/` 到响应头的生命周期，不读 body、不替换 Response、不保存 payload；不覆盖 fallback messageHandler 的请求计数。
- callback ID 集合各有 512 上限，年龄仅为连续观测下界；本轮未触及上限。Map 的只读采样补充 body/回调阶段信息，仍不能排除采样间隙发生并完成的工作。
- 静置 DOM 始终为 105 个元素、2 个 diff 行；ResourceTiming 条目为 0，只说明此 API 未记录这些请求，不证明 WebKit 没有资源保留。

## 内存：指标没有一致指向同一根因

末尾静置的最后样本，单位 MiB：

| 指标 | verbose | minimal |
| --- | ---: | ---: |
| Rust footprint | 49.9 | 51.3 |
| WebContent RSS | 537.31 | 362.62 |
| WebContent footprint | 159.1 | 146.1 |
| WebKit malloc `BYTES ALLOCATED` | **108.6** | **132.7** |
| Networking footprint | 22.2 | 14.0 |

**minimal 的 malloc allocated 反而更高。** allocated 不等于存活 JS heap，RSS/footprint 更低也不能证明泄漏消失或日志就是根因；本轮未强制 GC、未清缓存，不猜测回收机制。

两组各有 6 个固定阶段样本，**各 1 个无效且完整保留**：verbose cycle37、minimal cycle20 的采样跨过 END、不在前端 checkpoint 区间内，不纳入有效阶段趋势。

| 组别 | 有效 cycles | WebContent footprint（MiB，依次） | malloc allocated（MiB，依次） |
| --- | --- | --- | --- |
| verbose | 19, 54, 72, 90, 108 | 123.2, 137.7, 140.3, 160.0, 158.9 | 54.6, 76.5, 98.8, 100.8, 108.2 |
| minimal | 40, 60, 80, 99, 118 | 101.2, 102.7, 133.3, 151.3, 150.1 | 65.2, 97.8, 110.6, 118.5, 134.0 |

阶段 cycles 不对应，不能逐点配对。两组短负载阶段仍有增长，末尾 90 秒静置基本平台不等于持续循环的长期稳态。未见持续累计的 callback 注册项或观测时未完成的 fetch，但尚未取得 JS 保留链或 allocator 根因证据。

## 本地依赖生命周期核对

版本由 `src-tauri/Cargo.lock:3230`、`:3403`、`:3428`、`:4544` 核对：tauri 2.12.4、tauri-runtime / tauri-runtime-wry 2.12.2、wry 0.57.0；`node_modules/@tauri-apps/api/package.json:3` 为 2.12.4。下表依赖源码均相对本机 `/Users/garland/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`，应用源码行号固定于 `d71ca5b`。

| 路径与行号 | 已核实行为及限制 |
| --- | --- |
| `tauri-2.12.4/scripts/core.js:22`、`:24`、`:35`、`:64`、`:84` | 真实 callback Map 可只读访问；invoke 注册成功/失败两个 once callback，正常响应删除自己及另一项。事件 callback 也共用此表。 |
| `tauri-2.12.4/scripts/core.js:81`、`:93` | 内部 invoke 不可写；注册后若同步发送/序列化抛错，存在未清理 callback 的条件路径，但本轮没有该路径实际发生的证据。 |
| `tauri-2.12.4/scripts/ipc-protocol.js:25`、`:42`、`:57`、`:59`、`:84` | macOS 首选 custom-protocol fetch；读完 body 才 runCallback；fetch/解析失败后才改走 postMessage。因此响应头完成不等于 invoke 完成。 |
| `wry-0.57.0/src/wkwebview/mod.rs:642`；`tauri-2.12.4/src/ipc/protocol.rs:370` | fallback 通过冻结的 `window.ipc` 调用 WebKit messageHandler；macOS JSON/Raw 响应走 eval 回调，不能先假设积压在大对象 Channel 队列。 |
| `wry-0.57.0/src/wkwebview/class/url_scheme_handler.rs:68`、`:187`、`:292`、`:336` | custom-protocol task 在 start 入表，正常 didFinish 或 stop 删除；FnOnce 响应闭包持有 native 对象。正常成功路径未见每请求永久留项，异常路径是否发生仍需运行证据。 |
| `src/lib/measure.ts:113`、`:134` | `nextPaint` 提前完成不取消 timer/rAF，停止合成时存在暂留到帧恢复的条件；`nextTask` 收到消息后关闭接收 port。仅另一 port 未显式关闭不能证明本轮泄漏。 |
| `src/features/diff/log.ts:31`；`src-tauri/src/commands.rs:392` | 每条普通测量日志是一次 invoke，不是 MessageChannel；Rust 追加磁盘后返回，不保存日志历史。未完成日志请求可以暂时在途，但本轮采样未发现持续累积。 |

这次源码核对未找到能与运行证据闭合的泄漏根因，不据此先改缓存淘汰、renderer 或强制回收。

## 修复前四份呈现测量

下表依次对应归档中的 [split](./evidence/ipc-20261021/frame-split-01.json)、[unified](./evidence/ipc-20261021/frame-unified-01.json)、[cold-split](./evidence/ipc-20261021/frame-cold-split-01.json)、[cold-unified](./evidence/ipc-20261021/frame-cold-unified-01.json)，耗时单位毫秒。

| 负载 / 视图 | 首屏 fromListChanges | 原 appear / warmAppear | 独立跳出 / 返回文字 DOM | 返回缓存颜色 DOM | 滚动最大帧 / 慢帧数 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 标准 / 并排 | 92 | 30 / 18 | 2 / 1 | 26 | 21 / 1 |
| 标准 / 统一 | 95 | 22 / 14 | 1 / 4 | 29 | 20 / 0 |
| 30,000 行全改 / 并排 | 95 | 17 / 22 | 6 / 1 | 22 | 23 / 1 |
| 30,000 行全改 / 统一 | 95 | 15 / 5 | 2 / 1 | 30 | 18 / 0 |

- 标准负载原跳转目标已预取，不能叫冷跳；大文件两视图原目标均未缓存，独立跳出探针开始时均缺 40 行。大文件显示行数为 30,001 / 60,001，不混入标准首屏统计。
- 四份均可见、无隐藏转换；滚动各记录 60 次真实 rAF、0 兜底、0 数据未覆盖。表中偶发慢帧真实保留，沿用现有 backlog 边界，不临时要求清零。
- 同次独立跳出的首个 rAF 回调分别为 6/9/9/9ms，返回为 7/10/4/12ms；检查时画布、文字和行位置匹配，`frameReady=true`。这是 DOM 与 rAF 回调证据，不是屏幕扫描输出的精确计时。
- 原 warmAppear 包含消息等待，例如标准并排为同步 scroll 1ms + taskQueue 17ms；不删除超一帧样本，也不将它与独立 DOM 时间混成同一计时口径。
- 返回源位置在探针准备阶段显式取过双侧高亮并正常滚动往返；四份首个返回 rAF 的 `colorsMatched` 均为 false，后续 22–30ms 才匹配。不能据预热后的返回证明首次打开静置会自动补色；`ok:true` 仅表示测量完成。

## 当时的桌面发现与后续安排

规划方在同构建真实桌面观察到：首次 Vue 并排、Swift 统一视图静置不补色；Swift 切到并排后才出现新侧颜色。[桌面日志](./evidence/ipc-20261021/desktop-before-highlight-fix.log.gz) 显示 Vue 于 10:05:59Z、Swift 于 10:07:01Z 先只请求 New，切视图后分别于 10:06:47Z、10:09:19Z 才请求 Old。颜色观察来自会话截图，日志用于核对请求顺序，二者不互相替代。

已据此推进最小 `DiffView` 高亮完成通知修复；本文记录时新构建进行中，**尚未完成修复后验收，不写“修复已通过”**。该缺陷与内存上漂没有已证明的因果关系。

下一步在修复后完成相关呈现复测，再独立运行 `--baseline-minimal-logging` 的正式 30 分钟主动基线：不使用固定循环数结束、不追加本次的 90 秒静置，采样暂停仍从主动时长扣除，180 秒热身划分不变。继续保留完整时序及无效样本，不按有利内存口径选择结论；若仍持续上漂，再针对 JS 保留链或 MessageChannel/rAF pending 取证。任务 01 实际通过前不开始任务 02–11。


## 后续状态归档说明

上节“新构建进行中”和“下一步”是发现问题时的过程记录，不是当前状态。随后 `6f07f1a` 完成高亮通知修复、呈现复测和第四轮正式关键日志长测；第四轮内存门槛仍未通过。`c6cdf70` 再修复跨版本行号与离散 hunk 的高亮请求范围，最终四份有效测量及普通桌面完成本次 D06 回归验收，未在该版本上运行正式长测。最新数字、构建归属、保留的失败尝试和下一步见 [阶段报告](./report.md)；不能用本过程记录冒称内存已修复或任务 01 完成。
