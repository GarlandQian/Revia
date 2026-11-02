# M8 Paseo / Pi 实施交接

Status: ready-for-agent
日期：2026-11-02

## 目标与输入

在 `/Volumes/T7 Shield/project/Revia-m8` 的 `m8` 分支实现 Windows x64 安装版与便携版，基线 main `b9cf807`。用户已要求开始，Q204–Q206 已确认；使用 implement-spec 阅读 [规格](./spec.md)、[任务图](./issues/)与 [规划事实](./planning.md)，整包实现任务 01–06，并完成任务 07 中本机可行部分及 Windows 验收说明。

先读根 AGENTS、docs/README、GLOSSARY、相关 ADR 与 docs/agents/local-validation.md。本交接采用此前集中交付方式：同一个 Paseo / Pi 会话按依赖完成，不另派功能代理或建功能 worktree；不逐任务等待用户。Q76 允许直接整理确认后的规格和任务，无须重复请求批准。用户与仓库的集中验证策略优先于技能默认频繁测试或默认 TDD。

## 范围与隔离

- 仅在上述 m8 worktree 写代码和提交，主仓库及其他 worktree 保持原样。普通用户仓库、真实 GitHub 账号/凭据和系统配置不属于测试范围。
- 证据根用 mktemp/tempfile 独占创建并记录，测试仓库、profile、应用数据、HTTP 服务都在其下隔离。保留原始 stdout/stderr、精确 argv、退出码、源码 SHA/脏改动散列，失败原件保留。
- Windows 专属逻辑优先核对当前 Cargo.lock、Tauri CLI 及插件源码和官方资料。macOS 上不能把未编译的 cfg(windows) 代码声明为已通过 Windows。
- 保留现有 Git 安全边界、回收区、写前身份与指纹、关闭协调和结构化双语错误。Git 功能只通过 Rust 系统 Git CLI；stash 删除窄例外不扩大。
- Q204 允许两种版本都使用 Windows Credential Manager；其他便携数据和 WebView2 用户数据必须在 exe 旁 data，首次建窗前配置，数据目录不可写拒绝启动。
- 新文案/错误键遵循 x18n 与 x18n-check 技能，通过 CLI 管理语言资源，不手工改 locale，不调整已确认的 glossary/severity、不删除旧 key。
- 不修改用户 Git 配置、profile、执行策略、全局 MCP/agent 配置。只改 CI 文件，不推送、开 PR、打标签、执行远端 Actions、创建或公开 Release。

## 实施与验证

1. 确认 cwd、分支与初始文档提交，阅读任务和依赖。工程选型在已确认边界内自行决定；遇到真实产品取舍写 needs-info，继续其他不依赖部分。
2. 完成 01–06；同批相关修改完成后集中检查。复用现有 Rust 真实 Git/临时仓库、Node 与前端工具，只为真实回归/公共契约新增必要测试，不引入框架。
3. 交付前运行类型、lint、前端构建、x18n、相关 Node 工具检查，以及涉及共享启动/环境/更新的完整现有 Rust 目标。失败修复后只重跑受影响检查。依赖可用 pnpm install --frozen-lockfile，Rust 使用 /opt/homebrew/bin/cargo；不借用旧二进制冒称通过。
4. 技术任务的验收项按真实证据勾选。真实 Windows GUI、安装包执行、凭据和 WebView2 性能只写具体待验步骤，交由 Codex 取得 Windows 环境验收；任务 07 与整体缺真机时 ready-for-human。
5. 按明确路径提交到 m8；写 report.md：固定实现 SHA、改动、精确检查与结果、原始证据位置、未验证和原因、遗留问题。整包交付后统一交 Codex，不自行发正式独立审查、不合 main、不删除 worktree。

正式审查由 Codex 按 code-review 两轴进行，返修 Q113 最多一次，后续由 Codex 接手。模型请求/渠道错误才按 Q137 回退：GLM-5.3-Flash → GLM-5.3 → deepseek-v4.1-flash → gemini-3.8-flash；编译/测试失败不属于渠道错误。
