/**
 * 左右对照的显示行索引（返修 A1/Q138，设计 4.1）。
 *
 * 两栏各自连续显示整个文件；折叠未改动部分时，折叠区在显示行里占一行。
 * 这里把“文件行号 ↔ 显示行”的换算抽成纯函数：行号单调递增，换算用二分，
 * 避免大文件（8309 行、上千段）上线性扫描卡住主线程。
 */

import type { FoldRange } from "@/types/bindings/FoldRange";

export type Side = "left" | "right";

/** 显示行：文件行号（1 基）或折叠行（折叠区下标）。 */
export type DisplayRow = { line: number } | { fold: number; start: number; end: number };

export interface RowIndex {
  lineCount: number;
  rows: DisplayRow[];
  /** `lines[i]` 是第 i 个显示行对应的文件行号（折叠行取折叠区之后的第一行）。 */
  lines: number[];
  /** 折叠区 id → 显示行；滚动同步按同一个占位映射，不能借用折叠之后的正文行。 */
  foldRows: Map<number, number>;
}

/**
 * 构造显示行索引。
 *
 * `expanded` 是已展开的折叠区下标；不在其中的折叠区各占一个显示行。
 */
export function buildRows(
  lineCount: number,
  folds: FoldRange[],
  side: Side,
  expanded: Set<number>,
): RowIndex {
  const rows: DisplayRow[] = [];
  const foldRows = new Map<number, number>();
  const active = folds
    .map((fold, index) => ({ fold, index }))
    .filter(({ index }) => !expanded.has(index))
    .map(({ fold, index }) => ({
      index,
      start: (side === "left" ? fold.leftStart : fold.rightStart) + 1,
      end: (side === "left" ? fold.leftEnd : fold.rightEnd) + 1,
    }))
    .sort((a, b) => a.start - b.start);
  let line = 1;
  for (const fold of active) {
    if (fold.start < line || fold.end <= fold.start) continue;
    for (; line < fold.start; line += 1) rows.push({ line });
    foldRows.set(fold.index, rows.length);
    rows.push({ fold: fold.index, start: fold.start - 1, end: fold.end - 1 });
    line = Math.max(line, fold.end);
  }
  for (; line <= lineCount; line += 1) rows.push({ line });
  const lines = rows.map((row) => {
    if ("line" in row) return row.line;
    const fold = folds[row.fold];
    const end = side === "left" ? fold.leftEnd : fold.rightEnd;
    return Math.min(Math.max(1, lineCount), Math.max(1, end + 1));
  });
  return { lineCount, rows, lines, foldRows };
}

/** 显示行下标 → 文件行号（折叠行给出折叠区之后的行）。 */
export function rowToLine(index: RowIndex, row: number): number {
  const at = Math.min(Math.max(0, Math.floor(row)), Math.max(0, index.lines.length - 1));
  return index.lines[at] ?? 1;
}

/** 文件行号 → 显示行下标（二分：行号单调递增；找不到时退到最近的行）。 */
export function lineToRow(index: RowIndex, line: number): number {
  const values = index.lines;
  if (values.length === 0) return 0;
  let low = 0;
  let high = values.length - 1;
  let best = 0;
  while (low <= high) {
    const mid = (low + high) >> 1;
    if (values[mid] === line) {
      // 折叠行与紧随其后的行号相同：定位到真正的内容行，不落在折叠行上。
      const row = index.rows[mid];
      if (row && "fold" in row && mid + 1 < values.length) return mid + 1;
      return mid;
    }
    if (values[mid] < line) {
      best = mid;
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }
  return best;
}
