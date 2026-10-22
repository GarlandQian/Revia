/**
 * 应用外壳：首次打开 → 添加仓库；之后是「文件列表 + 只读 diff」的只读界面（M1）。
 *
 * 只读：M1 不提供暂存、丢弃、提交等写操作。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";
import type { RepoEntry } from "@/types/bindings/RepoEntry";
import type { ViewMode } from "@/types/bindings/ViewMode";
import type { WorktreeSessionInfo } from "@/types/bindings/WorktreeSessionInfo";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { ChangesList, type Selection } from "@/components/app/ChangesList";
import { logToBackend, setLoggingEnabled } from "@/features/diff/log";
import { ErrorBoundary } from "@/components/app/ErrorBoundary";
import { DiffView, ROW_HEIGHT, type DiffViewHandle } from "@/features/diff/DiffView";
import { HighlightStore } from "@/features/diff/highlightStore";
import { RowWindowStore } from "@/features/diff/rowWindowStore";
import { monitorSoakVisibility, sleep, waitForSoak, type SoakVisibility } from "@/lib/soak";
import { ipc, type SoakFocus } from "@/lib/ipc";
import { startIpcDiagnostics } from "@/lib/ipcDiagnostics";
import { diagnoseJumpDom } from "@/lib/jumpDiagnostics";
import { messages } from "@/lib/i18n/zh-CN";
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

type IpcDiagnosticSample = ReturnType<ReturnType<typeof startIpcDiagnostics>["snapshot"]>;

interface AppErrorInfo {
  kind: "rejected" | "git" | "environment" | "internal";
  message: string;
}

const DEFAULT_VIEWPORT_ROWS = 45;

function debugLog(message: string) {
  logToBackend(message);
}

export function App() {
  const [repos, setRepos] = useState<RepoEntry[]>([]);
  const [activeRepo, setActiveRepo] = useState<RepoEntry | null>(null);
  const [worktreeId, setWorktreeId] = useState<string | null>(null);
  const [info, setInfo] = useState<WorktreeSessionInfo | null>(null);
  const [snapshot, setSnapshot] = useState<ChangesSnapshot | null>(null);
  const [selected, setSelected] = useState<Selection | null>(null);
  const [view, setView] = useState<ViewMode>("split");
  const [fontSize, setFontSize] = useState(12);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState<AppErrorInfo | null>(null);
  const [measurements, setMeasurements] = useState<Measurement[]>([]);
  const [panelOpen, setPanelOpen] = useState(true);
  const [bootstrapDone, setBootstrapDone] = useState(false);

  const diffRef = useRef<DiffViewHandle | null>(null);
  const rowStore = useMemo(
    () =>
      new RowWindowStore(async (request) => {
        const slice = await ipc.getRows(request);
        return { rows: slice.rows, rowCount: slice.rowCount, entry: slice.entry };
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

  const openEntry = useCallback(
    async (entry: ChangeEntry, target: { view: ViewMode; worktreeId: string; expand?: boolean }) => {
      const selection: Selection = { path: entry.path, changes: entry.changes };
      if (target.expand && entry.collapsed) {
        // 测量等同于用户点开：默认收起的文件也要真正渲染，才能测到首屏。
        setExpanded((current) => new Set(current).add(`${entry.changes}:${entry.path}`));
      }
      const rowCount = target.view === "split" ? entry.rowCountSplit : entry.rowCountUnified;
      rowStore.setTarget(
        { worktreeId: target.worktreeId, path: entry.path, changes: entry.changes, view: target.view },
        rowCount,
        entry,
      );
      highlightStore.setTarget({
        worktreeId: target.worktreeId,
        path: entry.path,
        changes: entry.changes,
      });
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
    [highlightStore, rowStore],
  );

  const openWorktree = useCallback(
    async (id: string, options: { targetPath?: string | null; view?: ViewMode; expand?: boolean } = {}) => {
      const targetView = options.view ?? view;
      setBusy(true);
      setError(null);
      const openStart = performance.now();
      try {
        const sessionInfo = await ipc.openWorktree(id);
        debugLog(`open_worktree 完成：${Math.round(performance.now() - openStart)} ms`);
        setInfo(sessionInfo);
        setWorktreeId(id);
        const failed = sessionInfo.checks.filter((check) => !check.ok);
        if (failed.length > 0) {
          setError({ kind: "environment", message: failed.map((check) => `${check.name}：${check.detail}`).join("\n") });
        }

        const startedAt = performance.now();
        const next = applySnapshot(await ipc.listChanges(id));
        const listChangesMs = performance.now() - startedAt;
        debugLog(`list_changes 完成：${Math.round(listChangesMs)} ms，${next.entries.length} 个文件`);
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
        setError(toAppError(caught));
        return null;
      } finally {
        setBusy(false);
      }
    },
    [applySnapshot, openEntry, pickFirstEntry],
  );

  const refresh = useCallback(async () => {
    if (!worktreeId) return;
    setBusy(true);
    try {
      const next = applySnapshot(await ipc.listChanges(worktreeId));
      setSelected((current) => {
        if (current && next.entries.some((entry) => entry.path === current.path && entry.changes === current.changes)) {
          return current;
        }
        const entry = pickFirstEntry(next);
        return entry ? { path: entry.path, changes: entry.changes } : null;
      });
      setStatus(`${next.entries.length} 个文件`);
    } catch (caught) {
      setError(toAppError(caught));
    } finally {
      setBusy(false);
    }
  }, [applySnapshot, pickFirstEntry, worktreeId]);

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
          setError({ kind: "rejected", message: messages.firstRun.notARepo });
          return;
        }
        await ipc.selectRepo(repo.id, worktree.id);
        await openWorktree(worktree.id);
      } catch (caught) {
        setError(toAppError(caught));
      } finally {
        setBusy(false);
      }
    },
    [openWorktree],
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
        if (!config.logging) {
          setPanelOpen(false);
        }
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
        setError(toAppError(caught));
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
      const rowCount = next === "split" ? selectedEntry.rowCountSplit : selectedEntry.rowCountUnified;
      rowStore.setTarget(
        {
          worktreeId: worktreeId ?? "",
          path: selectedEntry.path,
          changes: selectedEntry.changes,
          view: next,
        },
        rowCount,
        selectedEntry,
      );
    },
    [rowStore, selectedEntry, view, worktreeId],
  );

  // 视图切换后把锚点行放回顶部（等数据到位再定位）
  useEffect(() => {
    const anchor = pendingAnchor.current;
    if (!anchor || anchor.view !== view || !selectedEntry) return;
    let cancelled = false;
    void (async () => {
      const index = await ipc.rowIndexForLine({
        worktreeId: worktreeId ?? "",
        path: selectedEntry.path,
        changes: selectedEntry.changes,
        view,
        line: anchor.line,
        version: anchor.version,
      });
      if (cancelled) return;
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


  // 快捷键（Q19）：只做 M1 范围内的只读操作
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const meta = event.metaKey;
      if (meta && event.key.toLowerCase() === "o") {
        event.preventDefault();
        void chooseRepo();
      } else if (meta && event.key.toLowerCase() === "r") {
        event.preventDefault();
        void refresh();
      } else if (meta && event.key === "+") {
        event.preventDefault();
        void changeFontSize(1);
      } else if (meta && event.key === "-") {
        event.preventDefault();
        void changeFontSize(-1);
      } else if (event.key === "F7") {
        event.preventDefault();
        jumpToHunk(event.shiftKey ? -1 : 1);
      } else if (event.key === "ArrowDown" && meta) {
        event.preventDefault();
        focusNextFile(1);
      } else if (event.key === "ArrowUp" && meta) {
        event.preventDefault();
        focusNextFile(-1);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [changeFontSize, chooseRepo, focusNextFile, jumpToHunk, refresh]);

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
        jumpDomError = String(caught);
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
            const buttons = Array.from(document.querySelectorAll<HTMLElement>('[data-slot="toggle-group-item"]'));
            const button = buttons.find((candidate) => candidate.textContent?.trim() === label);
            button?.click();
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
        report.error = String(caught);
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
          const buttons = Array.from(
            document.querySelectorAll<HTMLElement>('[data-slot="toggle-group-item"]'),
          );
          const button = buttons.find((candidate) => candidate.textContent?.trim() === label);
          if (!button) throw new Error(`找不到视图按钮：${label}`);
          button.click();
          if (!await waitForSoak(() => button.getAttribute("aria-pressed") === "true"
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
          if (!fixed || !large) throw new Error("单项诊断缺少固定文件");
          const focusEntry = focus === "idle" || focus === "ipc" || focus === "channel" ? fixed : large;
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
              } else if (focus === "channel") {
                // 只反复让出任务（与打开文件时 waitUntil 相同的 nextTask），不取数、不改界面。
                for (let index = 0; index < 20; index += 1) await nextTask();
                yieldedTasks += 20;
                await yieldStep(15);
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
                if (focusEntry !== fixed) await openSplit(focusEntry);
              }
            } catch (error) {
              // 遮挡期间计时器被节流，等待会超时；只有确实发生过遮挡才恢复后重建状态继续。
              if (!hiddenSeen && !document.hidden) throw error;
              await resumeWhenVisible(`遮挡导致操作失败：${String(error)}`);
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
        report.error = String(caught);
        report.samples = samples;
        debugLog(`长期运行基线异常中断：${String(caught)}`);
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
      try {
        setStatus("性能测量等待可见窗口，请将此窗口置于前台");
        const deadline = performance.now() + 120_000;
        let ready = false;
        while (performance.now() < deadline) {
          const sample = await monitor.sample();
          if (!sample.hidden && sample.rafPerSecond > 5) { ready = true; break; }
        }
        if (!ready) throw new Error("性能测量未获得可见合成窗口");
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
        const results: Measurement[] = [];
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
        const visibility = await monitor.sample();
        if (visibility.hidden || visibility.rafPerSecond <= 5 || visibility.hiddenTransitions > 0 || visibility.maxFrameGapMs > 2000) {
          throw new Error(`性能测量可见性无效：${JSON.stringify(visibility)}`);
        }
        if (results.length !== views.length) throw new Error("性能测量结果缺失");
        await ipc.writeMeasurement({ ok: true, combos: results, visibility });
        debugLog(`测量完成，共 ${results.length} 组`);
      } catch (caught) {
        debugLog(`自动测量失败：${String(caught)}`);
        await ipc.writeMeasurement({ ok: false, error: String(caught) });
      } finally {
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
    return <div className="flex h-full items-center justify-center text-sm text-muted-foreground">载入中…</div>;
  }

  if (repos.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4 p-8">
        <h1 className="text-lg font-medium">{messages.firstRun.title}</h1>
        <p className="max-w-md text-center text-sm text-muted-foreground">{messages.firstRun.description}</p>
        <Button onClick={() => void chooseRepo()} disabled={busy}>
          {messages.firstRun.addButton}
        </Button>
        <p className="text-xs text-muted-foreground">{messages.firstRun.dragHint}</p>
        {error ? <ErrorBanner error={error} onDismiss={() => setError(null)} /> : null}
      </div>
    );
  }

  const collapsed = diffArea === "collapsed";


  return (
    <div className="grid h-full grid-cols-[300px_1fr] grid-rows-1 overflow-hidden">
      <aside className="flex min-h-0 flex-col border-r bg-sidebar/60">
        <div className="flex items-center gap-2 px-3 py-2">
          <span className="truncate text-sm font-medium" title={activeRepo?.path}>
            {activeRepo?.name ?? messages.app.name}
          </span>
          <Button variant="ghost" size="sm" className="ml-auto" onClick={() => void chooseRepo()}>
            {messages.firstRun.addButton}
          </Button>
        </div>
        <Separator />
        {snapshot ? (
          <ChangesList snapshot={snapshot} selected={selected} onSelect={(next) => void selectFromList(next)} />
        ) : (
          <div className="p-3 text-xs text-muted-foreground">{messages.changes.loading}</div>
        )}
      </aside>

      <main className="flex min-h-0 min-w-0 flex-col overflow-hidden">
        <Toolbar
          view={view}
          onView={(next) => void switchView(next)}
          onRefresh={() => void refresh()}
          onFontSize={(delta) => void changeFontSize(delta)}
          busy={busy}
          status={status}
          environment={info}
        />
        {error ? <ErrorBanner error={error} onDismiss={() => setError(null)} /> : null}
        <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
          {!selectedEntry ? (
            <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
              {messages.changes.noSelection}
            </div>
          ) : selectedEntry.unmerged ? (
            <Hint text={messages.changes.conflict} />
          ) : collapsed ? (
            <Hint text={messages.changes.collapsed} action={{ label: messages.changes.expand, run: () => expandEntry(selectedEntry) }} />
          ) : selectedEntry.binary ? (
            <Hint text={messages.changes.binary} />
          ) : selectedEntry.hunkCount === 0 ? (
            <Hint text={messages.changes.emptyFile} />
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
              />
            </ErrorBoundary>
          )}
        </div>
        <MeasurePanel
          open={panelOpen}
          onToggle={() => setPanelOpen((value) => !value)}
          measurements={measurements}
          onClear={() => setMeasurements([])}
        />
      </main>
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

function Hint({ text, action }: { text: string; action?: { label: string; run: () => void } }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 text-sm text-muted-foreground">
      <span>{text}</span>
      {action ? (
        <Button variant="secondary" size="sm" onClick={action.run}>
          {action.label}
        </Button>
      ) : null}
    </div>
  );
}

function Toolbar({
  view,
  onView,
  onRefresh,
  onFontSize,
  busy,
  status,
  environment,
}: {
  view: ViewMode;
  onView: (view: ViewMode) => void;
  onRefresh: () => void;
  onFontSize: (delta: number) => void;
  busy: boolean;
  status: string;
  environment: WorktreeSessionInfo | null;
}) {
  return (
    <div className="flex items-center gap-3 border-b px-3 py-1.5">
      <ToggleGroup
        value={[view]}
        onValueChange={(values) => {
          const next = values[0];
          if (next === "split" || next === "unified") onView(next);
        }}
        size="sm"
        variant="outline"
      >
        <ToggleGroupItem value="split">{messages.diff.split}</ToggleGroupItem>
        <ToggleGroupItem value="unified">{messages.diff.unified}</ToggleGroupItem>
      </ToggleGroup>
      <Separator orientation="vertical" className="h-5" />
      <Button variant="ghost" size="sm" onClick={() => onFontSize(-1)}>
        A−
      </Button>
      <Button variant="ghost" size="sm" onClick={() => onFontSize(1)}>
        A+
      </Button>
      <Separator orientation="vertical" className="h-5" />
      <Button variant="ghost" size="sm" onClick={onRefresh} disabled={busy}>
        {messages.changes.refresh}
      </Button>
      <span className="ml-auto truncate text-xs text-muted-foreground">
        {environment ? `${environment.gitVersion} · ${status}` : status}
      </span>
    </div>
  );
}

function ErrorBanner({ error, onDismiss }: { error: AppErrorInfo; onDismiss: () => void }) {
  const title =
    error.kind === "environment"
      ? messages.environment.title
      : error.kind === "git"
        ? messages.errors.gitFailed
        : error.kind === "rejected"
          ? messages.errors.rejected
          : messages.errors.internal;
  return (
    <div className="flex items-start gap-3 border-b bg-destructive/10 px-3 py-2 text-xs text-destructive">
      <strong className="shrink-0">{title}</strong>
      <pre className="min-w-0 flex-1 whitespace-pre-wrap font-mono">{error.message}</pre>
      <Button variant="ghost" size="sm" onClick={onDismiss}>
        {messages.errors.dismiss}
      </Button>
    </div>
  );
}

function MeasurePanel({
  open,
  onToggle,
  measurements,
  onClear,
}: {
  open: boolean;
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

function toAppError(caught: unknown): AppErrorInfo {
  const message = caught instanceof Error ? caught.message : String(caught);
  const kind: AppErrorInfo["kind"] = /git|退出码/i.test(message)
    ? "git"
    : /找不到|环境|shell/i.test(message)
      ? "environment"
      : /拒绝|不在当前|请先/.test(message)
        ? "rejected"
        : "internal";
  return { kind, message };
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
