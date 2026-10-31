# 11：确认下载并安装应用更新

Status: ready-for-human
Blocked by: 03, 04

## What to build

启动检查稳定 / 可选预发布，在明确下载、处理当前工作并再次确认后安装重启。

依据：规格 H；Q181、Q182。

## Acceptance criteria

- [x] 检查 / 下载 / 已下载 / 安装 / 错误状态分开，取消不关闭，离线失败保留当前应用
- [x] 生产来源与公钥受信任，签名必须核验；未配置说明原因，不以测试值冒充生产
- [x] 安装协调草稿 / 合并编辑 / Git 写入 / 结果待核对 / 终端，处理失败不安装
- [x] 独占测试目录验证好坏签名、损坏 / 网络失败、取消与关闭门禁；不替换真实运行应用
- [x] 实际签名更新和首次下载启动交任务 14 / 外部接线，未执行要明确记录

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F13 P1 | “分开”“安装前协调”：下载即安装、绕门禁、重启按钮不可达 | src-tauri/src/m7/update.rs:260,293；src/features/m7/UpdatePanel.tsx:145 |

## F13 [P1] 更新下载即安装并绕过关闭协调，安装后无重启入口

规格 S122：“检查、下载、已下载待安装、安装 / 重启、失败状态分开，用户确认后下载和安装”；S123：“安装前协调未保存编辑、合并编辑、在途 Git 写入、结果待核对与运行中终端，再确认重启”。

`src/features/m7/UpdatePanel.tsx:73–90` 一次点击updateInstall；`src-tauri/src/m7/update.rs:260–278` 直接download_and_install，无下载待安装/取消状态；App5361–5364仅flushM7State并return true，既有CloseDriver没参与；commands691直接app.restart。安装后update.rs293清available，但UpdatePanel145重启按钮位于131 `status.available` 分支内，所以成功后永不可见。触发：正式来源接好后按更新安装；可边Git写入/终端运行边进入。结果：未经第二次确认安装，工作未协调、无法按界面重启。当前未接正式源，未实际安装；属于配置可用后确定的控制流缺陷。

| F22 P2 | “主动开启预发布”：无渠道选择，另增无效automatic方式 | src-tauri/src/settings.rs:123；tools/check-release-version.mjs:53 |

## F22 [P2] 缺稳定/预发布选择，增加无效更新方式

规格 S122：“默认稳定版；用户主动开启预发布”；S126：“公开后才进入所选渠道”。

`src-tauri/src/settings.rs:123–139` 只有Manual/Notify/Automatic；`settings.tsx:379–432` 同样无预发布开关；`update.rs:207,247`始终构建固定配置endpoint，`tools/check-release-version.mjs:53–58` 预发布端点还固定为当前tag，后续beta无法发现。Automatic文案说自动下载但303–328只检查。触发：稳定版用户想主动订阅预发布或选自动下载。结果：没有所需渠道，新增方式无对应行为，偏离已确认取舍。无部署前提能补齐本地缺失字段/分派。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Pi 正式返修处理 — 2026-10-31

- **F13**：更新拆为 检查 → 确认下载（update.download，只下载不安装）→ 已下载待安装 → 确认安装（download_and_install 校验 downloaded_version 匹配，未下载拒绝）→ 重启按钮独立可达（receipt 存在即显示，不再嵌 available 分支）。安装前门禁 beforeUpdateInstall 走完整协调：编辑草稿落盘 + 在途 Git 写/待核对（CloseCoordinator）+ 终端运行中（terminalRunningRef）+ 脏编辑，任一存在则双语错误阻止安装。
- **F22**：AppUpdateMode 移除未实现的 automatic；新增 allow_prerelease 设置（复选框 + settings patch），check 按 semver 预发布段过滤并动态切换 latest-prerelease.json endpoint（updater_builder().endpoints）。
- **F14/F15**（任务 12 范围，这里联动的 endpoint 生成）：见任务 12。

### Codex / review02 Spec / F13 — 2026-10-31

| F13 P1 | S122–123“分开”“协调”：丢下载包、重下；旧回执遮新版本；重启绕协调 | update.rs:370,407；UpdatePanel.tsx:55,112,154 |

<details>
<summary>本项证据原文</summary>

## F13 [P1] 更新分步状态、后续版本和关闭协调均未闭环

S122“检查、下载、已下载待安装、安装/重启…分开”；S123“安装前协调…合并编辑…结果待核对…终端…再确认重启”。后端download_inner:365–380调用download取得Vec后直接丢弃，只记version；install_inner:407–423再次download。所谓“已下载”无法离线安装，重启也没有保存包；公共AppUpdateStatus:51–61根本不含downloaded_version，前端不能恢复已下载态。fetch_update:327使用默认builder，未沿check_inner:250–269选择用户渠道。

新 `UpdatePanel.tsx:55`把任意历史receipt.version设为installedVersion；154先判断它，永远优先显示重启区而隐藏status.available。触发：完成一次更新并重启后，持久化旧安装回执仍在，后来有新版本，仍只能看到旧版重启。install成功分支:96–99本身没有setInstalledVersion，也仍依赖额外effect异步回读才使重启可见。

beforeUpdateInstall:App1646–1663只拒绝当前dirty/当前面板running与旧Git集合，没有协调合并编辑和全局终端，也没登记保存生命周期。`UpdatePanel.tsx:110–112 restart`直接updateRestart；m7命令最后直接app.restart，重启不再检查从安装至点击重启之间新增的编辑/写入/终端。正常仓库入口另被F02遮挡，不因入口缺失免除更新流程的确定缺陷。未执行下载、安装或重启真实应用。

</details>

### Codex / review02 Spec / F22 — 2026-10-31

| F22 P2 | S122“预发布”：两个清单同指latest稳定资产；保存开关未更新界面 | release.yml:158,168；App.tsx:4369 |

<details>
<summary>本项证据原文</summary>

## F22 [P2] 预发布开关与发布渠道均不可按约定工作

S122“默认稳定版；用户主动开启预发布”；S126“公开后才进入所选渠道”。App:4362向Rust保存allowPrerelease，却在updateSettings后只更新appUpdateMode:4369，从未setAllowPrerelease(next.allowPrerelease)；setter唯一生产调用在启动1306。checkbox为受控，用户勾选后仍显示false，无法在同一会话取消后台已开启状态。

渠道：release.yml:158所有产物URL固定releases/latest/download；168把stable manifest原样复制为latest-prerelease.json。稳定构建开启开关后仅替换文件名(update.rs:250)，仍读latest稳定Release里的同一版本，不能发现后续beta。beta构建endpoint又固定到构建时tag(check-release-version:58)，不能发现后续beta；beta清单的安装URL却仍指latest稳定资产，可能404或下载不相符版本。`fetch_update:327`下载/安装还不应用开关选择。GitHub官方明确draft/prerelease不可作为latest，已从[官方Release API文档](https://docs.github.com/en/rest/releases/releases#create-a-release)核对make_latest；原件github-release-official.txt。Grok读取GitHub页因网络错误失败，web.run open也不支持当前模型，改可用search_service原页读取完成。没有运行Actions或发布。

</details>


### Codex 直接修复 F13/F22 — 2026-10-31

已下载的真实包字节、散列和签名清单保存在应用数据目录；安装用锁定 Tauri updater 2.14.1 的 restore_update / verify 恢复并再次验签，直接安装保存的字节，不重新下载。签名必须绑定版本，旧的无版本签名不可恢复。公共状态补 downloadedVersion / restartRequired，历史回执不遮住新版本，重启后的旧回执不重复要求重启。下载回包同步全局状态，重新打开面板的较早读取不能覆盖操作结果。

安装和重启分别进入生产 CloseDriver / WriteGuard / CloseCoordinator，同步处理未保存编辑、Git 写入 / 待核对和全局终端；取消不安装、不重启。合并编辑仍归原 CodeMirror 实例，未应用内容先通过原有应用 / 取消入口处理；生命周期保留窗口与退出保护。安装成功回到应用，重启需要独立明确操作；失败取消本次更新意图，不自动重发。

渠道从构建配置绑定仓库的公开 Releases 完整分页中选取最高合适 semver；默认稳定，主动开启才含预发布，草稿排除。两类客户端使用同一仓库发现入口，再读取所选 tag 的清单；包 URL 固定到自身 tag，beta 客户端不再锁在自身旧标签。发布覆盖配置要求 requireSignedVersion，Tauri CLI 2.12.3 本机帮助确认 build 自动绑定签名版本。未读取真实私钥、未运行 Actions、未发布；docs/release.md 已同步新流程。

最终原件在批次 codex-checks：Rust 全部 23 个 integration targets 及 lib/main 已逐目标完成；首轮 codex-final-rust-01 真实 exit101，故不能称单轮全过。其前半 targets 通过，GitHub auth fixture accept 后读取 WouldBlock 导致失败；仅给隔离 HTTP fixture 的 accepted socket 明确 blocking，未改产品网络实现。codex-final-rust-continuation-01 后被用户中断，command 无真实 exit（保留 null 与 interruption.json），不得补造退出码。continuation-02 的剩余 11 targets exit0；另为 auth / checkout 补独立真实 exit。codex-final-doc-01 exit0、实际无 doc 用例。discard_recovery 保留原 1 ignored；不重复叠加授权子进程内外计数。

最终 codex-final-node-04 的 8 个相关生产模块入口 98 项通过、0 skipped；codex-final-frontend-build-02、codex-final-lint-03、codex-final-x18n-02 exit0，翻译 0 errors / 76 既有 unused-key warnings。原 updater-lifecycle-node-01 发现失败意图仍保留，修正后共享回归通过。另一次 codex-final-node-03 误用通配符，包含需要专用 fixture 参数的旧 M6 脚本，494 通过 / 8 失败、exit1；原件保留，按正确相关入口重跑通过，没有改旧 M6 范围以消除该调用错误。源码散列、精确 argv、真实 exit、分流 stdout/stderr 与全部失败原件均保留；生成绑定只清理本次空白，受影响前端检查在清理后执行。

update package 新 3 项实测下载验签、保存、关闭 fixture HTTP、新 updater 离线恢复并实际安装，只替换独占 fixture .app；另 3 项渠道与 1 项公开旧签名元数据回归通过。含坏包、版本 / 公钥 / 渠道篡改及原子落盘失败。上述均不代表真实 Revia 桌面更新、真实账号 / 钥匙串、发布 CI 或正式签名来源已验。任务13仍缺 Rebased 1.1.22 既有暂存内容真实桌面观察，14最终桌面受阻；不合 main、不删除 worktree、不删除监控。
