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
