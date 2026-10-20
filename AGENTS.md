# Revia 协作指南

Revia 是一个轻量的 macOS Git 客户端，只支持 Apple Silicon（arm64），核心工作流是“审阅 diff → 选择变更 → 提交”。技术栈为 Tauri 2 + React + TypeScript + Vite + shadcn/ui + Tailwind CSS，所有 Git 操作都由 Rust 层调用系统 Git 命令行完成。

设计已全部确认，实现尚未开始。

## 开始前必读

1. [docs/README.md](docs/README.md)：已确认的决策（Q1–Q43）和决策树。
2. [GLOSSARY.md](GLOSSARY.md)：领域术语。文档、代码命名和界面文案都使用这里的术语，不使用 _Avoid_ 中列出的说法。
3. [docs/adr/](docs/adr/)：架构决策，必须遵守。
4. 按任务需要阅读 [产品方向](docs/product-direction.md)、[UI 方向](docs/ui-direction.md)、[技术方向](docs/technical-direction.md)。

## 当前阶段：M0 原型

只做 M0，内容和验收标准见 [技术方向](docs/technical-direction.md) 中的“diff 渲染原型”“运行环境”两节，以及 [产品方向](docs/product-direction.md#实施顺序) 中的实施顺序。

完成后停下，把测量结果和结论写入 `docs/prototypes/m0-report.md`，不要开始 M1 及之后的里程碑。

## 规则

- **不擅自修改已确认的决策**：已确认的决策和 ADR 不得自行修改。发现行不通时，在报告中写明证据和建议，由用户决定。
- **不扩大首版范围**：[产品方向](docs/product-direction.md#首版范围) 中“首版不提供”一列的内容不做。
- **不突破安全边界**，具体包括：
  - hunk 必须来自 Git；
  - 写操作执行前校验 diff 指纹；
  - 不删除 `index.lock`；
  - 不修改用户的 Git 配置；
  - 丢弃改动前先存入回收区。

  完整列表见 [产品方向](docs/product-direction.md#安全边界)。
- **Git 调用只放在 Rust 层**：前端只调用固定、带类型的操作，不拼接 Git 参数或 shell 命令。
- **推送和 PR**：除非用户明确要求，不推送，不开 PR。
- **语言**：文档和界面文案使用简体中文。

## 文档维护

- 新确认的领域术语加入 [GLOSSARY.md](GLOSSARY.md)，只写术语定义，不写实现细节。
- 只有迁移成本高、需要背景解释且确有取舍的决定才写 ADR，编号在 `docs/adr/` 现有最大编号上递增。
- 工程初始化后，在本文件中补充开发、构建和测试命令。
