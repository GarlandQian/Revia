# M9 首轮审查（两轴）

日期：2026-11-08
审查方：规划方（Claude），code-review 两轴，范围 `main...m9`（`c3f7906..b3b26ff`，11 个提交）
依据：[spec](./spec.md)、[issues](./issues/) 01–09、docs/README Q208–Q222、AGENTS 规则、[local-validation](../../agents/local-validation.md)、ADR-0002/0003/0013/0015

结论：**不通过，进入唯一一次正式返修（Q113）。** report 中“全部完成”“远低于 0.3 秒”两处结论不成立，见 S1–S3、S11。

规划方已在代码中逐项核实以下各条：S1、S2、S3、S10（GUID）、T2、T3（bench）、T4。其余由审查子代理给出文件与行号，返修时逐项确认。

两轴分开记录，不合并排序。

## Spec（规格轴）

### P1

- **S1 被忽略路径仍触发刷新**（规格 C、任务 04）
  - `watch.rs:535` 判定全部被忽略时返回 `Verdict::Refresh(Ignored)`，回调 `watch.rs:170-174` 照样 `tx.send`；测试 `watch.rs:1384` 断言的正是这个错误行为。
  - `.git/info/exclude` 被判为 Irrelevant（`watch.rs:468`），既不清缓存也不刷新。
  - **要求**：全部被忽略时不发送刷新，只计数；`.gitignore`（任意层级）、`.git/info/exclude`、`core.excludesFile` 指向的文件变化时，清缓存并刷新一次。改正测试，补 `.git/info/exclude` 的测试。
- **S2 记住的 Git 失效后，所有仓库都打不开**（规格 D、Q222、任务 05）
  - `session.rs:403` 的 `git.version()?` 失败后直接返回，`spawn_env_capture` 不会执行；`session_retrying` 空等 30 秒后报错；记忆从不更新；“重新加载环境”调用的 `reset()`（`environment.rs:333`）不清 `EnvMemory`。
  - **要求**：记住的 Git 不可用（不存在、无法执行、版本低于下限）时，丢弃这份记忆，退回“从未找到过 Git → 等待完整环境”的路径；完整环境就绪后更新记忆；“重新加载环境”同时清掉对应记忆。补回归测试：记住的 Git 被删除、换成低版本时，仍能打开并更新记忆。
- **S3 Windows 采样每 5 秒启动 PowerShell，并闪出控制台窗口**（规格 A、Q211、任务 01）
  - `perf.rs:527/654/697` 启动 `powershell`、`reg` 时没有经过 `platform::hide_console`。采样子进程本身会被算进子进程 CPU，污染“空闲 CPU <2%”的测量。macOS 每 5 秒也启动一次 `ps`。
  - **要求**：周期采样不启动任何子进程。Windows 用 Win32 API 枚举本进程树（Toolhelp 快照，或 WebView2 的进程信息接口），用 `GetProcessTimes` 和内存计数取值；macOS 用 `proc_pidinfo` / `task_info` 这类系统调用。一次性的电脑配置采集可以保留子进程，但 Windows 上必须 `hide_console`，并且只在导出诊断信息时执行，不进周期循环。

### P2

- **S4 对照外观并不朴素**（Q210、规格 H、任务 02）。实现只让 `window_glass.rs:80` 返回 `Unsupported`，走的仍是 CSS 材质路径：`GlassLayer` 的模糊和半透明都在，macOS 上还有 WebGL 按钮，这正是 Q210 说不能当对照的路径。**要求**：对照状态下所有路径都完全不透明——窗口与 WebView 背景不透明，没有 `backdrop-filter`/`filter: blur`，不渲染 `GlassLayer`，不播放镜片，不用 WebGL 按钮，卡片与弹层用实色。
- **S5 AI diff 受用户 diff 配置影响**（规格 F）
  - `payload.rs:162-176` 没有固定前缀，用户开了 `diff.noprefix` 时，根目录 lock 文件识别失败，内容会被发出去。
  - `payload.rs:344` 用 `contains("160000")` 判断子模块，正文含这个数字的普通文件会被误判。
  - **要求**：
    - 命令行固定 `--src-prefix=a/ --dst-prefix=b/`，并用 `-c` 关掉会影响输出的选项（不改用户配置）；
    - 文件分类（二进制、子模块、lock）改用结构化输出（`--raw -z` 的模式位、`--numstat -z` 的 `-`），按路径判断，不在正文里查字符串。
- **S6 AI 发送的 diff 原文写进了 Git 命令日志**（Q214）。发送内容经 `git.run_ok` 生成，stdout 最多 64KB 进入 `CommandRecord.output`。**要求**：生成发送内容的 Git 调用只记录命令、耗时和退出码，不记录 stdout；补测试断言日志里没有 diff 正文。
- **S7 更改列表模式下 amend 时不看勾选内容**（规格 F、Q220）。`CommitComposer.tsx:250` 只要勾了 amend 就发 `amend`，取的是用户 index 相对 HEAD^ 的 diff。**要求**：更改列表模式下 amend，用勾选内容建私有 index（同 `prepare(amend=true)`），再相对原提交的父提交取 diff。
- **S8 配置好 AI 后“生成”不出现**（规格 E、Q219）。`aiStatus` 只在 `worktreeId` 变化时刷新（`CommitComposer.tsx:172-182`）。**要求**：设置保存、密钥写入或清除后，立即刷新。
- **S9 环境后台加载的缺口**（规格 D、Q222）
  - “就绪前读取失败，就绪后重试一次”只覆盖了打开会话，状态、diff、日志的读取都没有重试；
  - 后台捕获失败的 Q205 提示存进了 `slot.error`，界面从不显示；
  - 有写操作时会跳过 `apply_environment`（`commands.rs:360`），之后的提交仍用临时环境；
  - 切换分支、merge、rebase、cherry-pick、revert、pull、stash 应用等同样可能执行 hooks，却没有等待环境就绪。
  - **要求**：
    - 所有可能执行 hooks 或需要凭据的 Git 写操作和网络操作，统一在一个入口等待完整环境；
    - 写操作结束后补做被推迟的 `apply_environment`；
    - 读取类命令就绪前失败的，就绪后重试一次；
    - 捕获失败按 Q205 在界面上提示。
- **S10 性能记录的数据有错**（规格 A、Q217）
  - macOS 把 `ps %cpu` 当成累计秒数求差（`perf.rs:507`），稳态结果约为 0；
  - WebView2 版本查询用的注册表 GUID（`perf.rs:700` 的 `{F1E7E7A4-…}`）不对：按微软“检测 WebView2 运行时是否已安装”的文档核对，并同时查 HKLM 与 HKCU，或者直接用 WebView2 的版本接口；
  - Chromium 下 `noteFrameGap`（`perfRecorder.ts:293`）会覆盖 longtask 统计；
  - 缺少规格要求的“各进程各自数值”。
- **S11 0.3 秒门槛没有做端到端测量**（规格 D、Q211、任务 06）
  - 前端计时在文件列表数据到达后才开始（`App.tsx:1017`），`triggerMs` 实际是 `open_worktree` 的往返时间；
  - `m9_bench.rs` 用 `ShellEnv::minimal`、跑在 debug 测试里，只量 Rust 耗时，不含环境捕获、IPC 和绘制；report 写“含环境查找”不属实。
  - **要求**：
    - 前端从用户触发（点击仓库或 worktree）开始计时，到文件列表实际绘制后的下一帧结束，分段如规格 A 所列；
    - report 改正结论，只陈述测到了什么；
    - bench 处理见 T3。
- **S12 取消与错误的时序缺陷**。收到请求 ID 之前点“取消”不起作用；错误事件早于请求 ID 到达时，按钮一直停在“取消”，**提交信息框被清空**。**要求**：取消在发出请求前后都有效；任何错误都恢复生成前的内容、复位按钮。补前端可验证的状态处理，Rust 侧补“请求 ID 先注册再发请求”的回归测试。
- **S13 规格 Testing Decisions 中缺少的测试**：
  - 更改列表只勾选部分块、子模块、非 UTF-8 文件名、300 项清单上限；
  - 请求体内容（路径、`stream`、无 `temperature`、无密钥不带 Authorization）；
  - 取消后连接关闭；
  - 退出时写盘；
  - 间隔写盘断言磁盘状态。

### P3

- `%s` 取的不是严格的首行（用 `%s` 时含换行的标题会被合并），改为按首行截取。
- 对照外观的窗口标题写死了中文，改走 x18n。

### 范围外改动

无实质越界。

## Standards（规范轴）

### 违反仓库文档规范

- **T1 平台代码集中**（AGENTS 规则，Q55）
  - `m7/perf.rs:458-581`、`604-719` 在业务模块里直接写 `cfg(windows)`/`cfg(target_os="macos")` 和系统命令，应移到 `platform/` 下集中实现，并与 S3 一起处理；
  - Windows 子进程统一经过 `platform::hide_console`；
  - `environment.rs:344` 自己拼 PATH，改用 `ShellEnv::path_string`（`shell.rs:60`）；
  - 前端的 `isMac` 判断散在 `GlassButton.tsx:91`、`glassLens.ts:66`、`perfRecorder.ts:250`，改为一个平台判定入口。
- **T2 结构化错误缺少双语**（ADR-0013）。`errors.environment.stillLoading`（`commands.rs:402`、`environment.rs:375`）和 `errors.m7.measureOnly`（`commands.rs:3296`）在中英文资源里都没有，需通过 x18n 补齐。另外未使用的 `commit.ai.notConfigured/running`、`settings.ai.configured/unsavedKey`，要么用上，要么删除。
- **T3 测试写法**（local-validation 第 2 条）
  - 环境并发测试 `environment.rs:474` 写在模块内单元测试里，应移到 `src-tauri/tests/`，从会话或命令等生产入口，用 `Barrier` 安排先后；
  - `tests/m9_bench.rs` 读取固定目录 `/tmp/revia-m9-fixtures`，不存在时直接通过。应改为在测试内用 tempfile 生成 fixture，或者改成显式手动运行的测量工具（`#[ignore]`，并要求环境变量指定目录），不得在缺少 fixture 时报告通过。
- **T4 React Hooks 调用顺序**。`SettingsDialog.tsx:458` 的 `useEffect` 写在 `if (!open) return null`（442）之后，移到提前返回之前。

### 基线坏味道（判断题，返修时一并收拾）

- **Shotgun Surgery**：`Git.env` 改成 `RwLock` 后，`env.read().map(|e| e.clone()).unwrap_or_default()` 散落在 `commit.rs:1405`、`stash_lock.rs:101`、`stash_transaction.rs:140`、`git/mod.rs:256/304/518`。在 `Git` 上加一个取环境快照的方法。
- **Duplicated Code**
  - `changelists.rs:306` 的 `selection_index` 复制了 `prepare` 中建 index 的约 35 行，应抽出共用（S7 正好要复用）；
  - `ai/commands.rs` 中的 `settingsLock` 报错和 `confirmed` 判断重复；
  - `payload.rs:370` 与 `ai/mod.rs:247` 的空结果错误重复；
  - `await_environment` 与 `environment::still_loading` 重复；
  - `now_ms` 有多份；
  - `perf.rs` 中 unix 与 windows 两段进程闭包代码重复（S3 重写时消除）。
- **Mysterious Name**
  - `App.tsx:938` 的 `staged` 表示“分段加载”，与“已暂存”冲突，改名；
  - `perfRecorder.ts:81` 的 `Distribution` 与领域里的“发布形式”同名，改为 `Histogram` 一类的名字。
- **Primitive Obsession**
  - 性能记录的 `scenario`/`category` 用固定枚举（Rust 与 ts-rs 共享）；
  - AI 与环境事件的载荷用带类型的结构体加 ts-rs 绑定，不用 `json!` 临时拼。
- **Data Clumps**：`stream_inner` 有 9 个参数，把地址、模型、密钥合成一个配置结构。
- **Speculative Generality**：删除未使用的 `_request_id`、`_status`、`let _ = worktree_id`（注释与行为不符）、只有一处调用的 `run_diff(command)` 包装。
