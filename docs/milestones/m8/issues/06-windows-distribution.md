# 06：Actions 双平台产物与统一更新清单

Status: ready-for-human
Blocked by: 01, 05

## What to build

版本标签的构建可产生 Windows 安装版和便携版，与 macOS 汇总完整的版本清单和校验文件。

## Acceptance criteria

- [x] 保留 main 来源和版本一致性门禁，main 推送只检查；只改工作流，不执行推送、打标签或 Release 写操作。
- [x] Windows x64 NSIS currentUser，便携单 exe 与含 Revia 文件夹的 zip，按 Q65/Q66 命名并生成 windows-SHA256SUMS.txt。
- [x] 便携构建不混入安装版，包含必要资源，依赖系统 WebView2；核对 Cargo/Tauri 锁定版本支持的打包和签名格式。
- [x] macOS 与 Windows 先独立构建，再统一生成含 darwin-aarch64/windows-x86_64 的清单，不让并发 job 覆盖另一平台。
- [x] 保留稳定/预发布、HTTPS、updater 签名及 macOS 产物，缺关键产物或密钥明确失败。
- [x] 用现有 Node 测试或必要最小回归验证归档结构、签名配对、清单完整性与缺失失败，不引入新框架。

## Comments

实施说明（`a30b435`、`af869f9`）：

- Release 工作流拆为 `build-macos` / `build-windows` / `publish`；Windows 产出 NSIS currentUser
  安装版、`--features portable --no-bundle` 单 exe、含 `Revia/Revia.exe` 的 zip 与
  `windows-SHA256SUMS.txt`；汇总 job 统一下载并生成双平台 `latest.json`。
- 按 Tauri 官方 updater 文档核对：`createUpdaterArtifacts: true` 的 Windows updater 包是
  `setup.exe` + `.sig`；清单生成器同时接受 v1Compatible 的 `.nsis.zip`。
- 新增 `+crt-static` 与 `tools/check-windows-deps.mjs`，CI 对最终便携 exe 运行
  `dumpbin /dependents`，拒绝动态 CRT / `WebView2Loader.dll`；Node 测试覆盖通过/拒绝。
- 未执行远端 Actions，也未在 Windows runner 实际构建；CI 定义不等于通过，待 Codex 取得环境。
