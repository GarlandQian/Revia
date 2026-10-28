# M6 文案迁移规则（待确认）

Status: needs-info

Q163–Q168 已确认双语功能与验证；本文件仅提出 x18n skills 要求由用户决定的资源规则，不是已生效决策。
确认前不迁移语言资源，不安装或改写全局工具配置。

## 源语言、键与语气建议

- 源语言 `zh-CN`，目标语言 `en`；这与首次启动选择哪种界面语言分开，首次启动仍严格按 Q164。
- 保留现有语义分组与 camelCase 末级键，如 `toolbar.chooseRepo`；资源按现有嵌套分组管理，不为迁移批量改名。
- 新增 Rust 用户错误用独立 `errors` 分组和完整字面量键；用显式错误类型到键的映射，不运行时拼接翻译键，不把原中文塞入参数。
- 中英文均简洁、中性；按钮用清楚的动作，不引入营销或拟人化语气。动态值、快捷键与复数通过参数表达，保留平台差异。
- 品牌名 Revia、Git、Rebased、VS Code、x18n，以及 HEAD、worktree、stash、rebase、cherry-pick、revert、reset、blame 等 Git 专名保持原样。

## 术语建议

| 简体中文 | English | 区分 |
| --- | --- | --- |
| 仓库 | Repository | 不叫 Project |
| 工作区 | Working Tree | 不叫 Workspace |
| 暂存区 | Staging Area | 对应 Git index |
| 暂存区模式 | Staging Area Mode | 与未来更改列表模式区分 |
| 更改列表 | Changelist | 本轮不提前实现该模式 |
| 已暂存改动 | Staged Changes | 与工作区内容区分 |
| 未暂存改动 | Unstaged Changes | 与已暂存区分 |
| 未跟踪文件 | Untracked Files | 与已暂存新增文件区分 |
| 未提交改动 | Uncommitted Changes | 包含已暂存、未暂存和未跟踪 |
| 块 | Change Block | Revia 可操作的连续改动段，不与完整 Git hunk 混用 |
| diff 指纹 | Diff Fingerprint | 不是审阅标记 |
| 丢弃 | Discard | 不是 revert |
| 回收区 | Recovery | 不是 stash 或 Shelf |
| 回收记录 | Recovery Entry | 不是 stash 条目 |
| 恢复 | Restore | 普通文件恢复不改变暂存区 |
| 提交前快照 | Pre-commit Snapshot | 与用户 stash 区分 |
| stash 条目 | Stash Entry | 属于 Git 仓库 |
| 恢复为 stash 条目 | Restore as Stash Entry | 不直接应用到工作区 |
| 颜色主题 | Color Theme | 与系统深浅色区分 |
| 审阅标记 | Review Mark | 动作用 Mark as Reviewed |
| 审阅进度 | Review Progress | 不阻止提交 |
| 提交日志 | Commit Log | 不是命令日志 |
| 分支图 | Commit Graph | 不叫分支列表 |
| 提交 diff | Commit Diff | 只读历史比较 |
| 交互式 rebase | Interactive Rebase | 与更新区分 |
| 文件历史 | File History | 与 blame 区分 |
| 更新 | Update | 不是刷新界面 |
| 推送 | Push | 不叫 Upload |
| 强制推送 | Force Push | 安全语义仍只用 force-with-lease |
| 受保护分支 | Protected Branch | 不是默认分支 |
| 智能检出 | Smart Checkout | 保存再恢复本地改动 |
| 强制检出 | Force Checkout | 覆盖前先回收 |
| Git 命令日志 | Git Command Log | 不叫 Commit Log |
| 诊断信息 | Diagnostics | 不包含用户文件与 diff |
| 结果待核对 | Needs Verification | 不等于未执行 |
| 进行中的操作 | Operation in Progress | 不等于一定有冲突 |
| 编辑停点 | Edit Stop | 允许 Q166 的提交操作 |
| 冲突停点 | Conflict Stop | 处理冲突后仍须明确继续 |
| 领先 / 落后 | Ahead / Behind | 截至上次 fetch |

## 检查策略建议

- 缺失键、缺失翻译、空翻译、占位符不一致、无效消息语法作为错误。
- 用户界面代码中的写死中文作为错误；仅针对可见产品文案，不把用户内容、测试数据、内部日志或语言资源本身误报为界面硬编码。
- 未使用键与术语检查先保留警告，完整报告并复核；不自动删除键、不用全局 strict 把所有警告升级为失败。
- 已知动态用途先显式记录或改为完整字面量映射，不能靠忽略全部文件使检查通过。删除、广泛改名和更改检查级别仍须依 skill 请求确认。
