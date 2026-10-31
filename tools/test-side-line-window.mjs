import assert from "node:assert/strict";
import test from "node:test";
import { setImmediate as nextTurn } from "node:timers/promises";
import { SideLineWindow, lineRangesForRows } from "../src/features/diff/sideLineWindow.ts";
import { buildRows } from "../src/features/diff/rowIndex.ts";
import { RowHeights } from "../src/features/diff/rowHeights.ts";

function fixture() {
  const store = new SideLineWindow("snapshot-1");
  const calls = [];
  let current = true;
  let published = 0;
  const priorities = [];
  const loader = {
    load(side, from, to) {
      let resolve, reject;
      const promise = new Promise((accept, fail) => { resolve = accept; reject = fail; });
      calls.push({ side, from, to, resolve, reject });
      return promise;
    },
    isCurrent: () => current,
    publish: (visibleChanged) => { published += 1; priorities.push(visibleChanged); },
  };
  const window = { visible: [{ from: 1, to: 32 }], buffered: [{ from: 1, to: 96 }], lineCount: 1000 };
  return { store, calls, loader, window, priorities, retire: () => { current = false; }, publications: () => published };
}

async function complete(call) {
  call.resolve(Array.from({ length: call.to - call.from + 1 }, (_, index) => ({
    line: call.from + index, text: `line ${call.from + index}`, changed: false, words: [],
  })));
  await nextTurn();
}

test("两侧可见小片先到齐，随后仍补完整400行块", async () => {
  const f = fixture();
  f.store.ensure("left", f.window, f.loader);
  f.store.ensure("right", f.window, f.loader);
  assert.equal(f.store.pendingRequests, 2);
  assert.deepEqual(f.calls.map(({ side, from, to }) => [side, from, to]), [["left", 1, 32], ["right", 1, 32]]);
  await complete(f.calls[0]);
  f.store.ensure("left", f.window, f.loader);
  assert.equal(f.calls.length, 2, "左侧先完成不能让整块挡在右侧小片前面");
  await complete(f.calls[1]);
  f.store.ensure("right", f.window, f.loader);
  assert.deepEqual(f.calls.slice(2).map(({ side, from, to }) => [side, from, to]), [["left", 1, 400], ["right", 1, 400]]);
  f.store.ensure("left", f.window, f.loader);
  assert.equal(f.calls.length, 4, "小片和整块分别去重，重复render不重复请求");
  await complete(f.calls[2]);
  await complete(f.calls[3]);
  assert.equal(f.store.pendingRequests, 0);
  f.store.ensure("left", f.window, f.loader);
  assert.equal(f.calls.length, 4);
  assert.equal(f.store.old.size, 400);
  assert.equal(f.store.new.size, 400);
  assert.deepEqual(f.priorities, [true, true, false, false], "可见小片优先发布，纯预取不强制提交");
});

test("整块重叠回包保留已有行与词区间引用，补缺行后仍通知", async () => {
  const f = fixture();
  const window = { visible: [{ from: 1, to: 1 }], buffered: [{ from: 1, to: 3 }], lineCount: 3 };
  f.store.ensure("right", { visible: [], buffered: [], lineCount: 0 }, f.loader);
  f.store.ensure("left", window, f.loader);
  const first = { line: 1, text: "line 1", changed: true, words: [[0, 4]] };
  f.calls[0].resolve([first]);
  await nextTurn();
  assert.equal(f.publications(), 1);
  f.store.ensure("left", window, f.loader);
  assert.deepEqual(f.calls.map(({ side, from, to }) => [side, from, to]), [["left", 1, 1], ["left", 1, 3]]);
  const block = [
    { ...first, words: [[0, 4]] },
    { line: 2, text: "line 2", changed: false, words: [] },
    { line: 3, text: "line 3", changed: false, words: [] },
  ];
  f.calls[1].resolve(block);
  await nextTurn();
  assert.equal(f.store.old.get(1), first);
  assert.equal(f.store.old.get(1).words, first.words);
  assert.notEqual(f.store.old.get(1).words, block[0].words);
  assert.equal(f.store.old.get(2), block[1]);
  assert.equal(f.store.old.get(3), block[2]);
  assert.equal(f.store.old.size, 3);
  assert.equal(f.publications(), 2, "补入缺行仍通知界面，已有可见行引用不变");
});

test("纯新增的空左侧不会挡住右侧预取，尾块夹到真实行数", async () => {
  const f = fixture();
  f.store.ensure("left", { visible: [], buffered: [], lineCount: 0 }, f.loader);
  const right = { visible: [{ from: 1, to: 8 }], buffered: [{ from: 1, to: 28 }], lineCount: 28 };
  f.store.ensure("right", right, f.loader);
  assert.equal(f.calls.length, 1);
  await complete(f.calls[0]);
  f.store.ensure("right", right, f.loader);
  assert.deepEqual(f.calls.map(({ side, from, to }) => [side, from, to]), [["right", 1, 8], ["right", 1, 28]]);
});

test("跳转时优先请求新可见片，保留原来在途的整块", async () => {
  const f = fixture();
  f.store.ensure("left", f.window, f.loader);
  f.store.ensure("right", f.window, f.loader);
  await complete(f.calls[0]);
  await complete(f.calls[1]);
  f.store.ensure("left", f.window, f.loader);
  const jump = { visible: [{ from: 450, to: 482 }], buffered: [{ from: 400, to: 550 }], lineCount: 1000 };
  f.store.ensure("left", jump, f.loader);
  f.store.ensure("right", jump, f.loader);
  assert.deepEqual(f.calls.slice(4).map(({ side, from, to }) => [side, from, to]), [["left", 450, 482], ["right", 450, 482]]);
  await complete(f.calls[2]);
  assert.ok(f.store.old.has(400), "仍接收同一快照旧位置的在途块");
  await complete(f.calls[4]);
  await complete(f.calls[5]);
  f.store.ensure("left", jump, f.loader);
  assert.deepEqual(f.calls.slice(6).map(({ side, from, to }) => [side, from, to]), [["left", 401, 800], ["right", 401, 800]]);
});

test("快照/文件/档位换代后，旧小片不写缓存也不发通知", async () => {
  const f = fixture();
  f.store.ensure("left", f.window, f.loader);
  f.retire();
  await complete(f.calls[0]);
  assert.equal(f.store.old.size, 0);
  assert.equal(f.publications(), 0);
  f.store.ensure("right", f.window, f.loader);
  assert.equal(f.calls.length, 1);
});

test("小片失败不触发渲染重试环，显式滚动后的ensure仍可重试", async () => {
  const f = fixture();
  f.store.ensure("left", f.window, f.loader);
  f.calls[0].reject(new Error("read failed"));
  await nextTurn();
  assert.equal(f.calls.length, 1);
  assert.equal(f.publications(), 0);
  f.store.ensure("left", f.window, f.loader);
  assert.equal(f.calls.length, 2);
});

test("实际可见行映射跳过长折叠区，不把隐藏全文作为优先片", () => {
  const rows = buildRows(1000, [{ leftStart: 20, leftEnd: 900, rightStart: 20, rightEnd: 900 }], "left", new Set());
  assert.deepEqual(lineRangesForRows(rows.rows, 18, 25), [{ from: 19, to: 20 }, { from: 901, to: 904 }]);
  assert.deepEqual(lineRangesForRows(rows.rows, 20, 21), []);
  const heights = new RowHeights(rows.rows.length, 18, true);
  heights.measure(18, 180);
  const top = heights.top(18) + 90;
  const from = heights.rowAt(top);
  const to = Math.ceil(heights.rowAtFraction(top + 180));
  assert.deepEqual(lineRangesForRows(rows.rows, from, to), [{ from: 19, to: 20 }, { from: 901, to: 903 }]);
});


test("空响应没有进展时不发渲染通知", async () => {
  const f = fixture();
  f.store.ensure("left", f.window, f.loader);
  f.calls[0].resolve([]);
  await nextTurn();
  assert.equal(f.calls.length, 1);
  assert.equal(f.publications(), 0);
});


test("迟到小片按当前可见范围发布，旧位置只缓存不抢提交", async () => {
  const f = fixture();
  f.store.ensure("left", f.window, f.loader);
  const moved = { visible: [{ from: 900, to: 932 }], buffered: [], lineCount: 1000 };
  f.store.ensure("left", moved, f.loader);
  assert.deepEqual(f.calls.map(({ from, to }) => [from, to]), [[1, 32], [900, 932]]);
  await complete(f.calls[0]);
  await complete(f.calls[1]);
  assert.deepEqual(f.priorities, [false, true]);
  assert.ok(f.store.old.has(1), "相同快照的旧位置仍保留在缓存");
});

test("在途预取补到滚动后的可见行时，也优先发布", async () => {
  const f = fixture();
  f.store.ensure("right", { visible: [], buffered: [], lineCount: 0 }, f.loader);
  f.store.ensure("left", f.window, f.loader);
  await complete(f.calls[0]);
  f.store.ensure("left", f.window, f.loader);
  const block = f.calls.find((call) => call.from === 1 && call.to === 400);
  assert.ok(block);
  f.store.ensure("left", { visible: [{ from: 200, to: 232 }], buffered: [], lineCount: 1000 }, f.loader);
  await complete(block);
  assert.deepEqual(f.priorities, [true, true]);
  const slice = f.calls.find((call) => call.from === 200);
  await complete(slice);
  assert.deepEqual(f.priorities, [true, true], "同快照重叠回包未增加行时不重复提交");
});
