# 04：先保存完整状态再丢弃

**What to build:** 开发者确认后丢弃一个文件或一个 hunk 的未暂存改动；此前完整文件状态已经保存为可查看的回收记录，保存失败则工作区不动。

**Blocked by:** 03

**Status:** ready-for-human

依据：[M2 规格](../spec.md)实现决策 3、4、7，T05、T07、T10、T11、D02；Q9、Q81–Q83、Q85、Q87、Q91、Q93。

- [x] 整文件和 hunk 丢弃都有明确目标与确认；⌥⌘Z 只作用于当前文件；旧确认失效时不继续。
- [x] 在修改工作区之前可靠保存完整内容、相关路径、存在状态、可执行权限，记录归属原仓库与原 worktree。
- [x] 保存失败时拒绝丢弃；暂存区、其他文件和未选中的 hunk 保持不变；未跟踪文件没有删除入口。
- [x] 回收区能查看新记录的时间、来源、目标与保存状态，记录在重启后仍在，数据不进入用户仓库。
- [x] 保存完整的丢弃前状态，并能区分用于恢复安全检查的丢弃后状态；不确定结果不冒充已完成记录。
- [x] 通过可控存储失败与阶段中断验证：已保存记录保留，不自动继续丢弃或重跑；不同 worktree 记录不串用。
- [x] 持久身份不能只按路径相同认定；无法确认原身份的记录可查看，但不能关联为新 worktree 的可直接恢复记录。

> 上述勾选仅代表自动化（真实 Git + 独立数据目录）与静态实现检查通过；真实桌面键盘/焦点、双平台 CI 与两轴审查仍待规划方执行。

## Comments

### 2026-10-23 / Pi 接续实现与修复

本轮为同一任务第 2 次派发：上一轮渠道 `unexpected EOF`（2026-10-23T09:07:55Z）后由规划方写下第一版未提交实现；本轮在其上接续审查、修复并补测，未回滚或丢弃该实现，也未改动 05/07/08/12/13。

#### 实现概览（在第一版基础上确认成立的部分）

- **保存（`src-tauri/src/recovery.rs`）**：确认后、任何工作区写入前，把目标文件的完整丢弃前状态持久化到 Revia 数据目录：`content: Option<Vec<u8>>`（`None` 表示路径不存在）、可执行位、相关路径集合（重命名含新旧两条）、原 worktree 身份（工作区/gitDir/commonDir 的真实路径 + 文件系统标识）、时间、来源（整文件/单 hunk）、目标路径。单 hunk 也保存整个文件。
- **不写入用户仓库**：`ensure_outside` 拒绝把回收区放进工作区/gitDir/commonDir；不调用 `git stash`、不读写 reflog、不往仓库目录写任何文件（`grep stash|reflog` 无使用）。
- **原子发布**：记录先写到 `saving-<...>/saved.json` 并 `fsync` 文件与目录，再 `rename` 成 `record-<...>`；完成回执写 `outcome.pending` 后 `rename` 成 `outcome.json`，中途崩溃不会出现「半份完成回执」。
- **丢弃后基线**：`outcome.json` 只保存 `Completed { after }` 或 `NotApplied`；结果不确定时不写回执，列表状态为 `NeedsVerification`，不伪造 post-state。
- **写前双重核对**：保存前与实际写入前各核对一次真实 Git 状态、文件指纹、快照 revision、原始补丁字节与保存时字节；`read_lock` 防止核对与写入之间被内部刷新插入。
- **写互斥与归属**：沿用既有非排队写锁（`acquire_write_lock`），不新增队列；前端复用 `OperationResultGate`（按真实 `opWorktreeId` 登记）与 `SnapshotGate`，切走后原目标结果写入该 worktree 的错误字典但不污染当前界面，也不自动重跑。
- **确认失效**：确认捕获目标/组/指纹/revision + 选择 epoch + 激活代号；快照刷新、选择变化、切换 worktree 都会使确认按钮失效。

#### 本轮修复

1. **`checked_path` 反斜杠过度限制**：原来在所有平台拒绝 `\`，在 Unix 上把合法文件名（如 `back\slash.txt`）误判为只读。现只在 Windows 拒绝（那里 `\` 是路径分隔符，可用于绕过分段校验）；Unix 保留字面文件名。新增真实 Git 回归 `unix_backslash_filename_is_a_normal_discard_target`。
2. **预览与真实 `git apply` 的换行语义不一致**（真实缺陷）：第一版在普通临时目录里应用反向补丁，而 `git apply` 只有在**仓库内**才会执行工作区转换（`core.autocrlf`、`core.eol`、`.gitattributes` 的 `eol`/filter）。实测 `*.txt text eol=crlf` 时仓库外直接失败（`patch does not apply`）。现预览改为：临时目录 `git init`、复制目标路径经过的每一级 `.gitattributes`、并从真实仓库复制 `core.autocrlf`/`core.eol`/`core.safecrlf`/`core.ignorecase`/`core.filemode` 后再应用。新增真实回归 `crlf_attribute_text_discards_with_matching_git_semantics`（断言丢弃后回到提交时的 CRLF 字节、index 不变、记录里是丢弃前真实字节）。
3. **intent-to-add 没有安全基线**：`git add -N` 的文件在暂存区还没有 blob，反向应用会直接删除文件，而任务 02 已确认「未跟踪文件没有删除入口」。现明确拒绝并保持工作区与 index 不变；新增 `intent_to_add_is_rejected_without_touching_the_file`。这是既有只读边界的延伸，未引入新的产品取舍。
4. **补足遗漏覆盖**（真实 Git、独立数据目录）：无末尾换行 + 非 UTF-8（GBK）正文按原始字节保存与恢复；删除状态（`content: None`）记录与恢复、可执行位保留；保存后索引变化或权限变化使旧确认失效（`NotApplied`、工作区不动）；完成回执**真实落盘失败**（把记录目录改成只读）时保留已保存记录、不冒充完成、不自动续跑；原 worktree 被删除后记录仍可查看且身份不确认。
5. **指纹口径收窄复核**：第一版把所有未暂存文件的实际字节并入文本 diff 指纹，导致任务 06 的并发刷新断言失败。现只对未跟踪文件、以及未暂存但**没有文本 hunk** 的 diff（二进制 / 仅权限变化）追加实际字节；普通文本仍只用 Git 展示 patch，保持任务 06 的快照一致性与「列表刷新不逐文件起 Git 进程」约束。`refresh_isolation` 复跑通过。

#### 实际执行的检查（本轮，全部在本 worktree）

- `cargo test --manifest-path src-tauri/Cargo.toml`：**167 项通过**
  - lib 单元 85、`discard_recovery` 22、`file_staging` 20、`hunk_staging` 23、`refresh_isolation` 14、`session_flow` 3
- Node 内置工具：`test-row-window-store` 25、`test-visible-highlight-lines` 13、`test-ipc-diagnostics` 11、`test-refresh-gate` 19，共 **68 项通过**
- `pnpm exec tsc -b`、`pnpm lint`、`pnpm build`：通过（Vite 原有 `configLoader: native` 警告，非新增失败）
- `pnpm tauri build --bundles app`：成功，`Revia.app` 7.82 MiB，二进制 SHA-256 `5c398dfa812c45e86b99d8380f513377266497b055a6062e895f1816951b4879`（已 `touch src-tauri/src/lib.rs` 重新嵌入最新 `dist`）

#### 未验证 / 遗留风险

- **真实桌面未验收**：⌥⌘Z 在真实窗口的可达性、焦点/键盘不穿透、原生 `<dialog>` 的焦点约束、确认失效的视觉反馈、回收区列表在真实数据目录下的表现，均未执行。`ready-for-human`。
- **Windows 未在本机编译**：`platform.rs` 的 `GetFileInformationByHandle` FFI（`BY_HANDLE_FILE_INFORMATION` 布局、`FILE_FLAG_BACKUP_SEMANTICS`）与 `checked_path` 的 Windows 分支只做了静态核对，本机无 Windows target；以 CI 实际编译为准。
- **属性复制是「等价复制」而非同进程读取**：预览复制 `.gitattributes` 链与相关 config；若仓库在保存与写入之间修改了 `.gitattributes`，写后比对会失败并转 `NeedsVerification`（安全失败，不会写错内容）。
- **受控时钟接缝**（`RecoveryStore::with_clock`）已提供但本轮未用于 14 天到期断言（属于 05 范围）。
- 未做长时内存/大文件压力观测；单条记录按完整文件大小存储，未加容量上限（05 清理由后续任务负责）。
