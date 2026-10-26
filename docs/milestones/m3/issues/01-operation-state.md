# 01：进行中操作状态与提交门禁

**What to build:** 实现M3规格F01；F03的写前操作复核。

**Blocked by:** 无

**Status:** ready-for-human

初始Q114批次只实现功能；现按Q115解除执行限制并进行验证，未完成验收不伪标done。

- [x] Rust读取真实gitDir/Git状态并覆盖linked/sequencer
- [x] 中文常驻提示和冲突/操作状态提交禁用
- [x] 刷新/前台/切换与提交前重新核对
- [ ] 后续验证（Q115已放行，未完成验收不宣称通过）

## Comments

2026-10-26 / Codex：从m2 073ecfe建立独立m3，整包功能实现，禁止启动GUI/唤醒/测试/构建/lint/tsc/CI。

2026-10-26 / Pi（实施方）：功能实现完成，**未验证**（用户 Q114 只实现功能）。

实现位置与要点：

- `src-tauri/src/operation.rs`：`OperationKind` / `OperationState` / `detect` / `conflict_count` /
  `resolve_dirs`；标记路径全部由 `git rev-parse --git-path <name>` 让 Git 自己解析，
  因此 linked worktree 的私有 gitDir 与共享 commonDir 分工正确，不会误判主 worktree 的状态。
  覆盖 `MERGE_HEAD`、`rebase-merge`、`rebase-apply`、`CHERRY_PICK_HEAD`、`REVERT_HEAD`、
  `sequencer`（多提交序列中间阶段，按待办首行区分）。解析/读取失败一律显式报错，
  不当作"无操作"；同目录多种互斥标记显式报错，不猜优先级。
- `src-tauri/src/commit.rs::capture`：提交预期把进行中操作状态（种类、sequencer、冲突数）
  纳入指纹，执行前重新读取比较。
- `src-tauri/src/commit_flow.rs::run_commit_flow`：冲突禁用提交；rebase/cherry-pick/revert
  进行中禁用；merge 无冲突允许普通提交完成合并；任意进行中操作禁用 amend。
- `src/components/app/CommitComposer.tsx::operationHint`：常驻中文提示条（操作类型 +
  "请到终端处理" + 不提供 continue/abort/skip）。
- 刷新、切换 worktree、回到前台与写前都会重新读取上下文（`commit_context`），不只信 UI。

未执行：`cargo test` / `cargo check` / build / lint / tsc / GUI / 桌面验收。复选框中的
"后续验证"保持未勾选。

### 2026-10-26 规划方静态审查（唯一一次正式返修）

静态审查：无operation标记的unmerged门禁漏判；操作预期未绑定真实MERGE_HEAD等内容；rebase内部标记并存需正确识别。一次返修补齐，禁止运行验证。


### 2026-10-26 Pi（实施方）返修完成

已按审查意见补齐（未运行任何验证）：

- **index unmerged 独立门禁**：`CommitExpectation` 新增 `unmerged_paths()`/`has_unmerged()`，
  `capture()` 直接以 index 的 unmerged 条目数作为冲突数（不依赖 operation 标记），
  `run_commit_flow` 与 `commit_gates` 都先检查 `has_unmerged()`——`git stash pop` 之类
  留下冲突但没有 MERGE_HEAD 的情况也会被拦（`commit_flow.rs`、`commit.rs`）。
- **操作预期绑定真实元数据**：`OperationState` 新增 `metadata_version`，对全部存在的标记
  内容（MERGE_HEAD、rebase-merge/rebase-apply 状态文件、CHERRY_PICK_HEAD/REVERT_HEAD、
  sequencer 待办）做分片哈希；`fingerprint_of` 纳入该版本。同一种操作内容变化时旧确认
  失效（`operation.rs`、`commit.rs`、`CommitExpectation` TS 绑定）。
- **rebase 内部标记不误判**：probe 按 rebase > merge > cherry-pick > revert 优先级，
  rebase 期间的 CHERRY_PICK_HEAD 视为从属标记，不报互斥异常；仅 merge 与 rebase 并存等
  真实异常才显式报错，且仍硬门禁提交（`operation.rs::probe`）。

### 2026-10-26 Codex 直接接管收尾

补齐非 UTF-8 提交预期的 fail-closed、真实操作元数据与合法 rebase --rebase-merges 从属 MERGE_HEAD；完整状态读取仍在 Rust 写前重新进行。

功能代码已提交，未经编译/测试；本轮不打开 GUI，不运行任何验证，不勾选后续验证，不将 ready-for-human 当作验收通过。历史实施描述以本段和最终报告为准。


### 2026-10-26 Q115 验证恢复（Codex）

用户已解除不测试/不启动限制。固定源码`03736a0`的自动化验证已执行：Node 251项、Rust 304项通过，构建/类型/lint及固定Release只读冒烟通过；失败原件和覆盖范围见[验证记录](../validation-20261026.md)。原生桌面读取超时，未完成真实交互/截图、D06/完整长测及Windows编译；Status与最终验证复选框保持ready-for-human/未勾选，不冒称done。未取消休眠、不回派Pi、不推送或合并。
