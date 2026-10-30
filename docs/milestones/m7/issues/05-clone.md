# 05：克隆并打开仓库

Status: ready-for-review
Blocked by: None

## What to build

提供 URL、目标目录和高级选项，Rust 调用 Git 克隆，成功登记，取消 / 失败说明保留目录。

依据：规格 E；Q179、Q153。

## Acceptance criteria

- [x] 指定分支、浅克隆深度、子模块可选；独立 argv 防命令 / 选项注入
- [x] 不覆盖既有非空目录；取消或失败不自动删除残留，显示实际目标与状态
- [x] 成功登记真实仓库并打开；进度和错误随当前语言呈现，延迟结果不切换到另一目标
- [x] 传输使用既有凭据，需输入及时失败；自建 local remote 覆盖成功 / 失败 / 取消 / 子模块相关路径

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / review01 Standards — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/standards.md；完整证据附录保存在本轮证据目录。

8. **[P2] 克隆旁路统一 Git 执行器**：`src-tauri/src/m7/clone.rs:259` 自行选 PATH Git、启动进程，忽略设置路径且不记命令日志；违反 `docs/technical-direction.md:56,83`。

## H8 克隆不遵守统一 Git 选择与日志（P2）

规则：`docs/technical-direction.md:56` 每条 Git 命令记录参数/退出码/耗时/输出；83 行允许设置手动指定 Git 路径。现有 `git::Git::with_program` 负责该约定。

clone.rs:259–263 直接 shell_env.which("git")/Command::new；CloneManager 无 Settings 或 CommandLog 参数，m7_clone_start 只解析 shell_env 后传入，未读取设置覆盖。整个 run 路径未写命令日志。

触发：设置指定自装 Git（或默认 PATH Git 不可用）、然后克隆。后果推断：克隆执行不同 Git；成功/失败/耗时不进入 Git 命令日志与诊断。建议让共享执行层提供受控流式 clone，而非另建无日志进程路径。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Pi 正式返修处理 — 2026-10-31

- **H8**：CloneManager 携带设置中的 Git 程序（Q27）与统一 CommandLog；clone 完成写入真实命令日志（参数、耗时、退出码、尾部摘要），程序选择与统一执行器同源。
