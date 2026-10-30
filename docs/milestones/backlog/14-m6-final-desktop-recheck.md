# 14：M6 收尾小修的桌面复查

**What to build:** 在真实桌面（需全屏控制，能右键和 ⌘ 多选）复查 M6 合并前最后几处没有在 Release 上点过的小改动。

**Blocked by:** None (can start immediately)

**Status:** ready-for-human

- [ ] 日志多选两个以上提交右键：新建分支、修改提交说明、交互式 rebase、Reset 均禁用，并显示“多选时只能 cherry-pick 或 revert”；多选含合并提交时显示“所选提交包含合并提交，不能 cherry-pick 或 revert”
- [ ] 多选 cherry-pick / revert 确认框不再只显示右键那一个提交，说明为“按顺序……所选提交”
- [ ] Reset 选 Hard 执行后，再打开 Reset 对话框默认 Mixed；选 Soft / Keep 后再打开仍记住（Q175）
- [ ] rebase 停点占住 `.git/index.lock` 时点跳过：报“结果待核对”，锁文件仍在（Rust 回归已覆盖，界面提示待看）

## 背景

2026-10-30 Claude 在 Release a187c87 完成 M6 全部桌面组，之后的 ff38a02、ba1fd57、0113241 只改了上述几处；Release 0113241（SHA-256 见 `~/.cache/revia-m6-claude/release-0113241/manifest.txt`）上已复查 Stash 归档分组，其余项因全屏控制授权无人确认而未点。自动化检查：Rust 696 / 0 / 1、Node 485/485、build/tsc/lint、x18n 0 错误、提取 0 中文。用户确认继续合并，这几项合并后补查。
