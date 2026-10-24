# 07：持久审阅标记与连续审阅导航

**What to build:** 开发者用勾选框或 ⌥⌘R 标记看过的文件版本，重启后保留有效标记，文件变化后自动失效，并能去重地继续审阅。

**Blocked by:** 04, 06

**Status:** ready-for-human

依据：[M2 规格](../spec.md)实现决策 5、7，T10、T12、D04；Q20、Q37、Q48、Q87、Q94。

- [x] 使用已建立的 worktree 持久归属边界；标记记录文件的 HEAD 与工作区版本，任一变化失效，只暂存／取消暂存且版本未变时保留。
- [x] 同文件跨组共享标记与去重进度；重启后保留有效标记，身份不明确的新 worktree 不继承旧标记。
- [x] 文件行右侧勾选框只切换标记、不跳转；失效时恢复为空，不额外提示。
- [x] ⌥⌘R 只标记、不取消，不能进入 ⌘R 刷新分支；按当前列表顺序寻找下一个未审阅文件，去重、末尾环回、展开目标所在折叠组。
- [x] 全部已审阅时留在当前文件并提示；输入框、弹窗、按钮的键盘行为不穿透。
- [ ] M2 显示审阅进度，不提前实现提交或以标记阻止后续提交；真实 Git 持久化验证与桌面导航检查通过。
  - 真实 Git 持久化已由 `review_marks` 8 项通过；**桌面导航检查未验证**（本轮未启动 GUI），因此本项不勾选。

### 2026-10-24 / Pi 实现：任务07 持久审阅标记与连续审阅导航

只做 07；未做 08/12/13，未改 05 状态，未改已确认决策。

#### 1. 持久化与身份（Q20、Q87、T10）

- 新增 `src-tauri/src/review.rs`：`ReviewStore` 把标记存到 Revia 数据目录的 `review.json`（`{ version, worktrees: { <身份键>: { <路径>: ReviewMark } } }`），不写用户仓库、不改 Git 配置。
- 身份键 = `WorktreeIdentity` 的真实 worktree/gitDir/commonDir 路径 + 文件系统标识的 SHA-256（沿用任务 05 的身份，不按路径猜）。**同路径重建后标识不同 → 不继承旧标记**。
- 写入是「写临时文件 → rename → 同步目录」，失败如实返回错误。

#### 2. 版本与失效（Q20、T12）

- `ChangeEntry` 新增 `headVersion`（HEAD blob OID，新增/未跟踪为 null）与 `worktreeVersion`（工作区内容 SHA-256）。
- `refresh` 用 `ReviewStore::retain_valid` 只清理**确实失效**的标记：HEAD 或工作区版本任一变化即失效；**只暂存／取消暂存不改变这两个版本，标记保留**。
- 失效后勾选框恢复为空，不额外提示。

#### 3. 跨组共享与去重进度（T12）

- `ChangesSnapshot.review: ReviewMarks { paths, reviewed, total, reviewedCount }`；`paths` 与 `total` 按**文件去重**（同一文件在已暂存/未暂存两组只算一次），`reviewed` 也按路径去重。
- `ChangeEntry.reviewed` 供两组共享同一标记。

#### 4. 勾选框与 ⌥⌘R（Q37、Q94、D04）

- `ChangesList` 每行右侧新增 `role="checkbox"` 勾选框：只切换标记（`stopPropagation`，不改变当前选择、不触发暂存）；进度显示在列表头部。
- `src/lib/platform.ts` 新增 `isReviewShortcut`（`event.code === "KeyR"` + 主修饰键 + Alt），并在 `App` 的快捷键处理里**放在 ⌘R 刷新分支之前**，因此不会被刷新吞掉。
- `reviewNextFile`：先标记当前文件（若未审阅），再按「未暂存 → 未跟踪 → 已暂存」去重顺序从当前文件之后找第一个未审阅文件，**末尾环回**；目标折叠时自动展开；**全部已审阅时留在当前文件**并显示「全部文件都已审阅」。
- ⌥⌘R **只标记、不取消**；输入框/弹窗内的按键仍由 `isTextInputOrDialog` 放行，不穿透。

#### 5. 迟到响应隔离与失败可见

- `reviewEpochRef` 记录每次标记请求的代际；响应回来时校验代际、当前 worktree 与激活目标，**迟到响应不会污染其他 worktree 或新文件版本**。
- 持久化失败经 `setAppError` 如实展示（`messages.review.saveFailed` 语义），不会把界面状态改成已审阅。

#### 6. 生产调用点

`App.setReviewMark` / `App.reviewNextFile` → `ipc.setReviewMark` → `commands::set_review_mark` → `AppState::session`（带 `ReviewStore` 的 `WorktreeSession::open_with_review`）→ `WorktreeSession::set_review_mark` → `ReviewStore::set_mark`。
`ChangesSnapshot` 的审阅字段来自 `WorktreeSession::refresh` → `review_state` → `ReviewStore::retain_valid`。

#### 7. 实际执行的检查（改完后集中一次）

日志目录（实际路径）：`/private/tmp/revia-task07-jOyWKm`

- 基线：`234f248175ada05d44dbf639655e07a2957a9dac`
- 检查前源码 SHA-256（检查后复核未变）：
  - `b6ab27a41ae13c636a46c1b923ffb3ff8ea6b0ad7f39fd58524358e91f1d3ae2  src-tauri/src/review.rs`
  - `0fcd90eceb92c1bf92b2768219a73afa0d18e9df317f97c2cf73c64ac4953b48  src-tauri/src/session.rs`
  - `a509182ba8bb345b7ef0da4cdb85f786b8fc2441253d5433f7854c66d624a7ee  src-tauri/src/commands.rs`
  - `3a64cf21f3639df06ab6394eb2096dae6479589f74a2f32a2547d35d87c657e2  src-tauri/src/lib.rs`
  - `d06dd25d48bb311f78451687d8918f583fc40988f51521443dbd3e6f162a5b76  src-tauri/tests/review_marks.rs`
  - `c112a766026674e20150b6dac8227d068c6f40240b5b6fefcfc1ad3cd7c6b871  src/App.tsx`
  - `3fa9c41d157ad871b080adaa8d09aa51e8060e07d36b65342b4656f19b39adc3  src/components/app/ChangesList.tsx`
  - `bd0ac73319be1ea296c82713c9d80889b813933609edf6f75c076abda06b703c  src/lib/platform.ts`
  - `eb750feb672d383e0ef2519413b4a8c3c719676822c620f07192e886702e1ee7  src/lib/ipc.ts`
  - `63752d6112c124f7e2536253356af873e373bac94f70d31770339f6ff3b8f3c6  src/lib/i18n/zh-CN.ts`

| 命令 | 退出码 | 原始输出 |
| --- | --- | --- |
| `pnpm exec tsc -b` | 0 | `tsc.out` / `tsc.err` / `tsc.exit` |
| `pnpm lint` | 0 | `lint.out` / `lint.err` / `lint.exit` |
| `pnpm build` | 0 | `build.out` / `build.err` / `build.exit` |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 0 | `cargo-test.out` / `cargo-test.err` / `cargo-test.exit` |

Rust 合计 **239 项**：lib 99、`discard_recovery` 70（1 ignored 桌面夹具）、`file_staging` 20、`hunk_staging` 24、`refresh_isolation` 15、`review_marks` **8**、`session_flow` 3。

新增 `src-tauri/tests/review_marks.rs` 8 项全部走生产入口（`WorktreeSession::open_with_review` + `set_review_mark` + `refresh`），真实 Git 临时仓库 + 独立数据目录：
1. `mark_survives_stage_and_unstage` — 暂存/取消暂存后标记保留；
2. `worktree_change_invalidates_mark` — 工作区版本变化使标记失效；
3. `head_change_invalidates_mark` — HEAD 版本变化使标记失效；
4. `marks_survive_restart` — 重开会话后有效标记仍在；
5. `same_file_in_both_groups_shares_one_mark_and_counts_once` — 跨组共享、进度去重；
6. `recreated_worktree_at_same_path_does_not_inherit_marks` — 同路径重建不继承；
7. `unmark_only_affects_target_and_unknown_paths_are_rejected` — 取消只影响目标，未知路径拒绝；
8. `persistence_failure_is_reported_not_faked` — 数据目录只读时如实报错、界面不伪称已审阅。

#### 失败历史（保留）

首次完整 `cargo test` 时 `refresh_isolation::t13_refresh_does_not_spawn_per_file_git_processes` 失败（「实际新增 5 次」）：我最初在每次 `refresh` 里调用 `WorktreeIdentity::capture`，每次多起 2 次 `git rev-parse`。改为在 `open_with_review` 时捕获一次身份并缓存后通过；既有性能断言未放宽。

#### 未完成验收（留给规划方）

- **桌面未验证**：勾选框位置与观感、⌥⌘R 的实际按键行为（含 Option 修饰键下的键码）、折叠组自动展开、全部已审阅提示、进度显示、迟到响应隔离的真实时序。
- 本轮未构建 Release、未启动 GUI、未跑 D06/CI/长测与自动性能测试。
- 未实现提交功能（M3 范围），标记不阻止后续提交。
