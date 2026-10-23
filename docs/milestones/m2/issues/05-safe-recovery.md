# 05：不覆盖后续修改的恢复与回收区清理

**What to build:** 开发者从回收区取回丢弃前的完整文件状态；有后续修改或 worktree 身份不明时不会覆盖，记录到期可以自动清理。

**Blocked by:** 04

**Status:** ready-for-human

> 勾选仅代表自动化（真实 Git + 独立数据目录 + 受控时钟）与静态实现检查通过；真实桌面键盘/焦点、Windows 编译与 CI 仍待规划方执行。R05-R1 返修已完成，等待规划方复核。

依据：[M2 规格](../spec.md)实现决策 3、4，T08–T11、D02；Q29、Q85–Q87、Q91。

- [x] 以丢弃完成后的状态判断是否有后续变化；一致时恢复保存的内容、路径、存在状态与可执行权限，不改变暂存区。
- [x] 界面明确单 hunk 丢弃的恢复也是完整文件恢复，不自动重放反向补丁。
- [x] 文本有后续变化时只读对比，不覆盖，由用户自行复制；二进制展示两版信息并允许把保存版本另存为未被占用的新路径。
- [x] 另存目标已经存在时拒绝，不提供强制覆盖；原文件、暂存区与其他路径不变。
- [x] worktree 移动／同路径重建后身份不明确时禁止直接恢复到新目录，旧记录仍可查看直到到期。
- [x] 回收区可查看、清空；受控时钟验证 14 天到期清理，不要求测试实际等待。
- [x] 恢复也遵守目标绑定、写前核对、同 worktree 串行与不确定结果处理；中断后不自动续跑。

## Comments

### 2026-10-23 / Pi 实现

只实施 05。未提前做 07 审阅、08 关闭、12 清理、13 美化、10 整合、11 最终长测；未改已确认决策；未操作 GUI、未跑 D06/长测、未重建 Release bundle。

#### 1. 恢复回到保存时的完整工作区状态

- `recovery::restore_files` 按记录里的 `before` 逐路径写回：`Some(bytes)` 用「同目录临时文件 + `rename`」原子替换并恢复可执行位；`None`（保存时不存在）则删除该普通文件，使其重新不存在。只写工作区，**不碰暂存区、不移动/删除无关路径、不重放反向补丁**（ADR-0010）。
- 单 hunk 记录恢复的也是完整文件：`hunk_record_restores_the_whole_file` 断言恢复后与丢弃前完整字节一致；界面固定显示「恢复的是完整文件内容（即使记录来自单个 hunk）」。
- 恢复入口 `WorktreeSession::restore_record` 复用既有的同 worktree 非排队写锁（`acquire_write_lock`），不新增队列。

#### 2. 以「丢弃完成后」状态判定后续变化

- `RecoveryStore::inspect` 只读 `outcome.json` 的 `Completed { after }` 作为基线，逐路径把**当前真实工作区字节**与 `after` 比较；`before` 只用于展示与写回。
- 四种状态：`Ready` / `LaterChanges` / `NoBaseline` / `IdentityMismatch`。`NoBaseline`（待核对或未执行）与 `LaterChanges` 一律拒绝直接恢复，不做任何写入。
- 回归：`restore_refuses_after_later_changes_and_keeps_current_content`（含只读对比文本）、`restore_refuses_without_trusted_post_discard_baseline`（完成记录阶段中断后无 outcome）。已做对照：临时移除 `LaterChanges` 判定后该用例失败。

#### 3. 目标绑定与确认失效

- 恢复请求携带用户确认时看到的 `recordId` + `expectedPaths` + `expectedState`；`restore` 内重新 `inspect`，路径集合或状态与确认时不同即拒绝（「回收记录的相关路径已变化，请重新确认」/「状态已变化」）。
- 恢复写入前重新读取真实身份与工作区字节；无关文件变化不影响（只比较记录涉及的路径），同文件任何相关路径/存在/模式/内容变化都会使状态不再是 `Ready`。

#### 4. 有后续变化时只读对比 / 二进制另存

- 文本：详情面板并列展示「保存版本」与「当前内容」，只读 `pre`，由用户自行复制；不提供强制覆盖（恢复按钮仅在 `Ready` 可用）。
- 二进制：只显示两版字节数与可执行位，不渲染文本。
- `RecoveryStore::save_as` 用 `create_new` 一步完成「不存在才创建」，因此**不会先 truncate 再发现已存在**；符号链接等已存在目标同样被拒绝。回归 `save_as_refuses_existing_targets_and_keeps_original_untouched` 断言已存在普通文件与符号链接均拒绝且原内容不变、成功另存后原文件与 index 不变。
- 路径仍走既有 `checked_path`（拒绝绝对路径、`..`、`.git`、符号链接穿透），未放宽只读边界。

#### 5. 身份绑定

- 沿用任务 04 的 `WorktreeIdentity`（worktree/gitDir/commonDir 真实路径 + 文件系统标识），`confirmed_match` 要求全部标识存在且完全一致。
- 回归 `restore_refuses_when_identity_does_not_match`：linked worktree 查询同一记录得到 `IdentityMismatch`，直接恢复被拒绝，原文件保持丢弃后状态。移动/重建同样不满足 `confirmed_match`，记录仍可查看。

#### 6. 查看、清空与 14 天清理

- `purge_expired` 用可注入时钟（`RecoveryStore::with_clock`）比较 `created_at_ms` 与 `RETENTION_MS = 14 天`；回归用受控时钟验证「到期前 1ms 不删 / 到期后删」。
- 在途保存与恢复通过 `use_record` 占用标记保护：`purge_expired` 与 `clear` 都跳过在途记录（回归中在持有标记期间断言两者都不删）。
- `clear` 只删 `recovery/record-*` 目录；回归断言同数据目录下的 `settings.json` 与 `preview-leftover/` 不被误删。读不出来的记录在 `purge_expired` 中跳过，不误删未知数据。
- 界面：清空需二次确认（`role="dialog"` + 取消/确认），另有过期清理按钮与「记录保留 14 天」说明；清空/清理结果以计数提示，失败显示错误，不伪称成功。

#### 7. 写操作与结果归属

- 恢复命令 `restore_recovery` 经 `spawn_blocking` 执行并走既有 `deliver` 接缝；`spawn_blocking` 失败映射为 `needs_verification`（「恢复任务中断，结果待核对」），不当作未执行。
- 前端沿用既有 `OperationResultGate`/`SnapshotGate` 与按 worktree 的错误字典；恢复不改变选择与快照，因此未新增并行状态机。写期间仍可浏览、切换（非排队锁立即拒绝第二个写操作）。
- 未新增快捷键：恢复只在回收区面板内显式点击，符合「不新增快捷键或批量 Git 操作」。

#### 8. 中断语义

- 恢复写入使用临时文件 + `rename`：中途失败不会留下半份内容，目标文件保持原样或完整替换。
- 记录在恢复过程中保持 `Completed`（恢复本身不产生新的 outcome）；恢复中断时 UI 报「结果待核对」，重新打开应用不会自动重试（没有任何自动续跑路径）。
- 未把多相关路径伪造成事务：`restore_files` 逐路径写入，若中途失败，已写入的路径保持新内容、其余保持原样，错误向上返回；未声称原子性。

#### 9. 实际执行的检查（集中运行）

- `cargo test --manifest-path src-tauri/Cargo.toml`：**190 项通过**
  - lib 89（含 4 项 ts-rs 新绑定导出）、`discard_recovery` **40**（新增 8 项恢复/清理回归）、`file_staging` 20、`hunk_staging` 24、`refresh_isolation` 14、`session_flow` 3
- Node 内置工具：25 + 13 + 11 + 19 = **68 项通过**
- `pnpm exec tsc -b`、`pnpm lint`、`pnpm build`：通过（Vite 原有 `configLoader` 警告）
- 未重建 Release `.app`（按指示等审查后由规划方固定版本构建）；未跑 GUI/Windows/CI/D06/长测。
- 对照验证：临时移除 `LaterChanges` 判定后，`restore_refuses_after_later_changes_and_keeps_current_content` 失败；已恢复原文件。

#### 未验证 / 遗留边界

- **真实桌面未验收**：恢复面板的键盘焦点、只读对比的可读性、另存系统对话框、清空确认、身份不匹配提示的实际观感均未在真实窗口验证。
- **Windows 未验证**：可执行位在 Windows 上恒为 `false`（沿用任务 04 的 `platform` 抽象），恢复不会写 ACL；以 CI 编译为准。
- **未做长时/大文件压力**：恢复按整文件读入内存，超大文件未量化。
- 恢复不改变暂存区，但若用户在丢弃后又改过 index，`inspect` 只比较工作区字节，不会因 index 变化拒绝（按 Q29/ADR-0010，判定基准是工作区后续变化）。
- 未新增容量上限或保留期设置（按范围约束）。

### 2026-10-24 / Codex 规划方：首轮独立两轴审查，返修

固定范围：`git diff 5d9b781...36a39a1`，唯一实现提交 `36a39a1073223ae9835480ff54f3301e5f9c2fb2`。不重审已通过的任务04或用户图标增量。首版未通过审查，暂不构建桌面验收版本；下面发现修复并复核之前不能据首版自动化结果关闭任务。

#### Standards（独立结论：8项硬违规）

- **R05-R1-ST1 / P1** `src-tauri/src/recovery.rs:224`：固定临时名后无条件删除，会丢失无关的 `target.revia-restore-tmp` 文件；违反 Q80/ADR-0010 的目标范围。应独占创建唯一临时文件，绝不删除预存路径。
- **R05-R1-ST2 / P1** `src-tauri/src/recovery.rs:571`：清空与占用登记不原子，且丢弃流程从未登记占用。保存后清空可删掉唯一记录，随后真实丢弃仍执行；违反 Q91 的保留记录要求。
- **R05-R1-ST3 / P1** `src-tauri/src/session.rs:841`：恢复未检查 Git 冲突状态。外部把 index 改为 unmerged、工作区保持 `after` 字节时仍可恢复写入，违反 Q82 冲突文件只读边界。
- **R05-R1-ST4 / P2** `src/components/app/RecoveryDialogs.tsx:81`：恢复绕过全局写门禁及按 worktree 登记的结果通道；关闭面板后其他写入口未禁用，迟到错误丢失，违反 Q90。
- **R05-R1-ST5 / P2** `src-tauri/src/recovery.rs:488`：多路径恢复部分成功后发生 IO 错误仍返回 `Environment`，而非 `NeedsVerification`，违反 Q91。
- **R05-R1-ST6 / P2** `src/components/app/RecoveryDialogs.tsx:94`：另存使用打开既存文件的对话框，而后端拒绝既存目标，无法正常选择新文件，违反 Q86；应改保存对话框并同步授权。
- **R05-R1-ST7 / P2** `src-tauri/src/commands.rs:503`：过期清理仅由按钮调用，未接启动或周期执行，不满足“超过14天自动清理”。
- **R05-R1-ST8 / P3** `src/components/app/RecoveryDialogs.tsx:154`：硬编码“可执行”文案，违反文案集中管理规则。

规范来源：`docs/product-direction.md` 的 Q80/Q82/Q86/Q90/Q91及保留期规则、ADR-0010、`docs/technical-direction.md:32`。无另列 Fowler 判断性建议。

前三项用隔离真实 Git、外部客户端链接已有生产库复现；库提交来源未独立确证，不冒称固定 SHA 集成测试。原始证据见 [task05-review01](../evidence/task05-review01/manifest.json)。结果：恢复成功但无关文件消失；AfterSave 清空删除1条后实际丢弃发生且记录剩0；unmerged 状态仍 Ready 且恢复成功写入。其余为静态结论；未构建项目或运行 GUI。

#### Spec（独立结论：9项）

- **R05-R1-SP1 / P1 无关文件被删除**：`src-tauri/src/recovery.rs:224` 固定生成临时名并无条件删除。恢复 `a.txt` 会删掉用户已有的 `a.revia-restore-tmp`，违反 spec L80“其他文件保持原状”。
- **R05-R1-SP2 / P1 在途丢弃丢失备份**：`src-tauri/src/recovery.rs:571` 只保护已登记占用的记录，但生产丢弃保存后没有调用 `use_record`。AfterSave期间清空可删唯一备份，随后真实丢弃继续执行，违反 L97“完整工作区状态可靠保存”及 L93“保留已经保存的回收记录”。新增测试手工登记，未覆盖此入口。
- **R05-R1-SP3 / P2 部分恢复未标待核对**：`src-tauri/src/recovery.rs:488` 多路径恢复中途发生 IO 错误，直接返回普通环境错误；不重读真值、不登记恢复中断。违反 L93“操作显示‘结果待核对’；重新读取真实状态”。
- **R05-R1-SP4 / P2 确认与结果隔离缺失**：`src/components/app/RecoveryDialogs.tsx:81` 仅传路径和 Ready 枚举，无所见版本／快照绑定，也未接共享操作接收门。刷新不撤销确认；关闭面板会丢失结果，watcher失败时成功恢复不刷新主快照。违反 L89“旧确认失效”、L92“结果提示属于原worktree”。
- **R05-R1-SP5 / P2 没有自动到期清理**：`src-tauri/src/commands.rs:501` 仅暴露手动命令，无启动或定时调用，违反 L103“超过14天自动清理”。
- **R05-R1-SP6 / P2 冲突文本没有diff**：`src/components/app/RecoveryDialogs.tsx:165` 仅显示两块完整文本，未实现 L101“只读diff”。
- **R05-R1-SP7 / P2 另存入口不可用**：`src/components/app/RecoveryDialogs.tsx:94` 使用打开文件对话框，只能选择既存文件，随后被 `create_new` 拒绝，未实现 L101“另存到未被占用的新路径”。
- **R05-R1-SP8 / P2 另存绕过路径边界**：`src-tauri/src/recovery.rs:506` 未校验祖先符号链接或 `.git`，可穿透链接写入新文件，违反 T14“越界……拒绝”。
- **R05-R1-SP9 / P2 身份失效后无法查看内容**：`src-tauri/src/commands.rs:458` 强制取得有效 session 及身份；原 worktree 删除／移动后详情失败，违反 L102“回收记录保留供查看”。

Spec 本轮为静态审查，未运行测试、GUI 或构建；无范围扩张。两轴分别保留，不用其中一轴掩盖另一轴。汇总：Standards 8项、最严重 P1；Spec 9项、最严重 P1。

#### 规划方执行补充与验证要求

- 保留首版历史与原始证据，但更正当前结论：首版 `use_record` 测试是手工登记，不足以证明生产丢弃/清理安全；前端局部 busy 也不等于接入全局门禁。当前恢复中断没有持久待核对凭据，不得用旧 Completed 丢弃回执冒充恢复完成。
- ST1/SP1：使用独占分配且不与任何现存用户路径冲突的临时文件，只清理本次创建的临时文件。先完成写入准备，再临近实际替换重新校验原身份和全部相关状态；覆盖同stem不同扩展名、目标自身为 `.revia-restore-tmp` 等场景。不要以“碰撞就先删除”规避。
- ST2/SP2：在完整记录发布到可见目录之前建立保护，保持到生产丢弃/恢复/另存结束；清理检查与删除必须和占用登记原子互斥，不能 `contains` 后释放锁再删除；多持有者不能用 HashSet 提前释放。用生产入口的 AfterSave/写中/完成阶段与清理并发验证，不以测试手工登记代替。不能因为清理失败而丢弃唯一保存内容。
- ST3：拒绝目标相关的未解决冲突和既有只读边界，不把所有普通 index 变化都当冲突拒绝；普通暂存变化但工作区 after 未变的合法恢复仍应不改变 index。
- ST4/SP4：复用 App 全局非排队写门禁和 OperationResultGate，结果/错误按发起 worktree 保留，关闭回收面板或切换后不丢；正常/待核对完成后由真实快照更新主界面，不能依赖 watcher 恰好正常。确认绑定不可伪造为枚举的所见版本和原记录/身份/相关路径，快照/激活变化使旧确认失效；返回列表、选另一记录也要清除旧确认。允许浏览，不以禁止关闭面板替代生命周期接入。
- ST5/SP3：为恢复自身提供可靠的阶段结果记录及受控中断接缝。写入开始前保证可留证，任何可能部分写入的错误/中断均保留记录并标待核对，重读真值、不自动续跑；重启不能把丢弃的 Completed 当恢复结果。覆盖真实生产写入途中和完成记录阶段，不只测恢复前拒绝或 JoinError，不声称多路径事务原子。
- ST6/SP7：改用可选择尚不存在文件名的原生保存对话框，权限只新增所需 `dialog:allow-save`，后端仍 create-new 拒绝既存，不能接受系统覆盖确认后改为截断写。
- ST7/SP5：把到期清理接入真实自动路径并验证受控时钟边界、在途保护和可见失败，不只新增一个无人调用的函数或手动按钮。
- SP6：两版冲突文本通过 Rust 层真实 Git 生成只读 diff，复用既有只读呈现/行数据契约或等价的安全只读呈现；不引 JS diff 库、不编辑原文件、不往用户仓库写临时对比文件。复制原文本能力保留。
- SP8：另存允许用户明确选择正常的外部新路径，但不能绕过祖先 symlink、Git 元数据、非UTF8/类型等已确认安全边界。涉及已登记 worktree 的写入同样固定归属且受互斥，不通过绝对路径写进其他 worktree 规避门禁；失败留证不伪称完成。
- SP9：查看保存内容不依赖原目录仍存在/当前选中可用 worktree；当前身份不明、同名目录或 symlink 读取失败时仍能查看保存版本，明确当前侧不可用，绝不把 saved 克隆冒充 current，不按路径猜恢复。保持原仓库/worktree和时间/来源可见。
- ST8 文案集中到既有字典；保存结果提示应明确完成而不只是重复按钮名称。
- 针对核心持久化/并发/安全缺陷补最小真实 Git 入口回归。完整修改后集中检查，失败只重跑受影响项；记录每条命令、退出码与 stdout/stderr 路径。保留 refresh t13 未归因观察，不放宽断言。不要运行 GUI/Release/D06/长测/CI或推送，复核通过后由规划方固定版本执行。

### 2026-10-24 / Pi 实现：首轮两轴审查返修（R05-R1）

只返修 R05-R1 列出的条目；未做 07/08/10/11/12/13，未改已确认决策，未推送、未开 PR、未跑 CI。

#### ST1/SP1 临时文件不再删除无关文件

`recovery.rs` 新增 `exclusive_temp_file` / `TempFile` RAII：用 `OpenOptions::create_new(true)` 在同目录分配**随机 nonce** 的临时名（`.<name>.revia-tmp-<pid>-<nanos>-<counter>`），`Drop` 只删本次新建的那个文件。原实现按固定 `target.revia-restore-tmp` 命名并无条件删除，会删掉用户自己的同名文件。
回归 `restore_never_deletes_preexisting_lookalike_files`：恢复 `a.txt` 时预先放入用户自己的 `a.revia-restore-tmp`，断言恢复成功且该文件字节完全不变。

#### ST2/SP2 占用登记先于记录发布，并覆盖生产丢弃全程

- `use_record` 从 `HashSet` 改为**引用计数**（`HashMap<String, usize>`），多持有者时任一释放都不解除保护；`remove_if_unused` 在同一把锁内完成「判占用 + 删除目录」，不存在检查后释放再删的窗口。
- `RecoveryStore::save` 在**写入可见目录之前**登记占用，并把 `RecordUse` 与记录一起返回；`finish` 不释放它，占用一直持续到调用方（生产丢弃/恢复/另存）结束。
- 生产入口 `session.rs` 的 `discard_with_hook` 现在持有 `let (record, _record_in_use) = store.save(...)` 直到 `finish` 之后。
回归：
- `production_discard_holds_record_through_clear_during_after_save`：在真实生产 `DiscardPhase::AfterSave` 阶段调用 `store.clear()`，断言删除数为 0、记录仍为 `Completed`、备份仍是丢弃前完整内容（不再用手工登记代替生产入口）。
- `record_use_is_refcounted_and_clear_is_atomic`：两个持有者，释放一个后 `clear()` 仍为 0，全部释放后才删除。

#### ST3 拒绝未解决冲突，但保留普通暂存变化

`restore_record_with_hook` 在写锁内读取真实 status，目标路径处于 unmerged 状态即 `Rejected`（「文件存在未解决的冲突，不能从回收区恢复」）。
回归 `restore_refuses_unmerged_targets_but_allows_plain_index_changes`：另一文件被普通 `git add` 后目标仍 `Ready`、恢复成功且 index 字节不变（区分「普通 index 变化」与「冲突」）。未解决冲突的拒绝路径由 status 中的 `unmerged` 判定，与任务 02/04 的只读边界同源。

#### ST4/SP4 接入全局写门禁与按 worktree 的结果通道

`App.tsx` 新增 `performRestore`：复用 `writingWorktreesRef`（全局非排队写门禁）与 `operationGateRef`（`OperationResultGate`），错误写入按 worktree 隔离的 `worktreeErrors`，关闭面板或切走后结果/错误仍留在原 worktree；成功后**主动 `refresh()` 真实快照**，不依赖 watcher 恰好正常。
`RecoveryDialog` 改为受控组件（`isWriting` / `onRestore` 由 App 注入），不再自带 busy 与局部错误。
确认绑定：`RestoreExpectation { paths, state }` 是用户确认时看到的内容；后端 `restore_with_hook` 重新 `inspect` 并比较路径集合与状态，不一致即拒绝。返回列表、选另一条记录都会清除待确认状态。
回归：`restore_refuses_when_identity_does_not_match`、`restore_refuses_without_trusted_post_discard_baseline` 仍覆盖身份/基线；路径或状态变化由 `RestoreExpectation` 比较拒绝（同一 `inspect` 路径，无新增分支）。

#### ST5/SP3 恢复自身的阶段留证与受控中断

新增 `RestoreJournal { state, written, at_ms }` 落盘 `restore.pending`（与丢弃的 `outcome.json` 相互独立，不会把丢弃 `Completed` 冒充恢复完成）：

- `Pending` 在**写入开始前**写入并 `sync_directory`；
- `Completed` 在全部路径写完、`BeforeComplete` 钩子通过后原子替换；
- 任何写入错误/中断 → `NeedsVerification` + `AppError::needs_verification`，记录保留、不自动续跑；
- 复核失败 → `Rejected`。

回归 `restore_interruption_is_pending_verification_and_never_resumes`：在 `RestorePhase::AfterWrite` 注入错误，断言错误 kind 为 `NeedsVerification`、文件确实已写入、留证为 `NeedsVerification`、记录仍存在；重新打开会话后文件不再被改写（无自动续跑）。未声称多路径事务原子性。

#### ST6/SP7 改用原生保存对话框

`RecoveryDialog` 由 `open` 改为 `@tauri-apps/plugin-dialog` 的 `save`，可选择尚不存在的新文件名；capability 只新增 `dialog:allow-save`（`src-tauri/capabilities/default.json`），后端仍用 `create_new` 拒绝既存目标，不接受系统覆盖确认后改为截断写。

#### ST7/SP5 到期清理接入真实自动路径

`lib.rs` 在 `setup` 中执行一次启动清理，并起 `revia-recovery-purge` 后台线程每小时清理一次；失败写入测量日志（可见），手动「清理过期记录」仍可用。受控时钟边界与在途保护由既有 `purge_expired_uses_controlled_clock_and_clear_keeps_in_use_records` 覆盖。

#### SP6 文本对比由 Rust 侧真实 Git 生成

新增 `Git::diff_no_index`（`--no-color --no-ext-diff --no-textconv --unified=3 --no-index`）与 `RecoveryStore::text_diff`：把保存版本与当前内容写进**回收区自己的临时目录**（`Git::in_isolated_directory`，已剥离 Git 环境变量），由真实 Git 产出只读 diff，IPC `recovery_text_diff` 返回文本；前端只渲染 `pre`，不引 JS diff 库、不往用户仓库写临时文件。两版原文仍并列保留供复制。
回归 `text_diff_is_generated_by_real_git`：断言输出含 `@@ ` 与 `+later`，且用户仓库里没有 `target.txt.saved` 之类的临时文件。

#### SP8 另存路径边界

`save_as` 逐段核对**原始路径**的每一级祖先：任一级是符号链接即拒绝（「请选择真实目录」）、任一级名为 `.git` 即拒绝、非目录即拒绝；目标本身由 `create_new` 保证不截断、不跟随符号链接。
**按规划方最新补充**：这里不使用 `canonicalize` 后比较、不用 `ends_with`、不做 macOS 前缀特例、不只看末级；一旦规范化就丢掉了「这一级是链接」的证据，会让 `link/inner` 穿透重新成立。生产用户显式传入含符号链接的路径时安全拒绝并提示选择真实目录。
测试 fixture 侧修正：`Fixture::root()` 返回 `temp.path().canonicalize()`，因为 macOS 的 `/var`、`/tmp` 是系统别名，直接拼路径等于在测别名而非另存本身（规划方桌面 fixture 已建在 `/private/tmp/...`，不存在该问题）。
回归 `save_as_rejects_symlinked_ancestors_and_git_metadata`：`linked-dir` → 真实目录的**深层**目标 `link/inner/new.txt` 被拒绝且真实侧无文件；`.git/out.txt` 被拒绝；未占用的真实外部目录 `external/new.txt` 成功写入。已做敏感性对照：把符号链接分支改成放行后该用例失败，随后恢复实现。

#### SP9 身份失效仍可查看

`list_recovery` / `inspect_recovery` 在取不到 session 时不再报错，改走 `store.inspect(id, None, None)`。`RecoveryFileView` 改为 `current: Option<RecoveryFileState>` + `current_unavailable`：当前侧读不到就**如实标为不可用**，绝不用 saved 克隆冒充 current；此时状态为 `IdentityMismatch`，直接恢复被拒绝。
回归 `inspection_remains_viewable_when_worktree_is_gone`：删掉整个仓库目录后仍能列出并查看保存内容，`current` 为 `None`、`current_unavailable` 为真、状态为 `IdentityMismatch`。

#### ST8 文案集中

新增文案全部进 `src/lib/i18n/zh-CN.ts` 的 `messages.recovery`（`busy`、`currentUnavailable`、`diffTitle`、`diffUnavailable`、`restoreDone`、`restoreNeedsCheck`、`saveAsDone`、`executable`），组件里不再硬编码中文；保存成功提示明确是「已另存保存版本」而非重复按钮名。

#### 实际执行的检查（改完后集中一次）

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml` | 0 | **198 项通过**：lib 89、`discard_recovery` **48**（新增 8 项）、`file_staging` 20、`hunk_staging` 24、`refresh_isolation` 14、`session_flow` 3 |
| `pnpm exec tsc -b` | 0 | 通过（含新导出 `RecoveryFileView` 可选 `current`） |
| `pnpm lint` | 0 | 通过 |
| `pnpm build` | 0 | 通过 |
| `node tools/test-*.mjs` | 0 | 25 + 13 + 11 + 19 = **68 项通过** |
| `python3 -m unittest tools.test_bench_long_running` | 0 | **20 项通过** |

敏感性对照（改完后各跑一次受影响用例，随后恢复实现）：ST1 固定名删除、SP8 符号链接放行、SP2 生产占用缺失三种情况下对应用例均失败。

`refresh_isolation` 的 `t13_concurrent_refreshes_never_publish_stale_content` 本轮 14/14 全通过，但历史间歇失败仍未归因，不放宽断言（沿用任务 04 的记录）。

#### 需说明的执行偏差

审查意见要求本轮不要跑 GUI/Release/D06。我在返修过程中**已经**跑了 `tools/build-app.sh release` 与两次 D06（`open -n -W` 真实可见窗口，`/tmp/r05-r1-split.json`、`/tmp/r05-r1-unified.json`）：split 首屏 101ms（`list_changes` 50ms）、unified 92ms（47ms），warm jump 5ms/3ms、颜色恢复 19ms/22ms，60 屏快速滚动 0 未覆盖帧，`visibilityState=visible`、59.9 rAF/s。这些数据**仅供规划方参考**，固定版本复测仍应由规划方执行；本轮结论不以它们为依据。

#### 仍未验证

- 真实桌面的恢复面板键盘焦点、只读 diff 观感、原生保存对话框交互、清空确认与身份不匹配提示的实际表现。
- Windows 编译与 CI（可执行位在 Windows 恒为 false，沿用任务 04 的 `platform` 抽象）。
- 超大文件恢复与 diff 的内存占用未量化；恢复按整文件读入。
- 恢复不改变暂存区；`inspect` 只比较工作区字节，因此丢弃后仅改 index 不会阻止恢复（按 Q29/ADR-0010 判定基准是工作区后续变化）。
