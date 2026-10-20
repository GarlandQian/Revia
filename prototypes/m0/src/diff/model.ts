/**
 * 渲染模型：把 Rust 解析出来的 hunk 变成可虚拟化的行。
 *
 * 这里只做“排版配对”，不重新计算差异：
 * - 统一视图：hunk 的行按原顺序一维排列；
 * - 并排视图：把相邻的删除块与新增块按行配对，缺哪边就留空。
 * 两种视图的差异划分完全相同，只是显示方式不同（ADR-0004）。
 */

import type { DiffLine, FileChange, Hunk } from "./types";

export type RowKind = "hunk" | "context" | "pair" | "noEol";

export interface DiffRow {
  /** 在展开后的行数组里的下标。 */
  index: number;
  kind: RowKind;
  /** 统一视图的行；并排视图为 null。 */
  line: DiffLine | null;
  /** 并排视图左侧（旧版本）行；统一视图为 null。 */
  left: DiffLine | null;
  /** 并排视图右侧（新版本）行；统一视图为 null。 */
  right: DiffLine | null;
  /** 该行的锚点行号：优先用旧行号，其次用新行号；hunk 头用 hunk 起始行号。 */
  anchor: number;
  hunk: Hunk;
  section: string;
}

export interface FileModel {
  file: FileChange;
  rows: DiffRow[];
  /** 内容最长的行字符数，用于横向滚动宽度。 */
  maxLineLength: number;
  /** 需要高亮的旧版本行数 / 新版本行数。 */
  oldLineCount: number;
  newLineCount: number;
}

const HUNK_SECTION_MAX = 120;

export function buildFileModel(file: FileChange): FileModel {
  const rows: DiffRow[] = [];
  let maxLineLength = 0;
  let oldLineCount = 0;
  let newLineCount = 0;

  for (const hunk of file.hunks) {
    const section = truncate(hunk.section || hunk.header, HUNK_SECTION_MAX);
    rows.push(makeHunkRow(rows.length, hunk, section));

    let index = 0;
    while (index < hunk.lines.length) {
      const line = hunk.lines[index];

      if (line.kind === "context" || line.kind === "noEol") {
        maxLineLength = Math.max(maxLineLength, line.text.length);
        if (line.oldLine != null) oldLineCount = Math.max(oldLineCount, hunk.oldStart + hunk.oldCount - 1);
        if (line.newLine != null) newLineCount = Math.max(newLineCount, hunk.newStart + hunk.newCount - 1);
        rows.push({
          index: rows.length,
          kind: line.kind === "noEol" ? "noEol" : "context",
          line,
          left: null,
          right: null,
          anchor: line.oldLine ?? line.newLine ?? 0,
          hunk,
          section,
        });
        index += 1;
        continue;
      }

      // 收集相邻的删除块与新增块，再按行配对。
      const deletions: DiffLine[] = [];
      while (index < hunk.lines.length && hunk.lines[index].kind === "deletion") {
        deletions.push(hunk.lines[index]);
        index += 1;
      }
      const additions: DiffLine[] = [];
      while (index < hunk.lines.length && hunk.lines[index].kind === "addition") {
        additions.push(hunk.lines[index]);
        index += 1;
      }

      for (const line of deletions) {
        if (line.oldLine != null) oldLineCount = Math.max(oldLineCount, line.oldLine);
        maxLineLength = Math.max(maxLineLength, line.text.length);
      }
      for (const line of additions) {
        if (line.newLine != null) newLineCount = Math.max(newLineCount, line.newLine);
        maxLineLength = Math.max(maxLineLength, line.text.length);
      }

      const count = Math.max(deletions.length, additions.length);
      for (let pairIndex = 0; pairIndex < count; pairIndex += 1) {
        const left = deletions[pairIndex] ?? null;
        const right = additions[pairIndex] ?? null;
        rows.push({
          index: rows.length,
          kind: "pair",
          line: null,
          left,
          right,
          anchor: left?.oldLine ?? right?.newLine ?? 0,
          hunk,
          section,
        });
      }
    }
  }

  return {
    file,
    rows,
    maxLineLength,
    oldLineCount,
    newLineCount,
  };
}

function makeHunkRow(index: number, hunk: Hunk, section: string): DiffRow {
  return {
    index,
    kind: "hunk",
    line: null,
    left: null,
    right: null,
    anchor: hunk.newStart,
    hunk,
    section,
  };
}

export function rowCount(model: FileModel): number {
  return model.rows.length;
}

/** 视图切换用：取滚动位置对应的可锚定行（跳过 hunk 头）。 */
export function rowAt(model: FileModel, scrollTop: number, rowHeight: number): DiffRow | null {
  if (model.rows.length === 0) return null;
  const index = Math.min(Math.max(Math.floor(scrollTop / rowHeight), 0), model.rows.length - 1);
  for (let cursor = index; cursor < model.rows.length; cursor += 1) {
    if (model.rows[cursor].kind !== "hunk") return model.rows[cursor];
  }
  for (let cursor = index; cursor >= 0; cursor -= 1) {
    if (model.rows[cursor].kind !== "hunk") return model.rows[cursor];
  }
  return null;
}

/** 一行的展示用行号（旧行号优先）。 */
export function lineNumberOf(row: DiffRow): number | null {
  return row.left?.oldLine ?? row.line?.oldLine ?? row.right?.newLine ?? row.line?.newLine ?? null;
}

/** 在目标视图里找到承载同一行号的行，返回它的下标。 */
export function findRowForAnchor(model: FileModel, anchor: DiffRow): number {
  const wantOld = anchor.left?.oldLine ?? anchor.line?.oldLine ?? null;
  const wantNew = anchor.right?.newLine ?? anchor.line?.newLine ?? null;

  for (const row of model.rows) {
    if (row.kind === "hunk") continue;
    const oldLine = row.left?.oldLine ?? row.line?.oldLine ?? null;
    const newLine = row.right?.newLine ?? row.line?.newLine ?? null;
    if (wantOld != null && oldLine === wantOld) return row.index;
    if (wantNew != null && newLine === wantNew) return row.index;
  }

  // 退化情况：按锚点行号就近取。
  let best = 0;
  let bestDistance = Number.POSITIVE_INFINITY;
  for (const row of model.rows) {
    if (row.kind === "hunk") continue;
    const distance = Math.abs(row.anchor - (wantOld ?? wantNew ?? 0));
    if (distance < bestDistance) {
      bestDistance = distance;
      best = row.index;
    }
  }
  return best;
}

function truncate(text: string, max: number): string {
  return text.length <= max ? text : `${text.slice(0, max)}…`;
}
