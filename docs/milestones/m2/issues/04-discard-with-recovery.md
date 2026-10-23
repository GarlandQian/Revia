# 04：先保存完整状态再丢弃

**What to build:** 开发者确认后丢弃一个文件或一个 hunk 的未暂存改动；此前完整文件状态已经保存为可查看的回收记录，保存失败则工作区不动。

**Blocked by:** 03

**Status:** ready-for-human

依据：[M2 规格](../spec.md)实现决策 3、4、7，T05、T07、T10、T11、D02；Q9、Q81–Q83、Q85、Q87、Q91、Q93。

- [x] 整文件和 hunk 丢弃都有明确目标与确认；⌥⌘Z 只作用于当前文件；旧确认失效时不继续。
- [x] 在修改工作区之前可靠保存完整内容、相关路径、存在状态、可执行权限，记录归属原仓库与原 worktree。
- [ ] 保存失败时拒绝丢弃；暂存区、其他文件和未选中的 hunk 保持不变；未跟踪文件没有删除入口。
- [x] 回收区能查看新记录的时间、来源、目标与保存状态，记录在重启后仍在，数据不进入用户仓库。
- [x] 保存完整的丢弃前状态，并能区分用于恢复安全检查的丢弃后状态；不确定结果不冒充已完成记录。
- [ ] 通过可控存储失败与阶段中断验证：已保存记录保留，不自动继续丢弃或重跑；不同 worktree 记录不串用。
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

### 2026-10-23 / Codex 两轴独立审查：尚未通过

审查固定点 `f7286c31ee230ff80ce7fc4dbbf045574f6c9f61`，受审实现 `a38fd8527a8da34821597db2a4e4f9f7c157c000`；命令为 `git diff f7286c3...a38fd85`。审查期间未修改实现代码。两个审查代理独立按仓库规范与任务规格检查，以下分轴保留，不因同一问题在两轴出现而隐藏任一轴结果。随后合入 main 的 Q105 规则只改文档，不改变受审代码。

#### Standards

- **R04-ST1 / P2 / Windows：预览可能修改用户 Git 配置。** `src-tauri/src/recovery.rs:282`、`:286` 仅切换目录后执行 `init/config`，但 `Git::in_directory` 保留原环境；Windows 的 `src-tauri/src/shell.rs:326` 继承 `GIT_DIR/GIT_WORK_TREE`。从含这些变量的终端启动时，预览命令仍可能指向原仓库，且发生在回收记录保存前。违反 `docs/product-direction.md:206`「不修改用户的 Git 配置」。独立真实 Git 复现中，原配置的 `core.filemode=false` 被预览 `init` 改为 `true`。macOS 的现有环境白名单挡住此路径，不能声称 macOS 生产路径已复现。为 scratch 显式隔离 Git 定位／配置重定向环境，不能只改 cwd；回归同时核对用户仓库 config 与 index 未变。
- **R04-ST2 / P2：预览没有完整继承目标文件的属性语义。** `src-tauri/src/recovery.rs:283` 只复制五个 core 配置，`:293` 只复制工作区 `.gitattributes`，遗漏原 gitDir 的 `info/attributes`、仓库级 `core.attributesFile` 与本地 `filter.*`。例如 `info/attributes` 指定 `text eol=crlf` 的普通文本，预览拒绝真实仓库可正确应用的补丁。违反 `docs/product-direction.md:52` 的普通文本丢弃支持范围。应使用等价的有效属性／转换上下文，并考虑路径层级、worktree、配置来源等实际 Git 语义，不以新增未确认的只读限制代替修复。
- **R04-ST3 / P3：遮罩颜色未使用 CSS 变量。** `src/index.css:399` 写死 `rgb(0 0 0 / 35%)`，违反 `docs/technical-direction.md:34`「颜色一律定义为 CSS 变量」。提取变量，沿用既有浅／深色主题方式；不扩大成任务 13 的界面美化。

判断性建议：没有新增 Fowler 异味发现。本轴共 3 项，最高 P2。

#### Spec

- **R04-SP1 / P1：保存后改变属性仍会执行不等价写入。** 任务 04 要求「未选中的 hunk 保持不变」，规格实现决策 3 要求写前核对真实相关版本。`src-tauri/src/session.rs:789` 只复核身份、目标字节和 patch，没有复核预览使用的属性／转换配置。复现：LF 文本含 3 个 hunk，预览并保存之后把 `.gitattributes` 中目标文件的 `text eol=lf` 改成 `text eol=crlf`；目标字节和规范化 patch 均未变化，最终校验仍通过。真实 `git apply --reverse` 把整份文件变成 CRLF，包括未选中的 hunk，随后写后比较才进入待核对。事后检测不能补救「未选中部分不变」，也不能称为写前安全拒绝。实际写入前必须确认使用的转换上下文与预览／保存的上下文一致，发生相关变化则拒绝写入、保留记录，重新读取真值，不自动重试。
- **R04-SP2 / P2：预览缺失真实仓库属性语义，误拒正常文本。** 规格实现决策 2 要求普通已跟踪文本支持整文件／hunk 丢弃。只用 `.git/info/attributes` 设置 `*.txt text eol=crlf`、`core.autocrlf=false`，检出 CRLF 文本再修改一行；真实 diff 使用规范化 LF。当前 scratch 未带该属性，预览 `git apply` 退出 1（`patch does not apply`），真实仓库同一补丁退出 0 并正确恢复 CRLF。现有根 `.gitattributes` 回归不覆盖此路径。与 R04-ST2 同时修复，但保留两个轴各自的结论。

未确认范围扩张。本轴共 2 项，最高 P1。

#### 主审复现与验收缺口

- **证据范围**：仅在独立 `/private/tmp` fixture 使用真实 Git 2.56.0，没有对用户仓库执行复现写入。属性复现 `changedContext` 显示保存前后规范化 patch 相同、写前目标字节相同，但预期 0 个 CRLF、实际 180 个 CRLF；第 10、140 行未选 hunk 仍保留改动却被改了换行。`infoAttributes` 中 scratch 退出 1、真实仓库退出 0；两例暂存区均保持不变。Windows 定位变量复现只验证真实 Git 行为，Windows 应用可达性来自 `shell.rs` 静态审查，不冒充 Windows 实机验收。
- **R04-V1 / T11 验证缺口**：`discard_recovery.rs` 的 `AfterSave/BeforeWrite/AfterWrite/BeforeComplete` hook 覆盖调用前后中断，但尚未覆盖实际 `apply_worktree_patch` 调用返回非成功／中断时的行为。补真实 Git 子进程的受控失败／中断回归（可用限定 fixture 的 Git 包装器先执行真实写入再模拟失去成功结果），验证已保存完整记录仍在、结果待核对而非未执行、index 不变、重新打开只读取真值、实际写命令不自动续跑／重试。不能用 `AfterWrite` hook 直接声称已完成「写入中」验收。沿用现有 Rust 体系，不引入新框架。
- 原 Pi 备注「属性变动后写后检查是安全失败，不会写错内容」已被上面的字节复现否定；须在后续 Comments 和 report 明确更正，不删除原始过程记录。
- 复现脚本、结果及命令记录在 `../evidence/task04-review01/`。本轮未完成 GUI、D06、Windows CI 或最终长测，任务不得设为 done。

实施方完成上述相关修改后集中运行受影响的检查，按核心安全契约补最小回归；逐项回复 R04-ST1/ST2/ST3、R04-SP1/SP2、R04-V1。只提交 m2，不推送、不合并 main、不提前实施 05/07/08/12/13。按 Q105 继续交同一 Pi 会话；渠道报错保留改动并重试，连续三次才由规划方接管。

### 2026-10-23 / Pi 审查修复（第 1 轮）

受审实现 `a38fd85`；本轮逐项处理 `../evidence/task04-review01/` 的 6 项发现。原始证据目录未被修改或覆盖。

#### R04-ST1 / P2（Windows 预览可能改写用户 Git 配置）— 已修复

- 新增 `Git::in_isolated_directory`：在切换工作目录之外，**剔除所有把 Git 定位或配置重定向到别处的环境变量**（`GIT_DIR`、`GIT_WORK_TREE`、`GIT_INDEX_FILE`、`GIT_OBJECT_DIRECTORY`、`GIT_ALTERNATE_OBJECT_DIRECTORIES`、`GIT_COMMON_DIR`、`GIT_NAMESPACE`、`GIT_PREFIX`、`GIT_CEILING_DIRECTORIES` 等），按**大小写不敏感**比较（Windows 环境变量名不区分大小写）。预览的 `init`/`config`/`apply` 全部改用该隔离上下文；正常 worktree 操作不受影响。
- 回归 `preview_isolates_git_location_environment_from_the_real_repository`：把仓库 `core.filemode` 设为 `false` 后，以带 `GIT_DIR`/`GIT_WORK_TREE` 的环境打开会话并丢弃；断言**原仓库 config 字节完全不变**、`core.filemode` 仍为 `false`、index 不变、工作区正确回退。已用"临时改回 `in_directory`"验证该用例确实会失败（有牙）。
- 审查明确该路径在 macOS 生产白名单下不可达，本轮按 Windows 静态可达性修复，**未声称 macOS 生产路径已复现**。

#### R04-ST2 / P2 与 R04-SP2 / P2（预览缺少真实属性语义）— 已修复

- 预览不再只复制根 `.gitattributes` 与五个 core 键，而是在临时仓库里重建**等价上下文**：
  1. 目标路径经过的每一级工作区 `.gitattributes`；
  2. `$GIT_DIR/info/attributes`；
  3. 仓库级 `core.attributesFile`（复制内容并把预览配置指向副本）；
  4. 本地 `filter.*` 的全部子键（`clean`/`smudge`/`process`/`required` 等）；
  5. `core.autocrlf`/`core.eol`/`core.safecrlf`/`core.ignorecase`/`core.filemode`。
- 复制完成后**直接核对等价性**：对每个目标路径分别在本仓库与预览仓库运行 `git check-attr -a`，结果不同则停止丢弃（不改工作区）。这样覆盖了 `.gitattributes` 的层级、`info/attributes`、`core.attributesFile` 以及全局/系统属性的实际优先级，而不是靠再实现一遍规则。
- 回归 `preview_honors_info_attributes_and_repository_attributes_file`：`info/attributes` 的 `*.txt text eol=crlf` 与仓库级 `core.attributesFile` 两种普通文本都能正常丢弃并回到提交时的 CRLF 字节；`preview_honors_local_clean_filter_configuration`：本地 `filter.upper.clean/smudge` 配置下普通文本可丢弃。已用"临时不复制 info/attributes"验证该用例会失败。
- 未新增任何"只读"限制来回避问题；所有新增拒绝路径都只在上下文**无法等价复现**时触发，并明确告知不会修改工作区。

#### R04-SP1 / P1（保存后改属性仍执行不等价写入）— 已修复，并更正上一轮结论

- 新增 `recovery::context_fingerprint`：把上述**有效属性**（每个目标路径的 `check-attr -a` 输出）、相关 core 配置与本地 `filter.*` 一起哈希，保存时写入 `RecoveryRecord.context_fingerprint`（旧记录用 `#[serde(default)]` 兼容）。
- 写入前在 `read_lock` 内重新计算并与记录比较；**不一致即拒绝**（`ErrorKind::Rejected`），把记录标为 `NotApplied`，不写入工作区、不改 index、不自动续跑。
- 回归 `attribute_context_change_after_save_is_rejected_before_writing`：LF 文本 3 个 hunk、保存后把 `text eol=lf` 改成 `text eol=crlf`（目标字节与规范化补丁都不变），断言**写入前**就拒绝、工作区字节不变、index 不变、记录保留且为 `NotApplied`、重开会话不重试。已用"临时删掉该检查"复现审查里的原缺陷（会走到写后待核对），确认用例有牙。
- `unrelated_attribute_file_change_does_not_block_discard`：修改无关文件与无关 `.gitattributes` 不阻止丢弃，避免把"相关上下文"扩大成"任何变化都拒绝"。
- **更正上一轮结论**：上一轮 Comments 写的「若仓库在保存与写入之间修改了 `.gitattributes`，写后比对会失败并转 `NeedsVerification`（安全失败，不会写错内容）」是**错的**。审查已用真实 Git 复现：目标字节与补丁都不变时，属性变化会让真实 `git apply` 把未选中 hunk 的 180 行全部从 LF 改成 CRLF，属于"已经写错、事后才发现"，不能算写前安全。原过程记录保留，本段为更正。

#### R04-ST3 / P3（遮罩颜色写死）— 已修复

- `src/index.css` 新增 `--overlay` 主题变量（浅色 `oklch(0 0 0 / 35%)`、深色 `oklch(0 0 0 / 55%)`，含 `@media (prefers-color-scheme: dark)` 与 `.dark` 两处），`.recovery-dialog::backdrop` 改用 `var(--overlay)`。未做其它视觉改动，未引入测试框架。

#### R04-V1 / T11（写入中失败/中断缺少真实回归）— 已补

- 新增真实 Git 回归 `git_apply_lost_receipt_keeps_record_and_never_retries`（Unix）：用一个**严格限定 fixture** 的 `git` 包装器——只对真实仓库目录下的 `apply` 生效，先调用真实 Git 完成写入，再返回 141 模拟"失去成功回执"；其余子命令与预览 scratch 一律透传。
- 断言：错误为 `NeedsVerification`（不是"未执行"）；工作区确实已回到提交内容（真实写入发生）；index 不变；**实际 apply 只执行一次**（计数文件，无自动重试）；完整记录保留且无 `outcome.json`；重新打开会话只读真实状态、记录数仍为 1、apply 计数仍为 1。
- 该用例覆盖的是"调用内部失败/结果丢失"，与既有 `AfterWrite`/`BeforeComplete` 钩子（调用前后的受控中断）互补，不再把 `AfterWrite` 当作全部"写入中"覆盖。

#### 本轮实际执行的检查

- `cargo test --manifest-path src-tauri/Cargo.toml`：**173 项通过**
  - lib 85、`discard_recovery` **28**、`file_staging` 20、`hunk_staging` 23、`refresh_isolation` 14、`session_flow` 3
- Node 内置工具：25 + 13 + 11 + 19 = **68 项通过**
- `pnpm exec tsc -b`、`pnpm lint`、`pnpm build`：通过
- 未重建 `.app`（按指示等规划方复核后再构建），未运行 GUI/Windows/D06/长测。

#### 仍未验证 / 遗留风险

- **真实桌面、Windows 实机与 CI 未执行**：本轮只做本机 macOS 与真实 Git 自动化；`in_isolated_directory` 的 Windows 语义按静态推理与大小写不敏感写法处理，需 CI 实际编译/运行确认。
- **写前检查与 exec 之间的极小窗口**：上下文在 `read_lock` 内核对后、Git 子进程启动前，外部进程仍可能改写 `.gitattributes`。这是进程内锁无法覆盖的外部竞态；发生时写后比对会转 `NeedsVerification`（但按本轮结论，**这不等于写前安全**）。未宣称已消除该窗口。
- **属性等价性依赖 `check-attr` 自证**：若某来源的属性无法在临时仓库复现，会走"无法等价复现"的明确拒绝，而不是继续写入；这类拒绝尚未在真实桌面统计出现频率。
- 未做长时内存/大文件压力观测；单条记录仍按完整文件大小存储，容量上限与 14 天清理属 05。

### 2026-10-23 / Codex 第二轮两轴审查：仍需修复

固定点 `9b3f2831ef0d0cbb0474547d552fdc4526e69eb9`，受审实现 `6d8a7090d96e5b83a2f8a77c08b0106f2db7c566`；命令 `git diff 9b3f283...6d8a709`，唯一修复提交 `6d8a709`。`.codex/` 为未跟踪本地环境目录，用户已于本轮要求继续，按原样保留、忽略、不读取、不提交。以下仍按两个独立审查轴保留。

#### Standards

- **R04-R2-ST1 / P2 / 原 ST1 残留，Windows**：`src-tauri/src/git/mod.rs:175` 的隔离名单遗漏 `GIT_CONFIG`。Windows 继承该变量后，`src-tauri/src/recovery.rs:340` 的 `config --add` 仍向它指定的真实仓库配置追加 filter；`:378` 的属性路径设置同样能改写该配置。违反 `docs/product-direction.md:206`「不修改用户的 Git 配置」。独立临时真实 Git 已确认原 config 哈希改变、clean/smudge 重复；主审另复现真实 `core.attributesFile` 被改指向临时 scratch 文件（scratch 清除后该路径失效）。原 `GIT_DIR/GIT_WORK_TREE` 路径已修复，但不能据此称全部配置重定向已隔离。应确保 scratch 配置写入绝不可能落入用户配置，补 `GIT_CONFIG` 回归并核对原配置字节和 index。生产 Windows 可达性为环境继承静态结论，未冒称 Windows 实机验收。
- **R04-R2-ST2 / P2 / 原 ST2 残留**：`src-tauri/src/recovery.rs:360` 用 linked worktree 私有 `--git-dir` 拼 `info/attributes`；Git 实际使用 commonDir 下的文件。真实 Git fixture 里目标 `text/eol=crlf` 属性未复制到 scratch，`:385` 的比较会拒绝正常文本丢弃，仍违反 `docs/product-direction.md:52` 的普通已跟踪文本支持范围。应通过 Git 的 `rev-parse --git-path info/attributes` 等实际路径解析取有效位置，而不是猜私有目录布局。

原 ST3 已闭合：遮罩颜色使用主题变量。没有其他确定的规范违规或值得单列的 Fowler 判断性异味。本轴 2 项，最高 P2。

#### Spec

- **R04-R2-SP1 / P2 / 原 SP2 残留，linked worktree**：规格 `docs/milestones/m2/spec.md:73` 要求普通已跟踪文本支持丢弃。`src-tauri/src/recovery.rs:360` 取错共享属性位置，使真实有效属性为 `text/eol=crlf`、scratch 属性却为空，随后拒绝正常文本。与 R04-R2-ST2 同修，保留本轴独立结论。
- **R04-R2-SP2 / P2 / 原 SP2 残留，多行 filter 值损坏**：同一规格要求受影响。`src-tauri/src/git/mod.rs:414` 用 `.lines()` 拆配置，`src-tauri/src/recovery.rs:339` 再按空格拆键值，不能保留合法多行 shell 命令。真实复现 `clean=cat`、`smudge=":\ncat"`：复制后 smudge 只剩 `:`，两边 `check-attr` 完全相同，但 scratch apply 退出 128、真实仓库退出 0 并恢复原内容。应使用 NUL 分隔等无歧义的 Git 配置格式，保留完整值、空白、多值与必要键名边界；写前上下文指纹也使用同一无损口径。不能用「属性检查一致」冒充 filter 命令一致。
- **R04-R2-SP3 / P2 / 环境隔离残留**：`src-tauri/src/git/mod.rs:175` 仍保留 `GIT_CONFIG`，使预览配置命令改写用户配置，违反 `docs/product-direction.md:206` 的安全边界；与 R04-R2-ST1 同修。真实 Git 重定向已复现，Windows 应用可达性仅静态确认。

原 SP1 的保存后属性变化场景已有写前校验，静态闭合；V1 的新回归确实在真实 Git 写入后模拟失去成功结果，并断言记录保留、结果待核对、index 不变且不重跑，补齐了原验证缺口。无新范围扩张。本轴 3 项，最高 P2。

#### 主审复现与后续

主审在 `/private/tmp` 独立 fixture 复现了上述三条路径，原始脚本、命令、结果、config 前后字节及各轴独立结果保存在 `../evidence/task04-review02/`，manifest 记录来源与哈希。没有对用户仓库执行复现写入。linked worktree 同一补丁在真实 Git 中可正确应用且 index 不变；多行 filter 两侧有效属性相同，不能被现有自证检查识别。

本轮仅要求补齐上述安全与等价性缺口；沿用真实 Git、独立数据目录和现有测试体系，不扩大 04 范围，不提前做 05/07/08/12/13。相关修改完成后集中运行最小受影响回归；共享 Git 配置 API 的返回契约变化须检查其调用者。原 SP1/V1 已闭合，不无理由重做整套旧验收。GUI、D06、Windows CI 与最终长测仍未完成；修复交回 Pi，连续渠道报错三次才接管，不把测试失败或 `paseo wait` 等待到时计作渠道错误。

### 2026-10-23 / Pi 审查修复（第 2 轮）

受审实现 `6d8a709`；本轮逐项处理 `../evidence/task04-review02/` 的 3 条发现。原两轮证据目录均未修改或覆盖。上一轮已闭合的 SP1（保存后属性变化写前拒绝）、V1（真实写入后回执丢失）与 ST3（CSS 变量）本轮未重做。

#### R04-R2-ST1 / R04-R2-SP3（`GIT_CONFIG` 使 scratch 配置写入落进用户配置）— 已修复

- `Git::in_isolated_directory` 的剔除名单补齐配置重定向：`GIT_CONFIG`、`GIT_CONFIG_COUNT`、`GIT_CONFIG_PARAMETERS`，以及 `GIT_CONFIG_KEY_*` / `GIT_CONFIG_VALUE_*`（前缀匹配，大小写不敏感）。此前只覆盖定位类变量，确实漏掉了配置类。
- 回归 `git_config_redirect_environment_never_writes_user_config`：在真实仓库里预置 `filter.demo.clean/smudge` 与 `core.attributesFile`，把 `GIT_CONFIG` 指向该仓库 config 后打开会话并丢弃，断言**整份 config 字节完全不变**、`git config --get-all filter.demo.clean` 仍只有一条（无重复追加）、`core.attributesFile` 仍指向原文件且原文件仍在、index 不变、丢弃正常完成。
- 已用"临时从名单移除 `GIT_CONFIG`"验证该用例会失败（有牙），与审查复现一致。
- **更正上一轮范围声明**：上一轮 Comments 写「`init`/`config`/`apply` 全部改用隔离上下文」「剔除所有把 Git 定位或配置重定向到别处的环境变量」——当时只覆盖了定位类变量，配置类未覆盖，该说法过宽。原文保留，本段为更正。

#### R04-R2-ST2 / R04-R2-SP1（linked worktree 的 `info/attributes` 取错位置）— 已修复

- 改为使用 Git 自己解析的路径：`git rev-parse --git-path info/attributes`（新增 `Git::git_path`），相对路径按工作目录解析。不再用私有 `--git-dir` 拼 `info/attributes`——linked worktree 的私有 gitDir 下并没有这个文件，Git 实际用的是 commonDir 下那份。
- 回归 `linked_worktree_uses_shared_info_attributes_for_discard`：主仓库 + `git worktree add` 的 linked，在**共享** `.git/info/attributes` 设 `*.txt text eol=crlf`，linked 内两个 CRLF 文本分别做整文件丢弃与单 hunk 丢弃；断言整文件回到提交字节、未选中 hunk 保持提交内容、linked 的 index 不变、两条记录的身份确认为 linked 且 `worktreePath` 为 linked（canonical），main 身份查询同一批记录为未确认（不串记录）。
- 原 main 仓库的 `info/attributes` / `core.attributesFile` / 本地 filter 用例全部保留并通过。
- 已用"临时改回 `--git-dir` 拼路径"验证该用例会失败（有牙）。

#### R04-R2-SP2（多行 filter 值被按行拆坏）— 已修复

- `Git::config_entries` 改为**无损**读取：`git config --null --get-regexp`，解析 `key\nvalue\0` 记录，值内部换行、空白、空值与多值全部原样保留；返回值从 `Vec<String>` 改为带类型的 `Vec<(String, String)>`。
- 复制到预览仓库时用 `config --add <key> <value>` 逐条传单个参数，不再按空格拆分、不再按行重建命令。
- `context_fingerprint` 使用同一无损口径（键 + 值长度 + 值），避免"指纹相同但命令不同"。
- 共享 API 调用者已检查：`config_entries` 的两个调用点（复制 filter、上下文指纹）均已更新，`git_path` 单点新增；`file_staging`/`hunk_staging`/`refresh_isolation`/`session_flow` 全部复跑通过。
- 回归 `multiline_filter_command_is_copied_losslessly`：真实 Git 配置 `clean=cat`、`smudge=":\ncat"`、`required=true`，普通文本可正常丢弃且字节、index 正确；原单行 filter 用例保留。
- 已用"临时改回按行拆分"验证该用例会失败（有牙），失败形态与审查一致（scratch apply 失败、真实仓库成功）。

#### 本轮实际执行的检查

- `cargo test --manifest-path src-tauri/Cargo.toml`：**176 项通过**
  - lib 85、`discard_recovery` **31**、`file_staging` 20、`hunk_staging` 23、`refresh_isolation` 14、`session_flow` 3
- 未改前端，因此未重复 `tsc`/`lint`/`build`/Node 工具，也未重建 `.app`（按指示等规划方复核后再构建）。
- 三个新回归均做过"回退修复即失败"的对照验证。

#### 仍未验证 / 遗留风险

- **真实桌面、Windows 实机、CI、D06、最终长测均未执行**，未声称通过。
- `GIT_CONFIG` 相关路径在 macOS 生产环境被现有白名单挡住，Windows 可达性为**静态结论**（`shell.rs` 的 Windows 分支继承进程全部环境），未做 Windows 实机验证。
- `git rev-parse --git-path` 在极旧 Git 上的行为未单独验证（项目最低要求 Git 2.11；本轮在 Git 2.56 实测）。
- 写前核对与 Git 子进程启动之间的外部竞态窗口仍未消除（与上一轮结论一致）。
- 记录容量上限与 14 天清理仍属 05，未做。


### 2026-10-23 / Codex 第三轮两轴增量审查：通过；桌面发现新边界遗漏

第三轮固定增量 `git diff 970c690...36dad65`，受审提交 `36dad654fe0d2d337e34d7bd69aaae04d833903f`。两轴独立，未重复上一轮已闭合的 SP1/V1/ST3。

#### Standards

**无新增发现。** R2-ST1 的 `GIT_CONFIG` / 配置注入隔离、R2-ST2 的 linked 有效属性路径均闭合。配置 API 的 NUL 记录、首 LF 键值边界与两个调用者同步，未发现确定规范违规或值得单列的 Fowler 判断性建议。独立真实 Git 核对原配置前后字节与 SHA 完全相同，多行/空值/多值配置复制无损，linked 与 scratch 的实际属性相同。独立证据在 `../evidence/task04-review03/`。此轴未跑项目构建/测试，也未做 Windows 实机验收。

#### Spec

**无新增确定缺陷。** R2-SP1 的 linked `git-path`、R2-SP2 的多行 filter 无损值与上下文指纹、R2-SP3 的配置重定向隔离均静态闭合；无缺失或范围扩张。本轴仅静态复核与 `git diff --check`，未亲自重跑 Rust，不把未执行的桌面/Windows 验收记为通过。

#### 真实桌面第一轮：大部分通过，但任务不关闭

规划方以 `36dad65` 的 Release bundle（二进制 SHA-256 `6e119703a7ecf9db37b0f2db0072bbb802f6088432b59b819caeebab21d8dfc9`）完成隔离 A/B/L fixture 桌面操作。主要进程 PID 71299，重启保留 PID 79082，Git 2.56.0；生产 watcher 开启。前一 Git 2.54 进程未做写入，排除写验收。截图只在 CUA 会话展示，不声称有本地 PNG。两受测进程均正常退出，控制文件无待执行 arm，hold 已显式释放。

- 并排中间 hunk、统一首 hunk、默认取消焦点、弹窗内快捷键不穿透、Tab/Space 确认、diff 焦点下整文件快捷键范围通过；完整文件保存、未选 hunk 和 A/B/L index 不变。
- 打开旧 hunk 确认后修改同文件其他 hunk，watcher 使确认禁用并明确失效，键盘不能执行；取消后显示新快照，不套旧序号。
- 真实回收目录权限失败显示 `Permission denied`，工作区/暂存区/记录数全部不变。
- 暂停真实 A apply 时同 worktree 写入口禁用，仍可浏览文件及切到 linked；显式释放后 A 结果不覆盖 L。部分暂存只恢复到 index，不恢复到 HEAD。
- L 真实 apply 成功后 wrapper 返回 141，界面显示「结果待核对」、读取真实两 hunk，记录保留且无完成回执；重启后仍待核对，实际 apply 计数始终为 1。
- 二进制、mode-only、删除状态、rename、中文空格路径整文件丢弃通过。记录包含二进制完整字节、可执行位、不存在状态及 rename 新旧路径。已暂存、符号链接、未跟踪无丢弃入口，快捷键不执行。
- 回收区显示时间/来源/目标/保存状态/原 worktree，非当前记录明确标记；重启保留全部 10 条（9 完成 + 1 待核对），前后文件/回执一致。`assertions.json` 的 **151 项**字节/权限/归属/index 断言通过。
- `partial-finished-on-linked.json` 虽以 finished 命名，实际捕获在 release 后、完成前，不用于声称完成；后续稳定快照及 Completed 回执证明完成。原始记录不改写。证据在 `../evidence/task04-desktop01/`。

#### R04-GUI1 / P2 / Git rename 的未暂存 M 条目仍暴露 hunk 操作

依据规格第 2 节/T03（Q81）：「Git 识别的重命名」只整文件、无 hunk 操作。真实 fixture 的 porcelain type-2 `RM` 在已暂存组为 R、未暂存组为 M，两条都保留 `original_path`。桌面 `50-renamed-new.txt +1 −0 M` 的 Help 已显示 `50-renamed-old.txt → 50-renamed-new.txt`，但统一 diff `@@ -1 +1,2 @@` 仍有「暂存此 hunk」「丢弃此 hunk」。`src-tauri/src/session.rs` 的 `supports_hunk_actions` 只检查 `entry.status`/`file.status`，没有检查原路径元数据；这个 M 条目通过共享能力 gate，`hunk_patch` 也会接受其凭据。未在 rename 上执行 hunk，实际只执行了整文件丢弃。

请在共享能力边界按 Git 实际 rename 元数据拒绝 hunk，覆盖 type-2 RM 与后端拒绝，保持整文件丢弃、完整相关路径记录不回归；检查该 gate 对暂存/取消暂存调用者的影响。不得自行猜测删除/未跟踪配对，不扩大 04 到 05/07/08/12/13。此为既有契约漏验，不是新产品决策。修复后集中运行受影响检查、提交 m2 并逐项回应；规划方做增量两轴审查和针对性真实桌面补验。

已按 Q97 正常推送 `36dad65`，CI #39965209864 启动；本条记录时仍进行中。该版本 D06 未执行，最终 30 分钟长测仍留任务 11，不据前版数据冒称本版通过。按 Q105 继续同一 Pi；连续渠道错误计数为 0。

### 2026-10-23 / Pi 修复 R04-GUI1（Git rename 的 hunk 边界）

依据 `../evidence/task04-desktop01/` 的桌面发现。原始证据目录（`task04-desktop01`、`review01`–`review03`）未修改或覆盖；本轮只改共享能力 gate 与相应回归。

#### R04-GUI1 / P2（porcelain `RM` 的未暂存 `M` 仍暴露 hunk）— 已修复

- 根因：`supports_hunk_actions` 只看 `entry.status` / `file.status`。porcelain type-2 `RM`（已暂存改名 + 新路径有未暂存修改）会同时给出 staged `R` 与 unstaged `M`，两者都带 Git 的 `original_path`；未暂存侧因此被当成普通 `M`，暴露 hunk 并让 `hunk_patch` 接受其凭据。
- 修复：共享 gate 按 **Git 给出的 rename 元数据**判断——`entry.original_path.is_some() || file.old_path.is_some()` 时一律不提供 hunk 能力。该 gate 同时被 `stage_hunk`、`unstage_hunk`、`discard_hunk` 的 `hunk_patch` 复用，因此前端不渲染、后端也拒绝，不依赖前端是否隐藏按钮。没有猜测「删除 + 未跟踪」配对，普通 `M` 文件不受影响。
- 回归 `t03_git_rename_never_exposes_hunk_actions_on_either_side`（`hunk_staging.rs`，真实 Git）：构造 `RM`，断言 staged `R` 与 unstaged `M` 都保留 `original_path`；两个变更组 × 两种视图的 hunk 行 `hunk_action` 全为空，但未暂存侧的内容改动仍显示为 hunk 行（diff 可见）；`stage_hunk`/`unstage_hunk` 以「这个文件不支持 hunk 操作」拒绝；普通 `M` 文件仍有一个可用的 hunk 动作；整文件暂存/取消暂存仍可用且不删除工作区文件。
- 回归 `git_rename_unstaged_side_rejects_hunk_discard_but_keeps_whole_file`（`discard_recovery.rs`，真实 Git）：hunk 丢弃被同一 gate 拒绝且**不产生回收记录**、工作区与 index 不变；随后整文件丢弃成功，记录仍同时包含 `old.txt` 与 `new.txt` 两条相关路径，index 不变。
- 两个回归都做过对照：把 gate 判断临时改成恒 `false` 后，`hunk_staging` 用例在「rename 不得暴露 hunk 动作」处失败，`discard_recovery` 用例在「必须由共享能力 gate 拒绝」处失败（错误文案退化为「文件已变化，请重新确认」），确认用例有牙。

#### 附带发现并修复：`RM` 未暂存条目的整文件暂存失败（同一用例暴露）

- 现象：对 `RM` 的未暂存 `M` 条目执行整文件暂存时，`execute_staging_op` 把 `entry.original_path` 也传给 `git add -A --`，而 `old.txt` 已既不在工作区也不在暂存区，Git 直接 `fatal: pathspec 'old.txt' did not match any files`（退出码 128），整文件暂存改名条目实际不可用。
- 修复：暂存侧只用当前路径（`stage_file(path, None)`）。`original_path` 只在「取消暂存已识别改名」时需要（`git reset -- old new`），该行为与既有 T04 用例保持不变。
- 覆盖：上述 `hunk_staging` 回归里的「整文件暂存 rename 未暂存条目」一段；`file_staging` 全部 20 项（含改名取消暂存）保持通过。
- 说明：这是重命名整文件契约（Q81）内的既有缺陷，不是新增产品决策；仅做最小修正，未扩大 04 范围。

#### 前端

未改前端：hunk 按钮由 `row.hunk_action` 驱动，后端不再下发即不渲染，因此没有新增 UI 逻辑，也未重复 `tsc`/`lint`/`build`/Node 工具或重建 `.app`。

#### 本轮实际检查（同一受影响套件，各跑一次）

- `cargo test --lib`：85 项通过
- `cargo test --test discard_recovery`：**32** 项通过
- `cargo test --test hunk_staging`：**24** 项通过
- `cargo test --test file_staging`：20 项通过
- `cargo test --test session_flow`：3 项通过
- 未运行 GUI、Windows、CI、D06、长测；未重建 Release `.app`。

#### 一个与本轮改动无关的既有测试不稳（如实记录，未修）

- `cargo test --test refresh_isolation` 的 `t13_concurrent_refreshes_never_publish_stale_content` 在**未修改该文件**的 HEAD 版本上即间歇失败（观察：5 次运行中 2 次失败；两次完整套件运行中 1 次失败）。失败信息为「版本 N 的指纹不在预计算表中」。
- 最小复现：`cargo test --manifest-path src-tauri/Cargo.toml --test refresh_isolation`。
- 该用例属于任务 06 的刷新隔离，与本轮 rename gate / 整文件暂存改动没有代码路径交集；我在诊断中曾尝试把测试写入改为原子替换，仍会复现，故已**完整恢复该测试文件到 HEAD 原样**，没有通过忽略未知指纹、重试或放宽容差让它变绿，也没有改动 refresh 生产逻辑。失败原因未归因，交规划方判断。
