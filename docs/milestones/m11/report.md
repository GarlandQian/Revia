# M11 实施与验收报告

Status: done
日期：2026-11-11
实施目录：`/Volumes/T7 Shield/project/Revia-m11`
分支：`m11`
基线：`9f9250c73429e34a326df50ba0c497985d76cf18`
开始 HEAD：`c1af259197a68c0e6041da254835459c91ada25b`
初次实现提交：`9fd386b4231e52986b26205260d38f2c9335a2c1`
最终验收源码：`8587039`（后续仅验收文档）
返修开始 HEAD：`7897dbec52b69d6be4e1b398bd14ae6e2b1b20a9`
返修提交：`562dd9515cdeb9cf4a57e63116ead2d3faf8deab`

## 结论

任务 01 与任务 02 已完成，相关自动检查、两轴审查和真实 macOS 桌面验收通过，本机应用已更新。四个 AI 字段现在经真实 `ipc.updateSettings` 转发；AI 设置页由独立状态机管理草稿、已保存基准、串行写入和请求代次；Rust 新增受控的 OpenAI 兼容模型列表命令，并把地址校验与密钥读写放在同一个设置锁内。

实施方交付时未执行合并、推送、发布和真实桌面操作；规划方后续审查与验收结果见下文。真实 AI 服务和 Windows 实测继续保留为未验。

## 实现内容

### 前端

- `src/App.tsx`：`updateSettings` 不再手工复制字段，直接把带类型的 `SettingsPatch` 交给 `ipc.updateSettings`，避免新增设置字段再次遗漏。
- `src/lib/ipc.ts`：统一 `SettingsPatch` 类型；新增 `AiModelList`；`aiSetKey` / `aiClearKey` / `aiListModels` 改为传带预期服务地址的固定类型请求。
- `src/lib/aiSettings.ts`（新增）：AI 设置状态机。区分草稿与已保存基准；所有设置和密钥写入共用串行队列；地址先于密钥提交；状态读取、模型列表和页面会话都有代次/会话标记；关闭重开后等待已排队写入再读权威状态；地址末尾斜杠按 Rust 规则规范化。
- `src/components/app/SettingsDialog.tsx`：AI 页接入状态机；保留手填模型输入；增加模型下拉、加载/空/失败/重试/刷新状态；密钥输入不进入状态机，保存回调只清空仍是本次提交值的输入。
- `locales/zh-CN.json`、`locales/en.json`、`.x18n/reviews.yml`：新增模型列表与服务地址变化相关的中英文案。

### Rust

- `src-tauri/src/ai/models.rs`（新增）：`GET {base}/models`；普通 base URL 追加 `/models`，以 `/chat/completions` 结尾时先移除该后缀；解析 `data[].id`，去重、稳定排序、过滤空白/控制字符和超长项；10 秒整体超时、512 KiB 响应上限、最多 500 个模型；不跟随重定向；错误不包含响应正文、请求头或密钥。
- `src-tauri/src/ai/commands.rs`：新增 `ai_list_models`；`ai_set_key`、`ai_clear_key`、`ai_list_models` 都在设置锁内校验界面预期地址，密钥写入/清除也在锁内执行，避免和换地址交错；返回状态和模型列表不泄露密钥。
- `src-tauri/src/ai/mod.rs`：凭据测试改为独占 RAII guard；新增 `service_changed`、`models_unavailable` 结构化错误。
- `src-tauri/src/lib.rs`：注册 `ai_list_models`。
- `src-tauri/src/settings.rs`：增加 AI 四字段持久化与重载回归。

### 文档

- `docs/README.md`：保留并提交 Q226 决策记录。
- `docs/milestones/m11/issues/01-ai-settings-models.md`：验收项勾选，状态改为 `done`。

## 初次实施验证

本节为初次实施时的检查，后续受影响检查见返修与最终验证记录。检查在实施目录执行，`stdout` 与 `stderr` 原样重定向到下列日志，真实退出码写入 `exit-codes.txt`。没有用过滤后的摘要代替原始输出。

证据根：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m11-evidence-t5lri9em/task-01`

| 检查 | 命令 | 退出码 | 原始日志 | 结果 |
| --- | --- | --- | --- | --- |
| 类型 | `pnpm exec tsc -b` | 0 | `tsc.log` | 无输出，通过 |
| Lint | `pnpm lint` | 0 | `lint.log` | 通过；仅有既有 warning，无 error |
| 前端构建 | `pnpm build` | 0 | `build.log` | 通过；仅有既有 chunk 体积 warning |
| Node M11 | `node --test tools/test-m11-ai-settings.mjs` | 0 | `node-m11.log` | 11/11 通过 |
| Node 提交状态回归 | `node --test tools/test-commit-state.mjs` | 0 | `node-commit-state.log` | 9/9 通过 |
| 翻译检查 | `pnpm i18n:check` | 0 | `i18n-check.log` | 0 error / 76 个既有 unused-key warning |
| 硬编码中文 | `pnpm i18n:extract` | 0 | `i18n-extract.log` | 78 个候选，0 个含中文 |
| Rust 编译（含 tests） | `env CARGO_TARGET_DIR=src-tauri/target /opt/homebrew/bin/cargo check --manifest-path src-tauri/Cargo.toml --tests` | 0 | `cargo-check.log` | 通过；仅有既有 warning |
| Rust AI | `env CARGO_TARGET_DIR=src-tauri/target /opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml --lib ai::` | 0 | `cargo-ai.log` | 40/40 通过 |
| Rust 设置 | `env CARGO_TARGET_DIR=src-tauri/target /opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml --lib settings::tests` | 0 | `cargo-settings.log` | 10/10 通过 |
| 空白检查 | `git diff --check` | 0 | `git-diff-check.log` | 通过 |

`exit-codes.txt` 共 11 行，全部为 0；完整命令、工作目录、工具版本和 `CARGO_TARGET_DIR` 见 `commands.txt`、`tool-versions.txt`。

## 首次失败与修正

首次失败的原件来自 Pi 会话记录。已把原始会话文件复制到证据目录，并从中提取精确的 `toolResult` 文本；提取文件不是摘要。

- 原始 Pi 会话：`/Users/garland/.pi/agent/sessions/--Volumes-T7 Shield-project-Revia-m11--/2026-11-11T06-19-04-813Z_01a1c942-8eec-7358-ba42-636ba0666a8d.jsonl`
- 归档副本：`pi-session-original.jsonl`
- 首次失败 Node（初始接线）：`first-failure-node-initial.log`。测试把 `@/lib/ipc` 解析成 `ipc` 对象本身，`measureState` 读取失败；修正测试加载器的模块返回值。
- 首次失败 Node（接线修正后）：`first-failure-node-after-ipc.log`。三个用例失败：地址/密钥顺序断言、等待期间新密钥被旧回调清空、模型下拉选项查找；修正测试 fixture 的 `flush`、密钥回调和选项定位。
- 首次失败 Rust 编译：`first-failure-cargo-check.log`。`tauri::generate_context!()` 需要 `dist/`，而当时尚未构建前端；先执行 `pnpm build` 后通过。
- 首次失败翻译检查：`first-failure-i18n-extract.log`。`src/lib/aiSettings.ts` 有写死中文；改用 `errors.ai.notConfigured` 翻译键。
- 首次失败地址末尾斜杠用例：`first-failure-key-save-trailing-slash.log`。前端草稿未在保存时规范化，密钥保存后的比较失败；统一按 Rust 规则规范化地址，并只在草稿仍是提交值时写回。
- 基线既有工具加载问题：`known-existing-test-m6-i18n-css.log`。`tools/test-m6-i18n.mjs` 在 Node 26 下无法加载 `.css` 扩展名。规划方已在未改动的 main / `9f9250c` 上复现同样 exit 1 和 `ERR_UNKNOWN_FILE_EXTENSION`（指向 `GlassSurface.css`），原始 stdout/stderr、源码与退出码见证据根 `baseline-m6-i18n.{stdout,stderr,json}`。因此明确记为基线既有问题，不修复、不重跑，也不作为本轮门禁；本轮 `i18n:check` / `i18n:extract` 均已通过。

## 首次正式返修（2026-11-11）

规划方在 `7897dbec52b69d6be4e1b398bd14ae6e2b1b20a9` 发起 Q113 允许的唯一一次正式返修，审查全文见 [review.md](review.md)。本轮按 R1–R4 与 S1–S2 修改，未另派实施者或审查者，未建 worktree，未修改 `main`。

### 修正内容

- R1 / P1：控制器与串行队列改由 App 持有。新增 `src/lib/useAiSettings.ts`，用 App 的 `useRef` 保存唯一 `AiSettings` 实例，并把 `save` 通过 ref 转发到最新 `updateSettings`；两个 `SettingsDialog` 渲染点都传入 `aiSettings`。对话框只订阅控制器，订阅在卸载时清理；密钥输入仍是对话框内的临时状态。关闭设置再打开时，旧排队写入继续由同一控制器执行，重开后的新输入不会与旧写入各自为政。
- R2 / P2：地址 A→B→A 或只增/去末尾斜杠后失焦，`saveNow` 在地址无需写回但模型列表处于 `idle` 时重新调用 `fetchModels()`，不再停在无入口的 idle。
- R3 / P2：`saveKey` / `clearKey` 在收到操作意图时立即调用 `invalidateForKeyIntent()`，同步递增状态与模型代次、清空旧列表和旧错误，不等队列前面的设置写入结束；等待期间到达的旧列表、旧状态错误都会被代次检查丢弃。
- R4 / P2：密钥保存/清除完成回调同时检查页面会话 `sessionToken()`、密钥输入代次 `aiKeyEpochRef` 和当前输入值；切页再返回、真实关闭重开或重新输入同一字符串时，旧回调不会清空新输入。
- S1 / P3：设置与清除密钥的生命周期合并为内部 `runKeyOperation` 共同流程。
- S2 / P3：删除未使用的 `sameDraft` / `aiDraftEqual`。

### 返修验证

证据根：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m11-evidence-t5lri9em/repair-01`

| 检查 | 命令 | 退出码 | 原始日志 | 结果 |
| --- | --- | --- | --- | --- |
| Node M11 | `node --test tools/test-m11-ai-settings.mjs` | 0 | `node-m11.log` | 17/17 通过 |
| Node 提交状态回归 | `node --test tools/test-commit-state.mjs` | 0 | `node-commit-state.log` | 9/9 通过 |
| 类型 | `pnpm exec tsc -b` | 0 | `tsc.log` | 无输出，通过 |
| Lint | `pnpm lint` | 0 | `lint.log` | 通过；仅有既有 warning，修改文件无命中 |
| 前端构建 | `pnpm build` | 0 | `build.log` | 通过；仅有既有 chunk 体积 warning |
| 翻译检查 | `pnpm i18n:check` | 0 | `i18n-check.log` | 0 error / 76 个既有 unused-key warning |
| 硬编码中文 | `pnpm i18n:extract` | 0 | `i18n-extract.log` | 78 个候选，0 个含中文 |
| 空白检查 | `git diff --check` | 0 | `git-diff-check.log` | 通过（空输出） |

`repair-01/exit-codes.txt` 共 8 行，全部为 0；命令、工具版本、工作目录和 `git status` 原样保存。Rust 文件在本轮未修改，按返修说明沿用任务 01 已固定的 Rust 通过结果，不重复运行。

新增回归覆盖：App 级控制器跨真实卸载/新组件实例的旧保存交错、A→B→A 与末尾斜杠后恢复模型列表、密钥意图在队列等待时立即失效旧列表/旧错误/旧状态错误、切页返回及真实关闭重开后重新输入同一密钥不被旧回调清空。测试使用原有生产组件/App 接线和受控 Promise，不依赖固定 sleep，也不以源码字符串断言代替行为。

### 返修首次失败

- 原始 Pi 会话归档：`repair-01/pi-session-original.jsonl`。
- `repair-01/first-failure-node-useAiSettings-not-defined.log`：测试切片把新加入的 `useAiSettings(updateSettings)` 也载入，VM 中没有该符号；调整切片结束标记，停在调用之前。
- `repair-01/first-failure-node-status-epoch.log`：新增的旧状态错误用例最初在 `open` 状态被拒绝前尚未取得权威地址，密钥操作被判定为未配置；改为先完成一次正常打开，再切页并延迟第二次状态读取，验证密钥意图使旧状态错误失效。

### 返修变更文件

- `src/App.tsx`
- `src/components/app/SettingsDialog.tsx`
- `src/lib/aiSettings.ts`
- `src/lib/useAiSettings.ts`（新增）
- `tools/test-m11-ai-settings.mjs`
- `docs/milestones/m11/report.md`
- `docs/milestones/m11/issues/01-ai-settings-models.md`

## 最终桌面验收与本机更新

规划方固定 `8587039`，使用已构建的前端产物重新编译 Release；`cargo build --release` 退出 0。随后在独立数据目录和本机 HTTP fixture 上完成真实 macOS 窗口验收，中英文截图均已检查。

- 服务地址填写后失焦仍保留，尾斜杠会规范化；普通地址和带 `/chat/completions` 的地址均请求正确的 `/models`。
- 返回列表去重后可选择 `revia-fixture-alpha` / `revia-fixture-beta`，选择结果写入模型字段。
- 地址、模型、生成语言和提示词在关闭/卸载设置后重新打开均保留。
- 英文界面、空列表提示、HTTP 404 与重试、手填模型保留均通过；改回原地址或仅加尾斜杠后列表入口恢复。
- 验收使用合成模型名，没有使用真实密钥或真实 AI 服务。最终本机 HTTP 请求与隔离设置见 [桌面证据](evidence/desktop-result.json)。
- `/Applications/Revia.app` 原为 0.0.2，已更新为本轮验收过的 0.0.4 本地修复构建；旧应用留在本轮证据根 `local-install-backup/Revia.app`。已核对签名和二进制 SHA256，见 [安装记录](evidence/local-install.json)。公开的 v0.0.4、Release 和资产未改动，本轮修复未推送、未发布。
- 验收进程与本机模拟 HTTP 进程已经退出。最终二进制 SHA256：`850ce2e9b40e44d087269a8400f4456d634e29f13591e1b67e3ed6a89664a7ab`。

最终增量 `493096b...8587039` 的 Standards 与 Spec 复核均未发现新增阻塞问题。原始最终检查、Rust 相关测试、Release 构建与桌面结果已归档到 [evidence/](evidence/)；完整 Pi 首次失败与过程原件仍保留在本轮外部证据根。

归档日志逐字保留工具输出，包括 Vite 的行尾空格和 Rust 的末尾空行；因此最终文档暂存检查排除 `evidence/*.log`，不改写原始日志来消除空白提示。源码检查的原始通过结果保持有效。

## 规划方最终修正与验证

规划方在唯一一次正式返修后继续处理 R3 的中间凭据窗口，见 [review.md](review.md) 的增量审查。最终由在途密钥操作计数覆盖接收意图至队列全部结束，再统一刷新模型；对话框输入完成校验合并为共同函数。

规划方验证证据：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m11-evidence-t5lri9em/planner-5d582zwv/`。19/19 Node M11、`pnpm build`（包含类型检查）、`pnpm lint`、`pnpm i18n:check`、`pnpm i18n:extract` 均退出 0；源码 HEAD 与未提交 patch 的 SHA256、精确命令、原始输出及退出码均已归档。Rust 未修改，不重复已通过的 40/40 AI 与 10/10 设置测试。

## 未验项与边界

- 真实 AI 服务：未访问；所有 HTTP 测试只使用本机 loopback fixture 和合成 key。
- Windows 真实桌面：未验证；不属于本轮。
- 验收未读取用户 AI 配置或密钥，未修改用户业务仓库、Git 配置或全局 agent/MCP 配置。
- 本轮修复未推送、未打标签、未发布 Release；本地按项目工作流压成一个提交归并。

## 变更文件

- `docs/README.md`
- `docs/milestones/m11/issues/01-ai-settings-models.md`
- `docs/milestones/m11/report.md`
- `.x18n/reviews.yml`
- `locales/en.json`
- `locales/zh-CN.json`
- `src/App.tsx`
- `src/components/app/SettingsDialog.tsx`
- `src/lib/ipc.ts`
- `src/lib/aiSettings.ts`（新增）
- `src/lib/useAiSettings.ts`（新增）
- `src-tauri/src/ai/commands.rs`
- `src-tauri/src/ai/mod.rs`
- `src-tauri/src/ai/models.rs`（新增）
- `src-tauri/src/lib.rs`
- `src-tauri/src/settings.rs`
- `tools/test-m11-ai-settings.mjs`（新增）
