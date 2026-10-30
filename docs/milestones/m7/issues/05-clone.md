# 05：克隆并打开仓库

Status: ready-for-agent
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

### Codex / review02 Standards / R04 — 2026-10-31

<details>
<summary>本项证据原文</summary>

## N1 / R04：克隆新日志直接包含凭据（P1）

适用标准原文：`docs/milestones/m5/spec.md:170`（既有Git命令日志B07）：“凭据打码：URL 中的用户名、密码与令牌，以及携带认证信息的参数值和环境值都不显示”。其生产实现边界在 `git/mod.rs:62–64,73,81` 也明确CommandRecord内容已打码。

新代码 `m7/clone.rs:358` 直接以request.url填入args，`384–388` 直接拼tail输出。`validate:169–183,245–251` 允许URL userinfo，既不拒绝也不剥离；pump_stream `425–437` 只拆进度、去ANSI，未打码。`CommandLog::record`（git/mod.rs:99–108）仅分配序号和入队，不做脱敏。对照统一执行器 `git/mod.rs:536,542` 必须先调用 mask_credentials。因此诸如带user/password或token的克隆URL在克隆结束后以明文进入全局日志，可被日志面板/复制功能读取。这里没有提供、读取或打印任何真实凭据，也没有实际发出克隆请求。

建议：克隆使用共享执行器的参数/输出打码路径，保留真实argv及截断语义。该新泄露与H8“Git程序选择”是独立根因，单列新增项。

</details>

### Codex / review02 Standards / R07 — 2026-10-31

<details>
<summary>本项证据原文</summary>

## H8 / R07：Git路径只读启动快照（P2）

规则原文：`docs/technical-direction.md:83`：“设置中可以手动指定路径”；84要求环境缓存可手动重新加载。

`src-tauri/src/commands.rs:153–156,175–178` 仅AppState::new时读取git_path。CloneManager的字段 `clone.rs:78–80` 不可更新，start `118–121` 每次继续克隆该启动值。设置写入 `settings.rs:665–680` 更新Registry，App重新加载 `App.tsx:4433–4444` 走reload_environment；该命令仅清sessions（commands.rs:791–793），没有更新m7_clone。`m7/commands.rs:270–279` 也未按请求读取当前设置。

触发：启动后在设置选择另一个Git（即使执行“重新加载环境”）再克隆；普通WorktreeSession使用当前Registry路径（commands.rs:236–258），克隆仍使用旧Git/原PATH。原H8的“已选Git不生效”只修复了重启后场景，日志新接入也不能证明程序选择已统一。

</details>
