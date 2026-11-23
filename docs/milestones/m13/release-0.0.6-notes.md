# Revia 0.0.6

面向 macOS Apple Silicon 与 Windows x64 的早期试用版本。本版基于 0.0.5（M13 UI 重构），重点修复 Windows 端提交栏与侧栏布局、改动文件列表只显示半截以及暂存操作入口不够显眼的问题。

## 本次更新

- **修复改动文件列表只显示半截的问题**：修复 `ChangesTree` 在初始空态挂载后 `ResizeObserver` 未重新挂载导致视口高度停留在 0、列表仅渲染前 9 行的问题；统一文件行左右定位与纵向滚动，避免横向滚动条与行内空白占位。
- **补齐显性暂存入口**：
  - 左侧「未暂存 / 未跟踪 / 已暂存」分组标题右侧的 `+ / -`（全部暂存 / 全部取消暂存）按钮改为常驻可见；
  - 改动树中选中的文件行常驻显示右侧 `+`（暂存）/ `-`（取消暂存）/ `↺`（丢弃）操作按钮，移开鼠标不消失；
  - 右侧 Diff 顶部上下文栏新增当前文件的 **`+ 暂存` / `- 取消暂存`** 按钮，审阅时可直接一键暂存。
- **修复 Windows 提交栏与侧栏布局及滚动条**：
  - 调整 `.commit-card` 弹性伸缩与输入区高度策略，避免在 Windows WebView2（Chromium）或拖拽调矮提交区时底部「提交 / 提交并推送」按钮被裁剪或挤压；
  - 紧凑化提交栏底部「已审阅」胶囊与提交按钮排版，并规范化非标准字重，在 Windows 微软雅黑字形和默认侧栏宽度下保持单行清爽展示；
  - 为改动树设置最小高度保护并为提交区增加默认最大高度上限，避免较矮 Windows 窗口下提交区把改动树挤没；
  - 修复侧栏顶部「提交 / Stash / 回收区」栏窄宽时右侧工具按钮溢出边界；
  - 在 Windows 平台启用细滚动条样式（`scrollbar-width: thin`），消除 17px 原生实心滚动条挤占宽度的问题；
  - 补齐提交信息框 `onKeyDown` 的输入法组合期守卫（`isImeComposing`）并修复提交按钮 tooltip 重复显示快捷键的问题。
- **其他维护**：解除 Rust `--lib` 单元测试对 `tauri::test::mock_builder()` 的依赖。

## 下载与安装

- macOS Apple Silicon：`Revia-0.0.6-macos-arm64.dmg`，将 `Revia.app` 复制到「应用程序」。
- Windows x64 安装版：`Revia-0.0.6-windows-x64-setup.exe`，默认仅为当前用户安装。
- Windows x64 便携版：`Revia-0.0.6-windows-x64-portable.zip`，解压后运行 `Revia/Revia.exe`；也提供单独的 `portable.exe`。便携目录须可写，系统须有 WebView2 Runtime。
- 两个平台均需系统 Git。macOS 暂无 Apple Developer ID 签名与公证，首次打开可能受 Gatekeeper 拦截；请从本仓库下载并核对随附平台 SHA256 文件。
- `.sig`、`.app.tar.gz` 与 `latest*.json` 用于应用更新。macOS 和 Windows 安装版可在应用内更新；便携版通过下载新版手动替换。

## 验证与已知限制

- 本地完成 `pnpm exec tsc -b`、`pnpm lint`、`pnpm i18n:check`、`pnpm i18n:extract`、`pnpm build`、相关前端单测、发布版本与清单预检及 `cargo check --features portable`。
- Windows 真实桌面交互、滚动条与提交栏视觉效果需在本次打包产物上由 Windows 实机确认；真实远端 AI 服务与历史性能验收仍待补验。

[固定源码](https://github.com/GarlandQian/Revia/tree/v0.0.6) · [M13 重构报告](https://github.com/GarlandQian/Revia/blob/v0.0.6/docs/milestones/m13/report.md)
