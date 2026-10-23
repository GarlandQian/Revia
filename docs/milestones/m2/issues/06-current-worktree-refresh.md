# 06：当前 worktree 自动刷新与旧响应隔离

**What to build:** 编辑器或终端改变当前 worktree 时，列表与 diff 自动更新并保留合理选择；切换目录后旧响应不会污染新界面，监视失败可以明确降级。

**Blocked by:** 02

**Status:** ready-for-human

依据：[M2 规格](../spec.md)实现决策 3、6，T05、T13、D03、D05；Q88–Q90、Q95。

- [x] 只实时监视当前 worktree 及相关 Git 元数据，不为所有登记仓库常驻监视。
- [x] 切换 worktree、回到前台和手动刷新都重新核对真实状态；旧请求不能覆盖当前 worktree 或较新的快照。
- [x] 同文件同组仍有改动时留在原位置，跨组时跟随同文件，消失时选择附近文件，无条目时清空 diff。
- [x] 同路径、同组、同行数但内容已变时，文字、高亮、hunk 与操作指纹都更新，旧确认不再有效。
- [x] 监视失败持续提示“自动刷新不可用”，不静默失效；手动／前台刷新仍可用，写前读取或校验失败拒绝写入。
- [ ] 真实 Git 与桌面场景覆盖工作区内容变化、外部暂存／取消暂存及切换时的延迟响应；不退化既有滚动表现。（真实 Git 与自动测量已覆盖；桌面人工验收待规划方执行）

## Comments

### 2026-10-23 / Pi 实现

**范围**：只实施 06。未改 04/05/07/08/10/11，未改产品决策/ADR/GLOSSARY，未合并 main、未推送、未建 PR、未发布、未启动 30 分钟长测。

#### 1. 只监视当前 worktree（新模块 `src-tauri/src/watch.rs`）

- 新依赖 `notify = "8"`（8.2.0，官方 crates.io；本机 Rust 1.99.1 可编译，新增 7 个传递依赖）。用它做跨平台监视，没有自写平台监视框架。
- `WatchRoots` 由 `resolve_roots` 决定：工作区（递归）、该 worktree 自己的 gitDir（递归）、共享 commonDir 的 `refs/` 与顶层（递归）。
- linked worktree 的 `.git` 是**文件**：`resolve_dot_git` 读取 `gitdir:` 行解析真实 gitDir。测试 `resolve_dot_git_reads_gitdir_file_for_linked_worktree` 与真实 Git 用例 `t13_linked_worktree_watch_roots_resolve_real_git_dir` 覆盖，确认不会把 `.git` 文件当目录、gitDir 位于 commonDir 下。
- 不递归其他 worktree，不监视对象库。
- `WatchController` 只保留一个活动 watcher；`activate` 先替换（旧 watcher 在 `Mutex` 赋值时被 drop，停止线程与底层监视），`stop` 释放。测试 `controller_replaces_previous_watcher_on_switch` 验证激活代号递增且 stop 后 `active=false`。
- 事件合并：`DEBOUNCE=120ms` 合并窗口 + 后续事件一并吞掉，只回调一次。测试 `t13_watcher_emits_coalesced_change_notification` 连续写 6 次，断言通知次数 `>=1` 且 `<6`。
- 过滤 `EventKind::Access`，避免读取自身形成刷新环（`access_events_are_filtered`）。
- 事件只递增一个计数器（`AppState::watch_events`），具体读取由前端在合并后触发一次，不在后端排队。

#### 2. 切换 / 前台 / 手动核对与旧响应隔离

- 后端：`ChangesSnapshot` 新增顶层 `revision: u64`（空列表也有，测试 `t13_empty_snapshot_still_carries_revision`）；`RowSlice` 新增 `revision`。
- 后端：`WorktreeSession` 新增 `read_lock`，`refresh()` 把「读取 + 分配 generation + 发布」整段串行化，消除「早开始读取晚发布」的快照倒退。测试 `t13_concurrent_refreshes_never_publish_stale_snapshot` 用 4 线程 × 6 次刷新，断言产生 24 个互不相同且不重复的版本。
- 后端：`AppState::session` 改为全程持会话表锁创建，首次并发不再发布两个同 id 会话（避免分离的快照代际、高亮线程与写锁）；新增 `release_session`。
- 前端：新增 `src/lib/refreshGate.ts`
  - `SnapshotGate`：`activation` 代号 + 每 worktree 已接受最大 revision。只接受当前激活 worktree 且 revision 不倒退的快照。A→B→A 中第一次 A 的迟到响应因 activation 变化被丢弃，即使 worktreeId 相同。
  - `RefreshCoordinator`：single-flight + dirty，执行期间的事件风暴只补一轮，不排无限队列；单轮失败不终止协调器。
- `App.tsx`：`openWorktree` 在请求发起时立刻 `activate(id)`，每个 `await` 后核对 activation；旧请求的 `catch`/`finally` 不再改当前界面的错误或 busy。所有快照经 `acceptSnapshot` 统一门。`refresh`、自动刷新协调器、回到前台（`visibilitychange`/`focus`）走同一条 `readAndAccept` 路径。
- 测试：`tools/test-refresh-gate.mjs`（6 项）覆盖旧版本拒绝、A→B→A、空列表版本、事件合并只补一轮、单轮失败后可继续、dispose。

#### 3. 刷新保持选择

复用 03 已有的 `applyRefreshedSnapshot`：同组同文件 → 跨组同文件 → 原位置附近 → 无条目清空。自动刷新走同一函数，不额外重置视图或滚动位置。

#### 4. 同行数但内容变化

- 文件指纹来自展示用 diff 切片，内容变化即变化；`snapshot_revision` 每次刷新重签，旧 hunk 凭据失效。
- 测试 `t13_same_row_count_content_change_updates_text_and_versions`：等长改写（`line TWO` → `LINE two`，行数与字节数都不变），断言显示行数相同、文件指纹不同、条目版本不同、顶层版本递增、行切片版本与快照一致、新文字可见。
- `t13_untracked_content_change_updates_loaded_metrics`：未跟踪文件追加内容后指纹变化且刷新读到新内容。
- `t13_external_stage_and_unstage_are_reflected`：外部 `git add` / `git reset` 后分组与版本都跟上。
- `t13_refresh_keeps_write_preconditions_intact`：外部改动后用旧指纹写入仍被拒为「文件已变化，请重新确认」，刷新后新指纹可写入。写前校验边界未削弱。

#### 5. 高亮按版本绑定与缓存有界

- `HighlightRequest` 新增 `revision`；`HighlightResult` 新增 `revision` 与 `content_version`。
- 文档键从「全局 diff fingerprint + 路径」改为**稳定键** `{worktree_id}/{changes}/{version}/{path}`，不再随无关文件变化或每次刷新新增文档。
- `generation` 改为由「文档键 + 该版本文件指纹 + 内容」计算的内容版本；同内容不重建解析状态。
- 读取内容后再次核对快照 generation：若期间前进，重试一次，仍不一致则返回「文件已变化，请重新确认」，**绝不把旧内容标成新版本**。
- `highlight_generations` 每次写入前 `retain` 只留当前文档键，防止刷新累积旧键。
- 高亮线程 `documents` 加有界淘汰：上限 24 个，超出丢弃最久未使用的一半（`evict_documents`）。
- 前端 `HighlightStore` 的 target 增加 `snapshotRevision`；响应 `revision` 与 target 不符即丢弃（`丢弃旧高亮结果` 日志），不写入缓存。
- `RowWindowStore` 的 `entryMatchesTarget` 追加 `slice.revision` 校验，晚到的旧行同样使目标失效。
- 测试：`t13_highlight_results_carry_versions_and_stay_bounded` 连续 12 个版本刷新，断言每次高亮 revision 与最新快照对齐且版本递增。

#### 6. 监视失败持续提示

- `WatchStatus` 含 `active` / `error` / `activation` / `roots`；`WatchController` 额外保存 `last`，即使没有 watcher 也保留失败原因。
- 命令 `watch_worktree` / `stop_watch` / `watch_status`；前端每 600ms 轮询状态，失败时工具栏显示「自动刷新不可用」徽标，并在工具栏下方显示固定说明（`messages.changes.watchUnavailableHint`）。
- 手动刷新与回到前台不修改监视状态：测试 `t13_watch_failure_stays_visible_across_manual_refresh` 注入失败后成功 `refresh()`，断言 `active=false` 且 `error` 仍在。
- 确定性接缝：`WatchController::force_failure`（仅供测试/诊断），不靠删真实 `.git` 或大文件 + sleep 制造失败。真实失败路径另有 `watcher_reports_failure_for_missing_worktree`（目录不存在）。

#### 7. worktree 切换入口（验收准备补充）

- 工具栏新增原生 `<select>`，只在 `worktrees.length > 1` 时显示，列出当前仓库的 worktree（分支名，主目录标注「（主目录）」），切换时调用 `selectRepo` + `openWorktree`，监视随之切换。没有做 M4 的完整侧栏。
- 这解决了「只有添加仓库、默认取首个非 bare、选择 linked 目录可能回到 main」的问题：linked worktree 现在可以直接在界面上选中。

#### 8. 实际执行的检查

- `pnpm exec tsc -b`：通过。
- `pnpm lint`：通过。
- `pnpm build`：通过（136 模块）。
- `cargo test --manifest-path src-tauri/Cargo.toml`：**131 项全部通过**
  - lib 单元测试 74（含 watch 模块 6 项、shell 7 项、highlight 8 项）
  - `file_staging` 20、`hunk_staging` 23、`refresh_isolation` 11（本任务新增）、`session_flow` 3
- Node 内置测试：`test-row-window-store` 25、`test-visible-highlight-lines` 13、`test-ipc-diagnostics` 11、`test-refresh-gate` 6（本任务新增），共 55 项全部通过。
- `tools/test_bench_long_running.py`：20 项通过。
- `pnpm tauri build --bundles app`：打包成功（7.52 MiB）。
- `REVIA_SMOKE=1`：7 个 fixture 文件全部打开，视图往返行锚点 3322 与滚动位置不变。
- D06 自动测量（Release，可见窗口，独立数据目录）：

| 视图 | rAF | 首屏 | rafPainted | warm 跳转 | 颜色恢复 | 滚动未覆盖帧 | 帧 p95 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 并排 split | 61 | 90ms | true | 6ms | 22ms | 0 | 18ms |
| 统一 unified | 60 | 93ms | true | 7ms | 15ms | 0 | 17ms |

#### 9. 未验证项（需规划方在真实桌面确认）

- 人工桌面验收：D03 延迟响应（操作变慢期间切换文件/视图/worktree）、D05 四种选择衔接的实际手感、阅读位置保留、真实前台切换、监视失败的可见性与持续提示。
- 首次测量时统一视图出现 1 个未覆盖帧（`uncoveredMax=41`），定位为后台状态轮询与滚动取数争用 IPC；测量/长测/冒烟模式已禁用该轮询（脚本自己驱动），复测两视图均为 0。真实桌面交互下轮询仍启用，未在人工验收中复测滚动表现。
- 真实「外部编辑器原子替换」与「终端持续高频写入」的事件风暴未在桌面人工压测。
- 高亮文档缓存的淘汰只在单元层面（24 上限 + 最新未用淘汰）与 12 个版本刷新的集成测试中验证，未做长时内存观测。

### 2026-10-23 / Pi 审查修复（第 1 轮）

审查对 `e3383ff` 提出 6 项实现缺陷与 3 项证据修正，逐条处理如下。

#### 1. 高亮字节绑定到展示版本（原 P1）

- `session.rs::highlight` 在首次为某一版内容取高亮前，重新读取该文件**当前的相关 diff**（未暂存 / 已暂存 / 未跟踪分别走对应命令），与快照里保存的 `FileContent.raw_patch` **逐字节比较**；不一致直接返回「文件已变化，请重新确认」。
- 不再以 mtime、size 或「快照代际没变」作为正确性依据：代际不变只能说明没调用 refresh，不能说明磁盘字节没变。
- 已暂存侧仍只读该文件自己的 `diff_index_with_orig`，不把无关 worktree 内容并入指纹（ADR-0009 不变）。
- `DiffView.tsx` 增加 `tokensMatchText`：token 拼回的文本与显示文本不一致时退回纯文本，避免渲染出与 Git hunk / 写指纹不符的文字。
- 新增确定性回归 `t13_highlight_rejects_external_edit_before_refresh`：取完快照与行后、**未 refresh** 前做等长（同字节数、同行数）改写，断言高亮被拒；refresh 后断言返回文本来自新内容（含 `const a = 7;`，不含旧值 `const a = 9;`），并校验 `revision` 对齐。

#### 2. 写结果走同一接收门

- `performStaging` 用 `OperationResultGate.begin()` 记录发起时的激活上下文；成功结果经 `acceptSuccess` + `acceptSnapshot` 双重校验后才应用；失败/待核对只在发起目标仍激活时生效。
- 旧的 `catch`/`finally` 不再改当前界面；写互斥、不排队、写入期间可浏览、NeedsVerification 行为均保持不变。
- 新增 `OperationResultGate` 与 3 项 Node 测试：晚到 r2 不覆盖已接受的 r3、A→B→A 的旧写结果被拒、失败结果只在原目标生效。

#### 3. 迟到的选择与元数据响应

- 新增 `SelectionGate`（epoch + activation + 目标文件/组）并在 `openEntry` 中实际使用：未跟踪首次元数据读取后、绑定 store 前都核对；同 worktree U→V 与 A→B→A 的迟到结果均作废。
- 视图切换的行号定位（`rowIndexForLine`）增加选择 epoch、激活代号与目标 `snapshotRevision` 校验，过期结果不再移动视口；未改变既有滚动行为，也不会让每次刷新都等文件顶部。
- 新增 4 项 Node 测试覆盖 U→V、A→B→A、清空选择、激活代号变化。

#### 4. 迟到的监视激活不得替换当前 watcher

- `WatchController::activate` 新增 `requested: u64`（前端发起时的激活代号）与 `accepted` 最大值：代号小于已受理值的迟到请求直接拒绝，不安装、不替换当前监视。
- 启动在 `current` 锁外完成，替换与旧 watcher 释放（含线程 join）在锁外进行，避免持锁 join 阻塞安装。
- `WatchStatus` 增加 `target`，`WatchSnapshot` 改为 `changed_activation`（原为累计计数）；前端轮询只接受 `target` 与当前 worktree 一致、且 `changedActivation === 当前激活代号` 的状态与刷新。
- 新增 `late_activation_request_does_not_replace_current_watcher`（旧 A 请求被拒、当前仍是 B）与 `superseded_watcher_stops_emitting_notifications`（切换后旧目录写入不再产生通知，新目录写入产生通知）。

#### 5. 真实失败路径显式降级

- notify 回调不再丢弃 `Err`：记录首条错误并通过通道上报，监视线程把状态置为 `active=false` 且写入「自动刷新不可用：…」。
- 监视线程创建失败不再 `.ok()` 吞掉，而是写入不可用状态。
- 初始 watcher 创建失败不再重试 + `expect`，直接返回降级状态（`_watcher: None`），不会 panic。
- 手动刷新与回到前台不修改监视状态（`t13_watch_failure_stays_visible_across_manual_refresh` 保留）。

#### 6. 监视范围收敛

- commonDir 不再整棵递归：只注册 `refs/`（递归）与 `packed-refs` / `HEAD` / `ORIG_HEAD`（单文件非递归）；`objects/` 与其他 worktree 的 `worktrees/*/` 明确排除。
- 普通仓库的 gitDir 就是 `<worktree>/.git`，已由工作区递归覆盖，不再重复注册；linked worktree 的 gitDir（`<common>/worktrees/<name>`）单独递归注册。
- 新增 `WatchFilter` 按路径判定相关性，并做路径规范化（新建/删除路径不存在时规范化父目录再拼回文件名；macOS 的 `/var` 与 `/private/var` 归一）。
- 新增真实 Git/文件系统测试 `t13_watch_scope_covers_current_worktree_but_not_unrelated_metadata`：当前文件改动、原子替换（写临时文件后 rename）、外部 `git add`、共享 refs 均触发；**其他 linked worktree 的私有 index 变化不触发**。

#### 性能与证据修正

- **撤销**了「测量模式禁用状态轮询」的做法，生产行为保持启用。改为轮询让开滚动：连续滚动/滚轮/按键后 250ms 内不轮询，空闲时每 1 秒一次。
- 在该生产行为下复测 D06（Release、可见窗口、独立数据目录）：

| 视图 | rAF | 首屏 | rafPainted | warm 跳转 | 滚动未覆盖帧 | 帧 p95 |
| --- | --- | --- | --- | --- | --- | --- |
| 并排 split | 61 | 104ms | true | 7ms | 0 | 18ms |
| 统一 unified | 61 | 76ms | true | 6ms | 0 | 18ms |

- 早前「测量模式禁用轮询」的两次通过样本不作数；失败样本（统一视图 `uncoveredFrames=1`、`uncoveredMax=41`）原样保留在 `/tmp/revia-perf-unified-06.json` 与 `-06b.json`，新样本用 `-06c` / `-06fix-*` 命名，未覆盖旧证据。
- `t13_highlight_results_carry_versions_and_stay_bounded` 拆成两项：版本对齐（`t13_highlight_results_carry_versions...` 语义并入新用例）与**可观测上界** `t13_highlight_cache_stays_bounded_across_versions`（40 个版本后断言 `highlight_document_count() <= 24`，为此在 `Highlighter` 暴露实际文档数）。
- 并发测试改为确定性内容接缝：先单线程建立「指纹 → 内容标记」表，再并发刷新，断言版本递增时内容标记不倒退（旧的「24 个唯一版本」断言确实无法证明内容隔离，已替换）。
- watcher 替换测试已加强为「旧目录不再产生通知、新目录产生通知」，不再只看激活代号与 status。

#### 本轮实际执行的检查

- `pnpm exec tsc -b`：通过。
- `pnpm lint`：通过。
- `pnpm build`：通过（137 模块）。
- `cargo test --manifest-path src-tauri/Cargo.toml`：**136 项通过**
  - lib 77（watch 9）、`file_staging` 20、`hunk_staging` 23、`refresh_isolation` 13、`session_flow` 3
- Node：25 + 13 + 11 + 13 = **62 项通过**；`tools/test_bench_long_running.py` 20 项通过。
- `pnpm tauri build --bundles app`：成功（7.55 MiB）。
- D06：见上表，生产轮询启用下两视图 0 未覆盖帧。

#### 仍未验证 / 遗留风险

- 人工桌面验收（D03 延迟响应、D05 选择衔接手感、真实前台切换、监视失败可见性）未执行，仍为 `ready-for-human`。
- 高亮校验会在首次取某版内容时多读一次该文件 diff（单文件、字面路径），未在超大文件上量化该额外开销；本轮未做长时内存观测。
- `WatchFilter` 依赖路径规范化；若某平台事件路径无法规范化（例如已被删除且父目录也消失），会保守地按不相关处理，可能漏一次刷新（下次手动/前台核对仍可恢复）。
- 编辑器「原子替换」在测试中已覆盖；真实桌面高频写入的事件风暴未压测。
