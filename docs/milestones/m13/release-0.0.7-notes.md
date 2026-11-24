# Revia 0.0.7

面向 macOS Apple Silicon 与 Windows x64 的早期试用版本。本版基于 0.0.6，重点修复 Windows 端点击「全部暂存 / 暂存 / 取消暂存」无反应以及界面和启动卡死的问题。

## 本次更新

- **修复全部暂存 / 暂存 / 取消暂存无反应与批量性能问题**：
  - 重构 `WorktreeSession::stage_files` / `discard_files` 并新增 `Git::stage_files` / `Git::unstage_files`；在严格保留逐文件 diff 指纹校验（ADR-0005 / ADR-0009）、`index.lock` 不删锁重试与丢弃前先存回收区安全边界的前提下，将批量操作从每文件重复全量刷新收敛为合并预检、分块批执行与批末单次刷新。
  - 前端在 `writeOrchestrator.begin` 被关闭门禁拦截/异常或编辑前检查（`prepareEditTargets`）失败时补充持久错误通知，避免静默无反馈。
- **修复 Windows 端界面卡死与启动阻塞**：
  - 将 `ai_status`、`ai_set_key`、`ai_clear_key`、`ai_confirm` 改为异步 `spawn_blocking` 命令，避免在 WebView2 UI 线程争锁或同步访问 Windows Credential Manager；同步异步化命令层会话获取与环境等待。
  - `Registry::load` 新增 `RepoEntry::build_fast`（优先手动 Git、环境记忆与进程 `PATH`，并校验记忆 Git 满足最低版本要求与有效仓库目录），不再在启动建窗前同步阻塞唤醒登录 PowerShell；磁盘无编辑草稿时跳过批量打开仓库会话；延迟 Windows 主窗口创建到 `AppState` 就绪之后，过期回收区与 stash 归档清理移至后台；修复 `windows_env` `spawn` 后管道写端释放时机。
- **修复 Windows 文件监听与记忆环境兼容问题**：
  - 统一 `watch.rs` 忽略匹配中 `\` 与 `/` 分隔符处理，避免被忽略文件在 Windows 上持续触发无效刷新，并将 `git check-ignore` 合并移出 `notify` 系统回调线程、先卸载 `_watcher` 再回收工作线程。
  - `env_from_remembered` 在 Windows 下保留 `SystemRoot`、`windir`、`ComSpec`、`TEMP`、`TMP` 等必需系统环境变量；`Git::set_env` 在 Windows 下按大小写不敏感替换并去重 `PATH`；`recovery.rs` 在 Windows 下兼容可选 `\r\n` 行尾同时保持 Unix 对含 `\r` 文件名的行为不变。

## 下载与安装

- macOS Apple Silicon：`Revia-0.0.7-macos-arm64.dmg`，将 `Revia.app` 复制到「应用程序」。
- Windows x64 安装版：`Revia-0.0.7-windows-x64-setup.exe`，默认仅为当前用户安装。
- Windows x64 便携版：`Revia-0.0.7-windows-x64-portable.zip`，解压后运行 `Revia/Revia.exe`；也提供单独的 `portable.exe`。便携目录须可写，系统须有 WebView2 Runtime。
- 两个平台均需系统 Git。macOS 暂无 Apple Developer ID 签名与公证，首次打开可能受 Gatekeeper 拦截；请从本仓库下载并核对随附平台 SHA256 文件。
- `.sig`、`.app.tar.gz` 与 `latest*.json` 用于应用更新。macOS 和 Windows 安装版可在应用内更新；便携版通过下载新版手动替换。

## 验证与已知限制

- 本地完成 `pnpm exec tsc -b`、`pnpm lint`、`pnpm i18n:check`、`pnpm i18n:extract`、`pnpm build`、30 个独立前端 Node 测试脚本（585/586 通过，1 条缺 `REVIA_EDIT_ERROR_EVIDENCE` 跳过）、Rust 全套单元测试（455/455 通过）与相关 Rust 集成测试（含 `environment_concurrency` 13/13、`file_staging` 20/20、`refresh_isolation` 15/15）、发布版本与清单预检及 `cargo check --features portable`。
- 依赖 `REVIA_M6_ERROR_FIXTURE` 的 2 个前端 M6 脚本未在外部夹具下重跑；Windows 真实桌面卡顿与批量暂存流畅度需在本次打包产物上由 Windows 实机确认；真实远端 AI 服务与历史性能验收仍待补验。

[固定源码](https://github.com/GarlandQian/Revia/tree/v0.0.7) · [M13 重构报告](https://github.com/GarlandQian/Revia/blob/v0.0.7/docs/milestones/m13/report.md)
