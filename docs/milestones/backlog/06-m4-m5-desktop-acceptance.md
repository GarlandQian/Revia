# 06：M4、M5 未做完的桌面验收

**What to build:** 在真实桌面上补完 M4、M5 合并时未做完的验收，发现的问题按各自里程碑的规格修复。

**Blocked by:** None (can start immediately)

**Status:** needs-triage

- [ ] 弹层“无障碍树里有、截图看不见”的偶发问题：复现并定位，或连续多轮确认不再出现（M5 报告 22.13、22.17）
- [ ] 四套颜色主题的完整截图检查
- [ ] 与 Rebased 1.1.22 的完整对照（日志、分支、远端同步、提交、合并页签）
- [ ] 剩余安全工作流：强制推送、全部退出保护入口、M4 合并写入三个入口的全局保护（`6553772`）
- [ ] M4 软换行与左右对照修复（`e9344f7`）的桌面确认
- [ ] Windows 仅编译检查

## 背景

用户于 2026-10-28 认为 M5 可以了，M4、M5 带这些未做项合入 main（Q154）。任务记录见 [M5 任务 02](../m5/issues/02-log-ui.md)、
[M5 任务 03](../m5/issues/03-branches-and-remotes.md)，过程见 [M4 报告](../m4/report.md) 与 [M5 报告](../m5/report.md)。
原 `/private/tmp/revia-m5-integration-*` 的 fixture 与原件已在 2026-10-28 重启时丢失，需要重新准备（M5 报告 22.18）。
