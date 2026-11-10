# Revia 0.0.4 发布记录

Status: 返修验证中，尚未公开
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

Windows 夹具返修 `ecbcee0` 仅改 `environment_concurrency.rs`：规范化慢 worktree 的比较；保留完整 Git 安装，用两条等价不同路径测试替换，并预检实际执行；两个测试改为有限阶段与完成等待，保留原 3 秒 / 2 秒业务门槛。本地该目标 13 项全部通过，两轴审查均 0 项代码意见；真实 Windows 分支待 CI，见[夹具返修记录](release-0.0.4-windows-fixture.md)。
