# 08：创建 PR、提交审阅与确认合并

Status: ready-for-agent
Blocked by: 07

## What to build

用户明确提交后创建草稿 / 正式 PR、发评论与审阅，并在确认目标和 head 后合并。

依据：规格 F；Q176、Q194。

## Acceptance criteria

- [x] 多个远端明确选择来源 / 目标；所有远端写动作由用户明确提交，不能预览即发送
- [x] 审阅草稿本机持久化，绑定 PR 版本；离线、权限不足和版本变化保留草稿
- [x] 合并带确认 head SHA，409 或写后不确定先核对，不换新 SHA 重试
- [x] HTTP fixture 验证请求目标、次数、权限、版本变化与真实返回回执；前端展示原目标和已知结果

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F07 P1 | “确认…目标”：审阅/合并丢remote，可能写错仓库 | src-tauri/src/m7/commands.rs:535,576 |

## F07 [P1] 审阅与合并忽略所选远端

规格 S108：“明确来源仓库 / 分支和目标仓库 / 分支”；S110：“合并前确认方式、目标与 head SHA”。

`src/features/m7/GithubPanel.tsx:301–308,348–356` 审阅/合并payload不含remote；`src-tauri/src/m7/commands.rs:535,576` 使用 `PrContext::new(..., None)`，`github/pr.rs:312–315` 优先origin。列表详情175处却携带用户所选remote。触发：有origin与upstream，选upstream的PR后提交审阅或合并。结果：请求发origin的同号PR；SHA不同虽会拒绝，但fork常有相同head，足以实际写错仓库。HTTP写入未执行；路由错误是静态事实。

| F09 P1 | “草稿…绑定PR与提交版本”：键不匹配、无head | src/features/m7/GithubPanel.tsx:276；src-tauri/src/m7/commands.rs:507 |

## F09 [P1] 审阅草稿无法取回且不绑定版本

规格 S109：“PR 审阅草稿保存在本机并绑定 PR 与提交版本”。

`GithubPanel.tsx:274–286` 保存repoKey为 `login/headRef`；`m7/commands.rs:507–508` 列出用worktree规范路径SHA（github/mod.rs:285–287），键永不匹配。触发：添加行内草稿。结果：保存后立即刷新列表为空，重开仍找不到；`github/store.rs:20–30` 完全没有head/base版本字段。reviewBody/commentBody仅组件state，切PR共享且关闭丢失；即使修key，旧草稿仍可用刷新后的head提交。尚未远端发布。

| F18 P2 | “支持已有fork”：无法选来源或表达owner:branch | src/features/m7/GithubPanel.tsx:377；src-tauri/src/m7/github/pr.rs:1064 |

## F18 [P2] 创建PR不能选择已有fork来源

规格 S108：“支持已有 fork，明确来源仓库 / 分支和目标仓库 / 分支”。

`GithubPanel.tsx:377–384,740–767` 创建表单仅base/title/body/draft，head强制currentBranch，单一remote来自列表；`github/pr.rs:744–745,1054–1065` 对head使用分支名校验并拒绝冒号。触发：本地origin=自己的fork、upstream=目标项目，为origin feature向upstream main建PR。结果：不能表达 `owner:feature`，也不能独立选来源/目标，不支持已确认fork路径。未远端创建PR。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。
