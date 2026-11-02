# Revia 0.0.2 合并与发布记录

此处为 M7 时的准备记录，尚未公开 0.0.2。用户后来将 M8 纳入同一版本并授权替换旧 Release，实际发布见 [M8 发布记录](../m8/release-0.0.2-readiness.md)。

## 授权与范围

2026-11-02 用户明确要求“m7合并到main，只有main发布release，发布0.0.2版本”，记录为 Q203。本次可正常推送 main 与新版本标签、构建并公开 0.0.2；不重写 v0.0.1，不降低原验收标准，发布说明保留已知限制。

合并采用项目约定的 squash。m7 已提交候选包含 `ddd4810` 的提交区贴底与浏览器登录修复。原有六个未提交性能 / 验收文件保留在 m7 工作区，不纳入；main 的本地 .gitignore 配置改动也保留，不纳入提交。保留仍有改动的 m7 worktree，避免丢失工作。

## 发布约束

版本统一为 0.0.2（package.json、tauri.conf.json、Cargo.toml、Cargo.lock）。Release 仍由 v* 标签触发，首先完整读取 Git 历史并验证 HEAD 是 origin/main 的祖先；失败时不进入持有发布写权限及签名配置的打包任务。验证任务只有 contents:read，macOS 打包任务才有 contents:write。main 前进后仍允许重跑历史正式版本，不允许未合入分支直接发版。

## 本地检查

证据根：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-release-0.0.2-_7iw7qje`，原有两工作区状态、patch 与 hash 在 before.json。每次验证保存 argv、真实 exit、stdout/stderr。3 项真实 Git fixture 回归覆盖 main tag、未合入分支、main 后续前进及缺失 main 历史；版本一致性检查通过。实际远端 CI 与下载验证在完成后追加，未完成前不宣称公开。

## 增量审查

Standards：1 项 P3（fixture 临时目录未规范化）已改为 realpathSync，并重跑 3 项回归通过；Spec：0 项发现。新 Q203 表格空行同时修正。两轴原报告在证据根 standards-result.md / spec-result.md。定向 oxlint 与 diff 检查通过。
