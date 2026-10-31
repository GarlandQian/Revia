# M7 首轮审查非阻断维护项

Status: needs-triage
来源：Codex 两轴审查，2026-10-31T03:14:30.306434+08:00，固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`；完整原文在M7任务14 Comments和证据目录 `/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj`。按Q174登记，不扩大本轮重构，也不代表真实桌面已通过。

- Standards H10（P3）：约定的macOS安装包名称与macos-SHA256SUMS.txt尚未在打包工作流落实。原文见任务12 Comments。
- Standards H11（P3）：快捷键中文文案仍显示hunk，术语应为“块”；后续必须经x18n修改。原文见任务02 Comments。
- 判断建议（P3）：App.tsx欢迎分支重复挂载CloneDialog；PrMergeOutcome有限状态仍用String。保留为维护建议，若接线修复自然消除可记录，不为它们另作抽象重构。
- Spec非阻断观察：删除重建相同路径/相同内容时编辑草稿的旧身份继承，尚未动态复现。保留旧草稿，后续按实际风险验证。

PR网络写后不确定结果的查询核对不作为新增独立已复现数据损坏计数，但本轮PR写入/身份返修须核对该原规格流程，禁止自动重放远端写入。

## review02 非阻断补充 — 2026-10-31

<details>
<summary>Standards P3 与判断建议原文</summary>

## N2 / R09：任务Status引入未约定值（P3，非阻断）

规则原文：`docs/agents/issue-tracker.md:18`：“状态：needs-triage、needs-info、ready-for-agent、ready-for-human、wontfix、done”。

12个任务 `docs/milestones/m7/issues/01-liquid-glass.md` 至 `12-actions-package.md` 的第3行以及任务图README新改成ready-for-review；文档规范、状态集合没有同步变更。没有从实现交付推导done，也没有要求降低真实桌面验收。仅建议使用已约定状态并在Comments表达待审，或在获得决策后统一扩充状态模型。按Q174留backlog，不阻断。

## Smell基线完整筛查与建议

判断建议3项，全部P3非阻断；不自动计为硬违反、不要求本轮扩大重构：

1. **可能Duplicated Code**（原smell未消除且增多）：`src/App.tsx:5376–5384` 与 `5387–5397` 都挂CloneDialog；Diagnostics `5398/5429`、UpdatePanel `5399–5415/5430–5444` 也重复。摘录：`<CloneDialog open={cloneOpen} ... />` 与 `{cloneOpen ? <CloneDialog open ... /> : null}`。三对同源state/回调，可保留每种一个实例。实施自述说已删除重复Clone，实际是删除一处又加入一处。没有把重复JSX冒称明文禁止。
2. **可能Duplicated Code**：`diagnostics.rs:306–318,353–365` 两处完整构造DiagnosticsCommand，其中 `args: record.args.iter().map(|arg| redact(arg, &mut redactions)).collect()` 等字段相同。可抽取一个安全投影，减少后续脱敏规则漂移。
3. **可能Duplicated Code**：`github/pr.rs:560–577` 与同文件 `fetch_all_pages:1101–1109` 有相同“get_json / extend / next_page循环”；files可调用已新增的泛型helper后map，避免维护两套分页。

</details>

## 编辑组翻译维护观察

`edit-x18n-01`检查0错误/76个unused-key警告，原完整诊断在批次codex-checks中。编辑器替换使部分旧元信息文案不再直接引用，仍保留旧键，不扩大本组为删除翻译键或重组命名。

## 终端组维护观察

`terminal-lint-01`新增1条`unicorn(no-useless-spread)`提示：退出准备对当前pending集合取数组快照后Promise.allSettled。未产生检查错误；不扩大本组为样式/抽象重构。完整诊断和源码hash在本轮codex-checks。
