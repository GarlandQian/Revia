# 用户图标集成：独立两轴静态复核

日期：2026-10-24。固定范围为 `git diff bf658f7...5d9b781`，不包含任务05。

## Standards

无新增发现。未发现确定的文档规范违规，也无值得单列的 Fowler 判断性建议。

- `Assets.car` 配置符合本地 Tauri CLI 2.12.3 的处理方式：macOS 复制预编译资源，保留 ICNS 回退；Windows 按 `.ico` 后缀选择图标，未见 CAR 干扰该路径。
- macOS 导出工具依赖集中于独立脚本，未接入普通跨平台编译流程；符合 ADR-0007/Q55。不把用户明确授权的图案和配色误报为界面样式违规。
- 独立核对清单内24份文件的字节数及 SHA-256；14份 PNG 的 CRC、数据流及尺寸有效，均为8位 RGBA；ICO 含16至256像素条目；ICNS 容器长度及条目边界有效。

验证为固定提交解析、静态检查及同版 Tauri 官方打包源码核对：
https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.3/crates/tauri-bundler/src/bundle/macos/icon.rs

## Spec

无确定发现。

- 独立核对24份文件、提交内容、集成清单 SHA 与 main 源文件一致。
- 保留 PNG/ICNS/ICO，增加 CAR；本地 Tauri Windows 源码按 ICO 选择，未见 CAR 被误选。
- 导出脚本的 Xcode 依赖仅属于显式资源导出工具，不在普通构建链。

## 验证边界

两轴均未运行导出器、完整打包、GUI 或新 CI，不能由静态复核认定系统图标展示、CAR 实际装载、旧 macOS 回退或 Windows 实际编译已通过。04 的固定受测二进制不包含这些新图标。后续新 Release 与对应双平台 CI 必须单独留证。

汇总：Standards 0项；Spec 0项。两轴无严重度最高发现。
