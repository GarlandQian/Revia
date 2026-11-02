# Revia 0.0.1

macOS Apple Silicon 首个试用版本。核心流程为审阅 diff、选择变更与提交，提供 Liquid Glass 界面、简体中文 / English、内置终端、可编辑工作区 diff、克隆、GitHub PR 与更新入口。

## 下载与安装

- 下载 `Revia-0.0.1-macos-arm64.dmg`，将 `Revia.app` 复制到「应用程序」。本版不提供 Intel Mac 或 Windows 安装包。
- 暂无 Apple Developer ID 签名和公证，macOS 首次打开可能拦截。仅从本仓库下载并核对校验值，按系统「隐私与安全性」中的提示处理。
- `Revia.app.tar.gz`、`.sig` 和两个 `latest*.json` 为应用更新所用；更新包使用独立签名校验。

DMG SHA256：

```text
40d4775f5e119fbfd3b7a402a5416b2b471989edbb9f86f037e6d8e083f09225
```

## 已验证与已知限制

- GitHub Actions 已生成六项发布资产；下载后的 SHA256、更新包正式公钥验签及签名版本 `0.0.1` 均通过。
- 从 DMG 复制出的应用在独立数据目录中完成启动、读取 diff、暂存、提交与正式更新检查；未覆盖现有用户应用。
- 完整 M7 验收尚未完成：既有首屏测量 238ms，超过150ms目标；部分玻璃效果和辅助功能场景、真实 GitHub 账号授权、正式更新安装 / 重启、浏览器下载后的 Gatekeeper 首次打开仍待验收。
- 此版本适用于接受上述限制的早期试用，不代表全部里程碑验收通过。

## 构建来源

安装包由 [Release Draft 41284315737](https://github.com/GarlandQian/Revia/actions/runs/41284315737) 构建，固定源码为 `90166485279d5ae3df5b128d07538f6fdec40d80`，标签 `v0.0.1`。后续 m7 分支的 CI 修正不改变本版安装包。

[完整源码](https://github.com/GarlandQian/Revia/tree/v0.0.1)
