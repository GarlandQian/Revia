# 07：查看 PR 并检出来源分支

Status: ready-for-agent
Blocked by: 06

## What to build

显示 PR 列表、详情、diff 与讨论，明确来源和目标，通过 Rust 在确认后检出。

依据：规格 F；Q176、Q194。

## Acceptance criteria

- [x] 分页、草稿 / 状态、权限与离线失败呈现正确；文本内容不执行脚本或不可信 HTML
- [x] 检出前明确仓库 / worktree / remote / 分支 / head，沿既有未提交改动保护、回收及写前核验
- [x] 支持已有 fork，不自动创建 fork、添加 remote 或推送；无法映射时说明缺失条件
- [x] 真实自建 Git 与受控 HTTP 一起验证 head 变化、脏工作区、切目标与失败结果

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / review01 Standards — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/standards.md；完整证据附录保存在本轮证据目录。

4. **[P1] 无确认改写分支**：`src-tauri/src/m7/github/pr.rs:831` 对既有 `pr/N` 执行 `branch -f`；会移除本地新增提交的分支引用，违反 `docs/product-direction.md:277`“改写历史的操作执行前确认”。

## H4 无确认强制移动已有 PR 分支（P1）

规则：`docs/product-direction.md` 安全边界：“改写历史的操作执行前确认；……改写已在上游的提交时额外警告”（当前固定源码第 277 行）。

`m7/github/pr.rs:831–837` 在存在任意同名本地分支且该分支未在当前 worktree 检出时，直接 `git branch -f <branch> <resolved_sha>`。`GithubPanel.tsx:321–333` 的 checkout 点击请求默认 localBranch=null、strategy=smart，没有覆盖本地分支的确认；智能保护只保护工作区，不保护被强移的已有 branch tip。

触发：已有 pr/7 包含用户未推送的额外提交，切回 main 后重新检出 GitHub PR #7。后果推断：pr/7 被静默改到远端 PR head，额外本地提交从该分支引用消失；即使后续检出失败，之前的强移已经完成。建议拒绝非快进覆盖，或在核对旧 tip、展示额外提交及发布状态后单独确认。

5. **[P2] PR 写门过晚**：`src-tauri/src/m7/github/pr.rs:788,837` 在 `remote_ops::checkout` 取锁前 fetch/改分支，忙碌时也先修改引用，违反 `docs/technical-direction.md:60` 同 worktree 单写契约。

## H5 PR 引用写入先于会话写门（P2）

规则：`docs/technical-direction.md:60`：“同 worktree 一次只执行一个 Revia 写操作；其他写入口禁用，不积压点击队列”。

PR checkout 在 pr.rs:788–791 先 fetch 写入 refs/revia/pr/N，837 行再 run_write 移动/新建本地分支。直到 846 行调用 remote_ops::checkout 才在 `remote_ops.rs:638` acquire_write_lock。`Git::run_write`（git/mod.rs:324–337）没有会话锁，其职责仅执行与识别不确定结果；m7 命令层也未预先取锁。

触发：同 worktree 的提交/其他写操作正在持锁，发起 PR checkout（GitHubPanel 不接收全局 isWriting）。后果推断：前两次引用写入先完成，之后复用的 checkout 才因忙碌拒绝；这不是“未执行”。身份 require_identity 同样直到后段，不能用后段门禁证明前段已获授权。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F08 P1 | “迟到结果不得覆盖新目标”：PR响应无身份门禁 | src/features/m7/GithubPanel.tsx:141,165 |

## F08 [P1] GitHub迟到结果可覆盖新目标

规格 S105：“切账号 / 仓库后迟到结果不得覆盖新目标；失败保留原目标与草稿”。

`GithubPanel.tsx:124–139,141–198` 无取消/代际/身份校验，迟到list/detail始终setState；切worktree时组件没有key（App6035），旧selected/detail也未清。列表item521在busy时仍可点。触发：先打开慢PR A，再开B，A最后返回；或请求中切worktree。结果：selected与detail、当前worktree/remote不对应；301/348处组合错误number、head与目标进行写入，旧通知/草稿也覆盖新目标。未用网络fixture实测；没有把时间推断描述成运行观察。

| F11 P1 | “保护与回收”：检出先branch-f覆盖已有分支 | src-tauri/src/m7/github/pr.rs:831 |

## F11 [P1] PR检出先强制改写已有本地分支

规格 S107：“沿已有未提交改动、保护与回收策略”；任务07：“沿既有未提交改动保护、回收及写前核验”。

`github/pr.rs:831–837` 对已有pr/N（或传入branch）先执行 `git branch -f`，随后846才调用既有remote_ops::checkout；真正写门禁/操作状态校验在 `src-tauri/src/remote_ops.rs:638–646`。触发：已有pr/12含独有本地提交，当前在另一分支，再检出#12；也可已有写锁/操作导致后续checkout拒绝。结果：目标分支先丢失原指向，无预览/确认/回收，即使检出失败也已经改写。未运行真实Git复现；命令及先后顺序为固定源码事实。

| F19 P2 | “可分页”：仅100文件/评论，patch静默裁400行 | src-tauri/src/m7/github/pr.rs:548,577；src/features/m7/GithubPanel.tsx:70 |

## F19 [P2] PR文件/讨论与diff被静默截断

规格 S106：“PR 列表 / 详情 / diff / 讨论可分页”。

`github/pr.rs:548–552,577–584` 仅取各列表第一页per_page=100，无Link消费/页码；`GithubPanel.tsx:70–72,610–620` 把patch裁成400行，无截断提示或完整diff入口，虽api.ts177封装prDiff但未调用。触发：>100文件/讨论的常见大PR或单文件patch>400行。结果：后续改动和讨论不可见，可能误当完整审阅。固定源码明确边界，未远端读取。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。
