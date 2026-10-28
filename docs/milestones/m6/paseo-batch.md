# M6 整包实施交接（给 Pi）

## 业务目标与派发方式

在 Revia 中完成冲突后的继续操作、stash、历史改写、文件历史 / blame、外部工具和中英文日常工作流，文件、暂存区、历史与用户选择一致。
按 Q172 同一个 Pi 会话一次实现 [规格](./spec.md) 全部内容，单一任务是 [01](./issues/01-complete-m6.md)。内部安排实现顺序，不逐功能等规划方、不按功能开新实施代理或另建工作树。
使用 implement-spec 的整份规格思路；用户 Q172 优先于该 skill 默认的按任务多代理 / 合并编排。实现、必要回归与集中检查完成后整包交付；规划方之后独立审查、桌面验收。

## 工作范围

- 唯一允许修改的实施目录：`/Users/garland/.codex/worktrees/revia-m6/Revia`，分支 `m6`。该 worktree 由 Codex 管理，Paseo 只登记为现有目录，不能再次创建 checkout 或切主目录分支。
- 主目录 `/Volumes/T7 Shield/project/Revia` 保持 main；其他仓库、旧 worktree、全局 Pi / MCP / Git 配置均不修改。
- 产品起点为 main 的 M5 集成代码，规划提交起点 `3194c5bf2b25741104b3bb032151321118c7c535`；实际派发前已同步其后的本轮规格提交。启动时自行记录 worktree HEAD、分支、工作区状态，以工作树实际文件为准。
- 本轮独占证据根：`/private/tmp/revia-m6-batch-ig_nawgt`。用 `mktemp -d` 创建本次实施子目录，记录真实路径；测试 fixture 与独立应用数据均由当前测试创建。只操作自建 Git 仓库，本地远端仅用自建 `file://` 裸仓库。
- 所有提交在 `m6`，说明以 `M6：` 开头，按明确文件暂存。内部可分阶段提交以保护进度，最后仍是一次整包交付。主目录规划提交、main 合并、推送、PR、发布与清理 worktree 都由规划方另行处理。

## 先读

1. AGENTS.md、GLOSSARY.md、docs/agents/local-validation.md、docs/agents/issue-tracker.md。
2. docs/README.md 的 Q90–Q96、Q122–Q124、Q129、Q131、Q137、Q143、Q155–Q172；相关 ADR，尤其 0010 / 0012 / 0013。
3. 本目录 spec.md、issues/01-complete-m6.md、i18n-proposal.md、references.md；backlog 07 在本轮，其他 backlog 非本轮任务。
4. 现有 Git / session / operation / recovery / remote_ops、历史数据源、提交门禁、统一写生命周期、翻译字典与 getErrorMessage；复用真实生产入口，不复制一个测试专用实现。

## x18n MCP 与 skills：先确认目标再写资源

- 读取实际安装的 `/Users/garland/.pi/agent/skills/x18n/SKILL.md` 和 `/Users/garland/.pi/agent/skills/x18n-check/SKILL.md`，并遵守本轮已确认的文案规则。Codex 已核对 Pi 全局 MCP 的 `x18n` 为 connected，但这不代替你在本次 Paseo 会话核对加载与项目根。
- 在本会话列出 x18n 工具，先用 status / keys / get 等只读调用确认服务实际针对当前 `m6` 目录；若存在多个项目，写命令显式选择当前项目。
- 项目级依赖 / 配置在当前 worktree 建立，运行时与 React 接线自行选择适合的实现并记录实际锁定版本。不要把 MCP 已连接当作项目已完成初始化。
- 若 MCP 根不属于本 worktree，先使用服务支持的根 / 项目参数；不能向主目录误写。无法指向本目录时记录具体限制，在本 worktree 使用项目 CLI 的显式根作为等价工具路径，保留 MCP 查询证据；不改全局配置、不安装全局软件来绕过。
- 优先复用已有语义键，MCP 或 CLI 写资源，保留占位符、类型与参数；配置可编辑，资源不能直接手工重写。大范围改名、删未用键或降低检查级别不属于授权。
- 用户已定源语言 zh-CN、目标 en、现有嵌套语义分组与 camelCase 末级键；术语表唯一修订是 Snapshot Before Commit。不得把源语言和首次启动系统语言选择混为一事。
- 函数式文案转参数文案，动态查表转完整字面量键映射，覆盖 dateLabels / readonlyReasons / merge.unavailable。真实数量盘点写报告，不用用户估计的 66 / 784 机械判断完成。
- 两条验证：check 的缺失 / 占位符 / 语法错误必须失败；extract --list 的界面写死中文候选也必须使本地门禁失败。调试 / 测量日志用最小范围 x18n-ignore 标注，不翻译，不能通过忽略所有界面文件让检查通过。

## 安全与实现重点

- 规格的安全规则是写契约，保持固定目标、写前状态验证、回收、同 worktree 门禁、跨 worktree 归属、结果待核对和关闭保护。
- stash 对象 / 列表跨 worktree 共享，要有仓库级协调、精确对象身份和安全删除。完整回收归档在 Revia 数据目录，不给用户仓库加隐藏引用；必须验证原对象被清理后仍可完整取回。
- rebase 编辑停点允许 amend / 新提交用于拆分，冲突停点仍禁用普通提交。不能只看冲突数推导停点。空 cherry-pick 仅按确认过的策略自动处理，结果列出跳过项。
- 迁移 Rust 所有用户可见错误 / 原因 / 通知 / 结果说明为错误类型与参数，前端按语言呈现；原 Git 输出、调试与测量日志保留。不能把整段中文塞进 params、漏掉结构化结果中的字符串，或以对象字符串显示。
- 主界面沿用 M4/M5 的布局组件、主题和字体；新模块按领域组织，App 只接线。双语切换保持页签、草稿、合并编辑和撤销历史，不重载整页。
- Rebased 行为已知时照做；资料中的未知不是已验收事实。必要的未定产品边界写证据、报告安全禁用的具体入口，继续其他独立工作；不能自行默认 merge 主线、伪造支持能力或缩减整包为只做双语。
- import 自动解决仅在未排期 backlog；M7/M8 其他功能保持排期。

## 验证、证据与交付

先完成相关修改，再集中验证。此次跨共享错误契约、提交门禁和双语全界面，执行既有完整前后端检查，不新增测试框架：

1. 必要时安装当前工作树项目依赖；已有依赖用 frozen lockfile，新依赖需同步 lockfile；新 Rust 导出类型生成绑定。
2. 类型检查、lint、前端 build；cargo test --manifest-path src-tauri/Cargo.toml；现有 tools/test-*.mjs 的 Node 套件。
3. x18n check --json；提取扫描写死中文的独立门禁。未使用键和术语警告完整列出，不能静默删键。
4. 新 / 调整的真实 Git 回归覆盖 spec V；英文用户错误必须从实际 Git 失败通过生产载荷与英文格式化链路断言，不以模拟后端、只测试错误码或只检查中文字面量不存在替代。

每次命令保存源码 / 工作树差异、精确 argv、实际退出码、原始 stdout / stderr；失败原件不覆盖，修复后只重跑受影响项。任何缺 fixture / 取样失败都如实失败，不能跳过报成功。

- 更新 report.md：按规格 C/S/O/H/F/T/L/D/V 对照实现、生产入口、检查结果与证据路径；列真正未实现 / 未测 / 未验收项、偏差和本机 Rebased 待对照点。
- 不在本轮替规划方跑桌面操作或宣称独立审查、D06/Q98、最终集成通过；实施方可完成自身自动化，真实桌面由规划方后续在固定包上进行。
- 整包交付时任务转 ready-for-human（有真实缺项则保留准确状态），规划方三项验收不勾选，不标 done。最终回复源码提交 SHA、工作树状态、检查摘要与报告位置。

模型渠道失败由规划方保存原件并按 Q137 顺序换模：GLM-5.3-Flash → GLM-5.3 → deepseek-v4.1-flash → gemini-3.8-flash；四个都失败才由规划方接手。测试失败、等待超时不当成渠道失败。整包审查只交 Pi 一次返修，仍不合格按 Q113 由 Codex 接手。
