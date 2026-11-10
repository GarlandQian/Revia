# M10 唯一一次正式返修

Status: ready-for-agent
固定被审源码：d8babd7bcc4829889db1ca2ca965a81fdf84d47f
唯一实施目录：/Volumes/T7 Shield/project/Revia-m10，分支 m10。

按 review.md 与 issues/01–03 的 Comments 修复 SP1–SP7、ST1–ST2。ST3 是判断性建议，可随 SP4 收拢会话创建，避免增加一层 pass-through；不另扩范围。仍按 AGENTS.md 和 local-validation 的隔离、集中验证和生产 interface 规则。已有主目录和其他目录不可改，不另建 worktree、不删除现有 worktree。

先完成全部相关修正与必要回归，再集中检查前端类型/lint/build、两个新增 Node 套件与既有受影响 Node 套件、环境并发测试及完整 Rust 测试。需要控制时序时在真正的生产等待阶段设 seam，不能把线程启动等同于已等待，也不靠 sleep。共享会话变化后完整 Rust 复核必要；无需再次机械重复八轮同一测试。

全部 stdout/stderr、命令、真实退出码和当前源码 SHA 写入原始证据；证据根使用 /private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m10-evidence-ln6xfi08 下新建独占子目录。不使用固定 /tmp 新目录或只管道 tail。旧失败日志保留。

另补齐缺失的本地交付：创建 report.md，列固定实现 SHA、修改内容、检查结果/证据路径和未验证事项；同步 01–03 自动验收项，不冒称规划方审查和桌面 done。之前 node --test tools/test-*.mjs 的八个失败是专用运行条件缺失（fixture/CSS loader）；说明范围与实际证据，不把不适用的总入口说成产品回归或完全通过。已有基线复现没有保存原始证据时明确写未保存，不能编造。

规划方目前在构建 d8babd7 的 release，为之后的桌面验收准备缓存。你只完成返修与固定提交，不自行操作真实桌面，不推送/PR/main/标签/发布。完成后停止代码写入，交给规划方增量复核。此为 Q113 唯一正式返修。
