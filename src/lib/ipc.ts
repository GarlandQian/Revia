/**
 * Rust 命令的薄封装。前端只能调用这些固定、带类型的操作，
 * 不拼接 Git 参数，也不传任意目录路径（路径用 Rust 层登记的 ID）。
 *
 * 参数与返回类型由 ts-rs 从 Rust 侧生成（`pnpm bindings`）。
 */

import { invoke } from "@tauri-apps/api/core";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesKind } from "@/types/bindings/ChangesKind";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";
import type { AmendConfirmation } from "@/types/bindings/AmendConfirmation";
import type { BatchOutcome } from "@/types/bindings/BatchOutcome";
import type { BatchTarget } from "@/types/bindings/BatchTarget";
import type { WorktreeStatusEntry } from "@/types/bindings/WorktreeStatusEntry";
import type { CommitContext } from "@/types/bindings/CommitContext";
import type { CommitDraft } from "@/types/bindings/CommitDraft";
import type { CommitOutcome } from "@/types/bindings/CommitOutcome";
import type { CommitOutputEvent } from "@/types/bindings/CommitOutputEvent";
import type { FileVersion } from "@/types/bindings/FileVersion";
import type { RepoEntry } from "@/types/bindings/RepoEntry";
import type { RecoveryInspection } from "@/types/bindings/RecoveryInspection";
import type { RecoveryMaintenance } from "@/types/bindings/RecoveryMaintenance";
import type { SaveAsOwner } from "@/types/bindings/SaveAsOwner";
import type { ReviewMarks } from "@/types/bindings/ReviewMarks";
import type { RestoreOutcome } from "@/types/bindings/RestoreOutcome";
import type { RecoveryState } from "@/types/bindings/RecoveryState";
import type { RecoverySummary } from "@/types/bindings/RecoverySummary";
import type { RowSlice } from "@/types/bindings/RowSlice";
import type { SideBySideLayout } from "@/types/bindings/SideBySideLayout";
import type { SideLine } from "@/types/bindings/SideLine";
import type { IgnoreWhitespace } from "@/types/bindings/IgnoreWhitespace";
import type { ViewMode } from "@/types/bindings/ViewMode";
import type { WorktreeSessionInfo } from "@/types/bindings/WorktreeSessionInfo";

/** 与 Rust 侧 `HighlightResult` 对应。 */
export interface HighlightTokenPayload {
  text: string;
  /** 语法类别索引；名称表由 `syntaxCategories()` 返回。 */
  category: number;
}

export type HighlightLine = HighlightTokenPayload[] | null;

export interface HighlightRangeResult {
  fromLine: number;
  lines: HighlightLine[];
  kind: "highlighted" | "plainText";
  needContent: boolean;
  elapsedMs: number;
  parsedLines: number;
  totalLines: number;
  /** 本结果对应的快照版本；前端据此拒绝晚到的旧结果。 */
  revision: number;
  /** 本结果对应的文件内容版本。 */
  contentVersion: number;
}

/** 与 Rust 侧 `WatchStatus` 对应（Q88、Q95）。 */
export interface WatchStatusPayload {
  active: boolean;
  error: string | null;
  activation: number;
  /** 当前监视目标路径；前端据此确认状态属于自己请求的目标。 */
  target: string | null;
  roots: number;
}

/** 与 Rust 侧 `WatchSnapshot` 对应。 */
export interface WatchSnapshotPayload {
  status: WatchStatusPayload;
  /** 最近一次触发变化的激活代号；只接受属于自己的目标。 */
  changedActivation: number | null;
  /** 每次合并事件递增，不能用固定激活号代替。 */
  eventSequence: number;
}

/** 成功通知按类别开关（E.6）；失败与结果待核对始终弹出。 */
export interface NotifyKinds {
  stage: boolean;
  discard: boolean;
  commit: boolean;
  recovery: boolean;
}

/** 与 Rust 侧 `Settings` 对应。 */
/** 长测诊断只重复的一类操作（M2 任务 01 内存定位）。 */
export type SoakFocus = "idle" | "ipc" | "highlight" | "channel" | "open" | "remount" | "switch" | "scroll" | "jump"
  | "view";

export interface SettingsPayload {
  repos: string[];
  lastWorktreeId: string | null;
  diffFontSize: number;
  themeLight: string;
  themeDark: string;
  codeFontFamily: string | null;
  gitPath: string | null;
  mergeTool: string | null;
  sidebarWidth: number;
  viewMode: "split" | "unified";
  uiZoom: number;
  treeGroupByDirectory: boolean;
  diffContextLines: number;
  diffFoldUnchanged: boolean;
  diffSyncScroll: boolean;
  diffHighlight: "word" | "line" | "char" | "none";
  diffShowWhitespace: boolean;
  diffSoftWrap: boolean;
  diffJumpToNextFile: boolean;
  diffIgnoreWhitespace: number;
  commitSubjectLimit: number;
  commitBlankLineRequired: boolean;
  commitBodyLineLimit: number;
  commitShowRuler: boolean;
  commitWarnCrlf: boolean;
  commitWarnDetached: boolean;
  commitWarnLargeFileBytes: number;
  commitWarnInvalidFilename: boolean;
  /** 各类成功结果是否弹出通知（E.6）。 */
  notify: NotifyKinds;
  mergeAutoApply: boolean;
}

/** 与 Rust 侧 `SettingsPatch` 对应：只传要改的字段。 */
export interface SettingsPatchPayload {
  themeLight?: string;
  themeDark?: string;
  codeFontFamily?: string;
  diffFontSize?: number;
  gitPath?: string;
  mergeTool?: string;
  sidebarWidth?: number;
  viewMode?: "split" | "unified";
  uiZoom?: number;
  treeGroupByDirectory?: boolean;
  diffContextLines?: number;
  diffFoldUnchanged?: boolean;
  diffSyncScroll?: boolean;
  diffHighlight?: "word" | "line" | "char" | "none";
  diffShowWhitespace?: boolean;
  diffSoftWrap?: boolean;
  diffJumpToNextFile?: boolean;
  diffIgnoreWhitespace?: number;
  commitSubjectLimit?: number;
  commitBlankLineRequired?: boolean;
  commitBodyLineLimit?: number;
  commitShowRuler?: boolean;
  commitWarnCrlf?: boolean;
  commitWarnDetached?: boolean;
  commitWarnLargeFileBytes?: number;
  commitWarnInvalidFilename?: boolean;
  notify?: NotifyKinds;
  mergeAutoApply?: boolean;
}

export interface CommandRecordPayload {
  sequence: number;
  program: string;
  args: string[];
  directory: string;
  exitCode: number;
  elapsedMs: number;
  timedOut: boolean;
  stdoutBytes: number;
  stderrExcerpt: string;
}

/** 与 Rust 侧 `CommitOptions` 对应（固定类型，前端不能扩展成任意 Git 参数）。 */
export interface CommitExecuteOptions {
  /** 用户输入的原始提交信息（逐字保留，草稿按它比较）。 */
  message: string;
  amend: boolean;
  /** 跳过 hooks：仅本次命令生效（命令局部全跳过）。 */
  skipHooks: boolean;
  /** 用户所见提交预期（`commitContext` 的返回值原样带回）。 */
  expected: CommitContext["expectation"];
  /** amend 且 HEAD 已包含于本地上游时的二次确认凭据（绑定预期/信息/选项）。 */
  amendConfirmation: AmendConfirmation | null;
  /** 本次请求的 nonce：后端原样带回输出事件，前端据此只认领本次事件。 */
  clientNonce: string;
  draftRevision: string;
}

/** 与 Rust 侧 `CommitOutputEvent` 对应的实时输出事件名。 */
export const COMMIT_OUTPUT_EVENT = "revia://commit-output";
export type { CommitOutputEvent };

/** 生成一次提交请求的操作凭据（只允许后端接受的安全字符集）。 */
export function newCommitNonce(): string {
  const random = globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`;
  return `c${random.replace(/[^A-Za-z0-9_-]/g, "").slice(0, 48)}`;
}

export const ipc = {
  listRepos: () => invoke<RepoEntry[]>("list_repos"),
  listSettings: () => invoke<SettingsPayload>("list_settings"),
  addRepo: (path: string) => invoke<RepoEntry>("add_repo", { request: { path } }),
  selectRepo: (repoId: string, worktreeId?: string) =>
    invoke<RepoEntry>("select_repo", {
      request: { repoId, worktreeId: worktreeId ?? null },
    }),
  setDiffFontSize: (size: number) => invoke<SettingsPayload>("set_diff_font_size", { size }),
  updateSettings: (request: SettingsPatchPayload) => invoke<SettingsPayload>("update_settings", { request }),
  reloadEnvironment: () => invoke<void>("reload_environment"),
  listWorktreeStatus: (repoId: string) =>
    invoke<WorktreeStatusEntry[]>("list_worktree_status", { request: { repoId } }),
  openWorktree: (worktreeId: string) =>
    invoke<WorktreeSessionInfo>("open_worktree", { request: { worktreeId } }),
  watchWorktree: (worktreeId: string, activation: number) =>
    invoke<WatchStatusPayload>("watch_worktree", { request: { worktreeId, activation } }),
  stopWatch: () => invoke<WatchStatusPayload>("stop_watch"),
  watchStatus: () => invoke<WatchSnapshotPayload>("watch_status"),
  /** 切换审阅标记（任务 07）：标记按 worktree 身份 + HEAD/工作区版本记录。 */
  setReviewMark: (request: {
    worktreeId: string;
    path: string;
    marked: boolean;
    /** 用户所见版本；后端不一致即拒绝，避免把没看过的内容标成已审阅。 */
    expectedVersions: { head: string | null; worktree: string | null };
  }) =>
    invoke<ReviewMarks>("set_review_mark", { request }),
  listChanges: (worktreeId: string) =>
    invoke<ChangesSnapshot>("list_changes", { request: { worktreeId } }),
  getRows: (request: {
    worktreeId: string;
    path: string;
    changes: ChangesKind;
    view: ViewMode;
    /** 忽略空白档位（Q140）。 */
    ignoreWhitespace?: IgnoreWhitespace;
    from: number;
    to: number;
  }) => invoke<RowSlice>("get_rows", { request }),
  /** 左右对照布局（返修 A1/Q138）：段、折叠区、两侧行数与段操作目标。 */
  sideBySideLayout: (request: {
    worktreeId: string;
    path: string;
    changes: ChangesKind;
    ignoreWhitespace: IgnoreWhitespace;
    contextLines: number;
    highlight: "word" | "line" | "char" | "none";
  }) => invoke<SideBySideLayout>("side_by_side_layout", { request }),
  /** 左右对照取行（返修 A1/Q138）。 */
  sideLines: (request: {
    worktreeId: string;
    path: string;
    changes: ChangesKind;
    ignoreWhitespace: IgnoreWhitespace;
    contextLines: number;
    highlight: "word" | "line" | "char" | "none";
    version: FileVersion;
    from: number;
    to: number;
  }) => invoke<SideLine[]>("side_lines", { request }),
  /** 按段（块）暂存、取消暂存、丢弃（返修 A1/Q142）。 */
  stageSegment: (request: {
    worktreeId: string;
    path: string;
    /** `SegmentAction.credential` 原样传回。 */
    credential: string;
    hunkIndex: number;
    segmentIndex: number;
    ignoreWhitespace: IgnoreWhitespace;
  }) => invoke<ChangesSnapshot>("stage_segment", { request }),
  unstageSegment: (request: {
    worktreeId: string;
    path: string;
    /** `SegmentAction.credential` 原样传回。 */
    credential: string;
    hunkIndex: number;
    segmentIndex: number;
    ignoreWhitespace: IgnoreWhitespace;
  }) => invoke<ChangesSnapshot>("unstage_segment", { request }),
  discardSegment: (request: {
    worktreeId: string;
    path: string;
    /** `SegmentAction.credential` 原样传回。 */
    credential: string;
    snapshotRevision: string;
    hunkIndex: number;
    segmentIndex: number;
    ignoreWhitespace: IgnoreWhitespace;
  }) => invoke<ChangesSnapshot>("discard_segment", { request }),
  highlightRange: (request: {
    worktreeId: string;
    path: string;
    changes: ChangesKind;
    version: FileVersion;
    fromLine: number;
    toLine: number;
  }) => invoke<HighlightRangeResult>("highlight_range", { request }),
  rowIndexForLine: (request: {
    worktreeId: string;
    path: string;
    changes: ChangesKind;
    view: ViewMode;
    line: number;
    version: FileVersion;
    ignoreWhitespace?: IgnoreWhitespace;
    expectedFingerprint?: string;
    expectedSnapshotRevision?: string;
  }) => invoke<number>("row_index_for_line", { request }),
  syntaxCategories: () => invoke<string[]>("syntax_categories"),
  commandLog: () => invoke<CommandRecordPayload[]>("command_log_snapshot"),
  measureConfig: () =>
    invoke<{
      logging: boolean;
      /** 测量/冒烟/长测模式（普通启动为 false）。 */
      measurementMode: boolean;
      autoRun: boolean;
      only: string[] | null;
      outputPath: string | null;
      repoPath: string | null;
      targetPath: string | null;
      smoke: boolean;
      soakSeconds: number | null;
      minimalSoakLogging: boolean;
      soakDiagnostic: { loggingMode: "verbose" | "minimal"; idleSeconds: number; cycles: number | null;
        focus: SoakFocus | null } | null;
    }>("measure_config"),
  writeMeasurement: (payload: unknown) => invoke<string | null>("write_measurement", { payload }),
  debugLog: (message: string) => invoke<void>("debug_log", { message }),
  /**
   * 报告退出保护状态（任务 08）。
   *
   * 带单调递增的 `seq`：Rust 只接受更大的序号，因此「旧 false 晚到」
   * 不会解除已经建立的新保护。返回 Rust 是否接受了这次更新。
   */
  setExitBlocked: (request: { blocked: boolean; seq: number }) =>
    invoke<boolean>("set_exit_blocked", request),
  requestExit: () => invoke<void>("request_exit"),
  changeEntry: (request: {
    worktreeId: string;
    path: string;
    changes: ChangesKind;
    view: ViewMode;
    from: number;
    to: number;
  }) => invoke<ChangeEntry>("change_entry", { request }),
  stageFile: (request: { worktreeId: string; path: string; fingerprint: string }) =>
    invoke<ChangesSnapshot>("stage_file", { request }),
  unstageFile: (request: { worktreeId: string; path: string; fingerprint: string }) =>
    invoke<ChangesSnapshot>("unstage_file", { request }),
  stageHunk: (request: { worktreeId: string; path: string; fingerprint: string; hunkId: string }) =>
    invoke<ChangesSnapshot>("stage_hunk", { request }),
  unstageHunk: (request: { worktreeId: string; path: string; fingerprint: string; hunkId: string }) =>
    invoke<ChangesSnapshot>("unstage_hunk", { request }),
  discardFile: (request: { worktreeId: string; path: string; fingerprint: string; snapshotRevision: string }) =>
    invoke<ChangesSnapshot>("discard_file", { request }),
  discardHunk: (request: { worktreeId: string; path: string; fingerprint: string; snapshotRevision: string; hunkId: string }) =>
    invoke<ChangesSnapshot>("discard_hunk", { request }),
  stageFiles: (request: { worktreeId: string; targets: BatchTarget[] }) =>
    invoke<BatchOutcome>("stage_files", { request }),
  unstageFiles: (request: { worktreeId: string; targets: BatchTarget[] }) =>
    invoke<BatchOutcome>("unstage_files", { request }),
  discardFiles: (request: { worktreeId: string; targets: BatchTarget[]; snapshotRevision: string }) =>
    invoke<BatchOutcome>("discard_files", { request }),
  listRecovery: (worktreeId: string | null) => invoke<RecoverySummary[]>("list_recovery", { worktreeId }),
  inspectRecovery: (worktreeId: string, recordId: string) =>
    invoke<RecoveryInspection>("inspect_recovery", { request: { worktreeId, recordId } }),
  restoreRecovery: (request: {
    worktreeId: string;
    recordId: string;
    /** 用户确认时看到的记录版本；记录/回执/恢复历史任何变化都会使确认失效。 */
    expectedVersion: string;
    expectedPaths: string[];
    expectedState: RecoveryState;
  }) => invoke<RestoreOutcome>("restore_recovery", { request }),
  recoveryTextDiff: (request: { worktreeId: string; recordId: string; path: string }) =>
    invoke<string>("recovery_text_diff", { request }),
  /** 解析另存目标的真实归属（固定类型接口；前端不自己判断路径）。 */
  recoverySaveAsOwner: (destination: string) =>
    invoke<SaveAsOwner>("recovery_save_as_owner", { request: { destination } }),
  saveRecoveryAs: (request: {
    worktreeId: string | null;
    /** 发起前解析到的归属；后端重新解析后必须一致。 */
    expectedOwner: string | null;
    recordId: string;
    path: string;
    destination: string;
  }) => invoke<number>("save_recovery_as", { request }),
  recoveryMaintenance: () => invoke<RecoveryMaintenance>("recovery_maintenance"),
  clearRecovery: () => invoke<number>("clear_recovery"),
  purgeRecovery: () => invoke<number>("purge_recovery"),

  // ---------- 提交工作流（M3 F01–F06） ----------
  /** 提交上下文：预期、模板、最近信息、草稿、上游状态、门禁（只读）。 */
  commitContext: (worktreeId: string) =>
    invoke<CommitContext>("commit_context", { request: { worktreeId } }),
  /** amend 预填用户所见 HEAD 的完整信息；读取时 HEAD 变化即拒绝。 */
  amendHeadMessage: (worktreeId: string, expectedHead: string) =>
    invoke<string | null>("amend_head_message", { request: { worktreeId, expectedHead } }),
  /** 保存该 worktree 的持久草稿（身份绑定；失败必须展示且保留本地输入）。 */
  saveCommitDraft: (request: { worktreeId: string; draft: CommitDraft }) =>
    invoke<void>("save_commit_draft", { request }),
  /** 清除该 worktree 的持久草稿（用户明确丢弃输入时）。 */
  clearCommitDraft: (worktreeId: string) =>
    invoke<void>("clear_commit_draft", { request: { worktreeId } }),
  /** 未解决冲突的文件与两侧状态（冲突对话框）。 */
  listConflicts: (worktreeId: string) =>
    invoke<import("@/types/bindings/ConflictFile").ConflictFile[]>("list_conflicts", {
      request: { worktreeId },
    }),
  /** 合并页签的数据（Q143）：三个版本、按块分好的改动与初始结果。 */
  mergeOpen: (request: { worktreeId: string; path: string; autoApply?: boolean | null }) =>
    invoke<import("@/types/bindings/MergeLayout").MergeLayout>("merge_open", { request }),
  /** “应用”：写前校验、存回收区、写工作区、git add、写后核对。 */
  mergeApply: (request: { worktreeId: string; path: string; result: string; fingerprint: string }) =>
    invoke<import("@/types/bindings/MergeOutcome").MergeOutcome>("merge_apply", { request }),
  /** 整文件接受一侧（可多选）：每个文件带冲突列表或合并页签签发的指纹。 */
  mergeAcceptSide: (request: {
    worktreeId: string;
    targets: Array<{ path: string; fingerprint: string }>;
    side: "ours" | "theirs";
  }) => invoke<import("@/types/bindings/MergeOutcome").MergeOutcome[]>("merge_accept_side", { request }),
  /** 合并页签按内容取高亮（0 基、左闭右开的行范围）。 */
  highlightText: (request: {
    worktreeId: string;
    documentId: string;
    generation: number;
    language: string;
    fromLine: number;
    toLine: number;
    content?: string;
  }) => invoke<HighlightRangeResult>("highlight_text", { request }),
  /** 提交时警告（返修 E.3，Q139）：检查由 Rust 层完成。 */
  commitWarnings: (worktreeId: string) =>
    invoke<import("@/types/bindings/CommitWarning").CommitWarning[]>("commit_warnings", {
      request: { worktreeId },
    }),
  /**
   * 执行提交。只传登记 worktree ID、信息、amend/skip 与用户所见预期；
   * 不传任何 Git 参数。实时输出走 `revia://commit-output` 事件。
   */
  executeCommit: (request: { worktreeId: string; options: CommitExecuteOptions }) =>
    invoke<CommitOutcome>("execute_commit", { request }),
};
