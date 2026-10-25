# M2 功能后 D06 与 Q98 实测（2026-10-25）

**M2 未通过最终验收。** D06 四组已完成；Q98 两轮均未完成要求的同进程可见窗口主动 1800 秒，第二轮还存在 Rust 常规 footprint 阈值触发。不得以应用退出0、阶段采样成功或已有 CI 成功替代最终通过。

## 固定版本与证据

- 集成/CI 版本：`f10fc971c14e556c239cb4648cabfbbfc28ad4b4`。
- 实际 Release 构建来源：`2a51b160d1c3871b9c67fee81ae6f082910bc1f8`；f10 相对2a51只有测试fixture及任务10文档变化，生产代码/资源相同，不冒称f10重新构建。
- 二进制 SHA256：`fd5ffc86131aebbe77850487192005166a63c1a98f9e97aa72b3ddaa1d96988d`。
- 各次退出记录均确认二进制和测前2277份 tracked 文件未变。之后的 `9434e7a` 仅记录用户新指定的 Pi 模型顺序，不回填到受测来源。
- 原件根：`/private/tmp/revia-m2-final-measure-ex9swy__`；归档入口：[manifest](./evidence/final-measure-20261025/manifest.json)。保留失败、空输出、截图、命令、环境及源码SHA；派生审核与运行原件分开，不复制完整fixture、应用数据或bundle。

## D06：四组可见窗口测量

| 组 / PID | 首屏 ms | 原 appear / warm ms | 独立文字出 / 回 ms | 返回全色 DOM ms |
| --- | ---: | --- | --- | ---: |
| 标准并排 / 85755 | 137 | 10 / 4 | 2 / 1 | 33 |
| 标准统一 / 87247 | 124 | **18 / 7** | 1 / 1 | 33 |
| 真冷并排 / 87618 | 135（独立大负载） | 11 / 4 | 4 / 2 | 29 |
| 真冷统一 / 87831 | 137（独立大负载） | 13 / 4 | 3 / 1 | 32 |

四组 `ok=true`、`focused=true`、`focusLosses=0`、`hidden=false`，各自正常 Cmd-Q 退出0。标准首屏137/124ms低于150ms；冷仓库首屏不混入标准门槛。两个冷目标发起前各缺35行、`targetWasCached=false`，不是仅凭文件大小称真冷。

八次独立首rAF均35/35语义行匹配，覆盖文字、行号、marker和布局几何；冷文字4/3ms低于100ms。**标准统一原目标 `appearMs=18`、`withinOneFrame=false` 仍是不利结果**，独立DOM探针属于另一目标，不能倒填或声称所有原跳转均在一帧内。

四组各60个真rAF滚动样本，0 fallback、0 uncovered；最大帧18/31/48/18ms如实保留，不增设慢帧清零门槛。标准两视图另有真实滚动8页及测后截图，没有观察到整块白屏；不是逐帧录像。返回首帧颜色未全齐，29–33ms才达到严格token DOM匹配，不称首帧全色或普通模式首次静置补色已由本批证明。

准备冷仓库时第一次引用不存在的 `/opt/homebrew/bin/python3`，进程未启动，错误事后注记保留；改用 `/usr/bin/python3` 后只重跑冷仓库准备，未重新生成标准fixture。

## Q98：两次失败分别保留

两次均使用原 `tools/bench-long-running.py --minutes 30 --interval 10`、同一固定bundle及标准fixture；没有诊断模式、简化日志、置顶参数、阈值或负载更改。第二轮只在开始时显式点击前台激活，时长不与首轮拼接。

| 轮次 | PID | 驱动墙钟秒 | 结果 |
| --- | ---: | ---: | --- |
| `q98` | 88354 | 78.54 | 退出1，前端报告 `maxFrameGapMs=2179 > 2000`，可见连续合成中断 |
| `q98-attempt02` | 90685 | 1223.25 | 退出1，常规 `vmmap -summary 90688` 返回255；缺少前端最终JSON |

首轮开始readiness为focused=true，后续负载心跳focused=false、hidden=false。focused不是直接失败门，不证明遮挡；不能确定归因为产品、系统调度或vmmap。首次普通阶段采样因driver看到BEGIN过晚且取样跨窗口而无效，没有热身后有效压力样本，不能据此判内存增长。

第二轮已有120笔常规样本及18次阶段前后归属均固定 Revia90685、WebContent90688、Networking90689、GPU90687；最后失败瞬间没有额外归属快照，不据此前样本推定该瞬间进程仍正常。

### 第二轮的证据缺失与阈值

1. 18个阶段均 `samplingChecksPassed=true`、匹配END=true；最终全部invalid的唯一原因是缺少前端报告中唯一的staged/split checkpoint。**不是18次采样都失败**。BEGIN/END不足以补造完整的起止区间和 `hiddenDuringHold`。
2. `.frontend.json` 不存在，`frontend={}` 不是JSON解析失败。末尾日志仍在 `1220s/1800s cycle=389` 心跳后加载generated.ts，没有前端完成/abort结果；该1220秒是墙钟，不能当主动时长。rAF16.0来自 `vis.rafPerSecond`，表示约每秒16次rAF回调，不是16ms帧间隔。
3. 源码显示：常规vmmap异常时若应用尚存活则重新抛错，外层finally若仍存活会terminate、必要时kill；前端仅在runSoak返回后一次性保存完整报告。这是确定的中断后证据丢失路径，**不是本次实际发送terminate的独立证据，也不是vmmap失败根因证明**。
4. 现有程序按缺失setupSeconds回落0计算的34笔热身后Rust footprint：首149.2MiB、最大231.2、末164.8、最大抬升82MiB、斜率−0.9827MiB/min。+82超过+10门槛；负斜率不取消失败，也不等于证明持续泄漏。暖机仍是180秒，不是无暖机；真实setup元数据缺失，不能事后挑选较高首样本或擅改暖机来消除结果。
5. 三笔压力后的原始内存数值仍保留，但完整阶段证据不足，不能改valid。最终有效压力样本0，WebContent footprint和malloc趋势均无法验收。

## 后续与完成边界

按Q109已将原Pi会话切到 `magpie/cpa/GLM-5.3-Flash`，先做只读根因诊断；采样错误/生命周期记录、完整checkpoint渐进留证与Rust抬升分开处理。保持Q98阈值、主动时长、负载、采样及同PID规则，不用静默重试、提高权限或跳过失败来制造通过。

任务10/11及M2仍未完成。尚需必要桌面补验和修复后新的完整长测；已有首轮失败不删除，旧任务01的基线通过也不被改写，但不能替代功能后的最终长测。未squash main，未启动M3实施。
