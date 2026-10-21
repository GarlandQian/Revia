/**
 * 只读 diff 视图：自绘、窗口化、只渲染可见区域（Q32）。
 *
 * 数据来自 `RowWindowStore`（按可见范围分块取行）与 `HighlightStore`（按行范围取类别）。
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
import type { RenderRow } from "@/types/bindings/RenderRow";
import type { RowCell } from "@/types/bindings/RowCell";
import type { ViewMode } from "@/types/bindings/ViewMode";
import { ipc } from "@/lib/ipc";
import { logToBackend } from "./log";
import type { HighlightStore } from "./highlightStore";
import type { RowWindowStore } from "./rowWindowStore";
import { visibleHighlightLines } from "./visibleHighlightLines";

export const ROW_HEIGHT = 18;
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

interface DiffViewProps {
  worktreeId: string;
  entry: ChangeEntry;
  view: ViewMode;
  fontSize: number;
  rows: RowWindowStore;
  highlights: HighlightStore;
  handleRef?: React.Ref<DiffViewHandle>;
  /** 每帧采样：用于测量「可见区域里还没有数据的行数」。 */
  onSample?: (sample: { from: number; to: number; uncovered: number }) => void;
}

export function DiffView(props: DiffViewProps) {
  const { worktreeId, entry, view, fontSize, rows, highlights, handleRef, onSample } = props;
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const contentRef = useRef<HTMLDivElement | null>(null);
  const trackRef = useRef<HTMLDivElement | null>(null);
  const thumbRef = useRef<HTMLDivElement | null>(null);
  // 滚动位置只放在 ref 里：每帧直接写 DOM，不经过 React 状态。
  const scrollTopRef = useRef(0);
  // 只记录顶部是第几行：同一行内的像素级滚动不触发重新渲染。
  const [firstRow, setFirstRow] = useState(0);
  // 最近一次滚动的方向（1 向下，-1 向上），决定预渲染往哪边多放。
  const scrollDirection = useRef(1);
  const [scrollIdle, setScrollIdle] = useState(true);
  const idleTimer = useRef<number | null>(null);
  // 大幅跳转的那一帧只渲染可见行纯文本，保证 1 帧内画出文字（Q52）；语法颜色与预渲染余量在约 2 帧内恢复。
  const [compact, setCompact] = useState(false);
  const jumpRestoreRaf = useRef<number | null>(null);
  const jumpRestoreTimer = useRef<number | null>(null);
  const [viewportHeight, setViewportHeight] = useState(0);
  const version = useRowVersion(rows);
  const highlightSnapshot = useSyncExternalStore(highlights.subscribe, highlights.getSnapshot);

  const rowCount = rows.rowCountValue;
  const contentHeight = rowCount * ROW_HEIGHT;
  const geometry = useRef({ viewportHeight: 0, contentHeight: 0 });
  geometry.current = { viewportHeight, contentHeight };

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
      } else {
        jumpRestoreRaf.current = requestAnimationFrame(onFrame);
      }
    };
    jumpRestoreRaf.current = requestAnimationFrame(onFrame);
    jumpRestoreTimer.current = window.setTimeout(() => {
      cancelJumpRestore();
      setCompact(false);
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
    setScrollIdle(true);
  }, [cancelJumpRestore, entry.changes, entry.path, view]);

  // 组件卸载时清理所有定时器与 rAF
  useEffect(() => {
    const element = viewportRef.current;
    logToBackend(`DiffView 已挂载：${entry.path}/${view} clientHeight=${element?.clientHeight}`);
    return () => {
      logToBackend(`DiffView 卸载：${entry.path}/${view}`);
      cancelJumpRestore();
      if (idleTimer.current != null) {
        window.clearTimeout(idleTimer.current);
        idleTimer.current = null;
      }
    };
  }, [cancelJumpRestore, entry.path, view]);

  useLayoutEffect(() => {
    const element = viewportRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setViewportHeight(element.clientHeight));
    observer.observe(element);
    setViewportHeight(element.clientHeight);
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
        setFirstRow(Math.floor(top / ROW_HEIGHT));
        setScrollIdle(false);
        if (bigJump) setCompact(true);
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

  // 视口或内容高度变化（窗口缩放、切换视图、行数校正）时，把滚动位置收回有效范围并重画。
  useLayoutEffect(() => {
    const maxTop = Math.max(0, contentHeight - viewportHeight);
    if (scrollTopRef.current > maxTop) {
      scrollTopRef.current = maxTop;
      setFirstRow(Math.floor(maxTop / ROW_HEIGHT));
    }
    paintScrollPosition();
  }, [contentHeight, viewportHeight, paintScrollPosition]);

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
  const visibleFrom = Math.max(0, firstRow);
  // 视口底部可能没有对齐整行，必须额外多备 1 行防抖，
  // 保证 compact 期间即使停在半行也不会露出底部的“加载中的行”（Issue 09 审查意见 2）。
  const visibleTo = Math.min(rowCount, visibleFrom + viewportRows + 1);
  const scrollingUp = scrollDirection.current < 0;
  const renderFrom = Math.max(0, visibleFrom - (scrollingUp ? ahead : behind));
  const renderTo = Math.min(rowCount, visibleTo + (scrollingUp ? behind : ahead));

  useEffect(() => {
    void version;
    rows.ensure(visibleFrom, visibleTo, PREFETCH_SCREENS);
    if (version === 0) {
      rows.logWindow?.();
    }
    onSample?.({ from: visibleFrom, to: visibleTo, uncovered: rows.uncovered(visibleFrom, visibleTo) });
  }, [onSample, rows, scrollIdle, version, visibleFrom, visibleTo]);

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
      scrollToRow(index: number) {
        applyScroll(Math.max(0, index) * ROW_HEIGHT);
      },
      async scrollToLine(line: number, targetVersion: FileVersion) {
        const index = await ipc.rowIndexForLine({
          worktreeId,
          path: entry.path,
          changes: entry.changes,
          view,
          line,
          version: targetVersion,
        });
        applyScroll(Math.max(0, index) * ROW_HEIGHT);
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
    [applyScroll, compact, entry.changes, entry.path, rows, view, visibleFrom, visibleTo, worktreeId],
  );

  const indices = useMemo(() => {
    const list: number[] = [];
    for (let index = renderFrom; index < renderTo; index += 1) list.push(index);
    return list;
  }, [renderFrom, renderTo]);

  return (
    <div
      ref={viewportRef}
      className="diff-scroll relative h-full overflow-hidden bg-[var(--diff-bg)] font-mono outline-none"
      style={{ fontSize, lineHeight: `${ROW_HEIGHT}px` }}
      onKeyDown={onKeyDown}
      onScroll={onNativeScroll}
      tabIndex={0}
    >
      <div ref={contentRef} className={`diff-canvas diff-canvas-${view}`} style={{ height: contentHeight }}>
        {indices.map((index) => {
          const row = rows.getRow(index);
          // 数据没到的行不渲染节点，直接露出底图里“加载中的行”。
          return row ? (
            <RowView
              key={index}
              row={row}
              top={index * ROW_HEIGHT}
              view={view}
              highlights={highlights}
              highlightVersion={compact ? 0 : highlightSnapshot.version}
              plainTextOnly={compact}
            />
          ) : null;
        })}
      </div>
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

/** 订阅行窗口版本：块到达时重新渲染。 */
function useRowVersion(rows: RowWindowStore): number {
  const [version, setVersion] = useState(0);
  useEffect(() => rows.subscribe(() => setVersion(rows.getSnapshot().version)), [rows]);
  return version;
}

const RowView = memo(function RowView({
  row,
  top,
  view,
  highlights,
  plainTextOnly,
}: {
  row: RenderRow;
  top: number;
  view: ViewMode;
  highlights: HighlightStore;
  /** 缓存对象保持不变时，让 memo 行在 token 到达后仍能补绘。 */
  highlightVersion: number;
  plainTextOnly: boolean;
}) {
  const style: React.CSSProperties = { top, contain: "strict" };

  const getTokens = (version: FileVersion, line?: number | null) =>
    !plainTextOnly && line != null ? highlights.get(version, line) : undefined;

  if (row.kind === "hunk") {
    return (
      <div className="row row-hunk" style={style} title={row.section ?? ""}>
        {row.section ?? ""}
      </div>
    );
  }

  if (view === "unified") {
    const version: FileVersion = row.lineKind === "addition" ? "new" : "old";
    const line = row.lineKind === "addition" ? row.newLine : row.oldLine;
    return (
      <div className="row row-unified" style={style}>
        <span className="gutter">{row.oldLine ?? ""}</span>
        <span className="gutter">{row.newLine ?? ""}</span>
        <Cell
          text={row.text ?? ""}
          kind={row.lineKind ?? "context"}
          marker={markerFor(row.lineKind)}
          tokens={getTokens(version, line)}
          highlights={highlights}
        />
      </div>
    );
  }

  return (
    <div className="row row-split" style={style}>
      <span className="gutter">{row.left?.line ?? ""}</span>
      <SplitCell
        cell={row.left}
        highlights={highlights}
        tokens={getTokens("old", row.left?.line)}
      />
      <span className="gutter">{row.right?.line ?? ""}</span>
      <SplitCell
        cell={row.right}
        highlights={highlights}
        tokens={getTokens("new", row.right?.line)}
      />
    </div>
  );
});

function markerFor(kind?: RenderRow["lineKind"] | RowCell["kind"]): string {
  if (kind === "addition") return "+";
  if (kind === "deletion") return "-";
  return " ";
}

function Cell({
  text,
  kind,
  marker,
  tokens,
  highlights,
}: {
  text: string;
  kind: RowCell["kind"];
  marker: string;
  tokens: ReturnType<HighlightStore["get"]>;
  highlights: HighlightStore;
}) {
  return (
    <span className={`cell cell-${kind}`}>
      <span className="marker">{marker}</span>
      {tokens
        ? tokens.map((token, index) => (
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
  highlights,
  tokens,
}: {
  cell: RowCell | null;
  highlights: HighlightStore;
  tokens: ReturnType<HighlightStore["get"]>;
}) {
  if (!cell) return <span className="cell empty" />;
  return (
    <span className={`cell cell-${cell.kind}`}>
      <span className="marker">{markerFor(cell.kind)}</span>
      {tokens
        ? tokens.map((token, index) => (
            <span key={index} className={highlights.categoryClass(token.category)}>
              {token.text}
            </span>
          ))
        : cell.text}
    </span>
  );
}
