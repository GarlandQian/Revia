# 03：编辑工作区 diff 并恢复草稿

Status: ready-for-agent
Blocked by: None

## What to build

并排 diff 的工作区一侧可编辑、显式保存，草稿跨切换和重启保留，并纳入关闭与写前协调。

依据：规格 C；Q122、Q177、Q186、Q195。

## Acceptance criteria

- [x] 普通安全文本可编辑，其他类型 / 过大文件和历史侧只读；统一视图提供同文件并排入口
- [x] 生产 Rust 写入核验真实 worktree、路径类型、原内容和指纹，保留权限与既有换行 / 编码策略
- [x] 外部变化保留两版并拒绝旧保存；磁盘失败、重读失败保留草稿、原目标与已知回执
- [x] 切文件 / worktree / 语言 / 主题保留内容，关闭保存 / 放弃 / 取消，暂存 / 提交前处理相关草稿
- [x] 真实自建 Git 验证保存、外部修改、重启恢复、权限与跨目标失败；不修改用户文件

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / review01 Standards — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/standards.md；完整证据附录保存在本轮证据目录。

1. **[P1] 编辑丢失**：`src/features/m7/EditableWorkspace.tsx:171` 卸载取消 700ms 草稿定时器；`src/App.tsx:5801,5854` 直接关闭/切文件。快速切换丢失最新输入，违反 `docs/ui-direction.md:92`“切文件或 worktree 保留未保存编辑”。

## H1 编辑切换/关闭丢最新输入（P1）

规则：`docs/ui-direction.md:92`：“切文件或 worktree 保留未保存编辑；关闭时提供保存、放弃、取消”。

触发：在普通文本编辑区输入后不足 700ms，点另一个文件、切 worktree 或点外层“关闭编辑”。`EditableWorkspace.tsx:27` 设置 700ms 去抖，164–177 行 effect cleanup 只清理定时器；179–192 行全局 flusher 的 cleanup 则注销登记，无卸载前保存。App.tsx:5801 直接 setEditingPath(null)，5854–5856 行条件及 key 使组件卸载，避开组件自己的 requestClose。

源码事实：内容仅在组件 text state/latest ref，卸载没有保存或转交；后果推断：尚未发送保存请求的最新输入无可恢复副本。长于 700ms 但草稿保存失败时，150–161 行还会吞掉失败，切换同样没有保护。

2. **[P2] 保存绕过关闭协调**：`src/features/m7/EditableWorkspace.tsx:194` 直接调用保存，未登记写生命周期；关闭只 flush 草稿。退出可能中断 `src-tauri/src/m7/edit.rs:418` 的截断写入，违反 `docs/technical-direction.md:64`“正常关闭或退出请求不得直接打断写操作”。

## H2 编辑保存未接入关闭/退出写生命周期（P2）

规则：`docs/technical-direction.md:64`：“正常关闭或退出请求不得直接打断写操作；等待操作完成并记录结果后再执行用户的原请求……结果待核对时先展示，不静默退出”。

`EditableWorkspace.tsx:194–220` 用局部 saving state 直接等待 m7.saveEditable，未调用 WriteGuardOrchestrator 或向 App 登记进行中写入/结果待核对；App 新组件 props 只有 onDirtyChange/onSaved/onClose。`m7/commands.rs:130–136` 仅 spawn_blocking 调 edit::save。该函数的 session 写锁只约束其他会话写入，不参与 App 的 CloseCoordinator/ExitGuard。

对照 App.tsx:1638–1647，关闭仅 flushCommitDraft、flushM7State 后执行 destroy/requestExit。editor flusher 仅 saveEditDraft，不 await 在途 saveEditable。对照旧提交入口 2426 起必须在异步准备前 writeOrchestrator.begin。

后果推断：保存尚在进行时正常退出可以终止后端，edit.rs:418 已 truncate 的工作区文件可能只写部分；保存结果为 NeedsVerification 时也没有登记到现有关闭待核对状态。未用构造时序测试宣称已实际截断。

3. **[P1] 未保存编辑未阻止暂存/提交**：`src/App.tsx:5861` 的 dirty 状态仅供标题使用，现有暂存/提交入口继续操作旧磁盘内容；违反 `docs/ui-direction.md:92`“暂存或提交前处理相关未保存编辑”。

## H3 暂存/提交没有处理未保存编辑（P1）

规则：`docs/ui-direction.md:92`：“暂存或提交前处理相关未保存编辑”。

App.tsx:384 的 editDirty 只有声明和 5800 的 title 读取，5861 只接 onDirtyChange=setEditDirty；全文没有用它门控 stage、commit、相关键盘入口。`useBatchWrite` 在 App.tsx:1867 仍直接使用既有磁盘 snapshot；performCommit 在 2343 起没有编辑器门禁，提交的 flush 只有提交信息草稿。`flushM7State` 本身也仅持久化编辑草稿，不把编辑写回工作区。

触发：对一个已有未暂存改动的文本文件继续编辑，点提交工具窗口的整文件暂存和提交。后果推断：提交的是磁盘旧内容，编辑器中可见的未保存内容被遗漏，且没有保存/放弃/取消选择。此项与 H1 的卸载丢失不同，需要在写入口处理编辑状态。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F03 P1 | “切换…保留内容”：卸载丢草稿，基线变不恢复 | src/features/m7/EditableWorkspace.tsx:130,164 |

## F03 [P1] 切换与外部变化丢草稿

规格 S83：“切换文件、视图、仓库、语言或主题保留内容”；S84：“外部变化时保留草稿与磁盘两版”。

`src/features/m7/EditableWorkspace.tsx:164–176` 700ms debounce在unmount只清timer；`180–192` flusher注册在unmount被移除，无切换flush；`src/App.tsx:5854–5864` 切文件/worktree时条件/键导致卸载，顶部编辑按钮5801直接隐藏。触发：输入后700ms内切文件再回来。结果：最新文本既未保存草稿也无内存所有者，丢失。即使已存草稿，`EditableWorkspace.tsx:130–136` 只恢复相同baseSha，磁盘改变/重启时旧草稿直接不显示；用户再编辑后自动保存还可覆盖旧草稿。静态确定的控制流，尚未动态重现。

| F04 P1 | “先处理…草稿”：暂存/提交/关闭无协调 | src/App.tsx:1638,1765,2343 |

## F04 [P1] 未保存编辑未纳入暂存、提交与关闭门禁

规格 S85：“关闭时提供保存、放弃、取消。暂存、提交等依赖相关文件内容的操作先处理这些文件的草稿；未完成保存时不继续写操作”；S86要求写后不确定标记待核对。

`src/App.tsx:384,5800,5861` 的editDirty只用于按钮title；写入口 `1765`/`1867`/`2343` 没有草稿协调，关闭 `1638–1643` 仅持久化草稿后直接destroy/exit。`EditableWorkspace.tsx:194–221` 保存也未登记既有WriteGuardOrchestrator或NeedsVerification状态。触发：编辑未保存时从侧栏暂存并提交，或直接关闭。结果：提交磁盘旧内容；关闭没有保存/放弃/取消；保存进行中或写后回读失败仍可关闭。局部确认还错误：449“放弃”实际saveDraft并恢复；459–461保存成功后baseSha存在，永不onClose。全链路静态核验。

| F20 P2 | “统一视图保留只读”：统一/暂存入口打开独立全文编辑 | src/App.tsx:5794,5854 |

## F20 [P2] 编辑变成独立整文件视图，统一/暂存只读边界丢失

规格 S80：“历史、暂存区…继续只读”；S81：“统一视图保留只读并能定位到同文件的并排编辑。不增加…独立文件编辑器”。

`App.tsx:5794–5801,5854–5864` 仅以非binary/unmerged显示编辑，不判断changes或view；`EditableWorkspace.tsx:126–129,367–430` 两侧分别为磁盘打开时全文/textarea，而非Git原diff。触发：统一视图或暂存组点编辑。结果：直接进入独立二栏整文件编辑，view仍是unified、没有切到同文件split；暂存侧入口也可写磁盘。缺少已确认的diff工作区侧集成，不是未请求的通用编辑器需求。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。
