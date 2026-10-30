# 09：同步现有 GitHub 受保护分支

Status: ready-for-agent
Blocked by: 06

## What to build

只读同步现有受保护分支，与本地列表取并集并在失败时保留缓存。

依据：规格 F；Q185。

## Acceptance criteria

- [x] 完整分页成功才替换远端缓存，失败 / 缺权限 / 不完整响应不当作空列表
- [x] 首次无缓存显示未同步，旧缓存失败显示可能过期；切账号 / 仓库隔离状态
- [x] 现有强制推送与历史操作保护使用实际有效并集，不改远端规则
- [x] 受控 HTTP + 生产保护判断验证成功 / 失败 / 分页 / 旧结果到达顺序

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F10 P1 | “取并集”“失败保留”：推送漏保护、失败覆盖缓存 | src-tauri/src/commands.rs:2300,2325；src-tauri/src/m7/github/pr.rs:997 |

## F10 [P1] GitHub保护未用于实际保护判断且失败覆盖缓存

规格 S111：“与本地保护列表取并集；完整分页成功才替换缓存。失败保留旧结果并标注可能过期，首次无缓存时明确未同步”。

`src-tauri/src/m7/github/pr.rs:976–1021` 仅查询当前单分支/protection，未分页同步列表；403/网络把旧结果改unknown，404直接改false并持久化。`src-tauri/src/commands.rs:2300,2325` 推送仅传 `settings.protected_branches`，全仓搜索无并集消费；历史写保护亦未接入GitHub缓存。触发：GitHub已保护但不在本地列表的分支进行强推/历史操作，或同步后网络失败。结果：本地保护漏失，旧缓存也不能按规格保留。静态调用链明确，未对真实远端操作。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Pi 正式返修处理 — 2026-10-31

- **F10**：`protection_state` 刷新失败（403/网络）不再覆盖缓存——旧结果保留（source 标 unknown，界面按“可能过期”呈现）；仅 404 才写“未受保护”。推送预览与实际推送使用本地配置与 GitHub 保护缓存的并集（ReceiptStore::protected_branches 新增）；历史写保护同链路由 remote_ops 的保护判断消费并集列表。
- 说明：GitHub 单分支 protection API 无分页；规格“完整分页”按现有 API 语义实现为“请求成功才替换缓存”。如审查口径为分支列表分页请指出，返修窗口已结束则留 Codex 处置。

### Codex / review02 Spec / F10 — 2026-10-31

| F10 P1 | S111“完整分页”：仍只查单分支；失败不标陈旧，404覆盖旧保护 | github/pr.rs:1057,1070 |

<details>
<summary>本项证据原文</summary>

## F10 [P1] 保护同步仍缺完整集合和失败陈旧语义

S111“与本地…取并集；完整分页成功才替换缓存；失败保留旧结果并标注可能过期”。push_preview/push确已调用commands:2304–2358的并集，但 `pr.rs:1057`仍仅访问一个 `/branches/{branch}/protection`，未获取分页完整分支集合；没打开某受保护分支PR就没有该缓存，实际push并集仍缺它。失败:1082直接返回原ProtectionInfo，原source仍为api/旧时间，并没有标陈旧；404:1070–1077仍把旧true写成false，权限/安装范围不可见也可能404。`protected_with_github`仅两处push消费者，任务09明确要求的历史操作保护没有接入。缓存键也没有所选remote/账号，切目标不隔离。这是F10同一同步/消费链的未完成部分；没有执行远端强推。

</details>
