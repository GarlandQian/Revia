# M0 实施计划

更新日期：2026-10-19。状态：按 [m0-review.md](./review.md)（用户已同意）执行。

本文记录 M0 的实施计划：原计划、评审调整后的执行方式，以及验收与交付方式。本文不产生新的设计决策。

## 目标

只做 M0 的两件事，完成后停下：

1. **diff 渲染原型**：初始化最小 Tauri 2 + React + TypeScript + Vite 工程；Rust 层把 `git diff` 输出解析成 hunk 交给前端；前端自绘只读 diff 视图，只渲染可见区域，支持并排与统一视图切换；Shiki 与 syntect 各实现一版语法高亮；在 Tauri 实际 WebView 中按既定标准测量。
2. **运行环境**：按 worktree 在其目录下启动一次交互式登录 zsh 读取环境变量；用带 `.node-version` 和 husky pre-commit hook 的测试仓库验证 hook 能找到对应版本的 node；验证 `git mergetool --tool=vscode` 能找到 `code`。

## 工作位置

- 在独立 worktree 中工作：`git worktree add ../Revia-m0 -b m0-prototype main`。
- 工程放在 worktree 内的 `prototypes/m0/`，不在仓库根目录初始化正式工程。
- 不安装 rustup，使用 Homebrew 的 cargo / rustc 1.99.1。只构建 arm64（Q44）。

## 原计划（评审前）

- 用 create-tauri-app 搭 Tauri 2 + React + TS + Vite 最小工程，配 Tailwind + shadcn/ui。
- Rust 层三个操作：`get_diff`、`highlight_syntect`、`resolve_worktree_env`，外加一个无 GUI 的 `env-check` 验证程序。
- 前端自绘窗口化虚拟列表，并排 / 统一视图切换，三档高亮，内置测量面板。
- 脚本生成测试 fixture：10,000 行改动仓库、`.node-version` + husky 仓库、`merge.tool=vscode` 仓库。
- 在 Tauri WebView 中测量，写报告，补 AGENTS.md 命令。

## 评审调整后的执行方式

| 评审项 | 执行方式 |
| --- | --- |
| 工程位置 | `prototypes/m0/`，正式工程 M1 再初始化 |
| 组件基座 | 不装 shadcn/ui 和 Tailwind，原型里不定基座 |
| git 调用 | Rust 内 `std::process::Command`，不引入 tauri-plugin-shell，不经过 shell；前端无执行命令入口 |
| git 环境 | 所有 git 调用使用 `resolve_worktree_env` 解析出的环境；报告写明实际 git 路径和版本 |
| git diff 参数 | 固定 `--no-color --no-ext-diff --no-textconv --src-prefix=a/ --dst-prefix=b/`，路径参数前加 `--` |
| 高亮正确性 | 对旧、新两个版本的完整文件做高亮，再按行号映射到 diff 行；不逐行孤立高亮 |
| fixture 语言 | TypeScript + `.vue` + Swift，对比两套高亮对 Vue SFC 和 Swift 的效果 |
| 性能测量 | release 构建；首屏耗时拆成 Rust 解析、IPC 传输、前端渲染三段 |
| shell 超时 | `zsh -l -i -c` 加 5 秒超时；失败回退到最小环境并在报告记录耗时 |
| Node 版本验证 | 有 `.node-version`（v24.11.1）时 hook 内比较 `node -v`，不一致即失败；无 `.node-version` 时 hook 应拿到 fnm 默认 v26.5.0 |
| 真实启动验证 | 除剥离环境的 `env-check` 外，从 Finder 双击构建出的 `.app` 实测一次 hook 和 mergetool |
| fixture 生成 | 生成脚本提交进仓库，生成物不提交 |

## 实施步骤

1. 建 worktree 与工程骨架（`prototypes/m0/`），固定 Node/pnpm 与 Rust 依赖版本。
2. Rust 层：`resolve_worktree_env`（登录 zsh、NUL 分隔环境转储、缓存、超时回退）→ `get_diff`（受控参数、解析 hunk、返回 diff 指纹）→ `highlight_syntect`（syntect + two-face，对完整文件高亮）→ `env-check` CLI。
3. 前端：hunk 类型定义、窗口化虚拟列表、并排 / 统一视图、按行号映射高亮、lock / 二进制 / 超大文件默认收起、测量面板。
4. fixture 生成脚本与验证场景（两种 Node 场景、mergetool、10,000 行 diff、Vue/Swift 高亮样例）。
5. release 构建后在 Tauri WebView 中测量，采集测量面板数据与截图。
6. 写 `docs/milestones/m0/report.md`，在 `AGENTS.md` 补开发 / 构建 / 运行命令，提交到 `m0-prototype`。

## 验收与交付

- 按 [技术方向](../../technical-direction.md#diff-渲染原型) 的验收标准在 Tauri 实际 WebView 中测量并记录原始数据。
- 两套高亮的对比包含：正确性（多行注释、模板字符串、Vue SFC、Swift）、首屏影响、全量耗时、滚动表现。
- 报告写入 `docs/milestones/m0/report.md`；完成后提交到 `m0-prototype`，不推送、不开 PR、不开始 M1。
