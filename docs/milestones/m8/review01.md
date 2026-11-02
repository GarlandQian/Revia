# M8 首轮两轴审查

日期：2026-11-02
固定范围：`git diff b9cf807...7ea69a3`；提交 e7f4c25、a30b435、7ea69a3。
Pi 最终交付为 6ea3653；额外代码提交 af869f9 仅为便携 data/webview2 增加可写探测，规划方另行静态核对，该增量不消除下列发现。

## Standards（独立代理原文）

S1 · P2：shell.rs:318 将 profile 加载异常当作捕获成功。windows_env.rs:43–47 捕获异常后仍转储环境，resolve 随后固定 used_fallback: false；WorktreeSession::open_with_options 只按该标志生成环境检查，并未传出 diagnostics。因此 profile 抛错或被执行策略阻止、但 PATH 仍存在时，用户不会看到失败，也不会回退进程环境，hooks 可能使用错误 Node 环境。违反 docs/README.md Q205“失败或超时显式提示并回退进程环境”。应将加载失败显式返回并走可见回退。

S2 · P3，记 backlog：distribution.rs:241 原生启动错误直接显示 error.message，主错误始终为中文，附加提示又同时拼接中英文；新增的 errors.distribution.* 英文资源不会参与这条生产路径。英文系统缺少 WebView2 或便携目录不可写时，提示不遵循界面语言。违反 Q164、docs/ui-direction.md“界面语言”及 ADR-0013 的按当前语言呈现错误原则。启动前应使用原生呈现层按已保存语言或系统语言格式化同一错误语义。

固定审查 b9cf807...7ea69a3，只读静态核对；未运行测试，Windows 行为未实测。未发现其他有实际影响的规范违规或代码异味。

## Spec（独立代理原文）

X1 · P1 · Windows 构建必然失败。规格：“仅 Windows x64 MSVC”。shell.rs:288、296、307 调用 fallback_windows 时向 timed_out 传入 None，而该参数在 :339 定义为 bool。任何 Windows 目标构建都会产生类型不匹配，安装版、便携版均无法产出。此项由源码确定，与缺少 Windows 实机无关。

X2 · P2 · profile 失败被当作成功，未回退或展示诊断。规格：“失败、超时或策略阻止时显式标出回退及诊断。” windows_env.rs:43–46 在 profile 抛错或被执行策略阻止后仅写 stderr，继续转储环境；shell.rs:317–322 随后标为 used_fallback: false。session.rs:367–384 又未将该诊断带到前端。普通用户遇到 profile 加载失败时仍看到环境检查成功，hooks 可能使用缺少 fnm 初始化的环境。

X3 · P2 · 安装版隔离验收仍写正式 WebView2 数据。规格：“测量和验收隔离仍显式控制”；测试要求“独立应用目录”。distribution.rs:94–98 只为便携版设置覆盖后的 WebView2 目录，安装版即使指定 REVIA_ACCEPTANCE_DIR 或测量目录仍返回 None；lib.rs:112–115 因而不设置窗口数据目录。锁定 Tauri 2.12.4 的 manager/webview.rs:559–565 会退到真实 app_local_data_dir()，使安装版验收写入正式缓存、存储并破坏隔离。

未发现确定的范围扩大；无另列 P3。仅只读审查，未运行构建或测试，未将 Windows 尚未验证本身计为缺陷。

## 规划方生产入口复核

R1 · P2 · 环境捕获的 5 秒超时仍可能卡住。windows_env.rs:170–194 的 reader 使用阻塞 pipe，主进程 wait_timeout 到期或结束后仍无期限 join；profile 启动的子进程如果保留 stderr 写端，杀掉 PowerShell 也不能使 reader 结束，open/reload 环境将永远不返回。pipe.take(8192) 还会在输出较多时提前关读端。应复用现有可停止/非阻塞输出读取边界，在进程回收后限定读取收尾，持续排空但仅保留上限内容；测试真实子进程保留写端/大量 stderr，断言捕获有限时间返回并准确标出 timedOut。

R2 · P2 · Windows 最后一步未告知安装会退出/重启。UpdatePanel.tsx:83–85、166 的按钮仍是“立即安装”，App.tsx:1760 的生命周期无在途写/编辑/终端时直接执行；Windows updater 的 install 会自动退出，因此用户没有看到明确“安装并重启”的最终选择，原有 macOS 安装后再确认重启的语义不适用于 Windows。按规格 E，在 Windows 最终安装前用明确文案/确认把退出重启告知用户，取消不得执行安装，沿同一草稿/写入/终端协调；保留 macOS 现有流程。用现有 Node 生产状态机/入口回归验证。

## 处理

两轴独立保留：Standards 2 项（P2 1、P3 1）；Spec 3 项（P1 1、P2 2），S1 与 X2 重复。规划方另有 R1、R2。X1/X2/X3/R1/R2 集中交 Pi 正式返修一次（Q113），S2 记 backlog 17。返修后只审增量；Windows 编译和真实桌面未验证事实保留。
