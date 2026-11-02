# Revia 0.0.2

面向 macOS Apple Silicon 与 Windows x64 的早期试用版本。本版将 M7 扩展工作流和 M8 Windows 支持合入 main。

## 本次更新

- Windows 新增当前用户安装版与便携版；便携版将应用及 WebView2 数据保存在 exe 旁的 `data`，GitHub 凭据由当前用户的 Windows Credential Manager 保存。移动到其他电脑或用户后需重新登录。
- hooks 与内置终端优先使用 PowerShell 7，缺失时使用 Windows PowerShell；按 worktree 加载 profile，环境捕获失败或超时明确回退。
- 改善 Windows 路径、CRLF、AltGr 与 IME 处理；安装版提供签名更新，便携版提示下载新版。
- 包含 M7 的更改列表、可编辑工作区 diff、内置终端、克隆、GitHub PR、诊断导出与 Liquid Glass 界面。提交区固定于侧栏底部，GitHub 设置提供浏览器验证码授权和 Token 登录。

## 下载与安装

- macOS Apple Silicon：`Revia-0.0.2-macos-arm64.dmg`。将 `Revia.app` 复制到「应用程序」。
- Windows x64 安装版：`Revia-0.0.2-windows-x64-setup.exe`，默认仅为当前用户安装。
- Windows x64 便携版：`Revia-0.0.2-windows-x64-portable.zip`，解压后运行 `Revia/Revia.exe`；也提供单独的 `portable.exe`。便携目录须可写，系统须有 WebView2 Runtime。
- 两个平台均需系统 Git。macOS 暂无 Apple Developer ID 签名与公证，首次打开可能受 Gatekeeper 拦截。请从本仓库下载，并核对随附平台 SHA256 文件。
- `.sig`、`.app.tar.gz` 与 `latest*.json` 用于应用更新，更新包具有独立签名。便携版通过下载新版手动替换。

## 已知限制

- Windows 真实桌面尚未验收：安装、卸载、升级、便携启动、Credential Manager 登录、PowerShell profile/hooks、Mica、DPI、IME/AltGr 与 WebView2 性能仍待真机确认。CI 构建与回归结果不能替代这些验收。
- M7 尚未全部验收。既有首屏测量 238ms，超过 150ms 目标；玻璃与辅助功能、真实 GitHub 授权、正式更新安装/重启、浏览器下载后的 Gatekeeper 首次打开仍待完整验收。
- 本机完整 Rust 测试在 4 线程下通过，默认并发两轮出现失败，根因未确认；原始证据保留。
- 此版本适用于接受上述限制的早期试用，不代表全部里程碑验收通过。

[固定源码](https://github.com/GarlandQian/Revia/tree/v0.0.2)
