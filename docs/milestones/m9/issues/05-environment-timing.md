# 05：打开 worktree 时的环境加载时机

Status: done
Blocked by: None（可立即开始）

## What to build

两个平台第一次打开某个 worktree 时，不再让登录 shell 或 PowerShell profile 挡住文件列表。先用已经找到过的 Git 读取状态，该 worktree 的完整环境在后台加载（Q212、Q222），见规格 D 节。

## Acceptance criteria

- [x] 手动指定的 Git 始终直接使用；没有手动指定时，按顺序取已找到过的 Git 及其环境的 PATH：先取本次运行中其他 worktree 的结果，再取上次运行记住的结果（存在 Revia 应用数据中）。从未找到过 Git 时，照旧等待。
- [x] 会话先用这份 Git 创建并读取状态、diff 和日志；`shell::resolve` 移到后台执行，不在打开路径上同步等待，也不在持有会话表锁时执行。
- [x] 后台环境就绪后，Git 路径变了就切换并重读一次；就绪前失败的读取等环境就绪后重试一次，不立即报错。
- [x] 执行 hooks、提交、fetch/pull/push、打开内置终端时使用该 worktree 完整的环境；尚未就绪时等待，并在界面上说明正在等待。
- [x] 环境捕获失败、超时的提示与回退沿用 Q205；不修改用户的 profile、执行策略或 Git 配置。
- [x] 加载期间切走或关闭 worktree，迟到的环境结果不登记到别的 worktree。补并发回归测试，参照 `file_staging.rs` 的 Barrier 写法。
- [x] 补回归测试：记住的 Git 被删除或换版本时，切换并重读；hooks 和提交在环境就绪前会等待。测试用独立的应用数据目录和替身 shell，不读用户真实的 profile。

## Comments

- `src-tauri/src/environment.rs`：实现 `EnvMemory`（持久化记录上次找到过的 Git 与 PATH）与 `EnvCoordinator`（本次运行内各 worktree 状态与后台就绪通知）。
- `src-tauri/src/commands.rs`：首次打开时若有记住或已知的 Git，立即创建会话并启动后台异步线程解析环境，不阻塞文件列表；`await_environment` 守卫 `execute_commit`、`fetch_remotes`、`update_project`、`push_preview`、`push_branch` 与 `m7_terminal_open`；前端状态栏在未就绪时显示“正在加载登录环境”文案与图标。
- 环境就绪后若 Git 路径发生改变，后端自动刷新会话并通知前端重读；并发隔离回归测试（基于 `std::sync::Barrier`）及跨运行持久化回归测试全部通过。

### 规划方首轮审查 · 2026-11-08

不通过。本任务相关：S2（记住的 Git 失效后所有仓库打不开）、S9（读取重试、Q205 提示、写操作后补做环境、所有可能执行 hooks 的操作统一等待）；T2（`errors.environment.stillLoading` 缺双语）、T3（并发测试移到 `src-tauri/tests/` 走生产入口）；`Git` 取环境快照的方法、`ShellEnv::path_string` 复用。详见 [review01](../review01.md)。正式返修只有一次（Q113）。
