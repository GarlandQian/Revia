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
const model = await import("../src/features/merge/mergeModel.ts");
imports.deregister();

const {
  applySide, chunkContent, hasConflictMarkers, hasPendingNonConflicts, ignoreSide, initialStates,
  isResolved, lineStarts, mergeTexts, sliceLines, syncSegments, unresolvedConflicts,
} = model;

// 共同祖先 a b c d；你的版本改 b，对方改 d；另有一处冲突在 c。
const texts = mergeTexts("a\nB-ours\nc-ours\nd\n", "a\nb\nc\nd\n", "a\nb\nc-theirs\nD\n");
const chunk = (kind, base, ours, theirs, result, applied = [false, false]) => ({
  id: 0, kind,
  baseStart: base[0], baseEnd: base[1],
  oursStart: ours[0], oursEnd: ours[1],
  theirsStart: theirs[0], theirsEnd: theirs[1],
  resultStart: result[0], resultEnd: result[1],
  oursApplied: applied[0], theirsApplied: applied[1],
});

test("行起点与按行切片保留行尾，没有末尾换行时也对", () => {
  assert.deepEqual(lineStarts("a\nb\n"), [0, 2, 4]);
  assert.deepEqual(lineStarts("a\nb"), [0, 2, 3]);
  assert.deepEqual(lineStarts(""), [0]);
  const text = "x\ny\nz";
  assert.equal(sliceLines(text, lineStarts(text), 1, 3), "y\nz");
  assert.equal(sliceLines(text, lineStarts(text), 2, 2), "");
});

test("初始状态：自动应用过的一侧算已放进结果，冲突两侧待处理", () => {
  const chunks = [
    chunk("ours", [1, 2], [1, 2], [1, 2], [1, 2], [true, false]),
    chunk("conflict", [2, 3], [2, 3], [2, 3], [2, 3]),
  ];
  const states = initialStates(chunks, "a\nB-ours\nc\nd\n");
  assert.equal(states[0].ours, "applied");
  assert.equal(states[0].theirs, "none");
  assert.deepEqual([states[0].from, states[0].to], [2, 9]);
  assert.equal(states[1].ours, "pending");
  assert.equal(unresolvedConflicts(states), 1);
  assert.equal(hasPendingNonConflicts(states), false);
});

test("冲突两侧都放进结果时左边在前，左边末行没换行时补一个", () => {
  const conflict = chunk("conflict", [2, 3], [2, 3], [2, 3], [2, 3]);
  let state = initialStates([conflict], "a\nb\nc\nd\n")[0];
  assert.equal(chunkContent(texts, conflict, state), "c\n", "没处理时是共同祖先");
  state = applySide(state, "theirs");
  assert.equal(chunkContent(texts, conflict, state), "c-theirs\n");
  state = applySide(state, "ours");
  assert.equal(chunkContent(texts, conflict, state), "c-ours\nc-theirs\n");
  assert.ok(isResolved(state));

  const noEol = mergeTexts("x", "", "y\n");
  const added = chunk("conflict", [0, 0], [0, 1], [0, 1], [0, 0]);
  const both = applySide(applySide(initialStates([added], "")[0], "ours"), "theirs");
  assert.equal(chunkContent(noEol, added, both), "x\ny\n");
});

test("忽略一侧：结果不放这一侧；两侧都处理过才算解决", () => {
  const conflict = chunk("conflict", [2, 3], [2, 3], [2, 3], [2, 3]);
  let state = ignoreSide(initialStates([conflict], "a\nb\nc\nd\n")[0], "ours");
  assert.equal(isResolved(state), false);
  state = applySide(state, "theirs");
  assert.ok(isResolved(state));
  assert.equal(chunkContent(texts, conflict, state), "c-theirs\n");
});

test("两侧改得一样的块：放进任一侧就两侧都处理过", () => {
  const same = chunk("both", [1, 2], [1, 2], [1, 2], [1, 2]);
  const state = applySide(initialStates([same], "a\nb\nc\nd\n")[0], "theirs");
  assert.equal(state.ours, "applied");
  assert.equal(state.theirs, "applied");
});

test("同步段：结果行号在左，那一侧行号在右", () => {
  const chunks = [chunk("theirs", [3, 4], [3, 4], [3, 4], [3, 4])];
  assert.deepEqual(syncSegments(chunks, [[3, 5]], "theirs"), [{ leftStart: 3, leftEnd: 5, rightStart: 3, rightEnd: 4 }]);
});

test("冲突标记检测只看行首的七个尖括号", () => {
  assert.ok(hasConflictMarkers("a\n<<<<<<< ours\nb\n"));
  assert.ok(hasConflictMarkers(">>>>>>>\n"));
  assert.equal(hasConflictMarkers("a <<<<<<< b\n"), false);
});
