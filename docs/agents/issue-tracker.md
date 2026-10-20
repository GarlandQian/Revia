# 任务记录：本地 Markdown

需求规格、任务和里程碑交接文件都以 markdown 形式放在 `docs/milestones/` 下，并提交进仓库，供不同会话和工具读取。

## 约定

每个里程碑一个文件夹 `docs/milestones/m<N>/`，编号与分支名 `m<N>` 一致：

| 文件 | 内容 | 写入方 |
| --- | --- | --- |
| `spec.md` | 需求规格 | 评审方，经用户同意 |
| `plan.md` | 实施计划 | 实施方 |
| `review.md` | 评审意见；标明“用户已同意”后才能开工 | 评审方，经用户同意 |
| `report.md` | 实施报告 | 实施方 |
| `issues/NN-<slug>.md` | 任务，一个文件一个，从 `01` 编号 | 拆分任务时生成 |

- 任务文件顶部用 `Status:` 行记录状态：`needs-triage`、`needs-info`、`ready-for-agent`、`ready-for-human`、`wontfix`。
- 依赖写成 `Blocked by: NN, NN`；列出的任务全部完成后才能开始。
- 讨论和补充追加在文件底部的 `## Comments` 下。

## 技能要求“发布到任务记录”时

在对应的里程碑文件夹下新建文件，需要时新建文件夹。不属于任何里程碑的工作放进 `docs/milestones/backlog/`。

## 技能要求“读取相关任务”时

读取对应路径的文件。用户通常会直接给出路径或里程碑编号。
