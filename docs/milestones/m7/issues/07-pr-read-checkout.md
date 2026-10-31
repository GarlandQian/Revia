# 07：查看 PR 并检出来源分支

Status: ready-for-human
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

### Pi 正式返修处理 — 2026-10-31

- **F11/H4**：PR checkout 先 `acquire_write_lock` + `require_identity`，再 fetch/写引用（H5 写门过晚修复）；已有同名分支时先 `merge-base --is-ancestor` 判定快进——非快进必须 `overwrite_confirmed`，前端分叉时弹确认对话框（覆盖本地分支 / 取消）；不再无条件 `branch -f`。
- **F08**：列表与详情请求带代际 + 目标身份核对，迟到响应静默丢弃；切 worktree/remote 清空选中与详情。
- **F19**：files/comments 按 Link header 翻完整页（ApiMeta.next_page）；patch 不再裁 400 行；新增“查看完整 diff”入口（GitHub diff 媒体类型，prDiff 已接线）。

### Codex / review02 Spec / F08 — 2026-10-31

| F08 P1 | S105“迟到…不得覆盖”：旧评论/审阅/fullDiff响应仍污染新PR | GithubPanel.tsx:313,375,697 |

<details>
<summary>本项证据原文</summary>

## F08 [P1] 迟到保护只覆盖部分读取

S105“切账号/仓库后迟到结果不得覆盖新目标；失败保留原目标与草稿”。详情/list代际有改善；但 `GithubPanel.tsx:313–320`旧评论请求完成后直接setCommentBody/setComments；375–383旧审阅完成直接setDrafts([])/setReviewBody("")；697完整diff `.then(setFullDiff)`不核对当前PR。PR列表按钮:617在busy时仍可选，remote选择:587也可切。触发A发评论/审阅/读全diff，未完成前选B、输入B草稿，A随后完成：A评论/diff出现在B，B草稿被清。loadStatus:136–150和backendRepoKey:133也无代际，账号未在详情代际键内；旧身份信息仍可覆盖新目标。新增保护没有覆盖整个生产请求链。

</details>

### Codex / review02 Spec / F11 — 2026-10-31

| F11 P1 | S107“保护与回收”：操作状态拒绝前仍已fetch/改本地引用 | github/pr.rs:819,890；remote_ops.rs:640 |

<details>
<summary>本项证据原文</summary>

## F11 [P1] 检出前引用仍先写，后续拒绝不回收/回滚

S107“沿已有…保护与回收策略”；任务07“检出前…沿既有…保护、回收及写前核验”。新checkout:797–798提前取锁/require_identity，873加入分叉确认，确有改善。但819先fetch写`refs/revia/pr/N`，890先branch/branch-f，903主动drop锁，再904调用remote_ops::checkout；现有操作状态拒绝在remote_ops:640才发生。触发：仓库正在merge/rebase，用户检出PR；本地PR引用/分支已创建或更新，随后才报操作中不可检出，既无失败回滚也无回收记录。分叉确认的true也仅是布尔，不绑定原tip/目标；收到新PR详情后复用确认仍有风险，此处不另报构造竞态。没有执行真实Git复现，结论来自确定命令先后和可触发的既有操作状态门禁。

</details>

## Codex 直接修复 PR 检出 F11

操作状态在任何PR fetch/引用/本地分支写入前核对，读取失败也拒绝；从取PR到既有工作区检出持续持有同一个真实worktree写锁。原检出流程新增只能持同目标WriteGuard进入的内部入口，工作区覆盖仍走既有保护/回收。PR检出同时接到App既有全局写门、关闭协调器和按原目标登记待核对结果的入口。

fetch使用每次独占临时引用，不改已有PR引用、不额外取tag或写FETCH_HEAD；后续拒绝/需确认时按实际OID核对并回退本次本地分支变更。已部分检出到新分支时保留真实HEAD与工作区，返回NeedsVerification，不强行回退造成index/正文错配。临时ref按其实际OID删除；回退/清理失败保留原错误并转待核对，不自动重发。不删除index.lock。

`pr-checkout-cargo-03` 39项通过（新PR检出8项、既有branch_remote31项），只用受控loopback HTTP、虚构token和独占真实Git/local remote/HOME/appdata。覆盖真实merge中零HTTP/零ref写、已有写锁、普通检出拒绝后的新/旧分支回退、分叉未确认拒绝、真实index.lock保留、post-checkout失败后真实HEAD保留，以及Smart Checkout沿既有策略恢复正文并可从真实stash archive取回原暂存版本。`pr-checkout-node-01` 52项共享生产写门/关闭/终端回归通过；前端build/lint/x18n均exit0，翻译0错误/76个unused-key警告。全部原stdout/stderr、argv、源码hash和exit在批次codex-checks。

首轮`pr-checkout-cargo-01`真实exit101，6通过/1失败：测试原先假定既有Q150智能检出使用stash apply --index；实际沿既有stash apply恢复正文，暂存区划分未自动恢复。未修改该既有策略；修正用例并真实读取回收档案的原暂存版，完整失败原件保留。此结果不决定任务13的Rebased既有暂存内容语义。

`--no-write-fetch-head`在[官方Git 2.29说明](https://raw.githubusercontent.com/git/git/v2.29.0/Documentation/RelNotes/2.29.0.txt)中加入，当前可选PR检出在任何网络/写之前检查此能力，复用既有双语版本错误；其它功能的全局最低版本未改。回归用独占包装器报告2.28验证门禁，不能冒称运行了真实2.28二进制。官方读取原件保存在impl/pr-checkout-git-official.json；Grok连接失败后用可用官方网页读取渠道取得，不更换实施模型。

本组仍是中间检查点；F08/F09 PR迟到响应与草稿身份、F10完整保护缓存/并集、F13/F22更新协调仍未完成。真实PR账号、原生窗口关闭和桌面检出待任务14验收。

### Codex 直接修复 F08 — 2026-10-31

PR面板的列表、详情、评论提交/重载、审阅回执、完整diff、草稿保存/删除、合并、检出、创建及授权回包均按原目标与请求代际落状态。目标包含worktree、账号、remote及其仓库URL；详情另含PR。切换后旧成功、错误与finally均不能污染新目标或清掉新操作busy；刷新同一PR也使旧显示失效。账号设置完成时发出本地变更通知，面板重新核对；卸载失效全部旧请求。工作区检出仍由原请求进入既有全局写门，迟到的显示不会刷新当前另一worktree。

`github-requests-node-01` 8项生产GithubRequests/githubResponse竞态回归通过，验证账号/worktree/remote/URL/PR切换、A→B→A、同类重复请求、独立通道、错误与卸载；不是桌面验收。`github-requests-frontend-build-01`与`github-requests-lint-01` exit0，原hash/argv/真实exit/stdout/stderr均在批次codex-checks。该组没有新增locale键。

F09持久正文、账号/remote/PR/head草稿身份与发布前版本复核仍独立待修，不能把本组通过当作F09完成。F13/F22和任务13/14真实桌面也尚未完成。


### Codex / review03 Standards 正式原文 — 2026-10-31

固定候选`4c3b3f838828935dd6bb64efc084750fb292df51`，增量基点`7741dba8725d7b02cc09197cb88d26014c0c48df`。本轴原文如下，不合并重排；详证在本轮`/private/tmp/revia-m7-batch-g5scyw0r/review-03-zika9m9t/standards-evidence.md`。本轮明确P2由Codex直接处理，不再派Pi；P3登记backlog15。

```markdown
Standards：仅审 `7741dba…4c3b3f8` 的93路径、10提交；以下均为固定源码静态结论。

硬违反（1项）：

- **[P2，H1] 新fixture阻断Windows检查**：`src-tauri/tests/m7_diagnostics.rs:29,35`、`m7_pr_checkout.rs:87`、`m7_protection.rs:72`、`m7_review_drafts.rs:75` 将Git搜索路径限定为Unix目录，普通测试没有平台条件。Windows上的Git仅位于常规PATH时，生产 `git/mod.rs:208` 经 `shell.rs:83` 找不到Git，fixture的unwrap失败，现有 `.github/workflows/compile-check.yml:116` 因而失败。违反 `AGENTS.md:72` 与 `docs/adr/0007-target-platforms.md:7` 的“不写死macOS假设”。应读取平台PATH，仅将Unix专用断言限定平台；未实际运行Windows。

判断（均P3，非阻断；依据任务说明smell baseline，非硬违反）：

- **J1，可能Duplicated Code**：`src/App.tsx:1784,1793,1807,1819` 四次复制五类退出保护条件；新增保护来源须同步四处，容易漂移，可集中判定。
- **J2，可能Primitive Obsession**：`github/store.rs:30` 的草稿kind为String，`GithubPanel.tsx:119,125` 又把审阅事件写进原表示新旧行的side；新增表单/事件时须维护字符串分支与强制转换，可用明确领域类型。
- **J3，可能Duplicated Code（仅测试）**：`m7_protection.rs:24` 与 `m7_review_drafts.rs:27` 复制Fixture及HTTP支架；隔离和服务端时序修正需多处同步，可列共享fixture维护项。

无其他新增硬违反。已跳过工具强制检查项及既有backlog；本轮只读核对源码和原始检查，未运行产品/桌面。正式结果不是桌面通过结论，P3按Q174留backlog。
```


### Codex 处理 review03 两项 P2 — 2026-10-31

S03-01：撤销回磁盘原文也排在旧草稿持久化之后清理；重新打开等待原目标在途写，并保留最新内存正文（含撤销结果），避免加载旧草稿。自动草稿冲刷覆盖有待清理记录的干净正文。清理失败保留最新文本、原件与退出保护，明确冲刷成功后解除；等待期间的新输入仍有自己的后续草稿，不被较早清理删除。

H1：四个新增测试fixture共用平台环境构造，只从当前平台PATH解析Git；HOME/USERPROFILE、Git全局配置均绑定本次独占fixture。Unix专用hook/权限断言继续按平台限定；未通过跳过全部Windows测试来消除问题。

`review03-repair-node-01` 102项通过（含新增4个生产EditSession撤销/在途/失败/新输入回归）、0 skipped；`review03-repair-rust-01` 四个受影响targets共27项通过；`review03-repair-frontend-01`与`review03-repair-lint-01` exit0。源码hash、精确argv、真实exit、分流原件在批次codex-checks。未修改locale键，之前x18n02的0错误/76保留键警告对应键集未变。未运行Windows或真实桌面，不把macOS通过称Windows实测。只修明确P2，三个P3判断继续backlog15。


### Codex / review04 Standards 正式原文 — 2026-10-31

固定候选`628fff93cf19ce1f4e61f329857ed165e087b117`，增量基点`5a35ecd4821764da6da16de500942d657bd118cd`。本轴原文分别保留；详细证据在`/private/tmp/revia-m7-batch-g5scyw0r/review-04-lmwadog6/standards-evidence.md`。

```markdown
Standards：仅审固定 `5a35ecd…628fff93` 的14路径、1提交；新增硬违反0项，新增smell判断0项，未发现新增P1/P2。

H1：`src-tauri/tests/support/git_environment.rs:5`统一从平台PATH构建环境，四个fixture在`m7_diagnostics.rs:32`、`m7_pr_checkout.rs:80`、`m7_protection.rs:66`、`m7_review_drafts.rs:69`实际接入。原先违反`AGENTS.md:72`、`docs/adr/0007-target-platforms.md:7`的Unix固定搜索路径已移除；这是静态确认，未实跑Windows。

S03-01相关增量：`src/lib/m7/editSession.ts:89,91,123`等待在途草稿、保留撤销后的内存并清理旧记录；`src/features/m7/EditableWorkspace.tsx:82,85`接入干净正文的待清理冲刷。新增4项测试导入生产EditSession，覆盖撤销、在途、清理失败与后续输入。未确认此增量违反既定规则。

已直接核验原argv、stdout/stderr、exit及源码来源：Node102通过、0跳过；Rust四target共27通过；frontend/lint均exit0。候选相对这些检查仅6份说明文档变化。7,876项源码与920项原证据散列全部吻合。跳过工具强制检查项；既有P3仍留backlog，13/14桌面前提不重复计数。本轴仅只读审查与散列核验，未重跑产品测试、未启动桌面；本结论不代表Windows或桌面验收通过。详见standards-evidence.md。
```
