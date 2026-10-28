import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";

const sourceRoot = new URL("../src/", import.meta.url);
const imports = registerHooks({
  resolve(specifier, context, nextResolve) {
    let source;
    if (specifier.startsWith("@/")) {
      source = new URL(specifier.slice(2), sourceRoot);
    } else if ((specifier.startsWith("./") || specifier.startsWith("../"))
      && context.parentURL?.startsWith(sourceRoot.href)) {
      source = new URL(specifier, context.parentURL);
    }
    if (source) {
      if (!/\.[^/]+$/.test(source.pathname)) source.pathname += ".ts";
      return nextResolve(source.href, context);
    }
    return nextResolve(specifier, context);
  },
});
const { mapLine, clampLine } = await import("../src/features/diff/syncScroll.ts");
const { buildRows, lineToRow, rowToLine } = await import("../src/features/diff/rowIndex.ts");
const { RowHeights } = await import("../src/features/diff/rowHeights.ts");
const { filePositionAtY, yAtFilePosition, syncTargetTop, syncPositionError } = await import("../src/features/diff/sideGeometry.ts");
const { unifiedFileAnchor, sideFileAnchor, sameAnchorTarget } = await import("../src/features/diff/fileAnchor.ts");
imports.deregister();

test("快速切回只复用同一文件、选择代际、快照和显示选项的待恢复锚点", () => {
  const target = { worktreeId: "a", entry: { path: "src/a.ts", changes: "unstaged",
    fingerprint: "f1", snapshotRevision: "r1" }, selectionEpoch: 2, activation: 3, optionsKey: "off" };
  const pending = { ...target, line: 240, version: "new", view: "unified" };
  assert.equal(sameAnchorTarget(pending, { ...target, entry: { ...target.entry } }), true);
  for (const patch of [{ worktreeId: "b" }, { selectionEpoch: 4 }, { activation: 5 }, { optionsKey: "all" }]) {
    assert.equal(sameAnchorTarget(pending, { ...target, ...patch }), false, JSON.stringify(patch));
  }
  for (const patch of [{ path: "src/b.ts" }, { changes: "staged" }, { fingerprint: "f2" }, { snapshotRevision: "r2" }]) {
    assert.equal(sameAnchorTarget(pending, { ...target, entry: { ...target.entry, ...patch } }), false, JSON.stringify(patch));
  }
});

test("视图切换锚点跳过标题与缺行，并让文件行号和版本成对", () => {
  const rows = [null, { kind: "hunk" },
    { kind: "line", lineKind: "context", oldLine: 29, newLine: 31 },
    { kind: "line", lineKind: "deletion", oldLine: 30, newLine: null },
    { kind: "line", lineKind: "addition", oldLine: null, newLine: 32 }];
  const get = (index) => rows[index] ?? null;
  assert.deepEqual(unifiedFileAnchor(get, 0, 5), { line: 31, version: "new" });
  assert.deepEqual(unifiedFileAnchor(get, 3, 5), { line: 30, version: "old" });
  assert.deepEqual(unifiedFileAnchor(get, 4, 5), { line: 32, version: "new" });
  assert.equal(unifiedFileAnchor(get, 0, 2), null);
});

test("软换行锚点使用实际逻辑行而不是固定行高", () => {
  const rows = buildRows(100, [], "right", new Set());
  const heights = new RowHeights(100, 19, true);
  heights.measure(0, 95);
  const top = heights.top(29) + 7;
  assert.deepEqual(sideFileAnchor("right", rows, heights, top), { line: 30, version: "new" });
  assert.deepEqual(unifiedFileAnchor((index) => ({ kind: "line", lineKind: "context",
    oldLine: index + 1, newLine: index + 4 }), heights.rowAt(top), 40), { line: 33, version: "new" });
});

test("并排锚点保留主动侧版本并支持折叠、空侧与末尾", () => {
  const folds = [{ leftStart: 3, leftEnd: 20, rightStart: 5, rightEnd: 22 }];
  const left = buildRows(50, folds, "left", new Set());
  const right = buildRows(52, folds, "right", new Set());
  const leftHeights = new RowHeights(left.rows.length, 19, false);
  const rightHeights = new RowHeights(right.rows.length, 19, false);
  assert.deepEqual(sideFileAnchor("left", left, leftHeights, 3 * 19), { line: 4, version: "old" });
  assert.deepEqual(sideFileAnchor("right", right, rightHeights, 5 * 19), { line: 6, version: "new" });
  const expanded = buildRows(52, folds, "right", new Set([0]));
  const expandedHeights = new RowHeights(expanded.rows.length, 19, false);
  const target = yAtFilePosition(expanded, expandedHeights, 14);
  assert.deepEqual(sideFileAnchor("right", expanded, expandedHeights, target), { line: 15, version: "new" });
  assert.deepEqual(sideFileAnchor("left", left, leftHeights, leftHeights.total()), { line: 50, version: "old" });
  assert.equal(sideFileAnchor("left", buildRows(0, [], "left", new Set()), new RowHeights(0, 19, false), 0), null);
});

// 两侧各 100 行；一段修改（左侧 10..12 对右侧 10..14）、一段纯新增（右侧 50..52）、
// 一段纯删除（左侧 70..72）。
const SEGMENTS = [
  { leftStart: 10, leftEnd: 12, rightStart: 10, rightEnd: 14 },
  { leftStart: 50, leftEnd: 50, rightStart: 50, rightEnd: 52 },
  { leftStart: 70, leftEnd: 72, rightStart: 70, rightEnd: 70 },
];

test("行在所有段之前时原样返回", () => {
  assert.equal(mapLine("left", 0, SEGMENTS), 0);
  assert.equal(mapLine("left", 9.5, SEGMENTS), 9.5);
  assert.equal(mapLine("right", 4, SEGMENTS), 4);
  assert.equal(mapLine("left", 5, []), 5, "没有段时原样返回");
});

test("段之后按两侧 end 的差值平移", () => {
  // 第一段之后：左侧多 0 行、右侧多 2 行 → 左侧 20 对应右侧 22。
  assert.equal(mapLine("left", 20, SEGMENTS), 22);
  assert.equal(mapLine("right", 22, SEGMENTS), 20);
  // 第二段（纯新增 2 行）之后：左侧 60 → 右侧 62。
  assert.equal(mapLine("left", 60, SEGMENTS), 62);
  assert.equal(mapLine("right", 62, SEGMENTS), 60);
});

test("段内按比例换算", () => {
  // 第一段：左侧 2 行 → 右侧 4 行。
  assert.equal(mapLine("left", 10, SEGMENTS), 10);
  assert.equal(mapLine("left", 11, SEGMENTS), 12);
  assert.equal(mapLine("right", 12, SEGMENTS), 11);
  assert.equal(mapLine("left", 11.5, SEGMENTS), 13);
});

test("零长度段映射到该段的起点", () => {
  // 纯新增段：左侧没有行，左侧 50（段首）对应右侧段首 50。
  assert.equal(mapLine("left", 50, SEGMENTS), 50);
  // 纯删除段：右侧没有行，右侧 70 对应左侧段首 70。
  assert.equal(mapLine("right", 70, SEGMENTS), 70);
});

test("左右互逆误差不超过 1 行（两侧都非空的段）", () => {
  // 纯新增 / 纯删除段的一侧长度为 0，插入点附近本来不可逆（多行挤在一个点），
  // 因此互逆性只在两侧都有行的段上要求。
  const balanced = [
    { leftStart: 10, leftEnd: 12, rightStart: 10, rightEnd: 14 },
    { leftStart: 40, leftEnd: 46, rightStart: 42, rightEnd: 44 },
    { leftStart: 70, leftEnd: 71, rightStart: 68, rightEnd: 72 },
  ];
  for (let line = 0; line <= 100; line += 0.5) {
    const right = mapLine("left", line, balanced);
    const back = mapLine("right", right, balanced);
    assert.ok(Math.abs(back - line) <= 1, `left ${line} → ${right} → ${back}`);
    const left = mapLine("right", line, balanced);
    const forth = mapLine("left", left, balanced);
    assert.ok(Math.abs(forth - line) <= 1, `right ${line} → ${left} → ${forth}`);
  }
});

test("首尾夹紧", () => {
  assert.equal(clampLine(-5, 10), 0);
  assert.equal(clampLine(20, 10), 10);
  assert.equal(clampLine(3.5, 10), 3.5);
  assert.equal(clampLine(Number.NaN, 10), 0);
  assert.equal(clampLine(5, -1), 0);
});

// ---------- 显示行索引（折叠未改动部分） ----------

const FOLDS = [
  { leftStart: 9, leftEnd: 19, rightStart: 9, rightEnd: 21 },
  { leftStart: 39, leftEnd: 49, rightStart: 41, rightEnd: 51 },
];

test("没有折叠时显示行与文件行一一对应", () => {
  const index = buildRows(60, [], "left", new Set());
  assert.equal(index.rows.length, 60);
  assert.equal(rowToLine(index, 0), 1);
  assert.equal(rowToLine(index, 59), 60);
  assert.equal(lineToRow(index, 30), 29);
});

test("折叠区在显示行里各占一行，行号换算可逆", () => {
  const index = buildRows(60, FOLDS, "left", new Set());
  // 1..9 行 + 两个折叠行 + 20..39 + 折叠 + 50..60
  assert.equal(index.rows.length, 9 + 1 + 20 + 1 + 11);
  // 折叠区覆盖 1 基的 10..19 行，折叠行给出折叠区之后的第一行（20）。
  assert.equal(rowToLine(index, 9), 20, "折叠行给出折叠区之后的第一行");
  assert.equal(lineToRow(index, 20), 10);
  assert.equal(rowToLine(index, 10), 20, "折叠之后回到第 20 行");
  // 两侧折叠区不同长度时按各自一栏换算。
  const right = buildRows(62, FOLDS, "right", new Set());
  // 右侧折叠区覆盖 1 基的 10..21 行（比左侧长两行），折叠行给出 22。
  assert.equal(rowToLine(right, 9), 22);
  assert.equal(right.lines[10], 22);
  assert.equal(lineToRow(right, 22), 10);
});

test("展开的折叠区不再占用显示行", () => {
  const index = buildRows(60, FOLDS, "left", new Set([0]));
  assert.equal(index.rows.length, 60 - 10 + 1);
  assert.equal(lineToRow(index, 10), 9, "展开后第 10 行仍在第 10 个显示行");
  assert.equal(rowToLine(index, 9), 10);
});

test("二分换算在段边界与越界时退到最近的行", () => {
  const index = buildRows(20, [], "left", new Set());
  assert.equal(lineToRow(index, 1), 0);
  assert.equal(lineToRow(index, 20), 19);
  assert.equal(lineToRow(index, 100), 19, "超出末尾退到最后一行");
  assert.equal(lineToRow(index, 0), 0);
  assert.equal(rowToLine(index, -3), 1);
  assert.equal(rowToLine(index, 999), 20);
  const empty = buildRows(0, [], "left", new Set());
  assert.equal(empty.rows.length, 0);
  assert.equal(rowToLine(empty, 5), 1);
  assert.equal(lineToRow(empty, 5), 0);
});

// ---------- 软换行后的像素、文件位置与同步 ----------

const UNIT = 19;

function geometry(lineCount, side = "left", folds = [], expanded = new Set(), viewportHeight = UNIT) {
  const rows = buildRows(lineCount, folds, side, expanded);
  return { rows, heights: new RowHeights(rows.rows.length, UNIT, true), viewportHeight };
}

test("像素换算保留行内比例，文件末尾插入点在末行之后", () => {
  const { rows, heights } = geometry(10);
  heights.measure(9, 3 * UNIT);
  for (const position of [0, 0.5, 5, 8.75, 9, 9.25, 9.5, 10]) {
    assert.equal(filePositionAtY(rows, heights, yAtFilePosition(rows, heights, position)), position);
  }
  assert.equal(yAtFilePosition(rows, heights, 10), 12 * UNIT);
  assert.equal(yAtFilePosition(rows, heights, Infinity), 12 * UNIT);
  assert.equal(filePositionAtY(rows, heights, Infinity), 10);
  assert.equal(yAtFilePosition(rows, heights, -1), 0);
  assert.equal(yAtFilePosition(rows, heights, Number.NaN), 0);
});

test("同步映射统一为零基，修改段边界前后保留小数", () => {
  const left = geometry(30);
  const right = geometry(32, "right");
  const segments = [{ leftStart: 10, leftEnd: 12, rightStart: 10, rightEnd: 14 }];
  for (const [position, mapped] of [[9.5, 9.5], [10, 10], [10.5, 11], [11.5, 13], [12, 14], [12.5, 14.5]]) {
    const sourceTop = position * UNIT - UNIT / 2;
    const targetTop = mapped * UNIT - UNIT / 2;
    assert.equal(syncTargetTop("left", sourceTop, left, right, segments), targetTop);
    assert.equal(syncTargetTop("right", targetTop, right, left, segments), sourceTop);
  }
});

test("左右同一行分别折成两行和四行时按比例同步，使用各自视口高度", () => {
  const left = geometry(20, "left", [], new Set(), 2 * UNIT);
  const right = geometry(20, "right");
  left.heights.measure(5, 2 * UNIT);
  right.heights.measure(5, 4 * UNIT);
  const sourceTop = 5 * UNIT;
  const targetTop = 6.5 * UNIT;
  assert.equal(syncTargetTop("left", sourceTop, left, right, []), targetTop);
  assert.equal(syncPositionError("left", sourceTop, targetTop, left, right, []), 0);
  assert.equal(syncPositionError("left", sourceTop, targetTop + UNIT, left, right, []), 0.25);
});

test("折叠占位按同一个 fold id 同步，不落到后面的正文行", () => {
  const left = geometry(60, "left", FOLDS);
  const right = geometry(62, "right", FOLDS);
  const top = 9 * UNIT;
  assert.equal(left.rows.foldRows.get(0), 9);
  assert.equal(right.rows.foldRows.get(0), 9);
  assert.equal(filePositionAtY(left.rows, left.heights, top + UNIT / 2), 14);
  assert.equal(filePositionAtY(right.rows, right.heights, top + UNIT / 2), 15);
  assert.equal(syncTargetTop("left", top, left, right, []), top);
  assert.equal(syncTargetTop("right", top, right, left, []), top);
  assert.equal(syncPositionError("left", top, top, left, right, []), 0);
});

test("展开折叠区仍恢复相同的逻辑锚点，首尾折叠正确覆盖文件", () => {
  const folded = geometry(60, "left", FOLDS);
  const expanded = geometry(60, "left", FOLDS, new Set([0]));
  const anchor = filePositionAtY(folded.rows, folded.heights, 9.5 * UNIT);
  const y = yAtFilePosition(expanded.rows, expanded.heights, anchor);
  assert.equal(y, 14 * UNIT);
  assert.equal(filePositionAtY(expanded.rows, expanded.heights, y), anchor);
  assert.equal(yAtFilePosition(folded.rows, folded.heights, anchor), 9.5 * UNIT);

  const edges = [
    { leftStart: 0, leftEnd: 5, rightStart: 0, rightEnd: 5 },
    { leftStart: 15, leftEnd: 20, rightStart: 15, rightEnd: 20 },
  ];
  const { rows, heights } = geometry(20, "left", edges);
  assert.equal(rows.rows.length, 12);
  assert.equal(rows.foldRows.get(0), 0);
  assert.equal(rows.foldRows.get(1), 11);
  assert.equal(yAtFilePosition(rows, heights, 2.5), UNIT / 2);
  assert.equal(yAtFilePosition(rows, heights, 5), UNIT);
  assert.equal(yAtFilePosition(rows, heights, 15), 11 * UNIT);
  assert.equal(yAtFilePosition(rows, heights, 20), 12 * UNIT);
  assert.equal(filePositionAtY(rows, heights, 11.5 * UNIT), 17.5);
});

test("纯新增和纯删除映射到插入点，空侧和 EOF 不虚构一行", () => {
  const left = geometry(10);
  const right = geometry(12, "right");
  const segments = [{ leftStart: 2, leftEnd: 2, rightStart: 2, rightEnd: 4 }];
  assert.equal(syncTargetTop("right", 3 * UNIT, right, left, segments), 1.5 * UNIT);
  assert.equal(syncTargetTop("left", 1.5 * UNIT, left, right, segments), 1.5 * UNIT);
  assert.equal(syncTargetTop("left", 2 * UNIT, left, right, segments), 4 * UNIT);

  const empty = geometry(0);
  const added = geometry(5, "right");
  const allAdded = [{ leftStart: 0, leftEnd: 0, rightStart: 0, rightEnd: 5 }];
  assert.equal(filePositionAtY(empty.rows, empty.heights, 100), 0);
  assert.equal(yAtFilePosition(empty.rows, empty.heights, 100), 0);
  assert.equal(syncTargetTop("left", 0, empty, added, allAdded), 0);
  assert.equal(syncTargetTop("right", 2 * UNIT, added, empty, allAdded), 0);
  assert.equal(syncPositionError("right", 2 * UNIT, 0, added, empty, allAdded), 0);
});

test("同步误差对照夹紧后的可达目标，不把文件首尾限制算成失败", () => {
  const left = geometry(20, "left", [], new Set(), 4 * UNIT);
  const right = geometry(15, "right", [], new Set(), 4 * UNIT);
  const segments = [{ leftStart: 0, leftEnd: 5, rightStart: 0, rightEnd: 0 }];
  assert.equal(syncTargetTop("left", 0, left, right, segments), 0);
  assert.equal(syncPositionError("left", 0, 0, left, right, segments), 0);
  assert.equal(syncPositionError("left", 0, UNIT, left, right, segments), 1);
  const bottom = right.heights.total() - right.viewportHeight;
  const sourceBottom = left.heights.total() - left.viewportHeight;
  assert.equal(syncTargetTop("left", sourceBottom, left, right, segments), bottom);
  assert.equal(syncPositionError("left", sourceBottom, bottom, left, right, segments), 0);
});
