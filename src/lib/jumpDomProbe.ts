import type { HighlightStore } from "@/features/diff/highlightStore";
import type { LineKind } from "@/types/bindings/LineKind";
import type { RenderRow } from "@/types/bindings/RenderRow";
import type { ViewMode } from "@/types/bindings/ViewMode";

export interface JumpDomProbe {
  textMatched: boolean;
  matchedRows: number;
  requiredRows: number;
  colorsCached: boolean;
  colorsMatched: boolean;
  coloredCells: number;
  expectedColoredCells: number;
}

interface ProbeOptions {
  root: HTMLElement | null;
  view: ViewMode;
  from: number;
  to: number;
  rowHeight: number;
  getRow: (index: number) => RenderRow | null;
  highlights: Pick<HighlightStore, "get" | "categoryClass">;
}

interface ExpectedCell {
  text: string;
  kind: LineKind;
  tokens: ReturnType<HighlightStore["get"]>;
}

function markerFor(kind: LineKind): string {
  return kind === "addition" ? "+" : kind === "deletion" ? "-" : " ";
}

function expectedCells(row: RenderRow, options: ProbeOptions): Array<ExpectedCell | null> {
  if (row.kind === "hunk") return [];
  if (options.view === "unified") {
    const added = row.lineKind === "addition";
    const line = added ? row.newLine : row.oldLine;
    return [{
      text: row.text ?? "",
      kind: row.lineKind ?? "context",
      tokens: line == null ? undefined : options.highlights.get(added ? "new" : "old", line),
    }];
  }
  return [row.left, row.right].map((cell, side) => cell == null ? null : ({
    text: cell.text,
    kind: cell.kind,
    tokens: options.highlights.get(side === 0 ? "old" : "new", cell.line),
  }));
}

function hasCachedTokens(cell: ExpectedCell): boolean {
  return cell.tokens != null && cell.tokens.map((token) => token.text).join("") === cell.text;
}

function cellTextMatches(element: Element | undefined, expected: ExpectedCell | null): boolean {
  if (!element?.classList.contains("cell")) return false;
  if (!expected) {
    return element.classList.contains("empty") && element.textContent === "" && element.childNodes.length === 0;
  }
  const marker = element.firstElementChild;
  return element.classList.contains(`cell-${expected.kind}`)
    && !element.classList.contains("empty")
    && marker?.tagName === "SPAN" && marker.className === "marker"
    && marker.textContent === markerFor(expected.kind)
    && element.textContent === markerFor(expected.kind) + expected.text;
}

function cellColorsMatch(element: Element, expected: ExpectedCell, options: ProbeOptions): boolean {
  const tokens = expected.tokens;
  if (!hasCachedTokens(expected) || !tokens || element.childNodes.length !== tokens.length + 1) return false;
  return tokens.every((token, index) => {
    const span = element.childNodes[index + 1];
    return span instanceof Element && span.tagName === "SPAN"
      && span.className === options.highlights.categoryClass(token.category)
      && span.textContent === token.text;
  });
}

/** 只读语义 DOM；不触发布局，也不将缓存齐备等同于画面已提交。 */
export function inspectJumpDom(options: ProbeOptions): JumpDomProbe {
  const { root, view, from, to, rowHeight, getRow } = options;
  const validRange = Number.isSafeInteger(from) && Number.isSafeInteger(to) && from >= 0 && to > from;
  const result: JumpDomProbe = {
    textMatched: false, matchedRows: 0, requiredRows: validRange ? to - from : 0,
    colorsCached: false, colorsMatched: false, coloredCells: 0, expectedColoredCells: 0,
  };
  if (!validRange) return result;

  const canvases = root?.matches(".diff-scroll") && root.isConnected
    ? Array.from(root.children).filter((child) => child.matches(`.diff-canvas.diff-canvas-${view}`))
    : [];
  const canvas = canvases.length === 1 && canvases[0] instanceof HTMLElement ? canvases[0] : null;
  const translation = canvas?.style.transform.match(/^translateY\((-?(?:\d+(?:\.\d+)?|\.\d+))px\)$/);
  const aligned = Number.isFinite(rowHeight) && rowHeight > 0 && translation != null
    && Number(translation[1]) === -from * rowHeight;
  const nodes = new Map<number, Element | null>();
  if (canvas && aligned) {
    for (const child of canvas.children) {
      if (!(child instanceof HTMLElement) || !child.classList.contains("row")) continue;
      const top = child.style.top.match(/^(-?(?:\d+(?:\.\d+)?|\.\d+))px$/);
      if (!top) continue;
      const index = Number(top[1]) / rowHeight;
      if (Number.isSafeInteger(index)) nodes.set(index, nodes.has(index) ? null : child);
    }
  }

  let allCached = true;
  for (let index = from; index < to; index += 1) {
    const row = getRow(index);
    if (!row) { allCached = false; continue; }
    const expected = expectedCells(row, options);
    // 缓存判定独立于当前位置和 DOM，供跳转前检查目标使用。
    for (const cell of expected) {
      if (!cell) continue;
      result.expectedColoredCells += 1;
      if (!hasCachedTokens(cell)) allCached = false;
    }
    const node = nodes.get(index);
    if (!node) continue;
    if (row.kind === "hunk") {
      if (node.classList.contains("row-hunk") && node.children.length === 0 && node.textContent === (row.section ?? "")) {
        result.matchedRows += 1;
      }
      continue;
    }
    if (!node.classList.contains(`row-${view}`)) continue;
    const children = Array.from(node.children);
    const cells = view === "split" ? [children[1], children[3]] : [children[2]];
    const gutterPositions = view === "split" ? [0, 2] : [0, 1];
    const lines = view === "split" ? [row.left?.line, row.right?.line] : [row.oldLine, row.newLine];
    const guttersMatch = gutterPositions.every((position, side) =>
      children[position]?.matches("span.gutter") && children[position].textContent === String(lines[side] ?? ""));
    if (children.length !== (view === "split" ? 4 : 3) || !guttersMatch) continue;
    const cellsMatch = expected.map((cell, side) => cellTextMatches(cells[side], cell));
    if (cellsMatch.every(Boolean)) result.matchedRows += 1;
    expected.forEach((cell, side) => {
      if (cell && cellsMatch[side] && cellColorsMatch(cells[side], cell, options)) result.coloredCells += 1;
    });
  }
  result.textMatched = result.matchedRows === result.requiredRows;
  result.colorsCached = allCached && result.expectedColoredCells > 0;
  result.colorsMatched = result.textMatched && result.colorsCached
    && result.coloredCells === result.expectedColoredCells;
  return result;
}
