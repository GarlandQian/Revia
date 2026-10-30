# 10：预览并导出脱敏诊断

Status: ready-for-review
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
