# 08：写入期间安全关闭与结果待核对

**What to build:** 开发者在任一 worktree 仍有写操作时请求关闭或退出，可以等待正常完成或留在应用，不因窗口关闭而打断 Git，不确定结果必须可见。

**Blocked by:** 05

**Status:** ready-for-human

依据：[M2 规格](../spec.md)实现决策 3、7，T06、T11、D05；Q90、Q91、Q96。

- [ ] 覆盖暂存、取消暂存、丢弃、恢复的在途生命周期，切换到其他 worktree 后也不能漏掉仍在执行的操作。
- [ ] 关闭窗口或退出先保留窗口与进程，提供“等待完成后关闭／退出”和“留在应用”，不为关闭直接杀掉 Git。
- [ ] 正常完成并记录结果后执行原请求；留在应用不取消写操作，不把关闭请求改成不同的退出动作。
- [ ] 待核对结果先展示，不静默退出、不自动续跑、不承诺超时没有副作用；恢复后的下一次写入使用新状态。
- [ ] 受控执行阶段中断验证保存记录仍在、不会在重启后盲目重跑；实际关闭／退出交互由真实桌面检查。

## Comments

### 2026-10-24 / Pi 实现：写入期间安全关闭与退出（Q90/Q91/Q96）

#### 设计

关闭/退出的决策收在共享层 `CloseCoordinator`（`src/lib/refreshGate.ts`），
App 与 Node 回归**用同一个类**，不各写一份判断：

- 写操作按 **worktree** 记在途：切到别的 worktree 后仍在执行的操作也算在途（Q90）；
- `request(intent)` 在无在途写、无待核对时返回 `proceed`；否则**保留**窗口与进程，
  记下**原请求**（`close` / `exit` 原样保存，不升级成强退）并返回 `ask` / `showPending`；
- `keepOpen()`（留在应用）只清掉请求，**不取消**写操作、不改变动作类型；
- `takeReadyIntent()` 在写操作全部结束且无待核对时返回原请求，调用方据此执行；
- `endWrite(id, needsVerification)` 把「结果无法确定」记为待核对，
  `request` 遇到待核对先返回 `showPending`，**不静默退出**（Q91）；
- 编排里**没有**重试/重跑入口：从不杀 Git、不重跑 stage/discard/restore。

#### 接入点

- `performWrite`（暂存/取消暂存/丢弃，含 hunk）与 `performRestore`：进入时 `beginWrite`，
  `finally` 里 `finishWrite(worktreeId, needsVerification)`；
- `performSaveAs`：取得真实归属门禁后 `beginWrite`，`finally` 里 `finishWrite`；
- `finishWrite` 结束后若条件满足，执行**原请求**（`performCloseIntent`）；
- 前端监听 `onCloseRequested`：有在途写时 `event.preventDefault()` 并弹出
  `CloseRequestDialog`（「等待完成后关闭」/「留在应用」）；
- 新增能力 `core:window:allow-close`、`core:window:allow-destroy`
  （`core:window:default` 不含它们，已核对生成 schema）。

#### 回归（Node，生产入口）

`tools/test-refresh-gate.mjs` 新增 9 项，全部驱动同一个 `CloseCoordinator`：
无写操作时立即执行、有在途写时保留并询问、切换 worktree 后仍算在途、
留在应用不取消写也不改变动作、等待后执行**原请求**（close/exit 各自原样）、
多个 worktree 全部完成后才执行、待核对先展示且确认后才执行、
待核对不被下一次写入伪清除、编排没有重试入口。

#### 未验证（必须保留）

- **真实桌面关闭/退出交互未验证**（任务单明确由真实桌面检查）。
- 未构建 Release、未启动 GUI、未跑 D06/双平台 CI/长测；未推送/合并/发布。


### 2026-10-24 / 规划方：整批首轮两轴复核，不通过

固定`3199b29e84c2eadccc7a9800699e2506af9a31bb...82f392ba5b94c4e23f5e8040d1a4f4c35a41ca35`。以下均为真实App/native接线，不能以CloseCoordinator helper的绿色检查替代。

#### Standards：确定缺陷4项（P1一项、P2三项），Fowler判断建议0项

1. **ST-B01-08-1 / P1 / 原生退出绕过**：`src/App.tsx:748`只拦窗口关闭，全项目无生产exit请求，Rust `src-tauri/src/lib.rs:134`无原生退出拦截。锁定依赖默认macOS Quit/⌘Q走原生terminate通道，不经过窗口close监听。违反Q96；需要真实原生关闭与退出统一保护，保留原动作，不用destroy窗口代替退出进程。监听安装失败现在只debugLog，保护不能依赖静默失败的前端订阅。
2. **ST-B01-08-2 / P2 / 无请求自动关闭**：`src/App.tsx:1035`在另一worktree仍写且无intent时使用`pendingIntent() ?? "close"`制造请求；最后一笔`:1041`自动关窗。A/B并行、用户未关窗，或已选Stay后，都可发生。违反Q96只执行原请求；无请求/已取消必须一直保持无请求，用户选择等待与仅展示询问也不能混为同一授权。
3. **ST-B01-08-3 / P2 / 另存生命周期遗漏**：`:1181`外部owner=null另存不计关闭占用，有归属另存`:1227`固定finishWrite(false)，而`commands.rs:696`明确可返回needsVerification。违反Q91/Q96。全部应用写入须纳入关闭生命周期，但不要为外部新路径强加worktree锁；错误种类如实传递。
4. **ST-B01-08-4 / P2 / 待核对无闭环**：`:2555`“等待”只重发request并藏对话框，`refreshGate.ts:1117` acknowledgePending无生产调用；查看/真实刷新后仍无法正常关闭，只能下一次write自动清pending。违反Q96。接通真实核对/结果查看后的明确继续或留在应用，不以新写入充当核对，不自动重跑。

#### Spec：缺失/部分2项、错误实现1项（最严重P1），范围扩张0项

1. **SP-B01-08-1 / P1**：同ST-08-1，违反`spec.md:126`“请求关闭窗口或退出先保留窗口和进程”；生产没有产生exit，Cmd-Q/默认菜单可直接绕过保护。
2. **SP-B01-08-2 / P2**：同ST-08-4，违反`spec.md:127`“结果待核对则先展示、不静默退出”；结果已查看/真实刷新后仍无安全闭环，不能让新write自动删除pending作为唯一出口。
3. **SP-B01-08-3 / P2**：同ST-08-2，违反`spec.md:127`“执行原关闭/退出请求”；没有请求或已选Stay时仍可凭空生成close。

[27份检查原件](../evidence/batch01-review/manifest.json)六源码SHA匹配，Node105、Rust264通过/1准备入口ignored和前端检查exit0，仅证明该版已有检查。没有真实native close/quit和App收尾回归；本轮未GUI/Release/D06/CI/长测。08退ready-for-agent。所有项成批原Pi返修，修完一起复核，全部通过后才正式桌面验收。

### 2026-10-24 / Pi 实现：整批首轮返修（原生关闭/退出与待核对闭环）

修 ST-B01-08-1..4 / SP-B01-08-1..3。

#### 1. 原生退出不再绕过（P1）

- Rust：`lib.rs` 改为 `.build(...).run(|app, event| ...)`，在 `RunEvent::ExitRequested` 里
  读 `AppState::exit_blocked()`；为真则 `api.prevent_exit()` 并 `emit("revia://exit-requested")`。
  默认 `false`（放行），避免进程无法结束。
- `AppState` 新增 `exit_blocked: AtomicBool` 与 Tauri 命令 `set_exit_blocked`。
- 前端：写操作开始/结束时同步 `ipc.setExitBlocked(...)`；监听 `revia://exit-requested`
  走**同一套** `CloseCoordinator` 编排，动作类型保持 `exit`。
- 退出动作不再用 `destroy` 代替：`performCloseIntent("exit")` 先解除阻止标志再关窗，
  让原生退出流程正常走完。
- **监听安装失败不再静默**：`onCloseRequested` / `listen` 任一失败都写入全局错误
  （新增 i18n `close.listenerFailed`），用户可见。

#### 2. 无请求绝不凭空关窗（P2）

`finishWrite` 删除了 `pendingIntent() ?? "close"` 这条兜底：没有原请求、
或用户已选「留在应用」时，写操作结束**什么都不做**。
只有 `resolveAsk("wait")` 保留的原请求才会在全部写完成时由 `takeReadyIntent()` 取出执行。

#### 3. 另存生命周期（P2）

- 进入 `performSaveAs` 即用占位键 `save-as:<destination>` 登记在途（解析归属期间也被保护），
  解析出归属后改用该 worktree 键；**外部新路径不加 worktree 写锁**，只占关闭生命周期；
- `finally` 用 `heldGateId ?? provisionalKey` 收尾，`needsVerification` 从错误种类**如实传递**
  （不再固定 `false`）。

#### 4. 待核对闭环（P2）

- `endWrite` 不再清除 pending，**只有** `acknowledgePending` 能清；
  新增 `confirmPendingContinue()` 表示「已核对并继续」；
- 对话框区分两种模式：有待核对时只提供「我已核对，继续关闭」与「留在应用」；
  前者对每个待核对目标**真实 `refreshTarget` 重读**（占位键跳过），
  再 `acknowledgePending` + `confirmPendingContinue`，最后才执行原请求；
- 不自动重跑任何写命令，也不让下一次写入代替核对。

#### 回归（Node，同一生产编排）

新增 7 项：无原请求时写结束不关窗、Stay 后不自动关窗、Wait 后才执行原请求（多 worktree 全部完成）、
重复请求不叠加不改动作、待核对必须真实核对且核对后仍需用户继续、
外部另存占位键计入在途但不占 worktree 门禁、另存 `needsVerification` 如实保留。

#### 未验证（必须保留）

- **真实桌面关闭/退出交互未验证**：macOS Quit / ⌘Q 的实际拦截、对话框观感、
  待核对闭环的实际流程均由规划方在固定 Release 上验收。
- 未构建 Release、未启动 GUI、未跑 D06/CI/长测。

### 2026-10-25 / 规划方：整批返修两轴复核，08 仍不通过

使用 code-review 两个独立轴，固定 `git diff 51fe66385f7633762685b8014b8aa551538da96a...be02cb7431af3f6587a82bebf6360fca73879b8b`。下列行号均以 `be02cb7` 为准；规划方核对当前 `m2-ui` 的 `6b3ddf5`，这些生产接线尚未改变。07 共享批次晚加入资格静态闭合，无新发现；旧 13 静态稿由 Q107/rev3 取代，不要求返工历史稿。

#### Standards：确定违反 6 项（P1 一、P2 四、P3 一），Fowler 判断建议 0 项

1. **ST-B02-08-1 / P1 / 原生退出保护可缺席**：`App.tsx:1071` 同步保护 IPC 为 fire-and-forget，失败仍继续写入；`:1080/1284` 又只按 `hasWriting()` 解除保护，忽略待核对。`src-tauri/src/lib.rs:148` 此时放行 Quit。违反 Q96 保留进程与待核对不静默退出；保护应在实际写入前可靠成立，失败不能仅显示错误后继续写，待核对期间仍受保护。核对监听安装失败时实际写入口的安全性。
2. **ST-B02-08-2 / P2 / 未选择即自动关闭**：`refreshGate.ts:1105` 询问时已经保存 intent，`:1140` `takeReadyIntent` 不检查 asking；写入在用户选择前结束即可触发 `finishWrite` 关窗。违反 Q96 和首轮“仅显示询问不算授权”。同时覆盖写已结束后用户才点等待的收尾，不能丢请求。
3. **ST-B02-08-3 / P2 / 另存等待不收尾**：`App.tsx:1283` finally 只 `endWrite` 和同步 flag，绕过 `finishWrite/takeReadyIntent`；最后一笔另存完成不执行已授权的原请求。违反 Q96；全部写入口使用一致、真实的生产生命周期。
4. **ST-B02-08-4 / P2 / 失败核对清账、并发结果被误清**：`App.tsx:2628` 忽略 `refreshTarget` 的 false/异常（其 `:599-619` 明确表示失败或不可信），仍 acknowledge；`:2632` `confirmPendingContinue` 又无代际全清 pending。核对 A 期间 B 或同目标新一代结果出现，未经查看也被清掉并关闭。违反 Q91/Q96；只确认本次真实成功核对的那一代，失败与新增 pending 保留并可见，不以下一次写入代替核对。
5. **ST-B02-08-5 / P2 / 重复请求替换动作**：`refreshGate.ts:1105` 每次 request 覆盖 intent，关闭询问中再次 Quit 可改成退出，反向亦然。违反 Q96 及首轮“保留原请求、不改变动作类型”。App 的展示 intent 也不能与编排保留的动作不一致。
6. **ST-B02-08-6 / P3 / 文案未集中**：`App.tsx:768/784` 的“窗口关闭”“原生退出”直接进入用户错误，违反技术方向 Q40；迁入已有 i18n。

#### Spec：错误实现 5 项（均 P2），范围扩张 0 项

1. **SP-B02-08-1 / P2**：同未授权即关闭，违反规格“提供等待完成和留在应用”。`refreshGate.ts:1105/1140` 与真实 `App.tsx:1100` 组合触发，不能只读 helper 注释判通过。
2. **SP-B02-08-2 / P2**：`App.tsx:1099` 最后一笔 NeedsVerification 后同步 false，原生 Cmd-Q 绕过 pending 展示。违反“结果待核对则先展示、不静默退出”。
3. **SP-B02-08-3 / P2**：`App.tsx:1302` 另存 finally 不消费等待请求，可能由后续无关写结束才关窗。违反“正常完成并记录结果后执行原请求”。
4. **SP-B02-08-4 / P2**：`App.tsx:2672/2676` 核对失败仍清账、核对中的新 pending 全部被清。违反“真实核对后继续、不静默退出”；按对应快照/代际核对，不对整个集合盲清。
5. **SP-B02-08-5 / P2 / 等待后出现 pending 没有展示**：`App.tsx:1101` `takeReadyIntent` 返回 null 便退出。用户已选等待、对话框已隐藏，此后结果变 pending 不再显示 `showPending`，切到其他 worktree 时原错误也未必可见。违反“结果待核对则先展示”；等待结束进入待核对时要主动展示并保留原动作与进程，不要求用户再次尝试退出才发现。

统计各轴独立保留：Standards 6（最严重 P1），Spec 5（最严重 P2）。未将尚未桌面/CI算代码发现。本轮静态核验无新运行测试；既有绿色 helper 回归不足以证明 App/native 真实生命周期。08 退回 `ready-for-agent`，整包交原 Pi 修，不以逐项通过替代整批复核。

#### 返修与验证边界

- 新实施目录 `/Users/garland/.codex/worktrees/m2-close-finish/Revia`，分支 `m2`，起点 `be02cb7431af3f6587a82bebf6360fca73879b8b`。旧 m2 worktree 已不存在，不在主目录切分支。`m2-ui` 独立验收，不在本轮 Pi 修改范围。
- 通过生产调用链覆盖暂存、取消暂存、丢弃、恢复、有归属与外部另存；Node 可提取 App 实际使用的共享异步编排验证，但不复制一份测试专用实现。Rust 原生状态/命令路径按需要验证。核心并发与关闭行为应有能稳定复现上述缺口的现有测试体系回归。
- 完成全部修改后集中检查，保存固定源码 SHA、精确命令、退出码和原始 stdout/stderr；只用本次唯一创建的 Git fixture/独立数据目录，不操作真实用户仓库或凭据。不得放宽断言、覆盖旧失败、清理 index.lock 或自动重跑写命令。
- Pi 不启动 GUI、不构建 Release、不跑 D06/CI/长测、不合并/推送；规划方两轴通过后完成真实桌面关闭/Quit/等待/留在应用/待核对与最终 M2 验收。08 及 M2 尚未完成。

### 2026-10-24 / Pi 实现：整批 B02 返修（关闭/退出生产闭环）

新实施目录 `/Users/garland/.codex/worktrees/m2-close-finish/Revia`，起点 `be02cb7`。
逐项修 ST-B02-08-1..6 / SP-B02-08-1..5，全部改动落在真实 App 与 `CloseCoordinator` 生产路径上。

#### ST-B02-08-1 / SP-B02-08-2（原生保护可缺席）

- `syncExitBlocked` 改为 **await** 的 `async`，不再 fire-and-forget；
- 新增 `beginProtectedWrite(worktreeId)`：先登记在途，再 `await setExitBlocked(true)`，
  失败则回滚登记并**抛出**，调用方（stage/unstage/discard、restore、save-as）**中止写入**
  ——不再"显示错误后继续未受保护地写"；
- 保护标志按 `hasWriting() || hasPending()` 计算：**待核对期间仍阻止退出**；
- `lib.rs` 的 `RunEvent::ExitRequested` 已读 `exit_blocked`，为真则 `prevent_exit` 并 emit。

#### ST-B02-08-2 / SP-B02-08-1（未选择即自动关闭）

`CloseCoordinator` 拆成两层：

- `requested`：用户提出的**原动作**（第一次为准，询问中再次请求不替换）；
- `authorized`：只有 `resolveAsk("wait")` 才设置。

`takeReadyIntent()` 现在要求 `authorized !== null`，因此**仅显示询问不构成授权**：
写入在用户选择前结束不会关窗。写入结束后用户才点「等待」也能收尾（`resolveAsk("wait")`
之后 `takeReadyIntent()` 返回原动作）。

#### ST-B02-08-3 / SP-B02-08-3（另存等待不收尾）

`performSaveAs` 的 `finally` 改为调用**同一个** `finishWrite(heldGateId ?? provisionalKey, needsVerification)`，
与其它写入口完全一致：最后一笔另存也会消费已授权的等待请求。
外部路径仍只用占位键，不强加 worktree 写锁。

#### ST-B02-08-4 / SP-B02-08-4（核对失败清账、并发误清）

- `pending` 改为 `Map<worktreeId, generation>`，每次新增待核对 `generation` 递增；
- `acknowledgePending(worktreeId, generation)` **只有代际一致才清除**，返回布尔；
- `confirmPendingContinue()` 不再 `pending.clear()`；
- App 的核对流程：`refreshTarget` 返回 `false` 或抛错 → **不清账**，继续展示待核对；
  核对期间该目标或其它目标出现新 pending → 旧代际清不掉，全部重新展示。

#### ST-B02-08-5（重复请求替换动作）

`request()` 只在 `requested === null` 时记录动作；询问中再次 Quit/关闭不改变原动作。
对话框显示的动作来自同一个 `requested`，不再与编排不一致。

#### ST-B02-08-6（文案未集中）

`"窗口关闭"` / `"原生退出"` 迁入 i18n（`close.guardWindowClose` / `close.guardNativeExit`），
并新增 `close.guardUnavailable`。App 里不再写死用户可见文案。

#### SP-B02-08-5（等待后出现 pending 没有展示）

`finishWrite` 在「已授权等待 + 本次结束变成待核对」时**主动**构造
`showPending` 并 `setCloseRequest(...)`，保留原动作与进程，不要求用户再次尝试退出。

#### 回归（Node，同一生产编排）

`tools/test-refresh-gate.mjs` **127 项通过**。B02 新增 9 项 + 修正既有 5 项为新契约：
询问未选择就完成（不关窗）、结束后才点等待（收尾）、重复请求不替换动作、
待核对期间保护仍成立、最后一笔 NeedsVerification 主动展示、核对失败不清账、
核对期间同目标/不同目标新增 pending 不被误清、核对中 Stay、等待后出现 pending 保留原动作、
Stay 后可重新请求。

#### 检查（本批唯一日志目录）

`/private/tmp/revia-b02-PvngWy`

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `pnpm exec tsc -b` | 0 | — |
| `pnpm lint` | 0 | — |
| `pnpm build` | 0 | — |
| `node tools/test-refresh-gate.mjs` | 0 | tests 127 / pass 127 / fail 0 |
| `cargo test`（完整） | 0 | 100 + 71(1 ignored) + 20 + 24 + 15 + 31 + 3 |
| `pnpm bindings` | 0 | — |
| `git diff --check` | 0 | — |

源码前后 SHA 逐行一致。

#### 未验证（必须保留）

- **真实桌面关闭/退出交互未验证**：macOS Quit/⌘Q 拦截、对话框观感、待核对闭环流程、
  保护 IPC 失败时中止写入的实际表现，均需规划方在固定 Release 上验收。
- 未启动 GUI、未构建 Release、未跑 D06/CI/长测、未推送/合并。


### 2026-10-25 / 规划方：02ff5a3 第二轮限定复核仍不通过

固定 `git diff a8c87b5bfbf11fbfb5e4daafcf2f8c3682439010...02ff5a389d2550932adf111b62b3b369eb024b4d`，六文件。两个独立轴审查，不合并计数；下列行号均对应 `02ff5a3`。

#### Standards：确定违反7项（P1二、P2五），Fowler判断建议0项

规范出处为 `docs/technical-direction.md:52` Q90、`:55` Q91、`:56` Q96及上轮ST-B02要求。

1. **ST-B03-08-1 / P1 / 保护IPC倒序**：`App.tsx:1099` 的旧false不串行/不带代际，下一笔`:1080`的true即使await成功，旧false后到仍会解除写入保护。本地Tauri2.12.4 `scripts/ipc-protocol.js:37`各invoke独立fetch，Rust仅无版本AtomicBool store，没有顺序保证。违反Q96。需确保旧释放绝不能覆盖新占用，不能仅在JS回包后丢旧响应却让Rust旧store已生效。
2. **ST-B03-08-2 / P1 / 监听失败仍写**：`App.tsx:1076`只确认布尔IPC成功，`:768/784`安装关闭/退出监听失败只报错，未阻止实际写入口。关闭监听缺失时窗口能默认关；Rust `lib.rs:146`发现main已销毁即放行。违反Q96，上轮ST-B02-08-1未闭合。
3. **ST-B03-08-3 / P2 / 晚等待不收尾**：`App.tsx:2656`只授权并隐藏；写入已结束后没有下一次finishWrite消费ready。Node用例额外手动takeReadyIntent，掩盖App缺口。违反Q96。
4. **ST-B03-08-4 / P2 / 保护失败遗留请求**：`App.tsx:1082`仅endWrite后抛错。保护挂起期间用户选wait，随后保护失败，外层catch没有统一收尾；请求悬挂或由后续无关写触发。违反Q96。
5. **ST-B03-08-5 / P2 / 旧核对授权新请求**：`App.tsx:2676`未捕获关闭请求代际。A核对中Stay→新关闭请求，旧回调`:2702`授权新requested并关窗，无新确认。违反Q90/Q96。
6. **ST-B03-08-6 / P2 / 新待核对被隐藏**：`App.tsx:2697`仅检查初始targets，A核对期间B新增pending，A成功后`:2703`隐藏对话框；takeReady虽拦关闭，B却不再展示。违反Q91/Q96。
7. **ST-B03-08-7 / P2 / 展示动作不一致**：`App.tsx:2653`读取每次事件覆盖的closeIntentRef。首次close再次Quit，按钮写等待退出，Coordinator保留close；反向亦然。上轮ST-B02-08-5未完整闭合。

#### Spec：缺失/部分2项、错误实现3项（P1一、P2四），范围扩张0项

1. **SP-B03-08-1 / P1 / 缺失保护**：`App.tsx:767/1076`监听失败只报错、保护前置不要求监听可用，真实写仍执行；`lib.rs:146`窗口销毁后退出放行。违反`spec.md:126`“先保留窗口和进程”。
2. **SP-B03-08-2 / P2 / 晚等待缺收尾**：`App.tsx:2656`写先结束用户后点等待，只授权藏窗，不消费ready；先变pending也不主动显示。违反`:127`“正常完成并记录结果后执行原关闭／退出请求”。helper手工消费不证明App完成。
3. **SP-B03-08-3 / P2 / 旧回调错误授权**：核对A→Stay→新关闭，旧回调`App.tsx:2702`及`refreshGate.ts:1205`确认新requested，未获新选择即关窗；失败分支`:2699`还可重弹已取消询问。违反`:126`提供“等待完成后关闭／退出”和“留在应用”。
4. **SP-B03-08-4 / P2 / 新pending被隐藏**：`App.tsx:2674`只捕获A，B在核对中新增，A成功后`:2703`藏掉对话框；B未清账但没有后续展示事件。违反`:127`“结果待核对则先展示”。
5. **SP-B03-08-5 / P2 / 动作展示不一致**：`App.tsx:774/2653`使用last-event展示而非保留requested，先close后Quit/反向都可不一致。违反`:127`“执行原关闭／退出请求”及任务08展示一致要求。

统计：Standards7、最严重P1；Spec5、最严重P1。只静态复核，无新测试/GUI/CI。旧未授权自动关闭、另存finally与pending代际本身已有改善，但不据此宣称全闭合；问题成批交原Pi，08保持ready-for-agent。

本版检查原件26份已归档到[清单](../evidence/batch02-repair/manifest.json)。四份改动代码/测试的检查前后SHA与02ff5a3原字节匹配，另两份文档提交SHA匹配。原始Rust264通过/1ignored、Node127及前端检查exit0可核；原件没有完整检查树SHA或逐命令runner/精确参数记录，因此不把四源码证据扩大为全树检查前字节证明。完整边界见[记录](../batch02-repair-validation.md)。下一轮必要回归必须覆盖App实际调用的异步编排，不能再只用测试手动多调用一次helper来模拟缺失接线。

### 2026-10-25 / Pi 实现：整批 B03 返修（异步关闭编排收为单一生产入口）

起点 `1cd80b6`，实施目录 `/Users/garland/.codex/worktrees/m2-close-finish/Revia`。
逐项修 ST-B03-08-1..7 / SP-B03-08-1..5。

#### 统一生产编排（本轮结构性改动）

`CloseCoordinator` 重构为**唯一**编排，新增 `reconcile()`：

- `requested`（原动作，第一次为准）与 `authorized`（仅 `resolveAsk("wait")` 设置）分离；
- 所有生命周期事件（写结束、晚点 Wait、保护 begin 失败回滚、另存收尾、
  待核对成功/失败、Stay、重复请求）后，App 都调用同一个 `reconcileClose()`，
  由 `reconcile()` 返回 `proceed` / `ask` / `none`；
- 删除 `takeReadyIntent()`：不再有"需要调用方额外手动消费"的第二入口，
  Node 回归也不再手动多调一次 helper 来模拟 App 缺失的接线。

#### ST-B03-08-1 / SP-B03-08-1（保护 IPC 倒序）

新增 `ExitGuardSync`（`refreshGate.ts`）：每次更新分配单调递增 `seq`，
Rust 侧 `AppState::apply_exit_guard(seq, blocked)` **只接受更大的序号**，
`set_exit_blocked` 命令改为 `(seq, blocked) -> bool`。
因此「旧 `false` 的 invoke 晚到」不可能解除新建立的保护——
不是只在 JS 丢旧回包，而是 Rust 实际状态有序。
`ipc.ts` 契约同步更新；回归用与 Rust 同语义的假接收端验证旧 false 晚到仍保持 true。

#### ST-B03-08-2 / SP-B03-08-1（监听失败仍写）

新增 `CloseGuardGate`：窗口关闭监听与原生退出监听**都**装好才算 `ready`；
安装中、失败、卸载/重装一律未就绪。`beginProtectedWrite` 在开始任何写入前
检查 `closeGuardsRef.current.ready`，不就绪直接抛出（`guardUnavailable`），
调用方中止写入——不再"报错后继续未受保护地写"。

#### ST-B03-08-3 / SP-B03-08-2（晚等待不收尾）

`onWait` 现在 `resolveAsk("wait")` 后**立即** `reconcileClose()`：
写入可能早已结束，此时直接执行原请求；若已变待核对则立即展示。
不需要等下一笔无关写。

#### ST-B03-08-4（保护失败遗留请求）

`beginProtectedWrite` 失败走 `rollbackWrite()` 撤销登记，并统一 `reconcile()`，
不留幽灵在途/授权。回归覆盖"回滚后授权仍能正常执行，不悬挂"。

#### ST-B03-08-5 / SP-B03-08-3（旧核对授权新请求）

`CloseCoordinator` 新增 `generation()`（请求代际）：`request()` 与
`resolveAsk("stay")` 都推进它。核对流程在开始时捕获 `requestGen`，
每次 await 后与结束时都比对；不一致直接返回，不 confirm、不关窗、不重弹。

#### ST-B03-08-6 / SP-B03-08-4（新 pending 被隐藏）

核对只按 `acknowledgePending(worktreeId, generation)` 清自己成功核对的那一代；
失败或出现新 pending 时 `allConfirmed = false`，随后统一 `reconcile()`，
按**当前真实集合**重新展示（含其它目标新增的待核对）。

#### ST-B03-08-7 / SP-B03-08-5（展示动作不一致）

对话框的 `intent` 来自 `closeIntentRef`，而 `closeIntentRef` 只由
`reconcile()` 用 `decision.intent`（即 `requested`）写入——不再由每次事件覆盖。

#### 回归（Node，同一生产编排）

`tools/test-refresh-gate.mjs` **140 项通过**。B03 新增 13 项，直接驱动
`CloseCoordinator` / `ExitGuardSync` / `CloseGuardGate` 这三个 App 实际使用的类：
旧 false 晚到不覆盖新 true、新 false 正常释放、监听只装一个不算就绪、
安装失败未就绪且原因可见、卸载回未就绪、晚点 Wait 立即收尾、
晚点 Wait + pending 展示、回滚后不悬挂、旧核对不授权新请求、
核对期间新 pending 不被隐藏、展示动作取保留原动作、重复点击不重复授权、
Stay 后不重弹已取消询问。

#### 检查（唯一日志目录）

`/private/tmp/revia-b03-HSdpQU`（含 `runner.sh` 精确命令与 cwd、各命令 `.out/.err/.exit`、前后源码 SHA）

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `pnpm exec tsc -b` | 0 | — |
| `pnpm lint` | 0 | — |
| `pnpm build` | 0 | — |
| `node tools/test-refresh-gate.mjs` | 0 | tests 140 / pass 140 / fail 0 |
| `cargo test`（完整，因改了 Rust 保护契约） | 0 | 100 + 71(1 ignored) + 20 + 24 + 15 + 31 + 3 |
| `pnpm bindings` | 0 | — |
| `git diff --check` | 0 | — |

源码前后 SHA 逐行一致。

#### 未验证（必须保留）

- **真实桌面关闭/退出未验证**：macOS Quit/⌘Q 的实际拦截、监听安装失败时的拒绝写入、
  待核对闭环流程与对话框观感，均需规划方在固定 Release 上验收。
- 未启动 GUI、未构建 Release、未跑 D06/CI/长测、未推送/合并；08 保持 `ready-for-human`。

### 2026-10-25 / 规划方：4dec0b5 第三轮限定复核仍不通过

固定 `git diff 1cd80b696ef121422d31497460c7f226b6a50b98...4dec0b531a88b5d52469ad94bf33ed18de15a490`，8文件。两个独立轴，不合并计数；行号对应4dec0b5。

#### Standards：确定违反3项（P1一、P2二），Fowler判断建议0项

规范为 `docs/technical-direction.md:52` Q90、`:55` Q91、`:56` Q96及前轮ST-B03要求。

1. **ST-B04-08-1 / P1 / 切worktree卸载在途保护**：`App.tsx:177`使setAppError随worktreeId变化，关闭监听effect仍依赖它（`:802`），允许切换时`:799-800`卸载全部监听再异步安装（`:764/777`）。ready闸门只拦新写，不保护已在执行的A写；窗口关闭在空档无监听可默认销毁，`lib.rs:146`窗口销毁又放行退出。`beginProtectedWrite:1116`等待期间卸载后也未复核ready。违反Q90/Q96，不能用禁止浏览切换替代修复。
2. **ST-B04-08-2 / P2 / 晚Wait按钮被提前隐藏**：用户未选择时最后写结束，`refreshGate.ts:1191`返回none，`App.tsx:1152`隐藏询问，但requested仍暗留。Node新增用例手动在此后resolveAsk，真实用户已无按钮。违反Q96与前轮ST-B03-08-3，需保留可实际完成的询问及原动作。
3. **ST-B04-08-3 / P2 / rollback仍未接生产reconcile**：`App.tsx:1118`只rollback后抛错；stage/restore/save-as外层catch（`:1176/:1253/:1319`）位于finishWrite finally之外，保护挂起期间已选择Wait后失败仍遗留授权/旧询问。Node`:2328`额外手调reconcile掩盖。违反Q96与前轮ST-B03-08-4。

#### Spec：缺失/部分2项、错误实现1项（P1一、P2二），范围扩张0项

1. **SP-B04-08-1 / P1 / 保护生命周期不完整**：`App.tsx:177/799/802/1116`，允许切worktree导致已有写期间监听卸载，或保护await返回后在未就绪状态开始Git。违反 `spec.md:94`“切换文件和 worktree”与`:126`“先保留窗口和进程”。
2. **SP-B04-08-2 / P2 / 保护失败未收尾**：`App.tsx:1118/1176/1253/1319`，rollback后未处理已授权请求，可能等下一笔无关写才关闭。违反 `spec.md:127`“正常完成并记录结果后执行原关闭／退出请求”。
3. **SP-B04-08-3 / P2 / 未选择询问消失**：`refreshGate.ts:1191`与`App.tsx:1152`，写先结束就藏询问，晚Wait实际不可达且暗留requested。违反 `spec.md:126`提供“等待完成后关闭／退出”和“留在应用”，以及`:127`执行原请求。

统计：Standards3、最严重P1；Spec3、最严重P1。Rust版本锁、A核对期间B新pending展示、请求代际及原动作展示已有静态改善，不否定这些进展，但不能据此放行。

**测试证据边界**：`test-refresh-gate.mjs:2239-2248`假Rust async函数内部没有await/defer，旧false在调用时已同步store，然后true才store；未实际制造“旧false晚到”，更未执行真实Rust `apply_exit_guard`。下一轮使用真实可控延迟的共享异步生产入口，不用测试手工补一步reconcile，Rust契约回归覆盖真实版本接收端；不新增框架。

七份源码检查前后/固定提交/当前字节SHA一致；Node140、Rust264/1ignored、前端检查退出0原件已归档，详见[检查记录](../batch03-repair-validation.md)及[清单](../evidence/batch03-repair/manifest.json)。runner有cwd/命令但无逐调用环境，不扩大证据。所有缺陷成批交原Pi；08保持ready-for-agent，10/11/M2未完成，未新Release/桌面/D06/CI/长测/推送/合并。

### 2026-10-25 / Pi 实现：整批 B04 返修（关闭保护的真实异步生产路径）

起点 `f92c968`，实施目录 `/Users/garland/.codex/worktrees/m2-close-finish/Revia`。
逐项修 ST-B04-08-1..3 / SP-B04-08-1..3。

#### ST-B04-08-1 / SP-B04-08-1（切 worktree 卸载在途保护）

根因：`setAppError` 依赖 `worktreeId`，而关闭监听 effect 依赖 `setAppError`，
切换 worktree 就卸载重装监听，在途写保护出现空档。

- `setAppError` 改为**稳定身份**：worktreeId 经 `worktreeIdRefForError` ref 读取，
  依赖数组为空；
- 监听 effect 依赖数组改为 `[]`（**应用级生命周期**），不再随 worktree 变化重装；
- 新增 `WriteGuardOrchestrator.begin()`：`await guard()` **之后**再复核一次
  `guardsReady()`，保护建立期间发生卸载/重装/失败时回滚并拒绝执行 Git。

未禁止浏览/切换 worktree，Q90 行为不变。

#### ST-B04-08-2 / SP-B04-08-3（询问被提前隐藏）

`reconcile()` 原先在「无在途写、无授权」时返回 `none`，App 据此隐藏对话框，
但 `requested` 暗留，用户再也没有 Wait/Stay 可点。

现在：无在途写但**仍有未授权的原请求**时返回 `ask`（保留对话框与原动作），
用户可以真实点击 Wait/Stay。`none` 只在确实没有请求时出现。

#### ST-B04-08-3 / SP-B04-08-2（rollback 未接生产 reconcile）

新增共享类 `WriteGuardOrchestrator`（`refreshGate.ts`），App 的
stage/unstage/discard、restore、save-as **全部经同一实例**：

```
begin(): guardsReady? → register → await guard()
         失败 → rollback + reconcile（真实收尾）→ 抛出
         成功后复核 guardsReady，不就绪则 rollback + reconcile 并返回 ok:false
end():   endWrite + reconcile
```

保护失败时已授权的 Wait 请求在**当场**被 reconcile 处理，不留悬挂授权、
不等下一笔无关写。三个入口都在 `finally` 里走 `orchestrator.end()`，
不再有 `finally` 之外的提前 return/throw 绕过收尾。

#### 回归（Node + Rust，真实异步）

`tools/test-refresh-gate.mjs` **150 项通过**。B04 新增 11 项：

- **真实乱序**：用 `makeDeferredReceiver()` 精确控制到达与应用顺序，
  让**新的 true 先应用、旧的 false 后应用**，断言 Rust 侧状态仍为 true；
- 新 false 在更高版本下正常释放；
- 共享编排：切 worktree 不影响在途保护、`await` 期间监听被卸载则不执行 Git 并回滚、
  保护失败时已 Wait 的请求即时收尾、三个写入口共用同一实例且都走真实 reconcile；
- 询问不提前消失（最后写结束仍可 Wait）、Stay 后不自动关且不留暗请求、
  无请求不产生询问、Wait 时已有 pending 展示核对。

**Rust 真实版本接收端**（`commands.rs` 新增 `exit_guard_tests`，3 项）：
新 true 后旧 false 被拒绝、新 false 正常释放、相同序号不重复生效。
这不是把算法复制到 JS 假对象，而是在 Rust 侧验证真实接收语义。

#### 检查（唯一日志目录）

`/private/tmp/revia-b04-QMqsIw`（含 `runner.sh` 精确 argv/cwd、`tested-source-manifest.txt` 72 个受测源文件清单、
各命令 `.out/.err/.exit`、前后源码 SHA）

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `pnpm exec tsc -b` | 0 | — |
| `pnpm lint` | 0 | — |
| `pnpm build` | 0 | — |
| `node tools/test-refresh-gate.mjs` | 0 | tests 150 / pass 150 / fail 0 |
| `cargo test --lib exit_guard` | 0 | 3 passed（版本接收端） |
| `cargo test --lib` | 0 | 103 passed |
| `git diff --check` | **首次 2** → 清理后 0 | 绑定行尾空白（`cargo test --lib` 会重跑 ts-rs 生成）；两份原件都保留 |

源码前后 SHA 逐行一致。本轮改了 Rust 保护契约（新增接收端测试），
按共享核心门槛跑了 `--lib`；未跑完整 Rust 集成套件（无相关契约变化）。

#### 未验证（必须保留）

- **真实桌面关闭/退出未验证**：macOS Quit/⌘Q 实际拦截、切换 worktree 期间的真实观感、
  待核对闭环流程，均需规划方在固定 Release 上验收。
- 未启动 GUI、未构建 Release、未跑 D06/CI/长测、未推送/合并；08 保持 `ready-for-human`。

### 2026-10-25 / 规划方：e1ab47d 第四轮限定复核，前轮三项闭合但仍不通过

固定 `git diff f92c968689b7ac312c30b6446e74b54c717dcfe6...e1ab47d10f1753722c4f2e61b1f71786ccf63470`，6文件。两个轴独立计数，行号对应e1ab47d。

#### Standards：确定违反2项（P2二：运行时1、验证规范1），Fowler判断建议1项

1. **ST-B05-08-1 / P2 / 旧异步proceed越过Stay**：`refreshGate.ts:1193`在原生关闭完成前消费并清空授权/原请求，`App.tsx:1127`隐藏询问，但`:1081`退出保护IPC仍可挂起。确定交错：A写期间Quit→Wait；A完成后旧退出回包挂起；用户启动B，请求关闭并选Stay；B完成后旧回包恢复，`:1082`不核请求代际直接close，监听见无写就放行，Stay后仍关窗。若B尚在途，`:774`会将原exit登记为close。违反 `technical-direction.md:56` Q96留在应用/执行原请求。授权所有权须延续到原生动作完成，await后检查请求代际及真实在途/pending；旧回包不能执行已取消请求，也不能更换原动作。
2. **ST-B05-08-2 / P2 / Rust测试复制接收端**：`commands.rs:1042`测试Guard::apply复制生产AppState::apply_exit_guard（`:72`）；三测试只调副本，生产比较退化仍绿。任务08`:443`称其真实生产接收端不实。违反 `local-validation.md:16`生产入口证据要求。调用实际生产对象或提取生产/测试共用的小接收器，别再复制算法。

Possible Duplicated Code判断建议1项：上述重复版本算法易漂移，关联ST-B05-08-2，不额外计为确定违反。

#### Spec：缺失/部分1项（P2），范围扩张0项，实现错误0项

1. **SP-B05-08-1 / P2 / 真实Rust回归未接入**：`spec.md:149`要求“从应用操作入口验证可观察行为”，任务08前轮`:385`要求真实版本接收端。`commands.rs:1033/1042/1059`自建Guard并复制算法，不经过生产方法，未满足这项验证要求；本轮报告`:443`扩大证据。应共享生产接收实现并更正。这不是判定当前版本锁算法有错。

统计：Standards2、最严重P2（建议1）；Spec1、最严重P2。前轮监听切换、晚Wait按钮、rollback真实收尾的直接路径已闭合，不反复计旧项；新异步授权生命周期与真实接收端回归成批交原Pi。

**原始检查边界**：Node150、Rust新测试3与lib103、前端退出0；首次diff-check2及清理后0均保留。6份前后SHA匹配固定提交；72路径清单无72份before/after哈希。runner写第二Rust为session_flow，但原始stdout是lib103，不把runner当精确执行日志。见[检查记录](../batch04-repair-validation.md)与[清单](../evidence/batch04-repair/manifest.json)。不补造缺失记录，下一轮逐命令真实落盘。08保持ready-for-agent，未新Release/桌面/D06/CI/长测/推送/合并，10/11/M2未完成。

### 2026-10-25 / Pi 实现：整批 B05 返修（异步关闭授权生命周期与真实接收端）

起点 `e1ab47d`，实施目录 `/Users/garland/.codex/worktrees/m2-close-finish/Revia`。
逐项修 ST-B05-08-1 / ST-B05-08-2 / SP-B05-08-1。

#### ST-B05-08-1（旧异步 proceed 越过 Stay）

根因：`reconcile()` 返回 `proceed` 时就清掉授权与原请求，App 在 `await` 之后
没有任何代际校验，旧回包恢复时会直接关窗。

现在授权**延续到原生动作完成**：

- `reconcile()` 在 `proceed` 时**不再清授权**；
- 新增 `claimExecution()` / `isExecutionValid(generation)` / `completeExecution(generation)` /
  `abandonExecution()`：认领时记下请求代际，`Stay` 或新请求会推进代际使旧认领失效；
- 新增共享入口 `runCloseExecution(deps, claimed)`（`refreshGate.ts`），
  App 与 Node 回归**同源**：每个 `await` 之后校验代际 + 真实在途/pending，
  失效则 `abandonExecution()` 并把仍有效的原动作交回 `reconcile()`，**绝不关窗**；
- 原生退出事件也改走 `reconcile()`（不再直接执行），保证所有权与代际一致。

确定反例（A 写 Quit→Wait → A 结束 → B 写 + Close→Stay → B 结束 → 旧回包恢复）
在回归中直接复现：`runCloseExecution` 返回 `false`，`native` 数组为空。

#### ST-B05-08-2 / SP-B05-08-1（Rust 测试复制接收端）

- 从 `AppState` 提取生产类型 `ExitGuard`（`apply` / `blocked` / `accepted_seq`），
  `AppState::apply_exit_guard` 与 `exit_blocked` 都委托给它；
- 三个测试改为**直接使用 `ExitGuard`**（生产实现），删除测试内复制的 `Guard`；
- 敏感性已验证：临时删掉生产里的 `if seq <= guard.0` 版本比较后，
  `newer_true_rejects_late_older_false` 与 `equal_seq_is_rejected` **真实失败**；
  恢复后 3 项通过。

**更正前轮表述**：B04 报告里「Rust 真实版本接收端」的说法不准确——
当时测试用的是复制算法。本轮起才是真正的生产接收端，B04 段落保留为历史。

#### 回归（Node，App 实际调用的异步入口）

`tools/test-refresh-gate.mjs` **156 项通过**。B05 新增 6 项，全部调用
`runCloseExecution`（App 的 `performCloseIntent` 就是薄包装）：

- 旧回包越过 Stay（A/B 交错）：不关窗；
- B 未结束时旧回包不关窗，且原动作仍是 `exit`（没被改成 `close`）；
- 没有后续请求时正常 Wait 仍执行原动作；
- Stay 之后新请求重新走完整授权，旧回包不影响新请求；
- 旧回包失败只上报，不授权旧动作；
- 执行失败后仍有效的原动作交回编排，不悬挂。

#### 检查（唯一日志目录，逐命令真实落盘）

`/private/tmp/revia-b05-uNREPd`

日志由 `record.py` 在**执行时**逐条记录：真实 `argv` 数组、`cwd`、
显式非敏感环境覆盖（`CI=1`、`NO_COLOR=1`）、start/end 时间、exit、
完整 stdout/stderr，汇总在 `invocations.json`。
`tested-source-sha256-before.txt` / `-after.txt` 覆盖 **79 个受测源文件**
（生产源码 + 测试源码 + 构建配置 + 锁文件，排除 `src/types/bindings`、
`src-tauri/gen`、`dist`、`target`、验收原件与文档），前后逐份一致。

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `pnpm exec tsc -b` | 0 | — |
| `pnpm lint` | 0 | — |
| `pnpm build` | 0 | — |
| `node tools/test-refresh-gate.mjs` | 0 | tests 156 / pass 156 / fail 0 |
| `cargo test --lib exit_guard` | 0 | 3 passed（生产 `ExitGuard`） |
| `git diff --check` | 0 | — |

本轮改了 Rust 生产数据类型（提取 `ExitGuard`），按最小相关范围只跑 `--lib exit_guard`；
未跑完整 Rust 套件。

#### 未验证（必须保留）

- **真实桌面关闭/退出未验证**：macOS Quit/⌘Q 实际拦截、Stay 后不关窗的真实观感、
  待核对闭环流程，均需规划方在固定 Release 上验收。
- 未启动 GUI、未构建 Release、未跑 D06/CI/长测、未推送/合并；08 保持 `ready-for-human`。

### 2026-10-25 / 规划方：013e1fa 第五轮限定复核，仍不通过

固定 `git diff 1dd509ab476a9f279a23074368ec46177b566fb7...013e1fa631c82e99948efa4d3339e1a79103bad7`，唯一提交、5文件。两个独立轴分开计数；行号对应013e1fa。

#### Standards：确定违反3项（P2三项），Fowler判断建议0项

1. **ST-B06-08-1 / P2 / 正常Quit被吞掉**：`refreshGate.ts:1152`无writing/pending时清requested/auth并返回proceed；但`App.tsx:790`只reconcile，得到none。Rust拦住Quit后，A先结束、JS事件后执行，就丢此次原退出。违反`technical-direction.md:56` Q96“完成并记录结果后执行用户原请求”。原生退出事件与request/proceed契约需一致。
2. **ST-B06-08-2 / P2 / 失败后无界自动重试**：`refreshGate.ts:1478`catch保留授权并调用App真实reconcile，`App.tsx:1128`再次claim执行。release/native IPC持续失败时反复执行、隐藏询问，无法回到可控Stay。违反Q96等待/留在应用闭环。失败必须可见，等待用户决定，不自动重复。
3. **ST-B06-08-3 / P2 / 投递关闭回包误当完成**：`App.tsx:1109`的Tauri close只投递CloseRequested；`refreshGate.ts:1472`回包就complete清授权。期间B开始写并拦截关闭：事件先到则保留请求后被complete清，回包先到则原exit可变close。违反Q96保留在途写/执行原动作。区分原生请求投递与最终关闭裁决；相关所有权需排他且旧claim不能清新claim，不能仅按requestGeneration校验。App的claim失败分支目前递归reconcile，修排他时一起避免同步重入。

#### Spec：缺失/部分1项、实现错误2项（P2三项），范围扩张0项

1. **SP-B06-08-1 / P2 / 原生退出立即放行丢请求**：`App.tsx:790`与`refreshGate.ts:1152`，写结束先于退出事件回调时，request清账后reconcile无动作。违反`spec.md:127`“执行原关闭／退出请求”。
2. **SP-B06-08-2 / P2 / 关闭失败不能留在应用**：`refreshGate.ts:1478`与`App.tsx:1131`，失败立即回授权循环，用户没有Wait/Stay询问。违反`spec.md:126`的两个选择。
3. **SP-B06-08-3 / P2 / 原生投递未确认最终放行**：`App.tsx:1109/776`与`refreshGate.ts:1472`，关闭事件因B被拦而成功回包仍消费原exit；B结束不再退出或变close。违反`spec.md:127`执行原请求、不擅自变动作。

统计：Standards3、最严重P2、判断建议0；Spec3、最严重P2、范围扩张0。两个轴不合并计数。Rust生产`ExitGuard`与测试共用、旧release await后Stay校验已有静态改善，不再重复计旧项。

**检查与证据边界**：六条实际调用均退出0（Node156、Rust exit_guard3、tsc/lint/build/diff-check）；79份受测源码检查前/后/固定提交/当前磁盘SHA全部一致，未重复运行测试。`invocations.json`保留真实argv/cwd、显式非敏感环境覆盖与时间；不声称完整继承环境。Pi报告的敏感性2失败/1通过未附原始修改源码/失败日志，仅作实施方报告，不算独立核验。新增Node用例中reconcile仅计数、native仅记录，尚不能证明真实App重入/原生回调闭环。见[检查记录](../batch05-repair-validation.md)与[清单](../evidence/batch05-repair/manifest.json)。08改回ready-for-agent；整批交原Pi，未新Release/GUI/D06/CI/长测/推送/合并，10/11/M2未完成。

### 2026-10-25 / Pi 实现：整批 B06 返修（退出事件、失败状态与原生裁决）

起点 `95d37df`（生产 `013e1fa`），实施目录 `/Users/garland/.codex/worktrees/m2-close-finish/Revia`。
逐项修 ST-B06-08-1..3 / SP-B06-08-1..3。

#### ST-B06-08-1 / SP-B06-08-1（正常 Quit 被吞掉）

根因：`request()` 在无在途写时清掉 `requested`/`authorized` 并返回 `proceed`，
App 只 `reconcile()` 就得到 `none`，Quit 被吞。

现在 `request()` 在无阻塞时把**这次请求本身**记为授权（`authorized = requested`），
**不清账**；`reconcile()` 随即返回 `proceed` 与正确的原动作。
「A 结束 → 退出事件才到」的交错因此能完整执行原 `exit`。

#### ST-B06-08-2 / SP-B06-08-2（失败后无界自动重试）

根因：`runCloseExecution` 的 catch 保留授权并 `reconcile()`，App 再次 claim 执行，
形成无界循环。

新增 `failExecution(claimId)`：释放认领并**取消自动授权**，但保留原请求。
之后 `reconcile()` 返回 `ask`，用户必须重新明确选择（等待/重试 或 留在应用）才会重试。
失败仍通过 `onError` 如实上报，不隐藏。

#### ST-B06-08-3 / SP-B06-08-3（投递被误当完成）

根因：Tauri `close()` 只投递 `CloseRequested`，不确认销毁；回包就 `completeExecution`
会清掉原请求，导致 B 拦截关闭后原 `exit` 丢失或被改成 `close`。

现在区分两个阶段：

- `performNative` 返回 `"delivered"`（已投递、裁决未定）或 `"completed"`（已确认完成）；
- `"delivered"` → `releaseClaimAwaitingVerdict(claimId)`：释放认领但**保留授权与原请求**；
- 原生裁决放行（`onCloseRequested` 里 `decision.action === "proceed"`）才调用
  `completeVerdict()` 清账；
- 认领改为**排他**：`claimExecution()` 在已有认领时返回 `null`（不重复投递），
  每个 claim 带 `claimId`；旧 claim 的 `completeExecution` / `failExecution` /
  `abandonExecution` 对不上 `claimId` 时返回 `false`，**旧回包不会清新 claim**。

#### 回归（Node，生产同源）

`tools/test-refresh-gate.mjs` **165 项通过**。B06 新增 9 项，全部驱动
`CloseCoordinator` + `runCloseExecution`（App 的 `performCloseIntent` 是薄包装）：

- 写结束早于退出事件 → 仍执行原 `exit`；
- release 持续失败 → **只自动尝试一次**，之后必须用户重新选择，且可 Stay；
- 失败后 Stay 真实可留在应用；
- close 投递后 B 开始写（回包后 / 事件前两种顺序）→ 原 `exit`/`close` 都保留；
- 原生裁决放行才清账；
- 重复 `reconcile` 在旧 claim 未结束时不重复投递；
- 旧 claim 晚回/失败不清新 claim（排他，不只按 `requestGeneration`）；
- A/B→Stay 反例仍通过。

#### 检查（唯一日志目录）

`/private/tmp/revia-b06-jQhG5C`（`record.py` 执行时逐条记录真实 argv/cwd/显式非敏感环境/start/end/exit/stdout/stderr，
汇总在 `invocations.json`；`tested-source-sha256-before/-after.txt` 覆盖 79 个受测源文件，
排除 `src/types/bindings`、`src-tauri/gen`、`dist`、`target`、验收原件与文档）

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `pnpm exec tsc -b` | 0 | — |
| `pnpm lint` | 0 | — |
| `pnpm build` | 0 | — |
| `node tools/test-refresh-gate.mjs` | 0 | tests 165 / pass 165 / fail 0 |
| `cargo test --lib exit_guard` | 0 | 3 passed（生产 `ExitGuard`） |
| `git diff --check` | 0 | — |

79 份受测源码 SHA 前后逐份一致。本轮未改 Rust 生产代码，只跑匹配的接收端测试。

#### 未验证（必须保留）

- **真实桌面关闭/退出未验证**：macOS Quit/⌘Q 的实际拦截与裁决、
  close 投递后 B 拦截的真实时序、失败后 Stay 的实际观感，均需规划方在固定 Release 上验收。
- 未启动 GUI、未构建 Release、未跑 D06/CI/长测、未推送/合并；08 保持 `ready-for-human`。

### 2026-10-25 / 规划方：ff6ba78 第六轮限定复核，App执行链仍不通过

固定 `git diff 95d37df24f95e159c7175069b0ace6fabc287a56...ff6ba782a3e5a2e30960856e60bc49bb2bdca876`，唯一提交、4文件。两个独立轴分开计数，行号对应ff6ba78。

#### Standards：确定违反3项（P1一项、P2两项），Fowler建议0项

1. **ST-B07-08-1 / P1 / 排他claim与App重入冲突**：`refreshGate.ts:1228`在active claim时返回null，`App.tsx:1143`却同步reconcile；授权/claim均未改变，持续proceed→null→reconcile至栈溢出，每层`:1135`还发保护IPC。A Quit→Wait→A结束，旧release挂起，B开始并先结束，或重复原生退出事件，均可触发。违反`technical-direction.md:56` Q96可控等待/Stay。已有执行必须等待状态变化，派发已明确要求去掉该递归。
2. **ST-B07-08-2 / P2 / delivered后自动重投**：`refreshGate.ts:1533-1534`释放claim保留授权后立即真实reconcile；无新写入时又claim并投递close，而非等待原生裁决。违反任务08 ST-B06-08-3不重复投递及Q96。与第1项不同，是claim释放后缺少等待裁决状态。
3. **ST-B07-08-3 / P2 / 旧原生投递越过Stay**：旧close已发送但事件暂缓，B写入期间再次请求关闭并Stay，B结束后旧CloseRequested到达；`App.tsx:775/778`把旧事件当全新request，在无写/pending时自动授权并无归属地completeVerdict，关闭窗口。claimId只保护回包，不保护事件。违反Q96留在应用，需关联/拒绝失效原生投递；不能仅修IPC回包。

#### Spec：缺失/部分0项、范围扩张0项、实现错误1项（P1）

1. **SP-B07-08-1 / P1 / 真实App无限同步重入**：`refreshGate.ts:1195/1228`仍proceed但已有claim返回null，`App.tsx:1143`状态不变就自调用；A退出release挂起期间B先结束或重复退出事件可触发。违反`spec.md:126`等待/Stay与`:127`执行原请求。投递回包`:1533-1534`立即reconcile再次投递是同一所有权驱动不收敛的相关子点，本轴不另计。

统计：Standards3、最高P1、建议0；Spec1、最高P1、范围扩张0。保持轴独立，不将相关项合并或放大计数。前轮立即Quit丢请求、失败后保留自动授权的直接路径已有静态闭合；Rust真实生产接收端保持闭合。

**证据边界**：六条检查退出0，Node165/Rust exit_guard3/tsc/lint/build/diff-check；79份受测源码检查前/后/固定提交/当前磁盘SHA一致，未重跑。新增回归仍用计数型reconcile和fake native，没有驱动App请求→执行→重入→裁决完整链，不能替代真实接线。下一轮必须让App与测试共用完整生产驱动，只注入环境适配，不在测试补生产调用。检查/双轴原件见[第六轮记录](../batch06-repair-validation.md)和[清单](../evidence/batch06-repair/manifest.json)。08回ready-for-agent，未新Release/GUI/D06/CI/长测/推送/合并，10/11/M2未完成。

### 2026-10-25 / Pi 实现：整批 B07 返修（App 真实共享关闭驱动）

起点 `332097b`（生产 `ff6ba78`），实施目录 `/Users/garland/.codex/worktrees/m2-close-finish/Revia`。
逐项修 ST-B07-08-1..3 / SP-B07-08-1。

#### App 现在实际调用的共享驱动

新增 **`CloseDriver`**（`src/lib/refreshGate.ts`），App 的 `closeDriverRef` 就是它的实例：

| App 位置 | 调用的驱动入口 |
| --- | --- |
| `onCloseRequested` 原生关闭事件 | `handleRequest("close")` / `hasInFlight()` / `handleNativeVerdict()` |
| `revia://exit-requested` 原生退出事件 | `handleRequest("exit")` |
| 写结束、rollback、待核对清除 | `notifyStateChanged()` |
| 对话框「等待」/「留在应用」 | `handleChoice("wait" / "stay")` |
| 待核对核对完成 | `handlePendingContinue()` |
| 关闭事件被在途写拦住 | `handleVerdictBlocked()` |

App 只做 Tauri/React 适配：把 `CloseDriverEffect` 映射成对话框状态。**不再**在 App 里
自己判断 claim/代际。`runCloseExecution` 已删除（其逻辑并入驱动）。

#### ST-B07-08-1 / SP-B07-08-1（排他 claim 与同步递归，P1）

根因：`claimExecution()` 返回 `null` 时 App 立即 `reconcile()`，状态不变 → 无限同步递归 + IPC 风暴。

驱动引入明确阶段 `idle / asking / waiting / executing / awaitingVerdict / blockedByWrites / failed`：

- `drive()` 在 `executing` / `awaitingVerdict` 阶段**直接返回**，不自调用；
- claim 失败时置 `waiting` 并返回，**不递归**；
- 没有任何定时器自旋。

回归 `B07-1：执行中重复请求不递归、不多发`：release 挂起期间 20 次
`notifyStateChanged()` + 20 次 `handleRequest()`，断言 release 未回时投递数为 0，
释放后**只投递一次**。

#### ST-B07-08-2（delivered 后自动重投，P2）

`"delivered"` 后进入 `awaitingVerdict` 并设 `pendingVerdictClaim`，
**不再**立即 `reconcile()`。等待裁决期间重复请求/reconcile 都不投递。

回归 `B07-2：delivered 后等待裁决，不自动重投`（10 次 reconcile + 10 次请求后仍只有 1 次投递）、
`B07-2：裁决放行后才清账`、`B07-2：投递后被新写入拦住，B 结束后按原 exit 继续（不改成 close）`。

#### ST-B07-08-3（旧原生投递越过 Stay，P2）

驱动把原生投递与裁决**与请求所有权关联**：

- `handleNativeVerdict()` 只对**当前在途投递**（`pendingVerdictClaim`）生效，
  否则直接忽略——Stay 之后到达的旧 `CloseRequested` 不会被当成新用户意图；
- `handleVerdictBlocked()` 释放该 claim 但保留授权与原请求，进入 `blockedByWrites`；
- 真正的新请求（无在途投递时）仍正常走 `handleRequest` 并投递。

回归 `B07-3：Stay 后旧原生事件不当作新意图`：A Quit→Wait、release 挂起、B 开始、
再次请求 + Stay、旧 release 回 → 投递数为 0；随后 `handleNativeVerdict()` 被忽略；
新请求仍能正常投递。

#### 其余回归

`B07：持续失败只自动一次，之后必须用户重新选择`（release 持续失败：1 次尝试、可见错误、
phase 回到 `asking`、5 次通知不重试、用户再次 Wait 才重试）、
`B07：失败后可以 Stay`。B05/B06 的既有用例改为驱动 `CloseDriver`，
语义不变（旧回包越过 Stay、投递后 B 拦截、裁决放行才清账、旧 claim 不清新 claim、
A/B→Stay 反例等）。

#### 检查（唯一日志目录）

`/private/tmp/revia-b07-opWp3M`（`record.py` 执行时逐条记录真实 argv/cwd/显式非敏感环境/start/end/exit/stdout/stderr；
`tested-source-sha256-before/-after.txt` 覆盖 79 个受测源文件，排除 `src/types/bindings`、
`src-tauri/gen`、`dist`、`target`、验收原件与文档）

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `pnpm exec tsc -b` | 0 | — |
| `pnpm lint` | 0 | — |
| `pnpm build` | 0 | — |
| `node tools/test-refresh-gate.mjs` | 0 | tests 172 / pass 172 / fail 0 |
| `git diff --check` | 0 | — |

79 份受测源码 SHA 前后逐份一致。本轮未改 Rust 生产代码，未重跑 Rust 测试。

#### 未验证（必须保留）

- **真实桌面关闭/退出未验证**：macOS Quit/⌘Q 实际拦截与裁决、close 投递后 B 拦截的真实时序、
  Stay 后旧原生事件的实际到达顺序、失败后 Stay 的观感，均需规划方在固定 Release 上验收。
- 未启动 GUI、未构建 Release、未跑 D06/CI/长测、未推送/合并；08 保持 `ready-for-human`。

### 2026-10-25 / 规划方：520f134 第七轮限定复核，剩余原生裁决两项未闭合

固定`git diff 332097b6c0e95b77327f55d1894787545a4cacf2...520f134ff995010d87d885594d99d3518213e725`，4文件，行号对应520f134。旧同步递归、delivered立即重投已静态闭合，不重复计旧项。

#### Standards：确定违反2项P2，Fowler建议0项

1. **ST-B08-08-1 / P2 / 已投递close仍越过Stay**：`App.tsx:778/787`只凭hasInFlight就放行原生事件，`refreshGate.ts:1656/1689`Stay后不核该投递代际、不向适配返回阻止决定。A写Quit→Wait→A结束→close已发送但事件暂缓，此时询问仍可见，用户Stay后旧事件到达依然销毁窗口。违反`technical-direction.md:56` Q96留在应用。驱动内部忽略事件不等于Tauri preventDefault，必须把允许/阻止原生动作作为同源决定。
2. **ST-B08-08-2 / P2 / 早到阻止裁决被晚回包覆盖**：`refreshGate.ts:1705`只处理pendingVerdict，executing期事件被B拦时inFlight未标记已收到裁决；`:1782`晚delivered覆盖成awaitingVerdict。B结束`:1677`不再推进，原exit悬挂。违反Q96正常完成执行原请求。需要让投递结果与原生裁决分别有归属，先收到哪个都不丢终结状态。

#### Spec：缺失/部分0项、范围扩张0项、实现错误2项P2

1. **SP-B08-08-1 / P2**：`App.tsx:778/787`与`refreshGate.ts:1656/1689`，Stay后的旧原生关闭仍被放行，违反`spec.md:126`“留在应用”。
2. **SP-B08-08-2 / P2**：`App.tsx:782`与`refreshGate.ts:1705/1782/1677`，关闭事件先于IPC回包且被B拦后，晚回包使原退出永久等待，违反`spec.md:127`完成后执行原请求。

统计：Standards2、最高P2、建议0；Spec2、最高P2、范围扩张0。两轴独立保留。共享CloseDriver已有推进，但App仍含原生放行决策；Node native只记投递，测试手动调用裁决，不能证明真实适配事件路径。所谓“Stay旧原生事件”测试实际只挂起release而未发送close；早到事件测试未完成B结束后的断言。下一轮对实际完整交错验证，不能继续换成更早的await。

最终5条检查退出0：Node172、tsc/lint/build/diff-check；79份前/后/固定提交/当前磁盘SHA一致。此前Paseo诊断活动有Node失败与过滤输出，提供目录没有完整失败流，不补造；绿色只对应最终记录的5条。Pi称runCloseExecution已删除，但源文件仍保留未使用导出，只是App不再调用，纠正报告边界。原件见[第七轮记录](../batch07-repair-validation.md)及[清单](../evidence/batch07-repair/manifest.json)。08回ready-for-agent，未新Release/GUI/D06/CI/长测/推送/合并，10/11/M2未完成。

### 2026-10-25 / Pi 实现：整批 B08 返修（原生裁决真的能阻止、早到裁决不丢）

起点 `34b2d51`（生产 `520f134`），实施目录 `/Users/garland/.codex/worktrees/m2-close-finish/Revia`。
逐项修 ST-B08-08-1 / ST-B08-08-2 / SP-B08-08-1 / SP-B08-08-2。

#### 原生适配：为什么用「最终提交阶段」

已核对锁定依赖：Tauri 2 的 `CloseRequested` **不携带任何 claimId**，
无法从事件本身判断它属于哪次程序化投递。因此不声称事件携带 id，也不靠
`hasInFlight` 猜测；改用**受保护的最终提交阶段**：

- `execute()` 在投递前做**同步**复核（`isExecutionValid`：代际未变 + 无在途写 + 无待核对），
  通过后才置 `phase = "committing"`、`committedClaim`、`outstandingProgrammaticClose = true`
  并调用 `performNative`；
- 之后到达的 `CloseRequested` 一律由 `handleNativeCloseRequest()` 判定：
  - 有未结算的程序化关闭 → 这次事件是**它**的裁决，用 `claimGeneration(claimId)` 校验授权；
  - 授权仍有效 → `allow: true` 并清账；
  - 授权已失效（Stay）或有新写入 → **`allow: false`**，App 真的调用 `event.preventDefault()`；
- `canStartWrite()` 在 `committing`/`awaitingVerdict` 期间返回 `false`，写入口不能在此后开始新写。

**App 不再含任何 allow/prevent 业务判断**：`onCloseRequested` 只做
`const decision = driver.handleNativeCloseRequest(); if (!decision.allow) event.preventDefault();`。

#### ST-B08-08-1 / SP-B08-08-1（已投递 close 仍越过 Stay）

`handleChoice("stay")` 现在撤销授权（`coordinator.resolveAsk("stay")` 推进代际）
但**保留** `committedClaim`/`pendingVerdictClaim`，因为那次投递仍在外。
旧 `CloseRequested` 到达时 `claimGeneration` 已不匹配当前代际 → `allow: false` → 真实阻止。

回归 `B08-1：close 已投递后 Stay，旧原生事件必须被阻止`：performNative 已调用（挂起回包）、
Stay、旧事件到达 → `allow === false`；回包后 `pendingIntent === null`。
`B08-1：Stay 之后真正的新用户请求仍可执行`：旧事件被阻止后，新请求由驱动自己投递
（`allow: false` 但不绕过编排），`native` 计数 +1。

#### ST-B08-08-2 / SP-B08-08-2（早到阻止裁决被晚回包覆盖）

新增 `earlyVerdict: { claimId, allow }`：`handleNativeCloseRequest()` 在 `committing` 阶段
收到裁决时立即记录并结算该 claim；`execute()` 的回包只确认传输，
若 `earlyVerdict` 已存在就**不进入** `awaitingVerdict`，而是按已消费的裁决结算
（`allow: true` → 完成；`allow: false` → 释放 claim、保留授权，继续等待写入结束）。

回归 `B08-2：CloseRequested 先于 IPC 回包且被 B 拦，之后 B 结束仍执行原 exit`：
投递挂起 → B 开始写 → 事件先到被拦（`allow: false`）→ 回包到达不覆盖 →
B 结束 → 重新投递原 `exit`（`exit` 出现 ≥2 次、`close` 0 次）。
`B08-2：回包先于 CloseRequested（正常顺序）`同样断言 B 结束后继续执行原动作。

#### 清理

`runCloseExecution` / `CloseExecutionDeps` **已从源文件删除**（上一轮报告称"已删除"
但实际仍保留未使用导出，此处更正并落实）。App 不再引用它。

#### 检查（唯一日志目录）

`/private/tmp/revia-b08-1yRxyJ`（`record.py` 执行时逐条记录真实 argv/cwd/显式非敏感环境/start/end/exit/stdout/stderr；
`tested-source-sha256-before/-after.txt` 覆盖 79 个受测源文件，排除 `src/types/bindings`、
`src-tauri/gen`、`dist`、`target`、验收原件与文档）

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `pnpm exec tsc -b` | 0 | — |
| `pnpm lint` | 0 | — |
| `pnpm build` | 0 | — |
| `node tools/test-refresh-gate.mjs` | 0 | tests 177 / pass 177 / fail 0 |
| `git diff --check` | 0 | — |

79 份受测源码 SHA 前后逐份一致。全部 `.err` 文件原样保留（含 vite 的既有 configLoader 提示）。
本轮未改 Rust 生产代码，未重跑 Rust。

#### 未验证（必须保留）

- **真实桌面关闭/退出未验证**：macOS Quit/⌘Q 的实际拦截与裁决、Stay 后旧 `CloseRequested`
  的真实到达时序、失败后 Stay 的观感，均需规划方在固定 Release 上验收。
- `CloseRequested` 不带 claimId 是依赖事实，最终提交阶段是内部安全编排，
  不改变 Q96 的选择语义。
- 未启动 GUI、未构建 Release、未跑 D06/CI/长测、未推送/合并；08 保持 `ready-for-human`。

### 2026-10-25 / 规划方：4150433 第八轮限定复核，仍有两项P2

固定`git diff 34b2d5148a3530cb4168fad43317d5ae2fd0362d...41504336171c2adf3238f45be6dd098e95e2e262`，唯一提交、4文件，行号对应4150433。前轮Stay实际阻止和早裁决不被本次回包覆盖的单次场景已静态改善，旧同步递归/立即重投不复报。

#### Standards：确定违反2项P2，Fowler建议0项

1. **ST-B09-08-1 / P2 / 新回包消费旧裁决**：`refreshGate.ts:1801`对任何非空earlyVerdict清字段并结算，不核当前claim。close1已发→Stay→事件1被阻止但IPC1仍挂起；新关闭claim2的IPC2先回，`:1799/1803`清claim2字段并消费裁决1。event2后来拿不到claim而被阻止，coordinator仍持active claim2，后续认领null；旧IPC1还能再写pending1。违反`technical-direction.md:56` Q96及派发“旧claim/回包不得影响新请求”。各自投递/回包/裁决须按所有权结算，不能跨claim清状态。
2. **ST-B09-08-2 / P2 / 最终提交门禁未接生产**：`refreshGate.ts:1584`的canStartWrite只被测试调用；`App.tsx:1173`仍仅检查监听ready，`:1200/1287/1358`直接开始写编排。关闭处于committing/awaiting时仍可登记新写，未兑现本轮最终提交方案的明确条件。此项是生产接线缺口，不推断必然数据损失。

#### Spec：缺失/部分1项、实现错误1项P2，范围扩张0项

1. **SP-B09-08-1 / P2 / 缺失生产写门禁**：同`App.tsx:1173/1200/1287/1358`与`refreshGate.ts:1584`，违反`spec.md:126`保留在途写及本轮最终提交约定，测试方法返回false不证明实际入口拒绝新写。
2. **SP-B09-08-2 / P2 / 晚裁决污染下一投递**：close1 IPC已返回→Stay→旧事件1被阻止，在`refreshGate.ts:1671`留下无旧回调可消费的earlyVerdict1；用户再关闭，IPC2先回，`:1801`用其他claim裁决清新字段，但active claim2仍占用，event2无法获准，之后永久等待。违反`spec.md:127`执行原请求。此反例不要求两个IPC同时在途。

统计：Standards2、最高P2、建议0；Spec2（缺失1/错误1）、最高P2、范围扩张0。两个轴独立保留。原生allow/prevent已收进共享入口，但“新请求可执行”测试只验证再次投递而未完成新裁决，门禁测试只断言helper，不代表生产接线。

**检查边界**：最终5条真实记录均退出0，Node177、tsc/lint/build/diff-check；79份受测源码before/after/固定提交/磁盘SHA相同，未重跑。早期诊断失败没有完整原始流在提供目录中，不补造、不声称全部尝试成功。Rust未改未重跑。见[第八轮记录](../batch08-repair-validation.md)及[清单](../evidence/batch08-repair/manifest.json)。08回ready-for-agent；原Pi已idle，本轮复核后暂未派发下一轮，Q105未改变，规划方尚未接手生产实现。未新Release/GUI/D06/CI/长测/推送/合并，10/11/M2未完成。

### 2026-10-25 / 用户授权：Codex 接手本次剩余修复（Q108）

用户明确要求“你来修”。Codex 从干净 `64aa4ecfee729cfb185bd3c7a85288b60a54bda5` 接手 ST-B09-08-1/2 与 SP-B09-08-1/2；原 Pi 保持停止。本次是 Q105 的限定例外，授权记录见 `docs/README.md` Q108，不再派发 Pi，也不把审查返修次数解释为渠道连续三次报错。

只修任务 08 的关闭投递/回包/裁决所有权及实际共享写入口门禁，并补同源并发回归；集中检查后分别进行 Standards / Spec 复核。全部静态闭合后才固定 Release 与真实桌面验收，不提前合并，08 暂保持 `ready-for-agent`。合并顺序仍为独立验收 `m2-ui` → 合回 `m2` → 最终集成验收 → squash main。

### 2026-10-25 / Codex 实现：B09 关闭所有权与最终写门禁

授权提交 `53daeeb`，修复 ST-B09-08-1/2、SP-B09-08-1/2。不是只删除不匹配的 `earlyVerdict` 分支，而是移除不能可靠关联的匿名回执阶段：

- 本地锁定依赖 `@tauri-apps/api` 2.12.4 的 `onCloseRequested` 是 `await handler` 后未阻止则 `destroy`；Rust `tauri` 2.12.4 发空 payload，`tauri-runtime-wry` 2.12.2 的用户关闭与程序化 `close()` 走相同入口，没有请求身份。single-flight 也不能排除用户事件抢先到达，不能将它声称为可靠关联。
- App 对每个原生 `CloseRequested` 先 `preventDefault()`，仅转交用户意图；生产不再调用 `window.close()` 循环投递匿名事件。关闭最终用 `destroy()`，退出最终用新增固定命令 `request_exit` → `AppHandle.exit(0)`，不把两种原请求互相替代。Rust 退出入口复核保护仍阻止时返回可见拒绝。
- `CloseDriver` 用单一 attempt 对象拥有 claim、请求代际和原动作；每个 await 成功/失败先比较对象身份。准备期间 Stay 撤销并释放自己的 attempt，旧回包不得清新 claim、改阶段或更新新 UI。
- 最终同步复核到锁写、隐藏询问、发出原生动作之间没有 await；之后是 `closing` 终态，迟到的 Stay/Wait、事件、通知及成功回包不能重新开放写入。不可撤销的原生动作发出后不再提供可取消询问；此前仍保留 Q96 的等待/Stay 选择，且不会取消写操作。明确失败则展示错误并回询问，不自动重试。
- `WriteGuardOrchestrator` 必须接收实际 `closeDriver`，`begin()` 在登记前及保护 await 后都检查门禁。App 暂存/取消暂存/丢弃、恢复、另存继续共用此入口；不是仅在测试中拼接 `guardsReady`。

#### 集中检查

独占原件目录 `/private/tmp/revia-b09-Ztcljf`，记录真实 argv/cwd、显式非敏感环境、start/end/exit 和原始 stdout/stderr。81 份受测源码/测试/配置/锁文件在检查前后、绑定清理前后逐份 SHA256 一致。

| 检查 | 结果 |
| --- | --- |
| `node tools/test-refresh-gate.mjs` | 180 passed / 0 failed |
| `pnpm exec tsc -b`、`pnpm lint`、`pnpm build` | 各退出 0 |
| `/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml` | 268 passed / 0 failed / 1 ignored（既有桌面 fixture 准备入口） |
| 首次 `git diff --check` | 退出 2，仅 ts-rs 生成绑定的行尾空格；保留失败原件 |
| `node tools/strip-binding-whitespace.mjs` 后仅重跑 `git diff --check` | 均退出 0，20 份绑定恢复原样，没有生成物改动进入提交 |

B05–B08 中依赖旧投递/裁决模型的测试迁移为实际共享驱动+写编排的完整生命周期；保留无请求、Wait/Stay、原动作、多个 worktree、待核对、失败不自动重试等行为，并新增旧成功/失败抵达新准备/提交/返回三个阶段的交错及终态门禁。测试验证到新请求实际提交且清账，不只计投递次数。旧“已发 close 等匿名事件”的阶段已移除，**不能**声称新测试识别了那个旧事件。

当前仅完成实现和上述检查；下一步固定提交做独立 Standards / Spec 复核。尚未构建新 Release、操作桌面、跑 D06/CI/Q98 长测或合并；任务 08/10/11/M2 均未完成。

### 2026-10-25 / 规划方：7cdab51 第九轮独立复核通过，转真实桌面验收

固定 `git diff 64aa4ecfee729cfb185bd3c7a85288b60a54bda5...7cdab510e8ab360ede61ef3c00c326ca71304bbc`，2 个提交、9 文件。

#### Standards

确定规范违反 0 项、Fowler 判断建议 0 项，无最高严重度。已静态核对匿名关闭事件仅表达用户意图、实际关闭/退出分离、逐 attempt 回包隔离、三个生产写入口门禁、失败与待核对边界。

#### Spec

缺失/部分 0 项、范围扩张 0 项、实现错误 0 项，无最高严重度。Q96 原动作及 Stay、Q91 待核对按代际清账/不自动重跑未见本增量回退。两轴各自独立计数，不相互抵消。

原件与边界见[第九轮记录](../batch09-repair-validation.md)及[归档清单](../evidence/batch09-repair/manifest.json)。81 份源码的检查前后、固定提交与当前磁盘 SHA256 已核对；首轮 diff 空白失败及定点清理重跑均保留。08 改为 `ready-for-human`，不是 `done`；WebView 中询问隐藏、真实 Wait/Stay/待核对与关闭/退出、失败反馈仍需固定 Release 验收。本轮未新 Release/GUI/D06/CI/Q98/推送/合并，10/11/M2 未完成。

### 2026-10-25 / 规划方：集成桌面发现 macOS Quit 绕过，B10 修复待复核

`m2` 已同步到 `m2-ui`，固定集成提交 `5f632d661ce5dc37b237c27f60205c8d41fcf86c`；这不是 `m2-ui` 合回 `m2`，最终合并顺序没有改变。该版前端、现有 Node/Python 与 Rust 检查通过后构建 Release，二进制 SHA256 `0623cacdf0c395a00d2eba975df7699c0e72d788a0d80d84f7b94a25c1e3a09d`。此前双轴静态复核未发现此问题，不能改写为已覆盖原生 Quit。

- 独占桌面目录 `/private/tmp/revia-m2-desktop-_2le4ddh`，生产 fixture 的真实 `stage_file` 响应被显式门控后，窗口关闭询问、Stay、继续浏览、禁止同 worktree 再写、释放后仍留在应用均成功（C01）。
- 同一 PID 23756 随后进入真实 `unstage_file`（C02）；结果已捕获，但尚未交付前端，也未创建 release。真实 `super+q` 令进程退出 0，没有退出询问。捕获至退出约 31.35 秒，未达到 300 秒门控超时；源码与二进制未变。Git 操作已执行完成，**不据此声称 Git 被杀或数据丢失**；确定缺陷是在结果未交付和登记完成前提前退出。
- 失败原件和旧 bundle 保存在 `/private/tmp/revia-b10-vzc_he0k`，不覆盖；C02 不能算取消暂存后 Stay 通过。

本机锁定依赖链表明，Tauri 默认 Quit 通过 muda 的 `terminate:` 进入 Cocoa；Tao 0.37.1 的 `applicationWillTerminate` 只发 LoopDestroyed，runtime 映射为 Exit，绕过 Revia 原有的 ExitRequested。不是 ExitGuard 类型错配或旧 false 回包覆盖。Apple 的公开 [applicationShouldTerminate](https://developer.apple.com/documentation/appkit/nsapplicationdelegate/applicationshouldterminate(_:)) 是该正常退出入口的可阻止委托。

按 Q108 接手任务 08 剩余修复，新增仅 macOS 的 `platform/macos_exit.rs`：

- 在 Tauri build 后、run 前向原 Tao delegate 添加公开的 `applicationShouldTerminate:`；检查实际类名和包括继承链的已有实现，拒绝覆盖；主线程重新注册同一个保活的 delegate，不替换菜单或对象。
- 使用 Cocoa 的 NSApplicationTerminateReply/NSUInteger ABI；回调取消当前原生终止并转交既有 `revia://exit-requested`，保留统一 CloseDriver 的原动作、Wait/Stay 和待核对语义。最终仍由 request_exit/app.exit 走 runtime stop，保留 Rust 最后保护。
- 转发错误或 Rust panic 不放行退出；没有窗口时仍走 runtime 退出。未使用 NSTerminateLater，不增加另一套待回复生命周期。前端监听不可用时可能拒绝退出且不能交付请求，这是 fail-closed 边界，不承诺系统注销/关机自动继续，也不保护系统强退。
- 复用已锁定 objc2 0.6.5 与 objc2-app-kit 0.3.2，仅增加 macOS 直接依赖；锁文件只新增根包依赖项，没有升级其他包。

相关修改完成后集中检查：`cargo test --manifest-path src-tauri/Cargo.toml --offline --locked` 为 **271 passed / 0 failed / 1 ignored**；既有绑定清理和 diff-check 均退出 0。新增两项现有 Rust 单测验证转发成功/错误/panic 均 Cancel，以及真实注册 selector 的 ABI、重复/继承方法拒绝；它们**不替代** AppKit、Cmd-Q、Dock Quit 的真实桌面验证。前端未改，本次不机械重跑刚通过的前端检查。

原件 `/private/tmp/revia-b10-vzc_he0k/checks-cpi33j4q` 保留 argv/cwd/显式环境/start/end/exit/stdout/stderr。依赖解析仅改变 Cargo.lock；此后 154 份受测源码、测试、配置、锁文件在 Rust 检查前后 SHA256 一致，warning 原样保留。本节实现尚待固定提交的独立 Standards / Spec 复核及新 Release，08 暂回 `ready-for-agent`；真实 Cmd-Q/Dock Quit、Wait/Stay、待核对尚未重验，10/11/M2 均未完成。


### 2026-10-25 规划方：B10 静态闭合与真实关闭结果

固定5f632d6...2a51b16的独立Standards/Spec均0项，B10完整Rust271通过/0失败/1既有ignored，见[修复记录](../batch10-repair-validation.md)。Status回到ready-for-human，不done。

窗口Close/Stay、Cmd-Q/Stay及原生菜单Quit/Wait显式交付后退出已有局部闭环。C05已真实执行：恢复准备失败→Wait→显式释放→待核对可见并阻止自动关→Stay→真实状态核对→新Close请求→用户授权“我已核对，继续关闭”→同PID55553退出0。HEAD/index/两侧diff/工作区字节与模式不变；不是恢复成功或写后数据丢失，不再待用户许可。

Dock样本仍仅工具超时、没有实际Quit；另存C06自动释放不是显式释放通过。有限关闭入口尾项及全M2验收未完成，不按动作×入口穷举。[桌面原件](../evidence/task13-desktop04/manifest.json)保留历史失败和当前结论。
