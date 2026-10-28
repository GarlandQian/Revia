# 03：分支、远端同步与 Git 命令日志

Status: done
Blocked by: 无

范围：规格 F02–F05，细则见 [规格](../spec.md) 第二批 B01–B08。与任务 02 一起由 Pi 一次实施（Q146）。

## 验收项

- [x] B01：所有分支与远端操作走全局写门禁、关闭 / 退出保护、结果归属与“结果待核对”；先存回收区的步骤；网络操作 10 分钟上限；不等待凭据输入；不修改 Git 配置。
- [x] B02：分支组件弹层（搜索、本地 / 远端、领先 / 落后数）；检出本地与远端分支；智能检出与强制检出（都先存回收区，恢复冲突保留 stash）；新建分支对话框与三种起点。
- [x] B03：fetch 与四种标签方式；自动 fetch（每 10 分钟，默认关）；检查远端是否有新提交（默认关）。
- [x] B04：更新（⌘T）三种方式，默认 merge；有未提交改动时先存回收区再 autostash；merge 冲突走合并页签，rebase 冲突提示到终端。
- [x] B05：推送对话框（⌘⇧K）、新分支记录上游、推送标签、强制推送（受保护分支禁用、`--force-with-lease` 带期望值）、被拒后更新再推送与自动更新一次。
- [x] B06：提交并推送（⌥⌘K）与两项推送对话框设置；提交失败或待核对时不推送。
- [x] B07：Git 命令日志面板：完整记录、64 KB 截断、最近 1000 条、凭据打码、随新命令更新。
- [x] B08：“Git › 更新与推送”设置页与默认值。
- [x] 第二批验证中与分支、远端、命令日志相关的 Rust / Node 回归，集中检查通过。
- [x] 规划方独立审查通过（含本轮 `49ba903...1ad68be` 推送预览补修，两轴各 0 项确定问题）。
- [ ] 真实桌面与本机 Rebased 并排对照（规划方，Q146）。

## Comments

2026-10-27，Codex：现有 `command_log_snapshot` 仅有输出字节数与 stderr 摘要，不能直接宣称满足完整 Git 命令日志界面；保留凭据脱敏、输出限额与实际展示的设计工作。

2026-10-27，Claude：继承的全局写入保护缺口已在 `6553772` 修复并合入 `m5`（见继承状态）。安全与交互契约按 Q148–Q153 写进规格 B01–B08，状态改为 ready-for-agent。

2026-10-27，Pi（第二批，分支 `m5-batch2`）：B01–B08 已实现并交付审查，界面在 `src/features/branches/` 与 `src/features/commands/`，写编排为 `src/lib/guardedWrite.ts` + `src/features/branches/remoteWrite.ts`（`mergeWrite.ts` 也改为委托同一实现）。回归：`src-tauri/tests/branch_remote.rs`（26 个，本地裸仓库作 `file://` 远端）、`tools/test-remote-write.mjs`（10 个）、`src-tauri/src/git/mod.rs` 的打码与命令日志单元测试；集中检查 `cargo test`（517 通过 / 1 忽略 / 0 失败）与 15 个 Node 脚本全部通过，证据在 `/private/tmp/revia-m5-start.zEV4wp/batch2-GzR75r/`。交互选择与偏差（检出文件集合由 Rust 预判、智能 / 强制检出保存范围、推送被拒入口、“视图”菜单为新增顶栏下拉、命令日志轮询）与需要确认的问题见 [报告](../report.md) 第 14、18 节。真实桌面、Rebased 对照与独立审查未做，未标 done。

2026-10-27，Claude（规划方审查，`3c9a634...55326a3`）：安全边界都守住了（写锁、先存回收区、`--force-with-lease` 带期望值、不等待凭据、命令日志打码、Git 调用只在 Rust）；集中检查独立重跑结果与报告一致（Rust 517/0/1、Node 15/15）。下面几项按 Q113 一次返修，每项在仓库测试里补回归：

1. **P2 强制推送的受保护判断用错了分支。** `remote_ops.rs` `push` 与 `push_preview` 只检查本地分支名（`branch_name`），把本地 `feature` 强制推到远端 `main` 时会放行。规格 B05“受保护分支上强制推送不可用”保护的是被覆盖的远端分支。改为目标分支（`target_branch`）与本地分支任一受保护都拒绝，预览的 `forceAllowed` 同样按目标分支算。回归：本地 `feature` 以 `--force-with-lease` 推到远端 `main` 被拒，远端不变。
2. **P2 每次推送都重写上游配置。** 界面每次推送都带上目标分支（`performPush` 从预览取 `targetBranch`），后端 `set_upstream = upstream.is_none() || request.branch.is_some()` 因而总为真，每次都带 `--set-upstream` 重写 `branch.<name>.remote/merge`。规格 B01、B05 只允许“没有上游时推送成功后记录上游”。改为：没有上游，或用户把目标改成了与现有上游不同的远端 / 分支时才带 `--set-upstream`。回归：已有上游时推送到同一上游，`.git/config` 内容不变。
3. **P3 “检查远端”随引用变化反复联网。** `useBranches` 里检查远端的 effect 依赖 `refreshKey`，每次提交、检出、fetch 后都会立即再跑一次 `ls-remote`，不只是每 10 分钟一次。改为只在开启、切换 worktree 时检查一次，之后每 10 分钟一次；fetch / 更新成功后直接清除“远端有新提交”标记即可。
4. **P3 `git checkout` 参数后补 `--`**（`checkout` 本地分支、`-b … --track …`、智能检出与强制检出），避免与同名文件混淆。
5. **P3 去掉顶栏新增的“视图”下拉。** Rebased 的顶栏没有这个控件；Git 命令日志保留状态栏入口，菜单入口随 [backlog 03](../../backlog/03-native-menu-bar.md) 的原生菜单一起做。
6. **P3 新建分支失败时，对话框里直接显示 Git 给出的原因**（例如名字不合法），不只显示通用失败提示。

报告第 18 节的两个问题：merge 三栏用独立只读组件——接受；正则开启时不做哈希前缀匹配——接受，保持现状并写进报告。日期筛选按提交时间、合并线拉直阈值 24 行、提交 diff 软换行的近似做法——先接受，桌面验收时与 Rebased 对照。

2026-10-27，Pi（第二批返修，分支 `m5-batch2`，Q113 唯一一次返修）：审查第 1–6、9 项已修完。1）强制推送的受保护判断改为目标远端分支与本地分支任一受保护都拒绝，`push_preview` 的 `forceAllowed` 同样按目标分支算；回归 `force_push_is_refused_when_the_target_remote_branch_is_protected`、`push_preview_treats_the_target_branch_as_protected`。2）`--set-upstream` 只在没有上游或目标与现有上游不同时才带，常规推送不再重写 `branch.<name>.remote/merge`；回归 `push_does_not_rewrite_the_upstream_config_when_target_is_unchanged`（`.git/config` 字节不变、命令日志无 `--set-upstream`）。3）检查远端的 effect 去掉引用版本依赖（只在开启 / 切换 worktree 时查一次，之后每 10 分钟），fetch / 更新成功后清除“远端有新提交”标记；回归在 `tools/test-remote-write.mjs`。4）所有 `git checkout` 参数末尾补 `--`；回归 `checkout_separates_paths_with_double_dash`。5）删除顶栏“视图”下拉，Git 命令日志只保留状态栏入口（菜单入口随 backlog 03 的原生菜单）；回归在 `tools/test-remote-write.mjs`。6）新建分支失败时对话框直接显示 Git 原话（`branchFailureDetail`），写编排改为返回 error；回归两项。9）`guardedWrite.ts` 注释改为 `tools/test-remote-write.mjs`，并有存在性断言。1、2、4 项做了反向验证。集中检查（`cargo test` 522 通过 / 1 忽略 / 0 失败、15 个 Node 脚本、`tsc -b`、`lint`、`build`）全部通过，证据 `/private/tmp/revia-m5-start.zEV4wp/rework-batch2-QBs0bF/`；逐项改动与测试见 [报告](../report.md) 第 20 节。真实桌面与规划方复核未做，未标 done。


2026-10-27，Codex（规划方接手复核，Q113 返修已用完）：Standards 轴发现旧 worktree 的完成回调清错 incoming，以及分支/上游变化未失效旧结果；Spec 轴发现 `PushPreview.protected` 被远端目标保护污染，偏离 B06 当前分支语义。现分别用激活/分支/请求代际约束标记，完成清除淘汰在途检查；首次概览不错误失效操作凭据。`protected` 只表示当前分支，强制推送仍检查两端。独立闭环复核确认上述边界闭合；`branch_remote` 30/30、`test-remote-write` 23/23、类型/lint/build 通过，先前错误 fixture 的失败原件保留。详见报告第 21 节。独立审查项已勾选，状态转 `ready-for-human`；真实桌面与 Rebased 对照未做，不标 done。

2026-10-28，Codex（集成版本 `eaf14d2...d627156`，新的独立 Standards 轴审查）：发现两项 P1、一项 P2。

1. P1 / Q90、B01：提交并推送仅预览使用原 worktree，后续推送、确认与更新重试使用当前闭包的 worktree；A 操作中切 B 会换错写目标。须将冻结身份贯穿所有阶段，迟到确认不得套到新目标。
2. P1 / Q90、B01：检出被阻止时的确认状态未保存 worktree/激活代际，A 的迟到确认会调用 B 的检出入口。须绑定原目标并拒绝过期确认。
3. P2 / Q93：远端快捷键缺少输入框与模态门禁，在设置或新建分支输入时可能后台更新或提交并推送。

另核对提交成功后推送是否先于提交写门禁释放；回归使用生产协调器安排受控时序，不以源码正则代替归属验证。Q113 返修次数已用完，由 Codex 修复并重新独立复核；不操作真实远端。

2026-10-28，Codex：新的独立 Standards / Spec 两轴分别复核 `d627156...066d1e4`，
两轴均未发现新增确定问题，确认原目标贯穿确认、自动续步与唯一重试，写生命周期不在阶段间隙释放。
状态转 `ready-for-human`；普通模式 Release、完整桌面工作流与 Rebased 对照尚未完成，不标 done。

2026-10-28，Codex（第五包，源码 `5c2367f`）：普通隔离桌面已测通本轮自建 fixture 的更新、被另一 worktree 占用分支的检出拒绝、
新建并检出分支、暂存两个 tracked 文件及“提交并推送”。唯一远端是本轮 `file://` 裸仓库，远端同名分支 OID 与本地提交
`4c8eee92bca8441b42874d6e1c286ca21014fb61` 一致；未跟踪 `notes.txt` 未被提交。更新前的三份本地文件内容 SHA256 均保留，
回收记录 completed 且保存内容吻合；Git autostash 恢复后原已暂存文件变为未暂存，不能描述为暂存状态完全不变。
推送对话框与 worktree、最近提交信息等弹层在本轮窗口内完整可见。以上只覆盖正常本地工作流，不代替强制推送、
被拒后更新重推、全部退出保护与 Rebased 对照；仍不标 done。证据见报告 22.10。

2026-10-28，Codex（第七包普通模式）：在本轮自建 fixture 通过 UI 暂存并提交 `src/demo.ts`，未跟踪 `notes.txt` 保留。
已有上游且本地领先时，点推送无法打开预览，报“读取提交范围时缺少完整对象 ID”。后端预览在此分支传字面量 `HEAD`，
而 `read_range` 只接受完整 OID。现传本次已捕获的完整 `head`，在现有 `branch_remote` 套件补已有上游领先的预览回归，
并让现有远端拒绝用例先验证未 fetch 时也能打开预览。失败截图及独立 fixture 命令在报告 22.14；
待集中检查、新两轴复核及新固定包桌面，状态回到 ready-for-agent，不标 done。

2026-10-28，Codex（第八包，源码 `1ad68be`）：新的独立 Standards / Spec 两轴复核
`49ba903...1ad68be`，均为 0 项确定问题。新固定 Release 构建退出 0，普通隔离实例 PID 26767 无测量面板。
已有上游的推送预览正确列出本地 1 条提交；真正普通推送被本轮 `file://` 裸远端非快进拒绝，
点击“更新后再推送”后 merge 更新与重推成功。本地 HEAD、上游及裸远端均为
`253dc067603b0d97f4502a47a6209247b6036961`；原双方提交都可达，未跟踪 `notes.txt` 内容和 Git 配置 SHA256 不变。
两次对话框完整可见，实例正常退出 0；原件 `workflow-08-hgz15r26/`，详见报告 22.15。
恢复 ready-for-human，不把本次正常续步当作全部安全路径、Rebased 对照或偶发弹层问题已关闭。

- 2026-10-28，规划方 Claude：用户认为 M5 可以了，按 Q154 带未做完的桌面验收合入 main；剩余项转入 [backlog 04–06](../../backlog/)，本任务据此标为 done，不表示这些项已验收通过。
