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

### Codex 直接修复 F10 — 2026-10-31

保护同步改为读取 `branches?protected=true&per_page=100` 的完整分页集合；独立同步入口不依赖存在或打开PR。只有全部页成功才替换；401/403/404/429/网络、坏JSON、字段缺失和中途失败保留旧集合与原时间并标记可能过期。首次无缓存显示尚未同步。缓存绑定真实worktree身份、账号、remote及GitHub仓库，旧无身份缓存保留原件而不自动认领。请求代际与原子落盘共锁，旧请求不能覆盖较新成功或失败。

实际推送按固定的目标remote与本地列表取并集，生产强推入口拒绝保护分支；历史改写确认显示当前账号各GitHub远端与本地保护列表的并集。普通本地历史与推送只读账号资料及缓存，不访问或刷新API凭据。不修改远端保护规则。

原始证据在 `/private/tmp/revia-m7-batch-g5scyw0r/codex-checks/`：`protection-cargo-02` 7项通过，真实loopback HTTP及独占Git/HOME/appdata覆盖完整分页、清空替换、失败保留、迟到响应、账号/remote/仓库隔离、重建与权限失败；直接调用生产强推判断验证并集。`protection-cargo-01`既有branch_remote31项与PR检出8项通过，但本组6通过/1失败（测试HTTP服务把无请求空连接的读取超时当失败）；修正服务后仅重跑受影响7项，完整失败原件保留。`protection-lib-01`现有GitHub库32项通过；`protection-bindings-01`导出1项通过。前端`protection-frontend-build-01`、`protection-lint-01`和`protection-x18n-01` exit0，翻译0错误/76个既有unused-key警告，不删除保留键。源码hash、精确argv、真实exit和分流stdout/stderr逐轮保存；x18n首次沙箱缓存EPERM也保留，非产品检查失败。

官方branches文档原件在`impl/protection-official.json`。本组仍是中间检查点；F08/F09与F13/F22尚未完成，任务13/14的真实桌面事实仍缺，不称整体通过或最终候选。


### Codex / review03 Standards 正式原文 — 2026-10-31

固定候选`4c3b3f838828935dd6bb64efc084750fb292df51`，增量基点`7741dba8725d7b02cc09197cb88d26014c0c48df`。本轴原文如下，不合并重排；详证在本轮`/private/tmp/revia-m7-batch-g5scyw0r/review-03-zika9m9t/standards-evidence.md`。本轮明确P2由Codex直接处理，不再派Pi；P3登记backlog15。

```markdown
Standards：仅审 `7741dba…4c3b3f8` 的93路径、10提交；以下均为固定源码静态结论。

硬违反（1项）：

- **[P2，H1] 新fixture阻断Windows检查**：`src-tauri/tests/m7_diagnostics.rs:29,35`、`m7_pr_checkout.rs:87`、`m7_protection.rs:72`、`m7_review_drafts.rs:75` 将Git搜索路径限定为Unix目录，普通测试没有平台条件。Windows上的Git仅位于常规PATH时，生产 `git/mod.rs:208` 经 `shell.rs:83` 找不到Git，fixture的unwrap失败，现有 `.github/workflows/compile-check.yml:116` 因而失败。违反 `AGENTS.md:72` 与 `docs/adr/0007-target-platforms.md:7` 的“不写死macOS假设”。应读取平台PATH，仅将Unix专用断言限定平台；未实际运行Windows。

判断（均P3，非阻断；依据任务说明smell baseline，非硬违反）：

- **J1，可能Duplicated Code**：`src/App.tsx:1784,1793,1807,1819` 四次复制五类退出保护条件；新增保护来源须同步四处，容易漂移，可集中判定。
- **J2，可能Primitive Obsession**：`github/store.rs:30` 的草稿kind为String，`GithubPanel.tsx:119,125` 又把审阅事件写进原表示新旧行的side；新增表单/事件时须维护字符串分支与强制转换，可用明确领域类型。
- **J3，可能Duplicated Code（仅测试）**：`m7_protection.rs:24` 与 `m7_review_drafts.rs:27` 复制Fixture及HTTP支架；隔离和服务端时序修正需多处同步，可列共享fixture维护项。

无其他新增硬违反。已跳过工具强制检查项及既有backlog；本轮只读核对源码和原始检查，未运行产品/桌面。正式结果不是桌面通过结论，P3按Q174留backlog。
```


### Codex 处理 review03 两项 P2 — 2026-10-31

S03-01：撤销回磁盘原文也排在旧草稿持久化之后清理；重新打开等待原目标在途写，并保留最新内存正文（含撤销结果），避免加载旧草稿。自动草稿冲刷覆盖有待清理记录的干净正文。清理失败保留最新文本、原件与退出保护，明确冲刷成功后解除；等待期间的新输入仍有自己的后续草稿，不被较早清理删除。

H1：四个新增测试fixture共用平台环境构造，只从当前平台PATH解析Git；HOME/USERPROFILE、Git全局配置均绑定本次独占fixture。Unix专用hook/权限断言继续按平台限定；未通过跳过全部Windows测试来消除问题。

`review03-repair-node-01` 102项通过（含新增4个生产EditSession撤销/在途/失败/新输入回归）、0 skipped；`review03-repair-rust-01` 四个受影响targets共27项通过；`review03-repair-frontend-01`与`review03-repair-lint-01` exit0。源码hash、精确argv、真实exit、分流原件在批次codex-checks。未修改locale键，之前x18n02的0错误/76保留键警告对应键集未变。未运行Windows或真实桌面，不把macOS通过称Windows实测。只修明确P2，三个P3判断继续backlog15。
