# 02：PowerShell 环境、hooks 与终端

Status: ready-for-agent
Blocked by: None（可立即开始）

## What to build

从 Windows 桌面启动后，Git/hooks 和每个 worktree 的终端使用对应的 PowerShell/profile 环境。

## Acceptance criteria

- [x] 共用 PowerShell 7 → Windows PowerShell 的探测与选择，Git 仍手动路径优先、否则解析后 PATH。
- [x] 按 worktree 启动并加载对应 profile，支持现有 fnm use-on-cd；不改用户 profile、执行策略或 Git 配置。
- [x] 环境协议不受 profile 提示污染，支持 Unicode、等号、换行与 Windows 不区分大小写的变量名。
- [x] 超时及失败有可见诊断与进程环境回退，缓存按 worktree；重新加载生效。
- [x] 终端使用真实 PTY、正确起始目录和相同 shell 选择；普通后台 Git 进程不额外弹控制台。
- [ ] 通过独立 fixture/profile 与真实 Git 生产边界补必要回归；Windows 专属未运行如实记录。

## Comments

实施说明（`a30b435`、`af869f9`）：

- 新增 `shell/windows_env.rs`：pwsh 7 → Windows PowerShell 探测、四类 profile 显式加载、
  UTF-8/NUL 文件转储、5 秒硬超时、进程环境回退与诊断；profile 加载后再次进入 worktree
  以触发 fnm 等目录钩子。纯协议与大小写合并有单元测试。
- 内置终端改用同一 `shell::shell_program` / `shell_args`；Git 普通调用、stash 窄例外、
  提交与克隆在 Windows 设置 `CREATE_NO_WINDOW`。
- 未在真实 Windows 上运行 PowerShell/profile/hooks，也未建立 Windows fixture profile；
  最后一项保持未勾选，待任务 07。

### Codex 首轮审查 · 2026-11-02

X1 · P1：fallback_windows 的 timed_out: bool 收到 None，Windows 不能编译。X2/S1 · P2：profile 失败当成功且诊断未到前端，须回退。R1 · P2：阻塞 stderr reader.join 会突破硬超时。 详见 [review01](../review01.md) 与 [repair01](../repair01.md)。

### 正式返修 · a3abb92

X1/X2/S1/R1 已修：

- `fallback_windows` 的三处 `timed_out` 改为真实 bool；`CaptureFailure` 区分超时、
  profile、Set-Location 与一般捕获失败，退出码映射有单元测试。
- profile / Set-Location 失败整次捕获失败并回退进程环境；`EnvCaptureInfo` 携带结构化
  `fallback_reason` 与独立 `fallback_detail`，会话检查按错误键展示，x18n 已加中英文键。
- stderr 改用 `platform::output_pipe::OutputPipe` 的有界读取；进程回收后最多排空 250ms，
  超限继续排空但不保存。真实子进程（含后台孙进程持有写端、超限输出）回归通过。

证据见 [repair01-report](../repair01-report.md)。Windows 编译、真实 PowerShell/profile
仍未运行，任务 02 最后一项保持未勾选。

### Codex 增量审查与接管 · 2026-11-02

review02 的 S3/X4：profile 非终止错误仍可被当作成功。Codex 已将生产脚本设为 Stop，补 Windows 真实进程回归（隔离 profile，验证 exit 13 与不发布部分环境）。当前 Mac 仅可运行环境模块检查；Windows 专属用例未运行，等待真实平台。S4 已转 backlog 18。
