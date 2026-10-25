# 任务 10：集成验证矩阵与准备（本批交付）

受测源码：本批在 `be02cb7` 之上的独立提交（确切 SHA 见文末「受测提交」）。

这是**准备交付**，不是最终验收结论：任务 13 美化尚未实现、真实桌面 / D06 / 双平台 CI / 30 分钟长测均由规划方在固定版本上执行。本文件给出 T01–T14 / D01–D08 的映射、已有生产测试命令与真实结果、最终验收方法与未测边界。

## T01–T14 映射

| 规格 | 覆盖它的既有验证（真实 Git、生产入口） | 结果 |
| --- | --- | --- |
| T01 整文件暂存/取消暂存 | `tests/file_staging.rs`（20 项）：初始无 HEAD、空文件、部分暂存、Unicode/空格路径、模式变化、重命名 | 通过 |
| T02 hunk 暂存/取消暂存 | `tests/hunk_staging.rs`（24 项）：多 hunk、增删、无末尾换行、换行符、中文/空格路径、权限保留 | 通过 |
| T03 类型与状态边界 | `tests/file_staging.rs` 只读类型拒绝（符号链接/子模块/嵌套仓库/冲突/非 UTF-8）+ `session.rs` `supports_hunk_actions` | 通过 |
| T04 取消暂存重命名 | `tests/file_staging.rs` 重命名取消 → 删除 + 未跟踪两条状态 | 通过 |
| T05 过期指纹 | `tests/file_staging.rs` 内容/模式/重命名源/未跟踪/二进制指纹不匹配拒绝；无关文件改动仍允许 | 通过 |
| T06 锁与并发 | `tests/file_staging.rs` index.lock 重试不删锁、同 worktree 写互斥不排队；`tests/refresh_isolation.rs` | 通过 |
| T07 丢弃前完整保存 | `tests/discard_recovery.rs`（71 项，1 项桌面夹具 ignored）：保存失败不丢弃、单 hunk 保存整文件 | 通过 |
| T08 恢复完整性 | `tests/discard_recovery.rs`：内容/存在性/可执行位恢复，暂存区不变，基准是丢弃后状态 | 通过 |
| T09 恢复冲突 | `tests/discard_recovery.rs`：文本冲突只读比较、二进制另存不覆盖 | 通过 |
| T10 持久化隔离 | `tests/discard_recovery.rs` + `tests/review_marks.rs`：重启可见、跨 worktree 隔离、同路径重建不继承、14 天清理 | 通过 |
| T11 阶段中断 | `tests/discard_recovery.rs`：保存后/写入中/完成记录阶段中断 → 记录保留、不自动续跑、不确定结果 `NeedsVerification` | 通过 |
| T12 审阅 | `tests/review_marks.rs`（31 项）：版本失效、纯暂存保留、重启保留、跨组去重 | 通过 |
| T13 刷新与监视 | `tests/refresh_isolation.rs`（15 项）：工作区/元数据变化可见、切换/前台/手动刷新、监视失败降级、写前读取失败拒绝 | 通过 |
| T14 边界与路径 | `tests/file_staging.rs` + `tests/discard_recovery.rs`：未登记目标拒绝、越界拒绝、`--literal-pathspecs`、只读类型不可绕过 | 通过 |

Rust 合计：lib 100 + discard_recovery 71（1 ignored 桌面夹具）+ file_staging 20 + hunk_staging 24 + refresh_isolation 15 + review_marks 31 + session_flow 3 = **264 通过 / 1 ignored**。

## D01–D08 映射

| 规格 | 现状 | 谁在什么条件下验证 |
| --- | --- | --- |
| D01 30 分钟长测（Q98） | **未执行**（实施方不跑长测） | 规划方在固定 Release、可见窗口、同进程下主动操作 30 分钟，采 Rust/WebView 内存趋势 |
| D02 列表与 hunk 入口全流程 | Rust 层已覆盖（T01–T09）；桌面交互**未验证** | 规划方真实桌面 |
| D03 自动刷新与选择衔接 | `tests/refresh_isolation.rs` + `tools/test-refresh-gate.mjs`（选择门/快照门） | 自动化已覆盖；桌面观感由规划方验收 |
| D04 审阅交互 | `tests/review_marks.rs` + `test-refresh-gate.mjs` 导航用例 | 自动化已覆盖；桌面由规划方验收 |
| D05 写入期间关闭/退出与待核对 | 本轮新增 `CloseCoordinator` 与 9+ 项 Node 回归；**原生关闭/退出真实交互未验证** | 规划方真实桌面（macOS Quit / ⌘Q、关闭窗口、留在应用、待核对闭环） |
| D06 性能 | **本批未测**（实施方不跑 D06） | 规划方在固定 Release 上测首屏、两种跳转、两种视图滚动 |
| D07 检查与 Windows 编译 | 完整 Rust + 前端检查已通过；Windows x64 **需新 CI** | 规划方仅用现有 SSH origin 推 m2 触发双平台 CI |
| D08 界面清理与美化 | 12 已完成（测量面板仅测量模式、无 Git 版本与文件数、无 A−/A+）；13 **美化未实现**（效果图待确认） | 效果图经用户确认后实现，再由规划方桌面截图比对 |

## 检查命令与真实结果（本批）

日志目录：`/private/tmp/revia-batch02-8LEw43`（唯一 mktemp，原件保留）。

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `pnpm exec tsc -b` | 0 | — |
| `pnpm lint` | 0 | — |
| `pnpm build` | 0 | — |
| `node tools/test-refresh-gate.mjs` | 0 | tests 115 / pass 115 / fail 0 |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 0 | 264 通过 / 1 ignored |
| `pnpm bindings` | 0 | 生成并清理行尾空白 |
| `git diff --check` | 0 | — |

## 最终验收方法（规划方）

1. 固定版本：从 m2 取一个提交，记录 source SHA 与打包后的 binary SHA。
2. 构建 Release：`pnpm build` → `tools/build-app.sh release`。
3. 真实桌面：浅色/深色各截图，与 `issues/13-mockups-rev1/` 确认版比对（D08）。
4. D06：首屏、两种跳转、两种视图快速滚动，与 M1 基线对比（`docs/milestones/m2/benchmark.md` 方法）。
5. D01：可见窗口、同进程、主动操作 30 分钟，采内存趋势。
6. D07：仅用现有 SSH origin 推 m2，等双平台 CI（macOS arm64 + Windows x64）通过。
7. 全部通过后才合 main。

## 未测与边界（必须保留）

- 任务 13 美化**未实现**，D08 的「与确认的效果图一致」无法在本批判定。
- 真实桌面关闭/退出交互、macOS Quit/⌘Q 拦截、待核对闭环的实际观感**未验证**。
- D06 性能、30 分钟长测、双平台 CI **均未执行**。
- Windows x64 编译**需新 CI**（本机无法验证）。
- 侧栏半透明尚未实现，D06 影响未知。
- 本文件不改变任何已确认决策，也不扩大范围。

## 状态更新（整批 B02 返修后）

- **任务 08**：关闭/退出编排已按 B02 两轴返修（原生保护在真实写前成立、无授权不关窗、
  另存统一走 `finishWrite`、待核对按代际核对、重复请求不替换动作、文案集中 i18n）。
  **真实桌面关闭/Quit/等待/留在应用/待核对交互仍未验证。**
- **任务 13**：`m2-ui` 独立目录（`/Volumes/T7 Shield/project/Revia-m2-ui`，HEAD `0b87fa9`）
  已实现 rev3 界面，**尚待验收合入 m2**。本矩阵不把 `13-mockups/`（原稿）或
  `13-mockups-rev1/`（修订稿）当作最终当前状态；D08「美化后界面与确认效果图一致」
  必须在 UI 合入后的固定集成版本上重测。
- **任务 10 本身**：仍是**准备**，不是最终验收。合入 UI 后需重新固定集成版本，
  再跑一次完整检查（Rust / 前端 / CI），本文件届时更新受测 SHA 与结果。
- 旧日志目录（`/private/tmp/revia-batch01-sEGPJM`、`/private/tmp/revia-batch02-8LEw43`）
  **在本 worktree 已不可访问**（原 m2 worktree 被移除）；本批检查使用新的唯一目录，
  不重建旧日志、不声称旧原件仍在。

### 未测边界（补充）

- 侧栏半透明、浅/深色实际观感、D06 性能：**均未在 m2 验证**（UI 在 m2-ui 独立目录）。
- macOS Quit/⌘Q 的真实拦截：需要真实桌面（Rust 侧 `RunEvent::ExitRequested` +
  `exit_blocked` 已实现，但未在 GUI 中触发验证）。
- 保护 IPC（`set_exit_blocked`）失败时写操作会中止，这条路径**未在真实桌面复现**。

## 状态更新（整批 B03 返修后）

- **任务 08**：关闭/退出编排已收为单一生产入口 `CloseCoordinator.reconcile()`；
  退出保护改为**版本化**（Rust `apply_exit_guard(seq, blocked)` 只接受更大序号）；
  窗口关闭与原生退出监听都装好才允许写入（`CloseGuardGate`）；
  核对按请求代际 + pending 代际确认，旧回调不能授权新请求。
  **真实桌面关闭/Quit/等待/留在应用/待核对交互仍未验证。**
- **任务 13 当前基准为 Q107/rev3**（`m2-ui` 独立目录，HEAD 见其仓库），
  **尚未验收合入 m2**。本矩阵不把 `13-mockups/`（原稿）或 `13-mockups-rev1/`
  当作当前状态；这两份仅作历史存档。最终集成必须在 UI 合入后**重新固定版本**再测。
- **任务 10** 仍是准备，不是最终验收；合入 UI 后需重新固定集成版本并重跑检查。

### 历史内容（与当前状态无关，仅存档）

以下为早期轮次的描述，**不代表当前实现**：

- ~~「美化未实现」「侧栏半透明待测」~~ —— 这些是 rev1 阶段的记录；
  rev3 已在 m2-ui 实现，当前不把半透明当作待测项，等 UI 合入后按 D06 实测决定。
- 旧日志目录（`/private/tmp/revia-batch01-sEGPJM`、`/private/tmp/revia-batch02-8LEw43`）
  在本 worktree 已不可访问；各轮检查改用新的唯一目录，不重建旧日志。

### 未测边界（当前）

- macOS Quit/⌘Q 的真实拦截、监听安装失败时拒绝写入、待核对闭环的实际流程：
  需真实桌面（Rust 侧已实现，未在 GUI 触发验证）。
- D06 性能、30 分钟长测、双平台 CI：均未执行。
- Windows x64 编译需新 CI。
