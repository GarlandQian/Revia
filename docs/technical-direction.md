# 技术方向

更新日期：2026-10-19。状态：已确认（T1、Q1、Q6、Q7、Q18、Q21、Q26、Q27、Q32、Q34、Q36、Q38、Q40–Q42、Q44）；语法高亮等少数选型以原型结果或实现时的核对为准。

## 已确认的选择

```text
桌面框架：Tauri 2
前端框架：React
语言与构建：TypeScript + Vite
基础组件：shadcn/ui
样式工具：Tailwind CSS
Git 执行：Rust 层调用系统 Git CLI
```

具体的小版本号在实现时确定，不因这一确认锁定。

## 为什么选择 React

初始方案推荐 Vue 3，理由是复用已有的 Vue 经验。最终选择 React，因为 shadcn/ui 官方只支持 React，而 shadcn-vue 是社区移植版；虚拟列表、可调整面板、命令面板等桌面工具常用组件，在 React 生态中的选择也更多。详见 [ADR-0001](./adr/0001-react-over-vue.md)。

## 为什么选择 shadcn/ui

小范围验证的主要投入应放在 Git 工作流，而不是从零补齐通用界面组件。shadcn/ui 提供可修改的组件源码和默认样式，其官方组件目录包含 Context Menu、Command、Resizable、Tooltip 等，可作为桌面工具界面的起点。

### 使用边界

- 复用基础组件，不直接套用网站后台模板。布局密度、行高、焦点反馈和快捷键提示应服务 Git 审阅场景，视觉要求见 [UI 方向](./ui-direction.md)。
- diff、提交列表、文件树和长列表的性能不会因为采用 shadcn/ui 而自动获得，需要分别设计或选型。
- 系统菜单栏和右键菜单使用 Tauri 提供的 macOS 原生菜单，不使用 shadcn/ui 的 Menubar 和 Context Menu（Q38）。
- 界面文案集中管理，不写死在组件中；首版只提供简体中文（Q40）。
- 组件源码进入项目后需要自行维护；升级依赖不等于自动同步所有本地组件源码的上游修复。
- 组件选型不构成性能或可访问性保证，完整交互仍需在实际 Tauri WebView 中验证。

## Git 执行边界

- 所有 Git 读写都由 Rust 层调用系统的 Git 命令行完成，不引入 git2（libgit2）或 gix，见 [ADR-0002](./adr/0002-git-cli-only.md)。
- 读取状态一律使用机器可读格式，例如 `git status --porcelain=v2 -z`。
- Rust 层只向前端提供固定、带类型的操作（例如“暂存这几个 hunk”）。前端不能传入任意 Git 参数或 shell 命令。
- 界面显示的 hunk 由 Rust 层解析 `git diff` 输出得到；暂存、取消暂存、丢弃都使用同一个 hunk 生成的补丁，见 [ADR-0004](./adr/0004-hunks-come-from-git.md)。
- 每次写操作都附带用户看到的 diff 的指纹，执行前重新计算并比对，对不上就拒绝执行，见 [ADR-0005](./adr/0005-write-ops-carry-diff-fingerprint.md)。
- 遇到 `index.lock` 时短暂重试，仍被占用就提示用户；绝不删除锁文件（Q26）。
- 监视 worktree 和 `.git` 目录的变化，自动刷新状态（Q26）。
- Rust 层记录执行过的每条 Git 命令（参数、退出码、耗时、输出），供“Git 命令日志”面板显示，日志只保存在本机（Q36、Q42）。

## 运行环境

从 Finder 或 Dock 启动的应用不继承登录 shell 的环境变量，而 Git、hooks 和外部合并工具都依赖 `PATH`。

- **Git 程序**：使用登录 shell `PATH` 中的第一个 `git`，设置中可以手动指定路径；Revia 不自带 Git（Q27）。最低版本在实现时按实际用到的命令参数确定，启动时检查并提示。
- **环境变量**：每个 worktree 在自己的目录下启动一次用户的交互式登录 shell，读取环境变量；结果缓存，可以手动“重新加载环境”（Q21）。目标是让 hooks 的行为和在终端里进入该 worktree 后执行 `git commit` 一致。
- **原型验证场景**：开发者本机用 fnm 按目录切换 Node 版本（`.zshrc` 中为 `fnm env --use-on-cd`），node 的路径是每个 shell 会话临时生成的；VS Code 的命令行工具 `code` 位于 `/opt/homebrew/bin`。原型需要确认：按 worktree 解析后，husky hook 能找到对应项目版本的 `node`，`git mergetool --tool=vscode` 能找到 `code`。
- 只在应用启动时解析一次环境（VS Code 的做法）更简单，但项目各自的 Node 版本不会生效，因此不采用。

## diff 渲染原型

首版技术风险最大的一块，在其他界面之前先做（Q32）。

- 不使用 Monaco、CodeMirror 的 diff 组件。它们以“传入两份文本、组件自己计算差异”为核心，与 ADR-0004 冲突。
- 自绘只读、只渲染可见区域的 diff 视图。
- 语法高亮在原型中对比两种方案：前端 Shiki 和 Rust 侧 syntect。
- 验收标准，在 Tauri 实际 WebView 中测量：
  - 1 万行改动的 diff 在 300 毫秒内显示出第一屏；
  - 滚动不卡顿；
  - 并排和统一视图来回切换时停在同一位置。
- lock 文件、二进制文件和超大文件默认收起，点开才渲染。

## 分发

- 只构建 Apple Silicon（arm64）版本，不提供 Intel 版或通用包（Q44）。因此 Homebrew 安装的 Rust 工具链即可满足构建需要，不需要通过 rustup 添加其他编译目标。
- 自用阶段不做应用签名。
- 进入试用阶段之前，用 Apple Developer 账号（每年 99 美元）做 Developer ID 签名和公证（Q34）。未签名的应用在其他人的 Mac 上会被提示“已损坏”。
- 自用阶段不做自动更新。试用阶段使用 Tauri 的更新插件，从 GitHub Releases 获取新版本；启动时检查，提示后由用户确认安装，不静默更新（Q41）。更新插件需要另外生成一对更新包签名密钥，与 Apple 的签名相互独立。
- 不收集遥测，不联网上报。诊断信息由用户通过“帮助 → 导出诊断信息”手动导出，导出内容不包含文件内容和 diff（Q42）。

## 尚未确认的配套

| 项目 | 当前建议 | 状态 |
| --- | --- | --- |
| shadcn/ui 组件基座 | 实现前统一选择并验证 | 尚未选定，不默认混用不同基座 |
| 语法高亮 | Shiki 或 syntect | 由 diff 渲染原型决定 |
| 文件列表与历史列表渲染 | 依据实际数据规模选择虚拟列表方案 | 尚未选定 |
| Git 最低版本 | 按实际用到的命令参数确定 | 实现时确定 |

不把 shadcn/ui 固定描述为某一种底层组件库的封装；基座选择以实际采用的官方安装路径和组件实现为准。

## 官方依据

以下页面于本次选型讨论中核实；列出来源不表示锁定其当前发布版本。

- [shadcn/ui Introduction](https://ui.shadcn.com/docs)：源码交付、默认设计与组件组合方式。
- [shadcn/ui Components](https://ui.shadcn.com/docs/components)：可复用的通用组件范围。
- [shadcn/ui Vite 安装](https://ui.shadcn.com/docs/installation/vite)：前端配套的官方安装路径。
- [Tauri 前端配置](https://v2.tauri.app/start/frontend/)：桌面框架与前端工程的集成边界。
- [Git status 机器可读格式 v2](https://git-scm.com/docs/git-status#_porcelain_format_version_2)：状态读取格式。
- [git mergetool](https://git-scm.com/docs/git-mergetool)：外部合并工具的调用方式。
- [husky：版本管理器与图形界面](https://typicode.github.io/husky/how-to.html)：图形界面中 hooks 找不到 `node` 的问题。
- [Tauri 更新插件](https://v2.tauri.app/plugin/updater/)：自动更新与更新包签名。
- [Tauri 菜单](https://v2.tauri.app/learn/window-menu/)：原生菜单栏与右键菜单。
