# Revia 0.0.4 发布记录

Status: 构建与产物核验通过，待解锁完成桌面复验；尚未公开
日期：2026-11-10
依据：Q225

## 授权与范围

用户明确要求“推送到远端，发布 0.0.4 版本”。M10 已压成 `e673495` 合入本地 main，本轮将应用版本及 Cargo.lock 中的根包版本更新为 0.0.4，正常推送 main 与新标签 `v0.0.4`，通过现有 Compile Check 与 Release Draft 工作流构建、校验后公开稳定版。

主目录原有 `.gitignore` 未提交改动保留。旧版 Release 与标签保留。Windows 真实桌面、真实 AI 与历史性能不在本轮验收范围，已写入[发布说明](release-0.0.4-notes.md)。M10 代码验证与 macOS 桌面结果见[报告](report.md)。

## 执行与证据

本轮独占证据目录：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-0.0.4-release-w15h4prr`。发布前保存远端 main、旧版 Release 元数据及工作区状态；后续补充固定源码、CI、产物校验与公开结果。

## 首轮构建与返修

`6051ece` 将版本改为 0.0.4；main 与附注标签 `v0.0.4` 经 SSH 一次正常原子推送成功，标签对象为 `0ad5980`。本地版本一致性、Cargo.lock 根包版本与 13 项发布专项测试通过。

[首次 Release Draft](https://github.com/GarlandQian/Revia/actions/runs/42815099671) 构建成功，生成 11 项资产，草稿 ID 为 `440358262`，尚未公开。已从同次 Actions 下载 macOS 包，核对 SHA256，并用独立应用数据打开 DMG 内应用；首屏和手动更新检查通过，显示当前版本 0.0.4。该观察只对应首轮源码，不能代替返修后产物核对。

两次 Compile Check 的 macOS 均为 953 通过、1 失败、1 忽略：[main](https://github.com/GarlandQian/Revia/actions/runs/42815099704) 在更改列表 rebase 测试中未收到 Git 的 `sequencer` 路径；[标签](https://github.com/GarlandQian/Revia/actions/runs/42815099816) 在合并测试中把 `f.txt` 判为没有未解决冲突。对方失败的测试在另一次运行通过；原日志不足以确认两个失败一定同根因。

静态分析发现 M10 之前就存在的 `read_pipe` 竞态。按[返修规格](release-0.0.4-repair.md)补真实 Git 受控回归，`5a068e0` 稳定复现成功退出后 stdout / stderr 被丢成空。`c6868d8` 改为使用读取开始前的退出状态，并继续重试 `Interrupted`；三项回归通过，两轴审查 0 项代码意见，见[审查记录](release-0.0.4-repair-review.md)。

返修的完整 Rust 默认并发检查覆盖 35 个目标，956 通过、1 失败、1 忽略；唯一失败是 `m6_flows` 中等待 hook 就绪超过 15 秒，尚未进入用户刷新与历史写入断言。未改代码或超时门槛，单独复跑该用例通过（5.35 秒），随后仅重跑受影响的 `m6_flows` 整个目标，默认并发 133 项全部通过（146.30 秒）。原始失败保留，不能单凭复跑成功确定原因为调度迟到。修复证据在 `/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-004-git-output-fix-7jw7vev1`。

本次保留既有 100ms 管道排空上限；极端调度停顿下的截断风险未在本轮扩大处理。第一轮打包成功与 macOS 桌面通过不代表首轮 CI 通过，也不代替返修后的发布验证。

输出读取修复压成 `b832854` 正常推送 main；[对应 CI](https://github.com/GarlandQian/Revia/actions/runs/42822614141) 的 macOS Rust 与 release 编译通过。新推送按已有 concurrency 规则取消旧 main 运行，取得 Windows 原始日志后确认：编译仅用 2 分 58 秒，两个环境并发测试随后挂起约 44 分钟。规划方因此取消同样挂起且 macOS 已失败的旧标签运行；两次原失败和取消原因均留档。

Windows 夹具返修 `ecbcee0` 仅改 `environment_concurrency.rs`：规范化慢 worktree 的比较；保留完整 Git 安装，用两条等价不同路径测试替换，并预检实际执行；两个测试改为有限阶段与完成等待，保留原 3 秒 / 2 秒业务门槛。本地该目标 13 项全部通过，两轴审查均 0 项代码意见；后续真实 Windows CI 已通过，见[夹具返修记录](release-0.0.4-windows-fixture.md)。

## 最终源码与标签更新

Windows 夹具修正压成 `97211abb1ea0c5398eee3fe6f08c567801a8119b` 正常推送 main；与 `b832854` 的差异仅为测试夹具和记录，生产代码一致。[最终 main CI](https://github.com/GarlandQian/Revia/actions/runs/42826584536) 全部通过：macOS 957 项、Windows 859 项 Rust 测试通过，两平台均 0 失败、1 忽略；前端类型、lint、构建、release 编译与 Windows portable 检查通过。Windows 两项曾挂起的环境测试已在原始日志中确认 `ok`。

用户明确批准仅更新本轮尚未公开的标签。已保存原标签对象和草稿元数据，使用 `--force-with-lease=refs/tags/v0.0.4:0ad5980995cbfd8113db1d735de892f0559db777`，只推送 `refs/tags/v0.0.4`。新附注标签对象为 `96d2e6dd4ebe8fbcda4577657438e9c509781987`，解引用指向 `97211ab`。首轮资产继续作为历史证据，最终资产由[新 Release Draft](https://github.com/GarlandQian/Revia/actions/runs/42831536896)重建；同源码的[标签 CI](https://github.com/GarlandQian/Revia/actions/runs/42831536897)也已全部成功，测试计数与 main 一致。两次 Windows 运行中的原挂起用例均明确通过。

最终独占证据目录：发布证据根下 `final-ohi4188q`。仓库内的精简证据见 [evidence/release-0.0.4](evidence/release-0.0.4/README.md)。

## 最终产物核验

[最终 Release Draft](https://github.com/GarlandQian/Revia/actions/runs/42831536896) 已成功，仍使用草稿 ID `440358262`。11 项资产全部更新，主要安装包摘要均不同于首轮；两平台 checkout 与实际编译日志均固定到 `97211ab`。完整资产名称、ID、大小和摘要见[最终草稿快照](evidence/release-0.0.4/final-draft-assets.json)。

- 11 项下载的大小和 SHA256 与 GitHub API digest 一致；两个摘要文件覆盖全部预期载荷，稳定版与预发布清单逐字节相同，平台及下载 URL 精确对应 macOS 更新包和 Windows 安装器。
- 使用仓库变量 `REVIA_UPDATE_PUBKEY` 的公开公钥，分别验证两平台更新包的 payload 签名、trusted comment 签名及 0.0.4 版本绑定，全部通过。
- macOS 更新包内应用为 0.0.4 / arm64；DMG 内程序与更新包内程序逐字节一致，二进制 SHA256 为 `cd6ae972c8bcbfdba4834ea24d5a7f5393cc3eb359176c2877281fcc13cdf3dc`。
- Windows 安装器外壳、便携程序和解出的实际应用均由 PE 的 `RT_VERSION` 资源树确认文件及产品版本为 0.0.4.0；便携程序和实际安装应用均为 x64。ZIP 仅含 `Revia/Revia.exe`，与单独便携 EXE 字节一致。未执行 Windows 程序。

下载时的三次 EOF 和日志 GET 中断保留原件，只补取缺失文件；残片隔离，摘要通过后才归入最终资产目录。检查脚本最初把 NSIS 的结构版本字段当成应用版本前提，因其值为 0 而失败；改为读取真实资源树后通过，文件及产品版本要求不变。NSIS 官方源码明确初始化该结构为零、另外设置应用版本，来源和原始失败见[产物核验摘要](evidence/release-0.0.4/artifact-task-summary.json)。此次只修正验证器，没有重建应用或重复已通过的验签。

## 桌面与公开前状态

已将新 DMG 只读挂载，复制应用至最终证据目录的 `desktop/Revia.app`，以独立的 `desktop-app-data` 启动；PID 为 `94616`，启动日志为空，进程仍在运行。电脑操控工具报告 Mac 已锁定，已请求用户手动解锁。**新包的首屏和手动更新检查尚未完成，不能用首轮旧包的桌面结果替代。**

最终发布说明已通过 Release ID `440358262` 的 API 更新到草稿。按标签的 `gh release edit` 返回 `release not found`；API 更新时显式传入 `tag_name: v0.0.4` 与固定源码，确认 `draft: true`、`prerelease: false`、说明内容正确，以及 11 项资产的 ID、大小、摘要全部未变。原始 CLI 失败、API 返回的临时草稿标签及显式标签修正均保留；后续公开时仍显式传入版本标签。

准备文档推送时，现有 SSH origin 的只读复核返回 `sign_and_send_pubkey` / `communication with agent failed`。API 仍能核对固定标签和编辑本轮草稿；文档提交的正常推送待 SSH 签名代理恢复，继续使用现有 SSH origin。该失败记录位于最终证据目录的 `pre-notes-tag.*`。

后续：用户解锁后完成新包首屏与手动更新检查；再次核对草稿资产 ID、大小和 digest；公开为 stable / latest；匿名核对公开清单和下载入口；更新发布状态、正常推送文档提交并清理本轮两个返修 worktree。旧版标签与 Release 保留，主目录原有 `.gitignore` 改动保留。
