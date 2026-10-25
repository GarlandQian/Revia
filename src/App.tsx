/**
 * 应用外壳：文件列表与只读 diff；M2 提供目标明确的整文件和 hunk 暂存操作。
 */

import { Fragment, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { RecoveryState } from "@/types/bindings/RecoveryState";
import type { ReviewMarks } from "@/types/bindings/ReviewMarks";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";
import type { HunkAction } from "@/types/bindings/HunkAction";
import type { RepoEntry } from "@/types/bindings/RepoEntry";
import type { ViewMode } from "@/types/bindings/ViewMode";
import type { WorktreeEntry } from "@/types/bindings/WorktreeEntry";
import type { WorktreeSessionInfo } from "@/types/bindings/WorktreeSessionInfo";
import {
  ArrowDown,
  ArrowUp,
  ChevronDown,
  CircleCheck,
  CircleX,
  Columns2,
  GitBranch,
  GitCommitHorizontal,
  Inbox,
  Loader2,
  Lock,
  Minus,
  Plus,
  RefreshCwOff,
  RotateCw,
  Rows3,
  TriangleAlert,
  Undo2,
  type LucideIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { ChangesList, type Selection } from "@/components/app/ChangesList";
import { FileTypeIcon } from "@/components/app/FileTypeIcon";
import { CloseRequestDialog, DiscardDialog, RecoveryDialog } from "@/components/app/RecoveryDialogs";
import { logToBackend, setLoggingEnabled } from "@/features/diff/log";
import { ErrorBoundary } from "@/components/app/ErrorBoundary";
import { DiffView, ROW_HEIGHT, type DiffViewHandle } from "@/features/diff/DiffView";
import { HighlightStore } from "@/features/diff/highlightStore";
import { RowWindowStore } from "@/features/diff/rowWindowStore";
import { matchesHunkAction } from "@/features/diff/hunkAction";
import { monitorSoakVisibility, sleep, waitForSoak, type SoakVisibility } from "@/lib/soak";
import { ipc, type SoakFocus } from "@/lib/ipc";
import { startIpcDiagnostics } from "@/lib/ipcDiagnostics";
import { diagnoseJumpDom } from "@/lib/jumpDiagnostics";
import { messages } from "@/lib/i18n/zh-CN";
import { isButtonTarget, isDiscardFileShortcut, isPrimaryModifier, isReviewShortcut, isStageFileShortcut, isTextInputOrDialog, MAIN_TOOLBAR_INSET } from "@/lib/platform";
import { OperationResultGate, RefreshCoordinator, ReviewOverlay, SelectionGate, SnapshotGate, WatchEventGate } from "@/lib/refreshGate";
import { mergePublishedReview, publishReviewSuccess, readThroughReview, ReviewRecoveryScheduler, reviewedSetForNavigation } from "@/lib/refreshGate";
import { CloseCoordinator, CloseDriver, CloseGuardGate, ExitGuardSync, runReviewRecoveryBatch, WriteGuardOrchestrator } from "@/lib/refreshGate";
import type { CloseDriverEffect } from "@/lib/refreshGate";
import type { CloseDecision, CloseIntent } from "@/lib/refreshGate";
import type { ReviewReadAttempt, ReviewRecoveryBatchOutcome } from "@/lib/refreshGate";
import type { OperationContext } from "@/lib/refreshGate";
import { subscribeToForeground } from "@/lib/foregroundRefresh";
import { getErrorMessage, toAppError } from "@/lib/error";
import {
  formatMs,
  measureScroll,
  nextTask,
  nextPaint,
  nowStamp,
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

type StagingTarget = Pick<HunkAction, "worktreeId" | "path" | "changes" | "fingerprint"> & { hunkId?: string };
type DiscardTarget = StagingTarget & { snapshotRevision: string };
type DiscardConfirmation = { target: DiscardTarget; epoch: number; activation: number; worktree: string; section?: string };

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
  const [watchState, setWatchState] = useState<{ active: boolean; error: string | null }>({
    active: false,
    error: null,
  });
  const [selected, setSelected] = useState<Selection | null>(null);
  const [view, setView] = useState<ViewMode>("split");
  const viewRef = useRef(view);
  viewRef.current = view;

  const selectedRef = useRef(selected);
  selectedRef.current = selected;

  const [fontSize, setFontSize] = useState(12);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
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
  const rowStore = useMemo(
    () =>
      new RowWindowStore(async (request) => {
        const slice = await ipc.getRows(request);
        return { rows: slice.rows, rowCount: slice.rowCount, revision: slice.revision, entry: slice.entry };
      }),
    [],
  );
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
  const pendingAnchor = useRef<{ line: number; version: "old" | "new"; view: ViewMode } | null>(null);

  const selectedEntry: ChangeEntry | null = useMemo(() => {
    if (!snapshot || !selected) return null;
    return (
      snapshot.entries.find(
        (entry) => entry.path === selected.path && entry.changes === selected.changes,
      ) ?? null
    );
  }, [selected, snapshot]);

  // ---------------- 打开仓库 / 刷新 ----------------

  const applySnapshot = useCallback((next: ChangesSnapshot) => {
    snapshotRef.current = next;
    setSnapshot(next);
    return next;
  }, []);

  const pickFirstEntry = useCallback((next: ChangesSnapshot, preferred?: string | null): ChangeEntry | null => {
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
      fingerprint: entry.fingerprint, snapshotRevision: entry.snapshotRevision }, rowCount, entry);
    highlightStore.setTarget({ worktreeId: id, path: entry.path, changes: entry.changes,
      fingerprint: entry.fingerprint, snapshotRevision: entry.snapshotRevision });
    return rowCount;
  }, [highlightStore, rowStore]);

  // 所有接收快照的入口都重绑版本；不能因路径、行数相同而沿用旧行和旧 hunk 凭据。
  useLayoutEffect(() => {
    if (selectedEntry && worktreeId && snapshot?.worktreeId === worktreeId) {
      bindDiffTarget(selectedEntry, worktreeId, view);
    }
  }, [bindDiffTarget, selectedEntry, snapshot?.worktreeId, view, worktreeId]);

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
    async (requested: ChangeEntry, target: { view: ViewMode; worktreeId: string; expand?: boolean }) => {
      // 本次选择独占一个上下文：期间又选了别的文件或切走，本次结果一律作废（Q88）。
      selectionEpochRef.current += 1;
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
          throw { kind: "rejected", message: messages.changes.fileChanged };
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
      const covered = await waitUntil(
        () => rowStore.isCovered(0, Math.min(rowCount, DEFAULT_VIEWPORT_ROWS)),
        4000,
      );
      if (!covered) {
        debugLog(`首屏取行超时：${entry.path}（已加载块 ${rowStore.getSnapshot().loadedChunks}）`);
      }
      const paint = await nextPaint();
      const element = document.querySelector<HTMLElement>(".diff-scroll");
      return {
        covered,
        rafPainted: paint.rafPainted,
        viewportHeight: element?.clientHeight ?? null,
        devicePixelRatio: window.devicePixelRatio,
      };
    },
    [bindDiffTarget, rowStore],
  );

  const applyRefreshedSnapshot = useCallback((next: ChangesSnapshot) => {
    const current = selectedRef.current;
    const previous = snapshotRef.current;
    const display = getDisplayOrderEntries(next);
    const oldIndex = current && previous ? getDisplayOrderEntries(previous).findIndex(
      (entry) => entry.path === current.path && entry.changes === current.changes,
    ) : 0;
    const entry = current
      ? display.find((entry) => entry.path === current.path && entry.changes === current.changes)
        ?? display.find((entry) => entry.path === current.path)
        ?? display[Math.min(Math.max(0, oldIndex), display.length - 1)]
      : pickFirstEntry(next);
    applySnapshot(next);
    if (entry) {
      void openEntry(entry, { view: viewRef.current, worktreeId: next.worktreeId })
        .catch((caught) => setAppError(toAppError(caught), next.worktreeId));
    } else {
      setSelected(null);
    }
  }, [applySnapshot, openEntry, pickFirstEntry, setAppError]);

  const openWorktree = useCallback(
    async (id: string, options: { targetPath?: string | null; view?: ViewMode; expand?: boolean } = {}) => {
      const targetView = options.view ?? view;
      setBusy(true);
      const openStart = performance.now();
      // 切换请求发起时立即改激活目标：此后旧 worktree 的响应一律丢弃（含 A→B→A）。
      gateRef.current.activate(id);
      const myToken = gateRef.current.token();
      try {
        const sessionInfo = await ipc.openWorktree(id);
        if (gateRef.current.token().activation !== myToken.activation) return null;
        debugLog(`open_worktree 完成：${Math.round(performance.now() - openStart)} ms`);
        setInfo(sessionInfo);
        setWorktreeId(id);
        const failed = sessionInfo.checks.filter((check) => !check.ok);
        if (failed.length > 0) {
          setAppError({ kind: "environment", message: failed.map((check) => `${check.name}：${check.detail}`).join("\n") }, id);
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
        setStatus(`${next.entries.length} 个文件`);
        const firstRowsStart = performance.now();
        debugLog(`选中文件：${entry.path}（${entry.changes}）`);
        const opened = await openEntry(entry, { view: targetView, worktreeId: id, expand: options.expand });
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
    [applySnapshot, openEntry, pickFirstEntry, setAppError, view],
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
      setStatus(snap.entries.length > 0 ? `${snap.entries.length} 个文件` : messages.changes.empty);
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
            : { kind: "rejected", message: messages.review.recoveryExhausted },
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
    }),
    [readAndAccept, setAppError],
  );

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
        if (notifications.consume(token.activation, snapshot)) refreshCoordinator.request();
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
    const fail = (what: string) => (caught: unknown) => {
      if (disposed) return;
      // 保护安装失败：标记为不可用，写入口会据此拒绝开始写入（ST-B03-08-2）。
      closeGuardsRef.current.markFailed(
        messages.close.guardUnavailable(messages.close.listenerFailed(what, getErrorMessage(caught))),
      );
      setAppError({ kind: "internal", message: closeGuardsRef.current.blockedReason ?? "" }, null);
    };
    // 每次（重）装都从未就绪开始：安装中不允许写入。
    closeGuardsRef.current = new CloseGuardGate();
    void (async () => {
      let windowGuard = false;
      let exitGuard = false;
      try {
        const stop = await getCurrentWindow().onCloseRequested((event) => {
          // 只接收用户意图；绝不把匿名事件当作程序化关闭的回执。
          event.preventDefault();
          const driver = closeDriverRef.current;
          if (!driver || disposed) return;
          applyEffectRef.current?.(driver.handleNativeCloseRequest());
        });
        if (disposed) stop(); else stops.push(stop);
        windowGuard = true;
      } catch (caught) {
        fail(messages.close.guardWindowClose)(caught);
      }
      try {
        // 原生退出：Rust 已阻止退出并通知前端，这里按同一套编排询问用户。
        const stop = await listen("revia://exit-requested", () => {
          if (disposed) return;
          const driver = closeDriverRef.current;
          if (!driver) return;
          applyEffectRef.current?.(driver.handleRequest("exit"));
        });
        if (disposed) stop(); else stops.push(stop);
        exitGuard = true;
      } catch (caught) {
        fail(messages.close.guardNativeExit)(caught);
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
          setAppError({ kind: "rejected", message: messages.firstRun.notARepo });
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
        const [settings, list] = await Promise.all([ipc.listSettings(), ipc.listRepos()]);
        setFontSize(settings.diffFontSize);
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

  // 拖入文件夹即可添加仓库（Q35）
  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== "drop") return;
      const paths = event.payload.paths;
      if (paths.length > 0) void addRepo(paths[0]);
    });
    return () => {
      void unlisten.then((dispose) => dispose());
    };
  }, [addRepo]);

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
      if (next === view || !selectedEntry) {
        setView(next);
        return;
      }
      const handle = diffRef.current;
      const scrollTop = handle?.getScrollTop() ?? 0;
      const anchorRow = Math.floor(scrollTop / ROW_HEIGHT);
      const row = rowStore.getRow(anchorRow);
      const line = row?.left?.line ?? row?.oldLine ?? row?.right?.line ?? row?.newLine ?? null;
      const version: "old" | "new" = row?.left || row?.lineKind === "deletion" ? "old" : "new";
      pendingAnchor.current = line != null ? { line, version, view: next } : null;
      setView(next);
      if (worktreeId) bindDiffTarget(selectedEntry, worktreeId, next);
    },
    [bindDiffTarget, rowStore, selectedEntry, view, worktreeId],
  );

  // 视图切换后把锚点行放回顶部（等数据到位再定位）
  useEffect(() => {
    const anchor = pendingAnchor.current;
    if (!anchor || anchor.view !== view || !selectedEntry) return;
    let cancelled = false;
    void (async () => {
      // 绑定发起时的选择与版本：迟到的行号结果不得挪动已经切换的视口（Q88）。
      const anchorEpoch = selectionEpochRef.current;
      const anchorGate = gateRef.current.token();
      const anchorRevision = selectedEntry.snapshotRevision;
      const index = await ipc.rowIndexForLine({
        worktreeId: worktreeId ?? "",
        path: selectedEntry.path,
        changes: selectedEntry.changes,
        view,
        line: anchor.line,
        version: anchor.version,
      });
      if (cancelled) return;
      if (anchorEpoch !== selectionEpochRef.current
        || gateRef.current.token().activation !== anchorGate.activation
        || selectedRef.current?.path !== selectedEntry.path
        || selectedRef.current?.changes !== selectedEntry.changes
        || snapshotRef.current?.entries.find((entry) => entry.path === selectedEntry.path
          && entry.changes === selectedEntry.changes)?.snapshotRevision !== anchorRevision) {
        debugLog(`丢弃过期的行号定位结果：${selectedEntry.path}`);
        return;
      }
      await waitUntil(() => rowStore.isCovered(index, index + 1), 3000);
      if (cancelled) return;
      diffRef.current?.scrollToRow(index);
      pendingAnchor.current = null;
    })();
    return () => {
      cancelled = true;
    };
  }, [rowStore, selectedEntry, view, worktreeId]);

  const jumpToHunk = useCallback(
    (direction: 1 | -1) => {
      if (!selectedEntry) return;
      const current = Math.floor((diffRef.current?.getScrollTop() ?? 0) / ROW_HEIGHT);
      const candidates = selectedEntry.hunks
        .map((hunk) => (view === "split" ? hunk.rowSplit : hunk.rowUnified))
        .sort((a, b) => a - b);
      const target =
        direction > 0
          ? candidates.find((row) => row > current)
          : [...candidates].reverse().find((row) => row < current);
      if (target != null) diffRef.current?.scrollToRow(target);
    },
    [selectedEntry, view],
  );

  const focusNextFile = useCallback(
    (direction: 1 | -1) => {
      if (!snapshot || !selected) return;
      const index = snapshot.entries.findIndex(
        (entry) => entry.path === selected.path && entry.changes === selected.changes,
      );
      const next = snapshot.entries[index + direction];
      if (next && worktreeId) void openEntry(next, { view, worktreeId });
    },
    [openEntry, selected, snapshot, view, worktreeId],
  );

  const changeFontSize = useCallback(
    async (delta: number) => {
      const next = Math.min(24, Math.max(9, fontSize + delta));
      setFontSize(next);
      try {
        await ipc.setDiffFontSize(next);
      } catch {
        // 字号保存失败不影响阅读
      }
    },
    [fontSize],
  );

  // ---------------- 暂存写操作（Q79/Q89/Q90/Q93） ----------------

  const [writingWorktrees, setWritingWorktrees] = useState<Set<string>>(new Set());
  const writingWorktreesRef = useRef(new Set<string>());
  /**
   * 写入期间的安全关闭/退出编排（任务 08，Q90/Q91/Q96）。
   *
   * 只做决策：保留窗口与进程、等待完成后执行**原请求**、留在应用不取消写、
   * 待核对结果先展示。从不杀 Git、不重跑写命令。
   */
  const closeCoordinatorRef = useRef(new CloseCoordinator());
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
  const closeGuardsRef = useRef(new CloseGuardGate());
  /** reconcile 的转发（监听回调早于其定义注册）。 */
  const reconcileCloseRef = useRef<(() => void) | null>(null);
  const isWriting = worktreeId ? writingWorktrees.has(worktreeId) : false;
  const [discardConfirmation, setDiscardConfirmation] = useState<DiscardConfirmation | null>(null);
  const [recoveryOpen, setRecoveryOpen] = useState(false);
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
  const closeDriverRef = useRef<CloseDriver | null>(null);
  if (closeDriverRef.current === null) {
    closeDriverRef.current = new CloseDriver({
      coordinator: closeCoordinatorRef.current,
      releaseExitGuard: () => syncExitBlocked(false),
      closeWindow: () => getCurrentWindow().destroy(),
      exitApp: () => ipc.requestExit(),
      onError: (caught) => setAppError(toAppError(caught), null),
      onAsk: (intent, writing, pending) => {
        closeIntentRef.current = intent;
        setCloseRequest(
          pending.length > 0 ? { action: "showPending", writing, pending } : { action: "ask", writing },
        );
      },
      onSettled: () => setCloseRequest(null),
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
    void syncExitBlocked(coordinator.needsExitGuard()).catch((caught) => {
      setAppError(toAppError(caught), null);
    });
    applyEffect(closeDriver.notifyStateChanged());
  }, [applyEffect, closeDriver, syncExitBlocked]);

  // 监听回调通过 ref 到达同一驱动。
  reconcileCloseRef.current = reconcileClose;
  applyEffectRef.current = applyEffect;

  /**
   * 三个写入口共用的**共享异步编排**（任务 08）。
   *
   * 依赖全部经 ref/回调读取，所以实例稳定；App 与 Node 回归用同一个类。
   */
  const writeOrchestratorRef = useRef<WriteGuardOrchestrator | null>(null);
  if (writeOrchestratorRef.current === null) {
    writeOrchestratorRef.current = new WriteGuardOrchestrator({
      closeDriver,
      closeBlockedReason: messages.close.closing,
      guardsReady: () => closeGuardsRef.current.ready,
      guardBlockedReason: () => closeGuardsRef.current.blockedReason,
      register: (worktreeId) => {
        closeCoordinatorRef.current.beginWrite(worktreeId);
      },
      guard: () => syncExitBlocked(true),
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

  const performWrite = useCallback(async (target: StagingTarget | DiscardTarget, discard = false) => {
    const opWorktreeId = target.worktreeId;
    // ref 在同一事件循环内也互斥；按钮禁用和 Rust 写锁共同兜底，不积压点击。
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
          { kind: "internal", message: messages.close.guardUnavailable(started.reason) },
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
      if (discard) {
        if (!("snapshotRevision" in target)) throw { kind: "rejected", message: messages.changes.fileChanged };
        const discardRequest = { ...request, snapshotRevision: target.snapshotRevision };
        next = target.hunkId !== undefined
          ? await ipc.discardHunk({ ...discardRequest, hunkId: target.hunkId })
          : await ipc.discardFile(discardRequest);
      } else {
        next = target.hunkId !== undefined
          ? await (unstaging ? ipc.unstageHunk : ipc.stageHunk)({ ...request, hunkId: target.hunkId })
          : await (unstaging ? ipc.unstageFile : ipc.stageFile)(request);
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
      } else {
        debugLog(`写结果快照被接收门丢弃：${next.worktreeId}@${next.revision}`);
      }
    } catch (caught) {
      const err = toAppError(caught);
      needsVerification = err.kind === "needsVerification";
      // 原目标的结果不能因切走而丢失；字典按 worktree 隔离，不污染另一目标。
      if (operationGateRef.current.canRecord(opToken)) {
        setAppError(err, opWorktreeId);
        if (operationGateRef.current.acceptFailure(opToken)
          && (err.kind === "needsVerification" || err.kind === "rejected")) void refresh();
      }
    } finally {
      writingWorktreesRef.current.delete(opWorktreeId);
      setWritingWorktrees(new Set(writingWorktreesRef.current));
      writeOrchestrator.end(opWorktreeId, needsVerification);
    }
  }, [acceptSnapshot, refresh, scheduleReviewRecovery, setAppError, writeOrchestrator]);

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
      throw { kind: "rejected", message: messages.recovery.busy };
    }
    writingWorktreesRef.current.add(opWorktreeId);
    setWritingWorktrees(new Set(writingWorktreesRef.current));
    try {
      // 共享异步编排：失败时已回滚并真实收尾，这里只需如实抛出。
      const started = await writeOrchestrator.begin(opWorktreeId);
      if (!started.ok) {
        writingWorktreesRef.current.delete(opWorktreeId);
        setWritingWorktrees(new Set(writingWorktreesRef.current));
        throw { kind: "internal", message: messages.close.guardUnavailable(started.reason) };
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
        throw { kind: "internal", message: messages.close.guardUnavailable(started.reason) };
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
          throw { kind: "rejected", message: messages.recovery.busy };
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

  const stageFile = useCallback((entry: ChangeEntry) => {
    if (!worktreeId || entry.readonlyReason || entry.changes === "staged") return;
    return performWrite({ worktreeId, path: entry.path, changes: entry.changes, fingerprint: entry.fingerprint });
  }, [performWrite, worktreeId]);

  const unstageFile = useCallback((entry: ChangeEntry) => {
    if (!worktreeId || entry.readonlyReason || entry.changes !== "staged") return;
    return performWrite({ worktreeId, path: entry.path, changes: entry.changes, fingerprint: entry.fingerprint });
  }, [performWrite, worktreeId]);

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
      setAppError({ kind: "rejected", message: messages.review.unconfirmed }, targetWorktreeId);
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

    const unique = uniqueEntries(getDisplayOrderEntries(current));
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
  }, [openEntry, setReviewMark, worktreeId]);

  const changeHunk = useCallback((action: HunkAction) => {
    const current = selectedRef.current;
    const entry = snapshotRef.current?.entries.find(
      (entry) => current && entry.path === current.path && entry.changes === current.changes,
    );
    if (!entry || !matchesHunkAction(action, worktreeIdRef.current ?? "", entry)) {
      setAppError({ kind: "rejected", message: messages.changes.fileChanged }, action.worktreeId);
      if (worktreeIdRef.current === action.worktreeId) void refresh();
      return;
    }
    // 原样传递显示行绑定的目标，不从当前文件元数据重新拼装 hunk 凭据。
    void performWrite(action);
  }, [performWrite, refresh, setAppError]);

  const requestDiscard = useCallback((target: DiscardTarget, section?: string) => {
    const current = selectedRef.current;
    const entry = snapshotRef.current?.entries.find((entry) => entry.path === current?.path && entry.changes === current.changes);
    if (writingWorktreesRef.current.has(target.worktreeId) || target.changes !== "unstaged" || !entry || entry.readonlyReason
      || target.worktreeId !== worktreeIdRef.current || target.path !== entry.path || target.changes !== entry.changes
      || target.fingerprint !== entry.fingerprint || target.snapshotRevision !== entry.snapshotRevision) return;
    setDiscardConfirmation({ target, section, epoch: selectionEpochRef.current, activation: gateRef.current.token().activation,
      worktree: snapshotRef.current?.worktreePath ?? "" });
  }, []);

  const discardFile = useCallback((entry: ChangeEntry) => {
    if (worktreeId) requestDiscard({ worktreeId, path: entry.path, changes: entry.changes,
      fingerprint: entry.fingerprint, snapshotRevision: entry.snapshotRevision });
  }, [requestDiscard, worktreeId]);

  const discardHunk = useCallback((action: HunkAction, section: string) => {
    const current = selectedRef.current;
    const entry = snapshotRef.current?.entries.find((entry) => entry.path === current?.path && entry.changes === current.changes);
    if (!entry || !matchesHunkAction(action, worktreeIdRef.current ?? "", entry)) {
      setAppError({ kind: "rejected", message: messages.changes.fileChanged }, action.worktreeId);
      return;
    }
    requestDiscard(action, section);
  }, [requestDiscard, setAppError]);

  const confirmationValid = useCallback((confirmation: DiscardConfirmation) => {
    const target = confirmation.target;
    const current = selectedRef.current;
    const entry = snapshotRef.current?.entries.find((entry) => entry.path === current?.path && entry.changes === current.changes);
    return !!entry && target.worktreeId === worktreeIdRef.current && target.path === entry.path && target.changes === entry.changes
      && target.fingerprint === entry.fingerprint && target.snapshotRevision === entry.snapshotRevision
      && confirmation.epoch === selectionEpochRef.current && confirmation.activation === gateRef.current.token().activation
      && !writingWorktreesRef.current.has(target.worktreeId);
  }, []);

  // 快捷键（Q19、Q93、审查意见 6）：
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // 只对文本输入控件和弹窗内的按键放行，不穿透到后台文件操作（Q93、审查意见 6）
      if (isTextInputOrDialog(event.target)) return;

      if (isDiscardFileShortcut(event)) {
        event.preventDefault();
        if (selectedEntry && !isWriting) discardFile(selectedEntry);
        return;
      }

      if (isReviewShortcut(event)) {
        // 必须在 ⌘R 刷新分支之前处理，否则会被刷新吞掉（Q93、D04）。
        // 焦点在按钮/勾选框上时不穿透：那类控件用自己的键盘行为（spec 决策 7）。
        if (isButtonTarget(event.target)) return;
        event.preventDefault();
        void reviewNextFile();
        return;
      }

      if (isStageFileShortcut(event)) {
        event.preventDefault();
        if (selectedEntry && !isWriting && !selectedEntry.readonlyReason) {
          if (selectedEntry.changes === "staged") {
            void unstageFile(selectedEntry);
          } else {
            void stageFile(selectedEntry);
          }
        }
        return;
      }

      const primary = isPrimaryModifier(event);
      if (primary && event.key.toLowerCase() === "o") {
        event.preventDefault();
        void chooseRepo();
      } else if (primary && event.key.toLowerCase() === "r") {
        event.preventDefault();
        void refresh();
      } else if (primary && event.key === "+") {
        event.preventDefault();
        void changeFontSize(1);
      } else if (primary && event.key === "-") {
        event.preventDefault();
        void changeFontSize(-1);
      } else if (event.key === "F7") {
        event.preventDefault();
        jumpToHunk(event.shiftKey ? -1 : 1);
      } else if (event.key === "ArrowDown" && primary) {
        event.preventDefault();
        focusNextFile(1);
      } else if (event.key === "ArrowUp" && primary) {
        event.preventDefault();
        focusNextFile(-1);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [changeFontSize, chooseRepo, discardFile, focusNextFile, isWriting, jumpToHunk, refresh, reviewNextFile, selectedEntry, stageFile, unstageFile]);

  // ---------------- 测量 ----------------

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

      // T2：大幅跳转
      let jump: JumpSample | null = null;
      if (element && total > viewportRows * 4) {
        const from = Math.floor((diffRef.current?.getScrollTop() ?? 0) / ROW_HEIGHT);
        const to = Math.floor(total * 0.6);
        const targetWasCached = rowStore.isCovered(to, to + viewportRows);
        const started = performance.now();
        rowStore.prioritize(to, to + viewportRows);
        const prioritizedAt = performance.now();
        diffRef.current?.scrollToRow(to);
        const scrollReturnedAt = performance.now();
        const coveredTarget = await waitUntil(() => rowStore.isCovered(to, to + viewportRows), 4000);
        const dataAt = performance.now();
        await nextTask();
        const taskAt = performance.now();
        const appearMs = taskAt - started;
        debugLog(
          `跳转目标齐备：${coveredTarget}（取数 ${Math.round(dataAt - started)} ms，提交 ${Math.round(appearMs)} ms）`,
        );
        // 保留 M1 总计时；分段记录同步调用与消息任务等待，不能混称纯渲染。
        let warmAppearMs: number | null = null;
        let colorRestoreMs: number | null = null;
        const warmTargetWasCached = rowStore.isCovered(from, from + viewportRows);
        const warmStart = performance.now();
        diffRef.current?.scrollToRow(from);
        const warmScrollReturnedAt = performance.now();
        const warmCovered = await waitUntil(() => rowStore.isCovered(from, from + viewportRows), 4000);
        const warmDataAt = performance.now();
        await nextTask();
        const warmTaskAt = performance.now();
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
        const repo = list[0];
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
          const element = document.querySelector<HTMLElement>(".diff-scroll");
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
          const handle = diffRef.current;
          // 先滚到文件中间，锚点才有意义；等目标行数据到位后再取行号。
          const anchorIndex = Math.floor(target.rowCountSplit * 0.4);
          handle?.scrollToRow(anchorIndex);
          await waitUntil(() => rowStore.isCovered(anchorIndex, anchorIndex + 1), 4000);
          await waitUntil(() => {
            const visible = diffRef.current?.getVisibleRange();
            return !!visible && rowStore.isCovered(visible.from, visible.to);
          }, 4000);
          const beforeTop = handle?.getScrollTop() ?? 0;
          const beforeRow = Math.floor(beforeTop / ROW_HEIGHT);
          const lineOf = (row: typeof beforeAnchor) =>
            row?.left?.line ?? row?.oldLine ?? row?.newLine ?? row?.right?.line ?? null;
          const beforeAnchor = rowStore.getRow(beforeRow);
          const anchorLine = lineOf(beforeAnchor);

          const clickView = (label: string) => {
            selectViewInUi(label === messages.diff.unified ? "unified" : "split");
          };
          // 切换后等锚点行真正落到顶部（pendingAnchor 的定位是异步的）。
          const topLine = () => {
            const top = diffRef.current?.getScrollTop() ?? 0;
            const row = rowStore.getRow(Math.floor(top / ROW_HEIGHT));
            return { top, row, line: row ? lineOf(row) : null };
          };
          clickView("统一");
          await waitUntil(() => rowStore.rowCountValue === target.rowCountUnified, 4000);
          await waitUntil(() => {
            const visible = diffRef.current?.getVisibleRange();
            return !!visible && rowStore.isCovered(visible.from, visible.to);
          }, 4000);
          const unifiedTop = topLine().top;
          const unifiedAnchor = topLine().row;

          clickView("并排");
          await waitUntil(() => rowStore.rowCountValue === target.rowCountSplit, 4000);
          await waitUntil(() => {
            const visible = diffRef.current?.getVisibleRange();
            return !!visible && rowStore.isCovered(visible.from, visible.to);
          }, 4000);
          const splitTop = topLine().top;
          const splitAnchor = topLine().row;

          report.viewSwitch = {
            anchorLineBefore: anchorLine,
            unifiedRows: target.rowCountUnified,
            splitRows: target.rowCountSplit,
            unifiedAnchorLine: lineOf(unifiedAnchor),
            splitAnchorLine: lineOf(splitAnchor),
            unifiedScrollTop: Math.round(unifiedTop),
            splitScrollTop: Math.round(splitTop),
            sameLineAfterRoundTrip: anchorLine != null && lineOf(splitAnchor) === anchorLine,
          };
        }

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
        // 通过工具栏事件调用最新渲染闭包，避免长期任务持有旧的选中项。
        const clickView = async (label: string, rowCount: number) => {
          const target: ViewMode = label === messages.diff.unified ? "unified" : "split";
          const select = selectViewInUi(target);
          if (!select) throw new Error(`找不到视图下拉：${label}`);
          if (!await waitForSoak(() => select.value === target
            && rowStore.rowCountValue === rowCount && rowStore.getSnapshot().loadedChunks > 0, 2000)) {
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
          const settled = await waitForSoak(() => rowStore.getSnapshot().pendingChunks === 0
            && highlightStore.getSnapshot().pending === 0, 1000);
          if (!settled || !diffRef.current || diffRef.current.getScrollTop() !== 0
            || !rowStore.isCovered(0, entry.rowCountSplit)) throw new Error("同阶段内存采样准备失败");
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
            if (expand && entry.rowCountSplit > 0 && !await waitForSoak(() => !rowStore.getSnapshot().error
              && rowStore.rowCountValue > 0 && rowStore.getSnapshot().loadedChunks > 0, 2000)) {
              throw new Error(`等待行数据超时：${entry.path}`);
            }
            await yieldStep();
          };
          await openSplit(focusEntry);
          // 每次挂载的 diff 视口元素只留弱引用；压力取样后统计未回收的个数，
          // 区分“卸载后的 DOM 仍被持有”与“DOM 已回收但原生内存累积”。
          const mountedRoots: WeakRef<Element>[] = [];
          const trackRoot = () => {
            const element = document.querySelector(".diff-scroll");
            if (element && mountedRoots.at(-1)?.deref() !== element) mountedRoots.push(new WeakRef(element));
          };
          const rootSamples: Array<{ cycle: number; tracked: number; alive: number }> = [];
          report.mountedRootSamples = rootSamples;
          let step = 0;
          while (!isTimeUp()) {
            cycle += 1;
            if (document.hidden) await resumeWhenVisible("操作前窗口不可见");
            try {
              const handle = diffRef.current;
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
                rowStore.prioritize(target, target + 40);
                handle!.scrollToRow(target);
                jumps += 1;
                if (!await waitForSoak(() => rowStore.isCovered(target, target + 40), 2000)) {
                  throw new Error(`跳转数据超时：${large.path}`);
                }
                await yieldStep();
                handle!.scrollToRow(0);
                jumps += 1;
                await yieldStep();
              } else {
                await clickView("统一", large.rowCountUnified);
                currentView = "unified";
                viewSwitches += 1;
                await yieldStep();
                await clickView("并排", large.rowCountSplit);
                currentView = "split";
                viewSwitches += 1;
                await yieldStep();
              }
              step += 1;
              await heartbeat(focusEntry.path);
              if (performance.now() - lastCheckpoint >= 90_000
                && startedAt + soakSeconds * 1000 + pausedMs() - performance.now() > 5000) {
                if (focusEntry !== fixed) await openSplit(fixed);
                diffRef.current?.scrollToRow(0);
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

            let handle = diffRef.current;
            // 只有可展开且有行数的文件才等待行数据；二进制或 0 行文件无需等行，直接推进
            if (expand && entry.rowCountSplit > 0) {
              const coveredOk = await waitForSoak(() => !rowStore.getSnapshot().error && rowStore.rowCountValue > 0
                ? rowStore.isCovered(0, 1) || rowStore.getSnapshot().loadedChunks > 0
                : false, 2000);
              if (!coveredOk) {
                throw new Error(`等待行数据超时：${entry.path} ${currentView}`);
              }
            }

            handle = diffRef.current;
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
                rowStore.prioritize(target, target + 40);
                handle.scrollToRow(target);
                jumps += 1;
                debugLog(`[SOAK] jump to ${target}`);
                const jumped = await waitForSoak(() => rowStore.isCovered(target, target + 40), 2000);
                if (!jumped) throw new Error(`跳转数据超时：${entry.path}`);
                await yieldStep();

                handle.scrollToRow(0);
                jumps += 1;
                debugLog(`[SOAK] jump back 0`);
                await yieldStep();
              }

              if (!isTimeUp()) {
                await clickView("统一", entry.rowCountUnified);
                viewSwitches += 1;
                currentView = "unified";
                debugLog(`[SOAK] 已切换到统一视图 ${entry.path}`);
                await yieldStep();

                const unifiedRows = entry.rowCountUnified;
                for (let r = 0; r < Math.min(unifiedRows, 120); r += step) {
                  if (isTimeUp()) break;
                  const live = diffRef.current;
                  if (!live) break;
                  live.scrollToRow(r);
                  scrolls += 1;
                  await yieldStep();
                }
              }

              if (!isTimeUp()) {
                await clickView("并排", entry.rowCountSplit);
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
          diffRef.current?.scrollToRow(0);
          if (!await waitForSoak(() => rowStore.getSnapshot().pendingChunks === 0
            && highlightStore.getSnapshot().pending === 0, 2000)
            || !diffRef.current || diffRef.current.getScrollTop() !== 0
            || !rowStore.isCovered(0, fixedEntry.rowCountSplit)) throw new Error("诊断静置准备失败");
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

  // ---------------- 渲染 ----------------

  // 诊断：记录当前选中文件会走哪条渲染分支（默认收起 / 渲染 diff）。
  const diffArea = !selectedEntry
    ? "none"
    : selectedEntry.unmerged
      ? "conflict"
      : selectedEntry.changes === "untracked" && selectedEntry.readonlyReason
        ? "readonly"
        : selectedEntry.changes === "untracked" && !selectedEntry.metricsLoaded
          ? "loading"
          : selectedEntry.collapsed && !expanded.has(`${selectedEntry.changes}:${selectedEntry.path}`)
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

  if (!bootstrapDone) {
    return <div className="flex h-full items-center justify-center text-fg-2">载入中…</div>;
  }

  if (repos.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-8" data-tauri-drag-region>
        <h1 className="text-[15px] font-semibold">{messages.firstRun.title}</h1>
        <p className="max-w-md text-center text-fg-2">{messages.firstRun.description}</p>
        <Button onClick={() => void chooseRepo()} disabled={busy}>
          {messages.firstRun.addButton}
        </Button>
        <p className="text-xs text-fg-2">{messages.firstRun.dragHint}</p>
        {error ? (
          <div className="island w-full max-w-lg">
            <ErrorBanner error={error} onDismiss={dismissError} />
          </div>
        ) : null}
      </div>
    );
  }

  const collapsed = diffArea === "collapsed";
  const allReviewed = !!reviewNotice
    && reviewNotice.worktreeId === snapshot?.worktreeId
    && !!snapshot?.review
    && reviewNotice.total === snapshot.review.total
    && reviewNotice.reviewedCount === snapshot.review.reviewedCount
    && snapshot.review.total > 0
    && snapshot.review.reviewedCount === snapshot.review.total;

  // 布局照 Rebased（Q107）：主工具栏 / 图标条 + “变更”工具窗口 + diff 编辑区 / 状态栏。
  return (
    <div className="grid h-full grid-cols-[40px_300px_minmax(0,1fr)] grid-rows-[40px_minmax(0,1fr)_24px] overflow-hidden">
      <MainToolbar
        repo={activeRepo}
        worktrees={activeRepo?.worktrees ?? []}
        activeWorktreeId={worktreeId}
        onWorktree={(id) => void switchWorktree(id)}
        onChooseRepo={() => void chooseRepo()}
        busy={busy}
        watchError={watchState.error}
      />
      <ToolStripe recoveryOpen={recoveryOpen} onRecovery={() => setRecoveryOpen(true)} />
      <aside className="island mr-[3px] mb-1 flex flex-col bg-tool">
        <div className="flex h-[34px] shrink-0 items-center px-3 font-semibold">{messages.changes.toolWindow}</div>
        <ChangesToolbar
          selectedEntry={selectedEntry}
          isWriting={isWriting}
          busy={busy}
          review={snapshot?.review ?? null}
          onRefresh={() => void refresh()}
          onStage={(entry) => void stageFile(entry)}
          onUnstage={(entry) => void unstageFile(entry)}
          onDiscard={discardFile}
        />
        {snapshot ? (
          <ChangesList
            snapshot={snapshot}
            selected={selected}
            onSelect={(next) => void selectFromList(next)}
            onStage={(entry) => void stageFile(entry)}
            onUnstage={(entry) => void unstageFile(entry)}
            isWriting={isWriting}
            onToggleReview={(entry, marked) => void setReviewMark(entry, marked)}
          />
        ) : (
          <div className="px-3 py-2 text-fg-2">{messages.changes.loading}</div>
        )}
      </aside>

      <main className="island mr-1.5 mb-1 ml-[3px] flex min-w-0 flex-col bg-island">
        {selectedEntry ? (
          <EditorHeader
            entry={selectedEntry}
            view={view}
            onView={(next) => void switchView(next)}
            canNavigate={diffArea === "diff"}
            onJump={jumpToHunk}
            isWriting={isWriting}
            onStage={(entry) => void stageFile(entry)}
            onUnstage={(entry) => void unstageFile(entry)}
            onDiscard={discardFile}
          />
        ) : null}
        {error ? <ErrorBanner error={error} onDismiss={dismissError} /> : null}
        {allReviewed ? (
          <EditorBanner tone="info" icon={CircleCheck}>
            <span className="font-semibold">{messages.review.allReviewed}</span>
          </EditorBanner>
        ) : null}
        {watchState.error ? (
          <EditorBanner tone="warn" icon={RefreshCwOff} action={{ label: messages.changes.refresh, run: () => void refresh() }}>
            <span className="mr-1.5 font-semibold">{messages.changes.watchUnavailable}</span>
            {messages.changes.watchUnavailableDetail}
          </EditorBanner>
        ) : null}
        {selectedEntry?.unmerged ? (
          <EditorBanner tone="error" icon={TriangleAlert}>{messages.changes.conflict}</EditorBanner>
        ) : null}
        <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
          {!selectedEntry ? (
            <Hint title={messages.changes.noSelection} />
          ) : selectedEntry.unmerged ? (
            <Hint title={messages.changes.conflictTitle} />
          ) : diffArea === "readonly" && selectedEntry.readonlyReason ? (
            <Hint title={messages.changes.readonly} detail={messages.changes.readonlyReasons[selectedEntry.readonlyReason]} />
          ) : diffArea === "loading" ? (
            <Hint title={messages.changes.loadingFile} />
          ) : collapsed ? (
            <Hint
              title={messages.changes.collapsed}
              detail={messages.changes.collapsedDetail}
              action={{ label: messages.changes.expandDiff, run: () => expandEntry(selectedEntry) }}
            />
          ) : selectedEntry.binary ? (
            <Hint
              title={messages.changes.binary}
              detail={selectedEntry.changes === "unstaged" ? messages.changes.binaryDetail : undefined}
            />
          ) : selectedEntry.hunkCount === 0 ? (
            <Hint title={messages.changes.emptyFile} detail={messages.changes.emptyFileDetail} />
          ) : (
            <ErrorBoundary>
              <DiffView
                worktreeId={worktreeId ?? ""}
                entry={selectedEntry}
                view={view}
                fontSize={fontSize}
                rows={rowStore}
                highlights={highlightStore}
                handleRef={diffRef}
                isWriting={isWriting}
                onHunkAction={changeHunk}
                onHunkDiscard={discardHunk}
              />
            </ErrorBoundary>
          )}
        </div>
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

      <StatusBar
        repoName={activeRepo?.name ?? messages.app.name}
        branch={snapshot?.branch ?? null}
        entry={selectedEntry}
        watchError={watchState.error}
      />
      {discardConfirmation ? (
        <DiscardDialog path={discardConfirmation.target.path} worktree={discardConfirmation.worktree} section={discardConfirmation.section}
          valid={confirmationValid(discardConfirmation)} onClose={() => setDiscardConfirmation(null)}
          onConfirm={() => {
            if (!confirmationValid(discardConfirmation)) return;
            setDiscardConfirmation(null);
            void performWrite(discardConfirmation.target, true);
          }} />
      ) : null}
      {recoveryOpen ? (
        <RecoveryDialog
          worktreeId={worktreeId}
          isWriting={isWriting}
          snapshotRevision={snapshot?.revision ?? 0}
          activation={gateRef.current.token().activation}
          isWritingFor={(id) => writingWorktrees.has(id)}
          onRestore={performRestore}
          onSaveAs={performSaveAs}
          onClose={() => setRecoveryOpen(false)}
        />
      ) : null}
      {closeRequest && closeRequest.action !== "proceed" ? (
        <CloseRequestDialog
          intent={closeIntentRef.current}
          writing={closeRequest.writing}
          pending={"pending" in closeRequest ? closeRequest.pending : []}
          onWait={() => {
            // 用户明确选择「等待」：交给共享驱动（可能立刻执行，也可能继续等待）。
            applyEffect(closeDriver.handleChoice("wait"));
          }}
          onStay={() => {
            // 留在应用：驱动清掉请求与授权并推进代际，在途旧投递/核对失效。
            applyEffect(closeDriver.handleChoice("stay"));
          }}
          onPendingReviewed={() => {
            // 待核对闭环：捕获**请求代际**与每个目标的**待核对代际**，
            // 只清自己成功核对过的那一代（ST-B03-08-4/5/6）。
            const coordinator = closeCoordinatorRef.current;
            const requestGen = coordinator.generation();
            const targets = coordinator.pendingWorktrees()
              .map((worktreeId) => ({ worktreeId, generation: coordinator.pendingGeneration(worktreeId) }));
            void (async () => {
              let allConfirmed = true;
              for (const { worktreeId, generation } of targets) {
                // 用户在核对过程中取消/发起新请求：旧核对不得授权新请求。
                if (coordinator.generation() !== requestGen) return;
                if (!worktreeId.startsWith("save-as:")) {
                  let verified = false;
                  try {
                    verified = await refreshTarget(worktreeId);
                  } catch {
                    verified = false;
                  }
                  if (!verified) {
                    // 核对失败：保留待核对，不关闭、不重跑。
                    allConfirmed = false;
                    continue;
                  }
                }
                if (!coordinator.acknowledgePending(worktreeId, generation)) {
                  // 该目标出现了新的待核对（代际已变）：不清账。
                  allConfirmed = false;
                }
              }
              if (coordinator.generation() !== requestGen) return;
              if (!allConfirmed) {
                // 失败或出现新 pending：按**当前真实集合**主动重新展示。
                reconcileClose();
                return;
              }
              applyEffect(closeDriver.handlePendingContinue());
            })();
          }}
        />
      ) : null}
    </div>
  );

  function selectionKey(entry: ChangeEntry): string {
    return `${entry.changes}:${entry.path}`;
  }

  function expandEntry(entry: ChangeEntry) {
    setExpanded((current) => new Set(current).add(selectionKey(entry)));
  }

  async function selectFromList(next: Selection) {
    const entry = snapshot?.entries.find((item) => item.path === next.path && item.changes === next.changes);
    if (entry && worktreeId) await openEntry(entry, { view, worktreeId });
  }
}

/** 测量与长测通过真实的视图下拉切换视图，走与用户相同的 onChange 路径。 */
function selectViewInUi(view: ViewMode): HTMLSelectElement | null {
  const select = document.querySelector<HTMLSelectElement>("select[data-view-select]");
  if (!select) return null;
  select.value = view;
  select.dispatchEvent(new Event("change", { bubbles: true }));
  return select;
}

/** 编辑区中央的一句提示（JetBrains 的空状态写法）：标题、可选说明、可选链接。 */
function Hint({ title, detail, action }: { title: string; detail?: string; action?: { label: string; run: () => void } }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-1.5 px-6 text-center text-fg-2">
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

function IconAction({ icon: Icon, label, disabled, pressed, onClick }: {
  icon: LucideIcon;
  label: string;
  disabled?: boolean;
  pressed?: boolean;
  onClick?: () => void;
}) {
  return (
    <button type="button" className="icon-action" title={label} aria-label={label} aria-pressed={pressed}
      disabled={disabled} onClick={onClick}>
      <Icon aria-hidden className="size-4" strokeWidth={1.6} />
    </button>
  );
}

/** 主工具栏：仓库与 worktree 组件（Q107），空白处可拖动窗口。 */
function MainToolbar({
  repo,
  worktrees,
  activeWorktreeId,
  onWorktree,
  onChooseRepo,
  busy,
  watchError,
}: {
  repo: RepoEntry | null;
  worktrees: WorktreeEntry[];
  activeWorktreeId: string | null;
  onWorktree: (worktreeId: string) => void;
  onChooseRepo: () => void;
  busy: boolean;
  watchError: string | null;
}) {
  const active = worktrees.find((worktree) => worktree.id === activeWorktreeId);
  const worktreeLabel = (worktree: WorktreeEntry) =>
    `${worktree.branch ?? worktree.path}${worktree.isMain ? ` ${messages.changes.worktreeMain}` : ""}`;
  const name = repo?.name ?? messages.app.name;
  return (
    <header data-tauri-drag-region className="col-span-3 flex items-center gap-0.5 pr-2.5" style={{ paddingLeft: MAIN_TOOLBAR_INSET }}>
      <button type="button" className="toolbar-widget" disabled={busy} onClick={onChooseRepo}
        title={repo ? `${repo.path}\n${messages.toolbar.chooseRepo}` : messages.toolbar.chooseRepo}>
        <span aria-hidden className="project-badge">{name.slice(0, 1).toUpperCase()}</span>
        <span>{name}</span>
        <ChevronDown aria-hidden className="size-3 text-fg-2" strokeWidth={2} />
      </button>
      {active ? (
        worktrees.length > 1 ? (
          <label className="toolbar-widget relative has-[select:focus-visible]:shadow-[inset_0_0_0_2px_var(--ring)]">
            <GitBranch aria-hidden className="size-4 text-icon" strokeWidth={1.6} />
            <span>{worktreeLabel(active)}</span>
            <ChevronDown aria-hidden className="size-3 text-fg-2" strokeWidth={2} />
            <select
              aria-label={messages.changes.worktreeLabel}
              className="absolute inset-0 cursor-default opacity-0"
              value={activeWorktreeId ?? ""}
              onChange={(event) => onWorktree(event.target.value)}
            >
              {worktrees.map((worktree) => (
                <option key={worktree.id} value={worktree.id}>{worktreeLabel(worktree)}</option>
              ))}
            </select>
          </label>
        ) : (
          <span className="toolbar-widget">
            <GitBranch aria-hidden className="size-4 text-icon" strokeWidth={1.6} />
            <span>{worktreeLabel(active)}</span>
          </span>
        )
      ) : null}
      <span className="flex-1 self-stretch" data-tauri-drag-region />
      {watchError ? (
        <span className="warn-chip" title={watchError}>
          <RefreshCwOff aria-hidden className="size-3.5" strokeWidth={1.8} />
          {messages.changes.watchUnavailable}
        </span>
      ) : null}
    </header>
  );
}

/** 左侧工具窗口图标条：只有“变更”和“回收区”（不借此新增功能，Q100）。 */
function ToolStripe({ recoveryOpen, onRecovery }: { recoveryOpen: boolean; onRecovery: () => void }) {
  return (
    <nav aria-label={messages.toolbar.toolWindows} className="flex flex-col items-center gap-1 pt-0.5">
      <button type="button" className="icon-action size-[30px]" aria-pressed="true" title={messages.changes.toolWindow}
        aria-label={messages.changes.toolWindow}
        onClick={() => document.querySelector<HTMLElement>(".change-list")?.focus()}>
        <GitCommitHorizontal aria-hidden className="size-[18px]" strokeWidth={1.6} />
      </button>
      <button type="button" className="icon-action size-[30px]" aria-pressed={recoveryOpen} title={messages.recovery.title}
        aria-label={messages.recovery.title} onClick={onRecovery}>
        <Inbox aria-hidden className="size-[18px]" strokeWidth={1.6} />
      </button>
    </nav>
  );
}

/** “变更”工具窗口的图标工具条：刷新、暂存、取消暂存、丢弃，右侧是审阅进度。 */
function ChangesToolbar({
  selectedEntry,
  isWriting,
  busy,
  review,
  onRefresh,
  onStage,
  onUnstage,
  onDiscard,
}: {
  selectedEntry: ChangeEntry | null;
  isWriting: boolean;
  busy: boolean;
  review: ChangesSnapshot["review"] | null;
  onRefresh: () => void;
  onStage: (entry: ChangeEntry) => void;
  onUnstage: (entry: ChangeEntry) => void;
  onDiscard: (entry: ChangeEntry) => void;
}) {
  const writable = selectedEntry && !selectedEntry.readonlyReason && !isWriting ? selectedEntry : null;
  return (
    <div className="flex h-[30px] shrink-0 items-center gap-0.5 pr-2.5 pl-1.5">
      <IconAction icon={RotateCw} label={messages.changes.refresh} disabled={busy} onClick={onRefresh} />
      <span aria-hidden className="mx-1 h-4 w-px bg-border" />
      <IconAction icon={Plus} label={messages.changes.shortcuts.stageFile}
        disabled={!writable || writable.changes === "staged"} onClick={() => writable && onStage(writable)} />
      <IconAction icon={Minus} label={messages.changes.shortcuts.unstageFile}
        disabled={!writable || writable.changes !== "staged"} onClick={() => writable && onUnstage(writable)} />
      <IconAction icon={Undo2} label={messages.changes.shortcuts.discardFile}
        disabled={!writable || writable.changes !== "unstaged"} onClick={() => writable && onDiscard(writable)} />
      {review ? (
        <span className="ml-auto text-xs whitespace-nowrap text-fg-2 tabular-nums">
          {messages.review.progress(review.reviewedCount, review.total)}
        </span>
      ) : null}
    </div>
  );
}

/** 编辑区顶部：单个文件标签（不做多标签，Q16）和 diff 工具条。 */
function EditorHeader({
  entry,
  view,
  onView,
  canNavigate,
  onJump,
  isWriting,
  onStage,
  onUnstage,
  onDiscard,
}: {
  entry: ChangeEntry;
  view: ViewMode;
  onView: (view: ViewMode) => void;
  canNavigate: boolean;
  onJump: (direction: 1 | -1) => void;
  isWriting: boolean;
  onStage: (entry: ChangeEntry) => void;
  onUnstage: (entry: ChangeEntry) => void;
  onDiscard: (entry: ChangeEntry) => void;
}) {
  const slash = entry.path.lastIndexOf("/");
  const name = entry.path.slice(slash + 1);
  const dir = slash > 0 ? entry.path.slice(0, slash) : messages.changes.repoRoot;
  const group = entry.changes === "staged"
    ? messages.changes.staged
    : entry.changes === "untracked" ? messages.changes.untracked : messages.changes.unstaged;
  const ViewIcon = view === "unified" ? Rows3 : Columns2;
  return (
    <>
      <div className="flex h-9 shrink-0 px-1.5 shadow-[inset_0_-1px_0_var(--border)]">
        <div className="editor-tab" title={entry.path}>
          <FileTypeIcon path={entry.path} symlink={entry.readonlyReason === "symlink"} />
          <span className="truncate">{name}</span>
          <span className="truncate text-xs text-fg-2">{`${dir} · ${group}`}</span>
        </div>
      </div>
      <div className="flex h-8 shrink-0 items-center gap-0.5 px-2 shadow-[inset_0_-1px_0_var(--border)]">
        <IconAction icon={ArrowUp} label={messages.diff.previousHunkShortcut} disabled={!canNavigate} onClick={() => onJump(-1)} />
        <IconAction icon={ArrowDown} label={messages.diff.nextHunkShortcut} disabled={!canNavigate} onClick={() => onJump(1)} />
        <span aria-hidden className="mx-1.5 h-4 w-px bg-border" />
        <span className="relative inline-flex items-center">
          <ViewIcon aria-hidden className="pointer-events-none absolute left-2 size-4 text-icon" strokeWidth={1.6} />
          <select
            data-view-select
            aria-label={messages.diff.viewLabel}
            className="widget-select pl-7"
            value={view}
            onChange={(event) => onView(event.target.value as ViewMode)}
          >
            <option value="split">{messages.diff.viewSplit}</option>
            <option value="unified">{messages.diff.viewUnified}</option>
          </select>
        </span>
        <span className="flex-1" />
        {entry.readonlyReason ? (
          <span className="inline-flex h-[22px] items-center gap-1 rounded px-2 text-xs text-fg-2 ring-1 ring-edge ring-inset"
            title={messages.changes.readonlyReasons[entry.readonlyReason]}>
            <Lock aria-hidden className="size-3.5" strokeWidth={1.6} />
            {messages.changes.readonly}
          </span>
        ) : entry.changes === "staged" ? (
          <Button variant="outline" size="sm" disabled={isWriting} onClick={() => onUnstage(entry)}
            title={messages.changes.shortcuts.unstageFile}>
            {isWriting ? <Loader2 aria-hidden className="animate-spin" /> : <Minus aria-hidden />}
            {isWriting ? messages.changes.writing : messages.changes.unstageFile}
          </Button>
        ) : (
          <Button variant="outline" size="sm" disabled={isWriting} onClick={() => onStage(entry)}
            title={messages.changes.shortcuts.stageFile}>
            {isWriting ? <Loader2 aria-hidden className="animate-spin" /> : <Plus aria-hidden />}
            {isWriting ? messages.changes.writing : messages.changes.stageFile}
          </Button>
        )}
        {entry.changes === "unstaged" && !entry.readonlyReason ? (
          <Button variant="outline" size="sm" className="ml-1.5" disabled={isWriting} onClick={() => onDiscard(entry)}
            title={messages.changes.shortcuts.discardFile}>
            <Undo2 aria-hidden />
            {messages.changes.discardShort}
          </Button>
        ) : null}
      </div>
    </>
  );
}

/** 编辑区顶部提示条：信息、警告、错误三种（JetBrains 的编辑器通知）。 */
function EditorBanner({ tone, icon: Icon, action, children }: {
  tone: "info" | "warn" | "error";
  icon: LucideIcon;
  action?: { label: string; run: () => void };
  children: React.ReactNode;
}) {
  return (
    <div role={tone === "error" ? "alert" : "status"} className="editor-banner shrink-0" data-tone={tone}>
      <Icon aria-hidden className="size-4" strokeWidth={1.8} />
      <div className="min-w-0 flex-1">{children}</div>
      {action ? (
        <button type="button" className="banner-link" onClick={action.run}>
          {action.label}
        </button>
      ) : null}
    </div>
  );
}

/** 底部状态栏：仓库 › 分支 › 当前文件路径；自动刷新不可用时常驻提示（Q95）。 */
function StatusBar({ repoName, branch, entry, watchError }: {
  repoName: string;
  branch: string | null;
  entry: ChangeEntry | null;
  watchError: string | null;
}) {
  const crumbs = [repoName, branch ?? messages.changes.detachedHead, ...(entry ? entry.path.split("/") : [])];
  return (
    <footer className="col-span-3 flex items-center gap-1.5 overflow-hidden pr-3 pl-3.5 text-xs whitespace-nowrap text-fg-2">
      {crumbs.map((crumb, index) => (
        <Fragment key={index}>
          {index ? <span aria-hidden className="text-fg-3">›</span> : null}
          <span className="truncate">{crumb}</span>
        </Fragment>
      ))}
      <span className="flex-1" />
      {watchError ? (
        <span className="status-warn inline-flex items-center gap-1.5 text-foreground" title={watchError}>
          <RefreshCwOff aria-hidden className="size-3.5" strokeWidth={1.8} />
          {messages.changes.watchUnavailable}
        </span>
      ) : null}
    </footer>
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
    <EditorBanner tone={warn ? "warn" : "error"} icon={warn ? TriangleAlert : CircleX}
      action={{ label: messages.errors.dismiss, run: onDismiss }}>
      <strong className="mr-1.5 font-semibold">{title}</strong>
      <span className={error.kind === "git" ? "font-mono text-xs whitespace-pre-wrap" : "whitespace-pre-wrap"}>{error.message}</span>
    </EditorBanner>
  );
}

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
  return (
    <section className="border-t">
      <header className="flex items-center gap-3 px-3 py-1.5 text-xs">
        <Button variant="ghost" size="sm" onClick={onToggle}>
          {open ? "▾" : "▸"} {messages.measure.title}（{measurements.length}）
        </Button>
        <Button
          variant="ghost"
          size="sm"
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
        </Button>
        <Button variant="ghost" size="sm" disabled={measurements.length === 0} onClick={onClear}>
          {messages.measure.clear}
        </Button>
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

// ---------------- 工具 ----------------

/** 界面展示顺序：未暂存 -> 未跟踪 -> 已暂存（审查意见 5）。 */
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
