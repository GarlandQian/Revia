# Revia 0.0.2

macOS Apple Silicon 试用更新。本版将 M7 已提交内容整合到 main，后续 Release 只使用 main 历史上的提交。

## 本次修复

- 提交区固定在侧栏底部；空列表和短窗口长列表滚动时保持可见。
- GitHub 账号设置补齐浏览器登录。设置页与 PR 面板共用验证码授权流程，自动打开默认浏览器，支持复制验证码、再次打开与取消；Token 登录保留。
- 完善授权取消、过期、轮询及迟到响应处理；同步中英文提示。
- 纳入 0.0.1 发布后的 Windows 测试兼容修正与 macOS Actions runner 更新。

## 下载与安装

下载 `Revia-0.0.2-macos-arm64.dmg`，将 `Revia.app` 复制到「应用程序」。本版只提供 macOS Apple Silicon 安装包，Windows 发布仍在 M8。

暂无 Apple Developer ID 签名与公证，首次打开可能受到 macOS 拦截。请从本仓库下载，并用随附 `macos-SHA256SUMS.txt` 校验。`Revia.app.tar.gz`、`.sig`、`latest.json` 和 `latest-prerelease.json` 用于应用更新，更新包有独立签名。

## 已知限制

- M7 尚未全部验收。既有首屏测量 238ms，超过 150ms 目标；本版不宣称性能问题已解决。
- 部分玻璃效果与辅助功能、真实 GitHub 账号授权、正式更新安装 / 重启、浏览器下载后的 Gatekeeper 首次打开仍待完整验收。
- 浏览器登录已验证入口、浏览器打开、复制与取消；这不等于真实账号授权与钥匙串验收完成。

[固定源码](https://github.com/GarlandQian/Revia/tree/v0.0.2)
