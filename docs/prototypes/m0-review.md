# M0 计划评审

更新日期：2026-10-19。状态：**用户已同意**。按本文调整后即可开始 M0。

本文回应 M0 会话提交的计划。原计划包括：在仓库根目录用 create-tauri-app 搭工程并配置 Tailwind 和 shadcn/ui；Rust 层提供 `get_diff`、`highlight_syntect`、`resolve_worktree_env` 三个操作和一个 `env-check` 验证程序；前端自绘虚拟列表，支持并排和统一视图，带三档高亮和测量面板；用脚本生成测试 fixture；最后写报告。下文没有提到的部分，按原计划执行。

## 环境

- **Rust**：已装好，是 Homebrew 版的 rustc / cargo 1.99.1。M0 直接使用，不要再装 rustup。
- **平台**：只支持 Apple Silicon（arm64），见 [docs/README.md](../README.md) 的 Q44。只构建 arm64，不需要 x86_64 或 universal 目标。
- **旧分支**：`docs/design-direction` 已经合并进 main，只是没删，不用管。
- **工作位置**：主目录还有其他会话在用，不要在主目录里切换分支。从最新的 main（包含本文所在的提交）新建独立 worktree，在其中工作：

  ```bash
  git worktree add ../Revia-m0 -b m0-prototype main
  ```

## 对两个问题的答复

1. **安装 rustup**：不需要，见上文。
2. **引入 tauri-plugin-shell**：不要引入。这个插件主要用于给前端开放执行命令的能力，和 [ADR-0002](../adr/0002-git-cli-only.md) 冲突；只在 Rust 里用的话，和直接启动进程没有区别，也不会减少开销。Rust 里直接用 `std::process::Command`（需要异步时用 `tokio::process`）调用 git，不经过 shell。前端不能有任何执行命令的入口。

## 需要调整的地方

1. **工程位置**：放在 `prototypes/m0/`，不放在仓库根目录。正式工程到 M1 再初始化，避免两套高亮、测量面板这类原型代码混进正式工程。
2. **不装 shadcn/ui 和 Tailwind**：组件基座要在正式初始化工程时统一选定，不要在原型里顺手定下来，而且 M0 也用不上。
3. **git 的运行环境**：所有 git 调用都使用 `resolve_worktree_env` 解析出的环境。报告里写明实际用到的 git 路径和版本。
4. **git diff 参数**：固定加上 `--no-color --no-ext-diff --no-textconv --src-prefix=a/ --dst-prefix=b/`，路径参数前一律加 `--`，避免用户的 Git 配置或特殊文件名影响解析。
5. **高亮的正确性**：不要逐行孤立地高亮，否则多行注释、模板字符串会被标错颜色。应对旧、新两个版本的完整文件做高亮，再映射到 diff 行。即使只高亮可见行，也要验证这类情况显示正确。
6. **fixture 的语言**：除了 TypeScript，再加入 `.vue` 和 `.swift` 文件（用户的真实项目就是这两类），对比 Shiki 和 syntect 对 Vue 单文件组件和 Swift 的高亮效果。
7. **性能测量**：用 release 构建测量。syntect 在 debug 构建下慢很多，对比会失真。首屏耗时拆成 Rust 解析、IPC 传输、前端渲染三段，分别记录。
8. **shell 超时**：`zsh -l -i -c` 要加超时（例如 5 秒）和失败时的回退方案，并在报告里记录实际耗时。
9. **Node 版本验证**分两个场景：
   - 仓库带 `.node-version`（v24.11.1）：husky hook 里比较 `node -v` 和 `.node-version`，不一致就让 hook 失败，以此证明拿到的是项目指定的版本，而不是默认版本。
   - 仓库没有 `.node-version`：hook 应该拿到 fnm 的默认版本 v26.5.0。
10. **真实启动验证**：除了用剥离环境的 `env-check` 验证，最后至少从 Finder 双击构建出的 `.app`，实测一次 hook 和 mergetool。
11. **fixture**：生成脚本提交进仓库，生成出来的文件不提交。

## 交付

- 报告写入 `docs/prototypes/m0-report.md`，提交到 `m0-prototype` 分支。
- 在 `AGENTS.md` 中补充开发、构建和运行命令。
- 完成 M0 后停下：不推送，不开 PR，不开始 M1。
- 报告完成后告诉用户“M0 报告已就绪”。
