# 03：固定提交契约、实时hooks与安全写入

**What to build:** 实现M3规格F03、F04。

**Blocked by:** 01, 02（实现顺序）

**Status:** ready-for-human

初始Q114批次只实现功能；现按Q115解除执行限制并进行验证，未完成验收不伪标done。

- [x] 整个提交预期而非单文件指纹
- [x] 真实git commit、登录shell环境、实时输出、仅本次跳过hooks
- [x] 共享写门/退出保护/结果归属/待核对，不杀Git不自动重跑
- [ ] 后续验证（Q115已放行，未完成验收不宣称通过）

## Comments

### 2026-10-26 桌面收尾（Codex）

隔离 Release 桌面中真实 pre-commit 返回 1，草稿、工作区及 index 保留，回收记录实际为 Completed 且包含真实 after；结果面板却无条件把“提交失败且有快照”显示成“提交前快照收尾失败”。删除该错误推断，收尾异常继续由后端 verificationNote/needsVerification 的真实结果展示。原始失败截图与记录保留；修复后另做受影响的前端构建、Release 与桌面验证。

2026-10-26 / Codex：从m2 073ecfe建立独立m3，整包功能实现，禁止启动GUI/唤醒/测试/构建/lint/tsc/CI。

2026-10-26 / Pi（实施方）：功能实现完成，**未验证**（用户 Q114 只实现功能）。

实现位置与要点：

- 固定契约：`src-tauri/src/commands.rs::execute_commit` 只接受
  `worktreeId` + `CommitOptions { message, amend, skipHooks, expected, amendUpstreamConfirmed }`；
  前端（`src/lib/ipc.ts::executeCommit`）不传任何 Git 参数或命令。
- 整个提交预期：`src-tauri/src/commit.rs::CommitExpectation` 覆盖 HEAD（含 unborn）、分支、
  完整暂存区条目（路径/原路径/模式/对象/状态/冲突）与进行中操作，并有整组指纹；
  `run_commit_flow` 执行前重新 `capture` 并逐项比较。
- 真实 `git commit`：`commit.rs::execute_commit` 用会话的登录 shell 环境直接 spawn，
  `-F -` 从 stdin 传信息（多行/Unicode 不经 shell 解释）；amend → `--amend`；
  跳过 hooks → `-c core.hooksPath=<临时空目录> --no-verify`（本次命令全跳过，不改配置）。
- 实时输出：stdout/stderr 各起线程逐行读取，emit `revia://commit-output`
  （`operationId` / `worktreeId` / `stderr`），两侧各自有界（Rust 2000 行、前端 2000 行）；
  不设超时、不 kill、不自动重跑。
- 写门与归属：Rust `acquire_write_lock`（同 worktree 非排队）+ 前端 `WriteGuardOrchestrator`
  （`CloseDriver` / `CloseGuardGate` / 退出保护）；结果与错误按原 worktree 记录，
  `refreshTarget` 对原目标真实重读，切走不覆盖当前界面。
- 结果区分：`SpawnFailed`（快照 NotApplied）与 `Uncertain`（`needsVerification`，快照保持无回执、
  草稿保留）分开；错误仍是 Rejected / Git / Environment + NeedsVerification 四类；
  不删除 Git 锁、不添加签名设置。
- hooks 改动对照：`commit_snapshot.rs::HookBaseline::diff_against` 比较
  「提交前暂存区 / 新提交的 tree / 提交后暂存区」三者 + 相关路径工作区字节 +
  提交后新出现的 status 路径；提交成功导致暂存区与新 HEAD 一致**不算** hook 改动，
  避免误报"commit 成功后暂存内容消失"。

未执行：`cargo test` / `cargo check` / build / lint / tsc / GUI / 桌面验收。复选框中的
"后续验证"保持未勾选。

### 2026-10-26 规划方静态审查（唯一一次正式返修）

静态审查：仅暂存删除误判为空、合法空差异merge被拦；输出单行无界/非UTF8停读、事件未核操作ID；失败结果丢结构且不总刷新；模板/上游读取吞错。一次返修补齐，禁止运行验证。


### 2026-10-26 Pi（实施方）返修完成

已按审查意见补齐（未运行任何验证）：

- **仅暂存删除不再误判为空**：`read_index_entries` 除 ls-files 的 stage 0 条目外，从
  status 补出「暂存删除」（mode=`000000`、oid=None、`deleted=true`、`headOid` 为被删的
  HEAD 对象），指纹覆盖删除与 `intentToAdd` 标记；intent-to-add（`git add -N`）有占位
  条目但不参与 `has_staged_changes`，也不算真实 blob（`commit.rs`、TS 绑定）。
- **合法空差异 merge 收尾**：`OperationState::allows_empty_diff_commit()`（merge 且无冲突）
  与 `CommitExpectation::allows_empty_diff_commit()`；`commit_flow` 普通提交门禁放行，
  `CommitGates`/`CommitComposer` 前后端一致。
- **实时输出分片读取**：`read_stream` 改固定大小分片 `read`，非 UTF-8 用有损转换继续
  drain，单行超过 8 KiB 先交付，尾部按字节（256 KiB）有界；前端单条截断 + 总字节 256 KiB
  收敛（`commit.rs`、`App.tsx::trimOutputByBytes`）。
- **事件归属校验**：前端发起时生成 `clientNonce` 随请求传后端，后端校验字符集并原样回带；
  `CommitOutputEvent` 携带 nonce，前端只接收 nonce 匹配的当前这次事件（`ipc.ts`、`App.tsx`、
  `commands.rs`、`commit_flow.rs`）。
- **已确认失败结构化返回**：非零退出返回 `CommitOutcome{success=false, error: Git类}`，
  保留 output、hookChanges、snapshotRecordId、expectationAfter；前端总是刷新原 worktree
  并显示 Git 类错误（`commit_flow.rs`、`App.tsx`）。
- **模板/上游读取不吞错**：`read_template` 用 `git config --path --get commit.template`
  （展开 `~/`），未设置(退出码1)与读取失败(其它退出码)区分；失败进 `templateError` 可见但
  不阻断读草稿与提交。`read_upstream_config` 用 `branch.<name>.remote/merge` 区分
  未配置 / 已配置但本地无引用(Unknown) / 读取错误。`read_head` 用 `show-ref --verify`
  确认引用**确实不存在**才算 unborn，引用存在但解析失败按损坏 fail-closed
  （`commit.rs`、`CommitContext` TS）。

### 2026-10-26 Codex 直接接管收尾

修复异常退出、wait 失败持有 Child 直到真正回收、非零退出时 HEAD 异常变化待核对；按 worktree 隔离提交忙碌；输出无换行立即交付，跨分片 UTF-8 保留，前后端真实字节有界。

功能代码已提交，未经编译/测试；本轮不打开 GUI，不运行任何验证，不勾选后续验证，不将 ready-for-human 当作验收通过。历史实施描述以本段和最终报告为准。


### 2026-10-26 Q115 验证恢复（Codex）

用户已解除不测试/不启动限制。固定源码`03736a0`的自动化验证已执行：Node 251项、Rust 304项通过，构建/类型/lint及固定Release只读冒烟通过；失败原件和覆盖范围见[验证记录](../validation-20261026.md)。原生桌面读取超时，未完成真实交互/截图、D06/完整长测及Windows编译；Status与最终验证复选框保持ready-for-human/未勾选，不冒称done。未取消休眠、不回派Pi、不推送或合并。
