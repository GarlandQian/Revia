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
