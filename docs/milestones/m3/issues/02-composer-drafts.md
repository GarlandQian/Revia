# 02：提交区、模板、最近信息与持久草稿

**What to build:** 实现M3规格F02。

**Blocked by:** 01（实现顺序）

**Status:** ready-for-human

初始Q114批次只实现功能；现按Q115解除执行限制并进行验证，未完成验收不伪标done。

- [x] 现有设计中完整提交区与72字符提示
- [x] 模板、最近20条信息、身份绑定持久草稿
- [x] 代际/持久化失败/切换保护
- [ ] 后续验证（Q115已放行，未完成验收不宣称通过）

## Comments

2026-10-26 / Codex：从m2 073ecfe建立独立m3，整包功能实现，禁止启动GUI/唤醒/测试/构建/lint/tsc/CI。

2026-10-26 / Pi（实施方）：功能实现完成，**未验证**（用户 Q114 只实现功能）。

实现位置与要点：

- `src/components/app/CommitComposer.tsx`：变更工具窗口下方的提交区（岛屿布局、Inter /
  JetBrains Mono / 主题变量）；多行信息框、首行按**字符**（`Array.from`）计数的 72 字符提示、
  空白信息不可提交、最近 20 条信息下拉、amend 与跳过 hooks 复选框、审阅进度、⌘K 提示。
- `src-tauri/src/commit.rs`：`read_template`（只读 `commit.template`，不改配置）、
  `read_recent_messages`（`-z` + `%H%x01%B`，保留多行与 Unicode，无 HEAD 返回空）、
  `DraftStore`（按 worktree 身份哈希落盘，原子写 + 目录同步，读取失败显式报错）。
- `src-tauri/src/commit_flow.rs`：`commit_context` / `save_commit_draft` /
  `clear_commit_draft`；`load_draft` 把读取失败作为可见的 `draftError` 返回而不阻断提交。
- `src/App.tsx` + `src/lib/commitDraft.ts`：`CommitComposerState` 绑定 worktree + 代际，
  旧读取不覆盖新输入；写入串行排队（后一次一定最后落盘），400ms 去抖，提交前与切换前冲刷；
  模板仅在无草稿且未输入时初始化一次；成功只清除仍是本次输入的草稿（正文去尾空白 +
  amend/skip 都相等），失败/待核对保留。
- `src-tauri/src/session.rs::captured_identity`：身份不成立时显式报错，不从同路径旧身份继承。

未执行：`cargo test` / `cargo check` / build / lint / tsc / GUI / 桌面验收。复选框中的
"后续验证"保持未勾选。

### 2026-10-26 规划方静态审查（唯一一次正式返修）

静态审查：提交未等待草稿队列、切回读取与save竞态、正常关闭无flush；异步读取代际捕获过晚；条件清草稿非原子；skipHooks不应跨重启自动恢复。一次返修补齐，禁止运行验证。


### 2026-10-26 Pi（实施方）返修完成

已按审查意见补齐（未运行任何验证）：

- **提交前等待草稿落盘**：`persistCommitDraft` 按 worktree 串行入队并返回可 await 的
  Promise；`flushCommitDraft`/`ensureDraftPersisted` 可等待，`performCommit` 在写门内、
  发出提交前 `await ensureDraftPersisted(target)`，落盘失败则中止提交并显示
  `draftFailed`（不丢稿、不删错稿）。
- **切回读取与 save 竞态**：草稿写入按 worktree 排队（`draftQueuesRef`），切换 worktree
  先 `await draftQueueFor(worktreeId)` 再读上下文，避免先读旧盘内容。
- **正常关闭/退出 flush**：`CloseDriver` 的 `closeWindow`/`exitApp` 经
  `closeAfterDraftFlush` → `flushDraftsBeforeClose` 先冲刷全部 pending、等待所有队列、
  检查 `draftErrorRef`；任何失败经 `onError` 展示且**不**销毁窗口/退出（不静默丢稿）。
- **读取代际在发起时捕获**：`loadCommitContext` 用 `CommitComposerState.ticket()`
  在请求发起时绑定 worktree/激活代号/输入代际，await 回来只按 ticket 判断是否落地；
  模板初始化也用 `queueCommitDraft` 排队保存。
- **条件清草稿原子化**：`DraftStore` 增加跨 clone 共享的 `lock`，`clear_if_matches`
  在锁内「读→逐字比较→删」，与 `save` 互斥；比较用原文（不 trim），期间被重新编辑的
  草稿保留（`commit.rs`、`commit_flow.rs`）。
- **skipHooks 不持久**：`DraftStore.save` 强制 `skip_hooks=false`、`load` 读取时也强制
  false；切换 worktree 用 `resetInput()` 复位；提交完成后（无论结果）仅当输入代际未变
  时复原 skipHooks 并落盘（`commit.rs`、`commitDraft.ts`、`App.tsx`）。

### 2026-10-26 Codex 直接接管收尾

草稿加入身份键与精确 revision；前端切换/持久化失败保留本地缓存，后端锁内条件删除含 revision；提交前不会把旧信息重新写到新稿上。正常关闭等待全部队列。

功能代码已提交，未经编译/测试；本轮不打开 GUI，不运行任何验证，不勾选后续验证，不将 ready-for-human 当作验收通过。历史实施描述以本段和最终报告为准。


### 2026-10-26 最终静态收尾（Codex，`1ca1e52`）

草稿正文与身份键同次读取、返回前复核；身份不可确认时前端不应用旧持久/缓存正文，用户已输入的本地正文保留。模板路径改为 NUL 分隔读取，最近信息保留正文尾随换行。

仅源码实现及静态阅读；未运行任何编译、测试或 GUI，后续验证项仍未勾选。


### 2026-10-26 Q115 验证恢复（Codex）

用户已解除不测试/不启动限制。固定源码`03736a0`的自动化验证已执行：Node 251项、Rust 304项通过，构建/类型/lint及固定Release只读冒烟通过；失败原件和覆盖范围见[验证记录](../validation-20261026.md)。原生桌面读取超时，未完成真实交互/截图、D06/完整长测及Windows编译；Status与最终验证复选框保持ready-for-human/未勾选，不冒称done。未取消休眠、不回派Pi、不推送或合并。
