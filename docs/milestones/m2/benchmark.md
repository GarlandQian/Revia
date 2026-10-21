# 长期运行基线的复现方法

本工具只验证 M2 任务 01，不实施暂存等后续能力。运行结束不等于验收通过：还需检查真实桌面、内存趋势和 M1 性能回归。

## 准备与启动

在 `m2` worktree 执行；测试期间不要重建同一路径的应用或修改 fixture。

```bash
# fixture 尚不存在时生成；不要对正在受测的 fixture 再次执行。
tools/fixtures/setup.sh /tmp/revia-m1-fixtures

# 每次先构建当前源码，再启动单进程 30 分钟测试。
tools/run-long-benchmark.sh 30

# 已确认 bundle 与源码一致时，也可指定独立输出名直接运行。
caffeinate -di python3 tools/bench-long-running.py \
  --minutes 30 --repo /tmp/revia-m1-fixtures/diffs \
  --output /tmp/revia-baseline-new.json
```

将标题带 PID 的受测 Revia 窗口置于前台，保持可见，不最小化或锁屏。工具最多等待 120 秒取得真实合成帧；等待不计入 30 分钟操作时长。`caffeinate -di` 仅在命令存活期间避免空闲睡眠，不改变持久系统设置，也不能解除锁屏或防止手动隐藏窗口。

## 负载与证据

- 固定 `diffs` 的一个主 worktree、7 个改动条目：二进制 PNG、日志、lock 文件、Vue、Swift、大 TypeScript 文件、已暂存 TypeScript 文件。该任务不修改工作区、暂存区或 Git 配置。
- 同一应用进程循环打开全部条目；文本文件实际展开、切换并排/统一视图、滚动及中部跳转。等待加载失败、视图按钮未切换成功都会中断。
- 每约 10 秒记录真实 rAF 频率、可见性、最长合成帧间隔、操作位置和缓存计数；隐藏事件或超过 2 秒的合成中断使本次无效。2 秒是检测长时间停绘的工具保护，不是取代 Q84 的滚动性能指标。
- 系统侧按应用 PID 的 launchd `services` 表识别 WebContent、GPU、Networking；不使用系统新增 PID 差集。每个样本记录进程归属与 RSS，另每 3 次采样记录 Rust 和 WebContent 的 physical footprint。两种口径分别保留，不用一种替代另一种。进程改变或采样缺失不能作为同一 WebView 的长测通过。
- 使用新建的独立应用数据目录，避免把 fixture 添加到真实用户的仓库列表。路径记录在报告中；不自动删除原始现场。
- 驱动只控制自己启动的进程，不按应用名/路径批量终止；超时、没有前端结果或操作时长不足以非零状态退出。
- 启动前固定二进制 SHA-256；结束核对未被替换或移除。测试中途重建应用会使构建证据无效。

输出包括汇总 `.json`、前端原始 `.json.frontend.json`、逐条落盘的 `.json.samples.jsonl`、标准输出 `.json.log` 和前端/Rust 步骤 `.json.frontend.log`。输出已存在时拒绝覆盖。日志仅含本地测试信息，不上传。

内存分析保留完整时序，另列操作开始后 180 秒的热身分界、稳态分段均值和线性趋势；短测不产生稳态结论。不能仅凭进程退出码、一个峰值或斜率自动宣布通过。还需结合真实桌面截图和时序判定是否持续增长，必要时定位、修复后重新长测。

## 性能回归

M1 的首屏、跳转、滚动指标计算保持原口径，自动测量先等待可见窗口。可使用 `REVIA_MEASURE_DATA_DIR` 隔离其设置：

```bash
REVIA_MEASURE=1 REVIA_MEASURE_ONLY=split \
REVIA_MEASURE_REPO=/tmp/revia-m1-fixtures/diffs \
REVIA_MEASURE_PATH=src/generated.ts \
REVIA_MEASURE_DATA_DIR="$(mktemp -d /tmp/revia-perf.XXXXXX)" \
REVIA_MEASURE_LOG=/tmp/revia-perf-split-new.json \
src-tauri/target/release/bundle/macos/Revia.app/Contents/MacOS/revia
```

每个视图独立进程，启动后置前窗口；统一视图用 `REVIA_MEASURE_ONLY=unified`。前端结束报告的 `ok` 和可见性必须有效，再检查首屏、跳转、颜色恢复与未覆盖帧，不接受隐藏窗口的计时数据。

## 工具回归检查

```bash
PYTHONDONTWRITEBYTECODE=1 python3 tools/test_bench_long_running.py
```

标准库测试覆盖无结果/短时长不能通过、只归属目标进程服务、短测没有稳态结论，以及受测二进制替换/删除时仍保留报告并判无效。不引入新的测试框架。
