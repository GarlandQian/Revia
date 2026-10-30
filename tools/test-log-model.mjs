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
const {
  defaultFilter, filterForCommit, toHistoryFilter, filterKey, filterStart, activeFilterCount, hasActiveFilter,
  dateFilterOf, isDateText, looksLikeHash, formatCommitDate, refChips, authorKey, authorLabel,
  branchFilterOptions, commitTabKey, commitTabTitle, shortOid, logVisibleEnd, selectedLogCommit,
} = await import("../src/features/log/logModel.ts");
const { appendGraphRows, emptyGraphState, collapseLongEdges, laneColor } = await import("../src/features/log/logGraph.ts");
const { formatAppError } = await import("../src/lib/error.ts");
const { LogPager, LOG_PAGE_SIZE } = await import("../src/features/log/logPager.ts");
const { buildFileTree, effectiveMergeMode, firstHistoryFile } = await import("../src/features/log/logFiles.ts");
const { showCommitTab } = await import("../src/features/log/logTabs.ts");
const { LogWorkspaceStore } = await import("../src/features/log/useLogWorkspace.ts");
const { RefsWatch, refsSignature } = await import("../src/features/log/refsWatch.ts");
const { HistoryRowsReader, HistoryRowsStore, historyRowsKey, historyRowsWindow } = await import("../src/features/log/historyRows.ts");
const { readFileSync } = await import("node:fs");
imports.deregister();

const historyFile = {
  commit: "commit-a", parent: "parent-a", path: "src/a.ts", originalPath: null,
  oldOid: "old-blob", newOid: "new-blob", status: "modified", diffId: "diff-a",
};
const historyRequest = (extra = {}) => ({
  targetId: "target-a", file: historyFile, view: "unified", ignoreWhitespace: "off",
  ...historyRowsWindow(0, 50), ...extra,
});
const rowSlice = (request, rowCount = 1600) => ({
  targetId: request.targetId, diffId: request.file.diffId, view: request.view,
  from: Math.min(request.from, rowCount), rowCount,
  rows: Array.from({ length: Math.max(0, Math.min(request.to, rowCount) - request.from) }, (_, index) => ({
    text: `${request.ignoreWhitespace}:${request.from + index}`,
  })), binary: false, modeOnly: false, emptyReason: null,
});

test("历史行窗口覆盖400行边界两侧及预读，不必继续滚动才能补齐", async () => {
  const reader = new HistoryRowsReader(async (request) => rowSlice(request, 9000));
  for (const start of [0, 385, 397, 399, 400, 785, 7985]) {
    const end = start + 65;
    const window = historyRowsWindow(start, end);
    assert.ok(window.from <= start);
    assert.ok(window.to >= end + 400);
    assert.ok(window.to - window.from <= 1200, "缓存只保留有界窗口，不读取完整文件");
    const state = await reader.read(historyRequest(window));
    for (let position = start; position < end; position += 1) {
      assert.equal(state.rows[position - state.from]?.text, `off:${position}`);
    }
  }
});

test("历史行缓存身份包括目标、文件身份、视图和忽略档位", async () => {
  const requests = [];
  const reader = new HistoryRowsReader(async (request) => {
    requests.push(request);
    return rowSlice(request);
  });
  const base = historyRequest();
  const first = await reader.read(base);
  assert.strictEqual(await reader.read({ ...base, file: { ...base.file } }), first);
  assert.equal(requests.length, 1);
  for (const variant of [
    { targetId: "target-b" }, { view: "split" }, { ignoreWhitespace: "eol" },
    { ignoreWhitespace: "all" }, { ignoreWhitespace: "allAndBlankLines" },
    ...Object.keys(historyFile).map((field) => ({ file: { ...historyFile, [field]: `${field}-changed` } })),
  ]) {
    const request = { ...base, ...variant };
    const before = requests.length;
    assert.notEqual(historyRowsKey(request), historyRowsKey(base));
    const state = await reader.read(request);
    assert.equal(requests.length, before + 1);
    assert.equal(state.key, historyRowsKey(request));
    assert.equal(state.rows[0].text, `${request.ignoreWhitespace}:0`);
  }
});

test("历史行设置变更使旧请求失效，旧成功或失败都不能污染新缓存", async () => {
  for (const rejectOld of [false, true]) {
    const pending = [];
    const reader = new HistoryRowsReader((request) => new Promise((resolve, reject) => pending.push({ request, resolve, reject })));
    const oldRead = reader.read(historyRequest());
    const currentRequest = historyRequest({ ignoreWhitespace: "all" });
    const currentRead = reader.read(currentRequest);
    pending[1].resolve(rowSlice(currentRequest));
    const current = await currentRead;
    if (rejectOld) pending[0].reject(new Error("old failure"));
    else pending[0].resolve(rowSlice(pending[0].request));
    assert.equal(await oldRead, null);
    assert.strictEqual(await reader.read(currentRequest), current);
    assert.equal(pending.length, 2);
  }
});

test("历史行滚回缓存窗口会淘汰在途远处窗口，关闭目标同样淘汰", async () => {
  const pending = [];
  const reader = new HistoryRowsReader((request) => new Promise((resolve) => pending.push({ request, resolve })));
  const initial = reader.read(historyRequest());
  pending[0].resolve(rowSlice(pending[0].request));
  const cached = await initial;
  const distant = reader.read(historyRequest(historyRowsWindow(1200, 1250)));
  assert.strictEqual(await reader.read(historyRequest()), cached);
  pending[1].resolve(rowSlice(pending[1].request));
  assert.equal(await distant, null);
  const closing = reader.read(historyRequest({ targetId: "other-target" }));
  reader.invalidate();
  pending[2].resolve(rowSlice(pending[2].request));
  assert.equal(await closing, null);
});

test("历史行空结果可缓存；忽略设置缩短文件时接受后端裁剪的起点", async () => {
  let calls = 0;
  const reader = new HistoryRowsReader(async (request) => {
    calls += 1;
    return rowSlice(request, request.ignoreWhitespace === "off" ? 0 : 100);
  });
  const empty = await reader.read(historyRequest());
  assert.equal(empty.rowCount, 0);
  assert.strictEqual(await reader.read(historyRequest()), empty);
  assert.equal(calls, 1);
  const short = await reader.read(historyRequest({ ignoreWhitespace: "all", from: 400, to: 1200 }));
  assert.equal(short.error, null);
  assert.equal(short.from, 100);
  assert.equal(short.rowCount, 100);
});

test("历史行拒绝目标错配的响应，界面使用现有受控渲染器而非原生滚动", async () => {
  const reader = new HistoryRowsReader(async (request) => ({ ...rowSlice(request), targetId: "wrong-target" }));
  const state = await reader.read(historyRequest());
  assert.equal(state.error.code, "errors.history.rowsTargetMismatch");
  assert.deepEqual(state.rows, []);
  const view = readFileSync(new URL("../src/features/log/CommitDiffView.tsx", import.meta.url), "utf8");
  assert.match(view, /new HistoryRowsStore/);
  assert.match(view, /<DiffView history=/);
  assert.match(view, /<HistorySideDiffView/);
  assert.doesNotMatch(view, /overflow-auto|onScroll=|row-split/);
});

test("历史统一视图数据源跨块连续且小文件预取完整，不签发工作区写凭据", async () => {
  const requests = [];
  const rows = new HistoryRowsStore(historyRequest(), async (request) => {
    requests.push(request);
    return rowSlice(request);
  });
  let changes = 0;
  const stop = rows.subscribe(() => { changes += 1; });
  assert.equal(rows.ensure(0, 40), false);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(rows.rowCountValue, 1600);
  assert.equal(rows.isCovered(0, 1600), true);
  assert.equal(rows.ensure(385, 450), true);
  assert.equal(rows.getRow(397).text, "off:397");
  assert.equal(rows.getRow(400).text, "off:400");
  for (let index = 0; index < rows.rowCountValue; index += 1) {
    assert.equal(rows.getRow(index).hunkAction, null);
    assert.equal(rows.getRow(index).segmentAction, null);
  }
  assert.ok(requests.length > 1);
  assert.ok(requests.every((request) => !Object.hasOwn(request, "worktreeId") && !Object.hasOwn(request, "fingerprint")));
  assert.ok(changes > 0);
  assert.equal(rows.epochValue, 0, "分块到达不重置已量好的软换行高度");
  stop();
  rows.invalidate();
});

test("历史统一视图换目标后旧数据不复活，大文件不做全文件预取", async () => {
  const pending = [];
  const rows = new HistoryRowsStore(historyRequest(), (request) => new Promise((resolve) => pending.push({ request, resolve })));
  rows.ensure(0, 40);
  rows.invalidate();
  pending[0].resolve(rowSlice(pending[0].request));
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(rows.getSnapshot().details, null);
  assert.equal(rows.getRow(0), null);
  const requests = [];
  const large = new HistoryRowsStore(historyRequest(), async (request) => {
    requests.push(request);
    return rowSlice(request, 30_000);
  });
  large.ensure(0, 40);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(large.rowCountValue, 30_000);
  assert.equal(large.getRow(29_999), null);
  assert.ok(requests.length <= 2);
  large.ensure(29_950, 30_000);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(large.getRow(29_999).text, "off:29999");
  large.invalidate();
});

test("集成页签保留自动露出且历史页签没有工作区写工具条", () => {
  const tabs = readFileSync(new URL("../src/components/app/EditorTabs.tsx", import.meta.url), "utf8");
  assert.match(tabs, /ref=\{logActive \? activeTabRef : null\}/);
  assert.equal((tabs.match(/ref=\{active \? activeTabRef : null\}/g) ?? []).length, 2);
  assert.match(tabs, /\[activeKey, activeCommitKey, logActive, tabs, commitTabs\]/);
  assert.match(tabs, /editor-tabs-toolbar \$\{logActive \|\| activeCommitKey \? "hidden" : "flex"\}/);
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  assert.match(app, /activeKey=\{mainView === "diff" \? activeKey : null\}/);
  assert.match(app, /if \(mainView !== "diff"\) pendingAnchor\.current = null/);
  assert.match(app, /mainViewRef\.current === "diff" && !mergeTabPathRef\.current/);
});

test("M5筛选下拉复用修复后的受控弹层和缩放定位层", () => {
  const popover = readFileSync(new URL("../src/components/ui/popover.tsx", import.meta.url), "utf8");
  assert.match(popover, /onOpen: \(\) => void/);
  assert.match(popover, /PopoverParts\.Portal className="revia-popover-host"/);
  assert.match(popover, /PopoverParts\.Positioner className="pointer-events-auto"/);
  assert.match(popover, /export function Dropdown/);
  assert.match(popover, /onOpen=\{\(\) => setOpen\(true\)\}/);
});

const commit = (oid, parents = [], extra = {}) => ({
  oid,
  parents,
  author: { name: "Alice", email: "alice@example.com", time: 0, timezone: "+0000" },
  committer: { name: "Alice", email: "alice@example.com", time: 0, timezone: "+0000" },
  subject: `subject ${oid}`,
  refs: [],
  ...extra,
});

// ---------- 筛选条件 → 请求 ----------

test("默认筛选不传任何条件，起点固定为全部引用的快照", () => {
  const state = defaultFilter();
  const filter = toHistoryFilter(state);
  assert.equal(filter.text, null);
  assert.equal(filter.date, null);
  assert.deepEqual(filter.branches, []);
  assert.deepEqual(filter.authors, []);
  assert.deepEqual(filter.paths, []);
  assert.equal(filter.caseSensitive, false);
  assert.equal(filter.regex, false);
  assert.deepEqual(filterStart(state), { kind: "refs", fullNames: [] });
  assert.equal(activeFilterCount(state), 0);
  assert.equal(hasActiveFilter(state), false);
});

test("文字、分支、作者、日期、路径组合成结构化筛选", () => {
  const state = {
    ...defaultFilter(),
    text: "  fix build  ",
    caseSensitive: true,
    regex: true,
    branches: ["refs/heads/main", "refs/tags/v1"],
    authors: ["Alice <alice@example.com>"],
    date: "range",
    since: "2026-10-01",
    until: "2026-10-31",
    paths: ["src/a.ts", "  "],
  };
  const filter = toHistoryFilter(state);
  assert.equal(filter.text, "fix build");
  assert.equal(filter.caseSensitive, true);
  assert.equal(filter.regex, true);
  assert.deepEqual(filter.branches, ["refs/heads/main", "refs/tags/v1"]);
  assert.deepEqual(filter.authors, ["Alice <alice@example.com>"]);
  // 空白路径被丢掉：后端只接受真实路径。
  assert.deepEqual(filter.paths, ["src/a.ts"]);
  assert.deepEqual(filter.date, { kind: "range", since: "2026-10-01", until: "2026-10-31" });
  assert.equal(activeFilterCount(state), 5);
});

test("日期预设：最近 1 / 7 / 30 天与自定义区间", () => {
  assert.deepEqual(dateFilterOf({ ...defaultFilter(), date: "last1" }), { kind: "lastDays", days: 1 });
  assert.deepEqual(dateFilterOf({ ...defaultFilter(), date: "last7" }), { kind: "lastDays", days: 7 });
  assert.deepEqual(dateFilterOf({ ...defaultFilter(), date: "last30" }), { kind: "lastDays", days: 30 });
  assert.equal(dateFilterOf({ ...defaultFilter(), date: "all" }), null);
  // 自定义区间缺一半或格式不对时不下发筛选（由界面提示用户补全）。
  assert.equal(dateFilterOf({ ...defaultFilter(), date: "range", since: "2026-10-01" }), null);
  assert.equal(dateFilterOf({ ...defaultFilter(), date: "range", since: "2026/10/01", until: "2026-10-02" }), null);
  assert.ok(isDateText("2026-10-01"));
  assert.ok(!isDateText("2026-1-1"));
});

test("筛选标识随任一条件变化（游标不能跨筛选复用）", () => {
  const base = defaultFilter();
  const keys = new Set([filterKey(base)]);
  const variants = [
    { ...base, text: "fix" },
    { ...base, caseSensitive: true },
    { ...base, regex: true },
    { ...base, branches: ["refs/heads/main"] },
    { ...base, authors: ["Alice <alice@example.com>"] },
    { ...base, date: "last7" },
    { ...base, date: "range", since: "2026-10-01", until: "2026-10-02" },
    { ...base, paths: ["src/a.ts"] },
  ];
  for (const variant of variants) {
    const key = filterKey(variant);
    assert.ok(!keys.has(key), `条件变化后筛选标识必须不同：${JSON.stringify(variant)}`);
    keys.add(key);
  }
  // 顺序无关：同一组分支 / 路径重新排序不算变化。
  assert.equal(
    filterKey({ ...base, branches: ["b", "a"] }),
    filterKey({ ...base, branches: ["a", "b"] }),
  );
  // 首尾空白不算变化。
  assert.equal(filterKey({ ...base, text: " fix " }), filterKey({ ...base, text: "fix" }));
});

test("哈希前缀识别与短哈希", () => {
  assert.ok(looksLikeHash("deadbeef"));
  assert.ok(looksLikeHash("ABCD"));
  assert.ok(!looksLikeHash("abc"));
  assert.ok(!looksLikeHash("fix build"));
  assert.ok(!looksLikeHash("zzzz"));
  assert.equal(shortOid("0123456789abcdef"), "0123456");
});

// ---------- 显示格式 ----------

test("日期显示：今天、昨天与更早的绝对日期", () => {
  const now = new Date(2026, 9, 27, 14, 30).getTime();
  const at = (year, month, day, hour, minute) => new Date(year, month, day, hour, minute).getTime() / 1000;
  assert.equal(formatCommitDate(at(2026, 9, 27, 14, 22), now), "今天 14:22");
  assert.equal(formatCommitDate(at(2026, 9, 26, 9, 5), now), "昨天 09:05");
  assert.equal(formatCommitDate(at(2026, 9, 20, 23, 59), now), "2026/10/20 23:59");
  assert.equal(formatCommitDate(at(2026, 0, 5, 7, 8), now), "2026/01/05 07:08");
  assert.equal(formatCommitDate(0, now), "");
});

test("引用标签：HEAD、当前分支、远端、标签的顺序与突出显示", () => {
  const chips = refChips(
    [
      { fullName: "refs/tags/v1", shortName: "v1", kind: "tag" },
      { fullName: "refs/remotes/origin/main", shortName: "origin/main", kind: "remoteBranch" },
      { fullName: "refs/heads/main", shortName: "main", kind: "branch" },
      { fullName: "refs/heads/HEAD", shortName: "HEAD", kind: "branch" },
      { fullName: "refs/heads/feature", shortName: "feature", kind: "branch" },
    ],
    "main",
  );
  assert.deepEqual(chips.map((chip) => chip.label), ["HEAD", "main", "feature", "origin/main", "v1"]);
  assert.equal(chips.find((chip) => chip.label === "main").current, true);
  assert.equal(chips.find((chip) => chip.label === "feature").current, false);
});

test("作者标识与显示名、分支筛选候选项", () => {
  const item = commit("a", [], { author: { name: "Alice", email: "alice@example.com", time: 1, timezone: "+0800" } });
  assert.equal(authorKey(item), "Alice <alice@example.com>");
  assert.equal(authorLabel(item), "Alice");
  const options = branchFilterOptions([
    { fullName: "refs/heads/main", shortName: "main", kind: "branch" },
    { fullName: "refs/remotes/origin/dev", shortName: "origin/dev", kind: "remoteBranch" },
    { fullName: "refs/tags/v1", shortName: "v1", kind: "tag" },
    { fullName: "refs/stash", shortName: "stash", kind: "other" },
  ]);
  assert.deepEqual(options.map((option) => option.group), ["local", "remote", "tag"]);
  assert.deepEqual(options.map((option) => option.label), ["main", "origin/dev", "v1"]);
});

test("提交页签的键与标题", () => {
  assert.equal(commitTabKey("abc", "src/a.ts"), "commit:abc:src/a.ts");
  assert.equal(commitTabTitle("src/a.ts", "0123456789abcdef"), "a.ts（0123456）");
  assert.equal(commitTabTitle("a.ts", "0123456789abcdef"), "a.ts（0123456）");
});

// ---------- 分支图 ----------

test("分支图按拓扑顺序分配轨道并画出合并与分叉", () => {
  const history = [
    { oid: "m", parents: ["a", "b"] },
    { oid: "a", parents: ["base"] },
    { oid: "b", parents: ["base"] },
    { oid: "base", parents: [] },
  ];
  const { rows, state } = appendGraphRows(emptyGraphState(), history);
  assert.equal(rows.length, 4);
  assert.equal(rows[0].lane, 0, "merge 提交在第一条轨道");
  assert.equal(rows[1].lane, 0);
  assert.ok(rows[2].lane > 0, "第二个分支用新轨道");
  assert.deepEqual(state.active, [], "画完后所有轨道都结束");
  // 每一行的占用宽度足够容纳它画出的轨道。
  for (const row of rows) {
    for (const lane of [...row.railsAbove, ...row.railsBelow, row.lane, ...row.arms.flatMap((arm) => [arm.from, arm.to])]) {
      assert.ok(row.width > lane, `宽度 ${row.width} 应覆盖轨道 ${lane}`);
    }
  }
  // 合并提交有一条连到第二个父提交的横线。
  assert.ok(rows[0].arms.some((arm) => arm.to !== arm.from));
});

test("分页增量计算与一次性计算结果一致", () => {
  const history = [
    { oid: "c5", parents: ["c4"] },
    { oid: "c4", parents: ["c3"] },
    { oid: "c3", parents: ["c2"] },
    { oid: "c2", parents: ["c1"] },
    { oid: "c1", parents: [] },
  ];
  const oneShot = appendGraphRows(emptyGraphState(), history);
  const first = appendGraphRows(emptyGraphState(), history.slice(0, 2));
  const second = appendGraphRows(first.state, history.slice(2));
  assert.deepEqual([...first.rows, ...second.rows], oneShot.rows);
  assert.deepEqual(second.state, oneShot.state);
  assert.equal(laneColor(0), laneColor(0));
  assert.notEqual(laneColor(0), laneColor(1));
});

test("只看第一父提交时只画第一父边，但父提交列表不被改写", () => {
  const history = [{ oid: "m", parents: ["a", "b"] }, { oid: "a", parents: [] }];
  const { rows } = appendGraphRows(emptyGraphState(), history, { firstParent: true });
  assert.equal(rows[0].arms.length, 0, "第二父提交不画线");
  assert.equal(rows[1].lane, 0);
});

test("长边折叠：开关关闭时不折叠，开启时两端给可点击箭头", () => {
  // 一条侧分支的线在 40 行之后才遇到自己的提交：这就是需要折叠的长边。
  const history = [{ oid: "m", parents: ["c0", "far"] }];
  for (let index = 0; index < 40; index += 1) {
    history.push({ oid: `c${index}`, parents: index === 39 ? [] : [`c${index + 1}`] });
  }
  history.push({ oid: "far", parents: [] });
  const all = appendGraphRows(emptyGraphState(), history);
  const off = collapseLongEdges(all.rows, false);
  assert.equal(off.markers.length, 0);
  assert.equal(off.collapsed.size, 0);

  const on = collapseLongEdges(all.rows, true, 10);
  assert.ok(on.markers.length >= 2, "至少两端各一个箭头");
  assert.ok(on.collapsed.size > 0, "中间部分折叠");
  const first = on.markers.find((marker) => marker.direction === "down");
  assert.ok(first.row < first.targetRow);
  const last = on.markers.find((marker) => marker.direction === "up");
  assert.ok(last.targetRow < last.row);
});

// ---------- 分页 ----------

function page(commits, nextCursor, exhausted) {
  return { targetId: "t1", queryId: "q1", start: "x", firstParent: false, commitCount: commits.length, nextCursor, exhausted, commits };
}

test("分页：滚到接近末尾才取下一页，按 OID 去重", async () => {
  const requests = [];
  const pages = [
    page([commit("a"), commit("b")], "t1/q1:2", false),
    page([commit("b"), commit("c")], "t1/q1:3", false),
    page([commit("c"), commit("d")], null, true),
  ];
  const pager = new LogPager({
    targetId: "t1",
    pageSize: 2,
    loader: async (request) => {
      requests.push(request);
      return pages[requests.length - 1];
    },
  });
  const snapshots = [];
  pager.setListener((snapshot) => snapshots.push(snapshot.commits.length));
  pager.setQuery({ text: null, caseSensitive: false, regex: false, branches: [], authors: [], paths: [], date: null }, false);
  pager.reset("k");
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(pager.snapshot().commits.map((item) => item.oid), ["a", "b"]);
  assert.equal(requests[0].start.kind, "refs");
  assert.equal(requests[0].limit, LOG_PAGE_SIZE === 100 ? 2 : 2);

  await pager.loadMore();
  await pager.loadMore();
  assert.deepEqual(pager.snapshot().commits.map((item) => item.oid), ["a", "b", "c", "d"]);
  assert.equal(pager.snapshot().exhausted, true);
  assert.equal(requests[1].cursor, "t1/q1:2");
  assert.equal(requests[1].start, undefined, "续页只带游标，不重解析起点");
  // 已到末尾后不再发请求。
  await pager.loadMore();
  assert.equal(requests.length, 3);
  // 每次通知里的行数只增不减，最终是全部四条。
  assert.equal(snapshots[snapshots.length - 1], 4);
  for (let index = 1; index < snapshots.length; index += 1) {
    assert.ok(snapshots[index] >= snapshots[index - 1], "已加载行数不能倒退");
  }
});

test("分页：超过跨度上限是明确状态，不伪装成历史末尾", async () => {
  const pager = new LogPager({
    targetId: "t1",
    loader: async () => {
      throw {
        kind: "rejected",
        code: "errors.history.historyPagingExceededQuerySLimit",
        params: { "0": "20000" },
        message: "scanned 20000 commits without filling the query limit",
      };
    },
  });
  pager.setQuery({ text: "x", caseSensitive: false, regex: false, branches: [], authors: [], paths: [], date: null }, false);
  pager.reset("k");
  await new Promise((resolve) => setImmediate(resolve));
  const snapshot = pager.snapshot();
  assert.equal(snapshot.spanExceeded, true);
  assert.equal(snapshot.exhausted, true);
  assert.match(formatAppError(snapshot.error), /20000/);
  // 不自动重试。
  await pager.loadMore();
  assert.equal(pager.snapshot().error, snapshot.error);
});

test("分页：换筛选重新开始；旧查询的晚到响应不能写进新结果", async () => {
  const pending = [];
  const pager = new LogPager({
    targetId: "t1",
    loader: () => new Promise((resolve) => pending.push(resolve)),
  });
  pager.setQuery({ text: null, caseSensitive: false, regex: false, branches: [], authors: [], paths: [], date: null }, false);
  pager.reset("first");
  assert.equal(pending.length, 1);
  pager.reset("second", { force: true });
  assert.equal(pending.length, 2, "换筛选后重新查询");
  // 旧请求晚到：结果被丢弃。
  pending[0](page([commit("old")], null, true));
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(pager.snapshot().commits, []);
  pending[1](page([commit("new")], null, true));
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(pager.snapshot().commits.map((item) => item.oid), ["new"]);
});

test("分页：切换目标时清空，失败状态可重试", async () => {
  let fail = true;
  const pager = new LogPager({
    targetId: "t1",
    loader: async () => {
      if (fail) throw { kind: "git", message: "读取失败" };
      return page([commit("a")], null, true);
    },
  });
  pager.setQuery({ text: null, caseSensitive: false, regex: false, branches: [], authors: [], paths: [], date: null }, false);
  pager.reset("k");
  await new Promise((resolve) => setImmediate(resolve));
  assert.match(formatAppError(pager.snapshot().error), /读取失败/);
  assert.equal(pager.snapshot().spanExceeded, false);

  pager.setTarget("t2");
  assert.deepEqual(pager.snapshot().commits, []);
  fail = false;
  pager.reset("k2");
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(pager.snapshot().commits.map((item) => item.oid), ["a"]);
  assert.equal(pager.retains("a"), true);
  assert.equal(pager.retains("missing"), false);
  assert.equal(pager.retains(null), true);
});

test("分页：接近末尾才预取，滚回顶部不会重复请求", async () => {
  let calls = 0;
  const pager = new LogPager({
    targetId: "t1",
    pageSize: 100,
    loader: async () => {
      calls += 1;
      return page(
        Array.from({ length: 100 }, (_, index) => commit(`c${calls}-${index}`)),
        `t1/q1:${calls * 100}`,
        false,
      );
    },
  });
  pager.setQuery({ text: null, caseSensitive: false, regex: false, branches: [], authors: [], paths: [], date: null }, false);
  pager.reset("k");
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(calls, 1);
  // 可见行数远小于已加载数：不预取。
  pager.maybeLoadMore(1);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(calls, 1);
  // 可见行数接近已加载的 100 条：预取下一页。
  pager.maybeLoadMore(95);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(calls, 2);
});

// ---------- 审查第 7 项：引用只在监视事件到达时重读 ----------

const ref = (fullName, commit) => ({ fullName, shortName: fullName, kind: "branch", commit, current: false });

test("引用签名：同样的引用不触发重查，引用移动才触发", () => {
  const watch = new RefsWatch();
  const first = [ref("refs/heads/main", "aaa"), ref("refs/tags/v1", "bbb")];
  assert.equal(watch.observe(first), true, "第一份快照算一次变化");
  // 同样的引用（包括顺序不同）：不重查。
  assert.equal(watch.observe([ref("refs/tags/v1", "bbb"), ref("refs/heads/main", "aaa")]), false);
  // 分支前进：重查。
  assert.equal(watch.observe([ref("refs/heads/main", "ccc"), ref("refs/tags/v1", "bbb")]), true);
  // 引用删除：重查。
  assert.equal(watch.observe([ref("refs/tags/v1", "bbb")]), true);
  // 换目标后第一份快照重新算变化。
  watch.reset();
  assert.equal(watch.observe([ref("refs/tags/v1", "bbb")]), true);
  assert.equal(refsSignature(first), refsSignature([...first].reverse()));
});

test("日志不再定期轮询 history_refs，可信重读后核对引用", () => {
  const source = readFileSync(new URL("../src/features/log/LogView.tsx", import.meta.url), "utf8");
  // 没有任何定时器（审查第 7 项：后台持续执行 Git 会填满命令日志）。
  assert.doesNotMatch(source, /setInterval/);
  // 引用只在监视事件的 effect 里读取，且必须比较签名后才重查。
  const refsCalls = source.match(/ipc\s*\n?\s*\.historyRefs\(/g) ?? [];
  assert.equal(refsCalls.length, 2, "只有初始加载与监视事件两处读取引用");
  assert.match(source, /refsWatchRevision/);
  assert.match(source, /refsWatchRef\.current\.observe\(next\)/);

  // 所有可信快照共享门禁后的通知，包括提交/amend 写后重读与恢复路径。
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const acceptance = app.slice(app.indexOf("const acceptSnapshot ="), app.indexOf("const reviewRecoverySchedulerRef ="));
  assert.match(acceptance, /return false;[\s\S]*applyRefreshedSnapshot\(snap\);[\s\S]*setWatchRevision/);
  assert.equal((app.match(/setWatchRevision\(\(value\) => value \+ 1\)/g) ?? []).length, 1);
  for (const [start, end] of [
    ["const readAndAccept =", "const refresh ="],
    ["const refreshTarget =", "const recoveryReadOnce ="],
    ["const recoveryReadOnce =", "recoveryReadOnceRef.current ="],
  ]) {
    assert.match(app.slice(app.indexOf(start), app.indexOf(end)), /acceptSnapshot\(token, read\.snapshot\)/);
  }
  assert.match(app, /if \(notifications\.consume\(token\.activation, snapshot\)\) \{\s*refreshCoordinator\.request\(\)/);
  assert.match(app, /refsWatchRevision=\{watchRevision\}/);
  assert.match(app, /refreshKey: refsRevision \+ watchRevision/);
  assert.match(source, /\[firstParent, filter, headCommit, key, pager, refreshKey, resetLog, targetId\]/);
});

test("引用签名：检出到同一提交的另一分支也更新当前标记", () => {
  const watch = new RefsWatch();
  const main = ref("refs/heads/main", "aaa");
  const topic = ref("refs/heads/topic", "aaa");
  assert.equal(watch.observe([{ ...main, current: true }, topic]), true);
  assert.equal(watch.observe([main, { ...topic, current: true }]), true);
  assert.equal(watch.observe([{ ...topic, current: true }, main]), false);
});

test("历史目标生命周期不依赖每次渲染都会变化的页签容器", () => {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const tabs = readFileSync(new URL("../src/features/log/logTabs.ts", import.meta.url), "utf8");
  const lifecycle = app.slice(app.indexOf("* 打开只读历史目标"), app.indexOf("// 打开仓库 / 切换 worktree 时默认显示日志"));
  assert.match(app, /const dropCommitTarget = commitTabs\.dropTarget;/);
  assert.match(tabs, /const dropTarget = useCallback\([\s\S]*?\}, \[\]\);/);
  assert.match(lifecycle, /dropCommitTarget\(openedId\)/);
  assert.match(lifecycle, /\[dropCommitTarget, historyEpoch, setAppError, worktreeId\]/);
  assert.doesNotMatch(lifecycle, /\[commitTabs[,\]]/);
});

test("日志文件树保留 diffParent，不用展示分组的 parent 伪造比较身份", () => {
  const file = Object.freeze({ ...historyFile, openable: true });
  for (const [parent, diffParent] of [[null, "ordinary-parent"], [null, "merge-parent-1"],
    ["merge-parent-2", "merge-parent-2"], [null, null]]) {
    const group = Object.freeze({ parent, diffParent, files: Object.freeze([file]) });
    for (const grouped of [false, true]) {
      const tree = buildFileTree([group], grouped);
      let node = tree[0];
      while (node.kind !== "file") node = node.children[0];
      assert.equal(node.parent, diffParent);
      assert.equal(node.file, file);
      assert.equal(group.parent, parent);
    }
  }
});

test("日志分页发布不可变列表，首屏与续页能触发依赖且不改写旧快照", async () => {
  const pending = [];
  const pager = new LogPager({ targetId: "t1", loader: () => new Promise((resolve) => pending.push(resolve)) });
  pager.reset("query");
  const empty = pager.snapshot();
  Object.freeze(empty.commits);
  pending.shift()(page([commit("a")], "next-page", false));
  await new Promise((resolve) => setImmediate(resolve));
  const first = pager.snapshot();
  assert.notEqual(first.commits, empty.commits);
  assert.deepEqual(empty.commits, []);
  Object.freeze(first.commits);
  const reading = pager.loadMore();
  pending.shift()(page([commit("a"), commit("b")], null, true));
  await reading;
  assert.deepEqual(first.commits.map((item) => item.oid), ["a"]);
  assert.deepEqual(pager.snapshot().commits.map((item) => item.oid), ["a", "b"]);
});

test("日志预览复用完整只读渲染器，选择文件不强开预览，Enter 不重复冒泡", () => {
  const source = readFileSync(new URL("../src/features/log/LogView.tsx", import.meta.url), "utf8");
  assert.doesNotMatch(source, /function CommitDiffPreview|to: 200/);
  assert.match(source, /<CommitDiffView targetId=\{targetId\} file=\{selectedPreview.target\}/);
  assert.match(source, /<CommitMergeView targetId=\{targetId\} commit=\{selectedPreview.target.commit\}/);
  const select = source.slice(source.indexOf("const selectFile ="), source.indexOf("const activateFile ="));
  assert.doesNotMatch(select, /onPreviewOpen/);
  assert.match(source, /event\.stopPropagation\(\);\s*onActivateFile\(file, parent\)/);
  const open = source.slice(source.indexOf("const openFileTarget ="), source.indexOf("const toTarget ="));
  assert.match(open, /activateFile\(file,/);
});

test("多父 merge 默认自动按父分组，预览与页签按实际模式打开", () => {
  for (const count of [2, 3, 4]) {
    assert.equal(effectiveMergeMode(count, true), "byParent");
    assert.equal(effectiveMergeMode(count, false), count > 2 ? "byParent" : "combined");
  }
  const source = readFileSync(new URL("../src/features/log/LogView.tsx", import.meta.url), "utf8");
  assert.match(source, /mode: effectiveMergeMode\(detail.parents.length, state.mergeByParent\)/);
  assert.match(source, /details.merge.mode !== effectiveMergeMode\(details.detail.parents.length, state.mergeByParent\)/);
  assert.equal((source.match(/details.merge\?\.mode === "combined"/g) ?? []).length, 3);
  assert.doesNotMatch(source, /parents.length > 1 && !state.mergeByParent/);
});

test("刷新首批不能把尚未加载的旧选择误判为消失，只有完整结果才能改选", async () => {
  const pending = [];
  const pager = new LogPager({ targetId: "t1", loader: () => new Promise((resolve) => pending.push(resolve)) });
  const complete = (state) => state.exhausted && !state.spanExceeded && !state.error;
  pager.reset("initial");
  pending.shift()(page(Array.from({ length: 100 }, (_, i) => commit(`c${i}`)), "next", false));
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(selectedLogCommit(pager.snapshot().commits, null, false), "c0");
  const selected = "c99";
  pager.reset("new-head", { force: true });
  assert.equal(selectedLogCommit(pager.snapshot().commits, selected, complete(pager.snapshot())), selected);
  pending.shift()(page([commit("new"), ...Array.from({ length: 99 }, (_, i) => commit(`c${i}`))], "next", false));
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(selectedLogCommit(pager.snapshot().commits, selected, complete(pager.snapshot())), selected);
  const next = pager.loadMore();
  pending.shift()(page([commit(selected)], null, true));
  await next;
  assert.equal(selectedLogCommit(pager.snapshot().commits, selected, complete(pager.snapshot())), selected);
  assert.equal(selectedLogCommit([commit("new")], selected, true), "new");
  assert.equal(selectedLogCommit([], selected, true), null);
  assert.equal(selectedLogCommit([], selected, false), selected);
});

test("日志续页使用滚动后的视口末行，刷新恢复深视口前忽略临时夹紧事件", async () => {
  let calls = 0;
  const pager = new LogPager({ targetId: "t1", loader: async () => {
    calls += 1;
    return page(Array.from({ length: 100 }, (_, i) => commit(`p${calls}-${i}`)), `next-${calls}`, false);
  } });
  pager.reset("query");
  await new Promise((resolve) => setImmediate(resolve));
  pager.maybeLoadMore(logVisibleEnd(0, 520, 26));
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(calls, 1);
  pager.maybeLoadMore(logVisibleEnd(70 * 26, 520, 26));
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(calls, 2);
  const source = readFileSync(new URL("../src/features/log/LogView.tsx", import.meta.url), "utf8");
  assert.match(source, /pager.maybeLoadMore\(logVisibleEnd\(element.scrollTop, element.clientHeight, LOG_ROW_HEIGHT\)\)/);
  const scroll = source.slice(source.indexOf("const onScroll ="), source.indexOf("const moveSelection ="));
  assert.match(scroll, /if \(pendingScrollRef.current !== null\) return/);
  assert.match(source, /snapshot.commits.length < logVisibleEnd\(top, viewport, LOG_ROW_HEIGHT\)/);
});

test("日志全局键盘尊重已处理事件，方向导航只在表格焦点内执行", () => {
  const source = readFileSync(new URL("../src/features/log/LogView.tsx", import.meta.url), "utf8");
  const handler = source.slice(source.indexOf("const handler = (event: KeyboardEvent)"), source.indexOf('window.addEventListener("keydown", handler)'));
  assert.ok(handler.indexOf("if (event.defaultPrevented) return") < handler.indexOf("event.metaKey"));
  assert.ok(handler.indexOf("!tableRef.current?.contains(target)") < handler.indexOf('event.key === "ArrowDown"'));
});

test("日志隐藏时不恢复DOM滚动，恢复续页失败后释放滚动而非一直等待", () => {
  const source = readFileSync(new URL("../src/features/log/LogView.tsx", import.meta.url), "utf8");
  const restore = source.slice(source.indexOf("const top = pendingScrollRef.current;"), source.indexOf("const rows = snapshot.commits;"));
  assert.match(restore, /if \(!active \|\| top === null \|\| !targetId \|\| snapshot.loading\) return/);
  assert.match(restore, /if \(!snapshot.error && snapshot.commits.length < logVisibleEnd/);
  assert.doesNotMatch(restore, /snapshot.error\) return/);
  assert.match(restore, /setScrollTop\(element.scrollTop\);\s*pendingScrollRef.current = null/);
});

test("父提交分组不受平铺设置影响，空父组保留编号，首选文件跳过空组", () => {
  const file = { ...historyFile, openable: true };
  const groups = [
    { parent: "parent-1", diffParent: "parent-1", files: [] },
    { parent: "parent-2", diffParent: "parent-2", files: [file] },
    { parent: "parent-3", diffParent: "parent-3", files: [file] },
  ];
  for (const directory of [false, true]) {
    const tree = buildFileTree(groups, directory);
    assert.equal(tree.length, 3);
    assert.ok(tree.every((node) => node.kind === "parent"));
    assert.match(tree[0].name, /相对父提交 1/);
    assert.equal(tree[0].children.length, 0);
    assert.match(tree[1].name, /相对父提交 2/);
    let node = tree[1].children[0];
    while (node.kind !== "file") node = node.children[0];
    assert.equal(node.parent, "parent-2");
  }
  assert.equal(firstHistoryFile(groups).parent, "parent-2");
  assert.equal(firstHistoryFile(groups).file, file);
  assert.equal(firstHistoryFile([groups[0]]), null);
});

test("日志目录设置创建真实目录层级，平铺时直接显示文件", () => {
  const files = ["src/log/a.ts", "src/log/b.ts", "src/ui/c.ts", "root.txt"].map((path) => ({ ...historyFile, path }));
  const groups = [{ parent: null, diffParent: "p1", files }];
  assert.ok(buildFileTree(groups, false).every((node) => node.kind === "file"));
  const tree = buildFileTree(groups, true);
  assert.equal(tree[0].name, "src");
  assert.deepEqual(tree[0].children.map((node) => node.name), ["log", "ui"]);
  assert.deepEqual(tree[0].children[0].children.map((node) => node.file.path), ["src/log/a.ts", "src/log/b.ts"]);
  assert.equal(tree[1].file.path, "root.txt");
});

test("同一提交文件复用页签时更新比较父与分型，不重复或保留旧目标", () => {
  const request = { kind: "merge", worktreeId: "w1", targetId: "t1", commit: "merge-oid", path: "demo.ts", parent: null, title: "demo.ts（merge）" };
  const first = showCommitTab([], request);
  Object.freeze(first[0]); Object.freeze(first);
  const second = showCommitTab(first, { ...request, kind: "diff", parent: "parent-2", file: { ...historyFile, parent: "parent-2" } });
  const third = showCommitTab(second, { ...request, kind: "diff", parent: "parent-3", file: { ...historyFile, parent: "parent-3", diffId: "third-diff" } });
  assert.equal(third.length, 1);
  assert.equal(third[0].key, first[0].key);
  assert.equal(third[0].kind, "diff");
  assert.equal(third[0].parent, "parent-3");
  assert.equal(third[0].file.diffId, "third-diff");
  assert.equal(first[0].kind, "merge");
  assert.equal(second[0].parent, "parent-2");
  assert.deepEqual(showCommitTab(third, request), first);
  const source = readFileSync(new URL("../src/features/log/logTabs.ts", import.meta.url), "utf8");
  assert.match(source, /setTabs\(\(current\) => showCommitTab\(current, request\)\)/);
});

test("日志视口测量观察实际表格尺寸，内部预览布局变化也更新，忽略隐藏高度0", () => {
  const source = readFileSync(new URL("../src/features/log/LogView.tsx", import.meta.url), "utf8");
  assert.match(source, /if \(element.clientHeight > 0\) setViewport\(element.clientHeight\)/);
  assert.match(source, /const observer = new ResizeObserver\(measure\);\s*observer.observe\(element\)/);
  assert.match(source, /return \(\) => observer.disconnect\(\)/);
  assert.doesNotMatch(source, /setViewport\(element.clientHeight \|\| 400\)/);
});

test("日志切换目标后旧清理仍写回原目标，各自保留筛选、选择与滚动", () => {
  const store = new LogWorkspaceStore();
  const a = { filter: { ...defaultFilter(), text: "history" }, selectedCommit: "commit-a", scrollTop: 4200 };
  const b = { filter: { ...defaultFilter(), text: "fixture" }, selectedCommit: "commit-b", scrollTop: 26, mergeByParent: true };
  store.update("worktree-a", a);
  const updateA = (patch) => store.update("worktree-a", patch);
  assert.equal(store.read("worktree-b").selectedCommit, null);
  store.update("worktree-b", b);
  updateA({ scrollTop: 4226 });
  assert.deepEqual(store.read("worktree-b"), b);
  assert.deepEqual(store.read("worktree-a"), { ...a, scrollTop: 4226, mergeByParent: false });
  const hook = readFileSync(new URL("../src/features/log/useLogWorkspace.ts", import.meta.url), "utf8");
  assert.match(hook, /store\.current\.update\(key, patch\)/);
  assert.match(hook, /\}, \[key\]\)/);
});

test("日志视图按worktree隔离，历史目标同步核对归属与环境代际", () => {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  assert.match(app, /<LogView\s+key=\{worktreeId \?\? ""\}/);
  assert.match(app, /historySession\?\.worktreeId === worktreeId\s*&& historySession\.epoch === historyEpoch/);
  assert.match(app, /setHistorySession\(\{ worktreeId, epoch: historyEpoch, targetId: info\.targetId, refs: info\.refs \}\)/);
});

test("卸载日志后取消分页监听，在途旧页不再回填视图", async () => {
  let resolve;
  const pager = new LogPager({ targetId: "target-a", loader: () => new Promise((next) => { resolve = next; }) });
  const received = [];
  pager.setListener((snapshot) => received.push(snapshot));
  pager.reset("query");
  pager.setListener(null);
  const before = received.length;
  resolve(page([commit("a")], null, true));
  await new Promise((next) => setImmediate(next));
  assert.equal(received.length, before);
  const source = readFileSync(new URL("../src/features/log/LogView.tsx", import.meta.url), "utf8");
  assert.match(source, /return \(\) => pager\.setListener\(null\)/);
});


test("文件历史跳转使用完整OID并清除旧作者日期路径与分支条件", () => {
  const store = new LogWorkspaceStore();
  store.update("worktree", { filter: { ...defaultFilter(), text: "old", regex: true, caseSensitive: true,
    authors: ["Old Author <old@fixture.invalid>"], branches: ["refs/heads/unrelated"],
    paths: ["other.txt"], date: "range", since: "2020-01-01", until: "2020-01-02" }, scrollTop: 4200 });
  const oid = "abcdef0123456789".repeat(2) + "abcdef01";
  store.update("worktree", { selectedCommit: oid, scrollTop: 0, filter: filterForCommit(oid) });
  const state = store.read("worktree");
  assert.equal(state.selectedCommit, oid); assert.equal(state.scrollTop, 0);
  assert.deepEqual(toHistoryFilter(state.filter), { text: oid, caseSensitive: false, regex: false,
    branches: [], authors: [], paths: [], date: null });
});
