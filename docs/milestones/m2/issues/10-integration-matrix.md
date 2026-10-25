# M2 集成验证矩阵

更新日期：2026-10-25。由Pi准备，规划方按终态原件校正。自动化通过与桌面未完成分开，不代表M2通过。实际CI为f10fc971，受测Release仍是2a51b160；其后9434e7a仅模型文档。

**当前源码边界**：用户“继续吧”已放行验证。`201df2a` 后 Rust 排版减分配修改已完成本地 Rust 与前端检查，初轮缺 fixture 的 session_flow 三项已显式补跑；正式双轴复核与新版 Release/D06/CI/Q98 仍待完成。下列 CI、桌面、D06 和正式长测均属各自注明的旧版本，不代表本轮修复已通过。任务10最新 Comments 记录具体改动与边界。

## 当前结论摘要

| 轴 | 当前状态 |
| --- | --- |
| 自动化（T01–T14） | 最新双平台 CI 全部通过（见下） |
| D06 性能 | 四组真实可见窗口测量有效；标准首屏 137/124ms 达标；**原统一热跳 18ms/false 保留** |
| D01 30 分钟长测 | **完整运行但Q98未通过**：新版6f420a8主动1801.042秒、29阶段最终全有效；WebContent两条通过，Rust maxRise72.9MiB超+10。见[正式长测](../q98-formal-20261025.md)，旧两轮失败保留 |
| 真实桌面 | 已有局部成功证据；剩余补验见D02/D03/D04/D05/D08，不能概括为整体通过 |
| 任务 08 | `ready-for-human`，未 done |
| 任务 10/11、M2 | **未完成**，不得标 done |

## 最新固定版双平台 CI

`Compile Check #40174661706`（head `f10fc971c14e556c239cb4648cabfbbfc28ad4b4`，**success**）

| 平台 | 前端 tsc/lint/build | 完整 Rust | Release 编译 |
| --- | --- | --- | --- |
| macOS Apple Silicon (arm64) | 通过 | **271 passed / 0 failed / 1 ignored** | 通过 |
| Windows x64 (msvc) | 通过 | **243 passed / 0 failed / 1 ignored** | 通过 |

- `review_marks`：macOS **31** 通过、Windows **29** 通过；先前 Windows 失败的
  `deinitialized_submodule_is_not_confirmed_and_cannot_be_claimed` 两平台**实际通过**
  （CI 原始日志可核：`test deinitialized_submodule_is_not_confirmed_and_cannot_be_claimed ... ok`）。
- 两平台各保留 **1 个既有桌面 fixture 准备入口** `ignored`（`prepare_task05_desktop_fixture`，
  默认 `#[ignore]`，仅显式调用）；未新增跳过或放宽断言。
- 平台项数差异来自既有条件编译（Unix-only 路径/权限用例），不是遗漏。

**原件**：`/private/tmp/revia-m2-ci-final.cXI0qX/`
（`ci-run-40174661706-check01.json`、`ci-run-40174661706.log` 为证据；
`ci-counts-derived.json` 只是摘要导出，不能替代原始 log）。

**旧失败保留**：`2a51` 的 `#40172283582` 为 failure（Windows `review_marks` 28/1，
Release skipped），原件继续保留，不反写为成功。

## T01–T14 映射（当前实际测试文件与用例）

Rust 集成测试位于 `src-tauri/tests/`，均为真实 Git + 自建临时仓库 + 独立应用数据目录。

| 规格 | 测试文件 | 代表性当前用例（节选） | 结果 |
| --- | --- | --- | --- |
| T01 整文件暂存/取消暂存 | `file_staging.rs` | `t01_stage_and_unstage_regular_file_multi_file_invariants`、`t01_stage_and_unstage_initial_commit_no_head`、`t01_stage_and_unstage_empty_file`、`t01_stage_and_unstage_partial_staged_file`、`t01_literal_pathspecs_wildcard_safety`、`t01_literal_pathspecs_star_wildcard_safety` | 通过（平台套件项数不作T01单项数） |
| T02 hunk 暂存/取消暂存 | `hunk_staging.rs` | `t02_first_middle_last_hunks_keep_partial_staging_and_other_files`、`t02_pure_additions_deletions_and_shifted_line_numbers`、`t02_deleted_file_hunk_changes_only_index_existence`、`t02_added_file_hunk_with_and_without_head`、`t02_raw_bytes_survive_crlf_missing_eol_and_non_utf8_content`、`t02_hunk_preserves_whitespace_despite_repository_apply_defaults`、`t02_patch_header_lookalikes_are_body_content`、`t02_patch_larger_than_a_pipe_buffer_preserves_all_bytes`、`t02_content_hunk_does_not_stage_or_unstage_independent_mode_change` | 通过（平台套件项数不作T02单项数） |
| T03 类型与状态边界 | `file_staging.rs` / `hunk_staging.rs` | `t03_stage_and_unstage_binary_file`、`t03_stage_and_unstage_mode_change`、`t03_readonly_matrix_and_untracked_limits`、`t03_staged_file_at_literal_colon_paths`、`t03_binary_untracked_and_rename_never_expose_hunk_actions`、`t03_mode_only_changes_have_no_hunk_actions_on_either_side`、`t03_submodule_nested_repo_and_conflict_are_readonly_hunk_targets` | 通过 |
| T04 取消暂存重命名 | `file_staging.rs` | `t04_unstage_renamed_file_becomes_deleted_and_untracked`、`t04_stage_and_unstage_quoted_and_backslash_paths` | 通过 |
| T05 过期指纹 | `file_staging.rs` / `hunk_staging.rs` | `t05_comprehensive_fingerprint_verification`、`t05_other_hunk_change_rejects_both_sides_but_unrelated_file_change_does_not`、`t05_removed_or_replaced_target_rejects_old_hunk`、`t05_mode_and_symlink_type_changes_reject_old_hunk` | 通过 |
| T06 锁与并发 | `file_staging.rs` | `t06_index_lock_retry_and_never_deleted`、`t06_concurrent_write_operations_rejected_without_queuing` | 通过 |
| T07 丢弃前完整保存 | `discard_recovery.rs` | `binary_stale_bytes_are_rejected_by_discard_and_staging`、`mode_only_discards_and_preserves_executable_pre_state`、`crlf_attribute_text_discards_with_matching_git_semantics`、`preview_isolates_git_location_environment_from_the_real_repository`、`preview_honors_info_attributes_and_repository_attributes_file`、`preview_honors_local_clean_filter_configuration`、`linked_worktree_uses_shared_info_attributes_for_discard`、`git_rename_unstaged_side_rejects_hunk_discard_but_keeps_whole_file`、`attribute_context_change_after_save_is_rejected_before_writing`、`git_apply_lost_receipt_keeps_record_and_never_retries` | 通过（整份套件含既有1 ignored，不作T07单项数） |
| T08 恢复完整性 | `discard_recovery.rs` | `restore_returns_full_worktree_state_without_touching_index`、`hunk_record_restores_the_whole_file`、`deleted_file_restores_existence_and_executable_state`、`restore_refuses_after_later_changes_and_keeps_current_content`、`restore_refuses_without_trusted_post_discard_baseline`、`restore_refuses_when_identity_does_not_match`、`restore_refuses_unmerged_targets_but_allows_plain_index_changes`、`every_related_path_is_checked_before_publishing_any`、`publish_rechecks_real_state_after_prepare_and_does_not_overwrite` | 通过 |
| T09 恢复冲突 | `discard_recovery.rs` | `text_diff_is_generated_by_real_git`、`text_diff_handles_missing_saved_or_current_side`、`text_diff_never_disguises_read_failure_as_deletion`、`save_as_refuses_existing_targets_and_keeps_original_untouched`、`save_as_rejects_symlinked_ancestors_and_git_metadata`、`save_as_accepts_legal_parent_and_still_rejects_symlinked_ancestor` | 通过 |
| T10 持久化隔离 | `discard_recovery.rs` / `review_marks.rs` | `purge_expired_uses_controlled_clock_and_clear_keeps_in_use_records`、`clear_only_removes_this_applications_records`、`record_use_is_refcounted_and_clear_is_atomic`、`inspection_remains_viewable_when_worktree_is_gone`、`marks_survive_restart`、`recreated_worktree_at_same_path_does_not_inherit_marks`、`concurrent_marks_from_two_worktrees_are_all_kept` | 通过 |
| T11 阶段中断 | `discard_recovery.rs` | `restore_interruption_is_pending_verification_and_never_resumes`、`every_post_write_failure_is_pending_verification_and_rereads_truth`、`tail_failure_after_completion_downgrades_the_attempt`、`completion_journal_sync_failure_persists_needs_verification`、`git_apply_lost_receipt_keeps_record_and_never_retries`、`stale_confirmation_version_is_rejected` | 通过 |
| T12 审阅 | `review_marks.rs` | `mark_survives_stage_and_unstage`、`worktree_change_invalidates_mark`、`head_change_invalidates_mark`、`marks_survive_restart`、`same_file_in_both_groups_shares_one_mark_and_counts_once`、`unmark_only_affects_target_and_unknown_paths_are_rejected`、`stale_expected_versions_are_rejected`、`active_session_reconfirms_identity_and_stops_claiming_marks`、`packed_refs_symbolic_branch_switch_invalidates_submodule_mark`、`deinitialized_submodule_is_not_confirmed_and_cannot_be_claimed`、`unconfirmed_directory_cannot_be_claimed_but_can_be_unmarked`、`unrelated_head_change_keeps_mark_when_file_versions_unchanged`、`review_seq_advances_with_real_writes`、`review_marks_report_their_own_bound_versions`、`invalidation_advances_review_seq`、`mark_reply_confirms_every_path_with_state_and_versions`、`row_slice_reports_the_review_seq_it_read`、`row_slice_review_seq_tracks_invalidation`、`row_slice_review_seq_is_per_path_not_whole_snapshot` | 通过（macOS 31 / Windows 29） |
| T13 刷新与监视 | `refresh_isolation.rs` | `t13_same_row_count_content_change_updates_text_and_versions`、`t13_empty_snapshot_still_carries_revision`、`t13_external_stage_and_unstage_are_reflected`、`t13_untracked_content_change_updates_loaded_metrics`、`t13_highlight_rejects_external_edit_before_refresh`、`t13_highlight_cache_stays_bounded_across_versions`、`t13_concurrent_refreshes_never_publish_stale_content`、`t13_direct_save_short_read_never_publishes_forged_snapshot`、`t13_linked_worktree_watch_roots_resolve_real_git_dir`、`t13_watch_failure_stays_visible_across_manual_refresh`、`t13_watcher_emits_coalesced_change_notification`、`t13_refresh_keeps_write_preconditions_intact`、`t13_refresh_does_not_spawn_per_file_git_processes`、`t13_watch_scope_covers_current_worktree_but_not_unrelated_metadata`、`t13_current_linked_worktree_notifies_for_own_index_head_but_not_other_index` | 通过（macOS套件15项，不代表两平台相同项数） |
| T14 边界与路径 | `file_staging.rs` / `hunk_staging.rs` | `t14_controlled_entry_rejects_invalid_path`、`t14_unicode_spaces_and_windows_valid_literal_pathspecs`、`t14_unix_only_literal_colon_quote_backslash_and_star_paths`、`t14_tokens_are_bound_to_path_group_fingerprint_and_session` | 通过 |

补充：`session_flow.rs`（3 项）覆盖会话打开、行切片冷热成本、高亮请求形状；
`review_marks.rs` 另有 `review_seq_advances_with_real_writes` 等审阅序号契约。

**说明**：代表性104个不同测试名已与当前源码逐一核对存在。结果来自本次固定版实际CI；带条件编译的用例仅在对应平台运行，不把套件总数当每个T项数量，也不宣称两平台用例数相同。此映射不是新增覆盖率目标。

## D01–D08 当前证据与剩余项

| 规格 | 当前证据 | 剩余 |
| --- | --- | --- |
| **D01** 功能前后各 30 分钟同进程长测（Q98） | 新版6f420a8主动1801.042秒、同PID60845、29阶段最终完整有效、自然exit0、权威留证；WebContent footprint/malloc通过，Rust maxRise72.9MiB失败。见[完整正式记录](../q98-formal-20261025.md)。旧78.54墙秒帧间隔失败、1223.25墙秒vmmap中断及Rust+82均保留，不混入新结果 | Rust排版减分配源码已写未测；运行时分配归因及超幅是否消除仍未确认。用户放行后验证并独立复核，再重新完整正式验收；不是已证明泄漏，不放宽Q98 |
| **D02** 列表与 hunk 入口全流程 | Rust对应场景已有自动化并在实际CI通过；真实桌面有stage/unstage、成功恢复、另存、只读对比和二进制详情的局部证据 | 新 UI 键盘/hunk 串联（Tab 进入 hunk 按钮、`⌥⌘A`/`⌥⌘Z`）与一次真实 hunk 丢弃→回收→恢复，待补 |
| **D03** 自动刷新与选择衔接 | `refresh_isolation.rs` 15 项 + `tools/test-refresh-gate.mjs`（快照门/选择门/操作结果门） | 真实旧 A 列表捕获→切 B→显式释放→B 不被覆盖；旧 hunk 暂存被拒 + 常驻监视警告截图，待补 |
| **D04** 审阅交互 | `review_marks.rs` + Node 导航用例；真实桌面已过跨组共享、展开折叠组、去重/环回、全审留原地、取消/再标、重启保留 | 外改失效（等长内容变化 + 仅 HEAD blob 变化）待补 |
| **D05** 写入期间关闭/退出与待核对 | 共享 `CloseDriver` + 既有Node生产编排回归（准确命令及数量见静态归档，不把套件数当关闭场景数）；真实桌面 C02/run01 已过窗口 Close/Stay、Cmd-Q/Stay、菜单 Quit/Wait→显式释放→exit0；**C05 完整闭环已完成**（恢复准备失败→待核对→Stay→核对→新 Close→授权→同 PID 55553 exit0） | Dock 入口真实 Quit 样本、正常完成后的原 Close/Wait 收尾，待补；任务 08 仍 `ready-for-human` |
| **D06** 性能 | 四组真实可见窗口测量（见下，旧版结果保留） | 本次修改核心取行路径；用户放行并复核后须核对新版性能，不能沿用旧结果宣布本轮通过 |
| **D07** 检查与 Windows 编译 | 最新 `#40174661706` 双平台 success（含 Windows Release 编译，固定 f10fc971） | 本轮 Rust 产品源码已变化；用户放行后须完成最终固定版本检查与双平台编译，不倒填旧 run |
| **D08** 界面清理与美化 | 任务 12 三项已实现；13 rev3 已随 m2-ui 快进合入 m2；真实浅/深主界面、只读/冲突/二进制、确认框、焦点隔离已有原件 | `⌘+` 真实增大字号（可信按键来源）待补；pending已由C05补齐；真实Rejected提示仍待补，不将两者混为一项 |

### D06 实测（四组有效可见窗口测量）

原件根：`/private/tmp/revia-m2-final-measure-ex9swy__/`（`d06-*`）；
独立审核：`/private/tmp/revia-final-d06-audit-XIGT3O/d06-review.md` 与 `metrics.json`。

| 场景 | 首屏（含读状态） | 原 appear / warm | 独立文字出 / 回 | 独立首 rAF 核验 | 返回完整颜色 DOM |
| --- | ---: | --- | --- | --- | ---: |
| 标准并排 | **137** | 10 / 4 | 2 / 1 | 7 / 14 | 33 |
| 标准统一 | **124** | **18 / 7**（`withinOneFrame=false`） | 1 / 1 | 10 / 14 | 33 |
| 真冷并排 | 135\* | 11 / 4 | 4 / 2 | 9 / 9 | 29 |
| 真冷统一 | 137\* | 13 / 4 | 3 / 1 | 7 / 14 | 32 |

\* 冷仓库 30000 源行，首屏数据独立报告，不混入标准 150ms 门槛。

- 标准首屏 **137/124ms 均低于 150ms**；真冷独立文字 **4/3ms** 低于 100ms。
- 八次独立首 rAF 均 **35/35**、`textMatched=true`、`placeholderRows=0`。
- **原统一热跳 18ms / `withinOneFrame=false` 保留**，不倒填为 1ms/10ms，
  也不以另一目标探针消除这条未达原阈值记录。
- **返回首帧并非全色**：最终 29–33ms 才严格匹配 token DOM；
  原 `colorRestoreMs` 只代表退出 compact，不是同一指标。

滚动（4 组，均为 60 屏真 rAF、0 兜底、0 未覆盖）：

| 场景 | 最大帧 / p95 | droppedFrames |
| --- | --- | --- |
| 标准并排 | 18 / 17 | 0 |
| 标准统一 | 31 / 18 | 3 |
| 真冷并排 | 48 / 17 | 2 |
| 真冷统一 | 18 / 18 | 0 |

保留 31/48ms 等不利值；Q84/D06 未规定慢帧清零，不因孤立最大值增设门槛。

## 原件归档

| 归档清单 | 原件数 | 内容 |
| --- | ---: | --- |
| [静态](../evidence/batch10-repair-static/manifest.json) | 103 | 旧5f检查、新B10检查/构建与两轴复核 |
| [桌面](../evidence/task13-desktop04/manifest.json) | 1551 | 5f与2a51局部操作、C05完整闭环、失败/无效尝试 |
| [内部集成](../evidence/m2-ui-integration-20261025/manifest.json) | 24 | m2-ui快进与旧CI失败对照 |
| [CI修复](../evidence/task10-ci-fixture-final01/manifest.json) | 69 | 修复/更正/两轴原始发现及最新成功CI |
| [最终测量](../evidence/final-measure-20261025/manifest.json) | 见清单 | D06四组、Q98两轮失败与独立派生审核 |

归档不改写历史失败；原CI记录缺少的命令/环境及早期敏感性正对照不补造。文本LaterChanges、身份不明查看和清空UI尚需在任务05收口时区分已有自动化与未测桌面。

## 状态事实

- m2-ui 已按用户新授权在 `2a51` **快进集成**进 m2（tree 一致，无冲突，无新增合并代码）。
- `main` 仍为 `800ac01`，用户图标/AGENTS/配置改动保留，**未 squash**。
- **M3 未实施**：M2 全部通过并合 main 后才开始 M3 访谈与规格。

## 未测边界（必须保留）

- **D01 未通过**：第三轮主动1801.042秒、最终阶段证据完整，但Rust超幅；旧两轮失败保留，本文件不作Q98通过结论。
- 真实桌面剩余补验见 D02/D03/D04/D05/D08 行，未做动作×入口穷举。
- 既有桌面/D06 Release 对应 `2a51`，不是 `f10` 重新构建；本轮完整Q98另有 `6f420a8` 新版构建与独立binary SHA，不倒填其他验收。
- 上述旧版验收未新增测试；本轮补写布局回归但未执行，未扩张必须 GUI 的场景。
