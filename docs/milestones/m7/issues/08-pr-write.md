# 08：创建 PR、提交审阅与确认合并

Status: ready-for-agent
Blocked by: 07

## What to build

用户明确提交后创建草稿 / 正式 PR、发评论与审阅，并在确认目标和 head 后合并。

依据：规格 F；Q176、Q194。

## Acceptance criteria

- [x] 多个远端明确选择来源 / 目标；所有远端写动作由用户明确提交，不能预览即发送
- [x] 审阅草稿本机持久化，绑定 PR 版本；离线、权限不足和版本变化保留草稿
- [x] 合并带确认 head SHA，409 或写后不确定先核对，不换新 SHA 重试
- [x] HTTP fixture 验证请求目标、次数、权限、版本变化与真实返回回执；前端展示原目标和已知结果

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F07 P1 | “确认…目标”：审阅/合并丢remote，可能写错仓库 | src-tauri/src/m7/commands.rs:535,576 |

## F07 [P1] 审阅与合并忽略所选远端

规格 S108：“明确来源仓库 / 分支和目标仓库 / 分支”；S110：“合并前确认方式、目标与 head SHA”。

`src/features/m7/GithubPanel.tsx:301–308,348–356` 审阅/合并payload不含remote；`src-tauri/src/m7/commands.rs:535,576` 使用 `PrContext::new(..., None)`，`github/pr.rs:312–315` 优先origin。列表详情175处却携带用户所选remote。触发：有origin与upstream，选upstream的PR后提交审阅或合并。结果：请求发origin的同号PR；SHA不同虽会拒绝，但fork常有相同head，足以实际写错仓库。HTTP写入未执行；路由错误是静态事实。

| F09 P1 | “草稿…绑定PR与提交版本”：键不匹配、无head | src/features/m7/GithubPanel.tsx:276；src-tauri/src/m7/commands.rs:507 |

## F09 [P1] 审阅草稿无法取回且不绑定版本

规格 S109：“PR 审阅草稿保存在本机并绑定 PR 与提交版本”。

`GithubPanel.tsx:274–286` 保存repoKey为 `login/headRef`；`m7/commands.rs:507–508` 列出用worktree规范路径SHA（github/mod.rs:285–287），键永不匹配。触发：添加行内草稿。结果：保存后立即刷新列表为空，重开仍找不到；`github/store.rs:20–30` 完全没有head/base版本字段。reviewBody/commentBody仅组件state，切PR共享且关闭丢失；即使修key，旧草稿仍可用刷新后的head提交。尚未远端发布。

| F18 P2 | “支持已有fork”：无法选来源或表达owner:branch | src/features/m7/GithubPanel.tsx:377；src-tauri/src/m7/github/pr.rs:1064 |

## F18 [P2] 创建PR不能选择已有fork来源

规格 S108：“支持已有 fork，明确来源仓库 / 分支和目标仓库 / 分支”。

`GithubPanel.tsx:377–384,740–767` 创建表单仅base/title/body/draft，head强制currentBranch，单一remote来自列表；`github/pr.rs:744–745,1054–1065` 对head使用分支名校验并拒绝冒号。触发：本地origin=自己的fork、upstream=目标项目，为origin feature向upstream main建PR。结果：不能表达 `owner:feature`，也不能独立选来源/目标，不支持已确认fork路径。未远端创建PR。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Pi 正式返修处理 — 2026-10-31

- **F07**：ReviewSubmitRequest / MergeRequest 增加 remote 字段，命令层用所选 remote 构造 PrContext；缺 remote 拒绝（github.remoteRequired），不再回退 origin。前端审阅/合并都带列表/详情所选 remote。
- **F09**：ReviewDraft 增加 head_sha（ts-rs 绑定重新生成）；草稿列表按后端 repoKey（新增 m7_github_repo_key 命令）+ head 过滤，保存绑定 head；前端不再伪造 repoKey（原 `${login}/${headRef}` 键永不匹配的缺陷消除）；reviewBody/commentBody 切 PR 清空。
- **H7**：PrMergeOutcome 增加 `error: Option<AppErrorPayload>`；API 错误不再把中文 message 塞进 outcome，前端按结构化载荷走生产 formatAppError。
- **F18**：创建 PR 支持 `owner:branch` head（validate_pr_head 新增测试）；创建表单可选来源远端（fork 场景 owner 前缀自动拼接）。

### Codex / review02 Spec / F09 — 2026-10-31

| F09 P1 | S109“绑定…版本”：新增草稿后不传head重载，旧版可混发；正文切PR直接清空 | GithubPanel.tsx:346,372,212 |

<details>
<summary>本项证据原文</summary>

## F09 [P1] 草稿重载/提交仍能混入旧版本，正文切换直接丢失

S109“草稿…绑定PR与提交版本；发布前重新核对版本…head变化时保留…不自动改用新版本发送”。后端repoKey修正且ReviewDraft增加head。打开详情请求202带summary.headSha并在221用于草稿过滤，但添加后 `346 reviewDrafts({worktreeId,remote,number})`省略head，后端 `pr.rs:654–659 None=>all`立即把旧head草稿重新混进列表；提交:372把所有drafts仅映射path/line/body，丢弃head，并只用当前detail.headSha。后端681–729只核当前PR与request.expected_head_sha，无法识别旧草稿版本。触发：PR在H1有草稿，更新到H2后新增一条草稿，旧H1草稿随重载一起按H2发布。成功还 `pr.rs:742–746 clear(repo_key,number)`清掉所有head草稿。repo_key仍只有本地worktree路径，未包含账号/remote/真实owner/repo，两个remote同号PR会共用草稿。新 `openDetail:212–213 setReviewBody/setCommentBody("")`不是“按PR保留”，是切PR即丢未提交正文。上述具体错误不依赖真实App配置。

</details>

### Codex 直接修复 F08 — 2026-10-31

PR面板的列表、详情、评论提交/重载、审阅回执、完整diff、草稿保存/删除、合并、检出、创建及授权回包均按原目标与请求代际落状态。目标包含worktree、账号、remote及其仓库URL；详情另含PR。切换后旧成功、错误与finally均不能污染新目标或清掉新操作busy；刷新同一PR也使旧显示失效。账号设置完成时发出本地变更通知，面板重新核对；卸载失效全部旧请求。工作区检出仍由原请求进入既有全局写门，迟到的显示不会刷新当前另一worktree。

`github-requests-node-01` 8项生产GithubRequests/githubResponse竞态回归通过，验证账号/worktree/remote/URL/PR切换、A→B→A、同类重复请求、独立通道、错误与卸载；不是桌面验收。`github-requests-frontend-build-01`与`github-requests-lint-01` exit0，原hash/argv/真实exit/stdout/stderr均在批次codex-checks。该组没有新增locale键。

F09持久正文、账号/remote/PR/head草稿身份与发布前版本复核仍独立待修，不能把本组通过当作F09完成。F13/F22和任务13/14真实桌面也尚未完成。

### Codex 直接修复 F09 — 2026-10-31

本地草稿由Rust派生真实worktree身份、账号、remote与GitHub仓库、PR号及head组成的scope，不接受前端repoKey认领。评论正文、审阅正文/事件、行内输入表单和已加入的行内草稿都保存在本机；切PR/账号/remote或隐藏面板仍保留。重新打开按实际详情head装入；旧head稿件单列只读供取回，不混入当前发送集合，旧无身份记录保留而不自动认领。存储读改写共锁，坏文件或写失败不以空集合覆盖；已移除静默截断未发表稿件的上限。

前端生产ReviewDraftSession按scope串行保存并保留失败正文。新成功覆盖旧失败状态；发送期间新输入不被成功回包清掉。App级登记在面板隐藏后仍参加正常退出冲刷与原生退出保护，失败保留窗口。发布前先冲刷本地稿、核对账号/remote/真实仓库/head，Rust再次读取PR head；行内id必须属于同一scope和提交，评审POST显式绑定`commit_id`，不自动改用新head。成功只清理实际发送且内容/锚点/时间仍相同的稿件，旧版本及新输入保留；远端成功而本地回执/清理失败返回结构化NeedsVerification，禁止自动重发。

原件在批次`codex-checks`：最终`review-drafts-cargo-02`新10项+共享保护7项通过，使用独占真实Git/HOME/appdata与loopback假账号HTTP，覆盖旧版本混发拒绝、身份隔离、真实PR版本变化、POST commit_id、发送期间新正文、精确清理、并发写、坏存储、权限失败与远端成功后本地失败。`review-drafts-lib-01`GitHub库34项含新绑定导出通过；最后存储比较细化后`review-drafts-store-01`2项通过。`review-drafts-node-04`49项全部通过（新草稿8、目标请求8、编辑13、终端12、共享关闭写门8）；实际生产模块与协调器，不是桌面替代。`review-drafts-frontend-build-03`、`review-drafts-lint-02`、`review-drafts-x18n-01` exit0；翻译0错误/76个既有unused-key警告。

前端首轮`review-drafts-frontend-build-01`真实exit2，空表单headSha:null不符合已有可选string绑定，修正为空缺字段后重跑通过。最早Node轮缺既有Rust错误证据环境变量而跳过1项，补齐后最终49项无跳过。所有中间轮和原stdout/stderr、精确argv、源码hash、真实exit均保留；不把中间轮算作最终候选通过。测试HTTP helper两项未用函数产生非阻断dead_code提示，未清理后谎称复用旧通过。

[GitHub官方review接口](https://docs.github.com/en/rest/pulls/reviews#create-a-review-for-a-pull-request)已核对commit_id省略时默认最新提交；本组显式固定原head。Grok连接失败后用可用官方网页读取渠道，原件`impl/review-api-official.json`。这是中间检查点，F13/F22更新包/渠道/安装重启协调仍未完成；任务13/14真实桌面与真实账号/钥匙串仍未验。
