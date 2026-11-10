# M10 报告：三项架构深化

Status: done
日期：2026-11-10
最终代码：`3622879`；前端最后变更：`53ec183`。

## 规划方结论

Worktree 激活与刷新、运行环境、生成提交信息三个 module 已完成深化，任务 01–05 均已完成。正式返修使用一次，后续由规划方接手；最终增量 Standards / Spec 均无剩余意见，macOS 桌面验收通过，详见 [审查记录](review.md)及下方证据。

两个真实桌面问题已修复并补回归：生成期间切换会使取消恢复串写草稿；环境重载新会话的快照版本归零导致刷新拒收。两项修正均已完成可见窗口复验。

## 最终实现

- `WorktreeRefresh` 集中激活、监视登记、读取接收和写后原目标核对；保留审阅与快照判定 module 的 depth。
- `WorktreeEnvironment` 集中会话创建、环境安装、替换、等待和失效；慢 shell/Git 读取在生命周期锁外完成，登记前核对租约。拒绝重载时不部分作废会话。
- `CommitMessageGeneration` 与提交输入共享归属。每次生成写入携带 worktree 和输入代次；`resetTarget` 先恢复、保存旧目标，再重置。异步确认读取最新模式，更改列表 amend 发送说明完整。
- 快照版本在进程内递增，并在开始真实读取前分配；分配与会话失效共用短临界区，Git 读取在锁外执行。发布前拒绝失效会话，旧会话迟到结果不会获得新会话之后的版本。原有 activation、写锁和 diff 指纹校验继续有效。

## 规划方检查

证据根：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m10-evidence-ln6xfi08`。各子目录的 JSON 记录源码、命令、退出码、耗时，配套 `.stdout` / `.stderr` 保留原始输出。

| 固定版本 / 证据子目录 | 检查与结果 |
| --- | --- |
| `8300bfa` / `planner-final-u9d0_erb` | `pnpm exec tsc -b`、`pnpm lint`、`pnpm build` 均退出 0；五个相关 Node 套件 263 通过；完整 Rust 952 通过、0 失败、1 忽略；release 和 app 打包通过 |
| `53ec183` / `planner-draft-fix-comz2fr1` | `pnpm build`（含类型检查）、lint、两个提交相关 Node 套件均退出 0；Node 23 通过、0 失败（含新增两项）；release 和 app 打包通过 |
| `19e6e95` / `planner-revision-fix-szxjq1s5` | 完整 Rust 953 通过、0 失败、1 忽略；随后静态复核发现旧会话迟到读取问题，继续修正 |
| `3622879` / `planner-revision-order-shxe5eb5` | 完整 Rust 34 个目标，954 通过、0 失败、1 忽略；release 与 app 打包完成，三项均退出 0 |

后续仅重跑受影响检查：`53ec183` 之后未改前端，`3622879` 上完整 Rust 已重新验证共享读取契约。唯一忽略项 `prepare_task05_desktop_fixture` 是需要显式运行的桌面数据准备入口。

lint 有既存警告，涉及 mockup、`terminalSessions.ts`、`visibleChangelistSelection.ts`、`test-refresh-gate.mjs`、`write-updater-config.mjs`；Vite 有配置和 bundle 大小警告。没有为本轮扩大清理范围。实施方曾用不适用的 `node --test tools/test-*.mjs` 得到 8 个失败，原始输出与原因保留在下方历史报告；最终检查不冒称全部 Node 套件通过。

## macOS 桌面记录

使用固定源码的 release `.app`、本轮独占创建的 `desktop-fixtures/alpha` 与 `beta`，最终复验数据为 `planner-draft-fix-comz2fr1/app-data`。AI 只发送合成 fixture 到本机 `127.0.0.1` 模拟服务；服务不记录请求正文，未配置或输入真实密钥。

`53ec183` 的可见窗口已确认：打开与 A→B→A、首次发送地址/范围确认、已有文字替换确认、流式完成、一步撤销、显式取消、生成中切换后 A/B 草稿各自恢复、手动刷新后新增文件自动出现。对应原始截图位于 `planner-draft-fix-comz2fr1`，`desktop-launch.json` 记录 PID、二进制 SHA256 与独立数据目录。

已归档代表截图：[首次发送确认](images/desktop-first-send.png)、[B 草稿保留](images/desktop-beta-preserved.png)、[返回 A 恢复原文](images/desktop-alpha-restored.png)、[取消恢复原文](images/desktop-cancel-restored.png)、[手动刷新后监视生效](images/desktop-watch-refresh.png)。

2026-11-09 22:10 启动的最终桌面实例 PID 为 `90017`；当时电脑操控工具返回 Mac 已锁定、自动解锁失败，复验暂停。用户于 2026-11-10 确认已解锁后，使用相同独立数据目录重新启动，PID `9453`，实际启动时间为 19:42:41（Asia/Shanghai）。重新核对二进制 SHA256，仍为 `e2e7f08e51b74deae0f0639bf89536be40e975813b1e10c6c54a278f3418f30e`，对应最终代码 `3622879`。

本次在可见窗口连续手动刷新三次，随后在设置中点击“重新加载环境”，确认“已重新加载环境”后返回 alpha。在该 fixture 独占新建 `watch-reload-20261110-f46acd70.ts`，没有再次手动刷新，文件自动出现在列表中，未跟踪数量由 3 变为 4；提交草稿仍为 `alpha final draft`。最后的环境重载与监视复验通过，[代表截图](images/desktop-watch-environment-reload.png)已归档。

本次原始证据位于 `desktop-final-bu6l2f5h`：`desktop-verification.json` 记录结果、实际进程启动时间和截图 / AX 的 SHA256，`desktop-reload-watch.json` 记录 fixture 写入，另有设置完成和文件出现的 AX、原始截图及进程输出。`desktop-launch.json.started` 沿用了旧实例的启动时间，新实例实际时间以 `ps` 核对后写入的 `desktop-verification.json` 为准；旧记录保留。

验收应用与本机模拟 AI 服务均已退出。最终 `.app` 已复制到同一证据子目录并核对二进制 SHA256，清理实施 worktree 后仍可追溯；过程见 `desktop-cleanup.json`。

## 未验证范围

Windows 真实桌面与真实 AI 服务未验证，本机检查不代替它们。未重新执行 M8/M9 历史性能验收或长期运行，不声称重构带来性能提升。未推送、未开 PR、未打标签或发布新版本。

## 实施方返修报告（历史记录，642ea16）

以下保留当时的实施报告；其中“尚未复核”“锁内解析”等表述仅代表当时状态，最终结论以本页上半部为准。

### M10 实施报告：Worktree 刷新、执行环境与生成提交信息

Status: ready-for-review
日期：2026-11-09
实施方：Pi（Paseo）
被审源码：`d8babd7bcc4829889db1ca2ca965a81fdf84d47f`
审查/返修文档：`e8d45e9`（`review.md`、`repair.md`、`issues/01–03` 的 Comments）
固定实现提交：`642ea16b2a145fba62d5f64348d54a6c64274e24`
证据根目录：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m10-evidence-ln6xfi08/repair-20261109-172904`

#### 结论

任务 01–03 的**自动验收项**已按 `review.md` 与 `repair.md` 修复并补齐回归，`issues/01–03` 的验收清单已勾选；证据见上面的证据根目录（`commands.md` 是命令与退出码索引）。

本报告只代表实施方的自动验证：规划方的增量复核（任务 04）与真实桌面验收（任务 05）**没有做**，也不声称完成；没有推送、没有开 PR、没有合并 main、没有打标签或发布。

#### 本轮改动

##### SP7 · 写后核对与界面发布分离（`src/lib/refreshGate.ts`、`src/lib/worktreeRefresh.ts`）

- `SnapshotGate.acceptTarget(envelope)` 不再比较激活代号，只按该 worktree 自己的版本单调性判定“真实读取是否核对成功”。
- `WorktreeRefresh.readInto` 在 `target` 模式下把发布资格单独判：仍是当前激活目标**且**激活代号未变才发布。于是写后核对期间切换 worktree 不会让 `verifyWrite` 失败，A→B→A 之后同一份结果也不会被画到新界面。

##### SP2 · 重载的两阶段失效（`src-tauri/src/worktree_environment.rs`）

- `reload` 先在 `sessions` 锁内取**全部**会话的 `writing` 锁，再逐个看标志：`WriteGuard` 并不持有这把锁（写进行中时锁是空闲的、只有标志为 true），所以取锁不会等待，标志才是权威判断。
- 任一在写就整体拒绝并返回原结构化错误，此时**没有**任何会话被标记；全部通过后才统一 `mark_superseded()` 并清空会话表。重载被拒后，其他 worktree 的会话照常可写。

##### SP3 · 重载只有一个临界区（`src-tauri/src/environment.rs`）

- `EnvCoordinator::reset` 在 `slots` 锁内完成“作废 + 唤醒等待者 + 清缓存/记忆 + 清槽位”；`publish()` 拿同一把锁，迟到的发布只能等清除完成，并因代次失效被拒，不会把旧缓存写回来。

##### SP4 · 解析、发布与登记的顺序（`src-tauri/src/worktree_environment.rs`、`src-tauri/src/commands.rs`）

- `WorktreeEnvironment::open` 在 `AppState` 新增的会话生命周期锁内串行“解析 → 发布 → 登记”；发布失败返回结构化错误，不再把无效代次的会话交给调用方。
- 后台安装（`install_environment`）在同一把锁内先 `is_lease_valid` 再改动会话表，重载之后迟到的安装不会把旧会话登记回来。
- 命令层原先散落的会话解析/安装/失效流程改由 `WorktreeEnvironment` 承担，重复的会话锁错误构造收拢到 `commands::session_lock_error`。

##### ST2 · 生产等待点作为接缝（`src-tauri/src/environment.rs`、`src-tauri/tests/environment_concurrency.rs`）

- 新增 `EnvCoordinator::waiters(worktree_id)`：`wait()` 真正进入阻塞前计数（RAII 递减）。
- 三个等待类测试（完整环境安装前的写入、被替换会话、重载唤醒）改为等这个计数，不再用“线程已启动”的握手；轮询只用于观察接缝，断言仍落在生产行为上，观察不到就失败。`reload_rejects_write_waiting_for_environment` 里的 200ms sleep 已删除。

##### SP1 / SP5 / SP6 · 生成请求的生命周期（`src/lib/commitMessageGeneration.ts`、`src/components/app/CommitComposer.tsx`、`src/App.tsx`）

- `control()` 只生成一次控制面对象（引用稳定），App 用 `useMemo` 取一次；生成状态快照作为单独 prop（`generationView`）传入。界面重新渲染不再换引用，取消只由 worktree 切换、卸载与显式意图触发。
- `captureIntent(mode, message)` 在点击那一刻定下目标、模式与当时文字；`intentStillValid(intent, mode)` 在配置检查与两级确认回包后用目标代次 + 模式深比较（含更改列表勾选）复核。取消确认会清掉暂存意图。
- `displayedError = generationError ?? aiError` 同时用于判断与渲染，生成失败仍显示翻译文案与图标。

#### 检查结果（均为固定实现提交 `642ea16` 上重跑）

| 检查 | 结果 |
| --- | --- |
| `pnpm exec tsc -b` | 退出码 0 |
| `pnpm lint` | 退出码 0（仅 `docs/milestones/m7/mockups/...` 既有 mockup 警告） |
| `pnpm build` | 退出码 0 |
| `cargo test`（全套 34 个测试目标） | 退出码 0，34 个 `test result: ok`，0 个 FAILED |
| `node --test tools/test-refresh-gate.mjs` | 退出码 0 |
| `node --test tools/test-worktree-refresh.mjs` | 退出码 0（含 SP7 新用例） |
| `node --test tools/test-commit-generation.mjs` | 退出码 0（含 SP1、SP5 新用例） |
| `node --test tools/test-log-model.mjs` | 退出码 0（接线断言随 `acceptTarget` 签名更新） |
| `node --test tools/test-commit-state.mjs` | 退出码 0 |
| `cargo test --test environment_concurrency` 连续 10 次 | 全部退出码 0，0.66–0.73s，无挂起 |
| `node --test tools/test-*.mjs`（总入口） | 退出码 1：`tests 567 / pass 558 / fail 8`，见下 |

新增的回归：

- `tools/test-worktree-refresh.mjs`：写后核对原目标期间切换 worktree 仍算核对成功、但不发布到新界面；含 A→B→A 与版本倒退。
- `tools/test-commit-generation.mjs`：控制面引用稳定不取消在途请求；点击时捕获的完整意图在等待、确认与切换后失效。
- `src-tauri/tests/environment_concurrency.rs`：`rejected_reload_leaves_every_session_usable`、`reload_during_sync_resolve_does_not_resurrect_old_session`。
- `src-tauri/src/environment.rs`：`reset_never_leaves_stale_cache_behind_a_publish`、`lease_validity_tracks_reset_and_replacement`、`waiters_counts_threads_blocked_in_the_wait`。

##### 总入口的 8 个失败是专用运行条件缺失

`node --test tools/test-*.mjs` 不是适用的总入口：8 个失败都在本轮未改动的套件里，且原因与产品行为无关。

- 7 个在 `tools/test-m6-file-history.mjs`：用例读 `process.env.REVIA_M6_ERROR_FIXTURE + ".blame-options.json"` 等真实 Git 响应，本轮未设置该变量，报 `ENOENT: 'undefined.blame-options.json'`。
- 1 个在 `tools/test-m6-i18n.mjs`：该套件的模块钩子没有 `.css` loader，`src/components/m7/glass/GlassSurface.css` 触发 `ERR_UNKNOWN_FILE_EXTENSION`。

这两类失败在本轮开始前就存在；**基线复现的原始日志没有保存**，本报告不声称有。相应原始输出见 `final-node-all.stdout` 第 175–190 行与第 599 行起的 failing tests 段。

#### 未验证 / 未完成

- 规划方的增量代码复核（任务 04）与真实桌面验收（任务 05）未做，`issues/04`、`issues/05` 保持未勾选。
- 上述 8 个失败的两个套件在“设置 `REVIA_M6_ERROR_FIXTURE`、补 `.css` loader”的正确条件下是否通过，本轮未验证。
- 打包、release 构建、桌面测量与发布均由规划方接手，本轮未执行，也未触碰真实桌面。
- 未推送任何远端、未开 PR、未打标签、未合并 main、未修改 Git 配置。

#### 遗留与注意

- `EnvCoordinator::reset` 之后，正在等待的线程会立刻醒来并拿到“已作废”，不会等到超时；`publish` 在重载之后一律失败，调用方据此丢弃结果。
- `WriteGuard` 不持有 `writing` 锁（只置标志），任何“会话是否在写”的判断都必须读标志而不是 `try_lock` 结果，后续改动不要退回 `try_lock` 判忙。
- `SnapshotGate.acceptTarget` 不再看激活代号：调用方若要发布到界面，必须自己确认当前激活目标与代号。
