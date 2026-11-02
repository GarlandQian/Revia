# 05：安装版安全更新与便携提示

Status: ready-for-agent
Blocked by: 01

## What to build

安装版在最终确认并处理当前工作后安装重启；便携版只提示新版本与下载位置。

## Acceptance criteria

- [x] 前后端均识别分发形态，便携版即使直接调用命令也拒绝 updater 下载、安装和重启。
- [x] 便携版可以检查所选渠道、显示可用版本并打开对应便携下载入口，不调用 NSIS。
- [x] 安装版仍校验签名与已下载原字节；最终安装重启确认与草稿、写入、终端协调发生在 Windows 安装器退出前。
- [x] 更新回执区分待安装和新进程核对成功；失败保留包与明确状态，不假设退出后的代码会运行。
- [x] 保持 macOS 更新、稳定/预发布、安全 URL 与权限约束，通过相关既有更新生命周期/元数据回归。

## Comments

实施说明（`a30b435`、`af869f9`）：

- 后端 `ensure_install_allowed` 在下载、安装、重启入口拒绝便携版（单元测试）；前端根据
  `AppUpdateStatus.portable` 只显示“下载便携版”，隐藏下载/安装/重启。
- 便携下载地址由配置的 GitHub Release 仓库与 Q65 命名约定在 Rust 侧生成，`open_url`
  只允许 https://github.com；Node 回归覆盖 URL 生成。
- 安装前写入 `pendingInstall`，再调用安装器；新进程启动时按实际版本核对并转为安装回执，
  未达到则保留待安装状态。安装失败保留已下载字节供重试。
- 完整既有 Rust 更新测试目标通过；未在 Windows 真实安装器上验证退出/重启行为，待任务 07。

### Codex 首轮审查 · 2026-11-02

R2 · P2：Windows 安装调用会退出，最终选择须明确安装并重启、允许取消，沿既有生命周期处理编辑/写入/终端。 详见 [review01](../review01.md) 与 [repair01](../repair01.md)。

### 正式返修 · a3abb92

R2 已修：新增 `UpdateInstallConfirmation` 状态机。Windows 安装前明确提示“安装并重启”，
取消不执行安装；确认后进入原有 `startUpdateLifecycle`，仍经草稿落盘、Git 写入等待、
终端关闭协调与 CloseDriver。macOS 安装后仍单独确认重启。Node 生命周期回归覆盖
Windows 确认/取消、macOS 直接安装，以及确认后进入既有 CloseDriver 安装路径。证据见
[repair01-report](../repair01-report.md)；真实 Windows 安装器退出/重启仍待任务 07。
