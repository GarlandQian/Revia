# Revia 0.0.2 M8 合并与发布记录

## 授权与固定范围

2026-11-02 用户明确“合并入 main 发布 0.0.2 吧，现在的删除”（Q207）。M8 从 `1930657` 整理合并，产品代码候选为 `83d40704214faf89e910af399632c9883d958ec1`；沿项目约定 squash 到 main，版本保持 0.0.2。用户授权正常推送 main、新建 `v0.0.2` 标签、公开 Release 并删除现有旧 Release，不强推、不改写旧标签。

远端当前公开版本只有 `v0.0.1`，不存在 `v0.0.2`。先生成并验证 0.0.2 草稿，公开成功后删除 0.0.1 Release 及资产；保留 `v0.0.1` 标签与源码历史。main 原有 `.gitignore` 未提交改动不纳入，其他 worktree 保留。

## 验证与证据

既有本机检查、完整 Rust 低并发通过及默认并发失败现象见 [verification](./verification.md)。本轮不重复无变化的本机全套，运行发布来源、版本与清单相关检查；实际 Windows 编译、回归、便携依赖和打包由 Actions 执行。真实 Windows 桌面、凭据、安装升级与性能未验收状态继续披露，不降低验收门槛。

证据根：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m8-release-wasxw831`。保存原工作区 patch、旧 Release 元数据与资产、每次命令及退出码；远端运行和下载校验在取得结果后追加。

发布说明：[release-0.0.2-notes](./release-0.0.2-notes.md)。当前尚未推送、构建或公开，以下实际结果待执行后补录。

## 本地发布检查

- 发布来源、统一清单和 Windows 依赖判定工具共 10 项 Node 回归通过。
- `RELEASE_TAG=v0.0.2 RELEASE_REPOSITORY=GarlandQian/Revia node tools/check-release-version.mjs` 通过；三处应用版本一致，stable 更新入口正确。
- `git diff --check` 通过。远端 main `64f0ee8` 已通过 API 核对，为本地 main `b9cf807` 的祖先，可以正常推进，无需强推。
- 首次 SSH fetch 因 1Password agent 签名通信失败，等待用户解锁；未切换协议或修改用户配置。旧 Release 元数据已保存，资产下载暂遇 EOF，删除前须完成归档与校验。
