# M9 返修 01 增量复核

日期：2026-11-08
范围：`4913afb..6c15981`（返修 01 的 8 个提交）。按 Q174，只看增量。

## 规划方重跑检查

HEAD `6c15981` 上全部通过：tsc、lint、x18n、build、`cargo check --features measure`、`--features portable,measure`，完整 Rust 测试 929 个通过、0 失败。原始日志放在规划方的 scratchpad 证据目录（`m9-verify.smsu`）。

## 结论

不通过。Pi 的正式返修只有一次，已经用掉（Q113），剩余问题由规划方接手修复。report 中有几处说法与代码不符：S9 声称的等待入口、S11 的前端计时、`stagedLoading` 改名、S13 的非 UTF-8 与部分块测试。

## 已修

S3、S4、S5、S6、S7、S8、T2、T4。T1 基本完成，`plainAppearance` 判断仍有三份。

## 新问题与未修项（规划方接手）

### P1

- **N1 macOS CPU 少算约 42 倍**：`platform/system.rs:93` 把 `pti_total_*` 当成纳秒，在 Apple Silicon 上它的单位是 mach tick，需要用 `mach_timebase_info` 换算。规划方已核实。
- **N2 macOS 子进程列表为空**：`system.rs:116` 的 `proc_listchildpids` 返回的是个数，不是字节数，代码又除以 4。另外 WebKit 子进程由 launchd 拉起，本来就不是 Revia 的子进程。规格只要求 macOS 统计主进程，所以注释和数据要如实反映这一点（`perf.rs:113`）。

### P2

- **S9（未修）**：
  - 状态、diff、日志读取在环境就绪后重试一次：没有做；
  - Q205 捕获失败的提示：界面没有显示；
  - 统一等待入口：只有两处命令加了等待，checkout、create_branch、stash 保存与应用、cherry-pick、revert、merge 等仍不等待；
  - 推迟环境更新有竞态：写操作的 guard 释放后才 `defer`，环境要等到下一次写操作结束才生效；Git 路径变化时只换了环境，没有换会话。
- **S11（未修）**：`App.tsx:1017` 的 `beginRepoOpen(id)` 没有传入触发时刻，计时仍从数据到达后才开始。
- **S12（修错）与相关问题**：
  - 错误事件早于请求 ID 到达时，原内容恢复后，又被 `onMessage("")` 清空；
  - 完成事件早于请求 ID 到达时，结果被丢弃，界面停在“生成中”；
  - `activeRequestId` 为空时，任何 ai-error 都会被接受，已取消请求迟到的错误会覆盖用户新输入的文字；
  - 拿到请求 ID 之前点取消，diff 仍会被上传（隐私）。
- **S13（部分）**：缺少四项测试——只勾选部分块、非 UTF-8 文件名（同名测试里并没有这种文件）、取消后连接关闭；退出写盘没有走退出路径。间隔写盘只断言了间隔内不写盘。
- **S1（部分）**：linked worktree 公共目录的 `info/exclude` 没有监视；`core.excludesFile` 不在监视范围内，对应的比较是死代码，也没有测试。
- **S2（部分）**：
  - 回归测试自己写了丢弃记忆的逻辑，没有经过生产入口；
  - 没有记忆时会运行真实的登录 shell；
  - 失效路径同步解析一次之后，又在后台再跑一次登录 shell。
- **P3 首行（修错）**：`--format=%B%x00` 每条记录后还有换行，最近提交标题只剩 1 条。

### P3

- 对照窗口标题按系统语言手写，没有走 x18n，也不跟随界面语言（Q164）；
- `GlassLayer.tsx:26` 在 hook 之前提前 return；
- 冷打开时多跑一次 `git --version`；
- 快照失败应判 `INVALID_HANDLE_VALUE`，代码判的是 `is_null`；
- ai-config-changed 监听可能泄漏，且每次窗口获得焦点都读一次钥匙串；
- `prepare` 里执行了两次 read-tree；
- 残余坏味道：`confirmed` 判断重复、`await_environment` 与 `still_loading` 重复、多份 `now_ms`、`App.tsx` 的 `staged`、`scenario`/`category` 仍是 String、`json!` 拼事件载荷、死代码 `Verdict::is_refresh` 与 `_amend`。

## 规划方接手结果

提交 `5f9a897..bf29ccb`，共 5 个。

| 项 | 处理 | 提交 |
| --- | --- | --- |
| N1、N2 | macOS 的 `pti_total_*` 按 `mach_timebase_info` 换算；`proc_listchildpids` 的返回值按个数处理；注释写明 macOS 的 WebKit 进程不是子进程。补了真实 CPU 增量和单个子进程的回归测试 | `5f9a897` |
| P3 首行 | `log -z --format=%B` 取每条记录的首行；测试改为断言两条标题都在 | `5f9a897` |
| S12 及相关 | 请求 ID 由前端生成，后端先登记再整理内容，发请求前再查一次取消；取消早于登记时直接放弃，不上传；前端事件按自己的 ID 严格过滤，迟到的错误不覆盖新输入；不再每次聚焦时读钥匙串。补早到取消、上限、ID 校验测试 | `919287e` |
| S2、S9、T3 | 见下方说明 | `bdf08bb` |
| S11 | 计时从最近一次点击或按键开始（1 秒内），到文件列表绘制后的下一帧结束；空仓库也上报 | `ffbff49` |
| S1 | 监视公共目录的 `info/`，以及 `core.excludesFile` 所在目录（未配置时用 Git 默认的 `~/.config/git/ignore`）；仓库外的 excludesFile 按规则文件处理。补测试 | `ffbff49` |
| S13 | 补测试：只勾选部分块（核对用户 index 不变）、Git 输出中的非 UTF-8 路径、取消后连接关闭（服务端读到 EOF）、间隔写盘落盘、经 `AppState::on_exit` 的退出写盘 | `bf29ccb` |
| P3 | 对照窗口标题改由前端经 x18n 设置并跟随界面语言（新增 `core:window:allow-set-title`）；`GlassLayer` 的 hook 先于提前返回；`prepare` 只 read-tree 一次；删除 `_amend`、`Verdict::is_refresh`；`staged` 选项改名 `progressive` | `bf29ccb`、`ffbff49` |

S2、S9、T3 的处理：

- **统一等待入口**：所有写操作在 `acquire_write_lock` 处统一等待完整环境。
- **安装顺序**：后台解析得到的环境先装进会话（Git 路径变了就换会话，旧会话标记为已取代），再宣布就绪。安装只在持有 `writing` 锁且没有写操作时进行，取代原来的推迟机制。
- **槽位换代**：槽位按代号区分，重新加载环境后，旧的后台结果作废。
- **记住的 Git 不可用**：直接用打开会话时的版本检查判断，不再额外运行一次 `git --version`；之后只同步解析一次登录 shell。
- **不再跑真实 shell**：没有记忆时协调器不再自己运行真实的登录 shell，测试中也不会运行。
- **就绪后的处理**：环境就绪后前端重读一次，覆盖 Git 路径变化和就绪前失败的读取两种情况；捕获失败时按 Q205 弹出提示。
- **测试**：并发测试改为经过 `AppState::worktree_session` 与 `acquire_write_lock`，用替身解析器（`AppState::with_shell_resolver`）。

### 保留未改（判断题）

`confirmed` 判断重复、多份 `now_ms`、性能场景仍用字符串、部分事件载荷仍用 `json!` 拼。这些不影响行为，留作后续清理。

### 仍未验证

- Windows 编译与真机：等推送 `m9` 后由 CI 和用户测量；
- macOS 桌面上打开与切换仓库的实际耗时：由规划方在可见窗口中复测；
- 非 UTF-8 文件名的端到端测试：APFS 不允许这种文件名，在 macOS 上会明确标记为跳过；解析层已有字节级测试覆盖。
