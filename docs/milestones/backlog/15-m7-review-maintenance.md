# M7 首轮审查非阻断维护项

Status: needs-triage
来源：Codex 两轴审查，2026-10-31T03:14:30.306434+08:00，固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`；完整原文在M7任务14 Comments和证据目录 `/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj`。按Q174登记，不扩大本轮重构，也不代表真实桌面已通过。

- Standards H10（P3）：约定的macOS安装包名称与macos-SHA256SUMS.txt尚未在打包工作流落实。原文见任务12 Comments。
- Standards H11（P3）：快捷键中文文案仍显示hunk，术语应为“块”；后续必须经x18n修改。原文见任务02 Comments。
- 判断建议（P3）：App.tsx欢迎分支重复挂载CloneDialog；PrMergeOutcome有限状态仍用String。保留为维护建议，若接线修复自然消除可记录，不为它们另作抽象重构。
- Spec非阻断观察：删除重建相同路径/相同内容时编辑草稿的旧身份继承，尚未动态复现。保留旧草稿，后续按实际风险验证。

PR网络写后不确定结果的查询核对不作为新增独立已复现数据损坏计数，但本轮PR写入/身份返修须核对该原规格流程，禁止自动重放远端写入。
