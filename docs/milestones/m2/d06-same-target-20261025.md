# 原目标同次 DOM / 首帧观测

记录时间：2026-10-25T23:57:43.076055+08:00。产品增量 `50018c6...b185f6c`；仅改 `App.tsx`、`jumpDiagnostics.ts`、`measure.ts` 的显式测量路径。普通滚动、Rust及长测路径未改。用户最新要求不再测试；以下全部为此前已完成的结果。

## 实现与边界

- 在原60%外跳及原返回的**同一次操作**登记DOM和rAF探针，保留旧appear/warm/分段数据；不预热、不增加取数，不用独立80%探针替代原目标。
- 同步DOM核验增加旧dataWait，回跳前新增首帧屏障，不能称新旧运行调度完全一致。16.7/100ms阈值不变。
- 可观察正常渲染机会前的文字/占位，不等于GPU合成完成或硬件上屏；首帧有字不等于首帧全色。
- 143份产品源码在构建/四组测量前后相同；新版.app与裸binary SHA-256 `b7ec64a2ba602196f6fa2bb34fef87cfd9a38955259f9de625f2f92f130bf0a3`。

## 已有检查与四组实测

现有Node探针11通过、lint通过；首次build受沙箱EPERM阻止，提权重跑build通过，diff-check通过。固定两轴Standards确定0/Fowler0、Spec确定0/扩张0，原报告归档。Release构建09退出0；独立分析16退出0。上述不是用户暂停测试后新增运行。

| 场景 | 首屏ms | 旧appear/warm | 同次外跳文字/首帧ms | 同次返回文字/首帧ms | 滚动最大帧ms |
| --- | ---: | --- | --- | --- | ---: |
| 标准并排 | 127 | 11/4 | 4/15 | 2/16 | 23 |
| 标准统一 | 117 | 14/4 | 9/15 | 1/15 | 18 |
| 真冷并排 | 139* | 12/4 | 5/16 | 1/11 | 23 |
| 真冷统一 | 127* | **19/4** | 11/16 | 1/11 | 18 |

*30000源行冷负载首屏独立报告，不混标准150ms门槛。冷外跳实际缺35行，返回cached；八次同次目标的frameReady/textWithinBudget均通过、首帧35/35文字、0占位。四组各60真rAF、0fallback、0uncovered，无hiddenTransitions/focusLosses，各正常Cmd-Q退出0。不要求慢帧清零。

旧冷统一19ms的withinOneFrame=false保留；旧版18/22/28ms不回写为通过。独立80%返回严格全色33/33/30/32ms，首帧非全色，继续保留。结论只支持**本次同目标Q52文字/占位预算通过**，不声称历史失败消失。

## CI 与归档

- 原SSH正常推m2：首轮agent失败exit128保留，解锁后重试exit0。未切HTTPS或改凭据。
- 已在运行的 [Compile Check 40208392193](https://github.com/GarlandQian/Revia/actions/runs/40208392193)，固定 `b185f6cc18265181f917d0f18b04dad89492510c`，最新归档状态 **in_progress / pending**：macOS Apple Silicon (arm64): completed/success; Windows x64 (msvc): in_progress/pending。读取状态不触发新CI；未取得终态前不写新版双平台全部通过。
- [完整归档清单](./evidence/d06-same-target-20261025/manifest.json)含源码/产物SHA、四份JSON/log/AX/截图、原审查报告、失败调用、独立分析及CI状态。现场 `/private/tmp/revia-m2-d06-samejump.dtLTuT` 保留。
- Q98仍绑定 `59b925d`，主动1801.023秒的三条原门槛均通过，见[原报告](./q98-rust-memory-20261025.md)。本观测增量不冒称重新长测。
