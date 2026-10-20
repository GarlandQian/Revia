# 01：生成测试仓库

**What to build:** 一条命令生成原型需要的全部测试仓库：1 万行改动量级的 TypeScript diff，Vue 与 Swift 文件，lock 文件、二进制文件、超大文件，未跟踪与已暂存文件；以及带 `.node-version`、不带 `.node-version`、配置了 `merge.tool=vscode` 的仓库。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 生成脚本提交进仓库，生成出来的仓库不提交
- [x] 覆盖上述所有文件类型与运行环境场景
