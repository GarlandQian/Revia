# 14：整包审查与固定 Release 桌面验收

Status: needs-info
Blocked by: 01, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13

## What to build

Codex 核对完整 M7，集中修复审查发现，固定 Release 后完成约定真实桌面与性能验收。

依据：全规格；Q77、Q113、Q137、Q174、Q199。

## Acceptance criteria

- [ ] Pi 提交每项真实检查、源码 SHA、argv / exit / 原输出、失败原件，report 不把未测项标通过
- [ ] 首轮全范围独立两轴审查；唯一 Pi 正式返修后未达标由 Codex 直接接管；后续只审增量，Q174 非阻断项入 backlog
- [ ] 双语 / x18n、真实 Git / PTY / HTTP / 更新相关检查通过，固定候选后不再修改产品源码沿用旧结果
- [ ] Codex 操作固定 Release：四主题 / 双语 900px、编辑草稿、终端、克隆、PR、保护、诊断、更新和 D06；Windows 实机与 CI 无证据不冒称通过
- [ ] 更改列表实测与实现未完成、账号 / 更新正式接线或真实桌面仍缺时整包保持未完成；按原授权验收后才合并 / 清 worktree

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

- Codex，2026-10-30：等待任务 13 事实解阻及完整实施；此状态不阻止其他无依赖任务。实际 App 注册 / Secrets / 发布和桌面能力仍需后续真实证据。

### Codex / review01 Standards — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/standards.md；完整证据附录保存在本轮证据目录。

7. **[P2] 错误失去结构**：`src-tauri/src/m7/edit.rs:428` 将 AppError 字符串塞入参数；`src-tauri/src/m7/github/pr.rs:950` 返回中文 `error.message`。英文界面仍显示中文，违反 `docs/adr/0013-localize-structured-user-errors-at-presentation.md:3,7`。

## H7 中文已拼接错误绕过 ADR-0013（P2）

规则：`ADR-0013:3,7`：“Rust 返回稳定错误类型和结构化参数……详细原因也必须是可识别类型与参数，不能把原来的整段中文错误塞进参数规避迁移”。

两处同类违反：

- edit.rs:417 的闭包 Result 为项目 AppError；IO `?` 经 error.rs:240 的 From<io::Error> 生成带中文 `IO 错误：…` 的结构化错误。423–428 将其 to_string 塞进新错误的 error 参数，丢弃 cause 结构。即使前端外层按英文翻译，内层仍为中文。
- pr.rs:928–952 把 GitHub API AppError 转为 PrMergeOutcome.message=error.message；error.rs:150 明确此 message 是中文诊断模板结果。GithubPanel.tsx:357–360 把它嵌入 t("github.mergeNotDone")，英文界面无法递归 formatAppError。类型分类也被缩为不透明字符串状态。

原始 Git 输出可以保留，不适用于这两处 Revia 自己生成的中文诊断句。建议保留 AppErrorPayload/cause，通过生产 formatAppError 生成当前语言。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F02 P1 | “登记并打开”“按仓库记住”：主窗对话框/设置漏接 | src/App.tsx:5316,5348,6279 |

## F02 [P1] 仓库打开后新增对话框/设置接线缺失

规格 S75：“沿既有持久化和即时切换链路应用”；S98：“成功后登记并打开真实仓库”；S103：“多账号按仓库记住”；S115要求预览导出；S122要求确认下载安装。

源码 `src/App.tsx:5306–5378` 的 `if (!activeRepo)` 分支中才挂载 CloneDialog（5316和5339重复）、DiagnosticsDialog（5348）、UpdatePanel（5349）；主仓库分支工具按钮5426/5446/5454只设open，至函数结束均无对应组件。主设置 `6279–6285` 漏传worktreeId/updateStatus/onUpdateStatus/onThemesChanged，而welcome设置5330处才传。触发：打开任意仓库，点克隆、诊断、更新；或设置导入主题、切换仓库账号。结果：工具对话框不出现；新主题未刷新全局主题目录，选择时回退内置直到重启；账号页worktreeId默认null使绑定按钮317处不渲染，多账号仓库无法绑定。不是组件存在即已接入。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Codex / review01 Standards 正式完整报告 — 2026-10-31T03:14:30.306434+08:00

正式报告原文（原相对链接以 `/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj` 为基准；保持两轴分别存档，不合并排序）：

```markdown
Standards：固定 `581d152…4657407`，152 路径；静态审查，未运行产品或宣告桌面通过。规则原文、调用链和命令原件见 [证据](standards-evidence/analysis.md)。

硬违反（11 项）：

1. **[P1] 编辑丢失**：`src/features/m7/EditableWorkspace.tsx:171` 卸载取消 700ms 草稿定时器；`src/App.tsx:5801,5854` 直接关闭/切文件。快速切换丢失最新输入，违反 `docs/ui-direction.md:92`“切文件或 worktree 保留未保存编辑”。
2. **[P2] 保存绕过关闭协调**：`src/features/m7/EditableWorkspace.tsx:194` 直接调用保存，未登记写生命周期；关闭只 flush 草稿。退出可能中断 `src-tauri/src/m7/edit.rs:418` 的截断写入，违反 `docs/technical-direction.md:64`“正常关闭或退出请求不得直接打断写操作”。
3. **[P1] 未保存编辑未阻止暂存/提交**：`src/App.tsx:5861` 的 dirty 状态仅供标题使用，现有暂存/提交入口继续操作旧磁盘内容；违反 `docs/ui-direction.md:92`“暂存或提交前处理相关未保存编辑”。
4. **[P1] 无确认改写分支**：`src-tauri/src/m7/github/pr.rs:831` 对既有 `pr/N` 执行 `branch -f`；会移除本地新增提交的分支引用，违反 `docs/product-direction.md:277`“改写历史的操作执行前确认”。
5. **[P2] PR 写门过晚**：`src-tauri/src/m7/github/pr.rs:788,837` 在 `remote_ops::checkout` 取锁前 fetch/改分支，忙碌时也先修改引用，违反 `docs/technical-direction.md:60` 同 worktree 单写契约。
6. **[P1] 诊断隐私边界失效**：`src-tauri/src/m7/diagnostics.rs:146,168,290` 导出全局日志和原始失败输出，仅替换 HOME/凭据；文件正文、其他仓库、远端/邮箱仍可泄露。违反 `docs/technical-direction.md:160` 的范围、脱敏与正文/diff 排除规则。
7. **[P2] 错误失去结构**：`src-tauri/src/m7/edit.rs:428` 将 AppError 字符串塞入参数；`src-tauri/src/m7/github/pr.rs:950` 返回中文 `error.message`。英文界面仍显示中文，违反 `docs/adr/0013-localize-structured-user-errors-at-presentation.md:3,7`。
8. **[P2] 克隆旁路统一 Git 执行器**：`src-tauri/src/m7/clone.rs:259` 自行选 PATH Git、启动进程，忽略设置路径且不记命令日志；违反 `docs/technical-direction.md:56,83`。
9. **[P2] 关闭终端直接杀进程**：`src/features/m7/TerminalPanel.tsx:271` 无确认调用 close；违反 `docs/ui-direction.md:157`“关闭运行中进程前先确认”。
10. **[P3，非阻断] 发布约定**：`.github/workflows/release.yml:118` 原名复制 dmg、未生成 SHA256 清单，违反 `docs/technical-direction.md:150,153` 的命名与校验文件约定。
11. **[P3，非阻断] 术语**：`locales/zh-CN.json:2255` 快捷键显示“hunk”，违反 `GLOSSARY.md:64`“界面一律写‘块’”；当前 x18n glossary 未覆盖此词。

判断建议（2 项，均 P3 非阻断）：

- **可能 Duplicated Code**：`src/App.tsx:5316,5339` 重复挂载相同 CloneDialog；保留一处。
- **可能 Primitive Obsession**：`src-tauri/src/m7/github/pr.rs:224` 用字符串表达有限结果状态；可改枚举并生成绑定。

本轴最严重项：无确认强制移动已有 PR 分支。其余 smell 均按仓库规则和实际影响筛过，未将启发式当作硬规范。
```

### Codex / review01 Spec 正式完整报告 — 2026-10-31T03:14:30.306434+08:00

正式报告原文（原相对链接以 `/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj` 为基准；保持两轴分别存档，不合并排序）：

```markdown
Spec：22项，P1×15、P2×7。固定 `581d152…4657407`，完整152路径；首尾HEAD一致、工作树干净。源码根 `/Volumes/T7 Shield/project/Revia-m7`。[附录](./spec-evidence/findings.md)含逐项原规格、触发、调用链、hash、argv/exit及原输出。

| 发现 | 原规格与实际缺口 | 源码精确位置 |
|---|---|---|
| F01 P1 | “覆盖窗口”：玻璃层仅200×80，SSR确认 | src/features/m7/glass/GlassLayer.tsx:28；src/components/m7/glass/GlassSurface.tsx:355 |
| F02 P1 | “登记并打开”“按仓库记住”：主窗对话框/设置漏接 | src/App.tsx:5316,5348,6279 |
| F03 P1 | “切换…保留内容”：卸载丢草稿，基线变不恢复 | src/features/m7/EditableWorkspace.tsx:130,164 |
| F04 P1 | “先处理…草稿”：暂存/提交/关闭无协调 | src/App.tsx:1638,1765,2343 |
| F05 P1 | “切换保留进程”：混worktree页签；绝对cwd恢复被拒 | src/features/m7/TerminalPanel.tsx:131；src-tauri/src/m7/terminal.rs:275,429 |
| F06 P1 | “关闭…前确认”：页签×直接kill | src/features/m7/TerminalPanel.tsx:263 |
| F07 P1 | “确认…目标”：审阅/合并丢remote，可能写错仓库 | src-tauri/src/m7/commands.rs:535,576 |
| F08 P1 | “迟到结果不得覆盖新目标”：PR响应无身份门禁 | src/features/m7/GithubPanel.tsx:141,165 |
| F09 P1 | “草稿…绑定PR与提交版本”：键不匹配、无head | src/features/m7/GithubPanel.tsx:276；src-tauri/src/m7/commands.rs:507 |
| F10 P1 | “取并集”“失败保留”：推送漏保护、失败覆盖缓存 | src-tauri/src/commands.rs:2300,2325；src-tauri/src/m7/github/pr.rs:997 |
| F11 P1 | “保护与回收”：检出先branch-f覆盖已有分支 | src-tauri/src/m7/github/pr.rs:831 |
| F12 P1 | “字段白名单”：诊断复制跨仓库原输出，泄露正文/邮箱/路径 | src-tauri/src/m7/diagnostics.rs:146,168,278 |
| F13 P1 | “分开”“安装前协调”：下载即安装、绕门禁、重启按钮不可达 | src-tauri/src/m7/update.rs:260,293；src/features/m7/UpdatePanel.tsx:145 |
| F14 P1 | “URL…产物…对应”：Release不生成/上传latest.json | .github/workflows/release.yml:118,140 |
| F15 P1 | “签名校验”：公钥原文误传要求base64的插件 | tools/write-updater-config.mjs:27,40 |
| F16 P2 | “取消”“刷新轮换”：均缺，refresh_token丢弃 | src-tauri/src/m7/github/auth.rs:60,257 |
| F17 P2 | “确认替换”“提示…同步”：冲突只拒绝，提交框仍旧键 | src/features/m7/settings.tsx:199；src/components/app/CommitComposer.tsx:247 |
| F18 P2 | “支持已有fork”：无法选来源或表达owner:branch | src/features/m7/GithubPanel.tsx:377；src-tauri/src/m7/github/pr.rs:1064 |
| F19 P2 | “可分页”：仅100文件/评论，patch静默裁400行 | src-tauri/src/m7/github/pr.rs:548,577；src/features/m7/GithubPanel.tsx:70 |
| F20 P2 | “统一视图保留只读”：统一/暂存入口打开独立全文编辑 | src/App.tsx:5794,5854 |
| F21 P2 | “7天，可清空”“相同的预览”：均未实现 | src-tauri/src/m7/diagnostics.rs:113,129,146 |
| F22 P2 | “主动开启预发布”：无渠道选择，另增无效automatic方式 | src-tauri/src/settings.rs:123；tools/check-release-version.mjs:53 |

最严重F07：可能对错误仓库审阅/合并。F01/F17仅组件/纯函数观察，其余为静态事实、尚未动态复现。13/14既有阻塞不计缺陷；桌面、账号、Actions、真实更新未验。
```

### Codex / review01 原始交付验证复核 — 2026-10-31T03:14:30.306434+08:00

```markdown
# Codex 交付证据初核（非正式两轴报告）

固定候选：46574071b591b138a31d51be4403c29ba578a0d1；规划基点581d1527651e981018f2b0830d994bca49580254；工作树干净。

- delivery-log-hashes.json 固定了现存全部原始日志的字节散列，原件继续保留。pi-delivery-original.jsonl 保存完整实施工具调用/输出与交付自述，用于回溯argv与检查顺序。
- cargo-test-final.log 的各 test result 合计767 passed（1 ignored），对应 cargo-test-final.exit=0。报告所称768不能直接作为单次整套计数。
- cargo-test-final2.log 第二轮实际exit=101：前面组共669 passed，m6_flows 132 passed/1 failed，在15秒等候断言超时后整套停止，没有执行后续组。m6-retest.log/exit只证明该单项重跑1 passed/132 filtered；不能相加为最后一次全套exit0。
- m7-it-final.log：真实Git编辑5例和真实PTY2例，全部通过；是否覆盖最终候选及全部规格由源码/argv核实。
- 最后即时tsc命令经head管道后echo $?，管道状态不能替代tsc真实退出码；空tsc.log本身同样不证明exit0。后续应对最终修复候选以结构化runner补真实exit。
- fe-build-postcommit.log和cargo-build-postcommit.log各有成功尾行。当前release binary SHA256为7599baa2acc6f9f659092d2d4c5e0cb2e8fe6c79c3611c2470f9df2054a41220，与自述一致；这不是Codex固定Release验收。
- smoke-final2.json及smoke-postcommit.json仅为实施冒烟，ok=true；不认定强折射、真实Revia桌面、Rebased参考、双语窄窗或Windows验收通过。
- 报告明确HTTPfixture e2e、完整保护同步等有缺口，任务文件却标01–12 done。两轴须独立对照原始规格，不把实施自述当完成证明。
- 正式Pi返修尚未使用。任务13/14既有真实桌面事实门槛保留；不扩张到M6 backlog。
```

父代理已逐条回查关键生产链，并核对152源码与128原日志散列，无变化。所有正式P1/P2需本次返修；任务05/07/08等原规格明确要求的真实Git/local remote、受控HTTP及外部边界验证同样不得省略。任务13/14既有桌面事实门槛不变；禁止借更改任务条款消除未实现需求。完整报告中的P3、两项smell与构造边界登记 backlog 15，不阻断本轮。

### Codex / review02 Standards — 2026-10-31

固定范围 `f9247194eeb544de688ebbb27ac3d0ddd3847902...384360c4ea7b30b04f0114a60156107c9fa6c501`，52 路径 / 3 提交。正式报告原文如下，完整静态证据在批次 review02 的 standards-evidence/analysis.md。

<details>
<summary>Standards 正式报告（原文）</summary>

Standards：仅审 `f924719…384360c` 的 52 路径、3 提交；结论均为固定源码静态证据，未运行产品或宣告桌面通过。详见[附录](standards-evidence/analysis.md)。

硬违反（9 项）：

1. **[P1，H6] 诊断仍泄露正文**：`src-tauri/src/m7/diagnostics.rs:476` 原样取输出首行后持久化/导出；短 hook 正文仍通过，外置路径和普通 remote 也未脱敏。违反 `docs/technical-direction.md:160`“排除文件正文、diff”。
2. **[P1，H1] 恢复草稿会被覆盖**：`src/features/m7/EditableWorkspace.tsx:142` 基线变化时只载入磁盘文本；“保留我的版本”无法取回旧草稿，下次输入覆盖它。违反 `docs/ui-direction.md:92`“保留两版供对比”。
3. **[P1，H3] 隐藏编辑绕过提交门禁**：`src/App.tsx:1906,2407,5894` 外层关闭直接清 editingPath；保留草稿但守卫随 path=null 放行，暂存/提交旧内容。违反 `docs/ui-direction.md:92`“暂存或提交前处理相关未保存编辑”。
4. **[P1，新增] 克隆凭据进入日志**：`src-tauri/src/m7/clone.rs:358,384` 直接记录 URL/输出；CommandLog 不负责打码。违反 `docs/milestones/m5/spec.md:170`“凭据…都不显示”。
5. **[P2，H2] 保存仍未登记关闭生命周期**：`EditableWorkspace.tsx:226` 直接 IPC，关闭仅 flush 草稿；可中断 `edit.rs:418` 截断写。违反 `docs/technical-direction.md:64`“正常关闭或退出请求不得直接打断写操作”。
6. **[P2，H7] cause 未接翻译参数**：`src-tauri/src/m7/edit.rs:426,448` 改传 cause，locale 仍用 `{error}`；formatter 丢掉实际原因。违反 ADR-0013:3,7 的结构化展示契约。
7. **[P2，H8] 克隆仍使用过期 Git 路径**：`src-tauri/src/commands.rs:153` 只启动时读取设置，重新加载环境也不更新 CloneManager。违反 `docs/technical-direction.md:83` 的手动路径约定。
8. **[P2，H9] 退出仍无终端确认**：`src/App.tsx:1655,1667` 运行态只用于更新，关闭直接执行原生动作。违反 `docs/ui-direction.md:157`“关闭运行中进程前先确认”。
9. **[P3，非阻断] 状态值越出约定**：`docs/milestones/m7/issues/01-liquid-glass.md:3` 等 12 个任务改为 ready-for-review，未在 `docs/agents/issue-tracker.md:18` 状态集合中。

判断建议（3 项，均可能 Duplicated Code，P3 非阻断）：`src/App.tsx:5387` 与 5376/5429/5430 重复挂载对话框；`diagnostics.rs:306,353` 重复命令脱敏投影；`github/pr.rs:558,1097` 重复分页循环。

H4/H5/H10/H11 原问题在本次源码中已处理。最严重项：H6 仍把不受控正文持久化并导出。

</details>

### Codex / review02 Spec — 2026-10-31

同一固定增量，正式报告原文如下，逐项附录在批次 review02 的 spec-evidence/findings.md。两轴独立，不混加或重排发现。

<details>
<summary>Spec 正式报告（原文）</summary>

Spec：18项（P1×13，P2×5）。仅审固定 `f924719…384360c` 的52路径、3提交增量。S为原规格行号，源码根 `/Volumes/T7 Shield/project/Revia-m7`；[逐项证据](spec-evidence/findings.md)。

| 发现 | 规格、触发与后果 | 生产位置 |
|---|---|---|
| F02 P1 | S98“打开”：开仓库后克隆/诊断/更新仍不挂载 | App.tsx:5365,5387,5459 |
| F03 P1 | S84“两版”：磁盘变化后恢复只装入磁盘，旧草稿不可比较/取回 | EditableWorkspace.tsx:142 |
| F04 P1 | S85“保存、放弃、取消”：退出仍直接冲刷关闭；保存未登记生命周期 | App.tsx:1667；EditableWorkspace.tsx:226 |
| F05 P1 | S91“独立”：A的在途认领/打开晚回仍加入B页签 | TerminalPanel.tsx:136,149 |
| F06 P1 | S92“退出…协调器”：页签有确认，应用退出仍不问运行进程 | App.tsx:1667,1717 |
| F08 P1 | S105“迟到…不得覆盖”：旧评论/审阅/fullDiff响应仍污染新PR | GithubPanel.tsx:313,375,697 |
| F09 P1 | S109“绑定…版本”：新增草稿后不传head重载，旧版可混发；正文切PR直接清空 | GithubPanel.tsx:346,372,212 |
| F10 P1 | S111“完整分页”：仍只查单分支；失败不标陈旧，404覆盖旧保护 | github/pr.rs:1057,1070 |
| F11 P1 | S107“保护与回收”：操作状态拒绝前仍已fetch/改本地引用 | github/pr.rs:819,890；remote_ops.rs:640 |
| F12 P1 | S116“排除…正文”：原输出首行仍导出/持久化，外部路径及远端未脱敏 | diagnostics.rs:476,528 |
| F13 P1 | S122–123“分开”“协调”：丢下载包、重下；旧回执遮新版本；重启绕协调 | update.rs:370,407；UpdatePanel.tsx:55,112,154 |
| F14 P1 | S126“签名…对应”：清单将已编码.sig再次编码，合法包验签失败 | generate-latest-json.mjs:40 |
| F15 P1 | S124“签名校验”：按发布说明输入正常公钥全文仍exit1 | write-updater-config.mjs:39 |
| F16 P2 | S105“刷新轮换”：只存refresh_token，无刷新请求或到期处理 | github/auth.rs:402；github/mod.rs:386 |
| F17 P2 | S76“确认替换”：确认后旧动作回默认，默认键冲突原样保留 | settings.tsx:235 |
| F20 P2 | S81“并排…只读”：仍独立全文编辑，打开后切统一/暂存仍可写 | App.tsx:5948 |
| F21 P2 | S115“跨重启”“相同”：仅预览才存摘要、未消费历史；导出另取快照 | m7/commands.rs:645；diagnostics.rs:230,266 |
| F22 P2 | S122“预发布”：两个清单同指latest稳定资产；保存开关未更新界面 | release.yml:158,168；App.tsx:4369 |

F01/F07/F18/F19的首轮具体缺口在源码层面已修正。最严重F09：旧版本审阅可能误发到新版本；F12仍有隐私泄漏。脚本/纯处理器证据与静态事实已区分；桌面、账号、Actions和实际更新未验，13/14既有阻塞未重复计入。

</details>

### Codex / review02 Spec / F02 — 2026-10-31

| F02 P1 | S98“打开”：开仓库后克隆/诊断/更新仍不挂载 | App.tsx:5365,5387,5459 |

<details>
<summary>本项证据原文</summary>

## F02 [P1] 仓库打开后的三个入口仍未挂载

规格 S98“成功后登记并打开真实仓库”，S115预览/导出，S122确认下载/安装。增量把新Clone/Diagnostics/Update挂在 `App.tsx:5387–5415`，但它们仍处于 `5365 if (!activeRepo)`，该return在5456结束；真实主仓库return从5459开始。全文 JSX 引用均位于welcome分支，主工具入口5507等仅setOpen。触发：正常打开任一仓库后点克隆、诊断、更新；结果仍没有对应dialog，F02主体未修。welcome原有CloneDialog:5376、Diagnostics:5429、UpdatePanel:5430也仍存在，新增条件挂载反而重复；不将重复代码单列smell。Settings主分支6383–6386所缺四props确已补齐。

</details>

### Codex / review02 Codex 原件核对与接管 — 2026-10-31

<details>
<summary>交付证据核对（原文）</summary>

# 返修交付原始证据核对（Codex）

固定候选 `384360c4ea7b30b04f0114a60156107c9fa6c501`，产品提交 `70c59ac9672d89357f2a064f4813900b98213029`；后续两提交只改 docs，交付工作树干净。

- 原 Pi session JSONL 已逐字复制；精确工具命令与原结果在 `final-checks-original-tool-records.json`。
- 末轮完整 cargo 日志合计 **772 passed / 0 failed / 1 ignored**，17 个结果组均 ok；紧随命令记录 `cargo-test=0`，不拼接其它轮次。
- tsc、lint、x18n、前端构建、release 构建、隔离冒烟各自即时 `.exit` 均记录 0。原日志路径与 SHA 在 `delivery-evidence-audit.json`。多数日志合并 stdout/stderr，不能事后伪分流。
- 末轮检查前已提交产品；后续提交只有 docs。实施未记录每个检查输入的独立源码散列，当前 HEAD/source-hashes 和原工具日志可核查，不能冒称逐轮受控源码证明。后续 Codex 最终验证须按约定独立记录。
- 当前二进制 SHA256 `1e6d3f1c83164a07c25a77591b630eddfd2e5f63492c5a8f80c8bdbda64951fb` 与报告一致；尚不是 Codex 固定 Release 或真实桌面验收通过。
- `cargo-test-repair2.log` 的超时失败与 exit101 原件保留；末轮完整测试确实成功。“负载抖动”根因尚无证明，不能当确认结论。
- x18n 69 警告可观察，全部为预存警告的说法未单独证实。lint exit0 有警告，未称零警告。
- 原报告承认缺受控 GitHub HTTP fixture、真实账号/钥匙串、真实 CI/更新以及任务13/14桌面事实。实施冒烟不替代这些验收。

这是原件核对，不是重跑通过。审查期间产品冻结，先收齐本轮两轴，再由 Codex 直接修明确遗留并集中验证。

</details>

Standards 硬违反 9 项（P1×4/P2×4/P3×1），判断建议 3 项；Spec 18 项（P1×13/P2×5），存在重叠，不能合称 27 个产品缺陷。P3/smell 记 backlog15；本轮阻断问题由 Codex 直接修，正式 Pi 返修已用尽，不再派发。任务13/14真实桌面前提保持未完成。


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


### Codex / review03 Spec 正式原文 — 2026-10-31

固定候选`4c3b3f838828935dd6bb64efc084750fb292df51`，增量基点`7741dba8725d7b02cc09197cb88d26014c0c48df`。本轴原文如下，不合并重排；详证在本轮`/private/tmp/revia-m7-batch-g5scyw0r/review-03-zika9m9t/spec-evidence.md`。本轮明确P2由Codex直接处理，不再派Pi；P3登记backlog15。

```markdown
Spec：1 项 P2（需求部分未满足／实现错误），未发现新增 scope creep。仅审固定 7741dba…4c3b3f8 的 10 提交增量；源码根为 /Volumes/T7 Shield/project/Revia-m7。

- **[P2，S03-01，静态推断] 撤销后的旧草稿会复活。** 规格 docs/milestones/m7/spec.md:83：“切换文件、视图、仓库、语言或主题保留内容”。src/lib/m7/editSession.ts:120 在文本回到磁盘原文时跳过持久草稿清理；:89–90 重新打开仅在 dirty 时保留内存，否则加载旧草稿。生产 EditableWorkspace.tsx:79、82–84 与 App.tsx:6096 确实触发此链路：编辑已存在的工作区改动，等待自动落盘，手动撤回原文，再切文件／统一视图后返回，已撤销内容重新出现，后续保存可重新写入它。应让回到基线也有序更新／清除旧草稿。

已核对其余 review02 明确修复的生产入口与控制流，未再确认其他 P1/P2。固定源码 7,875 项、原始证据 900 项散列全吻合；原检查含 Rust exit101、中断 null 与后续 exit0，不称单轮全过；相关 Node 98 通过。此项未做动态或桌面复现，本轴未重跑产品测试。13／14 既有外部阻塞不重复计数。详见 spec-evidence.md。
```


### Codex / review04 Standards 正式原文 — 2026-10-31

固定候选`628fff93cf19ce1f4e61f329857ed165e087b117`，增量基点`5a35ecd4821764da6da16de500942d657bd118cd`。本轴原文分别保留；详细证据在`/private/tmp/revia-m7-batch-g5scyw0r/review-04-lmwadog6/standards-evidence.md`。

```markdown
Standards：仅审固定 `5a35ecd…628fff93` 的14路径、1提交；新增硬违反0项，新增smell判断0项，未发现新增P1/P2。

H1：`src-tauri/tests/support/git_environment.rs:5`统一从平台PATH构建环境，四个fixture在`m7_diagnostics.rs:32`、`m7_pr_checkout.rs:80`、`m7_protection.rs:66`、`m7_review_drafts.rs:69`实际接入。原先违反`AGENTS.md:72`、`docs/adr/0007-target-platforms.md:7`的Unix固定搜索路径已移除；这是静态确认，未实跑Windows。

S03-01相关增量：`src/lib/m7/editSession.ts:89,91,123`等待在途草稿、保留撤销后的内存并清理旧记录；`src/features/m7/EditableWorkspace.tsx:82,85`接入干净正文的待清理冲刷。新增4项测试导入生产EditSession，覆盖撤销、在途、清理失败与后续输入。未确认此增量违反既定规则。

已直接核验原argv、stdout/stderr、exit及源码来源：Node102通过、0跳过；Rust四target共27通过；frontend/lint均exit0。候选相对这些检查仅6份说明文档变化。7,876项源码与920项原证据散列全部吻合。跳过工具强制检查项；既有P3仍留backlog，13/14桌面前提不重复计数。本轴仅只读审查与散列核验，未重跑产品测试、未启动桌面；本结论不代表Windows或桌面验收通过。详见standards-evidence.md。
```


### Codex / review04 Spec 正式原文 — 2026-10-31

固定候选`628fff93cf19ce1f4e61f329857ed165e087b117`，增量基点`5a35ecd4821764da6da16de500942d657bd118cd`。本轴原文分别保留；详细证据在`/private/tmp/revia-m7-batch-g5scyw0r/review-04-lmwadog6/spec-evidence.md`。

```markdown
Spec：本增量未发现新增或未解决的 P1/P2；(a) 未确认遗漏／部分需求，(b) 未发现新增 scope creep，(c) 未确认实现错误。仅审 `5a35ecd…628fff9` 的 14 路径、1 提交。

S03-01 已按规格 `docs/milestones/m7/spec.md:83`“切换文件、视图、仓库、语言或主题保留内容”修复：`src/lib/m7/editSession.ts:89–100,123–126` 等待旧写、保留撤销后的内存正文并有序清理；`src/features/m7/EditableWorkspace.tsx:82–85` 自动冲刷也覆盖待清理草稿。清理失败仍保留文本和退出保护，符合规格 :86“保留内容、原目标和已知回执”。这些是生产控制流静态核对，原 Node 回归实际覆盖撤销、在途写、清理失败和等待期间新输入。

H1 的四个 fixture 已通过 `src-tauri/tests/support/git_environment.rs:5–34` 读取平台 PATH、隔离 HOME／USERPROFILE 与 Git 配置，未删除普通 Windows 检查，符合规格 :126“保留现有 Windows 编译检查”。未进行 Windows 实跑。

已核对原件：Node 102 通过、0 skipped；四个 Rust target 共 27 通过；build／lint exit0。检查时源码到候选仅变化 6 份说明文档；7,876 项源码及 920 项原始证据散列吻合。本轴未重跑产品测试、未启动桌面，不据此宣称桌面验收通过；任务 13／14 既有外部阻塞及 P3 backlog 不重复计数。详证见 `spec-evidence.md`。
```
