# M2 剩余任务整包实施与总体验证

依据Q106（2026-10-24用户要求“让paseo任务一起做，做完总体验证”）。本文件替代此前“整包队列、逐阶段放行”的临时说明；只调整本轮交付批次，不改变产品规格、安全边界或完成门槛。

## 实施批次

同一原Paseo/Pi会话在m2上完成剩余可实施工作，内部按代码依赖推进，不在每个任务后等待规划方单项审查或桌面验收。已实现但待验收的前置项可供本批集成使用；任务Status保持真实，最终总体验证通过前不标done。Blocked by仍记录产品依赖，不删除或伪造前置项完成。

1. 以任务07最新Comments修复剩余真实App接线和完整批次错误资格。
2. 完成任务08写入期间安全关闭/退出，使用05已实现的恢复生命周期；涵盖暂存、取消暂存、丢弃、恢复和跨worktree在途写入。正常完成/待核对/强退边界遵循spec，不因关闭杀Git或重跑写入。
3. 在同一批内修复05/12及其他已实现功能的确定集成缺陷，保留历史失败，不扩大范围。已完成任务不无故重写。
4. 功能实现稳定后准备任务13浅色/深色静态效果图，覆盖该任务规定的普通和异常状态。**用户确认效果图后才实现美化**。等待该确认时可完成任务10已有功能矩阵、报告与验证准备，不把未实现的13或最终10/11标完成。
5. 用户确认后，同一Pi继续完成13实现和10最终集成检查/报告，统一交规划方复核。返修次数与规划方接管按Q113；不恢复逐任务停等模式。

## 交付与验证

- 每个逻辑任务可独立新增提交，但本批一起交付。先完成相关修改再集中必要检查；核心流程完成后运行现有完整Rust及前端检查一次，修复失败后只重跑受影响项。检查范围与受测源码、命令、真实退出码、stdout/stderr、前后SHA对应；每轮原件独立保留。
- 规划方收到整体代码后统一两轴审查，问题成批写回Comments交原Pi；全部通过后固定Release/source/binary SHA，进行完整真实桌面、D06、最终同进程可见窗口主动30分钟Q98长测及最新双平台CI。旧版结果不替代新版证据。
- 任务11和最终合并仍由规划方执行；全M2通过后才squash main并处理worktree。M2未完成时不标完成，不推main/强推/tag/PR/发布。

## 执行边界

所有命令显式cd `/Users/garland/.codex/worktrees/revia-m2/Revia`，只提交m2，main/用户图标/`.codex/`/旧fixture保持原样。使用正常编辑、精确暂存和独立提交，保留全部失败日志，不checkout/restore/reset文件或amend。测试沿用现有体系，Git验证仅自建临时仓库及独立数据目录。

Pi负责实现、检查和静态效果图源文件，不启动GUI、不跑Release/D06/CI/长测、不推送/合并。规划方推送仅现有SSH origin正常推m2，不HTTPS、不改凭据/Git配置。原Pi会话保持，模型回退链与三次独立渠道失败的换模规则按[Q109–Q113](../../README.md)。

## 本批执行记录（2026-10-24）

| 项 | 状态 | 说明 |
| --- | --- | --- |
| 07 第十七轮返修 | `ready-for-human` | 恢复转发重新接线；错误归属改为批次级资格，编排收进 `runReviewRecoveryBatch` |
| 08 写入期间安全关闭/退出 | `ready-for-human` | 共享 `CloseCoordinator` + `onCloseRequested` + `CloseRequestDialog` + 窗口 close/destroy 能力 |
| 05/12 集成缺陷 | 未发现确定缺陷 | 12 的三项（测量面板仅测量模式、无 Git 版本与文件数、无 A−/A+）已实现；未无故重做 |
| 13 效果图 | **待用户确认** | 源文件在 `issues/13-mockups/`，未改生产代码 |
| 10 矩阵与报告 | 未开始 | 依赖 13 实现完成 |

### 检查（本批第一次集中）

日志：`/private/tmp/revia-batch01-sEGPJM`

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `pnpm exec tsc -b` | 0 | — |
| `pnpm lint` | 0 | — |
| `pnpm build` | 0 | — |
| `node tools/test-refresh-gate.mjs` | 0 | tests 105 / pass 105 / fail 0 |
| `cargo test`（完整） | 0 | 100 + 71(1 ignored) + 20 + 24 + 15 + 31 + 3 |
| `pnpm bindings` | 0 | — |
| `git diff --check` | 0 | — |

源码前后 SHA 逐行一致。**未启动 GUI、未 Release、未跑 D06/CI/长测、未推送/合并**。

### 下一步

等待用户确认 `issues/13-mockups/` 的浅色/深色效果图。确认后同一会话继续 13 实现与 10 最终集成检查/报告。


## 整批首轮复核（2026-10-24）

固定3199b29...82f392b：Standards6项（P1一/P2五），Spec6项（P1一/P2四/P3一），分别回写07/08/13/10 Comments。07转发和单调用错误代际已闭合；原生退出、无请求自动关闭、另存生命周期/未知结果、待核对闭环、共享批次晚加入资格、静态图越范围与10准备仍待修。全部交原Pi一起处理，不恢复逐项停等。当前两图不合范围，暂不提交用户确认；保留原稿，修订另存。六源码SHA及27份原件已核，Rust264/1ignored、Node105与前端检查exit0，不代表生产接线/桌面通过。M2未完成，无新Release/GUI/D06/CI/长测/推送。

## 整批首轮复核返修（2026-10-24）

| 项 | 状态 | 说明 |
| --- | --- | --- |
| 07 共享批次资格 | `ready-for-human` | 资格与失败展示移进真实共享批次内部，晚加入者不重新取得资格 |
| 08 原生退出 | `ready-for-human` | Rust `RunEvent::ExitRequested` + `exit_blocked` + `revia://exit-requested`；监听失败可见 |
| 08 无请求不关窗 | `ready-for-human` | 删除 `pendingIntent() ?? "close"` 兜底，只有用户选「等待」才执行原请求 |
| 08 另存生命周期 | `ready-for-human` | 占位键登记在途（外部路径不加 worktree 锁），`needsVerification` 如实传递 |
| 08 待核对闭环 | `ready-for-human` | 只有真实核对能清 pending，核对后由用户明确继续 |
| 13 修订图 | **待用户确认** | `issues/13-mockups-rev1/`（移除未排期结构、状态分开展示）；原稿保留为历史 |
| 10 矩阵与准备 | 已交付准备 | `issues/10-integration-matrix.md`：T01–T14 / D01–D08 映射、命令与真实结果、验收方法与未测边界 |

### 检查（本批第二轮集中）

日志：`/private/tmp/revia-batch02-8LEw43`

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `pnpm exec tsc -b` | 0 | — |
| `pnpm lint` | 0 | — |
| `pnpm build` | 0 | — |
| `node tools/test-refresh-gate.mjs` | 0 | tests 115 / pass 115 / fail 0 |
| `cargo test`（完整） | 0 | 100 + 71(1 ignored) + 20 + 24 + 15 + 31 + 3 = 264 通过 / 1 ignored |
| `pnpm bindings` | 0 | — |
| `git diff --check` | 0 | 首次 exit=2（`refreshGate.ts` EOF 空行），修复后 0 |

**失败原件保留**：`git-diff-check.out`（exit=2）与修复后的 `git-diff-check3.out`（exit=0）都在日志目录。

未启动 GUI、未 Release、未跑 D06/CI/长测、未推送/合并。
