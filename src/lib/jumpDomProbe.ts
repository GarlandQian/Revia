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
  canvasAligned: boolean;
  canvasReady: boolean;
  placeholderRows: number;
  placeholderExposed: boolean;
}

export interface JumpCanvasStyle {
  ready: boolean;
  viewportWidth: number;
  viewportHeight: number;
  backgroundColor: string | null;
  backgroundImage: string | null;
  backgroundSize: string | null;
  backgroundRepeat: string | null;
}

export interface JumpDomPreparation {
  style: JumpCanvasStyle;
  matches: (root: HTMLElement | null, canvas: HTMLElement | null) => boolean;
}

interface ProbeOptions {
  root: HTMLElement | null;
  view: ViewMode;
  from: number;
  to: number;
  rowHeight: number;
  getRow: (index: number) => RenderRow | null;
  highlights: Pick<HighlightStore, "get" | "categoryClass">;
  preparation?: JumpDomPreparation;
}

interface ExpectedCell {
  text: string;
  kind: LineKind;
  tokens: ReturnType<HighlightStore["get"]>;
}

function findCanvas(root: HTMLElement | null, view: ViewMode): HTMLElement | null {
  const canvases = root?.matches(".diff-scroll") && root.isConnected
    ? Array.from(root.children).filter((child) => child.matches(`.diff-canvas.diff-canvas-${view}`))
    : [];
  return canvases.length === 1 && canvases[0] instanceof HTMLElement ? canvases[0] : null;
}

function opaqueColor(color: string): boolean {
  if (!color || color === "transparent") return false;
  const alpha = color.match(/\/\s*([\d.]+)(%)?\s*\)$/)
    ?? color.match(/^rgba\([^,]+,[^,]+,[^,]+,\s*([\d.]+)(%)?\s*\)$/);
  return alpha == null || Number(alpha[1]) >= (alpha[2] ? 100 : 1);
}

function nonScrollStyle(canvas: HTMLElement): string {
  return Array.from(canvas.style).filter((name) => name !== "transform")
    .map((name) => `${name}:${canvas.style.getPropertyValue(name)}`).join(";");
}

/** 样式与几何只在计时前读取；计时内只核对同一 canvas、位移和实际缺席的行节点。 */
export function prepareJumpDom(root: HTMLElement | null, view: ViewMode, rowHeight: number): JumpDomPreparation {
  const canvas = findCanvas(root, view);
  const style: JumpCanvasStyle = {
    ready: false, viewportWidth: 0, viewportHeight: 0,
    backgroundColor: null, backgroundImage: null, backgroundSize: null, backgroundRepeat: null,
  };
  if (!root || !canvas) return { style, matches: () => false };

  const bounds = root.getBoundingClientRect();
  const canvasStyle = getComputedStyle(canvas);
  const rootStyle = getComputedStyle(root);
  const row = canvas.querySelector<HTMLElement>(".row");
  const rowStyle = row ? getComputedStyle(row) : null;
  const ancestors: Array<{ element: HTMLElement; classes: string; style: string | null }> = [];
  let visible = true;
  for (let element: HTMLElement | null = canvas; element; element = element.parentElement) {
    const computed = getComputedStyle(element);
    visible &&= computed.display !== "none" && computed.visibility === "visible"
      && Number(computed.opacity) > 0 && computed.contentVisibility !== "hidden";
    if (element !== canvas) {
      ancestors.push({ element, classes: element.className, style: element.getAttribute("style") });
    }
  }
  style.viewportWidth = bounds.width;
  style.viewportHeight = bounds.height;
  style.backgroundColor = canvasStyle.backgroundColor;
  style.backgroundImage = canvasStyle.backgroundImage;
  style.backgroundSize = canvasStyle.backgroundSize;
  style.backgroundRepeat = canvasStyle.backgroundRepeat;
  style.ready = visible && bounds.width > 0 && bounds.height > 0
    && bounds.top < window.innerHeight && bounds.left < window.innerWidth && bounds.bottom > 0 && bounds.right > 0
    && canvasStyle.position === "relative" && rootStyle.overflowY === "hidden"
    && rowStyle?.position === "absolute" && parseFloat(rowStyle.height) === rowHeight
    && rowStyle.transform === "none" && rowStyle.visibility === "visible" && rowStyle.display !== "none"
    && opaqueColor(canvasStyle.backgroundColor) && canvasStyle.backgroundColor !== rootStyle.backgroundColor
    && (canvasStyle.backgroundImage.match(/linear-gradient\(/g)?.length ?? 0) >= 2
    && canvasStyle.backgroundSize.split(",").every((size) => size.trim() === `100% ${rowHeight}px`)
    && canvasStyle.backgroundRepeat.split(",").every((repeat) => ["repeat-y", "no-repeat repeat"].includes(repeat.trim()));
  const classes = canvas.className;
  const fixedStyle = nonScrollStyle(canvas);
  return {
    style,
    matches: (currentRoot, currentCanvas) => style.ready && currentRoot === root && currentCanvas === canvas
      && canvas.isConnected && canvas.className === classes && nonScrollStyle(canvas) === fixedStyle
      && ancestors.every((entry) => entry.element.isConnected && entry.element.className === entry.classes
        && entry.element.getAttribute("style") === entry.style),
  };
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
  if (cell.tokens === null) return cell.text === "";
  return cell.tokens !== undefined && cell.tokens.map((token) => token.text).join("") === cell.text;
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
  if (!hasCachedTokens(expected)) return false;
  // Rust returns null for a cached empty line; the renderer keeps only its marker.
  if (tokens === null) return element.childNodes.length === 1;
  if (!tokens || element.childNodes.length !== tokens.length + 1) return false;
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
    canvasAligned: false, canvasReady: false, placeholderRows: 0, placeholderExposed: false,
  };
  if (!validRange) return result;

  const canvas = findCanvas(root, view);
  const translation = canvas?.style.transform.match(/^translateY\((-?(?:\d+(?:\.\d+)?|\.\d+))px\)$/);
  const aligned = Number.isFinite(rowHeight) && rowHeight > 0 && translation != null
    && Number(translation[1]) === -from * rowHeight;
  result.canvasAligned = aligned;
  const nodes = new Map<number, Element | null>();
  let validNodes = true;
  if (canvas && aligned) {
    for (const child of canvas.children) {
      if (!(child instanceof HTMLElement) || !child.classList.contains("row")) { validNodes = false; continue; }
      const top = child.style.top.match(/^(-?(?:\d+(?:\.\d+)?|\.\d+))px$/);
      if (!top) { validNodes = false; continue; }
      const index = Number(top[1]) / rowHeight;
      if (!Number.isSafeInteger(index) || index < 0) { validNodes = false; continue; }
      if (nodes.has(index) || child.style.transform || child.style.height || child.style.display === "none"
        || child.style.visibility === "hidden" || child.style.opacity === "0") validNodes = false;
      nodes.set(index, nodes.has(index) ? null : child);
    }
  }
  result.canvasReady = aligned && validNodes && canvas != null
    && parseFloat(canvas.style.height) >= to * rowHeight && !!options.preparation?.matches(root, canvas);

  let allCached = true;
  for (let index = from; index < to; index += 1) {
    if (result.canvasReady && !nodes.has(index)) result.placeholderRows += 1;
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
      const section = node.firstElementChild;
      if (node.classList.contains("row-hunk") && section?.matches("span.hunk-section")
        && section.textContent === (row.section ?? "")) {
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
  result.placeholderExposed = result.canvasReady && result.placeholderRows > 0
    && result.matchedRows + result.placeholderRows === result.requiredRows;
  return result;
}
