# M0 需求规格：diff 渲染与运行环境原型

> 补写于 2026-10-20（Q71）。M0 已完成，本文根据 [计划](./plan.md)、[评审意见](./review.md) 和 [报告](./report.md) 整理，用来记录当时的需求，不是事前规格。

**Status:** done

## 问题

正式开发之前，有两个技术风险还没有验证，任何一个不成立都会推翻后面的设计：

1. 在 Tauri 的 WebView 里自己画一个只读 diff 视图，hunk 必须来自 Git（[ADR-0004](../../adr/0004-hunks-come-from-git.md)），能不能做到首屏快、滚动顺、切换视图不丢位置？语法高亮放在前端（Shiki）还是 Rust 侧（syntect）？
2. 从 Finder 启动的应用拿不到登录 shell 的环境变量。用户用 fnm 按目录切换 Node 版本，husky hook 能不能找到项目指定的 node？`git mergetool --tool=vscode` 能不能找到 `code`？

## 方案

在 `prototypes/m0/` 做一个一次性的原型，把这两个问题量化，给出测量报告和推荐方案。原型代码不进入正式工程，只作参考。

## 用户故事

1. 作为开发者，我想打开 1 万行改动的 diff 时 300 毫秒内看到第一屏，这样大改动也能马上开始审阅。
2. 作为开发者，我想快速滚动长 diff 时不卡顿。
3. 作为开发者，我想在并排视图和统一视图之间切换后停在同一行，不用重新找位置。
4. 作为开发者，我想看到的 hunk 就是 Git 生成的 hunk，这样以后按 hunk 暂存时“看到的就是要暂存的”。
5. 作为开发者，我想多行注释、模板字符串的高亮颜色正确，不被 diff 片段截断。
6. 作为 Vue 开发者，我想 Vue 单文件组件的 template、script、style 三部分都按各自语法着色。
7. 作为 Swift 开发者，我想 Swift 文件（包括多行字符串）高亮正确。
8. 作为开发者，我想 lock 文件、二进制文件和超大文件默认收起，点开才渲染，不拖慢其他文件。
9. 作为用 fnm 的开发者，我想从 Finder 打开应用后，提交时 husky hook 用的是项目 `.node-version` 指定的 Node 版本。
10. 作为用 fnm 的开发者，我想没有 `.node-version` 的项目使用 fnm 的默认版本。
11. 作为开发者，我想冲突时 `git mergetool --tool=vscode` 能找到 VS Code。
12. 作为开发者，我想读取 shell 环境卡住时有超时和回退，应用不会一直等。
13. 作为决策者，我想看到 Shiki 与 syntect 在速度、正确性、体积和工程复杂度上的对比，再决定首版用哪个。
14. 作为决策者，我想性能数据来自 release 构建，并拆成 Rust 解析、IPC 传输、前端渲染三段，知道瓶颈在哪。

## 实现决策

- 原型工程独立放在 `prototypes/m0/`，技术栈为 Tauri 2 + React + TypeScript + Vite；不装 shadcn/ui 和 Tailwind，组件基座留到正式工程再定。
- 所有 Git 调用都在 Rust 层，直接启动进程，不经过 shell，不引入 tauri-plugin-shell。`git diff` 固定使用 `--no-color --no-ext-diff --no-textconv --src-prefix=a/ --dst-prefix=b/`，路径前加 `--`。
- Rust 层把 `git diff` 输出解析成 hunk 交给前端，前端不计算差异。
- 前端自绘只读、只渲染可见区域的 diff 视图，支持并排和统一两种视图。
- 高亮对旧、新两个版本的完整文件进行，再映射到 diff 行；Shiki 与 syntect 各实现一版做对比。
- 每个 worktree 在自己的目录下启动一次交互式登录 zsh 读取环境变量，5 秒超时，失败时回退到最小环境；结果缓存。
- 所有 Git 调用都使用解析出的环境。

## 测试决策

- Rust 单元测试覆盖 diff 解析（多文件、行号、重命名、二进制、无换行标记）和高亮正确性（多行注释、模板字符串、分块请求与一次性请求结果一致、Vue 与 Swift 识别、未知扩展名回退纯文本）。
- 运行环境用无界面的验证程序在剥离环境下测试，再从 Finder 启动打包后的应用实测一次。
- 性能用 release 构建、应用内测量面板采集，结果写成 JSON。
- 测试仓库由脚本生成，脚本入库，生成物不入库。

## 不在范围内

- 正式产品界面、任何写操作。
- shadcn/ui、Tailwind CSS。
- Windows 及其他平台。

## 补充说明

- 结论见 [M0 报告](./report.md)：首屏 207–228ms 达标，瓶颈在 IPC；推荐 syntect；运行环境验证全部通过。
- 由此确认的决策：Q45（syntect，[ADR-0006](../../adr/0006-syntect-for-highlighting.md)）、Q46（M1 先改造 diff 传输）、Q47（原型并入 main 作为参考）。
- 任务单见 `issues/`。
