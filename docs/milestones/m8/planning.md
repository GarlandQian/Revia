# M8：Windows 规划与确认

Status: ready-for-agent
日期：2026-11-02

## 启动与范围

用户要求“先拉分支出来实现 m8”。从包含 M7 的 main `b9cf807` 创建分支 `m8` 与独立 worktree `/Volumes/T7 Shield/project/Revia-m8`。主目录仍在 main；原有 `.gitignore` 未提交改动保持原样。

M8 按 Q55–Q61、Q65–Q67、Q126、Q200、Q202：Windows x64 安装版和便携版、Windows hooks 环境、换行符与路径、Git for Windows、原生外壳与 WebView2 性能。不纳入旧里程碑 backlog，不发布其他架构或 Linux。

## 本轮已确认

1. 用户问“能都用 credential 吗”：安装版和便携版均以 Windows Credential Manager 持久保存 GitHub 凭据，作为 Q58 的明确例外；其他 Revia 数据和 WebView2 数据在便携 exe 旁的 `data`。凭据属于当前 Windows 用户，不随便携目录迁移。见 Q204。
2. hooks 环境和内置终端统一优先 PowerShell 7，无则 Windows PowerShell；按 worktree 加载对应用户 profile，捕获失败或超时显式提示并回退进程环境，不改 profile 或执行策略。用户选择“按推荐：优先 PowerShell 7”。见 Q205。
3. 沿现有 Rust 生产入口、真实 Git 临时仓库、独立 profile 和应用数据目录补 Windows 回归；Mica、WebView2 滚动、安装与便携启动由真实 Windows 桌面验收。当前 Mac 无法验证的项目保持未验收。用户选择“按推荐执行”。见 Q206。

规格和任务依 Q76 整理以上确认，不重复请求批准。Paseo / Pi 实现，Codex 审查与验证，模型顺序 Q137，正式返修至多一次 Q113。

## 事实核对（源码基线 b9cf807）

- 普通启动只用 `app_data_dir()`，尚无便携模式；WebView2 数据目录必须在建窗前配置，不能仅改业务数据目录。
- Windows 环境只继承进程环境，shell 与终端默认 ComSpec / cmd.exe；没有 PowerShell profile 捕获。
- Git 可手动指定或从环境 PATH 查找，已有 Windows 扩展名解析；不自带 Git，不改用户 Git 配置。
- 已有 Windows 路径身份与外部进程路径转换、Mica 接线、平台快捷键与部分 CRLF 回归；应在原基础补齐实际缺口。
- Release 工作流仅 macOS；更新清单仅 darwin-aarch64；没有 NSIS、便携包和 Windows 校验文件。
- 更新下载与安装没有便携门禁；便携版按 Q58 只提示新版本。
- M7 历史 CI 对固定 `51844d7` 的 Windows 765 passed / 0 failed / 1 ignored，仅是历史证据，不代表 b9cf807 或 M8/Windows 桌面通过。原记录见 M7 release-0.0.1-readiness。

## 官方机制（2026-11-02 核对）

- [Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/)：NSIS 与 WebView2 安装方式；依锁定版本核对具体配置。
- [fnm README](https://github.com/Schniz/fnm#shell-setup)：PowerShell 使用 profile 初始化 fnm，支持按目录 Node 版本；[原始 README](https://raw.githubusercontent.com/Schniz/fnm/master/README.md)已读取。
- [PowerShell profiles](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_profiles?view=powershell-7.5)：profile 按宿主与版本区分，执行策略可能禁止加载，路径应由 PowerShell 自身解析。
- [Tauri updater](https://v2.tauri.app/plugin/updater/)：Windows installer 安装会自动退出应用，最终用户确认及草稿、写入、终端协调必须在安装调用前完成；静态清单采用 windows-x86_64。产物实际格式以锁定 CLI/plugin 源码为准。

## 验收分工

Pi 完成代码、必要回归与本机可行检查，保存精确命令、退出码和原始输出。Codex 作两轴审查与固定候选复核。真实 Windows 验收缺环境时保留 ready-for-human，不以 macOS、headless 或旧 CI 代替；不为触发 CI 自行推送。
