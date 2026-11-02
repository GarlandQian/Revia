# M8 正式返修报告（一次）

Status: ready-for-human
日期：2026-11-02
基线：`290ef9b`（审查记录），代码 `af869f9`
返修提交：`a3abb92`
证据根：`/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m8-repair01.9VOhvU`

按 Q113，本次只处理 X1、X2/S1、X3、R1、R2；S2 已转 backlog 17，不在本次范围。
未推送、未开 PR、未打标签、未创建或公开 Release、未合并 main、未改系统配置。

## 修复结果

### X1：Windows 分支编译类型错误

- `shell.rs` 的三处 `fallback_windows(...)` 不再向 `timed_out: bool` 传 `None`：
  worktree 缺失与空环境显式传 `false`，捕获失败传 `failure.timed_out`。
- `windows_env::run` 返回带 `timed_out` 的 `CaptureFailure`；只有真实超时才置位，
  profile 失败、Set-Location 失败、启动/读取失败保持 `false`，不再把所有失败伪装成超时。
- PowerShell 退出码映射为结构化原因（13 → profile，12/14 → worktree，其他 → 一般捕获失败），
  单元测试覆盖映射与超时标志。
- **仍未运行 Windows 编译**：本机只有 macOS 工具链，没有 `x86_64-pc-windows-msvc` target；
  CI 工作流已有 Windows 编译与 portable 检查，但本轮未推送、未执行远端 Actions。

### X2/S1：profile 失败回退与结构化诊断

- 捕获脚本在 profile 抛错 / 执行策略阻止时 `exit 13`，首次或 profile 后 Set-Location 失败
  `exit 12/14`；Rust 对非零退出直接返回捕获失败，不读取部分环境转储。
- `resolve` 对任何捕获失败使用进程环境回退，`used_fallback=true`，并记录：
  - `fallback_reason`：`errors.environment.shellProfileFailed` /
    `shellWorktreeMissing` / `shellTimeout` / `shellCaptureFailed`；
  - `fallback_detail`：截断后的原始 stderr（只随本次会话，不持久化）。
- `WorktreeSession` 的 shell 环境检查由纯函数 `shell_environment_check` 生成：
  `ok=false`，`hint` 使用结构化错误键，原始详情作为独立 `{detail}` 参数；前端按当前语言
  翻译，不把整段中文诊断塞进翻译参数。x18n 已加入中英文键。
- 单元测试覆盖 profile 回退原因的检查载荷；真实 PowerShell/profile 仍未运行。

### X3：显式验收/测量的 WebView2 隔离

- `distribution::plan` 改为纯路径计算，不再创建/探测目录。
- 新增 `with_isolated_data_dir`：安装版与便携版的显式验收/测量覆盖都把 WebView2 用户数据
  放到 `<覆盖目录>/webview2`。
- `lib.rs` 先选出最终覆盖目录，再调用 `ensure_writable`；便携版正常启动仍检查 exe 旁
  `data` / `data/webview2`，安装版正常启动保持 Tauri 默认位置。macOS 建窗路径未改。
- 单元测试覆盖：
  - 两种形态的显式覆盖最终目录与 `webview2`；
  - 覆盖时不创建标准目录或便携正常 `data`；
  - 便携正常 `data` 被文件占住时，显式覆盖仍能先成功，证明没有先触碰正常目录。

### R1：捕获/回收/stderr 有界

- 新增 `shell/bounded.rs`：复用 `platform::output_pipe::OutputPipe`，持续排空但只保留上限
  内容；进程回收后设置 `finished`，最多再排空 `DRAIN_GRACE`（250ms），超过上限继续排空
  而不关闭读端。
- `windows_env::run` 的 stderr 改为该有界 reader + `recv_timeout`，主进程在硬超时后
  kill/wait，再限定等待 reader；不再无限 `join` 阻塞 pipe。
- 真实子进程回归（`#[cfg(unix)]`，测试内自建并清理）：
  - 后台孙进程持有写端时，杀父进程后仍在 3 秒内返回；
  - 输出超过上限时只保留上限并标记截断，进程正常结束；
  - 正常退出保留完整小输出。
- Windows 侧使用同一 `OutputPipe`/reader 代码；Windows 编译与真实 PowerShell 未运行。

### R2：Windows 安装的最终确认

- 新增 `UpdateInstallConfirmation` 状态机（`src/lib/m7/updateState.ts`）：
  Windows 的 `installUpdate` 先返回 `confirm`，取消清空、不进入生命周期；确认后才返回
  `installUpdate`。macOS 仍直接安装，安装后单独确认重启。
- `App.tsx` 使用该状态机；Windows 确认弹窗明确“安装并重启”，取消不安装；确认后调用
  原有 `startUpdateLifecycle`，仍经草稿落盘、Git 写入等待、终端关闭协调和 CloseDriver。
- `UpdatePanel` 在 Windows 下载完成后按钮文案改为“安装并重启”。
- Node 回归（`tools/test-update-lifecycle.mjs`）覆盖：Windows 确认/取消/再次确认、
  macOS 直接安装、确认后进入既有 CloseDriver 生命周期，而不是只匹配文案。

## 检查与证据

命令均在 `/Volumes/T7 Shield/project/Revia-m8` 执行；日志、argv、退出码在证据根 `checks/`。

| 检查 | 命令 | 退出码 | 结果 |
| --- | --- | --- | --- |
| 前端类型 | `pnpm exec tsc -b` | 0 | `checks/tsc.log` |
| Lint | `pnpm lint` | 0 | `checks/lint.log`；仅既有 mockup 警告 |
| 前端构建 | `pnpm build` | 0 | `checks/build.log` |
| x18n check | `pnpm i18n:check` | 0 | `checks/x18n-check.log`；0 errors，75 个 unused-key 警告按 Q171 保留 |
| 写死中文扫描 | `pnpm i18n:extract` | 0 | `checks/x18n-extract.log`；78 候选，0 含中文 |
| Node 入口/生命周期 | `node --test tools/test-shortcuts.mjs tools/test-update-lifecycle.mjs tools/test-release-manifest.mjs tools/test-windows-deps.mjs tools/test-release-source.mjs tools/test-ui-input.mjs tools/test-terminal-sessions.mjs` | 0 | `checks/node-tests.log`；40 tests，40 pass |
| Rust 完整测试 | `/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml --no-fail-fast -- --test-threads=4` | 0 | `checks/cargo-test-t4.log`；32 个测试目标 ok，0 failed；lib 377 passed |
| 便携特性检查 | `/opt/homebrew/bin/cargo check --features portable --manifest-path src-tauri/Cargo.toml` | 0 | `checks/cargo-check-portable.log` |
| 绑定生成 | `node tools/strip-binding-whitespace.mjs` | 0 | `checks/bindings-strip.log`；本次无绑定语义变更，空白差异已恢复 |

补充说明：

- 首次完整测试用默认线程数时，`discard_recovery`、`history_readonly`、`m6_flows`、`merge`
  出现失败（原因尚未确认）；逐项重跑均通过，`m6_flows` 与完整套件改用
  `--test-threads=4` 后全绿。失败与重跑日志都保留在 `checks/`，没有把失败隐藏成通过。
- 绑定生成在 `cargo test` 后会重新写入行尾空白；已运行现有 strip，并把无语义变化的绑定
  恢复到 HEAD，`git diff --check` 通过。
- 证据根由 `mktemp -d` 独占创建；返修开始时两个固定 `/tmp` 输出已复制到 `pre-existing/`
  并注明原路径与命令，原文件未删除；后续检查未再写固定路径。

## 未验证

- Windows 编译、安装/卸载/升级、便携启动、WebView2 隔离与缺失提示；
- 真实 PowerShell 7 / Windows PowerShell、profile 执行策略、fnm hooks、超时；
- Credential Manager 重启登录、Mica / DPI / IME / AltGr / diff 性能；
- 真实 Windows 安装器退出/重启与更新回执。

这些仍由任务 07 在真实 Windows 环境验收；当前状态保持 ready-for-human。

## Codex 增量独立复核

review02 指出 S3/X4 的非终止 profile 错误仍未正确回退，已由 Codex 按 Q113 接手；本报告以上修复声明保留作实施记录，最终状态以 review02 和规划方最终验证记录为准。S4 转 backlog 18。默认并发的两轮失败均保留；低并发通过不能单独证明失败原因与本轮无关。
