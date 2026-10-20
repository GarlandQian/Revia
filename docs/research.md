# 调研与初始方案

更新日期：2026-10-19。状态：背景资料。

本文记录初始方案中的调研事实和建议，供后续设计取舍参考。建议部分不代表已确认决策；已确认的选择以 [设计文档首页](./README.md) 为准。

## 已核实的事实

以下数据于 2026-10-19 通过 GitHub 仓库与发布页核对。

| 对象 | 事实 | 解读 |
| --- | --- | --- |
| Rebased | 基于 IntelliJ 平台的独立 Git 客户端，约 6,000 Stars（核对时 6,042），最新版本 1.1.22，仍在持续发布 | “脱离完整 IDE 使用 JetBrains Git 工具”有关注度，但不代表另一个同类产品一定能获得用户 |
| Rebased | Apple Silicon 安装包约 330 MiB | 精简实现有缩小体积的空间；安装包大小不等于运行内存，也不等于响应速度，性能优势需要实测 |
| GitButler | 桌面端基于 Tauri，主要语言为 Rust、TypeScript、Svelte，已提供提交整理、分支管理等成熟功能 | Tauri 技术上可行，但不构成差异化 |
| GitButler | 主许可证为 FSL-1.1-MIT（Functional Source License），包含竞争用途限制 | 可以参考架构思路，不应默认能复制代码做同类产品 |
| JetBrains | 提交时提供两种工作方式：Changelists（默认）和 Git 暂存区（需在设置中开启） | 两者机制不同，见下文 |

初始方案还提到 Rebased 用户 Issue 中出现了 SSH 远程仓库、worktree 操作、差异审阅标记等需求，以及若干性能问题报告。这些需求未逐条核实；性能个案也不能直接证明换成 Tauri 就会解决。

### Changelists 与暂存区

JetBrains 的 Changelists 按文件内容的范围把改动分组，修改默认自动纳入，提交时按 Changelist 选择；Git 暂存区则是 Git 原生的 index，按文件或按块显式暂存。两者最终效果相近，但状态模型不同，不能混为一谈。暂存区方式允许同一文件的改动分批提交，并直接显示哪些内容已经暂存。

## 初始方案的建议

以下均为建议，尚未逐项确认。

### 定位

不做“Tauri 版 Rebased”，而是借鉴 JetBrains 的 Git 操作关系，做一个遵循标准 Git 工作流、重视差异审阅和提交整理的轻量桌面客户端。三个特点：

- **审阅舒服**：差异显示清楚，键盘操作顺手，快速切换文件，支持“已审阅”标记；文件再次变化时标记失效。
- **提交可信**：清楚区分未暂存、已暂存和最终提交内容，不偷偷改工作区，不自动进行危险操作。
- **项目切换高效**：多个仓库和 worktree 的分支、脏状态、进行中的 merge/rebase 一眼可见。

### 借鉴的工作流

借鉴的是操作关系，而不是照搬窗口外观。

| 工作流 | 价值 | 建议 |
| --- | --- | --- |
| 文件列表 → 查看 diff → 选择变更 → 提交 | 每天最常用，决定是否愿意长期使用 | 第一优先级 |
| 按块暂存、取消暂存、检查即将提交的内容 | 避免把无关改动一起提交 | 第一版重点打磨 |
| 提交历史 → 选择提交 → 查看文件与差异 | 查问题、理解改动的基本能力 | 第一版提供基础能力 |
| 多仓库、worktree 切换与状态概览 | 适合并行开发 | 可以作为重点 |
| JetBrains 风格三方冲突解决 | 很有价值，但实现难度高 | 单独验证，分阶段做 |
| Changelists、任意提交拆分、可视化 rebase | 很强，但状态和边界复杂 | 暂缓 |

### 技术栈

初始方案建议 Vue 3 + TypeScript + Vite → Tauri 2 / Rust → Git CLI，理由之一是复用 Vue 经验。前端框架后来已确认改为 React（见首页 T1），因此“复用 Vue 经验”的理由不再适用。

Git 方面的建议：

- 先以 Git CLI 为核心执行引擎，不同时引入 git2、gix 等多套实现，便于沿用用户已有的 Git 配置、SSH 和凭证工具。
- 状态读取使用 `git status --porcelain=v2 -z` 等机器可读接口。
- Rust 层提供受控操作，前端不能任意拼接 shell 命令。
- 图形应用中的环境变量、认证弹窗、提交签名需要专门处理。

需要接受的现实：Tauri 是系统 WebView + Rust，不是 SwiftUI 原生控件界面，显示效果和编辑器性能仍需验证。

### 首版范围

- 建议包含：打开本地仓库、查看 diff、整文件和按块暂存、提交、基础历史、打开已有 worktree。
- 冲突先允许调用外部合并工具。
- 暂不加入：可视化 rebase、托管平台 PR、AI 功能、远程仓库（SSH）。

### 验证方式

先在自己的 Vue 和 Swift 项目中连续使用，再请几位熟悉 JetBrains Git 的开发者试用。判断标准不是界面是否漂亮，而是他们是否愿意每天用它审阅和提交。

## 已识别的难点

真正的难点不是做出提交列表，而是让用户敢把真实仓库交给 Revia。

初始方案提出：

1. **部分暂存正确性**：同一文件同时存在已暂存和未暂存改动时，不能误覆盖。
2. **并发变化**：编辑器、终端或其他工具同时修改仓库时，界面不能拿过期状态继续操作。
3. **冲突处理**：三方合并不只是三个编辑器，还涉及 base、冲突区间和最终写回。
4. **恢复边界**：不能把 reflog 宣传成“所有操作都能撤销”，未提交内容需要另外保护。

本次检查补充（第 5、6 条已由 Q21、Q22、Q25 处理，第 7 条已由 Q17、Q18 处理，见 [产品方向](./product-direction.md)）：

5. **Git hooks 的运行环境**：从 Finder 或 Dock 启动的应用不继承登录 shell 的 `PATH`。Vue 项目常见的 husky、lint-staged 等 pre-commit hook 依赖 `node`，在图形应用中可能找不到。lint-staged 默认还会在运行任务前临时隐藏部分暂存文件中的未暂存改动，结束后再恢复，这会直接影响“部分暂存正确性”。
6. **提交签名**：开启 `commit.gpgsign` 后，GPG 需要图形化的密码输入程序，SSH 签名依赖 SSH agent；需要在实际环境中验证。
7. **所见即所暂存**：界面显示的 hunk 与暂存时实际应用的补丁必须来自同一份差异结果。如果前端另用一套 diff 算法或开启“忽略空白”显示，用户看到的块可能和真正暂存的内容不一致。

## 来源

- [Rebased 仓库](https://github.com/detachhead/rebased) · [1.1.22 发布页](https://github.com/detachhead/rebased/releases/tag/1.1.22)
- [GitButler 仓库](https://github.com/gitbutlerapp/gitbutler) · [GitButler 许可证](https://github.com/gitbutlerapp/gitbutler/blob/HEAD/LICENSE.md)
- [JetBrains：提交与推送改动](https://www.jetbrains.com/help/idea/commit-and-push-changes.html)
- [JetBrains：解决冲突](https://www.jetbrains.com/help/idea/resolving-conflicts.html)
- [Git status 机器可读格式 v2](https://git-scm.com/docs/git-status#_porcelain_format_version_2)
- [Tauri 架构说明](https://v2.tauri.app/concept/architecture/)
