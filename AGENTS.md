# Revia 协作指南

Revia 是一个照 Rebased 的布局和功能、配上 VS Code 外观的轻量桌面 Git 客户端（[ADR-0011](docs/adr/0011-rebuild-like-rebased-not-fork.md)），核心工作流是“审阅 diff → 选择变更 → 提交”。目标平台是 macOS Apple Silicon 与 Windows x64（安装版、便携版）；首版只做 macOS，Windows 在 M8（[ADR-0007](docs/adr/0007-target-platforms.md)）。技术栈为 Tauri 2 + React + TypeScript + Vite + shadcn/ui + Tailwind CSS，所有 Git 操作都由 Rust 层调用系统 Git 命令行完成。

M0 原型（[M0 报告](docs/milestones/m0/report.md)）与 M1 只读界面（[M1 报告](docs/milestones/m1/report.md)）已完成；M2、M3 已于 2026-10-26 合入 main，M4、M5 已于 2026-10-28 带已知未达标项合入 main（Q154），部分验收未完成（见各自报告）。

## 开始前必读

1. [docs/README.md](docs/README.md)：已确认的决策和当前决策树。
2. [GLOSSARY.md](GLOSSARY.md)：领域术语。文档、代码命名和界面文案都使用这里的术语，不使用 _Avoid_ 中列出的说法。
3. [docs/adr/](docs/adr/)：架构决策，必须遵守。
4. 按任务需要阅读 [产品方向](docs/product-direction.md)、[UI 方向](docs/ui-direction.md)、[技术方向](docs/technical-direction.md)。

## 当前阶段：M6 第 1 步

M6“冲突与历史”的范围见 [产品方向](docs/product-direction.md#实施顺序) 的里程碑表：stash；继续、中止、跳过；在日志中 cherry-pick、revert、reset、改提交信息、交互式 rebase；文件历史与 blame；外部合并工具保留为可选（Q126、Q143、Q149）。从下面工作流程的第 1 步开始，先与用户追问未定细节，再写 `docs/milestones/m6/spec.md`。

M4、M5 带已知未达标项合入 main（Q154）：Q98 长测、统一视图首次跳转与未做完的桌面验收在 `docs/milestones/backlog/` 04–06，不属于 M6 范围，除非用户在第 1 步中纳入。

布局与交互照本机 Rebased 1.1.22，外观沿用 VS Code 颜色主题（Q117、Q131–Q133）。旧决定冲突时按 Q129、Q131 处理，安全边界与 Revia 独有功能保持原决定。Pi 用 `paseo run --provider pi/magpie/cpa/GLM-5.3-Flash` 起会话，模型请求出错时按 Q137 换模型，返修按 Q113。

M2、M3 剩余与界面无关的验收继续进行，界面验收在 M4 新界面上重做（Q127）。不属于任何里程碑的遗留问题见 `docs/milestones/backlog/`。

## 工作流程

不同会话和工具之间通过仓库中的文件交接，不依赖用户转述。需求规格、任务单和报告的位置与写法见 [docs/agents/issue-tracker.md](docs/agents/issue-tracker.md)。

从 M2 起，每个里程碑按下面的顺序进行。“规划方”是负责写文档、做规划、审查、验收和合并的 Claude 或 Codex；“实施方”是负责写代码的 Pi，由规划方在命令行中驱动（Q75）。用户只参与第 1 步。

| 步骤 | 谁 | 做什么 |
| --- | --- | --- |
| 1 | 规划方与用户 | 用 grill-with-docs 追问这个里程碑还没定的细节（包括测试接缝），更新决策、术语表和 ADR |
| 2 | 规划方 | 用 to-spec 写 `spec.md` |
| 3 | 规划方 | 用 to-tickets 拆成 `issues/`，标好依赖 |
| 4 | 实施方 | 在里程碑分支 `m<N>` 上实现（有并行会话时放在独立 worktree）：整份规格用 implement-spec，集成分支就是 `m<N>`；单个任务用 implement。规划方用 `paseo send <会话> --prompt-file <任务说明>` 把任务发给 Paseo 中 Revia 工作区的 Pi 会话，再用 `paseo wait` 等它完成。模型按 GLM-5.3-Flash → GLM-5.3 → deepseek-v4.1-flash → gemini-3.8-flash 的顺序，当前模型请求出错就换下一个，四个都失败由规划方接手（[Q137](docs/README.md)）；返修最多一次（Q113）；保留已有改动和失败原件 |
| 5 | 规划方 | 用 code-review 审查 `m<N>` 相对 main 的改动，对照 `spec.md`；意见写进对应任务文件的 `## Comments` |
| 6 | 实施方 | 按 `## Comments` 修改，回到第 5 步 |
| 7 | 实施方 | 写 `report.md`：实测数据、验收结论、遗留问题 |
| 8 | 规划方 | 在真实桌面上验收：截图检查、在窗口可见时测量；`ready-for-human` 的任务由规划方用电脑操控完成（Q77） |
| 9 | 规划方 | 验收通过后，把 `m<N>` 压成一个提交合并进 main，删除 worktree（如有），告诉用户结果 |

- 规格和任务单只整理第 1 步确认过的内容，不需要用户另行确认；遇到第 1 步没覆盖的取舍，回到第 1 步补问用户（Q76）。
- 任务单与已确认的决策冲突时，停下来询问用户。
- 有并行会话时，实施方在独立的 git worktree 中工作（例如 `git worktree add ../Revia-m6 -b m6 main`），不要在主目录里切换分支；只有一个会话顺序工作时，直接在主目录建 `m<N>` 分支即可（Q154）。
- 交接文件只记录工作过程，不产生新的决策；新的决策仍以 [docs/README.md](docs/README.md) 为准。
- **合并方式**：里程碑分支合并进 main 时压成一个提交，提交说明写成“M<N>：<内容>”（例如“M1：只读界面与 diff 流式取数”），中间过程的提交只留在里程碑分支上；设计决策每轮一个提交。实施方只提交到里程碑分支，合并由规划方在验收通过后进行；用户可以随时要求撤回。
- M0、M1 当时用的是“实施方写计划 + 规划方写评审意见”的方式，它们的 `plan.md`、`review.md` 保留作记录。

## Agent skills

### 任务记录

需求规格、任务和交接文件以 markdown 形式放在 `docs/milestones/m<N>/`。见 `docs/agents/issue-tracker.md`。

### 领域文档

单一领域：根目录 `GLOSSARY.md` 加 `docs/adr/`。见 `docs/agents/domain.md`。

## 规则

- **本地验证**：写回归或复现测试、给 Pi 或子代理写任务说明、遇到平台风险提示时，先读 [本地验证与平台提示处理](docs/agents/local-validation.md)，明确业务目的、隔离范围、原始证据和验收分工。
- **不擅自修改已确认的决策**：已确认的决策和 ADR 不得自行修改。发现行不通时，在报告中写明证据和建议，由用户决定。
- **不扩大首版范围**：[产品方向](docs/product-direction.md#首版范围) 中“首版不提供”一列的内容不做。
- **不突破安全边界**，具体包括：
  - hunk 必须来自 Git；
  - 写操作执行前校验 diff 指纹；
  - 不删除 `index.lock`；
  - 不修改用户的 Git 配置；
  - 丢弃改动，以及中止、跳过或其他会覆盖工作区内容的操作之前，先存入回收区。

  完整列表见 [产品方向](docs/product-direction.md#安全边界)。
- **不写死 macOS 假设**：从 M2 起，平台相关的代码（登录 shell 环境解析、窗口外观、快捷键修饰键等）集中放在少数模块中，其他代码保持平台无关（Q55）。
- **Git 调用只放在 Rust 层**：前端只调用固定、带类型的操作，不拼接 Git 参数或 shell 命令。
- **推送和 PR**：除非用户明确要求，不推送，不开 PR。M2 的唯一例外是 Q97：为编译验收按需正常推送 `m2` 到现有 `origin` 并运行仅编译的 CI；不推送 `main`、不强推、不打标签、不创建 PR、不发布安装包。
- **语言**：文档和界面文案使用简体中文。

## 文档维护

- 新确认的领域术语加入 [GLOSSARY.md](GLOSSARY.md)，只写术语定义，不写实现细节。
- 只有迁移成本高、需要背景解释且确有取舍的决定才写 ADR，编号在 `docs/adr/` 现有最大编号上递增。
- 工程初始化后，在本文件中补充开发、构建和测试命令。

## 开发与构建命令（正式工程）

正式工程位于仓库根目录。

```bash
# 1. 准备测试 fixture（生成测试仓库到指定目录，生成物不进版本库）
tools/fixtures/setup.sh /tmp/revia-m1-fixtures

# 2. 依赖安装
pnpm install

# 3. 前端检查
pnpm exec tsc -b      # 类型检查（含 ts-rs 绑定）
pnpm lint             # oxlint
pnpm build            # 前端构建

# 4. 运行开发环境（如需编辑、构建、测试、运行完整应用；Tauri 前端产物在编译期嵌入二进制）
pnpm tauri dev

# 5. Rust 检查与测试
/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml

# 6. 构建 .app 与 .dmg（构建前先 pnpm build，脚本会重新编译以嵌入最新前端）
tools/build-app.sh release
open "src-tauri/target/release/bundle/macos/Revia.app"

# 7. 自动测量（首屏、跳转、滚动；每个视图一个进程）
REVIA_MEASURE=1 REVIA_MEASURE_ONLY=split \
REVIA_MEASURE_REPO=/tmp/revia-m1-fixtures/diffs REVIA_MEASURE_PATH=src/generated.ts \
REVIA_MEASURE_LOG=/tmp/split.json src-tauri/target/release/revia

# 8. 只读界面冒烟检查（添加仓库 → 文件列表 → 逐个打开 → 切换视图）
REVIA_SMOKE=1 REVIA_MEASURE_REPO=/tmp/revia-m1-fixtures/diffs \
REVIA_MEASURE_LOG=/tmp/smoke.json src-tauri/target/release/revia

# 9. 用 Icon Composer 改了 src-tauri/icons/Revia.icon 后，重新生成 Assets.car、icns、ico 和 PNG
python3 tools/export-app-icon.py
```

注意：

- 打包好的 `Revia.app` 直接双击运行时，前端资源已嵌入二进制，无需开发服务器；
  但改了前端代码必须重新 `pnpm build` 并重新编译 crate，否则跑的是旧界面。
- Rust 依赖来自 Homebrew，`cargo` 在非交互 shell 里不一定在 PATH，必要时用绝对路径 `/opt/homebrew/bin/cargo`。
- Git 调用只能在 Rust 层，前端不拼 Git 参数或 shell。

## 开发与构建命令（M0 原型，仅供参考）

M0 原型工程位于 `prototypes/m0/`。

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

