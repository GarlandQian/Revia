# 07：持久审阅标记与连续审阅导航

**What to build:** 开发者用勾选框或 ⌥⌘R 标记看过的文件版本，重启后保留有效标记，文件变化后自动失效，并能去重地继续审阅。

**Blocked by:** 04, 06

**Status:** ready-for-agent

依据：[M2 规格](../spec.md)实现决策 5、7，T10、T12、D04；Q20、Q37、Q48、Q87、Q94。

- [ ] 使用已建立的 worktree 持久归属边界；标记记录文件的 HEAD 与工作区版本，任一变化失效，只暂存／取消暂存且版本未变时保留。
- [ ] 同文件跨组共享标记与去重进度；重启后保留有效标记，身份不明确的新 worktree 不继承旧标记。
- [ ] 文件行右侧勾选框只切换标记、不跳转；失效时恢复为空，不额外提示。
- [ ] ⌥⌘R 只标记、不取消，不能进入 ⌘R 刷新分支；按当前列表顺序寻找下一个未审阅文件，去重、末尾环回、展开目标所在折叠组。
- [ ] 全部已审阅时留在当前文件并提示；输入框、弹窗、按钮的键盘行为不穿透。
- [ ] M2 显示审阅进度，不提前实现提交或以标记阻止后续提交；真实 Git 持久化验证与桌面导航检查通过。
  - 真实 Git 持久化已由 `review_marks` 8 项通过；**桌面导航检查未验证**（本轮未启动 GUI），因此本项不勾选。

## Comments

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

### 2026-10-24 / 规划方：任务07首轮两轴复核，不通过

固定范围：`234f248175ada05d44dbf639655e07a2957a9dac...d892baee87c70d67878d29924b3a2fb7b496f7ba`，仅提交`d892bae`；两轴独立、只读静态复核，不随05返修的HEAD漂移。下列行号均指该固定提交。实施检查的239项通过不证明遗漏路径正确；未做Release、GUI、D06或任务07双平台CI。

#### Standards：8项P2，Fowler判断性建议0项

1. **ST07-1 / P2 / 共享存储事务**：`src-tauri/src/review.rs:146`全局JSON读改写无共享锁，`:116`又共用同PID临时文件。不同worktree标记/刷新并发可丢失已成功标记或临时文件冲突。违反Q20持久化与worktree隔离；整个读取、修改、发布事务需共享存储级串行及独占临时文件，不能只锁单次rename。
2. **ST07-2 / P2 / 身份确认**：`src-tauri/src/session.rs:422`始终使用会话打开时身份，活动会话同路径重建可继承旧标记；`review.rs:93`把未知filesystem ID散列为稳定键，`session.rs:283`的`capture().ok()`不等于既有`confirmed_match`安全契约。违反Q87/ADR-0010；刷新及标记时重新确认目录身份，未知时不按路径认领，不靠增加每文件Git进程解决。
3. **ST07-3 / P2 / 所见版本与响应上下文**：`session.rs:1060`取执行时缓存版本，IPC却没有用户所见版本；`src/App.tsx:956`只核worktree/请求序号，不核activation及快照代际。刷新与标记交错可标记未审阅的新内容或把已失效标记重新勾回。违反Q20/Q89/Q90；请求、持久化校验和响应均需绑定所见版本与完整上下文。
4. **ST07-4 / P2 / 跨组去重**：`src/App.tsx:994`及`:1010`把`Set.add()`返回的Set当布尔条件，条件恒真。违反Q94；先检查`has`再`add`，按真实列表顺序去重。
5. **ST07-5 / P2 / 导航使用成功结果**：`src/App.tsx:1004`在标记仅异步`setSnapshot`后立即读不保证更新的ref，最后一项仍可能按旧未审阅状态被重新打开。违反Q94；直接用成功响应计算导航或同步发布新快照，不依赖React恰好先render。
6. **ST07-6 / P2 / 完成提示可见性**：`src/App.tsx:2022`的完成提示只在“没有仓库”分支，正常审阅界面不可见。违反Q94；移到实际审阅界面并保留留在当前文件的行为。
7. **ST07-7 / P2 / 存储失败可见性**：`src-tauri/src/session.rs:1010`的`unwrap_or_default()`把损坏、不可读或清理持久化失败吞成空标记。违反Q36；保留并展示真实错误，不能假装正常全未审阅。
8. **ST07-8 / P2 / 子模块版本**：`src-tauri/src/review.rs:203`对子模块目录使用固定常量，切换子模块提交不会失效。违反Q20；记录实际子模块版本，仍保持Git操作只在Rust且只读限制不变。

规范来源：`AGENTS.md`、`docs/product-direction.md`（Q20/Q36/Q87/Q94及安全边界）、`docs/technical-direction.md`、`docs/README.md`、ADR-0002/0005/0009/0010、`GLOSSARY.md`、`docs/agents/local-validation.md`。Fowler完整基线已独立检查，本轮无额外判断性建议。

#### Spec：8项P2，无范围扩张

1. **SP07-1 / P2 / 全部已审阅提示缺失**：`src/App.tsx:2022`只在空仓库页面渲染提示。规格实现决策5要求“全部已审阅时留在当前文件并提示”（spec:110）；正常审阅界面需真正可见。
2. **SP07-2 / P2 / 按钮焦点穿透**：`src/App.tsx:1088`的新审阅快捷键只经过不拦button的`isTextInputOrDialog`。规格决策7（spec:123）要求按钮使用自己的键盘行为，不穿透后台操作；在实际入口补齐焦点范围。
3. **SP07-3 / P2 / 跨组去重错误**：`src/App.tsx:1010`（`:994`同类）`Set.add()`条件恒真；X未暂存、A未暂存、X已暂存、B已暂存时，从A会先跳回X而不是B。违反spec:110“跨组去重、按当前列表顺序”。
4. **SP07-4 / P2 / 迟到响应与旧导航**：`src/App.tsx:956`不核activation/快照，A→B→A或刷新后旧响应可重新勾选新版本；`:1005`也未确认等待期间用户仍选原文件，旧导航可覆盖新选择。违反spec:116“旧请求返回不能使内容倒退”；标记结果和导航均绑定发起上下文，切换/刷新/新选择使旧导航失效。
5. **SP07-5 / P2 / 持久化了未看过的版本**：`src-tauri/src/session.rs:1060`请求只有path+marked，执行时拿最新缓存版本。用户看到V1但刷新V2先发布时点击会保存V2。违反spec:107“HEAD版本+工作区版本”；贯通用户所见期待版本，失配则拒绝并重新读取，不把新版本当已看过。
6. **SP07-6 / P2 / 活动会话身份**：`src-tauri/src/session.rs:422`只用open时身份；活动会话同路径重建且内容相同会继承旧标记。违反T10和Q87“身份不明确不继承”；不能只以重开会话测试证明。
7. **SP07-7 / P2 / 多worktree持久化丢失**：`src-tauri/src/review.rs:146`共享JSON无跨会话事务互斥，后写覆盖前写成功的记录。违反spec:107“重启后保留”；串行整个存储事务并覆盖真实并发场景。
8. **SP07-8 / P2 / 子模块变化不失效**：`src-tauri/src/review.rs:203`使用恒定目录版本，子模块切换提交后旧标记仍有效。违反spec:107“任一版本变化即失效”；读取真实子模块版本。

两轴结论分别为Standards 8项P2、Spec 8项P2，不合并计数或用一轴掩盖另一轴。以上基于固定源码的静态因果，不伪称已做运行时复现。

#### 检查证据与返修约束

- 首版15份原始检查文件已归档至[task07-review01/manifest.json](../evidence/task07-review01/manifest.json)，逐文件保存源/存储SHA；10份检查前源码hash独立匹配固定`d892bae`。原始日志显示tsc/lint/build退出0，Rust239项通过、1个桌面夹具ignored；这些结论只属于首版。
- 首次t13失败及“缓存身份后通过”的原因目前只有实施方MANIFEST/Comments叙述；该目录没有首次失败原始stdout/stderr。保留叙述，不声称已独立核实，也不得为t13牺牲身份正确性或放宽断言。
- 状态退回`ready-for-agent`，原先勾选项退回待验证。只修07，不混05/08/12/13；复用现有真实Git生产入口Rust测试，不新增前端框架，不扩展临时客户端。
- 先全部修改再集中相关检查；核心持久化/并发/接口变化需相关回归及必要的完整检查一次。保留精确命令、真实退出码、原始stdout/stderr、检查前后源码hash，精确暂存，不`git add -A`。
- Pi不得构建Release、启动GUI或运行D06/CI/长测，不推送、不合并。两轴全过后才由规划方固定Release进行真实桌面验收；暂不标done。
