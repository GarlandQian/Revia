# M7 Paseo / Pi 实施交接

Status: ready-for-agent
日期：2026-10-30
规划方：Codex；实施方：同一个 Paseo / Pi 会话。
用户已确认：Q193–Q199，第四版外观和确认清单补充项通过，可以开始。

## 业务目标与范围

把 [M7 规格](./spec.md) 中任务 01–12 的已就绪扩展工作流完整接入现有应用。读取 [任务图](./issues/) 和各任务，按依赖推进。整包以 m7 为集成分支；只在该分支的独立 worktree 修改、检查与提交，不在 main 写产品代码。

任务 13 更改列表是 needs-info：Q120 / ADR-0003 要求先实测 Rebased 终端已暂存内容，当前没有桌面操控入口，AXIsProcessTrusted() 返回 false。此门槛只阻塞任务 13 及最终任务 14；不猜测 index 行为，不拿静态源码 / mock 代替。可继续任务 01–12，并明确剩余阻塞。外观与其他已确认项不再向用户重复提问。

本交接是 implement-spec 工作流在本仓库的执行范围：使用同一个实施会话按任务图完成；不另派功能实施代理、创建功能 worktree、打开 PR 或自行作正式独立审查。仓库和用户既有集中验证策略优先于技能中频繁跑全套或默认 TDD 的通用建议。

## 必读与素材

1. 根 AGENTS、docs/README 的决策、GLOSSARY 与相关 ADR。
2. spec、issues、confirmation；必要时读产品 / UI / 技术方向。
3. docs/agents/local-validation.md：业务目的、隔离、证据和验收分工。
4. 外观第四版及 mockups/react-bits 的固定源码、参数适配和完整许可。产品接入不得只复制旧 CSS 冒称已使用组件；按真实布局适配。
5. GitHub 与 distribution facts 记录官方机制；上游库与版本在实现时以官方文档和锁文件核实。
6. x18n / x18n-check 技能：locale 通过 CLI / MCP 管理，既有中文源 / 英文目标、术语、错误严重性不改。

## 工作区与证据

- 主仓库：/Volumes/T7 Shield/project/Revia，main；实施前固定文档提交由证据 manifest 记录。
- 实施 worktree：/Volumes/T7 Shield/project/Revia-m7，m7。
- 本批独占证据根：/private/tmp/revia-m7-batch-g5scyw0r。
- 使用该根下由 mkdtemp / tempfile 唯一创建的 fixture、HOME、appdata、local remote；不复用 M6、用户仓库、实际 GitHub 账号或钥匙串。
- 每个检查保存固定源码或源码散列、精确 argv、真实 exit、原 stdout / stderr；失败、修改前源码、临时 helper 原件保留。
- 同一任务全部相关修改后集中最小验证；共享 / 核心契约改动才扩大，最后完整相关检查。失败修复只重跑受影响项，不因想清理 helper 而改完后沿用旧通过。
- 不覆盖此前证据目录，不删除未知归属目录；测试进程只结束自己创建且记录的进程。

## 工程契约

- Git 在 Rust 中调用系统 CLI；前端只调用带类型的受控操作。仅 stash 删除 / 清空沿 Q173 git2 窄例外。
- 编辑保存、PR 检出、更改列表和既有写操作沿身份、diff 指纹、写门禁、关闭协调和回收保护；不删除 index.lock。
- 网络 / 终端 / 更新测试接缝放在外部边界，生产入口真实执行核心行为；测试服务仅 test 构建使用，发布版来源受信任。
- 真实 PTY、独占本地 HTTP、真实 Git / local remote 可测；真实账号、钥匙串、公开发布、Windows 实机与桌面由 Codex 分开验收。
- 不读取或修改用户 Git 配置、凭据或系统偏好；不要生成生产签名身份、把测试公钥当生产公钥，或借用其他 GitHub App。
- App client_id / 更新正式公钥缺失时实现明确配置与说明；Token 备用可继续，不能硬编码虚假可用值。正式注册 / Secrets / CI 运行未执行如实报告。
- 英文用户报错通过结构化 Rust 载荷和生产 formatter 呈现；接口、回执、错误随目标身份绑定，迟到结果不得影响新目标。
- 原始终端输出 / Git stdout / stderr 不进入跨重启诊断；白名单提取与脱敏从持久化入口落实。

## 交付

- 任务验收项确已实现并有检查证据才勾选；技术任务只验证其定义范围，真实桌面统一留任务 14。部分实现或无法验证保留状态，不批量伪标 done。
- 按明确路径提交到 m7；不推送、不 PR、不打标签、不创建 Release，不合 main、不删 worktree。
- 在 report 记录固定 SHA、改动、真实检查、失败、未完成项与目录。实施结束一次交给 Codex，不因单项完成反复发交付请求。
- 首轮全范围两轴由 Codex 发起；所有正式审查意见追加相应任务 Comments。实现交付后的 Pi 正式返修最多一次（Q113），返修后仍有明确问题由 Codex 接手。
- 渠道错误才按 Q137：GLM-5.3-Flash → GLM-5.3 → deepseek-v4.1-flash → gemini-3.8-flash，保留改动与失败；编译 / 测试失败不是换模型理由。旧 M6 会话保持 closed，不复用。
- 不能把 Pi 自述、headless 或模拟 IPC 作为实际桌面通过；Codex 固定 Release 后验证。
