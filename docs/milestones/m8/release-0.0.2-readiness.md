# Revia 0.0.2 M8 合并与发布记录

## 授权与固定范围

2026-11-02 用户明确“合并入 main 发布 0.0.2 吧，现在的删除”（Q207）。M8 从 `1930657` 整理合并，产品代码候选为 `83d40704214faf89e910af399632c9883d958ec1`；沿项目约定 squash 到 main，版本保持 0.0.2。用户授权正常推送 main、新建 `v0.0.2` 标签、公开 Release 并删除现有旧 Release，不强推、不改写旧标签。

远端当前公开版本只有 `v0.0.1`，不存在 `v0.0.2`。先生成并验证 0.0.2 草稿，公开成功后删除 0.0.1 Release 及资产；保留 `v0.0.1` 标签与源码历史。main 原有 `.gitignore` 未提交改动不纳入，其他 worktree 保留。

## 验证与证据

既有本机检查、完整 Rust 低并发通过及默认并发失败现象见 [verification](./verification.md)。本轮不重复无变化的本机全套，运行发布来源、版本与清单相关检查；实际 Windows 编译、回归、便携依赖和打包由 Actions 执行。真实 Windows 桌面、凭据、安装升级与性能未验收状态继续披露，不降低验收门槛。

证据根：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m8-release-wasxw831`。保存原工作区 patch、旧 Release 元数据与资产、每次命令及退出码；远端运行和下载校验在取得结果后追加。

发布说明：[release-0.0.2-notes](./release-0.0.2-notes.md)。M8 已于本轮压成 `7c97fd664f3482585d59240a7b086081d59684e5` 合入本地 main，并通过既有 SSH origin 正常推送。当前双平台 CI 执行中，尚未创建 0.0.2 标签或公开新版。

## 本地发布检查

- 发布来源、统一清单和 Windows 依赖判定工具共 10 项 Node 回归通过。
- `RELEASE_TAG=v0.0.2 RELEASE_REPOSITORY=GarlandQian/Revia node tools/check-release-version.mjs` 通过；三处应用版本一致，stable 更新入口正确。
- `git diff --check` 通过。远端 main `64f0ee8` 已通过 API 核对，为本地 main `b9cf807` 的祖先，可以正常推进，无需强推。
- 首次 SSH fetch 因 1Password agent 签名通信失败，等待用户解锁；未切换协议或修改用户配置。旧 Release 元数据已保存，资产下载暂遇 EOF，删除前须完成归档与校验。


## main 推送与远端 CI（2026-11-02）

- 继续执行时核对：main 为 `7c97fd6`，m8 文档 HEAD 为 `656609a`，两棵树一致；产品代码仍为已审候选，未重复实施或审查。
- `git fetch origin` 已通过原 SSH 通道成功；正常推送 `64f0ee8..7c97fd6 main -> main`，没有强推。
- 本轮证据根：`/private/tmp/revia-m8-publish-tzrt7vd_`。保存继续执行时的引用、main 原有 `.gitignore` 内容与 patch、旧 Release 元数据、下载资产和后续远端命令 stdout/stderr/退出码。
- 发布来源、统一清单与 Windows 依赖判定 10 项 Node 回归再次通过；版本检查与 `git diff --check` 通过。
- main 的 [Compile Check #41441512775](https://github.com/GarlandQian/Revia/actions/runs/41441512775) 已开始，结果待补。
- 远端核实仅有公开 `v0.0.1`，没有现成的 `v0.0.2`；继续按先完成新版、再清理旧 Release 的顺序执行。旧版 6 项资产已全部归档并核对大小与 GitHub SHA256 digest；签名由 gh 下载遇 EOF 后，使用同一公开 GitHub 下载地址成功补齐。校验原件为 `old-assets-verified.json`。


## 发布前用户反馈与首轮 CI

- 用户随后反馈仓库列表缺移除、读取失败时工具栏不可见，并确认 SSH 克隆可用，只修前两项。独立 m8 worktree 完成 `0d08340` 与记录 `7d6d092`，在 main 分别为 `9a9eec2`、`c446008`；未更改 SSH 生产代码。
- [补修规格、验证与双轴审查](./followup-0.0.2-feedback.md)：Standards 0 项 / Spec 0 项；类型、lint、前端构建、x18n、9 项登记持久化回归、194 项现有生命周期/刷新检查通过。macOS 真实隔离窗口已操作失效登记与真实仓库的移除，保留磁盘文件，并确认工具栏和克隆对话框可用。
- Compile Check #41441512775：macOS 整组通过；Windows 源码编译通过，只有 `profile_nonterminating_error_rejects_the_partial_environment` 超过 15 秒，其余目标通过。没有据此放宽上限或改生产脚本；已补齐该测试的 Windows USERPROFILE/AppData/module cache 隔离与诊断，等待新 main CI。
- 旧 Release 尚未删除，新版尚未打标签或公开；配置项名称已核对齐全，未读取任何签名私钥内容。
