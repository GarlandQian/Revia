# 任务 03 验收：明确目标的 hunk 暂存与取消暂存

验收日期：2026-10-23。依据 [任务 03](./issues/03-hunk-staging.md)、[M2 规格](./spec.md) T02／T05／D02／D06／D07 及既定 [benchmark 口径](./benchmark.md)。

**任务 03 通过。** 原始 Git hunk 写入、两个视图的明确按钮与键盘行为、相关版本过期拒绝及 D06 呈现回归已完成本任务验收。
这不是 M2 整体验收：任务 06 尚未实施，功能完成后的最终集成与长期运行验收仍须按任务图执行；本轮不合并 `main`。

## 版本与审查边界

| 用途 | 源码提交 | Release 二进制 SHA-256 |
| --- | --- | --- |
| hunk 实现、自动检查、hunk 桌面操作及 01 测量 | `b7fa6cbba5154d95b6d9b15b13e38fce5ba58aba` | `724a7be4ce6404d40e24610c236a39acdd533f2a74deac8a232d80ce81708a73` |
| 最终 02 测量、普通呈现检查及最终 CI | `bbddd4f356e624e504e93d4db6bf5009df66f468` | `5a3cea3a86dc5721c16ca8ede06d1e67170787fb25a3fbb692abc2861ef5250d` |

- hunk 增量固定为 `911751a...b7fa6cb`；任务 01／02／09 的既有结论不在本轮重复验收。
- 测量增量固定为 `b7fa6cb...bbddd4f`：仅显式自动测量增加前台门槛、失焦记录、PID 标题与测后保留窗口；默认、long、smoke 的退出行为不变。
- 测量增量未修改生产写入／渲染逻辑、原计时边界或性能阈值；失败仍保留已完成的组合与可见性记录。
- 独立 Spec 复核未发现要求缺失、越界或表面满足实际错误；测量增量 Standards 无硬违规，保留 1 条非阻塞 P3 命名观察。
- `focusLosses` 是饱和的 0／1 失焦标志，不是累计次数；本报告按真实语义解释，不为命名重建或改写样本。

## 自动检查与 CI

| 检查 | 版本与实际结果 |
| --- | --- |
| 本地 Rust | `b7fa6cb`：68 单元 + 20 file_staging + 23 hunk_staging + 3 session_flow = 114 项，0 失败 |
| 本地 Node | `b7fa6cb`：行缓存契约 25 项、可见高亮 13 项，0 失败 |
| 本地类型／lint／构建 | `b7fa6cb` 通过；`bbddd4f` 再次类型检查、lint、前端及 Release 构建通过 |
| CI `b7fa6cb` | [#39873931980](https://github.com/GarlandQian/Revia/actions/runs/39873931980)：macOS arm64、Windows x64 均 success |
| CI `bbddd4f` | [#39876636277](https://github.com/GarlandQian/Revia/actions/runs/39876636277)：macOS arm64、Windows x64 均 success，匹配最终源码 |

两轮 CI 日志均记录 macOS Rust 68 + 20 + 23 + 3 = 114 项、Windows Rust 67 + 16 + 18 + 3 = 104 项，均 0 失败；两平台类型检查、lint、前端构建及 release 编译通过。
Windows 结果是实际 Windows x64 编译／测试，不是 Mac 交叉编译，也不等同于 Windows 完整桌面产品验收。
本地 114／25／13 项来自 `b7fa6cb`，不冒称全部在 `bbddd4f` 本地重跑；最终版本另有完整 CI 结果。
23 项真实 Git hunk 测试覆盖部分暂存、首中末 hunk、增删与无 HEAD、CRLF／无末尾换行／非 UTF-8 正文、路径与模式、配置干扰、旧目标与错目标、相关／无关文件变化、只读矩阵、写锁及锁重试。
自动检查与对应日志见 [automated-checks.json](./evidence/task03-20261023/automated-checks.json)；不以静态路径核对冒充真实超时或信号注入。

## hunk 真实桌面结果

使用独立 fixture 与 `HOME=/private/tmp/revia-accept03-qq5vxahd/home`，实际暂存区／工作区由 Git 和字节检查核对，不只观察按钮或成功提示。
[desktop-checks.jsonl](./evidence/task03-20261023/desktop-checks.jsonl) 共 **12 条**：11 条 `passed=true`，另 1 条为 `observedAllowed=true` 的相关版本语义观察。

| 场景 | 桌面与 Git 核对结果 |
| --- | --- |
| 并排暂存中间 hunk | `a-review.ts` 暂存区仅第 80 行变化，工作区 SHA 保持原值 |
| 统一按钮 Space／Enter | Space 取消暂存后，Enter 暂存首 hunk；暂存区仅第 10 行变化，工作区不变 |
| hunk 按钮焦点下 ⌥⌘A | 仍为整文件动作，暂存区包含第 10／80／140 行，不缩成 hunk |
| 并排取消暂存中间 hunk | 暂存区保留第 10／140 行，目标第 80 行被取消暂存 |
| 工作区单侧变化后取消暂存 | 仅工作区第 80 行改为 2080，HEAD／index 未变，旧 staged 第 10 行仍允许操作；index 只剩第 140 行 |
| 未暂存侧真实过期 | 另一个工作区 hunk 改变后点击旧目标，提示“文件已变化，请重新确认”，index 字节不变，刷新显示第 80 行 3080 |
| 已暂存侧真实过期 | 外部改变另一个 hunk 并更新 index 后点击旧目标，同样拒绝，index／工作区字节不变，仍选中 staged 组 |
| 预渲染区键盘可达 | 统一 Tab／Enter 使按钮可见并仅暂存第 130 行；并排 Tab 使 `item0226` 标题按钮进入可见区 |
| 全局导航与类型边界 | 按钮焦点下 ⌘O 打开原生面板，取消不换仓库；二进制展开与未跟踪文件均无 hunk 按钮 |
| 退出 | Cmd-Q 正常退出，退出码 0 |

“相关版本”按 [ADR-0009](../../adr/0009-file-scoped-write-fingerprints.md) 解释：staged 对应 HEAD／index，不能把无关的工作区内容变化强行扩大为 staged 指纹失效。
两侧真实过期用例都保留选中 hunk 不变、改变同文件其他相关 hunk；staged 用例明确更新 index，不把仅工作区变化误报为拒绝失败。
刷新后的旧 hunk 凭据仍受快照代际约束；原始补丁、目标绑定与重试安全由代码复核及真实 Git 测试覆盖，未把全部后续刷新竞态列为本任务已验收。

## D06：最终 02 测量

四份 01 JSON／日志原样保留，但因焦点连续性和截图进程归属证据不完整，**不作为最终验收样本**；02 使用新文件名，不覆盖 01。
最终四份均为 `bbddd4f`，各独立进程、窗口 1280 × 800、diff 高度 718、DPR=1；`ok=true` 只代表测量完成，不单独推出 D06 通过。

| 最终样本 | PID | `focused` | `focusLosses` | `hidden` / `hiddenTransitions` |
| --- | ---: | --- | ---: | --- |
| [标准／并排 02](./evidence/task03-20261023/frame-split-02.json) | 16520 | true | 0 | false / 0 |
| [标准／统一 02](./evidence/task03-20261023/frame-unified-02.json) | 16901 | true | 0 | false / 0 |
| [冷负载／并排 02](./evidence/task03-20261023/frame-cold-split-02.json) | 17150 | true | 0 | false / 0 |
| [冷负载／统一 02](./evidence/task03-20261023/frame-cold-unified-02.json) | 17316 | true | 0 | false / 0 |

四份均有测后原 PID 窗口的对应截图，截图在会话中核验，未归档本地 PNG；**测后同 PID 截图不是计时期间截图**。四个进程均已 Cmd-Q 退出。
下表单位均为 ms；首屏列是原 `fromListChangesMs`，另保留从打开 worktree 起的总计时，不混用边界。

| 样本 | 首屏 / 打开 worktree 起 | 原 appear / warm | DOM 去程缓存 / 缺行 | 文字 DOM 去 / 回 | 首 rAF 核验去 / 回 | 返回颜色 DOM |
| --- | ---: | ---: | --- | ---: | ---: | ---: |
| 标准／并排 | 115 / 251 | 23 / 7 | 命中 / 0 | 2 / 1 | 7 / 15 | 38 |
| 标准／统一 | 95 / 233 | 14 / 5 | 命中 / 0 | 1 / 1 | 9 / 15 | 34 |
| 冷负载／并排 | 110 / 246 | 28 / 5 | 未命中 / 40 | 5 / 1 | 8 / 12 | 31 |
| 冷负载／统一 | 85 / 220 | 21 / 3 | 未命中 / 40 | 4 / 0 | 6 / 13 | 32 |

普通负载首屏 115／95ms 均低于 150ms；独立 30,000 行全改冷负载的首屏单列，不混入普通首屏统计。
两个真冷目标发起前各缺 40 行，文字 DOM 5／4ms 低于 100ms；全部返回均已缓存、事前缺行 0。
八次同次首 rAF 均文字匹配 40/40、`frameReady=true`、`textWithinBudget=true`、`completed=true`，无准备错误或超时；原数据没有 `valid` 字段，不另造该字段。

| 样本 | 完成屏数 / 真 rAF / 兜底 / 未覆盖 | 帧间隔 max / p95 / p50 | 慢帧 >20ms | 未覆盖 max / p95 |
| --- | --- | ---: | ---: | ---: |
| 标准／并排 | 60 / 60 / 0 / 0 | 19 / 17 / 17 | 0 | 0 / 0 |
| 标准／统一 | 60 / 60 / 0 / 0 | 17 / 17 / 17 | 0 | 0 / 0 |
| 冷负载／并排 | 60 / 60 / 0 / 0 | 17 / 17 / 17 | 0 | 0 / 0 |
| 冷负载／统一 | 60 / 60 / 0 / 0 | 18 / 17 / 17 | 0 | 0 / 0 |

### 原计时与证据限制

- 标准并排原 `appearMs=23`、`withinOneFrame=false` 如实保留；原目标第 4985 行，独立 DOM 探针第 6647 行，不能拿 2ms／7ms 倒填成原操作达标。
- 冷负载原 appear28／21ms 的 `withinOneFrame` 也为 false；冷文字门槛是 100ms，不混用缓存热跳一帧门槛。
- appear／warm 含调用、数据及消息任务等待；DOM／首 rAF 是语义核验完成的上界，非硬件扫描上屏时间，0ms 只表示毫秒计时精度。
- 两个冷跳同步阶段看到 40 行占位，首 rAF 已无占位且文字齐全；不能据此声称占位实际显示过一帧。
- 四次返回首 rAF 颜色均未全齐，最终分别匹配 77/77、39/39、78/78、39/39；不声称首帧完整着色。去程 `colorDomMs=null`，不能当 0。
- 滚动未覆盖为 0 只证明行数据覆盖；慢帧为 0 是本次结果，不新增慢帧清零门槛。整段可见性 `maxFrameGapMs=31/31/32/24`，不得替换表中滚动 max。

## 普通桌面呈现核验

最终构建普通启动使用 `HOME=/private/tmp/revia-task03-visual-cjuzain1/home`，日志确认独立应用数据目录；未依赖仅对自动模式生效的隔离变量，也未执行 Git 写操作。
- Vue 首次并排静置后旧／新双侧自动补色；Swift 首次统一静置后自动补色。
- `generated` 统一视图下滚 10 屏、上滚 5 屏，远拖至约 5537 行并返回顶部，颜色恢复完整，无整块空白。
- 同文件并排视图执行相同滚动，远拖至约 5556 行并返回顶部，双侧颜色完整，无整块空白；最后 Cmd-Q 正常退出。

呈现结论来自真实桌面核验；[visual-checks.json](./evidence/task03-20261023/visual-checks.json) 记录场景，[visual.log.gz](./evidence/task03-20261023/visual.log.gz) 只佐证请求顺序，不能从日志推断像素。
**本任务 D06 回归按既定 DOM + 同次 rAF + 普通真实桌面口径通过**；保留原始超帧值与全部 01 样本，不改变阈值，也不升级成硬件帧精确测量结论。

## 证据与剩余边界

- [归档清单](./evidence/task03-20261023/manifest.json) 关联源码、二进制、测量 PID、源／存储 SHA；JSON 原样保存，日志 gzip 压缩，不改写原始结果。
- [02 完整提取](./evidence/task03-20261023/performance-02-extracted.json)、[01 完整提取](./evidence/task03-20261023/performance-01-extracted.json) 与 [提取脚本](./evidence/task03-20261023/analyze-performance.py) 支持独立复核，脚本只提取而不生成通过结论。
- [桌面 fixture 脚本](./evidence/task03-20261023/setup-desktop.py)、[hunk 桌面日志](./evidence/task03-20261023/desktop.log.gz) 与两份 CI JSON／日志保留，截图证据仅在会话中，不伪造 PNG 归档。
- 实际 Git 超时／信号中断注入本次未实测，是残余验证风险；不把复用的 `NeedsVerification` 与清理分支静态审查写成注入验收。
- [任务 06](./issues/06-current-worktree-refresh.md) 的监视器、完整读响应排序与自动刷新场景尚未实施，不能用本任务的写前指纹拒绝替代。
- 本轮仅依 Q97 正常推送 `m2` 并验证既定 CI；不推 `main`、不强推、不打标签、不创建 PR、不发布安装包。M2 尚未完成，不执行里程碑合并。
