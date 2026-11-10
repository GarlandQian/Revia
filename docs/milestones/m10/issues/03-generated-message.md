# 03：生成提交信息的目标归属

Status: done
Blocked by: None（可立即开始）

## What to build

交付[规格](../spec.md)中与生成提交信息的目标归属对应的完整行为。前三项由同一个 Pi 会话实施，04、05 由规划方接手；无依赖项可独立验证，实施顺序优先 01 → 02 → 03。

## Acceptance criteria

- [x] 请求生命周期和目标输入资格集中在同一个生产 module，显示层不再拥有重复的请求时序。
- [x] 流式完成、取消恢复、失败恢复与一步撤销保持既有行为，草稿与提交检查继续生效。
- [x] 切换、新请求、手动编辑或输入清空后，旧 delta/done/error/invoke 失败不能改写新输入。
- [x] 异步确认和监听安装有目标资格，取消先失效本地请求，后端取消失败也不接受迟到结果。
- [x] 复用现有 Node 测试与受控事件 adapter，执行生产 module 和提交输入组合。

## Comments

### 2026-11-09 · 规划方收尾与桌面复验

- `8300bfa`：异步配置和确认回包读取最新模式；共同发起入口再次校验完整意图；更改列表 amend 的首次确认包含原提交内容与信息。
- `53ec183`：修复真实桌面复现的切换取消草稿串写。生成写入携带 worktree、输入代次和内容，保存始终使用该目标，界面只镜像当前目标；`resetTarget` 先恢复并保存旧目标再重置。提交区卸载仍取消，worktree 切换不再由子显示层重复协调。
- 生产生成 module + CommitComposerState + CommitDraftPersistence 组合回归覆盖 A/B 不同草稿、切换 render 已指向 B 而状态仍属 A、返回 A 恢复原文，以及迟到事件和手动接管。该增量 23 项相关 Node 回归通过，两轴无剩余意见。
- `53ec183` 的 macOS 可见窗口已复验：首次发送本机地址确认、替换确认、流式完成、一步撤销、显式取消、生成期间 A→B→A，两份草稿各自保留。原始截图在报告列出的 `planner-draft-fix-comz2fr1`。

### 2026-11-09 · Spec 审查（af0a609...d8babd7）

- SP1 / P1：CommitComposer 的取消清理 effect 依赖 generation，而 App 每次 render 创建新的 generation 对象。begin 的输入/state 更新就会触发 cleanup，正常生成自行取消。生命周期不得受呈现对象引用变化驱动，补覆盖生产接线的行为验证。
- SP5 / P2：handleGenerateClick 等 aiStatus 后才捕获 target，却使用点击时的旧 message/mode。等待期间用户输入可绕过替换确认，切换会对新 worktree 发起旧点击。点击时捕获完整意图，异步完成后仍须匹配；确认弹窗期间 mode/selection 的归属也应维持一致。
- SP6 / P2：CommitComposer 用 generationError ?? aiError 决定显示却渲染 aiError；生成失败通常只剩图标。渲染相同的合并后文案，保留结构化错误翻译。

### 2026-11-09 · Standards 审查（af0a609...d8babd7）

- ST1 / P2：同 SP6，违反 docs/technical-direction.md 报错须在呈现层翻译并完整显示的要求。

### 2026-11-09 · 返修（实施方，SP1、SP5、SP6）

- **SP1 稳定控制面**：`CommitMessageGeneration.control()` 只生成一次控制面对象，App 用 `useMemo` 取一次；生成状态快照（`GenerationView`）作为单独 prop 传入。界面重新渲染不再换引用，`cancel()` 只由 worktree 切换、卸载与显式意图触发。
- **SP5 点击时捕获完整意图**：`captureIntent(mode, message)` 在点击那一刻定下目标、模式与当时文字；`intentStillValid(intent, mode)` 在配置检查与两级确认回包后用目标代次 + 模式深比较（含更改列表勾选）复核，任何变化都让这次点击失效，不再用旧文字判定新输入、也不把旧点击发到新 worktree。取消确认会清掉暂存意图。
- **SP6 统一错误渲染**：`displayedError = generationError ?? aiError` 既用于判断也用于渲染，生成失败仍显示翻译后的文案与图标。
- 回归：`tools/test-commit-generation.mjs` 新增“控制面引用稳定：重新渲染与取视图不取消在途请求”“点击时捕获完整意图：等待与确认期间的目标、文字与模式变化都使点击失效”。
