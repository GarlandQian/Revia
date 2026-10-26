# M3 功能实现报告

**Status:** ready-for-human（功能实现完成，Q115已放行验证，结果见末尾追记）

## 初始功能批次执行边界（Q114历史）

- 从 m2 `073ecfe` 建立的独立 `m3` worktree，分支 `m3`，基线提交 `1fd4fad`。
- 用户 Q114：本批**只实现功能**。整个批次**没有运行**任何 test / build / tsc / lint /
  `cargo check` / GUI / 测量 / 长测 / CI；没有启动应用、没有要求解锁或唤醒。
- 因此本报告**不包含任何验证通过结论**，也不能代替验收。`issues/*.md` 里的
  「后续验证」复选框保持未勾选。
- M2 未验项与 main 用户改动保持原样；本批未改 M2 阈值或历史结果。
- 未推送 `m3` 或 `main`，未合并 main，未开 PR，未发布。

## 实现范围

规格 [spec.md](./spec.md) F01–F06 全部有真实实现与接线，没有 TODO、`disabled` 占位
或"用文档代替功能"。

### 新增文件

| 文件 | 内容 |
| --- | --- |
| `src-tauri/src/operation.rs` | F01：进行中操作识别（merge/rebase/cherry-pick/revert + sequencer + 冲突数） |
| `src-tauri/src/commit.rs` | F02/F03/F04：提交预期与指纹、模板、最近 20 条信息、上游读数、每 worktree 持久草稿、真实 `git commit` 执行与实时输出 |
| `src-tauri/src/commit_flow.rs` | F01/F03/F04/F05/F06 编排：写门 → 预期复核 → 门禁 → 快照 → hooks 基准 → 再复核 → 提交 → 收尾 |
| `src-tauri/src/commit_snapshot.rs` | F05/F04.3：部分暂存完整工作区快照、提交前/后基准、hooks 改动对照 |
| `src/components/app/CommitComposer.tsx` | F02/F06 提交区：信息框、72 字符提示、模板/最近信息、amend、跳过 hooks、门禁提示、审阅进度、⌘K、结果区 |
| `src/lib/commitDraft.ts` | F02.5 草稿状态机：worktree 绑定 + 代际、旧回包不覆盖新输入 |
| `src/types/bindings/Commit*.ts`、`Operation*.ts`、`UpstreamStatus.ts` | 手工同步的 TS 契约（未运行 `cargo test` 生成绑定） |

### 修改文件

| 文件 | 内容 |
| --- | --- |
| `src-tauri/src/commands.rs` | 新增 `commit_context` / `amend_head_message` / `save_commit_draft` / `clear_commit_draft` / `execute_commit`；`AppState` 增加 `drafts` 与 `data_dir` |
| `src-tauri/src/lib.rs` | 注册 `commit` / `commit_flow` / `commit_snapshot` / `operation` 模块与 5 个命令 |
| `src-tauri/src/session.rs` | `WorktreeSession::captured_identity`（草稿归属身份） |
| `src-tauri/src/recovery.rs` | `RecoverySource::CommitSnapshot` + `is_commit_snapshot` / `label` |
| `src/App.tsx` | 提交区接线、草稿读写与串行化、共享写门/退出保护/待核对、结果与错误按原 worktree、审阅刷新、⌘K 快捷键、实时输出事件 |
| `src/components/app/RecoveryDialogs.tsx` | 回收区来源与状态文案按 `commitSnapshot` 区分，不把提交快照称为"已丢弃" |
| `src/lib/ipc.ts`、`src/lib/platform.ts`、`src/lib/i18n/zh-CN.ts`、`src/index.css` | 固定类型 IPC、⌘K/Ctrl+K 与平台差异、集中中文文案、提交信息框样式 |

## 关键实现点（对照规格）

### F01 进行中的操作

- 标记路径由 **Git 自己解析**：`git rev-parse --path-format=absolute --git-path <name>`。
  不手工拼 `.git/xxx`，因此 linked worktree 的私有目录与共享目录分工由 Git 决定，
  不会把主 worktree 的进行中操作误判成当前 worktree 的。
- 覆盖 `MERGE_HEAD`、`rebase-merge`、`rebase-apply`、`CHERRY_PICK_HEAD`、`REVERT_HEAD`、
  `sequencer`（多提交序列中间阶段，用待办首行区分 cherry-pick / revert）。
- 解析或读取失败**显式报错**（`AppError::environment`），不当作"无操作"；
  同一目录出现多种互斥标记也显式报错，不猜优先级。
- 有冲突一律禁用提交；merge 无冲突允许普通提交完成合并；rebase/cherry-pick/revert
  进行中禁用提交；任意进行中操作禁用 amend。
- 提示条常驻在提交区顶部（`operationHint`），说明操作类型与"请到终端处理"，
  不提供 continue/abort/skip。
- 刷新、切换 worktree、回到前台都会重新读取上下文（`commit_context`）；
  执行提交时 Rust 再次读取真实状态并逐项比较，不只信 UI。

### F02 提交信息与持久草稿

- 提交区在变更工具窗口下方，沿用岛屿布局、Inter/JetBrains Mono 与主题变量。
- 首行按**字符**（`Array.from`）计数，超过 72 只提示、不阻断；空白信息不能提交。
- 普通提交只提交真实暂存区，不自动暂存、不改工作区（Rust 侧以 `ls-files --stage`
  的真实条目判断，不提供 allow-empty）。
- `commit.template` 只读取、不修改 Git 配置；仅在**没有草稿且用户未输入**时初始化一次。
- 最近 20 条提交信息（`git log -z --format=%H%x01%B`，保留多行与 Unicode）；
  根仓库无 HEAD 返回空而不是报错。
- 草稿按 worktree **身份**（真实路径 + gitDir + commonDir + 文件系统标识）哈希落盘，
  同路径重建不继承；读取失败可见但不阻断提交；保存失败明确显示且保留本地输入。
- 前端 `CommitComposerState` 用 worktree + 代际判定异步落地：旧读取不覆盖新输入，
  写入串行排队（后一次一定最后落盘），输入停顿 400ms 才写盘，提交前立即冲刷。
- 成功只清除"仍是本次输入"的草稿（正文、amend 与草稿 revision 在同一锁内完全匹配才清），
  期间新输入不会被抹掉；失败/待核对保留草稿。

### F03 固定类型提交入口与指纹

- 前端只传 `worktreeId`、信息、`amend`/`skipHooks` 与**用户所见** `expected`；
  不传任何 Git 参数或命令。
- 提交预期（`CommitExpectation`）覆盖：HEAD（含 unborn）、分支、**完整暂存区**
  （路径/原路径/模式/对象/状态/冲突）、进行中操作状态，并有整组指纹。
- 执行前重新 `capture` 并与前端预期逐项比较，不一致即拒绝并要求重新确认。
- 使用既有同 worktree 写锁（`acquire_write_lock`，非排队）+ 前端
  `WriteGuardOrchestrator`（`CloseDriver`/`CloseGuardGate`/退出保护）；
  结果与错误按**原 worktree** 记录（`commitResults` 以 worktree ID 为键），
  切走后不覆盖当前界面，切回来仍能看到那次提交的输出；错误走既有 `setAppError(err, target)`。
- 写前失败/启动失败（`SpawnFailed`，快照记 NotApplied）与已启动后结果不确定
  （`Uncertain` → `needsVerification`，快照保持无回执、草稿保留）分开；
  不自动重跑、不杀 Git、不提供取消。
- 错误仍分三类（`Rejected` / `Git` / `Environment`）+ `NeedsVerification`；
  Git 锁从不删除；签名完全按用户 Git 配置，未添加任何签名设置。

### F04 hooks 与实时输出

- 用当前 worktree 的登录 shell 环境直接 spawn `git commit`（`env_clear` + 会话环境），
  默认正常运行 hooks；不另拼 shell 命令。
- stdout/stderr 各起线程固定分片读取并实时 emit `revia://commit-output`
  （带 `operationId`、`worktreeId`、`stderr` 标记），避免单流读满管道死锁；
  Rust 侧输出尾部限 256 KiB，前端每 worktree 也按 UTF-8 字节限 256 KiB；无换行提示立即交付，保留跨分片 UTF-8 字符。
- 输出区可展开、可复制；实时事件按 worktree 归属更新它自己的结果区，
  切走后不串到当前目标，切回来仍保留。
- "跳过 hooks"默认 false，仅本次命令生效：`-c core.hooksPath=<临时空目录> --no-verify`，
  进程结束即失效，不改用户 Git 配置；`--no-verify` 同时跳过 pre-commit 与 commit-msg，
  加上空 hooksPath 后本次调用的**全部** hooks 都找不到可执行文件（不冒充"全跳过"以外的能力）。
- 非零退出保留真实输出，并**重新读取真实状态**列出 hooks 改动文件（工作区 + 暂存区），
  不自动撤销；不假定 hook 失败意味着工作区/index 没变化。
- hook/签名等待期间不设超时、不杀进程；关闭仍走 M2 的 Wait/Stay。

### F05 提交前快照

- 提交前找出**部分暂存**文件（同一路径同时在已暂存与未暂存组），把它们的**完整工作区
  状态**（存在状态、字节、可执行位）存入现有回收区；不使用 stash/reflog、不存部分补丁。
- 快照落盘失败则**不启动** commit（错误直接返回）。
- 保存后到启动前**再次核对**：提交预期 + 这些路径的完整工作区状态；变化即拒绝并保留已存记录。
- 来源固定 `commitSnapshot`，与 discardFile/discardHunk 区分；复用既有查看/另存/恢复/
  14 天清理机制；恢复只写工作区、不改 index、不覆盖后续修改（沿用 M2 的
  `restore_with_hook` + `verify_publish` 基线判定）。
- 成功或 HEAD 未变化的已确认失败保存**真实**操作后基准；仅 Git 未启动时记 `NotApplied`；
  结果不确定/收尾失败保留 `NeedsVerification`，不伪造 after。
- 回收区文案按来源区分：提交快照显示"提交前快照（提交时的完整工作区状态，未丢弃任何内容）"，
  状态文案也用提交语义（`completedCommit` / `notAppliedCommit` / `needsVerificationCommit`）。

### F06 amend、安全确认与完成反馈

- 勾选"修改上一次提交"时读取用户所见 HEAD 的完整信息预填；请求显式携带 `expectedHead`，
  Rust 按固定对象读取并在前后复核 HEAD；前端绑定输入/刷新/激活代际及 HEAD，迟到回包不覆盖。
  正文保留自带尾随换行；无 HEAD 或任意进行中操作时禁用。
- 以**本地上游跟踪分支**判断 HEAD 是否已包含于上游（`merge-base --is-ancestor`），
  不 fetch；已包含时先弹二次确认，确认绑定整组提交预期、信息与选项、`upstreamVersion`，并由激活、刷新与输入代际使旧确认失效。
- 界面显示"截至上次 fetch"的本地记录边界；本地没有上游引用时如实显示"无法判断"，
  不猜远端状态、不把读取错误当作未发布。
- 提交成功后刷新列表、diff 与审阅进度（`refreshTarget` 走既有审阅叠加与快照接收门，
  HEAD 变化会按既有规则让旧审阅标记失效），并显示新 commit ID、输出与 hooks 改动文件。
- 失败/待核对保留草稿；清理信息、amend、skip 只作用原 worktree/原次编辑。
- 提交按钮旁显示审阅进度（不阻止提交）；`⌘K`/`Ctrl+K` 提交：信息框内由信息框自己处理，
  全局处理时其它输入控件与任何弹窗内不穿透；平台修饰键与展示文案在 `src/lib/platform.ts`，
  中文文案集中在 `src/lib/i18n/zh-CN.ts`。

## 固定提交

基于 m2 `073ecfe`，规格 `1fd4fad`；首批实现 `c398b98`、`6b6700e`、`8f0bc20`、`2016de9`；唯一返修 `da13d23`、`be0607b`、`8a130cd`；规划方接管源码 `b4b3bfb`、`1ca1e52`。均只在本地 m3，未推送、未合并 main。

## 已知限制

1. **本轮未运行任何验证**：Rust 与 TypeScript 都未经编译器检查，TS 绑定为手工同步。
   首次编译时可能出现类型/借用细节需要微调（见下条）。
2. TS 绑定（`src/types/bindings/Commit*.ts`、`Operation*.ts`、`UpstreamStatus.ts`）
   按 ts-rs 的既有输出格式手工书写；若以后运行 `pnpm bindings`，应以生成结果为准。
3. 实时输出的两条流（stdout/stderr）各自独立交付，行间**不保证**严格的时间先后
   （与多数 CLI 客户端一致）；每条流内部的顺序是真实的。
4. hooks 工作区对照读取全部 tracked 与前后 dirty/untracked 路径的内容摘要，文件按分片读取；
   子模块比较真实 HEAD、完整 index 与内部文件摘要，变化归于父仓库的子模块路径，不自动撤销。
5. 跳过 hooks 依赖"临时空目录 + `--no-verify`"；若用户的 `core.hooksPath` 指向 Git 自身
   不读取的位置以外的机制（例如 hook 由外部脚本手动调用），Revia 不会也不应干预。
6. 草稿写盘有 400ms 去抖；应用被强制杀死时，最后一次尚未落盘的输入可能丢失
   （正常关闭走 M2 的关闭编排，不会丢在途写）。
7. 提交结果只保存在本次进程内（按 worktree ID 索引），重启后不恢复；
   持久的是草稿、快照记录与 Git 真实状态本身。

## 未执行的验证（如实列出）

以下全部**未执行**，不得视为通过：

- `cargo test`、`cargo check`、`cargo build`
- `pnpm exec tsc -b`、`pnpm lint`、`pnpm build`
- 任何 GUI 启动、截图检查、桌面测量、冒烟、长测
- 自动化集成测试与双平台编译
- 任何新 CI

## 唯一一次正式返修（Q113，2026-10-26）

规划方静态阅读发现并移交 15 项功能缺口，实施方提交唯一一次返修（仍**未运行任何验证**）。其后的残余问题由规划方直接接管，不再派回 Pi；终态以本报告末尾接管记录为准。
逐项实现与文件映射见下方；每项都在对应 `issues/*.md` 的 Comments 里记录了修复。

### 返修改动摘要

| 项 | 缺口 | 修复位置 |
| --- | --- | --- |
| 1 | 仅暂存删除误判为无内容、intent-to-add 误当内容 | `commit.rs::read_index_entries`：补出暂存删除（`deleted`/`headOid`）、`intentToAdd` 标记；`has_staged_changes` 只看真实可提交条目；指纹覆盖新字段 |
| 2 | F03.4 空差异 merge 收尾被拦；无标记 unmerged 漏门禁 | `commit.rs::CommitExpectation::allows_empty_diff_commit`、`commit.rs::CommitExpectation::has_unmerged`；`commit_flow` 与 `CommitComposer` 前后端一致放行/拦截 |
| 3 | 操作预期不含真实对象内容；rebase 带 CHERRY_PICK_HEAD 误报互斥 | `operation.rs::OperationState::metadata_version`（全部标记内容分片哈希）、`commit.rs::fingerprint_of`；probe 按 rebase>merge>cherry-pick>revert 优先级，从属标记不报错 |
| 4 | F05.3 已确认失败不记 after 基准 | `commit_flow.rs` 收尾：Finished（无论成败）读真实状态写 `Completed{after}`；仅 `SpawnFailed` 记 NotApplied；读取/落盘失败保留 NeedsVerification、不伪造基准 |
| 5 | hook 基准拒绝 symlink/gitlink 导致无法提交 | `commit_snapshot.rs::digest_path`（symlink_metadata、不跟随链接、支持 symlink/gitlink）；部分暂存快照仍用 `recovery::capture_files` fail-closed |
| 6 | hook 报告只比 staged、漏报干净 tracked 变化 | `commit_snapshot.rs`：baseline 取全部 tracked ∪ 当时 dirty/untracked 的摘要；diff 再并当前 tracked/status 路径，含新出现/消失，按类型+内容摘要对照；读取失败显式报错 |
| 7 | flushCommitDraft 不可 await、提交不确认落盘 | `App.tsx`：草稿按 worktree 串行队列（`draftQueuesRef`）可 await；`flushCommitDraft` 与精确 revision 的保存于提交前等待，失败中止提交且可见 |
| 8 | 关闭/退出不 flush 草稿、失败静默 | `App.tsx` + `refreshGate.ts` CloseDriver：`closeWindow`/`exitApp` 经 `closeAfterDraftFlush`，冲刷全部 pending+等待队列+检查错误，失败不销毁窗口/退出 |
| 9 | 读取代际 await 后才取、模板未排队保存 | `commitDraft.ts::ticket()` 请求发起时绑定 worktree/激活/代际；`App.tsx` 模板初始化也 `queueCommitDraft` |
| 10 | amend 确认只绑上游版本 | `AmendConfirmation`（预期指纹+信息原文+选项+上游版本）前后端都绑定；刷新/切换/编辑/激活代际变化即失效；后端写前再校验（`commit_flow.rs::verify_amend_confirmation`） |
| 11 | 非零退出丢结构、不全刷新 | `commit_flow.rs` 返回 `CommitOutcome{success:false, error(Git类), output, hookChanges, snapshotRecordId, expectationAfter}`；前端按 Git 类展示并总是刷新原 worktree |
| 12 | 事件 operationId 未核对 | 前端 `newCommitNonce()` 发起时生成，后端校验字符集并原样回带；事件带 `clientNonce`，前端只接收当前这次 nonce 的事件 |
| 13 | 单行无界/非 UTF8 停读、内存无限 | `commit.rs::read_stream` 固定 8 KiB 分片、lossy UTF-8 持续 drain、跨分片保留不完整 UTF-8，无换行也立即交付；尾部按字节 256 KiB，前端按 UTF-8 字节限 256 KiB |
| 14 | skipHooks 持久化/跨重启恢复、条件清草稿非原子 | `DraftStore` 保存与读取强制 skipHooks=false；切换 worktree 复位；提交完成后复原（代际未变时）；`DraftStore.clear_if_matches` 在共享锁内逐字比较后清除 |
| 15 | 上游/模板/HEAD 读取吞错 | `read_upstream_config` 用 `branch.<name>.remote/merge` 区分未配置/未知/错误；`read_template` 用 `git config --path`（展开 `~/`）并区分退出码；`read_head` 用 `show-ref --verify` 证明引用确实不存在才算 unborn（损坏 fail-closed） |

### 返修新增/修改的文件

- 新增：`src/types/bindings/AmendConfirmation.ts`、`src/types/bindings/WorktreeEntryKind.ts`
- 重写/大改：`src-tauri/src/operation.rs`、`commit.rs`、`commit_flow.rs`、`commit_snapshot.rs`
- 修改：`src-tauri/src/error.rs`（`payload()`）、`commands.rs`、`src/App.tsx`、
  `src/components/app/CommitComposer.tsx`、`src/lib/commitDraft.ts`、`src/lib/ipc.ts`、
  `src/lib/i18n/zh-CN.ts`、以及 M3 全部 TS 绑定
- 文档：`docs/milestones/m3/report.md`、各 `issues/*.md` 的 Comments

### 返修后已知限制（更新）

1. **本轮仍未运行任何验证**：Rust/TS 未经编译检查，TS 绑定为手工同步；首次编译可能需要微调。
2. hook 工作区对照是「全部 tracked ∪ 当时 dirty/untracked ∪ 当前 tracked/status」的
   **摘要**（SHA-256/类型/模式），不整读文件字节进内存；覆盖新出现/消失路径。
3. gitlink（子模块）工作区比较真实 HEAD、完整 index、已跟踪及未忽略的未跟踪文件摘要，
   含嵌套子模块；未初始化子模块不向上误读父仓库，异常 Git 入口或嵌套超过 32 层显式报错。
4. 实时输出两条流各自独立交付，行间不保证严格的时间先后。
5. 草稿写盘有 400ms 去抖；正常关闭/退出前会冲刷并等待，被强制杀死时的最后输入可能丢失。
6. `commit_snapshot` 的 `WorktreeEntryKind` 仅为完整性导出的 TS 类型，前端当前未消费。

### 未执行的验证（如实列出，仍全部未执行）

- `cargo test`、`cargo check`、`cargo build`
- `pnpm exec tsc -b`、`pnpm lint`、`pnpm build`
- 任何 GUI 启动、截图检查、桌面测量、冒烟、长测
- 自动化集成测试与双平台编译
- 任何新 CI


## 规划方直接接管（2026-10-26）

唯一返修后不再返回 Pi，规划方以源码静态阅读完成以下收尾（接管提交 `b4b3bfb`，未编译/测试）：

- **提交可信**：index/tree 路径必须无损 UTF-8，暂存删除路径同样 fail-closed，不用有损替换参与指纹；HEAD 验证提交对象，分支读取失败不冒充 detached。本地上游包含判断使用已捕获的对象 ID。
- **真实进程终态**：无正常退出码、非零退出但 HEAD 已变化均待核对；等待失败继续持有 Child 和同 worktree 写门，直到真正回收进程，不杀 Git、不重跑提交，不能靠一次状态读取绕过仍活着的提交。
- **快照收尾**：保存后的所有启动前失败统一记 NotApplied；已确认 hook 失败仍保存真实 after；快照/状态收尾需核对时不清草稿，前端无论命令成功与否都将该状态登记到关闭保护。
- **草稿并发**：提交绑定精确 revision；后端同一锁内比较原文、amend 与 revision 后才清除，改后再改回同文字也不是同一稿。前端保留按身份绑定的本地缓存，保存失败或切换不丢输入；不会用旧提交信息覆盖新稿。
- **跨目标与关闭**：提交忙碌按 worktree 保存，A 在提交时 B 仍可编辑/提交。先登记共享写入生命周期再等待草稿；正常关闭冲刷全部队列，进入最终关闭阶段后不再接受迟到的编辑/模板回包。
- **确认与输出**：amend 确认绑定激活、刷新和输入代际；信息框与全局统一走 platform 快捷键，其他弹窗内不穿透。输出按原始片段保留换行，UTF-8 边界不拆字符，无换行提示也实时显示。

这里的“完成”只指功能源码与生产接线完成；没有编译、运行、桌面验收或发布通过结论。任务保留 ready-for-human，后续验证框不勾选。


## 最终源码收尾（2026-10-26，`1ca1e52`）

本次只编辑源码并本地提交，未运行测试、构建、类型检查、lint、GUI 或新 CI，未改变系统休眠设置。

- **Standards 静态发现闭环**：子模块 hooks 改动不再只记目录类型。`commit_snapshot.rs` 在独立 Git 定位环境中只读其真实 HEAD、完整 index 和内部文件摘要；父 index 不变的 checkout、已脏文件再次编辑及内部暂存变化也参与摘要对照，结果仍列父仓库子模块路径。文件分片读取、不跟随链接父路径；未初始化检出不回退父仓库。
- **Spec 静态发现闭环**：`amend_head_message` 增加 `expectedHead` 契约，读取固定提交对象并前后核对真实 HEAD；`App.tsx` 在成功与失败回包处同验 worktree、HEAD、上下文 epoch、激活代号和输入 generation。
- **草稿身份收尾**：身份键与草稿来自同一次读取，返回前复核；身份失效时只显示原因、不返回旧正文，前端也不从持久/缓存内容初始化输入，保留用户自己已输入的本地正文。
- **其余接口细节**：模板路径以 NUL 分隔无损读取，不吞掉路径自带换行；完整提交信息保留正文换行；amend 上游未知的二次确认不再冒称已经发布。

两轴原始发现及处理位置见 [静态审查记录](./review.md)。本轮已报告的确定问题均已对应源码修复；最终增量由接管方静态阅读，**没有编译或运行验收结论**。F01–F06 代码实现批次结束，所有任务与规格保持 ready-for-human，后续验证仍未执行；不推送、不合并 main、不发布。


## Q115 验证授权与直接接管（2026-10-26）

用户解除“只改代码、不测试、不启动应用”限制。现在执行必要编译、测试与应用验收；不调整系统休眠、不回派Pi、不推送。上文未验证结论均指Q114功能批次，历史事实不改写。

静态复核：Spec轴无确定新缺陷；Standards轴发现Git真实退出后仍可能等后台hook继承输出的EOF、持续占用写门，另有回收区状态重复显示。现直接修复：平台集中管理可限时读取通道，Git回收后只进行有界排空，不杀Git或后台进程；删除重复状态行。草稿/输出与shell捕获、回收区维护状态的架构收尾一并进入本次验证。

验证结果见下节及独立验证记录；未执行项不勾选。


### Q115 已执行结果与剩余验收

功能源码已固定为本地`03736a0`。前端构建/类型/lint通过；Node 251项、Rust 304项（另有1项专用桌面fixture准备入口ignore）、长测驱动离线回归46项通过。固定macOS arm64 Release构建及只读WebView冒烟通过。初始失败、修复、精确命令、源文件校验与原始输出均保留，见[验证记录](./validation-20261026.md)。

原生桌面控制两次AX读取超时，bundle ID回退因多个本地版本同名而歧义；未执行M3桌面写入验收、D06或完整30分钟长测，也未执行Windows编译。未取消休眠、未要求解锁、未回派Pi、未推送或合并main。M3仍为ready-for-human，不以自动化/只读冒烟冒充最终验收。
