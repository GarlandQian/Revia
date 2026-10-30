# 09：交互式 rebase 改成 Rebased 的工具栏布局

**What to build:** 交互式 rebase 对话框照本机 Rebased 1.1.22 的 Rebasing Commits：顶部工具栏 Pick / Stop to Edit / Reword / Squash / Drop 与上下移动，选中行后操作；行内只显示动作标记、说明和哈希。

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] 工具栏动作作用于所选行，首行不能 Squash / Fixup 到更早的行
- [ ] Reword 的新说明在行内或对话框中编辑，提交前可见
- [ ] 功能与 M6 现有执行路径一致（计划由 Rust 生成，不拼 Git 参数）

## 背景

M6 桌面验收（2026-10-30）：Revia 的交互式 rebase 每行一个动作下拉框，功能完整、验收通过；Rebased 是顶部工具栏（参照截图见 M6 验收记录 rebased-interactive-rebase.png）。Q131 要求布局照 Rebased，用户同意 M6 不阻断，转 backlog。
