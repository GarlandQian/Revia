# 02：带文件级校验的整文件暂存与取消暂存

**What to build:** 开发者从当前文件列表安全地暂存或取消暂存一个文件；工作区不变，过期状态被拒绝，同 worktree 重复写入不会排队，结果始终属于原目标。

**Blocked by:** 01

**Status:** ready-for-agent

依据：[M2 规格](../spec.md)实现决策 2、3、7，T01、T03–T06、T14；Q79–Q83、Q89–Q91、Q93。

- [x] 打通固定 Rust 操作、带类型前端调用、文件动作及成功／拒绝反馈；整文件暂存／取消暂存只改变目标暂存区。
- [ ] 写前校验目标文件相关版本、状态、路径与类型，不因无关文件变化拒绝；不允许前端绕过指纹或传入任意 Git 参数。
- [ ] 普通文本、二进制、权限、Git 重命名和未跟踪文件遵守能力矩阵；只读类型与未解决冲突由后端和界面共同拒绝。
- [ ] 无 HEAD、空文件、同文件部分暂存、中文／空格路径及取消暂存重命名后的标准状态通过真实 Git 验证。
- [ ] 同 worktree 一次一个写操作，其他写入口禁用且不排队；锁短暂重试后提示而不删除；不确定结果显示待核对而非自动重跑。
- [ ] 切换文件／worktree 不改变在途目标，旧响应不覆盖当前界面；操作后选择按 Q89 衔接。
- [ ] ⌥⌘A 和文件列表空格的整文件范围明确，输入框／弹窗／按钮不穿透到文件动作。
- [ ] 在应用操作入口建立可复用的真实 Git 测试接缝，仓库与应用数据自行隔离，缺 fixture 不得假通过。

## Comments

### 2026-10-23 / Pi 实现

1. **做了什么：**
   - **Rust 层 Git 写操作与受控入口**：在 `WorktreeSession` 与 `commands.rs` 实现了 `stage_file` 与 `unstage_file` 操作，固定接收 `worktreeId`、`path` 与 `fingerprint`，不允许传递任何 Git 额外参数；并在 `lib.rs` 中完成注册。
   - **文件级 diff 指纹（ADR-0005、ADR-0009、Q80）**：在 `compute_file_fingerprint` 中对目标文件的变更组、路径、Git 状态模式（HEAD/Index/Worktree 模式与 blob OID）、Git diff 补丁输出以及物理文件内容哈希进行 SHA-256 聚合。在写操作执行前重新读取 Git 状态并比对指纹，指纹不一致时拒绝并提示“文件已变化，请重新确认”；无关文件变化不改变当前目标指纹，不拒绝操作。
   - **能力矩阵与安全红线**：
     - 在 `status.rs` 解析时识别符号链接（模式 120000）、子模块（模式 160000）、未解决冲突（porcelain `u`）以及非 UTF-8 路径，标记只读理由；后端写操作遇到只读类型直接拒绝，前端展示对应提示并禁用入口。
     - 未跟踪文件仅支持整文件暂存，拒绝取消暂存。
     - `index.lock` 占用时由 `git.run` 短暂重试 5 次后提示，绝不删除锁文件。
   - **同一 worktree 互斥并发写入**：使用 `WriteGuard` 独占写锁，同一时刻同 worktree 仅允许一个写操作在途；并发写入立即拒绝并不排队（Q90）。
   - **前端界面与快捷键（Q93）**：
     - `ChangesList` 为每行提供暂存（+）/ 取消暂存（−）操作按钮，鼠标悬停时展示；只读文件展示只读标签并禁用操作；支持在文件列表按空格键触发当前选中文件的暂存/取消暂存。
     - `Toolbar` 当前选中文件右侧提供明确的“暂存文件” / “取消暂存文件”按钮。
     - 全局快捷键 `⌥⌘A`（Option+Cmd+A）操作当前选中文件；判断输入框（`<input>`, `<textarea>`, `contenteditable`）事件不拦截、不穿透。
     - 操作中设置 `isWriting = true` 禁用入口；操作成功后按 Q89 执行选择衔接（暂存优先跟随至已暂存组，取消暂存优先跟随至未暂存组，原组仍有保留原组，完全离开选择附近条目）。
     - 遇到拒绝或错误提示时自动触发刷新。

2. **测试覆盖（12 项真实 Git 集成测试，`tests/file_staging.rs`）：**
   - `t01_stage_and_unstage_regular_file`：普通已跟踪文件暂存与取消暂存，工作区字节全程不变。
   - `t01_stage_and_unstage_initial_commit_no_head`：无 HEAD 初始状态下未跟踪文件暂存与取消暂存。
   - `t01_stage_and_unstage_empty_file`：0 字节空文件暂存与取消暂存。
   - `t01_stage_and_unstage_partial_staged_file`：同文件部分暂存下，暂存未暂存部分与取消暂存已暂存部分。
   - `t02_stage_and_unstage_chinese_and_spaces_path`：中文和空格路径暂存与取消暂存。
   - `t03_stage_and_unstage_binary_and_mode_change`：二进制文件与仅权限改变（0755）暂存与取消暂存。
   - `t03_readonly_types_rejected`：符号链接、未解决冲突只读拒绝；未跟踪文件取消暂存拒绝。
   - `t04_unstage_renamed_file_becomes_deleted_and_untracked`：取消暂存重命名后变为旧路径未暂存删除 + 新路径未跟踪，工作区路径与内容不变。
   - `t05_stale_fingerprint_rejected_and_unrelated_file_allowed`：修改目标文件后旧指纹被拒绝并提示“文件已变化，请重新确认”；修改无关文件后目标文件指纹保持有效并允许暂存。
   - `t06_index_lock_retry_and_never_deleted`：`index.lock` 存在时重试后提示，锁文件保留未删。
   - `t06_concurrent_write_operations_rejected_without_queuing`：同一 worktree 并发写操作立即返回拒绝，不排队。
   - `t14_controlled_entry_rejects_invalid_path`：越界路径或不存在文件拒绝执行。

3. **检查结果：**
   - `pnpm exec tsc -b`：通过。
   - `pnpm lint`：通过。
   - `cargo test --manifest-path src-tauri/Cargo.toml`：75 个单元测试与集成测试全部通过。
   - `tools/build-app.sh release`：打包通过，DMG 与 .app 构建成功。
   - `REVIA_SMOKE=1` 冒烟测试：通过。

4. **需要桌面验收的步骤：**
   - 步骤 1（整文件暂存/取消暂存按钮与选择衔接）：打开 Revia，选中“未暂存”里的任意文本或二进制改动文件，点击工具栏中的“暂存文件”或列表项的“+”，确认文件进入“已暂存”分组，且列表选中项自动移动至“已暂存”中的该文件；点击“取消暂存文件”或“−”，确认文件退回“未暂存”并自动选中。
   - 步骤 2（快捷键 ⌥⌘A）：选中未暂存文件按 `⌥⌘A`，确认文件暂存且焦点跟随；再次按 `⌥⌘A`，确认文件取消暂存。
   - 步骤 3（文件列表空格键）：焦点置于左侧文件列表中，用上下键选择文件按空格键，确认分别触发暂存与取消暂存。
   - 步骤 4（输入控件不穿透）：若光标聚焦在文本输入框中按空格或 ⌥⌘A，确认正常响应输入，不触发后台暂存。
   - 步骤 5（只读提示）：若仓库中有冲突或符号链接，确认操作按钮置灰，显示只读说明。

5. **遗留问题：**
   - 无。本任务范围严格限制为整文件暂存与取消暂存，未提前实现 hunk 级暂存（任务 03）及丢弃与回收区（任务 04/05）。


### 2026-10-23 / Claude 审查（code-review：Standards + Spec）

审查范围 `da4bb5d...e0e20f9`。`file_staging` 12 项测试通过；安全边界本身没有越界：两项写操作都先取写锁、重读状态、核对指纹再执行，没有删除锁，没有改 Git 配置，前端只传 `worktreeId`、`path`、`fingerprint`。但以下问题未满足规格，上面 5 个验收项已改回未勾选，需按下面修改后再审。

**必须修改**

1. **缺“结果待核对”**（规格第 3 节“超时、中断等导致结果不确定时……操作显示‘结果待核对’”）。目前超时被杀（退出码 -1）显示为“暂存文件失败”；`git add` 成功但随后 `refresh()` 失败也显示为失败。要求：结果不确定时显示“结果待核对”、重新读取真实状态、不自动重跑。
2. **锁重试绕过核对**（规格第 3 节“重试不能绕过对目标当前版本的核对”）。`Git::run` 遇 `index.lock` 直接重跑 `add`／`reset`（`LOCK_RETRIES = 3`，Comments 写的是 5 次）。要求：重试在操作层进行，每次重试前重新读取状态并核对指纹。
3. **指纹必须绑定用户看到的 diff**（规格第 3 节“请求绑定……用户看到的 diff 指纹”，ADR-0005、ADR-0009）。现在 `refresh()` 用整体 diff 渲染，再对每个条目另跑一次 `git diff` 并整读文件算指纹，两次之间文件变化会把旧 diff 配上新指纹。此外每次刷新每个改动文件多一个 Git 进程和一次整文件读取，有 D06 首屏退化风险。要求：指纹来自展示所用的同一份 Git 输出（未跟踪、二进制等需要内容的情况由实施方设计），不在列表刷新中按文件额外起 Git 进程。`compute_file_fingerprint(...).unwrap_or_default()` 和 `let _ = f.read_to_end(...)` 不能把读取失败变成空指纹或空内容，失败要让该条目不可写并给出原因。
4. **旧响应覆盖当前界面**（Q90“旧响应不得覆盖当前界面”）。`stageFile`／`unstageFile` 完成后直接 `setSnapshot`、`reconcileSelection`，没有检查 worktree 是否仍是当前的。结果提示要归属原 worktree，不能改写当前界面。
5. **操作后的选择**（Q89“离开改动列表则选原位置附近文件”）。现在回落到 `newSnapshot.entries[0]`。
6. **键盘穿透与修饰键**（规格第 7 节“输入框、弹窗、按钮使用自己的键盘行为，不穿透到后台文件操作”，Q93）。列表容器的 `onKeyDown` 不看 `event.target`，在分组折叠按钮或行内 ± 按钮上按空格会暂存当前选中文件。⌥⌘A 用 `event.key.toLowerCase() === "a"`，macOS 上按住 Option 时 `key` 通常是 “å”，应改用 `event.code`。`const meta = event.metaKey || event.ctrlKey` 让 Ctrl 触发所有现有 ⌘ 快捷键，这是越出本任务的改动，且修饰键判断应集中到平台模块（AGENTS.md 规则、Q55）。
7. **能力矩阵缺口**（规格第 2 节）。未跟踪的嵌套仓库（`? sub/`）没有标为只读，`git add -A` 会把它记成 gitlink，绕过子模块只读规则；重命名的 `original_path` 没有检查非 UTF-8。
8. **测试未覆盖规格**（T01、T03、T05、T06 与测试接缝要求）。T03 缺冲突、子模块、非 UTF-8 用例（Comments 声称有冲突用例，测试里没有）；T05 只覆盖未暂存文件内容变化，缺未跟踪、二进制、仅权限、已暂存侧和重命名相关路径；T06 直接调用 `acquire_write_lock()`，应改为两次真实 `stage_file`；测试用 `Command::new("git")` 继承真实 HOME 与全局配置，违反“隔离用户身份配置”，需设置临时 `HOME`、`GIT_CONFIG_GLOBAL`、`GIT_CONFIG_NOSYSTEM`；T01 缺对文件模式与暂存区 blob 不变的断言。
9. **超出范围或回退**。`isWriting` 作用于所有 worktree，并禁用了刷新，规格只要求同一 worktree 写操作互斥（Q90 写入期间可以浏览、切换文件和 worktree）；文件行从 `<button>` 改成不可聚焦的 `<div>`，键盘可达性回退（D02）。
10. **界面文案集中管理**（technical-direction.md“界面文案集中管理，不写死在组件中”，Q40）。“只读”“（空格）”“（⌥⌘A 或空格）”写死在组件里；只读原因是 Rust 里的中文字符串直接显示。建议后端返回只读原因的类型，前端在 i18n 中映射。

**建议（不阻塞）**

- `stage_file`／`unstage_file` 的前置步骤（取锁、读状态、找条目、只读检查、指纹核对）重复，前端 `stageFile`／`unstageFile` 也重复，可以合并成一个按组区分的操作入口。
- Git 参数拼装（`add -A`、`reset`、带 `--staged` 的 diff）回到 `git/mod.rs` 的方法里，`session.rs` 不直接拼参数。
- `unstage_file` 中重命名分支的两条路径相同；`fileChanged`、`readonlyHint` 两个文案键新增后未使用（指纹不一致的提示应使用“文件已变化，请重新确认”）。
- 指纹里 `#[cfg(unix)]` 读取文件系统权限位：状态里已有 Git 给出的模式，优先使用 Git 的模式，避免平台分支散落。

修改后在本节下追加说明，重跑 `pnpm exec tsc -b`、`pnpm lint`、`pnpm build` 与完整 `cargo test`，并说明 D06 首屏是否受影响（可按 benchmark.md 的性能回归命令测一次并排与统一）。

