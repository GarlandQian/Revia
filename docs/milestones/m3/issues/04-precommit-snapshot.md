# 04：部分暂存提交前快照与回收区接线

**What to build:** 实现M3规格F05。

**Blocked by:** 03（实现顺序）

**Status:** ready-for-human

初始Q114批次只实现功能；现按Q115解除执行限制并进行验证，未完成验收不伪标done。

- [x] 完整工作区快照且失败阻断commit
- [x] commitSnapshot来源、后基准、查看/恢复/清理
- [x] 保存后再核对，无未知结果伪基准
- [ ] 后续验证（Q115已放行，未完成验收不宣称通过）

## Comments

2026-10-26 / Codex：从m2 073ecfe建立独立m3，整包功能实现，禁止启动GUI/唤醒/测试/构建/lint/tsc/CI。

2026-10-26 / Pi（实施方）：功能实现完成，**未验证**（用户 Q114 只实现功能）。

实现位置与要点：

- `src-tauri/src/commit_snapshot.rs::partial_staged_paths`：从 `git status --porcelain=v2`
  找同时出现在已暂存与未暂存组的路径。
- `save_snapshot`：保存**完整工作区状态**（存在状态、字节、可执行位）到现有回收区，
  来源 `RecoverySource::CommitSnapshot`，不使用 stash/reflog、不存部分补丁；
  返回记录 + 占用标记（覆盖保存→提交→收尾，期间清理不删）+ `SnapshotBaseline`。
- 顺序保证：快照保存失败直接返回、不启动 commit；保存后到启动前用
  `SnapshotBaseline::matches_now` + 再次 `commit::capture` **再核对**，变化即拒绝并保留已存记录。
- 收尾：成功时重新读取**真实**当前状态作为 `DiscardOutcome::Completed { after }`；
  提交失败/未启动记 `NotApplied`；结果不确定或收尾失败保留 `NeedsVerification`（不伪造 after）。
- 回收区：`recovery.rs` 增加 `CommitSnapshot` 与 `label`；`RecoveryDialogs.tsx` 按来源区分文案，
  提交快照显示"提交前快照（提交时的完整工作区状态，未丢弃任何内容）"，状态用提交语义，
  不出现"已丢弃"。既有查看 / 另存 / 恢复 / 14 天清理机制原样复用（恢复只写工作区、不改 index）。

未执行：`cargo test` / `cargo check` / build / lint / tsc / GUI / 桌面验收。复选框中的
"后续验证"保持未勾选。

### 2026-10-26 规划方静态审查（唯一一次正式返修）

静态审查：hook已确认失败后应记录真实after而非NotApplied；hook基准拒绝普通symlink/gitlink提交，且漏报其他原有dirty/干净tracked路径变化。一次返修补齐，禁止运行验证。


### 2026-10-26 Pi（实施方）返修完成

已按审查意见补齐（未运行任何验证）：

- **已确认失败也保存真实 after**：`run_commit_flow` 在 Finished（无论成功失败）后统一先
  读真实工作区状态并以 `DiscardOutcome::Completed{after}` 收尾快照；只有 `SpawnFailed`
  （命令从未启动）才 `NotApplied`；读取/落盘失败保留无回执 → NeedsVerification，不伪造基准
  （`commit_flow.rs`）。
- **hook 基准不再拒绝 symlink/gitlink**：`capture_hook_baseline` 改用 `digest_path`
  （`symlink_metadata`，不跟随链接；符号链接按目标摘要、gitlink 按 Submodule 类型、普通
  文件分片 SHA-256），普通提交即使含已暂存 symlink/gitlink 也可进行；部分暂存快照仍用
  `recovery::capture_files`，无法无损保存时 fail-closed 并给出明确提示
  （`commit_snapshot.rs`）。
- **hook 报告覆盖全部 tracked + 原 dirty/untracked**：baseline 取 `ls-files` 全部 tracked
  ∪ 本次提交路径 ∪ 当时 status 路径（含 rename 源）的摘要；diff 时再加当前 tracked/status
  路径，比较「新出现/消失/内容类型模式变化」，读取失败显式报错而不是悄悄当无改动；
  index 侧仍以「提交前暂存区 / 新 tree / 提交后暂存区」三者对照，正常提交后暂存区==新
  tree 不算 hook 改动（`commit_snapshot.rs`）。

### 2026-10-26 Codex 直接接管收尾

所有快照保存后的启动前错误统一 NotApplied；已确认失败真实 after；未确定结果及收尾失败保留草稿与关闭保护。hook 摘要拒绝越界/链接父路径，叶子链接仍按目标摘要。

功能代码已提交，未经编译/测试；本轮不打开 GUI，不运行任何验证，不勾选后续验证，不将 ready-for-human 当作验收通过。历史实施描述以本段和最终报告为准。


### 2026-10-26 最终静态收尾（Codex，`1ca1e52`）

Standards P2 子模块 hooks 改动漏报已修：只读比较子模块真实 HEAD、完整 index 与内部 tracked/untracked 文件摘要，包含父 index 未变的 checkout 与已脏文件继续编辑。报告归父仓库子模块路径；不跟随链接父路径、不改 index、不自动撤销。

仅源码实现及静态阅读；未运行任何编译、测试或 GUI，后续验证项仍未勾选。


### 2026-10-26 Q115 验证恢复（Codex）

用户已解除不测试/不启动限制。固定源码`03736a0`的自动化验证已执行：Node 251项、Rust 304项通过，构建/类型/lint及固定Release只读冒烟通过；失败原件和覆盖范围见[验证记录](../validation-20261026.md)。原生桌面读取超时，未完成真实交互/截图、D06/完整长测及Windows编译；Status与最终验证复选框保持ready-for-human/未勾选，不冒称done。未取消休眠、不回派Pi、不推送或合并。
