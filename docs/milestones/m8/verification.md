# M8 规划方验证记录

Status: ready-for-human
日期：2026-11-02
规划方：Codex
固定代码候选：`83d40704214faf89e910af399632c9883d958ec1`，主分支基线 `b9cf807`。
worktree：`/Volumes/T7 Shield/project/Revia-m8`，分支 `m8`。

## 交付与审查

- 初始规格与用户确认：e7f4c25，Q204–Q206；凭据边界 ADR-0015。
- Pi 主体 a30b435、补充 af869f9；首轮两轴 review01 固定审查至 7ea69a3，规划方另核补充增量。
- Pi 唯一正式返修 a3abb92、报告 50ffce7；review02 仅审 290ef9b...50ffce7。
- 增量发现 S3/X4：Continue 下的非终止 profile 错误仍被当作成功。Codex 按 Q113 接管，于 83d4070 改 Stop 并补隔离 profile 的 Windows 真实进程回归，不再交 Pi 往返。
- X1 类型修正、X3 最终路径先选择再探测、R1 stderr 有界收尾、R2 可取消安装并重启确认已通过源码复核；S3/X4 的最终修正按 Microsoft PowerShell 的终止错误/错误偏好语义核对。该结论不替代 Windows 实测。
- P3：启动前原生提示语言见 backlog 17；环境捕获详情夹中文见 backlog 18，均按 Q174 保留为非阻断。

## 核对过的检查证据

实施方初始证据根：`/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m8-evidence.Vij0gB`。
返修证据根：`/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m8-repair01.9VOhvU`。
Codex 最终增量证据根：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m8-codex-final-k8tg1gie`。

规划方实际读取返修原始日志、命令、退出码、源码差异与最终状态文件，未只采用实施方结论。

| 固定范围 | 检查 | 结果 |
| --- | --- | --- |
| a3abb92 返修 | `pnpm exec tsc -b`、`pnpm lint`、`pnpm build` | 均 exit 0；lint 仍有旧预览警告，build 有分块体积提示 |
| a3abb92 返修 | `pnpm i18n:check`、`pnpm i18n:extract` | exit 0；0 翻译 errors，75 个 unused-key 警告保留；78 候选中 0 个中文硬编码 |
| a3abb92 返修 | 7 份相关 Node 工具/入口测试，精确 argv 见 repair01-report | exit 0；40 通过 |
| a3abb92 返修 | `cargo test --manifest-path src-tauri/Cargo.toml --no-fail-fast -- --test-threads=4` | exit 0；32 个目标合计 883 passed、0 failed、1 ignored，含 lib 377 passed |
| a3abb92 返修 | `cargo check --features portable --manifest-path src-tauri/Cargo.toml` | exit 0；仅本机 macOS 工具链 |
| 50ffce7 独立 Spec 审查 | `node --test tools/test-update-lifecycle.mjs` | 11/11 通过 |
| 83d4070 语义增量 | `cargo test --manifest-path src-tauri/Cargo.toml --lib shell::windows_env::tests -- --test-threads=4` | Codex 运行，exit 0；本机可编译的 4 项通过；Windows 条件用例未运行 |
| 最终工作区 | `git diff --check` | exit 0 |

返修的默认并发完整检查两次 exit 101，失败分布不同，涉及 discard_recovery、history_readonly、m6_flows、merge、commit_flow。相关用例重跑以及 4 线程完整检查通过；没有固定旧源码的对照证据能断言根因与本轮无关，因此仅记录现象，不改断言或放宽产品超时。完整失败日志保留，未删除或隐瞒。

最后一次接管只改变 Windows 捕获脚本的错误偏好并增加 Windows 专属回归；本机运行环境模块的最小检查，没有重复无变化的前端或完整 Rust 套件。

## 未验证与完成边界

当前机器是 macOS Apple Silicon，没有 Windows 工具链、虚拟机、连接主机或 PowerShell。以下尚未执行：Windows 编译；NSIS 与便携包实际构建和运行；静态 CRT 依赖实测；Credential Manager 重启登录；PowerShell 7/Windows PowerShell/profile/hooks；安装器退出/重启回执；Mica、DPI、IME/AltGr 和 WebView2 diff 性能。

新增的 `profile_nonterminating_error_rejects_the_partial_environment` 是 Windows 条件真实进程测试，使用系统 PowerShell、-NoProfile 和临时 profile，不修改用户 profile 或执行策略。它已经加入 Windows 测试目标，但当前没有运行结果。

代码与当前环境可做的检查已完成；M8 整体和任务 07 保持 ready-for-human。下一步按 windows-acceptance 对此固定候选取得真实 Windows 证据；未通过前不合 main。未推送、未开 PR、未打标签、未运行远端 Actions、未创建或发布 Release。主目录 main 的原有 `.gitignore` 改动保留。

## 合并与发布授权（2026-11-02）

上述验证记录描述授权前的固定代码证据。用户随后明确“合并入 main 发布 0.0.2 吧，现在的删除”（Q207），允许带已披露限制合并发布；Windows 真机验收状态不变。实际 CI、打包、下载校验与公开结果另记 [0.0.2 发布记录](./release-0.0.2-readiness.md)，不将授权当作验收证据。
