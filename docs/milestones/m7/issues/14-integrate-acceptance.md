# 14：整包审查与固定 Release 桌面验收

Status: ready-for-agent
Blocked by: 01, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12

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


### Codex 固定Release与真实验收停点 — 2026-10-31

两轴审修已通过后，以源码`be8f9d882b59546a3a1e905812f92a5222dde086`（产品`628fff93cf19ce1f4e61f329857ed165e087b117`）运行`bash tools/build-app.sh release`，`codex-fixed-release-build-01`真实exit0。产物为macOS arm64 Release二进制，固定副本`/private/tmp/revia-m7-batch-g5scyw0r/fixed-release-jwgkkh07/revia`，SHA256 `b1a4c3d7da2840d97ff3b8e8dffe14c136213f2ab915ca9ae8c77ca28ae1bb38`。前端21个文件分别散列并归档；完整元数据`/private/tmp/revia-m7-batch-g5scyw0r/fixed-release-jwgkkh07/release.json`，包含原始构建记录、前端归档散列与源码对应关系。构建后全部源码hash未变。

未启动产品、未生成正式安装包或接触发布签名。当前仍无原生桌面工具，AXIsProcessTrusted=false，原证据`review-03-zika9m9t/desktop-capability.json`。任务13仍须在独占fixture中真实观察Rebased1.1.22既有暂存、部分暂存及提交后index/工作区，再补规格并实施；不能从Git猜测或模拟IPC决定。任务14的真实四主题/双语900px、D06、编辑终端、GitHub/诊断/更新等仍未验。没有将编译或测试称作桌面验收通过。

进入awaiting-native-desktop；当前可独立完成的源码审修和Release固定已完成，13/14保留阻塞。未push/PR/tag/发布、未合main/删除worktree；formalRepairUsed=true、Pi继续idle，监控保留。源码不变时不重复检查或启动代理；取得真实桌面能力后才续接约定验收。

### Codex / review05 Standards（2026-10-31）

固定增量 b5caf5a…ffaa343；以下为独立正式报告原文，完整附录 `/private/tmp/revia-m7-batch-g5scyw0r/review-05-n9a_z26m/standards-evidence.md`。

范围：仅 `b5caf5a...ffaa343` 的 task13 增量；23 路径，单提交。源码、规则与原始证据散列已核对。

硬性规则违反：

- **[P2] 绕过只读类型限制** — `src-tauri/src/m7/changelists.rs:351–364,513–517`：符号链接被建为可选条目，准备提交直接调用 `stage_file` / `apply_index_patch`，没有既有 `ReadonlyReason` 校验；子模块也未过滤。违反 `docs/product-direction.md:59–62`“符号链接、子模块只读；只读限制优先”。提交前应复用类型限制，并向界面返回原因。
- **[P2] 重新整份传输、原生滚动 diff** — 同文件 `:478,494` 将全部原始补丁写入每项 `preview`，整体 View 跨 IPC 返回；`src/features/m7/Changelists.tsx:84` 用 `overflow-auto` 的 `<pre>` 全量渲染。折叠不减少传输。违反 `docs/technical-direction.md:112`“按可见范围分批获取”和 `docs/adr/0008-diff-view-owns-scrolling.md:8–9`；应接入既有分批 diff 读取与滚动实现。
- **[P2] 错误冻结为旧语言** — `src/features/m7/Changelists.tsx:17,29,41,45` 保存 `formatAppError(...)` 字符串；切换语言后已有错误不会重新翻译。违反 `docs/adr/0013-localize-structured-user-errors-at-presentation.md:3` 与 `docs/ui-direction.md:191` 的即时切换要求。应保存结构化错误，在渲染时格式化。

P3 / backlog：

- **文档规则** — `src/features/m7/Changelists.tsx:60,72` 使用 Unicode 星号，文件名统一默认色；不符合 `docs/ui-direction.md:12–14,64` 的 Codicons 与 Git 状态色约定。
- **判断：可能的 Duplicated Code** — `src-tauri/src/m7/changelists.rs:203–205,217–219` 两次出现 `name.trim()` / `name.is_empty() || name.len() > 256`。可共享名称校验；不是硬性违反，不扩大本轮重构。

已核对 Rust03 的 42 项通过，build/lint/x18n 原件 exit0（x18n 76 警告）。未新增或重跑检查；未验证本增量真实桌面或 Windows。合计：3 项 P2、2 项 P3，无 P1；明确余项由主 Codex 接管。


### Codex / review05 Spec（2026-10-31）

固定增量 b5caf5a…ffaa343；以下为独立正式报告原文，完整附录 `/private/tmp/revia-m7-batch-g5scyw0r/review-05-n9a_z26m/spec-evidence.md`。

Spec：仅审固定 `b5caf5a…ffaa343` 的任务13增量。6项P2、1项P3；以下均为静态结论，未动态复现。

(a) 遗漏／部分实现：

- **[P2，S05-01] 块选择粒度错误。** `docs/milestones/m7/spec.md:133`要求“移动整文件与 Git 原始块”，`GLOSSARY.md:64`明确“一个 hunk 可能含几块”。`src-tauri/src/m7/changelists.rs:482–495,359–364`却每个hunk只建一个条目并应用整hunk；相隔少量未改行的两块不能分别归组／勾选。
- **[P2，S05-02] 新列表无法打开文件diff。** `docs/milestones/m4/spec.md:51`要求“单击文件：选中并在预览页签（标题斜体）中显示 diff”。`src/features/m7/Changelists.tsx:13–15,71,84`只有路径span与原始patch预览；`src/App.tsx:5858`未接文件激活／固定入口。用户无法从新列表切换到所点文件的既有并排diff及编辑器。
- **[P2，S05-03] 多文件丢弃缺失。** `docs/README.md:155`：“两种模式都支持多选、全部暂存和多个文件一起丢弃”。`src/App.tsx:5817`在更改列表模式显式禁用丢弃；新组件无替代入口，勾选也未接入既有批量丢弃目标。

(b) 越出能力边界：

- **[P2，S05-04] 只读类型可被新路径暂存提交。** `docs/product-direction.md:59`要求符号链接／子模块“无，只读并说明限制原因”。`changelists.rs:513–545`将未跟踪符号链接生成普通可选条目，`:353–355`直接调用Git暂存，绕过`session.rs:3191–3203`的只读门禁；已跟踪子模块也未过滤。

(c) 看似实现但错误：

- **[P2，S05-05] 仅改信息的amend被拒。** `docs/milestones/m3/spec.md:38`：“amend仅修改信息等合法情况按真实Git语义处理”。`src/App.tsx:2572`和`changelists.rs:311`强制至少勾选一项，即使勾选amend也不能只改提交信息。
- **[P2，S05-06] rebase编辑停点被误封。** `docs/README.md:203`：“在明确的 rebase‘编辑’停点允许 amend 和新建提交”。`changelists.rs:299–303`却拒绝所有`operation.is_some()`，未沿用现有允许停点的门禁；无冲突merge收尾也受影响。
- **[P3，S05-07，backlog] 保存无关设置后勾选状态脱节。** `docs/README.md:155`：“勾选的内容即提交内容”。`src/App.tsx:4578`每次保存设置都清空父selection；新组件`:21–32`未因该清空重新加载，仍显示勾选，提交却被`:2572/5886`拦住，须手动刷新或重选。

原件核对：7884项源码、1113项证据散列及diff一致；末轮Rust42通过，frontend/lint/x18n均exit0。未启动Revia、未实跑Windows、未宣称桌面通过。详见`spec-evidence.md`。


### Codex / review06 Standards（2026-10-31）

固定增量5839e56…2869c81；以下独立正式报告原文，细证据 `/private/tmp/revia-m7-batch-g5scyw0r/review-06-w57zfr6a/standards-evidence.md`。

范围：仅固定 `5839e56c…2869c81` 的 review05 修复增量，9 路径；未重审旧轮次。

硬性规则违反：

- **[P2，H06-01] 预览与待提交块使用不同基线。** `src-tauri/src/m7/changelists.rs:549–555` 将未匹配已暂存签名的 HEAD→工作区块标为 `unstaged`；`src/features/m7/Changelists.tsx:85,104–107` 随即打开既有暂存区→工作区 diff。同一位置先暂存 A→B、再编辑为 C 时，勾选和提交使用 A→C，完整预览却是 B→C。违反 `docs/adr/0004-hunks-come-from-git.md:3`：“补丁就由这同一个 hunk 生成”“保证用户操作的就是看到的、将要暂存的”。应让按范围读取的预览沿用选择项的 HEAD→工作区模型及指纹；仅切换 `source` 无法表达该比较。静态结论，未动态复现。

review05 的只读门禁、整份补丁跨 IPC／原生 diff 滚动、错误随语言切换问题在此增量已修复；图标／状态色亦已接回。未发现新增的基线 smell；旧名称校验重复维持原 backlog，不重复计数。

已核对 7,884 项源码、1,173 项证据散列与固定 diff。原件：Rust01 50 通过、segments01 49 通过；最终 Rust03 本机18通过，frontend03／lint03 exit0；Node01 8通过，既有 x18n 0错误／76警告且语言源未改。Rust02 的 APFS EILSEQ 与 frontend01 的 TS2345 失败原件均保留，未算作通过。未新增或重跑产品检查，未启动桌面，未实跑 Windows 或非 macOS 原字节文件名用例。

合计：1 项 P2 硬违反；P1、P3、新 smell 均为0。明确余项交主 Codex 处理。


### Codex / review06 Spec（2026-10-31）

固定增量5839e56…2869c81；以下独立正式报告原文，细证据 `/private/tmp/revia-m7-batch-g5scyw0r/review-06-w57zfr6a/spec-evidence.md`。

Spec：固定 `5839e56…2869c81`，仅 review05 修复增量；2项P2，均为静态结论，未动态复现。

(a) 遗漏／部分实现：未发现下述接线错误以外的独立遗漏。

(b) 范围外行为：未发现 scope creep。

(c) 实现错误：

- **[P2，S06-01] 多选按隐藏的暂存区顺序计算。** `docs/milestones/m4/spec.md:52`要求“Shift 点选连续范围”“⇧↑ / ⇧↓ 扩展”；`docs/README.md:155`明确“两种模式都支持多选”。新接线 `src/App.tsx:5858`直接复用按`treeOrder(snapshot)`计算的回调（`src/hooks/useTreeSelection.ts:68–74,92–106`），但列表按更改列表顺序显示（`src/features/m7/Changelists.tsx:69–81,92`）。例如三份未暂存文件a/b/c，把a移到第二列表，可见顺序b/c/a；单击b再Shift单击a只选中a/b，漏选中间的c；箭头导航也偏离可见顺序，批量丢弃目标随之错误。

- **[P2，S06-02] 相同改动内容使未暂存块打开错误版本。** `docs/milestones/m4/spec.md:51`要求“单击文件：选中并在预览页签……中显示 diff”；`docs/README.md:214`要求“diff 工作区一侧用 ⌘S 明确保存”。`src-tauri/src/m7/changelists.rs:549–555`仅凭增删文本签名存在于已暂存集合就标记`staged`，签名不含位置（`src-tauri/src/git/segments.rs:648–675`）。同文件两处相同替换，先暂存第一处再修改第二处，会把第二处也标记已暂存；`Changelists.tsx:85,104–107`因此打开第一处的已暂存diff，实际点击的工作区变化及其编辑入口缺失（`src/App.tsx:6046`）。应按实际位置／版本关联，不能只比较改动正文。

原件核对：7,884项源码、1,173项证据散列及diff一致。末轮Rust18通过，frontend/lint exit0；其余既有检查及源码映射见`spec-evidence.md`。本修复Revia桌面、Windows和非UTF8路径用例仍未实跑；不重启既有Rebased、不复测已过检查。


### Codex / review07 Standards（2026-10-31）

固定增量5f70d9c…150b274；以下正式报告原文，细证据 `/private/tmp/revia-m7-batch-g5scyw0r/review-07-wx8l6b6k/standards-evidence.md`。

范围：仅 `5f70d9c…150b274` 的 review06 修复增量，18 路径；不重审旧轮次。

硬性规则违反：

- **[P2，H07-01] 刷新会撤掉新的预览基线。** 新 `session.rs:569–584` 保留原条目并追加 `changelist`，但 `App.tsx:874–885` 仍经仅含旧三组的 `getDisplayOrderEntries` 选择刷新目标。打开 A→C 后刷新，即回到同路径的 `unstaged` B→C；勾选补丁仍是 A→C。违反 `docs/ui-direction.md:70`：“同文件在原组仍有改动则留在原位置”，及 ADR-0004:3：“保证用户操作的就是看到的、将要暂存的”。应使刷新保留模式对应读源。
- **[P2，H07-02] 新可见顺序未接入主 diff 导航。** `App.tsx:475–488,5486` 只接树选择和计数；`focusNextFile:1643–1647` 仍遍历含隐藏旧条目的 `snapshot.entries`，`reviewNextFile:3008` 仍只遍历旧三组。可见顺序 b/a/c 时，标记 b 后跳到隐藏顺序中的 c；上一文件还可跳回暂存区读源。违反 `docs/ui-direction.md:81`：“标记后按当前文件列表顺序跳到下一个未审阅文件”。这些入口应共用模式对应的文件顺序。

判断项：**[P3，可能 Duplicated Code]** `App.tsx:480` 与 `Changelists.tsx:36,110` 重复 `find(changelist) ?? find(untracked)`；可共用读源查找。非硬违反，按 Q174 留 backlog。

核对 7,886 项源码、1,253 项证据散列及固定 diff 一致。原件 Rust134＋会话18、Node13 通过，最终 frontend04/lint03 exit0；前端01/02失败保留。locale无新增，既有 x18n 0错误/76警告。本轴仅静态核对，未重跑检查、启动桌面或实跑 Windows/非法字节路径。

合计：2 项 P2 硬违反、1 项 P3 smell；P1=0。明确余项交主 Codex；未改产品。


### Codex / review07 Spec（2026-10-31）

固定增量5f70d9c…150b274；以下正式报告原文，细证据 `/private/tmp/revia-m7-batch-g5scyw0r/review-07-wx8l6b6k/spec-evidence.md`。

Spec：固定 `5f70d9c…150b274`，仅 review06 修复增量，18 路径、1 提交；3 项 P2，均为静态结论，未动态／桌面复现。

(a) 遗漏／部分实现：以下三处消费入口未接完整，未确认独立遗漏。

(b) 范围外行为：未发现新增 scope creep。

(c) 实现错误：

- **[P2，S07-01] 刷新后恢复错误预览基线。** ADR-0004:3 要求“保证用户操作的就是看到的、将要暂存的”；`docs/ui-direction.md:70`要求“同文件在原组仍有改动则留在原位置”。本轮新增 `changelist` 源（`src-tauri/src/session.rs:572–584`），但 `src/App.tsx:874–885` 仍经 `:6835–6839` 的旧过滤器选预览，排除所有该源，随后改开同路径 unstaged／staged。HEAD=A、index=B、工作区=C时，点击列表显示A→C，任一监视或手动刷新又变B→C，勾选提交仍是A→C。应让刷新保留当前模式的预览源。

- **[P2，S07-02] 文件导航仍走隐藏顺序／版本。** `docs/ui-direction.md:81`要求“按当前文件列表顺序跳到下一个未审阅文件”；M4规格:135要求“能切到上一个、下一个文件”。新 `entryOrder` 只接树选择和位置计数（`src/App.tsx:475–486,5486`）；实际工具条入口 `:6036–6037` 仍到 `:1640–1647`，直接索引含旧源的原始快照；审阅导航 `:3008` 也仍用旧过滤器。可见b/c/a时，从c点下一文件停住，从a点上一文件可打开隐藏的unstaged:c；审阅导航也会离开HEAD→工作区源。应让这些入口复用实际可见文件顺序。

- **[P2，S07-03] 更改列表内外部diff全部被拒。** M6规格:146保留“外部 diff 与合并为可选设置”。`src/features/m7/Changelists.tsx:110`现固定选择changelist，`src/App.tsx:6042`原样传给外部diff；`src-tauri/src/external_tools.rs:228,247–257`却要求该kind存在于原始Git status，而其解析器不生成changelist。菜单仍启用，所有已跟踪行均在启动前被拒。应支持该比较源及其HEAD基线，并保留既有保护。

已核对7,886项源码、1,253项原证据散列及diff；Rust152相关通过，末轮Node13通过，frontend04／lint03 exit0；失败原件保留。未重跑产品检查、未启动桌面或Windows。详见spec-evidence.md。



### Codex / review08 Standards（2026-10-31）

固定增量`ee8f662…37c1f44`；以下独立正式报告原文，详细原证据位于`/private/tmp/revia-m7-batch-g5scyw0r/review-08-qlvudspl`。

# Standards

固定 `ee8f6623…37c1f441`，仅 review07 修复增量，9 路径、1 提交（Q174）。

**文档标准违反：0。Smell 判断项：0。**

新增读源查询集中于 `visibleChangelistSelection.ts`，刷新、工具条和审阅导航复用；外部 diff 共用 HEAD／空树基线，保留身份、确认上下文、指纹及回收保护。未发现新增前端 Git 拼参、独立 hunk 计算、用户 Git 配置修改或未结构化报错。12 项固定 smell baseline 已逐项核对；工具强制规则跳过。

7,886 项源码、1,288 项原件哈希及 Git diff 全部一致。原始记录 Rust 156、Node 16 通过；最终 frontend02／lint02 exit0，x18n01 为 0 错误／76 警告。Rust／Node 后 `App.tsx` 有变化，后续前端检查与最终产品源码匹配；详见 `standards-evidence.md`。本轴未重跑检查、启动桌面或修改产品；真实桌面、Windows 与非 macOS 非法字节路径未验证。



### Codex / review08 Spec（2026-10-31）

固定增量`ee8f662…37c1f44`；以下独立正式报告原文，详细原证据位于`/private/tmp/revia-m7-batch-g5scyw0r/review-08-qlvudspl`。

Spec：固定 `ee8f662…37c1f44`，仅 review07 修复增量，9 路径、1 提交。**0 项 P1/P2；1 项 P3，仅 backlog，不阻断。**

(a) 规格遗漏／部分实现：未发现本增量新增的独立遗漏。S07-01 的初始／刷新／重开／按路径定位均转向当前模式源；S07-02 的工具条、F7 过尾与审阅导航共用可见顺序，异步审阅增加模式／列表 revision 门禁；S07-03 支持更改列表外部 diff 的 HEAD→工作区比较，原身份、确认、指纹与回收门禁保留。

(b) 范围外行为：0。

(c) 实现边界：

- **[P3，S08-B01，仅 backlog，静态] 空预览的冲突路径可能打不开内置合并。** M6 规格 `docs/milestones/m6/spec.md:146` 要求“保留内置合并”。新增 `src/App.tsx:6472` 将冲突对话框也限定为 `changelistEntryForPath`；该函数只接受 changelist／untracked（`src/lib/m7/visibleChangelistSelection.ts:41–42`）。当 index 仍 unmerged、工作区已恢复为 HEAD 内容时，HEAD→工作区补丁为空，`src-tauri/src/session.rs:572–584` 不生成 changelist 条目，已有 unmerged 条目又被排除；点“合并”只关闭对话框。未动态复现，按 Q174 作为边界 backlog，不要求本轮扩修。

已复核 7,886 项源码、1,288 项原证据散列和固定 diff；原记录 Rust156、Node16、frontend02／lint02 exit0，x18n 0错误／76警告。Rust／Node受测生产源未再变；最终 App 异步门禁已纳入 frontend02。未重跑检查、未启动桌面；Windows与本候选原生行为仍待验收。详见 `spec-evidence.md`。



### Codex / review09 Standards（2026-10-31）

固定增量`b5ddf9f…0dc11c0`；独立正式报告原文，详情原件位于`/private/tmp/revia-m7-batch-g5scyw0r/review-09-udxx1qjo`。

Standards：固定 `b5ddf9f…0dc11c0`，仅真实桌面归组修复增量，4 路径、1 提交。**0 项硬违反；0 项新增判断。**

已按 AGENTS、CLAUDE、GLOSSARY、local-validation、issue-tracker、决策表、产品/UI/技术方向及相关 ADR 核对。`changelists.rs:403–415` 只记录本次所选路径的未提交块；`:438–453` 唯一匹配后只迁移原列表归属。关联键使用路径、工作区位置和 Git 原始改动字节（`:570–589`），条目 ID、完整文件指纹与写前复核保留，新的条目不继承旧勾选（`:473–489、529、590–601`）。未发现突破 ADR-0004/0005/0009 或 Q197。

固定 12 项 smell 全部检查，均按启发式判断；无新增可报告项。跳过工具强制规则，未扩大旧范围或要求边界返修。

已核固定 diff、28 项相关源码/规范散列、174 项原证据散列及本轮受测 Rust 来源。`task13-native-group-rust-01` 原始 exit0：更改列表24项、共享提交12项通过。未重跑测试或启动桌面；旧原生证据只说明触发缺陷，新 Release 的真实桌面复验仍待完成。详见 `standards-evidence.md`。



### Codex / review09 Spec（2026-10-31）

固定增量`b5ddf9f…0dc11c0`；独立正式报告原文，详情原件位于`/private/tmp/revia-m7-batch-g5scyw0r/review-09-udxx1qjo`。

Spec：固定 `b5ddf9f…0dc11c0`，仅真实桌面归组修复增量，4路径、1提交。0项P1/P2，1项P3构造边界。

(a) 遗漏／部分实现：未发现独立遗漏。原组迁移限定本次所选路径的未提交块，不继承勾选；完整写前指纹与失败保护保留。

(b) 范围外行为：未发现 scope creep。

(c) **[P3，S09-B01，仅backlog] EOF换行上下文可能阻断归组关联。** 规格 `docs/milestones/m7/spec.md:133`：“提交后只移除已提交部分”。新 `src-tauri/src/m7/changelists.rs:573–588` 将切块补丁中所有反斜杠行计入关联payload；既有切块器会保留转为上下文行的无末尾换行标记（`segments.rs:591–600`）。同hunk有未提交块及被提交的EOF补换行块时，后者提交后该上下文标记消失，前者可能无法关联，留下失配原件并落默认组。应仅纳入目标块所属标记。静态推导，未动态复现；按规格`:151`／Q174记backlog，不阻断本轮。

已核对固定diff、相关源码及270项原证据散列；原记录m7_changelists24＋commit_flow12全部通过、真实exit0，受测生产源码一致。旧版原生Git结果正确，失败点是归组。本轮未重跑测试或桌面；新候选原生复验、Windows仍未验证。详见 `spec-evidence.md`。



### Codex：真实桌面场景集中修复与验证（2026-10-31）

固定 Release `5ca9178` / 产品`0dc11c0`已完成四主题及中英文900px真实查看；系统深色测试在finally中恢复原浅色。手动编辑Save前磁盘未变、Save后实际字节正确，草稿跨文件切换保留，外部变化后拒绝覆盖并保留草稿；原生退出按未解决草稿确认，先保留原件再仅丢弃fixture编辑，启动03真实exit0。本地clone实际目标`repos/native-clone-c_0_lf38/checkout`的HEAD与自建bare一致并自动登记打开。诊断原生导出的JSON不含文件正文标记与fixture邮箱。PR未登录和更新无签名公钥边界已观察，正式账号/保护/更新接线未验，D06尚未执行，不能将上述局部结果称整包通过。

原生发现并集中修复：更改列表view把元数据读取记成仓库写入，导致PTY逐字输入被拒及提交后Busy不能自动恢复；默认组未命名input冻结旧语言；设置玻璃透底文字干扰阅读。原始截图/AX/文件在`desktop-acceptance-myntirt_`，终端命令实际残缺并parse error，不能称PTY命令通过。

修复后元数据观察不再设置writing标志；真实写入开始/结束递增代际，只在同一空闲代际内发布元数据，读取不把仓库标为writing；最终发布的Mutex区仍包含身份重验证及其系统Git调用，可能短暂等待输入IPC，新版真实输入结果另行验证，跨写入结果拒绝。原写互斥与终端真实写入暂停保持。前端仅对明确Busy读取作三次有界重读，绑定原worktree/epoch且卸载取消；失败不继续显示可操作旧凭据。默认名用真实空name和即时翻译placeholder，避免翻译持久化成用户名称。统一浮层保留玻璃装饰/折射路径并增加主题tint与blur遮蔽透底，减少透明度/高对比度仍无滤镜降级。

`task14-native-scene-rust-01`真实exit0，共44通过（更改列表26、PTY3、会话3、共享提交12），含实际会话写代际与Barrier并发发布拒绝。`task14-native-scene-node-01`20通过，前端build/lint/x18n均exit0，翻译0错误/76既有unused警告。源码hash、argv、真实exit、分流原输出与失败原件保留；只改复用既有键，未改locale/绑定。下一步只审本新增量、固定新Release，再真实复验PTY/浮层/默认名与D06；当前不标任务14 done。


### Codex / review10 Standards (2026-10-31)

Standards：固定 `5ca917889efa5141442d0b73d6e48931828acf93...039fd1a3dbea7cf7e0c7bac91cd648509943dd41`，仅本轮10路径新增量，未读Spec。

硬违反0项；判断2项，均P3、仅backlog：

- S10-ST01，证据口径不一致：`docs/milestones/m7/report.md:303`及任务01:88、04:126、13:292、14:507称“持锁区不运行Git”，实际新增`session.rs:1624–1628`持有writing Mutex调用require_identity，经recovery.rs:188、160、122执行git rev-parse。按`docs/agents/local-validation.md:16`“每项结论都能对应固定源码、生产入口和实际可观察结果”，建议修正文档；没有另立“锁内禁Git”硬规范，未复现新输入丢失或输入延迟，不能据此升格P2。
- S10-ST02，possible Duplicated Code：`src/index.css:1848–1850`、1857–1859、1866–1868重复同一tint与两条blur声明，可合并选择器共用材质规则；无功能失败证据。

其余11项固定smell已逐项判断，无新增发现；仓库规则优先，工具强制项跳过。

已核固定diff、33项源码/规则快照、106项相关原件散列。原记录Rust44、node20通过，build/lint/x18n exit0，翻译0错误/76警告。Rust受测源码一致（CSS之后另改，最终前端快照一致）。未重跑、未启动桌面；新Release原生PTY/Busy/浮层/默认名、D06、Windows仍待规划方验证。两项P3按Q174不阻断验收。



### Codex / review10 Spec (2026-10-31)

Spec：固定 `5ca9178…039fd1a`，仅原生场景修复增量，10路径、1提交（Q174）。**新增发现0：P1/P2=0，P3=0。**

(a) 遗漏／部分实现：未发现。列表观察不再占用仓库写标志，真实写入前后代际校验阻止跨写发布；真实PTY输入仍按所属worktree的写门暂停。Busy读取最多重试三次，原epoch与卸载门禁保留，终态失败清掉列表凭据。

(b) 范围外行为：未发现scope creep。

(c) 实现错误：未发现。默认名称以空name保存，翻译只作即时placeholder；浮层增加主题tint／blur，减少透明度与高对比度仍移除滤镜。对应规格 `spec.md:68,90–93,133,138`、`ui-direction.md:191`；生产接线见 `changelists.rs:177–204`、`session.rs:1617–1653`、`Changelists.tsx:58–80,113`、`index.css:1846–1868,2010–2015,2035–2051`。

已核38项相关源码／规范、106项本轮原件及固定diff；Rust44、Node20、build／lint／x18n真实exit0，翻译0错误／76既有警告，受测Rust与最终前端源码对应。未重跑检查、未改产品或启动桌面；新Release原生复验、D06及Windows仍未验证。详见 `spec-evidence.md`。



### Codex：终端视图生命周期修复与原生停点 — 2026-10-31

固定 Release source `78c5658e4b1b370266e2044992e5de3787cd86ac` / 产品 `039fd1a` 的原生复验仍发现95字符输入丢字；原件 `final-pty-running-01` 保留，Python因路径残缺未启动。另一次聚焦提交说明后文本进入终端，不能将其当输入框通过。源码每次render产生新的ref回调，React detach时dispose xterm，随后重新读取snapshot并focus。

仅修复该生命周期：每个页签使用稳定ref，字号/字体通过现有更新路径同步，不重建视图；只有实际激活页签才请求焦点，布局回调检查实例仍存活。PTY后端、写保护、Git与locale均未改。`terminal-viewport-node-01`现有终端所有权/关闭12项通过；`terminal-viewport-frontend-01`、`terminal-viewport-lint-01`真实exit0。未新增以源码形状为断言的测试，相关UI仍须新固定Release真实复验；不把现有Node检查当作React焦点验证。全部argv/源码hash/分流输出保留。

完整第二次输入曾成功运行独占Python进程，`pty-final-process-proof.json`记录PID14626/parent12952和独占HOME/cwd；首轮连续输入缺陷仍成立。原生页签关闭提供Terminate & close/Cancel，取消后CmdQ显示全局End processes and continue；只终止本批shell，`task14-native-launch-04`真实exit0。下一步仅此增量全新两轴，再构建固定Release、复验持续输入/其他控件焦点和D06；整包14仍未完成。


### Codex / review11 正式两轴原文 — 2026-10-31

固定增量 `78c5658e4b1b370266e2044992e5de3787cd86ac…b0dcafe2b5321f6f480cefea4895bc72582787e8`。两轴已结束，以下分别原文追加。Standards 0硬违反/0判断；Spec 1项P2（S11-SP01）。Codex直接核对新字体fit后缺PTY resize属实，待修后重新固定增量；原生连续输入/焦点与D06尚未通过。

Standards

Standards：0 项新增硬违反，0 项需报告的 smell 判断。

固定范围 `78c5658…b0dcafe`，2 个提交、4 个文件，仅审增量；工作区与固定 head 一致。未发现 `CODING_STANDARDS` / `CONTRIBUTING` 文件，`CLAUDE.md` 指向 `AGENTS.md`。

`TerminalPanel.tsx:27–35,80–81,118–120,150–177,310–316`：每页签组件使 ref 回调稳定，创建实例读取最新字体参数，已有 effect 同步字体；布局回调核对实例，焦点 effect 随激活页签变化并取消待执行帧。未新增平台假设、Git/shell 拼接、文案或持久化，符合 `AGENTS.md`“不写死 macOS 假设／Git 调用只放 Rust 层”及技术方向的现有分层。

3 份文档新增段落保留失败原件、真实退出码和后续 Release 复验停点，没有把任务标为完成，符合 `docs/agents/local-validation.md` 第 3 条及“完成条件”、`docs/agents/issue-tracker.md` 的 Comments 约定。核实 Node 12 项通过、build/lint exit 0，受测关键源码散列对应固定 head；Node 只覆盖会话归属与关闭，不能证明 React 焦点。

已逐项判断全部 12 种 Fowler 基线，未形成新增发现；`TerminalViewport` 承担必要的 ref 生命周期，不按 Middle Man 报告。工具可管的格式与 lint 项跳过，未扩审旧代码或追加 P3 重构。

本次未重跑检查、运行应用或联系 Pi。新 Release 持续输入与其他控件焦点、D06、Windows 未由本轴验证；按技术方向“完整交互仍需在实际 Tauri WebView 中验证”，桌面复验仍须完成。核查明细见 `standards-evidence.md`。

Spec

Spec：固定 `78c5658e4b1b370266e2044992e5de3787cd86ac…b0dcafe2b5321f6f480cefea4895bc72582787e8`，仅 Q174 终端视图生命周期增量。**新增发现 1：P1=0，P2=1，P3=0。**

**[P2，S11-SP01] 字体变化保留视图，却没有同步 PTY 新尺寸。** 规格 `docs/milestones/m7/spec.md:90` 要求“前端发送用户的输入字节与尺寸”；`docs/ui-direction.md:12` 明确“代码字体与字号可配置”。新 `src/features/m7/TerminalPanel.tsx:160` 移除字体依赖，稳定 ref（`:33`）使字体变化不再重建终端；但保留的字体更新路径 `:180–192` 只更新选项并 `fit.fit()`，没有 `registry.resize`。本地 `@xterm/addon-fit` 0.10.0 的 `fit()` 仅调用前端 `terminal.resize`（`node_modules/@xterm/addon-fit/src/FitAddon.ts:34–46`）。生产 PTY 尺寸只经显式 IPC 更新（`terminalSessions.ts:159–162`、`src-tauri/src/m7/terminal.rs:358–380`）；容器固定时，字体变化不会触发观察容器尺寸的 `TerminalPanel.tsx:129–139`。以上为静态事实；影响推断：调字号/字体后 shell 仍按旧行列换行，全屏程序可能错位，直到另一次容器 resize。应在字体更新完成 fit 后同步所属 session 的真实 rows/cols，保留稳定实例。

未发现其他本增量缺漏、范围扩张或错误实现。新 Release 的连续输入、其他控件焦点与 D06 仍待真实桌面复验；旧版第二次完整输入成功及 Node 12 项不能证明本次 UI 修复通过。核查范围与限制见 `spec-evidence.md`。


### Codex：S11-SP01 尺寸同步修复 — 2026-10-31

保留稳定终端实例，统一监听 xterm 实际 onResize 事件，将 rows/cols 发送给同一 session 的 registry.resize；容器、字体/字号、页签激活的 fit 都走同一链路，移除原两处显式重复发送。错误继续归原会话，不取消真实写保护。`terminal-size-frontend-01` 与 `terminal-size-lint-01` 均真实exit0，原始源码hash/argv/stdout/stderr保留。未改Rust、locale或会话业务层，不重复不受影响的旧回归；字体后真实PTY尺寸仍待新固定Release原生验证。下一步仅此修复增量两轴，完成后重建Release并继续任务13/14。


### Codex / review12 两轴正式报告 — 2026-10-31

固定增量 `38d1bb883e6a0db6aa066a474490f7a50396151f…8eccde51e3cba87522fca4aa43324c185211435c`。两轴已结束；Standards 0硬违反/0判断，Spec 0项P1/P2、1项P3构造边界（S12-SP01）。以下分别原文追加。Codex核对源码与原始散列，P3仅登记backlog15，不继续扩产品；下一步固定新Release和真实原生复验。

Standards

# Standards

固定 `38d1bb883e6a0db6aa066a474490f7a50396151f...8eccde51e3cba87522fca4aa43324c185211435c`，仅审 S11-SP01 尺寸同步修复增量（Q174）。

**0 项硬违反，0 项 possible 代码异味，0 项新增 backlog 判断。**

`src/features/m7/TerminalPanel.tsx:129–131` 的 `onResize` 回调捕获所属 `sessionId`，通过现有带类型的 `registry.resize` 回传真实行列；错误仍由 registry 归属原 worktree（`src/lib/m7/terminalSessions.ts:159–162`）。符合 `AGENTS.md:73` 的前端调用边界及 ADR-0013 的结构化错误规则。移除容器与初始化两处显式回传，集中同类变化，没有新增无需求抽象。十二项 Fowler 基线仅作判断，未见本增量达到报告门槛的异味。

三份中文交接文档保留真实 Release 待验状态，未把构建或旧 Node 12 项当成本次 UI 通过证明，符合 `docs/agents/local-validation.md:9–10`。工具已覆盖的类型、lint 项不重复报告。

独立核对 diff、7,886 项源码及 1,723 项原始证据哈希全部匹配；已有 `terminal-size-frontend-01` / `terminal-size-lint-01` 均真实 exit 0，检查时产品源码与固定提交一致。本轮未运行应用、构建或测试；字体调整后的真实 PTY、焦点及 D06 仍待新固定 Release 原生验收。

Spec

Spec：固定 `38d1bb883e6a0db6aa066a474490f7a50396151f…8eccde51e3cba87522fca4aa43324c185211435c`，仅 Q174 的 S11-SP01 尺寸修复。**新增阻断发现 0；P3 构造边界 1，仅 backlog。**

(a) 缺漏 / partial：未见新增阻断项。规格 `spec.md:90`“前端发送用户的输入字节与尺寸”、`ui-direction.md:12`“代码字体与字号可配置”。事实：`TerminalPanel.tsx:129–131` 统一监听实际 `onResize`；字体 `:179–191`、容器 `:132–139`、激活 `:163–176` 的 fit 通过同一回调发送原 session 的行列。`terminalSessions.ts:159–162` 保留原归属及结构化错误。推断：S11-SP01 的字体尺寸漏发路径已补齐。

(b) scope creep：未见。7 行产品增量仅统一尺寸发送，3 份文档只记证据与待验收。

(c) **[S12-SP01，P3，仅 backlog] 重挂载的首 fit 恰好为 80×24 时可能漏对齐。** 仍对照规格 `:90`“前端发送…尺寸”。事实：移除初始化显式发送（新 `TerminalPanel.tsx:150–156`）；新 xterm 默认 80×24（`OptionsService.ts:13–14`），`FitAddon.ts:44–46` 只在尺寸变化时 resize。推断：若已存活 PTY 保留其他行列，关闭面板再重开（`App.tsx:6316–6328`），且新布局正好 80×24，就没有事件把后台改回该尺寸。仅构造边界，登记 backlog；不要求本轮返修。

已核对 build/lint 原件 exit 0 及产品散列；未重跑检查或启动应用。新 Release 的持续输入、其他控件焦点、字体后真实 PTY 尺寸及 D06 仍待桌面复验；旧 Node / 旧桌面不能证明本修复通过。详见 `spec-evidence.md`。


### Codex：最终Release原生验收与D06失败 — 2026-10-31

任务13已按真实事实与最终新fixture验收done。最终产品8eccde51、Release源码dfcdfe56，binary SHA256 3a61084ad2538562d4bc840913454013c3ae70fcea38de6c2e98946c40869fcc。终端连续163字符输入、输出时70字符提交说明焦点保持、字号16→20的真实PTY 92×9→74×7及全局两worktree关闭确认已核实；launch05真实exit0。

D06三轮可见性有效、exit0，但首屏并排252/220ms、统一176ms均超过150ms；不能以测量JSON的ok=true冒称门槛通过。统一首跳命中缓存，并排未记录缓存来源，未缓存100ms门槛仍缺证明；自动滚动60帧缺行0和截图抽样另记，不替代整体通过。诊断采样只用于定位，尚不能确认具体瓶颈，不据猜测修改产品。用户明确“尚未准备，保留这两项未验”，GitHub和更新正式接线分别保留未验；不再索取真实凭据。详见[原生验收最新记录](../native-acceptance.md)。Status为ready-for-agent，表示Codex继续D06定位，未通过整包，不合并或删除worktree/监控。
