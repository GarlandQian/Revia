# M11 Paseo / Pi 实施交接

Status: ready-for-agent
日期：2026-11-11

业务目标：修复用户无法填写 AI 设置，并从当前服务自动获取可选择的模型。基线 9f9250c73429e34a326df50ba0c497985d76cf18；开始先记录包含本规格的 m11 SHA。唯一实施目录 `/Volumes/T7 Shield/project/Revia-m11`，分支 m11。

先读 AGENTS.md、docs/agents/local-validation.md、M11 spec 与任务 01，使用 `/Users/garland/.config/magpie/library/skills/implement/SKILL.md` 实施。无需再问已覆盖的取舍，不另派实施者，不另建 worktree，不修改 main。02 审查/桌面由规划方负责。

导航：SettingsDialog.tsx:442 的 loadAi、834/846/896 的 blur、463 的 saveAiKey；App.tsx:4715 的 updateSettings 丢四字段；src/lib/ipc.ts；Rust ai/commands.rs 的 ai_set_key/ai_clear_key 与 settings.rs:826。测试可复用 tools/test-m6-file-history.mjs 的 TSX/hook 执行及 tools/test-commit-generation.mjs 的受控 Promise；Rust ai/mod.rs 既有凭据 mock。避免只造一个独立 helper 而生产接线未测试。

证据根：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m11-evidence-t5lri9em`。为自己建立子目录；每个测试 fixture 用 tempfile/mktemp 独占。不得读取用户 AI 配置/密钥，不访问真实 AI 服务，不修改用户仓库、Git 配置或全局 agent/MCP 配置。测试使用合成 key 与 loopback HTTP；不要把任何真实密钥写入日志。

所有相关修改完成后集中运行最小相关检查。使用现有 pnpm 和 /opt/homebrew/bin/cargo，可复用来源明确的构建缓存（记录 CARGO_TARGET_DIR），不能拿旧结果冒充通过。前端类型、lint、build、相关 Node、Rust ai/设置回归、翻译检查。新增 UI 文案用 `/Users/garland/.codex/skills/x18n/SKILL.md` 的 CLI；检查先读 x18n-check。不新增框架或依赖。

按明确文件暂存并提交 m11，写 report.md：实现 SHA、精确命令、退出码、原始 stdout/stderr 路径、未验项。不要做真实桌面、合并、推送、打标签或发布，不清理 worktree。

模型按 Q137：GLM-5.3-Flash → GLM-5.3 → deepseek-v4.1-flash → gemini-3.8-flash，仅请求/渠道错误回退，编译测试问题当前实施者修正。正式返修最多一次（Q113），随后规划方接管；后续只审增量（Q174）。
