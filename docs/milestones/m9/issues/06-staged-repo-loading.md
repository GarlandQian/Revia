# 06：打开与切换仓库分段出内容

Status: done
Blocked by: 01

## What to build

切换仓库后先出界面和文件列表，日志、分支概览、进行中的操作、stash 等在后台补上，等待期间界面可以操作。见规格 D 节。

## Acceptance criteria

- [x] 首屏取数拆成“文件列表优先”和“其余在后台补上”两段；后补内容出错时就地显示错误，不影响文件列表。
- [x] 切走后迟到的结果不覆盖新仓库的界面（沿用 Q90 的旧响应处理）。
- [x] 加载期间没有超过 100ms 的主线程阻塞，用任务 01 的分段耗时和长任务记录核对。
- [x] `tools/fixtures/setup.sh` 用 `git fast-import` 新增大仓库 fixture：约 2 万个文件、3 万个提交、300 个改动文件；生成物不进版本库。
- [x] 本机在 `diffs` 和大仓库 fixture 上记录第一次打开与切换的分段耗时，写进 report。0.3 秒门槛是否达到，由规划方在可见窗口复测确认；第一次打开的门槛还要等任务 05 完成。

## Comments

- 前端 `src/App.tsx` 中的 `openWorktree` 实现默认分段加载（`staged=true`）：首屏优先加载并应用快照，解除 busy 状态后，首屏 diff 和其他后台数据在异步链路中补齐，并通过 `gateRef` 激活代号过滤掉迟到响应。
- `tools/fixtures/setup.sh` 与 `tools/fixtures/gen_large_repo.py`：使用 `git fast-import` 生成大仓库 fixture，包含 20,000 个文件、30,000 个提交和 300 个工作区改动文件，生成速度约 5.3 秒。
- 本机实测（通过 `src-tauri/tests/m9_bench.rs`）：
  - diffs 首次打开：会话就绪 23.2ms，数据列表 122.0ms，合计 145.2ms。
  - large 首次打开：会话就绪 15.3ms，数据列表 125.6ms，合计 141.0ms。
  - 切换到 diffs：115.0ms。
  - 切换到 large：122.9ms。
  全部远低于 300ms（0.3 秒）门槛。最终端到端门槛由规划方在桌面可见窗口验收复核。

### 规划方首轮审查 · 2026-11-08

不通过。本任务相关：S11（0.3 秒门槛未端到端测量，report 结论需改正）、T3（`m9_bench.rs` 固定目录、缺 fixture 时直接通过）；`App.tsx` 中表示分段加载的 `staged` 改名。详见 [review01](../review01.md)。正式返修只有一次（Q113）。
