# M12 实施交接

在 Revia 的独立工作树 `/Users/garland/.codex/worktrees/selection-design/Revia` 的 `m12` 分支工作。先核对分支并记录起始 SHA。产品基线 `81c5ce9`，访谈决策至 `5650025`。读取 AGENTS.md、docs/agents/local-validation.md、本目录 spec.md 和 issues/01、02。使用本机 magpie Library 的 implement-spec 技能；沿用现有 m12 工作树、项目任务单和本地 Git 流程。

按 Q227–Q233 完成选区与预览解耦、目录折叠计数和选中历史差异按需加载。两个任务可独立实施，03 是规划方的两轴审查与桌面验收。覆盖生产入口，沿用现有测试。翻译遵循 x18n 工作流。所有相关修改完成后集中检查 TypeScript、lint、前端构建、翻译、相关 Node 行为与 Rust 真实 Git 回归，记录命令及退出码。Rust 使用 /opt/homebrew/bin/cargo，测试仓库和应用数据各用独占临时目录。

本轮原始证据根为 `/private/tmp/revia-m12-evidence.Xvl6wb3v`。保留原始输出与失败信息，报告写入本目录 report.md。实施提交仅在 m12，按具体文件暂存。规划方负责固定源码、macOS 桌面、复核与集成。
