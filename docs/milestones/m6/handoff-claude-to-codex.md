# M6 交接：Claude → Codex（2026-10-30）

Claude 因额度中止，剩余工作交回 Codex。决策以 [docs/README.md](../../README.md) 为准；本文件只记录进度和待办，不产生新决策（Q174、待补的 Q175 除外，均为用户已在对话中确认的内容）。

## 当前状态

- 分支 `m6`，worktree `/Users/garland/.codex/worktrees/revia-m6/Revia`，工作区干净（最新进度见下方“进度更新”）。未合 main、未推送、未开 PR。
- Claude 接手后的提交（`f00f469..HEAD`）：
  - `e6c58e1` review33 P1：自动恢复 stash 前重验原分支（`AutostashLink.branch` 记录保存时的分支上下文，自动恢复不在原分支则保留 stash 报待核对；`history_sequence` 结束前 `validate`）。
  - `ca646c2` 桌面验收第一批：跳过 / 继续停到下一冲突不再报待核对；提交区“请在终端完成”旧文案；“。”改资源断句；跳过确认标题；日志分支图表头、主题列最小宽度 160；过滤框内边距。
  - `a187c87` 第二批：历史批次冲突停点不附错误；文件历史与 stash diff 补 `core.quotePath=false` 与 `a/` `b/` 前缀；Stash 面板切仓库误报过期（只读确认重试 3 次 + 切换时清旧条目）；根提交禁用理由；根版本 blame 说明可见；设置搜索关键词。
  - `ff38a02` 用户确认的小修：多选时禁用新建分支 / 改说明 / 交互式 rebase / Reset；多选 cherry-pick / revert 确认文案；Reset 不把 hard 记为下次默认；Stash 归档只计本仓库，其他仓库归档折叠只读。
  - `ae7889a`、`e889a08` 报告与 Comments；`03cf98d` Q174、修正 Q173 表格空行、backlog 07 完成、新增 backlog 09–13。
- 检查（`03cf98d` 源码）：Rust 695 通过 / 0 失败 / 1 既有忽略，Node 485/485，build/tsc/lint，x18n 1718 键 0 错误 65 未用，提取 64 候选 0 中文。
- 桌面：Release `a187c87` 上全部桌面组已真实验收并复验通过，记录在 `~/.cache/revia-m6-claude/release-a187c87/observations.md`（逐组过程、证据目录、观察项）。**`ff38a02` 的 4 处小修尚未在 Release 上复查。**

## 增量审查（Q174）进度

固定点 `f00f469`，范围 `f00f469...HEAD`。

- **Standards 已完成**（独立子代理）：
  - P2：`HistoryActionDialog.rememberedResetMode` 不记 hard，与 spec.md H02 “确认后记住本次模式”不一致，且未记成决策。用户已在对话中对 Claude 的建议回复“可以”（建议原文：“记住 soft / mixed / keep，上次是 hard 时默认回到 mixed”）。→ 待办 1。
  - P3（判断项）：① `stash.rs` 3 处、`file_history.rs` 1 处手写固定 diff 参数与 quotePath，建议抽共用常量；② “冲突停点算正常结果”在 `operation_flow` 与 `rewrite::finish` 两种写法，且 `operation_flow` 未比较 `metadata_version`，Skip / Continue 失败但原地未前进时会丢掉“待核对”→ 待办 2 建议直接修；③ `AutostashLink.branch` 实为分支上下文指纹，命名易误读；④ `HistoryActionDialog` 多选三元表达式重复 3 次；⑤ 其他仓库归档列表写死分隔符“ · ”（与既有本仓库归档行同样写法）。
  - 未发现安全边界、Git 只在 Rust 层、平台假设、x18n 字面量键的违规。
- **Spec 未拿到结果**：Claude 额度中止前子代理仍在运行。需重跑 Spec 轴（同一范围，依据 spec.md、任务 Comments 末两节、review33 原报告 `/private/tmp/revia-m6-batch-ig_nawgt/review-33-y11p0km4/spec.md`、Q173–Q174）。

## 待办（按顺序）

1. **记录 Q175**：Reset 记住上次模式，但上次为 hard 时下次默认 mixed（用户 2026-10-30 回复“可以”）；同步修改 spec.md H02 对应句，单独 docs 提交。
2. **修 Standards P3-②**：`operation_flow::execute` 判断下一冲突停点时，要求 `after.kind == state.kind && after.conflict_count > 0 && after.metadata_version != state.metadata_version`；原地未前进的非零退出仍返回 NeedsVerification。补一条真实 Git 回归（未解决冲突时 Continue 失败仍报待核对），跑 m6_flows。其余 P3 记入 backlog 或维护建议，不阻断（Q174）。
3. **Spec 轴增量审查**：结果中 P1 / P2 修复，P3 记 backlog。修复后只对新增修复增量复审。
4. **打 Release 桌面复查**（只复查改动点）：多选时右键菜单禁用项与“多选时只能 cherry-pick 或 revert”；多选 cherry-pick / revert 确认框文案；Reset 选 hard 执行后再开对话框默认 mixed；Stash 面板“已删除 stash 的归档”只计本仓库、“其他仓库的归档（N，只读）”折叠显示来源路径且无恢复按钮；待办 2 的停点提示。
5. **合入 main**：在主目录 `/Volumes/T7 Shield/project/Revia`（main，干净）执行 `git merge --squash m6` 后提交，说明 “M6：冲突与历史、双语界面”。main 多出的 `26b68d1` 只加了 `docs/milestones/m6/dispatch.md`，与 m6 无重叠，不会冲突。合并后删除 m6 worktree，AGENTS.md “当前阶段”改为 M6 已合入、下一步 M7 第 1 步，backlog 04–06 与 09–13 保留。**不推送、不开 PR**。

## 环境与操作要点

- Fixture 与工具都在 `~/.cache/revia-m6-claude/`：`make-fixtures.sh <dir>` 生成各组独立仓库；`launch.sh <release目录> <appdata>` 以 `REVIA_ACCEPTANCE=1` 独立数据启动复制的 .app；`switch-repo.py <名称>` 在应用退出后把仓库排到首位；应用数据 `accept/appdata`（settings.json 可直接改 `language`）。不要用 T7 盘上的路径给新二进制读写（会触发可移动卷授权弹窗）。
- 打包：worktree 内 `touch src-tauri/src/lib.rs && pnpm tauri build --bundles app`，复制 .app 到独立目录并记录 SHA-256 后再启动。
- 检查：Node 测试需先带 `REVIA_M6_ERROR_FIXTURE=<目录>/errors` 跑 Rust（m6_flows、commit_flow、review_marks 导出真实载荷）；`cargo test` 默认并发时 `user_refresh_during_autostash_invalidates_the_unstarted_history_write` 的 15 秒等待可能超时，用 `-- --test-threads=4` 或单独跑即通过；跑完 Rust 后把 `src/types/bindings` 的纯空白差异 `git checkout` 还原。
- 桌面操控：后台模式无法向 WebView 打字、开原生下拉框和右键菜单，需全屏控制（用户须在场批准）。1Password 有一个截图里看不见的窗口会挡住部分区域，点击被拒时换坐标或用后台点击。
- 待用户决定、未改的观察：见任务 Comments“Claude 全屏桌面验收与第三批修复”的观察列表，已转 backlog 09–13 的除外。

## 进度更新

- 待办 1 已完成：`473419c` 记录 Q175 并改 spec.md H02。
- 待办 2 已完成：`ba1fd57` operation_flow 要求元数据版本变化才算正常停点，新增 `rebase_skip_that_does_not_advance_still_needs_verification`（index.lock 占用时跳过失败仍报待核对，且不删锁；修复前失败）。
- Spec 轴增量审查已完成，P2 即 ba1fd57；P3 在 `0113241` 修复（多选含合并提交的提示、切换仓库清空 Stash 面板状态）。
- Release 0113241 复查 Stash 归档分组通过；其余小改动因全屏授权无人确认未在真机点，转 backlog 14。
- 用户确认后已压成一个提交合入 main（见 main 上“M6：冲突与历史、双语界面”）。本文件保留作记录，Codex 不需要再做待办 1–5；backlog 14 待桌面补查。
