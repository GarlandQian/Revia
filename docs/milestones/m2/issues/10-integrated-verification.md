# 10：完整异常矩阵与实施报告

**What to build:** 将已完成的功能合起来验证，证明真实 Git 与持久数据在正常、过期、失败和中断情况下符合规格，交付可供规划方复核的结果与桌面验收方法。

**Blocked by:** 07, 08, 09, 13

**Status:** ready-for-agent

依据：[M2 规格](../spec.md)测试决策 T01–T14、D01–D08；Q92、Q99。

- [ ] 各功能的已有真实 Git 验证集成通过，覆盖规格自动化矩阵；不是到本任务才为前面功能补上全部正确性验证。
- [ ] 测试自行创建隔离仓库与应用数据；时钟、外部修改、保存失败、阶段中断可复现，缺 fixture 不会直接成功。
- [ ] 同文件部分暂存、hunk、重命名、二进制、权限、类型限制、旧响应、恢复冲突、身份与清理没有集成回归。
- [ ] 完整 Rust 测试及前端类型检查、lint、build 通过，不新增前端测试框架。
- [ ] 最终集成代码重新取得 Windows x64 实际编译结果，不以任务 09 的早期成功代替。
- [ ] 报告列出构建提交、检查命令与实际结果、CI 证据、已知遗留、真实桌面验收方法；未测和待规划方验证项明确标识。
- [ ] 为最终桌面验收提供可见窗口的 release 应用、可重复 fixture 与长测方法；不以短测或隐藏窗口数据代替桌面结果。
- [ ] 审查发现的问题回写到对应任务的 Comments 并修复，不能用报告改变已确认决策或扩大范围。


## Comments

### 2026-10-24 / Codex：保留未归因刷新测试观察与图标集成

- 任务04修复期间，Pi在包含`41be300`能力gate的代码上观察`refresh_isolation::t13_concurrent_refreshes_never_publish_stale_content`间歇“指纹不在预计算表中”。不能凭测试文件未改断言既有或无关；refresh确实经hunk_actions调用gate。Pi已在`764d13e`限定文档，未改refresh代码或放宽断言。
- 独立原样源码副本`a74e9d2`和`41be300`各3次exact均通过；首次副本缺dist的6次构建失败、未进入测试，不计成测试失败。最终`764d13e`双平台完整CI通过。原始结果、脚本和12份日志在`../evidence/task04-final/refresh-audit/`；Paseo摘录仅时间线证据，不冒充测试stdout。集成时复核该观察，若再现则先保留失败证据、定位真实Git读写一致性，不靠重试/忽略未知指纹/放宽断言通过；若未复现则如实报告，不宣称修复。
- 用户已确认主目录自制图标、`Assets.car`、`Revia.icon`、导出脚本与配套配置属于本次修改。规划方后续精确纳入m2（不覆盖用户原件），最终构建/CI需涵盖图标资源，不能用任务04未包含图标的二进制冒充最终版。

### 2026-10-24 / Codex：用户图标固定增量静态复核

`bf658f7...5d9b781` 的独立 Standards / Spec 均无新增发现；24份源文件与清单字节/SHA一致，资源容器静态检查通过。详见 [图标两轴复核](../icon-review.md)。未运行资源导出器、完整打包或GUI；04固定二进制不含图标，本次图标还需后续新版Release实际资源/系统展示与对应双平台CI，不能由静态通过代替。

### 2026-10-24 / Codex：任务05顺带修改 t13 的归因边界

Pi 在 `6071b59` 将并发写入从截断写改为临时文件后 rename，并声称所有 t13 历史间歇失败已归因。现有原始失败日志仅直接证明本次出现 Git“读入不完整 / cannot hash / unable to read”；这与旧记录的“指纹不在预计算表中”不是同一直接现象，不能据一次新失败关闭所有历史观察。实际编辑器和工具既可能直接保存也可能原子替换，不能笼统称直接写入是不真实场景。

原子替换场景可保留用于验证已完成版本的发布顺序，但不能替代直接保存场景。任务05返修要求保留两类有意义的验证：真实Git短暂读取失败不得伪造快照/覆盖较新状态，保存稳定后可重新读取；正常成功快照仍需满足指纹与版本一致性。既有断言和失败证据保留，不以简单忽略未知指纹、放宽断言或连续重试充当定位。现有归档只独立核到修后完整一次通过；未找到所称5次及敏感性对照的逐次原始输出时，按实施方报告而非已核实证据处理。日志见 [任务05第三轮证据](../evidence/task05-review03/manifest.json)。

### 2026-10-24 / Codex：73791d6 的刷新一致性增量复核边界

第三轮返修保留直接截断保存和原子替换两类负载；新增stat变化检查发生于快照版本发布前，稳定后的最终内容有断言。现有日志已经包括真实“未知指纹”失败和四份移除guard后的失败，对特定读取一致性缺口的证据较上一轮更强；8份检查前源码hash与73791d6一致，原始完整223项通过可核。

没有在指定日志目录找到“181种组合/16不一致”的探测源码和原始输出，也没有40次/15次完整逐次结果；这些数字仍为实施方报告，不用其关闭所有历史观察。stat只覆盖已列出的非staged路径及diff读取窗口，不声称完整内容原子性证明；直接保存测试对错误分类和失败时内部版本未推进的验证也有边界。当前无新确定刷新违规，不继续扩大本任务刷新改动；最终D06和长测仍按原规格执行。原始日志见[任务05第四轮证据](../evidence/task05-review04/manifest.json)。


### 2026-10-24 / 规划方：整批首轮交付准备缺口

固定`3199b29...82f392b`，Spec轴**SP-B01-10-1 / P3**：本批派发`/private/tmp/revia-m2-implement-batch-paseo-20261024.md`第3项明确要求“完成10现有功能矩阵/检查与报告准备”，`paseo-batch.md:35`却整体延期为未开始。应在图确认等待期交现有T01–T14与D01–D08映射、检查/未测边界及最终桌面步骤，而非提前宣称最终10通过。此项是准备交付遗漏，不以未完成13为由开生产功能缺陷。Standards轴无此独立发现。

本批Node105、Rust264/1ignored与前端检查exit0，六源码SHA匹配82f392b，27份原件见[manifest](../evidence/batch01-review/manifest.json)。两轴总计Standards6（P1一/P2五）、Spec6（P1一/P2四/P3一），各轴独立保留，不合并成计数；未Release/桌面/D06/最新CI/长测。核心关闭/审阅及静态图范围需原Pi成批修复，不提前done。

### 2026-10-25 / 规划方：整批返修交付准备复核边界

固定 `51fe663...be02cb7`，07 共享批次资格静态闭合；08 的生产关闭流程仍不通过，见其最新两轴 Comments（Standards 6 / Spec 5），不能据 helper 绿色放行。

现有 `10-integration-matrix.md` 属于历史准备交付，不是最终通过。它指向的 `/private/tmp/revia-batch02-8LEw43` 当前不存在，且写“见文末受测提交”却无该段及最终 SHA，因此本轮无法独立核对这批原始检查；不据此指称造假，不把无法核对的数字升级为已验证。后续矩阵应关联新的固定源码、实际归档原件/哈希清单，区分实施检查与规划方验收。

当前视觉基准为 Q107/rev3，`m2-ui` 已实现但尚未合入 `m2`；rev1、半透明侧栏和“美化未实现”等历史描述不能作为最终验收依据。Pi 本轮完成08返修及现有功能矩阵/报告准备，不编辑尚在独立验收的 `m2-ui`；最终集成源码仍须在合入UI后重新固定验证，旧结果不替代新版 Windows 编译、完整真实桌面、D06 和主动30分钟Q98长测。任务10、11及M2均不提前done。

### 2026-10-25 / Pi 实现：review_marks 子模块 fixture 提交身份（定点修复）

固定起点 `2a51b160d1c3871b9c67fee81ae6f082910bc1f8`（分支 `m2`，工作树干净）。
只改 `src-tauri/tests/review_marks.rs`；未改生产代码、未改用户 Git 配置、未动其他任务状态。

#### 真实失败与原因

固定 `2a51` 的 Compile Check（run 40172283582，Windows job 121105479132）中
`review_marks` 29 项有 1 项失败：`deinitialized_submodule_is_not_confirmed_and_cannot_be_claimed`，
在 `git commit -m two` 处报 `Author identity unknown`（断言位置 `review_marks.rs:860`），
Windows Release 编译随后被跳过。原始完整日志由规划方保存于
`/private/tmp/revia-m2-ui-merge-yk2t6656/windows-job-121105479132.log`。

原因：`f.repo/sub` 是 `git submodule add` **克隆**出来的仓库，
仓库本地 `user.name` / `user.email` / `commit.gpgsign` 不随 clone 继承。
父仓库与 `sub-source` 里设置的本地身份在克隆体里都不生效。
不显式设置时，Git 会尝试按系统用户名/主机名自动推断：
macOS 上能推断（伴随警告），Windows CI 上直接拒绝并报 `Author identity unknown`。

#### 修复（最小、仅测试）

在子模块克隆里显式确定身份并禁用签名，再创建第二个提交：

```rust
let sub = f.repo.join("sub");
run(&sub, &["config", "user.name", "Sub"]);
run(&sub, &["config", "user.email", ...]);   // 与 source 相同的字面量
run(&sub, &["config", "commit.gpgsign", "false"]);
fs::write(sub.join("f.txt"), b"two\n").unwrap();
run(&sub, &["add", "--all"]);
run(&sub, &["commit", "-m", "two"]);
```

未使用 `cfg(windows)` 跳过、`#[ignore]`、放宽断言或伪造 Git 结果。
安全断言全部保留：gitlink 改动仍出现在改动列表；deinit 后版本无法确认、
旧标记失效且不能新增认领；重新 init 不恢复旧标记。

#### 敏感性验证（复现 Windows 的失败条件）

Windows 失败的实质是「Git 不能自动推断身份」。用 `user.useConfigOnly = true`
在 fixture 的隔离全局配置里复现该条件（不改任何真实用户配置）：

- 移除上面三行 `config` 后运行同一测试 → **失败**，
  `panic` 内容为 `git ["commit", "-m", "two"] 失败：作者身份未知`
  （Git 的中文本地化输出，等价 Windows 的 `Author identity unknown`），
  位置 `review_marks.rs:860`。对应原件：`sensitivity/pre-fix-useconfigonly.out/.err/.exit`
  （修前代码 + fixture 内部全局配置 `user.useConfigOnly = true`，退出 101）。
- 恢复三行后**同一条件**（`user.useConfigOnly = true`）通过 → **实施方此前报告过，
  但当前留存原件不足以独立确认**。`sensitivity/` 里没有修复版 + 相同
  `useConfigOnly` 条件的运行输出；最终 31 项通过那次使用的是 fixture 自建的
  **空**全局配置，不能充当该正对照。此处不据缺件否认该次运行曾发生，只如实标注
  证据不足。

`sensitivity/pre-fix.out/.err/.exit`（退出 0、测试通过）是**无效对照**：
该次试图从外层传入隔离配置，但 fixture 在 `Command::env_clear()` 后自建空
`.gitconfig`，外层 `GIT_CONFIG_GLOBAL` 被覆盖，所以修前代码也能通过；
它不能证明任何身份推断条件。原件保留，仅作历史。

修复版快照：`sensitivity/review_marks.rs.fixed`。运行后已把测试文件恢复为修复版，
SHA 与最终检查一致（`a2ff3004f9458a8aeab61571bddd0a00f199caaf69d90776d7b7ecca67d4f9f9`）。

#### 检查（最终一次集中验证）

日志子目录：`/private/tmp/revia-ci-submod-lvvaQ9/run-final-ZUIsp1`（本轮唯一独占目录；`record2.py` 执行时逐条记录
真实 argv/cwd、显式非敏感环境（隔离 `HOME` + `GIT_CONFIG_GLOBAL` + `GIT_CONFIG_NOSYSTEM=1` +
显式 `CARGO_HOME` 以复用离线缓存）、start/end、exit、完整 stdout/stderr，汇总在 `invocations.json`）

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `cargo test --offline --locked --test review_marks` | 0 | 31 passed / 0 failed |
| `git diff --check` | 0 | — |

- 测试源码检查前/后 SHA 逐份一致：`a2ff3004...7d4f9f9`。
- 无生成绑定改动（`src/types/bindings` 未变），未运行绑定规范化。
- 只跑受影响的 `review_marks` 套件；未重跑前端或全量 Rust。

#### 证据边界（如实记录，不补造）

- 本轮最初用 `record.py` 运行过一次 `review_marks`（31 passed）并生成了
  `00-*.out/.err/.exit` 与 `invocations.json`，随后被我用 `rm -f` 删除，
  **这些原始流已不可恢复**，不从记忆重造；该次运行的完整原件缺失。
- 之后改用 `record2.py` 在隔离 HOME 下重跑并保留全部原件（即上表）。
- 另有一次在未隔离 HOME 下的 `record2.py` 尝试因 `--offline` 找不到 `notify`
  而失败（退出 101）——此事**仅为此前实施方描述，对应原件现已缺失、无法独立核对**。
  现存根目录与 `run-final-ZUIsp1/` 的两组 `00-*.out/.err/.exit` 均为成功（exit 0），
  两个 `invocations.json` 记录的环境也都显式设置了隔离 HOME 与 `CARGO_HOME`，
  不支持该失败描述。这里不补造输出或命令，也不把成功日志当作失败原件；
  该次失败曾发生与否以实施方报告为准，不作独立证据。

#### 未测边界

- **本机（macOS）通过不等于 Windows 通过**：Windows 必须在规划方再次推送 `m2`
  后的实际 CI 中验证。
- 未构建 Release、未启动 GUI、未跑 D06/CI/长测、未推送/合并。
- 旧 CI 失败原件由规划方保留，本任务不改写。

### 2026-10-25 / Pi：两轴审查后的交付记录更正（仅文档）

独立 Standards / Spec 两轴对 `d952a857b3faef2a4101ab5a6582217bd5a7c8e0` 的复核结论是
**代码无确定缺陷**（fixture 只配置自建临时子模块的本地身份与禁签名，仍走真实 Git，
gitlink/deinit/reinit 断言全部保留），但指出本任务交付记录有两处超出留存的证据。
本次只更正 `docs/milestones/m2/issues/10-integrated-verification.md`，
**未改生产代码、未改测试源码、未重跑任何测试**。

审查来源：

- Standards：`/private/tmp/revia-ci-fixture-standards-1PUoKq/standards.md`
  （ST-CI-1 / P2 正对照结论超出证据；ST-CI-2 / P3 失败日志"已保留"描述不实）
- Spec：`/private/tmp/revia-ci-fixture-spec-btOIT5/spec-review.md`
  （SP-CI-1 / P3 敏感性通过陈述超出原件；SP-CI-2 / P3 失败原件归档描述不准确；
  要求缺失/部分 0、范围扩张 0、错误实现 2，最高 P3）

更正内容：

1. 「恢复三行后同一条件 → 通过」改为**实施方此前报告过、但当前原件不足以独立确认**；
   并明确 `sensitivity/pre-fix.out/.err/.exit`（exit 0、通过）是**无效对照**——
   该次外层配置被 fixture 的 `Command::env_clear()` + 自建空 `.gitconfig` 覆盖，
   修前代码也能通过；`pre-fix-useconfigonly.out/.err/.exit` 才是修前 + fixture 内部
   `user.useConfigOnly = true` 的真实身份失败原件（exit 101）。
2. 未隔离 HOME 下 `notify` 离线失败（101）改为**仅实施方描述、原件现已缺失、
   无法独立核对**；现存两组 `00-*.out/.err/.exit` 均为成功（exit 0），
   两个 `invocations.json` 都显式设置了隔离 HOME 与 `CARGO_HOME`，不支持该描述。

保持不变（已核）：最终 `cargo test --offline --locked --test review_marks`
**31 passed / 0 failed**、`git diff --check` 退出 0、测试源码前后及提交后 SHA 一致
（`a2ff3004...7d4f9f9`）、无生成绑定改动；**Windows 仍需规划方再次推送 m2 后的实际 CI**。

历史失败与无效证据**全部保留未删**：根目录与 `run-final-ZUIsp1/` 的成功原件、
`sensitivity/pre-fix.*`（无效对照）、`sensitivity/pre-fix-useconfigonly.*`（真实失败）。
本轮未删除任何已有文件。


### 2026-10-25 规划方：最新集成原件与 Q98 失败

m2-ui→m2快进已完成。实际Release仍来自2a51b160；d952测试修复、f10证据更正和9434e7a模型决定不冒称新构建。最新[CI40174661706](https://github.com/GarlandQian/Revia/actions/runs/40174661706)对应f10，两端前端检查/完整Rust/Release编译成功：macOS271、Windows243通过，各0失败/1既有ignored。旧Windows失败、两轴发现和历史缺件边界保留。[矩阵](./10-integration-matrix.md)由Pi草稿按终态校正，104个不同代表性测试名已核源码存在，平台套件数不作各T编号数量。

**Status保持ready-for-agent。** 最终Q98首轮78.54墙秒因2179ms帧间隔中断；第二轮1223.25墙秒因常规WebContent vmmap255中断，无最终frontend.json。18阶段即时检查/END均通过，但完整checkpoint缺失使最终无效；不能补造或改松。Rust现有34样本maxRise82MiB超过+10，负斜率不取消，缺setupSeconds的影响单列，不认定泄漏。

已交原Pi按Q109首选GLM-5.3-Flash只读诊断，再评审最小修复；Rust振幅另行定位。不静默重试挑样本、不改暖机/阈值/负载、不拼接时长、不sudo采样。D06四组及两轮Q98见[最终测量](../final-measure-20261025.md)，其余归档见[B10记录](../batch10-repair-validation.md)。任务11/M2未通过，未合main，未启动M3。

### 2026-10-25 / Pi：Q98 测量失败留证修复（不改验收规则）

**这是测量失败的可观察性与证据保全修复，不是 vmmap 底层错误或 Rust 内存增长已修好。**
两轮失败原件与判定保持原样，未改阈值/暖机/采样间隔/负载/窗口，未新增长测。

**改动范围**（3 文件）：

- `tools/bench-long-running.py`
  - 新增 `CommandEvidence`：采样外部命令逐条留证（实际 argv、PID、起止、退出码/异常、
    完整 stdout/stderr；timeout 另存 `partialStdout/partialStderr`）。
  - `launchctl` 归属证据**只保留 WebKit services 行**，不保存原始 stdout（可能含进程环境
    凭据）；每条记录写入 `redaction` 说明裁剪范围与原因，不冒称未裁剪原件。
  - 常规采样失败时保存**已取得的部分数据**（`partialCandidates` + `*.evidence.jsonl`），
    含失败时 `proc.poll()`、已归属 services、失败原因；不再随异常无痕丢失。
  - 清理动作逐项留证：`pollBeforeCleanup`、`terminateSent`/`terminateSentAt`、
    `waitAfterTerminate`、必要时 `killSent`/`waitAfterKill`、`pollAfterCleanup`；
    清理前已退出时记 `terminateSent:false`，**不把「若活着则清理」写成「确实被杀」**。
  - 新增 `ProgressLog`：读取前端 `[SOAK_PROGRESS]` 增量记录。未闭合尾行不解析、
    重复/损坏/跨运行/锚点前记录/第二锚点/未知类型均计入 `problems`。
  - 阶段最终校验新增 `checkpointSource` 与 `authoritative`：只有真实最终前端报告
    才是权威来源；仅进度记录时标为 `progress-log` 且 `authoritative:false`，
    `q98MemoryVerdictAuthoritative` 为 false，判定只作诊断。缺失/损坏记录仍 fail-closed。
- `src/App.tsx`（仅原 soak 测量分支）
  - setup 完成后经 `ipc.debugLog` 写运行锚点 `[SOAK_PROGRESS] {kind:"run"}`（含
    `runId`/目标秒数/输出路径/setup 秒），不使用 `writeMeasurement`（它会 500ms 后退出）。
  - 每个 checkpoint 闭合后立即写 `{kind:"checkpoint"}` 完整描述：cycle/file/view/
    pressure/真实 `startedAt`+`endedAt`/`hiddenDuringHold`/当时 checkpoint 与可见性
    暂停秒数/当时 cycles、文件切换、视图切换、滚动、跳转计数。**未闭合段不写**。
  - 增量记录与既有 BEGIN/END 并存，后者仍是驱动判定窗口闭合的依据。
- `tools/test_bench_long_running.py`：复用既有 unittest，新增 12 项（20→32），
  覆盖命令留证、timeout 部分输出、归属凭据不外泄、归属失败不等于成功、
  进度记录只暴露已闭合唯一段、损坏/重复/跨运行/未闭合/未知类型 fail-closed、
  缺进度文件如实报告、进度不得替代最终报告（含 `authoritative` 区分）、
  仅进度记录时整轮仍失败、部分样本保全、清理动作区分 terminate 与已退出。

**未解决（单列待定位）**：Rust 热身后常规 footprint 最大抬升 82.0 MiB 超过 +10 MiB
门槛，斜率 −0.9827 MiB/min 在上限内，序列明显回落（最小 116.0）。现有日志不足以区分
存活分配与 allocator 高水位，本轮**未**改缓存/分配策略、未挑首点、未延长暖机、
未减少负载、未用同阶段样本替代常规样本。

**检查**（对已提交源码 `0a2d842` 的一次集中运行；每条命令的真实 argv/cwd/起止/
退出码/完整 stdout+stderr 由 `record.py` 在**执行时**落盘，汇总在
`final-checks-20261025-184247/invocations.json`）：

| 命令 | 退出码 | 结果 | 原件 |
| --- | --- | --- | --- |
| `python3 tools/test_bench_long_running.py` | 0 | 32 passed | `01-python-unittest.*` |
| `pnpm exec tsc -b` | 0 | — | `02-tsc.*` |
| `pnpm lint` | 0 | 仅既存 warning | `03-lint.*` |
| `pnpm build` | 0 | — | `04-build.*` |
| `git diff --check HEAD~1 HEAD` | 0 | 无输出 | `05-diff-check.*` |

受测源码 SHA-256 在检查前后与提交后三次一致（`tested-source-sha256-before/after.txt`），
HEAD 与工作树状态在检查前后未变（`head.txt`/`head-after.txt`、`status-before/after.txt`），
29 件原件逐份 SHA 见 `manifest.json`。

**证据边界**：本轮更早一次同类检查的原始流只在会话内、未落盘，因此**不称其已留存**；
上表为重跑后的真实原件，路径
`/private/tmp/revia-q98-evidence-repair.Qvsbwg/final-checks-20261025-184247`。

未重跑全量 Rust/无关 Node，未做 GUI/Release/D06/CI/长测。

**Status 保持 ready-for-agent**，等待双轴复核。
