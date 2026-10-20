# 技术方向

更新日期：2026-10-20。状态：已确认（T1、Q1、Q6、Q7、Q18、Q21、Q26、Q27、Q32、Q34、Q36、Q38、Q40–Q42、Q44–Q46、Q49–Q52、Q55–Q69、Q78、Q80、Q84）；M0 原型已完成，见 [M0 报告](./milestones/m0/report.md)。

## 已确认的选择

```text
桌面框架：Tauri 2
前端框架：React
语言与构建：TypeScript + Vite
基础组件：shadcn/ui（Base UI 基座）
样式工具：Tailwind CSS
Git 执行：Rust 层调用系统 Git CLI
语法高亮：Rust 侧 syntect
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
- 基座选用 Base UI，与 shadcn/ui 当前的官方默认一致，不与 Radix 混用（Q51）。
- 颜色一律定义为 CSS 变量，从 M1 起跟随系统浅色 / 深色（Q50）。语法高亮按语法类别返回，由前端映射到两套配色，切换深浅色时不重新高亮。
- 组件源码进入项目后需要自行维护；升级依赖不等于自动同步所有本地组件源码的上游修复。
- 组件选型不构成性能或可访问性保证，完整交互仍需在实际 Tauri WebView 中验证。

## Git 执行边界

- 所有 Git 读写都由 Rust 层调用系统的 Git 命令行完成，不引入 git2（libgit2）或 gix，见 [ADR-0002](./adr/0002-git-cli-only.md)。
- 读取状态一律使用机器可读格式，例如 `git status --porcelain=v2 -z`。
- Rust 层只向前端提供固定、带类型的操作（例如“暂存这个 hunk”）。前端不能传入任意 Git 参数或 shell 命令。
- 界面显示的 hunk 由 Rust 层解析 `git diff` 输出得到；暂存、取消暂存、丢弃都使用同一个 hunk 生成的补丁，见 [ADR-0004](./adr/0004-hunks-come-from-git.md)。
- 每次写操作都附带用户看到的 diff 的指纹，执行前重新计算并比对，对不上就拒绝执行，见 [ADR-0005](./adr/0005-write-ops-carry-diff-fingerprint.md)。
- M2 的写操作按目标文件及相关版本、状态和路径计算校验范围，不因无关文件变化而拒绝；同一文件其他部分的相关变化仍使 hunk 操作失效，见 [ADR-0009](./adr/0009-file-scoped-write-fingerprints.md)（Q80）。M1 的全 worktree diff 摘要不能直接充当这一写操作契约。
- 遇到 `index.lock` 时短暂重试，仍被占用就提示用户；绝不删除锁文件（Q26）。
- 监视 worktree 和 `.git` 目录的变化，自动刷新状态（Q26）。
- Rust 层记录执行过的每条 Git 命令（参数、退出码、耗时、输出），供“Git 命令日志”面板显示，日志只保存在本机（Q36、Q42）。

## 运行环境

从 Finder 或 Dock 启动的应用不继承登录 shell 的环境变量，而 Git、hooks 和外部合并工具都依赖 `PATH`。

- **Git 程序**：使用登录 shell `PATH` 中的第一个 `git`，设置中可以手动指定路径；Revia 不自带 Git（Q27）。最低版本在实现时按实际用到的命令参数确定，启动时检查并提示。
- **环境变量**：每个 worktree 在自己的目录下启动一次用户的交互式登录 shell，读取环境变量；结果缓存，可以手动“重新加载环境”（Q21）。目标是让 hooks 的行为和在终端里进入该 worktree 后执行 `git commit` 一致。
- **原型验证场景**：开发者本机用 fnm 按目录切换 Node 版本（`.zshrc` 中为 `fnm env --use-on-cd`），node 的路径是每个 shell 会话临时生成的；VS Code 的命令行工具 `code` 位于 `/opt/homebrew/bin`。M0 已验证通过：按 worktree 解析后，husky hook 拿到了项目指定的 `node` 版本，没有 `.node-version` 时拿到默认版本，`git mergetool --tool=vscode` 能找到 `code`；从 Finder 启动时同样通过，解析耗时 116–204 毫秒（见 [M0 报告](./milestones/m0/report.md#五运行环境验证)）。
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

M0 结果（详见 [M0 报告](./milestones/m0/report.md)）：

- 首屏 207–228 毫秒，达标；其中 Rust 和界面之间的数据传输占 164–187 毫秒，是主要瓶颈。
- 并排和统一视图切换后停在同一行，达标。
- 滚动不卡顿，但快速滚动时可见区域上方或下方会短暂出现空白，留给 M1 解决。
- 语法高亮采用 Rust 侧 syntect（Q45），见 [ADR-0006](./adr/0006-syntect-for-highlighting.md)。

## diff 传输与渲染（M1）

M1 的第一项任务（Q46）：

- 不再一次性把整份 diff 序列化成 JSON 传给界面，改为按可见范围分批获取数据，与“只渲染可见区域”保持一致；必要时改用更紧凑的编码。
- 修复快速滚动时出现空白的问题。
- 验收标准，在有图形界面的正常桌面会话中、用 release 构建测量：
  - 使用 M0 的同一组测试数据，首屏在 150 毫秒以内；
  - 用触控板快速滑动时，不出现肉眼可见的空白；
  - 拖动滚动条大幅跳转时：目标行已取过数据，1 帧内出现文字；还没取过，1 帧内出现“加载中的行”，100 毫秒内出现文字（Q52）。
  - 是否露白以用户在真实桌面会话中的目测为准。没有图形合成的会话里，自动测量只能逐步模拟滚动，测不到系统先滚动画面、行稍后补上的情况，结果只作参考。
- **diff 区域由程序接管滚动**（Q68，[ADR-0008](./adr/0008-diff-view-owns-scrolling.md)）：真实桌面实测发现，浏览器原生滚动在快速甩动时会整块变白。改为程序直接接收触控板和滚轮事件，同一帧内同时更新滚动位置和要显示的行；滚动条和键盘滚动自己实现。
- **小文件后台预取全部行**（Q69）：2 万显示行以内的文件，首屏显示后在后台取完全部行数据；更大的文件仍按可见范围分批获取。
- M0 原型代码（`prototypes/m0/`）可以作为参考移植，原型中的 Shiki 实现不带入正式工程。

## M2 长期运行验证

Q78 明确了当前版本无法复现旧故障时的验收方式，详见 [产品方向](./product-direction.md#m2-长期运行验收)。必须使用同一个进程连续运行并观察 Rust 与 WebView 内存趋势，不把每个视图独立启动进程的现有短测当成长测。独立滚动慢帧继续留在 backlog，不擅自并入 Q53 的验收条件（Q84）。

## 分发

- 应用标识（Tauri `identifier`）为 `io.github.garlandqian.revia`，数据目录随之为 `~/Library/Application Support/io.github.garlandqian.revia/`（Q49）。签名、公证和自动更新都使用这个标识，更改需要迁移用户数据。
- 目标平台：macOS Apple Silicon（arm64）与 Windows x64；不做 Intel Mac、ARM 版 Windows 和 Linux，见 [ADR-0007](./adr/0007-target-platforms.md)。首版只发布 macOS，Windows 在 M5（Q55）。
- **构建**：GitHub Actions 分别在 macOS（Apple Silicon）与 Windows 机器上构建，不在 Mac 上交叉编译 Windows 版。推送到 main 时只做编译检查（从 M2 起包括 Windows）；打版本标签才生成安装包和便携包（Q62）。本机开发仍可用 Homebrew 安装的 Rust 构建 macOS 版。
- **Windows 安装版**：NSIS 生成的 `.exe` 安装程序，默认只为当前用户安装、不需要管理员权限；不做 MSI（Q57）。
- **Windows 便携版**：同时提供单个 exe 和 zip，zip 内是一个 `Revia` 文件夹（Q66）；解压即用；设置、回收区、审阅标记等数据放在 exe 旁边的 `data` 文件夹，不写用户目录和注册表；目录不可写时提示用户把程序移到可写的位置；依赖系统自带的 WebView2；不自动更新，只提示新版本（Q58）。
- **安装包命名**（参考 PiPilot，Q65）：

  | 产物 | 文件名 |
  | --- | --- |
  | macOS | `Revia-<版本>-macos-arm64.dmg` |
  | Windows 安装版 | `Revia-<版本>-windows-x64-setup.exe` |
  | Windows 便携版 | `Revia-<版本>-windows-x64-portable.zip`、`Revia-<版本>-windows-x64-portable.exe` |
  | 校验文件 | `macos-SHA256SUMS.txt`、`windows-SHA256SUMS.txt` |

  GitHub 发布页的标签为 `v<版本>`，标题为 `Revia <版本>`。自动更新所需文件（Tauri 使用 `.app.tar.gz` 等格式）的命名到 M4 实现自动更新时再定。应用标识保持 `io.github.garlandqian.revia`（Q49、Q67）。
- **平台相关代码**：从 M2 起集中放在少数模块中（例如登录 shell 的环境解析、窗口外观、快捷键修饰键），其他代码不写死 macOS 假设（Q55）。
- **签名**：暂不做任何代码签名，首版完成几个月后再根据情况决定（Q63）。在此之前：Mac 版在别人的电脑上首次打开时，需要在“系统设置 → 隐私与安全性”中点“仍要打开”；Windows 版会出现 SmartScreen 提示，点“更多信息 → 仍要运行”。到时可考虑的方案：Mac 只能通过 Apple 开发者计划（个人每年 99 美元）签名和公证；Windows 可以申请 SignPath 基金会为开源项目提供的免费签名。
- 自用阶段不做自动更新。试用阶段使用 Tauri 的更新插件，从 GitHub Releases 获取新版本；启动时检查，提示后由用户确认安装，不静默更新（Q41）。更新插件需要另外生成一对更新包签名密钥，它是自己生成的，与代码签名证书无关。
- 不收集遥测，不联网上报。诊断信息由用户通过“帮助 → 导出诊断信息”手动导出，导出内容不包含文件内容和 diff（Q42）。

## 尚未确认的配套

| 项目 | 当前建议 | 状态 |
| --- | --- | --- |
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
