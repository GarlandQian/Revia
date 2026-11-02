# 07：集中复核与 Windows 桌面验收

Status: ready-for-agent
Blocked by: 01, 02, 03, 04, 05, 06

## What to build

固定完整候选，分别记录本机检查和真实 Windows 安装/日常工作流验收。

## Acceptance criteria

- [x] Pi 保存固定源码、精确命令、退出码和原 stdout/stderr；相关 Rust 完整目标、类型、lint、构建、x18n 与 Node 检查结果明确。
- [ ] Codex 首轮全范围两轴审查，意见写对应 Comments，Pi 正式返修最多一次。
- [x] 提供可直接执行的 Windows 10/11 安装、卸载/升级、便携改名/移动/不可写/WebView2 缺失验收步骤。
- [ ] 真实 Windows 核对 Credential Manager 重启登录、PowerShell profile/hooks、Git 核心工作流与目录实际落点。
- [ ] 真实可见窗口验证深浅色、100%/125%/150% DPI、窗口管理、IME/AltGr、Mica/辅助功能、diff 首屏/跳转/快速滚动。
- [x] 没有 Windows 桌面或 CI 权限时写明未验证，状态 ready-for-human，不把旧 CI 或 Mac 检查当 Windows 通过，不合 main。

## Comments

实施说明（`a30b435`、`af869f9`）：

- 本机证据根：
  `/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m8-evidence.Vij0gB`；
  详见 `../report.md` 与 `../windows-acceptance.md`。
- 真实 Windows 桌面未执行；未推送、未运行远端 Actions、未打标签、未创建或公开 Release、
  未合并 main。等待 Codex 两轴审查与 Windows 验收。

### Codex 首轮审查 · 2026-11-02

首轮两轴发现 X1/X2/X3、规划方 R1/R2 未修，不可把代码交付当验证完成。Windows 编译/真机依旧未验证；唯一正式返修后增量复核。 详见 [review01](../review01.md) 与 [repair01](../repair01.md)。

### 正式返修 · a3abb92

唯一一次正式返修已完成，X1、X2/S1、X3、R1、R2 逐项修复并记录在
[repair01-report](../repair01-report.md)；返修证据根
`/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m8-repair01.9VOhvU`。

本机检查：前端类型/lint/构建/x18n、40 个相关 Node 测试、Rust 完整套件（`--test-threads=4`，
32 目标 ok、lib 377 passed）与便携特性检查通过。默认并发完整测试的无关并发失败保留在
证据中，逐项重跑与 4 线程完整套件通过。Windows 编译与真机仍未验证；状态保持
ready-for-human，等待 Codex 增量复核。

### Codex 增量审查与接管 · 2026-11-02

review02 的 S3/X4：profile 非终止错误仍可被当作成功。Codex 已将生产脚本设为 Stop，补 Windows 真实进程回归（隔离 profile，验证 exit 13 与不发布部分环境）。当前 Mac 仅可运行环境模块检查；Windows 专属用例未运行，等待真实平台。S4 已转 backlog 18。
