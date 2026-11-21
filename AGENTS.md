# Revia 协作指南

Revia 是一个轻量跨平台桌面 Git 客户端，核心工作流是“审阅 diff → 选择变更 → 提交”。界面采用克制、清晰的实色现代桌面设计系统，外壳与代码颜色主题解耦，保留全部 Git 工作流能力与顺手交互（[ADR-0011](docs/adr/0011-rebuild-like-rebased-not-fork.md)、[ADR-0016](docs/adr/0016-decoupled-design-system-and-solid-surfaces.md)）。目标平台是 macOS Apple Silicon 与 Windows x64（安装版、便携版，[ADR-0007](docs/adr/0007-target-platforms.md)）。技术栈为 Tauri 2 + React + TypeScript + Vite + Base UI + Tailwind CSS + Lucide，Git 操作由 Rust 层调用系统 Git 命令行完成，stash 删除/清空的窄范围例外见 [ADR-0014](docs/adr/0014-stash-deletion-transaction-exception.md)。

## 开始前必读

1. [docs/README.md](docs/README.md)：已确认的决策和当前决策树。
2. [GLOSSARY.md](GLOSSARY.md)：领域术语。文档、代码命名和界面文案都使用这里的术语，不使用 _Avoid_ 中列出的说法。
3. [docs/adr/](docs/adr/)：架构决策，必须遵守。
4. 按任务需要阅读 [产品方向](docs/product-direction.md)、[UI 方向](docs/ui-direction.md)、[技术方向](docs/technical-direction.md)。

## 当前阶段：M13 UI 全面重构已合入本地 main；0.0.4 已发布

M0–M13 均已合入本地 `main`，旧特性分支已清理。M13（UI 全面重构，Q234–Q245）已完成实色现代桌面设计系统落地、外壳与代码颜色主题解耦、Lucide `AppIcon` 图标体系、精简顶栏与两栏卡片式设置弹窗、底部终端/命令日志统一抽屉、全量历史样式/组件清理及自动化验证，详见 [M13 报告](docs/milestones/m13/report.md) 与 [M13 视觉稿 v1](docs/milestones/m13/mockups/revia-ui-v1.html)。

历史发布状态：0.0.4 已于 2026-11-11 公开为 latest 稳定版（Q225），应用源码固定为 `v0.0.4` / `97211ab`。Windows 真实桌面、真实 AI 与历史性能验收仍待补验。Q204–Q206 仍有效：两种 Windows 形态均用 Credential Manager；便携版其余数据和 WebView2 在 exe 旁 data；Windows shell 优先 PowerShell 7，回 Windows PowerShell，按 worktree 加载 profile。

## 工作流程

M13 起取消双 agent 并行派发与强制独立 `git worktree` 规则（Q245）。默认由当前会话在当前仓库直接切出本地分支 `m<N>` 完成规划、实现、自审与验证，验收通过后压成一个提交合入 `main` 并删除本地分支：

1. **追问定稿**：用 `grill-with-docs` 追问未定细节，同步更新 [docs/README.md](docs/README.md)、[GLOSSARY.md](GLOSSARY.md)、方向文档与必要的 ADR。
2. **视觉/方案确认**（涉及大幅 UI 改动时）：先出高保真浅/深色静态视觉稿供用户确认定稿，再动正式代码。
3. **直接实施与验证**：在当前仓库切出本地分支 `m<N>` 直接实现（不额外建 worktree），通过类型检查、lint、build、x18n 检查、前端单测与相关 Rust 测试，并在真实桌面验收。
4. **合并收尾**：验收通过后将 `m<N>` 压成一个提交合入 `main`（提交说明写成 `M<N>：<内容>`），删除本地 `m<N>` 分支并向用户汇报；用户可以随时要求撤回。

- 里程碑文档按需放在 `docs/milestones/m<N>/`（见 [docs/agents/issue-tracker.md](docs/agents/issue-tracker.md)），不产生未经确认的新决策；新的决策仍以 [docs/README.md](docs/README.md) 为准。

## Agent skills

### 任务记录

需求规格、任务和报告以 markdown 形式放在 `docs/milestones/m<N>/`。见 [docs/agents/issue-tracker.md](docs/agents/issue-tracker.md)。

### 领域文档

单一领域：根目录 `GLOSSARY.md` 加 `docs/adr/`。见 [docs/agents/domain.md](docs/agents/domain.md)。

## 规则

- **本地验证**：写回归或复现测试、遇到平台风险提示时，先读 [本地验证与平台提示处理](docs/agents/local-validation.md)，明确业务目的、隔离范围、原始证据和验收分工。
- **不擅自修改已确认的决策**：已确认的决策和 ADR 不得自行修改。发现行不通时，在报告中写明证据和建议，由用户决定。
- **不扩大首版范围**：[产品方向](docs/product-direction.md#首版范围) 中“首版不提供”一列的内容不做。
- **不突破安全边界**，具体包括：
  - hunk 必须来自 Git；
  - 写操作执行前校验 diff 指纹；
  - 不删除 `index.lock`；
  - 不修改用户的 Git 配置；
  - 丢弃改动，以及中止、跳过或其他会覆盖工作区内容的操作之前，先存入回收区。

  完整列表见 [产品方向](docs/product-direction.md#安全边界)。
- **不写死 macOS 假设**：平台相关的代码（登录 shell 环境解析、窗口外观、快捷键修饰键等）集中放在少数模块中，其他代码保持平台无关（Q55）。
- **Git 调用只放在 Rust 层**：前端只调用固定、带类型的操作，不拼接 Git 参数或 shell 命令。
- **推送和 PR**：除非用户明确要求，不推送，不开 PR。
- **语言**：文档使用简体中文；界面文案按简体中文 / English 设置显示（Q164），回复仍默认简体中文。

## 文档维护

- 新确认的领域术语加入 [GLOSSARY.md](GLOSSARY.md)，只写术语定义，不写实现细节。
- 只有迁移成本高、需要背景解释且确有取舍的决定才写 ADR，编号在 `docs/adr/` 现有最大编号上递增。

## 开发与构建命令

正式工程位于仓库根目录。

```bash
# 1. 准备测试 fixture（生成测试仓库到指定目录，生成物不进版本库）
tools/fixtures/setup.sh /tmp/revia-m1-fixtures

# 2. 依赖安装
pnpm install

# 3. 前端检查与构建
pnpm exec tsc -b      # 类型检查（含 ts-rs 绑定）
pnpm lint             # oxlint
pnpm i18n:check       # x18n 文案一致性检查
pnpm i18n:extract     # 检查是否有未抽取的硬编码中文
pnpm build            # 前端生产构建

# 4. 前端单元测试（32 个独立脚本）
node --test tools/test-*.mjs

# 5. 运行开发环境（Tauri 前端产物在编译期嵌入二进制）
pnpm tauri dev

# 6. Rust 检查与测试
/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml

# 7. 构建 .app 与 .dmg（构建前先 pnpm build，脚本会重新编译以嵌入最新前端）
tools/build-app.sh release
open "src-tauri/target/release/bundle/macos/Revia.app"

# 8. 自动测量（首屏、跳转、滚动；每个视图一个进程）
REVIA_MEASURE=1 REVIA_MEASURE_ONLY=split \
REVIA_MEASURE_REPO=/tmp/revia-m1-fixtures/diffs REVIA_MEASURE_PATH=src/generated.ts \
REVIA_MEASURE_LOG=/tmp/split.json src-tauri/target/release/revia

# 9. 只读界面冒烟检查（添加仓库 → 文件列表 → 逐个打开 → 切换视图）
REVIA_SMOKE=1 REVIA_MEASURE_REPO=/tmp/revia-m1-fixtures/diffs \
REVIA_MEASURE_LOG=/tmp/smoke.json src-tauri/target/release/revia

# 10. 更新内置代码颜色主题（修改 src/themes/sources/ 后）
node tools/gen-themes.mjs

# 11. 用 Icon Composer 改了 src-tauri/icons/Revia.icon 后，重新生成 Assets.car、icns、ico 和 PNG
python3 tools/export-app-icon.py
```

注意：

- 打包好的 `Revia.app` 直接双击运行时，前端资源已嵌入二进制，无需开发服务器；
  改了前端代码必须重新 `pnpm build` 并重新编译 crate，否则跑的是旧界面。
- Rust 依赖来自 Homebrew，`cargo` 在非交互 shell 里不一定在 PATH，必要时用绝对路径 `/opt/homebrew/bin/cargo`。
- Git 调用只能在 Rust 层，前端不拼 Git 参数或 shell。
