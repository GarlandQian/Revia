# M5 首批实施交接（给 Pi）

## 执行任务

只实现 [任务 01](./issues/01-readonly-history.md) 的只读历史后端与 IPC，交付代码和集中检查结果。
这是 M5 功能的第一批，不是完整 M5；任务 02/03 尚未就绪，不自行决定或实现它们的产品行为。

## 工作位置与固定基线

- 允许修改的目录：`/Users/garland/.codex/worktrees/revia-m5/Revia`，分支 `m5`。
- 代码基线：`d58233c1bea3583cbec5868739b9618f7696eb99`；其后的规划提交包含本交接。启动时记录实际 HEAD。
- 只在 `m5` 提交；不推送、不合并、不创建 PR，不改主目录或 M4/其他旧 worktree，不清理旧会话。
- 验证根：`/private/tmp/revia-m5-start.zEV4wp/`。为本次运行在其内用 `mktemp -d` 创建独占子目录，记录路径；真实 Git fixture 和独立应用数据目录都在本次子目录，测试只读用户之外的自建 fixture。

## 先读

1. `AGENTS.md`、`GLOSSARY.md`、`docs/agents/local-validation.md`。
2. `docs/README.md` 的 Q90–Q96、Q122、Q126、Q137、Q139、Q145，以及 ADR-0002/0004/0006/0008。
3. [M5 规格](./spec.md)、任务 01、[只读历史契约检查](./history-contract-checks.md)、[继承状态](./inherited-state.md)。

## 实施要点

1. 阅读现有 `Git`、`WorktreeSession`、命令层、diff/rows/segments/highlight；实现独立历史领域。
2. 起点和比较父提交由调用者显式给定，固定为完整 OID；不要在本批决定默认筛选、merge 比较父项或 UI 默认值。
3. 历史内容来自 Git 对象；`ChangesKind`、工作区快照和写入凭据不承载历史目标。生产入口必须校验登记归属、对象类型与改动文件身份。
4. 完成 IPC 与生成绑定，提供前端后续可直接使用的固定类型读取接口；不在 `App.tsx` 堆临时展示，不放未实现按钮。
5. 用现有测试体系覆盖新公共契约，集中检查，最后写 `report.md` 并按明确文件暂存、提交。

工程内部选型由你完成并在报告写明；若遇到未决产品行为，保留无默认的读取原语并列出问题，不伪造用户确认，也不因此停止能独立完成的部分。

## 验证与交付

- `pnpm install --frozen-lockfile`；新 Rust 类型用 `pnpm bindings` 生成（必要时 PATH 加 `/opt/homebrew/bin`）。不要共用或覆盖 M4 的构建产物。
- 相关修改全部完成后集中执行：`pnpm exec tsc -b`、`pnpm lint`、`pnpm build`、`/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml`，逐个 `node --test tools/test-*.mjs`。
- 继承的 `test-merge-appearance.mjs` 导入失败可参照 `test-sync-scroll.mjs` 的 `registerHooks` 修正；修测试接线，不删断言或改合并行为。
- 留存实际 HEAD、未提交补丁、精确命令、退出码、原始 stdout/stderr。修复后只重跑受影响检查。
- 报告写实现位置、接口调用示例、资源限额/释放规则、测试结果、未完成项。桌面、Rebased 对照、Release 打包、D06 由规划方后续做，未做明确写未验收。
- 任务验收勾选仅反映已完成项；独立审查未过不标 done。提交说明以 `M5：` 开头，最后给出提交 SHA。

遇到模型请求/渠道错误保留原件，由规划方按 Q137 换下一模型；测试失败属于工程修复，不是换模型理由。

## 首次派发记录

2026-10-27，Paseo 工作区 `wks_934c484d24bf84b8`，目录为本文指定的 `m5` worktree。

| 模型 | 会话 | 本次结果 |
| --- | --- | --- |
| GLM-5.3-Flash | `2eef98ce-e41e-43a6-a85d-1f3bfd785fd6` | 模型请求报 quota limit，已停止；未产生代码改动 |
| GLM-5.3 | `d05f0b6e-7ab5-487d-b6c0-72fd7316c203` | 模型请求报 concurrency limit，已停止；未产生代码改动 |
| deepseek-v4.1-flash | `a258acf4-a835-451c-b60e-1e58f00b2aa0` | 首次执行完成源码调查和自建 Git 探针后遇到 `unexpected EOF`，未产生代码改动；按用户本轮反馈已在原会话重试，尚未交付 |
| gemini-3.8-flash | `82aa99e2-ebc0-4b1e-8ca0-d38f68bf56ac` | 曾因前述响应流中断按 Q137 启动；用户反馈 DeepSeek 应该仍可用后已停止，未产生代码改动 |

派发、错误和首轮等待的原始记录在 `/private/tmp/revia-m5-start.zEV4wp/paseo-*.json`（部分文件是 CLI 原样文本）。等待超时只表示任务仍运行，不计渠道失败。恢复时先用 `paseo inspect` / `paseo logs` 核对该会话，避免重复派发。

用户随后指出应持续监听并在 Pi 交付后接手验证。规划方已恢复等待，并建立本线程每 10 分钟的自动跟进 `m5`，在任务 01 审查与验证完成后停止。当前先跟进原 DeepSeek 的本次重试；`unexpected EOF` 是响应流中断，不能据此断言额度耗尽或模型不可用，本次重试也不改写未来任务的全局模型顺序。

## 第二批交接：任务 02、03（给 Pi）

依据 Q146：任务 02、03 一起实施，做完再统一审查与验收，中间不逐项派发。

### 工作位置

- 允许修改的目录：`/Volumes/T7 Shield/project/Revia-m5`，分支 `m5-batch2`（从 `m5` 分出）。只在这里修改和提交；不改主目录 `/Volumes/T7 Shield/project/Revia` 和其他 worktree，不切换分支，不推送，不合并。合并回 `m5` 由规划方做。
- 临时目录：需要时在 `/private/tmp/revia-m5-start.zEV4wp/` 下用 `mktemp -d` 新建并记录路径；测试仓库与应用数据目录用现有测试体系的临时目录。

### 先读

1. `AGENTS.md`、`GLOSSARY.md`（含新增“分支与远端”）、`docs/agents/local-validation.md`、`docs/ui-direction.md`。
2. `docs/README.md` 的 Q90–Q96、Q119–Q153，ADR-0004、0005、0008、0009、0010。
3. [M5 规格](./spec.md)全文，重点“第二批”L01–L06、B01–B08、V；任务 [02](./issues/02-log-ui.md)、[03](./issues/03-branches-and-remotes.md)；[只读历史契约检查](./history-contract-checks.md)；[继承状态](./inherited-state.md)。
4. 现有代码：任务 01 的 `src-tauri/src/history.rs`、`src-tauri/src/git/history.rs`；M3 的提交流程与进行中操作识别；M4 的写入编排 `src/features/merge/mergeWrite.ts`、`src/lib/refreshGate.ts`；回收区 `src-tauri/src/recovery.rs`；diff 组件与合并页签。

### 实施要点

1. 界面与后端都由你实现。外观延续 M4（VS Code 颜色主题、Codicons、Material Icon Theme、现有组件与样式变量），不另起视觉风格；文案集中在 `src/lib/i18n/zh-CN.ts`，用术语表的词。
2. 新界面放进独立的功能目录与 hook（例如 `src/features/log/`、`src/features/branches/`），`App.tsx` 只做接线，不继续堆大。
3. Git 调用只在 Rust 层：前端只传结构化参数（筛选条件、分支名、远端名、选项），由 Rust 组合固定参数；新类型用 ts-rs 生成，`src/lib/ipc.ts` 只做固定类型封装。
4. 所有写操作（检出、新建分支、fetch、更新、推送）照 B01：全局写门禁、关闭 / 退出保护、结果归属、结果待核对、先存回收区。前端编排照 `runMergeWrite` 的模式写成纯模块，App 与 Node 回归共用。
5. 回归按规格 V 加在仓库测试里：Rust 放 `src-tauri/tests/` 或模块单元测试，远端用本地裸仓库（`file://`）；需要缩短超时等情况时加测试接缝。所有复现和验证都写成仓库内的测试，用 `cargo test` / `node --test` 运行。
6. 规格没写到的交互：Rebased（IntelliJ）的做法清楚时照做；不清楚时取最简单、可撤销的做法，在报告中列出，不自行扩大范围。

### 验证与交付

- 先 `pnpm install --frozen-lockfile`；新 Rust 类型用 `pnpm bindings` 生成（必要时 PATH 加 `/opt/homebrew/bin`）。
- 全部修改完成后集中运行：`pnpm exec tsc -b`、`pnpm lint`、`pnpm build`、`/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml`、逐个 `node --test tools/test-*.mjs`。保存精确命令、退出码和原始输出；修复失败后只重跑受影响的检查。
- 更新 `report.md`：新增“第二批”一节，写实现位置、接口、各验收项对应的测试、资源与超时限额、未完成项与需要确认的问题。真实桌面、Rebased 对照、Release 打包、D06 由规划方后续做，写明未验收。
- 在任务 02、03 的验收项中勾选已完成项（规划方审查与桌面对照两项不勾），`## Comments` 末尾各追加一条交付说明。
- 按明确文件暂存并提交到 `m5-batch2`，提交说明以 `M5：` 开头，可以分几个提交；最后给出提交 SHA。
