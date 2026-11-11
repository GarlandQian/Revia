# M11 首次正式返修

唯一实施目录 `/Volumes/T7 Shield/project/Revia-m11`，分支 m11。固定已审实现 9fd386b，审查含报告 3fc111e；先记录当前 HEAD。

先读 AGENTS.md、docs/agents/local-validation.md、M11 spec.md、review.md 和 issues/01-ai-settings-models.md 的新增 Comments。按 R1–R4 与 S1–S2 完成这唯一一轮正式返修。不另派实施者或审查者、不另建 worktree、不修改 main。

业务目标仍是可靠地保存 AI 配置与自动获取模型。尤其保证真实 App 关闭/卸载设置后，旧排队保存不会和重开后的新编辑各自执行。控制器/队列归属须真实跨关闭存续，订阅和密钥输入仍按页面生命周期清理；不要用测试专有全局单例掩盖生产归属。

回归使用原有 Node 生产组件/App接线与受控 Promise，模拟实际卸载/新组件实例、旧写入待决、新编辑接替、A→B→A 后恢复模型列表、密钥意图在队列等待时立即失效、同一字符串在新页面重新输入不被旧完成回调清空。不依赖固定 sleep，不写源码字符串断言代替行为。

证据根 `/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m11-evidence-t5lri9em`，创建 repair-01 子目录；保留首轮原件。修改完集中运行受影响检查并记录真实退出码和原始 stdout/stderr；Rust 未改不重复其测试。原始 main 的 M6 CSS 加载失败已经由规划方核实，与本次无关，不修复。更新报告、任务状态并按明确路径提交到 m11。

这次返修完成后由规划方审查新增量和真实桌面验收；不合并、不推送、不打标签、不发布、不清理 worktree。
