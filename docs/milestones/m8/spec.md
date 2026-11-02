# M8：Windows x64 安装版与便携版

Status: ready-for-human
日期：2026-11-02
依据：Q55–Q61、Q63、Q65–Q67、Q126、Q200、Q202–Q206；ADR-0002、0004、0005、0007–0010、0012–0015。

## Problem Statement

M7 已合入 main，但当前 Windows 仅有编译接线和部分平台适配；用户尚不能获得具备正确运行环境、数据位置与更新行为的 Windows 安装版和便携版。

## Solution

在现有功能上补齐 Windows x64 产品交付。安装版为当前用户安装的 NSIS 程序；便携版提供单 exe 和内含 Revia 文件夹的 zip，数据放在 exe 旁的 data。两者使用 Windows Credential Manager。沿用相同业务交互与 Windows 平台外壳，并在真实 Windows 桌面完成最终验收。

## User Stories

1. 作为 Windows 用户，我希望无需管理员权限即可安装，以在当前用户下使用 Revia。
2. 作为便携版用户，我希望下载 exe 或解压 zip 即可启动，以免安装应用。
3. 作为便携版用户，我希望应用数据和 WebView2 数据保存在程序旁，以带走设置和草稿。
4. 作为用户，我希望两种版本均安全保留 GitHub 登录，以在重启后继续处理 PR。
5. 作为便携版用户，我希望知道登录属于当前 Windows 用户，以理解换机后需重新登录。
6. 作为用户，我希望数据目录无法写入时看到原因，以选择可写位置而非意外产生另一份数据。
7. 作为用户，我希望复用系统 WebView2，并在缺失时得到明确指引，以完成运行准备。
8. 作为开发者，我希望找到环境中的 Git 或手动指定 Git，以沿用自己的 Git for Windows。
9. 作为使用 fnm 的开发者，我希望 hooks 使用当前 worktree 的环境，以获得正确 Node 版本。
10. 作为终端用户，我希望优先使用 PowerShell 7，并在缺失时使用 Windows PowerShell，以保留 profile 行为。
11. 作为用户，我希望环境解析失败或超时时有提示和可见回退，以判断 hooks 的实际环境。
12. 作为用户，我希望中文、空格、大小写别名与常见 Windows 路径安全可用，以正确定位 worktree。
13. 作为用户，我希望暂存、丢弃、恢复和编辑保存尊重仓库换行策略，以避免额外改动。
14. 作为键盘用户，我希望看到并使用 Ctrl / Alt / Shift 快捷键，以按 Windows 习惯操作。
15. 作为输入用户，我希望 AltGr、输入框和终端的按键不触发后台写入，以正确输入文字。
16. 作为用户，我希望原生标题栏、最大化、拖动与贴靠正常，以使用 Windows 窗口管理。
17. 作为用户，我希望 Windows 11 显示 Mica，旧系统与辅助功能设置保留清晰回退，以阅读代码。
18. 作为用户，我希望缩放、深浅色、弹层与镜片在 WebView2 中正确呈现，以完成日常审阅。
19. 作为安装版用户，我希望确认下载与安装重启后更新，以保护未保存编辑、Git 写入和终端。
20. 作为便携版用户，我希望只得到新版本提示与下载位置，以自行替换应用。
21. 作为发布者，我希望 Actions 为两平台汇总完整产物、签名和更新清单，以避免互相覆盖。
22. 作为下载者，我希望按约定命名和 SHA256 校验文件核对下载，以确认文件完整。
23. 作为维护者，我希望清楚区分本机自动检查与真实 Windows 验收，以知道交付边界。

## Implementation Decisions

### A. 分发与数据边界

- 仅 Windows x64 MSVC，NSIS 安装版默认 currentUser，不要求管理员权限，不做 MSI。不作 Apple / Windows 开发者代码签名；updater 验签仍强制。
- 安装版使用当前用户的标准应用数据目录。便携版用可稳定识别的独立构建形态，改名后仍保持便携语义；单 exe 与 zip 内的程序行为一致。
- 便携的业务数据、缓存和 WebView2 用户数据在 exe 旁 data，首次建窗前配置完成。可写性失败明确终止，不回退用户目录；测量和验收隔离仍显式控制。
- GitHub 凭据是 Q204 / ADR-0015 规定的唯一系统存储例外；沿系统凭据库安全保存并轮换，不引入明文凭据后端。Git 传输身份和用户 Git 配置保持既有规则。
- 依赖系统 WebView2；安装版使用既有 Tauri 支持的运行时准备机制，便携版缺运行时时明确指引，不偷偷安装或向用户目录转存应用数据。

### B. Git 与 PowerShell 环境

- Git 优先手动路径，否则使用解析后 PATH 中的 Git；找不到时提示手动指定或准备 Git for Windows，不打包 Git，不修改 PATH 或 Git 配置。
- Windows hooks 环境与内置终端共用 shell 选择：PowerShell 7 → Windows PowerShell；按目标 worktree 加载版本对应的用户 profile。环境缓存按 worktree，重新加载明确失效旧缓存。
- 捕获保持既有硬超时；profile 输出不能污染环境协议，Unicode 与包含换行/等号的变量无损传递。尊重现有执行策略，失败、超时或策略阻止时显式标出回退及诊断。
- 内置终端保持真实 PTY、会话归属、尺寸和关闭协调，不重放命令；普通 Git 子进程不弹出多余控制台窗口。

### C. 路径、换行与写入契约

- 沿 Windows 文件身份与 canonical 路径核对；外部进程接收可用的 Windows 路径，不把同一路径的大小写别名当不同 worktree，也不放宽仓库范围校验。
- 中文、空格、常见带特殊字符路径、盘符和 UNC / verbatim 路径在相应生产边界正确处理；不以手工字符串拼 shell 参数代替独立参数。
- LF / CRLF 和 gitattributes/core.autocrlf 的行为以真实 Git 为准，保留编辑文件原换行，不为兼容而改用户设置。所有暂存、丢弃、恢复、保存仍校验指纹和身份、保护回收内容、保留结果待核对。

### D. 平台外壳

- 沿 Q200 / Q202：原生 Windows 标题栏、Windows 11 Mica、Windows 10 CSS 回退、共享浮动卡片与镜片；不重做已完成的 macOS 界面。
- Ctrl / Alt / Shift 的提示、匹配和原生菜单一致；输入框不执行 Ctrl+Alt 业务快捷键，AltGr 不触发后台操作。
- 深浅色、减少透明度 / 动态效果、高对比度与 100% / 125% / 150% 缩放保持可读可操作。真实效果与滚动性能留 Windows 桌面验收。

### E. 应用更新与 Actions

- 安装版沿既有签名、渠道、原字节缓存与确认流程；Windows installer 会退出应用，最终安装重启确认、草稿保存、Git 写入等待和终端关闭均在调用安装器前完成。回执明确区分待安装与新进程核对成功，不能在进程退出后假定代码继续运行。
- 便携版只检查和提示新版本；界面及后端均拒绝 updater 下载、安装与重启入口。提供对应便携下载入口，不用安装包替换便携 exe。
- main 推送只检查，版本标签才打包；本任务仅修改工作流，不自行推送、打标签、创建 Release 或发布。保留 main 来源与版本一致性门禁。
- macOS 与 Windows 独立构建，汇总后统一生成包含 darwin-aarch64 和 windows-x86_64 的完整更新清单，防止并发覆盖。保留现有稳定/预发布选择、HTTPS 和签名约束，Windows 具体产物格式核对锁定工具源码。
- Windows 产物：Revia-<版本>-windows-x64-setup.exe、Revia-<版本>-windows-x64-portable.exe、Revia-<版本>-windows-x64-portable.zip（内含 Revia 文件夹）、windows-SHA256SUMS.txt。不改变 macOS 命名或本轮版本号。

## Testing Decisions

- 用户已确认 Q206：复用现有 Rust 集成测试，从会话/命令等最高可行生产入口使用真实 Git、tempfile 独占仓库、独立应用目录和 profile；覆盖 Windows 运行环境、数据隔离、路径/CRLF、凭据存储选择及便携更新门禁。测试只使用自建数据和凭据替身，不读写真实凭据。
- 自动化验证关注外部结果和安全拒绝，不镜像实现。已有 Node 工具测试承接分发清单与更新生命周期；前端不引入新测试框架。
- 一批修改完成后集中检查。共享启动、环境与更新契约涉及全应用，交付时运行现有完整相关 Rust 目标、类型、lint、构建、x18n 及相关 Node 检查；失败只重跑受影响检查。保存固定源码、精确命令、exit 和原输出。
- 真实 Windows 清单覆盖安装/卸载/升级、无 WebView2 提示、便携改名与移动、数据实际落点、Credential Manager 重启登录、PowerShell/profile/hooks、IME/AltGr、窗口/DPI/辅助功能、核心 Git 工作流、WebView2 diff 首屏/跳转/滚动。使用固定 release 构建与可见窗口记录结果。
- 当前仅有 macOS；Windows 测试编译和真机未运行不得写通过。既有 D06 未达标事实和旧 backlog 不因 M8 擅自改成通过。

## Out of Scope

Linux、Intel Mac、Windows ARM 原生版、MSI、WSL 仓库后端、自带 Git 或固定版 WebView2、新功能或旧 backlog 清偿、生产凭据/签名配置变更、推送/PR/公开发布。

## Further Notes

基线、确认原文与官方来源见 planning；任务图见 issues；实施与规划方证据分开。最终 Windows 桌面未通过时保持 ready-for-human，m8 留独立分支，不合并 main。

当前进度（2026-11-02）：实施与一次 Pi 返修已交付，Codex 增量审查并接管修复 S3/X4。固定代码 83d4070；本机可行检查见 verification.md。Windows 编译与真实桌面尚未验证，故整体保持 ready-for-human。
