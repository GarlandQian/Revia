# 01：一次完成 M6 全部功能

Status: done
Blocked by: 无

## What to build

在同一个 `m6` worktree、同一个 Paseo / Pi 会话中，实现 [M6 规格](../spec.md) 全部端到端能力后统一交付。
以下是单一整包任务的验收清单，不按功能分派、不逐项等规划方审查。内部实现次序由实施方自行安排。

- [ ] C：固定目标、写门禁、相关状态校验、覆盖前回收、关闭保护与结果待核对保持。
- [ ] S：stash 读取、保存、应用、分支应用、删除 / 清空及完整独立回收与取回。
- [ ] O：进行中操作继续 / 中止 / 跳过、自动保存恢复与来源通知、编辑停点提交 / 拆分。
- [ ] H：日志 cherry-pick / revert / reset / 改信息 / 交互式 rebase，空提交策略及结果。
- [ ] F：文件历史（当前 / 所有分支）和 blame 三开关 / 上一版本导航。
- [ ] T：可选外部 diff / 合并工具，仍保留内置合并并核对写结果。
- [ ] L：x18n MCP / skills 接线，静态双语资源、占位符迁移、字面量键映射、结构化 Rust 用户错误及语言运行时。
- [x] L04：check 与 extract --list 两条门禁均按真实输出判定；调试 / 测量日志正确标注。
- [ ] D01：日志日期列折行修复，含英文窄窗。
- [ ] V：真实 Git / 中断 / 并发 / 完整回收 / 英文生产错误回归，集中检查与原始证据。
- [ ] 完整报告、明确文件提交、未完成项与偏差清单。
- [ ] 规划方独立两轴审查（实施方不自勾）。
- [ ] 规划方固定 Release 与真实桌面 / Rebased / 双语验收（实施方不自勾）。
- [ ] 规划方验收后的最终合并（实施方不执行）。

## Comments

2026-10-28，Codex：按用户 Q172 一次整包派发；一张任务单覆盖全部范围，替代按功能拆派。
Q169–Q171 的迁移、术语与提取扫描是整包交付的一部分。Q167 import 自动解决仅在未排期 backlog。

2026-10-29，实施方：M6 全部实现项已完成，最终自动化检查见 [report.md](../report.md)：Rust 576 passed / 22 个真实 Git M6 测试，Node 433 passed，tsc / lint / build / x18n check / 硬编码中文门禁均退出 0，`git diff --check` 退出 0。未做规划方的独立两轴审查、固定 Release 真实桌面验收和最终合并，三项保持未勾选。

### 2026-10-29：Codex 规划方第一次独立整包审查（Q113 唯一返修）

固定比较：`git diff 26b68d1fb0315cb85376932a8440204c118dfb78...1fdca4ed423a39cc8630caa010339115ff7310c7`，merge-base 为 `fad6483a9dd754c35677c32741c1978d419e8b56`。代码提交 `62720130feb1416903fed62b002f6612bb7f769a`，其后仅报告文档改动。

已核对最终检查原件，Rust 576/0/1、M6 22/0、Node 433/0、tsc/lint/build 退出 0、x18n 0 error/62 warning、中文提取正向 0/负向 1 与固定代码对应。这些记录保持有效，但不能代替以下未满足的生产契约。

本轮使用全新的两个独立审查代理，报告按轴分别保留，不跨轴合并或重新排序。实现和验证清单返回未通过状态；L04 已核对通过的门禁项保留。未做的英文窄窗不能算 D01 已完成验收。下面所有审查项和补充项作为一份完整返修清单交回原 Pi，会话、worktree 和模型不变，不按功能拆派；这是 Q113 的唯一一次正式返修。完成后规划方再开新的两轴复核，剩余不合格项由 Codex 接手。


#### Standards

固定范围：`26b68d1...1fdca4e`（merge-base `fad6483`）。以下均为文档规则冲突；未把 smell heuristic 当成硬违规。

1. **[P1] 旧确认可写入新 worktree。** `src/App.tsx:4750–4759,5597–5606`：reword 详情 await 后不核对原 worktree/activation；`HistoryActionRequest` 不保存归属，而对话框接收当前 worktree。A 的读取途中切到同仓库 B，旧确认可改写 B。违反技术方向 Q89/Q90“固定目标、旧响应不得污染”。应冻结归属并使旧请求/确认失效；外部工具和 stash 确认同样缺少归属封装。

2. **[P1] 覆盖没有确认版本契约。** `src-tauri/src/rewrite.rs:532–540,642–650,695–714`：请求只有 expected HEAD，preflight 不返回文件/index 指纹，且 HEAD/状态在写锁前检查。确认后编辑同文件但不移动 HEAD，Force/Hard 会接受并覆盖新内容。违反 ADR-0005/0009；应携带相关状态指纹，锁内及覆盖前复核，不能以先存回收代替拒绝旧确认。

3. **[P1] M6 回收记录永远没有完成基线。** `rewrite.rs:325–339`、`stash.rs:341–344`、`autostash.rs:359–362`、`operation_flow.rs:69–73`、`external_tools.rs:271–278`：save 后丢弃 record/RecordUse，从未 finish。成功的 Hard reset/覆盖/中止等记录始终 NoBaseline，普通恢复被 `recovery.rs:1178–1186` 拒绝。违反 ADR-0010/Q91 的完成后基线与结果记录；应贯穿写生命周期并持久化真实 after/outcome。

4. **[P1] 自动 stash 仅按路径认领。** `autostash.rs:27–35,71–78,262–290`：持久 link 无 filesystem identity，同路径重建 linked worktree 后仍会把原 stash 应用到新目录。违反 ADR-0010/Q87；保存并核对完整 WorktreeIdentity，身份不明只读保留。

5. **[P1] stash 归档仍依赖原祖先对象。** `stash_archive.rs:173–207,362–369`：只打包直接父提交及其树，既不保存祖先闭包也不声明 shallow 边界，却用 fetch 恢复。若旧基点历史也被改写并 GC，archive 的提交图断裂，upload-pack 无法遍历缺失祖先。违反 ADR-0012“不得依赖原库对象留存”；需自足、可实际导入的归档边界，并测旧基点历史亦不可达的 GC 场景。

6. **[P1] stash 删除仍存在浮动位置竞态。** `stash.rs:569–579,659–669`：读 OID 后再按 stash@{n} drop 或全 clear；终端在两条 Git 命令间 push/drop，可删除未确认且未归档条目。进程 Mutex 不约束终端。违反 Q90/Q168 的仓库共享资源与外部变化保护；删除须在保护实际 reflog/引用版本的原子边界核验，补受控并发回归。

7. **[P2] 结构化原因重新拼成中文参数。** `autostash.rs:223–234` 把 AppError 的 `to_string()` 塞入 detail；IO/JSON 转换的中文前缀来自 `error.rs:228–245`。关联落盘失败时英文 UI 仍含中文解释，违反 ADR-0013；使用 cause(error) 保留可翻译语义。

8. **[P2] 外部工具绕过 Git 执行日志与输出边界。** `external_tools.rs:411–431` 直接 Command.output()，未走 Git 执行器，且全量收集后才截断。真实 difftool/mergetool 不入命令日志，异常输出可无界占内存。违反技术方向“每条 Git 调用记录参数/退出码/耗时/输出”；扩展受控执行器支持外部工具的等待与有界流式输出。

结论：8 项（P1×6、P2×2）；最严重项为固定目标与覆盖前验证缺失。仅静态审查，未改产品、未运行额外测试或 GUI。

#### Spec

固定范围：`26b68d1...1fdca4e`（merge-base `fad6483`）。未通过，13 项：

1. **P1 — 参数文案导致界面崩溃**，`src/lib/i18n/build.ts:106`。L01：“覆盖已有及新增产品界面”。新增 `stash.archives`、`operation.inProgress`、`fileHistory.title` 等函数签名未进入 `FUNCTIONS`，运行时仍为字符串；打开对应界面即 TypeError。须统一类型与运行时注册。生产模块只读核对已在两种语言复现。

2. **P1 — stash 归档仍依赖原历史**，`src-tauri/src/stash_archive.rs:181`。S03：“依赖外部原对象的增量归档均不满足独立保存”。仅复制直接父提交，未复制其祖先或建立 shallow 边界；原仓库改写历史并 GC 后，恢复 fetch 遍历缺失祖先失败。须保存完整可取回闭包。

3. **P1 — 历史确认可改写另一 worktree**，`src/App.tsx:4752`、`:5597`。C01：“切 worktree…后旧确认失效”。读取 reword 正文期间切 A→B，迟到响应仍打开旧请求，却绑定当前 B；共享 HEAD 时校验可通过。须捕获 origin/activation，迟到或刷新后失效。

4. **P1 — rebase 停点丢失后续 reword**，`src-tauri/src/rewrite.rs:1034`、`operation_flow.rs:167`。H03：“支持冲突与编辑停点”。首个 edit/conflict 就删除消息脚本；Continue 改用 `GIT_EDITOR=true`，后面的指定说明不再执行。须持久保存计划和消息，直到完成/中止。

5. **P1 — 重排后预览对应错提交**，`HistoryActionDialog.tsx:335`。H03：“预览改写的提交和影响”。移动条目只移动 OID，标题和 reword 默认值仍取 `range[index]`，可能确认错误提交说明。须按 `entry.oid` 查源提交。

6. **P1 — 自动恢复冲突仍报完成**，`src/App.tsx:4789`。O02：“恢复冲突继续保留条目并提示”。`runHistoryRewrite` 忽略 `outcome.autostash`；原操作完成、恢复冲突/拒绝时仍发成功通知。须呈现恢复结果、保留关联并提供准确 stash 入口。

7. **P2 — 空提交策略未进入确认**，`HistoryActionDialog.tsx:259`。H02：“执行确认展示本次策略”。参数直接使用设置值，确认页没有 skip/keep；多选仅显示首条和数量。须展示本次策略及固定执行清单。

8. **P2 — 文件历史分页不固定且静默截断**，`file_history.rs:125`、`:243`。F01：“读取使用固定目标”“有截断…明确说明”。每页重读移动的 HEAD/--all，翻页时提交会重复/漏掉；2MB patch 截断无标识。须固定查询身份并返回截断状态。

9. **P2 — 工作副本 blame 实为 HEAD**，`file_history.rs:261`。F02：“保留原文件内容”。UI 的工作副本传 null，后端却显式传 HEAD，未提交行永不出现。须使用 Git 工作副本模式，并区分固定提交。

10. **P2 — 外部 diff 比较错暂存组**，`external_tools.rs:242`。C01：“相关…文件状态和确认条件”。同一路径两组都有改动时，总选 `--cached`，即使从未暂存 diff 启动。须传并校验 ChangesKind/版本指纹。

11. **P2 — 外部工具配置未接通**，`ExternalToolDialog.tsx:48`、`external_tools.rs:164`。T：“优先尊重已有 Git 工具配置”“选一次，保存”。UI 不读取 Git 配置，总传首个检测工具覆盖 fallback，选择也不持久化。须接通配置优先级及首次保存。

12. **P2 — 已显示错误不随语言切换**，`HistoryActionDialog.tsx:95`、`:154`。L03：“同一已存在错误切换语言后也能正确呈现”。多个新面板将格式化字符串存 state/通知，重渲染仍保留旧语言。须保存 payload，在呈现时格式化。

13. **P2 — 无依据限制 reword 数量**，`rewrite.rs:952`。H03：“提供…改信息”。前后端一律禁止计划中两个 reword；规格没有这一限制。须让每项消息绑定其提交并跨停点执行。

归档补充证据：`archive-connectivity-p8wniyq6/git-evidence.json`。这是自建仓库中按对象集合算法核对 Git 传输的机制探针：旧祖先被清理后 fetch 退出 128；不是生产入口回归。未修改产品源码、未操作用户仓库、未执行全套测试或 GUI。


### 根代理补充：生产双语与交付证据

- **R1 [P2] 提交前警告仍直显中文。** L01/L03 与 ADR-0013 要求全部用户可见 Rust 原因/结果结构化并随当前语言呈现。`src-tauri/src/commit_warnings.rs:75–111` 仍拼接 `CommitWarning.message`，`src/components/app/Dialogs.tsx:200` 直接显示。英文下触发游离 HEAD / rebase、CRLF、大文件、非法文件名警告就会显示中文，切语言也无法重新翻译。应返回可区分的类型与数据参数，由生产展示链路格式化；游离 HEAD 与 rebase 不可仅共用一个无细节 kind。资源仍须用 x18n MCP/CLI。核对其他未被错误码盘点覆盖的用户结果，而非只统计 `errors.*` 键。
- **E1 报告链接与状态。** `attempts/node-m6-i18n.attempt1.success.log` 不存在，实际有 `node-m6-i18n.attempt1.log` 与 `node-m6-i18n.final.log`；完整 Node 最终原件确实含该测试，请修报告引用。D01 的“含英文窄窗”和 V 的真实桌面部分尚未执行，区分已实现、自动化已测、规划方待验，不把待验项勾为已通过。
- **E2 失败证据如实说明。** 监控 `heartbeat-xxt39blg/` 已记录早期 `final-cargo-test.log` 在 branch_remote 失败后被同名 tee 覆盖，之后才切为独占 attempts。请给出可实际访问的早期原件；若已丢失，报告明确保留边界，不能笼统写“所有失败原件未覆盖”，也不能用成功重跑或重建摘要冒充丢失原件。后续 `cargo fmt --check` 曾失败于既有格式差异，也如实记录；该检查非必要，不为它格式化无关文件。

### 本次返修的验证与交付边界

- 业务目标仍是原完整 M6 规格，允许修改目录仍仅为 `m6` worktree。先落实全部相关修复，再集中跑最小相关检查；共享错误、回收和写生命周期改动按核心契约运行必要既有套件。失败只重跑受影响检查，不为报告数字重复全套。
- 对确定可回归的 TypeError、目标/版本过期、回收完成基线、GC 独立取回、stash 外部并发、停点后的多个 reword 等核心问题补现有测试。真实 Git 复现/回归放进仓库现有 Rust 集成测试，经生产入口执行；只用本轮 `mktemp` / `tempfile` 独占 fixture 和独立数据，本地裸远端。审查中的 archive 机制探针只作补充，不冒称生产入口回归。
- 不新增测试框架，不以模拟 Git 或仅断言源码字符串替代核心行为验证。前端参数文案须核对真实运行时字典和实际调用契约，不能只靠 `as` 断言或静态类型通过。
- 既有修改、归档、失败原件均保留。每个尝试使用独占文件名，保存精确 argv、真实退出码、完整 stdout/stderr 与源码身份；不覆盖日志。纯报告后续提交可以说明与已测代码的差异，不为自引用提交哈希无限重跑。
- 本次只做实施返修和自动化交付。规划方之后固定版本、新两轴复核及 Release/真实桌面验收；Pi 不用 AppleScript、osascript、screencapture 或模拟 IPC 界面充当桌面验收，不跑 D06/Q98 来关闭本轮任务。
- 更新整包报告，逐项说明修复、生产入口、真实检查证据和仍未完成的边界，提交 `m6` 后整包交付。不得推送、PR、合 main 或删除 worktree；不另开实施代理/会话、不顺手增加 M7 功能、不自行改变已确认决策。

两轴结论：Standards 8 项（P1×6、P2×2），最严重为目标与覆盖前验证；Spec 13 项（P1×6、P2×7），最严重为新增 M6 界面运行时崩溃。两轴均未通过，另有 R1/E1/E2 必须处理。


/private/tmp/revia-m6-batch-ig_nawgt/review-01-qwk2bi47

### 2026-10-29 07:55（Asia/Shanghai）：Codex 固定未完成返修并接管

Q113 的唯一正式返修已用完。Pi 的原会话 `c8376c03-6c8d-4df1-8e64-d4a6fdb6e333` 在多次续接后未完成安全项，部分交接也未完成；规划方已停止该回合并核实 `idle`，工作区保持 `background=false`，未创建新实施会话。该停止属于返修不合格后的 Q113 接管，不是模型渠道失败；DeepSeek 不切 Gemini。

当前 21 个修改路径完整保存在 `/private/tmp/revia-m6-batch-ig_nawgt/repair-partial-snapshot-ypbxgtkf/`，含原始 diff、逐文件 SHA256 与 Pi 返修清单。S2、S4、S5、S6、S7 及 Spec 第 6 项仍有明确未关闭内容；其他修改仅代表已落盘，尚待独立核对，不采用 Pi 的完成自述作验收。清单中关于预览按 OID、配置、错误即时翻译等声明也需按生产调用核实。

报告顶部已明确部分返修／未验收。首交付的成功日志不代表本快照通过；监控观察过本次 `cargo check` 编译失败，不能写成完全未做检查。新两轴复核将固定此快照对 main 的范围；复核后剩余实现由 Codex 直接处理，原完整 M6 规格、必要回归和真实桌面门槛保持不变。不推送、不 PR、不合 main、不删除 worktree。


### 2026-10-29：Codex 新两轴复核（Q113 后直接接管）

固定比较 `git diff 26b68d1fb0315cb85376932a8440204c118dfb78...e770e2218a2c37ad9ec0f35c7d672db346ba7421`，merge-base `fad6483`。本轮两位全新审查代理均已结束；只做静态及必要只读模块核对，未运行构建／整套测试／GUI。报告按轴原样保留，以下意见由 Codex 实施，不再交回 Pi。

#### Standards

固定源码：`26b68d1...e770e22`（三点比较，merge-base `fad6483`）。仅静态审查，未运行测试或构建。结论：7 项 P1、2 项 P2。

- **[P1] 覆盖确认仍无文件版本。** [rewrite.rs:97](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/rewrite.rs:97) 的请求仅带 HEAD；Force 同样缺指纹。确认后编辑文件但 HEAD 不变，Hard/Force 会覆盖新内容。违反 [ADR-0005](/Users/garland/.codex/worktrees/revia-m6/Revia/docs/adr/0005-write-ops-carry-diff-fingerprint.md)、[ADR-0009](/Users/garland/.codex/worktrees/revia-m6/Revia/docs/adr/0009-file-scoped-write-fingerprints.md) 的写前版本契约；须传确认指纹并在覆盖前复核。
- **[P1] 写锁取得过晚。** [rewrite.rs:525](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/rewrite.rs:525)、[operation_flow.rs:83](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/operation_flow.rs:83) 在锁外验证；[rewrite.rs:1040](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/rewrite.rs:1040) 甚至先清理、覆盖共享 rebase 计划再取锁。并发请求可让旧验证生效或破坏获锁者计划。违反 [技术方向](/Users/garland/.codex/worktrees/revia-m6/Revia/docs/technical-direction.md) Q90；锁须包住验证及全部共享写入。
- **[P1] 文件名被当作 pathspec。** [rewrite.rs:374](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/rewrite.rs:374)、[rewrite.rs:384](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/rewrite.rs:384) 的 clean/restore 未用字面路径。目标名 `a*` 会连带覆盖未保存的 `abc`。违反 [AGENTS.md](/Users/garland/.codex/worktrees/revia-m6/Revia/AGENTS.md)“覆盖前先回收”及 ADR-0010；须使用 `--literal-pathspecs`。
- **[P1] 回收生命周期仍不完整。** [rewrite.rs:338](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/rewrite.rs:338) 的 `RecordUse` 在 helper 返回即释放；[stash.rs:343](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/stash.rs:343)、[autostash.rs:380](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/autostash.rs:380)、[operation_flow.rs:72](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/operation_flow.rs:72)、[external_tools.rs:292](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/external_tools.rs:292) 同形，清空可删除在途唯一备份。[autostash.rs:205](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/autostash.rs:205) 还把超时/部分执行一律记 NotApplied；写后 finish 失败直接传播 IO 类别。违反 [技术方向](/Users/garland/.codex/worktrees/revia-m6/Revia/docs/technical-direction.md) Q91；贯穿持有 guard，失败保留真实基线或明确结果待核对。
- **[P1] 自动 stash 仍按路径认领。** [autostash.rs:27](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/autostash.rs:27)、[autostash.rs:71](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/autostash.rs:71) 未存文件系统身份；同路径重建 worktree 会应用旧 stash。违反 [ADR-0010](/Users/garland/.codex/worktrees/revia-m6/Revia/docs/adr/0010-recovery-records-preserve-worktree-state.md) Q87；须持久化并核验完整 WorktreeIdentity。
- **[P1] 归档仍非自足。** [stash_archive.rs:187](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/stash_archive.rs:187) 只复制直接父提交及树；[stash_archive.rs:362](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/stash_archive.rs:362) 却以 fetch 导入。旧基点祖先亦被改写、GC 后，缺祖先使传输失败。违反 [ADR-0012](/Users/garland/.codex/worktrees/revia-m6/Revia/docs/adr/0012-stash-recovery-outside-user-repository.md)“不依赖原对象”；需自足导入边界并覆盖该 GC 场景。
- **[P1] stash 删除竞态未消除。** [stash.rs:582](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/stash.rs:582)、[stash.rs:673](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/stash.rs:673) 在读列表后按浮动序号 drop/全 clear；终端插入条目可删掉未确认、未归档内容。违反 [产品方向](/Users/garland/.codex/worktrees/revia-m6/Revia/docs/product-direction.md)安全边界及 ADR-0012；须在原子引用/reflog边界验证，进程 Mutex 不足。
- **[P2] 中文原因仍被字符串化。** [autostash.rs:246](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/autostash.rs:246)、[session.rs:3346](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/session.rs:3346) 将 AppError Display 塞入参数，英文错误仍混中文。违反 [ADR-0013](/Users/garland/.codex/worktrees/revia-m6/Revia/docs/adr/0013-localize-structured-user-errors-at-presentation.md)；保留结构化 cause。
- **[P2] reword 迟到响应仍覆盖当前对话框。** [App.tsx:4762](/Users/garland/.codex/worktrees/revia-m6/Revia/src/App.tsx:4762) await 后未核验 activation；切到 B 后 A 响应仍可替换 B 对话框。违反 [技术方向](/Users/garland/.codex/worktrees/revia-m6/Revia/docs/technical-direction.md) Q89/Q90；发布前校验请求归属及序号。

首轮复核：#8 已关闭（受控 Git 执行器、有界读取）；#1 已修复错 worktree 写入，迟到界面仍未关；#3 成功路径已有 finish，不再是“永远无基线”，但上述生命周期问题仍在。#2、#4–7 未关闭。判断项：possible Duplicated Code——四组 protect helper 同形，宜归拢含 guard 的快照生命周期。


#### Spec

# Spec

固定 `26b68d1…e770e22`（merge-base `fad6483`）。未通过；未见范围扩张。规格行号均指 `docs/milestones/m6/spec.md`；下文 Rust 文件在 `src-tauri/src/`，面板在 `src/features/m6/`。

1. **P1 L03:171“绑定…同步”**：绑定/调用方漏 `detail`、`InvalidFilenameReason`、anchor、patchTruncated，相关编译被阻断（`src/lib/i18n/text.ts:8,247`、`src/lib/ipc.ts:717`、`src/types/bindings/CommitWarning.ts:6`、`FileHistoryPage.ts:4`、`FileHistoryEntry.ts:11`、`src-tauri/tests/m6_flows.rs:989`）。
2. **P1 S03:77“独立保存”**：归档只有直接父提交；旧祖先亦被 GC 后 fetch 失败（`stash_archive.rs:173–207,362`）。
3. **P1 C01:47“文件状态”**：确认只携带 HEAD，锁前检查；确认后编辑仍被 Force/Hard 覆盖（`rewrite.rs:56–65,525–537,704–723`）。
4. **P1 S03:78“不能误删新条目”**：读取后浮动 drop/clear 仍有终端竞态（`stash.rs:573–583,663–673`）；清空确认也未冻结集合，刷新会换目标（`StashPanel.tsx:410–425`）。
5. **P1 C01:47“worktree 身份”**：自动 stash 只比路径/commonDir；同路径重建后误恢复（`autostash.rs:27–35,71–78,277–280`）。
6. **P1 H03:124–126“改信息…冲突”**：消息队列逢 editor 就消费；pick 冲突 continue 会挪用后续 reword，耗尽则失败，须按原 OID/动作关联（`rewrite.rs:1047–1065`）。
7. **P1 O01:89“不能…冒认为编辑停点”**：edit 本身冲突、解决并暂存后，done 末行仍 edit，错误开放普通提交（`operation.rs:358,372–384`）。
8. **P1 H03:126“预览改写的提交”**：重排后标题/默认消息仍按 range[index]，对应另一 OID（`HistoryActionDialog.tsx:349,358,365`）。
9. **P1 O02:96“恢复冲突…提示”**：历史结果忽略 autostash，恢复冲突仍报成功；继续操作也漏 restored=false/conflicted=false（`src/App.tsx:4778–4805,4695–4700`）。
10. **P1 C02:56“完成回执失败…结果待核对”**：写后 finish_snapshot 的 IO/落盘失败仍返回 environment，写生命周期结束为已核对（`rewrite.rs:726–727`、`recovery.rs:994–997`、`src/App.tsx:4663`）。
11. **P2 F01:135“固定目标”**：anchor 只锁 HEAD，所有分支后页仍展开移动的 --all，新增提交造成重复/漏项（`file_history.rs:154–160`）。
12. **P2 T:146“尊重已有…配置”**：未列入 detected 的配置被首项替代再覆盖保存；设置允许程序路径，后端却拒绝（`ExternalToolDialog.tsx:55`、`src/App.tsx:5651`、`external_tools.rs:78–83,204`）。
13. **P2 L03:170“已存在错误切换语言”**：通知、日志详情、验证错误仍存译后字符串（`src/hooks/useNotifications.ts:57`、`src/features/log/LogView.tsx:363`、`HistoryActionDialog.tsx:197–218`）；关联落盘失败仍塞中文 error.to_string()（`autostash.rs:246`）。
14. **P2 F02:140“真实父关系”**：工作副本“上一版”取最近文件提交之父，跳过 HEAD 已提交版本（`FileHistoryDialog.tsx:143–147`）。

首轮确定关闭：#1 参数函数（中英生产模块只读执行均为 function）；#3 改写目标冻结/提交前校验；#7 策略与清单展示；#9 工作副本 blame；#10 ChangesKind；#8 的截断提示/当前分支 anchor。#13 数量限制已去，但见第6项；R1 警告已结构化且生产英文格式化成功，但绑定见第1项；#11 已接读写配置，残项见第12项。

仅静态与只读 Node 模块核对；未跑整套检查/GUI，未改源码。


两轴结论：Standards 9 项（P1×7、P2×2），最严重为覆盖前版本、写锁和备份生命周期；Spec 14 项（P1×10、P2×4），最严重为编译契约、历史停点及用户确认内容不符。两轴均未通过，完整 M6 范围和桌面验收保持未完成。下一步 Codex 一次落实全部相关修复再集中检查；只对必要核心回归补现有测试，不新增测试框架。

## Codex 接管检查点（2026-10-29，未验收）

已修复确认文件/index指纹、锁范围、literal pathspec、回收guard生命周期、完整祖先归档、worktree身份、按动作/OID持久reword、edit冲突门禁、全分支分页固定tips、工具配置与迟到响应，以及通知/验证错误的当前语言呈现。新增9项既有Rust真实Git回归、实际NotificationCard切换语言回归。自动stash先保存持久意图，以唯一说明定位自己的OID；恢复前写Applying，成功后写Restored，重启不会二次apply，并可只结束关联保留stash。

**S03仍未通过。** 系统Git的reflog删除边界不能携带已确认OID条件；正在等待用户决定是否为此评估ADR-0002的窄范围后端例外。单条删除、清空及自动清理已在共用出口拒绝删除，归档完整保存、原stash保留。这是临时安全封闭，不能解释为功能完成或用户已接受范围缩减。既有测试没有降低要求：最新M6结果27通过/4失败，失败分别是自动恢复删除、cherry-pick自动清理、应用后删除和单条删除，均对应该封闭。

本轮证据：`/private/tmp/revia-m6-batch-ig_nawgt/codex-checks-yf3rkhgl/`。封闭前M6真实Git31/31通过；封闭后上述4项如实失败。首次编译的旧测试漏传changes_kind已修正。Rust其余现有单元/集成检查已分别执行；沙箱内4个FSEvents用例收不到事件，精确升级后同用例通过（watcher-escalated、refresh-isolation-escalated），沙箱失败原件保留。其余原件见cargo-02、cargo-integrations、remaining-integrations。

前端build退出0（含tsc），lint退出0（既有unused-vars警告）；既有Node脚本逐项执行成功，实际Git错误载荷与既有通知即时切语言回归2/2通过。x18n在safeDeletionUnavailable新增前为1634键/双语完整、0 errors/63 warnings；新增后仍待最后复查。extract当轮0中文。原始argv、退出码、源码diff/哈希及所有失败原件保留。

本检查点未获新的独立审查、固定Release或真实桌面/Rebased/中英文窄窗验收；未续接Pi，未派第二次正式返修，未改main、推送、PR或删除worktree。

用户现已授权Q173/ADR-0014的窄范围后端例外评估。前述“等待用户决定”是检查点历史状态；事务实现与验证仍未完成。

## Q173 授权后的续接状态（2026-10-29 01:15 UTC 附近）

用户已授权窄范围后端例外，记录为 Q173 / ADR-0014，尚未引入 git2 依赖或实现事务，不再等待用户对该例外答复。剩余关键功能是 stash 删除/清空的持锁精确目标事务及其跨 worktree / 终端并发、锁冲突、失败回执验证。删除出口目前先归档后明确拒绝，4 项功能测试保持失败，不降低原断言。

额外盘点已迁移：CommitDraftPersistence 保留错误载荷（failureFor 在读取时格式化，failurePayloadFor 提供结构数据），App 本地验证错误改 code / params，草稿提示 lazy 呈现；冲突/回收区/三栏历史/左右 diff/自动 fetch 等错误在显示时格式化；CheckoutNote 替代中文 notes 字符串；commit 回执与线程错误保留结构化 cause；history 三栏不可用说明结构化。x18n 新增 callee 仅识别 localizedError 的完整字面量键，没有放宽扫描或严重程度。

最近检查原件在 codex-checks-yf3rkhgl：translation-contracts（现有 lib、branch_remote 31、commit_flow 12）通过；m6-receipt-errors（相关 lib、history_readonly 43、M6 29 通过/4 明确排除）通过；四个排除项正是 m6-safe-boundary 原件中实际失败项，不能汇总成全套通过。新增两项生产形状的 Saving / Restored 持久回执回归通过。实际 Git 错误/嵌套 cause/通知切语言与真实 CRLF/非法名/游离 HEAD 警告 3 项通过；草稿 8、历史 7、远端 32 亦通过。远端的旧静态断言因改存 payload、渲染时格式化失败一次，原件 i18n-ui-03 保留，按新契约同步后 remote-write-updated 通过。frontend-latest、lint-latest 通过；extract-latest 57 候选/0 中文；x18n-latest 1641 键/双语完整、0 错误、63 未用键警告，完整列表保留、不删键。

自动审批服务额度限制：官方 libgit2 transaction.c / git2 Reflog 文档读取、linked worktree 索引 git add、automation_update 新 prompt 均未执行，提示北京时间 12:12（04:12 UTC）后重试。原始错误见 approval-service-rejection.json。不得绕路、改渠道或在该时间前反复请求；没有安全拒绝或模型渠道错误。当前 Git 提交仍 e770e22，所有修改未暂存，无法在该服务恢复前固定新的源码提交/新两轴/Release 验收。监控更新失败，原 automation 仍 ACTIVE，但本 manifest / monitor / ledger 为最新状态。

后续：按原渠道恢复官方源码核实及依赖获取，验证候选事务实际持锁范围、reflog 与 ref 失败边界、仓库格式兼容；实施后集中必要检查，再明确文件暂存到 m6 固定源码。按 code-review skill 新开两轴，不复用 review02，不再派 Pi / 其他实施代理。最后固定 Release / 真实桌面 / Rebased / 中英文窄窗；未验收不合 main、推送、PR 或删 worktree。


### 2026-10-29 Codex 接管候选与 Q173

用户已授权窄范围例外；持锁精确删除/清空已实现，不再等待用户或审批服务。完整机制、依赖版本、仓库格式边界、源码证据与失败原件见 `../q173-validation.md`。本轮全套 Rust 603/0/1（M6 44/44）、Node 435/435、前端 build/tsc、lint、x18n 0 errors/63 unused warnings、提取 0 中文通过；最后命令日志长度调整另重跑受影响 stash 流程。review02 两轴原意见保持独立，接管后的整包候选将再开全新两轴审查；未正式验收，不续接 Pi、不发第二次返修、不合 main。


### 2026-10-29 review03：固定54f953d，两轴独立结果

Standards：8项明确问题（3 P1、5 P2）及1项启发式判断；最严重的是旧确认重绑及待核对退出保护。Spec：7项（3 P1、4 P2）；最严重的是ignored内容未回收即覆盖。两轴均未通过，以下保持分开，不合并排序。Codex直接修复，不再返交Pi。

## Standards

# Standards 独立审查（review-03）

固定源码：`54f953d9ec6f2545c452417adc27c07b86b56bb6`。比较：`git diff 26b68d1fb0315cb85376932a8440204c118dfb78...54f953d9ec6f2545c452417adc27c07b86b56bb6`。审查完整整包，未把检查通过视为免审；Q173/ADR-0014 的 stash 删除后端例外已计入。未实施、未操作 GUI、未构建 Release，未修改仓库。

## 正文

结论：Standards 不通过；8 项确定违反仓库契约，另有 1 项启发式重构判断。

1. **[P1] 旧中止／跳过确认会改绑新操作** — `src/features/m6/OperationBanner.tsx:141–144`、`src/App.tsx:4720–4728`。确认只保存 action，提交时取最新 worktree 与 metadataVersion。弹窗期间终端结束原操作并开启新操作后，旧确认可中止新操作。违反 `docs/README.md` Q89/Q90 的旧确认失效与目标固定规则。应冻结原目标及版本，刷新后使确认失效。
2. **[P1] 返回值中的待核对错误绕过退出保护** — `src/App.tsx:4697–4704`。只有 throw 才设置 needsVerification；stash 的 cleanupError、历史操作的 error 都可作为成功 Promise 返回。用户已选择等待退出时，`end(..., false)` 可先关闭窗口，未展示待核对结果。违反 Q91/Q96、`docs/technical-direction.md`“结果待核对时先展示，不静默退出”。
3. **[P1] Mixed reset 忽略确认指纹** — `src-tauri/src/rewrite.rs:787–807`。仅 Hard/Keep 校验 expected_state；确认后外部更新暂存区，Mixed 仍覆写新暂存状态。违反 ADR-0005 的“所有写操作执行前重新计算并比对指纹”。
4. **[P2] 外部合并缺少所见版本凭据** — `src-tauri/src/external_tools.rs:46–60, 361–397`。请求无 fingerprint，执行只检查路径仍冲突；同路径冲突版本变化后照样运行并可暂存结果。违反 ADR-0005、ADR-0009 的同文件相关状态校验规则。
5. **[P2] stash 保存超时被当成确定失败** — `src-tauri/src/stash.rs:361–368`。GitOutput.timed_out 未检查，杀进程后的 -1 仅返回 Git 类失败，即使 stash/工作区已部分变化。违反 Q91 的超时结果待核对契约。
6. **[P2] 仍把中文错误装进结构化参数** — `src-tauri/src/session.rs:3364–3369`、`commit_flow.rs:575–576`、`git/mod.rs:493–496, 543–548`。discard_pending 用 Display 抹平 cause；另外两处亦传中文原因。英文错误会夹中文。违反 ADR-0013“详细原因不能把整段中文塞进参数”。
7. **[P2] 快捷键占位符未填值** — `src/lib/i18n/build.ts:140–148`。只替换三条文案，遗漏 stage/review、noSelectionDetail、previousHunkShortcut、fontSizeHint/uiZoomHint；界面直接显示 `{shortcut}`／`{MOD_SYMBOLS}`。违反 Q169／`i18n-proposal.md`“快捷键参数保留平台差异”。
8. **[P2] stash 路径读取未解码 Git 引号** — `src-tauri/src/stash.rs:218–235`。name-status 未用 -z，却按换行／tab 原样取路径；默认 core.quotePath 下中文名显示为八进制转义，特殊路径也不正确。违反 ADR-0002 的机器可读状态约束及 Q81 的中文路径支持。
9. **[P2，判断项] possible Duplicated Code** — `operation_flow.rs:58–73`、`stash.rs:327–344`、`autostash.rs:400–414` 重复“收集全部相关路径→capture→context→save_protected”。共用已有 helper 可避免回收范围修复多处漂移；这是 baseline smell，非硬性标准违例。

## 证据补充

### 1. 原确认没有固定身份

`OperationBanner` 的确认状态为 `useState<OperationAction | null>(null)`（第33行），打开确认分别执行 `setConfirm("skip")` / `setConfirm("abort")`，不保存 worktree、kind 或 metadataVersion，没有 prop 变化时关闭确认的 effect。确认按钮调用当次渲染的 `onAction(action)`。`App.performOperationAction` 则从 `worktreeIdRef.current` 和最新 `operationState` 重建请求；后端对新 metadataVersion 的校验因此无法发现这是旧确认。无需用户切换弹窗背后的界面，终端操作及 watcher 刷新即可触发。

### 2. 返回载荷的错误没有进入 close coordinator

`performM6Write`：

```ts
let needsVerification = false;
try {
  const result = await work();
  // refresh...
  return result;
} catch (caught) {
  needsVerification = toAppError(caught).kind === "needsVerification";
  // ...
} finally {
  writeOrchestrator.end(targetWorktreeId, needsVerification);
}
```

`stash.rs:516–527`（apply cleanup）捕获事务错误并赋 `cleanup_error = Some(error.payload())` 后正常返回；事务提交失败可合法为 NeedsVerification（`stash_transaction.rs:254–257`）。`rewrite.rs:521–537` 在非零退出且存在操作状态时，把同类错误放进 `RewriteOutcome.error` 并正常返回。两者的通知处理都发生在 `await performM6Write(...)` 之后。应把结构化结果的待核对状态也交给统一写编排，再消费等待关闭请求。

### 3. Mixed reset 的确认凭据被丢弃

前端 `HistoryActionDialog.tsx:192–197` 对所有 reset 模式都发送 `expectedState: preflight.resetFingerprint`。后端仅对 `Hard | Keep` 调用 require_state；Mixed 仍会写暂存区。只校验 HEAD 不足以识别提交图不变、暂存区已变的情况。这里无须将正常 Mixed 行为改成回收／自动恢复，只须保证用户确认的版本仍有效。

### 4. 外部工具与内置合并契约不一致

`ExternalToolRequest` 只有 kind/path/tool/revision/changes_kind。Merge 路径只重新读取 status 并检查 entry.unmerged，之后 protect_external 保存当前内容。它无法判断当前冲突三方对象／工作区内容是否与用户打开页签时相同；内置 `WorktreeSession::merge_apply` 则明确接收并复核 expected_fingerprint。回收区保存不能替代 ADR-0005 的拒绝旧版本要求。

### 5. 超时载荷可观察性

`git/mod.rs:486–489` 超时后 kill，最终仍返回 `Ok(GitOutput { timed_out: true, exit_code: -1, ... })`（第553行起），并不统一 throw。stash.save 的 361–368 行只检查 exit_code；manual save 是新增生产调用路径，应识别 `timed_out` 并按 NeedsVerification 返回，随后重新读取真实状态。外部工具的 `external_tools.rs:443–469` 也将超时的 GitOutput 直接变成普通 ExternalToolOutcome；它同样需要保留待核对状态，而不能让普通 exit code 隐去超时语义。

### 6. 中文原因的具体来源

- `session.rs:2110–2114`：`discard_pending(format!("Git 退出码 {}：{}", ...))`；第2120行：`discard_pending("写后状态与已校验结果不同，可能有外部修改")`。第3369行以 `format!("{}", error)` 写入 params.error，前端 `formatAppError` 只递归 `cause`，不会翻译普通参数。
- `commit_flow.rs:576` 同时将 receipt 和 error（均 AppError）Display 为字符串，分别放入 params.receipt/error；前置校验失败再加回执失败时确定出现中文。
- `git/mod.rs:496` 将“Git stdin 写入线程中断”作为 String，随后第548行装入 params.error。IO 管道缺失的中文 `io::Error::other("Git stdin 管道不可用")`（第481行）亦会走同路径。

这些都不是 Git 原始输出或用户内容，不属于 ADR-0013 的免翻译范围。

### 7. 平台参数遗失的资源与使用点

资源既有模板示例：

```json
"stageFile": "暂存文件（{shortcut}A 或空格）",
"previousHunkShortcut": "上一处差异（{MOD_SYMBOLS}F7）",
"fontSizeHint": "9–24；也可以用 {MOD_SYMBOLS}+ / {MOD_SYMBOLS}- 调整。"
```

`materialize` 在不属于 FUNCTIONS 的字符串上原样返回（build.ts:128–130），`buildMessages` 又只填 commit/submitHint/gitPathPlaceholder 三条。`EditorTabs.tsx:314–315` 直接使用 previousHunkShortcut；`SettingsDialog.tsx:518,532` 直接使用 fontSizeHint/uiZoomHint；`App.tsx:5498` 直接使用 noSelectionDetail。与翻译两端占位符一致性无关，静态 check 可以通过而运行期仍保留占位符。

### 8. stash 名称解析

两套 files_args 均没有 -z；后续 `files_output.stdout_lossy().lines()` → `splitn(2, '\t')` → `parts.next().to_string()` 无 C 字符串反转义。标准 Git 输出可将中文／tab／换行路径加引号转义。该值继续作为 StashFile.path 展示，并与 patch 中路径匹配来计算统计，所以会显示错误名称或失去匹配。应使用 -z 及 NUL token 解析，与 file_history.rs 新实现保持一致。

### 9. baseline smell 引用

三处新代码均存在以下形状（省略模块前缀差异）：

```rust
let state = status::read(&session.git)?;
let mut paths = state.entries.iter()
    .flat_map(|entry| std::iter::once(entry.path.clone())
        .chain(entry.original_path.iter().cloned())).collect();
paths.sort();
paths.dedup();
let before = recovery::capture_files(&session.path, &paths)?;
let context = recovery::context_fingerprint(&session.git, &paths)?;
let record = store.save_protected(identity, source, target, before, context)?;
```

这是维护性判断，不宣称当前三处备份内容已不一致。

## 审查边界

正文均为固定源码的静态控制流／载荷／格式解析证据；未伪称运行了新的复现测试。没有覆盖用户仓库、应用数据或 GUI；末次 HEAD 核对仍为固定 SHA，仓库 git status --short 为空。Q173 事务具体已解决的并发序号／packed-ref 问题没有重复列为未修复，未将 git2 的窄范围使用误报成 ADR-0002 违规。


## Spec

# Spec 独立审查

固定源码 `54f953d9ec6f2545c452417adc27c07b86b56bb6`；完整三点比较 `26b68d1...54f953d`。结论：未通过，P1×3、P2×4；未发现额外范围扩张。

1. **[P1] Hard reset 漏回收 ignored 内容。** `src-tauri/src/rewrite.rs:784–807`。H02（spec:119）：“覆盖前回收”。旧目标跟踪 `generated.txt`，当前提交已移除并用 `.gitignore` 忽略它；磁盘独有内容不在普通 status 中。生产 reset 成功覆盖且 `recovery=[]`。须按目标树补齐实际覆盖路径。

2. **[P1] 旧会话可改写同路径的新仓库。** `src-tauri/src/rewrite.rs:780–788`、`src-tauri/src/session.rs:1575`。C01（spec:47）：“执行前绑定并重验 worktree 身份”。确认后移走原仓库，在原路径 clone 相同 HEAD 并放入相同确认内容，旧 reset 仍成功写入新仓库。须核对会话原始身份，不能现场重新认领。

3. **[P1] 中止／跳过确认偷换操作。** `src/features/m6/OperationBanner.tsx:33,141–144`、`src/App.tsx:4721–4728`。C01（spec:47）：“刷新、切仓库、切 worktree、环境重载后旧确认失效”。确认只存 action；外部中止 A、开启 B 后，刷新把回调绑定 B 的最新元数据，旧确认能中止／跳过 B。须冻结 origin 与元数据版本。

4. **[P2] rebase 范围来自筛选后的日志。** `src/features/m6/HistoryActionDialog.tsx:103–108,225–233`、`src/features/log/LogView.tsx:612–618`。H03（spec:125）：“通过固定范围、完整 OID 和许可动作构造计划，执行前重验”。文字／路径筛选漏掉范围内提交，默认全部分支又可能混入旁支；确认后被 Rust 拒绝，合法改写无法执行，旁支 merge 还会误禁用入口。须从当前 HEAD 的真实祖先范围取数。

5. **[P2] 文件历史把文件名当 pathspec。** `src-tauri/src/file_history.rs:159–170,255–268`。F01（spec:134）：“路径、重命名和不存在版本遵循 Git 的真实历史”。生产查询字面文件 `a*` 实际包含仅修改 `abc` 的提交与 patch。须对历史及 patch 查询使用字面路径。

6. **[P2] 已有分支错误仍冻结语言。** `src/features/branches/useBranches.ts:83`、`src/features/branches/BranchWidget.tsx:129`。L03（spec:170）：“同一已存在错误切换语言后也能正确呈现”。中文读取失败被格式化成 string 存 state，切英文仍直显中文；应保存载荷、展示时格式化。

7. **[P2] 全局工具选择覆盖仓库 Git 配置。** `src-tauri/src/external_tools.rs:148–153,177–195`。T（spec:146）：“优先尊重已有 Git 工具配置”。在 A 仓库选过工具后，B 的 `diff.tool`／`merge.tool` 被 Revia 全局设置遮住，默认启动错误工具。须先读当前 Git 配置，再回退保存的选择。

验证原件：[`spec-production-nndkemm_`](./spec-production-nndkemm_/manifest.json)；三项独占真实 Git／Rust 生产复现均暴露缺陷，Cargo 退出 101。完整说明见 [证据](./spec-evidence.md)。未实施、未运行全套／Release／GUI；临时测试已移除，工作树干净。


### 2026-10-29 Codex 对 review03 的修复与验证

Standards 1：确认冻结 origin（含 activation）和 OperationState；状态/元数据变化关闭确认，提交前再次核对原目标。2：成功 Promise 内的 error/cleanupError/关联错误也进入待核对退出保护，写后刷新失败同样不能静默关闭。3：Mixed 也核对确认指纹。4：外部工作区 diff/merge 都携带所见 fingerprint，备份前后复核，合并凭据来自实际 MergeView layout。5：真实 stash 与外部工具超时均 NeedsVerification；输出读取不再等继承管道的子进程无限退出。6：discard/stdin 保留结构化 cause，双重失败新增可选 related 载荷以保留两条原始错误并按当前语言显示。7：补齐平台键值，修正 commit/review/Shift/Control+Alt 的独立参数。8：stash 路径与统计统一 Git NUL 格式。9 为维护性判断：核心保存仍共用 save_protected，没有为五行路径收集再增加新抽象；不当作硬性安全违例。

Spec 1：reset 将 HEAD→目标树的所有实际变化路径并入指纹与覆盖前回收；ignored 原始字节有真实回归断言。2：统一 session 写锁入口核对原始文件系统身份，指纹也包含会话身份，不现场认领替换仓库。3 与 Standards 1 分别关闭。4：独立受控 rewrite_range 按固定 HEAD/目标的真实祖先读取范围，最大5000；菜单/计划不再依赖筛选日志或旁支；执行仍重验。5：文件历史与 patch 使用 literal pathspec。6：分支错误存结构载荷，正文及 tooltip 都渲染时翻译。7：仓库 Git 配置优先于全局 Revia 记忆，明确手动选项仍可供本次用户选择。

原件目录：`/private/tmp/revia-m6-batch-ig_nawgt/codex-checks-yf3rkhgl/`。`repair03-first` 编译失败（ts-rs optional 仅支持 Option），已修；`repair03-frontend` 两处旧调用类型错误，已修，失败原件保留。`repair03-second` lib+M6通过；`repair03-full-rust` 612 passed/0 failed/1既有忽略，M6 53/53；`repair03-full-node` 437/437；`repair03-frontend-02`（含tsc）通过，lint通过；`repair03-x18n` 0 errors/63 unused警告，未自动删键；`repair03-extract` 57候选/0中文。资源只经x18n get/apply，原件在 `repair03-i18n-k0ah1y6r`。绑定生成差异先保存，仅恢复逐行去尾后等同HEAD的格式差异，AppErrorPayload的related语义变更保留。

这些是接管修复和自动化结果，须再开全新两轴，固定Release/真实桌面/Rebased/中英窄窗尚未验收；不续接Pi、不推送、不PR、不合main或删除worktree。


### review04 独立审查（固定 37425c8；待 Codex 修复）

# Standards

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...37425c8ec44cad65d4587de8685caacc24f3f2de`。整包审查；以下为 7 项文档标准违反，另有 1 项判断性 smell。

1. **[P1] 历史改写缺少分支身份** — `src-tauri/src/rewrite.rs:220–230`。违反 `docs/product-direction.md`“写前重验状态”和 `docs/technical-direction.md`“旧确认失效”。仅比较 HEAD OID：终端切到同 OID 的另一分支后，旧 Soft reset 确认成功移动了新分支。须绑定并核对 symbolic HEAD，不能仅绑定提交。
2. **[P2] 自动 stash 分支忽略确认指纹** — `src-tauri/src/rewrite.rs:528–532`。违反 ADR-0005。`expected_state` 只在 Force 分支使用；preflight 后相关文件变化，Stash 分支仍执行 cherry-pick 并恢复成冲突，旧确认没有失效。
3. **[P2] 手动 stash 写入没有所见状态凭据** — `src-tauri/src/stash.rs:95–113,345–357`。违反 ADR-0005 的“所有写操作接口带指纹”。保存、应用请求均无文件/暂存状态凭据。实测保存将界面快照之后外部改写的新内容直接收起；执行时重读不能替代与所见状态比较。
4. **[P2] 操作确认只绑定元数据** — `src-tauri/src/operation_flow.rs:30–34,101–108`。违反 ADR-0005 与 `docs/ui-direction.md` Q89 旧确认失效。冲突文件在打开中止确认后改写，元数据版本不变，旧确认仍成功中止并覆盖新内容。先回收不等于确认仍有效。
5. **[P1] 自动 stash 删除的不确定性被吞掉** — `src-tauri/src/autostash.rs:355–359`。违反 ADR-0014 及 Q91/Q96：`Err(_) => false` 吞掉生产事务 NeedsVerification，删除关联并返回成功形状；`App.tsx:4701,4719` 因而以 `false` 收尾，等待退出不能据此停下。应保留结构化清理错误。
6. **[P1] 归档取回中断被报成普通失败** — `src-tauri/src/stash_archive.rs:323–324`。违反 Q91/Q96。真实 `stash store` 在引用已写入后被信号终止，条目已出现，接口却返回 Git / exit=-1；关闭保护不会登记待核对，用户还可能重复取回。写入后的异常退出须升级并核对。
7. **[P2] 结构化原因仍被拼成中文参数** — `src-tauri/src/commit_snapshot.rs:96–103`。违反 ADR-0013 禁止把原中文错误塞进参数。真实生产错误得到 `params.error="目标不是普通文件，不能丢弃"`、`cause=null`；生产英文格式化结果仍夹中文。同型位置见证据附录，应保留结构化 cause / related。

**判断项 [P2] possible Duplicated Code**：`autostash.rs:400–413`、`operation_flow.rs:58–72`、`stash.rs:309–334` 重复“status → paths.sort/dedup → capture_files/context_fingerprint → save_protected”；建议共享完整保护流程，避免各入口修补不一致。这是 smell 判断，不是硬性违反。

[验证原件与范围说明](standards-evidence.md)。未执行 GUI、Release 或完整套件。


# Spec 独立审查（review04）

固定源码 `37425c8ec44cad65d4587de8685caacc24f3f2de`；完整比较 `26b68d1...37425c8`，覆盖 C/S/O/H/F/T/L/D/V。结论：未通过，P1×3、P2×3；未发现范围扩张。以下均为未满足或行为错误，已解决项未重列。

1. **[P1] rebase 中止漏回收 ignored 文件。** `operation_flow.rs:58–70`。O01（spec:88）：“再回收可能被覆盖文件”。onto 删除并忽略文件，冲突停点写入独有内容后，中止成功覆盖为 orig-head 内容，回收仅含另一个冲突文件。须按实际目标树补齐保护路径。

2. **[P1] stash 新分支应用漏回收。** `stash.rs:442–469`。S02（spec:70）：“应用前把可能受影响的现有工作文件状态存入回收区”。旧 stash 基点重新引入当前已忽略文件，switch 覆盖独有内容，应用成功却 `recovery=[]`。须保护树切换实际路径。

3. **[P1] 旧 reset 确认可移动另一分支。** `rewrite.rs:220–230,817`。C01（spec:47）：“执行前绑定并重验 worktree 身份、操作元数据版本、相关引用 / 完整 OID、文件状态和确认条件”。main 确认后终端切到同 OID 的 other，旧指纹仍通过并重置 other。须冻结并核验 symbolic HEAD，不能仅比 OID。

4. **[P2] 自动 stash 清理吞掉待核对回执。** `autostash.rs:355–359`。C02（spec:56）：“保留归档和状态，标记‘结果待核对’”。删除事务实际返回 NeedsVerification，却被转换为 `removed=false` 并删除关联；前端无法识别，等待退出可放行。须保留结构化清理错误与状态。

5. **[P2] stash 迟到完成污染当前目标。** `App.tsx:5357–5359`、`StashPanel.tsx:145–150`。C01（spec:49）：“结果与通知归属原仓库 / worktree”。A 应用途中切 B，A 冲突回执打开 B 的冲突框；旧闭包还可重载 A 列表盖过 B。须逐个收尾回调校验 origin/activation。

6. **[P2] 外部 stash 变更未接刷新。** `App.tsx:5343`、`StashPanel.tsx:119–122`。S01（spec:63）：“其他 worktree / 终端变更后重读”。面板只依赖内部 stashRevision，watch/refs 刷新不触发读取；终端增删后持续显示旧列表。须接入共享引用变化。

四项真实 Git 生产复现、原始命令/退出码、静态调用链与审查边界见 [证据](./spec-evidence.md)。未运行全套、Release 或 GUI；仅清理本次自建的单独临时测试文件，保留全部 fixture 和失败原件。

### Codex 对 review04 的修复（25883b5，待新审查及桌面验收）

Standards 1：历史请求新增 expectedContext，冻结目录身份、symbolic HEAD 与操作元数据；HEAD OID 仍单独核对。Soft/Mixed/Hard/Keep、cherry-pick/revert、reword/interactive 均校验，自动保存和回收后再核对。2：Stash / Force / 无需选择三种路径都核对确认 fingerprint；内容、完整暂存区、HEAD、分支、转换上下文和目标路径变化均拒绝旧确认。3：手动保存/应用新增只读 stash_preflight IPC 与 expectedState，分别绑定面板读取、确认框打开时的完整状态。4：OperationState 新增 confirmationState，保护前后再次核对，编辑后旧中止确认不可执行。5：AutostashRestoreOutcome 保留 cleanupError；清理失败保留 Restored 关联，重新恢复显式 NeedsVerification，明确“保留 stash”才结束关联。6：受控 run_write 不自动重试写命令，超时/信号退出统一 NeedsVerification；真实 archive store 信号测试确认引用已写后仍返回待核对。7：快照、提交、写后重读和回收二次失败保留 cause/related；remote 超时不再塞入中文 what。locale 仅经 x18n get/apply，原件 repair04-i18n-4ktjulz1。

Standards 判断项：抽出 write_guard 统一路径集合、状态指纹和 save_protected 流程，替代各入口独立收集和保护。目录/符号链接无法完整保护时仍拒绝。

Spec 1/2：统一将 HEAD→真实目标树路径并入回收，覆盖 rebase orig-head、sequencer head、后续 todo、stash 基点/index/未跟踪树。新分支切换前及应用前各自保护；两个 ignored 唯一字节的真实回归均核对记录内容。3/4 分别按分支上下文与持久清理回执处理。5：StashPanel 按 worktree+activation 实例隔离，卸载后旧 load 不再启动或接纳；App 迟到冲突仅能打开原激活目标。6：refreshKey 纳入 watchRevision/refsRevision。根检查另修 performM6Write 消费 refreshTarget 的 false；refreshM6State 对切走的原目标仍真实读取，错误传播到待核对门禁，正常丢弃旧 UI 响应不算读取失败。

原件目录 codex-checks-yf3rkhgl。repair04-first 是新增测试误用 params 类型的编译失败；repair04-second 的两项旧预期与新回收/持久回执契约不符；repair04-full-rust 另因同步断言时误改只读外部 diff 测试而失败，已恢复原断言。repair04-m6-remainder 与 -02 为验证命令的目标名/参数错误，均保留原件，不计作通过。full-rust 已通过批次加 m6-remainder-03 共 620 passed / 0 未解决失败 / 1 既有 ignored；最后保护路径收口再重跑 final-m6，61/61。Node 437/437，frontend-02 含 tsc 通过，lint-02 只有既有警告，x18n 0 errors/63 unused warnings（未删键），extract 57 候选/0 中文。日志含 argv/exit/源码身份；生成绑定差异先保存，仅恢复证明无语义变化的尾空白。尚无本候选独立审查、固定 Release 或桌面通过结论。


### review05 独立审查（固定 ce06978；待 Codex 修复）

# Standards

固定范围 `26b68d1fb0315cb85376932a8440204c118dfb78...ce0697882e6a9e9f029f3c32c89696785a60a557`。未通过：两项文档标准违反，均通过真实 Git / Rust 生产入口复现。

1. **[P1] stash 删除前未确保归档位于用户仓库之外。** `src-tauri/src/stash_archive.rs:98` 直接创建归档目录，没有普通回收记录已有的 `ensure_outside` 等价校验。违反 ADR-0012“完整回收内容保存在用户仓库之外”。用户添加包含正常应用数据目录的祖先仓库（如版本化 HOME）即可触发；实测生产 `stash::drop` 返回 `deleted=true`、原列表为空，唯一归档却在该 worktree 内，能被 Git 清理一并移除。归档前应对 worktree/gitdir/common-dir 做真实路径包含校验，涵盖软链接；失败保留原 stash。

2. **[P1] 外部工具信号退出遗漏待核对回执。** `src-tauri/src/external_tools.rs:447–449` 仅检查 `timed_out`。实测 difftool 已改写文件后，其真实 Git 父进程收到 SIGTERM；日志为 `exit_code=-1, timed_out=false`，接口仍返回成功形状的 Outcome。`resultNeedsVerification` 不识别该退出码，`performM6Write` 会以 `false` 结束关闭保护，违反技术方向 Q91/Q96 的中断留证、待核对不静默退出规则。应和受控 `run_write` 一致处理异常退出，保留原诊断、回收记录并核对。

**判断项 [P2] possible Duplicated Code**：`external_tools.rs:274–284` 的 `capture identity → capture_files → context_fingerprint → save_protected` 与新增 `write_guard::protect` 重复；建议复用同一保护流程，避免身份/安全校验继续分叉。此项是维护性判断，不是硬性标准违反。

[证据、范围与未证实候选](standards-evidence.md)。未改产品或已有测试，未执行 GUI、Release 或完整套件。

# Spec 独立审查（review05）

固定 `ce0697882e6a9e9f029f3c32c89696785a60a557`；完整范围 `26b68d1...ce06978`。结论：未通过，P1×2、P2×2；未发现范围扩张。

1. **[P1] “先 stash 再恢复”仍会覆盖 ignored 内容而不恢复。** `rewrite.rs:477–480`、`autostash.rs:179–182`。O02（spec:93）要求明确按用户选择保存并恢复。目标提交新增 ignored 路径，本地已有独有 ignored 字节；preflight 要求选择，用户选 Stash，save_before 却因普通 status 为空返回 None，cherry-pick 成功并覆盖该内容，回执 autostash=None、列表为空。已有回收记录，不能称不可恢复丢失；但不能把 Stash 选择静默执行成仅回收后的覆盖。应真实保留/恢复，或在写前拒绝不支持的组合。

2. **[P1] 外部工具被信号终止仍返回普通成功形状。** `external_tools.rs:440–461`。C02（spec:56）要求中断结果标“待核对”，T（148）要求复用关闭保护。真实 difftool 已改文件，再终止本次 Git 进程；生产接口返回 Ok(exitCode=-1)，未带 NeedsVerification。App 的 resultNeedsVerification 不检查 exitCode，写生命周期按 false 结束。须让不确定写回执进入待核对门禁。

3. **[P2] 多提交 cherry-pick 的“继续”遗失剩余选择。** `rewrite.rs:614–624,659–662`、`operation_flow.rs:165–175`。H01（spec:111）要求明确顺序与部分成功后待处理项。选择 A/B/C，A 完成、B 冲突，C 只存在返回值 pending；解决 B 后 Continue 报 completed=true，但 C 未执行，未保存原选择序列。应持久绑定剩余计划并显式续接，避免把原多提交任务当完成；revert 同型。

4. **[P2] blame 静默损坏不可无损呈现内容。** `file_history.rs:318–338`。F02（spec:139、142）要求保留原内容并明确编码/二进制边界。真实提交包含 0xff，blame 返回 Ok，内容变为 `bad byte: �`，没有限制或截断说明。须验证编码/类型后呈现或明确拒绝。

四项均经真实 Git / Rust 生产入口复现，详见 [证据](./spec-evidence.md)。本轮未运行全套、Release 或 GUI；桌面 / Rebased、语言切换和窄窗仍须规划方独立验收。


### Codex 对 review05 的修复（待新两轴与桌面验收）

Standards 1：stash 归档创建前后共用真实路径隔离校验，检查实际 stash-archive 目录及所有 linked worktree 根、gitDir/commonDir；已有目录软链接及尚未创建的子路径均解析现有祖先。内嵌当前仓库、另一 worktree 主目录或仅 stash-archive 自身软链接指回仓库，均真实拒绝删除、保留原 stash。2：外部工具信号退出 -1 与 timeout 同样是 NeedsVerification，保留退出码、Git stderr 和回收记录；真实自建 difftool 改文件后只终止经核实的本次 Git 祖先进程，生产入口确实返回待核对。判断项：外部工具保护改用 write_guard::protect。

Spec 1：Stash 策略在写前拒绝普通 stash -u 不会收起的 ignored 阻塞路径，文案明确可选择先回收再强制覆盖或取消，不擅自加入 --all；Force 的完整回收仍可执行。2 同 Standards 2。3：history_sequence 持久绑定原身份/分支/起始 HEAD、完整剩余 OID、来源后缀与空提交选项、已完成/跳过列表及自动 stash 关联。写前落 Running，实测回执后推进 Ready，冲突保存 Stopped 元数据。Continue/Skip 经原目标确认后续接剩余选择，revert 同型；原多提交序列未结束前不恢复自动 stash。Abort 回到整个选择的原 HEAD，覆盖前保护同时包含已应用提交到原起点的路径。Running 重启不会重放，界面可明确“保留当前结果并结束计划”，只清 Revia 关联、不改 Git/文件、不恢复 stash；真实 Git 的中断和原操作停点保留。OperationOutcome.continuation 保留整体分段结果，通知与退出门禁识别其 error。4：blame 对非法 UTF-8/NUL 明确拒绝，原字节保持；目标 NUL 路径与 readonly 非 UTF-8 状态也不再有损转换后用于保护。

另处理 Standards 附录的部分自动恢复：Git 非零退出、无 unmerged 但可能已恢复部分内容时，保留 Applying 关联及结构化 NeedsVerification，而非仅 restored=false。新真实回归同时确认碰撞的未跟踪字节不变。

检查原件 codex-checks-yf3rkhgl：repair05-first 的唯一失败是旧超时测试把新增确认读取/并行锁等待计入命令期限；改为断言真实 timed_out Git 日志（包含输出排空）仍小于2秒，未放宽40ms命令期限。repair05-full-rust 完整628 passed/0 failed/1既有ignored（M6 69/69），Node437/437、frontend-02含tsc、lint、extract57候选/0中文通过；x18n首次新增1术语警告，已通过CLI改为 stash entries，x18n-02为0errors/63既有unused警告，未删键。资源写入原件 repair05-i18n-9jaxpp_1。

随后强化整体 Abort 回收基线发现新回归：已完成A从剩余队列移除后，保护路径漏了回到原起点会改动的A；final-operation 与 final-sequence-baseline 失败原件保留，已把 original_head 始终纳入保护路径，且回收完成基线延后到全部续接/整体中止/自动恢复收尾之后。sequence-baseline-02真实通过，核对每个受保护文件的最终字节/存在状态。最终M6重跑的结果见本次后续记录。尚未做新独立审查、固定Release或真实Revia桌面验收。

最终收口：repair05-final-m6 69/69通过，包含 original_head 的覆盖保护和整段完成后的回收基线；新增真实中断/持久队列/归档隔离/二进制/部分自动恢复回归均通过。固定本候选进入全新 review06，不复用旧代理；固定 Release 与真实桌面仍未验收。


### review06 独立审查（固定77eb68e；待Codex修复）

固定范围：`26b68d1...77eb68e`，154 个变更文件；Standards 发现 **2 项确定违反（P1 × 1，P2 × 1）**。

1. **[P1] 整体 Abort 的第二次 reset 未绑定 native abort 后的 HEAD** — `src-tauri/src/history_sequence.rs:173–176`。多选 cherry-pick 已完成 A、B 冲突时中止；真实 `reference-transaction` hook 在 native `--abort` 后把 main 移到新提交，代码仅核对分支身份，随即 `reset --merge original_head`，移走新 HEAD 并返回 `completed=true`。独立生产入口复现已确认。违反 `docs/adr/0005-write-ops-carry-diff-fingerprint.md:3` 的写前重新核对，以及 `docs/technical-direction.md:54`“内部串行不能代替外部变更校验”。第二次写入应验证预期 HEAD 与保护状态；变化时保留计划并要求核对。

2. **[P2] 恢复尝试仍持久化并展示整句中文错误** — `src-tauri/src/recovery.rs:1292–1305`；`src/components/app/RecoveryPanel.tsx:338`。生产恢复前检查失败后，`error.to_string()` 丢掉结构化语义，`RestoreAttempt.message` 保存完整中文，面板原样显示，切换 English 也无法翻译；多处恢复失败分支同样处理。真实 discard→restore→inspect 复现已确认。违反 `docs/adr/0013-localize-structured-user-errors-at-presentation.md:3,7` 与 `docs/technical-direction.md:67`；应持久化结构化错误，在当前语言下呈现，并明确兼容旧记录。

Fowler smell 均仅作判断项；本轮无另外达到报告置信度的 smell。原始证据见 `standards-evidence.md`。未运行全套、Release 或 GUI，不把父代理既有检查作为独立通过证据。

# Spec 独立审查 06

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...77eb68ea252330da567dad400100a62f471f0309`。本轮覆盖 C/S/O/H/F/T/L/D/V；发现 **2 项 P2，均为行为实现错误**。

1. **[SP06-01 / P2] 非空冲突提交的手动跳过被回执成空提交跳过。** 规格 `spec.md:111` 要求“列出完成、跳过、待处理项”，`:116` 要求“结果逐条列出空提交跳过项”。对 A/B/C 批量 cherry-pick，B 有真实文件变更且冲突；用户明确 Skip 后，`history_sequence.rs:181` 将 B 放进共用 `skipped`，`:229–230` 又作为 `skipped_empty` 返回。`App.tsx:4738` 用中英文“空提交已跳过”提示 B。真实生产入口复现 B 的 `diff-tree` 非空，而最终 `skippedEmpty=[B]`。会把用户主动舍弃的非空提交误说成无需应用的空提交；应分别保留跳过原因并据实呈现。

2. **[SP06-02 / P2] 所有分支历史改变了工作副本 blame 的上一版本。** 规格 `spec.md:140`：“上一版本绑定当前行 / 版本的真实父关系”。先勾所有分支，再打开工作副本 blame 并点上一版本，`FileHistoryDialog.tsx:144–150` 从全分支 `entries[0].oid` 取版本。若另一分支最近修改同文件，当前 main 的 blame 会跳到该无关分支。真实 Git 生产入口按此调用链复现：工作副本 `main branch data`，上一版本却是 `other branch data`。工作副本基准应固定当前 HEAD 的该文件历史，不能复用可切换的浏览列表。

未另发现可确定的范围扩张或整项缺失。证据与逐域覆盖见 `spec-evidence.md`。本轮最小真实 Git 检查为 0 通过 / 2 业务断言失败（exit 101）；未跑全套、Release 或 GUI，不代表桌面验收通过。

### Codex 对 review06 的修复（待全新两轴与桌面验收）

Standards 1：持久历史计划沿用原确认的 HEAD/context，首次写入不重新认领现场 HEAD。整体 Abort 的 native Git 收尾后，第二次 reset 要求 HEAD 仍等于已知停点、原分支/context 不变；再次保护实际文件/index结果并核对指纹，变化时保留 Running 计划并返回待核对。真实 reference-transaction hook 移动 HEAD 的回归确认 foreign HEAD 被保留。2：RestoreAttempt 新增可兼容旧数据的结构化 error，所有失败出口持久化 code/params/cause；面板按当前语言呈现。旧 message 保留诊断原件，旧回执显示双语说明，不原样显示中文产品句子。真实 discard→恢复前检查失败→重开读取载荷，再交生产 formatter 中英切换，均已验证。

Spec 1：RewriteOutcome/持久计划将用户明确跳过 skipped 与空提交 skippedEmpty 分开，Continue/Skip 完成通知分别据实呈现。2：BlameResult.previous 由后端在固定提交/工作副本 HEAD 下计算，前端不再使用全分支浏览列表 entries[0]。真实 main/other 同路径回归确认上一版仍是当前分支版本，原工作文件字节不变。显式版本先固定完整 OID，解析失败仍保留 blame 专用错误和原 stderr。

检查原件 codex-checks-yf3rkhgl：repair06-full-rust 首轮在 M6 70通过/2失败，分别是显式 blame 预解析误改错误码、fixture 同秒提交导致全分支排序不确定；原件保留，修复产品错误码并给 fixture 明确日期后，repair06-m6-remainder 全部通过。原全套已通过批次与重跑/剩余批次合计631 Rust passed/0未解决失败/1既有ignored（M6 72/72），Node438/438，frontend含tsc、lint、x18n 0errors/63既有unused警告、extract57候选/0中文通过。新增 locale 只经 x18n CLI get/apply，原件 repair06-i18n-jer7w_c1；生成绑定差异先保存，仅恢复逐行去尾等同HEAD的格式变化，保留三项语义绑定。整体Abort补上二次保护后的受影响复核见repair06-final-abort与repair06-final-sequence。

此候选尚未独立复核、构建固定Release或完成真实Revia桌面验收；既有Rebased观察不代替Revia验收。不续接Pi，不推送、不开PR、不合main或删worktree。

### review07 独立两轴（固定073147d；Codex接管修复）

#### Standards

固定审查 `26b68d1...073147d` 的完整 diff。硬违反 6 项；判断项 1 项。

- **STD07-01 [P1] 自动 stash 恢复可串到新 worktree**：`src/App.tsx:4769–4773` 取当前目标，`4623–4629` 切换时仍保留旧关联；B 的读取未完成或失败时，显示 A 的恢复按钮会调用 B 的恢复。请求没有显示关联的 ID/版本。违反 `docs/README.md:116`（Q90，目标归属）与 ADR-0005 的所见版本约束。
- **STD07-02 [P2] 占位符迁移丢失文件清单**：`locales/zh-CN.json:1245,1563`、`locales/en.json:1245,1582` 使用 `{?}`；生产格式器不替换它，hooks 修改路径和远端冲突路径均消失。违反 `i18n-proposal.md:12–14` 的参数化与原函数语义要求；运行已证实，专项 x18n 检查仍通过。
- **STD07-03 [P2] 日志错误绕过当前语言**：`src/features/log/logPager.ts:177–184` 保存原 `message`，`LogView.tsx:886` 直接显示。真实 Git 正则失败在英文模式仍显示“Git 命令失败”。违反 ADR-0013:3,9；Git 原始 stderr 不计入问题。
- **STD07-04 [P2] 环境失败仍传拼好的中文**：`src-tauri/src/session.rs:363–367` 返回中文 `detail/hint`，`src/App.tsx:773` 拼成无 `code` 的错误；shell 回退时英文界面出现“毫秒”。违反 ADR-0013:3,7。
- **STD07-05 [P2] 新 diff 绕过受控渲染**：`StashPanel.tsx:297–299`、`FileHistoryDialog.tsx:262–264` 以原生 `overflow-auto` 的 `<pre>` 一次渲染完整 patch（8/2 MiB）。违反 ADR-0008:8、`technical-direction.md:85` 的受控滚动与可见区域渲染约束；未作桌面性能结论。
- **STD07-06 [P3] 界面术语不一致**：`locales/zh-CN.json:1743–1772,1819,1827–1828` 把暂存区称“索引”，`985–1015` 等用户错误保留 hunk。违反 `GLOSSARY.md:23–25,51–53`；日志索引不在此列。

判断项 **[P3] possible Duplicated Code**：`rewrite.rs:589–621,641–668` 重复“`ensure_no_operation` → 预检/回收 → `history_sequence::create/run`”编排，后续安全修补须同步两处；可提取共同序列入口。属于 heuristic，非硬违反。

覆盖所有变更类别与安全/语言调用链；未发现 CODING_STANDARDS.md、CONTRIBUTING.md。未操作 GUI、未跑全套或 Release；仅最小真实 Git/生产前端探针。详细证据见 `standards-evidence.md`。

#### Spec

# Spec

固定范围：`26b68d1...073147d`，按 `scope.json` 的完整 three-dot diff 重新审查。未通过，6 类问题（P1×1、P2×5）：

1. **[P1] 浅克隆 stash 归档仍依赖原库。** S03（`spec.md:77–80`）：“依赖外部原对象的增量归档均不满足独立保存”。`src-tauri/src/stash_archive.rs:183–185,167–175` 遍历尊重原 shallow 边界，却不把边界写入归档。生产删除成功后，原历史退出所有引用并 GC，生产恢复 fetch 失败（退出128，缺父对象）。须保存可独立导入的边界，或在删除前拒绝不完整归档。

2. **[P2] 刷新/重载后旧确认仍可写入。** C01（`spec.md:47`）：“刷新…环境重载后旧确认失效”。`write_guard.rs:52–79` 指纹没有快照版本/会话实例；`App.tsx:4204–4210` 重载也未更换 activation。生产 preflight 后刷新、重开会话，旧 token 的 stash apply 仍成功。须把确认代次贯穿前后端。

3. **[P2] 本地比较遗漏 stash 的未跟踪文件。** S01（`spec.md:63–64`）：“能读取单条文件列表…比较对象提供本地文件或父提交”。`stash.rs:220–221,261–269` 只比较 stash 主树，遗漏 `^3`。真实 `stash -u` 默认显示 f/u，切本地比较仅剩 f，u 的文件与 diff 均消失。须纳入未跟踪树。

4. **[P2] 冲突解决后的空提交策略失效。** H02（`spec.md:116`）：“本次可以按策略处理空提交”。`history_sequence.rs:224–227` 仅初次执行处理策略；`operation_flow.rs:141–143,191` Continue 不处理。确认“创建空提交”，冲突解决成 HEAD 后点击 Continue 仍报 empty，界面也禁止普通提交。须在明确继续时沿用持久策略并准确记录结果。

5. **[P2] 分组/既有通知不会即时切语言。** L01（`spec.md:155–156`）：“覆盖已有及新增产品界面…设置即时切换”。`ChangesTree.tsx:39–44,141–155` 模块加载时冻结中文分组；`remoteWrite.ts:141–193` 将通知预先格式化，`App.tsx:4505,4533` 原样保存。英文分组仍是“已暂存”，已有更新冲突通知切换后仍中文。须在呈现时取当前语言。

6. **[P2] 日志分页错误绕过翻译链路。** L03（`spec.md:170`）：“前端唯一生产格式化链路按当前语言呈现”。`logPager.ts:177–184,218–223` 直接缓存 Rust `message`，`LogView.tsx:886` 原样显示。真实非法正则载荷经生产 LogPager，英文仍显示“Git 命令失败”，切语言也不变。须保存结构化载荷，呈现时格式化。

未发现已证实的范围扩张；Q173 例外已纳入。C/S/O/H/F/T/L/D/V 覆盖与原始证据见 `spec-evidence.md`。未操作 GUI、未跑全套/Release；未完成桌面验收没有算作缺陷。

### Codex 对 review07 的修复（待新两轴及桌面验收）

Standards 01：M6 操作/自动 stash 读数连 origin/activation 保存，切目标立即清空，旧读取按序号丢弃；恢复与保留动作冻结原目标。明确恢复携带界面显示的 link 与确认 state，后端持锁核对完整关联和文件状态。02：恢复 worktree/index/files 命名占位符，数组按既有连接、空列表和最多8项省略语义格式化；新增逐个函数模板的参数契约检查。03：LogPager、history rows、行窗口和高亮缓存保留结构载荷，界面按当前语言格式化；无类型的旧拒绝字符串保守使凭据失效，不以中英文句子判断业务。04：EnvironmentCheck detail/hint 改 code/params/cause，App、设置错误与回执在呈现时格式化。05：两个 patch 预览以只读 PatchRows 接入既有 DiffView 的受控滚动和可见行渲染，不签发工作区写凭据、不计算文件差异；Git patch 仍受既有读取上限约束。06：按既有术语表将中文产品文案的索引/hunk改为暂存区/块，英文用Staging Area/Change Block，保留原字段/键、index.lock及日志索引。所有资源经x18n CLI，原件 repair07-i18n-fdicgah5，未删任何未用键。

Standards 判断项 possible Duplicated Code 不作为硬违反处理：cherry-pick/revert 保留各自请求边界；共同确认/回收已在 write_guard、持久续接在 history_sequence。此轮不再为该判断扩大编排重构，独立审查可继续评价这一取舍。

Spec 01：归档按原始对象（不受 replace refs 影响）收集并保存可达 shallow 边界，删除前在独立 bare 库做 connectivity 检查；恢复经 Git --update-shallow 导入。真实浅克隆换全新孤立历史、移除全部旧 refs、GC 清理后，仍从独立归档恢复原 stash 字节，工作文件不变。实现依据已核对的 [Git shallow 说明](https://git-scm.com/docs/shallow) 与 [git-fetch 的 update-shallow](https://git-scm.com/docs/git-fetch)。02：公开刷新递增确认代次，确认绑定 session 实例与该代次，重开环境后旧 token 不可写；Soft reset、stash save/apply、operation 和外部工具均纳入，stash 删除/清空/归档恢复 IPC 也验证所见 token。内部写前重读不冒充新的用户刷新。03：stash 本地比较用独占临时 index 读取第三父树，主树排除重复路径；真实中文空格未跟踪文件在缺失、存在、已暂存三种情况均参与 Git diff，原 index/工作文件不变。tempfile 从既有 dev 依赖移到运行依赖，锁定版本不变。04：明确 Continue 后使用持久 keepEmpty 策略处理已解决为空的提交，分别创建空提交或记录空提交跳过；不自动跨过未解决冲突。05：改动树分组和已有远端通知惰性取当前语言，UI日期按所选locale，设置/错误保留载荷而非格式化句子。06 同 Standards03。

检查原件 codex-checks-yf3rkhgl：repair07-full-rust 首次编译因闭包括号和仅dev可见的tempfile失败，已修并保留；full-rust-02在M6 72通过/5失败，四项旧请求未从新确认入口取token/Soft不带确认，另一项fixture遗留dangling origin/HEAD。已同步生产契约、清理仅属于该fixture的远端HEAD，并在恢复前后均fsck核对。full-rust-02已通过批次与m6-remainder合计638 Rust passed/0未解决失败/1既有ignored（M6 77/77）。Node首轮441通过/2失败：一个旧fixture参数名不符真实Rust，另一个是移除旧字符串识别后的保守门禁回归；已修正参数和无类型拒绝的门禁，受影响103/103通过，分批共443通过。最终真实错误/函数参数/通知/patch适配11/11再次通过。frontend-03含tsc、lint-02、x18n-02通过；x18n为0 errors/62 unused warnings（新增at字面量读取识别后少1未用警告，未删除键），extract57候选/0中文。绑定生成差异先留原件，仅恢复逐行去尾等同HEAD的格式变化，语义EnvironmentCheck与两个新增绑定保留。

这些是修复与自动化结果，不是独立审查或桌面通过。下一步固定新候选，按 code-review skill 全新两轴；固定Release与真实Revia/Rebased/中英文窄窗验收仍待完成。不续Pi、不推送、不PR、不合main、不删worktree。


### 全新独立 review08：固定26b68d1...9f2cb8c（未通过）

完整原始报告与失败现场：`/private/tmp/revia-m6-batch-ig_nawgt/review-08-guelamq5`。两代理均完成并结束；没有把观察探针退出0当作产品正确性。以下按两轴保留原文，不跨轴合并评级。

#### Standards

固定范围：`26b68d1...9f2cb8c`，161 个文件；全新 Standards 审查。

硬性违反：

1. **[P1] rebase“继续”覆盖文件前没有回收。** `src-tauri/src/operation_flow.rs:127–145` 的保护分支只包含 `OperationAction::Abort | OperationAction::Skip`。真实 Git 的 rebase 在 edit 停点后还有提交添加 `victim.txt`；停点新建被忽略的该文件后 Continue，内容从 `USER IGNORED CONTENT AT STOP` 变成后续提交版本，成功回执 `recovery_id: None`，回收列表为空。违反 `AGENTS.md:71`、`docs/product-direction.md:257`“其他会覆盖工作区内容的操作之前，先存入回收区”。应在继续可能触及的路径上先完成保护。

2. **[P2] hook 失败详情仍把中文句子藏进 IPC 参数。** `src-tauri/src/commit_flow.rs:788–814` 在 hook 改过文件且提交失败时拼接 `"hooks 修改过的文件（未自动撤销）：工作区 {}；暂存区 {}"`，放进 `params.detail`。`src/lib/error.ts:12–15` 原样插值，`src/hooks/useNotifications.ts:14` 会呈现它，所以英文通知仍出现中文。违反 `ADR-0013:7` 和 `docs/technical-direction.md:68`。应只传路径列表等数据，在前端生成说明。

判断项（不是硬缺陷）：

3. **[P3] 可能 Duplicated Code。** `src-tauri/src/batch.rs:55–80` 和 `src-tauri/src/git/status.rs:54–79` 新增同样六项 `ReadonlyReason::Symlink => (..., "符号链接为只读类型...")` 映射，只换错误键前缀；两处维护易漏改。可让批量跳过复用 `reason.error().payload()`。

仅运行本人两个真实 Git 观察入口；Continue 已复现，hook 文案项为静态生产链证据。未跑全套、Release 或桌面验收。覆盖、原始输出、复现源码和附加观察见 `standards-evidence.md`。


#### Spec

# Spec

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...9f2cb8c51866d0295e3c358cbcab100bece30deb`。独立覆盖 C/S/O/H/F/T/L/D/V，发现 **3 项行为实现错误（P1×1、P2×2）**：

1. **[SP08-01 / P1] 外部合并覆盖已有 `.orig` 文件却没有回收。** 规格 `spec.md:148`：“外部工具可能写文件，启动与收尾遵守…回收”；`:53` 要求“完整回收…再执行有覆盖风险的写入”。`external_tools.rs:278–283,386–391` 只保护所选路径。真实生产 `launch` 调用 Git mergetool 后，已有 `conflict.txt.orig` 的用户字节被 Git 的默认备份覆盖；回收记录只有 `conflict.txt`，且回执为 exit 0 / 无冲突。应把 Git 已知会覆盖的伴随路径纳入保护，或安全拒绝已有目标。

2. **[SP08-02 / P2] 自动 stash 丢失会隐藏真实 Git 操作入口。** 规格 `spec.md:86`：“对 merge、rebase、cherry-pick、revert 按真实 Git 操作状态提供适用按钮”。`autostash.rs:307–311` 在读取 Pending 关联时要求 stash 仍在列表；终端删除后返回错误。`App.tsx:4628–4633,4641` 用 `Promise.all` 共同接纳操作与关联；任一失败使有效操作状态也不写入。重开应用时即无继续 / 中止入口。真实 fixture 中生产操作读取为 merge / 1 冲突，但 pending 读取失败；应独立呈现真实操作与关联错误。

3. **[SP08-03 / P2] stash 本地比较刷新后仍显示旧差异。** 规格 `spec.md:63–64`：“其他 worktree / 终端变更后重读”“比较对象提供本地文件”。`StashPanel.tsx:142–166,188` 刷新只重读列表；选中 OID 不变时 inspection effect 不执行。终端改文件后点 stash 刷新或收到 watcher 刷新，patch 仍是旧值。直接执行生产组件的隔离接线探针确认 inspect 调用数 2→2；应刷新所选比较结果。

未发现另外可确定的整项缺失或范围扩张；Q173 窄例外已计入。原始证据与逐域覆盖见 [spec-evidence.md](./spec-evidence.md)。真实 Git 最小检查 0 通过 / 2 业务断言失败（exit 101）；组件接线检查 exit 1。未运行全套、Release 或 GUI；未完成桌面验收未计作产品缺陷。


规划方核对：Continue与.orig的字节变化/回收列表原件以及hook文案生产链均支持发现；stash本地比较证据为组件接线，不冒称桌面实测。按Q113由Codex直接整包修复，不再续Pi/正式返修；修复前方案见外部impl/repair08-plan.md。Standards附录手动stash回执认领外部条目也纳入修复；P3映射重复按判断项处理。尚无固定Release/真实Revia桌面通过。


### Codex 对 review08 的修复（待新两轴及桌面验收）

Standards01：Continue与Abort/Skip共用操作全部后续目标及持久计划路径的确认/回收，新OperationContinue来源。真实rebase edit停点后新建ignored文件，被后续提交覆盖前已留下原字节；回收失败仍拒绝写。02：commit失败的hook说明从中文detail参数改结构化related载荷，参数只有原始worktree/index路径；真实hook退出7的生产载荷经前端formatter切中英均可读。P3判断项已以reason.error().payload()复用只读原因，旧未用资源未删除。

Spec01：外部merge保护目标及Git可能写的.orig（含原不存在状态），保护前后核对完整指纹；进程级mergetool.writeToTemp=true让LOCAL/BASE/REMOTE/BACKUP由Git在独占目录生成，不改用户配置。真实merge成功、已有.orig被Git改写但原字节在回收记录；存储被本fixture阻断时目标/备份字节都不变。02：Pending关联读取保留link、可空确认凭据和结构化error；无法确认时禁止恢复但关联可见。前端readM6State独立接受operation和pending的成功/错误，错误单独呈现，迟到/非当前origin仍不能污染当前。真实Git删stash后merge状态与关联仍可读，生产Banner SSR与formatter核对了可见错误及恢复禁用；这不是桌面通过。03：刷新（含面板按钮）递增inspection读取代次，当前选中的本地比较重新读取，旧异步结果被丢弃；生产组件接线回归覆盖终端刷新和按钮刷新。

附录stash保存回执：先用Git stash create固定本次worktree/base/index树身份，重新核对确认再push；后读列表只接受符合内容身份的唯一新条目，其他情况返回NeedsVerification并保留所有条目。真实Git生产阶段接缝插入外部push后仍返回自身条目；同内容不同OID歧义时明确待核对。仍不收未跟踪/ignored，所有非删除Git继续系统CLI。

检查原件codex-checks-yf3rkhgl：repair08-full-rust在M6 80通过/1失败，唯一失败是新fixture试图stash store同OID，但Git不新增reflog；修正为真实commit-tree同内容不同OID后repair08-stash-receipt通过。全套其余已过批次+receipt+rust-remainder+doc-tests合计642 Rust passed/0未解决失败/1既有ignored（M6 81/81）。Node首轮受新增Banner引入Vite图标资源的Node加载限制，i18n文件未执行；仅对装饰FileTypeIcon做SSR空呈现（保留生产Banner/Dialogs/formatter），i18n13/13通过；同首轮其他434通过合计447。frontend含tsc、lint、x18n0 errors/62 unused warnings、extract57候选/0中文均通过，未删键。翻译只经x18n CLI apply，原件repair08-i18n-enbmrrig；绑定差异先保存，28项逐行去尾等同HEAD的格式漂移恢复，仅保留AutostashPending/RecoverySource两项语义变化。

仍未固定Release或完成真实Revia桌面验收。下一步固定候选、按code-review skill全新Standards/Spec，不续Pi、不分派实施代理、不推送/PR/合main/删worktree。


### 全新独立 review09：固定26b68d1...c018123（未通过）

原件`/private/tmp/revia-m6-batch-ig_nawgt/review-09-kheied53`。两代理均结束，产品冻结期间无tracked改动。分别保留两轴原文：

#### Standards

# Standards

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...c018123d7165613b661de0ebe344b0612ae20682`，13 个提交、164 个文件。结论：4 项文档规范问题，1 项非阻塞设计判断。

- **STD09-01 [P2] 分支已切换，却仅返回可重试拒绝。** `src-tauri/src/stash.rs:551–558`：作为新分支应用 stash，切换成功后的条目重读失败直接返回 `rejected`。真实 Git probe 已确认新分支成为当前分支、apply 未启动，而载荷只说“刷新后重试”；`src/App.tsx:4721–4733` 因此不保留待核对关闭保护。违反 `docs/technical-direction.md:55–56` 的结果与关闭契约。须保留“已创建/切换分支、未应用”的部分回执；无法确定后果时进入待核对。
- **STD09-02 [P2] reset 漏掉已发布历史警告。** `src/features/m6/HistoryActionDialog.tsx:263–267` 仅对 reword/interactive 显示 `publishedWarning`；reset 即使 `preflight.published` 为 true 也能直接执行。违反 `docs/product-direction.md:257` / Q123。应在相应 reset 确认中显示发布影响。
- **STD09-03 [P2] M6 结果通知不标所属 worktree。** `src/App.tsx:4772`、`4794`、`4904`、`5767–5776`，以及 `StashPanel.tsx:174–179`，异步完成后向全局通知栈提交通用结果，缺少冻结目标的名称/路径。切到 B 后会在 B 窗口看到 A 的无归属结果，违反 `docs/ui-direction.md:67`。应把原 worktree 写进通知内容。
- **STD09-04 [P2] 产品状态/错误仍直接显示固定英文。** `StashPanel.tsx:306` 直接显示 `added/modified/typeChanged` 等内部枚举；`historyRows.ts:83`、`historySideSource.ts:42` 的固定英文错误经生产错误视图原样显示。中文界面不随语言切换，违反 `docs/ui-direction.md:177–179`、`docs/technical-direction.md:32,68`。应集中资源并用完整字面量键映射。

**设计判断（非硬性违反）**：可能的 Speculative Generality：`src-tauri/src/stash.rs:767–797` 新增的三个公开映射 helper，在产品和测试中均无调用；删除这些未使用接口，或把真实调用接上。

01 已实测；02–04 为静态链路结论。未执行 Release、GUI、全套测试，不把待验收项认作产品缺陷。完整覆盖与原始证据见 `standards-evidence.md`。


#### Spec

# Spec

固定完整范围 `26b68d1fb0315cb85376932a8440204c118dfb78...c018123d7165613b661de0ebe344b0612ae20682`；164 文件、13 提交。覆盖 C/S/O/H/F/T/L/D/V，结论：**未通过，P1×1、P2×3**。

1. **[SP09-01 / P1] 批量历史操作把步骤间的新文件当成已确认内容覆盖。** C01（`spec.md:47`）：“执行前绑定并重验…文件状态和确认条件”。`history_sequence.rs:223–226` 每步重新生成 expected，`:88–90` 只保留身份/分支/HEAD。真实 A/B cherry-pick 中，A 提交后独立写线程新建 ignored `victim.txt`，B 覆盖其字节，仍返回 completed=true；原字节有回收，不能称永久丢失。须保留尚未由本次操作写过路径的确认基线，变化时停止，不能以现场重采样续认授权。

2. **[SP09-02 / P2] stash 的未跟踪树牵入无关符号链接，正常应用被拒。** S02（`:70`）：“应用前把可能受影响的现有工作文件状态存入回收区”。`stash.rs:347–352` 将整个 HEAD 与只含未跟踪文件的 `^3` 比较，连未改动的 tracked symlink 也被当成保护路径；`:511–512` 保存时遭只读拒绝。真实 `stash -u` 仅改普通文件，生产 apply 失败，原生 Git 同条应用成功且链接不变。须按真实涉及路径收集，保留符号链接写限制。

3. **[SP09-03 / P2] 日志删除文件的外部 diff 永远报不存在。** T（`:146`）：“外部 diff 与合并为可选设置”。`LogView.tsx:589–598` 提供该入口，`external_tools.rs:263–275,351–354` 却要求提交后文件存在。真实删除提交的原生 difftool 成功，生产 launch 返回 pathNotInRevision。须允许 Git 比较父版本与不存在的新版本。

4. **[SP09-04 / P2，静态] reset 遗漏已发布历史警告。** H01（`:110`）：“已在上游的历史改写额外警告”。`HistoryActionDialog.tsx:263–267` 仅 reword/interactive 显示 publishedWarning，`:325–342` 的 reset 只解释模式/工作文件。已发布 HEAD reset 到旧提交时没有所需的额外警告；应按实际移除的已发布范围提示。

未发现已证实的范围扩张。前三项经真实 Git/Rust 生产入口复现；第4项为静态呈现链证据。最小探针首轮 0通过/3业务断言失败，exit101；两次有新证据目的的单项核对亦 exit101。未跑全套、Release、GUI；待桌面验收不算产品缺陷。详见 `spec-evidence.md`。


规划方核对：批量步骤间覆盖有回收，缺陷是旧确认被重新认领，不能写成永久丢失；stash未跟踪树牵入无关symlink、已删除历史外部diff以及新分支部分回执均有生产真实Git证据；reset发布警告、通知归属与固定英文为静态生产链证据。由Codex直接统一修复，不续Pi/正式第二返修。


### Codex 对 review09 的修复（待新两轴及桌面验收）

Spec01：持久Plan新增ConfirmedFiles，保留全部计划路径字节/权限/存在状态与全暂存区条目。每步在保护前及写前核对基线，成功后只允许该Git提交实际涉及的路径推进，其他路径不重新认领。外部变动时保存Ready计划、明确已完成/剩余/NeedsVerification，不恢复自动stash；仅新的明确Continue及新确认允许接受当前状态。真实reference-transaction停点由独立线程新增ignored文件，批次只完成A且保留foreign字节；用户明确Continue后完成B，foreign原字节在回收区。既有revert/Continue/Skip/Abort及中断计划回归全通过。旧计划无新字段不盲目自动运行。

Spec02：stash路径按base..工作树/base..暂存树及第三父树实际路径收集，第三父树不再当整棵HEAD比较。确认额外绑定可能切换的base树，只有用户选择新分支时才把切换路径加入实际保护；自动restore确认同步。真实含未跟踪条目的stash在无关tracked symlink存在时正常应用，link原样不变，写限制保留。Spec03：历史外部diff按固定parent..revision的真实Git差异验证路径，允许删除一侧不存在；中文空格已删除文件真实difftool成功。Spec04/Standards02：reset预检按HEAD与本地remote refs共同祖先中将移出的范围判断published，不联网；只移除未发布提交不误警告，移出已发布提交显示已有额外警告。

Standards01：stash新分支切换后的错误返回明确branch/recoveryId、applied=false与结构化NeedsVerification error（表示未确认完成，不能自动重apply）；前端消费error优先于成功提示，全局关闭门禁识别。真实post-checkout移除原条目，回执明确分支已切换、内容未应用，英文formatter保留完整说明。Standards03：M6各写结果/自动stash/文件历史/工具通知携带发起时所属worktree路径，通知卡片可查看完整path；晚到工具完成不关闭其他新目标弹框。Standards04：stash状态用完整字面量键映射；历史行/左右源错配错误保留结构化code按当前语言呈现。判断项：删除3个无调用stash状态helper，未扩大不相关重构。

检查原件codex-checks-yf3rkhgl：repair09-full-rust首次完整647 passed/0 failed/1既有ignored（M6 86/86）；Node首轮446通过/3失败，两条旧英文整句断言与新结构化契约不符，另一条因提前消费尚在运行Rust生成的branch证据而ENOENT，原件保留。Rust完成后同步旧断言，node-affected 69/69，分批共449通过。frontend含tsc、lint（既有unused警告）、x18n0errors/62unused警告、extract55候选/0中文均通过；未删键。资源只经x18n CLI，原件repair09-i18n-00zqq2n5。绑定差异先保存，27项逐行去尾等同HEAD的格式变化恢复，只保留StashApplyOutcome新增语义。

还没有固定Release或真实Revia桌面通过。Rebased参照额外证实日志reset四模式、blame三开关当前默认与编排界面，截图/AX和取消后只读Git证据在m6-desktop-_8y2drjl；新more-cases-a6hinndw仅验收fixture准备，不是验收结果。下一步固定候选并全新两轴，产品冻结期间不修改。不续Pi、不第二次正式返修、不另派实施代理、不推送/PR/合main/删worktree。


### 全新独立 review10：固定26b68d1...6cd7125（未通过）

原件`/private/tmp/revia-m6-batch-ig_nawgt/review-10-tn46y5pu`。两代理已结束并清理各自临时入口，HEAD固定、工作树干净。分别保留两轴原文：

#### Standards

# Standards 独立审查

固定范围：`26b68d1...6cd7125`，14 个提交、164 个文件；结论：**2 处已证实文档标准违反，最高 P1**。

- **[P1] 共享 stash 协调没有覆盖智能检出。** 新增 `src-tauri/src/stash.rs:419–423` 取得的锁未被 `remote_ops::checkout` 使用，后者在 `src-tauri/src/remote_ops.rs:705–706` 仍对浮动顶项 `stash pop`。真实双 worktree 探针中，A 智能检出暂停后经新 M6 入口保存 B 的 stash，A 随后应用并删除了 B 的条目；A 的改动未恢复，回执仍称恢复成功。违反 `docs/product-direction.md:126–129` 的原目标与可信回执、`docs/milestones/m6/spec.md:48` 的共享 stash 固定身份，以及 ADR-0012 的删除前独立归档。需要把所有 stash 生产/消费流程纳入协调，按本次保存的精确身份恢复和清理。

- **[P2] 提交失败会丢失快照收尾失败原因。** `src-tauri/src/commit_flow.rs:804–823` 只读取 `verification_note.is_some()`，随后把载荷字段设为 `None`，也没有把该错误并入 `error.related`。真实 pre-commit hook 拒绝提交并阻断快照回执写入后，结果 `needsVerification=true`，却没有快照失败原因；前端已有展示该字段的路径。违反 `docs/adr/0013-localize-structured-user-errors-at-presentation.md:7` 及 `docs/technical-direction.md:68` 的详细原因和写操作结果说明保留要求。应返回现有结构化原因。

本轮只运行两个生产入口探针，退出 0 表示两项缺陷均复现。未运行全套、Release 或桌面验收；未把这些未验收项列为产品缺陷。完整证据见 `standards-evidence.md`。


#### Spec

# Spec

固定完整范围 `26b68d1fb0315cb85376932a8440204c118dfb78...6cd7125d39cba68a91d9c2acead6608eb3eeac45`，14 提交、164 文件。覆盖 C/S/O/H/F/T/L/D/V。结论：**未通过，2 项 P2 行为错误**。

1. **[SP10-01 / P2] 全分支文件历史被旁支删除截断。** F01（`spec.md:133–135`）要求“默认当前分支，可切所有分支”“路径、重命名和不存在版本遵循 Git 的真实历史”。`src-tauri/src/file_history.rs:233–234` 在已接纳任何提交后遇到 `D` 就停止整个合并查询。真实 fixture：共同 base 新增中文空格路径，main 删除、other 随后修改同一文件；生产 `history(all_branches=true)` 只返回 other 的修改，遗漏 main 的删除及仍属于 other 的共同 base，并报 `hasMore=false`。须按祖先关系判定文件生命边界，不能把另一分支的删除当作全局截断。

2. **[SP10-02 / P2] 文件历史静默改写无法无损显示的内容。** F01（`:135`）要求“有截断或不支持的文件类型明确说明”，共同契约 C02（`:54`）要求沿用“编码…保护边界”。`src-tauri/src/file_history.rs:273–278` 把 patch 无条件 `from_utf8_lossy`，没有编码限制结果；`FileHistoryDialog.tsx:264–265` 直接显示。真实 Git 提交 `bad byte: 0xff` 后，生产接口成功返回 `bad byte: �`、`patchTruncated=false`，无任何不可无损呈现提示。应明确限制这类历史内容，避免把替换后的文本当作原文件差异。

未发现另外可确定的整项缺失或范围扩张；Q173 窄范围例外已计入。两项均经真实 Git / Rust 生产入口验证。首轮 1 通过 / 1 失败；日期固定后的全分支专项 0 通过 / 1 失败（均 exit 101）。原始证据与逐域边界见 `spec-evidence.md`。未跑全套、Release、GUI；未完成桌面验收未计作产品缺陷。


规划方核对：原始观察探针退出0表示复现缺陷，不能作通过；Spec两次退出101原件保留。root-verification-uf42zgjw补核真实分支日志、非法字节和两个结构化回执。Codex直接修复四项；另外autostash关联写入失败后的无保护apply是静态附录候选，未算已证实finding，将收紧为保留stash及Saving关联、明确待核对。无Pi/第二返修/另派实施代理。


### Codex 对 review10 的修复（待全新两轴及桌面验收）

Standards01：智能检出纳入共享stash锁，唯一说明+保存前后集合定位本次精确OID；恢复改为固定OID的apply，恢复前独立保护目标实际路径/暂存状态；成功后才独立归档并按Q173事务删除该OID，彻底移除浮动pop。原目标/起始HEAD/目标分支提交再次核对；异常不自动回滚或重复apply，结构化说明已开始、已恢复但清理未确认的边界。检出前保护包含会被切换覆盖的ignored路径。已有update的Git原生autostash写阶段也参与共享锁。真实双worktree（M6保存等待）及独立终端追加（不拿进程内锁）均核对A自己的字节恢复、B条目保留、只有A被归档；归档故障时分支已切换/原改动已恢复/条目保留且待核对。

Standards02：失败CommitOutcome保留原verification_note载荷。真实pre-commit拒绝并阻断本次快照outcome.pending写入后，失败回执同时保留snapshotFinishFailed及结构化cause，草稿/原始文件不被清理；生产formatter中英切换均显示原因。

Spec01：每个固定tip分别让Git --follow追溯至当前文件生命的新增，再按真实提交时间合并去重，固定tips仍跨分页保留。旁支删除/存活分支修改/共同祖先均保留，当前分支删除版本仍有祖先，旧同路径删除重建与rename回归保持。Spec02：历史patch严格UTF-8；不可无损显示时保留历史条目和结构化只读限制，UI显式呈现；正常大patch按合法字符边界截断，不插入替代字符。Git读取失败也保留错误而非静默空diff。

附录静态边界：autostash保存后Pending关联更新失败，保留原Saving意图与精确stash，不再无保护自动apply；回收收尾失败作为related保留。真实reference-transaction只阻断本fixture关联落盘，原改动仍在stash、工作文件为真实保存后状态、没有apply调用，后续也不会盲目重试。

检查原件codex-checks-yf3rkhgl：repair10-full-rust完整653通过/0失败/1既有ignored（M6 92/92），包含6条新增真实Git回归；Node首轮450通过/1失败是新增测试误把CommitOutcome交给M6通用判别器（提交生产路径按needsVerification处理），同步断言真实提交契约后node-affected 17/17，分批共451通过。原失败完整保留，未改产品来迎合该断言。frontend含tsc、lint（既有unused警告）、x18n0errors/62unused警告、extract55候选/0中文均通过；未删键。资源只经x18n CLI apply，原件repair10-i18n-c5jziwdi。绑定差异先保存，27项逐行去尾等同HEAD的格式变化恢复，仅保留FileHistoryEntry.patchError。检查后只删掉已无调用的旧scope helper、同步测试断言、绑定空白与此文档，没有再改行为。

尚无固定Release或真实Revia桌面通过；独占登录shell的验收HOME已补自身.zshenv隔离配置，不改用户配置。下一步固定新候选并全新Standards/Spec，之后Release与真实桌面。禁止续Pi/第二正式返修/实施代理/推送/PR/合main/删worktree。


### 全新独立 review11：固定26b68d1...2833208（未通过）

原件`/private/tmp/revia-m6-batch-ig_nawgt/review-11-p9l05m4f`。两代理已结束，固定源码产品未改。保留两轴原文：

#### Standards

# Standards

固定完整范围 `26b68d1fb0315cb85376932a8440204c118dfb78...2833208ec96cae3b6d69d789eb371a1ca1410c5f`，15 个提交、164 个文件。结论：**2 处静态文档标准违反（P1×1、P3×1），另有 1 项非阻塞启发式判断**。

- **[STD11-01 / P1，静态] 智能检出在 stash 保存后没有重验下一次覆盖的文件。** `src-tauri/src/remote_ops.rs:684–692` 只核对 worktree、分支和提交；最后一次文件指纹核对在 `:668`，之后 `:677` 执行完整 stash 保存。若此期间外部创建当前分支忽略、目标分支跟踪的路径，后续 checkout 可以覆盖它；最初回收记录仍只有该路径的“原不存在”状态，保存后也没有再次保护这些检出路径。违反 `docs/adr/0005-write-ops-carry-diff-fingerprint.md:3` 和 `docs/product-direction.md:253–257` 的写前重验、覆盖前回收要求。应在 stash 的预期自有变化之外保留确认基线，发现其他路径变化则停止，不把现场新状态当成原确认。

- **[STD11-02 / P3，静态] 产品 diff 界面使用整文件翻译扫描豁免。** `src/features/diff/DiffView.tsx:1`、`highlightStore.ts:2`、`rowWindowStore.ts:2` 新增 `x18n-ignore-file`，连正常产品界面/状态代码也退出提取扫描。违反 `docs/milestones/m6/i18n-proposal.md:65` 的“范围最小的 x18n-ignore”要求。只豁免实际诊断调用；无需翻译诊断日志。

**非阻塞判断：possible Speculative Generality。** `src-tauri/src/autostash.rs:421–431` 新增公开的 `operation_kind`、`state_for`，全仓产品与测试均无调用。可移除无需求的公开入口；这是 Fowler heuristic，不是仓库硬性规则。

本轮无产品动态实测；收到固定版本已有 E0425 编译阻断通知后未重复编译或增加探针。上述覆盖后果为静态推导，未冒称真实 Git 复现。未跑全套、Release 或 GUI；这些未验收项本身不算产品缺陷。详证见 `standards-evidence.md`。


#### Spec

# Spec

固定完整范围 `26b68d1fb0315cb85376932a8440204c118dfb78...2833208ec96cae3b6d69d789eb371a1ca1410c5f`，15 提交、164 文件。覆盖 C/S/O/H/F/T/L/D/V。结论：**未通过，P1×1、P2×2**。

1. **[SP11-01 / P1，实测编译阻断] 当前交付候选无法编译。** V（`spec.md:193`）要求“集中运行…现有 Rust 套件…保留源码 SHA、命令、真实退出码”。`src-tauri/src/remote_ops.rs:609` 仍调用已删除的 `unreliable_scope`，全仓无定义。本轮最小 Rust 生产入口 probe 实际退出 101，rustc 报 E0425；两个测试体均未执行。须恢复所需实现并对最终固定源码运行受影响检查，不能用删函数前的成功记录认定此候选可交付。

2. **[SP11-02 / P2，静态] stash 预览静默替换不可解码内容。** C02（`:54`）要求“继续使用已有路径、编码…保护边界”。`src-tauri/src/stash.rs:316–318` 对 Git patch 无条件 `from_utf8_lossy`；`StashPanel.tsx:314–315` 直接显示，载荷没有编码限制标志。含非法 UTF-8 的文本差异会以 `�` 冒充原内容；8 MiB 截断也可能切断合法多字节字符。应明确只读限制，合法文本在字符边界截断。此为呈现问题，不声称文件字节被修改。

3. **[SP11-03 / P2，静态与既有参照] blame 忽略空白初始值不符本机参照。** F02（`:142`）要求“默认值与入口细节按本机 Rebased 对照”。`FileHistoryDialog.tsx:52` 固定 `useState(false)`，首个生产 blame 请求据此不传 `-w`；已保存 `rebased-blame-computation-options.ax.txt:3–4` 明确记录本机 Ignore Whitespaces 为 checked。应对齐已观测值；本审查没有操作 GUI，也不把本机当前值冒称所有安装的出厂默认。

未发现另外可确定的整项缺失或范围扩张；Q173 窄例外已计入。详细覆盖、原件与未测候选见 [spec-evidence.md](./spec-evidence.md)。未运行全套、Release、GUI；待桌面验收未计作产品缺陷。


规划方核对：E0425是root在检查后误删仍有调用helper的失误，不能用旧653通过认此候选编译通过；恢复源已与原成功检查的remote_ops SHA逐字节匹配。STD11-01、SP11-02为静态链路，将补真实Git回归；blame按已保存本机当前参照（非所有安装默认）对齐。翻译豁免收窄至诊断调用，不改扫描门槛。Codex直接修复，无Pi或实施委派。


### Codex 对 review11 的修复（待全新两轴及桌面验收）

Spec01：恢复仍被普通检出调用的unreliable_scope。误删版2833208的E0425原件保留，明确不可验收；恢复前已把候选字节与此前成功全套实际记录的remote_ops SHA核对一致，再完成本轮其他改动并重新全套，不沿用旧结果。

Standards01：智能检出在stash保存前重新计算确认路径集合，保存后仅接受原已知dirty路径的Git变化；未受stash影响的字节/存在/权限基线不推进。checkout写前再次验证指纹/原context/HEAD/固定目标；检出后恢复也核对未由该次tree diff改动的文件、实际暂存/工作状态及已删除路径是否又出现ignored文件。真实Git受控hook覆盖三个阶段：stash保存后新建目标tracked/当前ignored文件；切分支后在原untracked恢复路径新建ignored文件；目标删除原tracked路径后新建ignored文件。均停止后续checkout/apply，foreign原字节在原路径保留，本次stash不删除，分支停在实际已完成边界。

Spec02：stash与文件历史共用严格只读patch解码，非法UTF-8/NUL不替换为伪原文；StashInspection保留文件列表与patchError，UI显式呈现结构化原因，8MiB合法文本在字符边界截断。真实stash的父提交和本地两种比较均无有损文本，index/工作字节不变；9MB多字节文本真实Git输出按上限无损截断。文件列表/numstat路径也不再有损认领。Spec03：blame忽略空白初始true，另外两项仍false，对齐保存的本机Rebased当前参照，不宣称所有安装默认。

Standards02：DiffView/highlightStore/rowWindowStore移除整文件豁免，仅诊断字面量就近x18n-ignore；全仓检查发现main.tsx同类bootstrap豁免也移除，其诊断callee已有最小忽略规则。只剩纯诊断模块的整文件标注；未放宽扫描配置/门禁。启发式项：全仓明确核对两公开autostash helper只有定义无调用后移除；这与误删仍在使用的checkout helper不同，全部发生在本轮集中检查前。

检查原件codex-checks-yf3rkhgl：repair11-full-rust完整655通过/0失败/1既有ignored（M6 94/94），repair11-node完整452/452；frontend含tsc、lint（既有unused警告）、x18n0errors/62unused警告、extract-final62候选/0中文通过，未删键。新增翻译只经x18n CLI，原件repair11-i18n-c6hhbbn9。绑定差异先保留，27项逐行去尾等同HEAD的格式变化恢复，仅保留StashInspection.patchError。最终非绑定源码逐文件哈希与全套记录一致，唯一后续扫描注释变化main.tsx已专门重跑提取；证明repair11-final-source-proof.json。此后不再清理函数或改行为再借旧检查通过。

当前仍未固定Release或完成真实Revia桌面。下一步固定新SHA，全新Standards/Spec后继续Release/真实桌面/Rebased/中英文窄窗；不续Pi/第二返修/实施代理/推送/PR/合main/删worktree。


### 全新独立 review12：固定26b68d1...3ae4910（未通过）

原件`/private/tmp/revia-m6-batch-ig_nawgt/review-12-te3joxay`。两个全新代理已完成并清理本人入口，分别保留两轴原文：

#### Standards

# Standards 独立审查（review-12）

结论：**未通过，1 项 P1 硬性违规**。固定完整范围为 `26b68d1fb0315cb85376932a8440204c118dfb78...3ae4910c3bb7e34d06216f3c7b02f09047044b4b`，16 个提交、163 个最终修改路径；另补查已恢复基线的 `src/main.tsx`，共 164 项覆盖。未采用旧审查通过结论。

**[P1] STD12-01：先 stash 后重新采样文件基线，允许未经确认的覆盖。** [history_sequence.rs:94](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/history_sequence.rs:94) 把 stash 完成后的现场状态设为 `confirmed`。[rewrite.rs:635](/Users/garland/.codex/worktrees/revia-m6/Revia/src-tauri/src/rewrite.rs:635) 此时仅重验 HEAD/context，未保留 stash 前相关文件的原确认。用户确认 cherry-pick 并选择先保存后，若 stash 期间出现目标提交会覆盖的 ignored 文件，首个历史步骤会接受这个新文件并覆盖它，返回成功。

违反 [ADR-0005:3](/Users/garland/.codex/worktrees/revia-m6/Revia/docs/adr/0005-write-ops-carry-diff-fingerprint.md:3)、[产品方向:118](/Users/garland/.codex/worktrees/revia-m6/Revia/docs/product-direction.md:118) 的“变化即拒绝并重新确认”，以及[技术方向:54](/Users/garland/.codex/worktrees/revia-m6/Revia/docs/technical-direction.md:54) 的外部修改边界。

真实 Git 经 Rust 生产 `rewrite::cherry_pick` 复现：stash hook 创建 `victim.txt=FOREIGN`，最终变为 `SOURCE`，回执 `completed=true,error=null`；专项断言失败，exit 101。后续回收记录保存了 `FOREIGN`，因此不声称永久丢失；违规是缺少新的用户确认。应在 stash 前保存全部覆盖路径基线，只推进本次 stash 确实拥有的路径，其余变化立即停止并保留 stash。

完整覆盖、原件及失败尝试见 [standards-evidence.md](/private/tmp/revia-m6-batch-ig_nawgt/review-12-te3joxay/standards-evidence.md)。没有额外阻塞性 smell；未跑全套、Release 或 GUI，不将该分工视为缺陷。临时入口已删除，HEAD 与工作树保持原状。


#### Spec

# Spec

固定完整范围 `26b68d1fb0315cb85376932a8440204c118dfb78...3ae4910c3bb7e34d06216f3c7b02f09047044b4b`，16 提交、163 文件。独立覆盖 C/S/O/H/F/T/L/D/V；结论：**未通过，1 项 P1 行为错误**。

1. **[SP12-01 / P1，真实 Git 生产入口复现] 交互式 rebase 在自动 stash 后覆盖未确认、未回收的新内容。** C01（`docs/milestones/m6/spec.md:47`）要求“执行前绑定并重验…文件状态和确认条件”；C02（`:53`）要求“完整回收并核验成功，再执行有覆盖风险的写入”。`src-tauri/src/rewrite.rs:1115–1135` 仅在 stash 前验证文件，stash 后只核对 HEAD/context。真实 fixture 中，历史 A 添加被忽略路径、B 删除它；本地改动触发自动 stash，其真实 `reference-transaction` hook 在该路径写入外部独有字节。随后生产 `rewrite::interactive` 仍重放 A/B，删除该文件，并返回无错误的 edit 停点。两条回收记录分别保存该路径“原不存在”和其他文件，没有这份新字节。应只接受 stash 的已知自有变更，保留其他路径原确认；变化时停止后续 rebase，保留 stash 与部分执行回执。

未另发现可确定的整项缺失或范围扩张；Q173 仅 stash 删除/清空例外已计入。探针首轮因本人测试入口参数类型错误未执行测试（exit101）；修正入口后 0通过/1业务断言失败（exit101），上述缺陷确实复现。原件、全域覆盖及边界见 [spec-evidence.md](./spec-evidence.md)。未跑全套、Release 或 GUI；待桌面验收本身未计作产品缺陷。


规划方核对：Standards的cherry-pick会覆盖但后续回收有FOREIGN，不能称永久丢失；Spec的interactive add/delete确实删除且没有在stash或回收中保存新字节（root-verification-zkzshb31补核）。两个P1分别成立，共同原因是自动stash前后的确认基线重新采样。由Codex统一修历史清理/序列首次基线/interactive与reword，不只修单个函数；不续Pi或实施委派。


### Codex 对 review12 的修复（待全新两轴及桌面验收）

STD12-01 / SP12-01：历史操作在任何自动stash前保存原文件字节、存在状态、执行权限、完整暂存条目、HEAD/context与确认代次。清理后只允许本次stash原dirty路径或Force明确路径推进，其余路径继续使用原基线。cherry-pick/revert首次持久计划直接接收该基线，不在stash后重新认领现场；interactive/reword构造完Git环境后临写再次核对。stash取得共享锁后及真正push前均校验原确认；过期时停止，保存后异常返回待核对并保留stash与关联，不自动恢复或删除。

真实Git新增3条回归覆盖5个场景：cherry-pick/revert在stash hook期间出现ignored文件；interactive/reword的add/delete中间路径出现新字节；stash暂停时用户refresh使旧确认失效。均保留foreign字节和本地stash，后续历史命令未开始，HEAD保持原值。中英formatter消费真实cherry/revert错误载荷。

本轮原件`/private/tmp/revia-m6-batch-ig_nawgt/codex-checks-yf3rkhgl/repair12-*`：完整Rust658通过/0失败/1既有ignored（M6 97/97），完整Node453/453，前端build含tsc、lint、x18n及extract全部退出0。x18n双语1680键完整，0错误/62未用键警告，未删键；extract62候选/0中文。绑定格式差异先保存，28项逐行去尾等同HEAD后恢复，无语义漂移。最终非绑定源码逐文件SHA与本轮full-rust记录完全一致，证明`repair12-final-source-proof.json`。检查后无行为改动或函数清理。

尚未固定Release或完成真实Revia桌面。下一步固定新提交并全新Standards/Spec完整范围复核，之后Release/真实桌面/Rebased/中英文窄窗。不续Pi、不第二正式返修、不分派实施、不推送/PR/合main/删worktree。


### 全新独立 review13：固定26b68d1...59f8b06（未通过）

原件`/private/tmp/revia-m6-batch-ig_nawgt/review-13-fys20q9z`，两代理已完成；保留两轴原文，互不合并排序。

#### Standards

# Standards — review 13

固定完整范围 `26b68d1fb0315cb85376932a8440204c118dfb78...59f8b065f5223afbf09b11467394b41c1301d5b8`；17 个提交、163 个路径。未实施修复。

## 文档规范违反

1. **[P2] 非 UTF-8 历史路径被转换成另一文件的读取凭据。** `src-tauri/src/file_history.rs:224` 用 `stdout_lossy()` 解码路径，`:263–268` 保存替换后的名字，`:182–188` 再用于 `git show`。真实 Git fixture 中，`old-\xff.txt` 改名为 `current.txt`，另有 `old-�.txt`；打开前者文件历史，其祖先条目实际返回后者的 `UNRELATED-UTF8-ALIAS` diff，`patchError=null`。违反 `docs/product-direction.md:59` 的非 UTF-8 只读限制、`docs/milestones/m5/spec.md:45`“不得有损转换后冒充另一条可打开路径”；M6 C02（`:54`）明确继承该边界。应按原字节解析路径，仅无损 UTF-8 路径可进入后续读取；不可读条目给结构化限制。同文件 `:440–441` 的上一版本路径也须同步处理。生产 probe 在固定 SHA 退出 101；原始证据见 `standards-probe.*`，未声称 GUI 已测。

## 判断项（heuristic，非硬性违反）

1. **[P3] possible Duplicated Code。** `src-tauri/src/rewrite.rs:614–649` 与 `:668–698` 重复“校验 → `prepare_dirty` → 创建计划 → `history_sequence::run` → `finish_snapshot`”，两段均含 `require_expected_head(&session.git, &request.expected_head)?;`，且重复 context 校验和异常升级。建议仅抽共用核验及计划执行部分，保留 cherry-pick / revert 的独立参数与语义，避免后续安全修复只改一处。这是 smell baseline 的维护性建议，不代表当前行为已错误。

计数：硬性 1，判断项 1；最高 P2。产品工作区干净。完整覆盖、规范依据及静态/实测边界见 `standards-evidence.md`。


#### Spec

# Spec 独立审查（review13）

固定完整范围 `26b68d1...59f8b06`，17 提交、163 路径。结论：未通过，3 项 P2；未发现范围扩张。规格行号指 `docs/milestones/m6/spec.md`。

1. **[P2] 重复 OID 的合法 stash 不能单独管理。** S01:63“展示真实 stash”、S03:76“确认绑定精确条目”。`src-tauri/src/stash.rs:471` 以旧 OID 集过滤保存回执；`stash_transaction.rs:215–222` 遇重复 OID 一律拒绝；`StashPanel.tsx:241–246` 也仅按 OID 标识。真实生产入口依次保留暂存区保存 A/B/A，Git 列表为三条（首尾同 OID），第三次误报待核对，单删 A 被拒绝。应把条目身份绑定到受锁保护的 reflog 版本及具体出现项，保留外部变更拒绝规则。原条目、归档均保留，未观察到误删。

2. **[P2] 暂存改动时缺少强制覆盖选择。** O02:93“cherry-pick / revert 会覆盖时提供…强制覆盖 / 取消”。`src/features/m6/HistoryActionDialog.tsx:311` 因任何 `hasStagedChanges` 禁用 Force，含仅暂存其他文件的情况，用户只剩 stash 或取消；这是未获规格授权的功能限制。应按已确认的覆盖保护实现该选择。

3. **[P2] stash 比较对象未实现为设置。** S01:64“按 Rebased 的既有设置表达”，并见 `docs/ui-direction.md:166` 的 M6 设置清单。`src/features/m6/StashPanel.tsx:70,297` 只用初始 false 的局部 checkbox；`src-tauri/src/settings.rs`、`SettingsDialog.tsx` 没有对应设置字段或控件。离开 stash 页签再回来即回到父提交比较。应接入设置、实际呈现与持久化。

#1 为 Rust 生产入口＋真实 Git 复现；#2/#3 为静态证据。完整 C/S/O/H/F/T/L/D/V 覆盖、失败原件与未测边界见 `spec-evidence.md`。未改产品；本人临时入口已删除，最终工作树干净。


规划方核对：重复OID的A/B/A列表与保存/删除错误载荷原件真实存在，未观察到误删；非UTF8历史路径读取别名patch的生产probe已复现，上一版本同解码边界一并修复。Force暂存限制及stash局部比较选项与规格/设置清单不符，属于可用性缺口。Codex直接统一修复，保留锁内精确身份、原确认/完整回收与只读边界；不续Pi、不另派实施。Standards重复代码为判断项，按本轮相关改动小范围处理，不泛化重构。


### Codex 对 review13 的修复（待全新两轴及桌面验收）

Standards01：历史log路径严格UTF-8解码，上一版本rename路径同样无损检查；非UTF8不生成替代字符读取凭据，明确只读限制。真实Git index-info导入old-\xff.txt，同时存在old-�.txt，历史与上一版本均拒绝别名读取，原文件与index不变。重复代码判断项保留为维护建议：cherry-pick与revert的独立参数/语义继续分开，当前共用PreparedState和持久序列，不为非阻塞判断扩大重构。

Spec01：StashEntry新增完整reflog版本/具体出现项key，OID只表示内容；列表行、选择、预览、确认与IPC携带key。锁内核对完整版本后按被确认出现项移除，允许相同OID分别管理；旧key或外部追加均拒绝。保存按出现数量差分认领，不把旧OID全部排除，Git顶端相同对象去重也返回正确回执。真实A/A/B/A保留index场景、旧出现项单删、外部追加拒绝、精确应用后删除均通过。清空确认绑定完整集合，内部自动保存按唯一OID的歧义保护保留。

Spec02：已有暂存改动时允许Force，确认列出将清理的阻断路径并说明已暂存内容也受影响；先完整回收/校验，再只清理声明路径。补齐rename原路径的还原。真实cherry-pick/revert含已暂存+未暂存同文件、暂存新增/rename、无关工作文件，结果与回收字节均正确，无关工作内容保留。

Spec03：stash比较接Settings/SettingsPatch和设置页，面板切换同步持久化，卸载/重开不丢值。新字段按已保存的本机Rebased当前local参照初始true，不宣称所有安装默认。两个比较能力保持。额外发现原迁移把10组设置搜索词写成数字下标，按base原中文词与英文语义恢复，新增回归；语言资源只经x18n CLI（repair13-i18n-bmc9s3xz），未删键。

集中检查原件codex-checks-yf3rkhgl/repair13-*：首轮完整Rust在M6出现4失败，2项是新令牌与自动恢复/排队保存兼容错误，已修；2项同步严格列表版本拒绝及OID歧义错误契约，仍断言现场完整和刷新后正确删除。受影响M6完整100/100、首轮未执行的remainder及doc通过。随后root补查“本次去重保存+终端同内容追加”，首次probe因误用不存在的Result方法编译未执行；修正后真实复现回执误认，原失败保留。增加保存说明匹配约束后stash相关50/50及新场景1/1通过。分批合计662个Rust测试通过/0未解决失败/1既有ignored，M6共101项；不能称首轮完整退出0。

完整Node455/455，最终frontend含tsc、lint、x18n0错误/62未用警告（1681键双语完整）、extract62候选/0中文通过。所有失败原件保留。绑定差异先保存，27项逐行去尾等同HEAD恢复，仅保留StashEntry.key语义；最终非绑定源码逐文件SHA与最后编译检查一致，证明repair13-final-source-proof.json。检查后不再改行为或清理函数。

当前整包仍未完成，尚无本候选固定Release或真实Revia桌面通过。下一步固定新提交，全新两轴完整范围审查，之后继续固定Release、真实桌面/Rebased/中英文窄窗。不续Pi、不第二次正式返修、不另派实施、不推送/PR/提前合main删worktree。


### 全新独立 review14：固定26b68d1...a20ac23（未通过）

原件`/private/tmp/revia-m6-batch-ig_nawgt/review-14-y4u2eugx`，两代理已结束，保留两轴原文。

#### Standards

# Standards 独立审查（第 14 轮）

固定范围：`26b68d1...a20ac23`，19 个提交、163 个路径；本轮未读其他审查轴报告与历史 Comments/report 摘录。

- **[P1，硬性违反] 新分支检出后自行重建 stash 确认基线。** `src-tauri/src/stash.rs:671–676` 用检出后的 `preflight_selected` 生成新的 `expected`，原 `request.expected_state` 只在没有新分支时校验（669）。因此检出 hook 或同时发生的外部修改会被当成本次已确认状态。违反 `docs/adr/0005-write-ops-carry-diff-fingerprint.md:3`：“Rust 层执行前重新计算并比对，对不上就拒绝执行”；也违背 `docs/technical-direction.md:44` 的“每次写操作”契约。真实 Git 生产入口复现：原确认后，`post-checkout` 修改并暂存同一文件第 28 行，仍成功应用 stash 的第 2 行，回执为 `applied=true`、`error=None`。应跨检出携带原文件/暂存区基线，仅接受检出本身拥有的变化，发现其他变化即停止后续 apply，并保留新分支的部分回执。源码、失败断言及原始输出见 evidence。
- **[P2，启发式] possible Duplicated Code。** `stash.rs:632–686` 与 `remote_ops.rs:714–760` 都实现“检出 → 校验 → 保护 → 应用 stash”；后者已有 `before_switch` / `after_switch` 和未提交改动核对，前者另写基线逻辑并遗漏同类保护。建议共享受控的检出状态转换校验；这是维护性判断，与上述 P1 是同一风险来源，不另算一个已复现产品缺陷。

结论：**1 项硬性违反，1 项异味建议；最高风险 P1。** 除该窄场景外，结论来自静态检查；未宣称全套、Release、GUI 或中英文桌面验收通过。未把实施方历史自述当验证证据。


#### Spec

# Spec 独立审查（第 14 轮）

范围：`26b68d1fb0315cb85376932a8440204c118dfb78...a20ac23b1308b6e281dbc0a647e0c8344e708734`，完整19提交/163路径。证据见 `spec-evidence.md`。

1. **[P2] 文件历史 / blame 跳日志时保留旧筛选。** 规格 `spec.md:134` 要求“跳到日志对应提交”，`:139` 要求“能跳到提交 / diff”。`src/App.tsx:4917–4923` 只替换文字哈希，保留旧分支、作者、日期和路径；`LogView.tsx:237–248` 全部传给后端。从作者A筛选的日志进入文件历史，再跳作者B提交，会得到空日志。真实生产探针：目标哈希加旧作者返回0条，移除作者后返回目标1条。跳转须确定展示目标或处理冲突筛选。

2. **[P2] “所有分支”混入 stash 的暂存区合成提交。** 规格 `spec.md:133` 写“默认当前分支，可切所有分支”。`src-tauri/src/file_history.rs:146–148` 的 `rev-list --no-walk --all` 纳入 `refs/stash`。真实生产探针：唯一分支只有一个普通提交，将后续已暂存内容存入stash，所有分支历史额外返回 `index on main: …` 及 `stash-only content` patch；它从未提交到任何分支。应从约定的分支引用与适用HEAD固定起点，排除stash用途引用。

缺失/部分0；范围外功能0；行为错误2，最高P2。未将Release / GUI未验收列为缺陷。未改产品、资源或需求；临时入口已移除，最终Git工作树干净。


规划方核对：Standards生产probe确实applied=true且hook新改第28行被接受，属于同文件变化未拒绝的P1，未声称该行丢失。Spec真实查询原件显示旧作者筛选0条/清除后1条，以及all_branches返回stash合成index行；两项成立。Codex统一修新分支阶段确认及F01跳转/分支引用范围；回收不代替旧确认拒绝，不派Pi或实施代理。


### Codex 对 review14 的修复（待全新两轴及桌面验收）

Standards01：stash作为新分支应用在检出前保留PreparedState的原文件/全暂存条目/确认代次，按固定stash base的Git目标树、Git转换后blob及集中平台权限规则计算预期检出结果。检出后未变路径仍逐字节核对原基线；目标变动路径核对预先固定的期望，不再将hook后现场采样当确认。新dirty路径/暂存变化/分支HEAD不符均停止后续apply，保留已创建分支的明确部分回执、stash与回收。保护后临apply再require同指纹。真实Git新增4场景：仅工作文件变化、仅index变化、检出确实改变目标文件后hook变化、正常切换且无关dirty保留；拒绝场景无stash apply命令，foreign字节/index原样保留。

Spec01：生产filterForCommit构造完整OID的独立筛选，跳日志清作者/日期/路径/分支等旧条件，选中目标且滚动归零；既有LogWorkspaceStore与后端筛选转换Node回归通过。Spec02：所有分支的起点仅heads/remotes与适用HEAD，固定完整tip，排除stash/tag等用途引用；保留分页anchor与每tip独立file-life算法。真实Git已暂存stash-only内容不再混入，普通旁支与共同祖先仍显示，原index/工作文件不变。

重复流程启发式已通过共享PreparedState/ConfirmedFiles的检出期望转换收拢本次新增校验；现有智能检出的独立业务编排保持，避免无需求扩大重构。判断项不作为另一个产品缺陷。

本轮原件codex-checks-yf3rkhgl/repair14-*：集中Rust lib271+M6 103，以及其余集成290，分批664通过/0失败/1既有ignored；完整Node456/456，frontend含tsc、lint、x18n0错误/62未用警告（1681键双语完整）、extract62候选/0中文均通过。本轮无失败，旧轮失败原件继续保留。绑定差异先保存，28项逐行去尾等同HEAD后恢复，无新绑定语义；最终非绑定源码SHA与最后编译检查完全一致，证明repair14-final-source-proof.json。检查后无行为修改或函数清理。

尚未固定本候选Release或完成真实Revia桌面。下一步固定新提交、全新两轴完整复核，之后继续固定Release/真实桌面/Rebased/中英文窄窗。不续Pi、第二返修、实施委派、推送PR或提前合main/删worktree。


### 全新独立 review15：固定26b68d1...f426a2d（未通过）

原件`/private/tmp/revia-m6-batch-ig_nawgt/review-15-xpfi_ahn`；两个既有审查代理均已完成并结束，产品工作树干净。保留两轴原文。

#### Standards

# Standards — Review 15

固定差异：`26b68d1…f426a2d`，20 提交、163 路径；未读取历史审查或另一轴报告。硬性规范 3 项，启发式 1 项，最高 P3。

- **ST15-01 [P3，硬性] 文件历史仍显示状态字母且未用有效状态色。** `src/features/m6/FileHistoryDialog.tsx:219` 直接渲染 `{entry.status}`，其 `--gitDecoration-modifiedResourceForeground` 在四套主题均未定义；文件名在 `:228` 也没有状态色。违反 `docs/ui-direction.md:12` 的“文件名按状态上色……不显示状态字母”（Q128）。应使用现有状态颜色映射，移除字母。
- **ST15-02 [P3，硬性] 提交前快照译名偏离唯一术语。** `locales/en.json:200` 的 `errors.commitFlow.theCommitDraftPreCommitSnapshot` 使用 “pre-commit snapshot”；生产错误来自 `src-tauri/src/commit_flow.rs:626`，经前端格式化呈现。违反 `docs/README.md:203`（Q170）与 `docs/milestones/m6/i18n-proposal.md:36` 的唯一译法 `Snapshot Before Commit`，重新引入与 hook 的混淆。
- **ST15-03 [P3，硬性] 已发布历史警告使用禁用说法。** `locales/zh-CN.json:1805` 写“需要强推的历史”，由 `src/features/m6/HistoryActionDialog.tsx:265` 呈现。`GLOSSARY.md:141` 将“强推”列入 Avoid；应使用“强制推送”。
- **ST15-S01 [P3，判断项：possible Duplicated Code / Shotgun Surgery]** `src-tauri/src/commands.rs:699`、`:716`、`:729`、`:746` 重复同一锁失败构造，却分别使用 `repositoryListLockFailed7/8/9/10`；整文件共 16 个等义编号键，中英文同步重复。可共享锁失败错误构造和语义键，避免同一文案修改散落多处；这不是硬性违规。

`x18n check --only glossary --json` 实测退出 0、0 诊断，未捕捉上述术语偏差。安全边界审查未确认新增硬性违规；未运行全套、Release 或 GUI。证据与逐路径覆盖见 `standards-evidence.md`。


#### Spec

# Spec 独立审查（第 15 轮）

固定完整差异：`git diff 26b68d1fb0315cb85376932a8440204c118dfb78...f426a2dbc5f37441d35a710d87b0a470a3ce25e8`；已读 20 个提交提示与 163 路径清单，独立覆盖 C/S/O/H/F/T/L/D/V。

## 行为错误

- **[P2] 日志文件历史入口丢失所选文件的版本身份。** 规格 F01：“从工作文件和日志相关文件入口打开只读文件历史”（`docs/milestones/m6/spec.md:133`）；“路径、重命名和不存在版本遵循 Git 的真实历史，不把同路径的新文件误认作同一对象”（`:134`）。`src/App.tsx:5664` 丢弃回调的 commit，只保存路径；`:5747` 挂载时也只给路径。`src/features/m6/FileHistoryDialog.tsx:64` 首查不带 anchor，`src-tauri/src/file_history.rs:155` 因此固定当前 HEAD，`:295` 在新文件的 A 记录终止。触发：提交 A 创建“中文 空格.txt”，随后删除，再在同路径创建无关新文件；从日志 A 的文件菜单打开文件历史。结果只显示新文件，旧文件的提交、版本无法在此历史中选择。本轮真实 Git + 生产 `file_history::history` 已复现：按 UI 参数仅返回新 OID；绑定旧 OID 才返回正确旧对象。最小探针 exit 101，失败原件及 fixture 均保留。

## 缺失、范围与边界

除上述 F01 部分满足外，未发现有足够证据可另列的缺失要求或 scope creep。Q173 的 stash 删除事务属于明确授权范围。Release、GUI、Rebased 运行时对照和完整套件本轮未运行，不列作产品缺陷，也不据旧报告宣称通过。

计数：行为错误 1；另列缺失 0；scope creep 0。覆盖与证据见 `spec-evidence.md`。


规划方核对：日志回调确实丢弃完整commit，真实生产探针按仅path打开返回新对象，传旧OID返回旧对象；F01问题成立。新增文件历史状态字母和未定义主题变量、唯一英文译名、禁用中文术语均有明确规则，统一修正。锁错误重复是可维护性判断，本轮不为此扩大Rust错误迁移或删除既有locale键；作为后续建议保留。由Codex直接修复，不续Pi、不正式返修或实施委派。


### Codex 对 review15 的修复（待全新两轴及桌面验收）

Spec01：工作文件/日志入口显式保存打开时的worktree身份、path和完整commit；对话框按这些字段固定挂载。日志初始查询与刷新均以原commit为anchor，后续分页沿用返回的固定anchor；显式切所有分支时读取分支tips，切回恢复原commit。初次blame也使用原版本；工作文件入口保持工作副本行为，切目标不将旧文件历史重新解释为新worktree。

Standards01：移除新增文件历史的Git状态字母与未定义主题变量，显示文件名时使用既有status-*主题色。Standards02/03：经x18n CLI修正唯一英文Snapshot Before Commit及中文“强制推送”，未删除既有键或放宽扫描。重复锁错误启发式保留为维护建议，本轮未扩大Rust迁移。

本轮最小相关检查均退出0，原件codex-checks-yf3rkhgl/repair15-*：Rust文件历史8/8（扩展旧对象/删除版本/固定分页），Node相关73/73（含新增2个真实组件hook回归），build含tsc、lint、x18n双语1681键0错误/63警告（62未用键、1待更新译文），extract63候选/0中文。生产Rust代码未改，因此未重复完整安全套件；上一候选完整Rust664/0/1保留其适用范围。组件回归验证调用参数与回复守卫，不能替代真实桌面验收。本轮无新增失败；review15失败probe及历史失败原件均保留。最终逐文件SHA与本轮6条检查一致，证明repair15-final-source-proof.json，检查后无行为修改。

仍未构建当前候选Release或验收真实Revia桌面；下一步固定提交并按code-review启动全新两轴完整复核。无Pi/第二返修/实施委派/推送PR/提前合main删worktree。


### 全新独立 review16：固定26b68d1...534dbaf（未通过）

两个既有代理已完成并结束，原始生产Git和组件失败证据保留于`/private/tmp/revia-m6-batch-ig_nawgt/review-16-ntjkw6mz`。两轴原文如下。

#### Standards

# Standards：第 16 轮独立审查

范围：`26b68d1...534dbaf`，全部 164 个路径已分类核对；禁止读取的报告与 Comments 按任务排除。以下均为本范围新增问题。

## 文档约束违反（3）

1. **[P2] 文件历史选择缺少路径身份** — `src/features/m6/FileHistoryDialog.tsx:167`（点击入口 222）。规格 F01 要求“选提交可查看该版本文件与 diff”“路径、重命名和不存在版本遵循 Git 的真实历史”。根提交同时创建 `a.txt`、`b.txt`，两个分支分别重命名为 `target.txt` 后，所有分支历史合法返回同 OID 的两个路径。点击 `b.txt` 却仍显示 `a.txt` 的 diff，两个记录同时高亮，blame 入口也使用错误路径。选择应绑定 `(oid, path)`。真实 Git＋生产组件复现，见证据 E1。

2. **[P2] 分隔字节截断合法文件名** — `src-tauri/src/file_history.rs:233`。规格 C02 要求“继续使用已有路径、编码……保护边界”，F01 要求路径遵循真实历史。对完整输出按 U+001E 切块会把包含该字符的合法 UTF-8 路径当作提交边界；生产入口把 `before\u001eafter.txt` 返回为 `before`，diff 为空且无错误。应按 NUL 字段协议解析，保持路径字节身份。真实 Git 集成断言失败，见 E2。

3. **[P3] 英文计数未区分单复数** — `locales/en.json:884`（同类 1462）。文案规则第 12、14 行要求“复数通过参数表达”“保留……复数语义”。生产 `buildMessages` 在数量 1 时仍输出 `1 files`、`1 branches`；应在语言资源与格式化链路中按数量选择单复数。见 E3。

## 启发式判断（0）

未另列可行动的 Fowler smell；固定类型入口、共享保护模块和生成绑定按仓库约束处理。未计入工具会检查的类型、lint、键错误，也未把桌面待验收当作产品缺陷。


#### Spec

固定审查范围 `26b68d1fb031...534dbafb4552`，累计 164 路径（报告及 Comments 按隔离要求未读）。发现 **1 项已复现问题**。

1. **[P2] 文件历史截短合法文件名后读取另一目标。** `src-tauri/src/file_history.rs:233` 用 `\u001e` 切分记录，但该字符可出现在合法文件名中。真实 Git fixture 提交 `note\u001e.txt` 后，通过生产 `file_history::history` 得到路径 `note`；随后 `git show` 也读取 `note`，返回空 patch 且无错误提示。规格 `spec.md:134` 要求“选提交可查看该版本文件与 diff……路径、重命名和不存在版本遵循 Git 的真实历史”。这会隐藏真实 diff；截短路径存在时还可能展示其他文件。应使用不可歧义的字段解析，或明确拒绝无法无损解析的结果，不能截短后继续读取。

最小复现退出码 **101**，原始 argv、SHA、stdout/stderr、载荷与临时测试源码见 `spec-evidence.md` 和 `spec16-*`；临时入口已移除。未确认其他可报告缺陷或范围扩张。未执行桌面验收，未宣称完整检查通过。


规划方核对：两个独立真实Git探针均确认U+001E路径被截断后继续读空diff；Standards另有真实all-branch page及生产组件点击结果，确实同OID不同path同时高亮并读错文件。英文1 files/1 branches实测成立，同类计数一并盘点，按已确认资源/复数规则修正。Codex直接完成相关修复，不续Pi或另派实施。


### Codex 对 review16 的修复（待全新两轴及真实桌面）

Standards01：文件历史选择、刷新保留、高亮和实际预览/blame统一按完整OID+准确path标识。真实Git两个分支把同一根提交的a.txt/b.txt分别重命名为target.txt，生产页保留两条历史；该真实page输入生产组件后分别点击两行、刷新与blame，路径/patch正确且仅一行高亮。

Standards02 / Spec01：保留Git NUL字段，按提交头、状态和单/双路径的语法消费，只有位于头位置的U+001E才作标记；不再对整份输出切该字节，不修剪路径。非法协议拒绝并给结构化当前语言错误。真实Git回归包括含U+001E/换行/tab的路径、看似头标记的文件名、author/subject中的分隔符、rename旧新路径和固定分页，同时存在截短别名文件；原路径、旧名和diff字节正确，没有读取别名。

Standards03：盘点并通过x18n CLI修正40条英文计数，ICU one/other覆盖数量名词及has/have、is/are、remains/remain，源中文参数契约不改。使用固定依赖intl-messageformat12.1.4，仅ICU计数进入其标准解析/格式化；普通占位符保留原行为，格式化带明确locale、数字字符串转计数、用户参数作为数据、标签保持文字。原有函数参数、占位符、通知即时切语言均通过完整Node；新增数量0/1/2、错误载荷的字符串计数、含ICU样式用户数据和语言切换回归。选用依据[FormatJS官方文档](https://formatjs.github.io/docs/intl-messageformat/)；GrokSearch连接与内置web能力失败后直接读取官方原页，原件impl/repair16-formatjs-source。依赖安装原件repair16-formatjs-install，未运行pnpm approve或改全局设置。

x18n细分纠正：repair15实际63 warnings为62 unused-key+1 stale-translation；前轮“63未用”措辞已修正，不改变原check数据。既有英文force push已与批准的中文强制推送一致，经x18n review mark-fresh记录源hash到.x18n/reviews.yml，本轮最后check为1682键双语完整、0错误/62未用键警告；不删键或放宽检查。extract63候选/0中文。

集中检查原件codex-checks-yf3rkhgl/repair16-*：M6首次104/105通过，唯一失败为既有user_refresh_during_autostash的15秒等待hook标记超时（原断言/失败stdout/stderr保留）；源码与门槛不改，单独重跑同一测试通过5.36秒，分批105项均有成功证据，不能称首次整套成功。两个新增真实Git回归首次均通过。完整Node460/460、build含tsc、lint、x18n、extract均退出0。本次只改Rust文件历史读取，Rust其余套件未重复；不借旧664项声称当前完整套件已跑。最终所有非文档源文件、依赖清单/锁文件和x18n元数据SHA与本轮检查输入一致，证明repair16-final-source-proof.json。检查后无行为修改或helper清理。

仍未固定当前候选Release或完成真实Revia桌面；下一步提交固定并启动全新独立两轴完整审查，之后才Release与真实桌面/Rebased/中英文窄窗。Q113不再派Pi或实施代理；不推送/PR/提前合main或删worktree。


### 全新独立 review17：固定26b68d1...e771313（未通过）

两代理已结束；原件`/private/tmp/revia-m6-batch-ig_nawgt/review-17-52x4b2t8`。分别保留原文。

#### Standards

# Standards

固定完整范围：`26b68d1...e771313`，165 个路径（按要求隔离 Comments、report 与 q173-validation）；完整覆盖与原始证据见 `standards-evidence.md`。

## 硬性违反：1

- **[P2] stash 删除在归档后沿用已失效确认** — `src-tauri/src/stash.rs:820–824`。`docs/ui-direction.md:65` 规定“新快照到来后旧 hunk 和旧确认失效”，M6 C01（`spec.md:47`）要求刷新后旧确认失效。这里只在归档前检查，持锁事务仅再验 stash 出现项与仓库身份。真实 Git 生产入口实测：在归档后的 `BeforeLock` 接缝调用 `session.refresh()`，代次 `(1,0)→(1,1)`、preflight 改变，仍返回 `deleted:true`，stash 数量 `1→0`。应把原确认贯穿归档，并在真正删除前重验；保留已经形成的归档。复现检查退出 **101**，失败断言与全部 Git 记录已保存。此问题由本轮新增删除流程引入。

## 判断性建议：1

- **[P3，可能的 Duplicated Code]** `src/App.tsx:4763–4768` 与 `4884–4902` 重复格式化同一 `RewriteOutcome` 的 applied/skipped/pending；两份已出现分歧，只有后一份包含 `failedOid`。统一为惰性结果格式化函数，避免后续回执字段或翻译变化需两处同步。这是启发式建议，不是仓库硬性规则。

本轴合计：1 项硬性违反、1 项判断性建议；最严重为 P2。未启动 GUI，未运行完整套件，未修改产品；临时测试入口已移除。


#### Spec

# Spec 独立审查 17

固定完整范围：`26b68d1fb0315cb85376932a8440204c118dfb78...e771313c8fbfe7cf9a5ffd90b3a4a7ef70095ed2`，覆盖全部 165 路径（禁读材料的边界见证据）。

1. **[P2] 关闭保护安装错误仍冻结为旧语言** — `src/App.tsx:1138–1141`。窗口关闭或原生退出监听安装失败时，先把两层产品说明拼成字符串，再存为无 `code` 的错误；`formatAppError` 直接返回该字符串。中文启动后切 English，现有错误和后续写入拒绝原因仍含中文。违反 spec L01:155“覆盖……错误”、L03:170“同一已存在错误切换语言后也能正确呈现”。这是 M6 对既有路径漏迁移，不是新增关闭保护逻辑回归。应保留错误语义与监听类型，展示时再翻译。

2. **[P2] 成功的自动 stash 流程可能完全没有保存通知** — `src/App.tsx:4683–4684`。快速完成并成功恢复、删除 stash 的 cherry-pick/revert/rebase，会返回 `restored && removed`；这里直接跳过通知。保存通知的另一个入口只观察仍存在的 pending link；若关联在下一次读取前已清除，用户仅见操作成功，没有 O02:95 要求的“通知说明已保存并提供‘查看 stash’”。应在保存成功时发带固定条目身份的通知，不能依赖刷新是否碰巧读到短暂关联。

未发现其他可证实规格偏离。未运行 GUI、Release 或完整套件；这些待验收项不作为缺陷。完整覆盖与原始源码证据见 `spec-evidence.md`。


规划方核对：真实删除探针exit101且刷新后deleted:true，原archive与Git日志保留；关闭保护把已翻译字符串存入无code错误、成功autostash跳过通知的路径均确实存在。由Codex直接统一修复；同类清空确认和已删除条目的查看动作同步保持安全与准确定位，历史结果说明重复判断项在相关通知改动内收拢。不续Pi或另派实施。


### Codex 对 review17 的修复（待全新两轴及真实桌面）

Standards01：stash 单删与清空的原确认贯穿独立归档和持锁事务，在 BeforeLock、Locked、BeforeCommit 的测试接缝之后重验原 preflight 及确认代次；内部清理同样保留代次。自身提交成功后不再用旧列表反向拒绝。过期时保留全部原 stash 及已经形成的归档，不删除任何条目。新增真实 Git 回归覆盖 1/2 条 stash × 单删/清空 × 三个提交前边界，共 12 场景；刷新后均拒绝，原列表、工作字节与 index 不变，独立归档存在。

Spec01：关闭/退出监听类型与原生错误保存为结构化 cause，写入门禁携带语义载荷；监听安装中及正在关闭的原因在调用时生成。既有错误和后续写入拒绝随当前语言展示。保留原 CloseGuardGate/WriteGuardOrchestrator 的空原因兼容，实际 runGuardedWrite 回归验证失败时未执行写入，未提前翻译并扁平化错误。

Spec02：自动 stash 的真实成功回执在任何刷新前发送去重的保存通知，不再依赖短暂 pending link；成功恢复并移除时说明内容已在删除归档。查看动作保留原 worktree/OID，live 条目不存在时展开并定位已确认的对应归档，live 缓存到归档刷新也能正确转换。真实 Rust 成功回执输入生产通知卡片，中英文切换及动作目标通过 Node；尚未作真实桌面验证，不以此代替 GUI。Standards 判断项在相关修改内收拢为共用惰性 RewriteOutcome 说明，首次与 Continue 都包含 failedOid。

本轮原件 `codex-checks-yf3rkhgl/repair17-*`：M6 首次 105/106 通过；唯一失败是旧重复 stash 场景期望事务层错误码，而新增原确认更早拒绝。只同步该断言及两行说明，保留原数据完整性与后续精确应用/删除断言，单独重跑同一测试 1/1 通过。分批 106 项有成功证据，不能称首次完整退出 0。Rust 其余套件本轮未重复，不借旧结果声称当前全套通过。

最终完整 Node 463/463、frontend04（含 tsc/build）、lint04、x18n04、extract04 均退出 0。x18n 双语各 1687 键，0 错误、64 unused-key 警告（保留旧监听标签键）；extract 64 候选、0 中文。所有资源与源 hash 仅经 x18n CLI 维护。最初 TS 三错、Node 的空原因兼容回归与两次新测试预期错误均已修，失败原件保留，未靠更改文案满足错误测试预期。

`repair17-final-source-proof.json` 核对最终前端/locale/tools 与末轮五检查逐文件 SHA 一致；Rust 生产源与首轮/专项一致，测试最终仅上述断言变化，反向重建字节匹配首轮 hash。依赖清单/锁文件与 HEAD 一致；新增 x18n metadata 另记 hash。检查后无源码修改或 helper 清理。

整包仍未完成；下一步固定提交并启动全新两轴完整范围审查，之后固定 Release 与真实桌面/Rebased/中英文窄窗。Q113 不续 Pi、不第二次正式返修、不另派实施；不推送/PR/提前合 main 或删 worktree。


### 全新独立 review18：固定26b68d1...7bdf8b2（未通过）

两轴均已结束；原件`/private/tmp/revia-m6-batch-ig_nawgt/review-18-daur43kg`。分别保留原文。

#### Standards

固定审查范围：`26b68d1...7bdf8b2`，25 提交、168 路径。按任务限制未读取旧审查、Comments、report 或 q173-validation。

**硬规则违反：1。**

- **[P1] historical revision 必须先固定为完整 OID** — `src-tauri/src/external_tools.rs:263–265`。ADR-0002 第 5 行规定“前端只能调用……固定、带类型的操作，不能传入任意 Git 参数或 shell 命令”；AGENTS.md 的安全边界要求覆盖前回收。`external_tool_launch` 原样传入的 `revision` 直接进入 `diff-tree` 的选项区。独占临时仓库经生产 `external_tools::launch` 传 `revision="--output=<fixture 外 sentinel>"`，Git 虽退出 129，验证步骤已经将该文件从 13 字节截为 0 字节，尚未创建回收记录或启动外部工具。新增模块引入此问题，非继承问题。应先拒绝选项并解析、验证完整提交 OID，再只使用该 OID 构造验证与 difftool 参数。原始失败、精确 argv、stdout/stderr 和测试源见同目录证据。

**启发式判断：0。** 已按完整 smell baseline 核对；字面量翻译映射、固定 IPC 包装等由仓库规则明确要求，不另算 smell。未把工具已覆盖的问题或待做 Release / 桌面验收列为缺陷。


#### Spec

# Spec 审查 18

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...7bdf8b2cb7cf3f4facb3a6efd2d3e7f486e9443e`，25 个提交、168 条路径。已核对 C/S/O/H/F/T/L/D/V；未阅读旧审查、任务 Comments、report.md、q173-validation.md 或另一轴结论。

(a) 缺失/部分要求：无单独列项。

(b) 扩范围：未发现。

(c) 看似实现但错误：1 项。

- **[P2] 静止的统一 diff 行不会即时切换语言。** 规格 L01（`spec.md:155–156`）要求“简体中文 / English 覆盖已有及新增产品界面、菜单、提示……”以及“设置即时切换并持久化”。位置：`src/App.tsx:360–362`、`src/features/diff/DiffView.tsx:523–542,571,658–684`。在中文下打开带块按钮的统一 diff，等待行与高亮稳定，再在设置切成 English；App 重绘，但 `RowView` 使用 `React.memo`，缓存行、回调和其余 props 保持相同，既不传语言版本，也不订阅语言，所以暂存/丢弃按钮的 title、aria-label（以及显示 hunk 操作时的按钮文字）继续为中文，直到行重新渲染或重新取数。应让 memo 行消费当前语言/语言版本，保留页签与滚动。此为 M6 引入语言运行时后遗漏的失效契约，不是要求修复基线 diff 性能问题。

结论依据为固定源码静态调用链；未启动 GUI，未将尚待规划方执行的 Release、桌面/Rebased 对照、D06/Q98 算作产品缺陷。完整覆盖和原始源码摘录见 `spec-evidence.md`。


规划方核对：外部工具生产探针真实exit101，Git exit129时sentinel由13字节截为0；revision在diff-tree选项区，保护尚未开始，原stdout/stderr与测试源码已保留。统一diff RowView memo且无locale依赖，稳定缓存与回调会跳过切语言重绘，源码调用链成立。由Codex直接修复并补生产入口/组件回归；保持原页面上下文。不续Pi或另派实施。


### Codex 对 review18 的修复（待全新两轴及真实桌面）

Standards01：历史外部 diff 在任何 diff-tree/difftool 命令前通过既有 history::read_commit 验证完整 OID 和 commit 对象类型，再读取并验证固定的第一父 OID；路径验证与实际工具启动都只使用这两个完整对象身份，不再透传请求中的任意 revision 表达式。根提交沿用明确的“没有可比对父提交”错误，历史删除文件仍允许读取。新增真实 Git 生产入口回归：`--output=<独占fixture仓库外sentinel>`、HEAD表达式、blob、缺失对象及根提交全部在diff命令前拒绝，sentinel原13字节、文件/index/HEAD保持不变且无回收记录；有效大小写OID归一后验证与difftool都用相同的完整父/子OID。未修改用户Git配置。

Spec01：DiffView把当前locale传入memo RowView，语言切换只使现有行重绘文案，不重挂整个diff、不改行缓存或滚动。现有Node组件harness执行生产DiffView/RowView，固定相同行对象与回调、实际滚动19px，并覆盖段按钮及hunk按钮两档；切语言后唯一失效prop为locale，title/aria-label及正文切为English，缓存引用与滚动不变。未添加测试框架或声称GUI验收。

本轮最小相关检查原件 `codex-checks-yf3rkhgl/repair18-*`：首次Rust检查因新增测试断言移动String导致E0507，尚未运行测试；只修测试比较为借用，保留编译失败原件，重跑受影响external筛选11/11通过（包含实际外部diff/merge、超时/信号、不确定回执及新输入边界）。本轮未重跑完整Rust或完整M6，不借旧结果宣称当前全套通过。

完整Node464/464，frontend（含tsc/build）、lint、x18n、extract均退出0。双语各1687键完整，0错误、64 unused-key警告；extract64候选、0中文。无locale/依赖/x18n metadata改动。`repair18-final-source-proof.json`逐文件核对：最终前端/Node/locale/tools与检查一致，Rust02与最终全部源码一致；前面五检查仅存在已说明的后续Rust测试比较改动。检查后不清理helper或改产品行为。

整包仍未完成，下一步固定候选并启动全新独立两轴完整范围审查，通过后固定Release与真实桌面/Rebased/中英文窄窗验收。不续Pi、不第二正式返修、不另派实施、不推送/PR/提前合main或删worktree。


### 全新独立 review19：固定26b68d1...d6b990d（未通过）

两轴均已结束；原件`/private/tmp/revia-m6-batch-ig_nawgt/review-19-ksr4qc70`。分别保留原文。

#### Standards

## Standards

硬性违例 **1**；启发式问题 **0**。

**[P1] 多路径强制覆盖必须在每次写入前校验剩余文件。** `src-tauri/src/rewrite.rs:439–461` 用初始状态逐个 restore/clean，期间没有重验或回收后续路径。用户为 cherry-pick / revert 选择强制覆盖后，首个文件的 Git hook 或其他工具若修改下一文件，后续 restore 会抹掉这些未确认、未归档的新字节。`prepare_dirty:515–517` 随后把全部路径当作授权改动推进基线，操作仍可报告完成。

违反 [ADR-0005:3](/Users/garland/.codex/worktrees/revia-m6/Revia/docs/adr/0005-write-ops-carry-diff-fingerprint.md:3) 的“执行前重新计算并比对，对不上就拒绝执行”，以及 [产品安全边界:254、257](/Users/garland/.codex/worktrees/revia-m6/Revia/docs/product-direction.md:254) 的写前重验与覆盖前回收要求。

独占临时仓库通过生产 `rewrite::cherry_pick` 和真实 Git 的 `post-index-change` hook 稳定复现：hook 在 restore a 后向 b 写入新内容，restore b 将其覆盖；结果 `completed=true`，全部回收记录均无该新内容。唯一集成复现退出 **101**，临时入口已移除，工作树干净。应逐步核对尚未处理路径的原确认基线，发现变化立即停止并保留部分回执。

完整 26 提交、168 路径的覆盖登记与原件见 [standards-evidence.md](/private/tmp/revia-m6-batch-ig_nawgt/review-19-ksr4qc70/standards-evidence.md)。


#### Spec

# Spec review19

结论：**0 项确定缺陷**。

固定范围为 `26b68d1fb0315cb85376932a8440204c118dfb78...d6b990d2e40e79ebb6030fa08a5dd261f4122226`，包含26个提交、168条路径。已按 M6 规格 C/S/O/H/F/T/L/D/V 核对原始要求、生产入口、Git行为接线、共享安全契约及错误/语言迁移，未确认可报告的缺项、错误实现或范围扩张。

完整规格映射、覆盖清单、审查边界与原始元数据见 [spec-evidence.md](./spec-evidence.md) 和 [spec-path-coverage.md](./spec-path-coverage.md)。未读取旧审查、issue Comments、实施报告或另一轴结论。

本轮为只读源码审查，未新增复现或重跑套件。固定Release、真实桌面/Rebased、D06/Q98及后续验收不包含在这个结论中；没有把尚未做的规划方验收当作产品缺陷。产品、语言资源和绑定未修改。


规划方核对：生产真实Git探针exit101，restore a后的hook写入b新内容，随后被restore b抹掉；cherry-pick仍completed且全部回收before没有新字节，原件完整保留。Codex直接修复逐步确认，保留已经发生的清理及后续文件/index，未开始历史动作时也明确返回待核对，不把部分清理冒称未写入。不续Pi或另派实施。


### Codex 对 review19 的修复（待全新两轴及真实桌面）

Standards01：Force 清理接收同一个 PreparedState，每一步 Git restore/clean 前重验全确认；该步完成后只推进该路径的文件/index基线，其他路径继续核对原确认。后续路径、无关暂存内容、HEAD/context或刷新代次发生变化即停止，不继续写后续路径或开始历史操作；已有清理以 NeedsVerification 及结构化 cause 明确返回，初始回收与现场保留。删除了原来一次推进全部Force路径的采样，不重新认领中途改动为授权状态。

新增现有Rust真实Git回归覆盖 cherry-pick/revert × 工作字节/index/ignored后续路径，共6场景。真实post-index-change hook在清理a后改写b或b暂存条目；所有场景立即停止，HEAD不动、不启动cherry-pick/revert，也没有对b的restore/clean；b的新字节/暂存内容原样保留，a只保留已经发生的清理，原dirty-a/dirty-b可从回收读取，未创建隐藏stash。既有Force含暂存新增/rename与字面量路径的正常流程仍通过。Spec轴原正式结论为0，本轮不另增产品范围。

原件 `codex-checks-yf3rkhgl/repair19-m6-rust.*`：完整M6 108/108、0失败、0忽略（130.21秒），含上述6场景。本轮没有新增验证失败；review19原失败探针及所有旧轮失败保留。Rust其他套件未重复，不称当前完整Rust全套通过。

本轮只改Rust生产与测试。前端、Node脚本、locale逐文件哈希与上一候选通过时完全相同，因此未重复运行未受影响的Node464/464、build/tsc/lint、x18n双语1687键0错误64未用警告/extract64候选0中文；保留这些原检查的范围和时间，不冒称本轮重新执行。依赖清单/锁文件/x18n metadata同HEAD。`repair19-final-source-proof.json`核对本轮全部源码与M6检查一致、前端相关输入与上述旧检查一致；检查后无源码/helper清理。

整包尚未完成；下一步固定候选、全新独立两轴完整复核，通过后固定Release及真实桌面/Rebased/中英文窄窗。不续Pi、不第二正式返修、不另派实施、不推送/PR/提前合main或删worktree。


### 全新独立 review20：固定26b68d1...5a9d1ca（未通过）

两轴均已结束；原件`/private/tmp/revia-m6-batch-ig_nawgt/review-20-cwxrcy2l`。分别保留原文。

#### Standards

# Standards review20

固定审查 `26b68d1fb0315cb85376932a8440204c118dfb78...5a9d1cad71bafd0f895c78fc633c8e590b00879f`，覆盖全部 27 个提交、168 路径；受限制的旧审查及报告未读。**硬违反 1，判断项 0。**

- **[P2] ST20-01：reset 未重验已发布历史的警告条件** — `src-tauri/src/rewrite.rs:724`。`reset_preflight` 用远端跟踪引用计算 `published`，但执行凭据只绑定 HEAD、文件/index 和分支/操作上下文。确认页打开时 `published=false`，终端随后把 `refs/remotes/origin/main` 更新至当前 tip，三个确认凭据仍相同；生产 Soft reset 接受旧确认并成功移到 base，遗漏应有的已发布历史额外警告。违反 `docs/product-direction.md:257` 的 Q123“改写已在上游的提交时额外警告”，以及 `docs/milestones/m6/spec.md:47` 的相关引用和确认条件写前重验。应将该范围/引用身份纳入确认并在变化时拒绝旧请求。

已用独占 tempfile 仓库、HOME/appdata 与真实 Git 的生产入口复现：编译成功，安全拒绝断言失败，cargo 退出 **101**；产品实际返回 `Ok` / `completed=true`。临时测试入口已保存并移除，工作区干净。证据及逐路径覆盖见 [standards-evidence.md](standards-evidence.md)，测试原件为 `standards-published-test.rs`。

未把工具可检问题、Q173 已授权的窄例外，或待做 Release/真实桌面验收列为产品缺陷。


#### Spec

# Spec review20

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...5a9d1cad71bafd0f895c78fc633c8e590b00879f`，27 个提交、168 个路径。

**确认缺陷：0。** 已按 C/S/O/H/F/T/L/D/V 核对原整包需求、生产入口和相关回归；没有足够证据报告新增的功能缺失、部分实现、范围扩大或行为违背规格。Q173 例外仅用于 stash 删除/清空，其余新增 Git 操作仍由 Rust 调系统 CLI。

唯一局部真实 Git 复现退出 101，原件完整保留：其断言要求 Git 未识别出的 rename 关系，因此不能证明产品缺陷，已撤销该候选。未把该失败标作通过。自有临时测试入口已删除，HEAD 未变，工作区干净。

完整覆盖、排除理由、原始命令/输出和全路径索引见 [spec-evidence.md](spec-evidence.md)。本结论不替代固定 Release、真实桌面/Rebased、D06/Q98 验收；未执行这些步骤不记为产品 finding。


规划方核对：原始shown/after preflight中published确由false变true，而三个凭据逐字相同；生产result仍completed且HEAD从tip变base，cargo101。Spec的rename探针失败因Git本身未判rename，不能当缺陷或通过，原件保留。Codex直接修复将本地远端跟踪引用集合纳入共用写确认上下文；这些引用共同决定已发布警告及来源后缀，原确认贯穿准备、保存与实际写入。无网络操作，不更改用户Git配置。


### Codex 对 review20 的修复（待全新两轴及真实桌面）

Standards01：共用写确认 context 纳入本地 refs/remotes 的完整名称与对象OID集合。任一本地远端跟踪引用都可能改变提交已发布判断、额外警告与来源后缀，变化后保守拒绝旧确认，不访问网络或写Git配置。初始preflight、文件/index指纹、PreparedState准备/自动保存及持久历史序列沿用同一context，避免只在打开确认页时核对；不将保存后的引用现场重新当作已授权状态。

新增3项真实Git回归，共10场景：四种reset模式在引用新增/推进后均拒绝旧确认，HEAD/index/文件不变且未建回收，新的published=true确认可正常执行各模式；cherry-pick/revert/reword/interactive四类历史动作也拒绝已过期的来源/警告凭据；interactive/reword自动保存中的真实reference-transaction hook推进远端跟踪引用后，保留stash/关联与原字节，不开始rebase，返回待核对。真实reset拒绝载荷导出并输入现有Node格式化链，同一错误中英切换均正确，英文不含产品中文。

集中检查原件 `codex-checks-yf3rkhgl/repair20-*`：首轮完整Rust因新增测试的两个不同长度字节数组分支E0308未编译，只改测试为slice。第二轮先通过lib271及前六组集成，M6 110/111；唯一失败为既有来源后缀用例在删除引用后复用旧确认，新增保护正确拒绝。增强该用例：先断言旧请求拒绝，再以新preflight执行并保留原来源后缀断言，专项1/1通过；尚未执行的后五组集成及doc另跑通过。分批合计672通过、0未解决失败、1既有ignored，M6共111项有成功证据；不能称首次完整退出0。所有编译/断言失败原件保留，未改生产源码来满足旧测试。

完整Node465/465、lint、x18n0错误64 unused-key警告（双语1687键完整）、extract64候选0中文均退出0。前端src/locale与repair18构建通过时逐文件字节相同，无IPC类型变化，未重复未受影响build/tsc；不冒称本轮重新执行。依赖清单/锁文件/x18n metadata同HEAD。Rust测试生成28项绑定行尾空白差异，先保存原件于impl/repair20-bindings-whitespace-eyb41l_8，逐行rstrip相同后恢复HEAD格式，未改绑定语义；最终Node在恢复后执行。

`repair20-final-source-proof.json`证明最终全部源码与最后Node/doc一致、生产Rust与第二轮完整检查及专项/剩余检查一致；原M6测试仅上述旧确认用例的加强，反向重建字节匹配第二轮元数据。28绑定纯格式证明、旧前端构建输入相同证明一并保留。检查后无源码修改或helper清理。

整包仍未完成；下一步固定候选并启动全新独立两轴完整审查，通过后固定Release与真实桌面/Rebased/中英文窄窗。不续Pi、不第二正式返修、不另派实施、不推送/PR/提前合main或删worktree。


### 全新独立 review21：固定26b68d1...ec8d7a1（未通过）

两轴均已结束，原件`/private/tmp/revia-m6-batch-ig_nawgt/review-21-338dca8c`。分别保留原文。

#### Standards

# Standards review21

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...ec8d7a1be9dca426d5753bf1b230eca37d132d6c`，28 个提交、168 个路径。

硬违反 **1**；判断项 **0**。

- **[P2] ST21-01：中性进程 locale 覆盖中文系统语言。** `src-tauri/src/settings.rs:825–828` 在原生系统检测前，将非空的 `LC_ALL=C.UTF-8` 直接判为 `en`。违反 `docs/README.md:197` Q164 与 `docs/milestones/m6/spec.md:155` L01：“首次无语言设置时，系统语言为中文则简体中文”。中文 macOS 从带该环境的启动器/终端首次运行会显示英文。独立 HOME/appdata/真实 Git 临时仓库中的生产函数复现：原生 `AppleLanguages=zh-Hans-CN`，实际返回 `en`；断言期望 `zh-CN`，退出 **101**。首次选择应依据原生首选语言，不能把 C/POSIX 当显式英文。

完整规则来源、逐路径覆盖和原始复现证据见 [standards-evidence.md](standards-evidence.md)。未将未完成的 Release/桌面验收计为缺陷。临时测试入口已移除，产品、语言资源与绑定未改动。


#### Spec

# Spec

固定范围：`26b68d1...ec8d7a1`，28 个提交、168 个路径；覆盖 C/S/O/H/F/T/L/D/V。

- **[P2] 监视失败详情未同步迁移为结构化错误。** `src-tauri/src/watch.rs:46` 已将 `WatchStatus.error` 改为 `AppErrorPayload`，但 `src/lib/ipc.ts:87`、`src/App.tsx:229,1069` 及 `src/components/app/Shell.tsx:237,263` 仍按字符串接收并直接设置 `title={watchError}`。规格 L03（`spec.md:170–171`）要求“前端唯一生产格式化链路按当前语言呈现”，并明确“不以 `[object Object]` 显示错误”。当文件监视安装/运行失败时，状态栏的具体路径和系统原因变成 `[object Object]`，中英文切换均无法呈现详情。该缺陷由本轮 Rust 契约迁移引入；手写 IPC 旧类型掩盖了跨端不匹配，非 tsc 可直接捕获的问题。应同步载荷类型并在展示处调用 `formatAppError`。

已用独占真实 Git linked worktree，经生产会话与原生监视入口复现，再把实际载荷交给生产 `StatusBar`、React 和 i18n 运行时：中英文 HTML 均出现 `title="[object Object]"`。Rust 与 Node 复现断言退出均为 0，原始记录见 `spec-evidence.md` 和 `spec-watch-ex2zh19_/`；临时 Rust 入口已移除。

其余规格项未发现确定的新违约或未经授权的扩范围。固定 Release、真实桌面/Rebased 与 D06/Q98仍归规划方验收，不计产品缺陷；本轮未宣称全套检查通过。


规划方核对：原生AppleLanguages为zh-Hans-CN，但中性LC_ALL=C.UTF-8使生产system_language返回en，真实断言101；WatchStatus.error实际为对象，前端手写IPC/状态及title仍声明string，生产StatusBar的中英HTML均含[object Object]，复现exit0表示缺陷确认，不是验收通过。Codex直接修复平台原生首选语言与完整结构化监视错误接线。


### Codex 对 review21 的修复（待全新两轴及真实桌面）

Standards01：系统语言检测集中到platform/language.rs，macOS优先使用NSLocale.preferredLanguages的首个界面语言，不把地区格式AppleLocale或中性进程locale当显式英文。仅当无原生语言时按环境变量回退，C/POSIX及其编码后缀跳过；中文标签映射zh-CN，其余实际语言按Q164用en。保留settings公开入口，用户已保存语言设置不改。macOS目标新增已在锁定依赖图中的objc2-foundation0.3.2直接依赖及最小NSLocale/NSArray/NSString features；Cargo.lock只为Revia依赖列表增加该项，无任何包版本变化。原生调用在autoreleasepool内，未启动shell或改系统偏好。

依据[Apple preferredLanguages官方文档](https://developer.apple.com/documentation/foundation/nslocale/preferredlanguages)，原始页保存在impl/repair21-apple-preferred-languages.json；Grok合成搜索超时，来源fallback后回到Apple原文核对，本地0.3.2生成API源码也已读取。离线cargo metadata原件repair21-native-dependency-metadata，不是测试或依赖升级。

Spec01：WatchStatusPayload.error、App watchState及StatusBar prop同步为AppErrorPayload，结构化原因贯穿IPC与现有状态，仅title展示时走formatAppError。生产状态栏与原生watch失败载荷的Node回归在同一对象上切中英，具体路径和系统原因保留，不显示[object Object]。未改变watch失败/部分安装的后端语义。

必要回归：平台纯选择测试覆盖中文原生语言与C/英文环境冲突、英文/法文原生语言与中文环境、C/POSIX回退；独占HOME/appdata的真实子进程设置所有进程locale为C/POSIX，独立defaults读取AppleLanguages作为原生偏好对照，生产函数实际zh-CN，与本机zh-Hans-CN一致。独占真实Git linked fixture移走自己的私有元数据，通过实际WatchController/原生监视触发错误，输出实际载荷到.watch.json，再输入生产StatusBar/React/formatter；这些是回归，不是GUI验收。

集中检查原件codex-checks-yf3rkhgl/repair21-*：首次frontend因IPC类型缺少import失败，补齐后受影响build/tsc重跑通过；首次Rust已确认语言测试通过，但新增watch测试误断言active必须false，实际部分根仍active且error存在（原review载荷也如此）。仅将测试改为核对结构化错误、原目标和缺失元数据路径，未改产品来满足错误预期。最终native筛选lib2+M6 3，共5项顶层测试通过；子进程内1项执行不重复计数。本轮未重跑完整Rust或完整M6，不借旧672项声称本轮全套通过。

最终完整Node466/466、frontend02含tsc/build、lint02、x18n02、extract02均退出0。双语各1687键完整、0错误64 unused-key警告；extract64候选0中文。无locale或x18n metadata改动，无生成绑定漂移。首轮TS/测试预期失败及审查失败原件保留。repair21-final-source-proof.json逐文件核对最终全部源码与末轮6检查一致，并固定Cargo清单/锁文件及真实watch载荷hash；检查后无源码修改或helper清理。

整包仍未完成；下一步固定候选、全新独立两轴完整复核，通过后固定Release与真实桌面/Rebased/中英文窄窗。不续Pi、不第二正式返修、不另派实施、不推送/PR/提前合main或删worktree。


### 全新独立 review22：固定26b68d1...ecc736b（未通过）

两轴正式报告均已收齐并结束，原件`/private/tmp/revia-m6-batch-ig_nawgt/review-22-exynt4kn`。分别保留原文。

#### Standards

# Standards

固定范围：`26b68d1...ecc736b`，29提交、169路径。硬违反 **1项**；判断项 **0项**。

- **[P2] 手动保存 stash 绕过符号链接只读边界** — `src-tauri/src/stash.rs:479,501–502`。`docs/product-direction.md:59,62`规定符号链接不允许写操作且只读限制优先；M6 `spec.md:54`明确沿用该边界。把已跟踪的`link → a`在外部改为`link → b`后，快照正确标记`readonlyReason="symlink"`，但保存入口只核对允许读取symlink的指纹，随后执行`stash push`。本轮真实Git、生产`stash::save`复现返回成功并把链接改回`a`：应用修改了它宣称只读的路径。应在保存前拒绝本次会清理的只读路径。此文件为本轮新增，并非继承问题。

独立定向复现编译成功，业务断言失败，真实退出码101；源码、SHA、精确命令及完整原stdout/stderr在`standards-repro-symlink.*`，临时测试入口已移除。完整规范、全路径覆盖及边界说明见`standards-evidence.md`。没有把待做的Release/真实桌面验收或工具已负责的问题计入发现。


#### Spec

# Spec review22

固定范围 `26b68d1...ecc736b`，29提交、169路径；覆盖C/S/O/H/F/T/L/D/V。发现1项新增规格缺陷。

## [P2] 显式恢复自动 stash 须在回收后重验原确认

位置：`src-tauri/src/autostash.rs:384–394`。

规格 `docs/milestones/m6/spec.md:47` 要求“执行前绑定并重验……确认条件”，且“刷新……后旧确认失效”。这里仅入口核对UI token，之后保存的是不含确认代次的文件指纹；回收期间刷新后仍直接执行恢复。

隔离真实Git生产入口复现：先保存自动stash并读取确认，在RecoveryStore现有时钟接缝触发session.refresh；代次由`(1,0)`变为`(1,2)`，仍返回`restored=true, removed=true`，文件由`base\n`变为`saved local\n`，stash被删除。用户刷新使旧确认失效后，原请求仍产生工作区写入及条目清理。需将原确认代次/条件贯穿回收与关联落盘，在Git写入前重验。

复现真实退出101，完整源码、精确命令、原stdout/stderr和保留fixture见[spec-evidence.md](./spec-evidence.md)。唯一临时测试入口已移除，产品/资源/绑定未改。

其余范围未发现可确定的新缺陷或扩范围。完整静态覆盖见证据；未重跑全套、Release或真实桌面，后两者仍归规划方验收，不列为产品缺陷。


规划方核对：Standards真实生产stash::save成功，link由b回a，cargo101；Spec回收时两次session.refresh使代次(1,0)变(1,2)，旧请求仍restored/removed=true，cargo101。原stdout/stderr、源码与精确argv已核对，失败原件保留。Codex直接修复手动stash的实际tracked只读边界，以及显式恢复确认贯穿回收/关联落盘至Git写入；拒绝尚未开始的恢复时保留可重新确认的Pending状态。未续Pi或派实施代理，尚无Release/真实桌面通过。


### Codex 对 review22 的修复（待全新两轴及真实桌面）

Standards01：手动stash保存前、真正push前均读取实际tracked状态并拒绝只读类型，覆盖符号链接含删除/类型替换、子模块gitlink和不可无损路径；未跟踪项不加入此保护集合。未把限制塞进共用preflight，因此不影响只读列表、未触及的干净tracked/未跟踪symlink或只修改stash集合的归档清空。继续不使用-u/-a，不改Git配置。

Spec01：显式恢复的原UI确认保留到回收完成及Applying关联落盘之后，临Git apply再重验确认代次、原stash列表、HEAD/context和文件/index状态；不再以同名局部raw指纹遮蔽原token。Applying落盘后若尚未开始Git便拒绝，成功撤回回执到Pending，让用户刷新重新确认；若撤回失败，则保留结构化附加原因和NeedsVerification。内部自动操作恢复的既有授权与不重复apply契约保持。

现有真实Git回归新增4项/14场景：8组symlink变更×keepIndex均拒绝，HEAD/index/原链接或字节保留且未启动stash create/push；2组正向普通保存保留clean tracked与untracked symlink，并在tracked link另变脏后仍可归档清空集合；gitlink与原始非UTF8 index路径各拒绝，UTF8同名替代文件不动；回收两时钟阶段各刷新使旧恢复请求拒绝，关联Pending、文件/index/HEAD/stash不变，保留回收，随后新确认正常恢复与归档清理。没有新测试框架或模拟Git。

原件codex-checks-yf3rkhgl/repair22-*：首次M6编译因新增测试误用ChangesSnapshot.status失败，只修为entries；第二轮完整117项为115通过/2失败。新增非UTF8文件创建先受sandbox EPERM，精准升级后得到macOS本身Illegal byte sequence，改用既有回归的真实Git原始字节index入口，不跳过断言；专项通过。另一个既有信号用例因sandbox拒绝/bin/ps未实际发信号，仅该用例精准升级定位自身difftool进程后通过，生产源码与断言未改。分批117项有成功证据、0未解决失败，不称首次完整退出0。全部失败原stdout/stderr、SHA/argv/exit保留，前两版测试反向重建hash与原metadata逐字一致。

完整Node466/466、x18n双语1687键0错误64 unused-key警告、extract64候选0中文均退出0。前端/工具/locale880文件与repair21 build/tsc/lint输入完全相同，未重复未受影响构建与lint；Rust其他套件未重复。依赖/锁文件/locale/x18n元数据与HEAD一致，无生成绑定漂移。repair22-final-source-proof.json核对所有937源码文件，当前全部生产源与完整M6及末轮检查一致，差异仅上述平台测试修正；检查后无源码/helper清理。

整包尚未完成；下一步固定新候选、全新独立两轴完整范围审查，通过后固定Release与真实桌面/Rebased/中英文窄窗验收。Pi保持停止，不第二次正式返修、不另派实施、不推送/PR/提前合main或删除worktree。


### 全新独立 review23：固定26b68d1...8063e85（Spec未通过）

两轴已完成并结束，完整原件`/private/tmp/revia-m6-batch-ig_nawgt/review-23-fxdx6hg5`，分别保留原文。

#### Standards

# Standards 独立审查 23

固定 `26b68d1…8063e85` 的完整三点范围，共 169 路径；禁止阅读的两份验收/报告正文仅登记对象元数据，任务单只读 Comments 之前。

**硬性规范违反：0。**

**启发式判断：1（P3，非阻断）。** `src-tauri/src/rewrite.rs:675` 与同文件 `:621` 的 cherry-pick/revert 重复执行选择校验、dirty 准备、写前复核、序列创建及快照收尾。任务指定 Fowler 的 Duplicated Code 建议抽取共享形状；将来调整同一门禁须同步两份。建议后续抽取公共 runner，保留各操作的 source/keep-empty 策略。这是本次新增代码的维护性判断，非仓库硬规则，也未证明功能或安全缺陷。

静态审查；本轴未运行构建、测试或复现实验，未改产品。Release 与真实桌面验收仍由规划方完成。完整覆盖和原始记录见 `standards-evidence.md`。


#### Spec

# Spec 独立审查 23

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...8063e85ce739ffaf56c12447f6d2239e672020e8`，169 个路径；覆盖 C/S/O/H/F/T/L/D/V。发现 1 项本轮引入的规格偏差。

- **[P2] stash 归档误绑定原 worktree，阻断同仓库取回** — `src-tauri/src/stash_archive.rs:288–294`（列表同样在 266–267）。S03（spec.md:79）规定“恢复为 stash 条目只放回原仓库列表”；C02（:57）明确两种恢复语义不混用，C01（:48）明确 stash 跨 worktree 共享。这里复用完整 `WorktreeIdentity::confirmed_match`，将 worktree 路径和私有 gitDir 也作为归档恢复条件。同仓库从 A 切到 B 后，归档按钮禁用，后端返回 `archiveBelongsAnotherRepository`，即使 commonDir 的路径及文件系统身份完全相同；移除 A 后也失去应用内取回入口。应按原仓库身份核验 stash 归属，保留当前目标的写门禁。

独占真实 Git fixture 经生产 `stash::drop` / `StashArchiveStore::restore` 复现；规格期望断言失败，Cargo 退出 101，非编译失败。原始输出、精确 argv、SHA 及 fixture 见 `spec-evidence.md`。临时测试入口已移除，工作区干净。

其他领域未确认具体缺陷。未启动 GUI、未运行全套检查，不将后续 Release、桌面、D06/Q98 验收列为产品缺陷。未读取旧审查、Comments、report.md、q173-validation 或另一轴。


规划方核对：真实生产drop后stash列表为空，A/B的commonDir路径与filesystemIds[2]完全相同，但B列表identityConfirmed=false、restore返回archiveBelongsAnotherRepository，实际cargo101。复现源码/argv/SHA/原stdout/stderr和fixture完整保留。Codex直接修复stash归档的仓库归属判断，普通文件回收仍绑定原worktree；P3流程重复为非阻断维护建议，本轮不重构历史执行链。尚无Release/真实桌面通过，不续Pi或派实施。


### Codex 对 review23 的修复（待全新两轴及真实桌面）

Spec01：stash归档列表和恢复统一按原仓库canonical commonDir及其已知文件系统身份确认归属，允许同仓库其他worktree取回，包括原worktree已正常移除。未知/缺失commonDir标识、克隆仓库或同路径新建仓库仍拒绝；没有把普通文件RecoveryStore的完整worktree身份检查放宽，也没有改变归档格式或14天保留期。当前执行目标继续走会话非排队写门和完整require_identity。

规划方补充A1：在核对“保留当前写门禁”时发现原命令只在调用restore前验一次UI token，真实fetch与store之间不再重验。将原token传入restore_confirmed，在锁内、fetch前及实际stash store前重验原确认与当前会话身份；受控after-fetch接缝只观察真实Git取回完成阶段，不替换Git。归档导入仍只写共享stash列表，不应用工作文件或index，内部无UI确认的既有restore入口仍保留当前身份复核。

必要真实Git回归新增3项：原worktree删除后从存活linked worktree恢复，原仓库所有worktree都可读同一OID，原工作文件、部分暂存index、HEAD和未跟踪文件不变，普通文件回收在另一worktree仍不可恢复；克隆、未知commonDir ID、同路径重建仓库及旧会话均拒绝；真实fetch后仅刷新或外部改字节两档均在store前拒绝，归档保留，重新确认后可取回且不改变文件/index/HEAD。未引入新测试框架或模拟Git。

Standards的P3重复流程判断作为非阻断维护建议保留。本轮不重构cherry-pick/revert执行链，也不更改已确认规格。

集中最小相关检查原件codex-checks-yf3rkhgl/repair23-*：首次Rust因归档summary先移动String再借用整record触发E0382，未运行测试；已保存原生产文件与metadata，先计算identity_confirmed再移动字段，受影响stash_archive筛选8/8通过，0失败/忽略。当前M6共有120项，本轮未重跑完整M6或Rust其他套件，不将以前117项说成本轮120项全套通过。

末轮x18n02双语各1687键、0错误64 unused-key警告；extract02 64候选0中文，两命令退出0。前端/工具/locale880文件与既有build/tsc/lint及Node466/466输入完全相同，IPC类型、依赖/锁文件、locale/x18n metadata均未变，本轮未重复这些未受影响检查。repair23-final-source-proof.json核对最终937源码文件与末轮三检查完全一致；无生成绑定漂移，检查后无源码/helper清理。所有审查与编译失败原件保留。

整包仍未完成；下一步固定候选、全新独立两轴完整范围复核，通过后固定Release与真实桌面/Rebased/中英文窄窗验收。不续Pi、不第二正式返修、不另派实施、不推送/PR/提前合main或删除worktree。


### 全新独立 review24：固定26b68d1...d36b49b（未通过）

两轴均结束，完整原件`/private/tmp/revia-m6-batch-ig_nawgt/review-24-n8ehad12`，分别保留原文。

#### Standards

# Standards

固定范围：`26b68d1…d36b49b`，169 路径、31 提交；按任务排除旧审查、Comments、report 与 q173-validation 正文。明确违反 **1**，启发式判断 **1**。

## 明确违反

- **[P2] 保留实时输出监听错误的语义参数** — `src/App.tsx:2576`。`docs/milestones/m6/spec.md:170` 规定“同一已存在错误切换语言后也能正确呈现”；`docs/ui-direction.md:177–179` 要求即时切换产品文案。这里先读取 `messages.commit.outputLive`，再把结果存成错误的 `what` 参数；`localizedError` 将参数字符串化，`formatAppError` 仅重新翻译外层模板。中文界面发生 `COMMIT_OUTPUT_EVENT` 监听安装失败，再切英文，同一错误会呈现 `Failed to install 实时输出 protection: …`；反向切换同样残留英文。应保留监听类型/子错误代码，展示时才翻译名称；原始失败详情也应保留结构化载荷。该行是本范围新增迁移，非继承问题。

## 启发式判断

- **[P3，可能的 Duplicated Code] 合并 blame 读取生命周期** — `src/features/m6/FileHistoryDialog.tsx:103–153`。`runBlame` 与选项变化 effect 重复同一 `ipc.fileBlame` 参数、序号、loading、成功与错误处理。两份已出现差异：前者成功清错并在失败时通知，后者成功不清错、失败不通知。后续调整迟到响应或报错行为须同步两处。可提取共用读取函数，由入口决定是否切页签。这是 Fowler 判断，非项目硬规范违反。

逐路径覆盖、完整规范清单和静态证据见 `standards-evidence.md`。本轮未运行测试、Release 或 GUI；待桌面验收不计产品缺陷。


#### Spec

# Spec 审查（第 24 轮）

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...d36b49b1481e294d52a6b961026fa3a33bda12bc`，169 个路径；按原规格核对 C/S/O/H/F/T/L/D/V。发现 **1 项 P2**。

## [P2] 首次 blame 在途时切换开关不会刷新结果

位置：`src/features/m6/FileHistoryDialog.tsx:131–134`。

规格 `docs/milestones/m6/spec.md:141`（F02）要求：“开关变化后刷新对应身份的缓存与结果。”首次 blame 请求尚未返回时，`blame` 为 `null`，用户可切换三项开关，但 effect 在递增请求代次前就返回。原请求仍会被接受，后续也不会按新开关重读。

已使用独占真实 Git fixture 验证：首行只有空白修改时，忽略空白归属 base，不忽略则归属新提交。组件先请求 `ignoreWhitespace=true`，在途改为 false 后仍仅有一次请求，最终开关为 false，却显示 base。结果持续错误，直到手动刷新或再次切换。应以正在请求的版本和开关驱动读取，并使旧参数回复失效。

Rust 生产入口生成载荷检查通过；消费实际载荷的生产组件时序回归失败（退出 1）。完整 argv、SHA、stdout/stderr 和逐项覆盖见 `spec-evidence.md`。

未修改产品、资源或绑定；临时测试入口已移除，工作树干净。固定 Release、桌面/Rebased 对照及 D06/Q98 仍由规划方执行，未将待验收项目作为产品缺陷。


规划方核对：Standards的参数先读messages.commit.outputLive并字符串化，formatAppError只翻译外层，静态证据成立，未冒称原生监听失败实测。Spec实际Git -w与无-w载荷归属不同，生产组件在开关改false后仅一次旧参数请求且仍显示base，原Node断言exit1、Rust载荷生成exit0。原始证据均保留。Codex直接修复实时输出监听错误的语义载荷和统一blame读取生命周期；P3重复生命周期与本轮F02修复相关，一并消除重复路径，不另重构其他流程。文案用x18n CLI，尚无Release/真实桌面通过。


### Codex 对 review24 的修复（待全新两轴及真实桌面）

Standards01：实时输出监听失败保留独立语义code和toAppError(caught)结构化cause，不再把当时的messages.commit.outputLive或已格式化错误放入普通字符串参数。实际App监听catch调用commitOutputListenerFailure；完整说明在呈现时翻译，原生API原始详情与内层code/params/related仍保留。先x18n查询既有文案/目标键，再仅用CLI set新增errors.commitOutput.listenerFailed双语及对应CLI review源hash，无手工locale改动，不删除旧键。

Spec01及相关P3：blame请求意图立即保存明确的原view身份、版本与路径，由一个effect统一执行首次、刷新、切版本及三个开关读取；开关/新目标先使旧请求代次失效，不再以已返回blame是否存在决定重读。读取生命周期共用loading/成功清错/失败语义通知，unmount取消迟到回调；切到另一版本时不再被上个结果的路径/版本覆盖。语言切换不进入请求依赖，不重挂或清空已有结果。

必要回归：现有M6真实Gitfixture新增1项，中文空格文件仅改首行空白，3种目标（新提交/原提交/工作副本）×8组-w/-M/-C组合，24个实际生产载荷及Git命令日志导出给现有Node组件harness；实际归属base/new差异已断言。生产FileHistoryDialog新增6项时序回归：三个开关分别在首次读取在途时重读、旧响应后到不覆盖；请求新版本同时再切开关仍绑定新目标；失败统一通知/成功清错；关闭后迟到失败不通知。VM执行真实App中的监听effect，传输拒绝采用实际Rust错误载荷/原始API Error，新增2项验证同一错误双向切语言及卸载后不更新。这是组件/错误回归，不冒称原生监听失败或桌面验收。

集中相关检查原件codex-checks-yf3rkhgl/repair24-*：真实Git专项1/1（内含24组合）、完整Node474/474、frontend含tsc/build、lint、x18n及extract均退出0，本轮没有新增验证失败，review24原失败保留。双语各1688键完整、0错误65 unused-key警告；多出的旧close.listenerFailed已不用，按约定保留不删；extract64候选0中文。生产Rust与HEAD完全相同，仅新增已有测试内的载荷fixture，未重跑完整121项M6或Rust其他套件。

repair24-final-source-proof.json核对最终937源码文件与6项末轮检查逐字一致，并记录实际blame载荷、CLI locale写入及.x18n/reviews.yml hash；依赖清单/锁文件未变，无绑定漂移。检查后不修改源码或清理helper。构建是前端构建，不是固定Release/真实桌面通过。

整包仍未完成；下一步固定新候选、全新独立两轴完整范围复核，通过后固定Release与真实桌面/Rebased/中英文窄窗。无Pi续接/第二正式返修/另派实施/推送PR/提前合main或删worktree。


### 全新独立 review25：固定26b68d1...7d78f4c（Standards未通过）

两轴均结束，原件`/private/tmp/revia-m6-batch-ig_nawgt/review-25-gl3dny9n`，分别保留原文。

#### Standards

# Standards 独立审查 25

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...7d78f4cd9c83cd510ee8d8c3acea6df81e884386`，169 个路径；按指示不读取旧审查、任务 Comments、report.md、q173-validation.md 或另一轴。

## 硬违反：1

- **S25-H1 / P3：危险按钮前景色绕过主题变量。** `src/features/m6/OperationBanner.tsx:159`、`HistoryActionDialog.tsx:410`、`StashPanel.tsx:466,491` 新增 `bg-destructive ... text-white`。`docs/technical-direction.md:34` 明确要求“颜色一律定义为 CSS 变量”。打开中止、Hard reset、删除或清空确认时，背景取主题色而文字固定白色；当前 `dark-2026` 背景 `#f48771` 配白字的静态对比为 2.456。应将危险按钮的配色集中为主题变量。旧 `Dialogs.tsx:121` 的同型写法是继承项，本次仅计新增四处为一个问题。

## 判断项：1

- **S25-J1 / P3，possible Duplicated Code：** `src-tauri/src/rewrite.rs:605` 与 `:659` 的 cherry-pick / revert 重复实现锁、非空选择、操作门禁、HEAD/context 核对、OID 规范化、线性检查、prepare_dirty、序列建档和快照收尾。共同安全校验再变更时必须修改两条路径，容易漂移。建议提取共有的准备及序列启动流程，保留来源后缀、空提交策略和操作类型差异。此项是 Fowler 启发式，不是硬违反。

未发现其他足够具体的新增规范问题。完整规范来源、12 项 baseline 结论、169 路径覆盖与原始证据见 `standards-evidence.md`。未运行构建或测试、未启动 GUI、未改产品文件；不把待做 Release / 桌面验收计作缺陷。


#### Spec

# Spec 独立审查 25

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...7d78f4cd9c83cd510ee8d8c3acea6df81e884386`。

**可报告发现：0。**

按原始整包派发、现行 M6 规格、Q155–Q173、ADR-0010/0012/0013/0014、文案规则和 backlog 07，独立核对了 C/S/O/H/F/T/L/D/V。169 个变更路径已归类覆盖，未只审查最后一个提交；禁止读取的报告、验证结论和 Comments 内容未作为依据。

本轮重点追踪固定目标和确认代次、stash 出现项身份与独立归档、自动保存与恢复收尾、持久历史序列、编辑停点、文件历史生命周期与 blame 请求身份、外部工具写前保护，以及已有错误和通知的即时翻译。未确认可复现的新增规格偏差、范围扩张或错误行为。

本轮是源码审查，未运行完整测试、构建或 GUI；没有将现有测试源码当作执行通过证明。规划方固定 Release、真实桌面/Rebased、英文窄窗及最终长测仍需另行验收，不作为本次产品缺陷。详细覆盖和证据见 `spec-evidence.md`。


规划方核对：四处新增按钮text-white固定前景，而bg-destructive来自当前主题conflictingResourceForeground，静态配色证据成立；Spec为0。Codex直接修复四处主题前景变量，按四套内置主题计算实际配色并运行最小前端构建。流程重复P3仍为非阻断维护建议，不重构历史执行。尚无固定Release/真实桌面通过。


### Codex 对 review25 的修复（待全新两轴及真实桌面）

Standards H1：在共用语义CSS映射中新增destructive-foreground，取当前主题input-background作为危险背景上的反色文字；四处M6新按钮统一使用text-destructive-foreground，去除text-white。背景仍是当前主题conflictingResourceForeground，全部颜色来自既有主题变量，没有修改主题源文件、生成器或继承的普通回收对话框。

四套内置主题的静态配色核对原件repair25-theme-colors：light-2026/one-light均为#ad0707配#FFFFFF，对比7.472；dark-2026为#f48771配#191A1B，从固定白字2.456改善为7.096；one-dark-pro为#e4676b配#1d1f23，从3.255改善为5.070。这是当前主题源的数值核对，不是桌面视觉验收。

最小相关pnpm build含tsc退出0，编译CSS实际含text-destructive-foreground及主题变量映射；本轮没有新增测试或验证失败。只改CSS两行和四处className，Rust/Node/lint/x18n未重复执行，原检查保持原时间/范围，不把此前474项等结果说成本轮重跑。locale、x18n元数据、依赖和锁文件均未变。repair25-final-source-proof.json核对937源码文件与构建/配色原件完全一致，并记录只有四个样式文件相对上轮Node输入变化；检查后无源码/helper清理。

Spec25原报告0项；Standards J1的非阻断流程重复继续保留为维护建议，本轮不重构历史执行。下一步固定候选、全新独立两轴复核，再推进固定Release与真实桌面/Rebased/中英文窄窗。整包仍未完成，无Pi续接/第二正式返修/实施委派/推送PR/提前合main删worktree。


### 全新独立 review26：固定26b68d1...b170435（Standards未通过）

两轴均已结束，原件`/private/tmp/revia-m6-batch-ig_nawgt/review-26-g6ydki4j`；分别保留原文。

#### Standards

# Standards

固定范围：`26b68d1...b170435`，170 个路径；完整覆盖与禁读边界见 `standards-evidence.md`。

## 硬违反：1

- **[P1] stash 保存部分清理后仍报普通失败** — `src-tauri/src/stash.rs:520–526`。`docs/technical-direction.md:55–56` 要求结果不确定时“操作标为结果待核对”，且不能静默退出。真实 Git fixture 配置必需 smudge filter 失败后，`stash push` 退出 128，但已新增 stash、复原 `a.txt` 并移除 `b.txt`；这里仍返回 `ErrorKind::Git`。`src/App.tsx:4734–4746` 因而以 `needsVerification=false` 收尾，丢失应有的待核对与关闭保护。写命令已启动后的非零结果应核对副作用，不能直接认定普通失败。唯一临时生产入口测试退出 **101**；原始输出、完整源文件与实际 fixture 均保留。

## 判断项：1

- **[P3] possible Duplicated Code** — `src-tauri/src/rewrite.rs:621–655` 与 `675–704` 重复 `ensure_no_operation → ensure_empty → require_expected_head → require_context → normalize/require_linear → preflight/prepare_dirty → create/run → finish_snapshot`。新增确认或收尾规则时必须同步维护两份，容易漂移。建议抽取共同序列启动流程，保留 cherry-pick 的来源后缀与空提交策略差异。这是 Fowler 启发式判断，**不是文档硬违反**。

本轴共 1 项硬违反、1 项判断；最高 P1。未把待做 Release/真实桌面验收计为缺陷。


#### Spec

# Spec 独立审查 26

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...b170435848c9b5f8ec7cf7e9df16a7267e12f55b`，170 个变更路径。

结论：**0 项可确认的产品缺陷**。已按原始整包任务、M6 规格全文、产品方向、文案规则、相关 ADR 与 backlog 07，覆盖 C / S / O / H / F / T / L / D / V；没有发现能给出具体触发条件及错误后果的遗漏、实现偏差或范围扩张。

重点核对了共享 stash 出现项身份、独立归档、删除事务、应用／恢复确认、历史批次的部分回执和停点、固定版本文件历史、外部工具收尾、结构化错误及语言切换、日志日期共享列宽。

本轮为独立静态审查，没有运行复现测试、完整套件、Release 或真实桌面；现有测试源码仅作为覆盖和生产入口证据，不宣称其本轮通过。固定 Release、英文窄窗、Rebased 运行时对照、D06 / Q98 与真实桌面仍由规划方另验，未把待验项目计为产品缺陷。

未读取旧审查、任务 Comments、report.md、q173-validation.md 或另一轴结果；未修改产品、locale 或绑定。完整覆盖和原始静态读取记录见 [spec-evidence.md](spec-evidence.md)。


规划方核对：真实Git stash push退出128却已新增stash、复原a.txt并移除b.txt，生产普通Git错误使关闭保护丢失；原始argv/exit/stdout/stderr及固定SHA已核实。Codex直接修复启动写入后的非零收尾，保留结构化原因并标待核对，不盲目重试。P3重复流程仍为非阻断维护建议。尚无固定Release或真实Revia桌面通过。


### Codex 对 review26 的修复（待全新两轴及真实桌面）

Standards P1：手动stash push已启动后的超时或非零退出统一通过recovery::pending_write保留NeedsVerification与failedSaveStash结构化cause，原始Git详情不扁平化，不自动重跑；保存后的stash与当前工作区留待核对。Spec原报告0；Standards重复流程P3继续作非阻断维护建议，不扩大重构。

现有真实Git回归新增1项含2场景：普通/keep-index保存遇必需smudge过滤器失败，实际push非零且已保存完整a/b内容、a已复原而b缺失；旧stash仍在，HEAD不动，同旧请求拒绝重放且push仅一次。生产载荷与完整命令日志导出.stash-save-partial.json，两个独占fixture保留。Node新增1项消费这两个实际载荷，执行原App performM6Write函数及生产WriteGuardOrchestrator/CloseCoordinator，核对原worktree待核对、真实收尾只刷新不重写、同错误即时中英格式化并保留Git原文；这是回归，不冒称桌面验收。

集中最小检查原件codex-checks-yf3rkhgl/repair26-*：相关manual_stash筛选6/6通过，Node双脚本209/209通过，x18n双语各1688键0错误65未用警告、extract64候选0中文。首轮Node208/209，唯一失败是新增测试猜错既有英文“Verify the result”措辞；已保存失败源码与原stdout/stderr/exit，只修测试英文预期后受影响Node重跑通过，未改locale或产品迎合断言。未重跑完整122项M6或其余Rust/完整Node/build/lint；生产前端/locale未变，旧结果保持原时间/范围。

repair26-final-source-proof.json记录最终937源码hash及本轮四检查，Rust/x18n/extract后唯一变化为上述Node测试英文预期，未影响其输入；最终Node与所有源完全一致。依赖/锁文件/locale/x18n元数据无改动，无绑定漂移；检查后无源码/helper清理。当前仍未固定Release、无真实Revia桌面通过，不续Pi/第二正式返修/新实施代理，不推送/PR/提前合main或删worktree。


### 全新独立 review27：固定26b68d1...2ae2e5f（源码两轴通过，待真实桌面）

两轴均已完成并结束。原件`/private/tmp/revia-m6-batch-ig_nawgt/review-27-q9lkyfsu`。

#### Standards

# Standards 独立审查 27

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...2ae2e5fc693150864f811982ea27ef890cf8de6f`；34 个提交、170 个路径。

硬性规范违反：**0**。

启发式判断：**1（P3，非阻断）**。

- **ST27-H01：集中两种历史序列的共同启动流程。** `src-tauri/src/rewrite.rs:675` 起的 `revert` 与同文件 `:621` 起的 `cherry_pick` 重复了操作门禁、HEAD／上下文校验、OID 规范化、线性检查、回收准备、准备后重验、计划创建、执行和回收收尾。它符合本轮 Fowler 基线的 Duplicated Code：“the same logic shape appears in more than one hunk or file”。当修改这些共同安全步骤时，目前必须同步编辑两份流程，形成同一契约的两个维护位置；建议抽出私有共同入口，将操作类型、来源后缀和空提交策略作为明确参数。此项是可维护性判断，不是仓库硬规则违反，也未据此声称当前有数据丢失或运行故障。

未将规划方待做的 Release／桌面验收计为产品缺陷。未运行无具体复现目的的测试，未修改产品、locale 或绑定。完整范围及依据见 `standards-evidence.md`。


#### Spec

# Spec 独立审查 27

结论：**0 个可确认缺陷**。

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...2ae2e5fc693150864f811982ea27ef890cf8de6f`，共 34 个提交、170 个路径；实际 merge-base 为 `fad6483a9dd754c35677c32741c1978d419e8b56`。当前 HEAD 与指定 head 一致，工作树干净。

依据原始整包任务、单一任务的 Comments 之前部分、M6 规格及已确认决策，覆盖 C/S/O/H/F/T/L/D/V。重点追踪固定目标与确认代次、stash 出现项和完整独立归档、自动保存关联与中断回执、历史序列续接、编辑与冲突停点、文件历史和 blame 身份、外部工具保护、结构化错误与即时语言切换、日期列约束。

未形成有具体触发条件、源码位置和规格违反结果的新增问题；不将推测、继承边界或工具可直接发现的类型/键错误列为缺陷。

本轮为静态独立审查，未启动 GUI、未修改产品/locale/绑定、未运行或宣称通过完整测试。固定 Release、Rebased 对照、英文窄窗等桌面验收仍由规划方完成；这些待做项不计为产品缺陷。

逐项覆盖和原始范围核验见 [spec-evidence.md](./spec-evidence.md)。


规划方核对：固定SHA/完整170路径diff/34提交与原始只读命令证据一致，源码无改动；Standards0硬违反/1P3非阻断判断，Spec0。P3留维护建议，不为此改写安全核心。源码复核通过，进入固定Release和真实桌面；尚不等于整包验收通过。

候选集中补充检查：M6首轮120/121、1过滤，唯一失败为并行运行时user_refresh测试等待阶段15秒超时；保持源码和断言原样，孤立专项1/1成功；需/bin/ps的既有signal精确升级专项1/1通过。分批122项有成功证据，不称首次整套退出0。完整Node最终475/475。source/argv/exit/原stdout/stderr及所有失败在candidate27-*，937源码与末轮检查完全一致。成功载荷曾被旧runner默认位置的同候选全M6更新，旧单测成功导出只余hash和保留fixture，未伪称原件仍在；全部失败原件未改，最终完整载荷另固定独占目录供Node02读取，详见candidate27-final-validation.json。后续runner支持显式输出目录，不再复用导出路径。


### 真实桌面首轮：Release27未通过（Codex直接修复）

固定源码6544e3d（产品源2ae2e5f），复制.app及binary hash、build/launch argv和原输出见独占release27-nqdfzhty。真正运行原生Tauri窗口，未使用headless或模拟IPC替代。首次主界面中文；草稿/同文件diff页签及实际滚动168–203行在中英切换后保留；默认stash保存和应用、Keep index保存已用截图/AX和原始Git事实核对。S01-preview还核实实际终端追加后原选中条目和父提交diff保持，未把新序号当旧条目。其余23组仍有待验，局部通过不代表整包通过。

- DESKTOP-L01-1 / P2：主界面中文时原生菜单仍File/Edit/View/Window/Help，实际File含Close Window/Close All。原始AX为L01-native-menu-zh.ax.txt；菜单态CUA没有截图，仅state且getScreenshot unavailable，不伪造图片。现产品无原生菜单定义，Tauri默认菜单未接语言运行时。按L01修复后重新构建/桌面重验。
- DESKTOP-S02-1 / P1：自建workflows干净tracked+未跟踪原文件，明确选Keep index stash，勾恢复暂存区/应用成功后删除，填写m6-desktop-stash后应用。实际分支创建成功、HEAD仍原0a0226d4，但tracked/index仍base、stash原三条全留、归档0，未完成应用。真实关闭保护显示一个结果待核对，已选择留在应用，未盲目重放。S02-index-branch-form/result/pending-close截图/AX、before/after原始Git facts和recovery快照已保留。after_checkout同时要求原确认代次；当前自动watch/焦点刷新与写门禁无协调，list_changes都会推进session.refresh确认代次，因此疑似自身checkout触发自动读取而使后续apply被拒。此根因仍须回归与新桌面实测核实，不能把推断写成已证明。

由Codex直接修复并验证；不续Pi、不发第二次正式返修、不派实施代理。原Release27、失败现场、修改和所有原始证据保留。修完重新固定候选并做新的独立两轴、固定Release，受影响自动化与桌面重验；当前Release不通过，不合main、不删worktree、不推送/PR。


### Codex 桌面首轮问题修复候选（尚待新两轴/新Release真实重验）

DESKTOP-S02-1：自动watch/前台刷新协调器新增可执行门禁，当前目标正在写入时只保留一个脏标记，不读取或忙轮询；写前先建立正常退出保护，再等待已发出的自动读取收尾，避免它迟到推进确认代次。写门禁释放后通过同一协调器补读。手动刷新仍走原读取与原confirmation epoch失效规则，Rust指纹/身份/确认不放宽，不跳过真实Git核对；写后固定目标真实重读保持。该候选修复自身checkout触发自动刷新与写入的竞争，实际新分支应用是否恢复仍待新Release桌面验证，未据自动化宣称通过。

DESKTOP-L01-1：新增集中平台原生菜单定义，所有菜单及已有系统角色标题按完整字面量翻译键生成；保留系统Undo/Redo/剪贴板/窗口/服务/退出角色。文件与窗口关闭通过main窗口正常close请求，沿用已有写中/待核对保护，不直接destroy或exit；macOS既有Close All显式沿同一单窗口关闭流程，其他平台保持File关闭/退出。Window/Help使用本机Tauri2.12.4默认角色ID。更新串行，排队工作执行时读当前locale，安装后释放客户端资源句柄，不重挂应用或清草稿/diff状态。locale先查询既有键/同义，22条新增双语仅经x18n CLI apply及其reviews元数据写入。x18n MCP实际initialize未响应(-32603)原件保留，CLI目标明确为本worktree配置，未误写其他项目。

必要回归新增5项：三项刷新协调器等待/合并/失败卸载释放，生产App真实工厂+写guard+补读effect VM回归，生产原生菜单定义两平台/中英和正常关闭回调回归；后两项是生产代码回归，不冒称原生GUI验收。现有真实Gitrefresh筛选2/2通过，继续断言显式刷新使未开始的历史写入拒绝；未修改Rust代码或断言。

集中检查desktop1-*：完整Node480/480、build含tsc、lint、x18n双语各1710键0错误65未用警告、extract64候选0中文、真实Git2/2均退出0，本轮无新增验证失败；既有lint/Vite警告和全部旧失败保留。未重跑完整Rust/M6；所有Rust源与candidate27的分批122项源完全相同。desktop1-final-source-proof.json记录最终938源码与六项末轮检查逐字相同、locale及CLI元数据hash；依赖/锁文件/生成绑定未改，最终检查后无源码/helper清理。

旧Release27已经正常核对并关闭（CUA库存确认Revia不运行），原.app/所有失败现场与独占fixture保留。当前仍非整包通过；接下来全新两轴完整范围复核、固定新Release、真实重验两问题，再继续剩余桌面组。无Pi续接、第二正式返修或实施委派，不推送/PR/提前合main删worktree。


### review28：独立两轴正式报告（2026-10-30）

#### Standards

# Standards

硬性违反：1。启发式判断：0。

- **[P2] 内层结构化错误被字符串化，英文仍显示中文。** `src-tauri/src/review.rs:261–266` 将 `platform::sync_directory` 返回的 `AppError` 以 `error.to_string()` 放入 `params.error`。这违反 `docs/adr/0013-localize-structured-user-errors-at-presentation.md:7`：“详细原因也必须是可识别类型与参数，不能把原来的整段中文错误塞进参数规避迁移”。本轮独占真实 Git fixture 经生产 `WorktreeSession::set_review_mark`，在应用数据目录权限为 `0300` 时触发同步失败；同一真实载荷经生产英文 `formatAppError` 得到 `Failed to sync review marks: IO 错误：Permission denied (os error 13)`。应保留结构化 `cause`，让内层 IO 错误也由当前语言格式化。这是本批双语迁移遗漏，不是类型/lint/键检查问题。

固定范围：`26b68d1...ee3301e`，36 个提交、172 条路径；按任务排除历史报告和 Comments 的内容。其余未形成具体规范违反。完整规则、路径覆盖与原始复现材料见 `standards-evidence.md`。

本结论为源码与独占 fixture 证据，不替代规划方固定 Release、真实桌面、Rebased、双语窄窗、D06 或 Q98 验收；这些待验收项未计为缺陷。


#### Spec

# Spec 审查（第 28 轮）

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...ee3301e1aa51c3dd6239b02c291c8d86d2d7761d`，36 个提交、172 条路径；覆盖 C/S/O/H/F/T/L/D/V。

## 发现：1 项

**[P2] 所有分支文件历史翻页会重复并永久漏掉提交** — `src-tauri/src/file_history.rs:167–185`，尤其第 179 行。

F01（`docs/milestones/m6/spec.md:133–135`）要求可切换所有分支，历史“遵循 Git 的真实历史”，且“读取使用固定目标、分页 / 有界缓存”。当前先按每条分支截取 `skip + limit + 1` 条，再合并按提交时间重排，最后按全局 `skip` 取页。Git 允许祖先的提交时间晚于子提交；翻页扩大的候选集会将这类祖先插入已返回的页前面，令全局偏移失效。

真实 Git 复现：两条固定分支，同一中文/空格文件，main 有 45 次变更；main-1 的提交时间为 2030，其余为 2026。按界面实际每页 40 条、同一 anchor 读取两页，原生 Git 有 47 个提交，生产接口返回 47 行但仅 46 个唯一 OID：main-6 重复，main-1 从未返回，且末页 `hasMore=false`。用户因此无法从文件历史找到漏项或查看其版本。应使用跨页稳定的全局顺序/游标，不能对逐页扩大的分支前缀重新排序后继续使用偏移。

这是本轮新增 `file_history.rs` 引入的问题。唯一 Rust 复现测试退出 101；源码、原 stdout/stderr、每次 Git argv/退出码和 fixture 均保存在同目录，见 `spec-evidence.md`。临时测试入口已移除，工作树干净。未运行 GUI/完整套件；固定 Release 与桌面验收属于后续规划方工作，不计为缺陷。


规划方已核对原始生产载荷、英文formatter输出、Rust失败exit101与固定范围。两轴均结束，解除产品冻结，由Codex直接修复；旧Release27未通过，两项桌面问题仍须新Release实测。


### Codex 对 review28 的修复（待全新两轴及新Release桌面）

Standards P2：审阅记录目录同步失败保留原AppError为cause，不再把中文IO错误拼入params.error。同一既有键的中英占位符先查询后仅通过x18n set改为cause，CLI reviews元数据同步保留。真实Git审阅会话在独占appdata目录0300权限下实际触发EACCES后立即恢复权限；原IO类型/参数跨IPC保留，生产formatter同一载荷en→zh-CN→en即时格式化，英文无中文解释。

Spec P2：全部分支各自按固定tip读取有界Git历史前缀，保留独立创建/重命名生命期；全局使用各流当前头的确定性堆归并并按(OID,path)去重，不再对逐页扩大的前缀整体按时间重排。每条流本身无重复，因此每条取skip+limit+1足够产生同样数量的唯一合并前缀；即使祖先时间更晚，也不会挤动已交付页面。补真实Git47项历史/时间倒序祖先/时间相同/共享祖先/分支后来移动，分别1、7、40分页与200单页的完整顺序逐项一致；已有同OID不同路径rename回归补limit1跨页核对，旧删除重建边界继续通过。

集中最小检查repair28-*：真实Git file_history11/11、review_marks同步错误1/1、完整Node481/481、build含tsc、lint、x18n及extract均退出0，本轮无新增失败，review28原失败全留。双语1710键0错误65未用键警告，extract64候选0中文；未修改门槛或删除unused键，未重跑完整Rust/M6（现M6共123项）。成功载荷本轮独占repair28-payloads-7zi0bqiw，承接Node其他真实载荷是desktop1原件副本，不伪称重新触发所有旧场景。repair28-final-source-proof.json核对最终938源与7末轮检查完全相同，依赖/锁文件/绑定未变，检查后无源码/helper清理。

原Release27仍不通过；桌面首轮L01/S02候选尚未新Release实测，不能沿用旧局部通过判整包完成。下一步固定候选并按code-review全新独立两轴，通过后打包新Release继续真实桌面23组。无Pi续接/第二返修/新实施代理，无推送PR/提前合main或删worktree。


### review29：独立两轴正式报告（2026-10-30）

#### Standards

# review29 Standards

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...aaf9d0763794083ee9c278e23827560e5b8a63f9`，37 个提交、173 个路径；审查开始与结束时 HEAD 均为固定版本，工作区均干净。

硬性规范发现：**0**。Fowler 启发式发现：**0**。

没有确认本范围新引入、可具体定位且有实际触发后果的规范问题。已覆盖 Git CLI / Q173 例外边界、写前身份与确认、覆盖前完整回收、待核对回执、语言呈现与领域术语、平台集中处理，以及测试隔离与 IPC 契约。大型既有模块和机械迁移本身不作为缺陷。

按正式任务，未读取旧审查、Comments、`report.md`、`q173-validation.md` 或另一轴；后两条路径只计入范围元数据，任务单只读 Comments 前正文。全路径覆盖与原始证据见 `standards-evidence.md`、`standards-raw/`。

本轴只做静态审查，未运行构建、测试或 GUI，也没有增加临时集成测试。没有把此结论解释为固定 Release、真实桌面、D06 或 Q98 验收通过。


#### Spec

# review29 · Spec

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...aaf9d0763794083ee9c278e23827560e5b8a63f9`（37 个提交、173 个路径）。结论：发现 1 项 P2，本轴尚未通过。

## [P2] 无原生停点的批次失败也应返回已完成清单

位置：`src-tauri/src/history_sequence.rs:289–292`（关联 `src-tauri/src/rewrite.rs:559–560`）。

规格 H01，第 111 行：“多提交操作保持明确执行顺序，失败 / 冲突后列出完成、跳过、待处理项，不能对部分成功整批重试。”

选择两条提交执行 cherry-pick；第一条的 post-commit hook 建立属于 fixture 的 `index.lock`，使第二条在建立 `CHERRY_PICK_HEAD` 前失败。第一条已经提交，第二条未应用。此时 `rewrite::finish(...)?` 提前返回 `Err`，后面的 `applied`、`failed_oid`、`pending` 赋值未执行；实际 IPC 载荷只有通用 `NeedsVerification` 错误，界面没有批次清单。磁盘计划虽保存第一条 OID，但 `operation_state` 也不导出这些进度，用户只能手工核对日志。

应在这种失败及等价的写入异常分支保留部分回执，同时继续保持待核对和禁止盲目续跑。真实 Git 生产入口复现：1 项回归断言失败，退出码 101；原始证据见 `spec-evidence.md`。

C/S/O/H/F/T/L/D/V 均已对照；未发现第二项达到报告门槛的新问题。固定 Release、真实桌面、D06/Q98 不属于本轮源码结论，尚待规划方验收。


规划方核对独占fixture原始observation、Git命令和exit101：第一条已落地、第二条未应用、index.lock仍留，磁盘计划保留已完成项但IPC仅通用错误。两轴均结束，Codex直接修复H01批次回执，保持待核对、不盲目重放。


### Codex 对 review29 的修复（待全新两轴及固定Release）

Spec P2：历史序列执行保留最后确认的可变计划。已确认部分进度或当前Git可能已启动后，非零无原生停点、信号/进程异常、回收/计划收尾或读取失败均转换为completed=false的结构化回执，保留applied/skipped/failedOid/pending、原错误及其他失败。原Running计划保持不确定，不改为可盲目续跑；读取状态失败也作为related保留而不丢清单。入口最后的回收收尾失败经共用finish_sequence_snapshot追加错误，不能把已生成的批次回执再变成普通Err。未开始且没有任何部分进度的拒绝仍按原Err返回，写前身份/指纹/确认不放宽。

新增既有真实Git测试1项含四场景：cherry-pick/revert各在第一条完成后创建fixture自身index.lock，使第二条在原生停点前失败；另在第二条实际提交后由fixture hook使独占回收outcome.pending写入失败。分别核对三条原OID的已完成/停留/尚未处理清单、真实文件/HEAD/index及原锁，重开会话仍待核对且明确Continue不得发Git重放。实际载荷经生产rewriteOutcomeDetail、formatAppError和NotificationCard双向切语言显示全部短OID和错误；不是GUI替代验收。

集中检查原件repair29-*：新四场景专项1/1；M6首轮122/123、1过滤signal，唯一失败是既有interrupted_history测试仍unwrap_err旧返回形状。按本次契约改为检查completed=false、NeedsVerification、未确认成功的applied为空、failed/pending原OID，原重开/拒绝Continue/结束关联不改文件和index断言全部保留；受影响专项1/1通过，signal精确权限1/1通过，当前124项M6分批均有成功证据，不称单次整套退出0。完整Node482/482，build含tsc、lint、x18n0错误65未用警告/extract64候选0中文通过，双语各1710键；其余Rust套件未执行。

首轮Rust与先启动的bindings均因helper参数误写RecoveryRecord而实际为ProtectedRecovery触发同一E0308、exit101；失败源码/原输出全部保留，修正借用类型后专项通过。绑定文档按pnpm bindings生成，154项导出通过；生成器还改变57个不相关绑定的纯空白，先保存完整diff并逐文件gitdiff-w证明后恢复，仅保留RewriteOutcome文档更新；受影响build/lint随后重跑02通过。其后只有上述既有Rust测试契约同步，并有对应专项重跑。repair29-final-source-proof.json逐检查记录实际输入差异与归属：最终Node/中断专项精确匹配938源，所有末轮Rust生产源相同，前端02之后仅Rust测试改动，x18n/extract排除的绑定空白/测试差异明确列出。最终检查后无源码/helper清理。

成功导出：首次四场景载荷独占repair29-payloads-*保留；全M6输出用另一独占repair29-final-payloads-*，完整Node只读其固定结果；signal/中断专项另独占输出，旧载荷和所有失败不覆盖。locale/x18n metadata/依赖锁文件未改。

整包仍未完成；旧Release27和失败现场保留，桌面首轮两问题尚待新Release真实重验。下一步固定候选、按code-review全新独立两轴，通过后立即固定Release与真实桌面23组。不续Pi/第二正式返修/新实施代理，不推送PR/提前合main或删worktree。


### review30：独立两轴正式报告（2026-10-30）

#### Standards

# review30 Standards

固定范围：`26b68d1...525f992`（三点 diff，merge-base `fad6483`），38 个提交、173 个路径。固定 HEAD 与干净工作区已核对。

硬性规范违规：1；Fowler 启发式问题：0。

## ST30-01 [P2] 外部工具选择的持久化失败必须可见

位置：`src/App.tsx:5781–5784`。

规则：`docs/product-direction.md:148`（Q28）要求没有 Git 工具配置时“让用户选一次，保存在 Revia 的设置中”。

首次选用外部 diff / 合并工具时，如果设置文件不可写、父目录不可访问或写入失败，`ipc.updateSettings()` 会拒绝；这里用空 `catch` 吞掉错误并继续启动工具，成功分支只通知工具完成。Rust 虽暂时更新内存设置，重启后选择仍丢失，用户既不知道未保存，也无法处理保存错误。应独立呈现保存失败，保留本次工具运行结果，避免把启动成功当成设置已保存。

这是本范围新增入口的静态可确定问题，不是继承问题。源码、完整路径覆盖和原始检查输出见 `standards-evidence.md`。未执行动态复现、构建或桌面验收；未改产品、语言资源或生成绑定。Release / 桌面验收仍由规划方后续完成。


#### Spec

# review30 Spec

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...525f9926da1ac90429c9534af74b50900da6b0e9`（38 个提交、173 条路径）。结论：1 项 P2，需修复。

- **[P2] 继续批次的回收收尾失败会丢弃逐条结果** — `src-tauri/src/operation_flow.rs:196–199`，核心位置第 197 行。H01 要求“失败 / 冲突后列出完成、跳过、待处理项，不能对部分成功整批重试”；C02 要求“完成回执失败而结果不确定时，保留归档和状态”。两个 cherry-pick 在首项冲突后，解决冲突再点击继续；若仅该次 OperationContinue 回收记录收尾不可写，`after_action` 已完成两项并清除持久计划，随后 `finish_snapshot(record)?` 把已有 `RewriteOutcome` 丢掉。真实 Git 复现得到两条新提交、`operationAfter=null`，返回值却只有通用 NeedsVerification，前端无法显示已完成清单，重启后也无批次回执可读。应在保留 continuation 的结构化结果上附收尾错误与待核对状态。

C/S/O/H/F/T/L/D/V 均已覆盖；本项属于 M6 新增续接链路，非继承问题。独占 fixture 与单项生产入口复现的命令、退出码、源码及原始输出见 `spec-evidence.md`。未运行 GUI、Release 或整套验收；这些后续验收不列为产品缺陷。未实现修复，临时测试入口已移除，固定 HEAD 与干净工作区保持。


规划方已核对原始静态Promise因果链及真实Git result.json：两个选择均完成、原计划清除，OperationContinue回收收尾EEXIST使IPC仅有通用错误。两轴已结束，由Codex直接补齐续接回执及独立设置保存失败通知；不以复现exit0当产品通过。


### Codex 对 review30 的修复（待全新两轴及新Release真实桌面）

Spec P2：operation_flow的原生Continue/Skip取得批次continuation后，其外围OperationContinue/OperationSkip回收收尾共用finish_sequence_snapshot；收尾失败将同一已取得回执标为completed=false并附结构化错误，不再通过?丢掉applied/skipped/pending等清单。该helper只改为crate内可见，原入口cherry/revert处理一致，类型/载荷字段不改；原Git状态和已完成计划不回滚、不盲目重放。

Standards P2：外部工具选择updateSettings失败独立发持久warning，原AppError在呈现时按当前语言格式化，归属对话框原worktree；本次工具运行的真实结果仍按原流程处理，成功只表示工具运行成功，设置保存错误不再吞掉。不增加或修改locale：先经x18n查询errors.settings.failedWriteSettingsCause既有双语，沿用后端实际结构化设置错误。

必要回归：新增真实Git续接1项含4场景，cherry-pick/revert分别Continue/Skip，首项真实冲突后解决或跳过，由本fixture hook仅破坏对应外围回收记录的outcome.pending；两项历史实际完成/跳过、原计划清除后仍返回准确清单、待核对错误和recoveryId，再点旧Continue/Skip不重放。新增真实设置/工具1项，在独占appdata中使settings.json成为目录，生产Registry保存失败、重开选择未保存；随后生产外部工具入口通过真实Git difftool执行fixture本地工具并成功、文件原字节保留。载荷和fixture保留。本轮既有正常cherry Continue/Skip/Abort/重开及多revert续接2项同样通过。

Node新增2项消费上述真实载荷：生产NotificationCard/rewriteOutcomeDetail完整保留完成/跳过短OID，双向切语言保留错误；实际App ExternalToolDialog onLaunch回调经AST取出执行，diff/merge两设置字段、工具结束后迟到保存拒绝仍通知原A，切到B不关B对话框，警告持久且即时双语。这是生产代码回归，不冒称GUI验收。

集中检查repair30-*：相关Rust4/4（其中新续接测试内4场景）、完整Node484/484、build含tsc、lint、x18n0错误65未用警告/extract64候选0中文均退出0；本轮无新增失败，review30原缺陷复现及全部旧失败保留。当前M6共126项，本轮未重跑完整126或其他Rust套件，不把旧124分批成功说成当前全套重跑。repair30-final-source-proof.json核对最终938源与9末轮检查完全一致，locale/x18n metadata、依赖锁文件、生成绑定均未变，检查后无源码/helper清理。成功载荷本轮独占repair30-payloads-*，既有正常续接检查使用另一独占输出目录，未覆盖旧载荷或失败原件。

当前仍未通过新的两轴，旧Release27仍不通过；真实桌面首轮L01/S02两问题候选尚待新Release实测，其余23组未完成。不续Pi/第二正式返修/实施代理，不推送PR/提前合main删worktree。下一步固定新候选、code-review全新独立两轴，通过后立即固定Release和真实桌面。


### review31：独立两轴正式报告（2026-10-30）

#### Standards

# review31 Standards

固定范围：`26b68d1...713a495`，共同祖先 `fad6483`；39 个提交、173 条路径已纳入覆盖记录。开始与结束核对的固定 HEAD 均为 `713a4953d412b2c27e3992e364d98952c87da85a`。按任务排除旧报告、任务 Comments 与另一轴。

硬规范违规：**0**。启发式问题：**1（P3，非阻断建议）**。

- **ST31-H1：复用空 cherry-pick 策略处理。** `src-tauri/src/history_sequence.rs:303–308` 与同文件 `243–251` 重复判断 Git 失败、cherry-pick 停点及空暂存区，再根据 `keep_empty` 执行空提交或 skip。首次批次遇空提交走前者；冲突后明确继续且结果为空走后者。同一策略修订需要同步两份分支，已有 `finish_empty_continue` 可作为共享实现并改成表达共同职责的名称。依据本轮 Fowler 基线 **Duplicated Code**：“the same logic shape appears in more than one hunk or file in the change → extract the shared shape”。该文件为本范围新增；当前两处行为一致，未证明功能回归。

本轴仅做源码与规范审阅，未修改产品或执行构建、测试、GUI。固定 Release 和真实桌面属于后续验收，不计作产品缺陷。完整逐路径记录、规则与原始证据见 `standards-evidence.md`。


#### Spec

# review31 · Spec

固定范围 `26b68d1...713a495`（merge-base `fad6483`），39 个提交、173 条路径；开始与结束均为固定 HEAD、干净工作树。按 C/S/O/H/F/T/L/D/V 核对；未读旧审查、Comments、实施报告或另一轴。

## [P2] 写后重读失败时，保留并展示已经返回的历史批次回执

位置：`src/App.tsx:4748–4762`（调用方 `4897–4907`、`4778–4790`）。

规格 H01（`docs/milestones/m6/spec.md:111`）要求：“失败 / 冲突后列出完成、跳过、待处理项，不能对部分成功整批重试。”

`work()` 已返回含 `applied/failedOid/pending/skippedEmpty` 的结果后，`refreshTarget()` 返回 false 或 `refreshM6State()` 抛错会使统一入口丢掉该结果并抛出通用待核对错误。历史对话框与继续操作调用方因此无法进入回执展示；`onResult` 只处理自动保存通知，也未保存清单。

真实 Git 复现得到 A 已完成、B 冲突、C 待处理，再令本次独占仓库的 index 不可读取，生产 `session.refresh()` 确实失败。将真实结果交给原样提取的生产 `performM6Write`，在对应重读失败接缝观察到调用方收到异常，三个 OID 均从异常中消失。需要让已知回执随待核对错误继续交付和展示，避免用户只看到通用错误而无法判断已执行边界。

这是本轮新增 M6 写编排问题；原件与完整覆盖见 `spec-evidence.md`。固定 Release 与真实桌面验收属于后续工作。


规划方核对固定范围、真实Git部分回执/重读错误及生产前端接缝：写本身已完成A、B冲突、C待处理，后续读取失败遮住清单。两轴均结束，由Codex直接保留调用方已收回执并在错误通知显示；P3空提交策略重复作为非阻断建议，不扩大重构。


### Codex 对 review31 的修复（待全新两轴及新Release真实桌面）

Spec P2：历史改写与原生继续操作调用方使用既有onResult接缝先持有真实后端回执；统一写入入口完成后的真实重读失败时，调用方从该回执生成持久待核对通知，完整保留已完成/跳过/停留/待处理OID及原后端错误，现有错误按当前语言懒格式化。历史确认只关闭原origin的对话框，切B后不关B、不把A通知归B；在未收到任何后端回执时仍沿原错误路径处理。performM6Write继续标记NeedsVerification并保持关闭/退出保护，不重发work；收尾再次读取失败保存为related，不能用后一次异常覆盖原错误或绕过待核对。

新增真实Git回归1项含2场景：完整三项成功、A完成/B冲突/C未处理，实际生产rewrite返回准确清单后，仅将本fixture的.git/index保存为index.preserved并改成目录，使生产session.refresh真实失败；原index、失败现场、原输出和实际载荷保留。没有修改Rust生产代码或模拟Git。

Node新增1项直接执行当前生产performM6Write、runHistoryRewrite、performOperationAction三函数、实际WriteGuardOrchestrator/CloseCoordinator与NotificationCard。消费上述两种真实回执/读取错误以及repair30实际续接回执，组合refreshTarget false/抛错及refreshM6State抛错共18场景；每次work仅执行1次、A待核对保护保留、写中状态释放、B对话框保留、仅有1条原A持久warning，已知清单和原后端错误在同一通知即时中英切换仍完整。这是生产函数回归，不冒称GUI验收；续接回执为原Rust源未变的repair30原件副本，不称本轮重新执行了其所有Git场景。

集中检查repair31-*：真实Git1/1（内含2场景）、完整Node485/485、build含tsc、lint、x18n双语1710键0错误65unused/extract64候选0中文全部退出0，本轮无新增失败，review31原缺陷复现及全部旧失败保留。当前M6共127项，本轮未重复完整127或其他Rust套件；Rust生产源完全未变。repair31-final-source-proof.json核对最终938源与6末轮检查逐字相同，locale/x18n元数据、依赖锁文件、绑定均未变，检查后无源码/helper清理。载荷独占repair31-payloads-*，不覆盖旧成功或失败原件。

Standards空cherry-pick策略重复P3为非阻断维护建议，本轮不重构。下一步固定候选、code-review全新独立两轴，通过后固定Release，真实重验首轮L01/S02并继续23组桌面/Rebased/中英文窄窗。整包仍未完成，无Pi续接/第二返修/新实施代理，不推送PR/提前合main删worktree。


### review32：独立两轴正式报告（2026-10-30）

#### Standards

# Standards review32

固定范围：`26b68d1fb0315cb85376932a8440204c118dfb78...39e6dcf715223e7454571e5a0c1c55d52830ab7e`，40 个提交、173 个路径；HEAD 一致且工作区干净。

硬性规范违反：**0**。Fowler 启发式意见：**1**（P3，非阻断）。

- **S-H01｜possible Duplicated Code** — `src-tauri/src/rewrite.rs:1095–1097`。同一 `interactive()` 已在 1035–1037 对同一 `commits` 执行完全相同的 `for oid in &commits { require_linear(&session.git, oid)?; }`。两处之间只校验编排，未更换提交集合。`require_linear` 经 `parents()` 每次启动 `git rev-list --parents -n 1`；因此一个含 N 个提交的合法线性计划会再启动 N 个相同校验进程。依据本轮 Fowler 基线：“the same logic shape appears in more than one hunk or file”。建议把固定范围的线性校验集中执行一次，保留后续目标身份、文件状态与指纹重验。这是可选简化判断，不是安全失败或已实测的耗时回归。

已按任务隔离两份过程报告与任务 Comments，逐路径覆盖及原始证据见 `standards-evidence.md`。未执行产品修改、测试、构建或桌面验收；本结论不代替固定 Release 与真实桌面验收。


#### Spec

## Spec

- **[P2] 不要指导用户在解决 stash 冲突后再次应用同一条目** — `locales/zh-CN.json:1834`、`locales/en.json:1834`，由 `src/features/m6/StashPanel.tsx:416` 展示。S02（`spec.md:71`）要求：“应用有冲突时保留条目，跳到已有冲突处理流程”。当前中英文提示却说“解决后可再次应用”。真实 Git / 生产入口复现中，首次应用发生冲突，用合并页签的生产入口完成解决后，照提示再次应用会把已解决文本重新写成冲突标记，用户必须重复解决；回收保护仍在。应说明保留条目用于保护，完成当前冲突处理并核对结果，无需重复应用。原始源码、完整命令与实际结果见 `spec-evidence.md`。

全范围 173 路径已登记核对；除此项，未确认其他遗漏、范围扩大或实现与规格不符。固定 Release、真实桌面 / Rebased 对照仍属后续验收，本结论不代替这些验收。


规划方已核对固定SHA、首次apply冲突/生产merge_apply解决/再次apply重新冲突的原始stdout、实际文件和exit101；不把原探索性断言失败称为产品通过。两轴结束，Codex直接仅经x18n改中英指导，说明解决现有冲突并核对、无需再次应用；不禁用其他合法情境的重复apply。P3重复线性校验留非阻断维护建议。


### Codex 对 review32 的修复（待全新两轴及新Release真实桌面）

Spec P2：stash.applyConflictHint先经x18n get核对现有键和实际StashPanel应用确认引用，再仅经x18n set改双语及CLI review源hash。中文为“应用发生冲突时，stash 条目会保留。请解决当前冲突并核对结果，无需再次应用该条目。”，英文对应说明保留条目、解决当前冲突并核对、无需再次应用。未改变stash Git执行或禁止其他合法重复应用情境，未更改已确认规格。Standards重复线性校验P3保留为非阻断维护建议。

集中最小检查repair32-*：既有相关Node双脚本40/40、build含tsc、x18n双语1710键0错误65unused、extract64候选0中文均退出0，本轮无新增失败。review32原真实Git复现exit101、原断言和再次应用后冲突文本全部保留，不把其测试失败当通过。仅两条资源及CLI reviews元数据变化，Rust/TypeScript生产源与测试完全未改，未重复Rust/完整Node/lint，不把此前485项当本轮重跑，也未新增复述文案测试。repair32-final-source-proof.json确认最终938源与四检查逐字一致，CLI两次修改的精确argv/exit/原输出另存repair32-set-zh/en；最终检查后无源码/helper清理。

旧Release27仍未通过，新菜单/写刷新修复未新Release真实重验，23组桌面未完成。下一步固定候选、code-review全新独立两轴，通过后固定Release和真实桌面。不续Pi/第二返修/另派实施代理，不推送PR/提前合main或删worktree。


### Claude 接手：review33 修复与固定 Release 真实桌面（2026-10-30）

用户要求 Claude 接手验收（“为什么一直验证不好 你来试试”）。

**为什么一直不收敛**：review27 源码两轴通过、Release27 桌面首轮发现 2 项后，每次修复都重新做“全新独立两轴、完整 173 路径”审查，再要求通过后才固定新 Release。review28–33 每轮各报 1–2 项新的 P2/P1 边界（越来越依赖 hook 改分支等构造场景），桌面 23 组因此一天多没有恢复。另一方面，真实桌面一小时内就发现了源码审查没有覆盖的 7 项可见问题。

**review33 Spec P1**（`e6c58e1`）：历史批次最后一步后重验原分支；自动保存关联记录保存时的分支，自动恢复不在原分支时保留 stash 与关联并报结果待核对。新增两项真实 Git 回归（修复前均失败）。

**固定 Release e6c58e1 桌面**（独立应用数据与 fixture，`~/.cache/revia-m6-claude/`，记录见 `release-ca646c2/observations-e6c58e1.md`）：

- 通过：L01 首启简体中文、中英原生菜单；S02 恢复暂存区+应用后删除；S03 删除（确认框打开时终端追加 stash，旧确认被关闭、不误删；按 OID 删除）、gc 清掉对象后从归档取回原 OID、清空全部；O01 merge 三栏合并“应用”只解决文件、点“继续”生成合并提交；rebase 跳过与中止；S01 同 OID 两行分别选中/删除、比较设置重启保留。
- 发现并修复（`ca646c2`，Release ca646c2 复验通过）：跳过/继续停到下一冲突被报为“未成功完成、待核对”并触发关闭保护；提交区仍写“Revia 不提供 continue / abort / skip，请在终端完成”与“请到终端处理”；提示用写死的“。”拼句；跳过确认框标题为“确认中止操作”；日志分支图表头折行压住首行；默认窗口日期列被面板裁掉（主题列最小 280→160）；过滤框放大镜盖住占位文字（M5 遗留）。
- 非阻断观察：编辑菜单中 AutoFill / Start Dictation… / Emoji & Symbols 与 Quit and Keep Windows 为 macOS 自动加入、仍英文；Stash 面板列出其他仓库的归档（只禁用恢复），待用户决定是否只显示本仓库。
- 未验（后台操控无法打字、开原生下拉框和右键菜单）：L01 即时切换与草稿保留、S02 新分支名、合并视图撤销/重做、H01–H03、O02、O03、F01、F02、T、C01-late、D01 900px 窄窗。需在全屏控制下继续。

检查：Rust 693 通过 / 0 失败 / 1 既有忽略（M6 130/130）、Node 485/485（含真实 Git 载荷）、build/tsc/lint、x18n 1713 键 0 错误 65 未用、提取 64 候选 0 中文。未推送、未开 PR、未合 main。


### Claude 全屏桌面验收与第三批修复（2026-10-30）

用户回复“现在继续”并批准全屏控制。固定 Release ca646c2 上补完其余各组，记录与原始 Git 事实见 `~/.cache/revia-m6-claude/release-a187c87/observations.md`、`release-ca646c2/evidence/`。

- 通过：L01 设置里即时切 English，草稿、diff 页签、第 180 行附近滚动保留，系统菜单变英文，重启后语言与草稿保留，切回跟随系统；D01 900 宽窗日期单行、表头对齐；S02 保留暂存区 stash 以“恢复暂存区 + 应用后删除 + 新分支 m6-desktop-stash”应用成功（Release27 的 P1 已不复现）；O01 合并视图手改结果、⌘Z / ⌘⇧Z、中途切英文后内容与撤销历史保留，Apply + Continue 生成合并提交；H01 多选 cherry-pick 冲突后解决并继续；H02 reset 四模式（hard 写回收）、reword、空提交跳过与保留、revert；H03 reword / fixup / 移动 / drop 与含合并范围禁用；O03 edit 停点 amend + 新建提交拆分后手动继续；O02 自动保存恢复冲突保留 stash、“查看 stash”定位、恢复按钮禁用、“保留 stash，结束自动恢复”；F01 中文空格重命名历史、所有分支、在日志中显示、二进制只读说明、同路径删除重建旧对象；F02 三开关默认值与 Rebased 一致、上一版本沿重命名到根；T 仓库局部 difftool / mergetool 真实调用并写回收；C01 迟到交付期间切换器不可用，放行后只更新原目标。
- 修复（`a187c87`，均在 Release a187c87 复验通过）：历史批次停在冲突时附 NeedsVerification 错误，界面报“未成功完成”并触发关闭保护；文件历史与 stash diff 头中文路径被转义（缺 core.quotePath=false 与固定前缀）；Stash 面板开着切仓库误报“状态已变化”并禁用保存；根提交改写禁用理由误为“不在当前分支历史内”；根版本 blame 的“没有更早的版本”只在禁用按钮 title 中；cherry-pick 选项在设置搜索中找不到。新增 3 项真实 Git 回归，修复前均失败。
- 检查：Rust 695 通过 / 0 失败 / 1 既有忽略（并行默认线程时 `user_refresh_during_autostash_invalidates_the_unstarted_history_write` 的 15 秒等待仍可能超时，单独与 4 线程均通过，属既有计时测试）、Node 485/485、build/tsc/lint、x18n 1714 键 0 错误 65 未用、提取 64 候选 0 中文。
- 待用户决定的观察（不阻断，未改）：Stash 面板列出其他仓库的归档（只禁用恢复）；交互式 rebase 是每行下拉框，Rebased 是顶部工具栏；Reset 对话框跨仓库记住上次模式（含 hard），与 IntelliJ 一致；多选时确认框只显示右键那一个提交、“修改提交信息 / 从此提交新建分支 / Reset”仍可点（作用于右键那个）；改动列表右键是 WebView 自带菜单（M4）；窄窗时详情面板占过半宽度（M5）；新仓库首次读取前仍显示上一仓库的改动列表（操作仍绑定原仓库并写明路径，M2/M4）；编辑菜单的 AutoFill 等系统项为英文。
- 未合 main、未推送、未开 PR、未删 worktree。


### Claude → Codex 交接（2026-10-30）

Claude 额度将尽，剩余工作（Q175 记录、operation_flow 停点判断补 metadata_version、Spec 轴增量审查、Release 复查 ff38a02 改动点、压成一个提交合入 main）见 [handoff-claude-to-codex.md](../handoff-claude-to-codex.md)。


### 增量审查与合并（2026-10-30）

按 Q174 只审 `f00f469...HEAD`。Standards：P2 Reset 不记 Hard 未成决策 → `473419c` 记 Q175、改 spec H02；P3 跳过 / 中止原地失败被当成正常停点 → `ba1fd57` 要求元数据版本变化并补 index.lock 回归（修复前失败）；其余 P3（diff 固定参数重复、`AutostashLink.branch` 命名、多选三元表达式重复、“ · ”分隔符）为维护建议。Spec：P2 同上已修；P3 多选含合并提交的提示、切换仓库残留 Stash 面板状态 → `0113241`；P3 旧版无 branch 字段的关联拒绝自动恢复（仅开发数据存在，显式恢复可用）记维护建议。Release 0113241 复查 Stash 归档分组通过，其余小改动的真机复查转 backlog 14。用户确认合并。
