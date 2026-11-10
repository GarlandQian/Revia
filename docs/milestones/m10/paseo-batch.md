# M10 Paseo / Pi 实施交接

Status: ready-for-agent
日期：2026-11-09

## 目标与固定输入

将三个架构候选全部深化，保留既有产品行为。源码基线为 2b1e504479de5de9b2fc399dd81172df42d9d52c；实际开始前记录包含本规格的 m10 提交。允许修改的唯一实施目录：`/Volumes/T7 Shield/project/Revia-m10`，分支 `m10`。

先读 AGENTS.md、docs/agents/local-validation.md、docs/README.md、GLOSSARY.md、相关 ADR，再用 `/Users/garland/.config/magpie/library/skills/implement-spec/SKILL.md` 阅读本目录 spec.md 和 issues/。同一 Pi 会话一次完成 01、02、03；不另派实施者、不另建功能 worktree、不逐任务等待规划方。04 两轴审查与 05 真实桌面由规划方负责。

用户的集中验证规则优先于 implement-spec 中的 TDD/逐步骤测试默认。接口具体形状可自行决定，不能改变产品契约。

## 导航与现有 seam

- 01：src/App.tsx 的 openPath、refresh、readAndAccept、refreshTarget、recoveryReadOnce、watchStatus 轮询、环境事件和 reloadEnvironment；src/lib/refreshGate.ts；tools/test-refresh-gate.mjs。注意手动 refresh 推进 activation 但未同步 watchWorktree 的静态风险。保留现有 ReviewOverlay / SnapshotGate 的 depth。
- 02：src-tauri/src/commands.rs 的 session/capture_environment/install_environment/spawn_env_capture/resolve_environment_now/env_gate；environment.rs 的缓存、generation、wait；session.rs 的 apply_environment、superseded 与 acquire_write_lock。现有测试 src-tauri/tests/environment_concurrency.rs。注意 complete_generation 检查与 complete 写入之间的重载间隙，必须保证旧代次不能重新发布。
- 03：src/components/app/CommitComposer.tsx 的 AI effects 和 actions，src/App.tsx 的 composerStateRef/editComposer 与回调，src/lib/commitDraft.ts，Rust ai/commands.rs 的既有事件契约。生产生成的请求目标须与草稿状态组合，不能只移动 React effects。

## 隔离与验证

- 测试只使用本轮独占创建的临时 Git 仓库、独立应用数据、受控解析器和本机模拟 HTTP；不读写用户真实仓库、系统凭据、Git 配置或 shell profile。
- 用 mktemp/tempfile 创建证据根并在 report.md 记录路径。保留源码 SHA、精确命令、真实退出码、原始 stdout/stderr；失败原件也保留。
- 不推送、不开 PR、不打标签、不发布、不合并 main、不删除任何 worktree；不改主目录 .gitignore 或任何全局 MCP / agent 配置。
- 完成所有相关修改后集中执行：pnpm exec tsc -b、pnpm lint、pnpm build、现有 tools/test-*.mjs Node 行为测试、完整 /opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml。共享 Rust 构建可使用已确认同一源码来源的缓存，但证据必须来自本轮命令；不要宣称旧二进制通过。
- 不新增框架。若确需修改翻译键，先读 x18n / x18n-check 技能，通过 CLI 管理；优先复用现有文案。
- 按明确路径暂存并提交实现到 m10。report.md 写完整改动、固定实现 SHA、检查和未验内容。01–03 自动检查通过可勾选对应项，规划方审查和桌面项不可冒称 done。

## 模型与返修

按 Q137：GLM-5.3-Flash → GLM-5.3 → deepseek-v4.1-flash → gemini-3.8-flash，只有模型请求/渠道错误才回退；编译测试失败由当前实施者修复。正式审查返修最多一次（Q113），随后规划方接管；后续只审新增量（Q174）。

## 派发记录

- 文档提交：af0a609。Paseo Pi 会话：`17f5162d-49bd-49cb-af0c-029e002eddd7`。
- 本轮证据根：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m10-evidence-ln6xfi08`。
- 首选 GLM-5.3-Flash 返回渠道错误 `400 cpa: exceed quota limit`，尚无实现改动；按 Q137 回退。
