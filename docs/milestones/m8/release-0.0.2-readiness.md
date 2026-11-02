# Revia 0.0.2 M8 合并与发布记录

发布结果：2026-11-02 已公开 [Revia 0.0.2](https://github.com/GarlandQian/Revia/releases/tag/v0.0.2)，0.0.1 误删后已恢复原说明和全部资产，0.0.2 保持 latest；两个标签保留。下方按执行阶段保留历史记录，最终结果见文末。Windows 真机验收仍为 ready-for-human。

## 授权与固定范围

2026-11-02 用户要求合并 main 并发布 0.0.2（Q207）。先前把清理范围笼统记录为“删除现有旧 Release”是规划方误读；用户提及的是现有 0.0.2，后续问“为什么要删 0.0.1”后已纠正，见文末恢复记录。M8 从 `1930657` 整理合并，产品代码候选为 `83d40704214faf89e910af399632c9883d958ec1`；沿项目约定 squash 到 main，版本保持 0.0.2。用户授权正常推送 main、新建 `v0.0.2` 标签与公开 Release，不强推、不改写旧标签；这不包含删除 0.0.1。

远端当前公开版本只有 `v0.0.1`，不存在 `v0.0.2`。当时规划方错误安排为公开新版后删除 0.0.1 Release 及资产；该范围判断有误，后来已恢复。`v0.0.1` 标签与源码始终保留。main 原有 `.gitignore` 未提交改动不纳入，其他 worktree 保留。

## 验证与证据

既有本机检查、完整 Rust 低并发通过及默认并发失败现象见 [verification](./verification.md)。本轮不重复无变化的本机全套，运行发布来源、版本与清单相关检查；实际 Windows 编译、回归、便携依赖和打包由 Actions 执行。真实 Windows 桌面、凭据、安装升级与性能未验收状态继续披露，不降低验收门槛。

证据根：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m8-release-wasxw831`。保存原工作区 patch、旧 Release 元数据与资产、每次命令及退出码；远端运行和下载校验在取得结果后追加。

发布说明：[release-0.0.2-notes](./release-0.0.2-notes.md)。M8 已于本轮压成 `7c97fd664f3482585d59240a7b086081d59684e5` 合入本地 main，并通过既有 SSH origin 正常推送。当时双平台 CI 执行中，尚未创建 0.0.2 标签或公开新版；最终公开结果见文末。

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
- main 的 [Compile Check #41441512775](https://github.com/GarlandQian/Revia/actions/runs/41441512775) 当时已开始；实际结论见后续段落。
- 远端核实仅有公开 `v0.0.1`，没有现成的 `v0.0.2`；继续按先完成新版、再清理旧 Release 的顺序执行。旧版 6 项资产已全部归档并核对大小与 GitHub SHA256 digest；签名由 gh 下载遇 EOF 后，使用同一公开 GitHub 下载地址成功补齐。校验原件为 `old-assets-verified.json`。


## 发布前用户反馈与首轮 CI

- 用户随后反馈仓库列表缺移除、读取失败时工具栏不可见，并确认 SSH 克隆可用，只修前两项。独立 m8 worktree 完成 `0d08340` 与记录 `7d6d092`，在 main 分别为 `9a9eec2`、`c446008`；未更改 SSH 生产代码。
- [补修规格、验证与双轴审查](./followup-0.0.2-feedback.md)：Standards 0 项 / Spec 0 项；类型、lint、前端构建、x18n、9 项登记持久化回归、194 项现有生命周期/刷新检查通过。macOS 真实隔离窗口已操作失效登记与真实仓库的移除，保留磁盘文件，并确认工具栏和克隆对话框可用。
- Compile Check #41441512775：macOS 整组通过；Windows 源码编译通过，只有 `profile_nonterminating_error_rejects_the_partial_environment` 超过 15 秒，其余目标通过。没有据此放宽上限或改生产脚本；已补齐该测试的 Windows USERPROFILE/AppData/module cache 隔离与诊断，等待新 main CI。
- 旧 Release 尚未删除，新版尚未打标签或公开；配置项名称已核对齐全，未读取任何签名私钥内容。


## 双平台固定源码检查与打包修复

- 最终应用源码 `v0.0.2` → `4c384ed4b3452ac1713aa3baed5dccaef2213b39`；[main CI 41448890283](https://github.com/GarlandQian/Revia/actions/runs/41448890283) 和 [tag CI 41453450323](https://github.com/GarlandQian/Revia/actions/runs/41453450323) 均成功。main 原始日志统计：macOS 886 passed、Windows 795 passed，均 0 failed / 1 ignored；Windows portable feature 与两平台 release binary 构建通过。此前超时的 Windows profile 回归已在原始日志中确认通过，未放宽 15 秒上限。
- 标签首次 SSH 推送遇连接中断；API 核实未创建后，沿既有 SSH origin 正常重试成功，无强推、无标签移动。
- 初次 [Release Draft 41453450255](https://github.com/GarlandQian/Revia/actions/runs/41453450255) 的 macOS 包成功，Windows gate 误拒 Windows 10/11 自带的 UCRT。依据 Microsoft 原文修正分类，继续拒绝额外 VC++ runtime、动态 WebView2Loader、空输出与 dumpbin 非零退出；详见 [打包修正 01](./release-repair-01.md)。该工具增量的两轴审查均 0 项，相关 Node 回归 13/13 通过，实际 32 项 dumpbin 输出复核通过。
- 通过 main 的发布工具 `45e3e712c6fecf665c662d008028f34de83ee5ff` 手动重建原标签；所有应用 checkout 仍为 `4c384ed`，工作流分别校验两个来源在 main 历史中。[重建 41459104017](https://github.com/GarlandQian/Revia/actions/runs/41459104017) 四个 jobs 均成功，Windows 便携依赖 gate、NSIS 和双平台草稿汇总均通过。原始日志已核对两个实际 checkout SHA。

## 最终产物与公开结果

公开时间：2026-11-02 22:42（Asia/Shanghai）。Release ID：`431567432`，稳定版、非草稿、latest。

- 11 项 Release 资产的大小、GitHub SHA256 digest 与本轮下载的构建产物/清单完全一致。两个平台校验文件中的 7 项文件 SHA256 全通过；Windows 校验文件为原始 CRLF，macOS shasum 最初将 CR 当文件名部分导致读取失败，最终用支持 CRLF 的校验器对原始字节逐项通过，没有改写产物。
- 正式公钥对 macOS updater tar.gz 与 Windows NSIS setup.exe 的 payload 和 trusted-comment 签名均验签成功；两个签名的版本均为 0.0.2，且与各自清单签名一致。stable/prerelease 清单字节相同，包含 `darwin-aarch64` 和 `windows-x86_64`，URL 固定在 v0.0.2。
- DMG 内部校验有效；Mac 更新包的应用版本 0.0.2、Mach-O arm64；Windows 便携 PE 为 x64，zip 仅含 `Revia/Revia.exe` 且与单 exe 字节一致；NSIS 产物存在且配对签名通过。未运行 Windows 安装器或冒称 Windows UI 已验收。
- 公开后，无认证请求 `releases/latest/download/latest.json` 成功，内容与已验清单逐字节一致。macOS DMG、Windows setup.exe 和 portable.zip 三个公开链接均返回 HTTP 206，首字节与已验产物一致；这是公开可访问性检查，完整内容校验来自已下载产物及服务器资产摘要。
- 更新说明已写入仓库与公开 Release，保留 Windows 真机、M7/历史性能、macOS Gatekeeper 等未验收/未达标项。发布授权不转写为验收通过。
- 新版公开和入口确认后，规划方曾错误删除旧 Release `430426575`（v0.0.1）及 6 项资产，随后已恢复（见下节）；全部旧资产已在 `/private/tmp/revia-m8-publish-tzrt7vd_/old-v0.0.1` 归档，大小/SHA256 与旧更新签名已验证。该次删除后的快照只含 v0.0.2，不是恢复后的最终状态；旧标签对象 `668c94b2984f32f425412da1e29192f55190b6d7` 保留，新标签身份与原应用提交不变。
- main 原有 `.gitignore` 改动逐字节保留，未纳入提交；没有改动其他旧 worktree，也未开 PR。m8 worktree 保留给后续 Windows 验收。

持久证据：[evidence/release-0.0.2](./evidence/release-0.0.2/) 包含首次失败、通过的主线检查和最终打包/来源日志压缩原件、CI 元数据、11 项资产摘要、验签/结构结果、公开下载回执与标签保留记录。完整临时命令 stdout/stderr/退出码、旧资产和下载产物在本轮独占证据根；未读取或提交签名私钥/用户令牌。


## 删除范围纠正与 0.0.1 恢复（2026-11-02）

用户询问“为什么要删 0.0.1”。规划方错误沿用 Q207 中笼统的“删除现有旧 Release”，把用户提及的 0.0.2 扩大到了 0.0.1；发布 0.0.2 不需要删除历史版本。已纠正决策表和协作入口中的范围记录，该错误不归因于用户授权。

从删除前归档恢复旧版原标题、原说明和全部 6 项原始资产，逐项大小与 SHA256 与原资产一致；原 v0.0.1 标签和源码未变，原下载 URL 恢复。恢复先以草稿核对，再以 `latest=false` 公开。最终核对：0.0.1 已公开，0.0.2 仍为 latest；无认证读取旧版原 latest.json 地址成功，字节与归档一致。

恢复后的 Release ID 为 `431595557`。说明、文件内容与下载地址已恢复，但原 Release ID、发布时间和下载计数等 GitHub 历史元数据不能原样还原；删除前元数据已保留。此操作恢复原历史版本和原资产，没有重新构建旧版代码。

当前证据为 `restored-old-release.json`、`latest-after-restore.json`、`restored-old-latest.json`。先前 `releases-after-cleanup.json` 只记录误删当时的历史状态，不代表最终状态。
