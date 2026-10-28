# M5 首批报告：只读历史后端与 IPC（任务 01）

Status: 交付待审（H01–H04 的代码与自查完成；规划方独立审查、真实桌面对照、Release 打包与 D06 未做）

日期：2026-10-27。分支 `m5`，代码基线 `d58233c1bea3583cbec5868739b9618f7696eb99`，规划提交 `a456195`（本交接）。
本批只做 [任务 01](./issues/01-readonly-history.md)，不涉及任务 02/03 的产品行为，也不改变 M4 未验收状态与 M4 合并写入阻塞项。

## 1. 结论

- 只读提交历史后端与 10 个生产命令已实现，新类型由 ts-rs 生成，`src/lib/ipc.ts` 提供固定类型封装。
- 历史内容全部来自 Git 对象：起点、父提交、路径与对象 ID 都绑定到完整 OID，不读工作区、不写暂存区、不 fetch；外部 diff 与 textconv 由固定参数排除。
- 集中检查全部通过：`tsc -b`、`lint`、`build`、Rust 448 通过 / 1 忽略 / 0 失败、12 个 Node 脚本 12 通过（数字为 2026-10-27 返修后的重测结果）。
- 未做（不声称通过）：规划方独立审查、真实桌面、与 Rebased 1.1.22 对照、Release 打包、D06、Windows 编译、长测。任务与 M5 均不标 `done`。

## 2. 实施位置

| 文件 | 内容 |
| --- | --- |
| `src-tauri/src/git/history.rs`（新增） | 只读 Git 读取：引用、HEAD、空 tree、提交遍历、提交对象、改动文件（`-z` 机器格式）、单文件 patch、blob；以及这些输出的解析 |
| `src-tauri/src/history.rs`（新增） | 历史领域：`HistoryStore` / `HistoryTarget`、查询与游标、分页、详情、改动文件、提交 diff（统一行 / 左右对照 / 高亮）、有界缓存与释放、公开类型 |
| `src-tauri/src/commands.rs` | `AppState.history`、`release_session` / `reload_sessions` 一并失效历史目标、10 个命令与请求类型 |
| `src-tauri/src/lib.rs` | 注册 `history` 模块与 10 个命令 |
| `src-tauri/src/git/mod.rs` | 新增 `Git::run_with_input`（只读计算空 tree 用），导出 `history` 模块 |
| `src-tauri/src/session.rs` | 新增只读的 `WorktreeSession::instance_id()`，供历史目标绑定会话实例 |
| `src-tauri/tests/history_readonly.rs`（新增） | 30 个真实 Git 临时仓库回归，覆盖 H01–H04、契约检查清单与返修 8 项 |
| `src/lib/ipc.ts` | 10 个固定类型封装（`historyOpen` … `historyFileHighlight`） |
| `src/types/bindings/History*.ts` | ts-rs 生成的 17 个新绑定 |
| `tools/test-merge-appearance.mjs` | 修继承的别名导入接线（见第 7 节），不改断言、不改合并行为 |

## 3. 命令与约定

| 命令 | 作用 |
| --- | --- |
| `history_open` | 按已登记 worktree ID 打开历史目标；返回 HEAD、HEAD 引用、本地引用、是否空仓库与工程限额 |
| `history_close` | 关闭目标（幂等），关闭后该目标的请求与游标立即失效 |
| `history_refs` | 重新读取本地引用 |
| `history_log_page` | 一页提交：首次给起点（本地引用全名 / 完整提交 OID / HEAD），之后只带游标 |
| `history_commit` | 按固定提交 OID 读完整正文、父提交与署名 |
| `history_commit_files` | 提交相对**显式指定**的真实父提交的改动文件；根提交明确用空 tree |
| `history_file_rows` | 统一 / 左右布局的显示行（分页） |
| `history_file_side_layout` | 左右对照：两侧行数、改动段、折叠区（没有段操作目标） |
| `history_file_side_lines` | 按需取一侧的行（含改动标记与词级区间） |
| `history_file_highlight` | 语法高亮（内容由 Rust 按对象 ID 读取） |

约定：

- 起点只接受三类值；引用必须是本地引用列表里的**全名**，完整提交 OID 必须是 40 / 64 位十六进制且对象类型是 `commit`。`--help`、`HEAD~1`、缩写、不存在对象、blob OID 全部拒绝，且不退回当前 HEAD。
- 查询开始时把起点解析成完整 OID 并固定；之后引用移动、删除或切换分支都不改变这条查询已读的历史。新查询重新读引用。
- 分页只向前推进，游标为不透明字符串（`<target_id>/<query_id>:<skip>`，返修第 4 项），与签发它的目标、遍历方式、进度一起校验；跨目标、跨仓库、倒退、重复、过期一律明确拒绝。
- 返回**全部真实父提交**；`first_parent` 只影响遍历，不改父列表。
- 返回的作者 / 提交者时间含 Unix 秒与 `+0800` 形式时区；正文完整返回，无法无损解码时 `message_lossy = true`，不静默替换。
- 改动文件模型独立于 `ChangeEntry` / `ChangesKind`，不含工作区分组、指纹、审阅标记或“已暂存”身份。
- 提交 diff 的请求必须原样带回后端签发的 `diffId` 与身份字段；任何字段被改动、换目标、换父提交都拒绝（详见第 5 节）。

### 调用示例（前端）

```ts
const target = await ipc.historyOpen(worktreeId);

// 主区域日志：第一页从当前分支开始，只看第一父提交
let page = await ipc.historyLogPage({
  targetId: target.targetId,
  start: { kind: "ref", fullName: "refs/heads/main" },
  firstParent: true,
  limit: 100,
});
while (page.nextCursor) {
  page = await ipc.historyLogPage({ targetId: target.targetId, firstParent: true, limit: 100, cursor: page.nextCursor });
}

// 打开某个提交的改动文件：merge 必须显式给父提交，根提交给 null
const detail = await ipc.historyCommit(target.targetId, oid);
const parent = detail.parents.length > 1 ? detail.parents[0] : (detail.parents[0] ?? null);
const files = await ipc.historyCommitFiles({ targetId: target.targetId, commit: oid, parent });

// 打开一个文件的提交 diff
const file = files.files.find((f) => f.openable)!;
const rows = await ipc.historyFileRows({
  targetId: target.targetId,
  file: { commit: oid, parent, path: file.path, originalPath: file.originalPath,
          oldOid: file.oldOid, newOid: file.newOid, status: file.status, diffId: file.diffId! },
  view: "split",
  ignoreWhitespace: "off",
  from: 0,
  to: 400,
});
// rows.rows 没有任何 hunkAction / segmentAction，历史 diff 只读

await ipc.historyClose(target.targetId);
```

### 调用示例（Rust，测试与命令层同一入口）

```rust
let store = HistoryStore::new();
let info = store.open(&session)?;                       // 校验已登记 worktree 的会话
let target = store.target(&info.target_id)?;
let page = target.log_page(Some(HistoryStart::Ref { full_name: "refs/heads/main".into() }), true, 100, None)?;
let files = target.commit_files(&page.commits[0].oid, page.commits[0].parents.first().map(String::as_str))?;
store.close(&info.target_id);
```

## 4. 工程选型（实施方决定，写入报告供审查）

1. **分页**：`git log --topo-order --skip=<n> --max-count=<m> <固定 OID>`，`-z` + NUL 分隔字段。查询状态保存在目标内（起点、遍历方式、下一个 skip、引用快照），游标只携带查询 ID 与 skip。选它是因为：起点固定后历史不可变，`--skip` 的结果确定；无需自己维护遍历前沿。
2. **限额**（`HistoryLimits` 随目标返回）：单页 ≤ 200 条；一次查询跨度 ≤ 20 000 条（超过报错，不伪装成末尾）；每目标最多 8 个未走完查询（超出淘汰最早，游标随后报“已过期”）；全局最多 4 个历史目标（超出淘汰最旧，旧 `target_id` 立即失效）；每个提交最多列 5 000 个改动文件（超出 `truncated = true`）；单文件 patch ≤ 16 MiB；单 blob ≤ 8 MiB；左右对照单侧 ≤ 200 000 行。
3. **空 tree**：`git hash-object -t tree --stdin`（空输入，不加 `-w`，不写对象库），按仓库实际哈希算法得到；不硬编码 SHA-1 常量。
4. **改动文件**：`git diff-tree -r -z --raw --numstat -M --no-commit-id --no-ext-diff --no-textconv <base> <commit>`，一次调用同时拿对象 / 模式 / 状态 / 增删行数 / 二进制标记。路径取 NUL 分帧的原始字节，不 trim、不反转义；`String::from_utf8` 失败才判定为非 UTF-8 路径。
5. **单文件 patch**：`git --literal-pathspecs -c core.quotePath=false diff --no-ext-diff --no-textconv -M <base> <commit> -- <paths>`，路径一律在 `--` 之后。身份以 raw 元数据为准，patch 只提供 hunk：单个结果直接用，多个结果用 `index <old>..<new>` 的缩写前缀与 raw 对象 ID 匹配，匹配不唯一就明确失败（不从 patch 标题反推路径身份）。
6. **缓存与释放**：每个目标 4 份文件内容、8 份排版、4 份左右段与折叠区；满则整组清空（与既有工作区缓存的做法一致）。blob 文本按需读取并挂在该份内容上，同一份内容只读一次。关闭 / 淘汰目标时先在目标表锁内移出条目、释放表锁，再置 `closed` 并清空全部缓存（返修第 7 项）；读取期间仍持目标自身的锁（同一目标的请求排队）。进行中的请求仍会带着旧 `target_id` 返回，前端按目标身份丢弃，不会重新填充已关闭目标的缓存。释放 / 环境重载会先让会话实例失效，旧实例不能再登记新目标（返修第 3 项）。高亮复用应用级 `Highlighter` 线程，文档键含目标与 diff 身份，缓存上限 24 份；释放目标不声称立即杀掉线程。
7. **不可打开的文件**：非 UTF-8 路径（`readonlyReason: "nonUtf8Path"`）与子模块（`"submodule"`）不给 `diffId`，请求一律拒绝；符号链接按 blob 内容（链接目标）作为文本显示；二进制给出 `binary = true` 与 `emptyReason = "binary"`；仅权限变化给 `modeOnly = true` 与 `"modeOnly"`；新增 / 删除的空文件给 `"emptyFile"` 且缺失侧为 `*Missing = true`（与空文件区分）。
8. **空白档位**：历史是只读，但仍接受现有 `IgnoreWhitespace` 四档用于查看；档位进缓存键，不进 `diffId`，因此不同档位不共享缓存，也不会因为切档位改不了身份。

## 5. 只读与安全保证

- 所有 diff 固定带 `--no-ext-diff --no-textconv`（包括 `diff-tree`），测试用 `diff.external` 与 `diff.textconv` 哨兵验证：两个脚本都不会被执行。
- 不执行 `fetch`、`checkout`、`reset`、`apply`、`add`，不写临时 index；读不到对象就报错。
- 历史读取使用自己的 `Git` 副本，固定 `GIT_NO_LAZY_FETCH=1`（返修第 1 项）：部分克隆里缺失对象明确报错，不自动向远端补取；工作区读取不受影响。
- 改动文件列表与单文件 patch 固定 `GIT_ATTR_SOURCE=<提交>`（返修第 2 项）：属性取自提交自己的树，工作区的 `.gitattributes` 不能改变已打开的提交 diff。
- 路径参数全部在 `--` 之后并带 `--literal-pathspecs`：`--help`、`:(glob)*`、`a[1].txt` 等都按字面匹配（有回归）。
- 生产入口只接受已登记 worktree（`history_open` 走 `worktree_session`，未登记直接拒绝）与后端签发过的目标 / 文件身份。
- `diffId = SHA-256(target 实例, commit, parent, status, path, originalPath, oldOid, newOid)`。请求先重算比对（无 Git 调用），不一致立即拒绝；缓存命中同样以该身份为键。
- 只读状态回归：完整读一轮（分页、详情、改动文件、两种视图、左右对照、左右行、高亮）前后比较 HEAD、全部 refs、`ls-files -s`、`status --porcelain=v2`、`config --list --show-origin`、`.git/index` 原始字节与全部工作区文件内容，完全一致。

## 6. 验证（集中执行，原始日志见证据目录）

证据目录：首次交付 `/private/tmp/revia-m5-start.zEV4wp/history-impl.C5uZkh/evidence/`；2026-10-27 返修 `/private/tmp/revia-m5-start.zEV4wp/rework-U1n2KH/evidence/`（各自独占 `mktemp -d` 子目录，下表为返修后的重跑结果）。
环境：`node v26.5.0`、`pnpm 12.14.0`、`cargo 1.99.1`、`git 2.56.0`，见 `environment.txt`。

| 检查 | 命令 | 退出码 | 结果 |
| --- | --- | --- | --- |
| 类型检查 | `pnpm exec tsc -b` | 0 | 通过（`tsc.log`，空输出） |
| lint | `pnpm lint` | 0 | 0 error；仅继承的 `tools/test-refresh-gate.mjs` 警告（`lint.log`） |
| 前端构建 | `pnpm build` | 0 | 通过（`build.log`，仅既有 chunk 体积提示） |
| Rust 测试 | `/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml` | 0 | 13 个测试二进制：448 通过、0 失败、1 忽略（`cargo-test.log`） |
| Node 回归 | `node --test tools/test-*.mjs`（12 个脚本逐个执行） | 0 ×12 | 12/12 通过（`node-summary.txt` 与 `node-*.log`） |

Rust 明细：lib **206**、`commit_flow` 12、`discard_recovery` 71、`file_staging` 20、`history_readonly` **30**、`hunk_staging` 24、`merge` 15、`refresh_isolation` 15、`review_marks` 31、`segment_staging` 21、`session_flow` 3、`main` 0、doc-tests 0。

新增回归覆盖（`src-tauri/tests/history_readonly.rs`，全部自建真实 Git 仓库 + 独立应用数据目录）：

| 契约 | 用例 |
| --- | --- |
| 固定起点 | `log_page_keeps_the_start_object_when_the_ref_moves`：读完第一页后把 `main` 移回旧提交，旧游标仍读原 OID，新查询读新引用 |
| 参数与错误 | `start_rejects_rev_syntax_options_missing_objects_and_non_commits`、`missing_objects_error_instead_of_returning_empty_history`、`limit_is_bounded_and_over_span_is_not_reported_as_history_end` |
| 游标身份 | `cursor_is_bound_to_target_query_and_close`（跨目标、重复、倒退、遍历方式、关闭、幂等）、`query_cap_expires_the_oldest_cursor_explicitly` |
| 父提交 | `merge_commit_keeps_all_real_parents_across_pages`（跨页父边、`--first-parent` 不改父列表）、`merge_files_require_an_explicit_real_parent` |
| 根提交 / 空 tree | `root_commit_compares_against_the_repository_empty_tree` |
| 详情 | `commit_detail_preserves_multiline_body_and_signatures`（多行正文、分隔符、空提交） |
| 文件身份 | `rename_reads_both_sides_and_delete_reports_missing_side`、`empty_file_differs_from_a_missing_side`、`binary_and_mode_only_changes_are_explicit`、`submodule_is_listed_but_not_openable`、`no_newline_at_end_of_file_is_preserved`、`symlink_content_is_the_link_target` |
| 特殊路径 | `special_paths_are_matched_literally_and_read_correctly`（`--help`、`:(glob)*`、`a[1].txt`、前后空格、空格、Tab、U+FFFD、中文） |
| 非 UTF-8 | `non_utf8_paths_stay_distinct_and_are_not_openable`（两个不同字节路径不合并、不可打开、伪造请求被拒） |
| 固定文件 | `opened_commit_diff_is_immutable_while_the_worktree_changes`（改工作区、暂存、切分支后行 / 布局 / 高亮不变；篡改身份被拒） |
| 缓存 | `caches_are_isolated_and_bounded`（跨目标、跨仓库、空白档位、容量上限） |
| 释放 | `releasing_a_worktree_and_clearing_the_store_invalidate_targets` |
| 空仓库 | `empty_repository_is_reported_separately_from_failures` |
| 只读副作用 | `history_reads_leave_git_state_untouched`、`external_diff_and_textconv_are_never_run` |
| 部分克隆（返修 1） | `partial_clone_missing_objects_error_instead_of_lazy_fetching`（`blob:none` 的 `file://` 部分克隆；缺失对象报错、读前后仍缺失） |
| 提交属性（返修 2） | `worktree_attributes_cannot_change_an_opened_commit_diff`（工作区 `*.txt -diff` + 缓存淘汰后重读仍是文本） |
| 会话失效（返修 3） | `released_and_reloaded_sessions_cannot_claim_new_history_targets`（`release_session` / `reload_sessions` 后旧会话 `open` 被拒且目标表不增加） |
| 游标身份（返修 4） | `cursor_is_not_accepted_by_another_target_with_the_same_progress` |
| 页尾空主题（返修 5） | `page_reads_the_last_commit_when_its_subject_is_empty`；解析单元 `keeps_an_empty_subject_on_the_last_record` |
| 非 UTF-8 重命名（返修 6） | `rename_from_a_non_utf8_path_is_not_openable`（`mktree -z` + `commit-tree` 构造） |
| 二进制左右对照（返修 8） | `binary_and_mode_only_changes_are_explicit` 补充 `file_side_layout` 的 `binary = true` / `emptyReason = Binary` |

另在 `src-tauri/src/git/history.rs` 与 `src-tauri/src/history.rs` 增加了解析与身份计算的单元测试（NUL 分帧、raw/numstat、重命名、二进制、Tab 路径、gpgsig 续行、有损正文、缩写对象 ID 匹配、`diffId` 覆盖全部字段）。

未执行：真实桌面验收、Rebased 1.1.22 并排对照、Release 打包与 D06、Windows 编译、长期运行 / 内存长测、规划方独立审查。

## 7. 继承问题处理

`tools/test-merge-appearance.mjs` 在原基线上无法运行：`@/lib/theme` 别名在 Node 里解析不到。按交接建议参照 `test-sync-scroll.mjs` 用 `node:module` 的 `registerHooks` 把 `@/` 映射到 `src/`、给相对导入补 `.ts`，再动态导入被测模块。断言与合并行为未改，现在 2 个用例通过。

## 8. 未完成项与未决产品行为

未做（留待规划方）：

- 规划方对返修改动的复核、真实桌面、Rebased 对照、Release 打包、D06、Windows 编译、长测。
- 任务 02/03 与日志界面：默认引用范围、列顺序、筛选组合、日期口径、merge 默认父项、路径是否跟踪重命名、跨 worktree 状态、日志索引。

需要产品确认（本批保留无默认值的读取原语，未替用户决定）：

1. merge 提交的比较父项默认值（现在必须显式传父提交；传 `null` 会被拒绝）。
2. 分支图是否需要在后端计算布局（本批只给父边与引用标签，布局留给界面）。
3. 历史日志里“路径筛选”是否跟随重命名（`-M` 已检测到重命名并提供 `originalPath`，但筛选语义未定）。
4. 提交详情里的邮件 / 时区展示格式。
5. 历史 diff 是否跟随设置里的“忽略空白”档位作为默认值（本批只提供参数）。

已知限制：

- **Git 版本要求**（返修新增）：`GIT_NO_LAZY_FETCH` 需要 Git 2.44+，`GIT_ATTR_SOURCE` 需要 Git 2.40+，项目最低支持 2.11。在低版本 Git 上这两项保证不成立：历史读取会退回“缺失对象自动补取”和“属性取自工作区”的旧行为；不认识的 `GIT_*` 变量被 Git 忽略，不会报错，也不会被检测出来。若要按最低版本交付，需要在读取前按 `git --version` 明确提示。
- 关闭目标不再阻塞其他 worktree 的请求（返修第 7 项），但**同一目标**的读取仍持有目标自身的锁：一份大 diff 读取期间，该目标的其它请求（含关闭时要等它结束）会排队；本批接受，未改成分片读取。
- 部分克隆的只读保证依赖 Git 自己遵守 `GIT_NO_LAZY_FETCH`；回归用 `rev-list --objects --all --missing=print`（同样带该变量）观察缺失对象是否仍然缺失。
- 非 UTF-8 文件名在 macOS APFS 上无法真实创建，回归改用 `git update-index --cacheinfo` / `git mktree -z` + `git commit-tree` 构造真实提交对象来覆盖；读取路径本身仍走生产代码。
- Git 超时沿用共享执行器（120 秒），表现为 Git 失败（退出码 `-1`），未单独分类。
- `git log --skip` 在极深分页时需要重新走到该位置，本批用“跨度 ≤ 20 000 条”的限额兜住；若后续界面需要更深的历史，需要换成分页前沿游标。
- 单文件 patch 每次请求会重读一次该提交的完整改动文件列表用于身份校验；同一个目标内已缓存的 diff 不再重读。

## 9. 2026-10-27 返修（审查 8 项，Q113 唯一一次返修）

分支 `m5-rework`，从 `m5` 的 `db84e91` 分出。只做任务 01 `## Comments` 里 Claude 审查意见的 8 项，不改已确认的决策、不改合并功能、不改任务状态。
每项都在仓库内的 `src-tauri/tests/history_readonly.rs` 或模块单元测试里加了回归（`cargo test` 运行，调用系统真实 Git）；并逐项做了反向验证：把对应修复临时去掉后，目标回归必须失败（原始输出见证据目录 `negative-controls.log`，7 项全部按预期失败；第 7 项按审查意见不做时序回归）。

| 项 | 改了什么 | 回归 |
| --- | --- | --- |
| 1 P1 部分克隆自动补取缺失对象 | `HistoryStore::open` 为历史读取构造带 `GIT_NO_LAZY_FETCH=1` 的 `Git` 副本（新增 `Git::with_env_vars`），历史目标的全部 Git 调用都用它；工作区读取不变 | `partial_clone_missing_objects_error_instead_of_lazy_fetching`：`uploadpack.allowFilter=true` 的 `file://` + `--filter=blob:none` 克隆；旧提交旧侧 blob 本地缺失 → `commit_files` 明确报错并指名缺失对象，读取前后 `rev-list --missing=print` 结果完全一致；同一克隆里对象齐全的提交仍能正常读 |
| 2 P1 工作区 `.gitattributes` 影响提交 diff | `read_changes`（numstat 二进制判断）与 `file_patch` 都固定 `GIT_ATTR_SOURCE=<提交>`，属性取自提交自己的树 | `worktree_attributes_cannot_change_an_opened_commit_diff`：读一次文本 diff → 工作区写 `*.txt -diff` → 再读 `MAX_CACHED_DIFFS + 1` 份别的 diff 逼缓存淘汰 → 重读仍是同样的文本行，`commit_files` 的 `binary` 仍为 false |
| 3 P1 已释放会话仍能登记新目标 | `HistoryStore` 记录每个 worktree 已失效的最大会话实例号（`invalidate_session`）；`open` 在持有目标表锁、登记之前校验；`release_session` / `reload_sessions` 先标记失效再清目标 | `released_and_reloaded_sessions_cannot_claim_new_history_targets`：走 `AppState` 生产路径；`release_session` 后用旧会话 `open` 被拒绝且目标表不增加，`reload_sessions` 同理，重载后的新会话仍能打开 |
| 4 P2 游标没有绑定目标 | 游标改为 `<target_id>/<query_id>:<skip>`，解析后先校验目标；原有“过期 / 倒退 / 重复 / 遍历方式”校验不变 | `cursor_is_not_accepted_by_another_target_with_the_same_progress`：A、B 各读完同样大小的第一页（查询编号同为 `q1`、skip 同为 2），A 的游标在 B 上被拒绝，B 自己的游标仍可用 |
| 5 P2 页尾空主题导致整页读不出来 | `parse_log_page` 只去掉 `-z` 补的那一个结尾空字段 | `page_reads_the_last_commit_when_its_subject_is_empty`（集成）与 `git/history.rs` 的 `keeps_an_empty_subject_on_the_last_record`（解析单元）：空主题提交作为一页最后一条时主题为 `""`、`exhausted = true` |
| 6 P2 重命名前的非 UTF-8 路径被标成可打开 | `describe_change` 的主路径或原路径任一不是合法 UTF-8 都置 `pathLossy`、`openable = false`、`NonUtf8Path`，不签发 `diffId` | `rename_from_a_non_utf8_path_is_not_openable`：`git mktree -z` + `commit-tree` 构造“非 UTF-8 名 → `renamed.txt`”的重命名；文件不可打开、无 `diffId`，用有损展示名伪造的请求被拒 |
| 7 P2 关闭目标卡住其他历史请求 | `close` 先在目标表锁内移除条目并释放表锁，再标记目标失效、清缓存（`retire`）；`open` 的容量淘汰同样改为表锁外回收；`history_close` 放进 `spawn_blocking` | 按审查意见不做时序回归；同目标读取仍持目标锁，写入第 8 节已知限制 |
| 8 P3 二进制文件的左右对照报错 | `file_side_layout` 先读缓存里的 `binary` 标记再决定是否读文本；二进制返回 `binary = true`、空段与空折叠区、`emptyReason = Binary` | `binary_and_mode_only_changes_are_explicit` 补充：`file_side_layout` 返回 `binary = true`、两侧 0 行、`emptyReason = Binary` |

返修只改了 `src-tauri/src/git/mod.rs`、`src-tauri/src/git/history.rs`、`src-tauri/src/history.rs`、`src-tauri/src/commands.rs`、`src-tauri/tests/history_readonly.rs` 与本报告、任务 01 的 Comments。公共类型与 ts-rs 绑定没有变化（游标仍是不透明字符串、`HistorySideLayout` 字段未动），未重跑 `pnpm bindings`。

## 10. 提交

首次交付提交在 `m5`；本次返修补丁提交到 `m5-rework`，提交说明以 `M5：` 开头。均未推送、未合并、未创建 PR，未改动主目录与既有 M4 worktree，未清理旧会话。

---

# M5 第二批报告：日志界面、分支与远端（任务 02、03）

Status: 交付待审（L01–L06、B01–B08 的代码与自查完成；规划方独立审查、真实桌面对照、Release 打包与 D06 未做）

日期：2026-10-27。工作位置 `/Volumes/T7 Shield/project/Revia-m5`，分支 `m5-batch2`，从 `m5` 的 `3c9a634` 分出。
本批按 Q146 把任务 02、03 一起实施，做完再统一审查与验收；中间不逐项派发。
M4 继承的合并写入保护与软换行三项仍是桌面验收阻塞项，本批不改其状态。

## 11. 结论

- 日志（L01–L06）与分支 / 远端 / Git 命令日志（B01–B08）的界面与后端都已实现；新界面在独立功能目录里，`App.tsx` 只做接线。
- 所有 Git 调用仍在 Rust 层：前端只传结构化参数（筛选条件、分支 / 远端名、选项），参数由 Rust 组合；新类型全部由 ts-rs 生成，`src/lib/ipc.ts` 只做固定类型封装。
- 所有写操作（检出、新建分支、fetch、更新、推送、提交并推送）走同一套全局写门禁、关闭 / 退出保护、结果归属与「结果待核对」；会覆盖或丢弃工作区内容的步骤先把内容存入回收区并注明来源。
- 集中检查全部通过：`tsc -b`、`lint`、`build`、Rust **517 通过 / 1 忽略 / 0 失败**、15 个 Node 脚本 15 通过（证据见第 17 节）。
- 未做（不声称通过）：规划方独立审查、真实桌面、与 Rebased 1.1.22 并排对照、Release 打包、D06、Windows 编译、长测。任务 02/03 与 M5 均不标 done。

## 12. 实施位置

| 文件 | 内容 |
| --- | --- |
| `src-tauri/src/git/mod.rs` | 命令日志扩展（时间、worktree 归属、参数、退出码、耗时、输出 64 KB 截断、凭据打码、最近 1000 条）；`Git::run_with_timeout` / `run_network`（网络 10 分钟上限、`GIT_TERMINAL_PROMPT=0`、空 stdin、`SSH_ASKPASS_REQUIRE=never`）；`with_worktree_id` |
| `src-tauri/src/git/branch.rs`（新增） | 分支概览：本地分支 + 上游跟踪 + 领先 / 落后（截至上次 fetch）、远端分支分组、当前分支 / 游离 HEAD、提交身份；分支名校验与存在判断 |
| `src-tauri/src/git/history.rs` | 带筛选的分页（作者 / 路径 / 日期 / Git 正则文本）、扫描模式（文字或哈希前缀，含完整提交信息）、多起点、`upstream_of`、`read_range`、`object_at_path`、`head_files` |
| `src-tauri/src/history.rs` | `HistoryFilter` / `HistoryDateFilter` / `HistoryMergeMode` / `HistoryMergeFiles` / `HistoryMergeView` 等公开类型；筛选解析与校验、起点集合、游标绑定筛选、扫描式分页与跨度限额；merge 提交的合并视图（文件集合与 `diff-tree -c` 一致）、按父提交分组、超过两个父提交的分组与三栏拒绝；三栏内容按对象读取 |
| `src-tauri/src/remote_ops.rs`（新增） | 检出（本地 / 远端分支、同名不跟踪时拒绝、智能 / 强制检出先存回收区）、新建分支（三种起点）、fetch（四种标签方式）、更新（fetch 上游 → merge / rebase / 按分支默认，autostash + 回收区）、推送（目标解析、上游记录、推送标签、`--force-with-lease` 带期望值、受保护分支禁用）、检查远端（`ls-remote` 只比较引用）、推送预览 |
| `src-tauri/src/settings.rs` | 日志设置（列、合并线拉直、只看第一父提交、预览位置）与更新 / 推送设置（更新方式、fetch 标签方式、自动 fetch、检查远端、被拒自动更新、推送对话框两项、受保护分支含 `*` 通配） |
| `src-tauri/src/recovery.rs` | 新增回收来源 `checkoutSmart` / `checkoutForce` / `updateAutostash`（回收记录注明是哪种操作之前保存的） |
| `src-tauri/src/commands.rs`、`src-tauri/src/lib.rs` | 扩展 `history_log_page` 的 `filter`；新增 `history_merge_files`、`history_merge_view`、`repo_files`、`branches_overview`、`checkout_branch`、`create_branch`、`fetch_remotes`、`update_project`、`push_preview`、`push_branch`、`check_remote` |
| `src/lib/guardedWrite.ts`（新增） | 通用受保护写编排；`src/features/merge/mergeWrite.ts` 改为委托它，行为与 3 个既有 Node 回归一致 |
| `src/lib/ipc.ts` | 新命令的固定类型封装；设置负载新增日志与更新 / 推送字段；命令日志记录新增字段 |
| `src/features/log/`（新增） | `logModel.ts`（筛选 → 请求、日期与引用标签格式化、作者标识、页签键）、`logGraph.ts`（分支图轨道、长边折叠）、`logPager.ts`（分页、去重、跨度与旧响应）、`useLogWorkspace.ts`（按 worktree 保留筛选 / 选中 / 滚动）、`logTabs.ts`（提交 diff 页签）、`LogView.tsx`（工具栏、虚拟滚动表格、详情、预览）、`CommitDiffView.tsx`（只读统一 / 左右对照）、`CommitMergeView.tsx`（merge 三栏只读）、`historyHighlight.ts` |
| `src/features/branches/`（新增） | `useBranches.ts`（概览加载、自动 fetch、检查远端定时）、`BranchWidget.tsx`（顶栏组件、更新 / 推送按钮）、`BranchDialogs.tsx`（新建分支、检出被拒、推送对话框）、`remoteWrite.ts`（受保护写编排与三个纯决策：推送对话框、被拒后更新、通知文案） |
| `src/features/commands/CommandLogPanel.tsx`（新增） | Git 命令日志面板（列表 + 输出、复制单条 / 全部、轮询更新） |
| `src/components/app/EditorTabs.tsx` | 固定“日志”页签（不可关闭）+ 提交 diff 页签（“文件名（短哈希）”，同提交同文件复用）；日志 / 提交页签激活时隐藏改动 diff 工具条按钮 |
| `src/components/app/Shell.tsx` | 顶栏插入分支组件与“视图”菜单，状态栏加“Git”按钮与自动 fetch 失败提示 |
| `src/components/app/SettingsDialog.tsx` | 新增“日志”页与“Git › 更新与推送”页 |
| `src/components/app/CommitComposer.tsx` | 提交区新增“提交并推送”按钮（⌥⌘K） |
| `src/components/app/RecoveryPanel.tsx` | 三种新回收来源的标签 |
| `src/components/ui/popover.tsx` | `Dropdown`（自带开关，打开 / 关闭都跟随 Base UI 触发点击）与可选 `onOpen`；修复“给触发按钮加 onClick 会与触发器自身切换互相抵消”的隐患 |
| `src/lib/i18n/zh-CN.ts` | 日志 / 分支 / 远端 / 提交 diff / 命令日志 / 新设置项的简体中文文案 |
| `src/index.css` | 日志表格、分支图、引用标签、命令日志、对话框按钮、三栏只读视图的样式（沿用既有颜色主题变量与 Codicon） |
| `src-tauri/tests/history_readonly.rs` | 新增 12 个筛选 / merge 回归（原 30 + 新增 12 = 42） |
| `src-tauri/tests/branch_remote.rs`（新增） | 26 个分支 / 远端 / 命令日志回归（本地裸仓库作 `file://` 远端） |
| `tools/test-log-model.mjs`、`tools/test-remote-write.mjs`（新增） | 18 + 10 个 Node 回归（筛选 → 请求、分支图、分页、受保护写、推送决策） |

## 13. 接口与调用示例

新增 / 扩展的命令（全部只接收已登记 worktree ID 与结构化参数）：

| 命令 | 作用 |
| --- | --- |
| `history_log_page`（扩展 `filter`） | 结构化筛选分页：文字 / 哈希前缀（关闭正则时由 Rust 扫描匹配提交信息含正文，或 OID 前缀）、Git 扩展正则、分支 / 标签多选、作者多选、日期（最近 N 天 / 区间）、路径（字面，目录含其下） |
| `history_merge_files` | merge 提交的改动：`combined`（文件集合与 `git diff-tree -c` 一致）/ `byParent`；超过两个父提交时默认按父提交分组并如实返回 `mode` |
| `history_merge_view` | merge 提交的三栏只读内容（父提交 1 / 合并结果 / 父提交 2）；路径必须属于该提交的改动文件；不是两个父提交时明确说明 |
| `repo_files` | 当前 HEAD 树的文件路径（路径筛选候选，上限 20 000） |
| `branches_overview` | 本地 / 远端分支、上游、领先落后、当前分支、游离 HEAD 短哈希、进行中操作、`user.name` / `user.email` |
| `checkout_branch` | `{ kind: "local", name }` / `{ kind: "remote", remote, branch }` + `strategy: plain / smart / force`；会被覆盖时返回 `needsConfirmation` 与文件列表 |
| `create_branch` | `{ name, start: head / ref(fullName) / commit(oid), checkout }` |
| `fetch_remotes` | 对所有远端 fetch，标签方式取设置（auto / sync / always / never） |
| `update_project` | 先 fetch 上游，再按设置的更新方式并入；返回状态、并入提交数、冲突、回收记录 |
| `push_preview` / `push_branch` | 推送预览与推送；被拒返回 `rejected`（由前端决定是否「更新后再推送」） |
| `check_remote` | `ls-remote` 比较上游引用（不取回对象） |

调用示例（日志分页与筛选）：

```ts
let page = await ipc.historyLogPage({
  targetId,
  start: { kind: "refs", fullNames: [] },   // 「全部」：本地 / 远端 / 标签 / HEAD 可达
  firstParent: false,
  limit: 100,
  filter: {
    text: "fix build", caseSensitive: false, regex: false,
    branches: ["refs/heads/main"], authors: ["Alice <alice@example.com>"],
    paths: ["src"], date: { kind: "lastDays", days: 7 },
  },
});
// 续页只带游标；游标与目标、遍历方式、筛选一起校验
page = await ipc.historyLogPage({ targetId, firstParent: false, limit: 100, cursor: page.nextCursor!, filter });
```

调用示例（检出与推送，均经 App 的统一写编排）：

```ts
const result = await ipc.checkoutBranch({ worktreeId, target: { kind: "remote", remote: "origin", branch: "main" }, strategy: "plain" });
if (result.kind === "needsConfirmation") {
  // 对话框三选一：智能检出 / 强制检出 / 取消
  await ipc.checkoutBranch({ worktreeId, target, strategy: "smart" });
}
await ipc.pushBranch({ worktreeId, request: { remote: "origin", branch: "main", tags: false, force: "none" } });
```

## 14. 交互实现选择（规格未写到的部分）

规格没写到的交互按「Rebased 已知行为清楚时照做，不清楚时取最简单、可撤销的做法」处理，逐项列出：

1. **提交 diff 视图**：新建只读 `CommitDiffView`（统一 / 左右对照，复用同一套行 / 格 / 色带类名与语法高亮）与 `CommitMergeView`（merge 三栏）。没有把 M4 的 `MergeView` 改成双用途：它是可编辑的 CodeMirror 编辑器（块状态、撤销、应用按钮），只读分支需要拆掉整条写入路径，风险大于收益。视觉与只读语义一致，但没有 M4 合并页签的块按钮 / 环形色带。
2. **软换行**：提交 diff 在开启软换行时按已加载窗口的普通流渲染（不做逐行高度测量），关闭时按固定行高虚拟滚动。默认关，长文件大量行时滚动精度略低于工作区 diff。
3. **合并线拉直**：实现为「超过 24 行的分支线折叠成上下两端的箭头」，点箭头跳到同一条线的另一端（已加载行内）。这是 Rebased Long Edges 的可撤销近似，阈值固定写在 `logGraph.ts`。
4. **哈希前缀**：关闭正则时由 Rust 逐批扫描（每批 200 条，缓存命中列表，仍受 20 000 条跨度限制），匹配完整提交信息（含正文）或 OID 前缀；开启正则时交给 Git 的 `--grep -E`，此时不做 OID 前缀匹配（组合极少用，已记入未决问题）。
5. **日期筛选**：`--since/--until` 用 Git 默认的提交（committer）时间；表格“日期”列按规格显示作者时间（本地时区）。
6. **路径筛选**：`--literal-pathspecs` + `--`，目录按前缀覆盖其下文件，不跟随重命名（M6）。候选来自 `repo_files`（HEAD 树），面板里同时列出目录与文件。
7. **“视图”菜单**：应用原本没有菜单栏，在顶栏新增“视图”下拉（当前只有「打开 Git 命令日志」），与状态栏“Git”按钮一起作为入口。
8. **命令日志更新方式**：面板打开时每 800 ms 轮询 `command_log_snapshot`（比较最新序号），不新增事件通道；面板关闭时不轮询。
9. **检出被拒的文件集合**：由 Rust 预判——`HEAD..目标提交` 之间不同的文件与本地有改动（含已暂存 / 未跟踪）的文件取交集，而不是解析 Git 的拒绝文本；Git 自己仍是最终兜底，两侧判断不同的极端情况会表现为普通 Git 错误提示。
10. **智能检出**：先存回收区（会被覆盖的文件与本地未跟踪 / 未暂存改动），再 `git stash push --include-untracked`、检出、`git stash pop`；恢复冲突时保留 stash 条目（Git 的 `pop` 失败本来就不删）并列出冲突文件。
11. **强制检出**：先存回收区（**全部**已跟踪文件的本地改动 + 会被覆盖的未跟踪文件），再 `git checkout --force`；未跟踪且不被覆盖的文件不动。
12. **新建分支失败提示**：对话框显示通用失败提示，后端原始原因（Git 的 `check-ref-format` 输出）在通知里；因为统一写编排不把错误透传给对话框（错误归属与待核对优先）。
13. **推送被拒**：开启「推送被拒时自动更新」时更新一次再推一次，仍失败只报告；关闭时弹对话框提供“更新后再推送 / 关闭”，不自动重试。
14. **自动 fetch / 检查远端**：由界面定时（每 10 分钟）触发，自动 fetch 走同一套受保护写编排但失败只在状态栏提示；有写操作时跳过本轮、不排队。检查远端一开始也立即执行一次，便于看到状态。
15. **引用变化后重新查询**：提交、检出、fetch、更新、推送由 App 递增引用版本立即触发；外部 Git 操作由日志每 5 秒比较引用 OID 快照发现。所选提交仍在结果中时保持选中，滚动位置按 worktree 保留。
16. **提交并推送**：提交成功且结果确定后才推送；“显示推送对话框”与“只在受保护分支显示”两项按 B06 / B08 组合判断（纯函数，有 Node 回归）。
17. **日志刷新与页签**：日志面板在非激活时保持挂载（`hidden`），以便保留筛选 / 选中 / 滚动；提交 diff 页签全部关闭后回到日志，不抢工作区 diff 的激活状态。

## 15. 验收项与测试对应

| 规格项 | 实现 | 回归 |
| --- | --- | --- |
| L01 位置与生命周期 | 固定日志页签（`EditorTabs` + `mainView`）、按 worktree 保留状态（`useLogWorkspace`）、引用变化重查（`refsRevision` + 5 秒引用快照）、提交页签分型（`logTabs`） | `tools/test-log-model.mjs`（分页 / 页签键）、`tools/test-editor-tabs.mjs` |
| L02 表格 | 列与默认可见性、日期格式、虚拟滚动、接近末尾取下一页、跨度上限单独提示、键盘与原生右键菜单 | `test-log-model.mjs`（日期、列、分页、跨度、预取）、`history_readonly.rs`（分页与跨度） |
| L03 分支图 | 前端增量轨道算法（`logGraph.ts`）、只看第一父提交、长边折叠 | `test-log-model.mjs`（轨道分配、增量一致、first-parent、长边标记） |
| L04 筛选 | 结构化筛选与 Rust 参数组合、哈希前缀、正则、区分大小写、分支 / 作者 / 日期 / 路径、组合、无结果与重置、游标绑定筛选 | `history_readonly.rs`：`text_filter_*`、`regex_filter_uses_git_extended_regex`、`branch_author_and_date_filters_narrow_the_same_query`、`path_filter_is_literal_and_covers_directories`、`cursor_bound_to_filter_and_first_parent_traversal`、`filter_values_are_validated`；`test-log-model.mjs`（请求构造与筛选标识） |
| L05 详情 / merge / 预览 | 改动文件树、详情、merge 默认合并视图与按父提交分组、三栏只读、双击 / Enter / ⌘D、预览开关与位置 | `history_readonly.rs`：`merge_files_default_matches_diff_tree_combined_set`、`merge_files_by_parent_lists_all_changes_per_parent`、`octopus_merge_defaults_to_parent_groups`、`merge_view_reads_three_versions_and_rejects_unrelated_paths` |
| L06 日志设置 | 设置页 + 表头菜单；随现有设置持久化 | `cargo test --lib`（设置校验）、`tsc`（字段类型） |
| B01 共同规则 | `guardedWrite` + `runRemoteWrite`、Rust 同 worktree 写锁、网络 10 分钟上限、不等待凭据、不改 Git 配置、回收来源 | `branch_remote.rs`：`write_lock_blocks_a_second_operation_on_the_same_worktree`、`network_timeout_is_reported_as_needs_verification`、`missing_credentials_fail_immediately_without_waiting_for_input`、`command_log_records_worktree_arguments_exit_code_and_output`；`test-remote-write.mjs`（门禁、保护失败不发 IPC、归属、收尾一次、待核对阻止退出） |
| B02 分支组件与新建分支 | 概览、搜索、本地 / 远端、领先落后、检出（本地 / 远端跟踪 / 同名不跟踪拒绝 / 其他 worktree 占用拒绝）、智能 / 强制检出、新建分支三种起点与名字校验 | `branch_remote.rs`：`branch_overview_lists_current_local_and_remote_branches`、`checkout_remote_branch_creates_tracking_branch_and_reuses_existing_one`、`same_named_local_branch_without_tracking_is_not_overwritten`、`checkout_is_refused_when_another_worktree_holds_the_branch`、`plain_checkout_reports_files_that_would_be_overwritten`、`staged_changes_that_would_be_overwritten_also_require_confirmation`、`smart_checkout_saves_recovery_stashes_and_restores_changes`、`smart_checkout_conflict_keeps_the_stash_and_reports_conflicts`、`force_checkout_saves_all_tracked_changes_left_unsaved_on_target`、`create_branch_validates_name_and_supports_three_starts` |
| B03 fetch / 自动 fetch / 检查远端 | 标签方式参数、失败显示原文；界面每 10 分钟自动 fetch（跳过写操作）；检查远端只比较引用 | `branch_remote.rs`：`fetch_follows_the_tag_mode_setting`、`fetch_reports_git_failure_with_raw_output`、`check_remote_compares_refs_without_fetching_objects`；`test-remote-write.mjs`（自动 fetch 走同一编排） |
| B04 更新 | 三种方式、先 fetch 上游、无上游 / 游离 HEAD 说明原因、先存回收区再 autostash、merge 冲突进入进行中操作、rebase 冲突提示终端 | `branch_remote.rs`：`update_merge_and_rebase_apply_the_upstream`（含按分支默认）、`update_without_upstream_or_on_detached_head_is_explained`、`update_saves_dirty_changes_to_recovery_before_autostash`、`update_merge_conflict_enters_in_progress_state` |
| B05 推送 | 预览提交列表、目标远端 / 分支、推送标签、`--force-with-lease` 带期望值、受保护分支禁用、非快进被拒、被拒后更新再推一次 | `branch_remote.rs`：`push_new_branch_records_upstream_and_counts_commits`、`push_is_rejected_when_the_remote_has_new_commits`、`force_with_lease_refuses_to_overwrite_unfetched_remote_commits`、`protected_branch_refuses_force_push`；`test-remote-write.mjs`（对话框决策、只更新一次） |
| B06 提交并推送 | 提交成功后按设置直接推送或弹对话框；失败 / 待核对不推送 | `test-remote-write.mjs`（`shouldShowPushDialog` 四种组合）、`App.tsx` 中提交成功分支才调用推送 |
| B07 Git 命令日志 | 时间 / worktree / 参数 / 退出码 / 耗时 / 输出 64 KB、最近 1000 条、凭据打码、随新命令更新 | `cargo test --lib`：`mask_credentials_hides_url_userinfo_and_secret_values`、`command_log_keeps_recent_records_and_caps_output`；`branch_remote.rs`：`command_log_records_worktree_arguments_exit_code_and_output`、`external_git_helper_is_used_for_worktree_label_and_docs`、`missing_credentials_fail_immediately_without_waiting_for_input`（参数打码） |
| B08 更新与推送设置 | 设置页与默认值（merge / auto / 关 / 关 / 关 / 开 / 关 / master、main） | `cargo test --lib`（设置默认值、受保护分支通配）；`tsc`（负载类型） |
| V 前端写入口 | 生产协调器（`CloseCoordinator` / `WriteGuardOrchestrator` / `OperationResultGate` / `CloseDriver`）与生产编排 | `tools/test-remote-write.mjs`、`tools/test-merge-write.mjs` |

## 16. 资源限额与超时

- **网络命令**（fetch / 更新 / 推送 / 检查远端）：单次上限 600 秒（原 120 秒的普通上限不变）；超时按“结果待核对”返回并阻止静默退出，不自动重试，不提供取消。
- **凭据**：网络命令一律 `GIT_TERMINAL_PROMPT=0`、stdin 为空、`SSH_ASKPASS_REQUIRE=never`；不覆盖用户的 ssh 命令 / askpass 程序，不改 Git 配置。
- **命令日志**：内存中最多 1000 条，单条输出 64 KB（原始读取上限 512 KB，超出标明截断），参数与输出统一打码。
- **日志分页**：前端每页 100 条（Rust 上限 200），一次查询跨度 20 000 条；扫描式文字筛选每批 200 条、命中结果缓存在查询内；每目标最多 8 个未走完查询、全应用最多 4 个历史目标（沿用任务 01）。
- **其它**：推送预览最多 200 条提交；路径候选最多 20 000 条（界面显示前 200 条）；单提交改动文件最多 5 000；回收记录沿用 14 天保留。
- **回收区**：检出（智能 / 强制）与更新（autostash）在写入前保存，回收记录来源分别为 `checkoutSmart` / `checkoutForce` / `updateAutostash`。

## 17. 验证（集中执行）

证据目录：`/private/tmp/revia-m5-start.zEV4wp/batch2-GzR75r/`（本批独占 `mktemp -d` 子目录）。
环境：`node v26.5.0`、`pnpm 12.14.0`、`cargo 1.99.1`、`git 2.56.0`，基线 `3c9a634`（见 `environment.txt`）。

| 检查 | 命令 | 退出码 | 结果 |
| --- | --- | --- | --- |
| 依赖 | `pnpm install --frozen-lockfile` | 0 | `install.log` |
| 类型检查 | `pnpm exec tsc -b` | 0 | 通过（`tsc.log`，空输出） |
| lint | `pnpm lint` | 0 | 0 error；仅 7 条继承的 `tools/test-refresh-gate.mjs` 警告（`lint.log`） |
| 前端构建 | `pnpm build` | 0 | 通过（`build.log`，仅既有 chunk 体积提示） |
| Rust 测试 | `/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml` | 0 | 13 个测试二进制：**517 通过、0 失败、1 忽略**（`cargo-test.log`） |
| Node 回归 | 逐个 `node --test tools/test-*.mjs`（15 个） | 0 ×15 | 15/15 通过（`node-summary.txt` 与 `node-*.log`） |

Rust 明细：lib **237**、`branch_remote` **26**（新增）、`history_readonly` **42**（原 30 + 新增 12）、`commit_flow` 12、`discard_recovery` 71（1 忽略）、`file_staging` 20、`hunk_staging` 24、`merge` 15、`refresh_isolation` 15、`review_marks` 31、`segment_staging` 21、`session_flow` 3。

Node 明细：`test-commit-state` 8、`test-editor-tabs` 19、`test-ipc-diagnostics` 11、`test-jump-dom-probe` 11、`test-log-model` **18**（新增）、`test-merge-appearance` 2、`test-merge-model` 7、`test-merge-write` 8、`test-refresh-gate` 180、`test-remote-write` **10**（新增）、`test-row-heights` 6、`test-row-window-store` 25、`test-sync-scroll` 17、`test-ui-input` 3、`test-visible-highlight-lines` 13。

新增 / 扩展回归清单：

- `src-tauri/tests/history_readonly.rs`（+12）：文字 / 大小写 / 哈希前缀、扫描分页与历史末尾、Git 扩展正则、分支 + 作者 + 日期组合、路径字面（目录、`--help`、`a[1].txt`、`*`）、游标绑定筛选与 first-parent、筛选取值校验、merge 合并文件集合与 `diff-tree -c` 一致、按父提交分组、octopus 分组与三栏拒绝、三栏内容读取与无关路径拒绝、筛选读取前后 Git 状态不变。
- `src-tauri/tests/branch_remote.rs`（新，26）：概览、远端检出 / 复用 / 同名不跟踪拒绝 / 其他 worktree 占用、三种起点与名字校验、plain 需确认（含已暂存）、智能检出（成功恢复 / 冲突保留 stash）、强制检出存回收区、fetch 四种标签方式与失败原文、更新 merge / rebase / 按分支默认 / 无上游 / 游离 HEAD / 脏工作区 autostash / merge 冲突、推送新分支记录上游与提交数、非快进被拒后更新再推、`--force-with-lease` 拒绝未取回的远端提交、受保护分支拒绝强制推送、网络超时（`ext::sleep` + 缩短超时）、凭据缺失立即失败且日志打码、检查远端不取对象、写锁互斥、命令日志归属与输出。
- `src-tauri/src/git/mod.rs` 单元：凭据打码（URL userinfo、`--password`、`extraHeader`、多行、OID 与普通文本不动）、命令日志容量与输出截断。
- `tools/test-log-model.mjs`（新，18）：筛选 → 请求、日期预设与校验、筛选标识、哈希识别、日期格式（今天 / 昨天 / 绝对）、引用标签顺序与突出、作者标识、分支候选、页签键与标题、分支图轨道与增量一致、first-parent、长边折叠、分页去重 / 续页游标 / 跨度 / 换筛选 / 换目标 / 预取。
- `tools/test-remote-write.mjs`（新，10）：受保护写（同 worktree 拒绝、保护失败不写、归属、收尾一次、成功结果回传、待核对登记退出询问）、与合并写入共用编排、提交并推送对话框决策、被拒只更新一次、更新 / 推送通知文案。

未执行：真实桌面验收（截图、窗口可见时测量）、与 Rebased 1.1.22 并排对照、Release 打包与 D06、Windows 编译、长测、规划方独立审查。

## 18. 未完成项与需要确认的问题

未做（留待规划方）：

- 规划方对本批的独立审查（任务 02、03 `## Comments`）、真实桌面与 Rebased 1.1.22 并排对照、Release 打包、D06、Windows 编译、长测。
- 合并回 `m5` 与后续里程碑安排。

需要产品 / 规划方确认：

1. **merge 三栏只读视图**是否必须复用 M4 的可编辑 `MergeView`（本批用独立只读组件，视觉一致但没有块按钮 / 环形色带）。若要完全一致，需要把 `MergeView` 拆出「只读渲染 + 可选编辑」两层。
2. **正则 + 哈希前缀**同时开启时是否也要按 OID 前缀匹配（当前正则交给 Git `--grep`，不做 OID 前缀）。
3. **合并线拉直**的折叠阈值（现为 24 行）与箭头样式是否需要按 Rebased 调整。
4. **日期筛选口径**：是否按作者时间而非提交时间（现按 Git 默认的提交时间；列显示作者时间）。
5. **路径筛选**是否需要在 M5 就跟随重命名（现按规格不跟随）。
6. **提交并推送**失败后的行为：现在提交成功、推送失败只报告；是否需要“重试推送”入口。
7. **检出被拒文件集合**：Rust 预判与 Git 判断可能有极小差异，是否需要改为解析 Git 输出或试运行后再确认。

已知限制：

- 提交 diff 的软换行模式不做逐行高度测量；大文件时滚动精度低于工作区 diff。
- 日志表格行高固定 26 px，字号缩放大于默认时行可能偏紧（未做行高测量）。
- diff 预览只取前 200 行（面板内不是完整编辑器；完整内容在提交 diff 页签里）。
- 分支图在前端计算：极深历史（数十万分页）时每行数据量随轨道数增长，未做轨道压缩。
- 自动 fetch / 检查远端的定时由界面驱动，应用最小化或后台时仍按定时器运行；没有跨启动的“上次 fetch 时间”记录。
- 命令日志面板随新命令更新依赖 800 ms 轮询；应用卡顿时列表可能延迟。
- `git checkout` 而非 `git switch`：与既有最低 Git 版本（2.11）兼容，`--force` / `-b --track` 行为一致。
- 网络超时回归使用 `ext::sleep` 远端，需要测试仓库自己配置 `protocol.ext.allow=always`（只在测试仓库里，不改用户配置）。

## 19. 提交

本批提交在 `m5-batch2`，提交说明以 `M5：` 开头；未推送、未合并、未创建 PR，未改动主目录与其他 worktree，未切换 `main` / `m5` 分支。

---

# M5 第二批返修（规划方审查 9 项，Q113 唯一一次返修）

Status: 交付待复核（9 项全部修完；规划方复核、真实桌面对照、Release 打包与 D06 仍未做）

日期：2026-10-27。分支 `m5-batch2`，返修前 HEAD `206e093`（规划方审查意见提交）。
只做任务 02、03 `## Comments` 里 Claude 审查的 9 项，不改已确认的决策、不改合并功能、不改任务状态与验收勾选。
每项都在仓库测试里补了回归；其中 1、2、4、8 项另做了**反向验证**：把对应修复临时去掉后，目标回归必须失败（结果见第 20.3 节）。

## 20.1 逐项改动

| 项 | 改了什么 | 回归 |
| --- | --- | --- |
| 1 P2 强制推送的受保护判断用错了分支 | `remote_ops::push` 改为**目标远端分支**与本地分支任一受保护都拒绝（先报目标分支名）；`push_preview` 的 `protected` / `forceAllowed` 同样按目标分支计算，`forceReason` 指出具体是哪个分支 | `src-tauri/tests/branch_remote.rs`：`force_push_is_refused_when_the_target_remote_branch_is_protected`（本地 `feature` 以 `--force-with-lease` 推到远端 `main` 被拒，远端 `main` 不变）、`push_preview_treats_the_target_branch_as_protected`（本地 `myfix` 跟踪 `origin/main` → 预览标受保护、不允许强制；改成跟踪 `myfix` 后允许） |
| 2 P2 每次推送都重写上游配置 | `set_upstream` 改为：没有上游，或用户请求的远端 / 分支与现有上游不同时才为真。界面上「目标不变」的常规推送不再带 `--set-upstream`，因此不会重写 `branch.<name>.remote/merge` | `branch_remote.rs`：`push_does_not_rewrite_the_upstream_config_when_target_is_unchanged`（显式带目标分支推送同一上游：`upstream_recorded = false`、命令日志里没有 `--set-upstream`、`.git/config` 字节不变；把目标改成 `copied` 时才记录上游） |
| 3 P3 “检查远端”随引用变化反复联网 | `useBranches.ts` 检查远端的 effect 依赖改为 `[checkRemote, worktreeId]`（只在开启 / 切换 worktree 时查一次，之后每 10 分钟一次），不再依赖引用版本；新增 `clearIncoming()`，`App.tsx` 在 fetch 与更新成功后各调用一次清除标记（不额外联网核对） | `tools/test-remote-write.mjs`：`检查远端只在开启 / 切换 worktree 时跑，引用变化不再联网`（effect 依赖不含 `refreshKey`、存在 `clearIncoming`、App 里清除点正好两处） |
| 4 P3 `git checkout` 参数后补 `--` | 本地分支检出、`-b … --track …`、智能检出与强制检出的 `git checkout` 参数末尾统一追加 `--`，避免与同名文件 / 目录混淆 | `branch_remote.rs`：`checkout_separates_paths_with_double_dash`（同名分支与文件 `topic`：检出切到分支、文件内容不被 pathspec 覆盖、命令日志 `checkout` 参数以 `--` 结尾） |
| 5 P3 去掉顶栏新增的“视图”下拉 | 删除 `Shell.TitleBar` 的 `viewMenu` 属性、App 里的视图菜单与 `viewMenuOpen` 状态、`messages.toolbar.viewMenu` 文案；Git 命令日志保留状态栏入口，菜单入口随 [backlog 03](../../backlog/03-native-menu-bar.md) 的原生菜单一起做 | `test-remote-write.mjs`：`Git 命令日志只有状态栏入口，顶栏没有“视图”下拉`（`Shell.tsx` 不含 `viewMenu`、保留 `onOpenCommandLog` 与状态栏按钮；`App.tsx` 不含 `viewMenu`） |
| 6 P3 新建分支失败时对话框直接显示 Git 原因 | `performRemoteWrite` 改为返回 `{ ok, result } / { ok, error }`，不再丢弃错误；新增纯函数 `branchFailureDetail(error)`（后端原话优先，没有消息才退回通用提示），`runCreateBranch` 用它设置对话框错误 | `test-remote-write.mjs`：`新建分支失败时对话框显示后端（Git）给出的原因`（原样返回 `分支名不合法：…`，空消息退回通用提示）、`新建分支对话框的错误来自写编排的返回，不是通用提示`（源码断言用 `branchFailureDetail(outcome.error)`、编排返回类型带 error） |
| 7 P2 去掉日志每 5 秒的引用轮询 | 删除 `LogView` 的 5 秒 `history_refs` 定时器；新增纯模块 `src/features/log/refsWatch.ts`（`RefsWatch` 记住引用签名，变化才重查）与 `refsWatchRevision` 属性：App 在**文件监视事件被消费时**递增代号（`watch.rs` 已覆盖 refs / packed-refs / HEAD），日志这时才读一次引用、签名变化才重查；内部写操作仍用 `refsRevision` | `tools/test-log-model.mjs`：`引用签名：同样的引用不触发重查，引用移动才触发`（首份快照、顺序无关、分支前进、引用删除、换目标重置）、`日志不再定期轮询 history_refs，只在监视事件里读一次`（`LogView.tsx` 无 `setInterval`、只有两处读取引用、监视事件 effect 比较签名，App 在 `notifications.consume` 内递增代号并传给日志） |
| 8 P3 正则模式下作者值被当成扩展正则 | 作者值一律按字面匹配：正则模式（`--extended-regexp` 生效）下转义成等价的扩展正则；非正则模式由 `log_args` 加 `--fixed-strings` 并按原值传（此前把转义值交给 Git 的**基本**正则，BRE 里 `\+` 是“一次或多次”，作者名带 `+` 时匹配错） | `src-tauri/tests/history_readonly.rs`：`author_filter_stays_literal_in_both_matching_modes`（`A+B` 作者在两种模式下都只命中自己，不误伤 `AAAB`；并反证 `AAAB` 用 `A+B` 正则会同时命中，说明转义生效） |
| 9 P3 `guardedWrite.ts` 注释指向不存在的文件 | 注释改为 `tools/test-remote-write.mjs`（实际存在的回归文件） | `test-remote-write.mjs`：`共用写编排的注释指向实际存在的回归文件`（含 `test-remote-write.mjs`、不含 `test-guarded-write`，且文件真实存在） |

审查同时确认接受的既有做法（本次未改，保持现状）：merge 三栏使用独立只读组件、正则模式不做哈希前缀匹配（已写在报告第 14、18 节）；日期筛选按提交时间、合并线拉直 24 行阈值、提交 diff 软换行近似做法先接受，桌面验收时与 Rebased 对照。

## 20.2 涉及文件

- Rust：`src-tauri/src/remote_ops.rs`（1、2、4）、`src-tauri/src/history.rs` 与 `src-tauri/src/git/history.rs`（8）。
- 前端：`src/features/branches/useBranches.ts`、`src/features/branches/remoteWrite.ts`、`src/features/log/LogView.tsx`、`src/features/log/refsWatch.ts`（新增）、`src/App.tsx`、`src/components/app/Shell.tsx`、`src/lib/guardedWrite.ts`、`src/lib/i18n/zh-CN.ts`。
- 测试：`src-tauri/tests/branch_remote.rs`（+4 → 30）、`src-tauri/tests/history_readonly.rs`（+1 → 43）、`tools/test-log-model.mjs`（+2 → 20）、`tools/test-remote-write.mjs`（+5 → 15）。

## 20.3 反向验证（去掉修复后回归必须失败）

证据：`negative-controls.log`（同一返修证据目录）。

| 项 | 临时改动 | 结果 |
| --- | --- | --- |
| 1 | 删掉 `push` 里对 `target_branch` 的受保护检查 | `force_push_is_refused_when_the_target_remote_branch_is_protected` 失败（推送没有被拒绝） |
| 2 | 恢复 `set_upstream = upstream.is_none() \\|\\| request.branch.is_some()` | `push_does_not_rewrite_the_upstream_config_when_target_is_unchanged` 失败（命令日志里出现 `--set-upstream`） |
| 4 | 去掉普通 / 强制检出路径的 `--` | `checkout_separates_paths_with_double_dash` 失败（`checkout` 参数末尾不是 `--`） |
| 8 | 恢复“总是转义 + 不加 `--fixed-strings`” | `author_filter_stays_literal_in_both_matching_modes` 失败（非正则模式把 `AAAB` 当成 `A+B` 命中） |

3、5、6、7、9 项是前端行为与注释、菜单类改动，回归以 `tools/test-*.mjs` 的纯函数与源码接线断言覆盖（与仓库既有做法一致，例如 `test-merge-write.mjs` 的“三个界面入口都经 App 的受保护写入”），未做反向验证。

## 20.4 集中检查（返修后重跑）

证据目录：`/private/tmp/revia-m5-start.zEV4wp/rework-batch2-QBs0bF/`（本次独占 `mktemp -d` 子目录）。
环境：`node v26.5.0`、`pnpm 12.14.0`、`cargo 1.99.1`、`git 2.56.0`；返修基线 `206e093`（见 `environment.txt`）。

| 检查 | 命令 | 退出码 | 结果 |
| --- | --- | --- | --- |
| 依赖 | `pnpm install --frozen-lockfile` | 0 | `install.log` |
| 类型检查 | `pnpm exec tsc -b` | 0 | 通过（`tsc.log`，空输出） |
| lint | `pnpm lint` | 0 | 0 error；仍只有 7 条继承的 `tools/test-refresh-gate.mjs` 警告（`lint.log`） |
| 前端构建 | `pnpm build` | 0 | 通过（`build.log`，仅既有 chunk 体积提示） |
| Rust 测试 | `/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml` | 0 | 13 个测试二进制：**522 通过、0 失败、1 忽略**（`cargo-test.log`） |
| Node 回归 | 逐个 `node --test tools/test-*.mjs`（15 个） | 0 ×15 | 15/15 通过（`node-summary.txt` 与 `node-*.log`） |

Rust 明细：lib 237、`branch_remote` **30**、`history_readonly` **43**、`commit_flow` 12、`discard_recovery` 71（1 忽略）、`file_staging` 20、`hunk_staging` 24、`merge` 15、`refresh_isolation` 15、`review_marks` 31、`segment_staging` 21、`session_flow` 3。
Node 明细：`test-log-model` **20**、`test-remote-write` **15**，其余 13 个脚本与上一轮一致（8 / 19 / 11 / 11 / 2 / 7 / 8 / 180 / 6 / 25 / 17 / 3 / 13）。

未执行：规划方复核、真实桌面验收、与 Rebased 1.1.22 并排对照、Release 打包、D06、Windows 编译、长测。

## 20.5 提交

返修提交在 `m5-batch2`，提交说明以 `M5：` 开头；未推送、未合并、未创建 PR，未改动主目录与其他 worktree，未切换 `main` / `m5` 分支。


## 21. Codex 规划方接手复核与修复（2026-10-27）

Pi 的 Q113 唯一一次返修为 `b6cd00e`；规划方以 `206e093...b6cd00e` 两轴复核，
后续以 `b6cd00e` 对照接手修复，不再向 Pi 追加返修。源码为本节同一提交内的改动。

### Standards

发现远端标记仍可能跨 worktree 被迟到的 fetch/update 清除，同一 worktree 的分支/上游变化也没有失效旧检查。
现由生产 `IncomingState` 统一持有 worktree 激活代际、分支身份代际与检查序号，
App 在操作发起时捕获完成凭据；切换目标、分支、上游或完成清除后旧响应均不得改写当前标记。
首次概览只建立身份，不错误失效先前已发起操作的凭据；真正的后续分支改变仍递增代际。
联网节奏仍是开启/切换一次和每十分钟一次，没有因本地引用刷新额外联网。

### Spec

- 正则开关打开而搜索文字为空时没有真正启用 ERE：作者转义现在依据实际 `git_filter.grep` 模式，保持字面匹配。
- `PushPreview.protected` 恢复“当前本地分支受保护”语义，供 B06 提交并推送选择对话框；强制推送是否允许仍检查本地与远端目标两端，保护未弱化。
- 去掉日志定时轮询后，手动/前台重读、提交/amend 写后重读和恢复路径都必须刷新引用。
  通知现收口到通过门禁的 `acceptSnapshot`；消费者只重读本地引用，不形成工作区刷新循环。
- 引用签名包含当前分支标记，日志查询包含 `headCommit`，同 OID 切分支和游离 HEAD 变化不再漏刷。

末次独立复核另指出首次概览凭据失效和提交/恢复路径漏通知，均已在本节修复中闭合；
新的独立只读复核确认上述边界无新增确定缺陷。M4 弹层修复同时独立核查，但不在本分支混入 M4 后续提交。

### 集中验证

| 检查 | 真实结果 |
| --- | --- |
| `git diff --check` | 退出码 0 |
| `node --test tools/test-log-model.mjs tools/test-remote-write.mjs` | **44/44** 通过（21 + 23） |
| `pnpm lint` | 退出码 0，保留 7 条原有 unused-vars 警告 |
| `pnpm build`（含 `tsc -b`） | 退出码 0 |
| `cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --test history_readonly --test branch_remote` | **73/73** 通过（43 + 30） |

作者回归扩展到 None/空串/空白/有文字、正则开关、加号作者名、点号邮箱与完整身份；
远端回归增加真实生产状态模块的 8 个受控时序用例；引用回归覆盖统一通知接线和同 OID 切分支。
Rust 首轮失败是新增测试未建立 `origin/myfix`，导致预览按无上游回退；现先在自建本地裸仓库创建真实引用，
并断言 `has_upstream` 和目标名字，生产代码未为错误 fixture 放宽行为。

证据（含固定基点、修改文件 SHA256、完整差异、命令、退出码与原始 stdout/stderr）：

- 首轮（含失败原件）：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-scoped-build-8uto0sb9`
- 闭环后通过：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m5-boundary-checks-t0966zsb`

本轮按受影响范围检查，未重跑整套 Rust 或全部 Node 脚本。任务 02、03 转为 `ready-for-human`，
表示独立静态审查与相关自动检查已完成；真实桌面、Rebased 对照、Release、D06 和里程碑集成仍未完成，不能标 done。
未推送、未合并、未开 PR。Paseo 0.13.2 可达、Pi provider available；本次没有为试模型创建无关实施会话。

## 22. M5 集成与桌面验收（2026-10-28，进行中）

用户同意先集成 M4 最新修复与 M5 第二批，再构建普通模式 Release 并验收完整工作流。
操作目录为 `/Volumes/T7 Shield/project/Revia`，保持原有 `m5` 分支，不切换 `main`。
输入固定为 `m5@3c9a634`、`m5-batch2@8b95b5e`、`m4@f546a06`；M4 的整体性能与长测
仍未验收，本次集成不改变该状态。原始 Git 命令、退出码和合并前身份记录在
`/private/tmp/revia-m5-integration-20u816pb/`。

- 先将 `m5` 快进到已复核的第二批，再合并 M4；四个文本冲突为 `App.tsx`、
  `EditorTabs.tsx`、`popover.tsx` 和 `test-merge-appearance.mjs`。
- 弹层保留 M4 必传的打开回调与缩放定位层，同时保留 M5 筛选所用的 `Dropdown`。
  页签保留 M4 窄窗口分行、自动露出，并覆盖日志与历史页签；历史页签不显示工作区写工具条。
- 工作区阅读位置恢复只在工作区 diff 激活时有效，切日志或历史页签使待恢复锚点失效。
  引用刷新、远端标记代际、历史只读身份与写门禁全部保留。现有 Node 脚本补集成接线回归。

独立审查、集中检查、固定包桌面与 Rebased 对照待执行；本节不代表验收通过。
后续 Git 写操作仅针对本轮独占临时 fixture 和本地裸仓库，不操作真实远端。
没有合入 main，没有推送，没有 PR。

### 22.1 集成固定点与独立审查

集成提交为 `d6271566943e45bb783d7d68923c35e6d834c17e`。新的两个独立子代理分别按
Standards / Spec 审查 `eaf14d2...d627156`，没有沿用第 21 节的通过结论。

- Standards：两项 P1（推送续步与检出确认丢失原 worktree），一项 P2（远端快捷键穿透输入/模态）。
- Spec：三项 P2（默认日志/历史页签被刷新抢激活，历史行缓存缺读取身份，400 行边界持续露白）。
- 新的只读历史补审另发现：Standards 一项 P2（原生滚动违反 ADR-0008），Spec 一项 P2
  （左右对照插空行配对违反 H03）。两轴合计各 4 项，分别保留，不混排；该固定点不通过。

### 22.2 Codex 接手修复

- 推送、检出与新建分支保存原 worktree 和激活代际；预览、确认、更新与重试不读取当前窗口目标。
  A -> B -> A 也不能使用旧确认。提交并推送意图属于单次提交，不再放在全局布尔值中。
- 提交到自动推送、推送到自动更新/唯一重试共用同一写生命周期。后续步骤使用受限原目标 lease，
  拒绝错目标、并发或已释放 lease；待核对传回原生命周期。等待退出不会从步骤间隙越过。
  续推需要人工确认时明确保留窗口，不把写锁跨人工确认持有。
- 远端快捷键隔离输入框、按钮与模态；提交信息框只明确允许提交并推送。首次打开默认日志，
  后台刷新只重绑数据，显式用户打开文件才激活工作区 diff；测量模式显式激活 diff。
- 历史统一/并排共用 M4 的受控滚动渲染器，通过明确的只读数据源分型接入。
  历史行没有工作区身份与写凭据；并排调用 `historyFileSideLayout` / `historyFileSideLines`，
  两侧连续排布，复用色带、同步、折叠与软换行，不再插空白配对。
- 历史缓存纳入目标、文件对象、视图、忽略档位；迟到响应不回填。统一视图覆盖完整可见区间及预读，
  小文件按 Q69 后台取完，大文件缓存有界。高亮独立有界，适配历史接口请求 1 基/响应 0 基的行号。
  共享并排视图订阅高亮更新并将语法类别映射为主题类名，文字优先于颜色请求。

### 22.3 集中检查原件

证据根：`/private/tmp/revia-m5-integration-20u816pb/`，每项含精确命令、退出码、stdout/stderr。
本轮没有改 Rust 生产或测试代码；Cargo 自动生成的 `RowSlice.ts` 行尾空格已按现有生成物规则清理。

| 阶段 | 结果 |
| --- | --- |
| 固定 `d627156` 首轮前端 | 16 个 Node 脚本全部通过，lint / build 退出 0；`checks/frontend-summary.json` |
| 固定 `d627156` Rust lib 首轮 | 238 通过、1 失败；失败为 watcher 收不到 B 的文件事件；`checks/cargo.*` 原件保留 |
| 单独核实 watcher | 同一源码、精确升级权限后该用例通过；`checks/watcher-unsandboxed.*`；不据一次复测断言根因 |
| 尚未执行的 Rust 集成套件 | 11 个套件，289 通过、1 忽略；`checks/rust-integrations.*` |
| 修复后集中检查 | 17 个 Node 脚本全部通过；lint 退出 0，7 条原有 unused-vars 警告；`checks/repair-01/` |
| 修复后首次 build | 退出 2：App 内数字忽略档位尚未转换为历史组件枚举；失败原件保留 |
| 修正接线后只重跑 build | 使用既有 `ignoreWhitespaceOf` 转换，退出 0（含 tsc）；`checks/repair-02/` |

本节同提交包含修复及回归，须再由新的两轴代理固定点复核，通过后才构建 Release 和操作桌面。
新增桌面 fixture 在 `fixtures/`，日志有 214 条初始提交、真实双父合并、未提交改动和 linked worktree；
远端仅 `file://.../fixtures/origin.git`。`other` 克隆已在该本地远端增加一条 1600 行文件提交，
供 fetch/更新与历史跨块检查使用。性能 fixture 独占分配为 `performance-5b9g0bu9/`，不写旧验收仓库。

尚未验收：本次 Release、普通模式完整工作流、弹层、Rebased 对照、D06、长测与四主题。
不能把静态/自动检查或旧 M4 包成绩当成本次桌面通过；M5 仍未完成。

### 22.4 修复固定点独立复核

固定源码为 `066d1e49b68750f582fec6af002a0144166ea9a5`，差异 `d627156...066d1e4`。
前一对复核会话因 429 失败，无结论；本轮另开两个独立只读代理，分别完成并结束。

| 轴 | 结果与范围 |
| --- | --- |
| Standards | 0 项确定违规；核实 Q90 原目标/激活代际、Q91/Q96 同一写生命周期、Q93 快捷键隔离及历史显式只读 |
| Spec | 0 项确定问题；核实 L01 不抢页签、H03/L05 缓存完整身份/跨块/连续并排、B01/B05/B06 原目标续步与失败阻断 |

两轴均为静态只读审查，未运行 GUI，不代表弹层、流畅度、D06 或长测通过。任务 02、03 转
`ready-for-human`；接下来构建本固定源码的普通模式 Release，再操作隔离 fixture 验收。

### 22.5 首个固定 Release 的桌面失败

`066d1e4` 的 `pnpm tauri build --bundles app -- --offline --locked` 退出 0，
二进制 SHA256 为 `b45e5a6a1929aff639a419192556bc8b5ca1e08782785d939bf52bd3db96b51f`。
构建原件 `checks/release-01/`，普通实例 PID 62583，仅设置 `REVIA_ACCEPTANCE=1` 与独立数据目录，
不含测量变量。默认选中了日志，未显示测量面板；分支弹层可见且该位置未被遮挡。

但测试仓库有 214 条提交，日志却显示“这个仓库还没有提交”；约两分钟内命令面板达到 1000 条，
大量重复读取引用。原始截图为 `desktop/revia-empty-log-console.png`，分支弹层为
`desktop/revia-branch-popup.png`。验收失败，已正常退出实例，不把该包当通过版本。

原因：历史目标 effect 依赖 `useCommitTabs()` 每次渲染创建的新容器，目标打开回填又触发渲染，
形成关闭/重开循环。修复只依赖稳定的 `dropTarget` 方法，并保留 worktree、环境代际和错误回调依赖。
新增现有 Node 脚本的生命周期接线回归；这不是完整 React 运行测试，仍须用新 Release 重走同一桌面场景。

修复后最小检查：`git diff --check`、`node --test tools/test-log-model.mjs tools/test-editor-tabs.mjs`
（52/52）、`pnpm lint`（既有 7 条警告）、`pnpm build` 均退出 0，原件 `checks/desktop-repair-01/`。

修复提交 `0a5e6a1118287aa0f20e479870cc81d45ca820c1` 再经两个新的独立只读代理分别复核，
Standards / Spec 各 0 项确定问题；两者均结束，未进行 GUI。构建第二个固定包后继续复测。
退出后核对 fixture：HEAD、214 条提交、工作区/暂存区状态与 `.git/config` SHA256 均保持初始值，
原件 `desktop/ordinary-failed-git-state.json`。

### 22.6 第二个固定 Release 与日志打开链路修复

源码 `0a5e6a1`，构建 `checks/release-02/` 退出 0，二进制 SHA256
`c900764d022f9335ed0b779e63b7bb3394740bd160ef03d17fa6fd9ea8c4725f`。普通隔离实例 PID 18225。
真实桌面确认日志有数据、手动选择能显示详情；合并提交的三栏正确显示 main/combined/feature，
没有写按钮，工作区刷新不抢走该页签。仓库与底部提交选项弹层可见且在本轮位置不被裁切，
普通模式没有测量面板。操作及截图在 `desktop/revia-02-*`；命令面板记录 289 条且最近命令对应实际操作，
不再出现首包的持续重开循环。以上仅为该窗口/流程的观察，不代表全部弹层、缩放和流畅度验收通过。

继续使用发现：

1. 普通提交 diff 与 combined merge 预览被正确拒绝身份不一致；树节点误把展示分组 `parent` 当比较父提交，
   普通/combined 的该字段为 null，真正比较父提交在 `diffParent`。现提取实际使用的纯文件树模型并保留该身份。
2. 分页器原地 push 到已发布数组，React 依赖不变，首屏没有自动选择，筛选后也不能可靠选择新首项。
   改为不可变列表发布，回归冻结旧快照并跨两页验证不会改写旧结果。
3. 文件 Enter 冒泡到全局日志快捷键造成重复打开，merge 可被后一个普通打开覆盖；现在阻止冒泡并共用正确分型。
4. 旧简化预览只读前 200 行、忽略当前视图并强开预览。改复用现有完整只读 diff/merge 渲染器，
   沿用当前 diff 设置，选文件不改变预览开关；预览与详情核对目标和提交，换目标/分组淘汰旧文件选择。
5. 并排布局错误用 `String(payload)` 只显示 `[object Object]`，改用已有 `getErrorMessage`，不放宽任何后端校验。

集中检查 `checks/desktop-repair-02/`：17 个 Node 脚本共 417/417、lint、build 均退出 0。
首次 `diff --check` 仅报删除旧预览后 EOF 多空行（退出 2）；移除后仅重跑该检查，退出 0，原件均保留。
Rust 未修改，本轮不重复其套件。旧包已正常退出，修复须新的独立两轴复核，再构建第三包继续桌面。

### 22.7 第三包前的独立复核与补修

固定 `0a5e6a1...4c099b1`，两个新的只读代理分别结束：Standards 发现 1 项 P2
（日志全局方向键抢走预览 diff 的键盘滚动）；Spec 发现 2 项 P2（超过两个父提交的自动分组
被当成旧模式、刷新首批把挤到第二页的旧选择覆盖）。原复核不通过，第三包尚未构建。

Codex 同批补修：

- 全局日志键盘先尊重 `defaultPrevented`，表格方向导航只在表格焦点内处理；不改共享 diff 渲染器。
- 请求、有效模式核对、预览与页签分型一致处理多父自动 `byParent`；首个可打开文件跨父分组查找，跳过空组。
- 只在查到真正末尾后才能认定旧选择消失；首批未加载的提交、查询错误及跨度截断都不覆盖旧选择。
- 自查另发现续页只传视口高度，漏掉滚动位置；改传可见末行。查询重置先补到原视口深度再恢复滚动，
  忽略中间因列表暂时变短而产生的夹紧事件，不为恢复位置一次读取整个历史。

集中最小检查 `checks/desktop-repair-03/`：日志模型、历史 diff、页签三个 Node 脚本共 66/66，
lint（原有 7 条警告）、build（含类型检查）、`git diff --check` 全部退出 0。
模型回归覆盖刷新跨页保留选择、真实视口末行续页和多父模式；键盘/恢复 effect 的断言仅为接线检查，
不冒充 React 运行或真实桌面结论。Rust 未改，不重复其套件。补修提交后须另开两轴复核，再打第三包。

补修固定 `8718b737eb2cf850aaa78f6cf40f9d479c32521f` 的新两轴复核再次不通过：Standards 2 项 P2，
Spec 1 项 P2（两轴均发现隐藏日志后后台恢复读回 0 并持久化；Standards 另发现恢复续页失败后 pending
一直不释放，屏蔽已加载列表滚动）。现仅在 `active` 时操作恢复 DOM；查询失败或跨度截断终止等待，
恢复到已加载的可读范围，保留错误提示与原选中项，不把错误当成真正历史末尾。
最小重检 `checks/desktop-repair-04/`：日志模型 40/40、lint、build、diff-check 退出 0，待新两轴复核。

### 22.8 第三个固定 Release 的桌面结果

`8718b73...5b5198be71b1c2589ad1627f5c0fa93fc1fa351b` 新 Standards / Spec 复核均 0 项确定问题，
仅静态。随后构建第三包 `checks/release-03/`，退出 0，二进制 SHA256
`b7ae93020f0a1b60454a43994111d7136a5eed547ec22bc8b7f69728c62943b8`。
普通隔离实例 PID 25934，仍只有 acceptance 两变量，不含测量变量。实例已正常退出。

真实桌面观察（原件 `desktop/revia-03-*`）：

- 默认日志选首条、预览默认关闭且没有测量面板；普通历史文件 Enter 打开正确父提交内容，没有写按钮。
- 日志滚过第 100、200 条到真正末尾。双父 merge 预览与页签正确显示 main / combined / feature。
- 在深页打开历史页签后通过分支弹层 fetch 本地 `origin.git`，历史页签未被抢激活；返回仍在深页并保留 merge 选择。
  Git 核对 HEAD 仍为 `ecf9d48`，tracking 前进到 `0daa53a`，已暂存/未暂存/未跟踪均未变，
  证据 `ordinary-03-before.json`、`ordinary-03-after-fetch.json`。
- 按哈希筛出新增 1600 行文件，预览 End 到第 1600 行，再按方向键没有切换提交；不是 200 行截断。
- 新独占多父 fixture `fixtures/multiparent-q1yznr0e`，真实三父提交 `370a038`，合并结果树与父 1 相同；
  首父组为空，默认预览成功选择父 2，Enter 打开普通 diff，工作区刷新未抢走历史页签。
- 仓库/分支/提交选项弹层在本轮所测位置可见；不能据此宣称全部窄窗口、缩放与边缘位置通过。

本包仍不通过：右侧预览把日志表格挤窄，列与引用文字重叠（`revia-03-hidden-fetch-restore.png`）；
多父列表在平铺设置下没有父分组标识且误显示三栏说明；切到同文件的父 3 后，复用页签仍保留父 2
（`revia-03-parent-tab-stale.*`）。停止长测/最终性能验收，先集中修复。

补修：复用页签时原位更新只读目标、父提交与分型；文件树始终保留父分组、空组编号和真实目录层级，
父分组默认展开，combined 说明只用于实际 combined；右侧预览在有限宽度下让表格/详情上下排列，预览位置不变，
表格自身有列宽下限和横向滚动，限制引用标签宽度，不再覆盖主题/作者。最小集中检查
`checks/desktop-repair-05/`：三个 Node 脚本 70/70、lint、build、diff-check 退出 0。
待新的两轴复核与第四包桌面；M5 未完成、未合 main、未推送。

`5b5198b...316a265` 新复核：Standards 0 项，另转交正确性风险；Spec 1 项 P2，
响应式预览切换改变表格高度却只在窗口 resize 测量，可能持续缺行。现改用表格元素的 `ResizeObserver`，
跟踪内部布局变化并忽略隐藏时的 0 高度。`checks/desktop-repair-06/` 最小重检日志模型 44/44、
lint、build、diff-check 退出 0，待新复核后再打包。

### 22.9 第四个固定 Release 与跨 worktree 日志隔离

前一轮 Standards 代理因 429 无结论；恢复后没有可继续收取的会话，另开两个新只读代理复核
`316a265...03c19beeaa8707d9c87fad7717f34d86ae572158`，Standards / Spec 均 0 项确定问题。
第四包构建 `checks/release-04/` 退出 0，二进制 SHA256
`c096312f396f5f39d9976eef48368b6d4a1b4057405576f3d2904f419c507e8d`。
普通隔离实例 PID 48330，仅 acceptance 两变量，无测量面板；已正常退出，精确 PID 检查确认不存在。

本包真实桌面结果（`desktop/revia-04-*`）：

- 三父文件从父 2 切父 3 后，复用同一页签正确显示 `2582a9c` 的 `third` 与合并结果的 `first`；
  平铺模式仍保留三个父组，父 1 空组编号正确，没有 combined 三栏误提示。
- 右侧预览下表格/详情上下排列，无文字重叠；开预览后原生缩放窗口，再关闭预览，表格下部继续有行，
  未见原先持续缺行。原生缩放前后截图为 2560x1600 / 3840x1940。
- 100% 与 150% 下所测提交选项弹层完整可见；150% 右边缘列菜单与分支子菜单完整可见。
  两次尝试拖窗口边缘未实际改变尺寸，不计为窄窗口通过；尚未覆盖全部弹层与边缘组合。
- 主 fixture 与三父 fixture 的 HEAD、refs、工作区/暂存区、config SHA256 前后完全一致，
  `ordinary-04-before.json` / `ordinary-04-after-exit.json`。截图原件有些沿用 `.png` 扩展名，内容实际为 CUA 返回的 JPEG。

本包仍不通过：从三父仓库切回主仓库时，日志详情沿用 `370a038`，显示“对象不存在”，主仓库已有选择丢失。
原件 `revia-04-return-repo.*`。原因不在 Git：日志组件复用旧分页快照，新 worktree 渲染时可能用旧快照自动选中；
状态更新回调还通过可变的当前 key 写入，旧清理/节流回调会污染新 worktree。

现集中补修：日志组件按 worktree 独立挂载；更新回调绑定原 worktree；历史会话在渲染时同步核对 worktree 和环境代际，
避免切换后 effect 清理之前暴露旧目标；卸载取消分页监听。现有 Node 脚本新增按目标状态与迟到页回归，
React 接线断言仍不冒充真实组件运行。待集中检查、新两轴复核和第五包复测；未开始最终 D06 或长测。

集中最小检查 `checks/desktop-repair-07/`：日志模型、只读历史与页签三个现有 Node 脚本 67/67，
lint（7 条既有 warning）、build（含类型检查）、diff-check 全部退出 0。保留检查时源码 patch；Rust 未改。

### 22.10 第五个固定 Release：普通工作流与弹层补验

固定 `03c19be...5c2367f51f173e9cd4c775d3b6a12616ee23dccb` 的两个新只读代理分别复核，
Standards / Spec 各 0 项确定问题；只代表该补修的静态复核。
第五包构建 `checks/release-05/` 退出 0，二进制 SHA256
`6a3f12afd9115ea0f7536fac4d638a591ee8e19501065d79fb68c643265db07d`。
普通隔离实例 PID 52572 只设 acceptance 两变量，没有测量面板；已正常退出，精确 PID 检查确认不存在。

日志真实操作（`desktop/revia-05-a-*`、`revia-05-b-restored.txt`）：

- A 主 fixture 用 `step` 筛选，滚至深页并选第 022 条；B 三父仓库独立用 `fixture` 筛选并选 third。
  A→B→A 恢复各自筛选、滚动位置、选中与详情，无跨仓库 OID、“对象不存在”或持续空白。
- 在 A 用文件 Enter 打开只读历史 diff，显示 step 021→022；隐藏日志后切 B 再回 A，日志状态仍正确恢复。
- 本轮补验作者、日期、路径、日志分支筛选、worktree、变更多菜单、最近提交信息、diff 选项与最右侧页签更多菜单。
  所测位置弹层可见，无上下裁切；较长列表有滚动，最近提交信息弹层自动向上避让。截图 `desktop/revia-05-*-popup.jpg`、
  `revia-05-recent-message.jpg`、`revia-05-diff-options.jpg`、`revia-05-diff-more.jpg`。
  在页签文字上右键得到 WebKit 系统文字菜单，不把它算作自定义页签菜单；页签操作入口实际在右侧“更多”。
- Atom One Light 与 VS Code Light 2026 已实际显示检查；后者截图 `revia-05-vscode-light.jpg`。
  应用主题随系统深浅色切换，本轮没有修改系统设置，两个深色主题不计通过；此前没有成功改变宽度的拖动也不计窄窗通过。
- Rebased 1.1.22 日志的 Text/hash、Branch/User/Date/Paths、Subject/Author/Date 默认列及右侧文件/详情布局已有原始截图；
  本轮补 `desktop/rebased-05-view-options.jpg`，可见 Long Edges / Columns 等原始选项。
  这是逐项对照证据，不等于所有屏幕已完成并排截图或获得用户外观认可。

本地完整写工作流（唯一远端为本轮自建 `fixtures/origin.git`，不含真实网络远端）：

1. 在主 fixture 的已暂存、未暂存与未跟踪改动共存时点“更新”，HEAD 从 `ecf9d48` 快进到
   `0daa53a8d24505eb5e4a78bc1349f19bd4b4c08a`；三份文件内容 SHA256 全部不变，`.git/config` 不变。
   回收记录 `data/recovery/record-1793160869423-52572-2/` 的 source 为 `updateAutostash`，outcome completed，
   保存的 tracked 文件内容吻合更新前版本。Git autostash 恢复后原已暂存 `src/staged.ts` 变为未暂存，
   此结果已披露，不宣称暂存区状态完全保留。
2. 检出 `occupied` 被 Git 拒绝，原文指出其被 `fixtures/linked` 占用；当前分支未改变。
3. 新建并检出 `acceptance/m5-ordinary-05`，暂存两个 tracked 文件，保持 `notes.txt` 未跟踪。
4. “提交并推送”生成 `4c8eee92bca8441b42874d6e1c286ca21014fb61`，信息为
   `acceptance: exercise commit and local push`；普通推送后，本地裸远端同名分支 OID 完全相同，
   上游为 `origin/acceptance/m5-ordinary-05`，status 仅 `?? notes.txt`。
   对话框完整可见，未使用强推。新目标分支此前不存在，预览列全部可达 216 提交、最多展示 50 条；
   推送后的旧“无上游”提示最终正常刷新，未持续残留。

Git 前后原件 `desktop/ordinary-05-before-update.json`、`ordinary-05-after-update.json`、`ordinary-05-after-push.json`；
推送预览与检出拒绝的 AX / 截图同存 `desktop/revia-05-*`。
正常本地工作流通过不代表所有分支安全路径与全部 M5 验收已完成；未合 main、未推送项目真实远端。

### 22.11 第五包 D06：首屏仍未稳定达标

使用 22.10 的同一固定二进制，普通实例退出后单独启动测量，每次清除继承的 `REVIA_*`，
指定本轮性能 fixture 与独立应用数据目录。原始启动变量、PID、stdout/stderr、退出码、JSON 在
`/private/tmp/revia-m5-integration-20u816pb/d06-05-ai1dvp9y/`，不覆盖早期证据。

有效结果均 `ok:true`、前台 focused、focusLosses 0、hiddenTransitions 0，约 59–61 rAF/s。
这里的 `ok:true` 表示测量流程成功，不等于 D06 门槛通过。

| 样本 | 首屏（含读取状态） | 跳转 / 滚动观察 | 结论 |
| --- | --- | --- | --- |
| `split-01`，7 文件 / 8309 行 | 149 ms（读取 54 / 首次取行 95） | 跳转 60 ms，热跳调用返回即覆盖；60 帧缺行 0、两栏同步误差 0 | 此次首屏达标 |
| `split-04`，同 fixture | 167 ms（读取 59 / 首次取行 107） | 跳转 57 ms，热跳调用返回即覆盖；60 帧缺行 0、同步误差 0，帧间隔 p50/p95 17/22 ms | 首屏超过 150 ms |
| `unified-01`，7 文件 / 13435 行 | 155 ms（读取 72 / 首次取行 83） | 目标已被后台预取，不当成冷跳；热跳文字 1 ms、同帧；60 屏缺行 0 | 首屏超过 150 ms |
| `cold-unified-01`，3 万行源文件 / 60001 diff 行 | 143 ms（读取 75 / 首次取行 68） | 目标未缓存；冷跳文字 19 ms，热跳文字 1 ms、同帧；60 屏缺行 0，p50/p95 17/17 ms | 此次冷跳、热跳与滚动达标 |

并排的跳转耗时包含等待绘制，`withinOneFrame` 由同步覆盖检查判定；没有统一视图那样的未缓存身份探针，
不把并排跳转命名为已证明的真实冷跳。统一小文件目标已缓存，也不挪作冷跳结论。
两次有效并排样本都保留，不挑最好值；首屏门槛不改、不用平均值覆盖失败。

无效启动同样保留：`split-02` 的驱动等待 180 秒超时，没有结果 JSON，CUA 绑定也超时，
不据此认定应用性能或沙箱根因；`split-03` 未在 120 秒内获得前台可见合成窗口，输出 `ok:false`、空结果。
该次首次 CUA 绑定曾显示普通默认数据窗口而非测量标题，未做仓库操作并退出；后续核对到准确测量 PID 59003。
具体多实例/绑定时序原因未确认，不把这轮计入性能样本。后续保持测量窗口打开，按标题核对 PID，
显式 Raise 并点入 WebView 后再采样；未规避任何可见性判据。

截至本节，所有诊断实例已正常退出，未留下测量面板。读取/解析与首屏准备两段都有耗时波动，
尚未通过生产入口进一步定位热点，不能将原因写成已确认的单一瓶颈。
当前 D06 首屏仍是交付未通过项；最终 30 分钟长测暂缓，四主题完整截图、窄窗与剩余工作流也未全部验完。
M5 未完成、未合 main、未推送。

### 22.12 首屏分段诊断与请求调度补修

以 `49d11db` 为基线，扩展现有 `session_flow::rows_cold_and_warm_cost`，经生产 `WorktreeSession`
与真实 Git 只读访问本轮 `performance-5b9g0bu9/diffs`；不引入临时框架，不写 Git，不添加固定毫秒断言。
Release 模式诊断原件在 `startup-profile-mv8wb3c2/`，含精确命令、源码 patch、stdout/stderr、退出码。
单次后端样本：状态读取 48.005 ms，冷左右布局 8.657 ms（2053 段与操作目标），两侧各 50 行
0.298 / 0.274 ms，各 400 行约 0.75–0.80 ms；首块高亮 52 ms、第二块 20 ms。
状态阶段的 Git 命令分别为 status 10 ms、未暂存 diff 18 ms、已暂存 diff 7 ms、身份 rev-parse 6 ms；
布局另有读取暂存区内容的 show 6 ms。这是后端诊断，不是桌面 D06，也不能证明前端的全部耗时来源。

第五包 `unified-01.log` 显示挂载后先发 `get_rows 0..18`，再发实际视口的 `0..52`。
零高度时请求出的微小 slice 会占住单请求入口，需要后续再补真实范围。现统一视图等实测视口高度大于 0
再取行；统一/并排的可见行及并排布局在 layout effect 中发起异步请求，避免先画空内容再等 passive effect。
没有改 IPC、后端安全校验、缓存身份、滚动契约、预取范围、D06 计时或门槛；高亮仍异步进行。
这只是待实测的调度优化，不能提前声明首屏通过。

集中检查原件 `checks/startup-repair-01/`：17 个现有 Node 脚本 429/429、lint（7 条既有 warning）、
build（含类型检查）、Release 模式 `session_flow` 3/3、diff-check 全部退出 0。
Rust 生产代码未改，未重复无关集成套件；独立复核及新固定包桌面结果待补。

### 22.13 第六包：窄窗补验、D06 与长测驱动失配

固定 `49d11db...c03ad81dcb44df3a39dd7b784972a4f9d8468377` 的新 Standards / Spec 两轴各 0 项；
第六包 `checks/release-06/` 构建退出 0，二进制 SHA256
`2f83ef3628993f60db11fce4370e6bd55eafed64015497d90713b94095be91ee`。以下均为该包，不挪用旧包成绩。

普通隔离实例 06、06b 只设置 acceptance 两变量，无测量面板，均已正常退出。
06 经过文件选择器添加性能仓库后，diff 与提交选项的 AX 有内容，截图却不可见；
原件 `desktop/revia-06-*-options*`、`revia-06-commit-options.*` 保留，原因未确认。
06b 重启后，已有仓库切换、经过选择器重新打开、展开大文件/连续滚动/统一与并排切换后，
弹层均实际可见，未稳定复现，不能写成已修复。经 GrokSearch-rs 优先检索时来源回退、原文读取连接失败，
备用网页工具核对了 [Base UI 官方 Portal 建议](https://base-ui.com/react/overview/quick-start#portals)；
它仅说明层叠上下文要求，不足以证明本次根因；当前依赖为 1.9.0，未据猜测改依赖或定位逻辑。

06b 通过原生 Window / Move & Resize 真正改变窗口尺寸，半屏截图为 1922x1940 像素，
紧凑四分之一屏为 1922x1042 像素（Retina 2x，约 961x521 逻辑点）。软换行确实发生；
右侧 diff 菜单贴边平移、缩短为内部滚动，底部“上下文行数”可滚到；底部提交选项完整可见。
证据 `desktop/revia-06b-narrow*`、`revia-06b-compact*`，这次不再把无效拖动当窄窗通过。
两个测试仓库的 HEAD、refs、status、config SHA256 与 06 开始前全部一致：
`ordinary-06-before.json` / `ordinary-06b-after.json`。本轮未写 Git。

D06 原件 `d06-06-w68_qaza/`：每样本独立数据、清除继承的 `REVIA_*`、核对窗口 PID，
最终全部 `ok:true`、focused、focusLosses 0、hiddenTransitions 0，约 60 rAF/s；所有样本均保留。

| 样本 | 首屏（含读取状态） | 跳转与滚动 | 结论 |
| --- | --- | --- | --- |
| split-01 / 02 / 03，8309 行 | 146 / 142 / 142 ms | 跳转含等待绘制 44 / 62 / 65 ms；热跳同步覆盖为 true；各 60 帧缺行 0、同步误差 0 | 本轮三次首屏达标 |
| unified-01 / 02 / 03，13435 行 | 114 / 119 / 115 ms | 目标已缓存，不冒称冷跳；快速滚动缺行 0，p95 均 18 ms | 本轮三次首屏达标 |
| cold-unified-01，60001 显示行 | 108 ms | 两个探针目标均未缓存，文字分别 3 / 23 ms；返回已缓存位置均 1 ms、同帧覆盖；缺行 0，p95 17 ms | 本次真冷跳与热跳达标 |

上述首屏/文字/滚动样本均满足既定门槛；不改变高亮口径，也不将短测冒称长测。

随后正式 30 分钟驱动在开始阶段失败：`soak-06-1gdm0_n7/`，驱动退出 1，
前端报“等待行数据超时：logs/app.log split”，没有足够操作时长、阶段或压力后样本，不能判定内存。
定位发现长测仍等待旧 `RowWindowStore` 和 `diffRef`；M4 并排已改用独立 `side_lines` / `sbsRef`，
普通界面能显示，而驱动误等永不填充的统一行缓存。首屏 D06 已分型，长测接线遗漏。

现补修仅长测/诊断入口：抽出按当前视图选真实渲染器的 `soakScroller`，并排检查两侧可见行、
经文件锚点滚动，统一仍用自身行入口；阶段采样额外等并排行请求与高亮结束，保持顶部同阶段。
不改时长、Q98 压力间隔/内存阈值、D06 测量口径、Git 写安全或普通滚动实现。
现有 Node 脚本补适配回归，不引入框架。待集中检查、新两轴复核和新固定包，不能沿用第六包作为补修后的长测版本。
四主题完整截图、完整 Rebased 对照、剩余安全工作流及偶发弹层问题仍未全部结束；M5 未完成、未合 main、未推送。

集中检查 `checks/soak-repair-01/`：17 个现有 Node 脚本 432/432、lint（7 条既有 warning）、
build（含类型检查）与 diff-check 全部退出 0；Rust 生产代码未改，不重复其测试。
适配回归是纯句柄测试，不代替真实 WebView 长测。待新的两轴复核后再构建。

### 22.14 第七包：正式长测与推送预览新失败

固定 `c03ad81...49ba90333aadf1fc8ccb24196930bf1b8280efb3` 的新 Standards / Spec 两轴各 0 项确定问题；
静态审查不能代替真实运行。第七包 `checks/release-07/` 构建退出 0，耗时约 106.6 秒，二进制 SHA256
`b61d0594b502a157ad37775358aada3ebdb953dfb4a7185669cb5be8a491b71e`。以下桌面和长测均为该包。

先做现有短诊断：`soak-07-diagnostic-skrzx9ir/`，应用完成 86.04 秒主动负载，25 循环、175 次文件切换、
300 次视图切换、825 次滚动与 150 次跳转，固定 staged/split 顶部样本完成。应用最终 `ok:true`、正常退出；
驱动在退出边界读取 Networking 子进程 `vmmap` 失败（退出 255，捕获时主进程已退出 0），整次驱动仍退出 1。
最终前端报告和失败候选均保留，不改成通过，不将短诊断用于内存验收。

正式原件 `soak-07-y4ss9m_7/`：使用现有 `bench-long-running.py --minutes 30 --baseline-minimal-logging`，
清继承的 `REVIA_*`，独立应用数据，未用 `--keep-on-top`。准确主 PID 98074、WebContent 98077 全程不变；
主动操作 1801.02 秒，阶段暂停 180.097 秒已扣除，可见性暂停 0；579 循环、切换文件 4052 次、视图 6944 次、
滚动 19099 次、跳转 3472 次。29/29 同阶段样本通过归属、闭合与可见性核验，驱动退出 0，
`runCompleted:true`、最终前端 `ok:true`；窗口可见、隐藏切换 0，约 60 rAF/s，未卡死。应用自行正常退出，未发终止信号。
`runCompleted` 只证明流程完成，内存门槛仍独立判定。

| 压力后样本（热身后） | WebContent footprint | WebKit malloc allocated |
| --- | --- | --- |
| 345.5 秒 | 89.0 MiB | 39.6 MiB |
| 684.7 秒 | 99.2 MiB | 41.2 MiB |
| 1028.2 秒 | 104.8 MiB | 41.8 MiB |
| 1367.6 秒 | 108.3 MiB | 44.5 MiB |
| 1708.7 秒 | 113.0 MiB | 44.0 MiB |

Q98 **不通过**，最终元数据权威有效：footprint 最大增长 24.0 MiB、回归趋势 +1.0048 MiB/分钟，
分别超过 10 MiB 和 0.5 MiB/分钟。malloc 最大增长 4.9 MiB、趋势 +0.2129 MiB/分钟，通过；
Rust 热身后常规 footprint 58 个样本，最大增长 0、趋势 -1.7091 MiB/分钟，通过。普通 RSS 保留，不单独作为不通过依据。
原始 `vmmap` 中 Untagged 与 WebKit dirty 区域也增加，但不能据汇总确定泄漏对象或把根因归到 JS、布局或 IPC。
经 Grok MCP 搜索时超时，随后 `web_fetch` 成功核对 [WebKit 官方低内存通知实现](https://github.com/WebKit/WebKit/blob/main/Source/WTF/wtf/cocoa/MemoryPressureHandlerCocoa.mm)；
该源码仅用于确认通知机制，不冒称与本机系统 WebKit 构建完全一致，更不能证明增长根因。

补充核对第六包 D06 完整 JSON：统一视图原 60% 目标的同次探针 `sameTargetDom.outward.frameReady` 为 false，
三次首个 rAF 核验完成在 37/32/39 ms（文字在 16/13/16 ms）；真冷跳同次探针为 37 ms（文字 23 ms）。
独立跳转探针、返回热跳与首屏/滚动的成功不能抹掉这些 false；第 22.13 节只保留相应数值和子项结论，
不能将其概括为 D06 全部通过，首帧调度证据仍待定位。

长测后回到普通隔离实例 PID 19450，只设 acceptance 两变量，没有测量面板，已正常退出。
`workflow-07-xgins74k/` 记录本轮已有自建 fixture 的准备：`other` 仅向本地 `file://.../fixtures/origin.git`
的 `acceptance/m5-ordinary-05` 推进到 `100aa5475bde435f528d434e116b63a2dc579f0d`，主 fixture 的 HEAD 与配置不变；
随后 UI 只暂存 `src/demo.ts`，普通提交生成 `8f642707a3caf7abac21ddb75356e7b7074cc82c`，`notes.txt` 未被提交。
点击推送后，没有预览对话框，报“读取提交范围时缺少完整对象 ID”，截图 `push-before-fetch.jpg` 与完整 AX 保留。
这不是已执行的非快进拒绝：`push_preview` 在已有上游且 HEAD 不同时，把字面量 `HEAD` 交给只接受完整 OID 的 `read_range`，
预览阶段就失败。现改用已经捕获的完整 `head`，沿用现有 Rust 套件补普通领先与远端未 fetch 前进的预览回归；
待集中检查、新两轴复核和新固定包桌面，不沿用第七包作为补修后的结果。

内存与同次首帧证据未关闭，推送预览需补修复验；四主题、完整 Rebased 对照、部分安全工作流和偶发弹层也尚未结束。
M5 未完成、未合 main、未推送；没有删除其他 worktree 或 Paseo 数据。

推送预览补修集中最小检查 `checks/push-preview-repair-01/`：`branch_remote` 31/31（真实 Git、本地裸远端）、
`test-remote-write.mjs` 32/32、diff-check 全部退出 0，原始命令、源码 patch、stdout/stderr 与退出码保留。
这次只改 Rust 预览读取的 OID 传值和现有测试，未改前端，故未重复无关前端构建或完整 Rust 套件；待新两轴复核。

### 22.15 第八包：已有上游推送与被拒后的更新重推

新的独立 Standards / Spec 两轴分别审查固定差异 `49ba903...1ad68bed12c1f6397f06d4c0bb25ed07dadf348e`，
均为 0 项确定问题。确认使用本次捕获的完整 HEAD OID，没有放宽 `read_range` 校验、重新解析可移动引用、
新增 fetch 或写配置。此静态结论与 22.14 的 Rust 31/31、Node 32/32 各自保留，不代替桌面结果。

`checks/release-08/`：固定 `1ad68be`，离线、锁文件 Release 构建退出 0，约 104.75 秒；二进制 SHA256
`948a40ddd2afd49aec3ec31caf3bbc8f7ab902a950ed0e77862e2106b109ce5d`。
普通隔离实例 PID 26767 只设 acceptance 两变量，使用本轮独立 data，无测量面板，最后通过应用正常退出，退出码 0。

本轮沿用自建 `fixtures/repo` 和唯一远端 `file://.../fixtures/origin.git`，不访问真实远端；
原件独占保存于 `workflow-08-hgz15r26/`：

- 推送预览列出本地 `8f64270` 的 1 条提交，目标 `origin/acceptance/m5-ordinary-05` 正确；截图与完整 AX 已保存。
- 真正点击普通推送后，Git 非快进拒绝，出现“推送被拒绝”及“更新后再推送”入口。
  拒绝后的本地 HEAD `8f64270`、旧跟踪引用 `4c8eee9`、裸远端 `100aa54` 与准备前一致，配置不变。
  这次确实执行了 push，不再把第七包预览失败冒称为远端拒绝。
- 点击一次“更新后再推送”，实际 merge 更新并重推成功。最终本地 HEAD、上游跟踪引用和裸远端完整 OID
  均为 `253dc067603b0d97f4502a47a6209247b6036961`，双方分歧提交都经 `merge-base --is-ancestor` 验证可达。
  `remote-incoming-07.txt` 正确出现，唯一工作区状态仍为 `?? notes.txt`，其 SHA256 不变。
- `.git/config` SHA256 仍为 `d5d3cfd6209718b56f31bb4c0ece0267254d73f847b845ea4f8018a72fe86e8a`，
  已有上游的正常重推没有改写配置。正常 Git 命令日志面板截图已保留，但该时点 AX 只暴露容器，未取得完整命令文本，
  不单凭最终 OID 断言所有内部调用次数；唯一重试边界仍由既有生产协调器回归覆盖。
- 本轮推送预览、拒绝对话框均真实完整可见，未见遮挡；不能由两个成功时点宣称 22.13 的偶发弹层问题已经修复。

任务 03 恢复 `ready-for-human` 并勾选本轮独立复核；强制推送、全部退出保护、完整 Rebased 对照等仍未收口。
Q98 内存与 D06 同次首帧失败不受此次 Rust 传值补修影响，仍未关闭；M5 未完成、未合 main、未推送。

### 22.16 第八包：两组限定负载诊断与 D06 原件核对

未改生产代码，继续使用 22.15 的同一二进制。现有 `bench-long-running.py` 分别以
`--seconds 300 --diagnostic-logging minimal --idle-seconds 0 --keep-on-top` 加 `--focus view` / `--focus open` 运行，
每组独立数据、清继承的 `REVIA_*`，仅本轮性能 fixture。`keep-on-top` 仅诊断使用，不倒填正式长测。
两组均驱动退出 0、前端 `ok:true`、errors 空、三个阶段最终有效且闭合；应用自行正常退出，未发终止信号，
CUA 最终确认 Revia 未运行。`q98MemoryVerdictAuthoritative:false`，均不是 Q98 验收。

| 负载与独占原件 | 主动时长 / 操作数 | 三次压力后 footprint | 三次 malloc allocated |
| --- | --- | --- | --- |
| 仅统一 / 并排切换，`diagnostic-08-view-0s7azag3/`，PID 28140 | 301.023 秒；1809 循环、3618 次视图切换 | 87.6 / 79.2 / 80.8 MiB | 47.4 / 47.5 / 49.5 MiB |
| 仅文件切换，`diagnostic-08-open-tifafgsr/`，PID 30203 | 301.069 秒；609 循环、4265 次文件切换 | 71.2 / 69.9 / 68.6 MiB | 35.2 / 36.5 / 35.8 MiB |

两组隐藏切换与可见性暂停均为 0；视图组最终 focused 为 true，文件组 30 个常规前端样本及最终 focused 为 false，
但保持 visible、约 60 rAF/s。焦点状态和具体负载不同，不以两组数值之差作因果归因。
本次短样本未复现 footprint 持续上升，但不能排除任一操作在更长时间或组合负载下的问题，
也不能用短样本替换 22.14 正式失败。没有增加强制 GC、猜测性缓存清理或放宽门槛。

按采样时间对齐的原始 `vmmap` 再核对：旧完整长测的压力样本 Untagged dirty 从 5264 KiB 到 15.2 MiB，
WebKit Malloc dirty 从 85.9 到 102.5 MiB；这只是区域级增长，不是对象或分配调用栈归因。
派生原件 `checks/release-08/pressure-regions-v2.json`；首版 `pressure-regions.json` 的正则误匹配 metadata 行，
错误原件保留，v2 要求数值尺寸并逐阶段核对，未重跑或覆盖原长测。

第六包统一视图原件进一步显示：首次外跳 `scrollCallMs` 为 12 / 14 / 14 ms，文字在 13 / 16 / 16 ms 就绪；
首个 rAF 的 callback 在 32 / 38 / 37 ms，核验在 32 / 39 / 37 ms。核验本身约 0–1 ms，
不能仅以“观测很贵”解释全部超时；真冷跳 callback / 核验为 36 / 37 ms。未改现有计时或成功条件。
本轮经 Grok MCP 检索后成功读取 [WebKit 官方 Speedometer 测量说明](https://webkit.org/blog/15131/speedometer-3-0-the-best-way-yet-to-measure-browser-performance/)，
用于核对同步脚本、rAF 与后续渲染工作的区别，不将同步 DOM 已就绪等同于完整绘制及时；该资料不证明本次延迟根因。
后续仍需定位完整负载的内存增长和首次外跳调度 / 渲染成本，不能宣称 M5 或 D06 已完成。

### 22.17 第八包续验：普通弹层、滚动 / 跳转限定诊断与 CPU 采样

本轮仍使用 `1ad68be` 的第八包，仓库起点为只改文档的 `0743781`；产品源码、二进制及测试门槛均未修改。
二进制 SHA256 与 22.15 一致。以下路径均位于独占证据根 `/private/tmp/revia-m5-integration-20u816pb/`。

**普通模式弹层。** `popover-09-ly80bl03/`，PID 48452；新独立 data，仅设 acceptance 两变量，未显示测量面板。
从仓库菜单进入原生文件选择器、重新打开本轮性能 fixture，再打开提交选项，截图确认实际可见。
工作区大文件 `src/generated.ts` 展开后切统一 / 并排，在并排正文用滚轮跨至约 500 行，显示选项仍可见；
首次用 AX 容器执行滚动未改变内容，随后坐标滚轮确实改变行号，不把前一次无变化当作滚动成功。

通过原生 Window 菜单分别调整到半屏和约 961x521 逻辑点的短窄窗口，实际检查：

- diff 长菜单限制在窗口内，可内部滚动到“上下文行数”，不是把底部选项裁掉；
- 底部提交选项、分支展开项、主 / linked worktree 菜单完整可见；
- 日志日期菜单、自定义区间输入和最右侧列菜单完整可见；
- 切回主验收 fixture 后，再次进入并取消文件选择器，分支长菜单仍可见。

每个所述弹层均保留 AX 和截图。未复现 22.13 的“AX 有而截图不可见”，只能记录这些成功时点，不能宣称偶发问题已修复。
原生全屏 / 窗口调整过程的截图曾有过渡画面，未把过渡画面算作产品弹层缺陷或窄窗验收；以稳定窗口截图为准。
正常退出码 0；`before.json` / `after.json` 核对三个自建 fixture 的 HEAD、refs、status、配置、index 及全部工作文件 SHA256，
完全一致（`integrity.json`）。没有执行 Git 写操作，没有操作真实远端，也未清理 worktree。

**仅滚动 / 仅跳转短诊断。** 继续复用现有 `bench-long-running.py`，参数均为
`--seconds 300 --diagnostic-logging minimal --idle-seconds 0 --keep-on-top`，另选对应 `--focus`。
每组独立 data、清理继承的 `REVIA_*`，只访问同一本轮性能 fixture。

| 负载与独占原件 | 主动时长 / 操作数 | 三次压力后 footprint | malloc allocated |
| --- | --- | --- | --- |
| 仅滚动，`diagnostic-09-scroll-ttef0omo/`，PID 56576 | 301.030 秒；7362 次滚动 | 71.5 / 69.5 / 69.5 MiB | 36.6 / 38.3 / 38.1 MiB |
| 仅跳转，`diagnostic-09-jump-u61dx93d/`，PID 58702 | 301.056 秒；3696 循环、7392 次跳转 | 66.2 / 78.0 / 78.4 MiB | 35.3 / 38.1 / 38.1 MiB |

两组 driver 退出 0，`runCompleted:true`、errors 空，三个阶段均最终 valid / authoritative 且 END 已确认；
应用自行退出 0，未发送 terminate / kill。两组 hiddenTransitions 为 0，保持 visible、约 60 rAF/s；
滚动组最终 focused 为 true，跳转组为 false。两组实际阶段包含往返固定文件以便同阶段采样，
“仅某负载”不是零次文件切换。滚动组另在主动负载期间对所属 WebContent PID 56581 取过 5 秒 `sample`，原件保留。

短样本没有显示持续线性上升：跳转组首至第二个压力样本增加 11.8 MiB，第三个只再增加 0.4 MiB，
不能因此断言没有泄漏，也不能拿不同焦点、负载、时长的组间数值作因果比较。
两组 `q98MemoryVerdictAuthoritative:false`；热身后都只有两个压力样本，少于正式判定所需三个。
本轮没有补跑正式 30 分钟，22.14 的 Q98 失败仍然有效。

**首次外跳 CPU 采样。** 仅为诊断，同一包、独立 data、统一视图，不将采样干扰下的耗时倒填 D06 验收：

- `d06-profile-09-c151snnt/`，PID 60419：窗口未获得前台焦点，现有 120 秒可见性检查拒绝，
  报 `性能测量未获得前台可见合成窗口`。当时 CPU 样本只覆盖等待阶段，不能用于归因实际跳转。失败结果及样本保留。
- `d06-profile-09b-g4he6sxm/`，PID 61106：通过 CUA 的原生 Raise 将新实例置前，
  等真实“测量开始”后对该实例所属 WebContent PID 61111 采样 8 秒。
  本轮结果完整、focusLosses 0、focused true、visible、hiddenTransitions 0，采样及应用退出码均为 0。
  同次外跳同步调用 30 ms、文字核验 33 ms、首个 rAF 核验 60 ms；回跳分别为 4 / 5 / 12 ms。
  快速滚动未覆盖帧为 0。上述是带 CPU 采样的诊断数值，不替换第六包无采样原件。

CPU 栈中可见 WebCore 样式计算、布局、绘制及渲染事务工作，但样本横跨首屏、跳转与滚动，
没有逐次 JS / React 提交归属，不能据此认定某一函数就是根因。当前证据支持继续细分首次全色行窗口转 compact 的同步提交成本，
不支持直接放宽 16.7 ms 门槛、修改计时、猜测性清缓存或断言 WebKit 泄漏。
普通与测量实例均已正常退出；本轮只补验收记录，无产品补丁、无新构建、不标 done、不合 main、不推送。

### 22.18 Claude 接手：首跳慢帧定位与证据现场丢失（2026-10-28）

**现场丢失。** 本机 19:32 重启（`/Library/Logs/DiagnosticReports/shutdown_stall_2026-10-28-193243_*`，正常关机，无 panic），
`/private/tmp` 被清空：`/private/tmp/revia-m5-integration-20u816pb/` 下的全部 fixture、本地裸远端、D06 / 长测 / 工作流原件均已不存在，
22.x 中引用的这些路径只作历史记录。后续证据分两处：分析脚本与 trace 在 `/Volumes/T7 Shield/project/Revia-evidence/m5-claude-20261028/`；
应用读写的 fixture、数据目录与测量 JSON 在 `~/.cache/revia-m5-claude/`（T7 是可移除宗卷，每次新构建的二进制签名不同，
访问它会弹出系统授权框并卡在 `open()`）。性能 fixture 用 `tools/fixtures/setup.sh` 重新生成，文件集合与原 `performance-*/diffs` 一致。

**统一视图首次外跳的首帧。** 用临时计时点（未提交）与 Instruments 分段定位：

- 首次外跳同步调用 7–14 ms 中，React 提交占约 7 ms，主要是删除旧窗口约 140 行着色行；同一路径之后只要 1–2 ms。
- 失败样本（首帧 27 ms）的 System Trace：首跳是一段 27 ms 的连续执行，前约 10 ms 在能效核（E 核），之后才迁到性能核；
  随后的返回跳转与滚动帧全部在性能核。挂着 Time Profiler 时 3/3 首跳全在性能核，首帧 16 / 14 / 16 ms 达标。
  测量流程在首跳前空等约 1 秒，主线程负载轻，被调度到能效核，首跳这一阵突发工作先在慢核上执行。
- 同机重建 M2 `b185f6c` 对照（同一 fixture、同一探针）：首跳首帧 16 / 19 / 22 ms，仅 1/3 达标；M2 记录的 15 ms 并不稳定。

已改：`DiffView` 大幅跳转时，旧窗口的行留在原位（视口之外）到颜色恢复那次渲染再卸载，跳转那一帧只插入目标可见行。
首跳同步调用降到 3–5 ms；首帧仍为 17–27 ms，**未达标**。没有改 16.7 ms 门槛、测量口径或计时。
跳转那一帧不渲染段操作按钮的对照实验无改善，已撤回。

**Q98 内存。** 对照 M2 通过原件（`docs/milestones/m2/evidence/q98-rust-memory-20261025/`）与第七包正式长测：

- WebContent 的 Untagged 区域在两者中都持续增长（每 5 分钟约 8 MiB 虚拟、约 2 MiB dirty），M2 通过时也有，不是 M5 新增。
- 差异在压力后的 WebKit Malloc：M2 dirty 约 54–65 MiB 平稳、allocated 22–27 MiB；第七包 dirty 86 → 103 MiB、allocated 40 → 44 MiB。
- 每次切换视图 `addRepo` 重建，拖放监听随之重装 4 次 listen / unlisten（第七包 30 分钟 27776 次，每次还向 WebView 注入一段脚本）。
  M2 同样如此，不是差异项；已改为只装一次。

尚未复跑正式长测，Q98 仍按 22.14 不通过；以上改动也尚未经过新的两轴复核与固定包桌面验收。
Node 现有脚本 432/432、类型检查、lint（7 条既有 warning）通过。临时 M2 对照 worktree `../Revia-m2-d06check`（detached `b185f6c`）保留供复测。

### 22.19 `7681f9f` 固定包：D06 与正式长测复测（2026-10-28）

二进制 SHA256 `7da8a2b942c148c80181af6302eeba3e3d0dc299061d3f01885252de38a304c0`（源码同 `7681f9f`，离线锁文件 Release）。
用户同意占用屏幕后连续运行；原件在 `/Volumes/T7 Shield/project/Revia-evidence/m5-claude-20261028/d06-c1/` 与 `soak-c1/`。

| D06 样本 | 首屏 | 跳转 | 滚动 |
| --- | --- | --- | --- |
| 统一 ×3，13435 行 | 112 / 116 / 113 ms | 首次外跳同步 3 / 4 / 4 ms，首帧 25 / 29 / 28 ms，**未达标**；返回首帧 7 / 7 / 5 ms | 缺行 0，p95 17 ms |
| 并排 ×2，8309 行 | 138 / 142 ms | 冷跳 52 / 56 ms 含等待绘制，热跳同步覆盖为 true | 缺行 0，同步误差 0，p95 18 ms |

正式长测参数同 22.14（`--minutes 30 --baseline-minimal-logging`，不加置顶）：主动 1801.47 秒，578 循环、文件切换 4042、
视图切换 6926、滚动 19050、跳转 3464；驱动退出 0，`runCompleted:true`、前端 `ok:true`、errors 空，结束时可见且聚焦。
拖放监听不再重装：`plugin:event|listen / unlisten` 由第七包的 27776 / 27776 次降为 0。

| 压力后样本 | WebContent footprint | malloc allocated | Untagged dirty | WebKit Malloc dirty |
| --- | --- | --- | --- | --- |
| 325 秒 | 126.5 MiB | 36.4 MiB | 5.0 MiB | 122.7 MiB |
| 664 秒 | 136.7 MiB | 37.1 MiB | 7.0 MiB | 151.8 MiB |
| 1004 秒 | 133.7 MiB | 37.1 MiB | 9.3 MiB | 136.5 MiB |
| 1345 秒 | 132.8 MiB | 37.9 MiB | 11.4 MiB | 126.4 MiB |
| 1685 秒 | 136.1 MiB | 37.1 MiB | 14.3 MiB | 118.2 MiB |

Q98 **仍不通过**：footprint 最大增长 10.2 MiB，超过 10 MiB 门槛 0.2 MiB；趋势 +0.27 MiB/分钟，在 0.5 门槛内（第七包为 24.0 MiB、+1.00）。
malloc allocated 最大增长 1.5 MiB、趋势 +0.04，Rust 趋势 −1.61，均通过。
footprint 在第一、二个样本之间上升一次后不再持续上升；WebKit Malloc dirty 起伏后回落，剩余的持续增长主要是 Untagged（+9.3 MiB）。
M2 通过的长测里 Untagged 也以相近速度增长，来源尚未确定。单次结果，不能据此认定拖放监听改动就是第七包增长的原因。

## 23. 验收结论（Q154，2026-10-28）

用户认为 M5 可以了，M4、M5 带已知未达标项合入 main（[Q154](../../README.md#已确认)）。合入时的状态：

- 通过：首屏、滚动缺行、返回跳转（22.19）；Rust 与 malloc allocated 的 Q98 两项；第八包的已有上游推送与被拒后更新重推（22.15）。
- 未达标，转入 backlog：Q98 WebContent footprint 最大增长 10.2 MiB（[04](../backlog/04-q98-webcontent-footprint.md)）；
  统一视图首次大幅跳转首帧 25–29 ms（[05](../backlog/05-first-jump-efficiency-core.md)）。
- 未做完的桌面验收：弹层偶发不可见、四主题、Rebased 完整对照、强制推送与退出保护等（[06](../backlog/06-m4-m5-desktop-acceptance.md)）。

门槛与测量口径不变。合并方式按协作指南，M4、M5 各压成一个提交进 main，中间提交留在 `m4`、`m5` 分支。
