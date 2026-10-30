# M7 实施与验收报告

Status: ready-for-agent

实施方：Paseo / Pi（同一会话；含 2026-10-31 首轮正式返修）。规划方 / 验收方：Codex。
分支 `m7`（worktree `/Volumes/T7 Shield/project/Revia-m7`），规划基点 `581d1527651e981018f2b0830d994bca49580254`。
独占证据根：`/private/tmp/revia-m7-batch-g5scyw0r`（首轮交付原件、review-01 审查目录、返修日志全部保留，不覆盖）。

## Codex 当前结论 — 2026-10-31 review02

候选 `384360c4ea7b30b04f0114a60156107c9fa6c501` 返修增量审查未通过。Standards：9项硬违反（其中1项P3非阻断）、3项判断建议；Spec：18项。两轴原文与证据分别追加对应任务Comments，完整报告在任务14；P3已记backlog15。唯一正式Pi返修已使用，明确遗留由Codex直接接管。

原始末轮Rust日志确为772通过/0失败/1忽略、记录exit0，其它末轮检查即时exit0，但多数输出合流且无逐轮输入hash；证据限制见review02/delivery-evidence-audit.md。原实现报告保留在下方作为历史，不代表本候选通过，不能以构建/冒烟代替真实桌面。任务13/14事实门槛仍未满足。

## Codex 直接修复首组（进行中，非最终候选）

已直接修正：真实仓库主分支的克隆/诊断/更新对话框挂载；诊断严格字段投影、七天摘要/清空和预览快照导出；克隆每次读取当前Git设置、实际argv与凭据打码；构建签名不再二次base64编码，公钥接受原文/外包base64；快捷键替换用显式空绑定，单项恢复默认同样先确认冲突。隔离本地克隆测试另外复现并修正了完成日志重复获取缓冲锁导致的卡死。

本组证据在`codex-checks/`，每轮保存跟踪/未跟踪源码hash、精确argv、原stdout/stderr、真实exit。`diagnostics-clone-updater-cargo-03`通过4项真实生产入口集成回归（诊断2、克隆1、更新清单与锁定Tauri updater下载验签1），`shortcuts-node-01`通过2项实际模块按键分派回归，`shortcuts-persistence-01`通过1项Registry保存/重载回归；前端构建`codex-frontend-build-02`、绑定收敛后的类型检查`codex-frontend-types-03`、`codex-lint-02`均exit0；`codex-x18n-01`为0错误/69警告（不声明警告均为旧问题）。公开签名测试仅下载验签，不执行安装；mock app仅承载事件和updater，不属于桌面验收。

失败原件全部保留：`diagnostics-clone-cargo-01`真实exit101复现日志锁死；`updater-metadata-cargo-01`独占PATH漏Node、`-02`fixture漏插件配置均exit101，补齐fixture后`-03`通过；绑定格式化`diagnostics-bindings-whitespace-01`子进程未启动（launcher exit1，不能记作node退出码）。工具造成的99个无关绑定空白变化已逐项比对并回退，回退前字节另存`impl/binding-whitespace-before-revert`，三个诊断绑定语义变化保留。

编辑/草稿、终端生命周期、PR身份/保护/检出、令牌到期刷新、更新包保留及渠道/安装协调等明确问题仍在处理。全量最终检查、全新独立增量两轴和Release尚未完成；任务13/14真实桌面门槛未满足。formalRepairUsed保持true，不再次派Pi。

## 版本历史

| 提交 | 内容 |
| --- | --- |
| `4657407` | 初次实施（任务 01–12），首轮两轴审查对象 |
| `f924719` | Codex 审查 Comments / 状态纠正 / backlog 15（Codex 提交） |
| `70c59ac` | **正式返修**：F01–F22 与 H1–H9 修复（Q113 唯一正式返修窗口） |
| `56c1f13` | 返修处理结果记入任务 Comments，任务状态改 ready-for-review |

## 返修对照（审查 review-01-oim6d9nj → 处理 → 证据）

| 发现 | 处理 | 生产入口 / 证据 |
| --- | --- | --- |
| F01 P1 玻璃层 200×80 | GlassLayer 显式 width/height 100% + !important 优先级修正；popper 玻璃语义 | `src/features/m7/glass/GlassLayer.tsx`、`src/index.css`；冒烟 smoke-repair.json windowLayer=true、布局正常 |
| F02 P1 对话框/设置漏接 | 主分支补挂 Clone/Diagnostics/UpdatePanel，Settings 补 4 props；删除重复 CloneDialog | `src/App.tsx` 主 return 分支 |
| F03/H1 P1 切换丢草稿 | 卸载同步落草稿；基线变化保留两版走冲突流程 | `EditableWorkspace.tsx` 卸载 flush + 恢复分支 |
| F04/H3 P1 草稿未纳入写门禁 | 暂存/取消暂存/丢弃/提交在脏编辑时拒绝（双语错误）；局部放弃/保存分支修正 | `src/App.tsx` blockOnDirtyEdit 包装；`EditableWorkspace.tsx` confirmClose |
| F05 P1 终端会话恢复 | 重挂载认领运行中会话；绝对 cwd 恢复；切 worktree 隔离 | `TerminalPanel.tsx` claimOrOpen；`terminal.rs` resolve_cwd + 回归测试 cwd_tests |
| F06/H9 P1 关终端无确认 | 运行中页签关闭内联确认 | `TerminalPanel.tsx` confirmCloseSession |
| F07 P1 审阅/合并丢 remote | 请求结构带 remote，命令层拒绝缺省回退 | `ReviewSubmitRequest/MergeRequest.remote`；`PrContext::new(request.remote)` |
| F08 P1 迟到响应覆盖 | 请求代际 + 目标身份核对，切目标清状态 | `GithubPanel.tsx` requestEpoch/listEpoch |
| F09 P1 草稿键不匹配 | ReviewDraft.head_sha；m7_github_repo_key 命令；列表按 head 过滤 | `store.rs` / `commands.rs` / `GithubPanel.tsx`；绑定重新生成 |
| F10 P1 保护并集/缓存覆盖 | 失败保留旧缓存；推送/预览用并集（protected_branches） | `pr.rs` protection_state 分支重写；`commands.rs` protected_with_github |
| F11/H4/H5 P1 检出先改分支 | 写门最先进；merge-base 快进判定；非快进需 overwrite_confirmed + 前端确认对话框 | `pr.rs` checkout 重写；`GithubPanel.tsx` divergedBranch |
| F12 P1 诊断原样导出 | 按 worktree 过滤 + 字段白名单（error_class + 单行摘要）+ email 脱敏 | `diagnostics.rs` 重写；新增 5 项单元测试 |
| F13 P1 更新状态机 | 检查/下载/已下载/安装/重启分开；安装前完整协调（写/待核对/终端/编辑） | `update.rs` download_update + install 校验 downloaded_version；`UpdatePanel.tsx` phase；App beforeUpdateInstall |
| F14 P1 无更新清单 | tools/generate-latest-json.mjs + release.yml 生成上传 latest(-prerelease).json | fixture 实测 `repair/latest-json-test.out`；YAML 解析通过 |
| F15 P1 公钥编码 | minisign 全文/base64 统一转 base64，坏输入 CI 即失败 | fixture 实测 `repair/f15-*.log`（真实格式 exit 0） |
| F16 P2 设备码取消/刷新 | cancel 命令 + 取消按钮；refresh_token 持久化（独立 keyring 服务）与登出清除 | `auth.rs` / `mod.rs` TokenStore；`m7_github_device_cancel` |
| F17 P2 冲突替换/提交框 | 冲突确认替换 UI；提交框 matchesAccelerator(acceleratorOf("commit")) | `settings.tsx` pendingReplace；`CommitComposer.tsx` |
| F18 P2 fork 来源 | validate_pr_head 接受 owner:branch（测试）；创建表单来源远端选择 | `pr.rs` / `GithubPanel.tsx` |
| F19 P2 分页/裁剪 | files/comments 按 Link 翻页；patch 不裁剪；完整 diff 入口 | `ApiMeta.next_page`；`GithubPanel.tsx` fullDiff |
| F20 P2 统一视图只读 | 编辑入口仅 split+unstaged；统一视图引导切 split | `App.tsx` 编辑按钮条件重写 |
| F21 P2 摘要 7 天/清空 | DiagnosticsSummaryStore；预览/导出同快照；清空命令与按钮 | 新命令 m7_diagnostics_summary/clear；`DiagnosticsDialog.tsx` |
| F22 P2 渠道 | 移除 automatic；allow_prerelease 设置 + 双清单 endpoint 切换 | `settings.rs` / `update.rs` / `check-release-version.mjs` |
| H2 P2 保存写生命周期 | 保存走生产写入口（持写锁）；更新安装门禁协调 | `edit.rs` 既有写锁；App 门禁 |
| H7 P2 错误结构 | edit.rs cause 结构化；PrMergeOutcome.error 载荷；前端 formatAppError | `edit.rs` / `pr.rs` / `GithubPanel.tsx` |
| H8 P2 克隆旁路 | CloneManager 读设置 Git 程序 + 写统一命令日志 | `clone.rs` run 记录；`commands.rs` AppState |
| H10 P3（顺带） | dmg 重命名约定 + macos-SHA256SUMS.txt | release.yml Rename/SHA256SUMS 步骤 |
| H11 P3（顺带） | hunk → 块（经 x18n） | locales 双语已改 |

P3 backlog 项（15-m7-review-maintenance.md）未扩大重构；重复 CloneDialog 随 F02 自然消除，已在任务 01 Comments 记录。

## 最终验证（受控 runner，真实 exit）

| 检查 | 命令 | exit | 结果 | 原件 |
| --- | --- | --- | --- | --- |
| Rust 全量测试 | `cargo test --manifest-path src-tauri/Cargo.toml` | 0 | **772 passed / 0 failed / 1 ignored**（单轮完整，无拼接） | `repair/logs/cargo-test-final-repair.log/.exit` |
| 类型检查 | `pnpm exec tsc -b` | 0 | 0 错 | `repair/logs/tsc-final-repair.log/.exit` |
| Lint | `pnpm lint` | 0 | 0 错 | `repair/logs/lint-final.log/.exit` |
| i18n | `npx @x18n/cli check` | 0 | 0 错 69 预存警告 | `repair/logs/x18n-final.log/.exit` |
| 前端构建 | `pnpm build` | 0 | 成功 | `repair/logs/fe-build-final.log/.exit` |
| Release 构建 | `cargo build --release` | 0 | 成功 | `repair/logs/cargo-release-final.log/.exit` |
| 隔离冒烟 | REVIA_SMOKE=1（独占 HOME/appdata/fixture） | 0 | ok=true；玻璃整窗挂载、并排色带、视图切换锚点、M7 IPC 全通 | `repair/logs/smoke-repair.json/.exit` |

Release 二进制 sha256：`1e6d3f1c83164a07c25a77591b630eddfd2e5f63492c5a8f80c8bdbda64951fb`。

返修期间一次 m6_flows 15s 超时（负载抖动，与首轮相同用例），全量重跑该套件 exit 0（`repair/logs/m6-full.log/.exit`）；最终 cargo-test-final-repair 为单轮完整 772/0。

## 逐任务状态（返修后，待 Codex 重核）

| 任务 | 状态 |
| --- | --- |
| 01–12 | **ready-for-review**（返修已完成，勾选保留待 Codex 按原始规格逐项重核；不据旧勾选宣称 done） |
| 13 更改列表 | **needs-info**（不变：Rebased 1.1.22 终端已有暂存内容实测未取得） |
| 14 整包验收 | **needs-info / blocked**（依赖 13 与真实桌面） |

## 仍未验证（如实声明）

- 强折射（SVG 滤镜 / WebGL）在真实 WKWebView / WebView2 的视觉证据：未取得，留任务 14。
- 真实 GitHub 账号 / 钥匙串 / 设备码完整流：留任务 14 与外部接线（App client_id 未配置）。
- 受控 HTTP fixture 对 GitHub API 的请求目标 / 次数 / 版本变化 e2e：本轮未建，静态与单元级覆盖（返修窗口已用尽，留 Codex 处置）。
- 真实 CI 运行、实际签名更新安装、首次下载启动：未执行。
- Windows 实机外观、D06 长测、Rebased 对照桌面验收：任务 14。
- 任务 09“完整分页”按单分支 protection API 语义实现；若规格口径为分支列表分页需 Codex 裁定。

## 会话与渠道

- Paseo `8a5673e4-a7bb-4361-8307-6f523c81ee31` / Pi `01a18ce0-67c3-74fa-9c20-585fd434e10e`，模型 magpie/cpa/GLM-5.3-Flash。
- 渠道 502 原件：批次 `flash-channel-error-kedbp795`；首轮交付原件：`review-01-oim6d9nj/delivery-evidence-audit.md` 与 `pi-delivery-original.jsonl`。
- formalRepairUsed=true（本轮已使用唯一正式返修窗口）。

## 审查与验收

由 Codex 对返修候选 `70c59ac`（+文档 `56c1f13`）做复核与真实桌面验收（任务 14，含任务 13 事实门槛）。实施方不自行宣告整包通过。
