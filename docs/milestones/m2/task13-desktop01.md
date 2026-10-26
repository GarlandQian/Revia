# 任务13：固定版本复核与首轮真实桌面 / D06

日期：2026-10-25。此页记录 `m2-ui` 的部分验收，不是任务13或整个 M2 完成结论。当前用户同意的顺序仍为：独立验收 UI 后合回 `m2`，再完成最终 M2 验收；截至本页没有合并、推送或发布。

## 固定版本与限定复核

`code-review` 两个独立轴复核 `35da167f8a2202b19a211ade2c311f5ff430cd90...6b3ddf5d3046e5745e65c81076806bbda441b359`：Standards 0 项（Fowler 建议 0），Spec 0 项（范围扩张 0）。首轮的焦点隔离、五节点语义探针、图标变量与状态后缀映射静态闭合。此前 78 份原件的存储/解压 SHA 及 110 份源码检查前/规范化后/当前提交归属均核对一致。沿用[本地检查](./task13-validation01.md)，没有因复核无故重跑完整测试。

在 `/Volumes/T7 Shield/project/Revia-m2-ui` 固定 `6b3ddf5d3046e5745e65c81076806bbda441b359` 构建：`pnpm build`，触碰 `src-tauri/src/lib.rs` mtime 强制嵌入新前端，再执行 `pnpm tauri build --bundles app --no-sign --ci --config '{"build":{"beforeBuildCommand":""}}'`。三步退出0，前后1835份跟踪文件 SHA 完全一致，归档前工作树干净。使用 Homebrew PATH、`CARGO_NET_OFFLINE=true`；此次为未签名 app，不声称发布包。

二进制为 `src-tauri/target/release/bundle/macos/Revia.app/Contents/MacOS/revia`，8,826,832 bytes，SHA256 `b0b2a64e324cda9156a301fd73b9e823094942eb0c16b31691ca758a956efb61`。四次测量启动/退出均核验此 SHA 未变。

## 真实桌面：浅色部分通过，深色未测

全部新材料位于唯一自建目录 `/private/tmp/revia-task13-final-HW71Io`。普通窗口使用 `REVIA_ACCEPTANCE=1` 与该目录下的 `ui-data-light`，未设置 `REVIA_MEASURE_LOG`，所以不是带测量面板的伪普通模式。fixture 为 `ui-fixture`，临时 HOME 为 `home`，Git 系统/全局配置隔离；普通启动没有独立启动环境 JSON 或测量日志，本页不把事后说明冒充原始启动日志。实际截图的独占 fixture 名称、settings、回收区目录与 pid 文件共同佐证本次普通窗口归属；D06 则另有逐次启动记录与真实 data_dir 日志。

- 主窗口布局、Inter/JetBrains Mono、岛屿面板、状态字母配色和不着色文件名符合当前 rev3/Q107；未跟踪组默认收起。红绿灯位置实看正常；系统共享屏幕胶囊遮挡的部分不当作产品元素或布局缺陷。
- 普通窗口没有测量面板、Git 版本、总文件数、A−/A+；分组文件数属于 Q107 既定布局，不误判为未清理内容。
- 两视图已有文字/高亮，符号链接只读且无可写 hunk，冲突说明与二进制默认收起可见；空工作区文件 AX 记录保留。仅覆盖所列状态，不推定所有提示态已验收。
- 丢弃确认框居中，完整文件状态/暂存区不变说明正确；只打开后取消，没有执行丢弃。select 焦点下按审阅/暂存/丢弃快捷键未穿透，仍0/12、选择不变且无新确认框；对话框焦点下审阅/暂存快捷键不穿透，取消正常。
- `ui-fixture-verification.json` 与准备时真实 Git porcelain-v2 比较，状态和 index 条目一致；未执行产品 Git 写入。不将此表述成所有未暂存字节逐一哈希相同。

普通浅色有效进程为 PID7127，Cmd-Q 后退出0。首次 CUA 选择与启动竞态曾重开普通 PID6516，显示旧 fixture；未执行写入，已退出，不计隔离验收。`ui-light.stderr` 为空不提供有效进程证据，保留原件。

深色 PID7686 的进程参数 `-AppleInterfaceStyle Dark` 没生效，`ui-dark-main.png` 实际仍是浅色；它是失败尝试，绝不是深色验收截图。该进程已正常退出。规划方已请求用户授权临时将系统外观切深色后恢复浅色，尚未获准，因此没有修改系统外观。深色、操作被拒绝/结果待核对/监视失败等完整真实状态仍待最终桌面，模拟 WebKit 截图不能代替。

## D06：四组有效测量与原始失利值

每组独立应用进程与新数据目录，使用同一固定二进制，测量后保留窗口核对 PID/目标/视图并 Cmd-Q 正常退出。`perf.py` 保存环境和启动/退出信息；四组 `ok=true`、focused=true、focusLosses=0、hiddenTransitions=0。截图均为测量后同进程取证，不声称是计时中的逐帧画面。

| 场景 / 视图 | 首屏 fromListChanges | 原 appear / warm | 原目标 cached / warm cached | 独立 DOM 文字出 / 回 | 首rAF核验出 / 回 | 缓存返回颜色 DOM |
| --- | ---: | ---: | --- | ---: | ---: | ---: |
| 普通 / 并排 | 116ms | 14 / 4ms | 是 / 是 | 3 / 1ms | 7 / 14ms | 33ms |
| 普通 / 统一 | 97ms | **20 / 7ms** | 是 / 是 | 2 / 1ms | 10 / 14ms | 33ms |
| 冷负载 / 并排 | 118ms（不混入普通） | 14 / 6ms | 否 / 是 | 4 / 1ms | 9 / 7ms | 26ms |
| 冷负载 / 统一 | 119ms（不混入普通） | 12 / 3ms | 否 / 是 | 6 / 1ms | 11 / 13ms | 33ms |

原普通统一 `appear=20ms`、`withinOneFrame=false` 如实保留，其分段为 scrollCall11ms + taskQueue9ms；不扣除、不删除、不用独立探针倒填。按现有 benchmark 方法，原计时含任务等待，独立诊断观察另一目标的实际 DOM/首rAF；两者不是同一操作。不能据全局 `ok=true` 就宣称每笔旧计时都达标。

四组独立探针均 sourceReady=true、preparationError=null、jumpDomError=null。真冷目标事前各缺35行，文字DOM4/6ms低于100ms；八次首rAF均35/35行文字、行号、marker、位置与画布几何匹配，frameReady/textWithinBudget=true。两次冷跳同步观察有底图、首rAF文字已齐，不据同步占位推断它已经显示一帧。普通首屏116/97ms低于150ms。

四次缓存返回首rAF的颜色都未全齐，最终26–33ms严格匹配；不声称首帧完整着色。源位置显式预热，不能以该诊断代替首次打开后静置补色；本轮普通窗口和真实滚动停止后的截图另行观察现有补色。

| 场景 / 视图 | 完成屏数 / 真rAF / 定时兜底 / 未覆盖 | 最大滚动帧 | p95 |
| --- | --- | ---: | ---: |
| 普通 / 并排 | 60 / 60 / 0 / 0 | 18ms | 18ms |
| 普通 / 统一 | 60 / 60 / 0 / 0 | 20ms | 17ms |
| 冷负载 / 并排 | 60 / 60 / 0 / 0 | 18ms | 17ms |
| 冷负载 / 统一 | 60 / 60 / 0 / 0 | 18ms | 17ms |

普通并排/统一另以真实滚轮下10屏、回5屏，离散截图中的文字完整，停止后补色恢复，无整块空白；没有逐帧录像，不能单靠0未覆盖帧证明最终像素。沿用Q84，不增设偶发慢帧清零指标。

本轮局部测量为现有 UI 的首屏、真冷文字、缓存首帧语义和滚动表现提供证据；保留旧20ms超帧与延迟补色边界。未据此放行完整M2；08返修与合入后的最终集成版本仍需重新固定D06。

## 原件与剩余门槛

[原字节清单](./evidence/task13-desktop01/manifest.json) 保存构建命令/退出码/stdout/stderr、源码前后SHA、测量JSON/日志/启动退出、原始PNG与AX、fixture准备和验证、汇总脚本。文本无损gzip，清单分别记原字节与存储SHA，PNG原字节复制；归档后逐份解压校验。普通启动缺少的原始记录明确不补造。

- 浅色所列状态和限定静态复核完成；真实深色与其余状态、Windows编译仍未完成，任务13不标done、不合 `m2-ui`。
- 任务08整批复核 Standards6（最严重P1）、Spec5（最严重P2）仍不通过，意见已回写 `m2`；原Pi在独立 `m2-close-finish` worktree返修，不归咎UI增量。
- 最终完整桌面、最新版双平台CI、同进程可见窗口主动30分钟Q98长测、任务10/11及squash main均未完成。主目录用户改动保持原样。
