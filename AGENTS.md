# Revia 协作指南

Revia 是一个轻量的桌面 Git 客户端，核心工作流是“审阅 diff → 选择变更 → 提交”。目标平台是 macOS Apple Silicon 与 Windows x64（安装版、便携版）；首版只做 macOS，Windows 在 M5（[ADR-0007](docs/adr/0007-target-platforms.md)）。技术栈为 Tauri 2 + React + TypeScript + Vite + shadcn/ui + Tailwind CSS，所有 Git 操作都由 Rust 层调用系统 Git 命令行完成。

设计已全部确认，M0 原型已完成（[M0 报告](docs/prototypes/m0-report.md)），当前进行 M1。

## 开始前必读

1. [docs/README.md](docs/README.md)：已确认的决策（Q1–Q67）和决策树。
2. [GLOSSARY.md](GLOSSARY.md)：领域术语。文档、代码命名和界面文案都使用这里的术语，不使用 _Avoid_ 中列出的说法。
3. [docs/adr/](docs/adr/)：架构决策，必须遵守。
4. 按任务需要阅读 [产品方向](docs/product-direction.md)、[UI 方向](docs/ui-direction.md)、[技术方向](docs/technical-direction.md)。

## 当前阶段：M1 只读

只做 M1，内容见 [产品方向](docs/product-direction.md#实施顺序) 中的实施顺序，以及 [技术方向](docs/technical-direction.md) 中的“diff 传输与渲染（M1）”一节：

1. **先做**：改造 diff 传输，按可见范围分批获取数据，并消除快速滚动时的空白（Q46）。验收标准见技术方向。
2. **再做**：在仓库根目录初始化正式工程（Tauri 2 + React + TypeScript + Vite + shadcn/ui + Tailwind CSS），实现添加仓库、读取状态、文件列表、显示 diff。
3. **技术选择**：语法高亮使用 Rust 侧 syntect（[ADR-0006](docs/adr/0006-syntect-for-highlighting.md)）；shadcn/ui 基座用 Base UI（Q51）；应用标识为 `io.github.garlandqian.revia`（Q49）；界面从 M1 起跟随系统浅色 / 深色（Q50）。计划与评审见 [m1-plan.md](docs/milestones/m1-plan.md)、[m1-review.md](docs/milestones/m1-review.md)，必须按评审意见执行。
4. **参考代码**：`prototypes/m0/` 中的原型代码可以参考移植，Shiki 实现不带入正式工程。不要修改原型目录。

M1 只读，不做任何写操作：暂存、丢弃、提交属于 M2、M3。完成后停下，不要开始 M2。

## 文件交接

不同会话和工具之间通过仓库中的文件交接，不依赖用户转述。M0 的交接文件在 `docs/prototypes/`，从 M1 起放在 `docs/milestones/`：

| 阶段 | 文件 | 写入方 |
| --- | --- | --- |
| 计划 | `docs/milestones/m1-plan.md` | 实施方 |
| 评审意见 | `docs/milestones/m1-review.md` | 设计评审方，经用户同意 |
| 报告 | `docs/milestones/m1-report.md` | 实施方 |

- 实施方写好计划后停下。评审文件标明“用户已同意”后才能开始。
- 评审意见与计划冲突时，以评审意见为准；与已确认的决策冲突时，停下来询问用户。
- 实施方在独立的 git worktree 中工作（例如 `git worktree add ../Revia-m1 -b m1 main`），不要在主目录里切换分支。
- 交接文件只记录工作过程，不产生新的决策；新的决策仍以 [docs/README.md](docs/README.md) 为准。
- 之后的里程碑沿用同样的命名，例如 `docs/milestones/m2-plan.md`。
- **合并方式**：里程碑分支合并进 main 时压成一个提交，提交说明写成“M<N>：<内容>”（例如“M1：只读界面与 diff 流式取数”），中间过程的提交只留在里程碑分支上；设计决策每轮一个提交。实施方只提交到自己的里程碑分支，合并由评审方在用户验收后进行。

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
- **不写死 macOS 假设**：从 M2 起，平台相关的代码（登录 shell 环境解析、窗口外观、快捷键修饰键等）集中放在少数模块中，其他代码保持平台无关（Q55）。
- **Git 调用只放在 Rust 层**：前端只调用固定、带类型的操作，不拼接 Git 参数或 shell 命令。
- **推送和 PR**：除非用户明确要求，不推送，不开 PR。
- **语言**：文档和界面文案使用简体中文。

## 文档维护

- 新确认的领域术语加入 [GLOSSARY.md](GLOSSARY.md)，只写术语定义，不写实现细节。
- 只有迁移成本高、需要背景解释且确有取舍的决定才写 ADR，编号在 `docs/adr/` 现有最大编号上递增。
- 工程初始化后，在本文件中补充开发、构建和测试命令。

## 开发与构建命令（M0 原型，仅供参考）

M0 原型工程位于 `prototypes/m0/`。正式工程初始化后，在本节之前补充正式工程的命令。

```bash
# 1. 准备测试 fixture（生成 4 个测试仓库，生成物不进版本库）
prototypes/m0/fixtures/setup.sh /tmp/revia-m0-fixtures

# 2. 依赖安装与前端检查
cd prototypes/m0
pnpm install
pnpm exec tsc --noEmit

# 3. 运行开发环境
pnpm tauri dev

# 4. Rust 单元测试与集成测试
cargo test --manifest-path src-tauri/Cargo.toml

# 5. Release 构建与打包（输出 .app 与 .dmg）
pnpm tauri build

# 6. 运行环境验证 CLI（模拟 Finder 剥离环境 / Finder 双击实测）
REVIA_M0_FIXTURES=/tmp/revia-m0-fixtures src-tauri/target/release/revia-m0 env-check hook
open "src-tauri/target/release/bundle/macos/Revia M0 Prototype.app" --args env-check mergetool
```

