# 07：查看 PR 并检出来源分支

Status: done
Blocked by: 06

## What to build

显示 PR 列表、详情、diff 与讨论，明确来源和目标，通过 Rust 在确认后检出。

依据：规格 F；Q176、Q194。

## Acceptance criteria

- [x] 分页、草稿 / 状态、权限与离线失败呈现正确；文本内容不执行脚本或不可信 HTML
- [x] 检出前明确仓库 / worktree / remote / 分支 / head，沿既有未提交改动保护、回收及写前核验
- [x] 支持已有 fork，不自动创建 fork、添加 remote 或推送；无法映射时说明缺失条件
- [x] 真实自建 Git 与受控 HTTP 一起验证 head 变化、脏工作区、切目标与失败结果

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments
