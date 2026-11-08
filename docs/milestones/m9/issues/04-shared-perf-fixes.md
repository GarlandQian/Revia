# 04：诊断摘要攒批、监视过滤与空闲重渲染

Status: ready-for-agent
Blocked by: None（可立即开始）

## What to build

两个平台共用的修正：每条 Git 命令不再整份重写诊断摘要并强制落盘；被忽略文件的变化不触发刷新；空闲时不再每秒整树重渲染。见规格 C 节。

## Acceptance criteria

- [ ] `CommandLog::record` 不在锁内同步写盘；摘要先进内存，后台约每 5 秒合并写一次，正常退出时再写一次。
- [ ] 7 天保留、手动清空、导出时合并内存中尚未落盘的记录这三项行为不变（Q187）。补 Rust 回归测试：记录时不同步写盘；到间隔、主动刷新、退出时写盘；导出时包含内存中的记录。
- [ ] 监视判断被忽略的标准与 Git 一致（嵌套 `.gitignore`、`.git/info/exclude`、`core.excludesFile`），通过 Git CLI 判断，已确认被忽略的目录缓存结果。
- [ ] 被忽略路径的事件不触发刷新；忽略规则文件变化时清空缓存并刷新；`.git` 内部（index、HEAD、refs）照旧刷新。补真实 Git 回归测试。
- [ ] `App.tsx` 的监视状态轮询只在 `active` 或 `error` 实际变化时更新状态。
- [ ] LogView 滚动、并排视图行号的 `will-change`、大组件 memo 这些不改变行为的渲染优化，等第一轮测量数据出来后再决定做不做。本任务不强制，做了的项要在 Comments 中写明收益依据。

## Comments
