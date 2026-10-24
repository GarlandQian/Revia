# 08：写入期间安全关闭与结果待核对

**What to build:** 开发者在任一 worktree 仍有写操作时请求关闭或退出，可以等待正常完成或留在应用，不因窗口关闭而打断 Git，不确定结果必须可见。

**Blocked by:** 05

**Status:** ready-for-human

依据：[M2 规格](../spec.md)实现决策 3、7，T06、T11、D05；Q90、Q91、Q96。

- [ ] 覆盖暂存、取消暂存、丢弃、恢复的在途生命周期，切换到其他 worktree 后也不能漏掉仍在执行的操作。
- [ ] 关闭窗口或退出先保留窗口与进程，提供“等待完成后关闭／退出”和“留在应用”，不为关闭直接杀掉 Git。
- [ ] 正常完成并记录结果后执行原请求；留在应用不取消写操作，不把关闭请求改成不同的退出动作。
- [ ] 待核对结果先展示，不静默退出、不自动续跑、不承诺超时没有副作用；恢复后的下一次写入使用新状态。
- [ ] 受控执行阶段中断验证保存记录仍在、不会在重启后盲目重跑；实际关闭／退出交互由真实桌面检查。

## Comments

### 2026-10-24 / Pi 实现：写入期间安全关闭与退出（Q90/Q91/Q96）

#### 设计

关闭/退出的决策收在共享层 `CloseCoordinator`（`src/lib/refreshGate.ts`），
App 与 Node 回归**用同一个类**，不各写一份判断：

- 写操作按 **worktree** 记在途：切到别的 worktree 后仍在执行的操作也算在途（Q90）；
- `request(intent)` 在无在途写、无待核对时返回 `proceed`；否则**保留**窗口与进程，
  记下**原请求**（`close` / `exit` 原样保存，不升级成强退）并返回 `ask` / `showPending`；
- `keepOpen()`（留在应用）只清掉请求，**不取消**写操作、不改变动作类型；
- `takeReadyIntent()` 在写操作全部结束且无待核对时返回原请求，调用方据此执行；
- `endWrite(id, needsVerification)` 把「结果无法确定」记为待核对，
  `request` 遇到待核对先返回 `showPending`，**不静默退出**（Q91）；
- 编排里**没有**重试/重跑入口：从不杀 Git、不重跑 stage/discard/restore。

#### 接入点

- `performWrite`（暂存/取消暂存/丢弃，含 hunk）与 `performRestore`：进入时 `beginWrite`，
  `finally` 里 `finishWrite(worktreeId, needsVerification)`；
- `performSaveAs`：取得真实归属门禁后 `beginWrite`，`finally` 里 `finishWrite`；
- `finishWrite` 结束后若条件满足，执行**原请求**（`performCloseIntent`）；
- 前端监听 `onCloseRequested`：有在途写时 `event.preventDefault()` 并弹出
  `CloseRequestDialog`（「等待完成后关闭」/「留在应用」）；
- 新增能力 `core:window:allow-close`、`core:window:allow-destroy`
  （`core:window:default` 不含它们，已核对生成 schema）。

#### 回归（Node，生产入口）

`tools/test-refresh-gate.mjs` 新增 9 项，全部驱动同一个 `CloseCoordinator`：
无写操作时立即执行、有在途写时保留并询问、切换 worktree 后仍算在途、
留在应用不取消写也不改变动作、等待后执行**原请求**（close/exit 各自原样）、
多个 worktree 全部完成后才执行、待核对先展示且确认后才执行、
待核对不被下一次写入伪清除、编排没有重试入口。

#### 未验证（必须保留）

- **真实桌面关闭/退出交互未验证**（任务单明确由真实桌面检查）。
- 未构建 Release、未启动 GUI、未跑 D06/双平台 CI/长测；未推送/合并/发布。
