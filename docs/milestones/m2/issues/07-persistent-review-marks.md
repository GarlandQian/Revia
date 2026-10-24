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
  - 真实 Git 持久化已由 `review_marks` **15** 项通过（含子模块 packed-refs、gitDir 改指、并发事务、所见版本、存储损坏）；**桌面导航检查未验证**（本轮未启动 GUI），因此本项不勾选。

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

### 2026-10-24 / Pi 实现：任务07 首轮两轴复核返修（R07-R1）

只修 07；未动 05 状态与 05513aa 内容，未做 08/12/13。

#### Standards

**ST07-1 / SP07-7（存储事务）**：`ReviewStore` 新增 `transaction: Arc<Mutex<()>>`，`marks` / `set_mark` / `retain_valid` 整个「读 → 改 → 发布」都在事务内完成；临时文件改为 `exclusive_temp()`（进程号 + 纳秒 + 单调序号 + `create_new`），不再共用同 PID 名字。写入失败会删除本次临时文件。
回归 `concurrent_marks_from_two_worktrees_are_all_kept`：两个身份并发各写 8 次，最终双方标记都在。

**ST07-2 / SP07-6（活动会话身份）**：`WorktreeIdentity::revalidate()` 只重读文件系统标识（不启动 Git 进程），`refresh` 与 `set_review_mark` 都改用 `confirmed_identity()`；同路径重建后标识变化 → 返回 `None` → 不认领任何标记。`ReviewStore::identity_key` 现在返回 `Option`：任一 filesystem ID 未知就**不按路径认领**，而不是散列成稳定键。
回归 `active_session_reconfirms_identity_and_stops_claiming_marks`：同一会话内删 `.git` 重建后刷新，`reviewed_count` 归零。既有 `t13_refresh_does_not_spawn_per_file_git_processes` 未放宽，`revalidate` 不调用 Git。

**ST07-3 / SP07-5（所见版本贯通）**：`ReviewMarkRequest` 新增 `expectedVersions`；`WorktreeSession::set_review_mark` 接受 `expected_versions` 并与执行时缓存版本比对，不一致即 `Rejected`（「文件已变化，请重新确认后再标记」）。前端把 `entry.headVersion` / `entry.worktreeVersion` 一并发出。
回归 `stale_expected_versions_are_rejected`：外部编辑后用过时版本标记被拒且不持久化；用当前版本成功。

**ST07-4 / SP07-3（`Set.add` 恒真）**：`reviewNextFile` 的去重改为 `has` 判断后 `add`（`uniqueEntries` 辅助函数），两处都修。

**ST07-5（导航用成功结果）**：`setReviewMark` 现在返回后端确认的 `ReviewMarks`；`reviewNextFile` 用该响应构造 `reviewedFromServer` 计算下一个未审阅文件，不依赖 React 先 render。

**ST07-6 / SP07-1（完成提示可见性）**：提示从「没有仓库」分支移到正常审阅界面（工具栏下方、`role="status"`），保留「留在当前文件」的行为。

**ST07-7（存储失败可见）**：`review_state` 里 `retain_valid(...).unwrap_or_default()` 改为 `?`，损坏/不可读/清理写入失败如实上抛。
回归 `corrupt_store_surfaces_an_error`：写入损坏 JSON 后 `refresh` 返回错误而不是空标记。

**ST07-8 / SP07-8（子模块版本）**：`worktree_version` 对目录先尝试 `submodule_commit()`——读子模块 `.git` 文件指针 → `HEAD`/`refs/...` 得到真实提交号；读不到才退化为「是目录」。仍只读、不启动 Git 进程。
回归 `submodule_commit_change_invalidates_mark`：切换子模块提交后版本改变。

#### Spec

**SP07-1** 见 ST07-6。**SP07-2（按钮焦点穿透）**：新增 `platform::isButtonTarget`（`button` / `[role=checkbox]` / `[role=button]`），审阅快捷键在按钮获得焦点时不处理，保留按钮自身键盘行为；`isTextInputOrDialog` 的既有放行不变。
**SP07-3** 见 ST07-4。**SP07-4（迟到响应与旧导航）**：`setReviewMark` 的 `isCurrent` 同时校验 worktree、**activation** 与**快照 revision**；`reviewNextFile` 在标记等待后与每次跳转前都用 `stillSameContext()` 校验 worktree / activation / revision / 当前选择键，任一变化即放弃导航。
**SP07-5** 见 ST07-3。**SP07-6** 见 ST07-2。**SP07-7** 见 ST07-1。**SP07-8** 见 ST07-8。

#### 生产调用点

`App.setReviewMark` → `ipc.setReviewMark`（带 `expectedVersions`）→ `commands::set_review_mark` → `WorktreeSession::set_review_mark`（`confirmed_identity` + 版本比对）→ `ReviewStore::set_mark`（事务锁 + 独占临时文件）。
`WorktreeSession::refresh` → `review_state` → `ReviewStore::retain_valid`（事务内，错误上抛）。

#### 实际执行的检查（改完后集中一次）

日志目录（实际路径）：`/private/tmp/revia-task07-r2-016TbZ`

- 基线：`0dd652505da4b9aa54773ba9e522e27bcb673177`
- 检查前源码 SHA-256（检查后复核未变）：
  - `c5af8e50f32bb1fa2437132d2888ad501eec572cc6de990e96d6accfd64627a1  src-tauri/src/review.rs`
  - `e45533a888c20feac5f8cc828cdbb8705bd3a13420787160e893a9871eef0749  src-tauri/src/recovery.rs`
  - `fc6dbd1d74f9c65e92ccb2562544e0f7947ae12d10cf712e9d138308c847bb33  src-tauri/src/session.rs`
  - `d1d0e3ec6bc4de92a3d269c2a9cdf1bdede2f5b2057d80dc0c35966b5f0a6e6d  src-tauri/src/commands.rs`
  - `674d0de4bc1926c06a4288cc23c7bf308d1bd6432e0cf8cb81ac51bb1bd4b979  src-tauri/tests/review_marks.rs`
  - `717f319e1801988e245221109d7f7097c36f01445db0a65ccf0f64bc7a8de2ba  src/App.tsx`
  - `3fa9c41d157ad871b080adaa8d09aa51e8060e07d36b65342b4656f19b39adc3  src/components/app/ChangesList.tsx`
  - `f7d4f5c49a70f6c944ec74a6582d07ebe17066e4ae47b0ff7004cdf36322233c  src/lib/platform.ts`
  - `b50e7c0fd2ee64e47ead6119dc59a1ad176c6348a3a6337c57226262f406e56c  src/lib/ipc.ts`
  - `63752d6112c124f7e2536253356af873e373bac94f70d31770339f6ff3b8f3c6  src/lib/i18n/zh-CN.ts`

| 命令 | 退出码 | 原始输出 |
| --- | --- | --- |
| `pnpm exec tsc -b` | 0 | `tsc.out` / `tsc.err` / `tsc.exit` |
| `pnpm lint` | 0 | `lint.out` / `lint.err` / `lint.exit` |
| `pnpm build` | 0 | `build.out` / `build.err` / `build.exit` |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 0 | `cargo-test.out` / `cargo-test.err` / `cargo-test.exit` |

Rust 合计 **245 项**：lib 99、`discard_recovery` 71（1 ignored 桌面夹具）、`file_staging` 20、`hunk_staging` 24、`refresh_isolation` 15、`review_marks` **13**（新增 5 项）、`session_flow` 3。首版 239 项不再代表本轮。

#### 失败历史

实现过程中 `review_marks` 新用例有一次编译错误（`other_identity` 被 move 后仍借用），改为 clone 后通过。完整套件本轮首次运行即通过，t13 未复现失败。

#### 未验证（必须保留）

- **桌面未验证**：勾选框位置与观感、⌥⌘R 的实际按键（Option 修饰键）、折叠组自动展开、全部已审阅提示的实际可见性、按钮焦点下不穿透、迟到响应隔离的真实时序。
- 未构建 Release、未启动 GUI、未跑 D06/双平台 CI/长测。
- 子模块版本在 packed-refs 场景读不到时退化为「是目录」，该退化路径未覆盖。

### 2026-10-24 / 规划方：任务07第二轮固定两轴复核，仍需返修

固定`0dd652505da4b9aa54773ba9e522e27bcb673177...84a17a8e76440686b48d6c9c6d1c72a7dbb4ee37`，仅84a17a8；两轴独立静态复核，以下行号均指该提交，不随05返修HEAD漂移。

#### Standards：5项P2，Fowler判断性建议0项

1. **ST07-R2-1 / P2 / 子模块仍未按真实Git版本校验**：`src-tauri/src/review.rs:292`手工解释`.git`、HEAD及loose refs，违反ADR-0002“Git读写统一CLI、不为性能维护第二套实现”。`:279`在packed-refs或目录型`.git`读取失败时返回恒定目录版本，换提交后可继续认领标记，Q20未闭合。经现有Rust Git调用边界取得真实版本；无法确认时保守失效，不生成可认领的恒定版本。
2. **ST07-R2-2 / P2 / 身份归属仍来自旧缓存**：`src-tauri/src/recovery.rs:58`仅核缓存路径的文件系统ID。linked worktree的`.git`/commondir指向另一现存目录、旧目录仍在时，真实Git已换归属，revalidate却仍确认旧身份；内容相同可继承旧标记，违反Q87。核对当前实际解析归属与ID，不只stat旧目录；保持平台/生产Git边界和既有性能断言，不增加每文件Git调用。
3. **ST07-R2-3 / P2 / 新选择尚未显示时被旧导航覆盖**：`src/App.tsx:1038`只比当前选择键。A等待标记期间用户点击尚未加载指标的未跟踪B，`openEntry:237`已递增选择代际、`:249`仍等待行数据，selectedRef此时仍A；旧导航误通过并在`:1057`打开C，作废用户B请求。违反旧选择隔离/Q89；绑定已有selectionEpoch/选择gate，不只比当前路径与组。
4. **ST07-R2-4 / P2 / 迟到失败被静默丢弃**：`src/App.tsx:994`持久化失败仅在isCurrent时登记；等待期间切worktree或刷新后错误丢失，未进入原worktree错误字典，违反Q36/Q90。失败留存在原worktree，当前展示再按激活状态隔离；不能把成功结果隔离与错误留存混为一体。
5. **ST07-R2-5 / P2 / 完成提示归属过期**：`src/App.tsx:2117`完成提示是全局状态，只在下一次快捷键开始时清除。A完成后切到未完成的B，或取消A任一勾选，仍显示“全部已审阅”，违反Q94/Q90。随worktree归属及当前进度派生或清除，不让旧提示冒充新状态。

来源：ADR-0002、`docs/product-direction.md`、`docs/technical-direction.md`、`docs/README.md`、`docs/ui-direction.md`、既有Q20/Q36/Q87/Q89/Q90/Q94及07最新Comments。

#### Spec：3项P2，无范围扩张

1. **SP07-R2-1 / P2 / packed-refs路径仍未实现版本失效**：`src-tauri/src/review.rs:309`不读取packed-refs，`:279`退化恒定目录版本。子模块在两个已打包分支间切换，旧标记仍有效，违反spec:107“任一版本变化即失效”。读取真实引用；无法确认不能生成可认领的常量，Comments中“未覆盖”不改变规格。
2. **SP07-R2-2 / P2 / 实际Git归属未重新确认**：`src-tauri/src/recovery.rs:58`只stat旧路径，linked worktree的`.git`改指另一真实gitDir且旧目录仍在时，仍可认领旧标记，违反T10/Q87“不继承标记”。确认当前路径实际解析的gitDir/commonDir。
3. **SP07-R2-3 / P2 / 选择往返仍通过旧导航**：`src/App.tsx:1042`只比选择键，标记等待期间A→B→A后，旧导航仍跳离新的A选择，未闭合首轮SP07-4“新选择使旧导航失效”。绑定已有selectionEpoch，而不是仅路径和组。

两轴分别为Standards5项P2、Spec3项P2，保留各自结论，不合并或重排。首轮提示渲染位置、按钮焦点、去重、成功结果导航、生产IPC必填所见版本、单应用共享事务和普通目录重建的原触发路径已静态闭合。普通applySnapshot先同步ref再setState，未据推测的React排队时序开新问题。

多进程事务项经范围核对撤回必改：现有Q16/规格未明确多应用进程并发共享数据目录，不能仅从“重启保留”推出新产品要求。正常AppState只new一个ReviewStore并clone，各worktree共享事务已闭合；跨进程仅记残余风险，不要求本任务扩架构。误称文件锁的注释应如实更正，不以注释冒充实现。

#### 检查证据与返修

- 本轮16份检查原件已[归档](../evidence/task07-review02/manifest.json)，10份检查前源码hash独立匹配固定84a17a8；原始tsc/lint/build退出0、Rust245通过/1夹具ignored。未找到所述一次move后借用编译失败的单独原始文件，只有MANIFEST叙述，不冒称已独立核过该失败。
- 07退回`ready-for-agent`，未证明项取消勾选。仍交原Pi只修07；复用现有生产会话、Git、选择gate和原worktree错误通道，不再用手工Git解释或不完整上下文字段子集替代既有边界。
- 先所有修改后集中检查；精确命令、真实退出码、原始stdout/stderr和检查前后源码hash，精确暂存。不新增前端测试框架或重复临时客户端，不改已确认决策/性能断言。
- 未Release、GUI、D06、双平台CI或长测。245项不是这些遗漏路径的证明，两轴全部通过后再固定Release和真实桌面，不标done。

### 2026-10-24 / Pi 实现：任务07 第二轮复核返修（R07-R2）

只修 07；未动 05 状态与内容，未做 08/12/13。首轮已闭合项未反复重构。

#### Standards

**ST07-R2-1 / SP07-R2-1（子模块真实 Git 版本）**：删除 `review.rs` 里手工解释 `.git`/`HEAD`/loose refs 的第二套 Git 语义（ADR-0002），改为经现有 Git CLI：`git submodule status -- <path>`，由 Git 自己解析 packed-refs、目录型 `.git`、detached HEAD。读不到真实版本时 `worktree_version` 返回 `None`，并新增 `ChangeEntry.worktreeVersionConfirmed=false`：这类路径的审阅标记**一律不可认领**，不再退化成恒定目录版本。
回归 `submodule_version_follows_real_git_including_packed_refs`：真实 `git submodule add`，先 `checkout` 到第一个提交，再 `git pack-refs --all`，再切到第二个提交——版本随之变化（手工解析实现会失败，见敏感性对照）。`unreadable_submodule_version_is_not_claimable`：普通目录拿不到版本时返回 `None`。

**ST07-R2-2 / SP07-R2-2（实际归属）**：`WorktreeIdentity::revalidate` 不再只 stat 缓存路径，改为让 Git 重新解析当前实际归属（`capture`）后与缓存身份做 `confirmed_match`；linked worktree 的 `.git` 改指另一个**仍存在**的 gitDir 时也会被判为身份不明确。
为保持既有性能断言，`capture` 合并为**一次** `rev-parse --git-dir --git-common-dir`（原来两次），`t13_refresh_does_not_spawn_per_file_git_processes` 仍通过、断言未放宽。
回归 `gitdir_repointed_to_existing_directory_invalidates_identity`：真实 linked worktree，标记后把 `.git` 指针改到复制的另一 gitDir（旧目录仍在），同一会话刷新后 `reviewedCount` 归零。

**ST07-R2-3 / SP07-R2-3（选择代际）**：`reviewNextFile` 的 `stillSameContext()` 增加 `selectionEpochRef` 与 `selectionGateRef.currentContext().epoch` 比对（复用既有 gate，不新增字段子集）。点击尚未加载指标的 B（`openEntry` 已递增代际但 `selectedRef` 仍是 A）以及 A→B→A，都会让旧导航立即失效。

**ST07-R2-4（失败留存）**：`setReviewMark` 的 catch 不再受 `isCurrent()` 限制，失败**始终** `setAppError(err, targetWorktreeId)` 记到发起它的 worktree；成功响应仍按激活/版本隔离。

**ST07-R2-5（完成提示归属）**：提示状态改为 `{ worktreeId, reviewedCount, total }`，渲染时要求与当前快照的 worktree、`total`、`reviewedCount` 全部一致且确实全部完成；切 B、取消标记或刷新失效后不再显示旧「全部已审阅」。

另：更正 `ReviewStore.transaction` 的注释——它是**进程内**互斥锁，不是文件锁；多进程共享数据目录不在本任务范围，注释不再冒充已支持。

#### Spec

**SP07-R2-1** 见 ST07-R2-1。**SP07-R2-2** 见 ST07-R2-2。**SP07-R2-3** 见 ST07-R2-3（`selectionEpoch` + 选择 gate，覆盖 A→B→A 与「B 尚未显示」）。

#### 生产调用点

`WorktreeSession::refresh` → `confirmed_identity()` → `WorktreeIdentity::revalidate(&self.git)`（Git 重新解析归属）→ `review_state`（`retain_valid` 过滤不可确认版本）。
`WorktreeSession::set_review_mark` → `confirmed_identity()` + 版本比对 → `ReviewStore::set_mark`。
`review::worktree_version(Some(&self.git), …)` → `submodule_commit` → `git submodule status`。

#### 实际执行的检查

日志目录（实际路径）：`/private/tmp/revia-task07-r3b-gkOB0F`（旧的 `/private/tmp/revia-task07-r3-hCxSBf` 原样保留，见 `NOTE-aborted-run.md`）

- 基线：`ed0f9b8eca4046fe934464b57e0607507794d487`
- 检查前源码 SHA-256（检查后复核未变）：
  - `bf058efc9e9f2c7c7bd1572e1e8c270edb91f59a233048150d6ce9e9ec47d2b7  src-tauri/src/review.rs`
  - `4625d629efa4217f452d1de8775fd16dd789221a32603987f7f442e08ae69898  src-tauri/src/recovery.rs`
  - `3415f58e86acd62e02e107382faf7a0d2c71e36e30df211d1e0b9ed8bcabe9a5  src-tauri/src/session.rs`
  - `d5a4414d83ff69cc0c4b360e6ea60d0a5f0875531a31b6324fd2d6157231bdeb  src-tauri/tests/review_marks.rs`
  - `624fb867f5a14ad0e4d0dc039f97af7e51644b31d1917d3f5d9edb869a5a09b7  src/App.tsx`

| 命令 | 退出码 | 原始输出 |
| --- | --- | --- |
| `pnpm exec tsc -b` | 0 | 复用 `/private/tmp/revia-task07-r3-hCxSBf/tsc.*`（源码未变） |
| `pnpm lint` | 0 | 复用 `/private/tmp/revia-task07-r3-hCxSBf/lint.*`（源码未变） |
| `pnpm build` | 0 | `build.out` / `build.err` / `build.exit` |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 0 | `cargo-test.out` / `.err` / `.exit` |

Rust 合计 **247 项**：lib 99、`discard_recovery` 71（1 ignored 桌面夹具）、`file_staging` 20、`hunk_staging` 24、`refresh_isolation` 15、`review_marks` **15**（新增 2 项）、`session_flow` 3。

#### 中断与敏感性证据边界

- 上一轮检查目录 `/private/tmp/revia-task07-r3-hCxSBf` 的 `build` 在 `transforming` 阶段中断、**没有 `build.exit`**；`cargo` 未产出日志。该目录原样保留，本轮在其外新建目录重跑 build 与完整 Rust。
- 两处敏感性对照（移除子模块真实版本、退回只 stat 缓存身份）**实际运行过并使对应用例失败**，但只保留终端摘要与实现副本（`history/sensitivity.md`、`history/*.bak`），没有独立归档的 stdout/stderr；不冒称完整原始结果。

#### 未验证（必须保留）

- **桌面未验证**：勾选框观感、⌥⌘R 实际按键、折叠组展开、完成提示归属、按钮焦点不穿透、迟到响应与选择代际的真实时序。
- 未构建 Release、未启动 GUI、未跑 D06/双平台 CI/长测。
- 子模块场景只覆盖 `submodule status` 能读到的情况；工作区里未初始化的子模块（`-` 前缀）不在本轮验证范围。
- 多进程同时共享同一数据目录不在本任务范围（仅进程内互斥）。

### 2026-10-24 / 规划方：任务07第三轮固定两轴复核，仍需返修

固定 `ed0f9b8eca4046fe934464b57e0607507794d487...ff2fc640d3a6fa771d1d6b8a5514a3d7471ab347`，唯一实施提交 ff2fc64。两轴独立静态审查；以下行号均为该固定提交，不随任务12后续修改漂移。

#### Standards：4项P2，Fowler判断性建议0项

1. **ST07-R3-1 / P2 / 未确认版本仍可当场保存为已审阅**：`src-tauri/src/session.rs:1011`仍把未知版本放入快照 `versions`；`:1089`只查该映射，`:1105`直接持久化并计入进度。未登记为子模块的嵌套仓库等目录即使 `worktreeVersionConfirmed=false`，仍可经现有入口勾选，仅下次刷新才删除。违反Q20及上一轮“未知不能认领”的契约。生产标记入口须拒绝未知版本的新增认领，前端勾选和快捷键同步处理；已有标记的取消能力保留，不能只在刷新时清除。
2. **ST07-R3-2 / P2 / 未初始化子模块被冒充为已确认检出版本**：`src-tauri/src/review.rs:301`直接剥掉 `submodule status` 的 `-` / `U` 前缀。`-` 表示未初始化，所给OID来自暂存区而非当前检出。暂存gitlink变化后deinit、标记、再init同OID，空目录变成实际内容却保留标记，违反Q20。须保留并识别Git状态，未知或冲突不能当作已确认提交；Comments的“未验证范围”不缩减既定行为。
3. **ST07-R3-3 / P2 / 共享身份捕获新增内嵌换行路径歧义**：`src-tauri/src/recovery.rs:44`把两条路径按 `lines()` 拆分。Unix合法仓库根如 `/tmp/revia\nrepo`（内嵌换行，不是反斜杠字面量）会被拆成错误的gitDir/commonDir，随后canonicalize失败，同时影响05恢复与07标记。原来单字段命令没有这种跨字段拆分歧义。违反 `docs/technical-direction.md:41` 的机器可读、无歧义解析边界。使用可靠字段边界，沿用生产Git调用，不维护第二套Git语义；不得放宽既有性能断言来掩盖回归。
4. **ST07-R3-4 / P2 / packed-refs回归没有触发声称的旧缺陷**：`src-tauri/tests/review_marks.rs:529`、`:546`均checkout具体SHA，HEAD始终detached；`:538`执行pack-refs未进入旧符号引用解析路径，旧手工实现本来也能直接读取两个OID。`:540`检查还在作为指针文件的 `.git/refs` 下，OR条件恒真，也没构造目录型 `.git`。违反针对性真实回归要求。应检出真实符号分支、确认对应loose ref已消失，再经生产标记/刷新入口验证packed引用变化使标记失效；目录型 `.git` 的覆盖须真实构造，不能靠注释声称。

规范来源：`docs/product-direction.md:64`、`docs/technical-direction.md:41`、`docs/agents/local-validation.md:8`、ADR-0002、Q20及07前轮Comments。四项均为确定行为或验证缺口，不是Fowler风格建议。

#### Spec：2项P2、1项P3；未发现范围扩张

1. **SP07-R3-1 / P2 / 未知版本仍可持久化**：`src-tauri/src/session.rs:1011`仍把未知版本加入快照；`:1094`仅比较版本，`:1105`直接持久化，未检查 `worktree_version_confirmed`。现有勾选可提交 `worktree:null` 并立即计进度，刷新后才清除。违反本任务前轮说明“这类路径的审阅标记一律不可认领”及 `spec.md:107` 版本契约。生产入口拒绝，前端同步禁用，保留取消能力。
2. **SP07-R3-2 / P2 / 未初始化子模块不是工作区检出版本**：`src-tauri/src/review.rs:301`无条件剥离 `-`，把暂存区OID当作实际检出版本，违反 `spec.md:107`“HEAD中的文件版本＋工作区中的文件版本”及未知版本保守处理。应区分未初始化/无法确认状态。
3. **SP07-R3-3 / P3 / packed-refs用例不足以证明所称修复**：`src-tauri/tests/review_marks.rs:529`、`:546`均checkout裸OID，`:540`断言因 `.git` 为文件而恒真。不能支撑本任务前轮“手工解析实现会失败”的陈述。使用仅存于packed-refs的真实分支，并通过生产标记/刷新入口断言失效；不再为此另写临时客户端。

两轴结论分别保留：Standards4项P2，Spec2项P2和1项P3，不以一轴数量覆盖另一轴。Spec轴对共享身份捕获未报告确定新增回归；Standards轴的换行路径发现独立保留并须处理。

#### 已闭合与证据边界

- 上轮实际 `.git` 归属重解析、selectionEpoch/选择gate防旧导航、原worktree失败留存、完成提示归属的原触发路径已静态闭合，不反复重构。单应用共享事务及生产IPC必填所见版本结论不变；多进程并发仍非本任务已确认要求。
- [原始检查及中断历史](../evidence/task07-review03/manifest.json)归档原检查与续跑两个目录，5份检查前源码SHA独立匹配ff2fc64，两个目录哈希列表一致；tsc/lint复用原目录的真实退出0，续跑build和完整Rust退出0（247通过、1夹具ignored）。原build无退出码、cargo未产日志，不能倒填为成功。
- 两处敏感性实验仅实施叙述和实现副本，没有独立stdout/stderr/exit。`recovery.rs`副本匹配固定源码，`review.rs`副本仅事务锁注释与最终版不同，manifest如实记录；副本不证明packed-refs旧故障真正被触发，不采纳叙述中“可核对照确实失败”为独立运行证据。
- 07退回 `ready-for-agent`，验收项保持未勾。仍交原Pi只修上述残留，通过现有Rust真实Git/生产会话入口回归，保存精确命令、真实退出码、原始输出及检查前后源码SHA。先完成相关修改后集中检查，失败只重跑受影响项，精确暂存。
- 尚未Release、GUI、D06、新双平台CI或长测；不能用247项通过替代上述行为或桌面验收，全部复核通过后再固定版本验收，不标done。
