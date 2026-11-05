# 分支归并与清理（2026-11-05）

用户要求把其他分支合入 main 后删除其他分支，并确认把 m7 worktree 中的 6 个未提交文件一并纳入，先完成审查和相关验证。

## 已提交成果核对

开始时 main / origin/main 均为 `0791e8384c9ccefd7764e2e975592bf7b207ea36`。11 个本地开发分支的成果此前已按里程碑压缩进入 main；Git 图仍显示分叉，不能仅据提交是否为祖先判断是否漏合。

| 开发分支 | main 中的对应提交 / 核对结果 |
| --- | --- |
| m1 | `470cf2b`，应用、工具与原型源码一致；文档已迁移到里程碑目录并更新链接 |
| m2 | `69d4f39`，完整 Git 树一致 |
| m2-ui | 为 m2 的祖先，成果随 m2 合入 |
| m3 | `246e191`，完整 Git 树一致 |
| m4 | `09e0707`，完整 Git 树一致 |
| m5 | `9795fb3`，完整 Git 树一致 |
| m5-batch2 | 为 m5 的祖先，成果随 m5 合入 |
| m6 | `ac47731`，应用源码一致；main 另含首次派发记录 |
| m7 | `b9cf807`，已提交 Git 树一致；另有下述未提交增量 |
| m7-glass | 为 m7 的祖先，成果随 m7 合入 |
| m8 | `45e3e71`，应用与发布工具源码一致；main 的发布记录还包含公开结果和误删范围纠正 |

origin 上另有 m2、m7，分别为本地对应分支的祖先，没有仅存在远端的成果。继续沿用 main 中较新的 Windows、发布记录和修复内容。

## 本次新增量

将 m7 的未提交增量应用到当前 main：

- 并排 diff 的色带先二分定位可见段，再绘制；文字缓存更新不重复重画固定几何的色带。
- Git 子进程启动后直接启动管道读取，移除最终会被完成记录覆盖的预估日志序号读取；完成序号仍由 CommandLog.record 持锁分配。
- 保留两项已有未提交的色带回归及两份历史观测记录。历史首屏 238ms 仍为失败；本次归并不表示 D06 或 Windows 真机性能已达标。

## 备份与目录（归并时快照）

清理前保存全部引用的完整 bundle，并通过 git bundle verify；四个 worktree 的 staged / unstaged patch、未提交文件原字节和散列也已保存。

当时的持久备份为 `.git/branch-backups/2026-11-05-zmwwl4wp/`，包含完整 bundle、原引用 SHA 和 6 个未提交文件。该目录及临时副本已按用户后续要求删除，见文末清理结果。

主目录原有 `.gitignore` 未提交改动继续保留。现有 Revia-glass、Revia-m7、Revia-m8 目录及其构建产物保留；删除所占用分支前只把各目录切到当前提交的 detached HEAD。m7 的原始未提交改动也原处保留，main 已收纳同一增量。

## 验证、审查与最终状态

新增量固定为 `50ef777405fbfdc0fe690ddce8383e7d15716b94`，对照 `0791e8384c9ccefd7764e2e975592bf7b207ea36`。规划方集中执行：

- `pnpm build` 通过，含 TypeScript 检查；保留 Vite 的配置与大块产物提示。
- `pnpm lint` 退出 0；保留 997 条既有警告，新增代码四个路径没有诊断。
- `node --test tools/test-sync-scroll.mjs tools/test-side-line-window.mjs tools/test-ui-input.mjs`，36 passed / 0 failed。
- Rust `--lib git::tests::` 5/5、`branch_remote` 的生产命令日志回归 1/1、`m7_diagnostics` 2/2、`session_flow` 3/3；均离线运行，使用 4 个测试线程。
- `git diff --check`、完整 bundle 验证、11 个原分支 SHA 核对、主目录 `.gitignore` 和 m7 六文件原字节检查均通过。

首次前端构建因本地缺依赖、pnpm 自动补装受沙箱网络限制而退出 127；随后 `pnpm install --frozen-lockfile` 成功，再跑前端检查通过。package.json 和 pnpm-lock.yaml 未改。失败原件与成功原件分别保留。

### Standards

按 `0791e83...50ef777` 审查，未发现新增硬违反或值得报告的代码异味判断。已核对协作规则、术语、技术/UI 规则、相关 ADR 与并排视图设计；二分保留原绘制边界，必要重绘入口与日志完成序号分配保留。

### Spec

按同一范围审查，0 项缺漏、范围扩张或错误实现。二分符合并排视图设计 4.3；缓存发布重绘调整保留几何、量高、折叠、悬停及滚动入口；Git 记录仍持锁分配完成序号，诊断持久化路径未变。238ms 失败及 Windows 未验状态保留。

首个 Standards 代理因任务文本显示为不可读内容而未执行，未计入审查；替代 Standards 与 Spec 代理均实际静态审查了上述范围，最终报告说明范围由当前提交和归并记录推定。规划方已核对其固定基线、候选及实际八文件范围与本次任务一致，并自行复核绘制触发和日志入口。两份有效原文、首个未执行记录与检查 stdout/stderr/退出码曾保存在上述持久备份目录，后随本轮备份按用户要求删除。两轴均 0 项，不代表真实桌面性能验收通过。

`50ef777` 已正常推送 origin/main 后，三个附属 worktree 转为各自原 HEAD 的 detached HEAD，文件未变；随后删除全部 11 个本地开发分支，并原子删除 origin/m2、origin/m7。再次读取远端 heads 和本地分支，均只剩 main；origin/HEAD 仍指向 origin/main。两个版本标签保留，Release 未改动。

本轮没有启动应用或进行新的 D06 测量，也没有 Windows 桌面环境；实际性能收益和 Windows 真机验收仍待完成。

## 后续 worktree 与备份清理

同日用户要求删除 worktree，并明确选择“备份也一起删除”。删除前再次核对：m7 四个代码文件的增量已应用到 main，两份文档的原内容也已进入 main；六文件原字节与备份一致。随后删除 `/Volumes/T7 Shield/project/Revia-glass`、`/Volumes/T7 Shield/project/Revia-m7`、`/Volumes/T7 Shield/project/Revia-m8` 及对应 Git worktree 登记。

用户指定的本轮持久备份 `.git/branch-backups/2026-11-05-zmwwl4wp/` 和临时副本 `/private/tmp/revia-branch-consolidation-zmwwl4wp/` 也已删除。其他历史备份未改动。本文件上方保留的是归并时的执行记录，不再表示这些目录当前存在。

最终检查 `git worktree list --porcelain` 只剩主目录的 main；三个附属目录、两个备份目录均不存在，主目录原有 `.gitignore` 字节保持不变。本次仅清理目录并更新记录，使用目录/登记核对与 `git diff --check` 验证，没有重跑应用构建和测试。
