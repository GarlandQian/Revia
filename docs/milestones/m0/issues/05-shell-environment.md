# 05：按 worktree 解析运行环境

**What to build:** 从 Finder 启动应用后，每个 worktree 在自己的目录下启动一次交互式登录 zsh 取得环境变量；提交时 husky hook 使用项目指定的 Node 版本，`git mergetool --tool=vscode` 能找到 `code`。

**Blocked by:** 01

**Status:** done

- [x] 带 `.node-version`（v24.11.1）的仓库，hook 拿到 v24.11.1，版本不符时 hook 失败
- [x] 不带 `.node-version` 的仓库，hook 拿到 fnm 默认版本 v26.5.0
- [x] 5 秒超时与回退，记录解析耗时
- [x] 剥离环境与 Finder 启动两种方式都实测通过
