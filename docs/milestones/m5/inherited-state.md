# M5 继承状态

记录时间：2026-10-27。源码基线：`d58233c1bea3583cbec5868739b9618f7696eb99`。

## 分支与证据

- `main` 为 `eaf14d2`，M4 未合并、未推送；`m4` 保存于 `d58233c`，保留原 worktree。
- `m5` 从该检查点建立，不把 M4 未验收状态改成通过。
- M4 检查原件：`/private/tmp/revia-m4-codex.6O3W9y/`。类型检查通过，lint 通过但有 7 条原警告；Rust 389 通过、1 忽略；12 个 Node 脚本中 11 个通过，合并外观脚本因别名导入失败。
- M4 检查点的 build、重新打包、真实桌面、D06、冒烟、Windows 编译均未运行。旧 `.app` 不包含检查点改动。
- 原临时交接已备份至 `/private/tmp/revia-m5-start.zEV4wp/m4-original-handoff.md`，其中 `eba7446` 和未提交状态是接手前事实，不是当前状态。

## 交付阻塞：合并写入的全局保护

`MergeView` 的应用/接受一侧与 `ConflictDialog` 的接受一侧只使用局部 busy，直接调用 IPC。
尚未接 App 的 `writingWorktreesRef`、`WriteGuardOrchestrator`、`OperationResultGate` 与 `CloseCoordinator`，违反 Q90/Q91/Q96；Rust 写锁不能代替窗口关闭与退出保护。

后续修复沿用 `App.tsx` 中 `performRestore` 的编排：同步占同 worktree 写门，退出保护成功后才发送 IPC，目标捕获于发起时，刷新和通知仍归原目标；组件卸载后保留失败/待核对。所有收尾只释放一次，待核对不静默退出，刷新失败不覆盖原错误。

沿用现有 Node 测试里的生产协调器与 deferred promise，覆盖重复写拒绝、保护失败不发 IPC、关闭等待、跨 worktree 错误归属、卸载收尾、待核对阻止退出。三个 UI 入口都须接线验证。

任务 01 的提交历史读取完全独立于合并写入，可先做；完成任务 01 不关闭此问题。

2026-10-27 更新：规划方已在 `m4` 修复（`6553772`，`runMergeWrite` + `tools/test-merge-write.mjs`），并合并进 `m5`（`c21cecd`）。代码与 Node 回归已通过；三个入口在真实桌面上的行为尚未验收，验收前仍按阻塞项对待。

## 其他未解决项

- 以下三项已在 `m4` 的 `e9344f7` 修复并合并进 `m5`，未在真实桌面确认：
  - 统一软换行：同一长逻辑行内滚动时，可见末行变化可能未触发渲染。
  - 左右对照：正文/色带同帧，但行号/按钮仍由后续 rAF 更新。
  - 左右对照：Tab 聚焦预渲染按钮可能触发原生滚动，缺复位与焦点揭示。
- Rebased 偏好恢复未获确认，不修改；旧 `Revia-m2-ui`、`Revia-m3`、M4 worktree 保留。M4 与 M5 任务 01 的旧 Pi 会话已按用户要求归档（2026-10-27）。

完整背景以 [M4 报告](../m4/report.md) 和 [软换行计划](../m4/soft-wrap-plan.md) 为准。

## 2026-10-28 集成验收新增阻塞

第七个固定 Release（源码 `49ba903`）完成同一进程 1801.02 秒主动长测，未卡死，29/29 阶段样本有效；
Q98 的 WebContent 压力后 footprint 增长 24.0 MiB、趋势 +1.0048 MiB/分钟，超过既定 10 MiB / 0.5 MiB/分钟门槛。
malloc 与 Rust 两项通过，增长根因尚未确定，不能称整个长测通过，也不改门槛。
此为当前 M5 集成版本的验收失败，不撤销 M2 已保存的历史通过记录。固定包、完整原件与其余待验项见 [M5 报告 22.14](./report.md#2214-第七包正式长测与推送预览新失败)。
