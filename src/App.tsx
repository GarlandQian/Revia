import type { NoticeText } from "@/components/app/Notifications";
import { closeGuardBlockedError, closeGuardFailure, commitOutputListenerFailure, localizedError } from "@/lib/error";
import { checkoutNoteText } from "@/lib/i18n/text";
import { updateNativeMenu } from "@/lib/nativeMenu";
/**
 * 应用外壳：文件列表与只读 diff；M2 提供目标明确的整文件和 hunk 暂存操作。
 */

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesKind } from "@/types/bindings/ChangesKind";
import type { RecoveryState } from "@/types/bindings/RecoveryState";
import type { ReviewMarks } from "@/types/bindings/ReviewMarks";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";
import type { HunkAction } from "@/types/bindings/HunkAction";
import type { RepoEntry } from "@/types/bindings/RepoEntry";
import type { ViewMode } from "@/types/bindings/ViewMode";
import type { WorktreeSessionInfo } from "@/types/bindings/WorktreeSessionInfo";
import type { WorktreeStatusEntry } from "@/types/bindings/WorktreeStatusEntry";
import type { OperationState } from "@/types/bindings/OperationState";
import type { OperationAction } from "@/types/bindings/OperationAction";
import type { AutostashLink } from "@/types/bindings/AutostashLink";
import type { AutostashRestoreOutcome } from "@/types/bindings/AutostashRestoreOutcome";
import type { HistoryCommit } from "@/types/bindings/HistoryCommit";
import type { RewriteOutcome } from "@/types/bindings/RewriteOutcome";
import type { AmendConfirmation } from "@/types/bindings/AmendConfirmation";
import type { CommitContext } from "@/types/bindings/CommitContext";
import type { CommitOutputEvent } from "@/types/bindings/CommitOutputEvent";
import { CommitArea, messageSubmittable, type CommitResultView } from "@/components/app/CommitComposer";
import { CommitComposerState, EMPTY_INPUT, resetInput, type ComposerInput } from "@/lib/commitDraft";
import { CommitDraftPersistence } from "@/lib/commitDraftPersistence";
import { CommitOutputStore } from "@/lib/commitOutput";
import { changelistEntriesForView, changelistEntryForPath } from "@/lib/m7/visibleChangelistSelection";
import { UpdateInstallConfirmation } from "@/lib/m7/updateState";
import { Changelists, selectionOf, type ChangelistControls } from "@/features/m7/Changelists";
import type { ChangelistView } from "@/types/bindings/ChangelistView";
import { invoke } from "@tauri-apps/api/core";
import { ChangesTree, entryKey, treeOrder } from "@/components/app/ChangesTree";
import { Breadcrumbs, EditorTabs, mergeTabKey, tabKey, type DiffToolbarOptions } from "@/components/app/EditorTabs";
import { MergeView } from "@/features/diff/MergeView";
import { ConflictDialog } from "@/components/app/ConflictDialog";
import { CloseRequestDialog, CommitWarningDialog, DiscardDialog, Modal, type DiscardDialogTarget } from "@/components/app/Dialogs";
import type { CommitWarning } from "@/types/bindings/CommitWarning";
import { NotificationStack } from "@/components/app/Notifications";
import { RecoveryPanel } from "@/components/app/RecoveryPanel";
import { SettingsDialog, type SettingsPatch } from "@/components/app/SettingsDialog";
import { ActivityBar, StatusBar, TitleBar, ToolbarGroup, WelcomeScreen } from "@/components/app/Shell";
import { Codicon } from "@/components/ui/codicon";
import { MenuItem, MenuSeparator, Popover } from "@/components/ui/popover";
import { beginSynchronousMeasurement, flushSynchronousMeasurements, logToBackend, setLoggingEnabled } from "@/features/diff/log";
import { ErrorBoundary } from "@/components/app/ErrorBoundary";
import { DiffView, ROW_HEIGHT, type DiffViewHandle } from "@/features/diff/DiffView";
import { SideBySideView, type SideBySideHandle, type DiffOptions } from "@/features/diff/SideBySideView";
import { sameAnchorTarget, type AnchorTarget, type FileAnchor } from "@/features/diff/fileAnchor";
import type { SideBySideLayout } from "@/types/bindings/SideBySideLayout";
import type { SegmentAction } from "@/types/bindings/SegmentAction";
import type { IgnoreWhitespace } from "@/types/bindings/IgnoreWhitespace";
import { HighlightStore } from "@/features/diff/highlightStore";
import { RowWindowStore } from "@/features/diff/rowWindowStore";
import { matchesHunkAction } from "@/features/diff/hunkAction";
import { runMergeWrite, type MergeWriteRequest } from "@/features/merge/mergeWrite";
import type { LogColumns, LogPreviewPosition, UpdateMode, FetchTagMode } from "@/lib/ipc";
import type { HistoryFileTarget } from "@/types/bindings/HistoryFileTarget";
import type { HistoryRef } from "@/types/bindings/HistoryRef";
import type { BranchStart } from "@/types/bindings/BranchStart";
import type { CheckoutTarget } from "@/types/bindings/CheckoutTarget";
import type { CheckoutResult } from "@/types/bindings/CheckoutResult";
import type { CheckoutStrategy } from "@/types/bindings/CheckoutStrategy";
import type { PushRequest } from "@/types/bindings/PushRequest";
import type { RemoteWriteIntent, RemoteWriteLease } from "@/features/branches/remoteWrite";
import {
  branchFailureDetail,
  pushNotice,
  runRemoteWrite,
  updateNotice,
} from "@/features/branches/remoteWrite";
import {
  captureRemoteTarget, commitPushNeedsDialog, confirmRemoteAction, finishCommitAndContinue,
  preparePush, remoteShortcutAction, remoteTargetIsCurrent, runPushSequence,
  type BoundPushPreview, type RemoteTarget,
} from "@/features/branches/remoteContinuation";
import { BranchWidget, SyncActions } from "@/features/branches/BranchWidget";
import { CheckoutConflictDialog, NewBranchDialog, PushDialog } from "@/features/branches/BranchDialogs";
import { useBranches } from "@/features/branches/useBranches";
import type { IncomingToken } from "@/features/branches/incomingState";
import { useLogWorkspace } from "@/features/log/useLogWorkspace";
import { filterForCommit } from "@/features/log/logModel";
import { useCommitTabs } from "@/features/log/logTabs";
import { LogView, type LogHistoryAction } from "@/features/log/LogView";
import { OperationBanner } from "@/features/m6/OperationBanner";
import { readM6State, type M6ReadState } from "@/features/m6/readState";
import { StashPanel } from "@/features/m6/StashPanel";
import { HistoryActionDialog, type HistoryActionRequest } from "@/features/m6/HistoryActionDialog";
import { FileHistoryDialog } from "@/features/m6/FileHistoryDialog";
import { savedAutostashNotice, rewriteOutcomeDetail, rewriteFailureNotice } from "@/features/m6/notices";
import { ExternalToolDialog, type ExternalToolTarget } from "@/features/m6/ExternalToolDialog";
import { CommitDiffView } from "@/features/log/CommitDiffView";
import { CommitMergeView } from "@/features/log/CommitMergeView";
import { CommandLogPanel } from "@/features/commands/CommandLogPanel";
import { EditorView } from "@codemirror/view";
import { monitorSoakVisibility, sleep, waitForSoak, type SoakVisibility } from "@/lib/soak";
import { soakScroller } from "@/lib/soakScroller";
import { ipc, COMMIT_OUTPUT_EVENT, newCommitNonce, type SoakFocus, type WatchStatusPayload } from "@/lib/ipc";
import { startIpcDiagnostics } from "@/lib/ipcDiagnostics";
import { diagnoseJumpDom, observeJumpDom } from "@/lib/jumpDiagnostics";
import { messages, resolveLocale, setLocale, setSystemLanguage, subscribeLocale, t, type LanguagePreference } from "@/lib/i18n/index";
import { operationKindText, readonlyReasonText } from "@/lib/i18n/text";
import { isButtonTarget, isCommitMessageTarget, isPrimaryModifier, isTextInputOrDialog, isAltGraph, isImeComposing, isWindows } from "@/lib/platform";
import { OperationResultGate, RefreshCoordinator, ReviewOverlay, SelectionGate, SnapshotGate, WatchEventGate } from "@/lib/refreshGate";
import { mergePublishedReview, publishReviewSuccess, readThroughReview, ReviewRecoveryScheduler, reviewedSetForNavigation } from "@/lib/refreshGate";
import { CloseCoordinator, CloseDriver, CloseGuardGate, ExitGuardSync, runReviewRecoveryBatch, WriteGuardOrchestrator } from "@/lib/refreshGate";
import type { CloseDriverEffect } from "@/lib/refreshGate";
import type { CloseDecision, CloseIntent } from "@/lib/refreshGate";
import type { ReviewReadAttempt, ReviewRecoveryBatchOutcome } from "@/lib/refreshGate";
import type { OperationContext } from "@/lib/refreshGate";
import { subscribeToForeground } from "@/lib/foregroundRefresh";
import { formatAppError, resultNeedsVerification, getErrorMessage, toAppError } from "@/lib/error";
import { useAppearance } from "@/hooks/useAppearance";
import { matchShortcut, setShortcutOverrides as applyShortcutOverrides } from "@/lib/shortcuts";
import { GlassLayer } from "@/features/m7/glass/GlassLayer";
import { reviewDraftSessions } from "@/lib/m7/reviewDrafts";
import { mergeBufferGuards } from "@/lib/m7/mergeBuffers";
import { flushM7State, registerM7Flusher } from "@/lib/m7/flush";
import { m7, onM7Events } from "@/lib/m7/api";
import { EditableWorkspace, EditDecisionDialog } from "@/features/m7/EditableWorkspace";
import { EditSessions, type EditSession, type EditTarget } from "@/lib/m7/editSession";
import { runEditWrite } from "@/lib/m7/editWrite";
import type { EditSavePayload } from "@/lib/m7/api";
import type { EditSaved } from "@/types/bindings/EditSaved";
import { TerminalPanel } from "@/features/m7/TerminalPanel";
import { TerminalSessions } from "@/lib/m7/terminalSessions";
import type { TerminalSessionInfo } from "@/types/bindings/TerminalSessionInfo";
import { CloneDialog } from "@/features/m7/CloneDialog";
import { GithubPanel } from "@/features/m7/GithubPanel";
import { DiagnosticsDialog } from "@/features/m7/DiagnosticsDialog";
import { UpdateBanner, UpdatePanel } from "@/features/m7/UpdatePanel";
import type { AppUpdateMode } from "@/types/bindings/AppUpdateMode";
import type { AppUpdateStatus } from "@/types/bindings/AppUpdateStatus";
import { useNotifications, type NoticeInput } from "@/hooks/useNotifications";
import { useTreeSelection } from "@/hooks/useTreeSelection";
import { useEditorTabs } from "@/hooks/useEditorTabs";
import { activeMergeEntry } from "@/hooks/editorTabsState";
import { useBatchWrite } from "@/hooks/useBatchWrite";
import {
  formatMs,
  measureScroll,
  nextTask,
  nextPaint,
  nowStamp,
  percentile,
  probeRaf,
  visibilitySample,
  type FirstScreenSample,
  type JumpSample,
  type Measurement,
  type ScrollSample,
} from "@/lib/measure";

interface MeasureConfig {
  /** 测量/冒烟/长测模式（任务 12：普通启动不显示测量面板）。 */
  measurementMode: boolean;
  smoke: boolean;
  autoRun: boolean;
  only: string[] | null;
  outputPath: string | null;
  repoPath: string | null;
  targetPath: string | null;
  soakSeconds: number | null;
  minimalSoakLogging: boolean;
  soakDiagnostic: { loggingMode: "verbose" | "minimal"; idleSeconds: number; cycles: number | null;
    focus: SoakFocus | null } | null;
}

/** 文件列表里的选择（同一文件可同时出现在两组）。 */
export interface Selection {
  path: string;
  changes: ChangesKind;
}

/**
 * 写目标：整块（hunkId）或按段（segment）二选一。
 * 整文件写走批量入口（performBatchWrite），不经过这里（返修 C15）。
 */
type StagingTarget = Pick<HunkAction, "worktreeId" | "path" | "changes" | "fingerprint"> &
  (
    | { hunkId: string; segment?: undefined }
    /** 按段（块）操作时的段目标（返修 A1/Q142）：与显示同批签发。 */
    | {
        segment: { hunkIndex: number; segmentIndex: number; ignoreWhitespace: IgnoreWhitespace; credential: string };
        hunkId?: undefined;
      }
  );
type DiscardTarget = StagingTarget & { snapshotRevision: string };

/** 一次丢弃确认：可含多个文件，或一个块。 */
interface DiscardRequestState {
  targets: DiscardDialogTarget[];
  entries: ChangeEntry[];
  worktree: string;
  snapshotRevision: string;
  activation: number;
  /** 只丢弃一个块时保留原来的单块写入口。 */
  hunk?: StagingTarget;
  /** 只丢弃一段（块）时的段目标（返修 A1/Q142）。 */
  segment?: StagingTarget;
}

type IpcDiagnosticSample = ReturnType<ReturnType<typeof startIpcDiagnostics>["snapshot"]>;

const DEFAULT_VIEWPORT_ROWS = 45;

function debugLog(message: string) {
  logToBackend(message);
}

/**
 * 不可信读取的重新核对次数上限（ST07-R15-2）。
 *
 * 正常情况一次重读就能拿到更新的后端真值（序号前进）；连续多次都不前进说明
 * 后端也没有更新的状态，继续重试只会造成请求风暴，所以有界停止。
 */
const REVIEW_RECOVERY_MAX_ATTEMPTS = 3;

export function App() {
  const [repos, setRepos] = useState<RepoEntry[]>([]);
  const [activeRepo, setActiveRepo] = useState<RepoEntry | null>(null);
  const [worktreeId, setWorktreeId] = useState<string | null>(null);
  const [info, setInfo] = useState<WorktreeSessionInfo | null>(null);
  const infoRef = useRef(info);
  infoRef.current = info;
  const [snapshot, setSnapshot] = useState<ChangesSnapshot | null>(null);
  const snapshotRef = useRef(snapshot);
  snapshotRef.current = snapshot;
  const worktreeIdRef = useRef(worktreeId);
  worktreeIdRef.current = worktreeId;
  /** 快照接收门：只接受当前激活 worktree 中版本不倒退的快照（Q88、Q90）。 */
  const gateRef = useRef(new SnapshotGate());
  /** 选择序号：每次选中/切换自增；晚到的选择与元数据响应必须核对它（Q88）。 */
  const selectionEpochRef = useRef(0);
  /** 选择上下文门与写结果门：与 refreshGate 中的实现共用，便于确定性测试。 */
  const selectionGateRef = useRef(new SelectionGate());
  const operationGateRef = useRef(new OperationResultGate(gateRef.current));
  /**
   * 本地已成功的审阅变更（R07-D1）：较早读取不能回退它们。
   *
   * 读取请求发起时记 `baseline()`，快照交付时只叠加「发起之后」且版本仍一致的变更；
   * 版本真的变了则不叠加，标记按后端结果失效。
   */
  const reviewOverlayRef = useRef(new ReviewOverlay());
  const [watchState, setWatchState] = useState<Pick<WatchStatusPayload, "active" | "error">>({
    active: false,
    error: null,
  });
  const [selected, setSelected] = useState<Selection | null>(null);
  // 并排 / 统一视图：选择存在设置里（返修 C12），初始值由 loadSettings 后端值填充。
  const [view, setView] = useState<ViewMode>("split");
  const viewRef = useRef(view);
  viewRef.current = view;

  // ---------- 返修 E：Q139/Q140 设置状态（存 Rust 设置，不用 localStorage） ----------
  /** 提交工具窗口宽度（可拖动分隔条，B4）。 */
  const [sidebarWidth, setSidebarWidth] = useState(330);
  const sidebarDragRef = useRef<{
    pointerId: number; clientX: number; width: number; scale: number; maxWidth: number; latestWidth: number;
  } | null>(null);
  /** 改动树按目录分组（false 为平铺，Q139/A2）。 */
  const [treeGroupByDirectory, setTreeGroupByDirectory] = useState(false);
  /** diff 显示选项（Q139）。 */
  const [diffOptions, setDiffOptions] = useState({
    contextLines: 3,
    foldUnchanged: false,
    syncScroll: true,
    highlight: "word" as "word" | "line" | "char" | "none",
    showWhitespace: false,
    softWrap: false,
    jumpToNextFile: true,
    ignoreWhitespace: 0 as 0 | 1 | 2 | 3,
  });
  /** 提交信息检查与提交时警告（Q139）。 */
  const [changelistMode, setChangelistMode] = useState(false);
  const changelistModeRef = useRef(changelistMode);
  changelistModeRef.current = changelistMode;
  const [changelistView, setChangelistView] = useState<ChangelistView | null>(null);
  const changelistRef = useRef<ChangelistView | null>(null);
  const changelistControls = useRef<ChangelistControls | null>(null);
  const [changelistSelectedPaths, setChangelistSelectedPaths] = useState<{worktreeId: string; paths: string[]}>({worktreeId:"",paths:[]});
  const changelistSelectedPathsRef = useRef(changelistSelectedPaths);
  changelistSelectedPathsRef.current = changelistSelectedPaths;
  const acceptChangelistSelection = useCallback((worktreeId: string, paths: string[]) => setChangelistSelectedPaths((current) => current.worktreeId === worktreeId && JSON.stringify(current.paths) === JSON.stringify(paths) ? current : {worktreeId,paths}), []);
  const acceptChangelistView = useCallback((view: ChangelistView | null) => { changelistRef.current = view; setChangelistView(view); setAmendConfirm(null); setCommitWarningConfirm(null); }, []);
  const [commitChecks, setCommitChecks] = useState({
    subjectLimit: 72,
    blankLineRequired: false,
    bodyLineLimit: 0,
    showRuler: false,
    warnCrlf: true,
    warnDetached: true,
    warnLargeFileBytes: 50 * 1024 * 1024,
    warnInvalidFilename: true,
  });
  const diffOptionsRef = useRef(diffOptions);
  diffOptionsRef.current = diffOptions;

  const { notifyKinds, setNotifyKinds, notify, setCommitResults, allNotices, dismissAllNotices } = useNotifications();
  const notifyM6 = useCallback((target: string, notice: NoticeInput) => {
    const path = repos.flatMap((repo) => repo.worktrees).find((worktree) => worktree.id === target)?.path ?? target;
    notify({ ...notice, worktreePath: path });
  }, [notify, repos]);
  /** 合并时自动应用无冲突改动（Q143）。 */
  const [mergeAutoApply, setMergeAutoApply] = useState(true);
  /** 文件历史是否显示每次提交对应的文件名/路径（F01，默认关）。 */
  const [fileHistoryShowNames, setFileHistoryShowNames] = useState(false);
  const [stashCompareLocal, setStashCompareLocal] = useState(true);

  // ---------- M5 第二批：日志、分支与远端状态 ----------
  /** 日志设置（F06，随现有设置持久化）。 */
  const [logSettings, setLogSettings] = useState<{
    columns: LogColumns;
    longEdges: boolean;
    firstParent: boolean;
    previewPosition: LogPreviewPosition;
  }>({ columns: { author: true, date: true, hash: false }, longEdges: false, firstParent: false, previewPosition: "right" });
  /** 更新与推送设置（F05）。 */
  const [pushSettings, setPushSettings] = useState<{
    updateMode: UpdateMode;
    fetchTagMode: FetchTagMode;
    fetchAutomatically: boolean;
    checkRemote: boolean;
    autoUpdateOnPushRejection: boolean;
    pushDialogOnCommit: boolean;
    pushDialogProtectedOnly: boolean;
    protectedBranches: string[];
  }>({
    updateMode: "merge",
    fetchTagMode: "auto",
    fetchAutomatically: false,
    checkRemote: false,
    autoUpdateOnPushRejection: false,
    pushDialogOnCommit: true,
    pushDialogProtectedOnly: false,
    protectedBranches: ["master", "main"],
  });
  /** 日志的只读历史目标（随 worktree 打开 / 关闭）。 */
  const [historySession, setHistorySession] = useState<{
    worktreeId: string; epoch: number; targetId: string; refs: HistoryRef[];
  } | null>(null);
  /** 环境重载后代际递增：旧会话的历史目标已失效，必须重新打开。 */
  const [historyEpoch, setHistoryEpoch] = useState(0);
  const currentHistorySession = historySession?.worktreeId === worktreeId
    && historySession.epoch === historyEpoch ? historySession : null;
  const historyTargetId = currentHistorySession?.targetId ?? null;
  const historyRefs = currentHistorySession?.refs;
  /** 主区域当前显示：日志、改动 diff 或提交 diff。 */
  const [mainView, setMainView] = useState<"log" | "diff" | "commit">("log");
  const mainViewRef = useRef(mainView);
  mainViewRef.current = mainView;
  /** diff 预览开关（L05，默认关；不持久化）。 */
  const [logPreviewOpen, setLogPreviewOpen] = useState(false);
  /** Git 命令日志面板（B07，默认隐藏；入口在状态栏，菜单入口随原生菜单一起做）。 */
  const [commandLogOpen, setCommandLogOpen] = useState(false);
  /** 新建分支 / 检出确认 / 推送对话框的界面状态。 */
  const [newBranchStart, setNewBranchStart] = useState<{ origin: RemoteTarget; start: BranchStart } | null>(null);
  const newBranchDialogRef = useRef(newBranchStart);
  newBranchDialogRef.current = newBranchStart;
  const [newBranchError, setNewBranchError] = useState<AppErrorPayload | null>(null);
  const [checkoutBlocked, setCheckoutBlocked] = useState<{
    origin: RemoteTarget; target: CheckoutTarget; label: string; files: string[];
  } | null>(null);
  const [pushDialog, setPushDialog] = useState<BoundPushPreview | null>(null);
  const [pushDialogError, setPushDialogError] = useState<string | null>(null);
  /** 推送被拒绝且没有开启自动更新时的提示（B05：提供“更新后再推送”）。 */
  const [pushRejected, setPushRejected] = useState<{ origin: RemoteTarget; request: PushRequest } | null>(null);
  /** 提交成功后进入“提交并推送”的转发（performCommit 早于推送编排定义）。 */
  const commitAndPushRef = useRef<((origin: RemoteTarget, lease: RemoteWriteLease) => Promise<void>) | null>(null);
  const logWorkspace = useLogWorkspace(worktreeId);
  const commitTabs = useCommitTabs();
  const dropCommitTarget = commitTabs.dropTarget;

  const selectedRef = useRef(selected);
  selectedRef.current = selected;

  const {
    themeLight, setThemeLight, themeDark, setThemeDark, codeFontFamily, setCodeFontFamily,
    fontSize, setFontSize, changeFontSize, uiZoom, setUiZoom, uiZoomRef, reloadImportedThemes,
  } = useAppearance();
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  // ---------- M4 界面状态 ----------
  /** 左侧工具窗口是否显示；点击图标条切换（F02.2）。 */
  const [sidebarVisible, setSidebarVisible] = useState(true);
  const [sideTab, setSideTab] = useState<"commit" | "recovery" | "stash">("commit");
  /** 语言切换后强制整树重新渲染（messages 代理随语言变化）。 */
  const [localeRevision, setLocaleRevision] = useState(0);
  useEffect(() => subscribeLocale(() => setLocaleRevision((value) => value + 1)), []);
  const [settingsOpen, setSettingsOpen] = useState(false);
  // ---------- M7 扩展状态（终端、编辑、克隆、PR、诊断、更新） ----------
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [terminalHeight, setTerminalHeight] = useState(240);
  const terminalCallbacksRef = useRef({ report: (_id: string | null, _error: AppErrorPayload) => {}, blocked: (_id: string): boolean => false, activity: (_id: string) => {} });
  const [terminalSessions] = useState(() => new TerminalSessions({
    list: () => m7.terminalList(), loadTabs: () => m7.terminalTabs(), saveTabs: (tabs) => m7.terminalSaveTabs(tabs),
    open: (id, cwd) => m7.terminalOpen({ worktreeId: id, cwd, rows: 24, cols: 80 }),
    close: (id) => m7.terminalClose(id), write: (id, data) => m7.terminalWrite(id, data),
    resize: (id, rows, cols) => m7.terminalResize(id, rows, cols),
    report: (id, error) => terminalCallbacksRef.current.report(id, error),
    blocked: (id) => terminalCallbacksRef.current.blocked(id), activity: (id) => terminalCallbacksRef.current.activity(id),
  }));
  useSyncExternalStore(terminalSessions.subscribe, terminalSessions.snapshot);
  const [terminalConfirmation, setTerminalConfirmation] = useState<TerminalSessionInfo[] | null>(null);
  const terminalConfirmationRef = useRef<((confirmed: boolean) => void) | null>(null);
  const askTerminalShutdown = useCallback((sessions: TerminalSessionInfo[]): Promise<boolean> => new Promise((resolve) => {
    terminalConfirmationRef.current = resolve; setTerminalConfirmation(sessions);
  }), []);
  const finishTerminalConfirmation = useCallback((confirmed: boolean) => {
    const resolve = terminalConfirmationRef.current; terminalConfirmationRef.current = null;
    setTerminalConfirmation(null); resolve?.(confirmed);
  }, []);
  const terminalActivityTimersRef = useRef(new Map<string, ReturnType<typeof setTimeout>>());

  /** 正在编辑的文件（同一时刻只编辑一个，切换文件前先处理未保存改动）。 */
  const [editingPath, setEditingPath] = useState<string | null>(null);
  const [editingWorktreeId, setEditingWorktreeId] = useState<string | null>(null);
  const editWriteRef = useRef<((request: EditSavePayload) => Promise<EditSaved>) | null>(null);
  const editReportRef = useRef<(target: EditTarget, error: AppErrorPayload) => void>(() => undefined);
  const editBlockedRef = useRef<(id: string) => boolean>(() => false);
  const [editSessions] = useState(() => new EditSessions({
    open: ({ worktreeId, path }) => m7.openEditable(worktreeId, path),
    loadDraft: ({ worktreeId, path }) => m7.loadEditDraft(worktreeId, path),
    listDrafts: (id) => m7.listEditDrafts(id),
    persist: (draft) => m7.saveEditDraft(draft),
    clear: ({ worktreeId, path }, identity) => m7.clearEditDraft(worktreeId, path, identity),
    save: (request) => editWriteRef.current ? editWriteRef.current(request) : Promise.reject(localizedError("internal", "errors.closeGuard.installing")),
    blocked: (id) => editBlockedRef.current(id),
    report: (target, error) => editReportRef.current(target, error),
  }));
  useSyncExternalStore(editSessions.subscribe, editSessions.snapshot);
  const editDirty = editingPath && editingWorktreeId
    ? editSessions.get({ worktreeId: editingWorktreeId, path: editingPath }).dirty : false;
  const [editDecision, setEditDecision] = useState<EditSession | null>(null);
  const editDecisionRef = useRef<((resolved: boolean) => void) | null>(null);
  const askEdit = useCallback((session: EditSession): Promise<boolean> => new Promise((resolve) => {
    editDecisionRef.current = resolve; setEditDecision(session);
  }), []);
  const finishEditDecision = useCallback((resolved: boolean) => {
    const resolve = editDecisionRef.current; editDecisionRef.current = null;
    setEditDecision(null); resolve?.(resolved);
  }, []);
  const editWorktreesRef = useRef<string[]>([]);
  editWorktreesRef.current = [...new Set([...repos.filter((repo) => repo.exists).flatMap((repo) => repo.worktrees.map((worktree) => worktree.id)), ...editSessions.worktreeIds, ...(worktreeId ? [worktreeId] : [])])];
  const lifecycleRequestRef = useRef<(intent: CloseIntent) => Promise<void>>(async () => undefined);
  const lifecyclePreparingRef = useRef(false);
  const updateLifecycleRef = useRef<{ intent: "installUpdate" | "restartUpdate"; resolve: (status: AppUpdateStatus | null) => void; reject: (error: unknown) => void } | null>(null);
  const [repoToRemove, setRepoToRemove] = useState<RepoEntry | null>(null);
  const [repoRemovalError, setRepoRemovalError] = useState<AppErrorPayload | null>(null);
  const [cloneOpen, setCloneOpen] = useState(false);
  const [githubOpen, setGithubOpen] = useState(false);
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const [updateOpen, setUpdateOpen] = useState(false);
  const [appUpdateMode, setAppUpdateMode] = useState<AppUpdateMode>("notify");
  /** F22：预发布渠道开关（设置持久化）。 */
  const [allowPrerelease, setAllowPrerelease] = useState(false);
  const updateChannelEpoch = useRef(0);
  const [shortcutsSnapshot, setShortcutsSnapshot] = useState<Record<string, string>>({});
  const [updateStatus, setUpdateStatus] = useState<AppUpdateStatus | null>(null);
  const [updateBannerVersion, setUpdateBannerVersion] = useState<string | null>(null);
  /** Windows 安装会退出应用：确认“安装并重启”后才进入既有生命周期（R2）。 */
  const updateInstallConfirmationRef = useRef(new UpdateInstallConfirmation(isWindows));
  const [updateInstallConfirmOpen, setUpdateInstallConfirmOpen] = useState(false);
  /** 冲突对话框（返修 F10）。 */
  const [conflictDialogOpen, setConflictDialogOpen] = useState(false);
  // 列表和页签共享导航入口；实际取数仍由 openEntry 的选择门校验。
  const entryNavigationRef = useRef<((entry: ChangeEntry) => void) | null>(null);
  const navigateEntry = useCallback((entry: ChangeEntry) => entryNavigationRef.current?.(entry), []);
  const entryOrder = useCallback((current: ChangesSnapshot): ChangeEntry[] => {
    return changelistModeRef.current ? changelistEntriesForView(current, changelistRef.current) : treeOrder(current);
  }, []);

  const {
    treeSelection, groupOpen, setGroupOpen, dirOpen, followActiveEntry,
    expandToSelection, treeSelectedEntries, activateTreeEntry, moveTreeSelection,
    selectAllEntries, toggleGroup, toggleDir,
  } = useTreeSelection({ snapshot, snapshotRef, selectedRef, openEntry: navigateEntry, order: entryOrder });
  /** worktree 下拉的只读状态（F02.1、F09.2）。 */
  const [worktreeStatuses, setWorktreeStatuses] = useState<WorktreeStatusEntry[]>([]);
  /** 丢弃确认（可含多个文件，或一个块）。 */
  const [discardRequest, setDiscardRequest] = useState<DiscardRequestState | null>(null);

  const [gitPath, setGitPath] = useState<string | null>(null);
  const [mergeTool, setMergeTool] = useState<string | null>(null);
  const [diffTool, setDiffTool] = useState<string | null>(null);
  const [language, setLanguage] = useState<LanguagePreference>("auto");
  const [cherryPickKeepEmpty, setCherryPickKeepEmpty] = useState(false);
  const [cherryPickIncludeSource, setCherryPickIncludeSource] = useState(false);
  // M6：进行中操作、Stash 自动保存与历史改写 / 文件历史的界面状态。
  const [m6State, setM6State] = useState<(M6ReadState & { origin: RemoteTarget }) | null>(null);
  const m6ReadSeq = useRef(0);
  const currentM6 = m6State && remoteTargetIsCurrent(m6State.origin, gateRef.current.token()) ? m6State : null;
  const operationState = currentM6?.operation ?? null;
  const autostashLink = currentM6?.pending?.link ?? null;
  const [m6Busy, setM6Busy] = useState(false);
  const [stashRevision, setStashRevision] = useState(0);
  const [stashFocusOid, setStashFocusOid] = useState<string | null>(null);
  const [historyAction, setHistoryAction] = useState<HistoryActionRequest | null>(null);
  const historyActionSeqRef = useRef(0);
  const notifiedAutostashRef = useRef(new Set<string>());
  const [historyMessage, setHistoryMessage] = useState("");
  const [fileHistory, setFileHistory] = useState<{ path: string; revision: string | null; origin: RemoteTarget } | null>(null);
  const openFileHistory = useCallback((path: string, revision: string | null, origin: RemoteTarget | null) => {
    if (origin && remoteTargetIsCurrent(origin, gateRef.current.token())) setFileHistory({ path, revision, origin });
  }, []);
  const [externalTool, setExternalTool] = useState<ExternalToolTarget | null>(null);
  /** 外部工具对话框绑定打开时的目标，而不是写操作发生时的当前 worktree。 */
  const openExternalTool = useCallback((target: Omit<ExternalToolTarget, "origin">, origin: RemoteTarget | null) => {
    if (origin && remoteTargetIsCurrent(origin, gateRef.current.token())) setExternalTool({ ...target, origin });
  }, []);
  const [busy, setBusy] = useState(false);
  /**
   * 状态文案。任务 12 起不再显示在工具栏（工具栏只保留日常操作），
   * 但仍由测量/长测诊断写入并在测量面板与日志里使用，因此保留采集。
   */
  /**
   * 状态文案。任务 12 起不再显示在工具栏（工具栏只保留日常操作），
   * 但仍由测量/长测诊断写入，并在测量模式下作为面板标题栏的状态显示。
   */
  const [status, setStatus] = useState("");
  const [worktreeErrors, setWorktreeErrors] = useState<Record<string, AppErrorPayload | null>>({});
  const [globalError, setGlobalError] = useState<AppErrorPayload | null>(null);
  /**
   * 无归属操作（另存到合法外部路径）的 App 级结果。
   *
   * 外部目标不属于任何 worktree：结果既不能借用当前 worktree 的错误槽
   * （`setAppError(err, null)` 会回落到当前选择），也不能只抛给面板
   * （面板关闭/返回列表后迟到结果会被丢弃）。这里用独立的序号通道保留，
   * 并保证旧请求的完成不会清掉较新的错误（Q90、Q91）。
   */
  const [externalResult, setExternalResult] = useState<{ seq: number; error: AppErrorPayload } | null>(null);
  const externalSeqRef = useRef(0);
  const externalResultRef = useRef(externalResult);
  externalResultRef.current = externalResult;
  const error = (worktreeId ? worktreeErrors[worktreeId] : null) ?? globalError ?? externalResult?.error ?? null;

  /**
   * 每个 worktree 的错误代际：每次写入错误/清除错误都递增。
   *
   * 恢复读取用它判断「我发起时看到的那条错误是否还是最新的」，
   * 从而避免旧恢复读的失败覆盖较新的错误（SP07-R16-2）。
   */
  const worktreeErrorEpochRef = useRef<Record<string, number>>({});

  /**
   * 当前 worktree id 的 ref：让 `setAppError` 的**身份保持稳定**。
   *
   * 关闭/退出保护监听的 effect 依赖 `setAppError`；如果它随 worktree 变化，
   * 切换 worktree 就会卸载并重装监听，在途写保护出现空档（ST-B04-08-1）。
   */
  const worktreeIdRefForError = useRef<string | null>(null);
  worktreeIdRefForError.current = worktreeId;

  const setAppError = useCallback((err: AppErrorPayload | null, targetWorktreeId?: string | null) => {
    const wtId = targetWorktreeId ?? worktreeIdRefForError.current;
    if (wtId) {
      worktreeErrorEpochRef.current[wtId] = (worktreeErrorEpochRef.current[wtId] ?? 0) + 1;
      setWorktreeErrors((prev) => ({ ...prev, [wtId]: err }));
    } else {
      setGlobalError(err);
    }
  }, []);

  editReportRef.current = (target, error) => setAppError(error, target.worktreeId);

  useEffect(() => {
    let active = true;
    const failed = (caught: unknown) => {
      if (active) setGlobalError({ ...localizedError("environment", "errors.nativeMenu.unavailable"), cause: toAppError(caught) });
    };
    void updateNativeMenu(() => {
      void getCurrentWindow().close().catch((caught) => { if (active) setGlobalError(toAppError(caught)); });
    }).catch(failed);
    return () => { active = false; };
  }, [localeRevision]);

  /** 记录一次无归属操作的结果；`seq` 用来拒绝旧完成覆盖较新的错误。 */
  const recordExternalResult = useCallback((seq: number, err: AppErrorPayload | null) => {
    setExternalResult((prev) => {
      if (err === null) {
        // 只有比当前错误更新的成功才能清除它。
        return prev && prev.seq >= seq ? prev : null;
      }
      return prev && prev.seq > seq ? prev : { seq, error: err };
    });
  }, []);

  const dismissError = useCallback(() => {
    // 用户主动关闭提示也算一次「错误状态变更」：推进代际，
    // 让仍在途的旧恢复读失去回填资格（ST07-R17-2）。
    if (worktreeId) {
      worktreeErrorEpochRef.current[worktreeId] = (worktreeErrorEpochRef.current[worktreeId] ?? 0) + 1;
      setWorktreeErrors((prev) => ({ ...prev, [worktreeId]: null }));
    }
    setGlobalError(null);
    setExternalResult(null);
  }, [worktreeId]);

  const [measurements, setMeasurements] = useState<Measurement[]>([]);
  /** 是否处于测量/冒烟/长测模式：只有这些模式才显示测量面板（任务 12、D08）。 */
  const [measurementMode, setMeasurementMode] = useState(false);
  const [panelOpen, setPanelOpen] = useState(true);
  const [bootstrapDone, setBootstrapDone] = useState(false);

  const diffRef = useRef<DiffViewHandle | null>(null);
  /** 左右对照视图（返修 A1/Q138）的操作句柄。 */
  const sbsRef = useRef<SideBySideHandle | null>(null);
  /** 左右对照布局：段数、改动处数与跳转目标（由视图回报）。 */
  const [sideLayout, setSideLayout] = useState<SideBySideLayout | null>(null);
  /** 冒烟检查用：转发到页签 hook 的合并入口。 */
  const openMergeTabRef = useRef<((entry: ChangeEntry) => void) | null>(null);
  const rowStore = useMemo(
    () =>
      new RowWindowStore(async (request) => {
        const slice = await ipc.getRows(request);
        return { rows: slice.rows, rowCount: slice.rowCount, revision: slice.revision, entry: slice.entry, navigationRows: slice.navigationRows };
      }),
    [],
  );
  const unifiedNavigationRows = useSyncExternalStore(rowStore.subscribe, rowStore.getNavigationRows);
  const unifiedJumpRef = useRef<{ epoch: number; row: number; scrollTop: number } | null>(null);
  const highlightStore = useMemo(() => new HighlightStore(), []);
  const autoRan = useRef(false);
  const firstScreenRef = useRef<FirstScreenSample>({
    fromListChangesMs: null,
    fromOpenWorktreeMs: null,
    listChangesMs: null,
    firstRowsMs: null,
    renderMs: null,
    rowCount: null,
    fileCount: null,
    rafPainted: false,
    viewportHeight: null,
    devicePixelRatio: null,
  });
  const pendingAnchor = useRef<(FileAnchor & AnchorTarget & { view: ViewMode }) | null>(null);
  useEffect(() => {
    if (mainView !== "diff") pendingAnchor.current = null;
  }, [mainView]);

  const clearEditorSelection = useCallback(() => {
    selectionEpochRef.current += 1;
    selectionGateRef.current.clear();
    setSelected(null);
    rowStore.setTarget({
      worktreeId: worktreeIdRef.current ?? "", path: "", changes: "unstaged",
      view: viewRef.current, fingerprint: "", snapshotRevision: "",
    }, 0, null);
  }, [rowStore]);
  const selectMergeEntry = useCallback((entry: ChangeEntry) => {
    // 激活合并页签也使仍在途的 diff 选择失效，避免旧回包夺回当前页。
    selectionEpochRef.current += 1;
    selectionGateRef.current.clear();
    setMainView("diff");
    setSelected({ path: entry.path, changes: entry.changes });
  }, []);
  const {
    tabs, mergeTabPath, mergeTabPathRef, mergeTabTargetRef, enterDiff, recordDiffTab, resetMergeTabs,
    openMergeTab, activateTab, closeTab, closeMergeTab, closeAllTabs,
  } = useEditorTabs({
    snapshot, snapshotRef, selectedRef, worktreeIdRef,
    openEntry: navigateEntry, selectMergeEntry, clearSelection: clearEditorSelection,
  });
  openMergeTabRef.current = openMergeTab;

  const selectedEntry: ChangeEntry | null = useMemo(() => {
    if (!snapshot || !selected) return null;
    return (
      snapshot.entries.find(
        (entry) => entry.path === selected.path && entry.changes === selected.changes,
      ) ?? null
    );
  }, [selected, snapshot]);

  // 计数与跳转只使用当前显示档位的段首；原始列表与写凭据不被改写。
  const navigationRows = selectedEntry && worktreeId && view === "unified" && rowStore.isCurrent({
    worktreeId, path: selectedEntry.path, changes: selectedEntry.changes, view,
    ignoreWhitespace: ignoreWhitespaceOf(diffOptions.ignoreWhitespace),
    fingerprint: selectedEntry.fingerprint, snapshotRevision: selectedEntry.snapshotRevision,
  }) ? unifiedNavigationRows : null;
  const displayChangeCount = view === "split" ? sideLayout?.segments.length ?? 0 : navigationRows?.length ?? 0;

  // ---------------- 打开仓库 / 刷新 ----------------

  const applySnapshot = useCallback((next: ChangesSnapshot) => {
    snapshotRef.current = next;
    setSnapshot(next);
    return next;
  }, []);

  const pickFirstEntry = useCallback((next: ChangesSnapshot, preferred?: string | null): ChangeEntry | null => {
    if (changelistModeRef.current) {
      const display = changelistEntriesForView(next, changelistRef.current);
      return display.find((entry) => entry.path === preferred) ?? display.find((entry) => entry.hunkCount > 0) ?? display[0] ?? null;
    }
    const order = ["unstaged", "staged", "untracked"] as const;
    if (preferred) {
      const match = next.entries.find((entry) => entry.path === preferred);
      if (match) return match;
    }
    for (const changes of order) {
      const entry = next.entries.find((candidate) => candidate.changes === changes && candidate.hunkCount > 0);
      if (entry) return entry;
    }
    return next.entries[0] ?? null;
  }, []);

  const bindDiffTarget = useCallback((entry: ChangeEntry, id: string, targetView: ViewMode) => {
    const rowCount = targetView === "split" ? entry.rowCountSplit : entry.rowCountUnified;
    rowStore.setTarget({ worktreeId: id, path: entry.path, changes: entry.changes, view: targetView,
      ignoreWhitespace: ignoreWhitespaceOf(diffOptionsRef.current.ignoreWhitespace),
      fingerprint: entry.fingerprint, snapshotRevision: entry.snapshotRevision }, rowCount, entry);
    highlightStore.setTarget({ worktreeId: id, path: entry.path, changes: entry.changes,
      fingerprint: entry.fingerprint, snapshotRevision: entry.snapshotRevision });
    return rowCount;
  }, [highlightStore, rowStore]);

  // 所有接收快照的入口都重绑版本；不能因路径、行数相同而沿用旧行和旧 hunk 凭据。
  // 忽略空白档位变化时也要重绑：统一视图按同一档位重新取行（Q140）。
  useLayoutEffect(() => {
    if (selectedEntry && worktreeId && snapshot?.worktreeId === worktreeId) {
      bindDiffTarget(selectedEntry, worktreeId, view);
    }
  }, [bindDiffTarget, selectedEntry, snapshot?.worktreeId, view, worktreeId, diffOptions.ignoreWhitespace]);

  /**
   * `scheduleReviewRecovery` 的转发：`openEntry` 定义在它之前，
   * 用 ref 避免定义顺序上的循环依赖（调用时一定已赋值）。
   */
  const scheduleRecoveryRef = useRef<((worktreeId: string) => Promise<ReviewRecoveryBatchOutcome>) | null>(null);
  const requestReviewRecovery = useCallback(
    (worktreeId: string) => scheduleRecoveryRef.current?.(worktreeId) ?? Promise.resolve(null),
    [],
  );

  const openEntry = useCallback(
    async (
      requested: ChangeEntry,
      target: { view: ViewMode; worktreeId: string; expand?: boolean; pin?: boolean; select?: boolean; activate?: boolean },
    ) => {
      // 本次选择独占一个上下文：期间又选了别的文件或切走，本次结果一律作废（Q88）。
      selectionEpochRef.current += 1;
      enterDiff();
      if (target.activate !== false) setMainView("diff");
      const gateToken = gateRef.current.token();
      const selectionContext = selectionGateRef.current.begin(
        target.worktreeId, requested.path, requested.changes, gateToken.activation,
      );
      const superseded = () =>
        !selectionGateRef.current.isValid(selectionContext, gateRef.current.token().activation)
        || worktreeIdRef.current !== target.worktreeId;
      let entry = requested;
      // 未跟踪文件的内容与行数在后端首次取行时才统计；先取一次空范围补全元数据，
      // 否则行数为 0 会被当成空文件，永远不挂载 diff 视图（任务 02 桌面验收）。
      if (entry.changes === "untracked" && !entry.metricsLoaded && !entry.readonlyReason) {
        const slice = await ipc.getRows({ worktreeId: target.worktreeId, path: entry.path, changes: entry.changes,
          view: target.view, from: 0, to: 0 });
        // 迟到的未跟踪元数据不得把选择拉回旧文件（含同 worktree U→V 与 A→B→A）。
        if (superseded()) {
          return { covered: false, rafPainted: false, viewportHeight: null, devicePixelRatio: window.devicePixelRatio };
        }
        const loaded = slice.entry;
        if (loaded.path !== entry.path || loaded.changes !== entry.changes
          || loaded.fingerprint !== entry.fingerprint || loaded.snapshotRevision !== entry.snapshotRevision) {
          throw localizedError("rejected", "changes.fileChanged");
        }
        // 不能用旧 entry **整条**替换：那会连带把同版本已成功的审阅状态勾回去，
        // 而进度仍保留，出现两处状态分裂（SP07-R7-3）。只合并审阅字段并重算进度。
        const overlay = reviewOverlayRef.current;
        // getRows 也是读取：用**后端这次实际读到**的审阅水位（`slice.reviewSeq`），
        // 不能用界面 revision 或快照 seq 去猜（ST07-R12-2）。
        const loadedSeq = slice.reviewSeq;
        const candidate = snapshotRef.current?.entries.find((item) =>
          item.path === loaded.path && item.changes === loaded.changes
          && item.snapshotRevision === loaded.snapshotRevision);
        const currentSeq = snapshotRef.current?.review.seq ?? 0;
        const mergedEntry = overlay.mergeEntry(target.worktreeId, loaded, loadedSeq, candidate, currentSeq);
        if (!mergedEntry.ok) {
          // 逐路径读取不可信：明确拒收并按**原目标**重新核对（ST07-R15-1）。
          debugLog(`元数据读取不可信，重新核对：${mergedEntry.worktreeId}@${mergedEntry.observedSeq}`);
          await requestReviewRecovery(mergedEntry.worktreeId);
          return { covered: false, rafPainted: false, viewportHeight: null, devicePixelRatio: window.devicePixelRatio };
        }
        entry = mergedEntry.entry;
        setSnapshot((current) => {
          if (!current) return current;
          // 与发布路径一致：只在 `current` 上改这条匹配条目的审阅字段。
          const inner = overlay.mergeEntry(
            target.worktreeId,
            loaded,
            loadedSeq,
            current.entries.find((item) =>
              item.path === loaded.path && item.changes === loaded.changes
              && item.snapshotRevision === loaded.snapshotRevision),
            Math.max(current.review.seq ?? 0, currentSeq),
          );
          if (!inner.ok) return current;
          return overlay.syncProgress({
            ...current,
            entries: current.entries.map((item) =>
              item.path === loaded.path && item.changes === loaded.changes
                && item.snapshotRevision === loaded.snapshotRevision ? inner.entry : item),
          });
        });
      }
      if (superseded()) {
        return { covered: false, rafPainted: false, viewportHeight: null, devicePixelRatio: window.devicePixelRatio };
      }
      const selection: Selection = { path: entry.path, changes: entry.changes };
      if (target.expand && entry.collapsed) {
        // 测量等同于用户点开：默认收起的文件也要真正渲染，才能测到首屏。
        setExpanded((current) => new Set(current).add(`${entry.changes}:${entry.path}`));
      }
      const rowCount = bindDiffTarget(entry, target.worktreeId, target.view);
      setSelected(selection);
      recordDiffTab(entry, target.pin === true);
      followActiveEntry(entry, target.select === true);
      // 默认日志与后台刷新只重绑工作区数据，不等待未挂载的 diff 首屏。
      if (target.activate === false && mainViewRef.current !== "diff") {
        return { covered: false, rafPainted: false, viewportHeight: null, devicePixelRatio: window.devicePixelRatio };
      }
      const finishCoverageMeasurement = beginSynchronousMeasurement("open.entry.coverage.wait");
      const covered = await waitUntil(
        () => {
          // 并排视图的行来自 side_lines（返修 A1），不用 RowWindowStore。
          if (target.view === "split") {
            const handle = sbsRef.current;
            if (!handle || handle.getViewportHeight() <= 0) return false;
            return (["left", "right"] as const).every((side) => {
              const visible = handle.getVisibleRange(side);
              return handle.isCovered(visible.from, visible.to, side);
            });
          }
          return rowStore.isCovered(0, Math.min(rowCount, DEFAULT_VIEWPORT_ROWS));
        },
        4000,
      );
      finishCoverageMeasurement();
      if (!covered) {
        debugLog(`首屏取行超时：${entry.path}（已加载块 ${rowStore.getSnapshot().loadedChunks}）`);
      }
      const finishPaintMeasurement = beginSynchronousMeasurement("open.entry.paint.wait");
      const paint = await nextPaint();
      finishPaintMeasurement();
      flushSynchronousMeasurements();
      const element = document.querySelector<HTMLElement>(".diff-scroll");
      return {
        covered,
        rafPainted: paint.rafPainted,
        viewportHeight: element?.clientHeight ?? null,
        devicePixelRatio: window.devicePixelRatio,
      };
    },
    [bindDiffTarget, enterDiff, followActiveEntry, recordDiffTab, rowStore],
  );
  entryNavigationRef.current = (entry) => {
    if (worktreeIdRef.current) {
      void openEntry(entry, { view: viewRef.current, worktreeId: worktreeIdRef.current });
    }
  };

  const applyRefreshedSnapshot = useCallback((next: ChangesSnapshot) => {
    const merging = activeMergeEntry(mergeTabTargetRef.current, next);
    if (merging) {
      // 合并编辑器保留原布局、指纹和草稿；刷新只更新列表，不重新打开文件。
      applySnapshot(next);
      setSelected({ path: merging.path, changes: merging.changes });
      return;
    }
    const current = selectedRef.current;
    const previous = snapshotRef.current;
    const display = changelistModeRef.current ? entryOrder(next) : getDisplayOrderEntries(next);
    const oldIndex = current && previous ? (changelistModeRef.current ? entryOrder(previous) : getDisplayOrderEntries(previous)).findIndex(
      (entry) => entry.path === current.path && entry.changes === current.changes,
    ) : 0;
    const entry = current
      ? display.find((entry) => entry.path === current.path && entry.changes === current.changes)
        ?? display.find((entry) => entry.path === current.path)
        ?? display[Math.min(Math.max(0, oldIndex), display.length - 1)]
      : pickFirstEntry(next);
    applySnapshot(next);
    if (entry) {
      void openEntry(entry, { view: viewRef.current, worktreeId: next.worktreeId, activate: false })
        .catch((caught) => setAppError(toAppError(caught), next.worktreeId));
    } else {
      enterDiff();
      clearEditorSelection();
    }
  }, [entryOrder, applySnapshot, clearEditorSelection, enterDiff, mergeTabTargetRef, openEntry, pickFirstEntry, setAppError]);

  const openWorktree = useCallback(
    async (id: string, options: { targetPath?: string | null; view?: ViewMode; expand?: boolean; activateDiff?: boolean } = {}) => {
      const targetView = options.view ?? view;
      setBusy(true);
      const openStart = performance.now();
      if (id !== worktreeIdRef.current) resetMergeTabs();
      // 切换请求发起时立即改激活目标：此后旧 worktree 的响应一律丢弃（含 A→B→A）。
      gateRef.current.activate(id);
      setM6State(null);
      setHistoryAction(null);
      setExternalTool(null);
      setCheckoutBlocked(null);
      setPushDialog(null);
      setPushDialogError(null);
      setPushRejected(null);
      setNewBranchStart(null);
      const myToken = gateRef.current.token();
      try {
        const sessionInfo = await ipc.openWorktree(id);
        if (gateRef.current.token().activation !== myToken.activation) return null;
        debugLog(`open_worktree 完成：${Math.round(performance.now() - openStart)} ms`);
        setInfo(sessionInfo);
        setWorktreeId(id);
        const failed = sessionInfo.checks.filter((check) => !check.ok);
        if (failed.length > 0) {
          const errors = failed.flatMap((check) => check.hint ? [check.detail, check.hint] : [check.detail]);
          setAppError({ ...errors[0], related: errors.slice(1) }, id);
        }

        // 监视只跟随当前 worktree；切换即释放旧监视（Q88）。
        // 把发起时的激活代号带给后端：迟到进入的旧请求会被拒绝，不会装成最新监视。
        const watch = await ipc.watchWorktree(id, myToken.activation).catch(() => null);
        if (gateRef.current.token().activation === myToken.activation && watch
          && watch.target === infoRef.current?.worktreePath) {
          setWatchState({ active: watch.active, error: watch.error });
        }

        const startedAt = performance.now();
        // 重开仓库（再次添加当前仓库会走到这里）同样要按请求前分界合并，
        // 不能把 rawSnapshot 直接发布而回退期间的审阅变更（SP07-R7-4）。
        const openRead = await readThroughReview(reviewOverlayRef.current, () => ipc.listChanges(id));
        if (!openRead.ok) {
          // 读取不可信（叠加层容量淘汰后信息不足）：必须按**原目标**重新核对，
          // 不能拿旧快照冒充成功（ST07-R15-1）。
          debugLog(`重开读取不可信，重新核对：${openRead.worktreeId}`);
          await scheduleReviewRecovery(openRead.worktreeId);
          return null;
        }
        const rawSnapshot = openRead.snapshot;
        if (!gateRef.current.accept(myToken, { worktreeId: rawSnapshot.worktreeId, revision: rawSnapshot.revision })) {
          debugLog(`丢弃非当前激活或过期的快照：${rawSnapshot.worktreeId}@${rawSnapshot.revision}`);
          return null;
        }
        const next = applySnapshot(rawSnapshot);
        const listChangesMs = performance.now() - startedAt;
        debugLog(`list_changes 完成：${Math.round(listChangesMs)} ms，${next.entries.length} 个文件`);
        const entry = pickFirstEntry(next, options.targetPath ?? null);
        if (!entry) {
          setSelected(null);
          setStatus(messages.changes.empty);
          return null;
        }
        setStatus(messages.changes.fileCount(next.entries.length));
        const firstRowsStart = performance.now();
        debugLog(`选中文件：${entry.path}（${entry.changes}）`);
        const opened = await openEntry(entry, {
          view: targetView,
          worktreeId: id,
          expand: options.expand,
          select: true,
          activate: options.activateDiff === true,
        });
        debugLog(`首屏取行完成：covered=${opened.covered}`);
        const firstScreenMs = performance.now() - startedAt;
        return {
          listChangesMs,
          firstRowsMs: performance.now() - firstRowsStart,
          firstScreenMs,
          openMs: performance.now() - openStart,
          rowCount: targetView === "split" ? entry.rowCountSplit : entry.rowCountUnified,
          fileCount: next.entries.length,
          entry,
          rafPainted: opened.rafPainted,
          viewportHeight: opened.viewportHeight,
          devicePixelRatio: opened.devicePixelRatio,
        };
      } catch (caught) {
        // 旧请求的失败同样只归属它自己的 worktree。
        if (gateRef.current.token().activation === myToken.activation) {
          setAppError(toAppError(caught), id);
        }
        return null;
      } finally {
        // 旧请求结束不能清掉新请求的忙碌状态。
        if (gateRef.current.token().activation === myToken.activation) setBusy(false);
      }
    },
    [applySnapshot, openEntry, pickFirstEntry, resetMergeTabs, setAppError, view],
  );

  /**
   * 统一的快照接收入口：先过接收门（激活 + 版本），再更新界面（Q88、Q90）。
   * 返回是否被接受，调用方据此决定后续动作。
   */
  const acceptSnapshot = useCallback(
    (
      token: { activation: number; worktreeId: string | null },
      snap: ChangesSnapshot,
    ): boolean => {
      if (!gateRef.current.accept(token, { worktreeId: snap.worktreeId, revision: snap.revision })) {
        debugLog(`丢弃过期快照：${snap.worktreeId}@${snap.revision}（当前激活 ${gateRef.current.current().worktreeId}）`);
        return false;
      }
      applyRefreshedSnapshot(snap);
      // 监视、手动刷新、写后重读与恢复均在门禁接受后通知引用消费者。
      setWatchRevision((value) => value + 1);
      setStatus(snap.entries.length > 0 ? messages.changes.fileCount(snap.entries.length) : messages.changes.empty);
      return true;
    },
    [applyRefreshedSnapshot],
  );

  /**
   * 不可信读取的**重新核对**调度（ST07-R15-2 / SP07-R15-1）。
   *
   * 叠加层在容量边界淘汰过更新记录时，仅凭现有信息无法安全合成；必须真实重读。
   * 约束：
   * - 按 **worktree** 单飞：同一个 worktree 同时只有一次重读，避免请求风暴；
   * - 有界：连续重读若水位不前进就停止（不无界重试）；
   * - 归属不变：重读只针对传入的 `worktreeId`，不借用当前界面目标。
   */
  /** 不可信读取的批次式重新核对（单飞、按批有限、不递归等待自身）。 */
  const reviewRecoverySchedulerRef = useRef(new ReviewRecoveryScheduler(REVIEW_RECOVERY_MAX_ATTEMPTS));
  /**
   * 单次恢复读取：**只做一次**真实 listChanges 并报告可信度/失败。
   *
   * 它绝不回调调度器（那会形成 task→runner→schedule→task 自等待），
   * 也不重跑任何写命令；后续要不要再试由外层批次决定（ST07-R16-1）。
   * 归属固定为传入的 `worktreeId`（原目标），只有它仍是激活目标时才更新界面。
   */
  const recoveryReadOnceRef = useRef<((worktreeId: string) => Promise<ReviewReadAttempt>) | null>(null);

  /**
   * 对一个 worktree 跑一次恢复批次（生产编排入口，App 与 Node 回归共用）。
   *
   * 展示资格按**批次**取一次：重试与耗尽出口用同一份判定，
   * 批次期间出现的新错误或被用户关闭的提示都不会被覆盖（ST07-R17-2）。
   * 所有分支都会 settle，写结果的 `finally` 一定释放门禁。
   */
  const scheduleReviewRecovery = useCallback(async (worktreeId: string): Promise<ReviewRecoveryBatchOutcome> => {
    const runner = recoveryReadOnceRef.current;
    if (!runner) {
      return { converged: false, attempts: 0, exhausted: true, lastFailure: null, failureShown: false };
    }
    const batchEpoch = worktreeErrorEpochRef.current[worktreeId] ?? 0;
    const outcome = await runReviewRecoveryBatch({
      scheduler: reviewRecoverySchedulerRef.current,
      worktreeId,
      read: () => runner(worktreeId),
      // 批次级资格：只认「批次开始时的那一代错误」，之后任何写入/关闭都让它失效。
      beginEligibility: () => () => (worktreeErrorEpochRef.current[worktreeId] ?? 0) === batchEpoch,
      showFailure: (failure) => {
        debugLog(`审阅重新核对未收敛：${worktreeId}`);
        setAppError(
          failure && failure.status === "error"
            ? toAppError(failure.error)
            : localizedError("rejected", "review.recoveryExhausted"),
          worktreeId,
        );
      },
    });
    return outcome;
  }, [setAppError]);

  /** 读取并接收一次快照；自动刷新、手动刷新与回到前台共用同一条路径。 */
  const readAndAccept = useCallback(
    async (targetWorktreeId: string): Promise<boolean> => {
      const token = gateRef.current.token();
      // 共享生产入口：先取分界再发请求，交付时合并（ST07-R7-1）。
      const read = await readThroughReview(reviewOverlayRef.current, () => ipc.listChanges(targetWorktreeId));
      if (!read.ok) {
        // 不可信读取：明确拒收并按**原目标**重新核对，不返回旧快照。
        debugLog(`读取不可信，重新核对：${read.worktreeId}@${read.observedSeq}`);
        await scheduleReviewRecovery(read.worktreeId);
        return false;
      }
      return acceptSnapshot(token, read.snapshot);
    },
    [acceptSnapshot],
  );

  const refresh = useCallback(async () => {
    if (!worktreeId) return;
    const targetWorktreeId = worktreeId;
    gateRef.current.activate(targetWorktreeId);
    setM6State(null);
    setHistoryAction(null);
    setExternalTool(null);
    const token = gateRef.current.token();
    setBusy(true);
    try {
      await readAndAccept(targetWorktreeId);
    } catch (caught) {
      // 旧请求的错误只归属它自己的 worktree。
      if (gateRef.current.token().activation === token.activation) {
        setAppError(toAppError(caught), targetWorktreeId);
      }
    } finally {
      if (gateRef.current.token().activation === token.activation) setBusy(false);
    }
  }, [readAndAccept, setAppError, worktreeId]);

  /**
   * 对**指定** worktree 做一次真实重读（Q90、Q91）。
   *
   * 写操作的结果归属原目标：即使已经切走，也要对原目标真实读一次，
   * 不能因为当前选择变了就跳过核对；但只有它仍是激活目标时才更新界面。
   */
  const refreshTarget = useCallback(async (targetWorktreeId: string): Promise<boolean> => {
    const token = gateRef.current.token();
    try {
      const read = await readThroughReview(reviewOverlayRef.current, () => ipc.listChanges(targetWorktreeId));
      if (!read.ok) {
        // 原目标归属不变：按 read.worktreeId 重新核对（不是当前界面目标）。
        debugLog(`原目标读取不可信，重新核对：${read.worktreeId}@${read.observedSeq}`);
        await scheduleReviewRecovery(read.worktreeId);
        return false;
      }
      if (token.activation === gateRef.current.token().activation
        && gateRef.current.current().worktreeId === targetWorktreeId) {
        acceptSnapshot(token, read.snapshot);
      }
      return true;
    } catch (caught) {
      if (token.activation === gateRef.current.token().activation
        && gateRef.current.current().worktreeId === targetWorktreeId) {
        setAppError(toAppError(caught), targetWorktreeId);
      }
      return false;
    }
  }, [acceptSnapshot, scheduleReviewRecovery, setAppError]);

  /**
   * 单次恢复读取（ST07-R16-1）：只做一次真实 listChanges，报告可信度或失败。
   *
   * 关键两点：
   * - 它**不调用** `scheduleReviewRecovery`。runner 若递归等待调度器，
   *   就会形成 task→runner→schedule→task 的自等待环，任务永不 settle。
   * - 它**不展示错误**。展示统一由批次出口按**批次级资格**决定，
   *   避免重试各自重取代际而获得覆盖新错误的资格（ST07-R17-2）。
   */
  const recoveryReadOnce = useCallback(async (worktreeId: string): Promise<ReviewReadAttempt> => {
    const token = gateRef.current.token();
    try {
      const read = await readThroughReview(reviewOverlayRef.current, () => ipc.listChanges(worktreeId));
      if (!read.ok) return { status: "untrusted", worktreeId: read.worktreeId, observedSeq: read.observedSeq };
      if (token.activation === gateRef.current.token().activation
        && gateRef.current.current().worktreeId === worktreeId) {
        acceptSnapshot(token, read.snapshot);
      }
      return { status: "trusted" };
    } catch (error) {
      return { status: "error", error };
    }
  }, [acceptSnapshot]);

  recoveryReadOnceRef.current = recoveryReadOnce;
  // 真实转发：getRows 等早于 scheduleReviewRecovery 定义的调用点经这里到达同一编排入口。
  scheduleRecoveryRef.current = scheduleReviewRecovery;

  // 自动刷新协调器：事件合并 + 单飞，执行期间的变化只补一轮（Q88）。
  const refreshCoordinator = useMemo(
    () => new RefreshCoordinator(async () => {
      const active = gateRef.current.current().worktreeId;
      if (!active) return;
      try {
        await readAndAccept(active);
      } catch (caught) {
        if (gateRef.current.current().worktreeId === active) {
          setAppError(toAppError(caught), active);
        }
      }
    }, () => {
      const active = gateRef.current.current().worktreeId;
      return !active || !writingWorktreesRef.current.has(active);
    }),
    [readAndAccept, setAppError],
  );
  const refreshCoordinatorRef = useRef(refreshCoordinator);
  refreshCoordinatorRef.current = refreshCoordinator;

  useEffect(() => () => refreshCoordinator.dispose(), [refreshCoordinator]);

  // 监视状态与变化通知：轮询后端，失败持续展示（Q95）。
  // 只接受属于当前激活目标的状态与变化，迟到的状态响应不会画到别的目标上。
  useEffect(() => {
    if (!worktreeId) return;
    let cancelled = false;
    const notifications = new WatchEventGate();
    const poll = async () => {
      const token = gateRef.current.token();
      try {
        const snapshot = await ipc.watchStatus();
        if (cancelled) return;
        // 目标或激活代号已变：这一份状态属于旧目标，丢弃。
        if (gateRef.current.token().activation !== token.activation) return;
        if (snapshot.status.activation !== token.activation) return;
        const expectedTarget = infoRef.current?.worktreePath ?? null;
        if (expectedTarget && snapshot.status.target !== expectedTarget) return;
        setWatchState({ active: snapshot.status.active, error: snapshot.status.error });
        if (notifications.consume(token.activation, snapshot)) {
          refreshCoordinator.request();
        }
      } catch {
        // 状态查询失败不改变监视结论，等下一轮。
      }
    };
    void poll();
    // 轮询必须让开滚动取数：连续滚动期间暂停，滚动停下后再核对一次。
    // 这样既保留生产环境的自动刷新，也不会与行数据争用 IPC 造成未覆盖帧。
    let timer = 0;
    let lastInteraction = performance.now();
    const noteInteraction = () => {
      lastInteraction = performance.now();
    };
    window.addEventListener("scroll", noteInteraction, { capture: true, passive: true });
    window.addEventListener("wheel", noteInteraction, { passive: true });
    window.addEventListener("keydown", noteInteraction);
    const tick = () => {
      const idle = performance.now() - lastInteraction >= 250;
      if (idle && !document.hidden) void poll();
      timer = window.setTimeout(tick, idle ? 1000 : 300);
    };
    timer = window.setTimeout(tick, 1000);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      window.removeEventListener("scroll", noteInteraction, { capture: true });
      window.removeEventListener("wheel", noteInteraction);
      window.removeEventListener("keydown", noteInteraction);
    };
  }, [refreshCoordinator, worktreeId]);

  // 回到前台（窗口重新可见）必须重新核对真实状态（Q88）。
  useEffect(() => {
    if (!worktreeId) return;
    const onVisible = () => {
      if (document.visibilityState === "visible") refreshCoordinator.request();
    };
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("focus", onVisible);
    // WKWebView 不保证原生窗口重新激活会触发 DOM focus / visibilitychange。
    const stopNativeFocus = subscribeToForeground(
      (onFocus) => getCurrentWindow().onFocusChanged(({ payload }) => onFocus(payload)),
      () => refreshCoordinator.request(),
      (caught) => debugLog(`原生窗口焦点监听失败：${getErrorMessage(caught)}`),
    );
    return () => {
      stopNativeFocus();
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("focus", onVisible);
    };
  }, [refreshCoordinator, worktreeId]);

  /**
   * 关闭/退出请求（任务 08，Q96）。
   *
   * 三个来源统一保护：
   * - 窗口关闭（`onCloseRequested`，拦截后询问）；
   * - 原生退出（macOS Quit / ⌘Q）：Rust 在 `RunEvent::ExitRequested` 里读
   *   `exit_blocked`，为真则阻止退出并 emit `revia://exit-requested`，这里接住；
   * - 前端监听安装失败**不能静默**：写入全局错误，让用户知道保护可能缺失。
   */
  useEffect(() => {
    let disposed = false;
    const stops: Array<() => void> = [];
    const fail = (what: "window" | "exit") => (caught: unknown) => {
      if (disposed) return;
      // 保护安装失败：标记为不可用，写入口会据此拒绝开始写入（ST-B03-08-2）。
      const failure = closeGuardFailure(what, caught);
      closeGuardsRef.current.markFailed(failure);
      setAppError(failure, null);
    };
    // 每次（重）装都从未就绪开始：安装中不允许写入。
    closeGuardsRef.current = new CloseGuardGate<AppErrorPayload>();
    void (async () => {
      let windowGuard = false;
      let exitGuard = false;
      try {
        const stop = await getCurrentWindow().onCloseRequested((event) => {
          // 只接收用户意图；绝不把匿名事件当作程序化关闭的回执。
          event.preventDefault();
          const driver = closeDriverRef.current;
          if (!driver || disposed) return;
          void lifecycleRequestRef.current("close");
        });
        if (disposed) stop(); else stops.push(stop);
        windowGuard = true;
      } catch (caught) {
        fail("window")(caught);
      }
      try {
        // 原生退出：Rust 已阻止退出并通知前端，这里按同一套编排询问用户。
        const stop = await listen("revia://exit-requested", () => {
          if (disposed) return;
          const driver = closeDriverRef.current;
          if (!driver) return;
          void lifecycleRequestRef.current("exit");
        });
        if (disposed) stop(); else stops.push(stop);
        exitGuard = true;
      } catch (caught) {
        fail("exit")(caught);
      }
      // 两个监听都装好才算就绪：缺一个都可能让关闭/退出绕过保护。
      if (!disposed && windowGuard && exitGuard) {
        closeGuardsRef.current.markInstalled("window");
        closeGuardsRef.current.markInstalled("exit");
      }
    })();
    return () => {
      disposed = true;
      closeGuardsRef.current.dispose();
      for (const stop of stops) stop();
    };
    // 依赖数组为空：这是**应用级**生命周期，不随 worktree 变化卸载重装。
    // 所有可变输入都经 ref 读取，因此切换 worktree 不会在在途写期间出现保护空档
    // （ST-B04-08-1 / SP-B04-08-1）。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** 执行关闭/退出的入口（供原生退出事件复用，避免定义顺序问题）。 */
  /** 驱动的转发：监听回调早于驱动定义注册。 */
  const applyEffectRef = useRef<((effect: CloseDriverEffect) => void) | null>(null);
  /** reconcile 的转发（供监听回调与关闭执行共用，避免定义顺序问题）。 */

  // ---------------- 添加仓库 ----------------

  const addRepo = useCallback(
    async (path: string) => {
      setBusy(true);
      try {
        const repo = await ipc.addRepo(path);
        setRepos((current) => {
          const others = current.filter((entry) => entry.id !== repo.id);
          return [...others, repo];
        });
        setActiveRepo(repo);
        const worktree = repo.worktrees.find((entry) => !entry.bare) ?? repo.worktrees[0];
        if (!worktree) {
          setAppError(localizedError("rejected", "firstRun.notARepo"));
          return;
        }
        await ipc.selectRepo(repo.id, worktree.id);
        await openWorktree(worktree.id);
      } catch (caught) {
        setAppError(toAppError(caught));
      } finally {
        setBusy(false);
      }
    },
    [openWorktree, setAppError],
  );

  const chooseRepo = useCallback(async () => {
    const picked = await openDialog({ directory: true, multiple: false, title: messages.firstRun.title });
    if (typeof picked === "string") await addRepo(picked);
  }, [addRepo]);

  // ---------------- 启动 ----------------

  useEffect(() => {
    void (async () => {
      try {
        const [settings, list, systemLanguage] = await Promise.all([
          ipc.listSettings(),
          ipc.listRepos(),
          ipc.systemLanguage().catch(() => ""),
        ]);
        if (systemLanguage) setSystemLanguage(systemLanguage);
        setFontSize(settings.diffFontSize);
        setThemeLight(settings.themeLight);
        setThemeDark(settings.themeDark);
        setCodeFontFamily(settings.codeFontFamily);
        setGitPath(settings.gitPath);
        setMergeTool(settings.mergeTool);
        setDiffTool(settings.diffTool);
        const preference = (settings.language ?? "auto") as LanguagePreference;
        setLanguage(preference);
        setLocale(resolveLocale(preference));
        setCherryPickKeepEmpty(settings.cherryPickKeepEmpty);
        setCherryPickIncludeSource(settings.cherryPickIncludeSource);
        setSidebarWidth(settings.sidebarWidth);
        setView(settings.viewMode);
        setUiZoom(settings.uiZoom);
        setAllowPrerelease(settings.allowPrerelease ?? false);
        setTreeGroupByDirectory(settings.treeGroupByDirectory);
        setDiffOptions({
          contextLines: settings.diffContextLines,
          foldUnchanged: settings.diffFoldUnchanged,
          syncScroll: settings.diffSyncScroll,
          highlight: settings.diffHighlight,
          showWhitespace: settings.diffShowWhitespace,
          softWrap: settings.diffSoftWrap,
          jumpToNextFile: settings.diffJumpToNextFile,
          ignoreWhitespace: settings.diffIgnoreWhitespace as 0 | 1 | 2 | 3,
        });
        changelistModeRef.current = settings.changelistMode;
        setChangelistMode(settings.changelistMode);
        setCommitChecks({
          subjectLimit: settings.commitSubjectLimit,
          blankLineRequired: settings.commitBlankLineRequired,
          bodyLineLimit: settings.commitBodyLineLimit,
          showRuler: settings.commitShowRuler,
          warnCrlf: settings.commitWarnCrlf,
          warnDetached: settings.commitWarnDetached,
          warnLargeFileBytes: Number(settings.commitWarnLargeFileBytes),
          warnInvalidFilename: settings.commitWarnInvalidFilename,
        });
        setNotifyKinds(settings.notify);
        setMergeAutoApply(settings.mergeAutoApply);
        setFileHistoryShowNames(settings.fileHistoryShowNames);
        setStashCompareLocal(settings.stashCompareLocal);
        setLogSettings({
          columns: settings.logColumns,
          longEdges: settings.logLongEdges,
          firstParent: settings.logFirstParent,
          previewPosition: settings.logPreviewPosition,
        });
        setPushSettings({
          updateMode: settings.updateMode,
          fetchTagMode: settings.fetchTagMode,
          fetchAutomatically: settings.fetchAutomatically,
          checkRemote: settings.checkRemote,
          autoUpdateOnPushRejection: settings.autoUpdateOnPushRejection,
          pushDialogOnCommit: settings.pushDialogOnCommit,
          pushDialogProtectedOnly: settings.pushDialogProtectedOnly,
          protectedBranches: settings.protectedBranches,
        });
        setShortcutsSnapshot(settings.shortcuts ?? {});
        applyShortcutOverrides(settings.shortcuts ?? {});
        setAppUpdateMode(settings.appUpdateMode ?? "notify");
        setRepos(list);
        await highlightStore.loadCategories();
        const config = await ipc.measureConfig();
        setLoggingEnabled(config.logging && !config.minimalSoakLogging);
        // 测量面板只在测量/冒烟/长测模式显示；普通启动（含普通 acceptance 实例）不显示。
        // 注意：这里只控制界面，不关闭任何测量/冒烟/长测的结果采集与 IPC 流程。
        setMeasurementMode(config.measurementMode);
        setPanelOpen(config.measurementMode);
        // 先让界面进入可交互状态，再跑自动测量：测量需要真正的 diff 视图存在。
        setBootstrapDone(true);
        if (config.soakSeconds && config.soakSeconds > 0) {
          autoRan.current = true;
          await runSoak(config);
          return;
        }
        if (config.smoke) {
          autoRan.current = true;
          await runSmoke(config);
          return;
        }
        if (config.autoRun) {
          autoRan.current = true;
          await runAutoMeasurement(config);
          return;
        }
        if (list.length > 0) {
          const repo = list[0];
          setActiveRepo(repo);
          const worktree =
            repo.worktrees.find((entry) => entry.id === settings.lastWorktreeId) ??
            repo.worktrees.find((entry) => !entry.bare) ??
            repo.worktrees[0];
          if (worktree) await openWorktree(worktree.id);
        }
      } catch (caught) {
        setAppError(toAppError(caught));
      } finally {
        setBootstrapDone(true);
      }
    })();
    // 只在首次挂载时执行
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 拖入文件夹即可添加仓库（Q35）。监听只装一次：addRepo 随视图切换重建，
  // 跟着重装会让每次切换都多出 4 次监听 / 取消的 IPC 与注入脚本。
  const addRepoRef = useRef(addRepo);
  addRepoRef.current = addRepo;
  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== "drop") return;
      const paths = event.payload.paths;
      if (paths.length > 0) void addRepoRef.current(paths[0]);
    });
    return () => {
      void unlisten.then((dispose) => dispose());
    };
  }, []);

  /** 切换当前 worktree：记录选择并重开，监视随之切换（Q88）。 */
  const switchWorktree = useCallback(
    async (id: string) => {
      if (!activeRepo || id === worktreeId) return;
      try {
        await ipc.selectRepo(activeRepo.id, id);
      } catch (caught) {
        setAppError(toAppError(caught), id);
        return;
      }
      selectionEpochRef.current += 1;
      selectionGateRef.current.clear();
      setSelected(null);
      rowStore.setTarget({ worktreeId: id, path: "", changes: "unstaged", view: viewRef.current,
        fingerprint: "", snapshotRevision: "" }, 0, null);
      await openWorktree(id);
    },
    [activeRepo, openWorktree, rowStore, setAppError, worktreeId],
  );

  // ---------------- 视图切换与跳转 ----------------

  const switchView = useCallback(
    async (next: ViewMode) => {
      if (next === view) return;
      if (!selectedEntry || !worktreeId || mainViewRef.current !== "diff") {
        pendingAnchor.current = null;
        setView(next);
        return;
      }
      // 记住视图选择：写入设置（返修 C12），失败仍完成本次切换。
      void ipc.updateSettings({ viewMode: next }).catch(() => {});
      const target: AnchorTarget = {
        worktreeId, entry: selectedEntry,
        selectionEpoch: selectionEpochRef.current,
        activation: gateRef.current.token().activation,
        optionsKey: JSON.stringify(diffOptionsRef.current),
      };
      // 快速切回时保留尚未恢复的原锚点，不从临时首屏重新取位置。
      const previous = pendingAnchor.current;
      const anchor = previous && sameAnchorTarget(previous, target) ? previous
        : (view === "split" ? sbsRef.current : diffRef.current)?.getFileAnchor();
      pendingAnchor.current = anchor ? { ...anchor, ...target, view: next } : null;
      setView(next);
      if (worktreeId) bindDiffTarget(selectedEntry, worktreeId, next);
    },
    [bindDiffTarget, selectedEntry, view, worktreeId],
  );

  // 视图切换后把锚点行放回顶部（等数据到位再定位）
  useEffect(() => {
    const anchor = pendingAnchor.current;
    if (!anchor || anchor.view !== view || !selectedEntry) return;
    let cancelled = false;
    const isCurrent = () => {
      const entry = snapshotRef.current?.entries.find((item) => item.path === anchor.entry.path
        && item.changes === anchor.entry.changes);
      return !!entry && !cancelled && pendingAnchor.current === anchor && viewRef.current === anchor.view
        && mainViewRef.current === "diff" && !mergeTabPathRef.current
        && selectedRef.current?.path === anchor.entry.path
        && selectedRef.current?.changes === anchor.entry.changes
        && sameAnchorTarget(anchor, {
          worktreeId: worktreeIdRef.current ?? "", entry,
          selectionEpoch: selectionEpochRef.current,
          activation: gateRef.current.token().activation,
          optionsKey: JSON.stringify(diffOptionsRef.current),
        });
    };
    void (async () => {
      try {
        if (!isCurrent()) return;
        if (view === "split") {
          await waitUntil(() => !isCurrent() || !!sbsRef.current?.restoreFileAnchor(anchor), 3000);
          return;
        }
        const index = await ipc.rowIndexForLine({
          worktreeId: anchor.worktreeId,
          path: anchor.entry.path,
          changes: anchor.entry.changes,
          view,
          line: anchor.line,
          version: anchor.version,
          ignoreWhitespace: ignoreWhitespaceOf(diffOptionsRef.current.ignoreWhitespace),
          expectedFingerprint: anchor.entry.fingerprint,
          expectedSnapshotRevision: anchor.entry.snapshotRevision,
        });
        if (!isCurrent()) return;
        const covered = await waitUntil(() => !isCurrent() || rowStore.ensure(index, index + 1, 0), 3000);
        if (isCurrent() && covered && rowStore.getRow(index)) diffRef.current?.scrollToRow(index);
      } catch (caught) {
        if (isCurrent()) debugLog(`恢复阅读位置失败：${getErrorMessage(caught)}`);
      } finally {
        if (!cancelled && pendingAnchor.current === anchor) pendingAnchor.current = null;
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [rowStore, selectedEntry, view, worktreeId, diffOptions, mergeTabPathRef, mainView]);

  /** focusNextFile 定义在后面；F7 走到最后一处时经这里切到下一个文件。 */
  const focusNextFileRef = useRef<(direction: 1 | -1) => void>(() => {});
  const jumpToHunk = useCallback(
    (direction: 1 | -1) => {
      if (!selectedEntry) return;
      // 设置“最后一处改动后跳到下一个文件”（E.4）：往后已经没有改动时打开下一个文件。
      const pastLast = () => {
        if (direction > 0 && diffOptionsRef.current.jumpToNextFile) focusNextFileRef.current(1);
      };
      // 并排视图按段跳转（返修 A1/Q138）：段就是色带连起来的那一块。
      if (view === "split") {
        const layout = sideLayout;
        const handle = sbsRef.current;
        if (!layout || !handle || layout.segments.length === 0) return;
        const top = handle.getScrollTop();
        const height = handle.getViewportHeight();
        const centerLine = (top + height / 2) / ROW_HEIGHT + 1;
        let index = -1;
        layout.segments.forEach((segment, position) => {
          if (segment.leftStart + 1 <= centerLine) index = position;
        });
        const target = Math.min(
          Math.max(0, index + direction),
          layout.segments.length - 1,
        );
        if (target !== index) handle.jumpToSegment(target);
        else pastLast();
        return;
      }
      const handle = diffRef.current;
      if (!handle || !navigationRows?.length) return;
      const previous = unifiedJumpRef.current;
      // 页尾不能滚到段首时仍记住刚跳过的块，避免 F7 永远停在同一块。
      const current = previous?.epoch === rowStore.epochValue && previous.scrollTop === handle.getScrollTop()
        ? previous.row
        : handle.getVisibleRange().from;
      const target = direction > 0
        ? navigationRows.find((row) => row > current)
        : [...navigationRows].reverse().find((row) => row < current);
      if (target != null) {
        handle.scrollToRow(target);
        unifiedJumpRef.current = { epoch: rowStore.epochValue, row: target, scrollTop: handle.getScrollTop() };
      } else pastLast();
    },
    [navigationRows, rowStore, selectedEntry, sideLayout, view],
  );

  const focusNextFile = useCallback(
    (direction: 1 | -1) => {
      if (!snapshot || !selected) return;
      const display = changelistModeRef.current ? entryOrder(snapshot) : snapshot.entries;
      const index = display.findIndex(
        (entry) => entry.path === selected.path && entry.changes === selected.changes,
      );
      const next = display[index + direction];
      if (next && worktreeId) void openEntry(next, { view, worktreeId });
    },
    [entryOrder, openEntry, selected, snapshot, view, worktreeId],
  );
  focusNextFileRef.current = focusNextFile;

  // ---------------- 暂存写操作（Q79/Q89/Q90/Q93） ----------------

  const [writingWorktrees, setWritingWorktrees] = useState<Set<string>>(new Set());
  const writingWorktreesRef = useRef(new Set<string>());
  useEffect(() => refreshCoordinator.resume(), [refreshCoordinator, writingWorktrees, worktreeId]);
  /**
   * 写入期间的安全关闭/退出编排（任务 08，Q90/Q91/Q96）。
   *
   * 只做决策：保留窗口与进程、等待完成后执行**原请求**、留在应用不取消写、
   * 待核对结果先展示。从不杀 Git、不重跑写命令。
   */
  const closeCoordinatorRef = useRef(new CloseCoordinator());
  /** 终端是否有运行中的会话（任务 04 / F13）：关闭协调与更新安装都要用它。 */
  // 运行态由App级TerminalSessions持有，隐藏面板不解除退出保护。
  /** 当前需要用户选择的关闭/退出请求（`null` 表示没有）。 */
  const [closeRequest, setCloseRequest] = useState<CloseDecision | null>(null);
  /** 当前询问对应的原动作（close / exit）：由 reconcile 从被保留的原请求写入。 */
  const closeIntentRef = useRef<CloseIntent>("close");
  /**
   * 关闭/退出保护监听的安装状态（任务 08、Q96）。
   *
   * 只有窗口关闭监听**和**原生退出监听都装好，才允许开始写入；
   * 安装中/失败/卸载时写入口会拒绝执行（ST-B03-08-2）。
   */
  const closeGuardsRef = useRef(new CloseGuardGate<AppErrorPayload>());
  /** reconcile 的转发（监听回调早于其定义注册）。 */
  const reconcileCloseRef = useRef<(() => void) | null>(null);
  const isWriting = worktreeId ? writingWorktrees.has(worktreeId) : false;
  /** 审阅进度提示（全部已审阅时留在原地提示）。 */
  /** 完成提示绑定 worktree 与当时的进度：切 worktree、取消标记或刷新失效后不再显示旧提示。 */
  const [reviewNotice, setReviewNotice] = useState<{ worktreeId: string; reviewedCount: number; total: number } | null>(null);
  /** 审阅请求代际：迟到的标记响应不能污染其他 worktree 或新文件版本（Q88、Q89）。 */
  const reviewEpochRef = useRef(0);

  /**
   * 真正执行关闭/退出（任务 08）。
   *
   * 只有关闭编排确认「没有在途写、没有待核对」后才会走到这里；
   * 动作类型完全按用户原请求，不升级、不替换。
   */




  /**
   * 把「是否需要阻止原生退出」同步给 Rust（任务 08）。
   *
   * 用**版本号**串行化：每次请求带上单调递增的版本，Rust 侧只接受更大的版本。
   * 这样「旧 false 的 invoke 晚到」不可能覆盖「新 true」——本地 Tauri 每个 invoke
   * 是独立 fetch，仅靠 JS 丢旧回包不足以保证 Rust 实际状态（ST-B03-08-1）。
   */
  /**
   * 退出保护同步器：版本化，旧 `false` 晚到也不会覆盖新 `true`（ST-B03-08-1）。
   */
  const exitGuardRef = useRef(new ExitGuardSync((request) => ipc.setExitBlocked(request)));
  const syncExitBlocked = useCallback(async (blocked: boolean): Promise<void> => {
    const accepted = await exitGuardRef.current.set(blocked);
    if (!accepted) {
      // 被更高版本取代：说明更新的状态已经生效，不算失败。
      debugLog(`退出保护更新被更新版本取代：blocked=${blocked}`);
    }
  }, []);

  /**
   * 关闭/退出的**共享生产驱动**适配（任务 08，ST-B07-08-1..3）。
   *
   * 状态机全部在 `CloseDriver` 里；App 只把它的 `CloseDriverEffect`
   * 映射成对话框与原生动作。Node 回归驱动同一个类。
   */
  /** 冲刷并等待草稿落盘的转发 ref（实现定义在提交工作流区；关闭编排先于它）。 */
  const flushCommitDraftRef = useRef<(targetWorktreeId?: string) => Promise<void>>(
    async () => undefined,
  );

  /** 先落草稿再执行不可撤销的原生动作；失败时抛出，绝不静默关闭。 */
  const closeAfterDraftFlush = useCallback(
    async (action: () => Promise<void>, releaseGuard = true): Promise<void> => {
      await flushCommitDraftRef.current();
      // 编辑草稿与终端页签先落盘，再执行不可撤销的原生动作（M7 任务 03、04）。
      await editSessions.flush();
      await flushM7State();
      if (mergeBufferGuards.hasWork) throw localizedError("rejected", "errors.update.mergeEditsPending");
      if (editSessions.hasWork) throw localizedError("rejected", "workspace.editUnsavedBlock");
      await terminalSessions.finishShutdown();
      if (releaseGuard) await syncExitBlocked(false);
      await action();
    },
    [editSessions, terminalSessions, syncExitBlocked],
  );

  const closeDriverRef = useRef<CloseDriver | null>(null);
  if (closeDriverRef.current === null) {
    closeDriverRef.current = new CloseDriver({
      coordinator: closeCoordinatorRef.current,
      releaseExitGuard: async () => {
        if (mergeBufferGuards.hasWork) throw localizedError("rejected", "errors.update.mergeEditsPending");
        if (editSessions.hasWork) throw localizedError("rejected", "workspace.editUnsavedBlock");
        await syncExitBlocked(true);
        // 真正解除原生保护留到最终草稿冲刷和已确认终端关闭之后。
      },
      // 关闭/退出前必须先把草稿落盘并等待完成：落盘失败时不得静默销毁窗口（F02.5）。
      closeWindow: () => closeAfterDraftFlush(() => getCurrentWindow().destroy()),
      exitApp: () => closeAfterDraftFlush(() => ipc.requestExit()),
      installUpdate: () => closeAfterDraftFlush(async () => {
        const status = await m7.updateInstall();
        updateLifecycleRef.current?.resolve(status); updateLifecycleRef.current = null;
      }, false),
      restartUpdate: () => closeAfterDraftFlush(async () => {
        await m7.updateRestart();
        updateLifecycleRef.current?.resolve(null); updateLifecycleRef.current = null;
      }),
      onError: (caught) => {
        updateLifecycleRef.current?.reject(caught); updateLifecycleRef.current = null;
        editSessions.setClosing(false); terminalSessions.cancelShutdown(); setAppError(toAppError(caught), null);
      },
      onAsk: (intent, writing, pending) => {
        closeIntentRef.current = intent;
        setCloseRequest(
          pending.length > 0 ? { action: "showPending", writing, pending } : { action: "ask", writing },
        );
      },
      onSettled: () => {
        setCloseRequest(null);
        if (closeDriverRef.current?.currentPhase() === "idle") {
          editSessions.setClosing(false); terminalSessions.cancelShutdown();
          updateLifecycleRef.current?.resolve(null); updateLifecycleRef.current = null;
        }
      },
    });
  }
  const closeDriver = closeDriverRef.current;

  /** 把驱动返回的效果落到 React 状态（驱动内部已调用 onAsk/onSettled）。 */
  const applyEffect = useCallback((effect: CloseDriverEffect) => {
    if (effect.kind === "none") return;
    if (effect.kind === "ask") {
      closeIntentRef.current = effect.intent;
      setCloseRequest(
        effect.pending.length > 0
          ? { action: "showPending", writing: effect.writing, pending: effect.pending }
          : { action: "ask", writing: effect.writing },
      );
      return;
    }
    setCloseRequest(null);
  }, []);

  /**
   * 生命周期状态变化后通知驱动（写结束、rollback、待核对清除等）。
   *
   * 驱动不会重复认领已有执行；最终 closing 阶段不会再开启写入或请求。
   */
  const reconcileClose = useCallback(() => {
    const coordinator = closeCoordinatorRef.current;
    if (closeDriver.currentPhase() !== "closing") void syncExitBlocked(coordinator.needsExitGuard() || editSessions.hasWork || terminalSessions.needsExitGuard || reviewDraftSessions.hasWork || mergeBufferGuards.hasWork).catch((caught) => {
      setAppError(toAppError(caught), null);
    });
    applyEffect(closeDriver.notifyStateChanged());
  }, [applyEffect, closeDriver, editSessions, terminalSessions, syncExitBlocked]);

  useEffect(() => {
    const sync = () => {
      if (closeDriver.currentPhase() === "closing") return;
      void syncExitBlocked(closeCoordinatorRef.current.needsExitGuard() || editSessions.hasWork || terminalSessions.needsExitGuard || reviewDraftSessions.hasWork || mergeBufferGuards.hasWork)
        .catch((caught) => setAppError(toAppError(caught), null));
    };
    sync();
    const stopEdit = editSessions.subscribe(sync), stopTerminal = terminalSessions.subscribe(sync), stopReview = reviewDraftSessions.subscribe(sync), stopMerge = mergeBufferGuards.subscribe(sync);
    return () => { stopEdit(); stopTerminal(); stopReview(); stopMerge(); };
  }, [closeDriver, editSessions, terminalSessions, setAppError, syncExitBlocked]);
  // 存在磁盘草稿但尚未打开编辑器时，原生Quit也必须进入同一决策入口。
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        await syncExitBlocked(true);
        await editSessions.discover(editWorktreesRef.current);
        if (!cancelled && closeDriver.currentPhase() !== "closing") await syncExitBlocked(closeCoordinatorRef.current.needsExitGuard() || editSessions.hasWork || terminalSessions.needsExitGuard || reviewDraftSessions.hasWork || mergeBufferGuards.hasWork);
      } catch (caught) { if (!cancelled) setAppError(toAppError(caught), null); }
    })();
    return () => { cancelled = true; };
  }, [repos, closeDriver, editSessions, terminalSessions, setAppError, syncExitBlocked]);
  useEffect(() => registerM7Flusher(() => editSessions.flush()), [editSessions]);
  useEffect(() => registerM7Flusher(() => terminalSessions.flush()), [terminalSessions]);
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      await syncExitBlocked(true);
      await terminalSessions.initialize();
      if (!cancelled && closeDriver.currentPhase() !== "closing") await syncExitBlocked(closeCoordinatorRef.current.needsExitGuard() || editSessions.hasWork || terminalSessions.needsExitGuard || reviewDraftSessions.hasWork || mergeBufferGuards.hasWork);
    })().catch((caught) => { if (!cancelled) setAppError(toAppError(caught), null); });
    return () => { cancelled = true; };
  }, [closeDriver, terminalSessions, editSessions, setAppError, syncExitBlocked]);


  lifecycleRequestRef.current = async (intent) => {
    if (mergeBufferGuards.hasWork) {
      const failure = localizedError("rejected", "errors.update.mergeEditsPending");
      updateLifecycleRef.current?.reject(failure); updateLifecycleRef.current = null;
      setAppError(failure, null); return;
    }
    if (lifecyclePreparingRef.current || closeDriver.currentPhase() === "closing") return;
    if (closeDriver.currentPhase() !== "idle") { applyEffect(closeDriver.handleRequest(intent)); return; }
    lifecyclePreparingRef.current = true;
    try {
      const edits = await editSessions.resolve(editWorktreesRef.current, null, askEdit);
      if (!edits.ok) return;
      editSessions.setClosing(true);
      if (!(await terminalSessions.prepareShutdown(askTerminalShutdown))) { editSessions.setClosing(false); return; }
      applyEffect(closeDriver.handleRequest(intent));
    } catch (caught) { editSessions.setClosing(false); terminalSessions.cancelShutdown(); setAppError(toAppError(caught), null); }
    finally {
      lifecyclePreparingRef.current = false;
      if (closeDriver.currentPhase() === "idle") { updateLifecycleRef.current?.resolve(null); updateLifecycleRef.current = null; }
    }
  };

  const startUpdateLifecycle = useCallback((intent: "installUpdate" | "restartUpdate"): Promise<AppUpdateStatus | null> => {
    if (updateLifecycleRef.current || lifecyclePreparingRef.current || closeDriver.currentPhase() !== "idle") {
      return Promise.reject(localizedError("rejected", "errors.m7.updateBusy"));
    }
    return new Promise((resolve, reject) => {
      updateLifecycleRef.current = { intent, resolve, reject };
      void lifecycleRequestRef.current(intent);
    });
  }, [closeDriver]);

  const requestUpdateLifecycle = useCallback((intent: "installUpdate" | "restartUpdate"): Promise<AppUpdateStatus | null> => {
    const decision = updateInstallConfirmationRef.current.request(intent);
    if (decision.kind === "confirm") {
      setUpdateInstallConfirmOpen(true);
      return Promise.resolve(null);
    }
    return startUpdateLifecycle(decision.intent);
  }, [startUpdateLifecycle]);

  const confirmUpdateInstallAndRestart = useCallback(() => {
    setUpdateInstallConfirmOpen(false);
    const intent = updateInstallConfirmationRef.current.confirm();
    if (intent) {
      void startUpdateLifecycle(intent).catch((caught) => setAppError(toAppError(caught), null));
    }
  }, [startUpdateLifecycle, setAppError]);

  const cancelUpdateInstallAndRestart = useCallback(() => {
    updateInstallConfirmationRef.current.cancel();
    setUpdateInstallConfirmOpen(false);
  }, []);

  // 监听回调通过 ref 到达同一驱动。
  reconcileCloseRef.current = reconcileClose;
  applyEffectRef.current = applyEffect;

  /**
   * 待核对结果闭环（任务 08）：逐个真实重读，全部核对成功后才继续关闭/退出。
   * 核对失败或出现新的待核对时不关闭，按真实集合重新展示。
   */
  const confirmPendingReviewed = useCallback(async () => {
    const coordinator = closeCoordinatorRef.current;
    const requestGen = coordinator.generation();
    const targets = coordinator
      .pendingWorktrees()
      .map((worktreeId) => ({ worktreeId, generation: coordinator.pendingGeneration(worktreeId) }));
    let allConfirmed = true;
    for (const { worktreeId, generation } of targets) {
      if (coordinator.generation() !== requestGen) return;
      if (!worktreeId.startsWith("save-as:")) {
        let verified = false;
        try {
          verified = await refreshTarget(worktreeId);
        } catch {
          verified = false;
        }
        if (!verified) {
          allConfirmed = false;
          continue;
        }
      }
      if (!coordinator.acknowledgePending(worktreeId, generation)) allConfirmed = false;
    }
    if (coordinator.generation() !== requestGen) return;
    if (!allConfirmed) {
      reconcileClose();
      return;
    }
    applyEffect(closeDriver.handlePendingContinue());
  }, [applyEffect, closeDriver, reconcileClose, refreshTarget]);

  /**
   * 三个写入口共用的**共享异步编排**（任务 08）。
   *
   * 依赖全部经 ref/回调读取，所以实例稳定；App 与 Node 回归用同一个类。
   */
  const writeOrchestratorRef = useRef<WriteGuardOrchestrator<AppErrorPayload> | null>(null);
  if (writeOrchestratorRef.current === null) {
    writeOrchestratorRef.current = new WriteGuardOrchestrator<AppErrorPayload>({
      closeDriver,
      closeBlockedReason: () => localizedError("internal", "close.closing"),
      guardsReady: () => closeGuardsRef.current.ready,
      guardBlockedReason: () => closeGuardsRef.current.blockedReason ?? localizedError("internal", "errors.closeGuard.installing"),
      register: (worktreeId) => {
        closeCoordinatorRef.current.beginWrite(worktreeId);
      },
      guard: async () => {
        await syncExitBlocked(true);
        await refreshCoordinatorRef.current.whenIdle();
      },
      rollback: (worktreeId) => {
        closeCoordinatorRef.current.rollbackWrite(worktreeId);
      },
      reconcile: () => reconcileCloseRef.current?.(),
      endWrite: (worktreeId, needsVerification) => {
        closeCoordinatorRef.current.endWrite(worktreeId, needsVerification);
      },
    });
  }
  const writeOrchestrator = writeOrchestratorRef.current;

  editBlockedRef.current = (id) => writingWorktreesRef.current.has(id) || closeDriver.currentPhase() === "closing";
  terminalCallbacksRef.current.report = (id, error) => setAppError(error, id);
  terminalCallbacksRef.current.blocked = (id) => writingWorktreesRef.current.has(id) || closeDriver.currentPhase() === "closing";
  terminalCallbacksRef.current.activity = (id) => {
    const timers = terminalActivityTimersRef.current;
    const previous = timers.get(id); if (previous !== undefined) clearTimeout(previous);
    timers.set(id, setTimeout(() => { timers.delete(id); void refreshTarget(id); }, 900));
  };
  editWriteRef.current = (request) => runEditWrite<OperationContext>({
    writing: writingWorktreesRef.current,
    publishWriting: () => setWritingWorktrees(new Set(writingWorktreesRef.current)),
    begin: (id) => writeOrchestrator.begin(id),
    end: (id, needsVerification) => writeOrchestrator.end(id, needsVerification),
    beginOperation: (id) => operationGateRef.current.begin(id),
    canRecord: (token) => operationGateRef.current.canRecord(token),
    execute: (write) => m7.saveEditable(write),
    refreshTarget,
    reportFailure: (id, error) => setAppError(error, id),
    toError: toAppError,
    busyMessage: messages.merge.busy,
    guardUnavailable: messages.close.guardUnavailable,
  }, request);

  const prepareEditTargets = useCallback(async (id: string, paths: string[]): Promise<boolean> => {
    try {
      const edits = await editSessions.resolve([id], paths, askEdit);
      if (!edits.ok) return false;
      if (edits.saved) {
        await refreshTarget(id);
        setAppError(localizedError("rejected", "edit.reviewAfterSave"), id);
        return false;
      }
      return true;
    } catch (caught) { setAppError(toAppError(caught), id); return false; }
  }, [askEdit, editSessions, refreshTarget, setAppError]);

  const closeEdit = useCallback(async (target: EditTarget) => {
    try {
      const edits = await editSessions.resolve([target.worktreeId], [target.path], askEdit);
      if (edits.ok && worktreeIdRef.current === target.worktreeId) {
        setEditingPath((path) => path === target.path ? null : path);
      }
    } catch (caught) { setAppError(toAppError(caught), target.worktreeId); }
  }, [askEdit, editSessions, setAppError]);

  const performWrite = useCallback(async (target: StagingTarget | DiscardTarget, discard = false) => {
    const opWorktreeId = target.worktreeId;
    // ref 在同一事件循环内也互斥；按钮禁用和 Rust 写锁共同兜底，不积压点击。
    if (writingWorktreesRef.current.has(opWorktreeId)) return;
    if (!(await prepareEditTargets(opWorktreeId, [target.path]))) return;
    if (writingWorktreesRef.current.has(opWorktreeId)) return;
    // 本次写操作的结果是否无法确定（Q91）：决定关闭时是否先展示待核对。
    let needsVerification = false;
    writingWorktreesRef.current.add(opWorktreeId);
    setWritingWorktrees(new Set(writingWorktreesRef.current));
    try {
      // 共享异步编排：登记 → await 保护 → 复核就绪；失败已回滚并真实收尾。
      const started = await writeOrchestrator.begin(opWorktreeId);
      if (!started.ok) {
        writingWorktreesRef.current.delete(opWorktreeId);
        setWritingWorktrees(new Set(writingWorktreesRef.current));
        setAppError(
          closeGuardBlockedError(started.reason),
          opWorktreeId,
        );
        return;
      }
    } catch (caught) {
      writingWorktreesRef.current.delete(opWorktreeId);
      setWritingWorktrees(new Set(writingWorktreesRef.current));
      setAppError(toAppError(caught), opWorktreeId);
      return;
    }
    setAppError(null, opWorktreeId);
    // 切换期间界面可能仍显示原目标；操作归属取真实写目标，不能取已前进的激活目标。
    const opToken = operationGateRef.current.begin(opWorktreeId);
    const request = { worktreeId: opWorktreeId, path: target.path, fingerprint: target.fingerprint };
    try {
      const unstaging = target.changes === "staged";
      let next: ChangesSnapshot;
      // 写结果的审阅分界必须在**请求发起前**取（ST07-R7-1 / SP07-R7-1）。
      const reviewRead = reviewOverlayRef.current.beginRead();
      if (target.segment) {
        // 按段（块）操作（返修 A1/Q142）：段目标原样传回，补丁由 Rust 从同一个 Git hunk 切开。
        const segmentRequest = {
          worktreeId: request.worktreeId,
          path: request.path,
          ...target.segment,
        };
        if (discard) {
          if (!("snapshotRevision" in target)) throw localizedError("rejected", "changes.fileChanged");
          next = await ipc.discardSegment({ ...segmentRequest, snapshotRevision: target.snapshotRevision });
        } else {
          next = await (unstaging ? ipc.unstageSegment : ipc.stageSegment)(segmentRequest);
        }
      } else if (discard) {
        if (!("snapshotRevision" in target)) throw localizedError("rejected", "changes.fileChanged");
        next = await ipc.discardHunk({ ...request, snapshotRevision: target.snapshotRevision, hunkId: target.hunkId });
      } else {
        next = await (unstaging ? ipc.unstageHunk : ipc.stageHunk)({ ...request, hunkId: target.hunkId });
      }
      // 写结果同样走统一接收门：晚到的 r2 不能覆盖已接受的 r3，A→B→A 也不能串味。
      const written = reviewRead.deliver(next);
      if (!written.ok) {
        // 写结果里的读取不可信：明确拒收并按**原写目标**重新核对（ST07-R15-1）。
        // 批次一定会 settle，所以下面的 finally 一定会释放写门禁（ST07-R16-1）。
        debugLog(`写结果读取不可信，重新核对：${written.worktreeId}@${written.observedSeq}`);
        await scheduleReviewRecovery(written.worktreeId);
        return;
      }
      if (operationGateRef.current.canRecord(opToken)) setAppError(null, opWorktreeId);
      if (operationGateRef.current.acceptSuccess(opToken, next)
        && acceptSnapshot(opToken, written.snapshot)) {
        setAppError(null, opWorktreeId);
        // 成功结果用右下角通知报告，不再走 diff 页签提示条（返修 C9，F07）。
        notify({
          tone: "success",
          kind: discard ? "discard" : "stage",
          title: () => (discard
            ? messages.changes.segmentDiscarded
            : unstaging
              ? messages.changes.segmentUnstaged
              : messages.changes.segmentStaged),
        });
      } else {
        debugLog(`写结果快照被接收门丢弃：${next.worktreeId}@${next.revision}`);
      }
    } catch (caught) {
      const err = toAppError(caught);
      needsVerification = err.kind === "needsVerification";
      // 原目标的结果不能因切走而丢失；字典按 worktree 隔离，不污染另一目标。
      // 写入失败也用通知报告（含结果待核对），提示条只留给环境类错误（返修 C9，F07）。
      if (operationGateRef.current.canRecord(opToken)) {
        notify({
          tone: err.kind === "needsVerification" ? "warning" : "error",
          title: () => (err.kind === "needsVerification" ? messages.errors.needsVerification : messages.changes.writeFailedTitle),
          detail: () => (formatAppError(err)),
          persistent: true,
        });
        if (operationGateRef.current.acceptFailure(opToken)
          && (err.kind === "needsVerification" || err.kind === "rejected")) void refresh();
      }
    } finally {
      writingWorktreesRef.current.delete(opWorktreeId);
      setWritingWorktrees(new Set(writingWorktreesRef.current));
      writeOrchestrator.end(opWorktreeId, needsVerification);
    }
  }, [acceptSnapshot, notify, prepareEditTargets, refresh, scheduleReviewRecovery, setAppError, writeOrchestrator]);

  const { performBatchWrite, stageFile, unstageFile } = useBatchWrite({
    worktreeIdRef, writingWorktreesRef, setWritingWorktrees, writeOrchestrator,
    operationGateRef, reviewOverlayRef, acceptSnapshot, scheduleReviewRecovery,
    refreshTarget, setAppError, notify, expandToSelection,
    beforeWrite: prepareEditTargets,
  });

  /**
   * 从回收记录恢复（任务 05）。复用全局非排队写门禁与操作结果通道：
   * 结果/错误按发起 worktree 保留，关闭面板或切换后不会丢；成功后真实读取快照。
   */
  const performRestore = useCallback(async (request: {
    worktreeId: string;
    recordId: string;
    expectedVersion: string;
    expectedPaths: string[];
    expectedState: RecoveryState;
  }) => {
    const opWorktreeId = request.worktreeId;
    if (writingWorktreesRef.current.has(opWorktreeId)) {
      throw localizedError("rejected", "recovery.busy");
    }
    writingWorktreesRef.current.add(opWorktreeId);
    setWritingWorktrees(new Set(writingWorktreesRef.current));
    try {
      // 共享异步编排：失败时已回滚并真实收尾，这里只需如实抛出。
      const started = await writeOrchestrator.begin(opWorktreeId);
      if (!started.ok) {
        writingWorktreesRef.current.delete(opWorktreeId);
        setWritingWorktrees(new Set(writingWorktreesRef.current));
        throw closeGuardBlockedError(started.reason);
      }
    } catch (caught) {
      writingWorktreesRef.current.delete(opWorktreeId);
      setWritingWorktrees(new Set(writingWorktreesRef.current));
      setAppError(toAppError(caught), opWorktreeId);
      throw caught;
    }
    setAppError(null, opWorktreeId);
    // 本次恢复的结果是否无法确定（Q91）：关闭时需先展示待核对。
    let needsVerification = false;
    const opToken = operationGateRef.current.begin(opWorktreeId);
    try {
      const outcome = await ipc.restoreRecovery(request);
      if (operationGateRef.current.canRecord(opToken)) setAppError(null, opWorktreeId);
      // 恢复不产生新的 ChangesSnapshot：真实读取一次，不依赖 watcher。
      // 切走时也必须重读原目标（真实重读不能因当前选择而跳过）。
      await refreshTarget(opWorktreeId);
      return outcome;
    } catch (caught) {
      const err = toAppError(caught);
      needsVerification = err.kind === "needsVerification";
      // 结果提示仍归原 worktree；但**真实重读**不依赖当前激活：
      // 切走之后原目标也必须核对真实状态（Q91），界面是否接收由 refreshTarget 判断。
      if (operationGateRef.current.canRecord(opToken)) setAppError(err, opWorktreeId);
      await refreshTarget(opWorktreeId);
      throw err;
    } finally {
      writingWorktreesRef.current.delete(opWorktreeId);
      setWritingWorktrees(new Set(writingWorktreesRef.current));
      writeOrchestrator.end(opWorktreeId, needsVerification);
    }
  }, [refreshTarget, setAppError, writeOrchestrator]);

  /**
   * 合并写入（Q143）：合并页签的“应用”、接受一侧，以及冲突对话框的接受一侧。
   * 与恢复同一套全局写门禁、关闭/退出保护和结果通道（Q90、Q91、Q96），见 `runMergeWrite`。
   * 失败用通知报告（返修 C9），组件关闭后仍保留。
   */
  const performMergeWrite = useCallback((request: MergeWriteRequest) => runMergeWrite<OperationContext>({
    writing: writingWorktreesRef.current,
    publishWriting: () => setWritingWorktrees(new Set(writingWorktreesRef.current)),
    begin: (worktreeId) => writeOrchestrator.begin(worktreeId),
    end: (worktreeId, needsVerification) => writeOrchestrator.end(worktreeId, needsVerification),
    beginOperation: (worktreeId) => operationGateRef.current.begin(worktreeId),
    canRecord: (token) => operationGateRef.current.canRecord(token),
    execute: (write) => write.kind === "apply"
      ? ipc.mergeApply({ worktreeId: write.worktreeId, path: write.path, result: write.result, fingerprint: write.fingerprint })
      : ipc.mergeAcceptSide({ worktreeId: write.worktreeId, targets: write.targets, side: write.side }),
    refreshTarget,
    reportFailure: (_worktreeId, error) => notify({
      tone: error.kind === "needsVerification" ? "warning" : "error",
      title: () => (error.kind === "needsVerification" ? messages.errors.needsVerification : messages.merge.writeFailedTitle),
      detail: () => (formatAppError(error)),
      persistent: true,
    }),
    toError: toAppError,
    busyMessage: messages.merge.busy,
    guardUnavailable: messages.close.guardUnavailable,
  }, request), [notify, refreshTarget, writeOrchestrator]);

  /**
   * 另存回收内容（任务 05）。
   *
   * 归属**由后端按目标路径解析**（`recoverySaveAsOwner`，与执行时同一条最具体匹配规则），
   * 前端不自己判断路径：
   * - 发起写入前先解析归属，再按该归属进入全局写门禁与 `OperationResultGate`；
   * - 错误按该归属 worktree 保存，面板关闭后仍留在原目标；
   * - 完成后对**该归属** worktree 真实重读，界面是否接收由 `refreshTarget` 判断；
   * - 外部新路径（无归属）合法：不进入任何 worktree 的门禁、不加强制锁，
   *   但结果走**独立的 App 级无归属通道**，面板关闭/返回列表/切换后仍保留（Q90、Q91）。
   */
  const performSaveAs = useCallback(async (request: {
    recordId: string;
    path: string;
    destination: string;
  }) => {
    // 无归属操作的序号：解析阶段就分配，保证「解析失败」也能落到可保留的通道，
    // 并且旧请求的完成不会清掉较新的错误。
    const seq = ++externalSeqRef.current;
    let ownerId: string | null = null;
    /**
     * 本次请求**实际取得**的写门禁。
     *
     * 只有它才允许在 `finally` 释放：目标已经忙时本请求没有取得门禁，
     * 释放会把另一笔仍在执行的操作的门禁清掉（Q90）。
     */
    let heldGateId: string | null = null;
    let needsVerification = false;
    // 进入即登记：解析归属期间也算在途，避免解析窗口内被关闭绕过。
    // 无归属（外部新路径）用占位键，不为此路径强加 worktree 写锁。
    const provisionalKey = `save-as:${request.destination}`;
    try {
      // 共享异步编排：失败时已回滚并真实收尾。
      const started = await writeOrchestrator.begin(provisionalKey);
      if (!started.ok) {
        throw closeGuardBlockedError(started.reason);
      }
    } catch (caught) {
      setAppError(toAppError(caught), null);
      throw caught;
    }
    let opToken: OperationContext | null = null;
    try {
      // 后端解析真实归属；解析失败同样落到 App 级结果通道，不猜路径、不借用当前 worktree。
      const owner = await ipc.recoverySaveAsOwner(request.destination);
      ownerId = owner.worktreeId;
      if (ownerId) {
        if (writingWorktreesRef.current.has(ownerId)) {
          // 目标已有在途写操作：本请求没有取得门禁，也不取新的结果令牌。
          throw localizedError("rejected", "recovery.busy");
        }
        writingWorktreesRef.current.add(ownerId);
        heldGateId = ownerId;
        setWritingWorktrees(new Set(writingWorktreesRef.current));
        // 有归属：改由该 worktree 承担关闭生命周期。
        closeCoordinatorRef.current.endWrite(provisionalKey, false);
        closeCoordinatorRef.current.beginWrite(ownerId);
        opToken = operationGateRef.current.begin(ownerId);
        setAppError(null, ownerId);
      }
      const bytes = await ipc.saveRecoveryAs({
        worktreeId: ownerId,
        expectedOwner: ownerId,
        recordId: request.recordId,
        path: request.path,
        destination: owner.resolvedDestination,
      });
      if (ownerId && opToken) {
        if (operationGateRef.current.canRecord(opToken)) setAppError(null, ownerId);
        await refreshTarget(ownerId);
      } else if (!ownerId) {
        // 外部目标：成功只清除不比它更新的外部错误。
        recordExternalResult(seq, null);
      }
      return bytes;
    } catch (caught) {
      const err = toAppError(caught);
      needsVerification = err.kind === "needsVerification";
      if (ownerId) {
        // 归属 worktree：结果按它保存，并对它真实重读（切走也要读）。
        if (!opToken || operationGateRef.current.canRecord(opToken)) setAppError(err, ownerId);
        await refreshTarget(ownerId);
      } else {
        // 外部目标（含归属解析阶段失败）：保留在 App 级无归属通道，
        // 不写进当前 worktree 的错误槽。
        recordExternalResult(seq, err);
      }
      throw err;
    } finally {
      // 只释放本次真正取得的门禁：成功、写错误、刷新错误都会走到这里；
      // 被忙碌拒绝（没有取得门禁）的请求不碰别人的占用。
      if (heldGateId) {
        writingWorktreesRef.current.delete(heldGateId);
        setWritingWorktrees(new Set(writingWorktreesRef.current));
      }
      // 关闭生命周期收尾：与其它写入口**完全一致**地走 finishWrite，
      // 这样「最后一笔另存」也会消费已授权的等待请求（ST-B02-08-3 / SP-B02-08-3）。
      // 有归属用 worktree 键，无归属用占位键；needsVerification 如实传递（Q91）。
      writeOrchestrator.end(heldGateId ?? provisionalKey, needsVerification);
    }
  }, [recordExternalResult, refreshTarget, setAppError, writeOrchestrator]);

  // ---------------- 提交工作流（M3 F01–F06） ----------------

  /**
   * 提交区状态（输入、代际、草稿落地判定）放在一个可变对象里：
   *
   * 异步草稿读写必须绑定 worktree 与代际，旧回包不能覆盖新输入（F02.5）。
   * 用 ref 保存，React 状态只做渲染镜像。
   */
  const commitStateRef = useRef(new CommitComposerState());
  const [commitInput, setCommitInput] = useState<ComposerInput>(EMPTY_INPUT);
  const commitInputRef = useRef(commitInput);
  commitInputRef.current = commitInput;
  const [commitContext, setCommitContext] = useState<CommitContext | null>(null);
  const commitContextRef = useRef<CommitContext | null>(null);
  commitContextRef.current = commitContext;
  const [commitContextError, setCommitContextError] = useState<AppErrorPayload | null>(null);
  const [commitSubmittingWorktrees, setCommitSubmittingWorktrees] = useState<Set<string>>(new Set());
  const commitSubmittingRef = useRef(new Set<string>());
  const commitSubmitting = worktreeId !== null && commitSubmittingWorktrees.has(worktreeId);
  // 磁盘保存失败时仍按身份保留本地输入；切换回来先核身份，不能继承同路径旧仓库。
  const localDraftsRef = useRef(new Map<string, { input: ComposerInput; revision: string; identity: string | null }>());
  const [commitDraftNotice, setCommitDraftNotice] = useState<NoticeText | null>(null);
  const setCommitResultFor = useCallback((view: CommitResultView) => {
    setCommitResults((current) => ({ ...current, [view.worktreeId]: view }));
  }, [setCommitResults]);
  /**
   * 提交进行中的实时输出（返修 C8，F04.3）：显示在提交区，可展开；
   * 完成后的结果由通知报告，这里只保留进行中的进度与输出。
   */
  const [commitLiveOutput, setCommitLiveOutput] = useState<{ worktreeId: string; output: string[] } | null>(null);
  /**
   * amend 二次确认（F06.2）：凭据绑定**当时的**整组预期 + 信息 + 选项。
   *
   * 任何刷新/切换/编辑都会清掉它，因此不会拿旧确认去改一份新的提交内容。
   */
  /** 提交时警告（返修 E.3）：发现问题时先弹提示，可以继续或取消。 */
  const [commitWarningConfirm, setCommitWarningConfirm] = useState<{
    warnings: CommitWarning[];
    worktreeId: string;
    activation: number;
    confirmation: AmendConfirmation | null;
    pushAfter: boolean;
  } | null>(null);
  const [amendConfirm, setAmendConfirm] = useState<{
    upstreamName: string;
    confirmation: AmendConfirmation;
    worktreeId: string;
    activation: number;
    contextEpoch: number;
    generation: number;
    pushAfter: boolean;
  } | null>(null);
  const commitOutputsRef = useRef<CommitOutputStore | null>(null);
  if (commitOutputsRef.current === null) commitOutputsRef.current = new CommitOutputStore();
  const commitOutputs = commitOutputsRef.current;
  /** 提交上下文读取序号：迟到的上下文不能覆盖新 worktree 的上下文（F02.5）。 */
  const commitContextEpochRef = useRef(0);
  const draftPersistenceRef = useRef<CommitDraftPersistence | null>(null);
  if (draftPersistenceRef.current === null) {
    draftPersistenceRef.current = new CommitDraftPersistence(({ write, error }) => {
      if (worktreeIdRef.current !== write.worktreeId) return;
      if (error !== null) {
        setCommitDraftNotice(() => () => messages.commit.draftFailed(formatAppError(error)));
      } else if (commitStateRef.current.recordSaved(write.worktreeId, write.generation, write.input)) {
        setCommitDraftNotice(() => () => messages.commit.draftSaved);
      }
    });
  }
  const draftPersistence = draftPersistenceRef.current;

  const queueCommitDraft = useCallback((targetWorktreeId: string, input: ComposerInput) => {
    const revision = newCommitNonce();
    const context = commitContextRef.current;
    const identity = context?.worktreeId === targetWorktreeId ? context.draftIdentity : null;
    localDraftsRef.current.set(targetWorktreeId, { input, revision, identity });
    draftPersistence.schedule({
      worktreeId: targetWorktreeId,
      input,
      revision,
      generation: commitStateRef.current.current().generation,
    });
  }, [draftPersistence]);

  // 编辑、提交和关闭共用持久化顺序；关闭处不再读取内部队列或失败表。
  flushCommitDraftRef.current = (target) => draftPersistence.flush(target);

  /**
   * 读取提交上下文（预期、模板、最近信息、草稿、上游、门禁）。
   *
   * 只读；任何读取失败都展示原因，不静默禁用提交（F01.1/F02.3）。
   * 读取在**发起时**就绑定 worktree / 激活代号 / 输入代际（ticket），
   * await 回来只按它判断能否落地，旧读取不会覆盖新输入（F02.5）。
   */
  const loadCommitContext = useCallback(async (targetWorktreeId: string): Promise<CommitContext | null> => {
    if (worktreeIdRef.current !== targetWorktreeId) return null;
    setAmendConfirm(null);
    const epoch = ++commitContextEpochRef.current;
    const activation = gateRef.current.token().activation;
    const ticket = commitStateRef.current.ticket(targetWorktreeId, activation);
    try {
      const next = await ipc.commitContext(targetWorktreeId);
      if (epoch !== commitContextEpochRef.current) return null;
      if (worktreeIdRef.current !== targetWorktreeId) return null;
      if (gateRef.current.token().activation !== activation) return null;
      commitContextRef.current = next;
      setCommitContext(next);
      setCommitContextError(null);
      if (next.templateError) setCommitDraftNotice(() => () => messages.commit.templateFailed(formatAppError(next.templateError!)));
      if (next.draftError) setCommitDraftNotice(() => () => messages.commit.draftLoadFailed(formatAppError(next.draftError!)));
      const state = commitStateRef.current;
      if (closeDriver.currentPhase() === "closing" || !next.draftIdentity) return next;
      const cached = localDraftsRef.current.get(targetWorktreeId);
      if (cached?.identity === null && state.current().touched && next.draftIdentity) {
        cached.identity = next.draftIdentity;
      }
      if (cached && next.draftIdentity && cached.identity === next.draftIdentity && !state.current().touched) {
        const restored = { ...cached.input, skipHooks: false };
        cached.input = restored;
        state.edit(restored);
        commitInputRef.current = restored;
        setCommitInput(restored);
        const failure = draftPersistence.failurePayloadFor(targetWorktreeId);
        if (failure) setCommitDraftNotice(() => () => messages.commit.draftFailed(formatAppError(failure)));
        return next;
      }
      if (cached?.identity && next.draftIdentity && cached.identity !== next.draftIdentity) {
        localDraftsRef.current.delete(targetWorktreeId);
      }
      const loaded = state.applyLoaded(ticket, activation, next.draft);
      if (loaded.applied) {
        localDraftsRef.current.set(targetWorktreeId, {
          input: loaded.input, revision: next.draft?.revision || newCommitNonce(), identity: next.draftIdentity,
        });
        commitInputRef.current = loaded.input;
        setCommitInput(loaded.input);
        setCommitDraftNotice(() => () => messages.commit.draftRestored);
      } else if (!next.draftError && state.needsTemplateSeed()) {
        // 没有草稿且用户还没输入：用模板初始化**一次**（模板不覆盖用户输入，F02.3）。
        // 初始化后立刻排队保存，避免「只是接受模板、没编辑」时正常关闭丢稿。
        state.markTemplateSeeded();
        // 模板只在「用户还没有任何本地输入」时初始化一次（F02.3）。
        const untouched = !state.current().touched;
        const snapshot = state.current();
        const empty = !messageSubmittable(snapshot.input.message)
          && !snapshot.input.amend && !snapshot.input.skipHooks;
        if (untouched && empty && next.template) {
          const seeded: ComposerInput = { ...snapshot.input, message: next.template };
          state.edit(seeded);
          commitInputRef.current = seeded;
          setCommitInput(seeded);
          setCommitDraftNotice(() => () => messages.commit.templateUsed);
          queueCommitDraft(targetWorktreeId, seeded);
        }
      }
      return next;
    } catch (caught) {
      if (epoch !== commitContextEpochRef.current) return null;
      if (worktreeIdRef.current !== targetWorktreeId) return null;
      setCommitContextError(toAppError(caught));
      return null;
    }
  }, [closeDriver, draftPersistence, queueCommitDraft]);

  /** 输入变化：推进代际、更新界面、按代际异步保存草稿（不阻塞输入）。 */
  const updateCommitInput = useCallback((next: ComposerInput, options?: { persist?: boolean }) => {
    const target = worktreeIdRef.current;
    if (!target || closeDriver.currentPhase() === "closing") return;
    commitStateRef.current.edit(next);
    commitInputRef.current = next;
    setCommitInput(next);
    // 任何输入变化都让旧确认失效（F06.2）。
    setAmendConfirm(null);
    if (options?.persist !== false) queueCommitDraft(target, next);
  }, [closeDriver, queueCommitDraft]);

  /** 切换 worktree / 重新打开会话时重置提交区（每个 worktree 各自独立，F02.5）。 */
  const resetCommitComposer = useCallback((targetWorktreeId: string | null) => {
    // 先把上一个 worktree 尚未落盘的输入排队写出去（不 await：切换不阻塞界面，
    // 但写入仍在它自己的队列里，稍后加载该 worktree 时会等这个队列）。
    void draftPersistence.flush().catch(() => undefined);
    commitStateRef.current.reset(targetWorktreeId);
    commitStateRef.current.setSubmitting(targetWorktreeId !== null && commitSubmittingRef.current.has(targetWorktreeId));
    commitInputRef.current = resetInput();
    commitContextRef.current = null;
    // 跳过 hooks 只属于原 worktree 的那一次命令：切换后一律复位（F04.2）。
    setCommitInput(resetInput());
    setCommitContext(null);
    setCommitContextError(null);
    setCommitDraftNotice(null);
    setAmendConfirm(null);
    setCommitWarningConfirm(null);
  }, [draftPersistence]);

  /**
   * worktree 切换 / 会话打开后：重置提交区并按该 worktree 读取上下文。
   */
  useEffect(() => {
    if (!worktreeId) {
      resetCommitComposer(null);
      return;
    }
    resetCommitComposer(worktreeId);
    // 切回某个 worktree 时必须先等它自己的草稿写入队列结束，
    // 否则会先读到旧盘内容并覆盖本地更新的输入（F02.5）。
    void (async () => {
      try {
        await draftPersistence.waitFor(worktreeId);
      } catch {
        // 队列失败已由写入路径展示；这里继续读取，读取结果本身仍可见。
      }
      if (worktreeIdRef.current !== worktreeId) return;
      await loadCommitContext(worktreeId);
    })();
  }, [draftPersistence, loadCommitContext, resetCommitComposer, worktreeId]);

  /**
   * 刷新 / 回到前台 / 监视事件后更新提交门禁（F01.3：刷新、切换、回到前台都要更新）。
   *
   * 只读上下文；不因为刷新而覆盖用户已经输入的正文。刷新出新 index 时
   * 旧的 amend 二次确认必须失效（F06.2）。
   */
  useEffect(() => {
    if (!worktreeId || snapshot?.worktreeId !== worktreeId) return;
    setAmendConfirm(null);
    void draftPersistence.waitFor(worktreeId).then(() => loadCommitContext(worktreeId));
  }, [draftPersistence, loadCommitContext, snapshot?.revision, snapshot?.worktreeId, worktreeId]);

  /** amend 勾选：勾上时用最近一次提交的完整信息预填（异步旧信息不得覆盖新输入，F06.1）。 */
  const toggleAmend = useCallback((checked: boolean) => {
    const target = worktreeIdRef.current;
    if (!target || closeDriver.currentPhase() === "closing") return;
    const current = commitInputRef.current;
    if (!checked) {
      updateCommitInput({ ...current, amend: false });
      return;
    }
    const context = commitContextRef.current;
    const expectedHead = context?.expectation.head;
    if (context?.worktreeId !== target || !context.gates.amendAvailable || !expectedHead) return;
    const pendingContextEpoch = commitContextEpochRef.current;
    const pendingActivation = gateRef.current.token().activation;
    const state = commitStateRef.current;
    // 先记下 amend 选择（立即生效），信息预填等异步结果回来再补。
    const withAmend = { ...current, amend: true };
    state.edit(withAmend);
    commitInputRef.current = withAmend;
    setCommitInput(withAmend);
    queueCommitDraft(target, withAmend);
    setAmendConfirm(null);
    // 预填必须绑定「勾上 amend 之后」的那一代输入：期间用户又编辑或切换
    // worktree 时，旧信息不得覆盖新输入（F06.1）。
    const pendingGeneration = state.current().generation;
    const canApplyPrefill = () => worktreeIdRef.current === target
      && closeDriver.currentPhase() !== "closing"
      && state.current().generation === pendingGeneration
      && commitContextEpochRef.current === pendingContextEpoch
      && gateRef.current.token().activation === pendingActivation
      && commitContextRef.current?.expectation.head === expectedHead;
    void ipc.amendHeadMessage(target, expectedHead)
      .then((message) => {
        if (!canApplyPrefill()) return;
        const seeded = { ...commitInputRef.current, message: message ?? "" };
        updateCommitInput(seeded);
        setCommitDraftNotice(() => () => messages.commit.amendPrefilled);
      })
      .catch(() => {
        if (!canApplyPrefill()) return;
        setCommitDraftNotice(() => () => messages.commit.amendPrefillFailed);
      });
  }, [closeDriver, queueCommitDraft, updateCommitInput]);

  /** 选择一条最近的提交信息（只替换信息正文，不改变 amend/skip 选项，F02.4）。 */
  const pickRecentMessage = useCallback((value: string) => {
    const current = commitInputRef.current;
    updateCommitInput({ ...current, message: value });
  }, [updateCommitInput]);

  /**
   * 执行提交（F03/F04/F05/F06）。
   *
   * 与其它写入口**完全一致**：同一套 `WriteGuardOrchestrator`（关闭/退出保护）、
   * 同一份 `writingWorktrees` 非排队互斥、同一套按 worktree 保留结果的错误通道。
   * 结果与错误归**原 worktree**，切走后不覆盖当前界面（F03.3）。
   */
  const performCommit = useCallback(async (
    confirmation: AmendConfirmation | null = null,
    options: { skipWarnings?: boolean; pushAfter?: boolean } = {},
  ) => {
    const origin = captureRemoteTarget(gateRef.current.token(), worktreeIdRef.current);
    if (!origin) {
      setAppError(localizedError("rejected", "commit.noWorktree"), null);
      return;
    }
    const target = origin.worktreeId;
    if (commitSubmittingRef.current.has(target) || writingWorktreesRef.current.has(target) || closeDriver.currentPhase() === "closing") return;
    const listView = changelistMode ? changelistRef.current : null;
    if (changelistMode && (!listView || listView.worktreeId !== target || (!listView.items.some((item) => item.selected && !item.stale && !item.readonlyReason) && !commitInputRef.current.amend && !commitContextRef.current?.gates.allowsEmptyDiffCommit))) {
      setAppError(localizedError("rejected", "errors.m7.changelistChanged"), target); return;
    }
    const listSelection = listView ? selectionOf(listView) : null;
    const related = listView ? listView.items.filter((item) => item.selected && !item.stale).map((item) => item.path) : commitContextRef.current?.expectation.index.filter((entry) => entry.status !== "F")
      .flatMap((entry) => [entry.path, ...(entry.originalPath ? [entry.originalPath] : [])]) ?? [];
    if (!(await prepareEditTargets(target, related))) return;
    if (worktreeIdRef.current !== target || gateRef.current.token().activation !== origin.activation) return;
    const context = commitContextRef.current;
    if (!context || context.worktreeId !== target) {
      setAppError(localizedError("rejected", "commit.stale"), target);
      return;
    }
    const input = commitInputRef.current;
    if (!messageSubmittable(input.message)) {
      setAppError(localizedError("rejected", "commit.emptyMessage"), target);
      return;
    }
    // amend 且 HEAD 已包含于本地上游（或本地无法判断是否已发布）：必须有一次
    // 绑定预期/信息/选项的二次确认（F06.2）。
    if (input.amend && (context.gates.upstreamContainsHead || context.gates.upstreamUnknown)
      && confirmation === null) {
      setAmendConfirm({
        upstreamName: context.upstreamName ?? "",
        confirmation: AmendConfirmationOf(context, input),
        worktreeId: target,
        activation: gateRef.current.token().activation,
        contextEpoch: commitContextEpochRef.current,
        generation: commitStateRef.current.current().generation,
        pushAfter: options.pushAfter === true,
      });
      return;
    }
    setAmendConfirm(null);
    // 提交时警告（返修 E.3）：检查由 Rust 完成，发现问题时先弹提示。
    if (!options.skipWarnings) {
      let warnings: CommitWarning[] = [];
      try {
        warnings = listSelection
          ? await invoke<CommitWarning[]>("m7_changelist_warnings", { request: { worktreeId: target, selection: listSelection, amend: input.amend } })
          : await ipc.commitWarnings(target);
      } catch (caught) {
        if (listSelection) { setAppError(toAppError(caught), target); return; }
        warnings = [];
      }
      if (!remoteTargetIsCurrent(origin, gateRef.current.token())) return;
      if (warnings.length > 0) {
        setCommitWarningConfirm({
          warnings,
          worktreeId: target,
          activation: origin.activation,
          confirmation,
          pushAfter: options.pushAfter === true,
        });
        return;
      }
    }
    if (commitSubmittingRef.current.has(target) || writingWorktreesRef.current.has(target)) return;
    const submitted: ComposerInput = { ...input };
    const submittedExpectation = context.expectation;
    const contextEpochAtSubmit = commitContextEpochRef.current;
    const activationAtSubmit = gateRef.current.token().activation;
    const submittedRevision = localDraftsRef.current.get(target)?.revision ?? newCommitNonce();
    localDraftsRef.current.set(target, {
      input: submitted, revision: submittedRevision, identity: context.draftIdentity,
    });
    const targetTitle = snapshotRef.current?.worktreePath ?? target;
    commitSubmittingRef.current.add(target);
    commitStateRef.current.setSubmitting(true);
    setCommitSubmittingWorktrees(new Set(commitSubmittingRef.current));
    writingWorktreesRef.current.add(target);
    setWritingWorktrees(new Set(writingWorktreesRef.current));
    const releaseFlight = () => {
      commitSubmittingRef.current.delete(target);
      setCommitSubmittingWorktrees(new Set(commitSubmittingRef.current));
      if (commitStateRef.current.current().worktreeId === target) commitStateRef.current.setSubmitting(false);
      writingWorktreesRef.current.delete(target);
      setWritingWorktrees(new Set(writingWorktreesRef.current));
    };
    let needsVerification = false;
    let pushAfterCommit = false;
    let gateHeld = false;
    try {
      // 先登记生命周期再等待草稿，关闭不能越过这段异步准备去销毁窗口。
      const started = await writeOrchestrator.begin(target);
      if (!started.ok) throw closeGuardBlockedError(started.reason);
      gateHeld = true;
      await draftPersistence.flush(target);
      // 即使草稿来自加载/模板，也保存并绑定这一次提交的精确版本。
      if (localDraftsRef.current.get(target)?.revision === submittedRevision) {
        await draftPersistence.save({
          worktreeId: target,
          input: submitted,
          revision: submittedRevision,
          generation: commitStateRef.current.current().generation,
        });
      }
      const failure = draftPersistence.failurePayloadFor(target);
      if (failure) throw failure;
      if (confirmation && (commitContextEpochRef.current !== contextEpochAtSubmit
        || gateRef.current.token().activation !== activationAtSubmit
        || localDraftsRef.current.get(target)?.revision !== submittedRevision)) {
        throw localizedError("rejected", "commit.amendConfirmStale");
      }
    } catch (caught) {
      setAppError(toAppError(caught), target);
      releaseFlight();
      if (gateHeld) writeOrchestrator.end(target, false);
      return;
    }
    setAppError(null, target);
    const opToken = operationGateRef.current.begin(target);
    const clientNonce = newCommitNonce();
    const outputRun = commitOutputs.start(target, clientNonce);
    // 先把结果区打开并清空输出：Git 的 stdout/stderr 会实时追加到这里（F04.1）。
    // 实时输出也显示在提交区（返修 C8，F04.3）。
    setCommitLiveOutput({ worktreeId: target, output: [] });
    setCommitResultFor({
      worktreeId: target,
      title: () => messages.commit.outputTitle(targetTitle),
      detail: null,
      outcome: null,
      output: [],
      needsVerification: false,
      hookChanges: null,
      snapshotRecordId: null,
      snapshotPaths: [],
    });
    try {
      const outcome = await ipc.executeCommit({
        worktreeId: target,
        selection: listSelection,
        options: {
          message: submitted.message,
          amend: submitted.amend,
          skipHooks: submitted.skipHooks,
          expected: submittedExpectation,
          amendConfirmation: confirmation,
          clientNonce,
          draftRevision: submittedRevision,
        },
      });
      // 已确认失败：结构化结果里带 Git 类错误，输出/hooks 改动/快照都保留（F04.3）。
      const failed = !outcome.success;
      const gitError = outcome.error;
      needsVerification = outcome.needsVerification;
      setCommitResultFor({
        worktreeId: target,
        title: () => outcome.success
          ? (submitted.amend
            ? messages.commit.resultAmend(outcome.newHead ?? "")
            : messages.commit.resultSuccess(outcome.newHead ?? ""))
          : gitError
            ? messages.commit.resultGitFailed(outcome.exitCode ?? -1)
            : messages.commit.resultTitle,
        detail: () => outcome.draftError
          ? messages.commit.resultDraftError(formatAppError(outcome.draftError))
          : [
            submitted.message.split("\n")[0],
            outcome.verificationNote ? formatAppError(outcome.verificationNote) : null,
          ].filter((part): part is string => typeof part === "string" && part.length > 0).join("\n") || null,
        outcome,
        output: commitOutputs.finish(outputRun, outcome.output),
        needsVerification: outcome.needsVerification,
        hookChanges: outcome.hookChanges,
        snapshotRecordId: outcome.snapshotRecordId,
        snapshotPaths: outcome.snapshotPaths,
      });
      if (failed) {
        // 已确认失败：错误按 Git 类归原 worktree，草稿保留。
        if (operationGateRef.current.canRecord(opToken)) {
          setAppError(gitError ?? localizedError("git", "commit.resultTitle"), target);
        }
      } else if (outcome.needsVerification) {
        // 提交已成功但收尾（如快照记录）需要核对：如实提示。
        if (operationGateRef.current.canRecord(opToken)) {
          setAppError(
            localizedError("needsVerification", "commit.resultNeedsVerification"),
            target,
          );
        }
      } else {
        // 成功且草稿确实被清：清掉本地输入（期间新输入不会被抹掉，F02.5）。
        if (outcome.draftCleared && localDraftsRef.current.get(target)?.revision === submittedRevision) {
          localDraftsRef.current.delete(target);
          if (commitStateRef.current.current().worktreeId === target) {
            commitStateRef.current.clear();
            commitInputRef.current = resetInput();
            setCommitInput(resetInput());
            setCommitDraftNotice(null);
          }
        }
        if (operationGateRef.current.canRecord(opToken)) setAppError(null, target);
        // 提交并推送（B06）：只有提交成功且结果确定时才推送；失败/待核对不推送。
        pushAfterCommit = options.pushAfter === true;
      }
      // 不论成功、已确认失败还是待核对，都刷新**原 worktree** 的真实状态：
      // 提交可能已经改变了 HEAD/index，界面必须与真实状态一致（F04.3/F06.4）。
      await refreshTarget(target);
      await loadCommitContext(target);
    } catch (caught) {
      const err = toAppError(caught);
      pushAfterCommit = false;
      needsVerification ||= err.kind === "needsVerification";
      const lines = commitOutputs.snapshot(outputRun);
      setCommitResultFor({
        worktreeId: target,
        title: () => err.kind === "needsVerification" ? messages.commit.resultNeedsVerification : formatAppError(err),
        detail: () => err.kind === "needsVerification" ? formatAppError(err) : null,
        outcome: null,
        output: lines,
        needsVerification: err.kind === "needsVerification",
        hookChanges: null,
        snapshotRecordId: null,
        snapshotPaths: [],
      });
      // 失败/待核对保留草稿：本地输入不清，也不改写已保存的草稿（F02.5）。
      if (operationGateRef.current.canRecord(opToken)) setAppError(err, target);
      // 任何失败都重新核对原目标：预期/门禁可能已变化（F01.3）。
      await refreshTarget(target);
      await loadCommitContext(target);
    } finally {
      // 所有退出分支都复原本次 skip；只触碰仍属于原次输入的缓存。
      const local = localDraftsRef.current.get(target);
      if (local?.revision === submittedRevision && local.input.skipHooks) {
        local.input = { ...local.input, skipHooks: false };
        if (commitStateRef.current.current().worktreeId === target) {
          commitStateRef.current.edit(local.input);
          commitInputRef.current = local.input;
          setCommitInput(local.input);
        }
      }
      await finishCommitAndContinue(origin, pushAfterCommit, (pushNeedsVerification) => {
        releaseFlight();
        commitOutputs.release(outputRun);
        // 提交结束（成功/失败/待核对）：实时输出清掉，结果由通知报告。
        setCommitLiveOutput((current) => (current && current.worktreeId === target ? null : current));
        writeOrchestrator.end(target, needsVerification || pushNeedsVerification);
      }, (original, lease) => commitAndPushRef.current?.(original, lease) ?? Promise.resolve());
    }
  }, [changelistMode, closeDriver, commitOutputs, draftPersistence, loadCommitContext, prepareEditTargets, refreshTarget, setAppError, writeOrchestrator]);

  /** 确认 amend 的二次警告：只对**当前**上下文成立时生效（F06.2）。 */
  const confirmAmend = useCallback(() => {
    const pending = amendConfirm;
    if (!pending) return;
    const context = commitContextRef.current;
    const input = commitInputRef.current;
    if (!context
      || worktreeIdRef.current !== pending.worktreeId
      || gateRef.current.token().activation !== pending.activation
      || commitContextEpochRef.current !== pending.contextEpoch
      || commitStateRef.current.current().generation !== pending.generation
      || context.expectation.fingerprint !== pending.confirmation.expectationFingerprint
      || input.message !== pending.confirmation.message
      || input.amend !== pending.confirmation.amend
      || input.skipHooks !== pending.confirmation.skipHooks
      || context.upstreamVersion !== pending.confirmation.upstreamVersion
      || !(context.gates.upstreamContainsHead || context.gates.upstreamUnknown)) {
      setAmendConfirm(null);
      setAppError(localizedError("rejected", "commit.amendConfirmStale"), worktreeIdRef.current);
      return;
    }
    setAmendConfirm(null);
    void performCommit(pending.confirmation, { pushAfter: pending.pushAfter });
  }, [amendConfirm, performCommit, setAppError]);

  /** 实时输出的传输与展示；归属、缓冲和终态判定由同一个 module 持有。 */
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<CommitOutputEvent>(COMMIT_OUTPUT_EVENT, (event) => {
      if (disposed) return;
      const update = commitOutputs.accept(event.payload);
      if (!update) return;
      // 实时输出进提交区（返修 C8，F04.3）；完成后的结果仍在 commitResults 里。
      setCommitLiveOutput((current) =>
        current && current.worktreeId === update.worktreeId ? { ...current, output: update.output } : current,
      );
      setCommitResults((current) => {
        const existing = current[update.worktreeId];
        if (!existing) return current;
        return { ...current, [update.worktreeId]: { ...existing, output: update.output } };
      });
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch((caught) => {
      if (disposed) return;
      // 监听安装失败必须可见：否则用户以为有实时输出而实际没有（F04.1）。
      setAppError(commitOutputListenerFailure(caught), null);
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [commitOutputs, setAppError]);

  /**
   * 切换某个文件的审阅标记（任务 07）。
   *
   * 勾选框只切换、不改变选择；⌥⌘R 复用同一个写入路径。
   * - 请求带上**用户所见**的 HEAD + 工作区版本，后端不一致即拒绝（Q20、Q89）；
   * - 迟到响应同时校验 worktree、activation 与快照版本，不污染其他 worktree 或新版本；
   * - 成功时返回后端确认过的进度，导航直接用它，不依赖 React 先 render。
   */
  const setReviewMark = useCallback(async (
    entry: ChangeEntry,
    marked: boolean,
  ): Promise<ReviewMarks | null> => {
    const targetWorktreeId = worktreeIdRef.current;
    const targetSnapshot = snapshotRef.current;
    if (!targetWorktreeId || !targetSnapshot) return null;
    // 代际推进必须在**任何**返回路径之前：本地拒绝也算一次较新的请求，
    // 否则先前那次请求（例如对 A 的勾选）返回时会通过 `isCurrent`，
    // 把这里刚显示的更新错误清掉（ST07-R5-1、Q36/Q90）。
    const epoch = ++reviewEpochRef.current;
    // 版本无法确认时本地就阻止新增认领（SP07-R4-1）：后端也会拒绝，但不必先发一次请求。
    // 取消已有标记不受限制，保留清理能力。
    if (marked && !entry.worktreeVersionConfirmed) {
      setAppError(localizedError("rejected", "review.unconfirmed"), targetWorktreeId);
      return null;
    }
    const targetActivation = gateRef.current.token().activation;
    const targetRevision = targetSnapshot.revision;
    /**
     * 发布与错误清理的判定**全部交给共享生产入口**（`publishReviewSuccess`）。
     *
     * 这里只负责如实提供上下文事实；`epoch`（点击代际）只用于错误清理，
     * 不参与发布判定（ST07-R11-2：更晚执行的真实成功不能被旧点击丢掉）。
     */
    try {
      const marks = await ipc.setReviewMark({
        worktreeId: targetWorktreeId,
        path: entry.path,
        marked,
        expectedVersions: {
          head: entry.headVersion ?? null,
          worktree: entry.worktreeVersion ?? null,
        },
      });
      // 记账**先于**展示门禁（ST07-R7-3 / SP07-R7-2）：后端已经持久化，
      // 迟到响应只是不该更新界面，不能让后续旧读取把这条真实变更回退。
      // 状态、版本、先后**都来自后端返回值**：`marks` 每条自带 reviewed 与它绑定的版本
      // （取消项是 false），`seq` 是真实提交序号。
      // 记账 + 合成展示状态都走同一个生产入口：展示**不能**直接用这次回执的集合，
      // 更晚的其它成功操作可能已经记账（B 先执行 seq1、A 后执行 seq2 但 A 的回执先到）
      // （ST07-R10-2 / SP07-R10-2）。错误清理与展示分开，不清掉更新的错误。
      // 记账 → 上下文判定 → 发布/导航，全部走同一个生产入口：
      // 发布只看上下文（不含点击代际），错误清理仍按点击代际（ST07-R11-2、Q36）。
      const published = publishReviewSuccess({
        overlay: reviewOverlayRef.current,
        worktreeId: targetWorktreeId,
        reviewed: marks.reviewed,
        marks: marks.marks,
        seq: marks.seq,
        snapshot: snapshotRef.current,
        context: {
          worktreeMatches: worktreeIdRef.current === targetWorktreeId,
          activationMatches: gateRef.current.token().worktreeId === targetWorktreeId
            && gateRef.current.token().activation === targetActivation,
          revisionMatches: snapshotRef.current?.revision === targetRevision,
          epochMatches: reviewEpochRef.current === epoch,
        },
      });
      if (!published.view) {
        // 发布被拒（`needsRefresh`）：叠加层容量淘汰后信息不足，必须按**原目标**
        // 真实重新核对，不能等未来恰好有 watcher 事件（ST07-R15-2 / SP07-R15-1）。
        if (published.needsRefresh) {
          debugLog(`审阅发布不可信，重新核对：${targetWorktreeId}`);
          await scheduleReviewRecovery(targetWorktreeId);
        }
        return null;
      }
      const view = published.view;
      setSnapshot((current) => current && current.worktreeId === targetWorktreeId
        // 只在 `current` 上合并审阅字段，不覆盖已排队的元数据补全（SP07-R11-4）。
        ? mergePublishedReview(current, view, marks.seq)
        : current);
      if (published.clearError) setAppError(null, targetWorktreeId);
      // 导航与进度用**合成后**的状态，不用后端这次返回的原始集合。
      // `marks` 字段保留后端这次确认的完整状态（含负状态），供调用方诊断。
      return { ...marks, ...published.navigation };
    } catch (caught) {
      // 失败**始终**记到发起它的 worktree（Q36、Q90）：切走或刷新只影响当前展示，
      // 不能把真实错误丢掉；成功响应才按激活状态隔离。
      setAppError(toAppError(caught), targetWorktreeId);
      return null;
    }
  }, [scheduleReviewRecovery, setAppError]);

  /**
   * ⌥⌘R：只标记、不取消；按当前列表顺序（未暂存 → 未跟踪 → 已暂存）去重，
   * 从当前文件之后找第一个未审阅文件，末尾环回；必要时展开折叠组。
   * 全部已审阅时留在当前文件并提示（Q37、Q94、D04）。
   */
  const reviewNextFile = useCallback(async () => {
    const current = snapshotRef.current;
    const active = selectedRef.current;
    const targetWorktreeId = worktreeId;
    if (!current || !active || !targetWorktreeId) return;
    const targetActivation = gateRef.current.token().activation;
    const targetRevision = current.revision;
    // 绑定已有的选择代际与选择 gate：新选择一发起（哪怕还没显示）旧导航立即失效。
    const targetSelectionEpoch = selectionEpochRef.current;
    const targetSelection = selectionGateRef.current.currentContext();
    const activeKey = `${active.changes}:${active.path}`;
    setReviewNotice(null);

    // 去重后的显示顺序（`has` 判断后 `add`，`Set.add` 的返回值是 Set，不能当布尔用）。
    const uniqueEntries = (entries: ChangeEntry[]): ChangeEntry[] => {
      const seen = new Set<string>();
      const unique: ChangeEntry[] = [];
      for (const entry of entries) {
        if (seen.has(entry.path)) continue;
        seen.add(entry.path);
        unique.push(entry);
      }
      return unique;
    };

    const targetChangelistMode = changelistModeRef.current;
    const targetChangelistRevision = targetChangelistMode ? changelistRef.current?.revision : null;
    const unique = uniqueEntries(changelistModeRef.current ? entryOrder(current) : getDisplayOrderEntries(current));
    if (unique.length === 0) return;
    const activeEntry = unique.find((entry) => entry.path === active.path);

    // 当前文件本身未审阅：先标记它；标记失败则不导航（错误已经显示）。
    // 版本无法确认时 `setReviewMark` 会在本地拒绝，这里不跳过、不改导航顺序。
    let marks: ReviewMarks | null = null;
    if (activeEntry && !activeEntry.reviewed) {
      marks = await setReviewMark(activeEntry, true);
      if (!marks) return;
    }

    // 导航必须绑定发起时的上下文：等待期间切换 worktree、刷新或另选文件都作废。
    const stillSameContext = () => worktreeIdRef.current === targetWorktreeId
      && changelistModeRef.current === targetChangelistMode
      && (!targetChangelistMode || changelistRef.current?.revision === targetChangelistRevision)
      && gateRef.current.token().activation === targetActivation
      && snapshotRef.current?.revision === targetRevision
      // 选择代际：点击尚未加载指标的 B、或 A→B→A，都会在这里被判为失效。
      && selectionEpochRef.current === targetSelectionEpoch
      && targetSelection !== null
      && selectionGateRef.current.currentContext()?.epoch === targetSelection.epoch
      && selectedRef.current !== null
      && `${selectedRef.current.changes}:${selectedRef.current.path}` === activeKey;
    if (!stillSameContext()) return;

    // 用**成功响应**里的合成真值计算哪些已审阅，不依赖 React 是否已经 render 出新快照。
    //
    // 不能无条件把发起时的当前文件补成已审：它可能已经被更晚的操作取消
    // （⌥⌘R 标 A → B 的结果先显示 → 用户取消 A → 旧 A 回执释放，合成真值是 A 未审）。
    // 那样会误报「全部已审阅」并跳过真正的未审文件（SP07-R12-1）。
    const reviewedFromServer = marks
      ? reviewedSetForNavigation({ view: marks, navigation: marks })
      : new Set(current.review.reviewed);
    if (!reviewedFromServer) return;
    const from = unique.findIndex((entry) => entry.path === active.path);
    for (let offset = 1; offset <= unique.length; offset += 1) {
      const index = (Math.max(from, 0) + offset) % unique.length;
      const candidate = unique[index];
      if (reviewedFromServer.has(candidate.path)) continue;
      if (!stillSameContext()) return;
      if (candidate.collapsed) {
        setExpanded((values) => new Set(values).add(`${candidate.changes}:${candidate.path}`));
      }
      await openEntry(candidate, { view: viewRef.current, worktreeId: targetWorktreeId });
      return;
    }
    // 全部已审阅：留在当前文件并提示，并记下归属 worktree 与当时的进度。
    setReviewNotice({
      worktreeId: targetWorktreeId,
      reviewedCount: reviewedFromServer.size,
      total: unique.length,
    });
    debugLog(`审阅：${messages.review.allReviewed}`);
  }, [entryOrder, openEntry, setReviewMark, worktreeId]);

  const changeHunk = useCallback((action: HunkAction) => {
    const current = selectedRef.current;
    const entry = snapshotRef.current?.entries.find(
      (entry) => current && entry.path === current.path && entry.changes === current.changes,
    );
    if (!entry || !matchesHunkAction(action, worktreeIdRef.current ?? "", entry)) {
      setAppError(localizedError("rejected", "changes.fileChanged"), action.worktreeId);
      if (worktreeIdRef.current === action.worktreeId) void refresh();
      return;
    }
    // 原样传递显示行绑定的目标，不从当前文件元数据重新拼装 hunk 凭据。
    void performWrite(action);
  }, [performWrite, refresh, setAppError]);

  /** 段（块）操作目标：凭证与签发时的忽略空白档位原样传回（Q140、Q142）。 */
  const segmentTarget = useCallback(
    (action: SegmentAction): StagingTarget => ({
      worktreeId: action.worktreeId,
      path: action.path,
      changes: action.changes,
      fingerprint: action.fingerprint,
      segment: {
        hunkIndex: action.hunkIndex,
        segmentIndex: action.segmentIndex,
        ignoreWhitespace: action.ignoreWhitespace,
        credential: action.credential,
      },
    }),
    [],
  );

  /** 暂存或取消暂存一段（块）：段与显示同批签发，切换文件后不再解释。 */
  const changeSegment = useCallback(
    (action: SegmentAction) => {
      const current = selectedRef.current;
      if (
        !current
        || current.path !== action.path
        || current.changes !== action.changes
        || worktreeIdRef.current !== action.worktreeId
      ) {
        setAppError(localizedError("rejected", "changes.fileChanged"), action.worktreeId);
        void refresh();
        return;
      }
      void performWrite(segmentTarget(action));
    },
    [performWrite, refresh, segmentTarget, setAppError],
  );

  /** 丢弃一段（块）：先确认，再先存回收区、后写工作区。 */
  const requestDiscardSegment = useCallback(
    (action: SegmentAction) => {
      const current = selectedRef.current;
      const entry = snapshotRef.current?.entries.find(
        (candidate) => candidate.path === action.path && candidate.changes === action.changes,
      );
      if (!current || !entry || current.path !== action.path || current.changes !== action.changes) {
        setAppError(localizedError("rejected", "changes.fileChanged"), action.worktreeId);
        return;
      }
      setDiscardRequest({
        targets: [{ path: action.path, changes: action.changes, section: messages.changes.discardSegment }],
        entries: [entry],
        worktree: snapshotRef.current?.worktreePath ?? "",
        snapshotRevision: action.snapshotRevision,
        activation: gateRef.current.token().activation,
        segment: segmentTarget(action),
      });
    },
    [segmentTarget, setAppError],
  );

  /** 丢弃一个或多个文件：先弹一次确认，再逐个先存回收区、后写工作区（Q120、F03.6）。 */
  const requestDiscardEntries = useCallback((entries: ChangeEntry[]) => {
    const current = snapshotRef.current;
    const activeWorktree = worktreeIdRef.current;
    if (!current || !activeWorktree) return;
    const listSelection = changelistSelectedPathsRef.current;
    const paths = entries.some((entry) => entry.changes === "changelist") && listSelection.worktreeId === activeWorktree && listSelection.paths.length
      ? listSelection.paths : entries.map((entry) => entry.path);
    const sourceEntries = entries.some((entry) => entry.changes === "changelist") ? current.entries.filter((entry) => paths.includes(entry.path)) : entries;
    const candidates = sourceEntries.filter(
      (entry) => entry.changes === "unstaged" || entry.changes === "untracked",
    );
    const discardable = candidates.filter((entry) => !entry.readonlyReason);
    if (discardable.length === 0) {
      notify({
        tone: "warning",
        title: () => (messages.changes.nothingToDiscard),
        detail: () => candidates
          .filter((entry) => entry.readonlyReason)
          .map((entry) => `${entry.path}: ${readonlyReasonText(entry.readonlyReason!)}`)
          .join("\n") || undefined,
        persistent: true,
      });
      return;
    }
    setDiscardRequest({
      targets: discardable.map((entry) => ({ path: entry.path, changes: entry.changes })),
      entries: discardable,
      worktree: current.worktreePath,
      snapshotRevision: discardable[0].snapshotRevision,
      activation: gateRef.current.token().activation,
    });
  }, [notify]);

  const discardHunk = useCallback((action: HunkAction, section: string) => {
    const current = selectedRef.current;
    const entry = snapshotRef.current?.entries.find((entry) => entry.path === current?.path && entry.changes === current.changes);
    if (!entry || !matchesHunkAction(action, worktreeIdRef.current ?? "", entry)) {
      setAppError(localizedError("rejected", "changes.fileChanged"), action.worktreeId);
      return;
    }
    setDiscardRequest({
      targets: [{ path: action.path, changes: action.changes, section }],
      entries: [entry],
      worktree: snapshotRef.current?.worktreePath ?? "",
      snapshotRevision: action.snapshotRevision,
      activation: gateRef.current.token().activation,
      hunk: action,
    });
  }, [setAppError]);

  const discardRequestValid = useCallback((request: DiscardRequestState) => {
    // 丢弃确认只绑定 worktree 激活与文件指纹，不绑定选择代际：文件选择、自动刷新
    // 都会推进选择代际，但目标文件没变时确认不应失效（返修 B1）。真正的内容变化
    // 由下面的 fingerprint / snapshotRevision 比对与后端写前复核兜住。
    if (request.activation !== gateRef.current.token().activation) return false;
    const current = snapshotRef.current;
    if (!current || worktreeIdRef.current === null) return false;
    if (request.hunk || request.segment) {
      const target = request.hunk ?? request.segment!;
      return current.entries.some(
        (entry) => entry.path === target.path && entry.changes === target.changes
          && entry.fingerprint === target.fingerprint
          && entry.snapshotRevision === request.snapshotRevision,
      ) && !writingWorktreesRef.current.has(target.worktreeId);
    }
    if (writingWorktreesRef.current.has(worktreeIdRef.current)) return false;
    return request.entries.every((target) =>
      current.entries.some(
        (entry) => entry.path === target.path && entry.changes === target.changes
          && entry.fingerprint === target.fingerprint
          && entry.snapshotRevision === request.snapshotRevision,
      ),
    );
  }, []);

  // 快捷键（Q19、Q93、Q199）：默认加速键与自定义覆盖都在 lib/shortcuts.ts 里，
  // 这里只按“命中的动作”分派，避免平台判断散落在 App 里（Q55）。
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // 输入法组合与 AltGr 字符输入不触发后台快捷键（M8 任务 04）。
      if (isImeComposing(event) || isAltGraph(event)) return;
      const action = matchShortcut(event);

      // 提交：在提交信息框里由信息框自己处理，其它输入控件与弹窗内不穿透（Q93）。
      if (action === "commit" && !isCommitMessageTarget(event.target)) {
        if (isTextInputOrDialog(event.target) || document.querySelector("[role='dialog'], dialog[open]")) return;
        event.preventDefault();
        void performCommit();
        return;
      }

      // 只对文本输入控件和弹窗内的按键放行，不穿透到后台文件操作（Q93、审查意见 6）
      if (isTextInputOrDialog(event.target)) return;
      if (!action) return;

      switch (action) {
        case "discardFile": {
          event.preventDefault();
          if (isWriting) return;
          const targets = treeSelectedEntries();
          requestDiscardEntries(targets.length > 0 ? targets : selectedEntry ? [selectedEntry] : []);
          return;
        }
        case "reviewNextFile": {
          // 焦点在按钮/勾选框上时不穿透：那类控件用自己的键盘行为（spec 决策 7）。
          if (isButtonTarget(event.target)) return;
          event.preventDefault();
          void reviewNextFile();
          return;
        }
        case "stageFile": {
          event.preventDefault();
          if (isWriting) return;
          const targets = treeSelectedEntries();
          const entries = targets.length > 0 ? targets : selectedEntry ? [selectedEntry] : [];
          // 不过滤只读条目：批量入口会让后端逐个返回跳过原因（返修 C6，F03.5）。
          const staged = entries.filter((entry) => entry.changes === "staged");
          const unstaged = entries.filter((entry) => entry.changes !== "staged");
          // 选中项混有两组时串行执行：同一 worktree 的写门只放一个（返修 C5）。
          const runSequentially = async () => {
            if (unstaged.length > 0) await performBatchWrite("stage", unstaged);
            if (staged.length > 0) await performBatchWrite("unstage", staged);
            if (unstaged.length === 0 && staged.length === 0 && entries.length > 0) {
              notify({ tone: "warning", title: () => (messages.changes.batchNothing), persistent: true });
            }
          };
          void runSequentially();
          return;
        }
        case "zoomIn":
        case "zoomOut":
        case "zoomReset": {
          event.preventDefault();
          const nextZoom =
            action === "zoomReset" ? 100 : Math.min(150, Math.max(80, Math.round(uiZoomRef.current / 10) * 10 + (action === "zoomIn" ? 10 : -10)));
          setUiZoom(nextZoom);
          void ipc.updateSettings({ uiZoom: nextZoom }).catch(() => {});
          return;
        }
        case "openRepo":
          event.preventDefault();
          void chooseRepo();
          return;
        case "cloneRepo":
          event.preventDefault();
          if (!busy) setCloneOpen(true);
          return;
        case "pullRequests":
          event.preventDefault();
          if (!busy && worktreeId) setGithubOpen(true);
          return;
        case "refresh":
          event.preventDefault();
          void refresh();
          return;
        case "fontUp":
          event.preventDefault();
          void changeFontSize(1);
          return;
        case "fontDown":
          event.preventDefault();
          void changeFontSize(-1);
          return;
        case "nextHunk":
        case "prevHunk":
          // 合并页签里的 F7 由合并页签自己处理（上一处、下一处冲突）。
          if (mergeTabPathRef.current) return;
          event.preventDefault();
          jumpToHunk(action === "nextHunk" ? 1 : -1);
          return;
        case "nextFile":
          event.preventDefault();
          focusNextFile(1);
          return;
        case "prevFile":
          event.preventDefault();
          focusNextFile(-1);
          return;
        case "toggleTerminal":
          event.preventDefault();
          if (worktreeId) setTerminalOpen((value) => !value);
          return;
        default:
          return;
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [busy, changeFontSize, chooseRepo, focusNextFile, isWriting, jumpToHunk, notify, performBatchWrite, performCommit, refresh, requestDiscardEntries, reviewNextFile, selectedEntry, treeSelectedEntries, worktreeId]);

  // ---------------- 测量 ----------------

  // x18n-ignore-start
  const runMeasurement = useCallback(
    async (
      targetView: ViewMode,
      targetWorktreeId: string,
      targetPath: string | null,
      configPath: string | null,
    ): Promise<Measurement | null> => {
      debugLog(`测量开始：view=${targetView} path=${targetPath ?? "(默认)"}`);
      setView(targetView);
      const opened = await openWorktree(targetWorktreeId, {
        targetPath: targetPath ?? configPath,
        view: targetView,
        expand: true,
        activateDiff: true,
      });
      if (!opened) return null;
      const entry = opened.entry;
      firstScreenRef.current = {
        fromListChangesMs: opened.firstScreenMs,
        fromOpenWorktreeMs: opened.openMs,
        listChangesMs: opened.listChangesMs,
        firstRowsMs: opened.firstRowsMs,
        renderMs: opened.firstRowsMs,
        rowCount: opened.rowCount,
        fileCount: opened.fileCount,
        rafPainted: opened.rafPainted,
        viewportHeight: opened.viewportHeight,
        devicePixelRatio: opened.devicePixelRatio,
      };

      const total = targetView === "split" ? entry.rowCountSplit : entry.rowCountUnified;
      const element = document.querySelector<HTMLElement>(".diff-scroll");
      const viewportRows = element ? Math.ceil(element.clientHeight / ROW_HEIGHT) : DEFAULT_VIEWPORT_ROWS;
      const rafPerSecond = await probeRaf(1);
      const hasCompositor = rafPerSecond > 5;
      const visibility = visibilitySample(rafPerSecond);

      // 并排视图（返修 A1）：行来自 side_lines，旧的行窗口探针不适用，
      // 这里单独测跳转、滚动覆盖与两栏同步误差（D06 门槛不变）。
      if (targetView === "split") {
        const handle = sbsRef.current;
        const viewportHeight = handle?.getViewportHeight() ?? 0;
        const contentHeight = handle?.getContentHeight() ?? 0;
        const segmentCount = handle?.segmentCount() ?? 0;
        const visibleCovered = () => {
          const current = sbsRef.current;
          if (!current) return false;
          return (["left", "right"] as const).every((side) => {
            const visible = current.getVisibleRange(side);
            return current.isCovered(visible.from, visible.to, side);
          });
        };
        const measureJump = async (top: number) => {
          const cachedBeforeJump = handle?.captureCoverage();
          const started = performance.now();
          handle?.setScrollTop(top);
          // 跳转同步提交行窗口：调用返回时已有内容，就在这一帧画出（口径同统一视图）。
          const withinOneFrame = visibleCovered();
          const targetWasCached = cachedBeforeJump && handle ? (["left", "right"] as const).every((side) => {
            const visible = handle.getVisibleRange(side);
            return cachedBeforeJump(visible.from, visible.to, side);
          }) : null;
          const covered = await waitUntil(visibleCovered, 4000);
          const dataAt = performance.now();
          await nextPaint();
          return { appearMs: performance.now() - started, dataAt: dataAt - started, covered, withinOneFrame, targetWasCached };
        };
        const cold = await measureJump(Math.floor(contentHeight * 0.6));
        const warm = await measureJump(0);
        let uncoveredFrames = 0;
        let uncoveredMax = 0;
        const frames: number[] = [];
        let last = performance.now();
        let syncErrorMax = 0;
        let syncErrorLast = 0;
        const column = document.querySelector<HTMLElement>(".sbs-column");
        for (let step = 0; step < 60; step += 1) {
          column?.dispatchEvent(new WheelEvent("wheel", { deltaY: 320, bubbles: true, cancelable: true }));
          await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
          const now = performance.now();
          frames.push(now - last);
          last = now;
          const current = sbsRef.current;
          if (!current) continue;
          let missing = 0;
          for (const side of ["left", "right"] as const) {
            const visible = current.getVisibleRange(side);
            for (let row = visible.from; row < visible.to; row += 1) {
              if (!current.isCovered(row, row + 1, side)) missing += 1;
            }
          }
          if (missing > 0) uncoveredFrames += 1;
          uncoveredMax = Math.max(uncoveredMax, missing);
          const error = current.syncError();
          syncErrorLast = error;
          syncErrorMax = Math.max(syncErrorMax, error);
        }
        handle?.setScrollTop(0);
        const splitMeasurement: Measurement = {
          id: measurements.length + 1,
          label: `${targetView} / ${entry.path}`,
          startedAt: nowStamp(),
          view: targetView,
          path: entry.path,
          visibility,
          firstScreen: firstScreenRef.current,
          jump: null,
          scroll: null,
          jumpDom: null,
          jumpDomError: "并排视图不使用旧的行窗口探针",
          highlight: null,
          split: {
            syncErrorMax,
            syncErrorLast,
            viewportHeight,
            contentHeight,
            segments: segmentCount,
            jumpAppearMs: cold.appearMs,
            jumpTargetWasCached: cold.targetWasCached,
            warmJumpTargetWasCached: warm.targetWasCached,
            warmJumpAppearMs: warm.appearMs,
            jumpWithinOneFrame: cold.withinOneFrame,
            warmJumpWithinOneFrame: warm.withinOneFrame,
            scrollFrames: frames.length,
            uncoveredFrames,
            uncoveredMax,
            frameP50Ms: percentile(frames, 0.5),
            frameP95Ms: percentile(frames, 0.95),
          },
        };
        flushSynchronousMeasurements();
        setMeasurements((current) => [splitMeasurement, ...current]);
        return splitMeasurement;
      }

      // T2：大幅跳转
      let jump: JumpSample | null = null;
      if (element && total > viewportRows * 4) {
        const from = Math.floor((diffRef.current?.getScrollTop() ?? 0) / ROW_HEIGHT);
        const to = Math.floor(total * 0.6);
        const targetWasCached = rowStore.isCovered(to, to + viewportRows);
        let started = 0;
        let prioritizedAt = 0;
        let scrollReturnedAt = 0;
        const observedOutward = observeJumpDom({
          view: targetView, rowHeight: ROW_HEIGHT, rows: rowStore, highlights: highlightStore,
          getScroller: () => diffRef.current, toRow: to, checkColors: false,
          jump: () => {
            started = performance.now();
            rowStore.prioritize(to, to + viewportRows);
            prioritizedAt = performance.now();
            diffRef.current?.scrollToRow(to);
            scrollReturnedAt = performance.now();
          },
        });
        const [legacyOutward, outwardDom] = await Promise.all([
          (async () => {
            const coveredTarget = await waitUntil(() => rowStore.isCovered(to, to + viewportRows), 4000);
            const dataAt = performance.now();
            await nextTask();
            return { coveredTarget, dataAt, taskAt: performance.now() };
          })(),
          observedOutward,
        ]);
        const { coveredTarget, dataAt, taskAt } = legacyOutward;
        const appearMs = taskAt - started;
        debugLog(
          `跳转目标齐备：${coveredTarget}（取数 ${Math.round(dataAt - started)} ms，提交 ${Math.round(appearMs)} ms）`,
        );
        // 保留 M1 总计时；分段记录同步调用与消息任务等待，不能混称纯渲染。
        let warmAppearMs: number | null = null;
        let colorRestoreMs: number | null = null;
        const warmTargetWasCached = rowStore.isCovered(from, from + viewportRows);
        let warmStart = 0;
        let warmScrollReturnedAt = 0;
        // 同次首帧已核验后才返回，避免在同一渲染机会前用回跳覆盖外跳画面。
        const observedReturn = observeJumpDom({
          view: targetView, rowHeight: ROW_HEIGHT, rows: rowStore, highlights: highlightStore,
          getScroller: () => diffRef.current, toRow: from, checkColors: false,
          jump: () => {
            warmStart = performance.now();
            diffRef.current?.scrollToRow(from);
            warmScrollReturnedAt = performance.now();
          },
        });
        const [legacyReturn, returnDom] = await Promise.all([
          (async () => {
            const warmCovered = await waitUntil(() => rowStore.isCovered(from, from + viewportRows), 4000);
            const warmDataAt = performance.now();
            await nextTask();
            return { warmCovered, warmDataAt, warmTaskAt: performance.now() };
          })(),
          observedReturn,
        ]);
        const { warmCovered, warmDataAt, warmTaskAt } = legacyReturn;
        if (warmCovered) warmAppearMs = warmTaskAt - warmStart;
        // 等待颜色与余量恢复（compact 复位到 false，约 2 帧内恢复）
        const colorRestored = await waitUntil(() => diffRef.current != null && !diffRef.current.isCompact(), 2000);
        if (colorRestored) colorRestoreMs = performance.now() - warmStart;
        debugLog(
          `热态跳转：齐备 ${warmCovered}，文字提交 ${warmAppearMs == null ? "?" : Math.round(warmAppearMs)} ms，颜色恢复 ${colorRestoreMs == null ? "?" : Math.round(colorRestoreMs)} ms`,
        );
        jump = {
          fromRow: from,
          toRow: to,
          targetWasCached,
          warmTargetWasCached,
          targetCovered: coveredTarget,
          warmCovered,
          timing: {
            prioritizeMs: prioritizedAt - started,
            scrollCallMs: scrollReturnedAt - prioritizedAt,
            dataWaitMs: dataAt - scrollReturnedAt,
            taskQueueMs: taskAt - dataAt,
          },
          warmTiming: {
            prioritizeMs: 0,
            scrollCallMs: warmScrollReturnedAt - warmStart,
            dataWaitMs: warmDataAt - warmScrollReturnedAt,
            taskQueueMs: warmTaskAt - warmDataAt,
          },
          appearMs,
          fetchMs: dataAt - started,
          withinOneFrame: appearMs <= 16.7,
          warmAppearMs,
          warmWithinOneFrame: warmAppearMs != null ? warmAppearMs <= 16.7 : null,
          colorRestoreMs,
          sameTargetDom: { outward: outwardDom, return: returnDom },
        };
      }

      // T3/T4：快速滚动
      let scroll: ScrollSample | null = null;
      const scroller = diffRef.current;
      if (scroller) {
        scroll = await measureScroll({
          scroller,
          screens: 60,
          hasCompositor,
          uncovered: () => {
            const visible = diffRef.current?.getVisibleRange();
            if (!visible || visible.to <= visible.from) throw new Error("滚动测量缺少有效行窗口");
            return rowStore.uncovered(visible.from, visible.to);
          },
        });
        scroller.scrollToRow(0);
        debugLog(`滚动测量完成：${scroll?.screens ?? 0} 屏，未覆盖帧 ${scroll?.uncoveredFrames ?? "?"}`);
      }

      const highlight = highlightStore.getSnapshot();
      let jumpDom: Measurement["jumpDom"] = null;
      let jumpDomError: string | null = null;
      try {
        if (jump) jumpDom = await diagnoseJumpDom({
          view: targetView,
          rowHeight: ROW_HEIGHT,
          rows: rowStore,
          highlights: highlightStore,
          getScroller: () => diffRef.current,
        });
      } catch (caught) {
        // Keep the original samples even when the separate diagnostic fails.
        jumpDomError = getErrorMessage(caught);
      }
      const measurement: Measurement = {
        id: measurements.length + 1,
        label: `${targetView} / ${entry.path}`,
        startedAt: nowStamp(),
        view: targetView,
        path: entry.path,
        visibility,
        firstScreen: firstScreenRef.current,
        jump,
        scroll,
        jumpDom,
        jumpDomError,
        highlight: {
          roundTrips: highlight.loadedLines,
          lastRoundTripMs: highlight.lastRoundTripMs,
          loadedLines: highlight.loadedLines,
        },
      };
      setMeasurements((current) => [measurement, ...current]);
      return measurement;
    },
    [highlightStore, measurements.length, openWorktree, rowStore],
  );

  /** 只读界面冒烟检查：打开仓库 → 逐个文件 → 切换视图，把结果写进测量日志。 */
  const runSmoke = useCallback(
    async (config: MeasureConfig) => {
      const report: Record<string, unknown> = {};
      try {
        if (config.repoPath) await ipc.addRepo(config.repoPath);
        const list = await ipc.listRepos();
        // 指定了测量仓库时优先选它，避免共享数据目录里其它仓库排在首位。
        const repoPath = config.repoPath;
        const repo = (repoPath
          ? list.find((entry) => entry.worktrees.some(
            (worktree) => worktree.path === repoPath
              || worktree.path.replace("/private", "") === repoPath.replace("/private", ""),
          ))
          : undefined) ?? list[0];
        setRepos(list);
        setActiveRepo(repo);
        report.repo = { id: repo.id, name: repo.name, worktrees: repo.worktrees.map((w) => ({ id: w.id, path: w.path, branch: w.branch, main: w.isMain })) };
        const worktree = repo.worktrees.find((entry) => !entry.bare) ?? repo.worktrees[0];
        setWorktreeId(worktree.id);
        const opened = await ipc.openWorktree(worktree.id);
        report.session = {
          gitPath: opened.gitPath,
          gitVersion: opened.gitVersion,
          shell: opened.shell,
          shellElapsedMs: opened.shellElapsedMs,
          checks: opened.checks,
        };
        const snapshot = await ipc.listChanges(worktree.id);
        setSnapshot(snapshot);
        report.changes = {
          branch: snapshot.branch,
          fingerprint: snapshot.fingerprint,
          elapsedMs: snapshot.elapsedMs,
          unstaged: snapshot.unstagedCount,
          staged: snapshot.stagedCount,
          untracked: snapshot.untrackedCount,
          entries: snapshot.entries.map((entry) => ({
            path: entry.path,
            changes: entry.changes,
            status: entry.status,
            added: entry.added,
            deleted: entry.deleted,
            hunks: entry.hunkCount,
            binary: entry.binary,
            collapsed: entry.collapsed,
            rowsSplit: entry.rowCountSplit,
            rowsUnified: entry.rowCountUnified,
          })),
        };

        // 逐个文件打开：收起、二进制、Unicode、Swift 都要覆盖到
        const visited: unknown[] = [];
        for (const entry of snapshot.entries) {
          const expand = !entry.binary;
          const result = await openEntry(entry, { view: "split", worktreeId: worktree.id, expand });
          const element = document.querySelector<HTMLElement>(".diff-scroll, .sbs-column");
          const rows = element ? Math.ceil(element.clientHeight / ROW_HEIGHT) : 0;
          const first = rowStore.getRow(0);
          visited.push({
            path: entry.path,
            changes: entry.changes,
            expanded: expand,
            covered: result.covered,
            viewportRows: rows,
            firstRowKind: first?.kind ?? null,
            firstRowText: (first?.text ?? first?.left?.text ?? first?.right?.text ?? "").slice(0, 40),
            firstRowLine: first?.oldLine ?? first?.left?.line ?? first?.newLine ?? first?.right?.line ?? null,
          });
          await nextPaint();
        }
        report.visited = visited;

        // 视图切换：通过工具栏按钮走真实交互路径（并排 → 统一 → 并排），检查锚点行是否保持。
        const target = snapshot.entries.find((entry) => entry.path === (config.targetPath ?? "src/generated.ts"));
        if (target) {
          await openEntry(target, { view: "split", worktreeId: worktree.id, expand: true });
          await nextPaint();
          await waitUntil(() => (sbsRef.current?.getViewportHeight() ?? 0) > 0, 4000);
          const handle = sbsRef.current;
          handle?.setScrollTop(handle.getContentHeight() * 0.4);
          await nextPaint();
          const beforeAnchor = handle?.getFileAnchor() ?? null;

          selectViewInUi("unified");
          const unifiedMatches = await waitUntil(() => {
            if (viewRef.current !== "unified" || pendingAnchor.current || !beforeAnchor) return false;
            const range = diffRef.current?.getVisibleRange();
            if (!range) return false;
            for (let index = range.from; index < range.to; index += 1) {
              const row = rowStore.getRow(index);
              if (!row || row.kind !== "line") continue;
              return (beforeAnchor.version === "old" ? row.oldLine : row.newLine) === beforeAnchor.line;
            }
            return false;
          }, 4000);
          const unifiedTop = diffRef.current?.getScrollTop() ?? 0;
          const unifiedAnchor = diffRef.current?.getFileAnchor() ?? null;

          selectViewInUi("split");
          const splitMatches = await waitUntil(() => {
            if (viewRef.current !== "split" || pendingAnchor.current || !unifiedAnchor) return false;
            const anchor = sbsRef.current?.getFileAnchor();
            return anchor?.line === unifiedAnchor.line && anchor.version === unifiedAnchor.version;
          }, 4000);
          const splitTop = sbsRef.current?.getScrollTop() ?? 0;
          const splitAnchor = sbsRef.current?.getFileAnchor() ?? null;

          report.viewSwitch = {
            anchorLineBefore: beforeAnchor?.line ?? null,
            anchorVersionBefore: beforeAnchor?.version ?? null,
            unifiedRows: target.rowCountUnified,
            splitRows: target.rowCountSplit,
            unifiedAnchorLine: unifiedAnchor?.line ?? null,
            unifiedAnchorVersion: unifiedAnchor?.version ?? null,
            splitAnchorLine: splitAnchor?.line ?? null,
            splitAnchorVersion: splitAnchor?.version ?? null,
            unifiedScrollTop: Math.round(unifiedTop),
            splitScrollTop: Math.round(splitTop),
            sameLineAfterRoundTrip: unifiedMatches && splitMatches,
          };
        }

        // 返修 A1：并排视图有左栏、窄栏、右栏，且窄栏 canvas 有内容。
        const splitTarget = snapshot.entries.find((entry) => entry.hunkCount > 0 && !entry.binary && !entry.unmerged);
        if (splitTarget) {
          debugLog(`冒烟：并排视图检查开始 ${splitTarget.path}`);
          await openEntry(splitTarget, { view: "split", worktreeId: worktree.id, expand: true });
          // 色带在两栏量高、布局提交之后才画：等到画布出现非透明像素，而不是固定等几次 RAF。
          await waitUntil(() => {
            const element = document.querySelector<HTMLCanvasElement>(".sbs-bands");
            if (!element) return false;
            const context = element.getContext("2d");
            if (!context) return false;
            const data = context.getImageData(0, 0, Math.max(1, element.width), Math.max(1, element.height)).data;
            for (let index = 3; index < data.length; index += 4) {
              if (data[index] !== 0) return true;
            }
            return false;
          }, 4000);
          const columns = document.querySelectorAll(".sbs-column").length;
          const canvas = document.querySelector<HTMLCanvasElement>(".sbs-bands");
          let canvasPainted = false;
          if (canvas) {
            const context = canvas.getContext("2d");
            const data = context?.getImageData(0, 0, Math.max(1, canvas.width), Math.max(1, canvas.height)).data;
            if (data) {
              for (let index = 3; index < data.length; index += 4) {
                if (data[index] !== 0) {
                  canvasPainted = true;
                  break;
                }
              }
            }
          }
          debugLog(`冒烟：并排视图检查完成 columns=${columns} painted=${canvasPainted}`);
          report.sideBySide = {
            columns,
            numbers: document.querySelectorAll(".sbs-numbers").length,
            canvasPainted,
            canvasWidth: canvas?.width ?? 0,
          };
        }

        // 返修 F10：冲突文件打开合并页签，检查三栏与可编辑的中栏。
        const conflictEntry = snapshot.entries.find((entry) => entry.unmerged);
        if (conflictEntry && openMergeTabRef.current) {
          debugLog(`冒烟：合并页签检查开始 ${conflictEntry.path}`);
          openMergeTabRef.current(conflictEntry);
          await nextPaint();
          await waitUntil(() => !!document.querySelector("[data-merge-view]"), 4000);
          // 中栏是 CodeMirror：从编辑器状态读全文，不依赖只渲染可见行的 DOM。
          const resultEditor = document.querySelector<HTMLElement>("[data-merge-pane='result'] .cm-editor");
          const result = resultEditor ? EditorView.findFromDOM(resultEditor)?.state.doc.toString() ?? null : null;
          debugLog(`冒烟：合并页签检查完成 view=${!!document.querySelector("[data-merge-view]")}`);
          report.merge = {
            view: !!document.querySelector("[data-merge-view]"),
            resultLines: result === null ? 0 : result.split("\n").length,
            hasConflictMarkers: result === null ? false : result.includes("<<<<<<<"),
            conflictCountText: document.querySelector("[data-merge-conflicts]")?.textContent ?? null,
            conflictGroup: document.querySelectorAll(".section-header").length,
          };
        }

        debugLog("冒烟：界面骨架检查开始");
        // 新界面骨架：页签、改动树、提交区与状态栏都按真实 DOM 检查（M4 F02–F05）。
        const tabNodes = [...document.querySelectorAll<HTMLElement>("[data-tab-key]")];
        const firstRow = document.querySelector<HTMLElement>(".change-row");
        const activeTab = () => document.querySelector<HTMLElement>('[data-tab-key][data-active="true"]');
        const ui: Record<string, unknown> = {
          tabs: tabNodes.map((node) => ({
            key: node.dataset.tabKey ?? null,
            active: node.dataset.active === "true",
            preview: node.dataset.preview === "true",
          })),
          treeRows: document.querySelectorAll(".change-row").length,
          groupHeaders: document.querySelectorAll(".section-header").length,
          sidebarVisible: !!document.querySelector(".changes-tree"),
          commitArea: !!document.querySelector("[data-commit-composer]"),
          commitMessage: !!document.querySelector("[data-commit-message]"),
          activityBadge: document.querySelector(".activity-badge")?.textContent ?? null,
          // 玻璃材质（M7 任务 01）：确认三个组件真的挂载，而不是只写了 CSS。
          glass: {
            rootChildren: document.getElementById("root")?.childElementCount ?? -1,
            bodyText: document.body.innerText.replace(/\s+/g, " ").slice(0, 120),
            windowLayer: !!document.querySelector(".glass-host > .glass-layer--window"),
            popoverLayer: document.querySelectorAll(".glass-layer--popover").length,
            panelLayers: {
              sidebar: !!document.querySelector(".changes-sidebar"),
              status: !!document.querySelector(".window-status"),
              toolbar: !!document.querySelector(".titlebar"),
            },
            glassButtons: document.querySelectorAll(".glass-button").length,
            specularButtons: document.querySelectorAll(".specular-button").length,
            refractionEnabled: Array.from(document.querySelectorAll(".glass-surface")).filter(
              (node) => !node.classList.contains("glass-surface--fallback") && !node.classList.contains("glass-layer--flat"),
            ).length,
            tools: document.querySelectorAll(".toolbar-group .toolbar-group-item").length,
            nativeGlass: document.documentElement.classList.contains("native-glass"),
            commitButton: !!document.querySelector(".m7-commit-button"),
          },
          statusBar: document.querySelector("footer")?.textContent?.slice(0, 120) ?? null,
        };
        if (firstRow) {
          firstRow.dispatchEvent(new MouseEvent("click", { bubbles: true }));
          await nextPaint();
          ui.previewAfterClick = activeTab()?.dataset.preview ?? null;
          firstRow.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
          await nextPaint();
          ui.pinnedAfterDoubleClick = activeTab()?.dataset.preview ?? null;
        }
        // 布局尺寸与四套主题的实际取值（验收时用于对照效果图与 VS Code）。
        const rect = (selector: string) => {
          const element = document.querySelector<HTMLElement>(selector);
          if (!element) return null;
          const box = element.getBoundingClientRect();
          return { x: Math.round(box.x), y: Math.round(box.y), w: Math.round(box.width), h: Math.round(box.height) };
        };
        ui.viewport = {
          innerWidth: window.innerWidth,
          innerHeight: window.innerHeight,
          zoom: getComputedStyle(document.documentElement).zoom,
          uiZoomVar: getComputedStyle(document.documentElement).getPropertyValue("--ui-zoom").trim(),
          dpr: window.devicePixelRatio,
          docScrollHeight: document.documentElement.scrollHeight,
        };
        ui.layout = {
          titlebar: rect(".titlebar"),
          activityBar: rect(".activity-bar"),
          sidebar: rect("aside"),
          tab: rect(".editor-tab"),
          statusBar: rect("footer"),
          commitArea: rect("[data-commit-composer]"),
          treeRow: rect(".change-row"),
          diff: rect(".diff-scroll"),
        };
        const main = document.querySelector<HTMLElement>("main");
        const previousTheme = document.documentElement.dataset.theme;
        const previousKind = document.documentElement.dataset.kind;
        ui.themes = ["one-light", "light-2026", "one-dark-pro", "dark-2026"].map((id) => {
          document.documentElement.dataset.theme = id;
          document.documentElement.dataset.kind = id.includes("dark") ? "dark" : "light";
          return {
            id,
            editorBackground: main ? getComputedStyle(main).backgroundColor : null,
            foreground: getComputedStyle(document.body).color,
            syntaxKeyword: getComputedStyle(document.documentElement).getPropertyValue("--syn-keyword").trim(),
          };
        });
        if (previousTheme) document.documentElement.dataset.theme = previousTheme;
        if (previousKind) document.documentElement.dataset.kind = previousKind;
        // M7 命令通路（任务 02–04、09、11）：走真实 IPC，确认 Rust 侧能返回而不 panic。
        const m7Probe: Record<string, unknown> = {};
        const attempt = async (name: string, work: () => Promise<unknown>) => {
          try {
            m7Probe[name] = await work();
          } catch (error) {
            m7Probe[name] = { error: String(error) };
          }
        };
        await attempt("importedThemes", async () => (await m7.listImportedThemes()).length);
        await attempt("updateStatus", async () => {
          const status = await m7.updateStatus();
          return { mode: status.mode, configured: status.configured, reason: status.unconfiguredReason };
        });
        await attempt("terminalTabs", async () => (await m7.terminalTabs()).length);
        await attempt("diagnostics", async () => {
          const preview = await m7.previewDiagnostics({ destination: "", worktreeId: worktree.id });
          return { chars: preview.text.length, redactions: preview.redactions };
        });
        await attempt("editable", async () => {
          const entry = snapshot.entries.find((candidate) => !candidate.binary && candidate.status !== "deleted");
          if (!entry) return null;
          const file = await m7.openEditable(worktree.id, entry.path);
          return { path: entry.path, readonlyReason: file.readonlyReason ?? null, bytes: file.text?.length ?? 0 };
        });
        // 内置终端（任务 04）：真实开一个 PTY、等首屏输出、再关闭；不向 shell 发任何命令。
        await attempt("terminal", async () => {
          const session = await m7.terminalOpen({ worktreeId: worktree.id, cwd: null, cols: 80, rows: 24 });
          await new Promise((resolve) => setTimeout(resolve, 1500));
          const snapshot = await m7.terminalSnapshot(session.sessionId);
          const running = (await m7.terminalList()).some((item) => item.sessionId === session.sessionId);
          await m7.terminalClose(session.sessionId);
          return { id: session.sessionId, shell: session.shell, running, bytes: snapshot.length };
        });
        report.m7 = m7Probe;
        report.ui = ui;
        report.ok = true;
      } catch (caught) {
        report.ok = false;
        report.error = getErrorMessage(caught);
      }
      await ipc.writeMeasurement(report);
    },
    [openEntry, rowStore, switchView],
  );

  /** 长期运行基线（M2 任务 01，Q53/Q78）：同进程连续循环切换文件、视图并滚动指定时长（默认 30 分钟）。 */
  const runSoak = useCallback(
    async (config: MeasureConfig) => {
      const soakSeconds = config.soakSeconds ?? 1800;
      debugLog(
        `长期运行基线启动：目标时长 ${soakSeconds} 秒（${(soakSeconds / 60).toFixed(1)} 分钟） repo=${config.repoPath}`,
      );
      const invokedAt = performance.now();
      const diagnostic = config.soakDiagnostic;
      const report: Record<string, unknown> = { targetDurationSeconds: soakSeconds,
        diagnostic, minimalSoakLogging: config.minimalSoakLogging,
        acceptance: diagnostic ? "diagnostic only; not baseline acceptance" : "pending review" };
      const monitor = monitorSoakVisibility();
      let diagnostics: ReturnType<typeof startIpcDiagnostics> | null = null;
      const samples: Array<{
        elapsedSec: number;
        cycle: number;
        file: string;
        view: string;
        visibility: SoakVisibility;
        loadedChunks: number;
        highlightLines: number;
        ipc?: IpcDiagnosticSample;
      }> = [];

      try {
        if (!config.repoPath) throw new Error("长测必须指定 fixture 仓库");
        const repo = await ipc.addRepo(config.repoPath);
        const list = await ipc.listRepos();
        setRepos(list);
        setActiveRepo(repo);
        const worktree = repo.worktrees.find((entry) => !entry.bare) ?? repo.worktrees[0];
        if (!worktree) throw new Error("没有找到可用 worktree");
        report.worktree = worktree;
        setWorktreeId(worktree.id);
        const opened = await ipc.openWorktree(worktree.id);
        report.session = {
          gitPath: opened.gitPath,
          gitVersion: opened.gitVersion,
          shell: opened.shell,
          shellElapsedMs: opened.shellElapsedMs,
          checks: opened.checks,
        };
        const snapshot = await ipc.listChanges(worktree.id);
        setSnapshot(snapshot);
        const entries = snapshot.entries;
        if (entries.length === 0) throw new Error("改动列表为空");

        report.files = entries.map(({ path, changes, rowCountSplit, rowCountUnified }) => ({ path, changes, rowCountSplit, rowCountUnified }));
        setStatus("长测等待可见窗口，请将此窗口置于前台");
        const readiness: SoakVisibility[] = [];
        report.readiness = readiness;
        const readyDeadline = performance.now() + 120_000;
        let ready = false;
        while (performance.now() < readyDeadline) {
          const vis = await monitor.sample();
          readiness.push(vis);
          if (!vis.hidden && vis.rafPerSecond > 5) { ready = true; break; }
        }
        if (!ready) throw new Error("120 秒内未获得可见合成窗口");
        monitor.reset();
        setStatus("长测进行中，请保持窗口可见");
        if (diagnostic || config.minimalSoakLogging) {
          diagnostics = startIpcDiagnostics();
          report.ipcInitial = diagnostics.snapshot();
        }
        const startedAt = performance.now();
        report.setupSeconds = (startedAt - invokedAt) / 1000;
        report.operationsStartedAt = new Date().toISOString();
        // 运行锚点：让中断后的增量记录能与本次运行关联。仅走非退出的 debugLog 通道，
        // 不使用 writeMeasurement（它会在 500ms 后结束进程）。
        const soakRunId = `${report.operationsStartedAt}/${config.outputPath ?? ""}`;
        const progress = async (record: Record<string, unknown>) => {
          await ipc.debugLog(`[SOAK_PROGRESS] ${JSON.stringify({ runId: soakRunId, ...record })}`);
        };
        await progress({ kind: "run", targetSeconds: soakSeconds, repo: config.repoPath,
          outputPath: config.outputPath ?? null, setupSeconds: report.setupSeconds });
        let cycle = 0;
        let fileSwitches = 0;
        let viewSwitches = 0;
        let scrolls = 0;
        let jumps = 0;
        let ipcRequests = 0;
        let yieldedTasks = 0;
        let currentView: ViewMode = "split";
        let lastHeartbeat = performance.now();
        let lastCheckpoint = startedAt;
        let checkpointPauseMs = 0;
        let visibilityPauseMs = 0;
        const checkpoints: Array<{ cycle: number; file: string; view: string; pressure: boolean; startedAt: string;
          endedAt?: string; hiddenDuringHold?: boolean; ipcBefore?: IpcDiagnosticSample;
          ipcAfter?: IpcDiagnosticSample }> = [];
        report.phaseCheckpoints = checkpoints;
        const focus = diagnostic?.focus ?? null;

        const pausedMs = () => checkpointPauseMs + visibilityPauseMs;
        const isTimeUp = () => (performance.now() - startedAt - pausedMs()) / 1000 >= soakSeconds;
        const interrupted = (vis: SoakVisibility) =>
          vis.hidden || vis.rafPerSecond <= 5 || vis.hiddenTransitions > 0 || vis.maxFrameGapMs > 2000;
        // 单项诊断只用于定位、不作验收：窗口被遮挡时暂停到恢复可见（最多 10 分钟），记录中断并从主动时长中扣除；
        // 压力阶段静置期间被遮挡的样本由驱动判为无效。正式长测仍在任何遮挡时中止。
        let hiddenSeen = false;
        const visibilityInterruptions: Array<{ elapsedSec: number; pausedSeconds: number; reason: string }> = [];
        if (focus) {
          report.visibilityInterruptions = visibilityInterruptions;
          document.addEventListener("visibilitychange", () => {
            if (document.hidden) hiddenSeen = true;
          });
        }
        const resumeWhenVisible = async (reason: string) => {
          const pauseStarted = performance.now();
          for (;;) {
            const vis = await monitor.sample();
            if (!vis.hidden && vis.rafPerSecond > 5) break;
            if (performance.now() - pauseStarted > 600_000) throw new Error(`窗口 10 分钟未恢复可见：${reason}`);
          }
          monitor.reset();
          hiddenSeen = false;
          visibilityPauseMs += performance.now() - pauseStarted;
          visibilityInterruptions.push({ elapsedSec: Math.round((pauseStarted - startedAt) / 1000),
            pausedSeconds: (performance.now() - pauseStarted) / 1000, reason });
        };
        // 模拟真实用户操作节奏：每步让出 30ms，避免 0ms 忙循环打满 JS 线程与事件循环。
        const yieldStep = async (ms = 30) => {
          await sleep(ms);
        };
        const scroller = () => viewRef.current === currentView
          ? soakScroller(currentView, diffRef.current, sbsRef.current) : null;
        const rowsReady = () => scroller()?.ready() === true;
        const rowsSettled = () => rowsReady() && scroller()?.settled() === true && highlightStore.getSnapshot().pending === 0
          && (currentView === "split" || rowStore.getSnapshot().pendingChunks === 0);
        // 通过工具栏事件调用最新渲染闭包，避免长期任务持有旧的选中项。
        const clickView = async (label: string) => {
          const target: ViewMode = label === messages.diff.unified ? "unified" : "split";
          if (!selectViewInUi(target)) throw new Error(`找不到视图切换按钮：${label}`);
          currentView = target;
          if (!await waitForSoak(rowsReady, 2000)) {
            throw new Error(`视图切换未完成：${label}`);
          }
        };
        const heartbeat = async (path: string) => {
          const now = performance.now();
          if (now - lastHeartbeat < 10_000 && !isTimeUp()) return;
          lastHeartbeat = now;
          const vis = await monitor.sample();
          const elapsedSec = Math.round((now - startedAt) / 1000);
          const sample = {
            elapsedSec,
            cycle,
            file: path,
            view: currentView,
            visibility: vis,
            loadedChunks: rowStore.getSnapshot().loadedChunks,
            highlightLines: highlightStore.getSnapshot().loadedLines,
            ...(diagnostics ? { ipc: diagnostics.snapshot() } : {}),
          };
          samples.push(sample);
          if (diagnostics) await ipc.debugLog(`[SOAK_HEARTBEAT] ${JSON.stringify(sample)}`);
          if (interrupted(vis)) {
            if (!focus) throw new Error(`可见连续合成中断：${JSON.stringify(vis)}`);
            await resumeWhenVisible(JSON.stringify(vis));
          }
          debugLog(
            `[SOAK] ${elapsedSec}s/${soakSeconds}s cycle=${cycle} file=${path} view=${currentView} ` +
              `vis=${vis.state} rAF=${vis.rafPerSecond.toFixed(1)} chunks=${sample.loadedChunks} tokens=${sample.highlightLines}`,
          );
        };
        // 固定阶段：staged.ts、并排、顶部、数据就绪后静置，由驱动在窗口内取样；压力阶段静置更久（Q98）。
        const holdCheckpoint = async (entry: ChangeEntry, pressure: boolean) => {
          const pauseStarted = performance.now();
          await sleep(500);
          const settled = await waitForSoak(rowsSettled, 1000);
          if (!settled || !scroller()?.atTop()) throw new Error("同阶段内存采样准备失败");
          const checkpoint = { cycle, file: entry.path, view: currentView, pressure, startedAt: new Date().toISOString(),
            endedAt: "", hiddenDuringHold: false,
            ...(diagnostics ? { ipcBefore: diagnostics.snapshot(), ipcAfter: undefined as IpcDiagnosticSample | undefined } : {}) };
          checkpoints.push(checkpoint);
          let hiddenDuringHold = document.hidden;
          const onHoldHidden = () => {
            if (document.hidden) hiddenDuringHold = true;
          };
          document.addEventListener("visibilitychange", onHoldHidden);
          await ipc.debugLog(`[SOAK_CHECKPOINT_BEGIN] ${cycle}${pressure ? " pressure" : ""}`);
          // Hold one known phase briefly; compositor monitoring remains active throughout.
          await sleep(pressure ? 16_000 : 3500);
          await ipc.debugLog(`[SOAK_CHECKPOINT_END] ${cycle}`);
          document.removeEventListener("visibilitychange", onHoldHidden);
          checkpoint.hiddenDuringHold = hiddenDuringHold;
          checkpoint.endedAt = new Date().toISOString();
          if (diagnostics) checkpoint.ipcAfter = diagnostics.snapshot();
          checkpointPauseMs += performance.now() - pauseStarted;
          lastCheckpoint = performance.now();
          // 已完成段立即增量落盘：中断后仍可核对该段的真实起止、视图、压力标记与遮挡状态。
          // 尚未完成的段不会写，避免伪造进度。
          await progress({ kind: "checkpoint", cycle, file: checkpoint.file, view: checkpoint.view,
            pressure, startedAt: checkpoint.startedAt, endedAt: checkpoint.endedAt,
            hiddenDuringHold, checkpointPauseSeconds: checkpointPauseMs / 1000,
            visibilityPauseSeconds: visibilityPauseMs / 1000,
            cycles: cycle, fileSwitches, viewSwitches, scrolls, jumps });
        };

        // 单项诊断：只重复一类操作，约每 90 秒回到固定阶段做一次压力取样，定位压力后仍保留的内存增长来自哪类操作。
        if (focus) {
          const fixed = entries.find((entry) => entry.path === "src/staged.ts" && entry.changes === "staged");
          const large = entries.find((entry) => entry.path === "src/generated.ts");
          // remount：小文本与二进制文件交替，diff 视图反复卸载、挂载；switch：两个小文本文件交替，diff 视图保持挂载。
          const binary = entries.find((entry) => entry.binary);
          const smallTexts = entries.filter((entry) => !entry.binary && entry.rowCountSplit > 0
            && entry.rowCountSplit <= 40 && entry !== fixed);
          if (!fixed || !large || !binary || smallTexts.length < 2) throw new Error("单项诊断缺少固定文件");
          const focusEntry = focus === "scroll" || focus === "jump" || focus === "view" || focus === "open" ? large : fixed;
          const openSplit = async (entry: ChangeEntry) => {
            const expand = !entry.binary;
            await openEntry(entry, { view: "split", worktreeId: worktree.id, expand });
            currentView = "split";
            fileSwitches += 1;
            if (expand && entry.rowCountSplit > 0 && !await waitForSoak(rowsReady, 2000)) {
              throw new Error(`等待行数据超时：${entry.path}`);
            }
            await yieldStep();
          };
          await openSplit(focusEntry);
          // 每次挂载的 diff 视口元素只留弱引用；压力取样后统计未回收的个数，
          // 区分“卸载后的 DOM 仍被持有”与“DOM 已回收但原生内存累积”。
          const mountedRoots: WeakRef<Element>[] = [];
          const trackRoot = () => {
            const element = document.querySelector(".sbs, .diff-scroll");
            if (element && mountedRoots.at(-1)?.deref() !== element) mountedRoots.push(new WeakRef(element));
          };
          const rootSamples: Array<{ cycle: number; tracked: number; alive: number }> = [];
          report.mountedRootSamples = rootSamples;
          let step = 0;
          while (!isTimeUp()) {
            cycle += 1;
            if (document.hidden) await resumeWhenVisible("操作前窗口不可见");
            try {
              const handle = scroller();
              if (focusEntry === large && focus !== "open" && !handle) throw new Error("diff 未挂载");
              if (focus === "idle") {
                await sleep(1000);
              } else if (focus === "ipc") {
                // 只取行、丢弃结果，界面停在固定文件：区分 IPC 本身与渲染。
                const from = (step * 500) % large.rowCountSplit;
                await ipc.getRows({ worktreeId: worktree.id, path: large.path, changes: large.changes, view: "split",
                  from, to: Math.min(from + 500, large.rowCountSplit) });
                ipcRequests += 1;
                await yieldStep(15);
              } else if (focus === "highlight") {
                // 只取两个小文件的高亮、丢弃结果，界面停在固定文件：区分高亮取数与渲染。
                const entry = smallTexts[step % 2];
                for (const version of ["old", "new"] as const) {
                  await ipc.highlightRange({ worktreeId: worktree.id, path: entry.path, changes: entry.changes, version,
                    fromLine: 1, toLine: 400 });
                  ipcRequests += 1;
                }
                await yieldStep(15);
              } else if (focus === "channel") {
                // 只反复让出任务（与打开文件时 waitUntil 相同的 nextTask），不取数、不改界面。
                for (let index = 0; index < 20; index += 1) await nextTask();
                yieldedTasks += 20;
                await yieldStep(15);
              } else if (focus === "remount") {
                await openSplit(binary);
                await openSplit(fixed);
                trackRoot();
              } else if (focus === "switch") {
                await openSplit(smallTexts[0]);
                await openSplit(smallTexts[1]);
              } else if (focus === "open") {
                for (const entry of entries) {
                  if (isTimeUp()) break;
                  await openSplit(entry);
                }
              } else if (focus === "scroll") {
                handle!.scrollToRow((step % 12) * 40);
                scrolls += 1;
                await yieldStep();
              } else if (focus === "jump") {
                const target = Math.floor(large.rowCountSplit * [0.25, 0.5, 0.75][step % 3]);
                handle!.scrollToRow(target);
                jumps += 1;
                if (!await waitForSoak(rowsReady, 2000)) {
                  throw new Error(`跳转数据超时：${large.path}`);
                }
                await yieldStep();
                handle!.scrollToRow(0);
                jumps += 1;
                await yieldStep();
              } else {
                await clickView("统一");
                currentView = "unified";
                viewSwitches += 1;
                await yieldStep();
                await clickView("并排");
                currentView = "split";
                viewSwitches += 1;
                await yieldStep();
              }
              step += 1;
              await heartbeat(focusEntry.path);
              if (performance.now() - lastCheckpoint >= 90_000
                && startedAt + soakSeconds * 1000 + pausedMs() - performance.now() > 5000) {
                if (focusEntry !== fixed) await openSplit(fixed);
                scroller()?.scrollToRow(0);
                await holdCheckpoint(fixed, true);
                if (mountedRoots.length > 0) {
                  const sample = { cycle, tracked: mountedRoots.length,
                    alive: mountedRoots.filter((ref) => ref.deref() !== undefined).length };
                  rootSamples.push(sample);
                  await ipc.debugLog(`[SOAK_ROOTS] ${JSON.stringify(sample)}`);
                }
                if (focusEntry !== fixed) await openSplit(focusEntry);
              }
            } catch (error) {
              // 遮挡期间计时器被节流，等待会超时；只有确实发生过遮挡才恢复后重建状态继续。
              if (!hiddenSeen && !document.hidden) throw error;
              await resumeWhenVisible(`遮挡导致操作失败：${getErrorMessage(error)}`);
              await openSplit(focusEntry);
            }
          }
        }

        while (!focus && !isTimeUp() && (diagnostic?.cycles == null || cycle < diagnostic.cycles)) {
          cycle += 1;
          for (const entry of entries) {
            if (isTimeUp()) break;

            const stepStarted = performance.now();
            fileSwitches += 1;
            const expand = !entry.binary;
            debugLog(`[SOAK] 打开 ${entry.path}（${currentView}）`);
            await openEntry(entry, { view: currentView, worktreeId: worktree.id, expand });
            await yieldStep();
            debugLog(`[SOAK] 已打开 ${entry.path}`);

            let handle = scroller();
            // 只有可展开且有行数的文件才等待行数据；二进制或 0 行文件无需等行，直接推进
            if (expand && entry.rowCountSplit > 0) {
              const coveredOk = await waitForSoak(rowsReady, 2000);
              if (!coveredOk) {
                throw new Error(`等待行数据超时：${entry.path} ${currentView}`);
              }
            }

            handle = scroller();
            if (expand && entry.rowCountSplit > 0 && !handle) throw new Error(`diff 未挂载：${entry.path}`);
            const totalRows = currentView === "split" ? entry.rowCountSplit : entry.rowCountUnified;
            if (expand && totalRows > 0 && handle && !isTimeUp()) {
              const step = 40;
              const maxScroll = Math.min(totalRows, 240);
              for (let r = 0; r < maxScroll; r += step) {
                if (isTimeUp()) break;
                handle.scrollToRow(r);
                scrolls += 1;
                debugLog(`[SOAK] step scroll ${entry.path} row=${r}`);
                await yieldStep();
              }

              if (totalRows > 100 && !isTimeUp()) {
                const target = Math.floor(totalRows * 0.5);
                handle.scrollToRow(target);
                jumps += 1;
                debugLog(`[SOAK] jump to ${target}`);
                const jumped = await waitForSoak(rowsReady, 2000);
                if (!jumped) throw new Error(`跳转数据超时：${entry.path}`);
                await yieldStep();

                handle.scrollToRow(0);
                jumps += 1;
                debugLog(`[SOAK] jump back 0`);
                await yieldStep();
              }

              if (!isTimeUp()) {
                await clickView("统一");
                viewSwitches += 1;
                currentView = "unified";
                debugLog(`[SOAK] 已切换到统一视图 ${entry.path}`);
                await yieldStep();

                const unifiedRows = entry.rowCountUnified;
                for (let r = 0; r < Math.min(unifiedRows, 120); r += step) {
                  if (isTimeUp()) break;
                  const live = scroller();
                  if (!live) break;
                  live.scrollToRow(r);
                  scrolls += 1;
                  await yieldStep();
                }
              }

              if (!isTimeUp()) {
                await clickView("并排");
                viewSwitches += 1;
                currentView = "split";
                debugLog(`[SOAK] 已切回并排视图 ${entry.path}`);
                await yieldStep();
              }
            }

            debugLog(
              `[SOAK] 循环 ${cycle} ${entry.path} 用时 ${Math.round(performance.now() - stepStarted)} ms ` +
                `（chunks=${rowStore.getSnapshot().loadedChunks} tokens=${highlightStore.getSnapshot().loadedLines}）`,
            );
            await heartbeat(entry.path);
            if (entry.path === "src/staged.ts" && entry.changes === "staged" && currentView === "split"
              && performance.now() - lastCheckpoint >= 60_000
              && startedAt + soakSeconds * 1000 + pausedMs() - performance.now() > 5000) {
              // 每第 5 个阶段（约 5 分钟一次）是压力阶段：驱动发送 WebKit 低内存通知，约 10 秒后取样（Q98）。
              await holdCheckpoint(entry, checkpoints.length % 5 === 4);
            }
          }
        }

        if (diagnostic?.cycles != null && isTimeUp()) throw new Error("固定循环诊断达到主动操作安全时限");
        const wallDurationSeconds = (performance.now() - startedAt) / 1000;
        report.operationsEndedAt = new Date().toISOString();
        const totalElapsed = wallDurationSeconds - pausedMs() / 1000;
        report.wallDurationSeconds = wallDurationSeconds;
        report.checkpointPauseSeconds = checkpointPauseMs / 1000;
        report.visibilityPauseSeconds = visibilityPauseMs / 1000;
        let finalVisibility = await monitor.sample();
        if (focus && interrupted(finalVisibility)) {
          await resumeWhenVisible(`结束时：${JSON.stringify(finalVisibility)}`);
          finalVisibility = await monitor.sample();
        }
        report.finalVisibility = finalVisibility;
        if (interrupted(finalVisibility)) throw new Error("结束时窗口不可见，长测无效");
        report.ok = true;
        report.targetDurationSeconds = soakSeconds;
        report.actualDurationSeconds = totalElapsed;
        report.totalCycles = cycle;
        report.totalFileSwitches = fileSwitches;
        report.totalViewSwitches = viewSwitches;
        report.totalScrolls = scrolls;
        report.totalJumps = jumps;
        report.totalIpcRequests = ipcRequests;
        report.totalYieldedTasks = yieldedTasks;
        report.finalVisibility = finalVisibility;
        report.samplesCount = samples.length;
        report.samples = samples;
        if (diagnostics && diagnostic) {
          report.ipcAfterOperations = diagnostics.snapshot();
          const prepareStarted = performance.now();
          const fixedEntry = entries.find((entry) => entry.path === "src/staged.ts" && entry.changes === "staged");
          if (!fixedEntry) throw new Error("诊断静置缺少固定 staged 文件");
          await openEntry(fixedEntry, { view: "split", worktreeId: worktree.id, expand: true });
          currentView = "split";
          scroller()?.scrollToRow(0);
          if (!await waitForSoak(rowsSettled, 2000) || !scroller()?.atTop()) throw new Error("诊断静置准备失败");
          setStatus("诊断静置中，请保持窗口可见");
          const idleStarted = performance.now();
          const idle = { file: fixedEntry.path, view: "split", preparationSeconds: (idleStarted - prepareStarted) / 1000,
            startedAt: new Date().toISOString(), endedAt: "", targetSeconds: diagnostic.idleSeconds,
            actualSeconds: 0, initial: diagnostics.snapshot(), samples: [] as Array<{
              elapsedSec: number; timestamp: string; visibility: SoakVisibility; ipc: IpcDiagnosticSample }> };
          report.idle = idle;
          await ipc.debugLog(`[SOAK_IDLE_BEGIN] ${JSON.stringify(idle.initial)}`);
          while (performance.now() - idleStarted < diagnostic.idleSeconds * 1000) {
            const next = Math.min(diagnostic.idleSeconds * 1000, (idle.samples.length + 1) * 10_000);
            await sleep(Math.max(0, idleStarted + next - performance.now() - 1000));
            const visibility = await monitor.sample();
            const sample = { elapsedSec: (performance.now() - idleStarted) / 1000,
              timestamp: new Date().toISOString(), visibility, ipc: diagnostics.snapshot() };
            idle.samples.push(sample);
            await ipc.debugLog(`[SOAK_IDLE_SAMPLE] ${JSON.stringify(sample)}`);
            if (visibility.hidden || visibility.rafPerSecond <= 5 || visibility.hiddenTransitions > 0
              || visibility.maxFrameGapMs > 2000) throw new Error("诊断静置期间窗口不可见或合成中断");
          }
          idle.actualSeconds = (performance.now() - idleStarted) / 1000;
          idle.endedAt = new Date().toISOString();
          await ipc.debugLog(`[SOAK_IDLE_END] ${JSON.stringify(diagnostics.snapshot())}`);
        }
        const completionMessage = `长期运行基线完成：总耗时 ${report.actualDurationSeconds}s 循环 ${cycle} 次 切换文件 ${fileSwitches} 次 切换视图 ${viewSwitches} 次 滚动 ${scrolls} 次 跳转 ${jumps} 次`;
        if (diagnostics) await ipc.debugLog(completionMessage);
        else debugLog(completionMessage);
      } catch (caught) {
        report.ok = false;
        report.error = getErrorMessage(caught);
        report.samples = samples;
        debugLog(`长期运行基线异常中断：${getErrorMessage(caught)}`);
      }
      if (diagnostics) {
        report.ipcFinal = diagnostics.snapshot();
        diagnostics.stop();
      }
      monitor.stop();
      await ipc.writeMeasurement(report);
    },
    [highlightStore, openEntry, rowStore],
  );

  const runAutoMeasurement = useCallback(
    async (config: MeasureConfig) => {
      debugLog(`自动测量启动：only=${config.only?.join(",") ?? "(全部)"} repo=${config.repoPath}`);
      const monitor = monitorSoakVisibility();
      let focusLosses = 0;
      const onWindowBlur = () => { focusLosses = 1; };
      const results: Measurement[] = [];
      let lastVisibility: (SoakVisibility & { focusLosses: number }) | null = null;
      try {
        setStatus("性能测量等待可见窗口，请将此窗口置于前台");
        const deadline = performance.now() + 120_000;
        let ready = false;
        while (performance.now() < deadline) {
          const sample = await monitor.sample();
          if (!sample.hidden && sample.focused && sample.rafPerSecond > 5) { ready = true; break; }
        }
        if (!ready) throw new Error("性能测量未获得前台可见合成窗口");
        // 入口与结束采样都聚焦还不够；测量途中失焦也使整次证据无效。
        window.addEventListener("blur", onWindowBlur);
        monitor.reset();
        const added = config.repoPath ? await ipc.addRepo(config.repoPath) : null;
        const list = await ipc.listRepos();
        const repo = added ?? list[0];
        if (!repo) throw new Error("没有可测量的仓库");
        setRepos(list);
        setActiveRepo(repo);
        const worktree = repo.worktrees.find((entry) => !entry.bare) ?? repo.worktrees[0];
        if (!worktree) throw new Error("没有可测量的 worktree");
        setWorktreeId(worktree.id);
        const views = (config.only?.length ? config.only : ["split", "unified"]) as ViewMode[];
        for (const targetView of views) {
          const measurement = await runMeasurement(
            targetView,
            worktree.id,
            config.targetPath ?? null,
            config.targetPath ?? null,
          );
          if (measurement) results.push(measurement);
          debugLog(
            `组合 ${targetView}：首屏 ${measurement?.firstScreen.fromListChangesMs?.toFixed(1) ?? "?"} ms，` +
              `跳转 ${measurement?.jump?.appearMs.toFixed(1) ?? "?"} ms，` +
              `滚动未覆盖帧 ${measurement?.scroll?.uncoveredFrames ?? "?"}`,
          );
        }
        const visibility = { ...await monitor.sample(), focusLosses };
        lastVisibility = visibility;
        if (!visibility.focused || visibility.focusLosses > 0 || visibility.hidden || visibility.rafPerSecond <= 5
          || visibility.hiddenTransitions > 0 || visibility.maxFrameGapMs > 2000) {
          throw new Error(`性能测量前台可见性无效：${JSON.stringify(visibility)}`);
        }
        if (results.length !== views.length) throw new Error("性能测量结果缺失");
        await ipc.writeMeasurement({ ok: true, combos: results, visibility });
        debugLog(`测量完成，共 ${results.length} 组`);
      } catch (caught) {
        debugLog(`自动测量失败：${getErrorMessage(caught)}`);
        await ipc.writeMeasurement({ ok: false, error: getErrorMessage(caught), combos: results, visibility: lastVisibility });
      } finally {
        window.removeEventListener("blur", onWindowBlur);
        monitor.stop();
      }
    },
    [runMeasurement],
  );
  // x18n-ignore-end

  // ---------------- 设置、worktree 状态与页签 ----------------

  /** 保存设置（F08）：成功返回 null，失败返回可展示的原因。 */
  /** diff 工具条与设置共用同一份选项：改一处即写设置，两处同步（Q139）。 */
  const changeDiffOptions = useCallback((patch: Partial<DiffToolbarOptions>) => {
    const next = { ...diffOptionsRef.current, ...patch };
    setDiffOptions({ ...next, ignoreWhitespace: Math.min(3, Math.max(0, next.ignoreWhitespace)) as 0 | 1 | 2 | 3 });
    void ipc
      .updateSettings({
        diffContextLines: next.contextLines,
        diffFoldUnchanged: next.foldUnchanged,
        diffSyncScroll: next.syncScroll,
        diffHighlight: next.highlight,
        diffShowWhitespace: next.showWhitespace,
        diffSoftWrap: next.softWrap,
        diffJumpToNextFile: next.jumpToNextFile,
        diffIgnoreWhitespace: next.ignoreWhitespace,
      })
      .catch(() => {
        /* 保存失败时仍按界面上的选择显示，下次启动回到上次保存的值 */
      });
  }, []);

  const updateSettings = useCallback(async (patch: SettingsPatch): Promise<AppErrorPayload | null> => {
    try {
      const next = await ipc.updateSettings({
        themeLight: patch.themeLight,
        themeDark: patch.themeDark,
        codeFontFamily: patch.codeFontFamily,
        diffFontSize: patch.diffFontSize,
        gitPath: patch.gitPath,
        mergeTool: patch.mergeTool,
        diffTool: patch.diffTool,
        language: patch.language,
        cherryPickKeepEmpty: patch.cherryPickKeepEmpty,
        cherryPickIncludeSource: patch.cherryPickIncludeSource,
        sidebarWidth: patch.sidebarWidth,
        viewMode: patch.viewMode,
        uiZoom: patch.uiZoom,
        treeGroupByDirectory: patch.treeGroupByDirectory,
        diffContextLines: patch.diffContextLines,
        diffFoldUnchanged: patch.diffFoldUnchanged,
        diffSyncScroll: patch.diffSyncScroll,
        diffHighlight: patch.diffHighlight,
        diffShowWhitespace: patch.diffShowWhitespace,
        diffSoftWrap: patch.diffSoftWrap,
        diffJumpToNextFile: patch.diffJumpToNextFile,
        diffIgnoreWhitespace: patch.diffIgnoreWhitespace,
        changelistMode: patch.changelistMode,
        commitSubjectLimit: patch.commitSubjectLimit,
        commitBlankLineRequired: patch.commitBlankLineRequired,
        commitBodyLineLimit: patch.commitBodyLineLimit,
        commitShowRuler: patch.commitShowRuler,
        commitWarnCrlf: patch.commitWarnCrlf,
        commitWarnDetached: patch.commitWarnDetached,
        commitWarnLargeFileBytes: patch.commitWarnLargeFileBytes,
        commitWarnInvalidFilename: patch.commitWarnInvalidFilename,
        notify: patch.notify,
        mergeAutoApply: patch.mergeAutoApply,
        logColumns: patch.logColumns,
        fileHistoryShowNames: patch.fileHistoryShowNames,
        stashCompareLocal: patch.stashCompareLocal,
        logLongEdges: patch.logLongEdges,
        logFirstParent: patch.logFirstParent,
        logPreviewPosition: patch.logPreviewPosition,
        updateMode: patch.updateMode,
        fetchTagMode: patch.fetchTagMode,
        fetchAutomatically: patch.fetchAutomatically,
        checkRemote: patch.checkRemote,
        autoUpdateOnPushRejection: patch.autoUpdateOnPushRejection,
        pushDialogOnCommit: patch.pushDialogOnCommit,
        pushDialogProtectedOnly: patch.pushDialogProtectedOnly,
        protectedBranches: patch.protectedBranches,
        appUpdateMode: patch.appUpdateMode,
        allowPrerelease: patch.allowPrerelease,
        shortcuts: patch.shortcuts,
      });
      if (patch.shortcuts) {
        setShortcutsSnapshot(patch.shortcuts);
        applyShortcutOverrides(patch.shortcuts);
      }
      setAppUpdateMode(next.appUpdateMode);
      setAllowPrerelease(next.allowPrerelease ?? false);
      if (patch.allowPrerelease !== undefined) {
        const epoch = ++updateChannelEpoch.current;
        void m7.updateStatus().then((status) => {
          if (epoch !== updateChannelEpoch.current) return;
          setUpdateStatus(status); setUpdateBannerVersion(status.available?.version ?? null);
        }).catch((caught) => setAppError(toAppError(caught), null));
      }
      setThemeLight(next.themeLight);
      setThemeDark(next.themeDark);
      setCodeFontFamily(next.codeFontFamily);
      setFontSize(next.diffFontSize);
      setGitPath(next.gitPath);
      setMergeTool(next.mergeTool);
      setDiffTool(next.diffTool);
      const preference = (next.language ?? "auto") as LanguagePreference;
      setLanguage(preference);
      setLocale(resolveLocale(preference));
      setCherryPickKeepEmpty(next.cherryPickKeepEmpty);
      setCherryPickIncludeSource(next.cherryPickIncludeSource);
      setSidebarWidth(next.sidebarWidth);
      setView(next.viewMode);
      setUiZoom(next.uiZoom);
      setTreeGroupByDirectory(next.treeGroupByDirectory);
      setDiffOptions({
        contextLines: next.diffContextLines,
        foldUnchanged: next.diffFoldUnchanged,
        syncScroll: next.diffSyncScroll,
        highlight: next.diffHighlight,
        showWhitespace: next.diffShowWhitespace,
        softWrap: next.diffSoftWrap,
        jumpToNextFile: next.diffJumpToNextFile,
        ignoreWhitespace: next.diffIgnoreWhitespace as 0 | 1 | 2 | 3,
      });
      changelistModeRef.current = next.changelistMode;
      setChangelistMode(next.changelistMode);
      if (patch.changelistMode !== undefined) { acceptChangelistView(null); await refresh(); }
      setCommitChecks({
        subjectLimit: next.commitSubjectLimit,
        blankLineRequired: next.commitBlankLineRequired,
        bodyLineLimit: next.commitBodyLineLimit,
        showRuler: next.commitShowRuler,
        warnCrlf: next.commitWarnCrlf,
        warnDetached: next.commitWarnDetached,
        warnLargeFileBytes: Number(next.commitWarnLargeFileBytes),
        warnInvalidFilename: next.commitWarnInvalidFilename,
      });
      setNotifyKinds(next.notify);
      setMergeAutoApply(next.mergeAutoApply);
      setFileHistoryShowNames(next.fileHistoryShowNames);
      setStashCompareLocal(next.stashCompareLocal);
      setLogSettings({
        columns: next.logColumns,
        longEdges: next.logLongEdges,
        firstParent: next.logFirstParent,
        previewPosition: next.logPreviewPosition,
      });
      setPushSettings({
        updateMode: next.updateMode,
        fetchTagMode: next.fetchTagMode,
        fetchAutomatically: next.fetchAutomatically,
        checkRemote: next.checkRemote,
        autoUpdateOnPushRejection: next.autoUpdateOnPushRejection,
        pushDialogOnCommit: next.pushDialogOnCommit,
        pushDialogProtectedOnly: next.pushDialogProtectedOnly,
        protectedBranches: next.protectedBranches,
      });
      return null;
    } catch (caught) {
      return toAppError(caught);
    }
  }, [refresh, acceptChangelistView]);

  /** 重新加载登录 shell 环境与 Git 程序（F08）。 */
  const reloadEnvironment = useCallback(async (): Promise<AppErrorPayload | null> => {
    try {
      if (worktreeId) gateRef.current.activate(worktreeId);
      setM6State(null); setHistoryAction(null); setExternalTool(null);
      await ipc.reloadEnvironment();
      if (worktreeId) {
        const session = await ipc.openWorktree(worktreeId);
        setInfo(session);
        // 旧会话的历史目标随环境重载失效：重新打开（任务 01 的失效规则）。
        setHistoryEpoch((value) => value + 1);
        await refresh();
      }
      return null;
    } catch (caught) {
      return toAppError(caught);
    }
  }, [refresh, worktreeId]);

  const refreshWorktreeStatuses = useCallback(async () => {
    if (!activeRepo) return;
    try {
      setWorktreeStatuses(await ipc.listWorktreeStatus(activeRepo.id));
    } catch {
      // 下拉里的状态读取失败不阻塞主流程；单个 worktree 的错误由后端逐项返回。
    }
  }, [activeRepo]);

  useEffect(() => {
    void refreshWorktreeStatuses();
  }, [refreshWorktreeStatuses, worktreeId, snapshot?.revision]);

  /** 从仓库下拉或欢迎界面打开一个已登记仓库。 */
  const openRepo = useCallback(async (repo: RepoEntry) => {
    setBusy(true);
    try {
      setActiveRepo(repo);
      const worktree = repo.worktrees.find((entry) => !entry.bare) ?? repo.worktrees[0];
      if (!worktree) {
        setAppError(localizedError("rejected", "firstRun.notARepo"));
        return;
      }
      await ipc.selectRepo(repo.id, worktree.id);
      await openWorktree(worktree.id);
    } catch (caught) {
      setAppError(toAppError(caught));
    } finally {
      setBusy(false);
    }
  }, [openWorktree, setAppError]);

  // 登记移除不清理草稿、回收记录或后端会话。先处理未完成工作，避免隐藏可恢复内容。
  const repoRemovalBlocked = repoToRemove?.id === activeRepo?.id && (writingWorktrees.size > 0 || closeCoordinatorRef.current.needsExitGuard()
    || editSessions.hasWork || terminalSessions.needsExitGuard || reviewDraftSessions.hasWork || mergeBufferGuards.hasWork);
  const removeRepo = async (repo: RepoEntry) => {
    if (busy || repoRemovalBlocked) return;
    setBusy(true);
    setRepoRemovalError(null);
    try {
      await ipc.removeRepo(repo.id);
      setRepos((current) => current.filter((entry) => entry.id !== repo.id));
      if (activeRepo?.id === repo.id) {
        gateRef.current.activate(null);
        selectionEpochRef.current += 1;
        selectionGateRef.current.clear();
        worktreeIdRef.current = null;
        setWorktreeId(null);
        setActiveRepo(null);
        setInfo(null);
        setSnapshot(null);
        setSelected(null);
        setWorktreeStatuses([]);
        setTerminalOpen(false);
        setGithubOpen(false);
        resetMergeTabs();
        await ipc.stopWatch().catch((caught) => setAppError(toAppError(caught), null));
      }
      setRepoToRemove(null);
    } catch (caught) {
      setRepoRemovalError(toAppError(caught));
    } finally {
      setBusy(false);
    }
  };

  const pinEntry = useCallback((entry: ChangeEntry) => {
    // 冲突文件双击 / Enter 直接打开合并页签（返修 F10，Q143）。
    if (entry.unmerged) {
      openMergeTab(entry);
      return;
    }
    if (worktreeIdRef.current) {
      void openEntry(entry, { view: viewRef.current, worktreeId: worktreeIdRef.current, pin: true });
    }
  }, [openEntry, openMergeTab]);

  const toggleReviewCurrent = useCallback(() => {
    if (!selectedEntry) return;
    void setReviewMark(selectedEntry, !selectedEntry.reviewed);
  }, [selectedEntry, setReviewMark]);

  // ---------------- M5 第二批：日志、分支与远端 ----------------

  /** 引用版本：内部写操作（提交、检出、fetch、更新、推送）后递增，日志据此重新查询。 */
  const [refsRevision, setRefsRevision] = useState(0);
  /**
   * 可信重读代号：监视事件、手动刷新与回到前台都核对一次引用。
   * 日志不自己定期轮询 `history_refs`，监视失败时仍能手动刷新。
   */
  const [watchRevision, setWatchRevision] = useState(0);
  const bumpRefsRevision = useCallback(() => setRefsRevision((value) => value + 1), []);
  const pushSettingsRef = useRef(pushSettings);
  pushSettingsRef.current = pushSettings;
  /** fetch / 更新成功后清除“远端有新提交”标记（branches 在写编排之后创建）。 */
  const branchesIncomingRef = useRef<{
    captureIncoming: (worktreeId: string) => IncomingToken | null;
    clearIncoming: (token: IncomingToken | null) => void;
  }>({ captureIncoming: () => null, clearIncoming: () => {} });

  /**
   * 打开只读历史目标（随 worktree 生命周期，任务 01 的失效规则）。
   * 切换 worktree 时关闭旧目标：它的页签随之失效，旧请求与新目标不能串味。
   */
  useEffect(() => {
    if (!worktreeId) {
      setHistorySession(null);
      return;
    }
    let cancelled = false;
    let openedId: string | null = null;
    setHistorySession(null);
    void ipc
      .historyOpen(worktreeId)
      .then((info) => {
        if (cancelled) {
          void ipc.historyClose(info.targetId).catch(() => undefined);
          return;
        }
        openedId = info.targetId;
        setHistorySession({ worktreeId, epoch: historyEpoch, targetId: info.targetId, refs: info.refs });
      })
      .catch((caught: unknown) => {
        if (!cancelled) setAppError(toAppError(caught), worktreeId);
      });
    return () => {
      cancelled = true;
      setHistorySession(null);
      if (openedId) {
        dropCommitTarget(openedId);
        void ipc.historyClose(openedId).catch(() => undefined);
      }
    };
    // 只依赖稳定方法；commitTabs 容器每次渲染都会变化，不能作为目标生命周期依赖。
  }, [dropCommitTarget, historyEpoch, setAppError, worktreeId]);

  // 打开仓库 / 切换 worktree 时默认显示日志（L01）。
  useEffect(() => {
    setMainView("log");
  }, [worktreeId]);

  // 提交页签全部关闭后回到日志（不抢工作区 diff 的激活状态）。
  useEffect(() => {
    if (mainView === "commit" && !commitTabs.active) setMainView("log");
  }, [commitTabs.active, mainView]);

  /** 执行一次分支 / 远端写操作的真实 IPC（参数全部结构化）。 */
  const executeRemoteIntent = useCallback((intent: RemoteWriteIntent): Promise<unknown> => {
    switch (intent.kind) {
      case "checkout":
        return ipc.checkoutBranch({
          worktreeId: intent.worktreeId,
          target: intent.target,
          strategy: intent.strategy,
        });
      case "createBranch":
        return ipc.createBranch({ worktreeId: intent.worktreeId, request: intent.request });
      case "fetch":
        return ipc.fetchRemotes(intent.worktreeId);
      case "update":
        return ipc.updateProject(intent.worktreeId);
      case "push":
        return ipc.pushBranch({ worktreeId: intent.worktreeId, request: intent.request });
      default:
        return Promise.reject(localizedError("internal", "errors.app.unknownRemoteOperation"));
    }
  }, []);

  /**
   * 受保护的分支 / 远端写入（B01）：写门禁、结果归属、真实重读、待核对阻止退出。
   * `silent` 用于自动 fetch：失败只记在状态栏，不弹通知（Q152）。
   */
  const performRemoteWrite = useCallback(
    async (
      intent: RemoteWriteIntent,
      options: {
        silent?: boolean;
        lease?: RemoteWriteLease;
        execute?: (intent: RemoteWriteIntent) => Promise<unknown>;
      } = {},
    ): Promise<{ ok: true; result: unknown } | { ok: false; error: AppErrorPayload }> => {
      const outcome = await runRemoteWrite<OperationContext>(
        {
          writing: writingWorktreesRef.current,
          publishWriting: () => setWritingWorktrees(new Set(writingWorktreesRef.current)),
          begin: (id) => writeOrchestrator.begin(id),
          end: (id, needsVerification) => writeOrchestrator.end(id, needsVerification),
          beginOperation: (id) => operationGateRef.current.begin(id),
          canRecord: (token) => operationGateRef.current.canRecord(token),
          executeIntent: options.execute ?? executeRemoteIntent,
          refreshTarget,
          reportFailure: (_id, error) => {
            if (options.silent) {
              setAutoFetchError(error);
              return;
            }
            notify({
              tone: error.kind === "needsVerification" ? "warning" : "error",
              title:
                () => (error.kind === "needsVerification"
                  ? messages.errors.needsVerification
                  : error.kind === "rejected"
                    ? messages.remote.rejectedTitle
                    : error.kind === "environment"
                      ? messages.remote.environmentTitle
                      : messages.remote.gitFailedTitle),
              detail: () => (formatAppError(error)),
              persistent: true,
            });
          },
          toError: toAppError,
          busyMessage: messages.branches.busy,
          guardUnavailable: messages.close.guardUnavailable,
          onSuccess: () => undefined,
        },
        intent,
        options.lease,
      );
      if (outcome.ok) {
        // 引用可能已经变化（检出、fetch、更新、推送）：日志与分支都要重新读取。
        bumpRefsRevision();
      }
      return outcome;
    },
    [bumpRefsRevision, executeRemoteIntent, notify, refreshTarget, setAppError, writeOrchestrator],
  );

  /** 检出（Plain 先试；会被覆盖时弹出智能 / 强制检出）。 */
  const runCheckout = useCallback(
    async (
      target: CheckoutTarget,
      strategy: CheckoutStrategy,
      origin = captureRemoteTarget(gateRef.current.token(), worktreeIdRef.current),
    ) => {
      if (!origin || !remoteTargetIsCurrent(origin, gateRef.current.token())) return;
      const outcome = await performRemoteWrite({ kind: "checkout", worktreeId: origin.worktreeId, target, strategy });
      if (!outcome.ok) return;
      const result = outcome.result as CheckoutResult;
      if (result.kind === "needsConfirmation") {
        if (remoteTargetIsCurrent(origin, gateRef.current.token())) {
          setCheckoutBlocked({ origin, target, label: result.target, files: result.files });
        }
        return;
      }
      setCheckoutBlocked((current) => current?.origin === origin ? null : current);
      notify({
        tone: "success",
        title: () => (messages.remote.checkoutDone(result.branch)),
        detail: () => ([result.stashKept ? messages.remote.checkoutStashKept : null, ...result.notes.map(checkoutNoteText)]
          .filter((part): part is string => Boolean(part))
          .join("\n") || undefined),
      });
    },
    [notify, performRemoteWrite],
  );

  /** 新建分支（B02）。 */
  const openNewBranch = useCallback((start: BranchStart) => {
    const origin = captureRemoteTarget(gateRef.current.token(), worktreeIdRef.current);
    if (!origin) return;
    setNewBranchError(null);
    setNewBranchStart({ origin, start });
  }, []);
  const runCreateBranch = useCallback(
    async (name: string, checkout: boolean) => {
      const pending = newBranchStart;
      if (!pending || !remoteTargetIsCurrent(pending.origin, gateRef.current.token())) return;
      setNewBranchError(null);
      const outcome = await performRemoteWrite({
        kind: "createBranch",
        worktreeId: pending.origin.worktreeId,
        request: { name, start: pending.start, checkout },
      });
      if (!outcome.ok) {
        // 对话框直接显示后端（Git）给出的原因，例如名字不合法。
        if (newBranchDialogRef.current === pending && remoteTargetIsCurrent(pending.origin, gateRef.current.token())) {
          setNewBranchError(outcome.error);
        }
        return;
      }
      const result = outcome.result as { name: string; checkedOut: boolean };
      setNewBranchStart((current) => current === pending ? null : current);
      notify({
        tone: "success",
        title: () => (result.checkedOut
          ? messages.remote.branchCreatedCheckedOut(result.name)
          : messages.remote.branchCreated(result.name)),
      });
    },
    [newBranchStart, notify, performRemoteWrite],
  );

  /** fetch（B03）。 */
  const runFetch = useCallback(
    async (targetWorktreeId: string, reason: "user" | "auto") => {
      const incomingToken = branchesIncomingRef.current.captureIncoming(targetWorktreeId);
      const outcome = await performRemoteWrite(
        { kind: "fetch", worktreeId: targetWorktreeId },
        { silent: reason === "auto" },
      );
      const result = outcome.ok ? (outcome.result as { remotes: string[] }) : null;
      if (result) {
        setAutoFetchError(null);
        // fetch 成功：远端对象已取回，“远端有新提交”标记不再需要（不额外联网核对）。
        branchesIncomingRef.current.clearIncoming(incomingToken);
        if (reason === "user") {
          notify({ tone: "success", title: () => (messages.remote.fetchDone(result.remotes.join("、"))) });
        }
      }
    },
    [notify, performRemoteWrite],
  );

  /** 更新项目（⌘T，B04）。 */
  const runUpdate = useCallback(
    async (targetWorktreeId: string): Promise<boolean> => {
      const incomingToken = branchesIncomingRef.current.captureIncoming(targetWorktreeId);
      const outcome = await performRemoteWrite({ kind: "update", worktreeId: targetWorktreeId });
      if (!outcome.ok) return false;
      const result = outcome.result as import("@/types/bindings/UpdateOutcome").UpdateOutcome;
      branchesIncomingRef.current.clearIncoming(incomingToken);
      notify(updateNotice(result));
      return result.status === "updated" || result.status === "upToDate";
    },
    [notify, performRemoteWrite],
  );

  /** 执行推送；被拒时按设置决定是否自动“更新后再推送”（只一次）。 */
  const performPush = useCallback(
    async (
      origin: RemoteTarget,
      request: PushRequest,
      options: { alreadyAttempted?: boolean; updateFirst?: boolean; lease?: RemoteWriteLease } = {},
    ): Promise<void> => {
      const outcome = await performRemoteWrite({ kind: "push", worktreeId: origin.worktreeId, request }, {
        lease: options.lease,
        execute: async () => {
          const update = async (targetWorktreeId: string) => {
            const incoming = branchesIncomingRef.current.captureIncoming(targetWorktreeId);
            const result = await ipc.updateProject(targetWorktreeId);
            branchesIncomingRef.current.clearIncoming(incoming);
            notify(updateNotice(result));
            return result.status === "updated" || result.status === "upToDate";
          };
          if (options.updateFirst && !await update(origin.worktreeId)) return "update-failed";
          return runPushSequence(origin, request, options.alreadyAttempted === true, {
            push: (worktreeId, targetRequest) => ipc.pushBranch({ worktreeId, request: targetRequest }),
            update,
            autoUpdate: () => pushSettingsRef.current.autoUpdateOnPushRejection,
            onResult: (result) => notify(pushNotice(result)),
          });
        },
      });
      if (!outcome.ok) return;
      const next = outcome.result;
      if (next === "offer-update" && remoteTargetIsCurrent(origin, gateRef.current.token())) {
        // 没有开启自动更新：给用户一个明确的“更新后再推送”入口，不自动重试。
        setPushRejected({ origin, request });
      }
      if (next === "update-failed") {
        notify({ tone: "warning", title: () => (messages.remote.autoUpdateFailed) });
      }
    },
    [notify, performRemoteWrite],
  );

  /** 推送预览、确认和提交后的续步都绑定原目标（B01、B05、B06）。 */
  const prepareBoundPush = useCallback(
    async (origin: RemoteTarget, afterCommit: boolean, lease?: RemoteWriteLease) => {
      setPushDialogError(null);
      try {
        const next = await preparePush(origin, {
          preview: ipc.pushPreview,
          current: () => gateRef.current.token(),
          showDialog: (preview) => !afterCommit || commitPushNeedsDialog(preview, pushSettingsRef.current),
          onConfirmationRequired: () => {
            if (afterCommit && closeCoordinatorRef.current.pendingIntent() !== null) {
              applyEffect(closeDriver.handleChoice("stay"));
              notify({
                tone: "warning", title: () => (messages.remote.pushConfirmationPending),
                detail: () => (messages.remote.pushConfirmationPendingDetail), persistent: true,
              });
            }
          },
        });
        if (next.kind === "dialog") {
          setPushDialog({ origin: next.origin, preview: next.preview });
        } else if (next.kind === "push") {
          await performPush(next.origin, next.request, { lease });
        } else if (afterCommit) {
          setAppError(localizedError("rejected", "remote.pushConfirmationOtherWorktree"), origin.worktreeId);
        }
      } catch (caught) {
        setAppError(toAppError(caught), origin.worktreeId);
        notify({ tone: "error", title: () => (messages.remote.pushRejected), detail: () => (getErrorMessage(caught)), persistent: true });
      }
    },
    [applyEffect, closeDriver, notify, performPush, setAppError],
  );
  const openPushDialog = useCallback(() => {
    const origin = captureRemoteTarget(gateRef.current.token(), worktreeIdRef.current);
    if (origin) void prepareBoundPush(origin, false);
  }, [prepareBoundPush]);
  commitAndPushRef.current = (origin, lease) => {
    return prepareBoundPush(origin, true, lease);
  };

  /** 自动 fetch 失败的状态栏提示（不弹通知，Q152）。 */
  const [autoFetchError, setAutoFetchError] = useState<AppErrorPayload | null>(null);
  const branches = useBranches({
    worktreeId,
    refreshKey: refsRevision + watchRevision,
    autoFetch: pushSettings.fetchAutomatically,
    checkRemote: pushSettings.checkRemote,
    writing: isWriting,
    runFetch: (targetWorktreeId, reason) => void runFetch(targetWorktreeId, reason),
    runCheck: async (targetWorktreeId) => {
      const result = await ipc.checkRemote(targetWorktreeId);
      return result.hasNewCommits;
    },
  });
  branchesIncomingRef.current = branches;

  // ---------------- M6：Stash、进行中操作、历史改写、文件历史、外部工具 ----------------

  /** 读取进行中操作与自动保存的改动；只对当前 worktree 的迟到结果生效。 */
  const refreshM6State = useCallback(async (targetWorktreeId: string) => {
    const origin = captureRemoteTarget(gateRef.current.token(), targetWorktreeId);
    const sequence = origin ? ++m6ReadSeq.current : null;
    // Always read the original target. Only UI acceptance depends on activation;
    // failures propagate to the write lifecycle and preserve NeedsVerification.
    const next = await readM6State(
      ipc.operationState(targetWorktreeId),
      ipc.autostashPending(targetWorktreeId),
    );
    if (origin && sequence === m6ReadSeq.current && remoteTargetIsCurrent(origin, gateRef.current.token())) {
      setM6State({ origin, ...next });
    }
    if (next.readErrors.length) throw next.readErrors[0];
  }, []);

  useEffect(() => {
    if (!worktreeId) {
      setM6State(null);
      return;
    }
    void refreshM6State(worktreeId).catch(() => undefined);
  }, [refreshM6State, refsRevision, snapshot?.revision, watchRevision, worktreeId]);

  const savedStashAction = useCallback((target: string, link: AutostashLink) => ({
    label: () => messages.operation.viewStash,
    run: () => {
      if (worktreeIdRef.current !== target) {
        notifyM6(target, { tone: "warning", title: () => messages.operation.stashTargetChanged, detail: link.worktreePath });
        return;
      }
      setStashFocusOid(link.stashOid);
      setSideTab("stash");
    },
  }), [notifyM6]);

  const notifyAutostashSaved = useCallback((target: string, link: AutostashLink, removed = false) => {
    if (notifiedAutostashRef.current.has(link.id)) return;
    notifiedAutostashRef.current.add(link.id);
    notifyM6(target, savedAutostashNotice(link, savedStashAction(target, link), removed));
  }, [notifyM6, savedStashAction]);

  useEffect(() => {
    if (autostashLink && worktreeId) notifyAutostashSaved(worktreeId, autostashLink);
  }, [autostashLink, worktreeId, notifyAutostashSaved]);

  const reportAutostash = useCallback((target: string, result: AutostashRestoreOutcome | null) => {
    if (!result) return false;
    notifyAutostashSaved(target, result.link, result.restored && result.removed);
    if (result.restored && result.removed) return false;
    notifyM6(target, {
      tone: "warning",
      title: () => result.cleanupError ? formatAppError(result.cleanupError) : result.conflicted ? messages.operation.autostashConflicted
        : result.restored ? messages.operation.autostashRestoredNotRemoved : messages.operation.autostashNotRestored,
      persistent: true,
      action: savedStashAction(target, result.link),
    });
    return true;
  }, [notifyM6, savedStashAction, notifyAutostashSaved]);

  /**
   * M6 写操作的统一入口：与恢复、合并共用全局写门禁和关闭/退出保护。
   * 操作完成后真实重读当前 worktree、引用与进行中状态；不依赖 watcher。
   */
  async function performM6Write<T>(targetWorktreeId: string, work: () => Promise<T>, onResult?: (result: T) => void): Promise<T> {
    if (writingWorktreesRef.current.has(targetWorktreeId)) {
      throw localizedError("rejected", "recovery.busy");
    }
    writingWorktreesRef.current.add(targetWorktreeId);
    setWritingWorktrees(new Set(writingWorktreesRef.current));
    setM6Busy(true);
    try {
      const started = await writeOrchestrator.begin(targetWorktreeId);
      if (!started.ok) {
        throw closeGuardBlockedError(started.reason);
      }
    } catch (caught) {
      writingWorktreesRef.current.delete(targetWorktreeId);
      setWritingWorktrees(new Set(writingWorktreesRef.current));
      setM6Busy(false);
      const failure = toAppError(caught);
      setAppError(failure, targetWorktreeId);
      throw failure;
    }
    setAppError(null, targetWorktreeId);
    let needsVerification = false;
    let completedWrite = false;
    try {
      const result = await work();
      completedWrite = true;
      needsVerification = resultNeedsVerification(result);
      onResult?.(result);
      if (!await refreshTarget(targetWorktreeId)) {
        throw localizedError("needsVerification", "errors.recovery.writeResultNeedsVerification");
      }
      bumpRefsRevision();
      await refreshM6State(targetWorktreeId);
      setStashRevision((value) => value + 1);
      return result;
    } catch (caught) {
      let failure = toAppError(caught);
      if (completedWrite) failure = { ...localizedError("needsVerification", "errors.recovery.writeResultNeedsVerification"), cause: failure };
      needsVerification ||= failure.kind === "needsVerification";
      const retainReadFailure = (caught: unknown) => {
        failure = { ...failure, related: [...(failure.related ?? []), toAppError(caught)] };
      };
      await refreshTarget(targetWorktreeId).catch(retainReadFailure);
      await refreshM6State(targetWorktreeId).catch(retainReadFailure);
      setAppError(failure, targetWorktreeId);
      throw failure;
    } finally {
      writingWorktreesRef.current.delete(targetWorktreeId);
      setWritingWorktrees(new Set(writingWorktreesRef.current));
      setM6Busy(false);
      writeOrchestrator.end(targetWorktreeId, needsVerification);
    }
  }

  const performOperationAction = useCallback(async (action: OperationAction, origin: RemoteTarget, operation: OperationState) => {
    const target = origin.worktreeId;
    if (!remoteTargetIsCurrent(origin, gateRef.current.token()) || operation.metadataVersion !== operationState?.metadataVersion || operation.kind !== operationState?.kind || operation.confirmationState !== operationState?.confirmationState) {
      notifyM6(target, { tone: "warning", title: () => messages.commit.stale });
      return;
    }
    let received: RewriteOutcome | null = null;
    try {
      const outcome = await performM6Write(target, () => ipc.operationExecute(target, {
        kind: operation.kind,
        metadataVersion: operation.metadataVersion,
        expectedState: operation.confirmationState,
        action,
      }), (result) => {
        received = result.continuation ?? null;
        if (result.autostash) notifyAutostashSaved(target, result.autostash.link, result.autostash.removed);
      });
      const continuation = outcome.continuation;
      const detail = () => rewriteOutcomeDetail(continuation);
      if (continuation?.error) {
        notifyM6(target, { tone: "warning", title: () => formatAppError(continuation.error!), detail, persistent: true });
      } else if (!outcome?.completed) {
        notifyM6(target, {
          tone: "warning",
          title: () => (messages.operation.stopped(
            operationKindText(outcome?.kind ?? operation.kind),
            outcome?.state?.conflictCount ?? 0,
          )),
          persistent: true,
        });
      } else if (reportAutostash(target, outcome.autostash)) {
        // The history operation finished, but its saved changes did not fully
        // restore/clean up. The warning owns this result, not a success toast.
      } else {
        notifyM6(target, { tone: "success", title: () => (messages.operation.completed), detail });
      }
    } catch (caught) {
      const failure = toAppError(caught);
      notifyM6(target, received ? rewriteFailureNotice(received, failure) : {
        tone: failure.kind === "needsVerification" ? "warning" : "error",
        title: () => (formatAppError(failure)),
        persistent: true,
      });
    }
  }, [notifyM6, operationState, reportAutostash, notifyAutostashSaved]);

  const performAutostashRestore = useCallback(async () => {
    const captured = currentM6;
    const confirmation = captured?.pending?.confirmationState;
    if (!captured?.pending || !confirmation || !remoteTargetIsCurrent(captured.origin, gateRef.current.token())) return;
    const target = captured.origin.worktreeId;
    try {
      const outcome = await performM6Write(target, () => ipc.autostashRestore(target, captured.pending!.link, confirmation), (result) => {
        if (result) notifyAutostashSaved(target, result.link, result.removed);
      });
      if (reportAutostash(target, outcome)) {
        // Saved changes remain available through the linked stash entry.
      } else if (outcome?.restored) {
        notifyM6(target, { tone: "success", title: () => (messages.operation.autostashRestored) });
      }
    } catch (caught) {
      const failure = toAppError(caught);
      notifyM6(target, {
        tone: failure.kind === "needsVerification" ? "warning" : "error",
        title: () => (formatAppError(failure)),
        persistent: true,
      });
    }
  }, [currentM6, notifyM6, reportAutostash, notifyAutostashSaved]);

  const keepAutostash = useCallback(async () => {
    const captured = currentM6;
    const link = captured?.pending?.link;
    if (!captured || !link || !remoteTargetIsCurrent(captured.origin, gateRef.current.token())) return;
    const target = captured.origin.worktreeId;
    try {
      await performM6Write(target, () => ipc.autostashForget(target, link.id));
      notifyM6(target, { tone: "info", title: () => messages.operation.autostashKept, action: savedStashAction(target, link) });
    } catch (caught) {
      const error = toAppError(caught);
      notifyM6(target, { tone: "warning", title: () => formatAppError(error), persistent: true });
    }
  }, [currentM6, notifyM6, savedStashAction]);

  /** 日志右键菜单打开改写对话框；reword 先取完整正文（只读，不改工作区）。 */
  const openHistoryAction = useCallback(async (
    action: LogHistoryAction,
    commit: HistoryCommit,
    commits: HistoryCommit[],
    selection: HistoryCommit[],
    origin: RemoteTarget | null,
  ) => {
    const sequence = ++historyActionSeqRef.current;
    if (!origin || !remoteTargetIsCurrent(origin, gateRef.current.token())) {
      notifyM6(origin?.worktreeId ?? worktreeIdRef.current ?? "", { tone: "warning", title: () => (messages.commit.stale) });
      return;
    }
    const head = branches.overview?.headCommit ?? null;
    if (!head) {
      notifyM6(origin?.worktreeId ?? worktreeIdRef.current ?? "", { tone: "warning", title: () => (messages.historyActions.noHead) });
      return;
    }
    let rangeCommits = commits;
    if (action === "interactive" || action === "reword") {
      try {
        rangeCommits = await ipc.rewriteRange(origin.worktreeId, commit.oid, head);
      } catch (caught) {
        if (sequence === historyActionSeqRef.current && remoteTargetIsCurrent(origin, gateRef.current.token())) {
          notifyM6(origin?.worktreeId ?? worktreeIdRef.current ?? "", { tone: "warning", title: () => formatAppError(toAppError(caught)) });
        }
        return;
      }
    }
    let message = commit.subject;
    if (action === "reword" && historyTargetId) {
      try {
        message = (await ipc.historyCommit(historyTargetId, commit.oid)).message;
      } catch (caught) {
        if (sequence !== historyActionSeqRef.current || !remoteTargetIsCurrent(origin, gateRef.current.token())) return;
        notifyM6(origin?.worktreeId ?? worktreeIdRef.current ?? "", { tone: "error", title: () => (formatAppError(toAppError(caught))), persistent: true });
        return;
      }
    }
    if (sequence !== historyActionSeqRef.current || !remoteTargetIsCurrent(origin, gateRef.current.token())) return;
    setHistoryMessage(message);
    setHistoryAction({ origin, action, commit, commits: rangeCommits, selection, head });
  }, [branches.overview?.headCommit, historyTargetId, notifyM6]);

  const runHistoryRewrite = useCallback(async (origin: RemoteTarget, run: () => Promise<RewriteOutcome>) => {
    if (!remoteTargetIsCurrent(origin, gateRef.current.token())) {
      setHistoryAction(null);
      notifyM6(origin.worktreeId, { tone: "warning", title: () => (messages.commit.stale) });
      return;
    }
    let received: RewriteOutcome | null = null;
    try {
      const outcome = await performM6Write(origin.worktreeId, run, (result) => {
        received = result;
        if (result.autostash) notifyAutostashSaved(origin.worktreeId, result.autostash.link, result.autostash.removed);
      });
      setHistoryAction((current) => current?.origin.worktreeId === origin.worktreeId && current.origin.activation === origin.activation ? null : current);
      const detail = () => rewriteOutcomeDetail(outcome);
      if (outcome.error) {
        notifyM6(origin.worktreeId, {
          tone: outcome.error.kind === "needsVerification" ? "warning" : "error",
          title: () => (formatAppError(outcome.error!)),
          detail: () => (detail() ?? messages.historyActions.conflictHint),
          persistent: true,
        });
      } else if (!outcome.completed) {
        notifyM6(origin.worktreeId, { tone: "warning", title: () => (messages.historyActions.needsContinue), detail, persistent: true });
      } else if (reportAutostash(origin.worktreeId, outcome.autostash)) {
        // The operation completed, while local changes still need attention.
      } else {
        notifyM6(origin.worktreeId, { tone: "success", title: () => (messages.historyActions.completed), detail });
      }
    } catch (caught) {
      if (!received) throw caught;
      // The backend has already returned a receipt. Close only its original
      // confirmation and report the known boundary with the readback failure.
      setHistoryAction((current) => current?.origin.worktreeId === origin.worktreeId && current.origin.activation === origin.activation ? null : current);
      notifyM6(origin.worktreeId, rewriteFailureNotice(received, toAppError(caught)));
    }
  }, [notifyM6, reportAutostash, notifyAutostashSaved]);

  /** 从文件历史跳到日志中的同一提交：用哈希筛选，避免提交不在当前分页里。 */
  const jumpToCommit = useCallback((oid: string) => {
    setMainView("log");
    logWorkspace.update({
      selectedCommit: oid,
      scrollTop: 0,
      filter: filterForCommit(oid),
    });
  }, [logWorkspace]);

  /** 复制到剪贴板（提交哈希等）：失败不打断操作。 */
  const copyText = useCallback((text: string) => {
    void navigator.clipboard?.writeText(text).catch(() => undefined);
  }, []);

  /** 打开提交 diff 页签（同一提交同一文件复用已有页签）。 */
  const openCommitFileTab = useCallback(
    (request: { target: HistoryFileTarget; title: string }) => {
      if (!historyTargetId || !worktreeId) return;
      commitTabs.open({
        kind: "diff",
        worktreeId,
        targetId: historyTargetId,
        commit: request.target.commit,
        path: request.target.path,
        parent: request.target.parent ?? null,
        title: request.title,
        file: request.target,
      });
      setMainView("commit");
    },
    [commitTabs, historyTargetId, worktreeId],
  );

  /** 打开 merge 提交的三栏只读视图（Q147）。 */
  const openMergeCommitTab = useCallback(
    (request: { commit: string; path: string; title: string }) => {
      if (!historyTargetId || !worktreeId) return;
      commitTabs.open({
        kind: "merge",
        worktreeId,
        targetId: historyTargetId,
        commit: request.commit,
        path: request.path,
        parent: null,
        title: request.title,
      });
      setMainView("commit");
    },
    [commitTabs, historyTargetId, worktreeId],
  );

  // 分支与远端快捷键（B04、B05、B06）：⌘T 更新、⌘⇧K 推送、⌥⌘K 提交并推送。
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (!isPrimaryModifier(event)) return;
      const action = remoteShortcutAction(event, {
        textOrDialog: isTextInputOrDialog(event.target),
        button: isButtonTarget(event.target),
        commitMessage: isCommitMessageTarget(event.target),
        dialogOpen: document.querySelector("[role='dialog'], dialog[open]") !== null,
      });
      if (action === "push") {
        event.preventDefault();
        if (isWriting) return;
        openPushDialog();
        return;
      }
      if (action === "commitAndPush") {
        event.preventDefault();
        if (isWriting) return;
        void performCommit(null, { pushAfter: true });
        return;
      }
      if (action === "update") {
        event.preventDefault();
        if (worktreeIdRef.current) void runUpdate(worktreeIdRef.current);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [isWriting, openPushDialog, performCommit, runUpdate]);

  // ---------------- 渲染 ----------------

  // 当前选中文件会走哪条渲染分支（默认收起 / 渲染 diff）。
  const diffArea = !selectedEntry
    ? "none"
    : selectedEntry.unmerged
      ? "conflict"
      : selectedEntry.changes === "untracked" && selectedEntry.readonlyReason
        ? "readonly"
        : selectedEntry.changes === "untracked" && !selectedEntry.metricsLoaded
          ? "loading"
          : selectedEntry.collapsed && !expanded.has(entryKey(selectedEntry))
            ? "collapsed"
            : selectedEntry.binary
              ? "binary"
              : selectedEntry.hunkCount === 0
                ? "empty"
                : "diff";
  useEffect(() => {
    if (!selectedEntry) return;
    debugLog(
      `渲染决策：entry=${selectedEntry.path} 区域=${diffArea} ` +
        `行数=${selectedEntry.rowCountSplit}/${selectedEntry.rowCountUnified} hunks=${selectedEntry.hunkCount}`,
    );
  }, [diffArea, selectedEntry]);

  // 应用更新状态：启动读一次；有新版本事件时刷新（手动模式不主动查询，由 Rust 侧决定）。
  useEffect(() => {
    let cancelled = false;
    void m7
      .updateStatus()
      .then((status) => {
        if (cancelled) return;
        setUpdateStatus(status);
        setAppUpdateMode(status.mode);
        if (status.available) setUpdateBannerVersion(status.available.version);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(
    () =>
      onM7Events({
        onTerminalOutput: (event) => terminalSessions.output(event),
        onTerminalExit: (event) => terminalSessions.exitedSession(event),
        onListenError: (caught) => setAppError(toAppError(caught), null),
        onUpdateAvailable: (event) => {
          setUpdateBannerVersion(event.version);
          void m7.updateStatus().then(setUpdateStatus).catch(() => undefined);
        },
      }),
    [],
  );

  useEffect(() => () => {
    for (const timer of terminalActivityTimersRef.current.values()) clearTimeout(timer);
    terminalActivityTimersRef.current.clear();
  }, []);

  if (!bootstrapDone) {
    return <div className="flex h-full items-center justify-center text-descriptionforeground">{messages.app.loading}</div>;
  }

  const activeKey = mergeTabPath ? mergeTabKey(mergeTabPath) : selected ? tabKey(selected.path, selected.changes) : null;
  const displayedM6Origin = captureRemoteTarget(gateRef.current.token(), worktreeId);
  const mergeEntry = snapshot?.worktreeId === worktreeId
    ? activeMergeEntry(mergeTabTargetRef.current, snapshot)
    : null;
  const reviewedCount = snapshot?.review.reviewedCount ?? 0;
  const reviewTotal = snapshot?.review.total ?? 0;
  const activeStatus = worktreeStatuses.find((status) => status.id === worktreeId);
  const selectedTreeEntries = treeSelectedEntries();
  // 工具条上的“第 n / m 个文件”：按改动树顺序（返修 A1/Q138）。
  const orderedEntries = snapshot ? entryOrder(snapshot) : [];
  const activeIndex = selected
    ? orderedEntries.findIndex((entry) => entry.path === selected.path && entry.changes === selected.changes)
    : -1;
  const filePosition = { index: activeIndex + 1, total: orderedEntries.length };
  const discardableSelected = (changelistMode && snapshot && changelistSelectedPaths.worktreeId === worktreeId ? snapshot.entries.filter((entry) => changelistSelectedPaths.paths.includes(entry.path)) : selectedTreeEntries).filter(
    (entry) => entry.changes === "unstaged" || entry.changes === "untracked",
  );
  const allReviewed = !!reviewNotice
    && reviewNotice.worktreeId === snapshot?.worktreeId
    && !!snapshot?.review
    && reviewNotice.total === snapshot.review.total
    && reviewNotice.reviewedCount === snapshot.review.reviewedCount
    && snapshot.review.total > 0
    && snapshot.review.reviewedCount === snapshot.review.total;
  const discardValid = discardRequest ? discardRequestValid(discardRequest) : false;

  const expandActive = () => {
    if (selectedEntry) setExpanded((current) => new Set(current).add(entryKey(selectedEntry)));
  };

  const sideBySideOptions: DiffOptions = {
    ignoreWhitespace: ignoreWhitespaceOf(diffOptions.ignoreWhitespace),
    highlight: diffOptions.highlight,
    foldUnchanged: diffOptions.foldUnchanged,
    syncScroll: diffOptions.syncScroll,
    showWhitespace: diffOptions.showWhitespace,
    softWrap: diffOptions.softWrap,
    contextLines: diffOptions.contextLines,
  };

  const settingsValues = {
    themeLight,
    themeDark,
    codeFontFamily: codeFontFamily ?? "",
    fontSize,
    gitPath: gitPath ?? "",
    mergeTool: mergeTool ?? "",
    diffTool: diffTool ?? "",
    language,
    cherryPickKeepEmpty,
    cherryPickIncludeSource,
    sidebarWidth,
    uiZoom,
    treeGroupByDirectory,
    diffContextLines: diffOptions.contextLines,
    diffFoldUnchanged: diffOptions.foldUnchanged,
    diffSyncScroll: diffOptions.syncScroll,
    diffHighlight: diffOptions.highlight,
    diffShowWhitespace: diffOptions.showWhitespace,
    diffSoftWrap: diffOptions.softWrap,
    diffJumpToNextFile: diffOptions.jumpToNextFile,
    diffIgnoreWhitespace: diffOptions.ignoreWhitespace,
    changelistMode,
    commitSubjectLimit: commitChecks.subjectLimit,
    commitBlankLineRequired: commitChecks.blankLineRequired,
    commitBodyLineLimit: commitChecks.bodyLineLimit,
    commitShowRuler: commitChecks.showRuler,
    commitWarnCrlf: commitChecks.warnCrlf,
    commitWarnDetached: commitChecks.warnDetached,
    commitWarnLargeFileBytes: commitChecks.warnLargeFileBytes,
    commitWarnInvalidFilename: commitChecks.warnInvalidFilename,
    notify: notifyKinds,
    mergeAutoApply,
    logColumns: logSettings.columns,
    fileHistoryShowNames,
    stashCompareLocal,
    logLongEdges: logSettings.longEdges,
    logFirstParent: logSettings.firstParent,
    logPreviewPosition: logSettings.previewPosition,
    updateMode: pushSettings.updateMode,
    fetchTagMode: pushSettings.fetchTagMode,
    fetchAutomatically: pushSettings.fetchAutomatically,
    checkRemote: pushSettings.checkRemote,
    autoUpdateOnPushRejection: pushSettings.autoUpdateOnPushRejection,
    pushDialogOnCommit: pushSettings.pushDialogOnCommit,
    pushDialogProtectedOnly: pushSettings.pushDialogProtectedOnly,
    protectedBranches: pushSettings.protectedBranches.join("\n"),
    detectedGitPath: info?.gitPath ?? null,
    appUpdateMode,
    allowPrerelease,
    shortcuts: shortcutsSnapshot,
  };

  const m7Dialogs = (
    <>
      {repoToRemove ? (
        <Modal title={t("repository.remove", { name: repoToRemove.name })} onClose={() => { if (!busy) setRepoToRemove(null); }} width={460}>
          <p className="mb-2 break-all text-descriptionforeground">{repoToRemove.path}</p>
          <p>{t("repository.removeHint")}</p>
          {repoRemovalBlocked ? <p role="alert" className="mt-3 text-descriptionforeground">{t("repository.removeBlocked")}</p> : null}
          {repoRemovalError ? <p role="alert" className="mt-3">{formatAppError(repoRemovalError)}</p> : null}
          <div className="mt-4 flex justify-end gap-2">
            <button type="button" className="toolbar-widget h-7 px-3" disabled={busy} onClick={() => setRepoToRemove(null)}>{t("common.cancel")}</button>
            <button type="button" className="toolbar-widget h-7 px-3" disabled={busy || repoRemovalBlocked} onClick={() => void removeRepo(repoToRemove)}>{t("repository.removeAction")}</button>
          </div>
        </Modal>
      ) : null}
      {editDecision ? <EditDecisionDialog session={editDecision} onDone={finishEditDecision} /> : null}
      {terminalConfirmation ? <Modal title={t("terminal.shutdownTitle")} onClose={() => finishTerminalConfirmation(false)} width={560}>
        <p>{t("terminal.shutdownConfirm", { count: terminalConfirmation.length })}</p>
        <ul className="my-3 max-h-48 overflow-auto text-[12px]">{terminalConfirmation.map((session) => <li key={session.sessionId} className="truncate" title={session.cwd}>{session.worktreePath} · {session.title}</li>)}</ul>
        <div className="flex justify-end gap-2">
          <button type="button" className="toolbar-widget h-7 px-3" onClick={() => finishTerminalConfirmation(false)}>{t("common.cancel")}</button>
          <button type="button" className="toolbar-widget h-7 px-3" onClick={() => finishTerminalConfirmation(true)}>{t("terminal.shutdownYes")}</button>
        </div>
      </Modal> : null}
      <CloneDialog
        open={cloneOpen}
        onClose={() => setCloneOpen(false)}
        onRegistered={(repo) => {
          setCloneOpen(false);
          setRepos((current) => [...current.filter((entry) => entry.id !== repo.id), repo]);
          void openRepo(repo);
        }}
      />
      <DiagnosticsDialog open={diagnosticsOpen} worktreeId={worktreeId} onClose={() => setDiagnosticsOpen(false)} />
      <UpdatePanel
        open={updateOpen}
        status={updateStatus}
        mode={appUpdateMode}
        onModeChange={(mode) => void updateSettings({ appUpdateMode: mode })}
        onStatusChange={(status) => {
          setUpdateStatus(status);
          if (!status.available) setUpdateBannerVersion(null);
        }}
        onInstall={() => requestUpdateLifecycle("installUpdate")}
        onRestart={() => requestUpdateLifecycle("restartUpdate")}
        onClose={() => setUpdateOpen(false)}
      />
      {updateInstallConfirmOpen ? (
        <Modal
          title={t("update.installAndRestartTitle")}
          width={440}
          onClose={cancelUpdateInstallAndRestart}
        >
          <div className="space-y-3 text-[12.5px]">
            <p>{t("update.installAndRestartBody")}</p>
            <div className="flex justify-end gap-2">
              <button type="button" className="toolbar-widget h-7 px-3" onClick={cancelUpdateInstallAndRestart}>
                {t("common.cancel")}
              </button>
              <button type="button" className="toolbar-widget h-7 px-3 font-semibold" onClick={confirmUpdateInstallAndRestart}>
                {t("update.installAndRestartConfirm")}
              </button>
            </div>
          </div>
        </Modal>
      ) : null}
    </>
  );

  const hasActiveWorktree = !!worktreeId && !!activeRepo?.worktrees.some((entry) => entry.id === worktreeId);
  const titleBar = (
      <TitleBar
        repo={activeRepo}
        repos={repos}
        worktrees={activeRepo?.worktrees ?? []}
        statuses={worktreeStatuses}
        activeWorktreeId={worktreeId}
        busy={busy}
        onSelectRepo={(repo) => void openRepo(repo)}
        onRemoveRepo={(repo) => { setRepoRemovalError(null); setRepoToRemove(repo); }}
        onChooseRepo={() => void chooseRepo()}
        onWorktree={(id) => void switchWorktree(id)}
        onSettings={() => setSettingsOpen(true)}
        onRefreshWorktrees={() => void refreshWorktreeStatuses()}
        branchWidget={hasActiveWorktree ? (
          <BranchWidget
            overview={branches.overview}
            loading={branches.loading}
            error={branches.error}
            incoming={branches.incoming}
            busy={isWriting}
            onRefresh={branches.reload}
            onCheckout={(target) => void runCheckout(target, "plain")}
            onNewBranch={openNewBranch}
            onFetch={() => { if (worktreeId) void runFetch(worktreeId, "user"); }}
          />
        ) : null}
        extra={(
          <>
          <SyncActions busy={busy || isWriting || !hasActiveWorktree} onUpdate={() => { if (worktreeId) void runUpdate(worktreeId); }} onPush={() => void openPushDialog()} />
          <ToolbarGroup
            id="tools"
            label={messages.toolbar.toolWindows}
            actions={[
              {
                id: "terminal",
                icon: "terminal",
                label: t("toolbar.terminal"),
                pressed: terminalOpen,
                disabled: busy || !hasActiveWorktree,
                onClick: () => setTerminalOpen((value) => !value),
              },
              {
                id: "clone",
                icon: "repo-clone",
                label: t("toolbar.clone"),
                disabled: busy,
                onClick: () => setCloneOpen(true),
              },
              {
                id: "github",
                icon: "git-pull-request",
                label: t("toolbar.pullRequests"),
                pressed: githubOpen,
                disabled: busy || !hasActiveWorktree,
                onClick: () => setGithubOpen(true),
              },
              {
                id: "diagnostics",
                icon: "debug-alt",
                label: t("toolbar.diagnostics"),
                onClick: () => setDiagnosticsOpen(true),
              },
              {
                id: "update",
                icon: "cloud-download",
                label: t("toolbar.update"),
                attention: !!updateBannerVersion,
                onClick: () => setUpdateOpen(true),
              },
            ]}
          />
          </>
        )}
      />
  );

  if (!activeRepo) {
    return (
      <div className="welcome-shell flex h-full flex-col overflow-hidden bg-[var(--editor-background)]">
        {titleBar}
        <WelcomeScreen
          repos={repos}
          busy={busy}
          onChooseRepo={() => void chooseRepo()}
          onSelectRepo={(repo) => void openRepo(repo)}
          onCloneRepo={() => setCloneOpen(true)}
          onRemoveRepo={(repo) => { setRepoRemovalError(null); setRepoToRemove(repo); }}
          error={error ? <ErrorBanner error={error} onDismiss={dismissError} /> : null}
        />
        {m7Dialogs}
        <NotificationStack notices={allNotices} onDismiss={dismissAllNotices} />
        {settingsOpen ? (
          <SettingsDialog
            open
            values={settingsValues}
            onClose={() => setSettingsOpen(false)}
            onChange={updateSettings}
            onReloadEnvironment={reloadEnvironment}
            worktreeId={worktreeId}
            updateStatus={updateStatus}
            onUpdateStatus={setUpdateStatus}
            onThemesChanged={reloadImportedThemes}
          />
        ) : null}
        {closeRequest && closeRequest.action !== "proceed" ? (
          <CloseRequestDialog
            intent={closeIntentRef.current}
            writing={closeRequest.writing}
            pending={"pending" in closeRequest ? closeRequest.pending : []}
            onWait={() => applyEffect(closeDriver.handleChoice("wait"))}
            onStay={() => applyEffect(closeDriver.handleChoice("stay"))}
            onPendingReviewed={() => void confirmPendingReviewed()}
          />
        ) : null}
      </div>
    );
  }

  return (
    <div className="glass-host grid h-full grid-rows-[var(--titlebar-height)_minmax(0,1fr)_var(--statusbar-height)] overflow-hidden">
      <GlassLayer variant="window" />
      {titleBar}

      <div className="shell-body flex min-h-0 overflow-hidden">
        {/* 原生玻璃下侧栏占满左列（红绿灯在它顶上），顶部留白可以拖动窗口。 */}
        <div className="sidebar-pane" data-tauri-drag-region>
        <ActivityBar
          pressed={sidebarVisible}
          count={new Set((snapshot?.entries ?? []).map((entry) => entry.path)).size}
          onToggle={() => setSidebarVisible((value) => !value)}
        />
        {sidebarVisible ? (
          <aside className="changes-sidebar relative flex min-h-0 shrink-0 flex-col border-r border-[var(--sidebar-border)]" style={{ width: sidebarWidth }}>
            <div
              className="sidebar-resizer"
              role="separator"
              aria-orientation="vertical"
              aria-label={messages.changes.resizeSidebar}
              onPointerDown={(event) => {
                const sidebar = event.currentTarget.parentElement;
                // 侧栏包在 .sidebar-pane 里，原生玻璃下外层行是 display: contents：按整个窗口外壳算可用宽度。
                const pane = sidebar?.closest<HTMLElement>(".sidebar-pane");
                const row = pane?.closest<HTMLElement>(".glass-host");
                if (!sidebar?.offsetWidth || !pane || !row) return;
                const activityWidth = sidebar.previousElementSibling?.getBoundingClientRect().width ?? 0;
                const scale = sidebar.getBoundingClientRect().width / sidebar.offsetWidth;
                const editorMinWidth = Number.parseFloat(getComputedStyle(sidebar).getPropertyValue("--editor-min-width"));
                const paneStyle = getComputedStyle(pane);
                const paneMargins = (Number.parseFloat(paneStyle.marginLeft) || 0) + (Number.parseFloat(paneStyle.marginRight) || 0);
                sidebarDragRef.current = {
                  pointerId: event.pointerId, clientX: event.clientX, width: sidebar.offsetWidth, scale,
                  maxWidth: Math.min(720, row.clientWidth - paneMargins - activityWidth / scale - editorMinWidth),
                  latestWidth: sidebar.offsetWidth,
                };
                event.currentTarget.setPointerCapture(event.pointerId);
                event.currentTarget.dataset.dragging = "true";
                event.preventDefault();
              }}
              onPointerMove={(event) => {
                const drag = sidebarDragRef.current;
                if (!drag || drag.pointerId !== event.pointerId) return;
                // clientX 是缩放后的坐标；用拖动起点的实际比例换回布局宽度。
                drag.latestWidth = Math.round(Math.min(drag.maxWidth, Math.max(220,
                  drag.width + (event.clientX - drag.clientX) / drag.scale,
                )));
                setSidebarWidth(drag.latestWidth);
              }}
              onPointerUp={(event) => {
                const drag = sidebarDragRef.current;
                if (!drag || drag.pointerId !== event.pointerId) return;
                sidebarDragRef.current = null;
                delete event.currentTarget.dataset.dragging;
                event.currentTarget.releasePointerCapture(event.pointerId);
                void ipc.updateSettings({ sidebarWidth: drag.latestWidth }).catch(() => {});
              }}
              onLostPointerCapture={(event) => {
                sidebarDragRef.current = null;
                delete event.currentTarget.dataset.dragging;
              }}
            />
            <div className="flex h-[var(--tab-height)] shrink-0 items-center gap-3 pr-2 pl-3">
              <button
                type="button"
                className="side-tab"
                aria-selected={sideTab === "commit"}
                onClick={() => setSideTab("commit")}
              >
                {messages.commit.tab}
              </button>
              <button
                type="button"
                className="side-tab"
                aria-selected={sideTab === "stash"}
                onClick={() => {
                  setStashFocusOid(null);
                  setSideTab("stash");
                }}
              >
                {messages.stash.title}
              </button>
              <button
                type="button"
                className="side-tab"
                aria-selected={sideTab === "recovery"}
                onClick={() => setSideTab("recovery")}
              >
                {messages.recovery.title}
              </button>
              {sideTab === "commit" ? (
                <div className="ml-auto flex items-center gap-0.5">
                  <button
                    type="button"
                    className="icon-action size-[22px]"
                    title={messages.changes.refresh}
                    aria-label={messages.changes.refresh}
                    disabled={busy}
                    onClick={() => void refresh()}
                  >
                    <Codicon name="refresh" size={15} />
                  </button>
                  <button
                    type="button"
                    className="icon-action size-[22px]"
                    title={messages.changes.discardSelected}
                    aria-label={messages.changes.discardSelected}
                    disabled={isWriting || discardableSelected.length === 0}
                    onClick={() => requestDiscardEntries(discardableSelected)}
                  >
                    <Codicon name="discard" size={15} />
                  </button>
                  <button
                    type="button"
                    className="icon-action size-[22px]"
                    title={messages.changes.expandAll}
                    aria-label={messages.changes.expandAll}
                    onClick={() => setGroupOpen({ conflicts: true, staged: true, unstaged: true, untracked: true })}
                  >
                    <Codicon name="expand-all" size={15} />
                  </button>
                  <button
                    type="button"
                    className="icon-action size-[22px]"
                    title={messages.changes.collapseAll}
                    aria-label={messages.changes.collapseAll}
                    onClick={() => setGroupOpen({ conflicts: false, staged: false, unstaged: false, untracked: false })}
                  >
                    <Codicon name="collapse-all" size={15} />
                  </button>
                  <SidebarMore
                    onSelectAll={() => changelistMode ? changelistControls.current?.selectAll() : selectAllEntries()}
                    onRefresh={() => void refresh()}
                    onCloseAll={closeAllTabs}
                    groupByDirectory={treeGroupByDirectory}
                    onToggleGroupByDirectory={() => {
                      const next = !treeGroupByDirectory;
                      setTreeGroupByDirectory(next);
                      void ipc.updateSettings({ treeGroupByDirectory: next }).catch(() => {});
                    }}
                  />
                </div>
              ) : null}
            </div>

            {sideTab === "commit" ? (
              <>
                {changelistMode && worktreeId && snapshot ? (
                  <Changelists key={`${worktreeId}:${gateRef.current.token().activation}`} worktreeId={worktreeId} revision={snapshot.revision} busy={isWriting || busy} onView={acceptChangelistView} controls={changelistControls} onSelection={acceptChangelistSelection} snapshot={snapshot} onActivate={activateTreeEntry} onPin={pinEntry} onDiscard={(paths) => requestDiscardEntries(snapshot.entries.filter((entry) => paths.includes(entry.path) && (entry.changes === "unstaged" || entry.changes === "untracked")))} />
                ) : snapshot ? (
                  <ChangesTree
                    snapshot={snapshot}
                    active={selected}
                    selectedKeys={treeSelection}
                    open={groupOpen}
                    groupByDirectory={treeGroupByDirectory}
                    dirOpen={dirOpen}
                    isWriting={isWriting}
                    onActivate={activateTreeEntry}
                    onPin={pinEntry}
                    onMove={moveTreeSelection}
                    onSelectAll={selectAllEntries}
                    onToggleGroup={toggleGroup}
                    onResolveConflicts={() => setConflictDialogOpen(true)}
                    onToggleDir={toggleDir}
                    onStageEntries={(entries) => void performBatchWrite("stage", entries)}
                    onUnstageEntries={(entries) => void performBatchWrite("unstage", entries)}
                    onStageEntry={(entry) => void stageFile(entry)}
                    onUnstageEntry={(entry) => void unstageFile(entry)}
                    onDiscardEntry={(entry) => requestDiscardEntries(expandToSelection(entry))}
                  />
                ) : (
                  <div className="px-3 py-2 text-descriptionforeground">{messages.changes.loading}</div>
                )}
                {worktreeId ? (
                  <CommitArea
                    context={changelistMode && commitContext ? { ...commitContext, gates: { ...commitContext.gates, hasStagedChanges: changelistView?.worktreeId === worktreeId && changelistView.items.some((item) => item.selected && !item.stale) === true } } : commitContext}
                    message={commitInput.message}
                    amend={commitInput.amend}
                    skipHooks={commitInput.skipHooks}
                    busy={busy || isWriting}
                    submitting={commitSubmitting}
                    reviewedCount={reviewedCount}
                    totalFiles={reviewTotal}
                    draftNotice={commitContextError ? formatAppError(commitContextError) : typeof commitDraftNotice === "function" ? commitDraftNotice() ?? null : commitDraftNotice}
                    templateError={commitContext?.templateError ? formatAppError(commitContext.templateError) : null}
                    amendConfirm={amendConfirm}
                    liveOutput={
                      commitLiveOutput && commitLiveOutput.worktreeId === worktreeId
                        ? commitLiveOutput.output
                        : null
                    }
                    checks={commitChecks}
                    onMessage={(value) => updateCommitInput({ ...commitInputRef.current, message: value })}
                    onAmend={toggleAmend}
                    onSkipHooks={(value) => updateCommitInput(
                      { ...commitInputRef.current, skipHooks: value },
                      { persist: false },
                    )}
                    onSubmit={() => void performCommit()}
                    onSubmitAndPush={() => void performCommit(null, { pushAfter: true })}
                    onPickRecent={pickRecentMessage}
                    onAmendConfirm={confirmAmend}
                    onAmendCancel={() => setAmendConfirm(null)}
                  />
                ) : null}
              </>
            ) : sideTab === "stash" ? (
              <StashPanel
                key={`${worktreeId}:${gateRef.current.token().activation}`}
                worktreeId={worktreeId}
                busy={isWriting || m6Busy}
                activation={gateRef.current.token().activation}
                refreshKey={`${stashRevision}:${watchRevision}:${refsRevision}`}
                focusOid={stashFocusOid}
                compareLocal={stashCompareLocal}
                onCompareLocal={(value) => updateSettings({ stashCompareLocal: value })}
                onSave={(message, keepIndex, expectedState) => {
                  if (!worktreeId) return Promise.resolve();
                  return performM6Write(worktreeId, () => ipc.stashSave(worktreeId, { message, keepIndex, expectedState })).then(() => undefined);
                }}
                onApply={async (entry, options) => {
                  if (!worktreeId) return;
                  const origin = captureRemoteTarget(gateRef.current.token(), worktreeId);
                  if (!origin) return;
                  const outcome = await performM6Write(worktreeId, () => ipc.stashApply(worktreeId, {
                    oid: entry.oid,
                    entryKey: entry.key,
                    restoreIndex: options.restoreIndex,
                    deleteAfter: options.deleteAfter,
                    newBranch: options.newBranch,
                    expectedState: options.expectedState,
                  }));
                  if (outcome?.conflicted && remoteTargetIsCurrent(origin, gateRef.current.token())) {
                    setSideTab("commit");
                    setConflictDialogOpen(true);
                  }
                  return outcome;
                }}
                onDrop={(entry, expectedState) => {
                  if (!worktreeId) return Promise.resolve();
                  return performM6Write(worktreeId, () => ipc.stashDrop(worktreeId, entry, expectedState)).then(() => undefined);
                }}
                onClear={(oids, expectedState) => {
                  if (!worktreeId) return Promise.resolve();
                  return performM6Write(worktreeId, () => ipc.stashClear(worktreeId, oids, expectedState)).then(() => undefined);
                }}
                onRestoreArchive={(archive, expectedState) => {
                  if (!worktreeId) return Promise.resolve();
                  return performM6Write(worktreeId, () => ipc.stashArchiveRestore(worktreeId, archive.id, expectedState)).then(() => undefined);
                }}
                onRefresh={() => setStashRevision((value) => value + 1)}
                onNotify={(notice) => notifyM6(worktreeId ?? "", notice)}
              />
            ) : (
              <RecoveryPanel
                worktreeId={worktreeId}
                isWriting={isWriting}
                snapshotRevision={snapshot?.revision ?? 0}
                activation={gateRef.current.token().activation}
                isWritingFor={(id) => writingWorktrees.has(id)}
                onRestore={performRestore}
                onSaveAs={performSaveAs}
                onNotify={(notice) => notify({ ...notice, kind: "recovery" })}
              />
            )}
          </aside>
        ) : null}
        </div>

        <main className="relative flex min-w-0 flex-1 flex-col overflow-hidden bg-[var(--editor-background)]">
          {updateBannerVersion ? (
            <UpdateBanner version={updateBannerVersion} onOpen={() => setUpdateOpen(true)} onDismiss={() => setUpdateBannerVersion(null)} />
          ) : null}
          <OperationBanner
            origin={captureRemoteTarget(gateRef.current.token(), worktreeId)}
            state={operationState}
            autostash={autostashLink}
            autostashError={currentM6?.pending?.error ?? null}
            canRestoreAutostash={!!currentM6?.pending?.confirmationState}
            readErrors={currentM6?.readErrors ?? []}
            busy={isWriting || m6Busy}
            onAction={(action, origin, operation) => void performOperationAction(action, origin, operation)}
            onRestoreAutostash={() => void performAutostashRestore()}
          onKeepStash={() => void keepAutostash()}
            onViewStash={() => {
              if (!autostashLink) return;
              setStashFocusOid(autostashLink.stashOid);
              setSideTab("stash");
            }}
          />
          <EditorTabs
            tabs={tabs}
            activeKey={mainView === "diff" ? activeKey : null}
            entry={selectedEntry}
            view={view}
            canNavigate={diffArea === "diff" && displayChangeCount > 0}
            isWriting={isWriting}
            changeCount={displayChangeCount}
            fileIndex={filePosition.index}
            fileTotal={filePosition.total}
            diffOptions={diffOptions}
            onDiffOption={changeDiffOptions}
            onActivate={(tab) => { setMainView("diff"); activateTab(tab); }}
            onClose={closeTab}
            onCloseAll={closeAllTabs}
            onView={(next) => void switchView(next)}
            onJump={jumpToHunk}
            onFilePrev={() => focusNextFile(-1)}
            onFileNext={() => focusNextFile(1)}
            onToggleReview={toggleReviewCurrent}
            onRefresh={() => void refresh()}
            onFileHistory={(path) => openFileHistory(path, null, displayedM6Origin)}
            onExternalDiff={() => {
              if (selectedEntry) openExternalTool({ kind: "diff", path: selectedEntry.path, revision: null, changesKind: selectedEntry.changes, expectedFingerprint: selectedEntry.fingerprint }, displayedM6Origin);
            }}
            logActive={mainView === "log"}
            onActivateLog={() => setMainView("log")}
            commitTabs={commitTabs.tabs}
            activeCommitKey={mainView === "commit" ? commitTabs.activeKey : null}
            onActivateCommit={(tab) => {
              commitTabs.activate(tab.key);
              setMainView("commit");
            }}
            onCloseCommit={(tab) => commitTabs.close(tab.key)}
          />
          {mainView === "diff" && selectedEntry && !mergeEntry ? (
            <div className="flex min-w-0 items-center gap-1">
              <div className="min-w-0 flex-1">
                <Breadcrumbs
                  entry={selectedEntry}
                  view={view}
                  changeCount={displayChangeCount}
                />
              </div>
              {/* F20：编辑只作用于 split 视图的工作区侧（unstaged）。
               * 统一视图 / 暂存区 / 历史保持只读；统一视图点编辑先切到同文件的并排视图。 */}
              {!selectedEntry.unmerged && !selectedEntry.binary && (selectedEntry.changes === "unstaged" || selectedEntry.changes === "changelist") ? (
                <button
                  type="button"
                  className="toolbar-widget mr-1 shrink-0"
                  aria-pressed={editingWorktreeId === worktreeId && editingPath === selectedEntry.path && view === "split"}
                  title={
                    view !== "split"
                      ? t("workspace.editSwitchToSplit")
                      : editDirty
                        ? t("workspace.editDirty")
                        : t("workspace.edit")
                  }
                  onClick={() => {
                    if (!worktreeId) return;
                    if (editingWorktreeId === worktreeId && editingPath === selectedEntry.path && view === "split") {
                      void closeEdit({ worktreeId, path: selectedEntry.path }); return;
                    }
                    if (view !== "split") void switchView("split");
                    setEditingWorktreeId(worktreeId);
                    setEditingPath(selectedEntry.path);
                  }}
                >
                  <Codicon name={editingWorktreeId === worktreeId && editingPath === selectedEntry.path && view === "split" ? "close" : "edit"} size={13} />
                  <span>{editingWorktreeId === worktreeId && editingPath === selectedEntry.path && view === "split" ? t("common.close") : t("workspace.edit")}</span>
                </button>
              ) : null}
            </div>
          ) : null}
          {mainView === "diff" && error && !mergeEntry ? <ErrorBanner error={error} onDismiss={dismissError} /> : null}
          {mainView === "diff" && allReviewed && !mergeEntry ? (
            <EditorBanner tone="info" icon="pass-filled">
              <span className="font-semibold">{messages.review.allReviewed}</span>
            </EditorBanner>
          ) : null}
          {mainView === "diff" && watchState.error ? (
            <EditorBanner
              tone="warn"
              icon="warning"
              action={{ label: messages.changes.refresh, run: () => void refresh() }}
            >
              <span className="mr-1.5 font-semibold">{messages.changes.watchUnavailable}</span>
              {messages.changes.watchUnavailableDetail}
            </EditorBanner>
          ) : null}
          {mainView === "diff" && diffOptions.ignoreWhitespace === 3 && !mergeEntry ? (
            <EditorBanner tone="info" icon="info">{messages.diff.ignoreWhitespaceReadonly}</EditorBanner>
          ) : null}
          {mainView === "diff" && selectedEntry?.unmerged && !mergeEntry ? (
            <EditorBanner
              tone="error"
              icon="warning"
              action={{ label: messages.diff.resolveConflict, run: () => selectedEntry && openMergeTab(selectedEntry) }}
            >
              {messages.diff.conflictBanner}
            </EditorBanner>
          ) : null}
          {mainView === "diff" && selectedEntry && diffArea === "readonly" && selectedEntry.readonlyReason ? (
            <EditorBanner tone="info" icon="lock">
              <span className="mr-1.5 font-semibold">{messages.changes.readonly}</span>
              {readonlyReasonText(selectedEntry.readonlyReason)}
            </EditorBanner>
          ) : null}
          {mainView === "diff" && selectedEntry && diffArea === "collapsed" ? (
            <EditorBanner
              tone="info"
              icon="info"
              action={{ label: messages.changes.expandDiff, run: expandActive }}
            >
              {`${messages.changes.collapsed}: ${messages.changes.collapsedDetail}`}
            </EditorBanner>
          ) : null}

          <div className="editor-surface min-h-0 min-w-0 flex-1 overflow-hidden">
            {mainView === "diff" ? (editingPath && worktreeId && editingWorktreeId === worktreeId && view === "split" && (selectedEntry?.changes === "unstaged" || selectedEntry?.changes === "changelist") && !selectedEntry.binary && !selectedEntry.unmerged && editingPath === selectedEntry.path ? (
              <EditableWorkspace
                key={`${worktreeId}/${editingPath}`}
                session={editSessions.get({ worktreeId, path: editingPath })}
                fontSize={fontSize}
                fontFamily={codeFontFamily}
                blocked={isWriting}
                onClose={() => void closeEdit({ worktreeId, path: editingPath })}
                renderDiff={(editor) => <SideBySideView
                  worktreeId={worktreeId} entry={selectedEntry} fontSize={fontSize} fontFamily={codeFontFamily}
                  options={{ ...sideBySideOptions, foldUnchanged: false, softWrap: false, syncScroll: false }}
                  highlights={highlightStore} isWriting={isWriting} rightEditor={editor}
                />}
              />
            ) : mergeEntry ? (
              <MergeView
                key={`${worktreeId}/${mergeEntry.path}`}
                worktreeId={worktreeId ?? ""}
                path={mergeEntry.path}
                fontSize={fontSize}
                fontFamily={codeFontFamily}
                softWrap={diffOptions.softWrap}
                onClose={() => closeMergeTab(mergeEntry.path)}
                onExternalMerge={(fingerprint) => openExternalTool({ kind: "merge", path: mergeEntry.path, revision: null, changesKind: null, expectedFingerprint: fingerprint }, displayedM6Origin)}
                onWrite={performMergeWrite}
              />
            ) : !selectedEntry ? (
              <Hint title={messages.changes.noSelection} detail={messages.changes.noSelectionDetail} />
            ) : selectedEntry.unmerged ? (
              // 冲突文件（设计 3.2）：只读对照 HEAD 与工作区，解决冲突在合并页签。
              <ErrorBoundary>
                <SideBySideView
                  worktreeId={worktreeId ?? ""}
                  entry={selectedEntry}
                  fontSize={fontSize}
                  fontFamily={codeFontFamily}
                  options={sideBySideOptions}
                  highlights={highlightStore}
                  isWriting={isWriting}
                  handleRef={sbsRef}
                  onLayout={setSideLayout}
                />
              </ErrorBoundary>
            ) : diffArea === "readonly" && selectedEntry.readonlyReason ? (
              <Hint
                title={messages.changes.readonly}
                detail={readonlyReasonText(selectedEntry.readonlyReason)}
              />
            ) : diffArea === "loading" ? (
              <Hint title={messages.changes.loadingFile} />
            ) : diffArea === "collapsed" ? null : selectedEntry.binary ? (
              <Hint
                title={messages.changes.binary}
                detail={selectedEntry.changes === "unstaged" ? messages.changes.binaryDetail : undefined}
              />
            ) : selectedEntry.hunkCount === 0 ? (
              <Hint title={messages.changes.emptyFile} detail={messages.changes.emptyFileDetail} />
            ) : (
              <ErrorBoundary>
                {view === "split" ? (
                  <SideBySideView
                    worktreeId={worktreeId ?? ""}
                    entry={selectedEntry}
                    fontSize={fontSize}
                    fontFamily={codeFontFamily}
                    options={sideBySideOptions}
                    highlights={highlightStore}
                    isWriting={isWriting}
                    handleRef={sbsRef}
                    onLayout={setSideLayout}
                    onStageSegment={changeSegment}
                    onUnstageSegment={changeSegment}
                    onDiscardSegment={requestDiscardSegment}
                  />
                ) : (
                  <DiffView
                    worktreeId={worktreeId ?? ""}
                    entry={selectedEntry}
                    view={view}
                    fontSize={fontSize}
                    fontFamily={codeFontFamily}
                    rows={rowStore}
                    highlights={highlightStore}
                    handleRef={diffRef}
                    isWriting={isWriting}
                    onHunkAction={changeHunk}
                    onHunkDiscard={discardHunk}
                    segmentActionsEnabled={diffOptions.ignoreWhitespace !== 3}
                    softWrap={diffOptions.softWrap}
                    onSegmentAction={changeSegment}
                    onSegmentDiscard={requestDiscardSegment}
                  />
                )}
              </ErrorBoundary>
            )) : null}
            {/* 日志始终挂载（隐藏时保留筛选、选中与滚动位置，L01）。 */}
            <div className={mainView === "log" ? "h-full min-h-0" : "hidden"}>
              <LogView
                key={worktreeId ?? ""}
                worktreeId={worktreeId ?? ""}
                targetId={historyTargetId}
                initialRefs={historyRefs}
                active={mainView === "log"}
                state={logWorkspace.state}
                onState={logWorkspace.update}
                settings={logSettings}
                onSettings={(patch) => {
                  const next = { ...logSettings, ...patch };
                  setLogSettings(next);
                  void ipc
                    .updateSettings({
                      logColumns: next.columns,
                      logLongEdges: next.longEdges,
                      logFirstParent: next.firstParent,
                      logPreviewPosition: next.previewPosition,
                    })
                    .catch(() => undefined);
                }}
                previewOpen={logPreviewOpen}
                onPreviewOpen={setLogPreviewOpen}
                diffPresentation={{ view, options: sideBySideOptions, fontSize, fontFamily: codeFontFamily }}
                groupByDirectory={treeGroupByDirectory}
                currentBranch={snapshot?.branch ?? branches.overview?.currentBranch ?? null}
                headCommit={branches.overview?.headCommit ?? null}
                me={branches.overview?.user ?? null}
                refreshKey={refsRevision}
                refsWatchRevision={watchRevision}
                onOpenCommitFile={openCommitFileTab}
                onOpenMergeCommitFile={openMergeCommitTab}
                onCreateBranchAt={(commit) => {
                  setNewBranchError(null);
                  openNewBranch({ kind: "commit", oid: commit });
                }}
                onFileHistory={(commit, path) => openFileHistory(path, commit, displayedM6Origin)}
                onFileExternalDiff={(commit, path) => openExternalTool({ kind: "diff", path, revision: commit, changesKind: null }, displayedM6Origin)}
                onHistoryAction={(action, commit, commits, selection) => void openHistoryAction(action, commit, commits, selection, displayedM6Origin)}
                onCopy={copyText}
                onNotify={(notice) => notify(notice)}
              />
            </div>
            {mainView === "commit" && commitTabs.active && historyTargetId ? (
              commitTabs.active.kind === "merge" ? (
                <CommitMergeView
                  key={commitTabs.active.key}
                  targetId={historyTargetId}
                  commit={commitTabs.active.commit}
                  path={commitTabs.active.path}
                  title={commitTabs.active.title}
                  fontSize={fontSize}
                  fontFamily={codeFontFamily}
                  onClose={() => commitTabs.close(commitTabs.active?.key ?? "")}
                />
              ) : (
                <CommitDiffView
                  key={commitTabs.active.key}
                  targetId={historyTargetId}
                  file={commitTabs.active.file as HistoryFileTarget}
                  title={commitTabs.active.title}
                  view={view}
                  options={{ ...diffOptions, ignoreWhitespace: ignoreWhitespaceOf(diffOptions.ignoreWhitespace) }}
                  ignoreWhitespace={ignoreWhitespaceOf(diffOptions.ignoreWhitespace)}
                  softWrap={diffOptions.softWrap}
                  fontSize={fontSize}
                  fontFamily={codeFontFamily}
                  onClose={() => commitTabs.close(commitTabs.active?.key ?? "")}
                />
              )
            ) : null}
          </div>

          {terminalOpen && worktreeId ? (
            <TerminalPanel
              worktreeId={worktreeId}
              worktreePath={activeRepo?.worktrees.find((entry) => entry.id === worktreeId)?.path ?? ""}
              fontSize={fontSize}
              fontFamily={codeFontFamily}
              height={terminalHeight}
              onHeightChange={setTerminalHeight}
              registry={terminalSessions}
              busy={isWriting}
              onClose={() => setTerminalOpen(false)}
            />
          ) : null}

          {githubOpen && worktreeId ? (
            <GithubPanel
              key={worktreeId}
              worktreeId={worktreeId}
              worktreePath={activeRepo?.worktrees.find((entry) => entry.id === worktreeId)?.path ?? ""}
              currentBranch={snapshot?.branch ?? null}
              onClose={() => setGithubOpen(false)}
              onCheckout={(request) => performM6Write(request.worktreeId, () => m7.prCheckout(request))}
              onCheckedOut={() => void refresh()}
              onWorkspaceTouched={() => void refresh()}
            />
          ) : null}

          {/* Git 命令日志（B07）：底部面板，默认隐藏。 */}
          {commandLogOpen ? (
            <CommandLogPanel open={commandLogOpen} onClose={() => setCommandLogOpen(false)} />
          ) : null}

          {measurementMode ? (
            <MeasurePanel
              open={panelOpen}
              status={status}
              onToggle={() => setPanelOpen((value) => !value)}
              measurements={measurements}
              onClear={() => setMeasurements([])}
            />
          ) : null}
        </main>
      </div>

      <StatusBar
        branch={activeStatus?.branch ?? snapshot?.branch ?? null}
        ahead={activeStatus?.ahead ?? 0}
        behind={activeStatus?.behind ?? 0}
        watchError={watchState.error}
        reviewed={reviewedCount}
        total={reviewTotal}
        onRefresh={() => void refresh()}
        onOpenCommandLog={() => setCommandLogOpen((value) => !value)}
        autoFetchError={autoFetchError ? formatAppError(autoFetchError) : null}
      />

      <NotificationStack notices={allNotices} onDismiss={dismissAllNotices} />

      {historyAction && worktreeId ? (
        <HistoryActionDialog
          request={historyAction}
          initialMessage={historyMessage}
          cherryPickKeepEmpty={cherryPickKeepEmpty}
          cherryPickIncludeSource={cherryPickIncludeSource}
          busy={isWriting || m6Busy}
          onClose={() => setHistoryAction(null)}
          onRun={(run) => historyAction
            ? runHistoryRewrite(historyAction.origin, run)
            : Promise.resolve()}
          onNotify={(notice) => notifyM6(historyAction.origin.worktreeId, notice)}
        />
      ) : null}

      {fileHistory && remoteTargetIsCurrent(fileHistory.origin, gateRef.current.token()) ? (
        <FileHistoryDialog
          key={`${fileHistory.origin.worktreeId}:${fileHistory.path}:${fileHistory.revision ?? "working"}`}
          worktreeId={fileHistory.origin.worktreeId}
          path={fileHistory.path}
          revision={fileHistory.revision}
          showFileNames={fileHistoryShowNames}
          onClose={() => setFileHistory(null)}
          onJumpCommit={(oid) => {
            jumpToCommit(oid);
            setFileHistory(null);
          }}
          onNotify={(notice) => notifyM6(fileHistory.origin.worktreeId, notice)}
        />
      ) : null}

      {externalTool ? (
        <ExternalToolDialog
          target={externalTool}
          busy={isWriting || m6Busy}
          onClose={() => setExternalTool(null)}
          onLaunch={async (run, tool) => {
            if (!remoteTargetIsCurrent(externalTool.origin, gateRef.current.token())) {
              setExternalTool(null);
              notifyM6(externalTool.origin.worktreeId, { tone: "warning", title: () => (messages.commit.stale) });
              return;
            }
            void ipc.updateSettings(externalTool.kind === "diff" ? { diffTool: tool } : { mergeTool: tool })
              .catch((caught) => {
                const failure = toAppError(caught);
                notifyM6(externalTool.origin.worktreeId, {
                  tone: "warning", title: () => formatAppError(failure), persistent: true,
                });
              });
            const outcome = await performM6Write(externalTool.origin.worktreeId, run);
            setExternalTool((current) => current?.origin.worktreeId === externalTool.origin.worktreeId
              && current.origin.activation === externalTool.origin.activation ? null : current);
            if (outcome.stillConflicted) {
              notifyM6(externalTool.origin.worktreeId, { tone: "warning", title: () => (messages.externalTools.stillConflicted), persistent: true });
            } else if (outcome.exitCode !== null && outcome.exitCode !== 0) {
              notifyM6(externalTool.origin.worktreeId, {
                tone: "warning",
                title: () => (messages.externalTools.failedExit(outcome.tool, outcome.exitCode!)),
                detail: (outcome.stderr || outcome.stdout).trim() || undefined,
                persistent: true,
              });
            } else {
              notifyM6(externalTool.origin.worktreeId, { tone: "success", title: () => (messages.externalTools.finished(outcome.tool)) });
            }
          }}
          onNotify={(notice) => notifyM6(externalTool.origin.worktreeId, notice)}
        />
      ) : null}

      {discardRequest ? (
        <DiscardDialog
          targets={discardRequest.targets}
          worktree={discardRequest.worktree}
          valid={discardValid}
          onClose={() => setDiscardRequest(null)}
          onConfirm={() => {
            if (!discardRequest || !discardRequestValid(discardRequest)) return;
            const request = discardRequest;
            setDiscardRequest(null);
            if (request.segment) {
              void performWrite({ ...request.segment, snapshotRevision: request.snapshotRevision }, true);
            } else if (request.hunk) {
              void performWrite({ ...request.hunk, snapshotRevision: request.snapshotRevision }, true);
            } else {
              void performBatchWrite("discard", request.entries, {
                snapshotRevision: request.snapshotRevision,
              });
            }
          }}
        />
      ) : null}

      {conflictDialogOpen && worktreeId ? (
        <ConflictDialog
          worktreeId={worktreeId}
          onClose={() => setConflictDialogOpen(false)}
          onOpenMerge={(path) => {
            const current = snapshotRef.current;
            const entry = current && (changelistModeRef.current ? changelistEntryForPath(current, path) : current.entries.find((candidate) => candidate.path === path));
            setConflictDialogOpen(false);
            if (entry) openMergeTab(entry);
          }}
          onWrite={performMergeWrite}
        />
      ) : null}

      {commitWarningConfirm ? (
        <CommitWarningDialog
          warnings={commitWarningConfirm.warnings}
          onCancel={() => setCommitWarningConfirm(null)}
          onContinue={() => {
            const pending = commitWarningConfirm;
            setCommitWarningConfirm(null);
            void confirmRemoteAction(pending, gateRef.current.token(), () =>
              performCommit(pending.confirmation, { skipWarnings: true, pushAfter: pending.pushAfter }));
          }}
        />
      ) : null}

      {/* 新建分支（B02）：起点由入口决定（当前 HEAD / 分支 / 日志所选提交）。 */}
      {newBranchStart ? (
        <NewBranchDialog
          overview={branches.overview}
          start={newBranchStart.start}
          busy={isWriting}
          error={newBranchError ? branchFailureDetail(newBranchError) : null}
          onCancel={() => {
            setNewBranchStart(null);
            setNewBranchError(null);
          }}
          onConfirm={(request) => void runCreateBranch(request.name, request.checkout)}
        />
      ) : null}

      {/* 检出被拒：智能检出 / 强制检出（Q150）。 */}
      {checkoutBlocked ? (
        <CheckoutConflictDialog
          target={checkoutBlocked.label}
          files={checkoutBlocked.files}
          busy={isWriting}
          onCancel={() => setCheckoutBlocked(null)}
          onSmart={() => {
            const pending = checkoutBlocked;
            setCheckoutBlocked(null);
            void confirmRemoteAction(pending.origin, gateRef.current.token(), (origin) =>
              runCheckout(pending.target, "smart", origin));
          }}
          onForce={() => {
            const pending = checkoutBlocked;
            setCheckoutBlocked(null);
            void confirmRemoteAction(pending.origin, gateRef.current.token(), (origin) =>
              runCheckout(pending.target, "force", origin));
          }}
        />
      ) : null}

      {/* 推送对话框（B05）。 */}
      {pushDialog ? (
        <PushDialog
          preview={pushDialog.preview}
          busy={isWriting}
          error={pushDialogError}
          onCancel={() => {
            setPushDialog(null);
            setPushDialogError(null);
          }}
          onPush={(request) => {
            const pending = pushDialog;
            setPushDialog(null);
            void confirmRemoteAction(pending.origin, gateRef.current.token(), (origin) =>
              performPush(origin, request));
          }}
        />
      ) : null}

      {pushRejected ? (
        <Modal title={messages.remote.pushRejectedTitle} onClose={() => setPushRejected(null)} width={460}>
          <div className="space-y-3">
            <p className="text-[12.5px]">
              {messages.remote.pushRejectedDetail}
              {`（${pushRejected.request.remote}/${pushRejected.request.branch}）`}
            </p>
            <div className="flex justify-end gap-2">
              <button type="button" className="dialog-button" onClick={() => setPushRejected(null)}>
                {messages.remote.close}
              </button>
              <button
                type="button"
                className="dialog-button primary"
                onClick={() => {
                  const pending = pushRejected;
                  setPushRejected(null);
                  void confirmRemoteAction(pending.origin, gateRef.current.token(), (origin) =>
                    performPush(origin, { ...pending.request, force: "none" }, { alreadyAttempted: true, updateFirst: true }));
                }}
              >
                {messages.remote.updateThenPushConfirm}
              </button>
            </div>
          </div>
        </Modal>
      ) : null}

      {m7Dialogs}

      {settingsOpen ? (
        <SettingsDialog
          open
          values={settingsValues}
          onClose={() => setSettingsOpen(false)}
          onChange={updateSettings}
          onReloadEnvironment={reloadEnvironment}
          worktreeId={worktreeId}
          updateStatus={updateStatus}
          onUpdateStatus={setUpdateStatus}
          onThemesChanged={reloadImportedThemes}
        />
      ) : null}

      {closeRequest && closeRequest.action !== "proceed" ? (
        <CloseRequestDialog
          intent={closeIntentRef.current}
          writing={closeRequest.writing}
          pending={"pending" in closeRequest ? closeRequest.pending : []}
          onWait={() => applyEffect(closeDriver.handleChoice("wait"))}
          onStay={() => applyEffect(closeDriver.handleChoice("stay"))}
          onPendingReviewed={() => void confirmPendingReviewed()}
        />
      ) : null}
    </div>
  );
}

/** 工具窗口“更多”菜单：查看选项（按目录分组，A2）、全选、全部关闭页签、刷新。 */
function SidebarMore({
  onSelectAll,
  onRefresh,
  onCloseAll,
  groupByDirectory,
  onToggleGroupByDirectory,
}: {
  onSelectAll: () => void;
  onRefresh: () => void;
  onCloseAll: () => void;
  groupByDirectory: boolean;
  onToggleGroupByDirectory: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative">
      <Popover
        open={open}
        onOpen={() => setOpen(true)}
        onClose={() => setOpen(false)}
        align="right"
        width={200}
        trigger={
          <button
            type="button"
            className="icon-action size-[22px]"
            title={messages.changes.more}
            aria-label={messages.changes.more}
          >
            <Codicon name="ellipsis" size={15} />
          </button>
        }
      >
        <MenuItem
          onClick={() => {
            setOpen(false);
            onToggleGroupByDirectory();
          }}
        >
          {groupByDirectory ? messages.changes.viewFlat : messages.changes.viewByDirectory}
        </MenuItem>
        <MenuItem
          onClick={() => {
            setOpen(false);
            onSelectAll();
          }}
        >
          {messages.changes.selectAll}
        </MenuItem>
        <MenuItem
          onClick={() => {
            setOpen(false);
            onCloseAll();
          }}
        >
          {messages.changes.closeAllTabs}
        </MenuItem>
        <MenuSeparator />
        <MenuItem
          onClick={() => {
            setOpen(false);
            onRefresh();
          }}
        >
          {messages.changes.refresh}
        </MenuItem>
      </Popover>
    </div>
  );
}

/** 测量与长测通过真实的视图下拉切换视图，走与用户相同的 onChange 路径。 */
function selectViewInUi(view: ViewMode): boolean {
  // 页签工具条上的切换按钮：`data-view-toggle` 是点击后要切到的视图。
  const button = document.querySelector<HTMLButtonElement>(`[data-view-toggle="${view}"]`);
  if (!button) return false;
  button.click();
  return true;
}

/** 编辑区中央的一句提示（VS Code 的空状态写法）：标题、可选说明、可选链接。 */
function Hint({ title, detail, action }: { title: string; detail?: string; action?: { label: string; run: () => void } }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-1.5 px-6 text-center text-descriptionforeground">
      <span className="text-foreground">{title}</span>
      {detail ? <span className="max-w-md">{detail}</span> : null}
      {action ? (
        <button type="button" className="banner-link mt-1" onClick={action.run}>
          {action.label}
        </button>
      ) : null}
    </div>
  );
}

/** 编辑区顶部提示条：信息、警告、错误三种（F07）。 */
function EditorBanner({ tone, icon, action, children }: {
  tone: "info" | "warn" | "error";
  icon: string;
  action?: { label: string; run: () => void };
  children: React.ReactNode;
}) {
  return (
    <div role={tone === "error" ? "alert" : "status"} className="editor-banner shrink-0" data-tone={tone}>
      <Codicon name={icon} size={16} />
      <div className="min-w-0 flex-1">{children}</div>
      {action ? (
        <button type="button" className="banner-link" onClick={action.run}>
          {action.label}
        </button>
      ) : null}
    </div>
  );
}

function ErrorBanner({ error, onDismiss }: { error: AppErrorPayload; onDismiss: () => void }) {
  const title =
    error.kind === "needsVerification"
      ? messages.errors.needsVerification
      : error.kind === "environment"
        ? messages.environment.title
        : error.kind === "git"
          ? messages.errors.gitFailed
          : error.kind === "rejected"
            ? messages.errors.rejected
            : messages.errors.internal;
  const warn = error.kind === "needsVerification" || error.kind === "environment";
  return (
    <EditorBanner
      tone={warn ? "warn" : "error"}
      icon={warn ? "warning" : "error"}
      action={{ label: messages.errors.dismiss, run: onDismiss }}
    >
      <strong className="mr-1.5 font-semibold">{title}</strong>
      <span className={error.kind === "git" ? "font-mono text-xs whitespace-pre-wrap" : "whitespace-pre-wrap"}>{formatAppError(error)}</span>
    </EditorBanner>
  );
}

// x18n-ignore-start
function MeasurePanel({
  open,
  status,
  onToggle,
  measurements,
  onClear,
}: {
  open: boolean;
  /** 测量/长测诊断写入的状态文案（普通界面不再显示）。 */
  status: string;
  onToggle: () => void;
  measurements: Measurement[];
  onClear: () => void;
}) {
  const latest = measurements[0];
  const smallButton = "rounded-[3px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-2 py-0.5 text-[11.5px] hover:bg-toolbar-hoverbackground disabled:opacity-50";
  return (
    <section className="border-t border-panel-border">
      <header className="flex items-center gap-3 px-3 py-1.5 text-xs">
        <button type="button" className={smallButton} onClick={onToggle}>
          {open ? "▾" : "▸"} {messages.measure.title}（{measurements.length}）
        </button>
        <button
          type="button"
          className={smallButton}
          disabled={measurements.length === 0}
          onClick={() => {
            const blob = new Blob([JSON.stringify(measurements, null, 2)], { type: "application/json" });
            const url = URL.createObjectURL(blob);
            const anchor = document.createElement("a");
            anchor.href = url;
            anchor.download = `revia-m1-measure-${Date.now()}.json`;
            anchor.click();
            URL.revokeObjectURL(url);
          }}
        >
          {messages.measure.export}
        </button>
        <button type="button" className={smallButton} disabled={measurements.length === 0} onClick={onClear}>
          {messages.measure.clear}
        </button>
        <span className="ml-auto text-muted-foreground">
          {status ? `${status} · ` : ""}
          {latest ? `${latest.label} · ${latest.startedAt}` : ""}
        </span>
      </header>
      {open && latest ? (
        <div className="max-h-48 overflow-auto px-3 pb-3 text-xs">
          <div className="grid grid-cols-2 gap-x-6 gap-y-1 md:grid-cols-3">
            <Metric label={messages.measure.firstScreen} value={formatMs(latest.firstScreen.fromListChangesMs)} />
            <Metric label={messages.measure.listChanges} value={formatMs(latest.firstScreen.listChangesMs)} />
            <Metric label={messages.measure.firstRows} value={formatMs(latest.firstScreen.firstRowsMs)} />
            <Metric label={messages.measure.render} value={formatMs(latest.firstScreen.renderMs)} />
            <Metric label="行数 / 文件" value={`${latest.firstScreen.rowCount ?? "—"} / ${latest.firstScreen.fileCount ?? "—"}`} />
            <Metric label="窗口可见性" value={`${latest.visibility.state} · rAF/s ${latest.visibility.rafPerSecond.toFixed(1)}`} />
            <Metric label={messages.measure.jump} value={formatMs(latest.jump?.appearMs)} />
            <Metric label="一帧内出现" value={latest.jump ? (latest.jump.withinOneFrame ? "是" : "否") : "—"} />
            <Metric label="滚动模式" value={latest.scroll?.mode ?? "—"} />
            <Metric label={messages.measure.uncovered} value={latest.scroll ? `${latest.scroll.uncoveredFrames} 帧 / 最多 ${latest.scroll.uncoveredMax} 行` : "—"} />
            <Metric label={messages.measure.frames} value={latest.scroll ? `${formatMs(latest.scroll.frameP50Ms)} / ${formatMs(latest.scroll.frameP95Ms)}` : "—"} />
            <Metric label="高亮行数" value={String(latest.highlight?.loadedLines ?? "—")} />
          </div>
        </div>
      ) : null}
    </section>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-mono tabular-nums">{value}</span>
    </div>
  );
}
// x18n-ignore-end

function AmendConfirmationOf(context: CommitContext, input: ComposerInput): AmendConfirmation {
  return {
    upstreamVersion: context.upstreamVersion,
    expectationFingerprint: context.expectation.fingerprint,
    message: input.message,
    amend: input.amend,
    skipHooks: input.skipHooks,
  };
}

/** 设置里的忽略空白档位（0–3）→ Rust 侧的枚举取值（Q140）。 */
function ignoreWhitespaceOf(level: number): IgnoreWhitespace {
  if (level === 1) return "eol";
  if (level === 2) return "all";
  if (level === 3) return "allAndBlankLines";
  return "off";
}

function getDisplayOrderEntries(snapshot: ChangesSnapshot): ChangeEntry[] {
  const unstaged = snapshot.entries.filter((entry) => entry.changes === "unstaged");
  const untracked = snapshot.entries.filter((entry) => entry.changes === "untracked");
  const staged = snapshot.entries.filter((entry) => entry.changes === "staged");
  return [...unstaged, ...untracked, ...staged];
}

/**
 * 轮询等待条件成立。用 MessageChannel 让出而不是 setTimeout：
 * 窗口不可见时定时器会被节流到百毫秒级，会把测量结果算高。
 */
async function waitUntil(predicate: () => boolean, timeoutMs: number): Promise<boolean> {
  const started = performance.now();
  while (performance.now() - started < timeoutMs) {
    if (predicate()) return true;
    await nextTask();
  }
  return predicate();
}

export { toAppError };
