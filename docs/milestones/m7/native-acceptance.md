# M7 原生验收最新记录（2026-10-31）

本记录覆盖早期“无桌面权限、任务13待Rebased事实”的停点。实际 ChatGPT 宿主的辅助功能和截图均已可用。

- 产品提交：`8eccde51e3cba87522fca4aa43324c185211435c`。
- Release 源码/审查文档提交：`dfcdfe5622cf4aff18b9dae9f3bed90f05a040df`。
- 固定产物：`/private/tmp/revia-m7-batch-g5scyw0r/fixed-release-native-3kjfm1yi`。
- 二进制 SHA256：`3a61084ad2538562d4bc840913454013c3ae70fcea38de6c2e98946c40869fcc`；构建 `task14-native-release-build-04` 真实 exit 0。
- 最终复验后核对1048项产品源码/构建配置散列，均与固定构建一致。后续提交只更新验收文档，不沿用旧源码测试替代新修改。

## 任务13：done

Rebased 1.1.22 的既有暂存、同文件部分暂存、模式切换三组原生事实已先记录到 [观察记录](rebased-index-observations.md)，再实施。此前原生分组、改名、默认组、整文件/独立Git块移动、删组回默认、勾选重启保持及切模式不改Git的原件保留。

本次在最终固定Release新建独占仓库 `final-changelist-h3vh9irk`：line4预先暂存，line30未暂存，另一existing.txt也预先暂存。真实界面创建Pick，将line30移入Pick并设Pick默认，仅勾选line30，执行一次提交。

真实提交 `e9000d4f16ea977c5729018847a9486f2a39da91` 的断言全部通过：

- HEAD只含line30；mixed.txt的index等于新HEAD；line4保留在工作区成为未暂存。
- 未触及existing.txt的原暂存内容没有混入提交，仍保留在index。
- 所有工作区文件字节不变。
- 未选line4仍属于原Default组，未因Pick为默认而改组；无失配副本，勾选清空。
- 提交后列表自动恢复可操作，无手动Refresh。

原件：`desktop-acceptance-myntirt_/task13-final-native-{fixture,proof}.json`、`codex-checks/task13-final-native-git-proof-01`、`evidence/revia-native/replay05-{commit-ready,commit-selected,after-commit-auto,changelist-final-shot}`。此前proof01/02、group-recheck及所有失败仍保留。任务13验收勾选完成；不表示整包14通过。

## 终端与浮层修复的最终复验

原Release03的95字符丢字和其他输入焦点被抢的失败保留。产品1536e0a稳定每页签ref，避免刷新销毁/重建xterm；review11发现字体fit缺PTY尺寸同步，产品8eccde5统一实际onResize事件。review12为Standards0硬违反/0判断、Spec0项P1/P2，S12-SP01的80×24重挂载构造边界仅backlog15。两轮原文分别已入任务04/14 Comments，代理结束。

最终Release真实逐字输入163字符命令，严格参数断言通过，Python PID20101/parent19938，HOME/cwd均在本批独占根。持续终端输出时，提交说明中的70字符完整保留，未被终端抢焦点。字号16→20时，同一真实PTY从92×9变成74×7；随后真实设置恢复12。全局退出确认明确列出两个不同fixture worktree的shell；确认后正常退出，launch05真实exit0。未重放旧命令或操作用户进程。

原件：`native-replay05-summary.json`、`pty-final-process-proof-02.json`、`pty-font-size-comparison-02.json`与replay05原生动作/截图。第一轮字号采样未建立预期的12→16设置差异，两个实际值均92×9，比较断言失败；原件和说明`font-sampling-first-attempt.json`保留，不算产品通过或掩盖成正常采样。

最终设置浮层截图已直接查看，背景文字不再穿透干扰正文；代码diff截图清晰。四主题、双语900px、编辑Save/草稿跨文件/外变拒绝、诊断导出与本地clone此前真实原件继续保留。以上为逐项证据，不能据此把全部任务01–12或任务14自动标done。

## 任务14：D06未达标，继续处理

下面三轮均使用同一最终固定二进制、独占HOME/appdata/本地fixture。实际窗口focused=true、hidden=false、focusLosses=0、hiddenTransitions=0，rAF约60Hz，记录器真实exit0。

| 原件轮次 | 视图 | 首屏 | list_changes | 取行/渲染等待 | 150ms门槛 |
| --- | --- | ---: | ---: | ---: | --- |
| task14-d06-split-native-01 | 并排 | 252ms | 104ms | 148ms | 未达标 |
| task14-d06-unified-native-01 | 统一 | 176ms | 102ms | 73ms | 未达标 |
| task14-d06-split-native-02 | 并排 | 220ms | 95ms | 125ms | 未达标 |

并排02仅为补运行中原生截图而使用全新appdata重放一次；首轮应用自动退出后，晚到的置前请求失败，不伪造它有截图，也不以第二轮覆盖首轮252ms失败。统一01和并排02各保留四张真实窗口截图，已直接查看抽样图；没有用截图颜色统计替代验收。

并排跳转计时49/37ms，但该记录没有缓存来源，不能称“未缓存跳转通过”。统一首跳16ms的targetWasCached=true，返回8ms、文字1帧内；同样不能证明未缓存100ms门槛。自动可见滚动各60帧，数据覆盖未缺行；这与触控板长段目测范围分开，整项D06仍未通过。JSON中的ok=true只表示本轮测量成功且可见性有效，不表示性能门槛通过。

完整原JSON/可见性、二进制与输出散列见 `desktop-acceptance-myntirt_/task14-d06-audit.json`。诊断采样profile01因进程自动退出未取得调用树，profile02使用已有KEEP_OPEN取得AppKit/Rust调用树；采样扰动下的计时不纳入验收。现有采样不足以把瓶颈归因于材质、诊断持久化或某一具体函数，未据猜测修改产品。下一步应分别定位约100ms列表读取与并排首屏125–148ms等待，并补真正未缓存目标证据。

## 用户确认保留未验

用户本轮明确回复“尚未准备，保留这两项未验”：

1. 专用GitHub App client ID及真实账号/PR/保护接线。
2. 更新验签公钥、测试清单端点、实际安装/重启接线。

不读取现有账号凭据、不索取令牌或私钥、不把本地HTTP/验签回归冒称上述正式桌面接线通过。Windows实机、CI/真实长测没有新证据，仍如实未测。任务14保持未完成；未push/PR/tag/发布、合main、删worktree或删除监控。


## 后续补证：统一视图真正未缓存目标（2026-10-31）

为避免原13,435显示行的小文件在测量等待期间已全量预取，本轮额外创建独占大文件fixture（原自建D06数据的四倍；53,737显示行）。这仅补冷跳证据，不替换原M0/D06数据与252/220/176ms首屏失败。

同一固定Release的真实前台窗口测得：`targetWasCached=false`，目标原有34行缺口；加载占位在同步DOM中已出现，文字DOM 12ms内补齐，跳转总计24ms。返回目标已缓存，文字DOM 2ms、总计4ms，`warmWithinOneFrame=true`。首次合成回调21ms且`frameReady=false`，因此不将整条“冷加载1帧内”条件一起判通过。自动可见滚动60帧，缺行0；原生截图已直接查看。记录器真实exit0，focused=true、hidden=false、focusLosses=0、rAF约60；应用正常自动退出。

原件：`task14-cold-fixture.json`、`task14-d06-cold-unified-audit.json`、`codex-checks/task14-d06-cold-unified-01`与其四张原生截图。首屏门槛仍失败、并排冷缓存来源仍未证明，任务14不done。产品源码未改，未重复构建、测试或开审查代理。

本轮只读源码核查排除了“仅绑定RowWindowStore就自动全量取行”和“未开启编辑时也挂载编辑器”的假设；未减少身份重验或数据保护。`launchctl procinfo`需要root而未执行成功，未尝试提权或修改全局配置，也未采样归属不明的WebContent。现有Rust/AppKit栈不足以定位前端热点，后续应补生产IPC边界的测量时序，而非据猜测改性能或安全门。


## D06：生产边界时序与并排缓存来源（2026-10-31 后续诊断）

这轮仅补观测，并修复旧测量日志多写者拼行；尚未实施性能优化。产品 `6021058deb8eb30b23c72907739a2e3177df0fd7`，Release 源码 `5d892029e095bc1c3a74980240df4a977592fc85`。新固定目录 `fixed-release-d06-zxligxrm`，二进制 SHA256 `e13744ef4c328a44903302533ca0bda2496fdf14ba48bc7232fc82d4351eabf7`；`task14-d06-timing-release-build-01` 真实 exit 0。原正式产物 `fixed-release-native-3kjfm1yi` 及原 252/220/176ms 失败不覆盖。

时序观测最小检查：既有 Node 80 项、Rust 段计算 10 项、前端 build/lint 均 exit 0；日志完整性修复另以真实独占临时文件、8线程512条记录验证通过。review13 的 Spec S13-SP01 日志拼行 P2 已修；review14 Standards 无 P1/P2、1项P3测试组织差异记 backlog15，Spec0项。两轮正式原文分别已进任务14 Comments，不再续接代理。

`task14-d06-timing-split-01` 使用原 D06 独占 fixture、全新独占 HOME/appdata 和真实前台可见窗口，记录器 exit 0。focused=true、hidden=false、focusLosses=0、hiddenTransitions=0、rAF约60Hz；四张原生截图已直接查看，首张是添加仓库状态，其余含首屏/滚动/最终224ms测量界面。测量后全部7,887项源码散列与本轮记录一致，二进制散列不变；进程44971已退出，只是历史PID。

- 首屏 **224ms**（状态往返96ms，首次行/绘制等待127ms），仍超过150ms。
- Rust `session.refresh` 95.020ms：status17.878ms、两个diff合计56.477ms、parse2.933ms、条目/快照锁1.587ms、身份及审阅状态16.138ms。相同线程四次Git调用总89.503ms，其中已确认的命令记录阶段合计26.035ms；这些是包括新增观测影响的墙钟时间，不能把跨线程并发命令相加当关键路径。
- `side_by_side_layout` worker28.353ms，其中全文读取25.421ms，布局一致性和段/折叠派生本身0.528ms；前端layout往返31ms。首批 `side_lines` worker约0.545/3.275ms，IPC约3.082/3.391ms，前端Promise却15/16ms。layout响应后约16ms才发首批取行，后续前端提交/绘制等待仍需定位；这排除了“段布局派生本身耗去百余毫秒”的猜测。
- 189条Rust TIMING、14条前端 TIMING_JS 均可按完整JSON行解析，没有损坏记录。测量模式新增I/O可能影响计时，本轮用于归属，不当优化后门槛通过。
- 并排首次目标跳转前缓存 **false**，旧测量边界总50ms；返回目标缓存 **true**、调用返回时数据已覆盖，总39ms。只补缓存来源与原边界数据；split仍未采集独立文字DOM/首合成帧证据，不据此宣称完整100ms/1帧验收通过。自动60滚动帧缺行0与真实截图抽样分别记录，不替代长段目测或整项验收。

原件：`desktop-acceptance-myntirt_/task14-d06-timing-audit.json`、`task14-d06-timing-original-hashes.json`、`apps/d06-timing-pblrlky4/{measure.json,measure.log,timing-analysis.json}`、`codex-checks/task14-d06-timing-split-01` 和四张截图。下一步沿已确认的前端状态发布/布局挂载等待与命令摘要记录成本定位或局部修复；保留同步持久化、脱敏、过期、故障/清空和身份重验边界，不凭猜测弱化保护。任务14保持 ready-for-agent；GitHub/更新正式接线按用户要求保留未验，Windows/CI/长测无新增证明。
