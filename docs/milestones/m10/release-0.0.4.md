# Revia 0.0.4 发布记录

Status: preparing
日期：2026-11-10
依据：Q225

## 授权与范围

用户明确要求“推送到远端，发布 0.0.4 版本”。M10 已压成 `e673495` 合入本地 main，本轮将应用版本及 Cargo.lock 中的根包版本更新为 0.0.4，正常推送 main 与新标签 `v0.0.4`，通过现有 Compile Check 与 Release Draft 工作流构建、校验后公开稳定版。

主目录原有 `.gitignore` 未提交改动保留。旧版 Release 与标签保留。Windows 真实桌面、真实 AI 与历史性能不在本轮验收范围，已写入[发布说明](release-0.0.4-notes.md)。M10 代码验证与 macOS 桌面结果见[报告](report.md)。

## 执行与证据

本轮独占证据目录：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-0.0.4-release-w15h4prr`。发布前保存远端 main、旧版 Release 元数据及工作区状态；后续补充固定源码、CI、产物校验与公开结果。
