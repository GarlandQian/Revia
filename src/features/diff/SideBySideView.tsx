/**
 * 左右对照视图（返修 A1/Q138，设计见 docs/milestones/m4/side-by-side-design.md）。
 *
 * 两栏各自连续显示整个文件，不插空行；中间窄栏带两列行号、弧形色带和每段的
 * » / « 按钮。段、折叠区与段操作目标都来自 Rust（同一次 Git diff 的解析结果，
 * ADR-0004）；前端不计算差异，也不拼补丁。
 *
 * 滚动由本组件接管（ADR-0008）：两栏各自维护滚动位置，主动栏变化时按
 * `mapLine` 把跟随栏映射到对应位置。
 */

import { memo, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { bandVisible, drawBand, horizontalGradient, prepareCanvas, type BandEnds } from "./bands";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangeSegment } from "@/types/bindings/ChangeSegment";
import type { SideBySideLayout } from "@/types/bindings/SideBySideLayout";
import type { SegmentAction } from "@/types/bindings/SegmentAction";
import type { FoldRange } from "@/types/bindings/FoldRange";
import type { IgnoreWhitespace } from "@/types/bindings/IgnoreWhitespace";
import { ipc } from "@/lib/ipc";
import { messages } from "@/lib/i18n/zh-CN";
import { codeFontStack } from "@/lib/theme";
import type { HighlightStore } from "./highlightStore";
import { clampLine } from "./syncScroll";
import { buildRows, type RowIndex, type Side } from "./rowIndex";
import { RowHeights } from "./rowHeights";
import { SideLineWindow, lineRangesForRows, type SideLineRange } from "./sideLineWindow";
import { filePositionAtY, yAtFilePosition, syncPositionError, syncTargetTop } from "./sideGeometry";
import { useFontRevision } from "./useFontRevision";
import { ROW_HEIGHT } from "./DiffView";
import { sideFileAnchor, type FileAnchor } from "./fileAnchor";

const LINE_HEIGHT = ROW_HEIGHT;
/** 视口外预取的屏数。 */
const OVERSCAN_SCREENS = 2;
const SCROLLBAR_MIN_THUMB = 24;
const GUTTER_BAND = 26;
const GUTTER_NUMBERS = 34;
const EMPTY_SEGMENTS: ChangeSegment[] = [];
const EMPTY_FOLDS: FoldRange[] = [];
const EMPTY_WORDS: Array<[number, number]> = [];

export type { Side } from "./rowIndex";

/** 差异高亮方式（Q139）。 */
export type DiffHighlight = "word" | "line" | "char" | "none";

export interface DiffOptions {
  ignoreWhitespace: IgnoreWhitespace;
  highlight: DiffHighlight;
  foldUnchanged: boolean;
  syncScroll: boolean;
  showWhitespace: boolean;
  softWrap: boolean;
  contextLines: number;
}

export interface SideBySideHandle {
  getFileAnchor(): FileAnchor | null;
  restoreFileAnchor(anchor: FileAnchor): boolean;
  jumpToSegment(index: number): void;
  segmentCount(): number;
  getScrollTop(): number;
  setScrollTop(top: number): void;
  getViewportHeight(): number;
  getContentHeight(): number;
  getVisibleRange(side?: Side): { from: number; to: number };
  isCovered(from: number, to: number, side?: Side): boolean;
  isCompact(): boolean;
  /** 两栏中线的对应误差（行）：测量同步滚动用。 */
  syncError(): number;
}

interface SideBySideViewProps {
  worktreeId: string;
  entry: ChangeEntry;
  fontSize: number;
  fontFamily?: string | null;
  options: DiffOptions;
  highlights: HighlightStore;
  isWriting?: boolean;
  onStageSegment?: (action: SegmentAction) => void;
  onUnstageSegment?: (action: SegmentAction) => void;
  onDiscardSegment?: (action: SegmentAction) => void;
  onLayout?: (layout: SideBySideLayout | null) => void;
  onSample?: (sample: { from: number; to: number; uncovered: number }) => void;
  handleRef?: React.Ref<SideBySideHandle>;
}

export function SideBySideView(props: SideBySideViewProps) {
  const { worktreeId, entry, fontSize, fontFamily, options, highlights, isWriting, onLayout, onSample, handleRef } = props;
  const { ignoreWhitespace, highlight, foldUnchanged, syncScroll, showWhitespace, softWrap } = options;
  const [layout, setLayout] = useState<SideBySideLayout | null>(null);
  const [layoutKey, setLayoutKey] = useState<string | null>(null);
  const [layoutError, setLayoutError] = useState<string | null>(null);
  const [cacheVersion, setCacheVersion] = useState(0);
  const [expandedFolds, setExpandedFolds] = useState<Set<number>>(new Set());
  const [columnWidths, setColumnWidths] = useState({ left: 0, right: 0 });
  const [heightsVersion, setHeightsVersion] = useState(0);
  /** 悬停在某一块的按钮上：两边与色带一起加亮（设计第 2 节）。 */
  const [hovered, setHovered] = useState<number | null>(null);
  const cache = useRef(new SideLineWindow(""));
  const leftRef = useRef<SideColumnHandle | null>(null);
  const rightRef = useRef<SideColumnHandle | null>(null);
  const leftGutterRef = useRef<GutterHandle | null>(null);
  const rightGutterRef = useRef<GutterHandle | null>(null);
  const gutterRef = useRef<HTMLCanvasElement | null>(null);
  const gutterBoxRef = useRef<HTMLDivElement | null>(null);
  const scrollTops = useRef({ left: 0, right: 0 });
  const activeSide = useRef<Side>("left");
  const jumpTarget = useRef<number | null>(null);
  const lineTarget = useRef<FileAnchor | null>(null);
  const gutterWidth = GUTTER_NUMBERS * 2 + GUTTER_BAND;

  const cacheKey = `${worktreeId}/${entry.changes}/${entry.path}/${ignoreWhitespace}/${options.contextLines}/${highlight}/${entry.snapshotRevision}/${entry.fingerprint}`;
  const currentLayout = layoutKey === cacheKey && layout?.worktreeId === worktreeId
    && layout.path === entry.path && layout.changes === entry.changes
    && layout.fingerprint === entry.fingerprint && layout.snapshotRevision === entry.snapshotRevision
    && layout.ignoreWhitespace === ignoreWhitespace;

  // 布局：段、折叠区、两侧行数与段操作目标都来自 Rust。
  useEffect(() => {
    let cancelled = false;
    setLayoutError(null);
    void ipc
      .sideBySideLayout({
        worktreeId,
        path: entry.path,
        changes: entry.changes,
        ignoreWhitespace,
        contextLines: options.contextLines,
        highlight,
      })
      .then((next) => {
        if (cancelled) return;
        setLayout(next);
        setLayoutKey(cacheKey);
        onLayout?.(next);
        setExpandedFolds(new Set());
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setLayout(null);
        setLayoutKey(null);
        onLayout?.(null);
        setLayoutError(error instanceof Error ? error.message : String(error));
      });
    return () => {
      cancelled = true;
    };
  }, [worktreeId, entry.path, entry.changes, entry.snapshotRevision, ignoreWhitespace, options.contextLines, highlight, onLayout, cacheKey]);

  // 换文件、换档位或快照前进时清空行缓存。
  useEffect(() => {
    if (cache.current.key === cacheKey) return;
    jumpTarget.current = null;
    lineTarget.current = null;
    cache.current = new SideLineWindow(cacheKey);
    setCacheVersion((value) => value + 1);
  }, [cacheKey]);

  const segments = layout?.segments ?? EMPTY_SEGMENTS;
  const folds = layout?.folds ?? EMPTY_FOLDS;
  const leftRows = useMemo(
    () => buildRows(layout?.leftLineCount ?? 0, folds, "left", foldUnchanged ? expandedFolds : new Set(folds.map((_, index) => index))),
    [layout?.leftLineCount, folds, foldUnchanged, expandedFolds],
  );
  const rightRows = useMemo(
    () => buildRows(layout?.rightLineCount ?? 0, folds, "right", foldUnchanged ? expandedFolds : new Set(folds.map((_, index) => index))),
    [layout?.rightLineCount, folds, foldUnchanged, expandedFolds],
  );
  const fontRevision = useFontRevision(softWrap);
  const wrapFont = softWrap ? `${fontSize}/${fontFamily ?? ""}/${showWhitespace}/${fontRevision}` : "";
  const leftWidth = softWrap ? columnWidths.left : 0;
  const rightWidth = softWrap ? columnWidths.right : 0;
  const leftHeights = useMemo(() => {
    void [wrapFont, leftWidth, cacheKey];
    return new RowHeights(leftRows.rows.length, LINE_HEIGHT, softWrap);
  }, [leftRows, softWrap, wrapFont, leftWidth, cacheKey]);
  const rightHeights = useMemo(() => {
    void [wrapFont, rightWidth, cacheKey];
    return new RowHeights(rightRows.rows.length, LINE_HEIGHT, softWrap);
  }, [rightRows, softWrap, wrapFont, rightWidth, cacheKey]);
  const onColumnWidth = useCallback((side: Side, width: number) => {
    setColumnWidths((current) => current[side] === width ? current : { ...current, [side]: width });
  }, []);
  const onHeightsChanged = useCallback(() => setHeightsVersion((value) => value + 1), []);

  /** 先补两栏可见小片，再按原来的 400 行块补足预渲染窗口。 */
  const ensureLines = useCallback(
    (side: Side, visible: SideLineRange[], buffered: SideLineRange[], lineCount: number) => {
      const current = cache.current;
      if (current.key !== cacheKey) return;
      current.ensure(side, { visible, buffered, lineCount }, {
        load: (targetSide, from, to) => ipc.sideLines({
          worktreeId,
          path: entry.path,
          changes: entry.changes,
          ignoreWhitespace,
          contextLines: options.contextLines,
          highlight,
          version: targetSide === "left" ? "old" : "new",
          from,
          to,
        }),
        isCurrent: () => cache.current === current,
        publish: () => setCacheVersion((value) => value + 1),
      });
    },
    [cacheKey, worktreeId, entry.path, entry.changes, ignoreWhitespace, options.contextLines, highlight],
  );

  /** 按两栏当前的滚动位置画色带（跟随栏移动之后再画，避免慢一帧）。 */
  const redrawBands = useCallback(() => {
    const leftTop = leftRef.current?.getScrollTop() ?? scrollTops.current.left;
    const rightTop = rightRef.current?.getScrollTop() ?? scrollTops.current.right;
    drawBands(gutterRef.current, gutterBoxRef.current, { leftTop, rightTop, segments, leftRows, rightRows, leftHeights, rightHeights, hovered });
  }, [segments, leftRows, rightRows, leftHeights, rightHeights, hovered]);

  const alignFollower = useCallback((side: Side, top: number) => {
    if (!syncScroll || !layout) return;
    const own = side === "left" ? leftRef.current : rightRef.current;
    const follower = side === "left" ? rightRef.current : leftRef.current;
    if (!own || !follower) return;
    const source = {
      rows: side === "left" ? leftRows : rightRows,
      heights: side === "left" ? leftHeights : rightHeights,
      viewportHeight: own.getViewportHeight(),
    };
    const target = {
      rows: side === "left" ? rightRows : leftRows,
      heights: side === "left" ? rightHeights : leftHeights,
      viewportHeight: follower.getViewportHeight(),
    };
    follower.setScrollTop(syncTargetTop(side, top, source, target, segments), false);
  }, [syncScroll, layout, segments, leftRows, rightRows, leftHeights, rightHeights]);

  /** 主动栏滚动：同步跟随栏（设计 4.2）。 */
  const handleScroll = useCallback(
    (side: Side, top: number) => {
      scrollTops.current[side] = top;
      activeSide.current = side;
      jumpTarget.current = null;
      lineTarget.current = null;
      alignFollower(side, top);
      redrawBands();
    },
    [alignFollower, redrawBands],
  );

  const positionSegment = useCallback((index: number) => {
    const segment = segments[index];
    if (!segment) return;
    for (const [column, rows, heights, start] of [
      [leftRef.current, leftRows, leftHeights, segment.leftStart],
      [rightRef.current, rightRows, rightHeights, segment.rightStart],
    ] as const) {
      if (!column) continue;
      const viewport = column.getViewportHeight();
      column.setScrollTop(clampLine(yAtFilePosition(rows, heights, start) - viewport / 3, heights.total() - viewport), false);
    }
    redrawBands();
  }, [segments, leftRows, rightRows, leftHeights, rightHeights, redrawBands]);

  const positionFileAnchor = useCallback((anchor: FileAnchor) => {
    const side = anchor.version === "old" ? "left" : "right";
    const column = side === "left" ? leftRef.current : rightRef.current;
    if (!column) return;
    const rows = side === "left" ? leftRows : rightRows;
    const heights = side === "left" ? leftHeights : rightHeights;
    column.setScrollTop(yAtFilePosition(rows, heights, anchor.line - 1), false);
    alignFollower(side, column.getScrollTop());
    redrawBands();
  }, [leftRows, rightRows, leftHeights, rightHeights, alignFollower, redrawBands]);

  // 两栏量高后只由最近主动的一栏重新对齐，避免左右异步到数时互相拉扯。
  useLayoutEffect(() => {
    if (lineTarget.current && currentLayout) positionFileAnchor(lineTarget.current);
    else if (jumpTarget.current !== null) positionSegment(jumpTarget.current);
    else {
      const side = activeSide.current;
      const column = side === "left" ? leftRef.current : rightRef.current;
      if (column) alignFollower(side, column.getScrollTop());
    }
    redrawBands();
  }, [alignFollower, positionFileAnchor, positionSegment, redrawBands, heightsVersion, currentLayout]);

  // 布局到达、折叠展开、悬停变化后重画色带。
  useLayoutEffect(() => {
    redrawBands();
  }, [redrawBands, cacheVersion, gutterWidth]);

  useImperativeHandle(
    handleRef,
    () => ({
      getFileAnchor() {
        if (!currentLayout) return null;
        const read = (side: Side) => {
          const column = side === "left" ? leftRef.current : rightRef.current;
          return column ? sideFileAnchor(side, side === "left" ? leftRows : rightRows,
            side === "left" ? leftHeights : rightHeights, column.getScrollTop()) : null;
        };
        return read(activeSide.current) ?? read(activeSide.current === "left" ? "right" : "left");
      },
      restoreFileAnchor(anchor: FileAnchor) {
        const side = anchor.version === "old" ? "left" : "right";
        const column = side === "left" ? leftRef.current : rightRef.current;
        if (!currentLayout || !column || column.getViewportHeight() <= 0) return false;
        lineTarget.current = anchor;
        jumpTarget.current = null;
        activeSide.current = side;
        const fold = foldUnchanged ? folds.findIndex((item) => {
          const start = side === "left" ? item.leftStart : item.rightStart;
          const end = side === "left" ? item.leftEnd : item.rightEnd;
          return start <= anchor.line - 1 && anchor.line - 1 < end;
        }) : -1;
        if (fold >= 0 && !expandedFolds.has(fold)) {
          setExpandedFolds((current) => new Set(current).add(fold));
        } else {
          positionFileAnchor(anchor);
        }
        return true;
      },
      jumpToSegment(index: number) {
        if (!segments[index]) return;
        lineTarget.current = null;
        jumpTarget.current = index;
        activeSide.current = "left";
        flushSync(() => positionSegment(index));
      },
      segmentCount() {
        return segments.length;
      },
      getScrollTop() {
        return leftRef.current?.getScrollTop() ?? 0;
      },
      setScrollTop(top: number) {
        leftRef.current?.setScrollTop(top, true);
      },
      getViewportHeight() {
        return leftRef.current?.getViewportHeight() ?? 0;
      },
      getContentHeight() {
        return leftHeights.total();
      },
      getVisibleRange(side: Side = "left") {
        const column = side === "left" ? leftRef.current : rightRef.current;
        const heights = side === "left" ? leftHeights : rightHeights;
        const top = column?.getScrollTop() ?? 0;
        const height = column?.getViewportHeight() ?? 0;
        return {
          from: heights.rowAt(top),
          to: Math.min(heights.count, Math.ceil(heights.rowAtFraction(top + height))),
        };
      },
      isCovered(from: number, to: number, side: Side = "left") {
        const map = side === "left" ? cache.current.old : cache.current.new;
        const rows = side === "left" ? leftRows : rightRows;
        for (let row = Math.max(0, from); row < to; row += 1) {
          const item = rows.rows[row];
          if (!item) continue;
          if ("line" in item && !map.has(item.line)) return false;
        }
        return true;
      },
      isCompact() {
        return false;
      },
      syncError() {
        if (!layout) return 0;
        const left = leftRef.current;
        const right = rightRef.current;
        if (!left || !right) return 0;
        const leftGeometry = { rows: leftRows, heights: leftHeights, viewportHeight: left.getViewportHeight() };
        const rightGeometry = { rows: rightRows, heights: rightHeights, viewportHeight: right.getViewportHeight() };
        return activeSide.current === "left"
          ? syncPositionError("left", left.getScrollTop(), right.getScrollTop(), leftGeometry, rightGeometry, segments)
          : syncPositionError("right", right.getScrollTop(), left.getScrollTop(), rightGeometry, leftGeometry, segments);
      },
    }),
    [segments, leftRows, rightRows, leftHeights, rightHeights, layout, positionSegment,
      currentLayout, folds, foldUnchanged, expandedFolds, positionFileAnchor],
  );

  useEffect(() => {
    if (!onSample || !layout) return;
    const top = leftRef.current?.getScrollTop() ?? 0;
    const height = leftRef.current?.getViewportHeight() ?? 0;
    const from = leftHeights.rowAt(top);
    const to = Math.min(leftHeights.count, Math.ceil(leftHeights.rowAtFraction(top + height)));
    let uncovered = 0;
    for (let row = from; row < to; row += 1) {
      const item = leftRows.rows[row];
      if (item && "line" in item && !cache.current.old.has(item.line)) uncovered += 1;
    }
    onSample({ from, to, uncovered });
  }, [onSample, layout, cacheVersion, leftRows, leftHeights, heightsVersion]);

  if (layoutError) {
    return <div role="status" className="p-3 text-[12px] text-destructive">{layoutError}</div>;
  }
  if (!layout) {
    return <div className="p-3 text-[12px] text-descriptionforeground">{messages.changes.loading}</div>;
  }
  if (layout.binary) {
    return <div className="p-3 text-[12px] text-descriptionforeground">{messages.diff.binaryNotCompared}</div>;
  }
  const labels = messages.diff.sideLabels(layout.changes, layout.conflict);
  // 冲突文件只读：不显示块按钮（设计 3.2）。
  const leftActions = layout.conflict ? "none" : layout.changes === "unstaged" ? "discard" : layout.changes === "staged" ? "unstage" : "none";
  const rightActions = layout.conflict ? "none" : layout.changes === "unstaged" ? "stage" : "none";

  // 段按钮是否可用：Q140“同时忽略空行”一档下按钮变灰并说明原因。
  const actionsEnabled = layout.segmentActionsAvailable && !isWriting;
  const actionsDisabled = !actionsEnabled;
  const actionFor = (index: number): SegmentAction | null => layout.segmentActions[index] ?? null;

  return (
    <div className="sbs flex h-full min-h-0 flex-col" style={{ "--font-mono": codeFontStack(fontFamily ?? null) } as React.CSSProperties}>
      <div className="sbs-header flex shrink-0 items-center text-[11.5px] text-descriptionforeground">
        <div className="flex min-w-0 flex-1 items-center gap-2 px-2">
          <span className="shrink-0 font-semibold">{labels.left}</span>
          <span className="truncate">{entry.path}</span>
        </div>
        <div className="shrink-0" style={{ width: gutterWidth }} />
        <div className="flex min-w-0 flex-1 items-center gap-2 px-2">
          <span className="shrink-0 font-semibold">{labels.right}</span>
          <span className="truncate">{entry.path}</span>
        </div>
      </div>
      <div className="relative flex min-h-0 flex-1">
        <SideColumn
          handleRef={leftRef}
          side="left"
          onPaint={(top) => {
            scrollTops.current.left = top;
            leftGutterRef.current?.place(top);
          }}
          onWindow={(top) => leftGutterRef.current?.setWindow(top)}
          lineCount={layout.leftLineCount}
          rows={leftRows}
          heights={leftHeights}
          folds={folds}
          segments={segments}
          cache={cache.current}
          cacheVersion={cacheVersion}
          highlights={highlights}
          highlight={highlight}
          showWhitespace={showWhitespace}
          fontSize={fontSize}
          onWidth={onColumnWidth}
          onHeightsChanged={onHeightsChanged}
          ensureLines={ensureLines}
          onScroll={(top) => handleScroll("left", top)}
          hovered={hovered}
          onExpandFold={(index) => setExpandedFolds((current) => new Set(current).add(index))}
        />
        <div className="sbs-gutter relative shrink-0" style={{ width: gutterWidth }} ref={gutterBoxRef}>
          <canvas ref={gutterRef} className="sbs-bands" />
          <GutterNumbers
            side="left"
            rows={leftRows}
            heights={leftHeights}
            segments={segments}
            actions={leftActions}
            actionsDisabled={actionsDisabled}
            onSegmentAction={(index) => {
              const action = actionFor(index);
              if (!action) return;
              if (layout.changes === "staged") props.onUnstageSegment?.(action);
              else props.onDiscardSegment?.(action);
            }}
            handleRef={leftGutterRef}
            onReveal={(top, height) => leftRef.current?.reveal(top, height)}
            onHover={setHovered}
            width={GUTTER_NUMBERS}
            align="right"
          />
          <GutterNumbers
            side="right"
            rows={rightRows}
            heights={rightHeights}
            segments={segments}
            actions={rightActions}
            actionsDisabled={actionsDisabled}
            onSegmentAction={(index) => {
              const action = actionFor(index);
              if (action) props.onStageSegment?.(action);
            }}
            handleRef={rightGutterRef}
            onReveal={(top, height) => rightRef.current?.reveal(top, height)}
            onHover={setHovered}
            width={GUTTER_NUMBERS}
            align="left"
            offset={GUTTER_NUMBERS + GUTTER_BAND}
          />
        </div>
        <SideColumn
          handleRef={rightRef}
          side="right"
          onPaint={(top) => {
            scrollTops.current.right = top;
            rightGutterRef.current?.place(top);
          }}
          onWindow={(top) => rightGutterRef.current?.setWindow(top)}
          lineCount={layout.rightLineCount}
          rows={rightRows}
          heights={rightHeights}
          folds={folds}
          segments={segments}
          cache={cache.current}
          cacheVersion={cacheVersion}
          highlights={highlights}
          highlight={highlight}
          showWhitespace={showWhitespace}
          fontSize={fontSize}
          onWidth={onColumnWidth}
          onHeightsChanged={onHeightsChanged}
          ensureLines={ensureLines}
          onScroll={(top) => handleScroll("right", top)}
          hovered={hovered}
          onExpandFold={(index) => setExpandedFolds((current) => new Set(current).add(index))}
        />
      </div>
    </div>
  );
}

// ---------- 单栏 ----------

interface SideColumnHandle {
  getScrollTop(): number;
  setScrollTop(top: number, notify: boolean): void;
  getViewportHeight(): number;
  /** 让一段内容进入可见区（键盘聚焦预渲染的按钮时），经自绘滚动入口并同步另一栏。 */
  reveal(top: number, height: number): void;
}

interface SideColumnProps {
  handleRef: React.Ref<SideColumnHandle>;
  side: Side;
  lineCount: number;
  rows: RowIndex;
  heights: RowHeights;
  folds: FoldRange[];
  segments: ChangeSegment[];
  cache: SideLineWindow;
  cacheVersion: number;
  highlights: HighlightStore;
  highlight: DiffHighlight;
  showWhitespace: boolean;
  fontSize: number;
  onWidth: (side: Side, width: number) => void;
  onHeightsChanged: () => void;
  ensureLines: (side: Side, visible: SideLineRange[], buffered: SideLineRange[], lineCount: number) => void;
  onScroll: (top: number) => void;
  /** 每次写入滚动位移时同步调用（窄栏行号与正文同一帧移动）。 */
  onPaint: (top: number) => void;
  /** 行窗口需要随滚动位置更新时调用（与本栏的行窗口同一次提交）。 */
  onWindow: (top: number) => void;
  hovered: number | null;
  onExpandFold: (index: number) => void;
}

function SideColumn(props: SideColumnProps) {
  const { handleRef, side, lineCount, rows, heights, folds, cache, cacheVersion, highlights, highlight, showWhitespace, fontSize, onWidth, onHeightsChanged, ensureLines, onScroll, onWindow } = props;
  const onPaintRef = useRef(props.onPaint);
  onPaintRef.current = props.onPaint;
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLDivElement | null>(null);
  const trackRef = useRef<HTMLDivElement | null>(null);
  const thumbRef = useRef<HTMLDivElement | null>(null);
  const scrollTopRef = useRef(0);
  const [firstRow, setFirstRow] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(0);
  const previous = useRef({ rows, heights, viewportHeight });
  const geometry = useRef({ viewportHeight: 0, contentHeight: 0 });
  const contentHeight = heights.total();
  geometry.current = { viewportHeight, contentHeight };
  const heightsRef = useRef(heights);
  heightsRef.current = heights;
  const changedGeometry = previous.current.heights !== heights || previous.current.rows !== rows;
  // 新宽度下先渲染原内容，再量高和夹紧；先用估算总高夹紧会把文件底部的锚点弄丢。
  const renderTop = changedGeometry
    ? yAtFilePosition(rows, heights, filePositionAtY(previous.current.rows, previous.current.heights, scrollTopRef.current))
    : scrollTopRef.current;
  const renderFirst = changedGeometry ? heights.rowAt(renderTop) : firstRow;

  const paint = useCallback(() => {
    const top = scrollTopRef.current;
    if (canvasRef.current) canvasRef.current.style.transform = `translateY(${-top}px)`;
    onPaintRef.current(top);
    const track = trackRef.current;
    const thumb = thumbRef.current;
    if (!track || !thumb) return;
    const box = thumbGeometry(top, geometry.current.viewportHeight, geometry.current.contentHeight);
    track.style.display = box ? "block" : "none";
    if (box) {
      thumb.style.height = `${box.size}px`;
      thumb.style.transform = `translateY(${box.offset}px)`;
    }
  }, []);

  const applyScroll = useCallback(
    (nextTop: number, notify: boolean) => {
      const { viewportHeight: height, contentHeight: total } = geometry.current;
      const top = Math.min(Math.max(0, total - height), Math.max(0, nextTop));
      if (top === scrollTopRef.current) {
        if (notify) onScroll(top);
        return;
      }
      scrollTopRef.current = top;
      paint();
      const update = () => {
        setFirstRow(heightsRef.current.rowAt(top));
        onWindow(top);
        if (notify) onScroll(top);
      };
      // 主动滚动将两栏行窗口一起提交；量高后的跟随更新留在 layout effect 中完成。
      if (notify) flushSync(update);
      else update();
    },
    [onScroll, onWindow, paint],
  );

  const reveal = useCallback(
    (top: number, height: number) => {
      const current = scrollTopRef.current;
      const viewport = geometry.current.viewportHeight;
      if (top < current) applyScroll(top, true);
      else if (top + height > current + viewport) applyScroll(top + height - viewport, true);
    },
    [applyScroll],
  );

  useImperativeHandle(
    handleRef,
    () => ({
      getScrollTop: () => scrollTopRef.current,
      setScrollTop(top: number, notify: boolean) {
        applyScroll(top, notify);
      },
      getViewportHeight: () => geometry.current.viewportHeight,
      reveal,
    }),
    [applyScroll, reveal],
  );

  useLayoutEffect(() => {
    const element = viewportRef.current;
    if (!element) return;
    const measure = () => {
      setViewportHeight(element.clientHeight);
      onWidth(side, element.clientWidth);
    };
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    return () => observer.disconnect();
  }, [onWidth, side]);

  useLayoutEffect(() => {
    const old = previous.current;
    const changed = old.rows !== rows || old.heights !== heights || old.viewportHeight !== viewportHeight;
    const anchor = filePositionAtY(old.rows, old.heights, scrollTopRef.current);
    let measured = false;
    if (heights.wrap && canvasRef.current) {
      for (const child of canvasRef.current.children) {
        const element = child as HTMLElement;
        const row = element.dataset.row;
        if (row !== undefined && heights.measure(Number(row), element.offsetHeight)) measured = true;
      }
    }
    const total = heights.total();
    geometry.current = { viewportHeight, contentHeight: total };
    previous.current = { rows, heights, viewportHeight };
    if (changed || measured) {
      scrollTopRef.current = clampLine(yAtFilePosition(rows, heights, anchor), total - viewportHeight);
      setFirstRow(heights.rowAt(scrollTopRef.current));
      onWindow(scrollTopRef.current);
      onHeightsChanged();
    }
    paint();
  });

  useEffect(() => {
    const element = viewportRef.current;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      if (event.ctrlKey) return;
      event.preventDefault();
      const unit =
        event.deltaMode === WheelEvent.DOM_DELTA_LINE ? LINE_HEIGHT : event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? geometry.current.viewportHeight : 1;
      if (event.deltaY !== 0) applyScroll(scrollTopRef.current + event.deltaY * unit, true);
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, [applyScroll]);

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      const page = Math.max(LINE_HEIGHT, geometry.current.viewportHeight - LINE_HEIGHT);
      const targets: Record<string, number> = {
        ArrowDown: scrollTopRef.current + 2 * LINE_HEIGHT,
        ArrowUp: scrollTopRef.current - 2 * LINE_HEIGHT,
        PageDown: scrollTopRef.current + page,
        PageUp: scrollTopRef.current - page,
        Home: 0,
        End: geometry.current.contentHeight,
      };
      const next = targets[event.key];
      if (next === undefined) return;
      event.preventDefault();
      applyScroll(next, true);
    },
    [applyScroll],
  );

  const onTrackPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const track = trackRef.current;
      const thumb = thumbRef.current;
      if (!track || !thumb || event.button !== 0) return;
      event.preventDefault();
      const { viewportHeight: height, contentHeight: total } = geometry.current;
      const box = thumbGeometry(scrollTopRef.current, height, total);
      if (!box) return;
      const trackTop = track.getBoundingClientRect().top;
      const pointerY = event.clientY - trackTop;
      if (event.target !== thumb) {
        const page = Math.max(LINE_HEIGHT, height - LINE_HEIGHT);
        applyScroll(scrollTopRef.current + (pointerY < box.offset ? -page : page), true);
        return;
      }
      const startY = event.clientY;
      const startTop = scrollTopRef.current;
      const ratio = (total - height) / Math.max(1, height - box.size);
      thumb.setPointerCapture(event.pointerId);
      const onMove = (move: PointerEvent) => applyScroll(startTop + (move.clientY - startY) * ratio, true);
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

  const viewportRows = Math.max(1, Math.ceil(Math.max(viewportHeight, LINE_HEIGHT) / LINE_HEIGHT));
  const from = Math.max(0, renderFirst - viewportRows * OVERSCAN_SCREENS);
  const to = Math.min(rows.rows.length, renderFirst + viewportRows * (1 + OVERSCAN_SCREENS) + 1);

  const visibleFrom = heights.rowAt(renderTop);
  const visibleTo = Math.min(rows.rows.length, Math.ceil(heights.rowAtFraction(renderTop + viewportHeight)));
  useEffect(() => {
    if (viewportHeight <= 0) return;
    ensureLines(side, lineRangesForRows(rows.rows, visibleFrom, visibleTo), lineRangesForRows(rows.rows, from, to), lineCount);
  }, [ensureLines, side, from, to, visibleFrom, visibleTo, viewportHeight, rows, lineCount, cacheVersion]);

  const map = side === "left" ? cache.old : cache.new;
  const hoveredRange = useMemo(() => {
    const segment = props.hovered === null ? undefined : props.segments[props.hovered];
    if (!segment) return null;
    return side === "left" ? [segment.leftStart + 1, segment.leftEnd] : [segment.rightStart + 1, segment.rightEnd];
  }, [props.hovered, props.segments, side]);

  return (
    <div
      ref={viewportRef}
      className={`sbs-column relative min-w-0 flex-1 overflow-hidden font-mono outline-none${heights.wrap ? " sbs-wrap" : ""}`}
      style={{ fontSize, lineHeight: `${LINE_HEIGHT}px` }}
      tabIndex={0}
      onKeyDown={onKeyDown}
      onScroll={resetNativeScroll}
    >
      <div ref={canvasRef} className="sbs-canvas" style={{ height: contentHeight }}>
        {Array.from({ length: Math.max(0, to - from) }, (_, offset) => {
          const rowIndex = from + offset;
          const item = rows.rows[rowIndex];
          if (!item) return null;
          const top = heights.top(rowIndex);
          if ("fold" in item) {
            const fold = folds[item.fold];
            const start = side === "left" ? fold.leftStart : fold.rightStart;
            const end = side === "left" ? fold.leftEnd : fold.rightEnd;
            return (
              <div key={`fold-${rowIndex}`} className="sbs-fold" style={{ top }}>
                <button
                  type="button"
                  className="sbs-fold-button"
                  onClick={() => props.onExpandFold(item.fold)}
                  onFocus={() => reveal(top, heights.height(rowIndex))}
                >
                  {messages.diff.foldLines(Math.max(0, end - start))}
                </button>
              </div>
            );
          }
          const line = map.get(item.line);
          return (
            <LineRow
              key={item.line}
              rowIndex={rowIndex}
              top={top}
              text={line?.text ?? ""}
              changed={line?.changed ?? false}
              words={line?.words ?? EMPTY_WORDS}
              lineNumber={item.line}
              side={side}
              highlight={highlight}
              showWhitespace={showWhitespace}
              tokens={highlights.get(side === "left" ? "old" : "new", item.line)}
              hovered={hoveredRange !== null && item.line >= hoveredRange[0] && item.line <= hoveredRange[1]}
            />
          );
        })}
      </div>
      <div ref={trackRef} className="diff-scrollbar" onPointerDown={onTrackPointerDown}>
        <div ref={thumbRef} className="diff-scrollbar-thumb" />
      </div>
    </div>
  );
}

function thumbGeometry(top: number, viewportHeight: number, contentHeight: number): { size: number; offset: number } | null {
  if (contentHeight <= viewportHeight || viewportHeight <= 0) return null;
  const size = Math.max(SCROLLBAR_MIN_THUMB, (viewportHeight / contentHeight) * viewportHeight);
  const maxTop = contentHeight - viewportHeight;
  return { size, offset: (top / maxTop) * (viewportHeight - size) };
}

const LineRow = memo(function LineRow({
  rowIndex,
  top,
  text,
  changed,
  words,
  lineNumber,
  side,
  highlight,
  showWhitespace,
  tokens,
  hovered,
}: {
  rowIndex: number;
  top: number;
  text: string;
  changed: boolean;
  words: Array<[number, number]>;
  lineNumber: number;
  side: Side;
  highlight: DiffHighlight;
  showWhitespace: boolean;
  tokens: ReturnType<HighlightStore["get"]>;
  hovered: boolean;
}) {
  const tone = !changed ? "" : side === "left" ? " sbs-del" : " sbs-ins";
  const useWords = (highlight === "word" || highlight === "char") && words.length > 0;
  return (
    <div className={`sbs-row${tone}${hovered ? " sbs-hover" : ""}`} style={{ top }} data-row={rowIndex} data-line={lineNumber}>
      <span className="sbs-text">
        {renderLine(text, useWords ? words : [], tokens, showWhitespace)}
      </span>
    </div>
  );
});

/** 行内文字：语法颜色与改动字词叠加；没有 token 时退回纯文本。 */
function renderLine(
  text: string,
  words: Array<[number, number]>,
  tokens: ReturnType<HighlightStore["get"]>,
  showWhitespace: boolean,
) {
  if (showWhitespace) {
    return highlightWords(visibleWhitespace(text), words, "sbs-word");
  }
  if (!tokens || tokens.map((token) => token.text).join("") !== text) {
    return highlightWords(text, words, "sbs-word");
  }
  if (words.length === 0) {
    return tokens.map((token, index) => (
      <span key={index} className={`tok-${token.category}`}>{token.text}</span>
    ));
  }
  // 把 token 按改动区间切开，同一段文字既是语法颜色又是改动字词。
  const parts: React.ReactNode[] = [];
  let offset = 0;
  for (const [index, token] of tokens.entries()) {
    const start = offset;
    const end = offset + token.text.length;
    offset = end;
    let cursor = start;
    let piece = 0;
    for (const [wordStart, wordEnd] of words) {
      if (wordEnd <= cursor || wordStart >= end) continue;
      const from = Math.max(cursor, wordStart);
      const to = Math.min(end, wordEnd);
      if (from > cursor) {
        parts.push(<span key={`${index}-${piece++}`} className={`tok-${token.category}`}>{token.text.slice(cursor - start, from - start)}</span>);
      }
      parts.push(<span key={`${index}-${piece++}`} className={`tok-${token.category} sbs-word`}>{token.text.slice(from - start, to - start)}</span>);
      cursor = to;
    }
    if (cursor < end) {
      parts.push(<span key={`${index}-${piece++}`} className={`tok-${token.category}`}>{token.text.slice(cursor - start)}</span>);
    }
  }
  return parts;
}

function highlightWords(text: string, words: Array<[number, number]>, className: string) {
  if (words.length === 0) return text;
  const parts: React.ReactNode[] = [];
  let cursor = 0;
  for (const [index, [start, end]] of words.entries()) {
    if (start > cursor) parts.push(text.slice(cursor, start));
    parts.push(
      <span key={index} className={className}>
        {text.slice(start, end)}
      </span>,
    );
    cursor = end;
  }
  if (cursor < text.length) parts.push(text.slice(cursor));
  return parts;
}

/** 显示空白字符（Q139）：空格显示为中点、制表符显示为箭头。 */
function visibleWhitespace(text: string): string {
  return text.replace(/ /g, "·").replace(/\t/g, "→");
}

// ---------- 窄栏 ----------

interface GutterHandle {
  /** 写入滚动位移（与正文同一次调用，同一帧移动）。 */
  place(top: number): void;
  /** 按滚动位置更新行窗口（只在首行变化时重新渲染）。 */
  setWindow(top: number): void;
}

/** 窄栏里的两列行号与段按钮：由对应一栏在滚动时直接驱动。 */
function GutterNumbers({
  side,
  rows,
  heights,
  segments,
  actions,
  actionsDisabled,
  onSegmentAction,
  handleRef,
  onReveal,
  onHover,
  width,
  align,
  offset = 0,
}: {
  side: Side;
  rows: RowIndex;
  heights: RowHeights;
  segments: ChangeSegment[];
  actions: "stage" | "unstage" | "discard" | "none";
  actionsDisabled: boolean;
  onSegmentAction: (index: number) => void;
  handleRef: React.Ref<GutterHandle>;
  onReveal: (top: number, height: number) => void;
  onHover: (index: number | null) => void;
  width: number;
  align: "left" | "right";
  offset?: number;
}) {
  const [firstRow, setFirstRow] = useState(0);
  const [height, setHeight] = useState(0);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const linesRef = useRef<HTMLDivElement | null>(null);
  const eofRef = useRef<HTMLDivElement | null>(null);
  const topRef = useRef(0);
  const heightsRef = useRef(heights);
  heightsRef.current = heights;
  const heightRef = useRef(height);
  heightRef.current = height;

  const place = useCallback((top: number) => {
    topRef.current = top;
    if (linesRef.current) linesRef.current.style.transform = `translateY(${-top}px)`;
    if (eofRef.current) {
      const y = Math.min(Math.max(0, heightRef.current - LINE_HEIGHT), heightsRef.current.total() - top);
      eofRef.current.style.transform = `translateY(${y}px)`;
    }
  }, []);

  useImperativeHandle(
    handleRef,
    () => ({
      place,
      setWindow(top: number) {
        setFirstRow(Math.max(0, heightsRef.current.rowAt(top) - 1));
      },
    }),
    [place],
  );

  // 行高、视口高度或窗口变化后重新写位移（滚动本身不经过这里）。
  useLayoutEffect(() => {
    place(topRef.current);
  });

  useLayoutEffect(() => {
    const element = rootRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setHeight(element.clientHeight));
    observer.observe(element);
    setHeight(element.clientHeight);
    return () => observer.disconnect();
  }, []);

  const visibleRows = Math.ceil(Math.max(height, LINE_HEIGHT) / LINE_HEIGHT) + 3;
  const segmentStart = useMemo(() => {
    const result = new Map<number, number>();
    segments.forEach((segment, index) => {
      const line = side === "left" ? segment.leftStart + 1 : segment.rightStart + 1;
      result.set(line, index);
    });
    return result;
  }, [segments, side]);

  const actionLabel =
    actions === "discard"
      ? messages.changes.discardSegment
      : actions === "unstage"
        ? messages.changes.unstageSegment
        : messages.changes.stageSegment;
  const actionButton = (segmentIndex: number, reveal: (() => void) | null) => (
    <button
      type="button"
      className="sbs-gutter-action"
      title={actionLabel}
      aria-label={actionLabel}
      disabled={actionsDisabled}
      onClick={() => onSegmentAction(segmentIndex)}
      onMouseEnter={() => onHover(segmentIndex)}
      onMouseLeave={() => onHover(null)}
      onFocus={() => {
        onHover(segmentIndex);
        reveal?.();
      }}
      onBlur={() => onHover(null)}
    >
      {actions === "stage" ? "«" : "»"}
    </button>
  );

  // 文件末尾的段（例如末尾新增）：按钮固定在内容末尾，滚到底时贴住视口底边。
  const eofSegment = actions === "none" ? undefined : segmentStart.get(rows.lineCount + 1);
  const showEof = eofSegment !== undefined && firstRow + visibleRows >= rows.rows.length;

  return (
    <div
      ref={rootRef}
      className={`sbs-numbers absolute inset-y-0 overflow-hidden ${align === "right" ? "text-right" : "text-left"}`}
      style={{ left: offset, width }}
      onScroll={resetNativeScroll}
    >
      <div ref={linesRef} className="absolute inset-x-0 top-0">
        {Array.from({ length: visibleRows }, (_, offsetRow) => {
          const rowIndex = firstRow + offsetRow;
          const item = rows.rows[rowIndex];
          if (!item || !("line" in item)) return null;
          const segmentIndex = actions === "none" ? undefined : segmentStart.get(item.line);
          const rowTop = heights.top(rowIndex);
          return (
            <div key={rowIndex} className="sbs-number" style={{ transform: `translateY(${rowTop}px)` }}>
              <span className="sbs-number-text">{item.line}</span>
              {segmentIndex !== undefined
                ? actionButton(segmentIndex, () => onReveal(rowTop, heights.height(rowIndex)))
                : null}
            </div>
          );
        })}
      </div>
      {showEof ? (
        <div ref={eofRef} className="sbs-number">
          <span className="sbs-number-text" />
          {actionButton(eofSegment, null)}
        </div>
      ) : null}
    </div>
  );
}

/** 视口不允许原生滚动（聚焦预渲染的按钮时浏览器会自动滚动），出现就复位。 */
function resetNativeScroll(event: React.UIEvent<HTMLDivElement>) {
  const element = event.currentTarget;
  if (element.scrollTop !== 0) element.scrollTop = 0;
  if (element.scrollLeft !== 0) element.scrollLeft = 0;
}

/**
 * 画窄栏（设计 4.3，照 Rebased）：改动块的底色穿过两侧行号列，中间用弧形色带连起来。
 * 修改块从删除色渐变到新增色；悬停的块用实色并描边。
 */
function drawBands(
  canvas: HTMLCanvasElement | null,
  box: HTMLDivElement | null,
  options: {
    leftTop: number;
    rightTop: number;
    segments: ChangeSegment[];
    leftRows: RowIndex;
    rightRows: RowIndex;
    leftHeights: RowHeights;
    rightHeights: RowHeights;
    hovered: number | null;
  },
) {
  const prepared = prepareCanvas(canvas, box);
  if (!prepared) return;
  const { context, width, height, styles } = prepared;
  const read = (name: string, fallback: string) => styles.getPropertyValue(name).trim() || fallback;
  const lineIns = read("--diff-ins-bg", "rgb(60 120 60 / 25%)");
  const lineDel = read("--diff-del-bg", "rgb(160 60 60 / 25%)");
  const bandIns = read("--sbs-band-ins", "rgb(60 120 60 / 35%)");
  const bandDel = read("--sbs-band-del", "rgb(160 60 60 / 35%)");
  const edgeIns = read("--sbs-band-ins-edge", bandIns);
  const edgeDel = read("--sbs-band-del-edge", bandDel);
  const bandFrom = GUTTER_NUMBERS;
  const bandTo = width - GUTTER_NUMBERS;
  const { segments, leftTop, rightTop, leftRows, rightRows, leftHeights, rightHeights, hovered } = options;

  segments.forEach((segment, index) => {
    // 行区间换成显示行（折叠区占一行）；长度为 0 的一侧落在插入点上。
    const ends: BandEnds = {
      leftTop: yAtFilePosition(leftRows, leftHeights, segment.leftStart) - leftTop,
      leftBottom: yAtFilePosition(leftRows, leftHeights, segment.leftEnd) - leftTop,
      rightTop: yAtFilePosition(rightRows, rightHeights, segment.rightStart) - rightTop,
      rightBottom: yAtFilePosition(rightRows, rightHeights, segment.rightEnd) - rightTop,
    };
    if (!bandVisible(ends, height)) return;
    const deleted = segment.leftEnd > segment.leftStart;
    const added = segment.rightEnd > segment.rightStart;
    const strong = hovered === index;
    const leftColor = deleted ? (strong ? bandDel : lineDel) : bandIns;
    const rightColor = added ? (strong ? bandIns : lineIns) : bandDel;
    // 两侧行号列里的底色（与该栏的行底色一致）；长度为 0 的一侧画 2px 的线。
    context.fillStyle = leftColor;
    context.fillRect(0, ends.leftTop, bandFrom, Math.max(2, ends.leftBottom - ends.leftTop));
    context.fillStyle = rightColor;
    context.fillRect(bandTo, ends.rightTop, width - bandTo, Math.max(2, ends.rightBottom - ends.rightTop));
    const fill = deleted && added
      ? horizontalGradient(context, bandFrom, bandTo, bandDel, bandIns)
      : deleted
        ? bandDel
        : bandIns;
    drawBand(context, bandFrom, bandTo, ends, fill, strong ? (added ? edgeIns : edgeDel) : undefined);
  });
}
