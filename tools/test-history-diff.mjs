import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { registerHooks } from "node:module";
import { setImmediate as nextTurn } from "node:timers/promises";
import test from "node:test";

const sourceRoot = new URL("../src/", import.meta.url);
const hooks = registerHooks({
  resolve(specifier, context, nextResolve) {
    let source;
    if (specifier.startsWith("@/")) source = new URL(specifier.slice(2), sourceRoot);
    else if ((specifier.startsWith("./") || specifier.startsWith("../")) && context.parentURL?.startsWith(sourceRoot.href)) {
      source = new URL(specifier, context.parentURL);
    }
    if (source) {
      if (!/\.[^/]+$/.test(source.pathname)) source.pathname += ".ts";
      return nextResolve(source.href, context);
    }
    return nextResolve(specifier, context);
  },
});
const { createHistorySideSource } = await import("../src/features/log/historySideSource.ts");
const { HistoryHighlightStore } = await import("../src/features/log/historyHighlightStore.ts");
const { buildRows } = await import("../src/features/diff/rowIndex.ts");
const { SideLineWindow } = await import("../src/features/diff/sideLineWindow.ts");
hooks.deregister();

const file = {
  commit: "fixed-commit", parent: "fixed-parent", path: "src/example.ts", originalPath: null,
  oldOid: "old-blob", newOid: "new-blob", diffId: "signed-diff", status: "modified",
};
const options = { ignoreWhitespace: "off", contextLines: 3, highlight: "word" };
const layout = {
  targetId: "history-a", diffId: file.diffId, ignoreWhitespace: "off", leftLineCount: 3, rightLineCount: 5,
  segments: [{ leftStart: 1, leftEnd: 2, rightStart: 1, rightEnd: 4 }], folds: [],
  binary: false, modeOnly: false, emptyReason: null, leftMissing: false, rightMissing: false,
};

test("历史左右数据源只调用固定历史接口，保留两侧独立连续行而不插空白", async () => {
  const requests = [];
  const source = createHistorySideSource("history-a", file, options, {
    historyFileSideLayout: async (request) => { requests.push(request); return layout; },
    historyFileSideLines: async (request) => {
      requests.push(request);
      return Array.from({ length: request.to - request.from + 1 }, (_, offset) => ({
        line: request.from + offset, text: `${request.version}:${request.from + offset}`, changed: true, words: [],
      }));
    },
  });
  const read = await source.layout();
  const left = buildRows(read.leftLineCount, read.folds, "left", new Set());
  const right = buildRows(read.rightLineCount, read.folds, "right", new Set());
  assert.deepEqual(left.rows.map((row) => row.line), [1, 2, 3]);
  assert.deepEqual(right.rows.map((row) => row.line), [1, 2, 3, 4, 5]);
  const cache = new SideLineWindow(source.key);
  const loader = { load: source.lines, isCurrent: () => true, publish() {} };
  cache.ensure("left", { lineCount: 3, visible: [{ from: 1, to: 3 }], buffered: [] }, loader);
  cache.ensure("right", { lineCount: 5, visible: [{ from: 1, to: 5 }], buffered: [] }, loader);
  await nextTurn();
  assert.equal(cache.old.size, 3);
  assert.equal(cache.new.size, 5);
  assert.equal(cache.old.get(3).text, "old:3");
  assert.equal(cache.new.get(5).text, "new:5");
  for (const request of requests) {
    assert.equal(request.targetId, "history-a");
    assert.deepEqual(request.file, file);
    assert.equal(request.ignoreWhitespace, "off");
    assert.equal(request.contextLines, 3);
    for (const key of ["worktreeId", "fingerprint", "changes", "segmentActions"]) assert.ok(!(key in request));
  }
});

test("历史左右源冻结身份和设置，键包含每个影响读取的参数", async () => {
  const mutableFile = { ...file };
  const mutableOptions = { ...options };
  const requests = [];
  const api = {
    historyFileSideLayout: async () => layout,
    historyFileSideLines: async (request) => { requests.push(request); return []; },
  };
  const source = createHistorySideSource("history-a", mutableFile, mutableOptions, api);
  mutableFile.commit = "moved-head";
  mutableOptions.ignoreWhitespace = "all";
  await source.lines("right", 401, 420);
  assert.equal(requests[0].file.commit, "fixed-commit");
  assert.equal(requests[0].ignoreWhitespace, "off");
  for (const variant of [
    createHistorySideSource("history-b", file, options, api),
    createHistorySideSource("history-a", { ...file, diffId: "another-diff" }, options, api),
    createHistorySideSource("history-a", { ...file, parent: "another-parent" }, options, api),
    createHistorySideSource("history-a", file, { ...options, ignoreWhitespace: "all" }, api),
    createHistorySideSource("history-a", file, { ...options, highlight: "char" }, api),
    createHistorySideSource("history-a", file, { ...options, contextLines: 10 }, api),
  ]) assert.notEqual(variant.key, source.key);
});

test("历史左右源拒绝其他目标或档位的布局", async () => {
  for (const changed of [{ targetId: "history-b" }, { diffId: "other-diff" }, { ignoreWhitespace: "all" }]) {
    const source = createHistorySideSource("history-a", file, options, {
      historyFileSideLayout: async () => ({ ...layout, ...changed }), historyFileSideLines: async () => [],
    });
    await assert.rejects(source.layout(), { kind: "rejected", code: "errors.history.layoutTargetMismatch" });
  }
});

function highlighted(request, text = "first") {
  return { fromLine: request.fromLine - 1, lines: [[{ text, category: 1 }]], kind: "highlighted", needContent: false,
    elapsedMs: 1, parsedLines: 1, totalLines: 20000, revision: 0, contentVersion: 1 };
}

test("历史高亮请求1基、接收0基并转换为共享渲染器的1基行号与类别名", async () => {
  const requests = [];
  const store = new HistoryHighlightStore({ targetId: "history-a", file, ignoreWhitespace: "eol" }, {
    highlight: async (request) => { requests.push(request); return highlighted(request); },
    categories: async () => ["plain", "keyword"],
  });
  let notifications = 0;
  const unsubscribe = store.subscribe(() => { notifications += 1; });
  store.ensure("old", 1, 1);
  store.ensure("old", 1, 1);
  await nextTurn();
  assert.equal(requests.length, 1);
  assert.equal(requests[0].fromLine, 1);
  assert.equal(requests[0].toLine, 400);
  assert.equal(requests[0].ignoreWhitespace, "eol");
  assert.equal(store.get("old", 0), undefined);
  assert.deepEqual(store.get("old", 1), [{ text: "first", category: 1 }]);
  assert.equal(store.categoryClass(1), "tok-keyword");
  assert.ok(notifications > 0);
  assert.equal(store.getSnapshot().loadedLines, 1);
  unsubscribe();
});

test("历史高亮关闭后丢弃迟到结果，StrictMode式清理后仍可重新读取", async () => {
  const pending = [];
  const store = new HistoryHighlightStore({ targetId: "history-a", file, ignoreWhitespace: "off" }, {
    highlight: (request) => new Promise((resolve) => pending.push({ request, resolve })), categories: async () => ["plain", "keyword"],
  });
  store.ensure("new", 1, 1);
  store.dispose();
  store.ensure("new", 1, 1);
  assert.equal(pending.length, 2);
  pending[0].resolve(highlighted(pending[0].request, "stale"));
  await nextTurn();
  assert.equal(store.get("new", 1), undefined);
  pending[1].resolve(highlighted(pending[1].request, "current"));
  await nextTurn();
  assert.equal(store.get("new", 1)[0].text, "current");
});

test("历史高亮缓存有界，越过16块后释放旧块而不是随文件大小增长", async () => {
  const requests = [];
  const store = new HistoryHighlightStore({ targetId: "history-a", file, ignoreWhitespace: "off" }, {
    highlight: async (request) => { requests.push(request); return highlighted(request, String(request.fromLine)); },
    categories: async () => ["plain", "keyword"],
  });
  for (let chunk = 0; chunk < 20; chunk += 1) {
    store.ensure("new", chunk * 400 + 1, chunk * 400 + 1);
    await nextTurn();
  }
  assert.equal(requests.length, 20);
  assert.equal(store.getSnapshot().loadedLines, 16);
  assert.equal(store.get("new", 1), undefined);
  assert.equal(store.get("new", 7601)[0].text, "7601");
});

test("历史包装复用受控左右渲染器且历史分支没有工作区写按钮", () => {
  const view = readFileSync(new URL("../src/features/diff/SideBySideView.tsx", import.meta.url), "utf8");
  const wrapper = readFileSync(new URL("../src/features/log/HistorySideDiffView.tsx", import.meta.url), "utf8");
  assert.match(wrapper, /<SideBySideView history=\{source\}/);
  assert.doesNotMatch(wrapper, /ChangeEntry|fingerprint|SegmentAction|overflow-auto/);
  assert.match(view, /const workspaceLayout = !history && layout/);
  assert.match(view, /const leftActions = !workspaceLayout \|\| workspaceLayout\.conflict \? "none"/);
  assert.match(view, /const rightActions = !workspaceLayout \|\| workspaceLayout\.conflict \? "none"/);
  assert.match(view, /history \? history\.lines\(targetSide, from, to\) : ipc\.sideLines/);
});
