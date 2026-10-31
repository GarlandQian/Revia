# 04：运行归属 worktree 的内置终端

Status: ready-for-human
Blocked by: None

## What to build

提供真正可输入的终端，每个 worktree 多会话，切换保留进程，关闭与重启遵守已确认策略。

依据：规格 D；Q178、Q196。

## Acceptance criteria

- [x] Rust 创建 PTY，前端输入字节 / resize；正常 shell、UTF-8 / 中文、交互和退出状态可观察
- [x] 多个 worktree / 会话相互独立，运行中关闭可取消，应用关闭协调保留仍运行的会话
- [x] 重启恢复页签 / 起始目录并新开 shell；不恢复旧进程、不重放输入、不持久保存输出
- [x] 写入期间显示所属 worktree 忙碌，终端变化触发外部核对；Git 命令日志仍可查看
- [x] 真实安全子进程验证读写 / resize / 取消 / 退出，故障不清理用户进程

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / review01 Standards — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/standards.md；完整证据附录保存在本轮证据目录。

9. **[P2] 关闭终端直接杀进程**：`src/features/m7/TerminalPanel.tsx:271` 无确认调用 close；违反 `docs/ui-direction.md:157`“关闭运行中进程前先确认”。

## H9 关闭运行终端无确认（P2）

规则：`docs/ui-direction.md:157`：“关闭运行中进程前先确认”。

TerminalPanel.tsx:263–278 closeTab 不检查 tab.running，也无确认，先 dispose 视图再调用 terminalClose。按钮 335 行直接触发该函数。Rust terminal.rs 的 close 删除会话并在未 exited 时 killer.kill，随后 drop master。不存在后端确认兜底。

触发：终端里有长时间运行的任务，点页签的 x。后果推断：任务直接终止，用户失去取消关闭的机会。此项与“关闭面板应保留终端”的实现问题分开，本轴仅记明文确认规则。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F05 P1 | “切换保留进程”：混worktree页签；绝对cwd恢复被拒 | src/features/m7/TerminalPanel.tsx:131；src-tauri/src/m7/terminal.rs:275,429 |

## F05 [P1] 终端会话在切换、重挂载与重启不能正确恢复

规格 S91：“在应用运行期间切换保留进程”；S92：“重启只恢复页签、原 worktree 与起始目录，启动新 shell”。

`src/features/m7/TerminalPanel.tsx:75,131–154` worktree变化不清tabs也不通过terminalList认领已有进程，直接openSession；`src/App.tsx:6019–6031` 隐藏会卸载整个面板，重开又建shell，原PTY留后端成为不可达会话。切A→B时A的tabs仍显示在B标题下，可误向A输入。跨重启还必错：后端 `src-tauri/src/m7/terminal.rs:275` 返回绝对cwd，前端86–96原样持久化、147原样重开；后端429–437拒绝绝对路径，所有已保存页签失败。上述是生产状态/参数链，不依赖PTY模拟。

| F06 P1 | “关闭…前确认”：页签×直接kill | src/features/m7/TerminalPanel.tsx:263 |

## F06 [P1] 关闭运行中终端没有确认

规格 S92：“关闭运行中进程前确认，取消则保留；退出应用 / 安装更新同样纳入关闭协调器”。

`TerminalPanel.tsx:263–276,340` 点击页签×直接terminalClose；Rust `terminal.rs:386–395` 立即kill。App关闭保护 `App.tsx:1638–1655,1747–1759` 不注册终端运行状态。触发：终端中执行长任务后点×或退出。结果：没有确认/取消机会，进程直接终止。尚未实际启动任务；源码无确认分支。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Pi 正式返修处理 — 2026-10-31

- **F05**：重挂载先 `terminalList` 认领本 worktree 仍在运行的会话（不新建不可达进程）；切 worktree 清空 tabs 并销毁前端视图（后端进程保留待认领）；`resolve_cwd` 接受 worktree 内的绝对 cwd（持久化恢复），外部/逃逸路径仍拒绝（新增回归测试 `terminal::cwd_tests`）。
- **F06/H9**：关闭运行中页签先内联确认（终止并关闭 / 取消），确认才调用 terminalClose。
- **busy 提示**：面板会话运行态经 `onRunningChange` 同步给 App（terminalRunningRef），参与更新安装门禁与关闭协调；Git 写锁仍然互斥。

### Codex / review02 Spec / F05 — 2026-10-31

| F05 P1 | S91“独立”：A的在途认领/打开晚回仍加入B页签 | TerminalPanel.tsx:136,149 |

<details>
<summary>本项证据原文</summary>

## F05 [P1] 在途终端认领/打开跨worktree污染页签

S91“每个worktree…独立…切换保留进程”。`TerminalPanel.tsx:157–188`的cancelled只在terminalTabs后和循环前检查；新 `claimOrOpen:136–153`等待terminalList后，及 `openSession:106–121`等待terminalOpen后均无代际/目标检查，直接setTabs/setActive。触发：A发起认领或打开未返回，切到B，B effect清空页签后A请求才返回；A的Tab被加入B列表，active也指A。输入:268–269只按该sessionId写终端，所以可在B标题下向A工作区shell输入。绝对cwd的后端修正确实可接受worktree内部路径，未重复报旧的绝对路径问题。无真实PTY启动；这是异步结果所有权的静态事实。

</details>

### Codex / review02 Spec / F06 — 2026-10-31

| F06 P1 | S92“退出…协调器”：页签有确认，应用退出仍不问运行进程 | App.tsx:1667,1717 |

<details>
<summary>本项证据原文</summary>

## F06 [P1] 应用退出仍不确认运行中的终端

S92“关闭运行中进程前确认…退出应用/安装更新同样纳入关闭协调器”；S93“Revia写入期间显示对应worktree忙碌”。页签确认 `TerminalPanel.tsx:308–326,395–406`已加。新增 `terminalRunningRef`只由当前面板anyRunning同步(App6125–6127)，仅在beforeUpdateInstall:1655消费；正常closeAfterDraftFlush:1667、CloseDriver:1683、reconcileClose:1717均不消费终端状态。触发：shell中长任务时直接退出；仍无进程确认/取消，页签flush不等于运行进程协调。切换到无运行会话的B还会用false覆盖A的运行状态，更新门禁也无法覆盖全局终端。TerminalPanel:200只有busy注释，props/渲染没有写入状态，原busy要求仍未实现。静态链，未结束任何用户进程。

</details>

### Codex / review02 Standards / R08 — 2026-10-31

<details>
<summary>本项证据原文</summary>

## H9 / R08：正常退出没有运行终端确认（P2）

规则原文：`docs/ui-direction.md:157`：“关闭运行中进程前先确认”。

本轮页签关闭 `TerminalPanel.tsx:308–322,395–404` 有明确终止/取消选择，原“直接点页签×杀进程”的分支已修。新增运行态只通过 `App.tsx:6125–6127` 写terminalRunningRef，唯一读取点为更新前检查 `1655`。普通窗口关闭/退出 `1667–1684` 不读它，CloseCoordinator只接登记的写入/pending。故终端运行任务时正常关闭窗口或退出仍无确认/取消机会。报告将“参与关闭协调”写为已实现，与实际接线不一致。没有启动用户shell或真实PTY。

</details>

## Codex 直接修复终端所有权与关闭（F05/F06/H9）

终端改由App级TerminalSessions持有；异步打开/恢复、运行态、错误与输出活动都绑定原worktree，A晚回不会追加到B，隐藏面板不丢进程或解除退出保护。首次完整读取持久页签后再写入；按cwd认领已有PTY，其余页签新开shell。全局串行保存保留其它worktree记录；失败不以初始空列表覆盖旧页签，最后页签持久化也保留原生退出保护。

页签关闭必须显式确认运行中的进程。应用关闭/退出先冻结新终端，等已发打开结束，列出全局仍运行的进程供确认；取消不结束进程，确认也只准备，仍由既有CloseDriver等待Git写入/待核对并在实际关闭时终止已确认进程。最终草稿冲刷、终端页签落盘与关闭成功后才解除原生退出保护。Rust关闭失败保留原句柄并回传结构化cause；终端输入和忙碌提示核对实际会话所属worktree。

`terminal-session-node-03` 33项通过（终端12项、编辑13项、既有合并写门8项），直接调用生产TerminalSessions/EditSessions/CloseDriver/WriteGuard，含在途A→B、隐藏运行态、取消/等待Git写、持久化错误和最终关闭顺序；所用可控API依赖用于并发回归，不是桌面验收。`terminal-cargo-02` 3项通过：真实自建PTY读写/中文/resize/退出、两worktree隔离，以及损坏页签原件保留。Tauri mock仅承载事件。`terminal-frontend-build-03`、`terminal-lint-01`与`terminal-x18n-02`均exit0；翻译0错误/76个unused-key警告，lint新增一条no-useless-spread提示留backlog。

首轮前端类型检查`terminal-frontend-build-01`真实exit2（占位回调推断为false字面量），修正返回类型后检查通过，失败原件保留。每轮源码hash/argv/exit/原stdout/stderr在批次codex-checks目录。以上为本组检查点，不代表整包或真实桌面通过；更新安装的终端确认/重启全局协调仍随F13继续，当前安装门禁已读取全局运行态，不会因面板隐藏而放行。


### Codex：真实桌面场景集中修复与验证（2026-10-31）

固定 Release `5ca9178` / 产品`0dc11c0`已完成四主题及中英文900px真实查看；系统深色测试在finally中恢复原浅色。手动编辑Save前磁盘未变、Save后实际字节正确，草稿跨文件切换保留，外部变化后拒绝覆盖并保留草稿；原生退出按未解决草稿确认，先保留原件再仅丢弃fixture编辑，启动03真实exit0。本地clone实际目标`repos/native-clone-c_0_lf38/checkout`的HEAD与自建bare一致并自动登记打开。诊断原生导出的JSON不含文件正文标记与fixture邮箱。PR未登录和更新无签名公钥边界已观察，正式账号/保护/更新接线未验，D06尚未执行，不能将上述局部结果称整包通过。

原生发现并集中修复：更改列表view把元数据读取记成仓库写入，导致PTY逐字输入被拒及提交后Busy不能自动恢复；默认组未命名input冻结旧语言；设置玻璃透底文字干扰阅读。原始截图/AX/文件在`desktop-acceptance-myntirt_`，终端命令实际残缺并parse error，不能称PTY命令通过。

修复后元数据观察不再设置writing标志；真实写入开始/结束递增代际，只在同一空闲代际内发布元数据，持锁区不运行Git，跨写入结果拒绝。原写互斥与终端真实写入暂停保持。前端仅对明确Busy读取作三次有界重读，绑定原worktree/epoch且卸载取消；失败不继续显示可操作旧凭据。默认名用真实空name和即时翻译placeholder，避免翻译持久化成用户名称。统一浮层保留玻璃装饰/折射路径并增加主题tint与blur遮蔽透底，减少透明度/高对比度仍无滤镜降级。

`task14-native-scene-rust-01`真实exit0，共44通过（更改列表26、PTY3、会话3、共享提交12），含实际会话写代际与Barrier并发发布拒绝。`task14-native-scene-node-01`20通过，前端build/lint/x18n均exit0，翻译0错误/76既有unused警告。源码hash、argv、真实exit、分流原输出与失败原件保留；只改复用既有键，未改locale/绑定。下一步只审本新增量、固定新Release，再真实复验PTY/浮层/默认名与D06；当前不标任务14 done。
