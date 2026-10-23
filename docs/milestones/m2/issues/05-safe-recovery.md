# 05：不覆盖后续修改的恢复与回收区清理

**What to build:** 开发者从回收区取回丢弃前的完整文件状态；有后续修改或 worktree 身份不明时不会覆盖，记录到期可以自动清理。

**Blocked by:** 04

**Status:** ready-for-human

> 勾选仅代表自动化（真实 Git + 独立数据目录 + 受控时钟）与静态实现检查通过；真实桌面键盘/焦点、Windows 编译与 CI、两轴审查仍待规划方执行。

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
