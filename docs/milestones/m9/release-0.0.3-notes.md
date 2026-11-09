# Revia 0.0.3

面向 macOS Apple Silicon 与 Windows x64 的早期试用版本。本版合入 M9：Windows 与 macOS 性能优化、生成提交信息。

## 本次更新

- **Windows 减少渲染负担**：Windows 11 去掉弹层、通知、对话框、日志表头等处的网页模糊和选中项玻璃镜片，打开对话框时不再模糊整个界面；保留侧栏、编辑区卡片和 Mica 窗口底。Windows 10 不再渲染折射和 WebGL 按钮特效。macOS 外观不变。
- **打开仓库不再等待环境加载（两个平台）**：先用已经找到过的 Git 立即读取状态、diff 和日志，完整的登录 shell / PowerShell 环境在后台加载。hooks、提交、fetch/pull/push 和内置终端会等环境加载完。打开和切换仓库时先显示文件列表，日志、分支等随后补上。
- **降低空闲开销**：诊断摘要改为攒批写盘；文件监视按 Git 的忽略规则跳过被忽略的文件；去掉空闲时每秒一次的整界面重绘。
- **生成提交信息**：在设置的“提交 › AI”中填写 OpenAI 兼容服务的地址和模型（密钥可选），提交信息框旁会出现“生成”。内容按下一次提交将包含的改动生成：暂存区、更改列表勾选的内容，或 amend 后的完整改动。文字流式写入，可以取消，也可以一步撤销；只生成文字，不自动提交。密钥保存在 macOS 钥匙串或 Windows Credential Manager，不写入设置文件、日志或诊断信息。每个服务地址第一次使用前，会说明要发送的内容。未配置时不显示“生成”。
- **诊断信息附带性能记录**：导出时附带打开耗时、帧时间、长任务、CPU 占用和电脑配置，方便反馈卡顿。

## 下载与安装

- macOS Apple Silicon：`Revia-0.0.3-macos-arm64.dmg`。将 `Revia.app` 复制到「应用程序」。
- Windows x64 安装版：`Revia-0.0.3-windows-x64-setup.exe`，默认仅为当前用户安装。
- Windows x64 便携版：`Revia-0.0.3-windows-x64-portable.zip`，解压后运行 `Revia/Revia.exe`；也提供单独的 `portable.exe`。便携目录须可写，系统须有 WebView2 Runtime。
- 两个平台均需系统 Git。macOS 暂无 Apple Developer ID 签名与公证，首次打开可能受 Gatekeeper 拦截。请从本仓库下载，并核对随附平台 SHA256 文件。
- `.sig`、`.app.tar.gz` 与 `latest*.json` 用于应用更新，更新包具有独立签名。0.0.2 安装版和 macOS 版可在应用内更新；便携版通过下载新版手动替换。

## 已知限制

- **Windows 性能还没有真机数据**：M9 为 Windows 定的门槛（空闲 CPU 低于 2%、滚动时 95% 的帧不超过 16.7ms、拖动和缩放窗口流畅、点击 100ms 内有反应）尚未实测。本版 Windows 的改动是否足够，以真机反馈为准；实色窗口底等进一步调整取决于实测结果。
- **Windows 真实桌面验收仍未完成**：安装、卸载、升级、便携启动、Credential Manager 登录、PowerShell profile/hooks、Mica、DPI、IME/AltGr 仍待真机确认。CI 构建与回归结果不能替代这些验收。
- **生成提交信息**只在自动化测试中对模拟服务验证过，尚未连接真实 AI 服务做桌面验收。
- 第一次切换到某个仓库时，后台读取 worktree 状态仍会解析一次登录 shell。这不阻塞界面，但在 Windows 上可能带来短暂的 CPU 占用，待实测后处理。
- macOS 实测（Apple M2 Pro，测量版）：第一次打开到文件列表画出 163–261ms，切换已打开的仓库 88–233ms，空闲时主进程 CPU 约 0.16%。
- M7 及更早里程碑的既有未达标项保留，见 [0.0.2 说明](https://github.com/GarlandQian/Revia/releases/tag/v0.0.2)。
- 此版本适用于接受上述限制的早期试用，不代表 M9 验收通过。

[固定源码](https://github.com/GarlandQian/Revia/tree/v0.0.3)

## 构建记录

固定源码：`03bda233ce371e28cd242ff8886d2cd310463118`，来自 main。
[双平台 CI](https://github.com/GarlandQian/Revia/actions/runs/42485916531) 已通过：macOS 943 项、Windows 848 项测试通过，均无失败，各保留 1 项既有忽略；Windows 便携特性检查与两平台 release binary 构建通过。
[安装包构建](https://github.com/GarlandQian/Revia/actions/runs/42557562343) 由修正后的发布流程按原标签重建，应用源码仍为上述固定源码。首次构建的 Windows 安装包误用了缓存中的 0.0.2 产物，该草稿未公开即已删除。上述自动检查不替代已列出的 Windows 真机验收。
