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
