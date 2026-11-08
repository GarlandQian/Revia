# M9 Paseo / Pi 实施交接

Status: ready-for-agent（派发前由规划方把本轮文档提交到 main，并创建 worktree）
日期：2026-11-08

## 目标与输入

在 `/Volumes/T7 Shield/project/Revia-m9` 的 `m9` 分支上实施 M9，基线是包含本交接的 main 提交。用 implement-spec 阅读 [规格](./spec.md)、[任务图](./issues/) 和 [排查事实](./perf-facts.md)。

开始前先读根目录的 AGENTS、docs/README（重点是 Q208–Q221）、GLOSSARY、ADR-0002/0003/0004/0013/0015，以及 docs/agents/local-validation.md。

用同一个 Paseo / Pi 会话按依赖顺序完成任务，不另派功能代理，也不另建功能 worktree，不逐任务等待用户：

1. 先做性能：01、02、03、04、06。任务 05 处于 needs-info，等规划方在 Comments 中写明访谈 Q27 的结论后再做；在此之前不要自行决定环境加载时机。
2. 再做生成提交信息：07、08、09（Q208）。

任务 10、11 由规划方和用户完成，Pi 只提供本机检查证据和 report。

用户与仓库的集中验证策略，优先于技能默认的频繁测试或 TDD。

## 范围与隔离

- **写入范围**：只在上述 `m9` worktree 写代码和提交；主仓库和其他 worktree 保持原样。用户的真实仓库、真实凭据库、真实 AI 服务和系统配置不属于测试范围。
- **证据目录**：证据根目录用 mktemp 或 tempfile 独占创建，并记录实际路径；测试仓库、应用数据、模拟 HTTP 服务都放在其下。保留原始 stdout/stderr、精确命令、退出码和源码 SHA，失败的原件也保留。
- **安全边界**：
  - 保留现有 Git 安全边界、回收区、写前身份与指纹、结构化双语错误；
  - Git 只通过 Rust 层调用系统 Git CLI，生成提交信息所需的 diff 也一样；
  - 更改列表模式只在私有 index 上读取，不改用户的暂存区。
- **密钥**：AI 密钥只进系统凭据库。前端读不到原文；设置、日志、诊断信息里都没有密钥。
- **平台条件**：Windows 降级只改 Windows 路径；macOS 外观不变。平台条件集中放置（Q55）。不能在 macOS 上把没编译过的 `cfg(windows)` 代码说成已通过。
- **文案**：新文案和错误键照 x18n 与 x18n-check 技能，通过 CLI 管理，不手改 locale，不删除旧键。
- **不做的事**：
  - 不修改用户的 Git 配置、profile、执行策略，也不改全局 MCP 或 agent 配置；
  - 不推送、不开 PR、不打标签、不运行远端 Actions、不创建 Release。`m9` 的推送由规划方按 Q216 进行。

## 实施与验证

1. 确认 cwd、分支和基线提交，阅读任务和依赖。在已确认的边界内自行做工程选型；遇到真实的产品取舍，在对应任务里写 needs-info，继续不依赖它的部分。
2. 同一批相关修改完成后集中检查。复用现有 Rust 真实 Git 临时仓库测试，只为规格 Testing Decisions 列出的回归点新增测试，不引入新框架。
3. 交付前运行以下检查。失败修复后只重跑受影响的检查；依赖可用 `pnpm install --frozen-lockfile`；不能拿旧二进制冒称通过。
   - `pnpm exec tsc -b`、`pnpm lint`、`pnpm build`、x18n check；
   - `/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml`（完整目标）；
   - `cargo check --features measure`；
   - 便携特性检查。
4. 按真实证据勾选验收项。性能门槛、Windows 表现、真实 AI 服务只写待验步骤，不勾选。
5. 按明确的路径提交到 `m9`。写 `report.md`，内容包括：
   - 固定实现 SHA、改动内容；
   - 精确的检查命令与结果、原始证据位置；
   - 本机记录的打开与切换分段耗时；
   - 未验证的项及原因、遗留问题。

   整包交付后交给规划方。Pi 不自行发起正式审查，不合并 main，不删除 worktree。

正式审查由规划方按 code-review 两轴进行，返修最多一次（Q113），之后由规划方接手。只有模型请求或渠道出错才按 Q137 的顺序换模型：GLM-5.3-Flash → GLM-5.3 → deepseek-v4.1-flash → gemini-3.8-flash；编译或测试失败不算渠道错误。
