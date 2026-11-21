# Revia 0.0.5

面向 macOS Apple Silicon 与 Windows x64 的早期试用版本。本版合入 M11（AI 配置与模型获取）、M12（改动树选区与文件历史优化）与 M13（UI 全面重构、外观模式与代码颜色主题解耦、历史样式清理），并补齐多项交互与快捷键修复。

## 本次更新

- **实色现代桌面设计系统与主题解耦（M13）**：全面重构主外壳与组件样式，取消 Liquid Glass 与旧版 VS Code 外壳变量；界面外壳改为独立的浅/深色实色设计令牌（跟随系统 / 浅色 / 深色外观模式），代码 diff、三方合并与 Blame 表面独立跟随所选内置或导入的 VS Code 颜色主题。
- **精简界面结构与统一 Lucide 图标**：引入 `AppIcon` 统一图标体系；收拢 40px 顶部工具栏（仓库 / Worktree / 分支层级切换 + 更新/推送 + PR / 终端 / 设置），合并工作区 Diff 上下文栏，侧栏采用中性文件名 + 行尾状态徽标悬停切换快捷操作；「内置终端」与「Git 命令日志」收拢为底部可拖拽抽屉，PR 面板作为主区页签呈现；设置弹窗改为宽敞两栏卡片式布局。
- **改动树选区与文件历史优化（M12）**：多选与批量暂存/丢弃严格遵循可见展开顺序，预览选区外文件不破坏当前选区；文件历史改为按需读取当前所选提交的 diff，避免首次打开满页时批量请求。
- **AI 配置保存与模型自动获取（M11）**：修复 AI 设置保存与状态刷新流程，配置完地址与凭据后自动拉取可用模型列表。
- **交互与细节修复**：修复提交日志默认视图下错误横幅不可见、终端聚焦时快捷键无法收起抽屉、推送对话框强制推送确认区被下拉关闭遮挡、设置页修改数值清空搜索框与快捷键捕获按 `Esc` 关闭弹窗、Diff「更多」菜单点击后不关闭、非 100% 缩放拖拽高度跳变以及多处残留样式类失效问题。

## 下载与安装

- macOS Apple Silicon：`Revia-0.0.5-macos-arm64.dmg`，将 `Revia.app` 复制到「应用程序」。
- Windows x64 安装版：`Revia-0.0.5-windows-x64-setup.exe`，默认仅为当前用户安装。
- Windows x64 便携版：`Revia-0.0.5-windows-x64-portable.zip`，解压后运行 `Revia/Revia.exe`；也提供单独的 `portable.exe`。便携目录须可写，系统须有 WebView2 Runtime。
- 两个平台均需系统 Git。macOS 暂无 Apple Developer ID 签名与公证，首次打开可能受 Gatekeeper 拦截；请从本仓库下载并核对随附平台 SHA256 文件。
- `.sig`、`.app.tar.gz` 与 `latest*.json` 用于应用更新。macOS 和 Windows 安装版可在应用内更新；便携版通过下载新版手动替换。

## 验证与已知限制

- 本地完成 `pnpm exec tsc -b`、`pnpm lint`、`pnpm i18n:check`、`pnpm i18n:extract`、`pnpm build`、32 项前端单测（569/569 通过）、Rust 离线单测与核心集成测试、`portable` 特性编译检查及 macOS 真实窗口 `REVIA_SMOKE=1` 冒烟验证。
- Windows 真实桌面与性能验收仍待真机确认（安装、卸载、升级、便携启动、Credential Manager、PowerShell profile/hooks、DPI、IME/AltGr 等）；真实远端 AI 服务与 GitHub 线上账号流程尚未做桌面端到端验收。

[固定源码](https://github.com/GarlandQian/Revia/tree/v0.0.5) · [M13 重构报告](https://github.com/GarlandQian/Revia/blob/v0.0.5/docs/milestones/m13/report.md)
