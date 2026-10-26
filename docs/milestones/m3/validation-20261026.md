# M3 Q115 验证记录

日期：2026-10-26。功能源码固定提交 `03736a0e66b87e98abcda3d6207993db955be9b5`。本记录不代替桌面验收，M3仍为ready-for-human，未合并main、未推送或发布。

## 授权与范围

用户选中“只改代码、不测试、不启动应用”并要求“去除这个限制”（Q115）。解除编译、测试、GUI限制，不解除安全要求，不关闭系统休眠，不唤醒或要求解锁。Q114功能批次没有验证的历史保留。

自动化使用独占临时目录 `/private/tmp/revia-m3-completion.WlOGeT`，Git集成回归通过生产会话/提交/恢复入口操作真实Git；HOME、全局配置、仓库和应用数据在fixture内隔离。只在这些自建仓库配置测试身份和禁用签名，没有改用户Git配置。没有新增测试框架。

## 修复与静态复核

- 提交进程真实退出后，原阻塞读取线程可能因hook后台子进程继承stdout/stderr而无限等待EOF。现在平台模块提供限时/非阻塞读取，Git被真正回收后才按100ms或256KiB上限排空输出并释放写门；不为超时杀Git或后台进程，不改变hook运行时长语义。
- 草稿持久化顺序/失败记账/关闭前flush与提交输出归属/缓冲/终态各自收拢到单一模块；shell捕获不再占用worktree固定文件，诊断有界且继续排空；回收区维护状态发布与重试在同一锁内。
- 删除回收区详情重复状态行。首次前端编译还发现提交操作上下文初始化的无效占位，已恢复真实`OperationResultGate.begin(target)`调用并重跑受影响构建。
- 两轴审查：Spec无确定新发现；Standards为1项P1及1项重复文案启发，均已修复，聚焦复核未见新确定问题。Windows分支只静态阅读，不能视为平台编译通过。

## 已执行结果

| 检查 | 结果 | 原始证据目录（归档内） |
| --- | --- | --- |
| `pnpm install --frozen-lockfile --offline` | 成功，安装到M3工作树；终端输出保留在会话，未伪造文件日志 | 会话安装输出 |
| `pnpm build` | 最终退出0；含tsc与Vite | `frontend-build-r2`，另见固定SHA的`release-app` |
| `pnpm lint` | 退出0；7条既有未使用变量警告，未改无关测试消警 | `frontend-lint` |
| Node现有及新增回归 | 251通过、0失败、0跳过（含8项草稿/输出回归） | `node-regressions` |
| M3真实Git回归 | 12通过，已包含于完整Rust总数 | `rust-commit-flow` |
| 完整Rust回归 | 304通过、0失败；1项专用桌面fixture准备入口按原有ignore未运行 | `rust-regressions-r2` |
| 长测驱动离线回归 | 46通过；不代表实际长测 | `bench-driver-regressions-r2` |
| ts-rs绑定生成、清理、再类型检查 | 成功；新增UpstreamRead生成物，CommitDraft仅注释变化 | `strip-bindings`、`frontend-types-generated` |
| `git diff --check` | 退出0 | `diff-check` |
| 固定SHA的macOS arm64 Release.app | 离线构建退出0，跳过发布签名，仅本地验收产物 | `release-app` |
| 固定Release只读WebView冒烟 | 进程退出0、`ok=true`，访问7项文件，视图往返目标行3322保持 | `smoke` |

新增真实Git回归覆盖：初始提交只提交index、多行Unicode、精确revision草稿清除/保留、外部index/操作变化拒绝、暂存删除与intent-to-add、已发布amend确认绑定、无差异merge、快照落盘失败阻止启动、hook失败真实after与工作区恢复不改index、局部全跳hooks且配置不变、后台继承管道不持有写门、UTF-8分片与大输出有界、linked worktree草稿和写锁隔离。shell捕获与维护状态各有必要回归。

首次失败原件未覆盖：`frontend-build`退出2；`rust-regressions`退出101（旧测试match漏CommitSnapshot，未跑完套件）；`bench-driver-regressions`退出1（模拟路径未canonicalize）。前端占位修复后仅重跑受影响构建；两处测试fixture分别补枚举分支、统一真实路径，不放宽生产校验，随后对应检查通过。

编译/回归先于源码本地提交执行，证据保留调用时HEAD、完整working diff及逐源码SHA256；最终生成绑定后再类型检查。Release和只读冒烟直接绑定上述固定源码提交。证据索引包含每条精确argv、时间、退出码和原始stdout/stderr摘要校验值，见[证据索引](./evidence/validation-20261026/manifest.json)与同目录`raw-evidence.tar.gz`；原始日志/patch保留原字节归档，未因空白或忽略规则删改。

Release二进制：`src-tauri/target/release/bundle/macos/Revia.app/Contents/MacOS/revia`。SHA256：`3ceb290ede17ae693aee57d75496268a48babde65b643604364b1041ab464bb6`。

## 桌面阻塞与未完成项

原生电脑操控按M3完整app路径两次读取AX状态均超时（`-10005 timeoutReached`）；按bundle ID连接又因本机多个里程碑app标识相同而歧义失败。运行清单显示本次Revia已运行，且另一个独立数据的只读冒烟进程成功；因此不能把控制器超时当作产品缺陷，也不能用冒烟替代真实桌面。

没有发出UI提交、恢复或清空操作。显式普通验收实例的数据目录尚未登记仓库；但`getApp`可能自动启动应用，无法确认每个自动启动实例都继承了诊断环境，不能据此声称所有原生实例都使用了隔离数据。结束的进程仅为此次M3构建路径的测试实例；正常应用安装、用户仓库与main工作树未做代码或Git配置变更。

- M3真实桌面交互、截图与提交/草稿/确认/关闭往返：未完成。
- 新固定版本D06、完整30分钟Q98长测：未启动，不用短测替代，不更改休眠换取验收。
- Windows编译：未执行，未借用Q97的M2有限授权推送M3。
- M2历史未验项、里程碑最终验收及合并：未冒称完成。

应用与自动化通过只证明对应范围，不代表M3全部验收通过。待真实桌面控制可用后，继续同一固定源码验收；如需改源码，更新版本和相关证据后再做性能/长测。
