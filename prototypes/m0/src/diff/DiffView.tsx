/**
 * 自绘只读 diff 视图。
 *
 * - 只渲染可见区域（固定行高 + 窗口化），并排 / 统一两种视图共用同一份 hunk 数据；
 * - token 只来自外部传入的高亮结果，组件本身不做任何差异计算（ADR-0004）。
 */

import { useCallback, useLayoutEffect, useMemo, useRef, useState } from "react";
import { buildFileModel, type DiffRow, type FileModel } from "./model";
import type { DiffLine, FileChange, HighlightToken, HighlightedDocument, ViewMode } from "./types";

export const ROW_HEIGHT = 18;
const FONT_SIZE = 12;
const OVERSCAN = 24;
const CHAR_WIDTH = 7.2;

interface DiffViewProps {
  file: FileChange;
  view: ViewMode;
  /** 高亮结果；null 表示纯文本。 */
  highlights: HighlightedDocument | null;
  viewportRef: React.RefObject<DiffViewport | null>;
}

export interface DiffViewport {
  scrollTo(row: number): void;
  scrollBy(delta: number): void;
  readScrollTop(): number;
  rowHeight: number;
}

export function DiffView({ file, view, highlights, viewportRef }: DiffViewProps) {
  const model = useMemo(() => buildFileModel(file), [file]);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(0);

  useLayoutEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => {
      setHeight(element.clientHeight);
    });
    observer.observe(element);
    setHeight(element.clientHeight);
    return () => observer.disconnect();
  }, []);

  useLayoutEffect(() => {
    const viewport: DiffViewport = {
      rowHeight: ROW_HEIGHT,
      scrollTo(row: number) {
        containerRef.current?.scrollTo({ top: row * ROW_HEIGHT });
      },
      scrollBy(delta: number) {
        containerRef.current?.scrollBy({ top: delta });
      },
      readScrollTop() {
        return containerRef.current?.scrollTop ?? 0;
      },
    };
    viewportRef.current = viewport;
    return () => {
      viewportRef.current = null;
    };
  }, [viewportRef]);

  const onScroll = useCallback((event: React.UIEvent<HTMLDivElement>) => {
    setScrollTop(event.currentTarget.scrollTop);
  }, []);

  const rows = model.rows;
  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  const last = Math.min(rows.length, Math.ceil((scrollTop + Math.max(height, 1)) / ROW_HEIGHT) + OVERSCAN);
  const visible = rows.slice(first, Math.max(first, last));
  const contentWidth = Math.max(model.maxLineLength * CHAR_WIDTH + 160, 600);

  const gutters: React.CSSProperties = {
    color: "#9aa0a6",
    textAlign: "right",
    paddingRight: 8,
    userSelect: "none",
    borderRight: "1px solid #f0f0f0",
  };

  return (
    <div
      ref={containerRef}
      className="diff-scroll"
      onScroll={onScroll}
      style={{
        position: "relative",
        overflow: "auto",
        height: "100%",
        fontFamily: '"SF Mono", SFMono-Regular, ui-monospace, Menlo, monospace',
        fontSize: FONT_SIZE,
        lineHeight: `${ROW_HEIGHT}px`,
        background: "#ffffff",
      }}
    >
      <div style={{ height: rows.length * ROW_HEIGHT, width: contentWidth, position: "relative" }}>
        {visible.map((row) => (
          <div
            key={row.index}
            style={{
              position: "absolute",
              top: row.index * ROW_HEIGHT,
              left: 0,
              right: 0,
              height: ROW_HEIGHT,
              width: contentWidth,
              whiteSpace: "pre",
              display: "grid",
              gridTemplateColumns:
                view === "split" ? "56px 1fr 56px 1fr" : "56px 56px 1fr",
            }}
          >
            {view === "split" ? (
              <>
                <LineNumber value={row.left?.oldLine ?? null} style={gutters} />
                <LineCell line={row.left} kind="left" row={row} highlights={highlights} />
                <LineNumber value={row.right?.newLine ?? null} style={gutters} />
                <LineCell line={row.right} kind="right" row={row} highlights={highlights} />
              </>
            ) : (
              <>
                <LineNumber value={row.line?.oldLine ?? null} style={gutters} />
                <LineNumber value={row.line?.newLine ?? null} style={gutters} />
                <LineCell line={row.line} kind="unified" row={row} highlights={highlights} />
              </>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function LineNumber({ value, style }: { value: number | null; style: React.CSSProperties }) {
  return <div style={style}>{value ?? ""}</div>;
}

interface LineCellProps {
  line: DiffLine | null;
  kind: "left" | "right" | "unified";
  row: DiffRow;
  highlights: HighlightedDocument | null;
}

function LineCell({ line, kind, row, highlights }: LineCellProps) {
  if (row.kind === "hunk") {
    return (
      <div
        style={{
          gridColumn: kind === "left" ? "2 / 5" : kind === "right" ? "4 / 5" : "3 / 4",
          background: "#f6f8fa",
          color: "#57606a",
          paddingLeft: 8,
          overflow: "hidden",
        }}
      >
        {kind === "right" ? "" : row.section}
      </div>
    );
  }

  if (!line) {
    return <div style={{ background: "#fafbfc" }} />;
  }

  const background =
    line.kind === "addition"
      ? "#e6ffec"
      : line.kind === "deletion"
        ? "#ffebe9"
        : line.kind === "noEol"
          ? "#f6f8fa"
          : "transparent";
  const marker = line.kind === "addition" ? "+" : line.kind === "deletion" ? "-" : " ";
  const highlight = highlights ? pickTokens(highlights, line, kind) : null;

  return (
    <div style={{ background, overflow: "hidden", paddingLeft: 4, display: "flex" }}>
      <span style={{ color: "#8b949e", width: 10, flex: "0 0 auto" }}>{marker}</span>
      <span style={{ overflow: "hidden" }}>
        {highlight ? (
          highlight.map((token, index) => <TokenSpan key={index} token={token} />)
        ) : (
          line.text || " "
        )}
      </span>
    </div>
  );
}

/** 按渲染位置取高亮：左侧读旧版本，右侧读新版本。 */
function pickTokens(
  highlights: HighlightedDocument,
  line: DiffLine,
  kind: "left" | "right" | "unified",
): HighlightToken[] | null {
  if (kind === "left") {
    return line.oldLine != null ? (highlights.old.get(line.oldLine) ?? null) : null;
  }
  if (kind === "right") {
    return line.newLine != null ? (highlights.new.get(line.newLine) ?? null) : null;
  }
  if (line.kind === "addition") {
    return line.newLine != null ? (highlights.new.get(line.newLine) ?? null) : null;
  }
  if (line.oldLine != null) {
    return highlights.old.get(line.oldLine) ?? null;
  }
  return line.newLine != null ? (highlights.new.get(line.newLine) ?? null) : null;
}

function TokenSpan({ token }: { token: HighlightToken }) {
  return (
    <span
      style={{
        color: token.color || undefined,
        fontWeight: token.bold ? 600 : undefined,
        fontStyle: token.italic ? "italic" : undefined,
        textDecoration: token.underline ? "underline" : undefined,
      }}
    >
      {token.text}
    </span>
  );
}

export type { FileModel };
