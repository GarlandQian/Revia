# 10：预览并导出脱敏诊断

Status: ready-for-human
Blocked by: None

## What to build

帮助菜单可预览和导出当前仓库的脱敏摘要 / 错误，跨重启保留 7 天并可清空。

依据：规格 G；Q180、Q187。

## Acceptance criteria

- [x] 持久化 / 导出采用明确字段白名单；路径、邮箱、远端、凭据脱敏
- [x] 始终排除正文、diff、草稿、终端输入输出和 Token，不直接持久化原 stderr / HTTP 响应
- [x] 预览和实际导出内容一致，无遥测；7 天过期、清空和故障保持其他应用数据
- [x] 独占标记 fixture 验证所有排除项及脱敏，真实命令错误仍保留可用的结构化类别

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / review01 Standards — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/standards.md；完整证据附录保存在本轮证据目录。

6. **[P1] 诊断隐私边界失效**：`src-tauri/src/m7/diagnostics.rs:146,168,290` 导出全局日志和原始失败输出，仅替换 HOME/凭据；文件正文、其他仓库、远端/邮箱仍可泄露。违反 `docs/technical-direction.md:160` 的范围、脱敏与正文/diff 排除规则。

## H6 诊断违反范围与内容排除（P1）

规则：`docs/technical-direction.md:160`：“默认只含当前仓库相关日志与环境信息；路径、远端地址、作者邮箱等默认脱敏……排除文件正文、diff、终端输入输出……不能仅凭既有凭据打码就原样持久化或导出；错误信息的提取同样遵守这些排除规则”。

`diagnostics.rs:146` 直接取全局 CommandLog.snapshot，无 worktree_id/仓库筛选；session 仅用于 194–197 的版本/环境展示。AppState 的 command_log 是应用级共享日志。即使 request.worktree_id 给定，commands/failures/slowest 都来自所有仓库。

168 行把失败命令 record.output 前 2048 字符放入 output_excerpt，278 行把任意 stderr_excerpt 放入清单。`git/mod.rs:72–95` 明确记录合并后的 stdout/stderr，写操作 hook、Git 补丁错误等输出可以包含用户文件正文/diff。源码并没有内容白名单解析或文件正文检测。

290–295 行 redact 只调用 redact_home、redact_url_credentials、redact_tokens；它不会脱敏普通 /Volumes 等仓库路径、完整无凭据 remote 地址或裸作者邮箱。后果为静态推断：这些输入若出现在 args/失败输出，会被原样导出，甚至属于其他仓库。修复应先按仓库筛选，再仅记录允许的脱敏摘要字段，不能用通用字符串打码代替正文排除。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F12 P1 | “字段白名单”：诊断复制跨仓库原输出，泄露正文/邮箱/路径 | src-tauri/src/m7/diagnostics.rs:146,168,278 |

## F12 [P1] 诊断直接导出原输出，跨仓库泄露内容

规格 S115：“以当前仓库为默认范围”；S116：“采用明确字段白名单提取，不把原 Git 命令日志输出拷贝后打码当作合格诊断”；S117：“不把不受控的 stderr…直接持久化”。

生产链：`m7/commands.rs:607–617,620–643` → `diagnostics.rs:140–216`；146对全局CommandLog.snapshot，不按record.worktree_id过滤；168拷贝失败记录output，278拷贝stderr_excerpt，160–164/269–274拷贝program/argv。290–294仅HOME、URL凭据及已知Token前缀打码，没有正文/diff/任意绝对路径/邮箱白名单。触发：自建仓库pre-commit hook向stderr输出唯一正文、diff、非HOME路径、邮箱标记，失败后预览/导出；另一个仓库的错误也会一起进入。结果：标记进入预览，export130原子写入用户选择destination文件；HOME外路径、邮箱等保持明文。未动态复现、未读取用户日志；这是输入→原字段→持久化位置的完整静态事实。

| F21 P2 | “7天，可清空”“相同的预览”：均未实现 | src-tauri/src/m7/diagnostics.rs:113,129,146 |

## F21 [P2] 诊断缺7天持久化/清空，预览与导出不一致

规格 S115：“跨重启只保留脱敏操作摘要和错误，正常保留7天，可清空；导出前展示与实际产物相同的预览”。

`diagnostics.rs:113,129,145–146` 预览、导出各自重新render当前内存日志与当前时间；无摘要存储/到期/clear API；`src-tauri/src/git/mod.rs:55–57,88–115`全局CommandLog只在内存；`DiagnosticsDialog.tsx:101–109`也无清空入口。触发：重启后查看近7天错误，或预览后发生新命令再导出。结果：历史全失、无法清空，导出内容不等于用户已预览的快照。静态事实，未读用户历史。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Pi 正式返修处理 — 2026-10-31

- **F12**：render 按 request.worktree_id 过滤命令日志（其它仓库不进入导出）；字段白名单——program 只留文件名、args 脱敏、失败命令不再拷贝 output/stderr_excerpt，改为 error_class + 单行摘要（多行/diff 形态/超长行排除）；redact 增加 email 脱敏；scope 字段写入清单。
- **F21**：DiagnosticsSummaryStore（diagnostics-summary.json）持久化脱敏摘要 7 天（500 条上限）；预览/导出共用同一份 command_log 快照并在调用时并入摘要；新增 m7_diagnostics_summary / m7_diagnostics_clear 命令与界面“清空摘要”按钮。
- 新增单元测试：邮箱/URL/Token 脱敏、diff 形态摘要排除、按仓库过滤渲染、摘要 store 范围与清除。

### Codex / review02 Spec / F12 — 2026-10-31

| F12 P1 | S116“排除…正文”：原输出首行仍导出/持久化，外部路径及远端未脱敏 | diagnostics.rs:476,528 |

<details>
<summary>本项证据原文</summary>

## F12 [P1] “单行摘要”仍来自不受控原输出，路径/远端未脱敏

S116“明确字段白名单…始终排除文件正文、diff”；S117“宁可省略…不把不受控stderr…直接持久化”。新 `diagnostics.rs:476–491`取record.output第一条非空行，若长度≤240且非几个diff前缀就原样经通用redact保存为summary；并非从安全字段提取。真实Git hook、过滤器、外部diff工具可输出任意短单行正文，这一行会进入preview/export，及persist_summaries:251–252→store.record:168–190。`looks_like_body:485`还将+++与---用&&相连，永不命中；`+正文`/普通正文均放行。

redact:528–533只替换HOME、URL凭据、Token、邮箱；HOME以外绝对路径和无凭据remote URL完整保留。scope:297–300也仅套该redact，外置盘仓库路径仍泄漏。仓库worktree过滤确已修；没有读取真实日志，本轮未启动带hook的生产入口实验，因此这是可观察字段的静态数据流结论，未冒称动态标记实验通过。

同一新增脱敏实现另有确定Unicode边界风险：`redact_email:500–512`用chars().count()结果直接参与字节切片。包含中文的地址localpart如`张三@example.com`令`at-before`落入UTF-8中间，生成诊断可panic。此细节与F12脱敏实现一并修正，未单列扩大编号。

</details>

### Codex / review02 Spec / F21 — 2026-10-31

| F21 P2 | S115“跨重启”“相同”：仅预览才存摘要、未消费历史；导出另取快照 | m7/commands.rs:645；diagnostics.rs:230,266 |

<details>
<summary>本项证据原文</summary>

## F21 [P2] 7天历史与相同预览仍未接入生产数据流

S115“跨重启只保留脱敏操作摘要和错误…7天，可清空…与实际产物相同的预览”。新store虽能list/clear，但persist_summaries唯一生产调用是m7/commands:645的preview；没有预览过的会话结束后摘要不会持久化。前端diagnosticsSummary只有api定义，无消费者；后端preview/export均只使用当前内存CommandLog而不读summary store，重启后的历史仍不可见。clear按钮只清store，原内存日志未清，下次preview会把它重新持久化。

此外preview `diagnostics.rs:229–230`各自snapshot/render，export:266–267另snapshot/render，render:284每次now_ms；请求字段仍只有worktreeId/destination，前端DiagnosticsDialog:70未传已预览快照。触发预览后有任意新Git命令再导出：内容与预览不同；即使无命令时间字段也重建。新注释“同一份快照”只是同一个渲染函数，不是同一个用户确认过的数据快照。静态事实，未读取用户诊断历史。

</details>

### Codex / review02 Standards / R01 — 2026-10-31

<details>
<summary>本项证据原文</summary>

## H6 / R01：诊断内容与脱敏边界仍失效（P1）

规则原文：`docs/technical-direction.md:160`：“路径、远端地址、作者邮箱等默认脱敏”“排除文件正文、diff、终端输入输出”“不能仅凭既有凭据打码就原样持久化或导出；错误信息的提取同样遵守这些排除规则”。

改动点：`src-tauri/src/m7/diagnostics.rs:476–491` 的 failure_summary 从 **record.output** 取第一行非空字符串，除超过240字符、diff --git、@@ 等少数形状外原样交给 redact；并没有生成安全白名单语义。485行的 `starts_with("+++") && starts_with("---")` 不可能同时为真，+++ / --- 行也漏过。

生产链：失败 hook 输出 → `git/mod.rs:529,543` 的 command_output → 诊断 `render:329` 的 failure_summary → failures.summary；另在 `m7/commands.rs:647` 预览前调用 persist_summaries → `diagnostics.rs:251–252,174–182` 将同一字符串写入 diagnostics-summary.json。hook 输出的普通短文件正文（如一个短源码行或配置值）不匹配排除形状，会同时进入本机持久化和导出。改成第一行不等于排除了正文。

此外 `diagnostics.rs:299,313,360` 对路径/参数调用 redact，但 `528–533` 仅 HOME、URL userinfo、token、email 四步。仓库在 `/Volumes/...`、其他非HOME绝对路径、无凭据的完整 remote URL 均不被替换。`scope` 字段直接暴露外置仓库绝对路径。新的过滤在 Some(worktree) 场景确实排除了其他worktree；None分支293仍取全部，并非注释所称 detection-only。以上结论未用真实用户日志验证，例子只是输入类别，未执行网络或fixture。

建议：从固定安全字段生成分类摘要，完全排除不受控正文；统一解析、替换任意仓库位置及远端标识。原输出不能通过截短变成白名单。

</details>


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
