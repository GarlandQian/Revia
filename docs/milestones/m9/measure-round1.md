# M9 第一轮实测

Status: ready-for-human（macOS 部分已测，含切换；Windows 等用户发回诊断信息）
日期：2026-11-08
依据：Q211、Q216、Q218；任务 [10](./issues/10-measure-round1.md)

## 构建来源

测量包全部由 GitHub Actions 的 `measure-build.yml` 打包，不用本地构建。

- **Windows 便携版**：run [42469621241](https://github.com/GarlandQian/Revia/actions/runs/42469621241)，产物 `revia-m9-a5453db-portable`；
- **macOS dmg**：run [42471366459](https://github.com/GarlandQian/Revia/actions/runs/42471366459)，产物 `revia-m9-fda158e-macos`。诊断信息中的 `build` 为 `m9`、`fda158e`，`measure: true`。

Windows release 编译首次通过（此前 Windows 专属代码只在 Mac 上写过）。NSIS 打包两次因 GitHub 下载 NSIS 返回 504 失败，属于外部临时故障；随后工作流改为先上传便携版、NSIS 失败时重试。

## macOS（规划方，Apple M2 Pro，macOS 27.2，1920×1080 @2x）

方法：

- 从 Actions 的 dmg 中取出 Revia.app，以验收隔离模式（`REVIA_ACCEPTANCE=1`，独立数据目录）在可见窗口中启动；
- 测试仓库由 `tools/fixtures/setup.sh` 生成：`diffs`（7 个改动文件）、`large`（约 2 万个文件、3 万个提交、300 个改动文件）；
- 每次启动时打开仓库列表中的第一个仓库，然后从“导出诊断信息”读取 `performanceRecording.opens`；
- 计时从发起打开开始，到文件列表绘制后的下一帧结束。启动时的打开没有用户点击，所以起点是程序发起打开的时刻，不含点击排队。

| 场景 | 改动文件 | 环境捕获 | 会话就绪 | 文件列表取数 | 文件列表绘制 | 日志首屏 | 结论 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `diffs` 第一次打开，从未找到过 Git（同步解析登录 shell） | 7 | 149ms | 157ms | 72ms | **261ms** | 239ms | ≤300ms，通过 |
| `diffs` 第一次打开，有记住的 Git（Q222） | 7 | 0 | 27ms | 96ms | **163ms** | 135ms | 通过 |
| `large` 第一次打开，有记住的 Git | 300 | 0 | 26ms | 194ms | **243ms** | 173ms | 通过 |

空闲：主进程平均 CPU 0.15–0.16%，峰值 0.43%（78 次采样，约 6.5 分钟）。macOS 的 WebKit 进程由 launchd 拉起，不在 Revia 的进程树里，没有计入。监视触发的刷新为 0 次。

### 切换（点击触发，端到端）

用户同意全屏接管后补测。同一次运行中先打开 `large`，再从仓库选择器依次点 `diffs`、`large`、`diffs`……共 7 次。计时从点击那一刻开始（含主线程排队），到文件列表绘制后的下一帧结束。

| 切换到 | 改动文件 | 次数 | 文件列表取数 | 文件列表绘制 | 日志首屏 | 结论 |
| --- | --- | --- | --- | --- | --- | --- |
| `diffs` | 7 | 4 | 65–70ms | **88–110ms** | 80–110ms | ≤300ms，通过 |
| `large` | 300 | 3 | 199–218ms | **228–233ms** | 135–162ms | ≤300ms，通过 |

7 次切换的会话就绪都是 0（会话已在），没有卡顿。时间主要花在读取改动文件列表：`large` 的 300 个改动文件取数约 200ms。

记录说明：

- 启动时的那次打开发生在窗口被其他应用遮住时，WebKit 不绘制，所以这条记录没有绘制耗时，不计入；
- 第一次点 `diffs` 时，它的会话已经由别的命令先建立，记录为“切换”（`firstOpen: false`），耗时 104ms，与后面的切换一致。第一次打开的耗时以上一节的启动测量为准；
- 记录里的 `triggerMs`（1–20ms）实际量的是 `open_worktree` 往返，不是点击排队；端到端的绘制耗时从点击时刻算起，不受影响。

原始诊断文件放在规划方 scratchpad 的 `m9-mac-evidence/` 中，共 4 份（第 4 份是切换）。

### 测量中发现，待第二轮处理

- **切换到没有会话的仓库时，`list_worktree_status` 会同步解析一次登录 shell**：它在 async 命令里直接调用，占住一个异步运行时线程，与 `open_worktree` 并行，不在文件列表的计时路径上。macOS 上约 150ms；Windows 上要加载 PowerShell profile，可能是数秒的后台 CPU。等 Windows 诊断信息回来后，结合“空闲与切换时的 CPU”判断是否改为复用记住的环境（Q212、Q222 的思路）。

### 未测

- **滚动帧时间与交互延迟**：macOS 不在 Q211 的帧时间门槛范围内，本轮没有测。

## Windows（用户）

待用户按 [windows-measure](./windows-measure.md) 测量，并发回两份诊断信息：对照关、对照开。
