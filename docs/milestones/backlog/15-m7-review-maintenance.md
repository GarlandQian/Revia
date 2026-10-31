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


## review03 判断建议原文 — 2026-10-31

固定候选4c3b3f8，均P3非阻断，按Q174登记，不扩大本轮重构。

判断（均P3，非阻断；依据任务说明smell baseline，非硬违反）：

- **J1，可能Duplicated Code**：`src/App.tsx:1784,1793,1807,1819` 四次复制五类退出保护条件；新增保护来源须同步四处，容易漂移，可集中判定。
- **J2，可能Primitive Obsession**：`github/store.rs:30` 的草稿kind为String，`GithubPanel.tsx:119,125` 又把审阅事件写进原表示新旧行的side；新增表单/事件时须维护字符串分支与强制转换，可用明确领域类型。
- **J3，可能Duplicated Code（仅测试）**：`m7_protection.rs:24` 与 `m7_review_drafts.rs:27` 复制Fixture及HTTP支架；隔离和服务端时序修正需多处同步，可列共享fixture维护项。

### review05 非阻断登记

- Standards：更改列表Unicode星号/Codicons及Git文件状态色；名称校验重复为可能Duplicated Code判断。
- Spec S05-07：保存无关设置使父勾选凭据清空、子列表仍显示勾选；改动入口修复时核对，不据此扩大重构。

两轴原文在review-05-n9a_z26m及任务13/14 Comments。

### review07非阻断判断

Standards可能Duplicated Code：多处重复查找changelist/untracked读源；P3，优先在修当前明确刷新/导航路由时复用同一查询，不为判断扩大重构。原文在review-07-wx8l6b6k及任务13/14。


### review08 Spec：S08-B01（P3，仅backlog）

- **[P3，S08-B01，仅 backlog，静态] 空预览的冲突路径可能打不开内置合并。** M6 规格 `docs/milestones/m6/spec.md:146` 要求“保留内置合并”。新增 `src/App.tsx:6472` 将冲突对话框也限定为 `changelistEntryForPath`；该函数只接受 changelist／untracked（`src/lib/m7/visibleChangelistSelection.ts:41–42`）。当 index 仍 unmerged、工作区已恢复为 HEAD 内容时，HEAD→工作区补丁为空，`src-tauri/src/session.rs:572–584` 不生成 changelist 条目，已有 unmerged 条目又被排除；点“合并”只关闭对话框。未动态复现，按 Q174 作为边界 backlog，不要求本轮扩修。

来源：/private/tmp/revia-m7-batch-g5scyw0r/review-08-qlvudspl/spec.md。静态判断，未动态复现；不扩大本轮任务13修复或阻断真实桌面验收。


### review09 Spec S09-B01（P3，仅backlog）

**[P3，S09-B01，仅backlog] EOF换行上下文可能阻断归组关联。** 规格 `docs/milestones/m7/spec.md:133`：“提交后只移除已提交部分”。新 `src-tauri/src/m7/changelists.rs:573–588` 将切块补丁中所有反斜杠行计入关联payload；既有切块器会保留转为上下文行的无末尾换行标记（`segments.rs:591–600`）。同hunk有未提交块及被提交的EOF补换行块时，后者提交后该上下文标记消失，前者可能无法关联，留下失配原件并落默认组。应仅纳入目标块所属标记。静态推导，未动态复现；按规格`:151`／Q174记backlog，不阻断本轮。

来源：/private/tmp/revia-m7-batch-g5scyw0r/review-09-udxx1qjo/spec.md；未动态复现，不扩大本轮修复范围。


### review10 Standards P3

Standards：固定 `5ca917889efa5141442d0b73d6e48931828acf93...039fd1a3dbea7cf7e0c7bac91cd648509943dd41`，仅本轮10路径新增量，未读Spec。

硬违反0项；判断2项，均P3、仅backlog：

- S10-ST01，证据口径不一致：`docs/milestones/m7/report.md:303`及任务01:88、04:126、13:292、14:507称“持锁区不运行Git”，实际新增`session.rs:1624–1628`持有writing Mutex调用require_identity，经recovery.rs:188、160、122执行git rev-parse。按`docs/agents/local-validation.md:16`“每项结论都能对应固定源码、生产入口和实际可观察结果”，建议修正文档；没有另立“锁内禁Git”硬规范，未复现新输入丢失或输入延迟，不能据此升格P2。
- S10-ST02，possible Duplicated Code：`src/index.css:1848–1850`、1857–1859、1866–1868重复同一tint与两条blur声明，可合并选择器共用材质规则；无功能失败证据。

其余11项固定smell已逐项判断，无新增发现；仓库规则优先，工具强制项跳过。

已核固定diff、33项源码/规则快照、106项相关原件散列。原记录Rust44、node20通过，build/lint/x18n exit0，翻译0错误/76警告。Rust受测源码一致（CSS之后另改，最终前端快照一致）。未重跑、未启动桌面；新Release原生PTY/Busy/浮层/默认名、D06、Windows仍待规划方验证。两项P3按Q174不阻断验收。


S10-ST01: documentation wording clarified before native replay; no product expansion. S10-ST02 remains backlog only.
