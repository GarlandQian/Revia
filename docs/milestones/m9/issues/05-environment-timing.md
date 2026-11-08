# 05：打开 worktree 时的环境加载时机

Status: needs-info
Blocked by: None（等访谈 Q27 的结论）

## What to build

第一次打开某个 worktree 时，不再让环境捕获挡住文件列表。Windows 不等 PowerShell profile 加载完（Q212）；macOS 是否同样处理，以及 profile 加载完成前用哪个 Git 读取状态，等访谈 Q27 的结论，见规格 D 节。

## Acceptance criteria

- [ ] 按 Q27 结论实现，结论出来后补全本节。
- [ ] 执行 hooks、提交、fetch/pull/push、打开内置终端时，使用该 worktree 完整的环境；尚未加载完时等待，并在界面上说明正在等待。
- [ ] 环境捕获失败、超时的提示与回退沿用 Q205；不修改用户的 profile、执行策略或 Git 配置。
- [ ] 加载期间切走或关闭 worktree，迟到的环境结果不登记到别的 worktree。补并发回归测试，参照 `file_staging.rs` 的 Barrier 写法。

## Comments
