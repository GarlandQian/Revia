# 02：带文件级校验的整文件暂存与取消暂存

**What to build:** 开发者从当前文件列表安全地暂存或取消暂存一个文件；工作区不变，过期状态被拒绝，同 worktree 重复写入不会排队，结果始终属于原目标。

**Blocked by:** 01

**Status:** ready-for-agent

依据：[M2 规格](../spec.md)实现决策 2、3、7，T01、T03–T06、T14；Q79–Q83、Q89–Q91、Q93。

- [x] 打通固定 Rust 操作、带类型前端调用、文件动作及成功／拒绝反馈；整文件暂存／取消暂存只改变目标暂存区。
- [x] 写前校验目标文件相关版本、状态、路径与类型，不因无关文件变化拒绝；不允许前端绕过指纹或传入任意 Git 参数。
- [x] 普通文本、二进制、权限、Git 重命名和未跟踪文件遵守能力矩阵；只读类型与未解决冲突由后端和界面共同拒绝。
- [x] 无 HEAD、空文件、同文件部分暂存、中文／空格路径及取消暂存重命名后的标准状态通过真实 Git 验证。
- [x] 同 worktree 一次一个写操作，其他写入口禁用且不排队；锁短暂重试后提示而不删除；不确定结果显示待核对而非自动重跑。
- [ ] 切换文件／worktree 不改变在途目标，旧响应不覆盖当前界面；操作后选择按 Q89 衔接。
- [ ] ⌥⌘A 和文件列表空格的整文件范围明确，输入框／弹窗／按钮不穿透到文件动作。
- [x] 在应用操作入口建立可复用的真实 Git 测试接缝，仓库与应用数据自行隔离，缺 fixture 不得假通过。

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

