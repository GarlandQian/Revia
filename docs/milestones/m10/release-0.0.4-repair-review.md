# 0.0.4 输出读取返修审查

日期：2026-11-10
固定范围：`6051ece...c6868d8`
规格：[发布返修](release-0.0.4-repair.md)

## Standards

通过，0 项标准违规、0 项实质性 smell。Git 执行继续集中于 Rust；观察者仅传入单次调用，生产入口传 `None`，有明确的时序回归需求。集成测试使用独占 tempfile、规范化路径、隔离 HOME / Git 配置、真实 Git 与 Barrier，符合本地验证规则；不访问用户仓库或凭据。

## Spec

0 项代码问题，未发现遗漏、范围扩张或错误实现。读取开始前捕获退出状态，`Interrupted` 单独重试；保留既有 100ms 排空上限。回归复用生产进程与管道收集路径，预期路径由独立真实 Git 调用取得，红阶段丢失 stdout / stderr，绿阶段三项通过。

审查时完整 Rust 命令退出 101：`m6_flows::user_refresh_during_autostash_invalidates_the_unstarted_history_write` 等待超过 15 秒，需继续核对并保留失败记录。静态审查通过不替代验证；也不据新增回归推断首轮两个 CI 失败一定同根因。后续验证结果记入[发布记录](release-0.0.4.md)。

两轴合计：Standards 0 项，Spec 0 项；无剩余代码意见。
