import assert from "node:assert/strict";
import test from "node:test";
import { visibleHighlightLines } from "../src/features/diff/visibleHighlightLines.ts";

function unified(lineKind, oldLine, newLine) {
  return {
    kind: "line", hunkIndex: 0, section: null, oldLine, newLine, lineKind,
    text: "const value = 1;", left: null, right: null,
  };
}

function split(oldLine, newLine) {
  return {
    ...unified(null, oldLine, newLine), kind: "pair", text: null,
    left: oldLine == null ? null : { line: oldLine, kind: "deletion", text: "const oldValue = 1;" },
    right: newLine == null ? null : { line: newLine, kind: "addition", text: "const newValue = 2;" },
  };
}

function hunk(oldLine, newLine) {
  return { ...unified(null, oldLine, newLine), kind: "hunk", section: "@@", text: null };
}

function collect(rows, view, from = 0, to = rows.length) {
  return visibleHighlightLines((index) => rows[index] ?? null, from, to, view);
}

// Pure row selection only: no DOM, IPC, renderer, or replacement cache implementation.
test("split keeps a large insertion offset separate for the two versions", () => {
  assert.deepEqual(collect([split(100, 30100)], "split"), { new: [30100], old: [100] });
});

test("unified deletion and addition keep their own version and line", () => {
  const rows = [unified("deletion", 100, null), unified("addition", null, 30100)];
  assert.deepEqual(collect(rows, "unified"), { new: [30100], old: [100] });
});

test("unified context uses only old tokens, including the renderer's null-kind fallback", () => {
  const rows = [unified("context", 100, 30100), unified(null, 101, 30101)];
  assert.deepEqual(collect(rows, "unified"), { new: [], old: [100, 101] });
});

for (const view of ["split", "unified"]) {
  test(`${view} pure additions do not request a nonexistent old side`, () => {
    const rows = view === "split"
      ? [split(null, 30100), split(null, 30101)]
      : [unified("addition", null, 30100), unified("addition", null, 30101)];
    assert.deepEqual(collect(rows, view), { new: [30100, 30101], old: [] });
  });

  test(`${view} pure deletions do not request a nonexistent new side`, () => {
    const rows = view === "split"
      ? [split(30100, null), split(30101, null)]
      : [unified("deletion", 30100, null), unified("deletion", 30101, null)];
    assert.deepEqual(collect(rows, view), { new: [], old: [30100, 30101] });
  });

  test(`${view} keeps distant visible hunks as discrete lines, not a continuous range`, () => {
    const rows = view === "split"
      ? [hunk(100, 100), split(100, 100), hunk(30100, 30100), split(30100, 30100)]
      : [hunk(100, 100), unified("deletion", 100, null), unified("addition", null, 100),
        hunk(30100, 30100), unified("deletion", 30100, null), unified("addition", null, 30100)];
    assert.deepEqual(collect(rows, view), { new: [100, 30100], old: [100, 30100] });
  });
}

test("both views ignore missing rows, numbered hunk headers, and no-EOL rows without line numbers", () => {
  for (const view of ["split", "unified"]) {
    const noEol = view === "split" ? split(null, null) : unified("noEol", null, null);
    assert.deepEqual(collect([null, hunk(100, 30100), noEol], view), { new: [], old: [] });
  }
});

test("only the half-open visible row window is read", () => {
  for (const view of ["split", "unified"]) {
    const rows = view === "split"
      ? [split(1, 1), split(100, null), split(null, 30100), split(60000, 60000)]
      : [unified("context", 1, 1), unified("deletion", 100, null),
        unified("addition", null, 30100), unified("context", 60000, 60000)];
    const read = [];
    const lines = visibleHighlightLines((index) => {
      read.push(index);
      return rows[index] ?? null;
    }, 1, 3, view);
    assert.deepEqual(read, [1, 2]);
    assert.deepEqual(lines, { new: [30100], old: [100] });
  }
});

test("an empty visible window requests neither version and reads no rows", () => {
  for (const view of ["split", "unified"]) {
    const lines = visibleHighlightLines(() => {
      throw new Error("An empty window must not read a row");
    }, 4, 4, view);
    assert.deepEqual(lines, { new: [], old: [] });
  }
});

test("split reads the actual cells, not duplicated row-level line metadata", () => {
  const row = { ...split(100, 30100), oldLine: 1, newLine: 60000 };
  assert.deepEqual(collect([row], "split"), { new: [30100], old: [100] });
});
