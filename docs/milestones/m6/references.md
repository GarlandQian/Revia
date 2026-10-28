# M6 资料核对记录

核对日期：2026-10-28。这里只记录来源事实与未确认项，不产生新的产品决策；决定以 `docs/README.md` 为准。

## 本机 Rebased

安装包：`/Applications/Rebased.app`。`Contents/Info.plist` 显示版本 `1.1.22`、构建 `IC-262.10968.SNAPSHOT`。
以下来自只读安装资源与类方法检查，没有执行 stash、reset 或历史改写，也没有读取用户配置。
静态核对不是运行时复现或桌面验收；未证实的行为不能补成已通过。

Git 插件的资源基址为 `Contents/plugins/vcs-git/lib/`；表中的共享资源指
`modules/intellij.vcs.git.shared.jar!/messages/GitBundle.properties`，类来自 `vcs-git.jar`，另有注明者除外。

| 项目 | 本机证据 | 能确认的结论与边界 |
| --- | --- | --- |
| stash 保存 | `GitStashDialog`；`GitStashUtils#createStashHandler(Project,VirtualFile,boolean,String)` | 说明、Keep index；处理器拼接 keep-index / message 后委托通用处理器，未发现额外 include-untracked / -u 常量。这支持不额外纳入未跟踪文件的判断，尚非真实保存实验。Keep index 默认值仍须桌面核对。 |
| stash 应用 | `GitUnstashAsDialog` 字段初值与 `bindSelected` | Pop stash、Reinstate index、As new branch；新对话框前两项布尔字段初值 false，分支为空。 |
| 清理工作区 | 共享资源 `settings.clean.working.tree`、`local.changes.save.policy.stash/shelve`；`GitChangesSaver#getSaver` | 存在 Stash / Shelve，实际分别构造两种 saver，而非仅残留文案。Revia 按 Q161 固定 stash、不提供该设置。 |
| 自动保存恢复冲突 | `GitStashChangesSaver#load`、`GitStashOperations.unstash` 与专用 conflict resolver | 有恢复冲突提示及后续手动删除条目的文案；不能据此断言冲突时自动删条目或自动继续原操作。 |
| reset | `GitResetAction#actionPerformed`；共享模块 `GitResetMode#getDefault` | Soft / Mixed / Hard / Keep；无已保存模式时初始 Mixed，确认后用 `setResetMode` 记住本次选择。 |
| 空 cherry-pick | `GitVcsApplicationSettings$State`；`GitCherryPickContinueProcess#isEmptyCommit` | 初始策略 SKIP，另有创建空提交；检测包含重放后变空。尚未分别核实原提交本来为空的全部路径，也未证明执行前确认页是否展示该策略。 |
| 来源后缀 | `GitVcsOptions`；`GitCherryPickProcess#shouldAddSuffix` | 设置默认开，但实际还要求 `isCommitPublishedBlocking`，不是每次 cherry-pick 都加来源。 |
| 多选顺序 | `intellij.platform.vcs.dvcs.impl.jar!/VcsCherryPickManager#lambda$cherryPick$0`；revert 流程入口 | cherry-pick 将日志完整详情列表反序后执行；revert 直接传选择列表。最终顺序仍须按实际日志选择核对，不将容器顺序无条件称为拓扑顺序。 |
| merge 提交 | `GitRevertAction`；共享资源 `action.description.cant.revert.merge.commit` | revert 拒绝父提交数大于 1 的提交。未证明 cherry-pick merge 的主线选择方式。 |
| 改提交信息 | `GitCommitEditingActionBase`、`GitRewordAction` | 检查提交在 HEAD 历史内和受保护远端分支；HEAD 有进行中操作的特例，不能据此直接放开 Revia 的所有提交门禁。 |
| rebase 暂停 | `GitRebaseProcess` | 有继续通知和显式暂存重试入口；未完整证明 edit 停点的 GUI amend 路径。 |
| 文件历史 / blame | 共享资源中的 show all branches、ignore whitespaces、detect movements within/across files | 存在用户指定的入口；三个 blame 开关的初始化及“上一版本”具体导航仍须核对。 |
| 自动解决 import 冲突 | `Contents/lib/intellij.platform.diff.impl.jar!/MergeImportUtil$Companion` | 依赖 `PsiManager` 和 `ImportBlockRangeProvider.getRange/isFileSupported`。不是匹配 import 文本行后拼接；未找到足以证明特定语言支持的 provider 声明，不能承诺 Vue / TS / Swift 的等价行为。 |

## x18n

本轮通过 Grok MCP 直接读取用户提供的原始页面及关联官方文档：

- [Agent workflows](https://x18n.dev/agents/workflows)：Agent 以 CLI 的 JSON 输出查询文案，使用 set / apply 等命令写入语言资源，再执行 check；批量导入拒绝占位符不一致的值。迁移期间保留命令退出码与被拒键，不能忽略失败。
- [Getting started](https://x18n.dev/guide/getting-started)：要求 Node.js 22 或更新版本，可作为项目开发依赖安装，并以配置描述资源；提供检查命令接入项目验证。
- [Configuration](https://x18n.dev/guide/configuration)：资源支持 JSON 和多种语言文件格式，可配置源语言、资源路径与使用位置扫描；预设针对既有 i18n 库，不能据此假定运行时语言切换已实现。
- [首页](https://x18n.dev/)：定位为 i18n 资源检查与维护工具，CLI、编辑器、UI 共用引擎。

当前工程只有静态中文文案对象，包含函数、平台快捷键和动态参数；尚未安装 x18n，也未完成语言运行时接线。
本机核对到 Node.js `v26.5.0`、pnpm `12.14.0`；正式实施仍需记录实际安装并锁定的 x18n 版本。
Q163 已将简体中文 / English 和 x18n 提前到 M6；具体即时切换、翻译边界与验收规则须在规格中明确。
