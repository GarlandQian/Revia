# B09：关闭所有权与写门禁修复、独立复核

日期：2026-10-25。用户明确“你来修”，限定授权见 Q108（独立提交 `53daeeb`）。固定生产 `7cdab510e8ab360ede61ef3c00c326ca71304bbc`，复核基点 `64aa4ecfee729cfb185bd3c7a85288b60a54bda5`；本轮由 Codex 接手，原 Pi 未再派发。

## 修复

- 不再调用 `window.close()` 产生无法识别归属的程序化 `CloseRequested`。所有原生关闭事件先阻止默认行为，只作为用户意图；最终关闭用 `destroy()`，退出用固定 Rust `request_exit` 命令执行实际应用退出。
- 每次异步执行持有自己的 attempt；准备阶段 Stay 可撤销该 attempt，所有成功/失败回包先核身份，旧回包不能影响新 claim、阶段或 UI。
- 最终同步复核通过后进入 `closing` 并请求隐藏询问；实际原生提交后不再提供取消选择，写门禁持续关闭至销毁/退出，不能因 IPC 回包或迟到选择重新开放。
- 真实 `WriteGuardOrchestrator.begin()` 在登记前和保护 await 后都检查必填的关闭驱动；App 的暂存/取消暂存/丢弃、恢复、另存均接此实例。Rust 退出命令再次复核退出保护，拒绝时不投递退出。

锁定依赖行为来自本机源码，不靠事件身份假设：`@tauri-apps/api` 2.12.4、`tauri` 2.12.4、`tauri-runtime-wry` 2.12.2 的相关路径、完整文件 SHA256 与摘录保存在 `dependency-behavior.json.gz`。方案移除了旧匿名裁决阶段，不能声称新测试识别了那个旧事件。

## 检查与原件

独占目录 `/private/tmp/revia-b09-Ztcljf`；全部原件无损 gzip 并逐份解压核对，见[清单](./evidence/batch09-repair/manifest.json)。记录器保留真实 argv/cwd、最小显式环境、start/end/exit、原始 stdout/stderr；Git 全局/系统配置隔离，测试只使用既有临时 fixture 体系，不操作用户仓库。

| 检查 | 结果 |
| --- | --- |
| Node 共享生产驱动与写编排回归 | 180 passed / 0 failed |
| TypeScript、lint、前端构建 | 各退出 0 |
| 全量 Rust 单元/集成测试 | 268 passed / 0 failed；既有 `prepare_task05_desktop_fixture` 1 项 ignored |
| 首次 diff 空白检查 | 退出 2：ts-rs 导出产生的行尾空格，原件保留 |
| 现有绑定清理脚本、仅重跑空白检查 | 均退出 0；20 份生成绑定恢复原样，无生成物改动进入提交 |

81 份受测生产/测试/配置/锁文件在检查前、后、生成物清理前、后、固定提交及磁盘逐份 SHA256 一致。Rust/Vite 等 warning 原样保留，不宣称无警告。没有为格式化失败重复执行已通过的整套检查。

旧 B05–B08 的匿名裁决测试迁移到实际准备/最终提交生命周期；新增成功/失败旧回包抵达新准备、提交、返回三个阶段的交错，以及五类写动作在提交与回包后均被共享入口拒绝。验证到新请求实际原生适配提交并清账，不仅验证再次投递计数。

## Standards

独立静态复核：确定规范违反 0 项，Fowler 判断建议 0 项，无最高严重度。实际 App/共享驱动/写入口/Rust 退出链已核对；完整报告见 `standards-review.md.gz`。本轴未运行检查或 GUI。

## Spec

独立静态复核：缺失/部分 0 项、范围扩张 0 项、实现错误 0 项，无最高严重度。Q90/Q91/Q96 未发现本增量的确定反例；完整报告见 `spec-review.md.gz`。本轴未运行检查或 GUI。

两轴各 0 项，分别保留结论；不是 M2 最终验收。任务 08 转 `ready-for-human`，真实 WebView 的询问隐藏、Wait/Stay、待核对、窗口关闭和 Quit/⌘Q，以及失败反馈仍需固定 Release 桌面验证。

本轮未构建新 Release、操作真实桌面、运行 D06/双平台 CI/Q98 长测，也未推送或合并；`m2-ui`、main 及用户改动未动。任务 08/10/11/M2 尚未完成，合并顺序不变。
