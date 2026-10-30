# M6 报告：冲突与历史、双语界面

> 2026-10-30 验收结论（Claude）：按 Q174 对 `f00f469...HEAD` 做增量两轴审查，Standards P2（Reset 默认）已记成 Q175 并改规格，Spec P2 / Standards P3（跳过、中止原地失败被当成正常停点）在 ba1fd57 修复并补回归，Spec 两项 P3 在 0113241 修复，其余 P3 为维护建议。Release 0113241 上复查了 Stash 归档分组；多选菜单、多选确认文案、Reset 默认 Mixed 因全屏授权无人确认未在真机点，转 [backlog 14](../backlog/14-m6-final-desktop-recheck.md)。最终检查 Rust 696 / 0 / 1、Node 485/485、build/tsc/lint、x18n 1719 键 0 错误、提取 0 中文。用户确认后压成一个提交合入 main。D06 / Q98 仍在 backlog 04–05。

> 2026-10-30 Claude 全屏桌面验收：用户批准全屏控制后，在固定 Release ca646c2 上补完 L01 即时切换与草稿 / 滚动保留、D01 窄窗、S02 作为新分支（Release27 的 P1 场景）、合并视图手改与撤销重做并中途切语言、H01–H03、O02、O03、F01、F02、T、C01、revert 全部组；发现 6 项问题，在 a187c87 修复（历史批次冲突停点误报待核对、文件历史与 stash diff 中文路径转义、Stash 面板切仓库误报过期、根提交禁用理由、根版本 blame 说明不可见、cherry-pick 选项搜不到），Release a187c87 复验通过。Rust 695/0/1、Node 485/485、build/tsc/lint、x18n 1714 键 0 错误、提取 0 中文。剩余为待用户决定的观察项（见任务 Comments），D06 / Q98 仍在 backlog。

> 2026-10-30 Claude 接手：修复 review33 Spec P1（e6c58e1，自动恢复 stash 前重验原分支）；固定 Release e6c58e1 真实桌面验收，S02 恢复暂存区+应用后删除、S03 删除/并发追加/gc 后取回/清空、O01 merge 应用+继续与 rebase 跳过/中止、S01 重复条目与比较设置、L01 首启中文与中英原生菜单通过；发现 7 项界面问题并修复（ca646c2），Release ca646c2 上复验通过。Rust 693/0/1（M6 130/130）、Node 485/485、build/tsc/lint、x18n 1713 键 0 错误、提取 0 中文。后台操控无法打字、开下拉框和右键菜单，L01 即时切换与草稿、S02 新分支、撤销/重做、H01–H03、O02、O03、F01、F02、T、C01、D01 窄窗仍待全屏控制下验收，整包未完成。详见任务 Comments“Claude 接手”一节。

> review32后：stash应用冲突提示双语经x18n修正，明确解决当前冲突并核对，无需再次应用。只改两条资源和CLI元数据；相关Node40/40、build/tsc、x18n0错误65unused/extract64候选0中文通过，938源与四末轮检查相同，无本轮新增失败。原真实Git复现exit101及现场保留，Rust/完整Node/lint未重复。仍待全新两轴、新Release真实桌面，旧Release27未通过，整包未完成。

> review31后：写后重读失败仍在原目标显示已知历史/续接清单及原错误，保留NeedsVerification与关闭保护，不重放写入。真实Git1/1含2场景、完整Node485/485含18生产App组合、build/tsc/lint/x18n0错误65unused/extract64候选0中文通过，无本轮新增失败，938源与6末轮检查一致；Rust生产源未改，未重跑当前127项M6或其他Rust。Standards P3保留维护建议。仍待全新两轴、新Release和真实桌面，旧Release27未通过，整包未完成。

> review30两项P2已修：原生Continue/Skip的外围回收收尾失败保留逐条continuation回执；外部工具选择保存失败独立持久通知，原目标和即时语言保持。真实Git/设置相关Rust4/4、完整Node484/484及build/tsc/lint/x18n0错误65unused/extract64候选0中文通过，本轮无新增失败，938源与9末轮输入完全一致。未重跑当前126项M6或其他Rust；仍待全新两轴、新Release真实桌面，旧Release27和失败现场保留，整包未完成。

> review29：Standards0/Spec1P2，批次无原生停点或收尾失败保留部分回执与待核对已修。四个真实场景通过；M6首轮122/123，旧中断测试返回形状已同步、禁止重放断言保留，专项与signal各1/1通过，124项分批有成功证据；完整Node482/482及build/tsc/lint/x18n/extract通过。首轮E0308、原测试失败、生成绑定纯空白差异原件保留，输入映射见repair29-final-source-proof.json。仍待全新两轴、固定Release和真实桌面，整包未完成。

> review28两项P2已形成修复候选：审阅同步保留结构化IO cause、所有分支历史按固定Git流稳定有界归并。真实Git历史11/11、审阅错误1/1、完整Node481/481及build/tsc/lint/x18n0错误65unused/extract64候选0中文通过，938源与七检查一致，无本轮新增失败。尚待全新两轴和新Release真实桌面；旧Release27未通过，桌面两问题未重验，整包未完成。

> 桌面首轮两问题已形成修复候选：自动读取与写入互斥/写前等待/释放补读，原生菜单接当前语言并保留正常关闭保护。完整Node480/480、真实Git显式刷新2/2、build/tsc/lint/x18n1710键0错误65未用/extract64候选0中文通过，938源与六检查一致；Rust源不变，未重跑完整M6。尚待新两轴/新Release真实重验，不将旧Release27局部通过沿用为新版本通过。

> 真实Release27桌面未通过：发现中文原生菜单仍英文，以及stash新分支创建后应用未完成、结果待核对；安全关闭保护实际生效，现场已保留。由Codex直接修复后重新固定候选/审查/构建并重验。局部通过的首次语言、草稿/滚动保留、S01保存及默认Apply不代表整包通过。详见任务Comments和release27-nqdfzhty。

> 当前阶段：review27独立两轴源码复核通过，固定产品候选2ae2e5f；Standards0硬违反/1非阻断P3，Spec0。当前122项M6分批有成功证据（首轮120/121的阶段等待超时原件保留，原断言未改、专项及权限signal通过），完整Node475/475，最终937源与检查输入一致。进入固定Release及真实桌面验收，23组尚未通过；Status仅为ready-for-human，整包未完成。

> review26后候选：手动stash push非零退出保留待核对与原始Git原因，相关真实Git6/6、Node209/209、x18n0错误65未用警告/extract64候选0中文通过。新增2场景验证部分清理和原stash保留，实际载荷经App收尾触发关闭保护；首次Node英文断言失败原件保留，只修预期后重跑通过。未重跑完整122项M6/其他Rust/完整Node/build/lint；最终源码与相关检查输入映射见repair26-final-source-proof.json。仍待全新两轴、固定Release及真实桌面。

> review25后样式修复候选：四处M6危险按钮统一用主题destructive-foreground，颜色均取既有CSS变量；四套主题静态配色对比为7.472/7.472/7.096/5.070，构建含tsc通过，编译CSS含新语义类。仅CSS/className变更，无新增测试/失败；Rust/Node/lint/x18n未重跑，旧结果保留原范围，最终937源码与本轮两检查一致。Spec25为0，Standards P3硬违反已修、重复流程P3留维护建议。仍待新两轴、固定Release和真实桌面，整包未完成。

> review24后修复候选：实时输出监听错误保留语义/cause并按当前语言格式化；blame目标与三开关共用读取生命周期，首次在途和切版本时旧响应均失效，相关重复路径消除。真实Git专项1/1含24组合载荷、Node474/474、build/tsc/lint/x18n/extract均通过，无本轮新增失败；双语1688键0错误65未用警告，旧close.listenerFailed保留，extract64候选0中文。最终937源码与6检查输入一致，locale仅CLI维护，未重跑完整121项M6。仍待全新两轴、固定Release与真实桌面，整包未完成。

> review23后修复候选：stash归档按共享仓库commonDir及其已知文件系统身份确认，原worktree移除后仍可取回；普通文件回收身份不放宽，显式导入确认贯穿真实fetch到store。相关真实Git归档8/8通过，覆盖linked存活取回、克隆/未知身份/原路径重建拒绝及fetch后旧确认拒绝；首次E0382编译失败原件保留，已修后重跑。末轮x18n0错误64未用警告/extract64候选0中文通过，最终937源码hash一致。前端880文件与既有build/tsc/lint及Node466输入相同未重跑，未执行完整120项M6。Standards P3仅记维护建议。尚待新两轴、固定Release和真实桌面，整包未完成。

> review22后修复候选：手动stash写前拒绝tracked只读路径，显式自动stash恢复确认贯穿回收和关联落盘，未开始时拒绝保留Pending可重新确认。完整M6尝试115/117通过，两项平台环境失败已精准排查/修正测试接缝后专项通过，分批117项有成功证据；Node466/466、x18n0错误64未用警告/extract64候选0中文通过。首轮测试编译错误、sandbox ps限制与macOS非UTF8文件名错误原件保留，不称首轮全套成功。生产源最终hash一致，前端880文件与既有build/tsc/lint通过输入相同未重复，尚无固定Release/真实桌面通过。

> review21后修复候选：macOS首选界面语言移平台层，优先原生preferredLanguages且跳过C/POSIX回退；监视错误保持结构化并在状态栏即时格式化。相关Rust5项、Node466/466、最终build/tsc/lint/x18n0错误64未用警告/extract64候选0中文通过。原生独占子进程实际验证中文系统+C.UTF-8仍选zh-CN；实际原生watch错误的生产StatusBar中英详情正确。首次TS漏import与新测试误判active的失败原件保留，未重跑完整Rust/M6。仅直接声明既有objc2-foundation0.3.2，无包升级；最终源码/依赖hash一致。仍待全新两轴、固定Release和真实桌面，整包未完成。

> review20后修复候选：远端跟踪引用名称/OID纳入共用写确认context，相关历史警告与来源条件变化后拒绝旧确认，并贯穿自动保存/序列。分批Rust672通过/0未解决失败/1既有忽略，M6共111项有成功证据；Node465/465、lint/x18n0错误64未用警告/extract64候选0中文通过。首轮E0308与旧确认用例失败、专项/后续检查、28绑定纯格式原件及最终源码证明均保留，不称首次整套成功。前端/locale与既有build/tsc通过源相同，未重复构建。仍待全新两轴、固定Release和真实桌面，整包未完成。

> review19后修复候选：Force每一步写前重验、只推进当前路径基线，hook改变后续文件/index则停止，保留部分清理回执与回收。完整M6 108/108（新增6场景）、0失败；仅改Rust，前端/Node/locale字节与上一轮已通过的检查相同，未重复未受影响检查。最终源码证明保留，检查后无源码清理。仍待全新两轴、固定Release与真实桌面，整包未完成。

> review18后修复候选：历史外部diff在任何diff命令前固定完整提交/父OID，拒绝选项与非提交输入；统一diff memo行随locale失效并保留缓存与滚动。相关真实Git11/11、完整Node464/464、build/tsc/lint/x18n0错误64未用警告/extract64候选0中文通过。首轮新增测试E0507编译失败原件保留，修测试借用后通过；未重跑完整Rust/M6。最终分组源码hash一致，检查后无行为修改。仍待全新两轴、固定Release和真实桌面，整包未完成。

> review17 后修复候选：stash 删除/清空确认贯穿归档及事务，关闭保护错误保留语义并即时翻译，成功自动 stash 提供保存通知与归档定位。M6 首轮105/106，唯一旧错误码预期同步后单测通过，分批106项有成功证据；完整Node463/463、最终build/tsc/lint、x18n双语1687键0错误64未用警告/extract64候选0中文通过。Rust其余套件本轮未重复；全部失败及分组源码SHA证明保留，检查后无源码变更。仍待全新两轴、固定Release和真实桌面，整包未完成。

> review16后修复候选：特殊字符历史路径按NUL语法解析、同OID多路径选择和英文复数已修正。M6首轮104/105、1项既有hook等待超时，未改源码或门槛的单独重跑通过；分批105项有成功证据。完整Node460/460、build/tsc/lint、x18n双语1682键0错误/62未用警告、extract63候选0中文通过，失败原件及最终源码/依赖SHA证明保留。Rust其他套件未重复。仍待全新两轴、固定Release和真实桌面，整包未完成。

> review15后修复候选：日志历史入口固定原worktree/path/commit，旧对象与初次blame不再误指同路径新对象；状态色和两条既定术语已修正。最小相关Rust8/8、Node73/73、build/tsc/lint/x18n0错误63警告（62未用键、1待更新译文）/extract63候选0中文通过，逐文件源码SHA一致。生产Rust未改，未重复完整套件。仍待全新两轴、固定Release与真实桌面，整包未完成。

> review14 后修复候选：stash新分支的原确认贯穿检出和应用，文件历史跳转与分支范围已修正。集中分批Rust664通过/0失败/1既有忽略（M6 103/103）、完整Node456/456，build/tsc/lint/x18n0错误62未用警告/extract62候选0中文通过。最终源码哈希一致，检查后无行为修改。仍待全新两轴、固定Release与真实桌面，整包未完成。

> review13 后修复候选：历史非UTF8路径凭据、stash重复出现项/回执、Force暂存选择与比较设置均已处理，设置搜索关键词迁移错误一并修正。分批Rust662通过/0未解决失败/1既有忽略（M6共101项）、Node455/455，最终build/tsc/lint/x18n0错误62未用警告/extract62候选0中文通过；首轮失败及专项编译/业务失败保留，不能称完整首轮成功。最终源码哈希已核对。仍待全新两轴、固定Release与真实桌面，整包未完成。

> review12 后修复候选：自动stash前后的原文件、暂存区与确认代次已统一保留；真实Git新增5场景。完整Rust658通过/0失败/1既有忽略（M6 97/97）、完整Node453/453，build/tsc/lint/x18n0错误62未用警告/extract62候选0中文通过。最终非绑定源码哈希与检查一致，未在检查后修改行为。仍待全新两轴、固定Release和真实桌面，整包未完成。

> review11 后修复候选：误删helper的E0425已修复并对最终源码重新全套；完整Rust655通过/0失败/1既有忽略（M6 94/94）、Node452/452，build/tsc/lint/x18n0错误62警告/extract62候选0中文通过。确认边界、stash编码和最小翻译扫描豁免已处理，原失败保留。仍待全新两轴、固定Release与真实桌面，整包未完成。

> review10 后修复候选：两轴四项明确问题及附录关联写入失败边界已处理；完整Rust653通过/0失败/1既有忽略（M6 92/92）、分批Node451通过，build/tsc/lint/x18n/extract通过；x18n0错误/62未用警告。原始失败保留，逐项见Comments。仍待全新两轴、固定Release、真实桌面，整包未完成。

> review09 后修复候选：两轴明确问题已处理，完整Rust647通过/1既有忽略（M6 86/86）、分批Node449通过，build/tsc/lint/x18n/extract通过；原始失败保留。逐项见Comments。尚待全新两轴、固定Release、真实桌面，整包仍未完成。

> review08 后修复候选：两轴明确问题及附录stash保存回执已处理，分批642 Rust通过/1既有忽略（M6 81/81）、Node447通过，build/tsc/lint/x18n/extract通过；x18n 0错误/62未用警告。原始失败保留，逐项见Comments；尚待全新两轴、固定Release与真实桌面，整包未完成。

> review07 后修复候选：两轴明确意见已处理，分批638 Rust通过/1既有忽略（M6 77/77）、Node443通过，build/tsc/lint/x18n/extract通过；x18n为0错误/62未用键警告，未删除键。失败原件与逐项处理见任务Comments。尚待全新两轴、固定Release及真实桌面验收。

> review06 后接管修复候选：四项明确意见已处理，分批Rust631通过/1既有忽略（M6 72/72）、Node438/438、build/tsc/lint/x18n/extract通过，失败原件保留。整体Abort二次写入另补保护并按受影响入口复核。待全新两轴审查、固定Release与真实桌面验收，当前未完成。

> review05 后接管修复已完成自动化候选：完整 Rust 628/0/1（M6 69/69），Node437/437、build/tsc/lint/x18n/extract通过；强化整体 Abort 的回收基线后已修复，最终 M6 重跑69/69通过。此候选仍待全新独立审查及真实桌面验收。

> review04 后候选 `25883b5`：两轴明确意见已修复，分批验证共 620 Rust passed / 1 既有 ignored（M6 61/61）、Node 437/437、build/tsc/lint/x18n/extract 通过，失败尝试原件保留。待全新两轴复核；真实 Revia Release / 桌面尚未验收。下文为历史检查点记录。

> 最新接管修复候选：review03全部明确意见已修复，完整Rust612/0/1（M6 53/53）、Node437/437、build/tsc/lint/x18n/extract通过；详见任务Comments的逐项处理。待全新两轴复核及真实桌面验收，仍不视为已完成。

> 最新review03固定54f953d：Standards 8项明确问题（3 P1、5 P2）+1判断项，Spec 7项（3 P1、4 P2），均未通过。完整独立报告已写唯一任务Comments，真实生产复现原件在review-03-08btp6lr/spec-production-nndkemm_。当前进入Codex直接修复；既有603项通过不代替验收。

> 2026-10-29 Codex 当前候选：Q173 持锁 stash 事务已实现，Rust 全套 603 passed / 0 failed / 1 ignored（M6 44/44），Node 435/435、前端构建/tsc、lint、x18n 与提取门禁通过。原安全封闭导致的四条功能失败已恢复。源码、机制、边界与失败原件见 [Q173 验证](./q173-validation.md)。接下来进行全新两轴独立审查及固定 Release / 真实桌面验收，当前仍未验收。下文旧检查点保留为历史记录，旧“等待用户决定/审批服务”的状态已结束。

> 2026-10-29 07:55（Asia/Shanghai），规划方接管：**Q113 唯一返修未完成，当前代码为部分返修快照，未经复核与桌面验收。** Pi 在原会话多次续接后仍有已确认缺口；已停止该实施回合，保留全部 21 个修改路径。锁内文件／状态指纹、autostash 文件系统身份、stash 完整祖先归档、外部 Git 并发删除、嵌套错误翻译及历史操作后的自动恢复提示仍未关闭。后续由 Codex 新两轴复核并直接修复，不再交回第二轮返修。原件与文件哈希：`/private/tmp/revia-m6-batch-ig_nawgt/repair-partial-snapshot-ypbxgtkf/`；Pi 的未核实返修清单原件同目录 `repair-ledger.md`。其“本轮未执行任何检查”不准确：监控曾观察到一次 `cargo check` 编译错误与全仓格式化；无可据以判定本快照通过的集中检查证据。旧通过记录仅适用于首交付源码。

> 2026-10-29 规划方初审：**未通过**。以下实现与自动化结果是 Pi 在 `6272013` / `1fdca4e` 交付时的记录，不表示独立审查或桌面验收通过。新的两轴审查分别发现 Standards 8 项（6 P1、2 P2）、Spec 13 项（6 P1、7 P2）；包括界面 TypeError、固定目标、写前版本、回收完成基线、stash 独立归档和并发删除问题。详见唯一任务的 `Comments`，整包进入 Q113 唯一一次正式返修，D01 英文窄窗仍待规划方实测。根代理另补提交警告双语遗漏及证据引用/保留边界。最终检查原件已核对，原检查通过记录保留；尚未构建本轮验收 Release、未合 main、未推送。



Status: ready-for-agent
日期：2026-10-29
实施分支：`m6`
实施 worktree：`/Users/garland/.codex/worktrees/revia-m6/Revia`
基线：`fad6483a9dd754c35677c32741c1978d419e8b56`
代码实现提交：`62720130feb1416903fed62b002f6612bb7f769a`（最终文档提交见 `m6` 分支 HEAD）

## Codex 接管检查点（2026-10-29，未验收）

已修复确认文件/index指纹、锁范围、literal pathspec、回收guard生命周期、完整祖先归档、worktree身份、按动作/OID持久reword、edit冲突门禁、全分支分页固定tips、工具配置与迟到响应，以及通知/验证错误的当前语言呈现。新增9项既有Rust真实Git回归、实际NotificationCard切换语言回归。自动stash先保存持久意图，以唯一说明定位自己的OID；恢复前写Applying，成功后写Restored，重启不会二次apply，并可只结束关联保留stash。

**S03仍未通过。** 系统Git的reflog删除边界不能携带已确认OID条件；正在等待用户决定是否为此评估ADR-0002的窄范围后端例外。单条删除、清空及自动清理已在共用出口拒绝删除，归档完整保存、原stash保留。这是临时安全封闭，不能解释为功能完成或用户已接受范围缩减。既有测试没有降低要求：最新M6结果27通过/4失败，失败分别是自动恢复删除、cherry-pick自动清理、应用后删除和单条删除，均对应该封闭。

本轮证据：`/private/tmp/revia-m6-batch-ig_nawgt/codex-checks-yf3rkhgl/`。封闭前M6真实Git31/31通过；封闭后上述4项如实失败。首次编译的旧测试漏传changes_kind已修正。Rust其余现有单元/集成检查已分别执行；沙箱内4个FSEvents用例收不到事件，精确升级后同用例通过（watcher-escalated、refresh-isolation-escalated），沙箱失败原件保留。其余原件见cargo-02、cargo-integrations、remaining-integrations。

前端build退出0（含tsc），lint退出0（既有unused-vars警告）；既有Node脚本逐项执行成功，实际Git错误载荷与既有通知即时切语言回归2/2通过。x18n在safeDeletionUnavailable新增前为1634键/双语完整、0 errors/63 warnings；新增后仍待最后复查。extract当轮0中文。原始argv、退出码、源码diff/哈希及所有失败原件保留。

本检查点未获新的独立审查、固定Release或真实桌面/Rebased/中英文窄窗验收；未续接Pi，未派第二次正式返修，未改main、推送、PR或删除worktree。

## 结论

2026-10-29 新两轴复核固定 `e770e22`：Standards 9 项（7 P1、2 P2）、Spec 14 项（10 P1、4 P2），均未通过。两份原始报告位于 `/private/tmp/revia-m6-batch-ig_nawgt/review-02-0krny3u3/`，完整意见已追加唯一任务 Comments；由 Codex 直接实施后再复核。

当前结论是未通过，后续复核、修复与真实桌面验收仍需完成。以下为首交付 `6272013` 的历史自述与检查记录，已经被初审发现的缺口及本报告顶部接管说明限定，不能作为当前部分返修通过的结论。

首交付自动化检查结果：

- Rust：`cargo test` 15 个测试二进制、576 个通过、0 个失败、1 个忽略；其中 `tests/m6_flows.rs` 22 个真实 Git 集成测试全部通过。
- 前端：`tsc -b`、`oxlint`、`vite build` 均退出 0。
- Node：18 个测试文件、433 个通过、0 个失败。
- x18n：1,607 个键，zh-CN / en 各 1,607，缺失 0、空值 0、过期 0、错误 0；62 个未使用键警告按规格保留并报告。
- 硬编码中文门禁：57 个候选、0 个含中文；负向探针注入 1 条中文后退出 1。
- `git diff --check` 退出 0。

实施方没有做规划方负责的三项：独立 Standards / Spec 两轴审查、固定 Release 与真实桌面 / Rebased / 双语验收、最终合并。这三项在任务单中保持未勾选。

## 基线与交付范围

改动集中在以下区域：

- Rust 新增 `stash.rs`、`stash_archive.rs`、`autostash.rs`、`operation_flow.rs`、`rewrite.rs`、`file_history.rs`、`external_tools.rs`，并接入 `commands.rs`、`settings.rs`、`operation.rs`、`commit_flow.rs`、`lib.rs`。
- Rust 既有 `AppError` / IPC 结果迁移为 ADR-0013 的结构化代码与参数；生成绑定同步更新。
- 前端新增 `src/features/m6/` 下的 `OperationBanner`、`StashPanel`、`HistoryActionDialog`、`FileHistoryDialog`、`ExternalToolDialog`，接入 `App.tsx`、`LogView.tsx`、`EditorTabs.tsx`、`MergeView.tsx`、设置和通知。
- 前端文案迁入 `locales/zh-CN.json`、`locales/en.json`，新增 x18n 配置、运行时和门禁；旧 `src/lib/i18n/zh-CN.ts` 删除。
- `src-tauri/tests/m6_flows.rs` 与 `tools/test-m6-i18n.mjs`、`tools/check-i18n-extract.mjs` 覆盖本轮真实 Git 和英文错误链路。

新增前端开发依赖 `@x18n/cli@^0.1.0`；未新增 Rust 依赖，未修改用户 Git 配置，未向仓库写入 `refs/revia/...` 隐藏引用。

## 实现对照

### C：共同契约

- 所有 Git 读写继续经 Rust 受控执行器；前端只传固定类型参数，用户文本、路径、分支和说明不拼 shell。
- 结构化错误按 ADR-0013 落地：`AppError::coded(ErrorKind, code, template, params)`，`AppErrorPayload` 带 `code`、`params`、`cause` 与回退 `message`。后端静态盘点有 684 个实际使用的 `errors.*` 代码，前端统一经 `formatAppError` 递归格式化；Git 原始 stderr 只作为 `detail` 参数，不冒充产品文案。
- 写操作继续走 `WorktreeSession` 的非排队写锁；执行前重验 worktree、完整 OID / 期望 HEAD、引用读数、操作元数据 SHA-256 版本和确认集合。`OperationState.metadataVersion` 在 sequencer 前进后失效旧确认。
- stash 是仓库共享资源：删除 / 清空先读 reflog 的完整 OID 集，执行前再核对；不使用浮动的 `stash@{n}` 作为删除 / 恢复身份。
- 继续、中止、跳过、reset、外部工具、文件历史和迟到 IPC 分别绑定当前 worktree；前端用请求序号守卫丢弃旧响应，旧结果不污染新目标。
- 覆盖前回收沿用 `RecoveryStore`；回收失败不继续写入。关闭 / 退出保护沿用现有 `ExitGuard`，M6 写生命周期接入同一守护。
- 未删除 `index.lock`，未改用户 Git 配置；非交互编辑器只使用本次进程临时脚本。

### S：stash

- `StashPanel` 展示真实 stash 的说明、时间、原分支、文件列表和只读 diff；`compareLocal` 可在父提交 / 本地工作文件之间切换，默认父提交（`COMPARE_WITH_LOCAL_IN_STASHES_ENABLED = false`）。
- 保存对话框只有说明和“保留暂存区”；不提供包含未跟踪 / ignored 文件选项。真实 fixture 验证未跟踪文件不会被 stash 保存。
- 应用默认不恢复暂存区、不应用后删除；可作为新分支应用，分支名经校验，基点为 `entry.oid^1`。应用前把可能受影响的工作文件状态存入 `RecoveryStore`（`RecoverySource::StashApply`）。
- 应用冲突时保留条目并跳到已有冲突处理页签；即使勾选“应用后删除”也不删除，不自动继续其他操作。
- 应用成功后删除同样先归档；归档或删除失败时返回 `cleanup_error`，界面明确“已应用、未删除”，不重复应用。
- 删除 / 清空按完整条目集合归档到用户仓库之外的 bare Git 回收区，目录权限 0700，保留 14 天；归档包含重建所需的对象闭包，不依赖原 stash 对象或 reflog。
- 回收区“恢复为 stash 条目”只通过 `git fetch` + `git stash store` 放回列表，不直接改工作区或暂存区，不覆盖现有 stash。
- 真实 `git gc --prune=now` 后仍能从归档恢复；同仓库外部追加 / 删除会以 OID 集合变化拒绝旧确认。归档维护在启动和周期性恢复维护中清理过期条目。

### O：进行中的操作

- `OperationBanner` 对 merge、rebase、cherry-pick、revert 显示继续、中止和适用的跳过；merge 不显示 skip，继续在存在未合并冲突时禁用。
- 继续、中止、跳过重新核对操作种类和元数据版本，绑定当前目标；中止 / 跳过后先回收可能被覆盖的工作文件。
- 继续 / 中止 / 跳过使用固定命令和非交互环境；不提供任意 shell 或 todo 文本。
- rebase 需要清理时自动保存未提交改动；cherry-pick / revert 在会覆盖时提供“先 stash 再执行再恢复 / 强制覆盖 / 取消”。自动保存使用 Git stash，不使用 Shelf，说明注明 Revia 和操作名称。
- 自动保存关联写在应用数据目录的 `links.json`，跨关闭 / 重启保留；操作暂停时条目保留，完成 / 中止后再恢复。恢复冲突继续保留条目并提示，不自动删除或重试。
- 操作完成后恢复自动保存；只有确认完整应用才归档并删除，删除失败保留条目并如实报告。
- 交互式 rebase 的 `edit` 停点通过 `rebase-merge/done` 末尾命令识别；在无冲突时 `edit_stop = true`，允许 amend 和新建提交，用于拆分。冲突停点和其他进行中操作仍禁止普通提交。

### H：日志历史操作

- 日志菜单接入 cherry-pick、revert、reset、改提交信息和交互式 rebase；确认页列出固定提交集、范围和后果。
- 多提交选择保持日志显示顺序（新到旧）；cherry-pick 按时间顺序执行（旧到新），revert 按 Rebased 行为保持显示顺序，不反转。
- cherry-pick 默认跳过空提交，也可选择创建空提交并在结果中列出跳过项；`includeSource` 只在目标提交位于本地远程跟踪分支时加 `-x`，不联网。
- revert 接入正常冲突 / 继续流程和 O02 的本地改动保护；revert merge 入口不可执行。
- reset 提供 Soft / Mixed / Hard / Keep，首次默认 Mixed 并记住本次模式；确认页说明 index / 工作文件影响。Hard / Keep 前把工作区修改存入 `RecoverySource::HistoryReset`，不放宽为统一 autostash。
- 改提交信息绑定当前分支可改写的固定提交范围，保留父子关系；根提交、merge 或不在当前 HEAD 历史上的目标在写入前拒绝。
- 交互式 rebase 支持排序、改信息、squash、fixup、删除和暂停编辑；没有 exec / 任意 shell 输入。计划必须完整覆盖 `upstream..HEAD`，不允许重复或遗漏，禁止首项 squash / fixup。
- 范围含 merge 提交时功能入口不可用并说明原因；不复用猜测的主线父项，不静默展平。
- 计划在确认后重新验证，使用隔离的临时 `GIT_SEQUENCE_EDITOR` / `GIT_EDITOR` 脚本执行，不把用户文本当脚本参数；部分成功只列出已完成、跳过、失败和待处理项，不整批重试。
- 已发布历史警告按本地远程跟踪引用核对，不联网；不隐式推送或强推。

### F：文件历史与 blame

- 工作文件和日志文件行可打开只读文件历史；默认当前分支，可切所有分支。
- `file_history.rs` 使用 `git log --follow --name-status` 读取历史，跨分支时不用 `--simplify-by-decoration`，避免丢掉中间提交；以 `-z` 解析路径，支持中文和空格。
- 前一版本通过 `git diff-tree -M -r <oid>` 检查重命名，不加 pathspec 限制，跨重命名返回结构化 `FileRevision { oid, path }`。根提交返回无上一版本，不是伪造一个空版本。
- 同路径删除后重建在历史中被识别为新生命周期，不把新文件误认作旧对象。
- 分页从目标版本向前读取，延迟到当前页才生成 patch；单条 patch 上限 2 MB，二进制文件保持只读并明确说明。
- blame 使用 `git blame --line-porcelain`，提供忽略空白、检测文件内移动、检测跨文件移动三个开关；上一版本 blame 绑定当前行 / 版本的真实父关系。
- “显示文件名”设置接入 `Settings` / `SettingsPatch`，默认 false，与 Rebased 的 `isShowRootNames` 默认一致。
- `FileHistoryDialog` / `StashPanel` 用单调请求序号丢弃迟到的 IPC 响应；提交双击、blame 哈希点击可跳到日志对应提交。

### T：外部工具

- 设置可保存外部 diff / merge 工具；保留内置合并。工具列表从 `git difftool --tool-help` / `git mergetool --tool-help` 解析，严格区分可用工具和“valid, but not currently available”区段。
- 工具名限制为字母数字、连字符、下划线、点和空格，拒绝路径分隔符和冒号，避免直接用路径注入可执行文件；目标路径拒绝空值、NUL、绝对路径和 `..` 逃逸。
- 外部命令在 Rust 层以固定参数启动，使用 worktree 环境变量，不经过 shell；带 `--no-prompt`，输出上限 1 MB。
- 当前工作区 diff 会拒绝未跟踪文件；已暂存修改传 `--cached`，未暂存修改比较工作区与 index。历史提交 diff 通过 `git cat-file -e <rev>:<path>` 验证对象存在，并通过父提交比较，不要求当前工作区有该文件。
- 合并工具启动前把冲突文件存入 `RecoverySource::ExternalTool`；进程退出 0 不等同冲突已解决，结束后重新读取未合并状态并扫描残留冲突标记 `<<<<<<<`、`=======`、`>>>>>>>`。
- UI 区分“工具非零退出”和“仍有冲突”，不会把两者混成同一成功 / 失败提示。

### L：双语与 x18n

- 资源为 `locales/zh-CN.json` 与 `locales/en.json`，源语言 `zh-CN`，目标 `en`；按 `x18n.config.ts` 管理，资源和键结构一致。
- 现有 1,607 个键全部有 zh-CN / en 翻译：913 个界面文案、694 个结构化错误文案；缺失 0、空值 0、过期 0。
- 旧 `src/lib/i18n/zh-CN.ts` 已删除，函数式文案改为带命名占位符的翻译；动态查表改为显式完整字面量键，集中在 `src/lib/i18n/text.ts` 的展示辅助函数，不在运行时拼翻译键。
- `messages` 代理和 `t(key, params)` 支撑即时切换；切换语言不重载页面。App、页签、选择、滚动、提交草稿、合并编辑、撤销 / 重做状态保持。
- 首次启动优先使用 Rust 的 `system_language`：检查环境 locale，macOS 上回退读取 `AppleLocale`；前端 `navigator` 仅作为后备。设置保存 `language`，`auto` 时即时解析。
- 后端用户可见错误、原因和操作结果全部走结构化代码 / 参数；`formatAppError` 在展示时递归翻译代码、参数和 cause，切换语言后既有错误也能重新呈现。`error.message` 仅作为无法翻译时的回退，不作为业务判断依据。
- `Snapshot Before Commit` 术语已统一；英文侧避免 glossary 警告的 “autostash”“stashes” 复数写法，改用 “automatic stash” / “stash”。
- `x18n check` 最终 0 errors、62 个 unused-key warnings；按规格未自动删除这些旧键，报告保留。
- `x18n extract --list` 最终 57 个候选、0 个中文。`tools/check-i18n-extract.mjs` 将它变成可失败门禁：有中文候选时退出 1；负向探针已验证该失败路径。
- 调试 / 测量日志使用最小范围 `x18n-ignore`，没有排除整个界面目录。

### D01：日志日期列

- `LogView.tsx` 的日期列统一为 124 px，表头、行和最小表格宽度共享该约束；日期使用 `tabular-nums`、`whitespace-nowrap`、`overflow-hidden`、`text-ellipsis`。
- Headless Chrome 测量 11.5 px 字体下 `2026/09/29 17:00` 文本宽 97.73 px，110 px 列宽（16 px 内边距）会截断，124 px 列宽可完整显示；早先对 12 px 字体的测量为 101.42 px，结论一致。
- 这次只做真实字体宽度测量和前端构建，实际窄窗桌面截图与英文菜单交互留给规划方验收。

## 验证与证据

证据根目录：`/private/tmp/revia-m6-batch-ig_nawgt/impl/`。每次尝试都有独立 `.log` 与 `.meta`，记录 argv、cwd、开始 / 结束时间、退出码。最终证据在提交 `62720130feb1416903fed62b002f6612bb7f769a` 的干净工作区上运行，`.meta` 同时记录 `source_sha` 和 `source_tree`；失败原件未覆盖。

| 检查 | 最终命令 | 结果 | 证据 |
| --- | --- | --- | --- |
| Rust 全量 | `/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml` | 576 passed / 0 failed / 1 ignored，15 个测试二进制 | `attempts/cargo-test.final.log` |
| M6 真实 Git 集成 | 同上，`--test m6_flows` | 22 passed / 0 failed | `attempts/m6-flows.final.log` |
| TypeScript | `pnpm exec tsc -b` | 退出 0，无诊断 | `attempts/tsc.final.log` |
| Lint | `pnpm lint` | 退出 0；仅既有 `test-refresh-gate.mjs` unused-vars 警告 | `attempts/lint.final.log` |
| 前端构建 | `pnpm build` | 退出 0，735 ms；仅有既有 chunk-size 提示 | `attempts/build.final.log` |
| Node 全量 | `node --test tools/test-*.mjs` | 433 passed / 0 failed，18 个文件 | `attempts/node-tests.final.log` |
| 英文错误链路 | `node --test tools/test-m6-i18n.mjs` | 1 passed；真实 Git 失败的 `detail` 经生产 `formatAppError` 输出英文且无中文产品文案 | `attempts/node-m6-i18n.attempt1.success.log` |
| x18n 检查 | `pnpm i18n:check` | 0 errors，62 warnings，1,607 / 1,607 翻译 | `attempts/x18n-check.final.log` |
| 硬编码中文门禁 | `pnpm i18n:extract` | 57 候选 / 0 中文，退出 0 | `attempts/i18n-extract.final.log` |
| 门禁负向探针 | `pnpm i18n:extract`（注入临时中文文件） | 58 候选 / 1 中文，退出 1 | `attempts/i18n-extract-gate.negative-attempt1.log` |
| 绑定空白 | `node tools/strip-binding-whitespace.mjs` | 83 个绑定文件清理后 `git diff --check` 退出 0 | `attempts/strip-bindings.final.log` |

### 真实 Git 集成覆盖

`src-tauri/tests/m6_flows.rs` 的 22 个测试全部使用临时仓库、本地裸远端、真实系统 Git 和临时 CLI 工具脚本，不 mock Git：

- stash 归档跨 `git gc --prune=now` 仍可恢复，用户工作区 / index / refs 不被改动；归档失败不删条目。
- stash 应用冲突保留条目、恢复暂存区可选、新分支应用 `{oid}^1`、应用后删除先归档。
- 按 OID 删除较旧条目、外部追加后拒绝旧 clear 集合、归档恢复为 stash 条目。
- 未跟踪文件不进入 stash save；`restoreIndex` 默认关闭。
- autostash 保存 / 恢复暂存与未跟踪改动、冲突保留 link 和 stash、轮转完成后归档删除。
- 操作元数据版本失配、冲突时禁止 continue、merge 禁止 skip、abort 前回收。
- 交互式 rebase edit 停点允许 amend，普通操作阻止 amend / 新提交；merge 范围 rebase 和 revert 在写入前拒绝。
- cherry-pick 暂存改动恢复、空提交跳过 / 保留、`-x` 条件、多提交进度。
- revert 显示顺序、reset Soft / Mixed / Hard / Keep 的 index / 工作区语义与 `HistoryReset` 快照。
- reword 改写目标提交而非只改 HEAD；rebase 计划重复 / 缺失项在运行前拒绝。
- 文件历史分页、根提交、二进制 patch、同路径删除重建边界、重命名跟踪、中文 / 空格路径、blame 行归属与上一版本。
- 外部 diff 历史版本无需工作区改动、未跟踪拒绝、merge 工具退出 0 但仍有冲突标记时不误报已解决。
- 真实 Git 失败产生的结构化 payload 写入 `src-tauri/target/m6-en-error-payload.json`，再由生产英文格式化链路断言。

### 失败尝试与修正

失败日志均保留，未覆盖：

1. 早期 Node 套件因 32 个文件直接导入 `@/lib/i18n` 目录而在 Node ESM 下解析失败，且部分断言仍期待旧中文错误。改为 `@/lib/i18n/index`，断言改为结构化错误码 / 英文 invariant 后，最终 433 个通过。
2. `m6_flows` 的同路径删除重建测试最初把刚创建的文件也写成 `D`，导致历史查询正确地返回空；测试改为先保留旧生命周期、再重建新文件，最终通过。
3. 英文化测试最初使用真实 Git 的本地化 stderr，CI / 本机 locale 不同会得到中文 `致命错误`。最终测试只在该测试环境设置 `LC_ALL=C` 取得确定性的原始 Git detail；生产仍继承用户环境，不改变用户 Git 配置。
4. 一次 `cargo fmt` 触及 15 个本轮未修改的 Rust 文件，已用格式化前状态与 `rustfmt` 输出比对确认等价后恢复，只保留本轮有意改动。

## 偏差、限制与未完成项

- 实施方未做真实桌面验收、没有固定 Release、没有 Rebased 截图对照；英文窄窗、真实 GUI 外部工具、完整交互式 rebase 编排和键盘焦点验收由规划方执行。这是流程分工，不是自动化通过项。
- 外部工具集成测试使用临时可执行 shim 验证启动、参数、快照和冲突复核，没有在自动化中打开真实 GUI 编辑器；桌面验收需至少跑一次真实 difftool / mergetool。
- 交互式 rebase 的固定计划执行器、计划校验、reword 和 edit 停点已有真实 Git 测试；squash / fixup / drop 的完整桌面点击流程留给规划方验收。
- x18n MCP 在当前 worktree 的资源映射曾返回 `X18N4005`；资源写入改用同版本 CLI `pnpm exec x18n`，最终 `x18n check` 和硬编码中文门禁均按真实输出判定，未跳过检查。
- 62 个 unused-key 警告是既有 / 迁移后未引用键；规格要求先警告、不自动删除，故保留并在此报告。
- 日志日期修复只做 headless 字体测量和构建验证，没有在真实桌面窄窗中截图；规划方桌面验收仍必须覆盖英文窄窗。
- Q98 / D06 等既有未达标项没有被本报告伪标为通过；它们仍在 backlog，除非规划方在桌面验收中转交。

## 任务单勾选

`docs/milestones/m6/issues/01-complete-m6.md` 中实施方负责的 C、S、O、H、F、T、L、L04、D01、V 和交付材料已勾选；规划方独立审查、固定 Release / 桌面验收和最终合并保持未勾选。

## Q173 授权后的续接状态（2026-10-29 01:15 UTC 附近）

用户已授权窄范围后端例外，记录为 Q173 / ADR-0014，尚未引入 git2 依赖或实现事务，不再等待用户对该例外答复。剩余关键功能是 stash 删除/清空的持锁精确目标事务及其跨 worktree / 终端并发、锁冲突、失败回执验证。删除出口目前先归档后明确拒绝，4 项功能测试保持失败，不降低原断言。

额外盘点已迁移：CommitDraftPersistence 保留错误载荷（failureFor 在读取时格式化，failurePayloadFor 提供结构数据），App 本地验证错误改 code / params，草稿提示 lazy 呈现；冲突/回收区/三栏历史/左右 diff/自动 fetch 等错误在显示时格式化；CheckoutNote 替代中文 notes 字符串；commit 回执与线程错误保留结构化 cause；history 三栏不可用说明结构化。x18n 新增 callee 仅识别 localizedError 的完整字面量键，没有放宽扫描或严重程度。

最近检查原件在 codex-checks-yf3rkhgl：translation-contracts（现有 lib、branch_remote 31、commit_flow 12）通过；m6-receipt-errors（相关 lib、history_readonly 43、M6 29 通过/4 明确排除）通过；四个排除项正是 m6-safe-boundary 原件中实际失败项，不能汇总成全套通过。新增两项生产形状的 Saving / Restored 持久回执回归通过。实际 Git 错误/嵌套 cause/通知切语言与真实 CRLF/非法名/游离 HEAD 警告 3 项通过；草稿 8、历史 7、远端 32 亦通过。远端的旧静态断言因改存 payload、渲染时格式化失败一次，原件 i18n-ui-03 保留，按新契约同步后 remote-write-updated 通过。frontend-latest、lint-latest 通过；extract-latest 57 候选/0 中文；x18n-latest 1641 键/双语完整、0 错误、63 未用键警告，完整列表保留、不删键。

自动审批服务额度限制：官方 libgit2 transaction.c / git2 Reflog 文档读取、linked worktree 索引 git add、automation_update 新 prompt 均未执行，提示北京时间 12:12（04:12 UTC）后重试。原始错误见 approval-service-rejection.json。不得绕路、改渠道或在该时间前反复请求；没有安全拒绝或模型渠道错误。当前 Git 提交仍 e770e22，所有修改未暂存，无法在该服务恢复前固定新的源码提交/新两轴/Release 验收。监控更新失败，原 automation 仍 ACTIVE，但本 manifest / monitor / ledger 为最新状态。

后续：按原渠道恢复官方源码核实及依赖获取，验证候选事务实际持锁范围、reflog 与 ref 失败边界、仓库格式兼容；实施后集中必要检查，再明确文件暂存到 m6 固定源码。按 code-review skill 新开两轴，不复用 review02，不再派 Pi / 其他实施代理。最后固定 Release / 真实桌面 / Rebased / 中英文窄窗；未验收不合 main、推送、PR 或删 worktree。
