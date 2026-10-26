# Q98 留证：一次返修复审及 Codex 接管

日期：2026-10-25。Pi 的一次返修仍不合格；按用户新确认的 Q113，Codex 直接接管，不再交回 Pi。以下两轴固定审查 `43e4ff51...85a2f5c5`；后面的接管修改不是该版本已经通过的证据。

## Standards

确定的规范违例 2 项，最高 P2；另有判断性 smell 1 项：

1. **ST-Q98-R2-1 / P2**：生产 main 的 RSS 失败分支把未初始化或上一轮 `sample` 交给候选保全。首次 ps 超时变为 UnboundLocalError，候选为空；后续超时混入上轮 RSS/footprint/timestamp。现有新 main 测试让 ps 总成功，未覆盖此确定缺陷，违背生产入口及结论可核验要求。
2. **ST-Q98-R2-2 / P3**：任务10声称删除候选假测试，但它仍只顺序调用 RSS/footprint，没有检查候选或 poll，删除陈述不实。
3. **possible Duplicated Code / P3（启发式）**：52 个测试定义有 9 个同名副本，实际只发现 43 项；43 通过的计数属实，但前定义会被后定义静默覆盖。

旧 ST-Q98-1 部分闭合，ST-Q98-2 已闭合。Pi 的30件清单原件及5条检查均核对匹配；检查通过不能抵消上述失败入口问题。完整轴报告随原件归档，不与 Spec 合并计数。

## Spec

4 项，最高 P1（2 P1、2 P2），范围扩张 0：

1. **R2-SP1 / P1**：同 runId 重复锚点和未闭合尾行仍被接受；真实 main 两例均 exit0/runCompleted=true。
2. **R2-SP2 / P1**：任一 checkpoint 集合为空时跳过核对，不比较 file/view/真实起止；main 对缺失或矛盾进度仍完成。helper 组合还能产生权威阶段与 Q98=true，后者不是完整 main 的内存通过复现。
3. **R2-SP3 / P2**：清理只在末尾唯一落盘；kill 抛错后仍无期限 wait。外部边界 mock 显示各动作调用时没有清理记录；未执行真实阻塞或信号。
4. **R2-SP4 / P2**：候选保全调用新增未初始化/跨轮 sample 回归。旧 Rust123 后 WebContent 解析失败原例已闭合，不能抵消新增错配。

旧 SP1/2/3 部分闭合，SP4 原例闭合但有新回归，SP5 已闭合。复现脚本均执行前保存，只 mock 系统边界，不启动真实 Revia、vmmap、通知或信号。

## Codex 接管修改与最小检查

Q113 独立流程提交 `22809f2`。原 Pi 已 idle，stop 返回 stoppedCount=0，没有再次派发。接管修改限定原 driver、既有 unittest 和 benchmark/任务文档：

- 每次采样先初始化本轮状态；ps 失败仍保留已取得的归属，不挪用上一轮 RSS/footprint。常规失败候选进入失败资格，保留旧启动取样节奏，不静默挑样本。
- 拒收任何重复锚点、损坏 JSON、未闭合尾行；绑定 fixture/output/target。最终报告必须有完整阶段集合，核对 file/view、真实起止、压力/遮挡、操作开始/setup 和当时累计计数；空集合或缺失不再跳过。
- 增量仍只作诊断；缺真实 setup 不猜暖机分界。无完整有效运行或是诊断模式时，整体 Q98 判定不标权威；流程完成与内存阈值判定分开。
- 清理在每个 terminate/kill/wait 前后立即追加日志，两次 wait 各限5秒；尝试、返回和交付未知分开，异常仍保留终态与汇总。清理失败或执行过清理不冒充应用自行成功完成。
- 删除9份完全相同的同名副本；候选假测试改为真实 main 两个 ps 故障时点，新增真实 main 的清理异常、自然退出竞态、解析部分值和损坏最终报告回归。notifyutil 超时经过真实阶段采样入口。

集中检查原件：`/private/tmp/revia-q98-root-takeover.3EEtAR/checks.SZuTne`。实际 argv/cwd/显式非敏感环境/起止/exit/完整 stdout+stderr 执行时落盘；隔离 HOME/TMPDIR/Git 配置。Python **46 passed**，源码契约检查和 diff-check 均 exit0。AST 为46个定义/46个唯一测试/0同名副本；4份受核文件前后 SHA 一致，App.tsx 与85a2f5c逐字节相同，7个 Q98/阶段常量均未变。

本轮只改 Python 和文档，因此没有重复 Pi 已核的前端检查、全量 Rust 或无关 Node。旧 Release 仍来自2a51b160，Rust +82MiB 和 vmmap255 根因仍未解决；本次不是 M2/Q98 通过。接管源码尚须新的独立双轴复核，通过后才构建新版 Release 和完整30分钟长测。

## 原件边界

见 [manifest](./evidence/q98-evidence-review02/manifest.json)，区分 Pi 交付、两个独立轴复现及 Codex 检查。原始失败、旧检查及第一轮审查不回写。Codex 曾误在 Pi 原证据目录追加一次纯文档 diff-check，覆盖了 aggregate invocations.json；已保存追加后的快照和第6次新调用，再按原 manifest 的字节数/SHA 恢复原5次 aggregate。旧命令原始流未覆盖，Pi全部清单文件现逐份匹配原hash；不能把此事写成“原目录从未被改动”。事件与恢复核验单独归档，不归为 Pi 错误。

任务10保持 ready-for-agent，任务11/M2未完成，未合 main、未启动 M3。
