# M7 实施与验收报告

Status: ready-for-human

实施方：Paseo / Pi（同一会话；含 2026-10-31 首轮正式返修）。规划方 / 验收方：Codex。
分支 `m7`（worktree `/Volumes/T7 Shield/project/Revia-m7`），规划基点 `581d1527651e981018f2b0830d994bca49580254`。
独占证据根：`/private/tmp/revia-m7-batch-g5scyw0r`（首轮交付原件、review-01 审查目录、返修日志全部保留，不覆盖）。






## 固定Release与当前剩余验收

两轴审修已通过后，以源码`be8f9d882b59546a3a1e905812f92a5222dde086`（产品`628fff93cf19ce1f4e61f329857ed165e087b117`）运行`bash tools/build-app.sh release`，`codex-fixed-release-build-01`真实exit0。产物为macOS arm64 Release二进制，固定副本`/private/tmp/revia-m7-batch-g5scyw0r/fixed-release-jwgkkh07/revia`，SHA256 `b1a4c3d7da2840d97ff3b8e8dffe14c136213f2ab915ca9ae8c77ca28ae1bb38`。前端21个文件分别散列并归档；完整元数据`/private/tmp/revia-m7-batch-g5scyw0r/fixed-release-jwgkkh07/release.json`，包含原始构建记录、前端归档散列与源码对应关系。构建后全部源码hash未变。

未启动产品、未生成正式安装包或接触发布签名。当前仍无原生桌面工具，AXIsProcessTrusted=false，原证据`review-03-zika9m9t/desktop-capability.json`。任务13仍须在独占fixture中真实观察Rebased1.1.22既有暂存、部分暂存及提交后index/工作区，再补规格并实施；不能从Git猜测或模拟IPC决定。任务14的真实四主题/双语900px、D06、编辑终端、GitHub/诊断/更新等仍未验。没有将编译或测试称作桌面验收通过。

进入awaiting-native-desktop；当前可独立完成的源码审修和Release固定已完成，13/14保留阻塞。未push/PR/tag/发布、未合main/删除worktree；formalRepairUsed=true、Pi继续idle，监控保留。源码不变时不重复检查或启动代理；取得真实桌面能力后才续接约定验收。

## 最新 Codex 审修结论与剩余验收

review03原文已入对应任务：Standards1项P2与Spec1项P2均由Codex修复；三个P3判断留backlog15。产品检查点`628fff93cf19ce1f4e61f329857ed165e087b117`经全新review04两轴只审该次14路径增量，Standards硬违反0/新增判断0，Spec新增或未解决P1/P2为0；原文分别保存在任务Comments与本轮证据目录。此为本次审修通过，不能表述为全部M7已完成。

01–12转ready-for-human表示实现与审修已交付待真实验收，不表示已完成全部勾选项；13仍needs-info，必须先取得Rebased1.1.22既有暂存内容观察。14最终桌面与性能、双语900px、真实账号/钥匙串、正式来源更新与CI/Windows仍未验。当前无原生桌面工具且AXIsProcessTrusted=false；不会用浏览器或模拟IPC替代。下一步在固定候选构建Release并保存散列，继续保留全部失败原件。未push/PR/tag/发布、未合main或删除worktree，监控继续。

## Codex review03 两项 P2 已修复

S03-01：撤销回磁盘原文也排在旧草稿持久化之后清理；重新打开等待原目标在途写，并保留最新内存正文（含撤销结果），避免加载旧草稿。自动草稿冲刷覆盖有待清理记录的干净正文。清理失败保留最新文本、原件与退出保护，明确冲刷成功后解除；等待期间的新输入仍有自己的后续草稿，不被较早清理删除。

H1：四个新增测试fixture共用平台环境构造，只从当前平台PATH解析Git；HOME/USERPROFILE、Git全局配置均绑定本次独占fixture。Unix专用hook/权限断言继续按平台限定；未通过跳过全部Windows测试来消除问题。

`review03-repair-node-01` 102项通过（含新增4个生产EditSession撤销/在途/失败/新输入回归）、0 skipped；`review03-repair-rust-01` 四个受影响targets共27项通过；`review03-repair-frontend-01`与`review03-repair-lint-01` exit0。源码hash、精确argv、真实exit、分流原件在批次codex-checks。未修改locale键，之前x18n02的0错误/76保留键警告对应键集未变。未运行Windows或真实桌面，不把macOS通过称Windows实测。只修明确P2，三个P3判断继续backlog15。

## Codex 当前候选准备：F13/F22 与最终检查

已下载的真实包字节、散列和签名清单保存在应用数据目录；安装用锁定 Tauri updater 2.14.1 的 restore_update / verify 恢复并再次验签，直接安装保存的字节，不重新下载。签名必须绑定版本，旧的无版本签名不可恢复。公共状态补 downloadedVersion / restartRequired，历史回执不遮住新版本，重启后的旧回执不重复要求重启。下载回包同步全局状态，重新打开面板的较早读取不能覆盖操作结果。

安装和重启分别进入生产 CloseDriver / WriteGuard / CloseCoordinator，同步处理未保存编辑、Git 写入 / 待核对和全局终端；取消不安装、不重启。合并编辑仍归原 CodeMirror 实例，未应用内容先通过原有应用 / 取消入口处理；生命周期保留窗口与退出保护。安装成功回到应用，重启需要独立明确操作；失败取消本次更新意图，不自动重发。

渠道从构建配置绑定仓库的公开 Releases 完整分页中选取最高合适 semver；默认稳定，主动开启才含预发布，草稿排除。两类客户端使用同一仓库发现入口，再读取所选 tag 的清单；包 URL 固定到自身 tag，beta 客户端不再锁在自身旧标签。发布覆盖配置要求 requireSignedVersion，Tauri CLI 2.12.3 本机帮助确认 build 自动绑定签名版本。未读取真实私钥、未运行 Actions、未发布；docs/release.md 已同步新流程。

最终原件在批次 codex-checks：Rust 全部 23 个 integration targets 及 lib/main 已逐目标完成；首轮 codex-final-rust-01 真实 exit101，故不能称单轮全过。其前半 targets 通过，GitHub auth fixture accept 后读取 WouldBlock 导致失败；仅给隔离 HTTP fixture 的 accepted socket 明确 blocking，未改产品网络实现。codex-final-rust-continuation-01 后被用户中断，command 无真实 exit（保留 null 与 interruption.json），不得补造退出码。continuation-02 的剩余 11 targets exit0；另为 auth / checkout 补独立真实 exit。codex-final-doc-01 exit0、实际无 doc 用例。discard_recovery 保留原 1 ignored；不重复叠加授权子进程内外计数。

最终 codex-final-node-04 的 8 个相关生产模块入口 98 项通过、0 skipped；codex-final-frontend-build-02、codex-final-lint-03、codex-final-x18n-02 exit0，翻译 0 errors / 76 既有 unused-key warnings。原 updater-lifecycle-node-01 发现失败意图仍保留，修正后共享回归通过。另一次 codex-final-node-03 误用通配符，包含需要专用 fixture 参数的旧 M6 脚本，494 通过 / 8 失败、exit1；原件保留，按正确相关入口重跑通过，没有改旧 M6 范围以消除该调用错误。源码散列、精确 argv、真实 exit、分流 stdout/stderr 与全部失败原件均保留；生成绑定只清理本次空白，受影响前端检查在清理后执行。

update package 新 3 项实测下载验签、保存、关闭 fixture HTTP、新 updater 离线恢复并实际安装，只替换独占 fixture .app；另 3 项渠道与 1 项公开旧签名元数据回归通过。含坏包、版本 / 公钥 / 渠道篡改及原子落盘失败。上述均不代表真实 Revia 桌面更新、真实账号 / 钥匙串、发布 CI 或正式签名来源已验。任务13仍缺 Rebased 1.1.22 既有暂存内容真实桌面观察，14最终桌面受阻；不合 main、不删除 worktree、不删除监控。

## Codex 直接修复 PR 检出 F11

操作状态在任何PR fetch/引用/本地分支写入前核对，读取失败也拒绝；从取PR到既有工作区检出持续持有同一个真实worktree写锁。原检出流程新增只能持同目标WriteGuard进入的内部入口，工作区覆盖仍走既有保护/回收。PR检出同时接到App既有全局写门、关闭协调器和按原目标登记待核对结果的入口。

fetch使用每次独占临时引用，不改已有PR引用、不额外取tag或写FETCH_HEAD；后续拒绝/需确认时按实际OID核对并回退本次本地分支变更。已部分检出到新分支时保留真实HEAD与工作区，返回NeedsVerification，不强行回退造成index/正文错配。临时ref按其实际OID删除；回退/清理失败保留原错误并转待核对，不自动重发。不删除index.lock。

`pr-checkout-cargo-03` 39项通过（新PR检出8项、既有branch_remote31项），只用受控loopback HTTP、虚构token和独占真实Git/local remote/HOME/appdata。覆盖真实merge中零HTTP/零ref写、已有写锁、普通检出拒绝后的新/旧分支回退、分叉未确认拒绝、真实index.lock保留、post-checkout失败后真实HEAD保留，以及Smart Checkout沿既有策略恢复正文并可从真实stash archive取回原暂存版本。`pr-checkout-node-01` 52项共享生产写门/关闭/终端回归通过；前端build/lint/x18n均exit0，翻译0错误/76个unused-key警告。全部原stdout/stderr、argv、源码hash和exit在批次codex-checks。

首轮`pr-checkout-cargo-01`真实exit101，6通过/1失败：测试原先假定既有Q150智能检出使用stash apply --index；实际沿既有stash apply恢复正文，暂存区划分未自动恢复。未修改该既有策略；修正用例并真实读取回收档案的原暂存版，完整失败原件保留。此结果不决定任务13的Rebased既有暂存内容语义。

`--no-write-fetch-head`在[官方Git 2.29说明](https://raw.githubusercontent.com/git/git/v2.29.0/Documentation/RelNotes/2.29.0.txt)中加入，当前可选PR检出在任何网络/写之前检查此能力，复用既有双语版本错误；其它功能的全局最低版本未改。回归用独占包装器报告2.28验证门禁，不能冒称运行了真实2.28二进制。官方读取原件保存在impl/pr-checkout-git-official.json；Grok连接失败后用可用官方网页读取渠道取得，不更换实施模型。

本组仍是中间检查点；F08/F09 PR迟到响应与草稿身份、F10完整保护缓存/并集、F13/F22更新协调仍未完成。真实PR账号、原生窗口关闭和桌面检出待任务14验收。

## Codex 直接修复终端所有权与关闭（F05/F06/H9）

终端改由App级TerminalSessions持有；异步打开/恢复、运行态、错误与输出活动都绑定原worktree，A晚回不会追加到B，隐藏面板不丢进程或解除退出保护。首次完整读取持久页签后再写入；按cwd认领已有PTY，其余页签新开shell。全局串行保存保留其它worktree记录；失败不以初始空列表覆盖旧页签，最后页签持久化也保留原生退出保护。

页签关闭必须显式确认运行中的进程。应用关闭/退出先冻结新终端，等已发打开结束，列出全局仍运行的进程供确认；取消不结束进程，确认也只准备，仍由既有CloseDriver等待Git写入/待核对并在实际关闭时终止已确认进程。最终草稿冲刷、终端页签落盘与关闭成功后才解除原生退出保护。Rust关闭失败保留原句柄并回传结构化cause；终端输入和忙碌提示核对实际会话所属worktree。

`terminal-session-node-03` 33项通过（终端12项、编辑13项、既有合并写门8项），直接调用生产TerminalSessions/EditSessions/CloseDriver/WriteGuard，含在途A→B、隐藏运行态、取消/等待Git写、持久化错误和最终关闭顺序；所用可控API依赖用于并发回归，不是桌面验收。`terminal-cargo-02` 3项通过：真实自建PTY读写/中文/resize/退出、两worktree隔离，以及损坏页签原件保留。Tauri mock仅承载事件。`terminal-frontend-build-03`、`terminal-lint-01`与`terminal-x18n-02`均exit0；翻译0错误/76个unused-key警告，lint新增一条no-useless-spread提示留backlog。

首轮前端类型检查`terminal-frontend-build-01`真实exit2（占位回调推断为false字面量），修正返回类型后检查通过，失败原件保留。每轮源码hash/argv/exit/原stdout/stderr在批次codex-checks目录。以上为本组检查点，不代表整包或真实桌面通过；更新安装的终端确认/重启全局协调仍随F13继续，当前安装门禁已读取全局运行态，不会因面板隐藏而放行。

## Codex 直接修复编辑生命周期（F03/F04/F20、H1/H2/H3/H7）

实际编辑会话按worktree与路径持有正文、磁盘版、原基线及身份摘要，不随组件隐藏或切换目标销毁。恢复装入真草稿；磁盘再次变化只更新对比，确认后仍须显式保存。保存复用现有WriteGuardOrchestrator/CloseCoordinator，失败和待核对保留原目标、正文和退出保护，不自动重发。放弃排在已发草稿写后清除，并阻止卸载将它复活；持久化失败可见且可明确放弃。

关闭/退出及相关暂存、段操作和提交先执行保存/放弃/取消；后台和启动发现的草稿也参与。保存改变磁盘时先刷新并停止旧的Git动作，要求重新核对。后台存储以真实worktree身份隔离，同路径替换不继承旧身份；原旧草稿文件保留。生产Rust写门与草稿存储共锁，相关文件有草稿时拒绝暂存/提交。编辑保留在Git并排工作区右侧，统一/历史/暂存/冲突只读；草稿尚未保存时不沿用磁盘行号和Git段操作目标。

本组独占验证原件均在`/private/tmp/revia-m7-batch-g5scyw0r/codex-checks/`：`edit-io-cargo-01` 10项真实Git/权限/BOM/CRLF/身份/冲突/元数据/草稿门禁通过；`edit-final-cargo-01`含原9项编辑及152项共享暂存/丢弃/提交通过、1项ignored；`edit-bindings-cargo-01` 7项含ts-rs导出通过；`edit-session-node-03` 13项实际生产状态/写门/关闭协调器回归通过，含将真实Rust权限错误载荷交给实际双语formatter。前端`edit-frontend-build-02`与`edit-lint-02`exit0；`edit-x18n-01`为0错误/76个unused-key警告，保留键不自动删除。

`edit-cargo-01`编译exit101发现批量丢弃Result包裹不匹配，已修复，失败原stdout/stderr保留；后续检查均单独记录源码hash、精确argv、真实exit和分流原件。清理生成绑定只涉及EditDraft/EditableFile，不清理其它绑定。无关rustfmt差异已去除，并重跑受影响Rust检查。上述不是最终候选/全量最终检查/真实桌面验收；并排编辑、双语窄窗与原生退出仍须任务14真机检查。

## Codex 当前结论 — 2026-10-31 review02

候选 `384360c4ea7b30b04f0114a60156107c9fa6c501` 返修增量审查未通过。Standards：9项硬违反（其中1项P3非阻断）、3项判断建议；Spec：18项。两轴原文与证据分别追加对应任务Comments，完整报告在任务14；P3已记backlog15。唯一正式Pi返修已使用，明确遗留由Codex直接接管。

原始末轮Rust日志确为772通过/0失败/1忽略、记录exit0，其它末轮检查即时exit0，但多数输出合流且无逐轮输入hash；证据限制见review02/delivery-evidence-audit.md。原实现报告保留在下方作为历史，不代表本候选通过，不能以构建/冒烟代替真实桌面。任务13/14事实门槛仍未满足。

## Codex 直接修复首组（进行中，非最终候选）

已直接修正：真实仓库主分支的克隆/诊断/更新对话框挂载；诊断严格字段投影、七天摘要/清空和预览快照导出；克隆每次读取当前Git设置、实际argv与凭据打码；构建签名不再二次base64编码，公钥接受原文/外包base64；快捷键替换用显式空绑定，单项恢复默认同样先确认冲突。隔离本地克隆测试另外复现并修正了完成日志重复获取缓冲锁导致的卡死。

本组证据在`codex-checks/`，每轮保存跟踪/未跟踪源码hash、精确argv、原stdout/stderr、真实exit。`diagnostics-clone-updater-cargo-03`通过4项真实生产入口集成回归（诊断2、克隆1、更新清单与锁定Tauri updater下载验签1），`shortcuts-node-01`通过2项实际模块按键分派回归，`shortcuts-persistence-01`通过1项Registry保存/重载回归；前端构建`codex-frontend-build-02`、绑定收敛后的类型检查`codex-frontend-types-03`、`codex-lint-02`均exit0；`codex-x18n-01`为0错误/69警告（不声明警告均为旧问题）。公开签名测试仅下载验签，不执行安装；mock app仅承载事件和updater，不属于桌面验收。

失败原件全部保留：`diagnostics-clone-cargo-01`真实exit101复现日志锁死；`updater-metadata-cargo-01`独占PATH漏Node、`-02`fixture漏插件配置均exit101，补齐fixture后`-03`通过；绑定格式化`diagnostics-bindings-whitespace-01`子进程未启动（launcher exit1，不能记作node退出码）。工具造成的99个无关绑定空白变化已逐项比对并回退，回退前字节另存`impl/binding-whitespace-before-revert`，三个诊断绑定语义变化保留。

编辑/草稿、终端生命周期、PR身份/保护/检出、更新包保留及渠道/安装协调等明确问题仍在处理。全量最终检查、全新独立增量两轴和Release尚未完成；任务13/14真实桌面门槛未满足。formalRepairUsed保持true，不再次派Pi。

## Codex F16 直接修复检查点（待最终增量审查）

到期令牌在client_for发出业务请求前刷新；访问/刷新令牌与到期时间以同一凭据条目原子轮换，兼容迁移旧独立条目，登出与刷新串行且清理失败明确报错。普通账号配置不含令牌；OAuth刷新禁止重定向，不自动重放业务写请求。设备授权scope沿GitHub App权限模型处理。官方契约与独占测试细节见任务06新增Comments。

`github-auth-cargo-05`顶层1项集成测试通过，包含四个独立子进程的成功轮换/拒绝/刷新令牌过期/重定向场景，成功场景另验证重建GithubState读取及并发复用，各场景验证存储、登出失败/恢复和旧版迁移；嵌套harness计数不相加。`github-unit-cargo-01`32项通过（含类型导出），`command-log-unit-cargo-01`通过。前端构建`codex-frontend-build-03`与`codex-x18n-02`exit0，翻译检查0error/69warning。全部原始证据逐轮保留，测试只使用假令牌和独占文件存储/loopback服务，不操作真实用户钥匙串。真实GitHub/系统钥匙串桌面验收仍未进行；M7整体仍未完成。

## 版本历史

| 提交 | 内容 |
| --- | --- |
| `4657407` | 初次实施（任务 01–12），首轮两轴审查对象 |
| `f924719` | Codex 审查 Comments / 状态纠正 / backlog 15（Codex 提交） |
| `70c59ac` | **正式返修**：F01–F22 与 H1–H9 修复（Q113 唯一正式返修窗口） |
| `56c1f13` | 返修处理结果记入任务 Comments，任务状态改 ready-for-review |

## 返修对照（审查 review-01-oim6d9nj → 处理 → 证据）

| 发现 | 处理 | 生产入口 / 证据 |
| --- | --- | --- |
| F01 P1 玻璃层 200×80 | GlassLayer 显式 width/height 100% + !important 优先级修正；popper 玻璃语义 | `src/features/m7/glass/GlassLayer.tsx`、`src/index.css`；冒烟 smoke-repair.json windowLayer=true、布局正常 |
| F02 P1 对话框/设置漏接 | 主分支补挂 Clone/Diagnostics/UpdatePanel，Settings 补 4 props；删除重复 CloneDialog | `src/App.tsx` 主 return 分支 |
| F03/H1 P1 切换丢草稿 | 卸载同步落草稿；基线变化保留两版走冲突流程 | `EditableWorkspace.tsx` 卸载 flush + 恢复分支 |
| F04/H3 P1 草稿未纳入写门禁 | 暂存/取消暂存/丢弃/提交在脏编辑时拒绝（双语错误）；局部放弃/保存分支修正 | `src/App.tsx` blockOnDirtyEdit 包装；`EditableWorkspace.tsx` confirmClose |
| F05 P1 终端会话恢复 | 重挂载认领运行中会话；绝对 cwd 恢复；切 worktree 隔离 | `TerminalPanel.tsx` claimOrOpen；`terminal.rs` resolve_cwd + 回归测试 cwd_tests |
| F06/H9 P1 关终端无确认 | 运行中页签关闭内联确认 | `TerminalPanel.tsx` confirmCloseSession |
| F07 P1 审阅/合并丢 remote | 请求结构带 remote，命令层拒绝缺省回退 | `ReviewSubmitRequest/MergeRequest.remote`；`PrContext::new(request.remote)` |
| F08 P1 迟到响应覆盖 | 请求代际 + 目标身份核对，切目标清状态 | `GithubPanel.tsx` requestEpoch/listEpoch |
| F09 P1 草稿键不匹配 | ReviewDraft.head_sha；m7_github_repo_key 命令；列表按 head 过滤 | `store.rs` / `commands.rs` / `GithubPanel.tsx`；绑定重新生成 |
| F10 P1 保护并集/缓存覆盖 | 失败保留旧缓存；推送/预览用并集（protected_branches） | `pr.rs` protection_state 分支重写；`commands.rs` protected_with_github |
| F11/H4/H5 P1 检出先改分支 | 写门最先进；merge-base 快进判定；非快进需 overwrite_confirmed + 前端确认对话框 | `pr.rs` checkout 重写；`GithubPanel.tsx` divergedBranch |
| F12 P1 诊断原样导出 | 按 worktree 过滤 + 字段白名单（error_class + 单行摘要）+ email 脱敏 | `diagnostics.rs` 重写；新增 5 项单元测试 |
| F13 P1 更新状态机 | 检查/下载/已下载/安装/重启分开；安装前完整协调（写/待核对/终端/编辑） | `update.rs` download_update + install 校验 downloaded_version；`UpdatePanel.tsx` phase；App beforeUpdateInstall |
| F14 P1 无更新清单 | tools/generate-latest-json.mjs + release.yml 生成上传 latest(-prerelease).json | fixture 实测 `repair/latest-json-test.out`；YAML 解析通过 |
| F15 P1 公钥编码 | minisign 全文/base64 统一转 base64，坏输入 CI 即失败 | fixture 实测 `repair/f15-*.log`（真实格式 exit 0） |
| F16 P2 设备码取消/刷新 | cancel 命令 + 取消按钮；refresh_token 持久化（独立 keyring 服务）与登出清除 | `auth.rs` / `mod.rs` TokenStore；`m7_github_device_cancel` |
| F17 P2 冲突替换/提交框 | 冲突确认替换 UI；提交框 matchesAccelerator(acceleratorOf("commit")) | `settings.tsx` pendingReplace；`CommitComposer.tsx` |
| F18 P2 fork 来源 | validate_pr_head 接受 owner:branch（测试）；创建表单来源远端选择 | `pr.rs` / `GithubPanel.tsx` |
| F19 P2 分页/裁剪 | files/comments 按 Link 翻页；patch 不裁剪；完整 diff 入口 | `ApiMeta.next_page`；`GithubPanel.tsx` fullDiff |
| F20 P2 统一视图只读 | 编辑入口仅 split+unstaged；统一视图引导切 split | `App.tsx` 编辑按钮条件重写 |
| F21 P2 摘要 7 天/清空 | DiagnosticsSummaryStore；预览/导出同快照；清空命令与按钮 | 新命令 m7_diagnostics_summary/clear；`DiagnosticsDialog.tsx` |
| F22 P2 渠道 | 移除 automatic；allow_prerelease 设置 + 双清单 endpoint 切换 | `settings.rs` / `update.rs` / `check-release-version.mjs` |
| H2 P2 保存写生命周期 | 保存走生产写入口（持写锁）；更新安装门禁协调 | `edit.rs` 既有写锁；App 门禁 |
| H7 P2 错误结构 | edit.rs cause 结构化；PrMergeOutcome.error 载荷；前端 formatAppError | `edit.rs` / `pr.rs` / `GithubPanel.tsx` |
| H8 P2 克隆旁路 | CloneManager 读设置 Git 程序 + 写统一命令日志 | `clone.rs` run 记录；`commands.rs` AppState |
| H10 P3（顺带） | dmg 重命名约定 + macos-SHA256SUMS.txt | release.yml Rename/SHA256SUMS 步骤 |
| H11 P3（顺带） | hunk → 块（经 x18n） | locales 双语已改 |

P3 backlog 项（15-m7-review-maintenance.md）未扩大重构；重复 CloneDialog 随 F02 自然消除，已在任务 01 Comments 记录。

## 最终验证（受控 runner，真实 exit）

| 检查 | 命令 | exit | 结果 | 原件 |
| --- | --- | --- | --- | --- |
| Rust 全量测试 | `cargo test --manifest-path src-tauri/Cargo.toml` | 0 | **772 passed / 0 failed / 1 ignored**（单轮完整，无拼接） | `repair/logs/cargo-test-final-repair.log/.exit` |
| 类型检查 | `pnpm exec tsc -b` | 0 | 0 错 | `repair/logs/tsc-final-repair.log/.exit` |
| Lint | `pnpm lint` | 0 | 0 错 | `repair/logs/lint-final.log/.exit` |
| i18n | `npx @x18n/cli check` | 0 | 0 错 69 预存警告 | `repair/logs/x18n-final.log/.exit` |
| 前端构建 | `pnpm build` | 0 | 成功 | `repair/logs/fe-build-final.log/.exit` |
| Release 构建 | `cargo build --release` | 0 | 成功 | `repair/logs/cargo-release-final.log/.exit` |
| 隔离冒烟 | REVIA_SMOKE=1（独占 HOME/appdata/fixture） | 0 | ok=true；玻璃整窗挂载、并排色带、视图切换锚点、M7 IPC 全通 | `repair/logs/smoke-repair.json/.exit` |

Release 二进制 sha256：`1e6d3f1c83164a07c25a77591b630eddfd2e5f63492c5a8f80c8bdbda64951fb`。

返修期间一次 m6_flows 15s 超时（负载抖动，与首轮相同用例），全量重跑该套件 exit 0（`repair/logs/m6-full.log/.exit`）；最终 cargo-test-final-repair 为单轮完整 772/0。

## 逐任务状态（返修后，待 Codex 重核）

| 任务 | 状态 |
| --- | --- |
| 01–12 | **ready-for-review**（返修已完成，勾选保留待 Codex 按原始规格逐项重核；不据旧勾选宣称 done） |
| 13 更改列表 | **needs-info**（不变：Rebased 1.1.22 终端已有暂存内容实测未取得） |
| 14 整包验收 | **needs-info / blocked**（依赖 13 与真实桌面） |

## 仍未验证（如实声明）

- 强折射（SVG 滤镜 / WebGL）在真实 WKWebView / WebView2 的视觉证据：未取得，留任务 14。
- 真实 GitHub 账号 / 钥匙串 / 设备码完整流：留任务 14 与外部接线（App client_id 未配置）。
- 受控 HTTP fixture 对 GitHub API 的请求目标 / 次数 / 版本变化 e2e：本轮未建，静态与单元级覆盖（返修窗口已用尽，留 Codex 处置）。
- 真实 CI 运行、实际签名更新安装、首次下载启动：未执行。
- Windows 实机外观、D06 长测、Rebased 对照桌面验收：任务 14。
- 任务 09“完整分页”按单分支 protection API 语义实现；若规格口径为分支列表分页需 Codex 裁定。

## 会话与渠道

- Paseo `8a5673e4-a7bb-4361-8307-6f523c81ee31` / Pi `01a18ce0-67c3-74fa-9c20-585fd434e10e`，模型 magpie/cpa/GLM-5.3-Flash。
- 渠道 502 原件：批次 `flash-channel-error-kedbp795`；首轮交付原件：`review-01-oim6d9nj/delivery-evidence-audit.md` 与 `pi-delivery-original.jsonl`。
- formalRepairUsed=true（本轮已使用唯一正式返修窗口）。

## 审查与验收

由 Codex 对返修候选 `70c59ac`（+文档 `56c1f13`）做复核与真实桌面验收（任务 14，含任务 13 事实门槛）。实施方不自行宣告整包通过。

### Codex 直接修复 F10 — 2026-10-31

保护同步改为读取 `branches?protected=true&per_page=100` 的完整分页集合；独立同步入口不依赖存在或打开PR。只有全部页成功才替换；401/403/404/429/网络、坏JSON、字段缺失和中途失败保留旧集合与原时间并标记可能过期。首次无缓存显示尚未同步。缓存绑定真实worktree身份、账号、remote及GitHub仓库，旧无身份缓存保留原件而不自动认领。请求代际与原子落盘共锁，旧请求不能覆盖较新成功或失败。

实际推送按固定的目标remote与本地列表取并集，生产强推入口拒绝保护分支；历史改写确认显示当前账号各GitHub远端与本地保护列表的并集。普通本地历史与推送只读账号资料及缓存，不访问或刷新API凭据。不修改远端保护规则。

原始证据在 `/private/tmp/revia-m7-batch-g5scyw0r/codex-checks/`：`protection-cargo-02` 7项通过，真实loopback HTTP及独占Git/HOME/appdata覆盖完整分页、清空替换、失败保留、迟到响应、账号/remote/仓库隔离、重建与权限失败；直接调用生产强推判断验证并集。`protection-cargo-01`既有branch_remote31项与PR检出8项通过，但本组6通过/1失败（测试HTTP服务把无请求空连接的读取超时当失败）；修正服务后仅重跑受影响7项，完整失败原件保留。`protection-lib-01`现有GitHub库32项通过；`protection-bindings-01`导出1项通过。前端`protection-frontend-build-01`、`protection-lint-01`和`protection-x18n-01` exit0，翻译0错误/76个既有unused-key警告，不删除保留键。源码hash、精确argv、真实exit和分流stdout/stderr逐轮保存；x18n首次沙箱缓存EPERM也保留，非产品检查失败。

官方branches文档原件在`impl/protection-official.json`。本组仍是中间检查点；F08/F09与F13/F22尚未完成，任务13/14的真实桌面事实仍缺，不称整体通过或最终候选。

### Codex 直接修复 F08 — 2026-10-31

PR面板的列表、详情、评论提交/重载、审阅回执、完整diff、草稿保存/删除、合并、检出、创建及授权回包均按原目标与请求代际落状态。目标包含worktree、账号、remote及其仓库URL；详情另含PR。切换后旧成功、错误与finally均不能污染新目标或清掉新操作busy；刷新同一PR也使旧显示失效。账号设置完成时发出本地变更通知，面板重新核对；卸载失效全部旧请求。工作区检出仍由原请求进入既有全局写门，迟到的显示不会刷新当前另一worktree。

`github-requests-node-01` 8项生产GithubRequests/githubResponse竞态回归通过，验证账号/worktree/remote/URL/PR切换、A→B→A、同类重复请求、独立通道、错误与卸载；不是桌面验收。`github-requests-frontend-build-01`与`github-requests-lint-01` exit0，原hash/argv/真实exit/stdout/stderr均在批次codex-checks。该组没有新增locale键。

F09持久正文、账号/remote/PR/head草稿身份与发布前版本复核仍独立待修，不能把本组通过当作F09完成。F13/F22和任务13/14真实桌面也尚未完成。

### Codex 直接修复 F09 — 2026-10-31

本地草稿由Rust派生真实worktree身份、账号、remote与GitHub仓库、PR号及head组成的scope，不接受前端repoKey认领。评论正文、审阅正文/事件、行内输入表单和已加入的行内草稿都保存在本机；切PR/账号/remote或隐藏面板仍保留。重新打开按实际详情head装入；旧head稿件单列只读供取回，不混入当前发送集合，旧无身份记录保留而不自动认领。存储读改写共锁，坏文件或写失败不以空集合覆盖；已移除静默截断未发表稿件的上限。

前端生产ReviewDraftSession按scope串行保存并保留失败正文。新成功覆盖旧失败状态；发送期间新输入不被成功回包清掉。App级登记在面板隐藏后仍参加正常退出冲刷与原生退出保护，失败保留窗口。发布前先冲刷本地稿、核对账号/remote/真实仓库/head，Rust再次读取PR head；行内id必须属于同一scope和提交，评审POST显式绑定`commit_id`，不自动改用新head。成功只清理实际发送且内容/锚点/时间仍相同的稿件，旧版本及新输入保留；远端成功而本地回执/清理失败返回结构化NeedsVerification，禁止自动重发。

原件在批次`codex-checks`：最终`review-drafts-cargo-02`新10项+共享保护7项通过，使用独占真实Git/HOME/appdata与loopback假账号HTTP，覆盖旧版本混发拒绝、身份隔离、真实PR版本变化、POST commit_id、发送期间新正文、精确清理、并发写、坏存储、权限失败与远端成功后本地失败。`review-drafts-lib-01`GitHub库34项含新绑定导出通过；最后存储比较细化后`review-drafts-store-01`2项通过。`review-drafts-node-04`49项全部通过（新草稿8、目标请求8、编辑13、终端12、共享关闭写门8）；实际生产模块与协调器，不是桌面替代。`review-drafts-frontend-build-03`、`review-drafts-lint-02`、`review-drafts-x18n-01` exit0；翻译0错误/76个既有unused-key警告。

前端首轮`review-drafts-frontend-build-01`真实exit2，空表单headSha:null不符合已有可选string绑定，修正为空缺字段后重跑通过。最早Node轮缺既有Rust错误证据环境变量而跳过1项，补齐后最终49项无跳过。所有中间轮和原stdout/stderr、精确argv、源码hash、真实exit均保留；不把中间轮算作最终候选通过。测试HTTP helper两项未用函数产生非阻断dead_code提示，未清理后谎称复用旧通过。

[GitHub官方review接口](https://docs.github.com/en/rest/pulls/reviews#create-a-review-for-a-pull-request)已核对commit_id省略时默认最新提交；本组显式固定原head。Grok连接失败后用可用官方网页读取渠道，原件`impl/review-api-official.json`。这是中间检查点，F13/F22更新包/渠道/安装重启协调仍未完成；任务13/14真实桌面与真实账号/钥匙串仍未验。

### Codex：任务13实现与集中检查（2026-10-31）

基于上方三组真实 Rebased 事实实现更改列表；设置默认仍为暂存区。应用数据按 worktree 身份保存默认列表、创建/重命名/删除、整文件/原始 Git 块单一归属与勾选；切换不删组。失配块保留原组和预览，取消勾选。文件与块均可移动，提交成功只移除本次已提交条目。

选择提交使用系统 Git 生成的 HEAD→工作区原始块建立独立 index，执行前再次核对真实 HEAD/index/操作状态/文件版本及 worktree 身份；沿用现有提交输出、快照、hooks、草稿和退出保护。成功后只用 Git reset 更新所选路径的真实 index，未触及路径的原暂存不混入提交且保留，工作区不被写入。原 index entries/binary patch、选项及独立 index 原件保存在应用数据 changelists/commit-evidence；失败不自动续跑、不删 index.lock。收尾失败不清草稿，报告待核对。所选文件的编辑草稿经现有入口处理；不要求处理未选文件的草稿。

`task13-rust-03`：42通过（新更改列表10、既有commit_flow12、file_staging20），覆盖不同路径已有暂存、同文件部分暂存、二进制/未跟踪/删除、归组持久化与默认组、外部修改失配、锁保留、hook失败、初始提交和仓库身份替换。此前task13-rust-01为40通过，02为42通过；所有轮次保留。`task13-frontend-01`、`task13-lint-01`、`task13-x18n-01`均exit0，x18n 0错误/76既有unused-key警告；lint有既有非阻断提示。x18n首次沙箱缓存写入EPERM的原件在task13-locale-query-01，提升后查询/维护通过。绑定生成task13-bindings-01通过。

后两次产品变化仅Rust收尾与命令相关编辑范围，已重跑上述Rust组；前端与locale检查后无对应源码变化。不把这些CLI检查称作桌面通过，Windows未实跑。下一步固定本增量、按Q174新两轴审查，再重建Release和任务14真实桌面；旧Release不适用于本次新源码。

### review05：任务13增量正式两轴已收齐

Standards：3项P2、2项P3；Spec：6项P2、1项P3（只读能力边界在两轴各自原文保留，不合并计数）。正式报告分别逐字入任务13/14 Comments。均为静态分析，未把CLI检查当桌面通过。Codex集中修明确P2；P3/smell登记backlog15，不扩大重构。产品待重新检查、固定增量审查及新Release后继续真实桌面。

### Codex：review05明确问题处理与验证（2026-10-31）

- 两轴只读能力问题：更改列表复用status能力表并补工作区符号链接/嵌套仓库检查；返回结构化只读原因，前端禁勾选，后端写前再次拒绝。真实本地子模块、已跟踪/未跟踪符号链接回归通过。非UTF8路径仅生成只读行，不隐藏其他有效文件。
- Standards全量diff传输：移除每项整份patch正文与原生滚动pre；列表只传至多200字符的摘要，点击交给既有按范围读取/自绘滚动的diff与编辑器。
- Standards语言：错误保存AppErrorPayload，渲染时按当前语言格式化。
- S05-01：从同一次系统Git hunk按既有segment划分独立块；同hunk多个已选块合成一次原始上下文补丁，支持分别归组/选择。近邻两块独立提交、三块选其中两块的真实Git断言通过。
- S05-02/03：接回文件预览/固定页签、鼠标与键盘多选、现有丢弃确认和逐文件回收入口；不在新模式禁用批量丢弃。
- S05-05/06：沿用现有operation.commit_allowed门禁，允许真实rebase编辑停点amend/新建提交与无冲突merge收尾；空选择amend只改信息且保留其他原暂存，空paths绝不执行全index reset。
- 在相同行内接线中同时恢复Codicons/Git状态色；保存无关设置不再清空父勾选凭据。两项先前P3已按实际修复记录，名称校验重复的smell仍留backlog，不做扩展重构。

原件：`task13-review05-rust-01` 50通过（新18+既有commit12/文件暂存20）；`task13-review05-segments-01` 49通过（hunk24/segment25）；`task13-review05-node-01` 8通过。最后新增非UTF8只读分支后，`task13-review05-rust-03` 本机18通过；其他生产相关源码未再变化。新增非法字节文件名用例在02轮创建fixture时被macOS APFS以EILSEQ拒绝（18通过/1失败），保留全部stdout/stderr/源码散列；该文件名用例现仅在支持它的非macOS Unix编译，本机不能冒称实跑。总相关Rust回归99项跨上述轮次核实，不称单一全套执行。

前端首轮因鼠标事件使用键盘修饰键类型报TS2345，原失败保留；改用ChangesTree既有isPrimaryClick。最后`task13-review05-frontend-03`与`task13-review05-lint-03`均exit0。locale键集/译文未改，既有task13-x18n-01为0错误/76警告，所有新增UI使用已查询键。生成绑定原件task13-review05-bindings-01。Windows和本修复的真实Revia桌面尚未运行。下一步仅新增修复范围独立两轴，审修后重新构建Release；不复用旧二进制验收。

### review06正式两轴

Standards1P2（预览与选择基线不一致），Spec2P2（可见列表顺序、多处相同正文误识别来源）；原文分别入13/14 Comments。Codex继续直接修：提供明确HEAD→工作区diff源，列表范围选择按真实可见行计算。未构建新版Release，Revia尚未启动，不冒称桌面通过。

### Codex：review06预览基线与可见顺序修复（2026-10-31）

H06-01/S06-02：新增明确的 `ChangesKind::Changelist` 只读比较源。启用更改列表的会话额外缓存系统Git HEAD→工作区模型；原staged/unstaged/untracked条目及指纹保留供既有安全写入口。列表、分页行、并排文本与高亮的旧侧均取HEAD、新侧取工作区，选择补丁也调用同一 `Git::diff_changelist`；删除按正文签名猜staged的分支。此源不签发暂存/hunk/segment写凭据，工作区一侧编辑仍复用既有编辑会话。模式切换只改变应用设置与读取模型，不改变真实index。

S06-01：行身份为group:path，点击范围、Command/Ctrl多选、Shift箭头及全选都使用实际可见顺序；文件跨组显示有独立行身份。主diff文件位置/导航按同一可见顺序去重文件。批量/键盘丢弃把实际选中文件投影到原unstaged/untracked条目，继续沿用原确认、指纹和回收流程；不借内部比较源改写Git。

`task13-review06-rust-01` 六targets共134项通过（commit12、文件20、hunk24、更改列表21、审阅32、segment25）；`task13-review06-session-01`会话/刷新隔离18项通过，共152相关Rust检查。新增真实A→B暂存→C预览、相同正文两位置、普通/更改列表模式切换断言，均通过生产WorktreeSession入口读取实际Git内容。`task13-review06-node-02`13通过（可见顺序与导航5、既有提交8），`task13-review06-frontend-04`与`task13-review06-lint-03` exit0。后续前端类型/导航修正未改Rust源码，未重复已过Rust。locale键集/译文不变，复用已查询的HEAD/工作区标签；既有x18n为0错误/76警告。

前端01/02先后暴露新增类型映射遗漏，TS错误与原始源码hash/argv/exit/stdout/stderr全部保留，未算通过。所有原件位于codex-checks/task13-review06-*。非法字节文件名用例仍仅非macOS Unix条件，本机与Windows不冒称实跑。未启动本候选Revia、未构建新版Release；下一步仅此新增量独立两轴，收齐报告后才固定新版Release与真实桌面。

### review07正式结果

Standards2P2/1P3，Spec3P2：新读源尚未接入刷新/实际文件导航/外部diff。原文分别入任务13/14 Comments。Codex将收口当前模式读源与可见顺序路由，补外部diff的HEAD基线与原保护。尚未Release或启动Revia，不能沿用旧候选通过。

### Codex：review07消费入口修复与验证（2026-10-31）

H07-01/S07-01：新模式初始打开、刷新保留、重开与按路径定位都走严格Changelist/Untracked读源；待分组元数据加载时也只回退到当前模式源，绝不回到index基线。模式保存在ref供异步回调读取当前值，切换时立即更新；普通模式原规则保留。

H07-02/S07-02：实际focusNextFile工具条/F7过尾与reviewNextFile都接同一entryOrder，不再直接索引隐藏snapshot.entries。审阅异步导航还绑定发起模式和列表revision，换模式/归组后取消旧导航。共同查询提炼至visibleChangelistSelection.ts，组件与主路由复用，顺带消除上轮P3重复查询判断，没有扩展重构。

S07-03：外部diff显式验证Changelist路径的HEAD→工作区模型，仍执行原身份/确认上下文/指纹/保护快照门禁；Git difftool用同一固定HEAD（unborn时empty tree），不使用--cached。Git::changelist_base给内外diff共用。真实Git/difftool fixture捕获LOCAL为HEAD原文、REMOTE为工作区C，确认未把index中间B当作左侧；原index与工作区内容保留。过期请求在工具启动前拒绝，命令日志没有difftool启动。

`task13-review07-rust-01`共156通过（m6_flows133、更改列表23）；`task13-review07-node-01`16通过（可见顺序/路由8、既有提交8）。最后仅增加前端异步模式一致性检查，Rust与生产选择模型未再改，未重复已过项；最终`task13-review07-frontend-02`、`task13-review07-lint-02`均exit0，`task13-review07-x18n-01`0错误/76既有unused警告。argv/源码hash/真实exit/分流原件位于codex-checks。Windows、非macOS非法字节路径与本候选原生Revia尚未实跑，不冒称桌面通过。下一步固定本增量、全新两轴，仅审新增修复，完成后再构建固定Release。


### review08收齐与新版Release准备（2026-10-31）

产品`37c1f441d77d08c874dee930281d961bfbec4bae`，只审`ee8f662…37c1f44`九路径新增量。Standards 0硬违反/0新增判断，Spec 0项P1/P2、1项P3边界；独立原文分别入13/14 Comments，S08-B01入backlog15。Codex直接核对diff和检查原件，156Rust/16Node及最后frontend02/lint02/x18n01有真实exit0；App晚加异步模式一致性门经过最终前端检查，Rust和生产选择模型未再改。任务13转ready-for-human，尚未声称Revia桌面通过。下一步从本次完整源码构建并固定新Release，再真实13/14桌面；旧Release不能复用。


### Codex：真实桌面局部提交后的归组收尾（2026-10-31）

固定Release `b5ddf9f` 的真实提交 `5f75fdcb` 只含line30，HEAD/index/剩余工作区均核对正确；分组/默认组/选择重启保持。但line4未提交块因整文件patch头OID改变而被标失配，新的line4落入不同默认组。原件 `desktop-acceptance-myntirt_/evidence/revia-native/post-commit-group-loss` 及Git快照保留。Codex直接修：仅在本次提交收尾，对所选路径中未提交块按工作区位置和Git原始改动字节迁移原分组；重复正文仍按位置区分，不继承旧勾选，不放松完整文件写前指纹。相关修改完成后集中回归、固定新候选和Release再真实复验；当前不标任务13 done。


归组修复集中验证：`task13-native-group-rust-01`真实exit0，共36项通过（m7_changelists24、共享commit_flow12），含原生同文件分组选不同默认组的场景、同hunk剩余块和前方插入导致旧侧行号移动的回归。原始argv/源码hash/分流输出保留。仅改Rust私有收尾与相关回归，无locale/绑定/前端生产源码变更；下一步仅此增量独立两轴，再重新固定Release并实际复验，不沿用旧桌面作为新版本通过。


### review09结束，原生归组修复转Release复验

产品`0dc11c07e6926b5eef1f756fe2bcff9f0901b29d`，仅四路径新增量。Standards0硬违反/0判断、Spec0P1/P2和1P3构造边界；原文分别入任务13/14 Comments，S09-B01仅backlog15。36项Rust原记录已核对，未改前端和locale。下一步从新候选重新构建固定Release，用新的独占fixture复验原生归组收尾及继续14；此前fixed-release-native-7d8tke5w只作为历史证据。


### Codex：真实桌面场景集中修复与验证（2026-10-31）

固定 Release `5ca9178` / 产品`0dc11c0`已完成四主题及中英文900px真实查看；系统深色测试在finally中恢复原浅色。手动编辑Save前磁盘未变、Save后实际字节正确，草稿跨文件切换保留，外部变化后拒绝覆盖并保留草稿；原生退出按未解决草稿确认，先保留原件再仅丢弃fixture编辑，启动03真实exit0。本地clone实际目标`repos/native-clone-c_0_lf38/checkout`的HEAD与自建bare一致并自动登记打开。诊断原生导出的JSON不含文件正文标记与fixture邮箱。PR未登录和更新无签名公钥边界已观察，正式账号/保护/更新接线未验，D06尚未执行，不能将上述局部结果称整包通过。

原生发现并集中修复：更改列表view把元数据读取记成仓库写入，导致PTY逐字输入被拒及提交后Busy不能自动恢复；默认组未命名input冻结旧语言；设置玻璃透底文字干扰阅读。原始截图/AX/文件在`desktop-acceptance-myntirt_`，终端命令实际残缺并parse error，不能称PTY命令通过。

修复后元数据观察不再设置writing标志；真实写入开始/结束递增代际，只在同一空闲代际内发布元数据，读取不把仓库标为writing；最终发布的Mutex区仍包含身份重验证及其系统Git调用，可能短暂等待输入IPC，新版真实输入结果另行验证，跨写入结果拒绝。原写互斥与终端真实写入暂停保持。前端仅对明确Busy读取作三次有界重读，绑定原worktree/epoch且卸载取消；失败不继续显示可操作旧凭据。默认名用真实空name和即时翻译placeholder，避免翻译持久化成用户名称。统一浮层保留玻璃装饰/折射路径并增加主题tint与blur遮蔽透底，减少透明度/高对比度仍无滤镜降级。

`task14-native-scene-rust-01`真实exit0，共44通过（更改列表26、PTY3、会话3、共享提交12），含实际会话写代际与Barrier并发发布拒绝。`task14-native-scene-node-01`20通过，前端build/lint/x18n均exit0，翻译0错误/76既有unused警告。源码hash、argv、真实exit、分流原输出与失败原件保留；只改复用既有键，未改locale/绑定。下一步只审本新增量、固定新Release，再真实复验PTY/浮层/默认名与D06；当前不标任务14 done。


### review10完成与最终原生复验准备（2026-10-31）

Standards 0硬违反、2项P3判断；Spec 0新增发现。原文分别入对应任务Comments，S10-ST01证据口径已澄清，S10-ST02仅backlog不扩重构。当前产品`039fd1a3dbea7cf7e0c7bac91cd648509943dd41`保持冻结。另补`task14-native-write-guard-regression-01`文件20/块24/段25共69项真实通过；加先前44项，113项相关Rust跨两轮核实，不能写成一次全套。Node20与最终前端build/lint/x18n通过，无后续产品修改。下一步以新Comments提交构建task14-native-release-build-03，固定新Release再真实PTY输入/列表恢复/默认名/浮层和D06；Windows、正式GitHub账号/保护与更新公钥接线未验，整包不done、不合并或删除worktree/监控。


### Codex：终端视图生命周期修复与原生停点 — 2026-10-31

固定 Release source `78c5658e4b1b370266e2044992e5de3787cd86ac` / 产品 `039fd1a` 的原生复验仍发现95字符输入丢字；原件 `final-pty-running-01` 保留，Python因路径残缺未启动。另一次聚焦提交说明后文本进入终端，不能将其当输入框通过。源码每次render产生新的ref回调，React detach时dispose xterm，随后重新读取snapshot并focus。

仅修复该生命周期：每个页签使用稳定ref，字号/字体通过现有更新路径同步，不重建视图；只有实际激活页签才请求焦点，布局回调检查实例仍存活。PTY后端、写保护、Git与locale均未改。`terminal-viewport-node-01`现有终端所有权/关闭12项通过；`terminal-viewport-frontend-01`、`terminal-viewport-lint-01`真实exit0。未新增以源码形状为断言的测试，相关UI仍须新固定Release真实复验；不把现有Node检查当作React焦点验证。全部argv/源码hash/分流输出保留。

完整第二次输入曾成功运行独占Python进程，`pty-final-process-proof.json`记录PID14626/parent12952和独占HOME/cwd；首轮连续输入缺陷仍成立。原生页签关闭提供Terminate & close/Cancel，取消后CmdQ显示全局End processes and continue；只终止本批shell，`task14-native-launch-04`真实exit0。下一步仅此增量全新两轴，再构建固定Release、复验持续输入/其他控件焦点和D06；整包14仍未完成。
