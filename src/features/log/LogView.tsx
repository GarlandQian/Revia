/**
 * 提交日志（M5 F01、F06，规格 L01–L06）。
 *
 * 主区域固定的“日志”页签内容：工具栏筛选、分支图表格、所选提交的改动文件与详情、
 * 可选的 diff 预览。数据全部来自任务 01 的只读历史命令；这里不签发任何写入凭据，
 * 也不拼 Git 参数。
 *
 * 状态（筛选、选中提交、滚动位置）按 worktree 保留，由 `useLogWorkspace` 持有；
 * 本组件只读它并回报变化，因此切换 worktree 再回来时位置不丢。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { HistoryCommit } from "@/types/bindings/HistoryCommit";
import type { HistoryCommitDetail } from "@/types/bindings/HistoryCommitDetail";
import type { HistoryCommitFiles } from "@/types/bindings/HistoryCommitFiles";
import type { HistoryFile } from "@/types/bindings/HistoryFile";
import type { HistoryFileTarget } from "@/types/bindings/HistoryFileTarget";
import type { HistoryMergeFiles } from "@/types/bindings/HistoryMergeFiles";
import type { HistoryRef } from "@/types/bindings/HistoryRef";
import type { HistoryRefLabel } from "@/types/bindings/HistoryRefLabel";
import type { LogColumns } from "@/lib/ipc";
import { ipc } from "@/lib/ipc";
import { messages } from "@/lib/i18n/zh-CN";
import { getErrorMessage } from "@/lib/error";
import { Codicon } from "@/components/ui/codicon";
import { Dropdown, MenuItem, MenuSeparator } from "@/components/ui/popover";
import { FileTypeIcon } from "@/components/app/FileTypeIcon";
import type { LogFilterState, DatePreset } from "./logModel";
import {
  activeFilterCount,
  authorKey,
  authorLabel,
  branchFilterOptions,
  commitTabTitle,
  defaultFilter,
  filterKey,
  formatCommitDate,
  hasActiveFilter,
  logVisibleEnd,
  refChips,
  selectedLogCommit,
  shortOid,
} from "./logModel";
import { LogPager, type LogSnapshot } from "./logPager";
import { RefsWatch } from "./refsWatch";
import { appendGraphRows, collapseLongEdges, emptyGraphState, laneColor, type GraphRow, type GraphState } from "./logGraph";
import type { LogWorkspaceState } from "./useLogWorkspace";
import { buildFileTree, effectiveMergeMode, firstHistoryFile, type FileTreeNode } from "./logFiles";
import { CommitDiffView, type CommitDiffViewProps } from "./CommitDiffView";
import { CommitMergeView } from "./CommitMergeView";

/** 固定行高（虚拟滚动与分支图共用）。 */
export const LOG_ROW_HEIGHT = 26;
/** 分支图每条轨道占的宽度（像素）。 */
export const LOG_LANE_WIDTH = 14;
/** 表格在滚动时多渲染的行数，避免快速滚动出现空白。 */
const ROW_OVERSCAN = 8;

export interface LogSettings {
  columns: LogColumns;
  firstParent: boolean;
  longEdges: boolean;
  previewPosition: "right" | "bottom";
}

export interface LogViewProps {
  worktreeId: string;
  /** 只读历史目标；为空表示尚未打开（显示读取中）。 */
  targetId: string | null;
  /** 打开历史目标时的引用快照（分支筛选候选项）。 */
  initialRefs?: HistoryRef[];
  active: boolean;
  state: LogWorkspaceState;
  onState: (patch: Partial<LogWorkspaceState>) => void;
  settings: LogSettings;
  onSettings: (patch: Partial<LogSettings>) => void;
  previewOpen: boolean;
  onPreviewOpen: (open: boolean) => void;
  diffPresentation: Pick<CommitDiffViewProps, "view" | "options" | "fontSize" | "fontFamily">;
  groupByDirectory: boolean;
  /** 当前分支名（引用标签突出显示用）。 */
  currentBranch: string | null;
  /** 当前 HEAD 的完整提交 ID（显示 HEAD 标签）。 */
  headCommit: string | null;
  /** 当前仓库生效的提交身份（作者筛选的“我”）。 */
  me: { name: string | null; email: string | null } | null;
  /** 内部写操作（提交、检出、fetch、推送）递增：直接触发重新查询。 */
  refreshKey: number;
  /**
   * 监视事件、手动刷新或回到前台后的可信重读代号。
   * 引用变化才重查日志，不设置后台 Git 轮询。
   */
  refsWatchRevision: number;
  onOpenCommitFile: (request: { target: HistoryFileTarget; title: string }) => void;
  onOpenMergeCommitFile: (request: { commit: string; path: string; title: string }) => void;
  onCreateBranchAt: (commit: string) => void;
  onCopy: (text: string) => void;
  onNotify: (notice: { tone: "success" | "warning" | "error"; title: string; detail?: string }) => void;
}

interface DetailsState {
  targetId: string | null;
  commit: HistoryCommit | null;
  detail: HistoryCommitDetail | null;
  files: HistoryCommitFiles | null;
  merge: HistoryMergeFiles | null;
  error: string | null;
  loading: boolean;
}

const EMPTY_DETAILS: DetailsState = { targetId: null, commit: null, detail: null, files: null, merge: null, error: null, loading: false };

export function LogView(props: LogViewProps) {
  const {
    worktreeId, targetId, initialRefs, active, state, onState, settings, onSettings, previewOpen, onPreviewOpen, diffPresentation,
    groupByDirectory, currentBranch, headCommit, me, refreshKey, refsWatchRevision, onOpenCommitFile, onOpenMergeCommitFile,
    onCreateBranchAt, onCopy, onNotify,
  } = props;

  const [snapshot, setSnapshot] = useState<LogSnapshot>({
    commits: [], exhausted: false, spanExceeded: false, loading: false, error: null, queryKey: "",
  });
  const [refs, setRefs] = useState<HistoryRef[]>(initialRefs ?? []);
  const [repoFiles, setRepoFiles] = useState<string[]>([]);
  const [details, setDetails] = useState<DetailsState>(EMPTY_DETAILS);
  const [previewFile, setPreviewFile] = useState<HistoryFileTarget | null>(null);
  const [previewTitle, setPreviewTitle] = useState("");
  const [previewTargetId, setPreviewTargetId] = useState<string | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(400);
  const [filterFocus, setFilterFocus] = useState(false);

  const filter = state.filter ?? defaultFilter();
  const firstParent = settings.firstParent;
  const key = useMemo(() => filterKey(filter), [filter]);
  const pagerRef = useRef<LogPager | null>(null);
  const graphRef = useRef<GraphState>(emptyGraphState());
  const graphRowsRef = useRef<GraphRow[]>([]);
  const [graphVersion, setGraphVersion] = useState(0);
  const tableRef = useRef<HTMLDivElement | null>(null);
  const refsWatchRef = useRef(new RefsWatch());
  const handledWatchRef = useRef(refsWatchRevision);
  const filterInputRef = useRef<HTMLInputElement | null>(null);
  const scrollTopRef = useRef(0);
  scrollTopRef.current = scrollTop;
  const restoreScrollRef = useRef(state.scrollTop);
  restoreScrollRef.current = state.scrollTop;
  const pendingScrollRef = useRef<number | null>(state.scrollTop);

  // ---------- 分页 ----------
  if (pagerRef.current === null) {
    pagerRef.current = new LogPager({
      loader: (request) => ipc.historyLogPage(request),
      targetId: null,
    });
  }
  const pager = pagerRef.current;
  const resetLog = useCallback((queryKey: string) => {
    pendingScrollRef.current = restoreScrollRef.current;
    pager.reset(queryKey, { force: true });
  }, [pager]);

  const resetGraph = useCallback((commits: HistoryCommit[]) => {
    const result = appendGraphRows(emptyGraphState(), commits, { firstParent });
    graphRef.current = result.state;
    graphRowsRef.current = result.rows;
    setGraphVersion((value) => value + 1);
  }, [firstParent]);

  const graphQueryKeyRef = useRef("");
  useEffect(() => {
    pager.setListener((next) => {
      setSnapshot(next);
      // 换查询（筛选 / 遍历方式 / 目标）时整张图重算；同一查询的续页只追加。
      if (next.queryKey !== graphQueryKeyRef.current) {
        graphQueryKeyRef.current = next.queryKey;
        graphRowsRef.current = [];
        resetGraph(next.commits);
        return;
      }
      if (next.commits.length === 0) {
        resetGraph([]);
        return;
      }
      const known = graphRowsRef.current.length;
      if (next.commits.length < known) {
        resetGraph(next.commits);
      } else if (next.commits.length > known) {
        const tail = next.commits.slice(known);
        const result = appendGraphRows(graphRef.current, tail, { firstParent });
        graphRef.current = result.state;
        graphRowsRef.current = [...graphRowsRef.current, ...result.rows];
        setGraphVersion((value) => value + 1);
      }
    });
    return () => pager.setListener(null);
  }, [firstParent, pager, resetGraph]);

  // 目标变化：清空并重新查询。
  // 只在目标变化时重置：不能把 `refsWatchRevision` 放进来，否则这个 effect 会先于
  // 监视事件 effect 把代号标记成“已处理”，引用就永远不会被重新读取。
  useEffect(() => {
    refsWatchRef.current.reset();
    handledWatchRef.current = refsWatchRevision;
    pager.setTarget(targetId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pager, targetId]);

  // 筛选 / 遍历方式 / 外部引用变化：重新查询（保留选中与滚动，若提交仍在结果中）。
  useEffect(() => {
    if (!targetId) return;
    pager.setQuery(
      {
        text: filter.text.trim().length > 0 ? filter.text.trim() : null,
        caseSensitive: filter.caseSensitive,
        regex: filter.regex,
        branches: [...filter.branches],
        authors: [...filter.authors],
        paths: filter.paths.map((path) => path.trim()).filter((path) => path.length > 0),
        date: filter.date === "all"
          ? null
          : filter.date === "range"
            ? (filter.since && filter.until ? { kind: "range", since: filter.since, until: filter.until } : null)
            : { kind: "lastDays", days: Number(filter.date.replace("last", "")) },
      },
      firstParent,
    );
    resetLog(`${key}|${firstParent ? "1p" : "all"}|${refreshKey}|head:${headCommit ?? ""}`);
  }, [firstParent, filter, headCommit, key, pager, refreshKey, resetLog, targetId]);

  // 引用快照与文件树（路径筛选候选）：随目标与外部引用变化刷新。
  useEffect(() => {
    if (!targetId) {
      setRefs([]);
      return;
    }
    if (initialRefs && initialRefs.length > 0) {
      setRefs(initialRefs);
      refsWatchRef.current.observe(initialRefs);
    }
    let cancelled = false;
    void ipc
      .historyRefs(targetId)
      .then((next) => {
        if (cancelled) return;
        refsWatchRef.current.observe(next);
        setRefs(next);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [initialRefs, targetId, refreshKey]);

  useEffect(() => {
    if (!worktreeId) {
      setRepoFiles([]);
      return;
    }
    let cancelled = false;
    void ipc
      .repoFiles(worktreeId)
      .then((next) => {
        if (!cancelled) setRepoFiles(next);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [worktreeId]);

  // 外部 Git 操作：可信重读后核对引用；监视失败时手动/前台刷新同样可用。
  // 引用没变则不重查日志；游离 HEAD 的移动由 headCommit 变化触发重查。
  const queryRef = useRef({ key, firstParent, resetLog });
  queryRef.current = { key, firstParent, resetLog };
  useEffect(() => {
    if (!targetId) return;
    if (refsWatchRevision === handledWatchRef.current) return;
    handledWatchRef.current = refsWatchRevision;
    let cancelled = false;
    void ipc
      .historyRefs(targetId)
      .then((next) => {
        if (cancelled) return;
        if (!refsWatchRef.current.observe(next)) return;
        setRefs(next);
        const current = queryRef.current;
        current.resetLog(
          `${current.key}|${current.firstParent ? "1p" : "all"}|watch:${refsWatchRevision}`,
        );
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [refsWatchRevision, targetId]);

  // ---------- 选中提交的详情与改动文件 ----------
  const selectedOid = state.selectedCommit;
  const selectedPreview = useMemo(() => {
    if (!details.detail || details.targetId !== targetId || details.detail.oid !== selectedOid || details.loading) return null;
    if (details.merge && details.merge.mode !== effectiveMergeMode(details.detail.parents.length, state.mergeByParent)) return null;
    return previewTargetId === targetId && previewFile?.commit === selectedOid
      ? { target: previewFile, title: previewTitle }
      : firstFileOf(details, selectedOid ?? "");
  }, [details, previewFile, previewTitle, previewTargetId, selectedOid, state.mergeByParent, targetId]);
  useEffect(() => {
    setPreviewFile(null);
  }, [selectedOid, state.mergeByParent, targetId]);
  useEffect(() => {
    if (!targetId || !selectedOid) {
      setDetails(EMPTY_DETAILS);
      return;
    }
    let cancelled = false;
    setDetails((previous) => ({ ...previous, loading: true, error: null }));
    void (async () => {
      try {
        const commit = snapshot.commits.find((item) => item.oid === selectedOid) ?? null;
        const detail = await ipc.historyCommit(targetId, selectedOid);
        if (cancelled) return;
        if (detail.parents.length > 1) {
          const merge = await ipc.historyMergeFiles({
            targetId,
            commit: selectedOid,
            mode: effectiveMergeMode(detail.parents.length, state.mergeByParent),
          });
          if (cancelled) return;
          setDetails({ targetId, commit, detail, files: null, merge, error: null, loading: false });
        } else {
          const files = await ipc.historyCommitFiles({
            targetId,
            commit: selectedOid,
            parent: detail.parents[0] ?? null,
          });
          if (cancelled) return;
          setDetails({ targetId, commit, detail, files, merge: null, error: null, loading: false });
        }
      } catch (caught) {
        if (cancelled) return;
        setDetails({ targetId, commit: null, detail: null, files: null, merge: null, error: getErrorMessage(caught), loading: false });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [selectedOid, state.mergeByParent, targetId, snapshot.commits.length, refreshKey]);

  // 首次打开选首条；刷新时只有查到真正末尾才能证明旧选择已不在结果中。
  useEffect(() => {
    const selectedCommit = selectedLogCommit(snapshot.commits, state.selectedCommit,
      snapshot.exhausted && !snapshot.spanExceeded && !snapshot.error);
    if (selectedCommit !== state.selectedCommit) onState({ selectedCommit });
  }, [onState, snapshot.commits, snapshot.error, snapshot.exhausted, snapshot.spanExceeded, state.selectedCommit]);

  // 分页重置会暂时缩短原生滚动区；先补到原视口深度，再恢复，忽略中间的夹紧事件。
  useEffect(() => {
    if (!active) return;
    pendingScrollRef.current = restoreScrollRef.current;
  }, [active, targetId]);
  useEffect(() => {
    const top = pendingScrollRef.current;
    if (!active || top === null || !targetId || snapshot.loading) return;
    if (!snapshot.error && snapshot.commits.length < logVisibleEnd(top, viewport, LOG_ROW_HEIGHT) && !snapshot.exhausted) {
      void pager.loadMore();
      return;
    }
    const element = tableRef.current;
    if (!element) return;
    element.scrollTop = top;
    setScrollTop(element.scrollTop);
    pendingScrollRef.current = null;
    if (element.scrollTop !== top) onState({ scrollTop: element.scrollTop });
  }, [active, onState, pager, snapshot, targetId, viewport]);

  const rows = snapshot.commits;
  const graphRows = graphRowsRef.current;
  const longEdges = useMemo(
    () => collapseLongEdges(graphRows, settings.longEdges, 24),
    // graphVersion 参与依赖：图变化后重新折叠。
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [graphVersion, settings.longEdges],
  );

  const total = rows.length;
  const firstVisible = Math.max(0, Math.floor(scrollTop / LOG_ROW_HEIGHT) - ROW_OVERSCAN);
  const lastVisible = Math.min(total, Math.ceil((scrollTop + viewport) / LOG_ROW_HEIGHT) + ROW_OVERSCAN);
  const visible = rows.slice(firstVisible, Math.max(firstVisible, lastVisible));
  const tableHeight = total * LOG_ROW_HEIGHT;
  const graphColumnWidth = Math.max(2, ...graphRows.map((row) => row.width)) * LOG_LANE_WIDTH;
  const tableMinWidth = graphColumnWidth + 280 + (settings.columns.author ? 150 : 0)
    + (settings.columns.date ? 110 : 0) + (settings.columns.hash ? 80 : 0);

  // 滚动位置按 worktree 保留，但不逐像素写入全局状态：节流后再存，
  // 否则外层 App 会被每一次滚动事件重渲染。
  const persistScrollRef = useRef<number | null>(null);
  const onScroll = (event: React.UIEvent<HTMLDivElement>) => {
    if (pendingScrollRef.current !== null) return;
    const element = event.currentTarget;
    setScrollTop(element.scrollTop);
    if (persistScrollRef.current !== null) window.clearTimeout(persistScrollRef.current);
    persistScrollRef.current = window.setTimeout(() => {
      persistScrollRef.current = null;
      onState({ scrollTop: scrollTopRef.current });
    }, 200);
    pager.maybeLoadMore(logVisibleEnd(element.scrollTop, element.clientHeight, LOG_ROW_HEIGHT));
  };
  useEffect(
    () => () => {
      if (persistScrollRef.current !== null) {
        window.clearTimeout(persistScrollRef.current);
        persistScrollRef.current = null;
        onState({ scrollTop: scrollTopRef.current });
      }
    },
    [onState, targetId],
  );

  useEffect(() => {
    const element = tableRef.current;
    if (!element) return;
    const measure = () => {
      if (element.clientHeight > 0) setViewport(element.clientHeight);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const moveSelection = useCallback((delta: number | "home" | "end") => {
    if (!rows.length) return;
    const index = selectedOid ? rows.findIndex((commit) => commit.oid === selectedOid) : -1;
    let nextIndex: number;
    if (delta === "home") nextIndex = 0;
    else if (delta === "end") nextIndex = rows.length - 1;
    else nextIndex = Math.min(rows.length - 1, Math.max(0, index + delta));
    const next = rows[nextIndex];
    if (!next || next.oid === selectedOid) return;
    onState({ selectedCommit: next.oid });
    revealRow(nextIndex);
  }, [onState, rows, selectedOid]);

  const revealRow = (index: number) => {
    const element = tableRef.current;
    if (!element) return;
    const top = index * LOG_ROW_HEIGHT;
    const bottom = top + LOG_ROW_HEIGHT;
    if (top < element.scrollTop) element.scrollTop = top;
    else if (bottom > element.scrollTop + element.clientHeight) {
      element.scrollTop = bottom - element.clientHeight;
    }
  };

  // 键盘：↑/↓、Home/End、Esc（从筛选框回到表格）、⌘F 聚焦文字筛选。
  useEffect(() => {
    if (!active) return;
    const handler = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const target = event.target as HTMLElement | null;
      const inInput = target?.tagName === "INPUT" || target?.tagName === "TEXTAREA";
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        filterInputRef.current?.focus();
        filterInputRef.current?.select();
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "d") {
        event.preventDefault();
        const file = selectedPreview?.target ?? null;
        if (file) openFileTarget(file);
        return;
      }
      if (event.key === "Escape" && filterFocus) {
        event.preventDefault();
        setFilterFocus(false);
        filterInputRef.current?.blur();
        tableRef.current?.focus();
        return;
      }
      if (inInput) return;
      if (!target || !tableRef.current?.contains(target)) return;
      if (event.key === "ArrowDown") {
        event.preventDefault();
        moveSelection(1);
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        moveSelection(-1);
      } else if (event.key === "Home") {
        event.preventDefault();
        moveSelection("home");
      } else if (event.key === "End") {
        event.preventDefault();
        moveSelection("end");
      } else if (event.key === "Enter") {
        // Enter 打开所选文件；没有预览目标时用这个提交的第一个可打开文件。
        const file = selectedPreview?.target ?? null;
        if (file) {
          event.preventDefault();
          openFileTarget(file);
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [active, details, filterFocus, moveSelection, selectedPreview, selectedOid]);

  const openFileTarget = (file: HistoryFileTarget) => {
    activateFile(file, commitTabTitle(file.path, file.commit));
  };

  const toTarget = (file: HistoryFile, parent: string | null): HistoryFileTarget | null => {
    if (!file.diffId || !selectedOid || details.targetId !== targetId || details.detail?.oid !== selectedOid || details.loading) return null;
    return {
      commit: selectedOid,
      parent,
      path: file.path,
      originalPath: file.originalPath,
      oldOid: file.oldOid,
      newOid: file.newOid,
      status: file.status,
      diffId: file.diffId,
    };
  };

  const selectFile = (target: HistoryFileTarget, title: string) => {
    setPreviewFile(target);
    setPreviewTitle(title);
    setPreviewTargetId(targetId);
  };

  const activateFile = (target: HistoryFileTarget, title: string) => {
    if (details.targetId !== targetId || details.detail?.oid !== target.commit || details.loading) return;
    if (details.merge?.mode === "combined") {
      onOpenMergeCommitFile({ commit: target.commit, path: target.path, title });
      return;
    }
    onOpenCommitFile({ target, title });
  };

  const onRowContextMenu = async (event: React.MouseEvent, commit: HistoryCommit) => {
    event.preventDefault();
    onState({ selectedCommit: commit.oid });
    await showCommitContextMenu({
      copy: () => onCopy(commit.oid),
      branch: () => onCreateBranchAt(commit.oid),
    });
  };

  const filterCount = activeFilterCount(filter);
  return (
    <div className="log-view flex h-full min-h-0 flex-col bg-[var(--editor-background)]">
      {/* 工具栏：文字、分支、作者、日期、路径（L04），以及列与预览开关。 */}
      <div className="flex shrink-0 flex-wrap items-center gap-1 border-b border-panel-border px-2 py-1">
        <div className="relative">
          <Codicon name="search" size={12} className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-descriptionforeground" />
          <input
            ref={filterInputRef}
            className="log-input w-[220px] pl-6"
            placeholder={messages.log.textPlaceholder}
            aria-label={messages.log.text}
            spellCheck={false}
            value={filter.text}
            onFocus={() => setFilterFocus(true)}
            onBlur={() => setFilterFocus(false)}
            onChange={(event) => onState({ filter: { ...filter, text: event.target.value } })}
          />
        </div>
        <Dropdown
          width={280}
          trigger={
            <button type="button" className={`log-filter-button${filter.branches.length ? " active" : ""}`}>
              <Codicon name="git-branch" size={13} />
              <span className="max-w-[130px] truncate">
                {filter.branches.length ? messages.log.branchCount(filter.branches.length) : messages.log.branchAll}
              </span>
              <Codicon name="chevron-down" size={11} />
            </button>
          }
        >
          <BranchFilterMenu
            refs={refs}
            selected={filter.branches}
            onChange={(branches) => onState({ filter: { ...filter, branches } })}
          />
        </Dropdown>
        <Dropdown
          width={260}
          trigger={
            <button type="button" className={`log-filter-button${filter.authors.length ? " active" : ""}`}>
              <Codicon name="person" size={13} />
              <span className="max-w-[110px] truncate">
                {filter.authors.length ? messages.log.authorCount(filter.authors.length) : messages.log.authorAll}
              </span>
              <Codicon name="chevron-down" size={11} />
            </button>
          }
        >
          <AuthorFilterMenu
            commits={rows}
            selected={filter.authors}
            me={me}
            onChange={(authors) => onState({ filter: { ...filter, authors } })}
          />
        </Dropdown>
        <Dropdown
          width={260}
          trigger={
            <button type="button" className={`log-filter-button${filter.date !== "all" ? " active" : ""}`}>
              <Codicon name="calendar" size={13} />
              <span>{messages.log.dateLabels[filter.date] ?? messages.log.dateLabels.all}</span>
              <Codicon name="chevron-down" size={11} />
            </button>
          }
        >
          <DateFilterMenu
            filter={filter}
            onChange={(patch) => onState({ filter: { ...filter, ...patch } })}
          />
        </Dropdown>
        <Dropdown
          width={320}
          trigger={
            <button type="button" className={`log-filter-button${filter.paths.length ? " active" : ""}`}>
              <Codicon name="file" size={13} />
              <span className="max-w-[110px] truncate">
                {filter.paths.length ? messages.log.pathCount(filter.paths.length) : messages.log.pathAll}
              </span>
              <Codicon name="chevron-down" size={11} />
            </button>
          }
        >
          <PathFilterMenu
            files={repoFiles}
            selected={filter.paths}
            onChange={(paths) => onState({ filter: { ...filter, paths } })}
          />
        </Dropdown>
        <span className="flex-1" />
        <button
          type="button"
          className={`log-filter-button${previewOpen ? " active" : ""}`}
          title={messages.log.previewToggle}
          aria-pressed={previewOpen}
          onClick={() => onPreviewOpen(!previewOpen)}
        >
          <Codicon name="open-preview" size={13} />
          {messages.log.preview}
        </button>
        <Dropdown
          width={220}
          trigger={
            <button type="button" className="log-filter-button" title={messages.log.columns}>
              <Codicon name="settings" size={13} />
              <Codicon name="chevron-down" size={11} />
            </button>
          }
        >
          <p className="px-2 py-1 text-[11px] text-descriptionforeground">{messages.log.columns}</p>
          {(["author", "date", "hash"] as const).map((column) => (
            <label key={column} className="flex items-center gap-2 rounded-[3px] px-2 py-1 text-[12px] hover:bg-toolbar-hoverbackground">
              <input
                type="checkbox"
                checked={settings.columns[column]}
                onChange={(event) => onSettings({ columns: { ...settings.columns, [column]: event.target.checked } })}
              />
              {messages.log.columnLabels[column]}
            </label>
          ))}
          <MenuSeparator />
          <label className="flex items-center gap-2 rounded-[3px] px-2 py-1 text-[12px] hover:bg-toolbar-hoverbackground">
            <input
              type="checkbox"
              checked={settings.longEdges}
              onChange={(event) => onSettings({ longEdges: event.target.checked })}
            />
            {messages.log.longEdges}
          </label>
          <label className="flex items-center gap-2 rounded-[3px] px-2 py-1 text-[12px] hover:bg-toolbar-hoverbackground">
            <input
              type="checkbox"
              checked={settings.firstParent}
              onChange={(event) => onSettings({ firstParent: event.target.checked })}
            />
            {messages.log.firstParent}
          </label>
        </Dropdown>
        {filterCount > 0 ? (
          <button
            type="button"
            className="log-filter-button"
            title={messages.log.resetFilter}
            onClick={() => onState({ filter: defaultFilter() })}
          >
            <Codicon name="clear-all" size={13} />
            {messages.log.resetFilter}
          </button>
        ) : null}
      </div>

      <div className={`flex min-h-0 flex-1 ${settings.previewPosition === "bottom" ? "flex-col" : "flex-row"}`}>
        {/* 表格 + 详情 */}
        <div className={`log-main flex min-h-0 min-w-0 flex-1 ${previewOpen && selectedPreview && settings.previewPosition === "right" ? "log-main-with-preview" : ""}`}>
          <div
            ref={tableRef}
            tabIndex={0}
            role="grid"
            aria-label={messages.log.title}
            className="min-h-0 min-w-0 flex-1 overflow-auto outline-none"
            onScroll={onScroll}
          >
            <div style={{ minWidth: tableMinWidth }}>
              <div className="log-table-head sticky top-0 z-[1] flex h-[24px] items-center border-b border-panel-border bg-[var(--editor-background)] text-[11px] text-descriptionforeground">
                <span className="log-graph-col shrink-0" style={{ width: graphColumnWidth }}>{messages.log.graph}</span>
                <span className="min-w-0 flex-1 px-2">{messages.log.subject}</span>
                {settings.columns.author ? <span className="w-[150px] shrink-0 px-2">{messages.log.author}</span> : null}
                {settings.columns.date ? <span className="w-[110px] shrink-0 px-2">{messages.log.date}</span> : null}
                {settings.columns.hash ? <span className="w-[80px] shrink-0 px-2">{messages.log.hash}</span> : null}
              </div>
              <div className="relative" style={{ height: tableHeight }}>
                {visible.map((commit, offset) => {
                  const index = firstVisible + offset;
                  const graph = graphRows[index];
                  const selected = commit.oid === selectedOid;
                  const chips = refChips(
                    commit.refs,
                    currentBranch,
                  ).map((chip) => ({
                    ...chip,
                    // 当前分支突出显示；HEAD 也按当前分支处理。
                    current: chip.current || (commit.refs.some((ref) => ref.shortName === "HEAD") && chip.label === currentBranch),
                  }));
                  const rails = graph ? graph.railsAbove : [];
                  if (headCommit === commit.oid) {
                    chips.unshift({ kind: "branch", label: "HEAD", fullName: "HEAD", current: true });
                  }
                  return (
                    <div
                      key={commit.oid}
                      role="row"
                      aria-selected={selected}
                      tabIndex={-1}
                      className={`log-row absolute inset-x-0 flex items-center ${selected ? "selected" : ""}`}
                      style={{ top: index * LOG_ROW_HEIGHT, height: LOG_ROW_HEIGHT }}
                      onClick={() => onState({ selectedCommit: commit.oid })}
                      onDoubleClick={() => {
                        const first = firstFileOf(details, commit.oid);
                        if (first) activateFile(first.target, first.title);
                      }}
                      onContextMenu={(event) => void onRowContextMenu(event, commit)}
                    >
                      <GraphCell
                        graph={graph}
                        columnWidth={graphColumnWidth}
                        rails={rails}
                        longEdges={longEdges}
                        row={index}
                        onJump={(target) => {
                          revealRow(target);
                          const jump = rows[target];
                          if (jump) onState({ selectedCommit: jump.oid });
                        }}
                      />
                      <span className="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden px-2">
                        {chips.length ? <span className="flex max-w-[50%] shrink-0 gap-1.5 overflow-hidden">
                          {chips.map((chip) => (
                            <span
                              key={`${chip.fullName}`}
                              className={`log-ref-chip ${chip.current ? "current" : ""} ${chip.kind === "tag" ? "tag" : ""} ${chip.kind === "remoteBranch" ? "remote" : ""}`}
                              title={chip.fullName}
                            >
                              <Codicon
                                name={chip.kind === "tag" ? "tag" : chip.kind === "remoteBranch" ? "cloud" : "git-branch"}
                                size={10}
                              />
                              {chip.label}
                            </span>
                          ))}
                        </span> : null}
                        <span className="min-w-0 truncate" title={commit.subject}>{commit.subject}</span>
                      </span>
                      {settings.columns.author ? (
                        <span className="w-[150px] shrink-0 truncate px-2 text-[11.5px] text-descriptionforeground" title={authorKey(commit)}>
                          {authorLabel(commit)}
                        </span>
                      ) : null}
                      {settings.columns.date ? (
                        <span className="w-[110px] shrink-0 px-2 text-[11.5px] text-descriptionforeground tabular-nums">
                          {formatCommitDate(commit.author.time)}
                        </span>
                      ) : null}
                      {settings.columns.hash ? (
                        <span className="w-[80px] shrink-0 px-2 font-mono text-[11.5px] text-descriptionforeground">
                          {shortOid(commit.oid)}
                        </span>
                      ) : null}
                    </div>
                  );
                })}
              </div>
              {snapshot.loading ? (
                <div className="px-3 py-2 text-[12px] text-descriptionforeground">{messages.log.loading}</div>
              ) : null}
              {snapshot.error ? (
                <div className="flex items-center gap-2 px-3 py-2 text-[12px] text-destructive">
                  <Codicon name="warning" size={13} />
                  <span className="min-w-0 flex-1">{snapshot.error}</span>
                  <button type="button" className="log-filter-button" onClick={() => resetLog(`${key}|retry`)}>
                    {messages.log.retry}
                  </button>
                </div>
              ) : null}
              {!snapshot.loading && !snapshot.error && total === 0 ? (
                <div className="flex flex-col items-start gap-2 px-3 py-4 text-[12.5px] text-descriptionforeground">
                  <span>{hasActiveFilter(filter) ? messages.log.noMatches : messages.log.empty}</span>
                  {hasActiveFilter(filter) ? (
                    <button type="button" className="log-filter-button" onClick={() => onState({ filter: defaultFilter() })}>
                      {messages.log.resetFilter}
                    </button>
                  ) : null}
                </div>
              ) : null}
              {snapshot.exhausted && !snapshot.spanExceeded && total > 0 && !snapshot.loading ? (
                <div className="px-3 py-2 text-[11.5px] text-descriptionforeground">{messages.log.endOfHistory}</div>
              ) : null}
            </div>
          </div>
          <CommitDetails
            details={details}
            refLabels={refs
              .filter((ref) => ref.commit === selectedOid)
              .map((ref) => ({ fullName: ref.fullName, shortName: ref.shortName, kind: ref.kind }))}
            groupByDirectory={groupByDirectory}
            mergeByParent={state.mergeByParent}
            onMergeByParent={(value) => onState({ mergeByParent: value })}
            onSelectFile={(file, parent) => {
              const target = toTarget(file, parent);
              if (target) selectFile(target, commitTabTitle(file.path, selectedOid ?? ""));
            }}
            onActivateFile={(file, parent) => {
              const target = toTarget(file, parent);
              if (target) activateFile(target, commitTabTitle(file.path, selectedOid ?? ""));
            }}
            onCopy={onCopy}
            onLocateParent={(oid) => {
              const index = rows.findIndex((commit) => commit.oid === oid);
              if (index >= 0) {
                onState({ selectedCommit: oid });
                revealRow(index);
              } else {
                onNotify({ tone: "warning", title: messages.log.parentNotLoaded });
              }
            }}
          />
        </div>
        {previewOpen && selectedPreview && targetId ? (
          <div className={`min-h-0 min-w-0 shrink-0 border-panel-border ${settings.previewPosition === "bottom" ? "h-[45%] border-t" : "w-[45%] border-l"}`}>
            <div className="flex h-[24px] items-center gap-2 border-b border-panel-border px-2 text-[11.5px] text-descriptionforeground">
              <Codicon name="open-preview" size={12} />
              <span className="min-w-0 flex-1 truncate" title={selectedPreview.title}>{selectedPreview.title}</span>
              <button type="button" className="icon-action size-[20px]" title={messages.log.openInTab} onClick={() => activateFile(selectedPreview.target, selectedPreview.title)}>
                <Codicon name="link-external" size={12} />
              </button>
              <button type="button" className="icon-action size-[20px]" title={messages.log.closePreview} onClick={() => onPreviewOpen(false)}>
                <Codicon name="close" size={12} />
              </button>
            </div>
            <div className="h-[calc(100%-24px)] min-h-0">
              {details.merge?.mode === "combined" ? (
                <CommitMergeView targetId={targetId} commit={selectedPreview.target.commit}
                  path={selectedPreview.target.path} title={selectedPreview.title}
                  fontSize={diffPresentation.fontSize} fontFamily={diffPresentation.fontFamily}
                  onClose={() => onPreviewOpen(false)} />
              ) : (
                <CommitDiffView targetId={targetId} file={selectedPreview.target} title={selectedPreview.title}
                  {...diffPresentation} ignoreWhitespace={diffPresentation.options.ignoreWhitespace}
                  softWrap={diffPresentation.options.softWrap} onClose={() => onPreviewOpen(false)} />
              )}
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}

/** 一行的分支图：轨道、提交点、横向连接与长边折叠箭头。 */
function GraphCell({
  graph,
  columnWidth,
  rails,
  longEdges,
  row,
  onJump,
}: {
  graph: GraphRow | undefined;
  columnWidth: number;
  rails: number[];
  longEdges: ReturnType<typeof collapseLongEdges>;
  row: number;
  onJump: (row: number) => void;
}) {
  if (!graph) return <span className="log-graph-col shrink-0" style={{ width: columnWidth }} />;
  const markers = longEdges.markers.filter((marker) => marker.row === row);
  return (
    <span className="log-graph-col relative shrink-0" style={{ width: columnWidth }}>
      {rails.map((lane) => {
        const collapsed = longEdges.collapsed.has(`${row}:${lane}`);
        return (
          <span
            key={`rail-${lane}`}
            className={`log-rail${collapsed ? " collapsed" : ""}`}
            style={{ left: lane * LOG_LANE_WIDTH, background: collapsed ? "transparent" : laneColor(lane) }}
          />
        );
      })}
      {graph.railsBelow.map((lane) =>
        rails.includes(lane) ? null : (
          <span
            key={`below-${lane}`}
            className="log-rail"
            style={{ left: lane * LOG_LANE_WIDTH, top: "50%", height: "50%", background: laneColor(lane) }}
          />
        ),
      )}
      {rails.map((lane) =>
        graph.railsBelow.includes(lane) ? null : (
          <span
            key={`above-${lane}`}
            className="log-rail"
            style={{ left: lane * LOG_LANE_WIDTH, top: 0, height: "50%", background: laneColor(lane) }}
          />
        ),
      )}
      {graph.arms.map((arm) => {
        const left = Math.min(arm.from, arm.to) * LOG_LANE_WIDTH;
        const widthValue = Math.max(LOG_LANE_WIDTH, Math.abs(arm.to - arm.from) * LOG_LANE_WIDTH);
        return (
          <span
            key={`arm-${arm.from}-${arm.to}`}
            className="log-arm"
            style={{ left, width: widthValue, background: laneColor(arm.to) }}
          />
        );
      })}
      <span
        className="log-node"
        style={{ left: graph.lane * LOG_LANE_WIDTH, background: laneColor(graph.lane) }}
      />
      {markers.map((marker) => (
        <button
          key={`marker-${marker.lane}-${marker.direction}`}
          type="button"
          className="log-long-edge"
          style={{ left: marker.lane * LOG_LANE_WIDTH + (marker.direction === "down" ? 0 : 0) }}
          title={messages.log.longEdgeJump}
          onClick={(event) => {
            event.stopPropagation();
            onJump(marker.targetRow);
          }}
        >
          <Codicon name={marker.direction === "down" ? "chevron-down" : "chevron-up"} size={9} />
        </button>
      ))}
    </span>
  );
}

/** 提交详情：完整信息、作者 / 提交者、哈希、父提交与引用标签（L05）。 */
function CommitDetails({
  details,
  refLabels,
  groupByDirectory,
  mergeByParent,
  onMergeByParent,
  onSelectFile,
  onActivateFile,
  onCopy,
  onLocateParent,
}: {
  details: DetailsState;
  refLabels: HistoryRefLabel[];
  groupByDirectory: boolean;
  mergeByParent: boolean;
  onMergeByParent: (value: boolean) => void;
  onSelectFile: (file: HistoryFile, parent: string | null) => void;
  onActivateFile: (file: HistoryFile, parent: string | null) => void;
  onCopy: (text: string) => void;
  onLocateParent: (oid: string) => void;
}) {
  const [toggledNodes, setToggledNodes] = useState<Set<string>>(new Set());
  const detail = details.detail;
  if (details.loading && !detail) {
    return <div className="log-details w-[360px] shrink-0 border-l border-panel-border p-3 text-[12px] text-descriptionforeground">{messages.log.loadingDetails}</div>;
  }
  if (details.error) {
    return <div className="log-details w-[360px] shrink-0 border-l border-panel-border p-3 text-[12px] text-destructive">{details.error}</div>;
  }
  if (!detail) {
    return <div className="log-details w-[360px] shrink-0 border-l border-panel-border p-3 text-[12px] text-descriptionforeground">{messages.log.noSelection}</div>;
  }
  const isMerge = detail.parents.length > 1;
  const groups = details.merge
    ? details.merge.groups
    : details.files
      ? [{ parent: null, diffParent: detail.parents[0] ?? null, files: details.files.files }]
      : [];

  const fileRow = (file: HistoryFile, parent: string | null) => {
    const status = fileStatusLetter(file);
    return (
      <div
        key={`${parent ?? ""}:${file.path}`}
        role="button"
        tabIndex={0}
        className="log-file-row"
        title={file.readonlyReason ? readonlyReasonText(file) : file.path}
        onClick={() => onSelectFile(file, parent)}
        onDoubleClick={() => onActivateFile(file, parent)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            event.stopPropagation();
            onActivateFile(file, parent);
          }
        }}
      >
        <FileTypeIcon path={file.path} />
        <span className="min-w-0 flex-1 truncate">{file.path}</span>
        {file.originalPath ? <span className="shrink-0 text-[11px] text-descriptionforeground">← {file.originalPath}</span> : null}
        <span className={`log-status log-status-${file.status}`}>{status}</span>
        <span className="shrink-0 font-mono text-[10.5px] text-descriptionforeground">
          {file.added ? `+${file.added}` : ""}
          {file.deleted ? ` -${file.deleted}` : ""}
        </span>
        {!file.openable ? <Codicon name="lock" size={11} className="shrink-0 text-descriptionforeground" /> : null}
      </div>
    );
  };

  const tree = buildFileTree(groups, groupByDirectory);
  const renderNode = (node: FileTreeNode): React.ReactNode => {
    if (node.kind === "file") return fileRow(node.file, node.parent);
    const open = node.kind === "parent" ? !toggledNodes.has(node.key) : toggledNodes.has(node.key);
    return (
      <div key={node.key}>
        <button type="button" className="log-file-row w-full text-left" aria-expanded={open}
          onClick={() => setToggledNodes((current) => {
            const next = new Set(current);
            if (next.has(node.key)) next.delete(node.key);
            else next.add(node.key);
            return next;
          })}>
          <Codicon name={open ? "chevron-down" : "chevron-right"} size={12} />
          <Codicon name={node.kind === "parent" ? "git-commit" : "folder"} size={13} />
          <span className="min-w-0 flex-1 truncate">{node.name}</span>
        </button>
        {open ? <div className="pl-3">{node.children.length
          ? node.children.map(renderNode)
          : <p className="px-3 py-1 text-[11.5px] text-descriptionforeground">{messages.log.noFiles}</p>}</div> : null}
      </div>
    );
  };
  return (
    <div className="log-details flex w-[360px] shrink-0 flex-col overflow-hidden border-l border-panel-border">
      <div className="flex flex-wrap items-center gap-1.5 border-b border-panel-border px-2 py-1 text-[11.5px] text-descriptionforeground">
        <span className="min-w-0 flex-1 truncate">
          {messages.log.changedFiles} · {messages.log.fileCount(groups.reduce((total, group) => total + group.files.length, 0))}
        </span>
        {isMerge ? (
          <label className="flex min-w-0 items-center gap-1" title={messages.log.mergeByParentHint}>
            <input type="checkbox" checked={mergeByParent} onChange={(event) => onMergeByParent(event.target.checked)} />
            {messages.log.mergeByParent}
          </label>
        ) : null}
      </div>
      <div className="max-h-[45%] min-h-[90px] overflow-auto py-1">
        {tree.length === 0 ? (
          <p className="px-3 py-2 text-[12px] text-descriptionforeground">{messages.log.noFiles}</p>
        ) : (
          tree.map(renderNode)
        )}
        {details.files?.truncated || details.merge?.truncated ? (
          <p className="px-3 py-1 text-[11.5px] text-[var(--editorwarning-foreground)]">{messages.log.filesTruncated}</p>
        ) : null}
        {details.merge?.mode === "combined" ? (
          <p className="px-3 py-1 text-[11.5px] text-descriptionforeground">{messages.log.mergeCombinedHint}</p>
        ) : null}
      </div>
      <div className="min-h-0 flex-1 overflow-auto border-t border-panel-border px-3 py-2 text-[12px]">
        <div className="flex items-start gap-2">
          <span className="min-w-0 flex-1 whitespace-pre-wrap break-words font-medium">{detail.message.trim() || messages.log.emptyMessage}</span>
          <button type="button" className="icon-action size-[20px]" title={messages.log.copyHash} onClick={() => onCopy(detail.oid)}>
            <Codicon name="copy" size={12} />
          </button>
        </div>
        <dl className="mt-2 space-y-1 text-[11.5px] text-descriptionforeground">
          <div className="flex gap-2">
            <dt className="w-[64px] shrink-0">{messages.log.author}</dt>
            <dd className="min-w-0 flex-1 break-words">
              {detail.author.name} &lt;{detail.author.email}&gt; · {formatCommitDate(detail.author.time)}
            </dd>
          </div>
          {detail.committer.time !== detail.author.time || detail.committer.email !== detail.author.email ? (
            <div className="flex gap-2">
              <dt className="w-[64px] shrink-0">{messages.log.committer}</dt>
              <dd className="min-w-0 flex-1 break-words">
                {detail.committer.name} &lt;{detail.committer.email}&gt; · {formatCommitDate(detail.committer.time)}
              </dd>
            </div>
          ) : null}
          <div className="flex gap-2">
            <dt className="w-[64px] shrink-0">{messages.log.hash}</dt>
            <dd className="min-w-0 flex-1 break-all font-mono">{detail.oid}</dd>
          </div>
          {refLabels.length > 0 ? (
            <div className="flex gap-2">
              <dt className="w-[64px] shrink-0">{messages.log.refsLabel}</dt>
              <dd className="flex min-w-0 flex-1 flex-wrap gap-1">
                {refChips(refLabels, null).map((chip) => (
                  <span key={chip.fullName} className={`log-ref-chip ${chip.kind}`} title={chip.fullName}>
                    {chip.label}
                  </span>
                ))}
              </dd>
            </div>
          ) : null}
          <div className="flex gap-2">
            <dt className="w-[64px] shrink-0">{messages.log.parents}</dt>
            <dd className="flex min-w-0 flex-1 flex-wrap gap-1">
              {detail.parents.length === 0 ? <span>{messages.log.rootCommit}</span> : null}
              {detail.parents.map((parent) => (
                <button
                  key={parent}
                  type="button"
                  className="log-parent-link"
                  title={`${messages.log.gotoParent}：${parent}`}
                  onClick={() => onLocateParent(parent)}
                >
                  {shortOid(parent)}
                </button>
              ))}
            </dd>
          </div>
          {detail.messageEncoding ? (
            <div className="flex gap-2">
              <dt className="w-[64px] shrink-0">{messages.log.encoding}</dt>
              <dd className="min-w-0 flex-1">{detail.messageEncoding}</dd>
            </div>
          ) : null}
        </dl>
        {detail.messageLossy ? (
          <p className="mt-2 text-[11.5px] text-[var(--editorwarning-foreground)]">{messages.log.messageLossy}</p>
        ) : null}
      </div>
    </div>
  );
}

function fileStatusLetter(file: HistoryFile): string {
  switch (file.status) {
    case "added":
      return "A";
    case "deleted":
      return "D";
    case "renamed":
      return "R";
    case "copied":
      return "C";
    case "typeChanged":
      return "T";
    default:
      return "M";
  }
}

function readonlyReasonText(file: HistoryFile): string {
  switch (file.readonlyReason) {
    case "nonUtf8Path":
      return messages.log.readonlyNonUtf8;
    case "submodule":
      return messages.log.readonlySubmodule;
    default:
      return file.path;
  }
}

function firstFileOf(details: DetailsState, commit: string): { target: HistoryFileTarget; title: string } | null {
  if (details.detail?.oid !== commit) return null;
  const groups = details.merge?.groups ?? [{
    parent: null, diffParent: details.detail.parents[0] ?? null, files: details.files?.files ?? [],
  }];
  const node = firstHistoryFile(groups);
  if (!node?.file.diffId) return null;
  const { file, parent } = node;
  return {
    target: {
      commit,
      parent,
      path: file.path,
      originalPath: file.originalPath,
      oldOid: file.oldOid,
      newOid: file.newOid,
      status: file.status,
      diffId: node.file.diffId,
    },
    title: commitTabTitle(file.path, commit),
  };
}

/** 原生右键菜单（M5 只有“复制哈希”“从此提交新建分支…”）。 */
async function showCommitContextMenu(actions: { copy: () => void; branch: () => void }): Promise<void> {
  try {
    const { Menu } = await import("@tauri-apps/api/menu");
    const menu = await Menu.new({
      items: [
        { id: "copy-hash", text: messages.log.copyHash, action: () => actions.copy() },
        { id: "new-branch", text: messages.log.branchFromCommit, action: () => actions.branch() },
      ],
    });
    await menu.popup();
  } catch {
    // 原生菜单不可用时退到同一个动作的最小入口：复制哈希 + 新建分支由工具栏提供。
    actions.copy();
  }
}

// ---------- 筛选子菜单 ----------

function BranchFilterMenu({
  refs,
  selected,
  onChange,
}: {
  refs: HistoryRef[];
  selected: string[];
  onChange: (next: string[]) => void;
}) {
  const options = branchFilterOptions(refs);
  const groups: Array<{ key: "local" | "remote" | "tag"; label: string }> = [
    { key: "local", label: messages.log.localBranches },
    { key: "remote", label: messages.log.remoteBranches },
    { key: "tag", label: messages.log.tags },
  ];
  return (
    <div className="max-h-[320px] overflow-auto">
      <MenuItem onClick={() => onChange([])}>
        <span className="flex items-center gap-2">
          {selected.length === 0 ? <Codicon name="check" size={12} /> : <span className="w-3" />}
          {messages.log.branchAll}
        </span>
      </MenuItem>
      <MenuSeparator />
      {groups.map((group) => {
        const items = options.filter((option) => option.group === group.key);
        if (items.length === 0) return null;
        return (
          <div key={group.key}>
            <p className="px-2 py-1 text-[11px] text-descriptionforeground">{group.label}</p>
            {items.map((option) => {
              const active = selected.includes(option.fullName);
              return (
                <label key={option.fullName} className="flex items-center gap-2 rounded-[3px] px-2 py-1 text-[12px] hover:bg-toolbar-hoverbackground">
                  <input
                    type="checkbox"
                    checked={active}
                    onChange={() =>
                      onChange(active ? selected.filter((name) => name !== option.fullName) : [...selected, option.fullName])
                    }
                  />
                  <span className="min-w-0 flex-1 truncate" title={option.fullName}>{option.label}</span>
                </label>
              );
            })}
          </div>
        );
      })}
    </div>
  );
}

function AuthorFilterMenu({
  commits,
  selected,
  me,
  onChange,
}: {
  commits: HistoryCommit[];
  selected: string[];
  me: { name: string | null; email: string | null } | null;
  onChange: (next: string[]) => void;
}) {
  const authors = useMemo(() => {
    const map = new Map<string, string>();
    for (const commit of commits) {
      const key = authorKey(commit);
      if (key) map.set(key, authorLabel(commit));
    }
    return [...map.entries()].sort((left, right) => left[1].localeCompare(right[1]));
  }, [commits]);
  const meKey = me && (me.name || me.email)
    ? me.name && me.email ? `${me.name} <${me.email}>` : (me.name ?? me.email ?? "")
    : null;
  return (
    <div className="max-h-[320px] overflow-auto">
      <MenuItem onClick={() => onChange([])}>
        <span className="flex items-center gap-2">
          {selected.length === 0 ? <Codicon name="check" size={12} /> : <span className="w-3" />}
          {messages.log.authorAll}
        </span>
      </MenuItem>
      {meKey ? (
        <MenuItem
          onClick={() => onChange(selected.includes(meKey) ? selected.filter((item) => item !== meKey) : [...selected, meKey])}
        >
          <span className="flex items-center gap-2">
            {selected.includes(meKey) ? <Codicon name="check" size={12} /> : <span className="w-3" />}
            {messages.log.me}（{meKey}）
          </span>
        </MenuItem>
      ) : null}
      <MenuSeparator />
      {authors.length === 0 ? <p className="px-2 py-1 text-[11.5px] text-descriptionforeground">{messages.log.noAuthors}</p> : null}
      {authors.map(([key, label]) => {
        const active = selected.includes(key);
        return (
          <label key={key} className="flex items-center gap-2 rounded-[3px] px-2 py-1 text-[12px] hover:bg-toolbar-hoverbackground">
            <input
              type="checkbox"
              checked={active}
              onChange={() => onChange(active ? selected.filter((item) => item !== key) : [...selected, key])}
            />
            <span className="min-w-0 flex-1 truncate" title={key}>{label}</span>
          </label>
        );
      })}
    </div>
  );
}

function DateFilterMenu({
  filter,
  onChange,
}: {
  filter: LogFilterState;
  onChange: (patch: Partial<LogFilterState>) => void;
}) {
  const presets: DatePreset[] = ["all", "last1", "last7", "last30", "range"];
  return (
    <div>
      {presets.map((preset) => (
        <MenuItem key={preset} onClick={() => onChange({ date: preset })}>
          <span className="flex items-center gap-2">
            {filter.date === preset ? <Codicon name="check" size={12} /> : <span className="w-3" />}
            {messages.log.dateLabels[preset]}
          </span>
        </MenuItem>
      ))}
      {filter.date === "range" ? (
        <div className="flex items-center gap-1 px-2 py-1.5">
          <input
            className="log-input w-[110px]"
            placeholder="2026-10-01"
            value={filter.since}
            onChange={(event) => onChange({ since: event.target.value })}
          />
          <span className="text-descriptionforeground">–</span>
          <input
            className="log-input w-[110px]"
            placeholder="2026-10-31"
            value={filter.until}
            onChange={(event) => onChange({ until: event.target.value })}
          />
        </div>
      ) : null}
    </div>
  );
}

function PathFilterMenu({
  files,
  selected,
  onChange,
}: {
  files: string[];
  selected: string[];
  onChange: (next: string[]) => void;
}) {
  const [query, setQuery] = useState("");
  const text = query.trim().toLowerCase();
  const directories = useMemo(() => {
    const set = new Set<string>();
    for (const file of files) {
      const parts = file.split("/");
      parts.pop();
      let prefix = "";
      for (const part of parts) {
        prefix = prefix ? `${prefix}/${part}` : part;
        set.add(prefix);
      }
    }
    const list = [...set].sort();
    return (text ? list.filter((dir) => dir.toLowerCase().includes(text)) : list).slice(0, 100);
  }, [files, text]);
  const options = useMemo(() => {
    const list = text ? files.filter((file) => file.toLowerCase().includes(text)) : files;
    return list.slice(0, 200);
  }, [files, text]);
  const toggle = (path: string) => {
    onChange(selected.includes(path) ? selected.filter((item) => item !== path) : [...selected, path]);
  };
  return (
    <div>
      <div className="px-2 py-1">
        <input
          className="log-input w-full"
          placeholder={messages.log.pathSearch}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
      </div>
      {selected.length > 0 ? (
        <div className="flex flex-wrap gap-1 px-2 pb-1">
          {selected.map((path) => (
            <button key={path} type="button" className="log-ref-chip" onClick={() => toggle(path)} title={messages.log.removePath}>
              {path}
              <Codicon name="close" size={10} />
            </button>
          ))}
        </div>
      ) : null}
      <div className="max-h-[280px] overflow-auto">
        {directories.length > 0 ? (
          <p className="px-2 py-1 text-[11px] text-descriptionforeground">{messages.log.directories}</p>
        ) : null}
        {directories.map((directory) => (
          <label key={`dir:${directory}`} className="flex items-center gap-2 rounded-[3px] px-2 py-1 text-[12px] hover:bg-toolbar-hoverbackground">
            <input type="checkbox" checked={selected.includes(directory)} onChange={() => toggle(directory)} />
            <Codicon name="folder" size={12} />
            <span className="min-w-0 flex-1 truncate" title={directory}>{directory}</span>
          </label>
        ))}
        {options.length === 0 && directories.length === 0 ? (
          <p className="px-2 py-1 text-[11.5px] text-descriptionforeground">{messages.log.noPaths}</p>
        ) : null}
        {options.length > 0 ? (
          <p className="px-2 py-1 text-[11px] text-descriptionforeground">{messages.log.filesLabel}</p>
        ) : null}
        {options.map((path) => (
          <label key={path} className="flex items-center gap-2 rounded-[3px] px-2 py-1 text-[12px] hover:bg-toolbar-hoverbackground">
            <input type="checkbox" checked={selected.includes(path)} onChange={() => toggle(path)} />
            <FileTypeIcon path={path} />
            <span className="min-w-0 flex-1 truncate" title={path}>{path}</span>
          </label>
        ))}
      </div>
    </div>
  );
}
