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

### Pi 正式返修处理 — 2026-10-31

- **F03/H1**：EditableWorkspace 卸载时立即落草稿（不只取消 timer）；打开时草稿不再要求基线一致——基线已变时显示磁盘内容、保留草稿存储并进入冲突流程（保留两版）。
- **F04/H3**：暂存 / 取消暂存 / 丢弃 / 提交入口在编辑器当前文件脏时拒绝（errors.workspace.editUnsavedBlock，双语）；局部关闭对话框“放弃”改为清草稿恢复磁盘，“保存”成功后真正关闭。
- **H2**：保存继续走生产写入口（m7_save_editable 内部持会话写锁）；更新安装门禁（beforeUpdateInstall）阻止脏编辑，参见任务 11。
- **H7**：edit.rs 写失败/回读失败保留结构化 cause（AppError::cause），不再把中文诊断句塞进参数。
- **F20**：编辑入口只在 split 视图 + unstaged 侧出现；统一视图点击先切到同文件 split 再开编辑；历史/暂存侧不出现入口。

### Codex / review02 Spec / F03 — 2026-10-31

| F03 P1 | S84“两版”：磁盘变化后恢复只装入磁盘，旧草稿不可比较/取回 | EditableWorkspace.tsx:142 |

<details>
<summary>本项证据原文</summary>

## F03 [P1] 外部基线变化后旧草稿仍不可取回或比较

S83“切换…保留内容”；S84“外部变化时保留草稿与磁盘两版…用户核对后”。`EditableWorkspace.tsx:130–145`把baseline、savedText、text全部设为opened.text；基线不相同时只setConflict，既不保存draft.text到state/ref，也不交给对比UI。`reload(true):249–271`只保留当前text，该text就是磁盘版，因此“保留我的版本”保留的也是磁盘。旧草稿虽暂时在存储中，界面不能取回；任意再编辑触发saveDraft后用新磁盘基线/新内容覆盖同key旧稿。触发为已存草稿后外部改文件，再打开/重启；用户看到“草稿已恢复”但实际只有磁盘版。新增cleanup:189–200能发保存调用，但`.catch(()=>undefined)`仍使持久化失败静默，不能称切换草稿安全闭环。没有动态写用户文件。

</details>

### Codex / review02 Spec / F04 — 2026-10-31

| F04 P1 | S85“保存、放弃、取消”：退出仍直接冲刷关闭；保存未登记生命周期 | App.tsx:1667；EditableWorkspace.tsx:226 |

<details>
<summary>本项证据原文</summary>

## F04 [P1] 保存、关闭与草稿处理仍未接完整写生命周期

S85“关闭时提供保存、放弃、取消…未完成保存时不继续写操作”；S86“结果待核对”。增量App:1907–1925/2404确有当前dirty拒绝，但保存仍在 `EditableWorkspace.tsx:226`直接m7.saveEditable，无既有WriteGuardOrchestrator begin/end、NeedsVerification登记；Rust会话锁:edit.rs384只防并发，不登记前端退出保护。正常关闭仍是 `App.tsx:1667–1672`冲刷草稿即destroy/exit，`reconcileClose:1717–1722`只读既有coordinator，不询问编辑草稿。触发：有未保存编辑时关闭，或保存IPC在途/返回NeedsVerification后关闭；仍没有三选项，写入/待核对也不被关闭器看见。

局部“放弃”新增 `EditableWorkspace.tsx:477–480 clearEditDraft(...).finally(onClose)`，但未把dirty/text复位；随即卸载执行新增cleanup:190–199，以仍dirty的latest又saveEditDraft，因此放弃清除的草稿被重新建回。局部保存关闭使用dirtyRef等待React状态提交，代码仍无save成功结果返回；未将其冒称已真机验证。

</details>

### Codex / review02 Spec / F20 — 2026-10-31

| F20 P2 | S81“并排…只读”：仍独立全文编辑，打开后切统一/暂存仍可写 | App.tsx:5948 |

<details>
<summary>本项证据原文</summary>

## F20 [P2] 改入口没有把编辑接入diff，也没约束活动编辑视图

S80历史/暂存只读；S81“并排视图…工作区一侧…统一视图保留只读…不增加…独立文件编辑器”。增量App:5875按钮加unstaged且统一点击先switchView(split)，但真正渲染:5948仍仅检查editingPath==selectedEntry.path，无view或changes判断；switchView:1432–1458不退出编辑，selectedEntry同path的staged行也会复用该编辑器。触发先编辑后切统一视图/暂存同路径：仍可编辑磁盘。`EditableWorkspace:130,393–438`仍以磁盘全文为左baseline、全文textarea为右侧，替掉现有Git diff而没有编辑工作区一侧的Git diff、块/增删上下文。此为F20明确未完成的规格，不是要求扩建通用编辑器。

</details>

### Codex / review02 Standards / R02 — 2026-10-31

<details>
<summary>本项证据原文</summary>

## H1 / R02：磁盘基线变化后恢复草稿无法取回（P1）

规则原文：`docs/ui-direction.md:92`：“重启或崩溃后提供恢复；磁盘文件已变化时保留两版供对比，不自动覆盖”；`docs/technical-direction.md:164` 同义。

触发：保存了一份未保存编辑草稿，外部编辑器改磁盘，随后重新进入编辑或重启恢复。新分支 `EditableWorkspace.tsx:142–147` 只将 **opened.text** 放进 text/savedText，设置 conflict；draft.text 没有进入任何状态或ref。整个组件除134行初始化load外没有再次加载旧草稿。`reload(true):253–271` 只重读磁盘并保留当前text；此时text本来就是磁盘版，因此“保留我的版本”无法恢复旧文本，左/右两侧也都是磁盘版。继续输入后 `saveDraft:166–173` 用新文本覆盖同一worktree/path草稿，旧未保存编辑被实际替换。这不是完整的两版恢复。

同项修复的另一静态分支：`473–480`“放弃”先clear草稿再finally(onClose)，但dirty/text未重置；卸载cleanup `189–200` 又save相同草稿，“放弃”后可再恢复。另 `App.tsx:5894` 外层关闭完全不经 requestClose 的保存/放弃/取消。卸载新增flush是未等待的Promise，失败被吞，不宜称“同步落草稿”。这些均属于原H1涉及的编辑保留/关闭链，不另加重复finding。

</details>

### Codex / review02 Standards / R03 — 2026-10-31

<details>
<summary>本项证据原文</summary>

## H3 / R03：编辑面板隐藏后暂存/提交漏处理草稿（P1）

规则原文：`docs/ui-direction.md:92`：“暂存或提交前处理相关未保存编辑”；ADR-0004同样重申该规则。

触发：在已有未暂存改动的A文件继续输入，点击外层编辑按钮的“关闭”（`App.tsx:5894`），随后暂存A、提交。关闭设 editingPath=null，同时卸载cleanup把文本作为草稿保留，既没有保存磁盘，也没有放弃确认。新增 `blockOnDirtyEdit:1904–1908` 在path为空时直接返回false，新增提交检查 `2405–2410` 同样要求 dirty && path；因此即使editDirty仍是true，守卫也放行，磁盘旧内容继续进入暂存/提交。切换到B并打开干净编辑器也会将全局dirty变false，无法代表仍在A的草稿。

不是要求所有仓库存在草稿时一律禁用，而是必须按写入目标处理相关未保存编辑，不能仅看当前挂载组件。当前保护只解决了编辑面板保持打开的分支。

</details>

### Codex / review02 Standards / R05 — 2026-10-31

<details>
<summary>本项证据原文</summary>

## H2 / R05：保存仍绕过正常关闭与结果待核对（P2）

规则原文：`docs/technical-direction.md:64`：“正常关闭或退出请求不得直接打断写操作；等待操作完成并记录结果后再执行用户的原请求……结果待核对时先展示，不静默退出”。

本轮H2处理自述仅援引既有会话写锁。固定源码仍由 `EditableWorkspace.tsx:220–247` 直接调用m7.saveEditable，局部saving不登记到App写编排、CloseCoordinator或ExitGuard。组件Props `29–40` 也没有写生命周期接口。关闭flusher `205–218` 仅保存编辑草稿；App `1667–1684` flushM7State后执行destroy/requestExit，并不等待saveEditable。

Rust `edit.rs:384` 的会话锁只能拒绝其他仓库写入，不阻止进程退出；`418–420` 仍是truncate + write_all + sync_all，正常退出可在中途结束进程。NeedsVerification在组件 `238–245` 只保存局部error，没有进入App pending。因此本轮改变error.cause/更新前dirty检查均未解决原H2。没有构造中断时序、没有声称已实际观察到截断损坏。

</details>

### Codex / review02 Standards / R06 — 2026-10-31

<details>
<summary>本项证据原文</summary>

## H7 / R06：结构化cause与locale占位符不一致（P2）

规则原文：`ADR-0013:3,7`：“Rust 返回稳定错误类型和结构化参数，前端按当前语言生成文案”“详细原因也必须是可识别类型与参数”。`docs/technical-direction.md:75–76` 同时要求错误语义与结果说明不能遗漏。

新 `edit.rs:426–432,448–454` 改用空params + cause，正确消除了把已翻译AppError塞进error参数的旧问题。但对应既有词条 `locales/zh-CN.json:1222,1224` 与 `locales/en.json:845,847` 仍为 `{error}`。生产 `src/lib/error.ts:12–16` 递归结果只放params.cause；`src/lib/i18n/format.ts:7–10` 对缺失参数原样保留占位符，formatter也不在末尾追加cause。

故发生写失败/写后回读失败时，中英文最终文本包含字面 `{error}`，实际IO原因不可见。此为Rust载荷到生产formatter的参数契约静态证明，不是x18n现有check已检查的“两个locale占位符互相匹配”问题；没有用模拟payload冒充真实Git双语验收。PrMergeOutcome.error改动在静态层面正确保留了结构，未重复报其原中文混入问题。

建议：经x18n将这两个词条的参数契约迁移到cause，再验证真实失败生产入口及英文formatter。

</details>

## Codex 直接修复编辑生命周期（F03/F04/F20、H1/H2/H3/H7）

实际编辑会话按worktree与路径持有正文、磁盘版、原基线及身份摘要，不随组件隐藏或切换目标销毁。恢复装入真草稿；磁盘再次变化只更新对比，确认后仍须显式保存。保存复用现有WriteGuardOrchestrator/CloseCoordinator，失败和待核对保留原目标、正文和退出保护，不自动重发。放弃排在已发草稿写后清除，并阻止卸载将它复活；持久化失败可见且可明确放弃。

关闭/退出及相关暂存、段操作和提交先执行保存/放弃/取消；后台和启动发现的草稿也参与。保存改变磁盘时先刷新并停止旧的Git动作，要求重新核对。后台存储以真实worktree身份隔离，同路径替换不继承旧身份；原旧草稿文件保留。生产Rust写门与草稿存储共锁，相关文件有草稿时拒绝暂存/提交。编辑保留在Git并排工作区右侧，统一/历史/暂存/冲突只读；草稿尚未保存时不沿用磁盘行号和Git段操作目标。

本组独占验证原件均在`/private/tmp/revia-m7-batch-g5scyw0r/codex-checks/`：`edit-io-cargo-01` 10项真实Git/权限/BOM/CRLF/身份/冲突/元数据/草稿门禁通过；`edit-final-cargo-01`含原9项编辑及152项共享暂存/丢弃/提交通过、1项ignored；`edit-bindings-cargo-01` 7项含ts-rs导出通过；`edit-session-node-03` 13项实际生产状态/写门/关闭协调器回归通过，含将真实Rust权限错误载荷交给实际双语formatter。前端`edit-frontend-build-02`与`edit-lint-02`exit0；`edit-x18n-01`为0错误/76个unused-key警告，保留键不自动删除。

`edit-cargo-01`编译exit101发现批量丢弃Result包裹不匹配，已修复，失败原stdout/stderr保留；后续检查均单独记录源码hash、精确argv、真实exit和分流原件。清理生成绑定只涉及EditDraft/EditableFile，不清理其它绑定。无关rustfmt差异已去除，并重跑受影响Rust检查。上述不是最终候选/全量最终检查/真实桌面验收；并排编辑、双语窄窗与原生退出仍须任务14真机检查。
