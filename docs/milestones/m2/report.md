# M2 阶段报告

更新日期：2026-10-24。任务 01、02、03、04、06、09 已完成；任务 06 两轴复核、真实桌面、D06 与最终双平台 CI 已通过，旧前台尝试的取证限制已更正；M2 未完成。工作保留在 `m2`，未合并回 `main`；已按 Q97 正常推送 `m2` 用于编译验收，未创建 PR 或发布。

## 当前进度（2026-10-23）

- 任务12已交付4848265，限定独立两轴静态复核Standards0/Spec0；[16份检查原件](./evidence/task12-review01/manifest.json)已归档、3份源码SHA匹配，tsc/lint/build与Rust247项通过/1夹具ignored。仍ready-for-human，普通/测量模式、字号记忆、环境错误、D06及新版双平台CI尚待07复核全过后统一固定版本验收，不标done。

- 任务07第三轮复核仍未通过：固定`ed0f9b8...ff2fc64`，Standards4项P2、Spec2项P2和1项P3，分别保留在[07 Comments](./issues/07-persistent-review-marks.md#comments)。未知版本新增标记、未初始化子模块、共享身份的内嵌换行路径解析及packed-refs真实回归仍待原Pi修复；旧身份重解析、选择代际、错误留存、完成提示原路径已静态闭合。[原检查与续跑证据](./evidence/task07-review03/manifest.json)已归档，5份源码hash匹配ff2fc64，tsc/lint/build与Rust247通过属该版；保留原build中断、敏感性无独立输出边界。仍未Release/桌面/D06/新双平台CI/长测，07退ready-for-agent。

- 任务05限定身份测试返修`ebe9f92...3d70aa8`两轴0/0，测试证明缺口静态闭合；[7份原件](./evidence/task05-ci03/manifest.json)已核验，macOS discard_recovery71通过/1ignored，敏感性实验只叙述不冒称独立证据。仍ready-for-human，Windows和完整桌面/D06未完成；待07复核全过再固定新版本统一CI/Release。

- 任务07再次退回`ready-for-agent`：固定`0dd6525...84a17a8`第二轮独立复核Standards5项P2、Spec3项P2；子模块真实Git版本、实际身份指向、新选择代际、原worktree失败留存和完成提示归属仍待原Pi返修。正常单应用跨worktree事务已闭合，不扩成未确认多进程要求。[16份原始检查](./evidence/task07-review02/manifest.json)已归档，10份源码hash匹配84a17a8，tsc/lint/build与Rust245项通过仅属该版；未做Release/桌面/D06/Windows/长测。详见[07 Comments](./issues/07-persistent-review-marks.md#comments)。

- 任务05退回`ready-for-agent`：固定`1056557...05513aa`的Windows返修增量两轴各1项P2，均为身份回归同时改字节、仅通用Rejected，可被内容保护兜底；不是已证明生产身份校验失败。[7份检查原件](./evidence/task05-ci02/manifest.json)已归档、3份源码hash匹配05513aa，macOS Rust240通过；Windows仍未验证。待当前07返修交付后原Pi单独补此用例，复核过后再新CI/桌面，不提前done。详见[05 Comments](./issues/05-safe-recovery.md#comments)。

- 任务07退回`ready-for-agent`：固定`234f248...d892bae`首轮独立两轴复核Standards8项P2、Spec8项P2，分别列在[任务07 Comments](./issues/07-persistent-review-marks.md#comments)。共享存储事务、活动/未知身份、所见版本与迟到响应、去重/导航、按钮焦点、完成提示、错误可见及子模块版本尚待原Pi返修。首版15份[原始检查](./evidence/task07-review01/manifest.json)已归档，10份源码hash匹配d892bae，tsc/lint/build及Rust239项通过只属于首版；首次t13失败仅实施叙述、无原始输出，不冒称独立核实。未做07 Release/桌面/D06/CI，复核全过才桌面。

- 任务05当前`ready-for-agent`：固定234f248的[CI40048580349](https://github.com/GarlandQian/Revia/actions/runs/40048580349) macOS全部通过，Windows有5项回收测试失败、release跳过，[完整日志](./evidence/task05-ci01/manifest.json)保留。第二次[普通桌面局部观察](./task05-desktop02.md)已确认R05-D1标签、两次二进制原生另存及旧确认失效；未执行恢复/清空，未完成D06/完整功能验收。07交付后仍交原Pi单独修复CI，复核通过前不继续功能桌面，不以旧版结果补位。
- 任务05最新`ready-for-human`：804d88d恢复/清空确认标签R05-D1限定两轴0/0，13份原始检查已[归档](./evidence/task05-desktop01-fix/manifest.json)，tsc/lint/build退出0。接着重新构建固定Release与桌面/最新双平台CI；旧de89796测量保留为历史，不倒填到新构建，不据静态标签修复关闭任务。
- 任务05当前退回`ready-for-agent`：固定de89796 Release普通UI首轮发现恢复/清空最终确认按钮误写“保存到回收区并丢弃”（R05-D1），仅观察后取消、未执行写入，前后工作区/暂存区语义项/记录不变。已[归档原始证据](./evidence/task05-desktop01/manifest.json)，继续原Pi限定修复，复核后再桌面。此前四组自动性能与夹具0/0结论保留，不冒称功能验收完成。
- 任务05验收准备：de89796固定Release已重新打包，SHA`05316c294400392019e7215cd1f329041d48082fd8d9f527e178a026fed20268`；[四组可见窗口自动测量](./task05-perf01.md)均ok，标准首屏117/111ms、真冷文字DOM5/7ms，60屏无未覆盖，保留23ms慢帧。Pi生产入口桌面夹具在8e8a16b限定两轴0/0，新清单5份原字节SHA一致；原错误证据保留。普通UI/恢复功能桌面和对应双平台CI未完成，仍ready-for-human，不替代任务11最终Q98长测。
- 任务05最新：`ready-for-human`。固定`7e5a2c8...e217c1f`第七轮限定复核Standards0/Spec0；此前外部结果留存和门禁实际持有者问题静态闭合。App.tsx检查前SHA匹配e217c1f，tsc/lint/build原始退出0，13份日志已[归档核验](./evidence/task05-review07/manifest.json)。将固定Release版本进入真实桌面/D06/对应双平台CI，尚未done；未以静态或类型检查替代运行时验收。
- 任务05最新：`ready-for-agent`。固定`69d6527...7e5a2c8`第六轮两轴各1项P2：目标已有写入时另存被拒绝，但finally误清他人前端门禁；后端锁仍在，不冒称并发写已发生。外部结果留存原问题静态闭合，继续原Pi限定返修。Pi先前临时日志路径有误，真实目录1792800590的13份原件已在f7f5b4归档，规划方核对SHA及tsc/lint/build退出0。未Release/桌面/D06/CI。详见[任务05 Comments](./issues/05-safe-recovery.md#comments)。
- 任务05最新：`ready-for-agent`。固定`73791d6...69d6527`第五轮独立复核：Standards1项P2、Spec1项P2，均为合法外部另存没有App级结果留存，关闭面板后迟到失败/待核对丢失；继续交原Pi限定返修。已登记目标归属贯通、返回列表busy、完成journal同步失败三项原路径静态闭合。7份检查前源码hash匹配69d6527，原始日志可核Rust70+94及tsc/lint/build通过；敏感性实验只有实施方叙述，不冒称已独立核实。未构建Release、未桌面/D06/CI，M2未完成。详见[任务05 Comments](./issues/05-safe-recovery.md#comments)及[原始检查](./evidence/task05-review05/manifest.json)。
- 任务 12：`ready-for-human`（实现与自动检查完成，等待两轴复核与桌面）。只做已确认三项：（1）`MeasureConfig` 新增 `measurementMode`（`REVIA_MEASURE_LOG`/`REVIA_MEASURE`/`REVIA_SMOKE`/`REVIA_SOAK_SECONDS` 任一成立），前端只在测量模式下渲染 `<MeasurePanel>`；普通隔离 acceptance 实例不设这些变量，仍按普通模式不显示。隐藏只针对界面——`loggingEnabled`、测量记录、`write_measurement` IPC 与自动测量/冒烟/长测流程全部保留；（2）工具栏删掉 `gitVersion · status` 文本与对应 props，环境问题仍经既有 `ErrorBanner` 全局提示；（3）删掉工具栏 A−／A+ 按钮与 `onFontSize` prop，`⌘+`/`⌘−` 与字号持久化未改动。本轮实测：tsc/lint/build 退出 0，Rust **247** 项通过；未新增前端测试框架或 Rust 回归。**真实桌面与 D06 未验证**；未 Release、未 GUI、未跑 D06/CI/长测。日志见 `/private/tmp/revia-task12-zyZuUB`。详见 [任务 12](./issues/12-ui-cleanup.md)。
- 任务 07：`ready-for-human`（第二轮复核返修完成，等待复核）。固定 `84a17a8` 的 Standards 5 项、Spec 3 项已逐条闭合：（1）删除手工解释 `.git`/HEAD/refs 的第二套 Git 语义（ADR-0002），子模块版本改走 `git submodule status`，覆盖 packed-refs 与目录型 `.git`；读不到真实版本时不生成可认领常量（新增 `worktreeVersionConfirmed`，该类路径标记一律不认领）；（2）`WorktreeIdentity::revalidate` 让 Git 重新解析**当前实际归属**再比对，linked worktree 的 `.git` 改指另一现存 gitDir 也判为不明确；`capture` 合并为一次 `rev-parse` 保持 `t13` 进程计数断言不放宽；（3）导航绑定既有 `selectionEpoch` 与选择 gate，覆盖「B 尚未显示」与 A→B→A；（4）持久化失败始终记到原 worktree 错误通道，成功响应仍按激活隔离；（5）完成提示改为绑定 worktree 与当前进度，切换/取消/失效后不再显示旧提示；并更正 `ReviewStore.transaction` 的注释为进程内互斥锁（非文件锁）。本轮实测：Rust **247** 项（`review_marks` **15**）、tsc/lint/build 通过。上一轮检查目录的 build 曾中断（无退出码），本轮在其外新建目录重跑并保留该记录。**桌面未验证**；未 Release、未 GUI、未跑 D06/CI/长测。日志见 `/private/tmp/revia-task07-r3b-gkOB0F`。详见 [任务 07](./issues/07-persistent-review-marks.md)。
- 任务 05：`ready-for-human`（R05-CI02 身份回归补足完成，等待固定增量复核）。`publish_rechecks_identity_instead_of_reusing_captured_one` 原先把「重建身份」和「改目标字节」同时做，只断言通用 `Rejected`，内容检查可以兜底，无法独立证明 T10/Q87。现在重建 `.git` 时把目标写回**丢弃后基线**，显式断言 `!old.confirmed_match(new)` 且 `filesystem_ids` 变化，要求错误原因含「身份」，并保留未写入断言。敏感性对照：临时移除 `verify_publish` 身份核对后该用例**失败**（恢复被成功执行），随后实现按原字节恢复。本轮只改测试：`cargo test --test discard_recovery` 71 passed / 1 ignored，退出 0；未重跑前端与完整 Rust。**Windows 未验证**（本机通过不等于 Windows 通过）；桌面验收与最新双平台 CI 仍未完成，任务未 done。日志见 `/private/tmp/revia-task05-ci03-IaNg8P`。详见 [任务 05](./issues/05-safe-recovery.md)。
- 任务 05：`ready-for-human`（第三轮复核返修完成，等待规划方复核）。固定 `cb82836...6071b59` 的 Standards 9 项、Spec 11 项已逐条闭合：恢复改为「先核对真实身份/全部相关状态 → 准备临时内容 → 每次发布前重新核对全部相关路径与冲突 → 最后核对通过才记 Completed」，部分写入后的失败统一 `NeedsVerification` 并如实保留 `written`，尾部失败（含最终重读、历史读取）同样改回待核对；身份不匹配时连准备动作都不做，不会在别人的 worktree 里建出目录；另存保护**源与实际目标**的真实 `gitDir`/`commonDir`（含 `.git` 文件指针、`--separate-git-dir`、嵌套仓库），归属按最具体 worktree 取，命令与测试共用 `save_recovery_as_impl` 走真实锁；前端另存进入 App 全局写门禁与结果通道；面板代际与 busy 分离、确认绑定 worktree/activation/快照/记录 version、无当前 worktree 也能经 IPC 查看保存版本；文本 diff 只在确认不存在时用空侧，读取失败如实报错；trash 残留纳入可重试清理且失败可见。另查明 t13「未知指纹」的真实根因：并发直接保存时 Git 单次 `git diff` 会给出 index 行与 hunk 文本来自不同状态的内部不一致输出；`refresh_locked` 增加读取前后工作区 stat 一致性校验，不一致就不发布（移除校验后 40 次失败 4 次，加回后 15 次 0 失败）。本轮实测：Rust **223** 项（`discard_recovery` 68、`refresh_isolation` 15）、Node 68 项、Python 20 项通过，tsc/lint/build 通过；按交接要求未构建 Release、未操作 GUI、未跑 D06/CI/长测。原始日志与失败历史见 `/private/tmp/revia-task05-r3-1792792216/MANIFEST.md`。详见 [任务 05](./issues/05-safe-recovery.md)。
- 任务 05：`ready-for-human`（第二轮复核返修完成，等待规划方复核）。固定 `36a39a1...cb82836` 的 Standards 5 项、Spec 10 项已逐条闭合：恢复改为「准备临时内容 → 受控阶段 → **每个路径发布前**重新捕获真实身份/全部相关状态/冲突 → 发布」，多路径维护已写路径预期；写后任何阶段失败统一 `NeedsVerification` 并真实重读；恢复留证改为追加历史并进入 `list`/`inspect` 生产查看，重启后仍显示待核对，重新核对为 `Ready` 时可再次显式恢复（不再是一次性）；确认绑定 `version`（记录 + 丢弃回执 + 恢复历史），主快照刷新即失效；面板 `panelEpoch`/`requestEpoch` 绑定记录与请求代际；另存按**实际目标路径**取该 worktree 写锁与结果通道，并按 Git 解析出的真实 `gitDir`/`commonDir` 保护；自动清理失败持久化并可经 `recovery_maintenance` 在普通模式显示；文本 diff 支持任一侧缺失（`/dev/null`）；原 worktree 不可用时仍可查看保存版本。本轮实测：Rust **213** 项（`discard_recovery` **59**）、Node 68 项、Python 20 项通过，tsc/lint/build 通过；另定位并修掉 `refresh_isolation` t13 的真实原因（测试用非原子写导致 Git 读到半截文件），断言未放宽。按交接要求**未**构建 Release、未操作 GUI、未跑 D06/CI/长测。原始日志见 `/private/tmp/revia-task05-r2-1792786030/MANIFEST.md`。详见 [任务 05](./issues/05-safe-recovery.md)。
- 任务 05：`ready-for-agent`（首版审查返修）。固定 `5d9b781...36a39a1`：Standards 8项、Spec 9项，各自最严重 P1；存在无关临时名文件删除、在途清理丢失唯一记录、冲突文件恢复绕过只读、恢复中断/确认/结果门禁及恢复查看/另存/自动到期等缺口。下面的首版自动化统计不等于这些路径已通过。意见与受限复现证据已归档至 [任务05 Comments](./issues/05-safe-recovery.md#comments)，修复并复核前不构建桌面验收版、不标 done。
- 任务 01：`done`，MessagePort 泄漏修复及 Q98 长测结论不变，见下一节。
- 任务 02：`done`，最终键盘与跨仓库提示归属结果按用户交接补录，见 [任务 02](./issues/02-safe-file-staging.md)。未稳定复现的按键序列观察单独保留，不猜测根因或声称修复。
- 任务 09：`done`，`3f22230c60bf8772f9d1b5334dc8e785a8da4a77` 的 [CI #39869756281](https://github.com/GarlandQian/Revia/actions/runs/39869756281) 已完成，macOS Apple Silicon 与 Windows x64 的前端检查、Rust 测试及 release 编译全部通过。Windows 实际通过 66 单元 + 16 file_staging + 3 session_flow；测试修复增量的 Standards / Spec 双审均无新问题。最终集成仍需任务 10、11 再次验收，见 [任务 09](./issues/09-platform-compile-checks.md)。
- 按新规则先派给 Paseo 的 deepseek；本轮 3 次重试均为 `503 auth_unavailable`，会话进入 error 且未改文件，由 Codex 接手，不切 Gemini。已把 `main` 的 `837d68a` 规则合入 `m2`，主目录不变。
- 任务 04：`done`。四轮两轴修复／限定复核、第一轮真实桌面与重启151项断言、修复版RM两组/两视图边界及整文件stage/unstage、D06与额外104项断言通过。最终`764d13e`固定Release SHA及版本覆盖见[任务04验收](./task04-acceptance.md)：标准首屏113／99ms、真冷文字DOM4／5ms，四份各60屏60rAF、0兜底／未覆盖；原跳转与慢帧保留。最终[CI39972484556](https://github.com/GarlandQian/Revia/actions/runs/39972484556)双平台全部通过（macOS178／Windows161项Rust及release）。并发刷新旧/新各3次exact均通过但不能消除Pi间歇失败观察，已交任务10，不称已归因。05恢复与清理、11最终30分钟长测未完成；用户新图标后续纳入m2并验证。
- 任务 05：`ready-for-human`（实现完成，等待两轴审查、真实桌面与 CI）。实现「不覆盖后续修改的恢复与回收区清理」：恢复只写工作区（字节/存在状态/可执行位，临时文件 + rename），不动暂存区、不重放反向补丁；是否有后续变化以**丢弃完成后**的 `after` 为基准逐路径比对，分 `Ready`/`LaterChanges`/`NoBaseline`/`IdentityMismatch` 四态，只有 `Ready` 且身份确认时才允许直接恢复；有后续修改时文本只读对比、二进制只显示信息，另存用 `create_new` 一步拒绝已存在目标；清空需二次确认，14 天过期清理用受控时钟验证，在途记录由 `use_record` 保护不被清理。本轮实测：Rust **190** 项（`discard_recovery` 40）、Node 68 项通过，tsc/lint/build 通过；未重建 `.app`，未跑 GUI/Windows/CI/D06/长测。详见 [任务 05](./issues/05-safe-recovery.md)。
- 任务 06：`done`。两轴独立复核通过；`26b6dc4` 的刷新／跨组／linked／八个迟到响应，加 `1fc9ec3` 的真实隐藏恢复前台核对、计数、独立 A↔B、迟到锚点、稳定阅读位置、40 次保存及真实写前读取失败已验收。早期 AX／Raise 未证明实际焦点切换，不再冒称已确认缺陷根因。最终同构建 D06 标准首屏 **105／115ms**，真冷文字 DOM **6／7ms**；四份各60屏、60rAF、0兜底／未覆盖，普通呈现补验通过。前端 gate 19 项、build／lint 通过；[`1fc9ec3` CI #39923257047](https://github.com/GarlandQian/Revia/actions/runs/39923257047) 双平台类型／lint／build／Rust／release 编译通过（macOS **142**、Windows **132** 项）。版本、旧观察限制和两轮证据归档见[任务 06 验收](./task06-acceptance.md)，最终 30 分钟长测仍留 11。
- 任务 03：`done`，原始 hunk 写入、两视图键盘与真实暂存区核对、相关版本过期拒绝及 D06 已验收。最终 `bbddd4f` 的 [CI #39876636277](https://github.com/GarlandQian/Revia/actions/runs/39876636277) 双平台通过（macOS 114 / Windows 104 项 Rust）；标准首屏 115/95ms、真冷文字 DOM 5/4ms。原超帧值、旧取证不足样本与 P3 命名观察保留，详见 [任务 03 验收](./task03-acceptance.md)。

## 任务 01 结论（2026-10-22）

**任务 01 通过：内存持续增长已定位、修复，修复后的同进程 30 分钟长测按 Q98 通过，持续卡死未复现。** 详细过程与数据见[任务 01 记录](./issues/01-long-running-baseline.md)和 [leak-20261022](./evidence/leak-20261022/)。

- 判定口径改为 Q98：约每 5 分钟发送 WebKit 低内存通知、约 10 秒后取样，压力后仍保留的内存不再持续增长才算通过。修复前长测在此口径下压力后 malloc 22.9 → 36.0 MiB（+0.76 MiB/分钟），不通过。
- 单项诊断定位到“切换选中文件”：`nextPaint()` 每次新建 `MessageChannel` 且只关闭一端，挂着监听的另一端连同监听器被 WebKit 一直保留。强制回收后 `MessagePort` 仍持续增加，修复后不再增加。
- 修复 `a29ec4a`：让出任务共用一条 `MessageChannel`。修复后长测：压力后 malloc 10.8 → 12.6 MiB（+0.09 MiB/分钟）、footprint 81.8 → 82.8 MiB（−0.09），Rust 主进程 −0.03 MiB/分钟，均在 Q98 门槛内。
- D06 回归：首屏 88 / 84 ms、跳转 19 / 15 ms、滚动 60 屏 60 rAF / 0 兜底 / 0 未覆盖，不劣于 `c6cdf70`。

以下为 2026-10-21 的状态，保留作过程记录。

### 2026-10-21 结论

**持续卡死未复现，但任务 01 的内存门槛仍未通过，不能放行后续写入功能。** 第四轮完成有效的同进程 30 分钟主动负载，固定阶段 footprint 与 malloc allocated 的分桶和尾段仍上移，不能写“未复现，长测通过”，也没有证据把日志或高亮问题认定为内存根因。

- 已修复并在 `6f07f1a` 桌面核验首次异步高亮不自动补色；该版本也完成四份呈现测量和长测 04。
- 审查随后发现可见高亮区间混用旧、新版本行号；`c6cdf70` 已做最小修复并完成静态复核、相关检查及 Release 构建。
- `c6cdf70` 的最终四份有效测量与普通桌面检查完成，**本次任务 01 的 D06 回归验收通过**；一次未获得可见窗口的失败尝试完整保留，不用补测覆盖。
- **长测 04 只覆盖 `6f07f1a`，`c6cdf70` 尚未完成正式长测。** 没有用旧构建长测冒充最新版本通过。

前三轮的全部数据、早期无效样本、计时争议及接手过程完整保留在 [长测 01–03 历史报告](./baseline-history-01-03.md)，不因本报告更新而删除。A/B、依赖源码生命周期和修复前 frame 过程见 [2026-10-21 诊断记录](./diagnostic-20261021.md)；该记录的“修复待验收”是当时状态，最新状态以本文为准。

## 构建与环境

- 固定基点为 `main` 的 `7ab76f271fa78338ae12259ac61a9ad3a49da9eb`；工作目录 `/Users/garland/.codex/worktrees/m2/Revia`。
- macOS 27.0.1、arm64、Release `.app`，窗口逻辑尺寸 1280 × 800、devicePixelRatio=2；真实桌面由 Codex 操作并核验，截图保留在会话中。
- 长测固定 `/tmp/revia-m1-fixtures/diffs` 同一主 worktree 的 7 个改动条目：PNG、日志、lock 文件、Vue、Swift、大 TypeScript、已暂存 TypeScript；各轮长测使用独立应用数据目录，不改真实用户仓库列表、工作区、暂存区或 Git 配置。普通 desktop02/03 使用正常应用数据目录 `/Users/garland/Library/Application Support/io.github.garlandqian.revia`，只打开已有 fixture、切文件和视图、滚动，不执行 Git 写入；不称其为隔离运行。

| 用途 | 源码版本 | 二进制 SHA-256 |
| --- | --- | --- |
| A/B、修复前 frame | `d71ca5b` | `ce01397a75ecd615254b3271ade6e9169d3b0ea4123cbbf2e7f182cc58b6a2df` |
| 高亮通知修复、fixed frame、长测 04 | `6f07f1a` | `368eab41e47bfcf48c0a501c9163b136462fcbcb2aa9b08c64fe15e167c2d4bd` |
| 可见行范围修复、最终 D06 构建 | `c6cdf70` | `bfd46392e9786b4c2f3fefee9477dad8bced349ddc64d19aed63a5de72d8482f` |

长测驱动启动前固定 SHA，结束核验未替换；短测构建关联由对应归档清单记录，不向原始 JSON 倒填 SHA。当前已修改生产 `DiffView` 的高亮通知和可见行选取，不再沿用历史报告中“生产 renderer 未改”的旧状态描述。

## 本轮修改与诊断

### 测量日志与 IPC

同构建 A/B 各完成 120 轮及 90 秒末尾静置；文件/视图/滚动/跳转均为 840/1440/3960/720。verbose/minimal 主动耗时为 416.830/376.873s，日志 fetch 为 28,121/50，总 fetch 为 48,597/20,512。逐步日志是显著额外负载，但单组 A→B 不是随机重复实验。

末尾 WebContent footprint 为 159.1/146.1MiB，**malloc allocated 却为 108.6/132.7MiB，minimal 更高**；每组 6 个阶段样本各有 1 个无效，完整保留。不能只挑 RSS/footprint 较低宣称日志是泄漏根因或问题消失。正式长测因此改用显式 `--baseline-minimal-logging`，仍保留关键日志、心跳、PID/可见性、完整主动时长及原验收条件。

诊断 hook 仅显式启用，计数集合有界，不保留 payload、Response body 或 DOM 引用，不替换不可写的 Tauri 内部函数；退出时仅在 fetch 仍为自身 wrapper 的情况下恢复原引用。实际 Tauri 2.12.4/Wry 0.57.0 正常 callback/task 清理路径、条件性异常保留路径及观测盲区均已记录，尚未与内存上漂建立根因闭环。

### 两项有证据的高亮修复

1. `6f07f1a`：高亮完成快照触发补绘，继续请求被单请求背压跳过的另一侧，并将高亮版本传入 memo 行组件；保留“先文字、滚动时不续发高亮、停下后补色”的策略。
2. `c6cdf70`：Standards 发现旧行 100、新行 30100 被合并为一个连续范围，新完成重试会将两侧扩到约 152 个 400 行块，跨越大量不可见空洞。`visibleHighlightLines` 按实际呈现侧返回可见行，逐行调用既有 `ensure`，复用块级去重，不再跨版本或离散 hunk 合并区间。

第二项是可由代码和回归样例确认的取数范围问题，**不是当前固定 7 文件内存增长的已证实根因**。没有加入无依据的缓存淘汰、强制 GC 或清缓存操作；Pi 早先无长测依据的缓存淘汰改动未保留。

## 正式长测 04

原始结果见 [report.json](./evidence/baseline-04/report.json)，复算见 [analysis.json](./evidence/baseline-04/analysis.json) 与 [analyze.py](./evidence/baseline-04/analyze.py)。运行退出码为 0 只表示流程完成，不表示内存验收通过。

| 项目 | 实测 |
| --- | --- |
| 构建 | `6f07f1a`，SHA 与上表一致 |
| 进程起止（北京时间） | 18:37:14–19:10:03 |
| 主动操作区间 | 18:38:02.897–19:10:01.504 |
| 主动操作 / 操作墙钟 / 阶段暂停 | 1801.027s / 1918.603s / 117.576s |
| 循环 / 文件 / 视图 / 滚动 / 跳转 | 614（末轮部分）/ 4294 / 7358 / 20244 / 3682 |
| Rust / WebContent / GPU / Networking PID | 25611 / 25614 / 25613 / 25615 |
| 常规样本 / 有效阶段样本 | 194 / 29，阶段无效 0、缺失 0 |
| 180 秒后 RSS / 常规 footprint / 阶段样本 | 171 / 57 / 27 |
| 隐藏转换 / 最长帧间隔 | 0 / 777ms |

所有记录的进程归属一致，来自 `launchctl print pid/25611` 的 services 表，不取系统新增 PID 差集。184 次心跳加最终观测均符合既有可见性保护；rAF 范围约 7.03–61/s，末次 59.94/s，**不宣称全程稳定 60fps**。没有固定轮数提前结束，也没有把末尾静置算入 30 分钟。

阶段采样固定在 `src/staged.ts`、并排视图、顶部；准备和停顿全部扣除。pending 清零仅表示当前追踪项清零，不证明所有旧 IPC 完全结束。29 个样本均通过最终 BEGIN/END、前端区间及归属校验。

统计以带时区的 `operationsStartedAt/EndedAt` 对齐完整采样区间，**180 秒热身不变**；分桶使用操作开始后的墙钟，不伪称扣暂停后的逐点主动时间。旧 `elapsedSec - setupSeconds` 原点仅差 0.377475s，本轮热身后选样相同，旧口径也一并保留。常规 footprint 每三次采样取得一次，未采的 allocated/Networking footprint 不填零。

### 热身后内存

数值单位为 MiB；斜率仅作描述，不单独决定通过。

| 指标 | 首值 → 末值 | 范围 | 斜率（MiB/分钟） |
| --- | --- | --- | ---: |
| Rust 常规 footprint | 52.1 → 51.3 | 50.0–52.1 | -0.0055 |
| WebContent 常规 RSS | 260.88 → 424.22 | 260.88–424.25 | +3.5078 |
| WebContent 常规 footprint | 161.5 → 205.8 | 138.8–250.6 | +0.7261 |
| 固定阶段 WebContent footprint | 153.8 → 190.1 | 130.7–190.1 | +1.2645 |
| 固定阶段 malloc allocated | 124.7 → 174.0 | 104.4–186.4 | +1.9711 |

全部热身后固定阶段分桶均值如下；最后一桶不足 5 分钟，不隐藏尾段或只选最小值。

| 操作墙钟分钟 | 样本数 | footprint | malloc allocated |
| --- | ---: | ---: | ---: |
| 3–5 | 2 | 142.250 | 121.900 |
| 5–10 | 5 | 166.140 | 130.120 |
| 10–15 | 4 | 168.125 | 141.800 |
| 15–20 | 5 | 176.440 | 161.980 |
| 20–25 | 4 | 180.300 | 163.275 |
| 25–30 | 5 | 184.820 | 168.460 |
| 30–31.98 | 2 | 188.800 | 172.350 |

固定阶段最后两点 footprint 为 187.5/190.1MiB，allocated 为 170.7/174.0MiB；多个分桶与尾段一致上移，未显示稳定平台。常规 footprint 波动较大，不能覆盖同阶段证据。Networking RSS 为 13.30→54.86MiB，约第 22 分钟后平台；GPU RSS 为 45.50–46.98MiB，Rust 基本稳定。

**D01/Q78 内存门槛未过。** allocated 不等于存活 JS heap，RSS 也不能等同泄漏；当前只能确认本负载下增长仍需定位，不能命名 GC、allocator、日志或高亮修复为原因。

### IPC 与 DOM 观察

- 244/244 个必需 IPC 快照齐全；callback registry 始终为原有 4 个、新 ID=0，无集合溢出或观测错误。callback 数包含事件监听，不可换算为未完成请求数。
- 观测时 fetch active=0，运行期 maxActive=7，failed=0；总 fetch 105,052，其中 `debug_log` 243、`get_rows` 90,705、`highlight_range` 6,746、`row_index_for_line` 7,358。最终计数包含完成日志，不标为精确的纯主动期计数。
- fetch 统计只到响应头，不读 body、不覆盖 fallback messageHandler；callback 是离散采样，不是实时全程零在途。现有结果未显示持续累计的未完成注册项，但不能排除采样盲区或其他保留机制。
- 固定阶段 DOM 始终为 105 个元素、2 个 diff 行；ResourceTiming 为 0 仅表示 API 未记录请求。挂载节点数稳定不等于不存在脱离 DOM、旧对象或 native 内存保留。

## D06：分清构建与计时口径

`6f07f1a` 的四份 [fixed-frame 证据](./evidence/performance-fixed/) 已完成：标准首屏 96/86ms；冷负载首屏 95/96ms，后者使用独立 30,000 行全改文件，不混入普通首屏统计。八次同次首个 rAF 核验均为 40/40 行文字匹配，检查完成 6–11ms；真正未缓存跳出文字 DOM 为 4/8ms。

原 appear/warm 数值依次为 40/20、26/16、43/18、14/4ms，仍完整保留；例如原并排 warm20ms 含 scroll2ms + 消息等待18ms，不删除、不改写成探针数值。四份各有 60 真 rAF、0 兜底、0 未覆盖；最大滚动帧为 21/20/30/18ms，慢帧 1/0/1/0，沿用 Q84 backlog 边界。

缓存返回颜色 DOM 为 25/27/25/29ms，首个返回 rAF 尚未完整着色，最终均匹配；源位置经过缓存准备，不能替代首次打开测试。普通 desktop02 实际确认 Vue 并排、Swift 统一首次静置自动补色，以及大 TS 两种视图快滚、反向、拖滑块、返回缓存后恢复颜色且无整块空白。[日志](./evidence/performance-fixed/desktop-after-highlight-fix.log.gz) 佐证请求顺序，着色结论来自真实桌面，不从日志推断像素。

以下四份有效样本均为 **`c6cdf70` / SHA `bfd46392…`**，单位毫秒，来自 [最终呈现证据](./evidence/performance-final/)。首帧列为同次首个 rAF 的核验完成时间，不与原 appear/warm 混用。

| 最终样本 | 首屏 | 原 appear / warm | 去程 / 返回事前缓存 | 文字 DOM：去 / 回 | 同次首帧核验：去 / 回 | 返回缓存颜色 |
| --- | ---: | ---: | --- | ---: | ---: | ---: |
| 标准 / 并排 | 103 | 35 / 19 | 是 / 是 | 2 / 1 | 6 / 6 | 25 |
| 标准 / 统一 | 79 | 23 / 15 | 是 / 是 | 2 / 1 | 8 / 10 | 28 |
| 冷负载 / 并排 | 104 | 18 / 20 | 否 / 是 | 5 / 1 | 8 / 6 | 25 |
| 冷负载 / 统一（第 2 次） | 102 | 16 / 14 | 否 / 是 | 7 / 0 | 10 / 10 | 29 |

普通首屏 103/79ms 低于 150ms；独立冷负载首屏不混算。两个真冷目标事前各缺 40 行，文字 DOM 5/7ms 低于 100ms。八次同次首帧核验均为 40/40 行文字、位置和画布匹配，`frameReady=true`、`textWithinBudget=true`。返回的 0ms 是毫秒计时精度下的结果，不表示执行没有耗时。两次冷跳同步阶段均记录到占位、首帧核验时文字已齐，不能据此宣称占位实际显示过一帧。

| 最终样本 | 真 rAF / 兜底 / 未覆盖 | 滚动最大帧 | p95 | 慢帧数 |
| --- | ---: | ---: | ---: | ---: |
| 标准 / 并排 | 60 / 0 / 0 | 21 | 18 | 1 |
| 标准 / 统一 | 60 / 0 / 0 | 21 | 18 | 1 |
| 冷负载 / 并排 | 60 / 0 / 0 | 31 | 18 | 1 |
| 冷负载 / 统一（第 2 次） | 60 / 0 / 0 | 20 | 18 | 0 |

四份均可见、无隐藏转换；原超一帧 appear/warm 与 31ms 偶发慢帧不删除、不扣除改写，按 Q84 不另加慢帧清零门槛。独立探针与原测量目标不同，不能把探针时间倒填成原操作结果。四次返回首帧颜色均未全齐，随后 25–29ms 完整匹配，仍是既有 compact/补色策略，不宣称首帧完整着色。

[冷统一第 1 次](./evidence/performance-final/frame-cold-unified-01.json) 为 `ok:false`，错误为“性能测量未获得可见合成窗口”，没有性能 combo；连同日志原样保留。仅补跑此项，主动置前窗口后 [第 2 次](./evidence/performance-final/frame-cold-unified-02.json) 有效，不是重启拼接长测，也没有覆盖失败记录。

最终普通 desktop03 在相同 bundle 上实际确认：Vue 首次并排双侧、Swift 首次统一均能静置自动补色；大 TS 的两种视图各快滚 10 屏、反向 5 屏、远拖至约 5700 行并返回顶部，停止后恢复颜色，无整块空白。最后并排返回顶部的截图显示两侧已着色，之后 Cmd-Q 正常退出。[桌面日志](./evidence/performance-final/desktop-final.log.gz) 仅佐证请求顺序，着色结论来自真实桌面截图。普通模式不因设置 `REVIA_MEASURE_DATA_DIR` 而隔离应用数据，未把它与自动测量的独立目录混淆。

**当前构建的任务 01 D06 回归验收通过。** 同次 rAF 证明正常首渲染机会前 DOM 已就绪，结合真实桌面完成呈现验收；它不是硬件扫描输出的精确计时，也不另加 GPU 仪器门槛。这不改变 D01 内存未通过，不表示后续功能完成后的 D06 已验收。

## 检查与审查

| 检查 | 已核验结果及范围 |
| --- | --- |
| 可见高亮行 Node 原生回归 | 13 项通过，覆盖两视图、版本偏移、离散 hunk、纯增删、缺失行和可见窗口；无新测试框架 |
| `pnpm exec tsc -b` / `pnpm lint` / Release 构建 | `c6cdf70` 通过，最终 bundle SHA 如上 |
| IPC 诊断 Node 原生回归 | 11 项已通过；模块未改，沿用已有结果 |
| 长测 Python 工具回归 | 17 项已通过；本次可见行修复未改驱动，不重复运行 |
| Rust 单元 / 集成测试 | 60 / 3 项已通过；本次前端修复未改 Rust，沿用已有结果 |

Standards 的 P2 可见行范围问题已修复，静态复核未发现新增问题；不再把修复前的范围合并写成当前实现。Spec 的阻塞项仍是 D01 内存门槛，D06 已由当前构建实测与桌面核验闭合；检查通过不等于 M2 或 D07 全部通过，Windows 实际编译等后续任务尚未开始。

## 证据索引

- [长测 01–03 完整历史](./baseline-history-01-03.md)、[前三轮统计](./evidence/memory-analysis-123.json)、[archive-03](./evidence/archive-03-manifest.json)：保留旧报告、原始时序、早期失败和不利样本。
- [本轮诊断过程](./diagnostic-20261021.md)、[IPC A/B 原始材料](./evidence/ipc-20261021/)、[archive-04](./evidence/archive-04-manifest.json)：23 个文件，包括每组无效阶段样本及修复前 frame。
- [长测 04](./evidence/baseline-04/)、[fixed frame / desktop02](./evidence/performance-fixed/)、[archive-05](./evidence/archive-05-manifest.json)：17 个文件，包含报告、时序、日志、分析脚本与复算结果，清单保留源与存储 SHA。
- [最终 frame / desktop03](./evidence/performance-final/)、[archive-06](./evidence/archive-06-manifest.json)：11 个文件，四份有效结果、一次无效窗口尝试及全部对应日志；源字节、存储字节与清单 SHA 均核验一致，保留最终 bundle 的版本归属。
- [Q98 长测与低内存通知实验](./evidence/q98-20261022/)、[泄漏定位、修复与修复后长测](./evidence/leak-20261022/)：单项诊断各模式报告与汇总、强制回收后的 `heap` 计数、修复后正式长测、修复后性能与 `rafPainted` A/B。
- 原始日志与 `vmmap` 内容不为格式检查改写；压缩文件的校验区分源内容与存储字节。桌面截图只引用会话记录，不伪造本地 PNG。

## 下一步：先定位，再决定重测（2026-10-21，已完成，见当前结论）

任务 01 保持 `ready-for-agent`，不是要求用户再手动跑一次。已证明降低逐步日志仍不足以通过门槛，不再无依据重复相同 30 分钟负载。

1. 在显式短诊断中只加有界标量：应用自身 `nextPaint`、`nextTask`、timer/rAF 的创建、完成、当前未完成、峰值，以及“已完成但 rAF 仍待执行”的数量；不保存闭包、消息正文或 DOM 引用，不改变原调度/完成边界。
2. 用同一构建、相同固定循环数和固定 `staged/split` 阶段，核对负载后这些计数是否回基线；必要时沿用 120 轮 + 90 秒静置的短对照，明确不算正式基线，也不以静置代替主动负载。不要凭未显式关闭某个 MessagePort 就认定泄漏。
3. 若任务数回落但保留量继续上移，先确认工具能在不强制回收的条件下提供何种对象/分配证据，再取少量同阶段快照，区分旧行/token、脱离 DOM、闭包与 WebKit native 分配；记录观测成本和工具局限，不以 allocator 保留页猜测代替保留链。
4. 取得可重复机制后才做对应最小修复，集中跑必要检查及 D06，再对实际受测构建完成同进程 30 分钟验证。180 秒热身不移动，不强制 GC、不清缓存、不重启规避；没有证据时继续明确未通过。

本轮全部长测、短测和普通桌面实例均已退出，没有后台测试继续运行。任务 02–11 继续等待任务 01，保持不合并 `main`、不推送、不发布；本报告不产生新的产品决策或验收门槛。
