# 0.0.4 Windows 环境测试夹具返修

Status: ready-for-review（本地检查与两轴审查通过，Windows CI 待完成）
日期：2026-11-10
基线：`b83285451ed6965f5903dc8117c12b982e01e699`

## 已观察到的问题

首轮 Windows CI 在三分钟内完成 Rust 编译，随后 `environment_concurrency` 中 `replaced_session_rejects_waiting_write`、`slow_open_does_not_block_an_existing_worktree` 一直未结束。main 旧运行被新推送按既有 concurrency 规则取消后，原始日志确认两项至少挂起约 44 分钟；规划方随后取消已知同样挂起的旧标签运行，保留原始失败。

夹具直接比较 Windows canonical/verbatim 路径与 Git 回传路径，可能跳过慢解析阶段；另一项夹具仅复制 Git 可执行文件，不能假设 Git for Windows 入口脱离安装布局后仍能运行。无效 Git 会正常触发同步解析回退，而测试在同一线程后面才抵达 Barrier，形成等待循环。日志未含复制品的实际 stderr，复制运行失败仍是有静态与官方文档支持的推断。

## 修正要求

- 只修改测试夹具，不改生产环境规则、路径校验或产品行为。
- 慢 worktree 以规范化后的同一文件系统路径识别；保留“另一个 worktree 仍可读取”的实际断言。
- Windows 用完整 Git 安装中的两条不同可执行路径触发既有会话替换；可用保留 `ParentDir` 的等价路径，先明确排除 verbatim 语义。先在实际测试环境预检两个真实 Git 路径并报告失败的退出码 / stderr。
- 两项测试在打开、阶段进入、释放和收尾失败时均有有限退出路径；不把不满足的前提转成跳过，不让主线程等待自身的后续 Barrier。
- 原 3 秒写者等待与 2 秒跨 worktree 读取门槛保持不变；夹具建立与收尾可使用独立的有限看门狗。
- 本地仅验证受影响的 `environment_concurrency` 目标；Windows 特有分支须由真实 Windows CI 验证，不以本机结果代替。

## 证据

固定实现：`ecbcee0`，仅修改 `src-tauri/tests/environment_concurrency.rs`。本地 13 项全部通过，退出码 0；Windows 分支待 CI。

本地证据：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-004-windows-fixture-i8mf6vml`。
旧 Windows 日志、固定源码与分析：发布证据根的 `ci-monitor/windows-hang-analysis.md`。

## Standards

0 项实质违规、0 项可能 smell。真实 Git、独占临时仓库、独立应用数据和生产入口保留；Windows 路径处理集中在夹具助手并复用 `platform::external_path`。阶段与完成观察有界，释放 sender 可在失败路径放行解析器；原 3 秒 / 2 秒业务门槛保留，生产代码不变。

## Spec

0 项代码偏差、范围扩张或实现错误。两条真实 Git 路径预检、规范化 worktree 身份、失败时有限退出，以及旧会话拒绝写入、新会话身份、Git 路径和环境标记断言均保留。Windows 特有分支仍以 CI 验证为准，本地 13 项通过与静态审查不替代它。
