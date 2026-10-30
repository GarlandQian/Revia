/**
 * 只读 diff 视图：自绘、窗口化、只渲染可见区域（Q32）。
 *
 * 数据来自 `RowWindowStore`（按可见范围分块取行）与 `DiffHighlights`（按行范围取类别）。
 * 视图本身不做差异计算，也不持有整份 diff（ADR-0004）。
 *
 * 滚动由本组件接管，不用浏览器原生滚动（ADR-0008）：原生滚动交给系统单独处理，
 * 快速甩动时画面会先跑到内容前面，整块变白。这里直接接收滚轮 / 触控板事件，
 * 在同一帧里同时更新内容位移、滚动条和要渲染的行。数据还没到的行露出
 * `.diff-canvas` 底图里“加载中的行”。
 */

import { memo, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { flushSync } from "react-dom";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { FileVersion } from "@/types/bindings/FileVersion";
import type { HunkAction } from "@/types/bindings/HunkAction";
import type { SegmentAction } from "@/types/bindings/SegmentAction";
import type { RenderRow } from "@/types/bindings/RenderRow";
import type { RowCell } from "@/types/bindings/RowCell";
import type { ViewMode } from "@/types/bindings/ViewMode";
import { ipc } from "@/lib/ipc";
import { formatAppError } from "@/lib/error";
import { getLocale, messages, type Locale } from "@/lib/i18n/index";
import { codeFontStack } from "@/lib/theme";
import { Codicon } from "@/components/ui/codicon";
import { logToBackend } from "./log";
import type { DiffHighlights } from "./highlightStore";
import type { DiffRows } from "./rowWindowStore";
import { visibleHighlightLines } from "./visibleHighlightLines";
import { matchesHunkAction, segmentActionDisabled } from "./hunkAction";
import { RowHeights } from "./rowHeights";
import { useFontRevision } from "./useFontRevision";
import { unifiedFileAnchor, type FileAnchor } from "./fileAnchor";

export const ROW_HEIGHT = 19;
/** 滚动停止多久之后才请求新高亮（毫秒）。 */
const SCROLL_IDLE_MS = 150;
/** 大幅跳转后恢复全量预渲染与语法颜色的目标帧数（约 2 帧，33ms）。 */
const JUMP_RESTORE_FRAMES = 2;
/** 大幅跳转后恢复预渲染与颜色的兜底超时（毫秒）。 */
const JUMP_RESTORE_FALLBACK_MS = 60;
/** 预渲染的屏数：滚动方向多渲染 3 屏，反方向 1 屏；每屏约 47 行，渲染成本约 1ms。 */
const OVERSCAN_AHEAD_SCREENS = 3;
const OVERSCAN_BEHIND_SCREENS = 1;
/** 滚动方向额外预取的屏数：快速滑动时目标块通常已经在内存里。 */
const PREFETCH_SCREENS = 2;
/** 滚动条滑块的最小高度（像素）。 */
const SCROLLBAR_MIN_THUMB = 24;
/** 方向键一次滚动的行数。 */
const ARROW_STEP_ROWS = 2;

export interface DiffViewHandle {
  getFileAnchor(): FileAnchor | null;
  scrollToRow(index: number): void;
  scrollToLine(line: number, version: FileVersion): Promise<void>;
  getScrollTop(): number;
  setScrollTop(top: number): void;
  getViewportHeight(): number;
  getContentHeight(): number;
  getVisibleRange(): { from: number; to: number };
  isCovered(from: number, to: number): boolean;
  isCompact(): boolean;
}

interface DiffViewBaseProps {
  view: ViewMode;
  fontSize: number;
  fontFamily?: string | null;
  rows: DiffRows;
  highlights: DiffHighlights;
  /** 软换行（Q139）：长行折行显示，行高随内容。 */
  softWrap?: boolean;
  /** 每帧采样：用于测量「可见区域里还没有数据的行数」。 */
  onSample?: (sample: { from: number; to: number; uncovered: number }) => void;
}

type DiffViewProps = DiffViewBaseProps & ({
  history?: never;
  worktreeId: string;
  entry: ChangeEntry;
  isWriting?: boolean;
  /** 段（块）操作是否可用（Q140：同时忽略空行一档只用于查看）。 */
  segmentActionsEnabled?: boolean;
  onHunkAction?: (action: HunkAction) => void;
  onHunkDiscard?: (action: HunkAction, section: string) => void;
  onSegmentAction?: (action: SegmentAction) => void;
  onSegmentDiscard?: (action: SegmentAction) => void;
  handleRef?: React.Ref<DiffViewHandle>;
} | {
  history: { key: string; path: string };
  worktreeId?: never;
  entry?: never;
  isWriting?: never;
  segmentActionsEnabled?: never;
  onHunkAction?: never;
  onHunkDiscard?: never;
  onSegmentAction?: never;
  onSegmentDiscard?: never;
  handleRef?: never;
});

export function DiffView(props: DiffViewProps) {
  const locale = getLocale();
  const { worktreeId, entry, view, fontSize, fontFamily, rows, highlights, handleRef, onSample, isWriting, onHunkAction, onHunkDiscard } = props;
  const displayPath = props.history ? props.history.path : props.entry.path;
  const sourceKey = props.history ? props.history.key : `${props.entry.changes}/${props.entry.path}`;
  const segmentActionsEnabled = props.segmentActionsEnabled ?? true;
  const softWrap = props.softWrap ?? false;
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const contentRef = useRef<HTMLDivElement | null>(null);
  const trackRef = useRef<HTMLDivElement | null>(null);
  const thumbRef = useRef<HTMLDivElement | null>(null);
  // 滚动位置只放在 ref 里：每帧直接写 DOM，不经过 React 状态。
  const scrollTopRef = useRef(0);
  // 只记录顶部是第几行：同一行内的像素级滚动不触发重新渲染。
  const [firstRow, setFirstRow] = useState(0);
  // 软换行时一个逻辑行可能高过视口：顶部行不变、底部已进入下一行时也要重新渲染。
  // 只在换行时更新，不换行的固定行高路径（D06）不受影响。
  const [, setBottomRow] = useState(0);
  // 最近一次滚动的方向（1 向下，-1 向上），决定预渲染往哪边多放。
  const scrollDirection = useRef(1);
  const [scrollIdle, setScrollIdle] = useState(true);
  const idleTimer = useRef<number | null>(null);
  // 大幅跳转的那一帧只渲染可见行纯文本，保证 1 帧内画出文字（Q52）；语法颜色与预渲染余量在约 2 帧内恢复。
  const [compact, setCompact] = useState(false);
  // 跳转那一帧只插入目标可见行：旧窗口的上百行着色节点先留在原位（视口之外），
  // 等颜色恢复的那次渲染再卸载，不在跳转的同步提交里删除。
  const [retained, setRetained] = useState<{ from: number; to: number } | null>(null);
  // 最近一次完整（非 compact）渲染的行范围。
  const renderedRange = useRef<{ from: number; to: number } | null>(null);
  const jumpRestoreRaf = useRef<number | null>(null);
  const jumpRestoreTimer = useRef<number | null>(null);
  const [viewportHeight, setViewportHeight] = useState(0);
  const [viewportWidth, setViewportWidth] = useState(0);
  // 软换行时量过的行高变了：重新排位置。
  const [, setLayoutVersion] = useState(0);
  const rowSnapshot = useSyncExternalStore(rows.subscribe, rows.getSnapshot);
  const version = rowSnapshot.version;
  const highlightSnapshot = useSyncExternalStore(highlights.subscribe, highlights.getSnapshot);

  const rowCount = rows.rowCountValue;
  // 软换行时宽度、字号或内容一变，量过的行高就作废（不换行时与宽度无关）。
  const wrapWidth = softWrap ? viewportWidth : 0;
  const fontRevision = useFontRevision(softWrap);
  const wrapFont = softWrap ? `${fontSize}/${fontFamily ?? ""}/${fontRevision}` : "";
  const epoch = rows.epochValue;
  const heights = useMemo(
    () => {
      void [wrapWidth, wrapFont, epoch, sourceKey, view];
      return new RowHeights(rowCount, ROW_HEIGHT, softWrap);
    },
    [rowCount, softWrap, wrapWidth, wrapFont, epoch, sourceKey, view],
  );
  const heightsRef = useRef(heights);
  const previousHeights = useRef(heights);
  heightsRef.current = heights;
  const contentHeight = heights.total();
  const geometry = useRef({ viewportHeight: 0, contentHeight: 0 });
  geometry.current = { viewportHeight, contentHeight };
  const changedHeights = previousHeights.current !== heights;
  const renderTop = changedHeights
    ? heights.yAt(previousHeights.current.rowAtFraction(scrollTopRef.current))
    : scrollTopRef.current;

  const cancelJumpRestore = useCallback(() => {
    if (jumpRestoreRaf.current != null) {
      cancelAnimationFrame(jumpRestoreRaf.current);
      jumpRestoreRaf.current = null;
    }
    if (jumpRestoreTimer.current != null) {
      window.clearTimeout(jumpRestoreTimer.current);
      jumpRestoreTimer.current = null;
    }
  }, []);

  const scheduleJumpRestore = useCallback(() => {
    cancelJumpRestore();
    let frameCount = 0;
    const onFrame = () => {
      frameCount += 1;
      if (frameCount >= JUMP_RESTORE_FRAMES) {
        jumpRestoreRaf.current = null;
        if (jumpRestoreTimer.current != null) {
          window.clearTimeout(jumpRestoreTimer.current);
          jumpRestoreTimer.current = null;
        }
        setCompact(false);
        setRetained(null);
      } else {
        jumpRestoreRaf.current = requestAnimationFrame(onFrame);
      }
    };
    jumpRestoreRaf.current = requestAnimationFrame(onFrame);
    jumpRestoreTimer.current = window.setTimeout(() => {
      cancelJumpRestore();
      setCompact(false);
      setRetained(null);
    }, JUMP_RESTORE_FALLBACK_MS);
  }, [cancelJumpRestore]);

  // 切换文件、改动侧或视图时，复位跳转与滚动状态，取消未完成的恢复任务（Issue 09 审查意见 1）
  useEffect(() => {
    cancelJumpRestore();
    if (idleTimer.current != null) {
      window.clearTimeout(idleTimer.current);
      idleTimer.current = null;
    }
    setCompact(false);
    setRetained(null);
    setScrollIdle(true);
  }, [cancelJumpRestore, sourceKey, view]);

  // 组件卸载时清理所有定时器与 rAF
  useEffect(() => {
    const element = viewportRef.current;
    // x18n-ignore
    logToBackend(`DiffView 已挂载：${displayPath}/${view} clientHeight=${element?.clientHeight}`);
    return () => {
      // x18n-ignore
      logToBackend(`DiffView 卸载：${displayPath}/${view}`);
      cancelJumpRestore();
      if (idleTimer.current != null) {
        window.clearTimeout(idleTimer.current);
        idleTimer.current = null;
      }
    };
  }, [cancelJumpRestore, displayPath, view]);

  useLayoutEffect(() => {
    const element = viewportRef.current;
    if (!element) return;
    const measure = () => {
      setViewportHeight(element.clientHeight);
      setViewportWidth(element.clientWidth);
    };
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    return () => observer.disconnect();
  }, []);

  /** 把当前滚动位置写到内容位移和滚动条上。 */
  const paintScrollPosition = useCallback(() => {
    const top = scrollTopRef.current;
    const content = contentRef.current;
    // 二维位移：内容随视口一起重绘，不单独提升成合成层，避免出现还没绘制的图块。
    if (content) content.style.transform = `translateY(${-top}px)`;
    const track = trackRef.current;
    const thumb = thumbRef.current;
    if (!track || !thumb) return;
    const thumbBox = thumbGeometry(top, geometry.current.viewportHeight, geometry.current.contentHeight);
    track.style.display = thumbBox ? "block" : "none";
    if (thumbBox) {
      thumb.style.height = `${thumbBox.size}px`;
      thumb.style.transform = `translateY(${thumbBox.offset}px)`;
    }
  }, []);

  /** 统一的滚动入口：滚轮、滚动条、键盘和外部调用都走这里（ADR-0008）。 */
  const applyScroll = useCallback(
    (nextTop: number, options: { sync: boolean } = { sync: true }) => {
      const { viewportHeight: height, contentHeight: total } = geometry.current;
      const top = Math.min(Math.max(0, total - height), Math.max(0, nextTop));
      const delta = top - scrollTopRef.current;
      if (delta === 0) return;
      scrollTopRef.current = top;
      rows.noteDirection(delta);
      scrollDirection.current = delta > 0 ? 1 : -1;
      paintScrollPosition();
      const bigJump = Math.abs(delta) > height * 2;
      const update = () => {
        setFirstRow(heightsRef.current.rowAt(top));
        if (heightsRef.current.wrap) setBottomRow(heightsRef.current.rowAt(top + height));
        setScrollIdle(false);
        if (bigJump) {
          setCompact(true);
          setRetained(renderedRange.current);
        }
      };
      // 同步提交新的行窗口：这一帧画出来的位置上已经有行，不会先露白再补。
      if (options.sync) flushSync(update);
      else update();
      if (bigJump) {
        scheduleJumpRestore();
      }
      // 滚动期间不请求高亮，停下 150ms 后再补齐，避免着色与滚动抢主线程。
      if (idleTimer.current != null) window.clearTimeout(idleTimer.current);
      idleTimer.current = window.setTimeout(() => {
        idleTimer.current = null;
        setScrollIdle(true);
      }, SCROLL_IDLE_MS);
    },
    [paintScrollPosition, rows],
  );

  // 先按原逻辑位置渲染并量高，再夹紧；避免新宽度下的估算总高提前弄丢底部锚点。
  useLayoutEffect(() => {
    const previous = previousHeights.current;
    const anchor = previous.rowAtFraction(scrollTopRef.current);
    const reset = previous !== heights;
    previousHeights.current = heights;
    let changed = false;
    if (heights.wrap && contentRef.current) {
      for (const child of contentRef.current.children) {
        const element = child as HTMLElement;
        const row = element.dataset.row;
        if (row !== undefined && heights.measure(Number(row), element.offsetHeight)) changed = true;
      }
    }
    const total = heights.total();
    geometry.current = { viewportHeight, contentHeight: total };
    const top = reset || changed ? heights.yAt(anchor) : scrollTopRef.current;
    scrollTopRef.current = Math.min(Math.max(0, total - viewportHeight), Math.max(0, top));
    setFirstRow(heights.rowAt(scrollTopRef.current));
    if (heights.wrap) setBottomRow(heights.rowAt(scrollTopRef.current + viewportHeight));
    paintScrollPosition();
    if (changed) setLayoutVersion((value) => value + 1);
  });

  // 滚轮与触控板（含 macOS 惯性滚动）。必须是非被动监听，才能阻止浏览器自己滚动。
  useEffect(() => {
    const element = viewportRef.current;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      // 捏合缩放手势以 ctrl+wheel 的形式到达，交给系统处理。
      if (event.ctrlKey) return;
      event.preventDefault();
      const unit =
        event.deltaMode === WheelEvent.DOM_DELTA_LINE
          ? ROW_HEIGHT
          : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
            ? geometry.current.viewportHeight
            : 1;
      if (event.deltaY !== 0) applyScroll(scrollTopRef.current + event.deltaY * unit);
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, [applyScroll]);

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      const top = scrollTopRef.current;
      const page = Math.max(ROW_HEIGHT, geometry.current.viewportHeight - ROW_HEIGHT);
      const targets: Record<string, number> = {
        ArrowDown: top + ARROW_STEP_ROWS * ROW_HEIGHT,
        ArrowUp: top - ARROW_STEP_ROWS * ROW_HEIGHT,
        PageDown: top + page,
        PageUp: top - page,
        Home: 0,
        End: geometry.current.contentHeight,
      };
      const next = targets[event.key];
      if (next === undefined) return;
      event.preventDefault();
      applyScroll(next);
    },
    [applyScroll],
  );

  // 视口本身不允许发生原生滚动（例如聚焦或选中文字时浏览器自动滚到可见处），出现就复位。
  const onNativeScroll = useCallback((event: React.UIEvent<HTMLDivElement>) => {
    const element = event.currentTarget;
    if (element.scrollTop !== 0) element.scrollTop = 0;
    if (element.scrollLeft !== 0) element.scrollLeft = 0;
  }, []);

  const revealHunk = useCallback((top: number) => {
    // Tab 可进入预渲染区的按钮，滚动仍须经过自绘入口，不能依赖浏览器自动滚动。
    const current = scrollTopRef.current;
    if (top < current) applyScroll(top);
    else if (top + ROW_HEIGHT > current + geometry.current.viewportHeight) {
      applyScroll(top + ROW_HEIGHT - geometry.current.viewportHeight);
    }
  }, [applyScroll]);

  const onTrackPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const track = trackRef.current;
      const thumb = thumbRef.current;
      if (!track || !thumb || event.button !== 0) return;
      event.preventDefault();
      const { viewportHeight: height, contentHeight: total } = geometry.current;
      const thumbBox = thumbGeometry(scrollTopRef.current, height, total);
      if (!thumbBox) return;
      const trackTop = track.getBoundingClientRect().top;
      const pointerY = event.clientY - trackTop;
      if (event.target !== thumb) {
        // 点击滚动条空白处：向点击的方向翻一页。
        const page = Math.max(ROW_HEIGHT, height - ROW_HEIGHT);
        applyScroll(scrollTopRef.current + (pointerY < thumbBox.offset ? -page : page));
        return;
      }
      // 拖动滑块：按滑块可移动范围与内容可滚动范围的比例换算。
      const startY = event.clientY;
      const startTop = scrollTopRef.current;
      const ratio = (total - height) / Math.max(1, height - thumbBox.size);
      thumb.setPointerCapture(event.pointerId);
      const onMove = (move: PointerEvent) => applyScroll(startTop + (move.clientY - startY) * ratio);
      const onUp = () => {
        thumb.removeEventListener("pointermove", onMove);
        thumb.removeEventListener("pointerup", onUp);
        thumb.removeEventListener("pointercancel", onUp);
      };
      thumb.addEventListener("pointermove", onMove);
      thumb.addEventListener("pointerup", onUp);
      thumb.addEventListener("pointercancel", onUp);
    },
    [applyScroll],
  );

  const viewportRows = Math.max(1, Math.ceil(Math.max(viewportHeight, ROW_HEIGHT) / ROW_HEIGHT));
  const ahead = compact ? 0 : viewportRows * OVERSCAN_AHEAD_SCREENS;
  const behind = compact ? 0 : viewportRows * OVERSCAN_BEHIND_SCREENS;
  const visibleFrom = changedHeights ? heights.rowAt(renderTop) : Math.max(0, firstRow);
  // 视口底部可能没有对齐整行，必须额外多备 1 行防抖，
  // 保证 compact 期间即使停在半行也不会露出底部的“加载中的行”（Issue 09 审查意见 2）。
  const visibleTo = Math.min(rowCount, softWrap
    ? heights.rowAt(renderTop + viewportHeight) + 1
    : visibleFrom + viewportRows + 1);
  const scrollingUp = scrollDirection.current < 0;
  const renderFrom = Math.max(0, visibleFrom - (scrollingUp ? ahead : behind));
  const renderTo = Math.min(rowCount, visibleTo + (scrollingUp ? behind : ahead));

  useLayoutEffect(() => {
    // 先量出真实视口，避免 0 高度小请求占住首屏 slice，随后又为完整视口补一次。
    if (viewportHeight <= 0) return;
    void version;
    rows.ensure(visibleFrom, visibleTo, PREFETCH_SCREENS);
    if (version === 0) {
      rows.logWindow?.();
    }
    onSample?.({ from: visibleFrom, to: visibleTo, uncovered: rows.uncovered(visibleFrom, visibleTo) });
  }, [onSample, rows, scrollIdle, version, visibleFrom, visibleTo, viewportHeight]);

  useEffect(() => {
    if (!scrollIdle || !highlightSnapshot.supported || highlightSnapshot.pending > 0
      || !rows.isCovered(visibleFrom, visibleTo)) return;
    // 高亮等可见行文本就绪之后再请求，且滚动期间不请求：两者共用同一条 IPC 与 JS 主线程，
    // 首屏和滚动时优先保证文本（ADR-0006：先纯文本、再异步着色）。滚动停下后再补齐颜色。
    // 请求结束的快照会再次触发 effect，继续补齐被单请求背压跳过的块和旧版本。
    // 只请求实际呈现的行，不把两版本或离散 hunk 之间的空洞扩成连续区间。
    const lines = visibleHighlightLines((index) => rows.getRow(index), visibleFrom, visibleTo, view);
    for (const fileVersion of ["new", "old"] as const) {
      for (const line of lines[fileVersion]) highlights.ensure(fileVersion, line, line);
    }
  }, [highlights, highlightSnapshot, rows, scrollIdle, version, view, visibleFrom, visibleTo]);

  useImperativeHandle(
    handleRef,
    () => ({
      getFileAnchor() {
        return unifiedFileAnchor((index) => rows.getRow(index), visibleFrom, visibleTo);
      },
      scrollToRow(index: number) {
        applyScroll(heightsRef.current.top(Math.max(0, index)));
      },
      async scrollToLine(line: number, targetVersion: FileVersion) {
        if (!entry || !worktreeId) return;
        const index = await ipc.rowIndexForLine({
          worktreeId,
          path: entry.path,
          changes: entry.changes,
          view,
          line,
          version: targetVersion,
        });
        applyScroll(heightsRef.current.top(Math.max(0, index)));
      },
      getScrollTop() {
        return scrollTopRef.current;
      },
      setScrollTop(top: number) {
        applyScroll(top);
      },
      getViewportHeight() {
        return geometry.current.viewportHeight;
      },
      getContentHeight() {
        return geometry.current.contentHeight;
      },
      getVisibleRange() {
        return { from: visibleFrom, to: visibleTo };
      },
      isCovered(from: number, to: number) {
        return rows.isCovered(from, to);
      },
      isCompact() {
        return compact;
      },
    }),
    [applyScroll, compact, entry, rows, view, visibleFrom, visibleTo, worktreeId],
  );

  useLayoutEffect(() => {
    if (!compact) renderedRange.current = { from: renderFrom, to: renderTo };
  });

  const kept = compact ? retained : null;
  const indices = useMemo(() => {
    const list: number[] = [];
    if (kept) for (let index = kept.from; index < Math.min(kept.to, renderFrom); index += 1) list.push(index);
    for (let index = renderFrom; index < renderTo; index += 1) list.push(index);
    if (kept) for (let index = Math.max(kept.from, renderTo); index < kept.to; index += 1) list.push(index);
    return list;
  }, [kept, renderFrom, renderTo]);

  return (
    <div
      ref={viewportRef}
      className={`diff-scroll relative h-full overflow-hidden bg-[var(--diff-bg)] font-mono outline-none${softWrap ? " diff-wrap" : ""}`}
      style={{ fontSize, fontFamily: codeFontStack(fontFamily ?? null), lineHeight: `${ROW_HEIGHT}px` }}
      onKeyDown={onKeyDown}
      onScroll={onNativeScroll}
      tabIndex={0}
    >
      <div ref={contentRef} className={`diff-canvas diff-canvas-${view}`} style={{ height: contentHeight }}>
        {indices.map((index) => {
          const row = rows.getRow(index);
          const plain = compact && index >= renderFrom && index < renderTo;
          // 数据没到的行不渲染节点，直接露出底图里“加载中的行”。
          return row ? (
            <RowView
              key={index}
              locale={locale}
              index={index}
              row={row}
              top={heights.top(index)}
              wrap={heights.wrap}
              view={view}
              highlights={highlights}
              highlightVersion={plain ? 0 : highlightSnapshot.version}
              plainTextOnly={plain}
              onHunkAction={onHunkAction}
              onHunkDiscard={onHunkDiscard}
              onSegmentAction={props.onSegmentAction}
              onSegmentDiscard={props.onSegmentDiscard}
              segmentActionsEnabled={segmentActionsEnabled}
              onHunkFocus={revealHunk}
              hunkDisabled={!entry || !worktreeId || isWriting || !!rowSnapshot.error || !row.hunkAction
                || !matchesHunkAction(row.hunkAction, worktreeId, entry)}
              segmentDisabled={!entry || !worktreeId || segmentActionDisabled(row.segmentAction, worktreeId, entry,
                !!isWriting || !!rowSnapshot.error || !segmentActionsEnabled)}
            />
          ) : null;
        })}
      </div>
      {rowSnapshot.error ? (
        <div role="status" className="absolute inset-x-0 top-0 bg-background/95 px-3 py-2 font-sans text-xs text-destructive">
          {formatAppError(rowSnapshot.error)}
        </div>
      ) : null}
      <div ref={trackRef} className="diff-scrollbar" onPointerDown={onTrackPointerDown}>
        <div ref={thumbRef} className="diff-scrollbar-thumb" />
      </div>
    </div>
  );
}

/** 滚动条滑块的高度和位置；内容不超过视口时返回 null（不显示滚动条）。 */
function thumbGeometry(
  top: number,
  viewportHeight: number,
  contentHeight: number,
): { size: number; offset: number } | null {
  if (contentHeight <= viewportHeight || viewportHeight <= 0) return null;
  const size = Math.max(SCROLLBAR_MIN_THUMB, (viewportHeight / contentHeight) * viewportHeight);
  const maxTop = contentHeight - viewportHeight;
  return { size, offset: (top / maxTop) * (viewportHeight - size) };
}

const RowView = memo(function RowView({
  index,
  row,
  top,
  wrap,
  view,
  highlights,
  plainTextOnly,
  onHunkAction,
  onHunkDiscard,
  onSegmentAction,
  onSegmentDiscard,
  segmentActionsEnabled,
  onHunkFocus,
  hunkDisabled,
  segmentDisabled,
}: {
  /** Invalidate memoized labels while retaining cached rows and scroll state. */
  locale: Locale;
  index: number;
  row: RenderRow;
  top: number;
  /** 软换行：行高随内容，交给视图量（不能用 size containment）。 */
  wrap: boolean;
  view: ViewMode;
  highlights: DiffHighlights;
  /** 缓存对象保持不变时，让 memo 行在 token 到达后仍能补绘。 */
  highlightVersion: number;
  plainTextOnly: boolean;
  onHunkAction?: (action: HunkAction) => void;
  onHunkDiscard?: (action: HunkAction, section: string) => void;
  onSegmentAction?: (action: SegmentAction) => void;
  onSegmentDiscard?: (action: SegmentAction) => void;
  segmentActionsEnabled: boolean;
  onHunkFocus: (top: number) => void;
  hunkDisabled: boolean;
  segmentDisabled: boolean;
}) {
  const style: React.CSSProperties = { top, contain: wrap ? "layout paint style" : "strict" };

  const getTokens = (version: FileVersion, line?: number | null) =>
    !plainTextOnly && line != null ? highlights.get(version, line) : undefined;

  if (row.kind === "hunk") {
    const action = row.hunkAction;
    const label = action?.changes === "staged" ? messages.changes.unstageHunk : messages.changes.stageHunk;
    // 按段操作时 hunk 标题行只显示 @@ 信息（设计第 2 节）；
    // “同时忽略空行”一档下段按钮不可用，标题行保留灰掉的整块入口并说明原因（Q140）。
    if (segmentActionsEnabled) {
      return (
        <div className="row row-hunk" style={style} data-row={index} title={row.section ?? ""}>
          <span className="hunk-section">{row.section ?? ""}</span>
        </div>
      );
    }
    return (
      <div className="row row-hunk" style={style} data-row={index} title={row.section ?? ""}>
        <span className="hunk-section">{row.section ?? ""}</span>
        {action && onHunkAction ? (
          <button
            type="button"
            className="hunk-action"
            disabled={hunkDisabled}
            aria-label={`${label}：${row.section ?? ""}`}
            title={messages.diff.ignoreWhitespaceReadonly}
            onFocus={() => onHunkFocus(top)}
            onClick={() => onHunkAction(action)}
          >
            <Codicon name={action.changes === "staged" ? "remove" : "add"} size={13} />
            {label}
          </button>
        ) : null}
        {action?.changes === "unstaged" && onHunkDiscard ? (
          <button type="button" className="hunk-action" disabled={hunkDisabled}
            aria-label={`${messages.changes.discardHunk}：${row.section ?? ""}`}
            onFocus={() => onHunkFocus(top)} onClick={() => onHunkDiscard(action, row.section ?? "")}>
            <Codicon name="discard" size={13} />
            {messages.changes.discardHunk}
          </button>
        ) : null}
      </div>
    );
  }

  if (view === "unified") {
    const version: FileVersion = row.lineKind === "addition" ? "new" : "old";
    const line = row.lineKind === "addition" ? row.newLine : row.oldLine;
    const tone = toneOf(row.lineKind);
    const segmentAction = row.segmentAction;
    const segmentLabel =
      segmentAction?.changes === "staged" ? messages.changes.unstageSegment : messages.changes.stageSegment;
    return (
      <div className="row row-unified" style={style} data-row={index}>
        <span className={gutterClass(tone)}>
          {segmentAction && onSegmentAction ? (
            <button
              type="button"
              className="segment-action"
              title={segmentLabel}
              aria-label={messages.changes.segmentAt(segmentLabel, row.oldLine ?? row.newLine ?? 0)}
              disabled={segmentDisabled}
              onFocus={() => onHunkFocus(top)}
              onClick={() => onSegmentAction(segmentAction)}
            >
              <Codicon name={segmentAction.changes === "staged" ? "chevron-right" : "chevron-left"} size={11} />
            </button>
          ) : null}
          {row.oldLine ?? ""}
        </span>
        <span className={gutterClass(tone)}>
          {segmentAction && onSegmentDiscard && segmentAction.changes === "unstaged" ? (
            <button
              type="button"
              className="segment-action"
              title={messages.changes.discardSegment}
              aria-label={messages.changes.segmentAt(messages.changes.discardSegment, row.oldLine ?? row.newLine ?? 0)}
              disabled={segmentDisabled}
              onFocus={() => onHunkFocus(top)}
              onClick={() => onSegmentDiscard(segmentAction)}
            >
              <Codicon name="discard" size={11} />
            </button>
          ) : null}
          {row.newLine ?? ""}
        </span>
        <Cell
          text={row.text ?? ""}
          tone={tone}
          kind={row.lineKind ?? "context"}
          marker={markerFor(row.lineKind)}
          tokens={getTokens(version, line)}
          highlights={highlights}
        />
      </div>
    );
  }

  const leftTone = toneOf(row.left?.kind);
  const rightTone = toneOf(row.right?.kind);
  return (
    <div className="row row-split" style={style} data-row={index}>
      <span className={row.left ? gutterClass(leftTone) : "gutter gutter-none"}>{row.left?.line ?? ""}</span>
      <SplitCell
        cell={row.left}
        tone={leftTone}
        highlights={highlights}
        tokens={getTokens("old", row.left?.line)}
      />
      <span className={`mid${rightTone ? ` mid-${rightTone}` : leftTone ? ` mid-${leftTone}` : ""}`} />
      <span className={row.right ? gutterClass(rightTone) : "gutter gutter-none"}>{row.right?.line ?? ""}</span>
      <SplitCell
        cell={row.right}
        tone={rightTone}
        highlights={highlights}
        tokens={getTokens("new", row.right?.line)}
      />
    </div>
  );
});

/** 行的着色：修改块蓝、纯新增绿、纯删除灰（JetBrains diff 配色，Q107）。 */
type LineTone = "mod" | "ins" | "del" | null;

function toneOf(kind: RenderRow["lineKind"] | RowCell["kind"] | undefined): LineTone {
  // VS Code 的 diff 只有“新增 / 删除”两种底色：修改行在并排视图里也是左删右增。
  if (kind === "addition") return "ins";
  if (kind === "deletion") return "del";
  return null;
}

function gutterClass(tone: LineTone): string {
  return tone ? `gutter gutter-${tone}` : "gutter";
}

function markerFor(kind?: RenderRow["lineKind"] | RowCell["kind"]): string {
  if (kind === "addition") return "+";
  if (kind === "deletion") return "-";
  return " ";
}

/**
 * 只有 token 拼回来的文本与显示文本完全一致时才用高亮结果。
 * 高亮来自另一个版本时（例如未刷新前外部等长改写），宁可显示纯文本，
 * 也不能让渲染出的文字与 Git hunk / 写指纹不一致（Q88、T05）。
 */
function tokensMatchText(tokens: ReturnType<DiffHighlights["get"]>, text: string): boolean {
  if (!tokens) return false;
  let joined = "";
  for (const token of tokens) joined += token.text;
  return joined === text;
}

function Cell({
  text,
  tone,
  kind,
  marker,
  tokens,
  highlights,
}: {
  text: string;
  tone: LineTone;
  kind: RowCell["kind"];
  marker: string;
  tokens: ReturnType<DiffHighlights["get"]>;
  highlights: DiffHighlights;
}) {
  return (
    <span className={`cell cell-${kind}${tone ? ` cell-${tone}` : ""}`}>
      <span className="marker">{marker}</span>
      {tokensMatchText(tokens, text)
        ? tokens!.map((token, index) => (
            <span key={index} className={highlights.categoryClass(token.category)}>
              {token.text}
            </span>
          ))
        : text}
    </span>
  );
}

function SplitCell({
  cell,
  tone,
  highlights,
  tokens,
}: {
  cell: RowCell | null;
  tone: LineTone;
  highlights: DiffHighlights;
  tokens: ReturnType<DiffHighlights["get"]>;
}) {
  if (!cell) return <span className="cell empty" />;
  return (
    <span className={`cell cell-${cell.kind}${tone ? ` cell-${tone}` : ""}`}>
      <span className="marker">{markerFor(cell.kind)}</span>
      {tokensMatchText(tokens, cell.text)
        ? tokens!.map((token, index) => (
            <span key={index} className={highlights.categoryClass(token.category)}>
              {token.text}
            </span>
          ))
        : cell.text}
    </span>
  );
}
