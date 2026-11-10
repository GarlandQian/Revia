# Revia 0.0.4

面向 macOS Apple Silicon 与 Windows x64 的早期试用版本。本版合入 M10，集中整理 worktree 刷新、运行环境和生成提交信息的生命周期，并修复验收中发现的归属与刷新问题。

## 本次更新

- **刷新与切换更可靠**：打开、手动刷新、环境就绪和写操作后的核对使用统一的目标归属。切换仓库后，旧快照和错误不会覆盖当前界面；写操作仍核对原来的 worktree。
- **重新加载环境后保持自动刷新**：修复新会话快照版本归零、导致文件列表拒收更新的问题。重载完成后，外部文件变化继续自动出现在列表中。
- **环境替换与等待统一处理**：集中处理登录 shell / PowerShell 环境的安装、会话替换、等待和失效。旧环境的迟到结果不会重新登记失效会话；环境解析期间释放共享生命周期锁。
- **生成提交信息时保留各仓库草稿**：修复生成中切换仓库时，取消恢复可能写入另一仓库草稿的问题。流式接收、取消、失败与一步撤销共享目标归属，迟到事件不会覆盖后来输入。

## 下载与安装

- macOS Apple Silicon：`Revia-0.0.4-macos-arm64.dmg`，将 `Revia.app` 复制到「应用程序」。
- Windows x64 安装版：`Revia-0.0.4-windows-x64-setup.exe`，默认仅为当前用户安装。
- Windows x64 便携版：`Revia-0.0.4-windows-x64-portable.zip`，解压后运行 `Revia/Revia.exe`；也提供单独的 `portable.exe`。便携目录须可写，系统须有 WebView2 Runtime。
- 两个平台均需系统 Git。macOS 暂无 Apple Developer ID 签名与公证，首次打开可能受 Gatekeeper 拦截；请从本仓库下载并核对随附平台 SHA256 文件。
- `.sig`、`.app.tar.gz` 与 `latest*.json` 用于应用更新。macOS 和 Windows 安装版可在应用内更新；便携版通过下载新版手动替换。

## 验证与已知限制

- M10 最终源码的本地完整 Rust 测试为 954 通过、0 失败、1 忽略；前端类型检查、lint、构建和相关 Node 行为测试通过。macOS 可见窗口已验证切换、手动刷新与环境重载后的文件监视，以及本机模拟 AI 的生成、取消、草稿归属和撤销。
- Windows 真实桌面与性能验收仍未完成：安装、卸载、升级、便携启动、Credential Manager、PowerShell profile/hooks、Mica、DPI、IME/AltGr 等仍待真机确认。CI 构建与回归不能替代这些验收。
- 生成提交信息尚未连接真实 AI 服务做桌面验收；本轮只使用本机模拟服务。
- 本轮未重新执行 M8/M9 的历史性能验收或长期运行检查，不将重构表述为已测得的性能提升。M7 及更早里程碑的既有未达标项保留。

[固定源码](https://github.com/GarlandQian/Revia/tree/v0.0.4) · [M10 验收报告](https://github.com/GarlandQian/Revia/blob/v0.0.4/docs/milestones/m10/report.md)
