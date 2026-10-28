# 01：只读历史后端与 IPC

Status: done
Blocked by: 无

## 业务目标

为提交日志、分支图和提交 diff 提供真实 Git 数据，浏览历史不改变暂存区、工作区、引用或用户配置。
依据 [规格 H01–H04](../spec.md) 与 [只读历史契约检查](../history-contract-checks.md)；后者是本任务的必读验收清单。本任务不含日志 UI 默认值、筛选语义、分支/网络写操作或 M4 合并修复。

## 实施范围

- 独立 `history` 领域与只读目标/查询生命周期；复用现有 Git 执行器与 diff 底层。
- 引用、提交分页、详情、按父提交的改动文件、提交 diff 行/左右布局/高亮读取。
- 在 `commands.rs`、`lib.rs` 注册生产命令，生成 TS 绑定并增加 `src/lib/ipc.ts` 薄封装。
- 为新公共契约与路径/分页边界补现有 Rust 测试。不创建新的前端测试框架。
- 修正继承的 `tools/test-merge-appearance.mjs` 别名导入接线，使本批能得到可信的 Node 回归结果；不改合并功能。

## 验收项

- [x] H01：固定起点、有界分页、完整父关系、空仓库/错误区分。
- [x] H02：显式父项、根提交、对象/路径身份、特殊文件正确性。
- [x] H03：两种 diff 读取与高亮固定于同一历史目标，无写入凭据，缓存/释放有界。
- [x] H04：真实 Git 回归、生产命令与 TS 绑定接线、只读状态验证。
- [x] 集中检查及原始日志齐全，报告区分已执行/未执行，全部修改提交到 `m5`。
- [x] 规划方独立审查通过。

代码与自查交付后保留待审状态；不得把完整 M5 或 M4 标为 done。

## Comments

2026-10-27，Codex：`ChangesKind` 表示当前工作区分组，不能新增一个伪 staged 历史入口。共享渲染模型如需解耦，只抽必要只读数据结构，不修改现有写入协议。

2026-10-27，Pi：代码与自查已交付，见 [实施报告](../report.md)。H01–H04 与集中检查项已勾选；独立审查未做，状态保留待审。新增 `src-tauri/src/git/history.rs`、`src-tauri/src/history.rs`、`src-tauri/tests/history_readonly.rs`（24 个真实 Git 回归），10 个命令与 ts-rs 绑定、`src/lib/ipc.ts` 封装已接线；`tools/test-merge-appearance.mjs` 的别名导入接线已修。原始日志在 `/private/tmp/revia-m5-start.zEV4wp/history-impl.C5uZkh/evidence/`。

2026-10-27，Claude（规划方审查，`d58233c...c19f4b5`）：整体结构可用，下面 8 项按 Q113 一次返修。每项修复都在 `src-tauri/tests/history_readonly.rs` 或模块单元测试里补对应回归，经 `cargo test` 运行（见 `docs/agents/local-validation.md` 第 1、2 条）；不要另写仓库外的程序。

必须修：

1. **P1 部分克隆会自动补取缺失对象。** `git/history.rs:259/367/456/615/630/644` 的 `cat-file`、`diff-tree`、`diff` 在部分克隆（`--filter=blob:none`）里遇到本地没有的对象时，会自动向远端补取，违反契约检查“不隐式 fetch 缺失对象”。修法：历史目标使用的 `Git` 额外设置 `GIT_NO_LAZY_FETCH=1`（可在 `git/mod.rs` 加一个返回带额外环境变量副本的方法，不改工作区读取）。回归：测试里用本地裸仓库（仓库自身配置 `uploadpack.allowFilter=true`）做 `file://` 部分克隆，打开一个旧提交中本地缺内容的文件 diff，应明确报错，且读取前后克隆里缺失的对象仍缺失。
2. **P1 历史 diff 受当前工作区的 `.gitattributes` 影响。** `read_changes`（numstat 的二进制判断）和 `file_patch` 读的是工作区里的属性文件。在工作区把 `*.txt` 标成 `-diff` 后，缓存淘汰再读同一个已打开的提交 diff，会从文本变成二进制，违反规格 H03“当前工作区或分支改变不改变已打开的提交 diff”。修法：这两类调用设置 `GIT_ATTR_SOURCE=<该提交 OID>`，属性取自提交自己的树。回归：读一次文本 diff → 在工作区写入 `*.txt -diff` → 读满 `MAX_CACHED_DIFFS` 份别的 diff 让缓存淘汰 → 重读，仍是同样的文本行；`commit_files` 的 `binary` 也不变。
3. **P1 会话已释放后仍可能登记新的历史目标。** `history.rs:1388–1421` 的 `open` 先读 Git、最后才登记；`commands.rs:216` `release_session` 或 `:192` `reload_sessions` 若在这段时间里完成，`close_worktree`/`clear` 找不到它，`open` 随后仍把旧会话登记成可用目标，违反规格 H03 末条和契约检查“旧实例关闭后不能认领到新实例”。修法：释放、重载时先把会话标记为已失效（或在 `HistoryStore` 里记录失效的会话实例），再关闭目标；`open` 在持有目标表锁、登记之前检查该标记，失效则明确拒绝。回归（不需要卡时序）：取得会话后先 `release_session`，再对这个会话调用 `open`，应被拒绝且目标表不增加；`reload_sessions` 同理。

本次一并修：

4. **P2 分页游标没有绑定目标。** 查询编号每个目标都从 `q1` 起（`history.rs:514`），游标只有 `qN:skip`（`:562`、`:1162`）。两个目标各读完同样大小的第一页后，A 的游标在 B 上会被当成合法续页，违反契约检查“游标跨仓库或关闭后拒绝”。现有测试里 B 还没有查询，覆盖不到。修法：游标带上目标 ID 并校验，或查询编号全局唯一。回归：A、B 各读一页同样大小的历史，A 的游标交给 B 必须被拒绝。
5. **P2 页尾提交的主题为空时整页读不出来。** `git/history.rs:322` 循环弹出所有末尾空字段，把最后一条提交的空主题（`--allow-empty-message`）也弹掉，字段数不再是 11 的整数倍而报错。修法：只去掉 `-z` 补的那一个结尾空字段。回归：最后一条提交主题为空的历史能正常分页，主题为空字符串。
6. **P2 重命名前的路径是非 UTF-8 时被标为可打开。** `history.rs:666–679` 只检查主路径；原路径非 UTF-8 时 `openable = true`，但用有损的显示名签发 `diffId`，之后打开必然被 `verify_change` 拒绝。契约要求“不支持则明确不可打开”。修法：主路径或原路径任一不是合法 UTF-8 都给 `NonUtf8Path`、`openable = false`。回归：沿用现有 `update-index --cacheinfo` + `commit-tree` 的构造方式，做一个从非 UTF-8 名重命名到 UTF-8 名的提交。
7. **P2 关闭目标会卡住其他历史请求。** `history.rs:1445–1461` 的 `close` 在持有目标表锁的同时等待目标自身的锁；而 `log_page`、`ensure_diff` 在持有目标锁期间运行 Git（最长 120 秒超时）。关闭一个正在读大 diff 的目标时，所有 worktree 的历史请求都要排队等它；`commands.rs:1543` 的 `history_close` 还直接在异步线程上等。修法：`close`/`close_worktree`/`clear` 先从表中移除并释放表锁，再去标记目标已关闭；`history_close` 放进 `spawn_blocking`。读取期间仍持有目标锁可以保留（同一目标的请求排队，本批接受），在报告里写明。此项不要求时序回归。

顺手修（成本低）：

8. **P3 二进制文件的左右对照布局报错。** `history.rs:771–772` 先调用 `ensure_texts`，它对二进制直接返回错误，所以 `:785` 返回 `binary: true` 的分支永远走不到。修法：先判断二进制再读文本。回归：在 `binary_and_mode_only_changes_are_explicit` 里补 `file_side_layout` 返回 `binary = true`、`empty_reason = Binary`。

报告补充：`GIT_NO_LAZY_FETCH` 需要 Git 2.44+、`GIT_ATTR_SOURCE` 需要 Git 2.40+，项目最低支持 2.11，低版本上这两项保证不成立，写进 report 的已知限制。修完后按 `docs/milestones/m5/paseo-batch.md` 的集中检查重跑，更新 report 的测试数与第 8 节，提交到 `m5`，说明以 `M5：` 开头。

2026-10-27，Pi：按 Q113 完成唯一一次返修，上面 8 项全部修完，改动在 `m5-rework`（从 `m5` 的 `db84e91` 分出，未合并、未推送）。回归都加在仓库里：`src-tauri/tests/history_readonly.rs` 新增 6 个用例（24 → 30），`src-tauri/src/git/history.rs` 新增 1 个解析单元（lib 205 → 206），`binary_and_mode_only_changes_are_explicit` 补了左右对照断言。逐项做了反向验证：把对应修复临时去掉后目标回归失败，7 项全部按预期失败（第 7 项按意见不做时序回归）。集中检查全部通过：`pnpm exec tsc -b`、`pnpm lint`、`pnpm build` 退出码 0，`cargo test` 448 通过 / 0 失败 / 1 忽略，12 个 Node 脚本 12 通过；原始日志在 `/private/tmp/revia-m5-start.zEV4wp/rework-U1n2KH/evidence/`（含 `negative-controls.log`）。报告新增第 9 节逐项说明，第 8 节补了 Git 版本限制（`GIT_NO_LAZY_FETCH` 2.44+、`GIT_ATTR_SOURCE` 2.40+，最低支持的 2.11 上两项保证不成立）、关闭只解除表锁不解除同目标读取锁，以及部分克隆的观察方式。没有遇到意见未覆盖的产品取舍；公共类型与 ts-rs 绑定无变化，未重跑 `pnpm bindings`。规划方独立审查、真实桌面与 Release 打包仍未做，状态保持待审。

2026-10-27，Claude：返修 `2eddd49` 审查通过，8 项都按意见修完且各有回归。独立重跑（`/Volumes/T7 Shield/project/Revia-m5`，HEAD `2eddd49`）：`tsc -b`、`lint`（7 条原有警告）、`pnpm build` 通过；Rust 448 通过 / 0 失败 / 1 忽略；12 个 Node 脚本全部通过。另做反向核对：分别去掉 `GIT_NO_LAZY_FETCH` 和 `GIT_ATTR_SOURCE` 后，`partial_clone_missing_objects_error_instead_of_lazy_fetching`、`worktree_attributes_cannot_change_an_opened_commit_diff` 都失败，恢复后通过。Git 低于 2.44 / 2.40 时这两项保证不成立，已记在 report 已知限制。本任务只含后端与 IPC，界面与桌面验收在任务 02。
