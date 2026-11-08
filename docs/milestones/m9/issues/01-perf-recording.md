# 01：诊断信息附带性能记录与 Windows 电脑配置

Status: ready-for-agent
Blocked by: None（可立即开始）

## What to build

用户复现卡顿后导出一次诊断信息，就能看到帧耗时、长任务、交互延迟、进程 CPU 与内存、打开仓库的分段耗时，以及 Windows 电脑配置。见规格 A 节。

## Acceptance criteria

- [ ] 交互开始后才采样帧，最后一次输入 1 秒后停止；空闲时不跑 rAF 循环，记录器本身不产生持续 CPU 占用。
- [ ] 帧耗时按 diff 滚动、日志滚动、窗口缩放、其他分别统计 p50、p95、最大值，以及超过 16.7ms 和 33ms 的帧数。
- [ ] Chromium 下记录长任务和 Event Timing 交互延迟（按页签、文件、弹层、其他分类）；WebKit 下用超过 100ms 的帧间隔代替长任务。
- [ ] Rust 每 5 秒采样一次 CPU 与内存，保留 30 分钟；Windows 覆盖主进程和全部 WebView2 进程，macOS 至少覆盖主进程。
- [ ] 每次打开或切换仓库，记录触发、会话就绪（含环境捕获和读取 Git 版本）、文件列表数据、文件列表绘制、日志首屏绘制，并注明是第一次打开还是切换。
- [ ] 记录监视触发刷新的频率，以及触发路径属于被忽略、`.git` 内部还是普通文件。
- [ ] Windows 环境信息包括 CPU 型号、显卡名称、显示器分辨率与缩放、Windows 版本号、WebView2 版本、安装形态、当前窗口材质。
- [ ] `ipcDiagnostics.ts` 能识别 `http://ipc.localhost/`，Windows 上 IPC 计数生效。
- [ ] 新记录照 Q180 脱敏，不含文件正文、diff、路径原文；预览和导出流程不变。

## Comments
