# 02：Worktree 运行环境

Status: done
Blocked by: None（可立即开始）

## What to build

交付[规格](../spec.md)中与Worktree 运行环境对应的完整行为。前三项由同一个 Pi 会话实施，04、05 由规划方接手；无依赖项可独立验证，实施顺序优先 01 → 02 → 03。

## Acceptance criteria

- [x] 环境 module 拥有临时可读、完整解析、会话安装替换、就绪发布及重载失效的完整协议。
- [x] 完整环境安装先于等待写入继续；Git 改变后旧会话写入拒绝，不自动重放。
- [x] 重载使旧解析的安装、缓存、ready 和通知失效；同 worktree 并发首次打开只发布有效会话。
- [x] 保留手动 Git 优先、失效记忆回退、按 worktree 隔离、写锁与结构化错误。
- [x] 通过现有生产入口、真实 Git fixture 和受控 ShellResolver 验证上述并发行为。

## Comments

### 2026-11-09 · 规划方收尾

- `8300bfa`：慢 shell/Git 读取移出全局生命周期锁，登记前在锁内重新核对租约；下面实施方记录的“锁内串行解析”已被替换。重载回归验证实际完成顺序，另验证慢打开不阻塞已有 worktree。
- `19e6e95`、`3622879`：桌面发现重载新会话的快照从头计数，与前端跨激活水位冲突。改为进程内递增版本，取数前分配并与会话失效互斥；旧会话发布前再核资格，迟到内容不会获得新会话之后的版本。
- 新回归 `reloaded_session_publishes_revisions_above_the_previous_session` 与 `superseded_read_cannot_publish_after_the_replacement_snapshot` 通过 AppState、真实 Git 临时仓库及受控发布阶段验证。最终增量两轴均无剩余意见。

### 2026-11-09 · Spec 审查（af0a609...d8babd7）

- SP2 / P1：worktree_environment.reload 遍历会话时先 mark_superseded，再发现另一会话 writing 返回拒绝；已标记会话仍在表中，后续写操作永久被拒绝。拒绝重载不得部分提交；在全部资格检查完成前不改变任何会话，检查和提交间也不得有写入窗口。
- SP3 / P2：environment.reset 在 slots 锁外先清缓存/记忆；publish 可在间隙重新写入旧缓存，随后只清 slot。重载作废与清除必须和发布在同一受控临界区，保留一致锁顺序。
- SP4 / P1：resolve_now 忽略租约发布失败，commands.session 随后仍登记会话。同步解析期间重载可使旧环境会话在重载后重新出现；空 slot 又被 wait_ready 视为就绪。旧代次不能登记/安装，调用方获得结构化失效结果，不自动重放写入。

### 2026-11-09 · Standards 审查（af0a609...d8babd7）

- ST2 / P2：environment_concurrency 在 acquire_write_lock 之前发 started，不能证明线程已经进入真实等待处。需要在生产等待入口确认阶段后再释放 resolver 或 reload；只用启动握手、不控制实际等待会产生假阳性。
- ST3 / P3（判断性意见）：commands.session 仍组合 prepare / resolve_now / open / set_env_gate / settle，并维护 sessions 与失效记忆；WorktreeEnvironment 持整个 AppState，生命周期知识仍分散。可随 SP4 修正把会话创建及有效性提交收进单一入口，减少调用方须知道的顺序；不单独扩大清理范围。

### 2026-11-09 · 返修（实施方，SP2–SP4、ST2）

- **SP2 两阶段失效**：`WorktreeEnvironment::reload` 先在 `sessions` 锁内取**全部**会话的 `writing` 锁再判断——`WriteGuard` 并不持有这把锁，写进行中时锁是空闲的、只有标志为 true，所以拿锁不会等待，标志才是权威判断。任一在写就整体拒绝，此时没有任何会话被标记；全部通过后才统一 `mark_superseded` 并清空会话表。
- **SP3 单一临界区**：`EnvCoordinator::reset` 在 `slots` 锁内完成“作废 + 唤醒等待者 + 清缓存/记忆 + 清槽位”，`publish()` 拿同一把锁，迟到的发布只能等清除完成并因代次失效被拒。
- **SP4 同步解析与重载的顺序**：`WorktreeEnvironment::open` 在 `AppState` 的会话生命周期锁内串行“解析 → 发布 → 登记”，发布失败返回结构化错误；后台安装也在同一把锁内校验租约（`is_lease_valid`），重载不会与“登记旧会话”交错。
- **ST2 生产等待点**：`EnvCoordinator` 新增 `waiters(worktree_id)`，在 `wait()` 真正进入阻塞前计数（RAII 递减）。三个等待类测试改为等这个计数，不再用线程启动握手；轮询只用于观察接缝，断言仍落在生产行为上。
- 回归：`rejected_reload_leaves_every_session_usable`（拒绝重载后另一 worktree 会话仍可写、会话表未清空）、`reload_during_sync_resolve_does_not_resurrect_old_session`（重载等打开完成，旧会话不复活）、`reset_never_leaves_stale_cache_behind_a_publish`、`lease_validity_tracks_reset_and_replacement`、`waiters_counts_threads_blocked_in_the_wait`。
