// Component hook regression; real Git and desktop acceptance remain separate.
import fs from "node:fs";
import vm from "node:vm";
import ts from "typescript";
import assert from "node:assert/strict";
import test from "node:test";

function historyFixture(revision = "a".repeat(40)) {
  const slots = []; let cursor = 0; let effects = []; let tree; let disposed = false;
  let writesAfterDispose = 0;
  const equal = (a, b) => a && b && a.length === b.length && a.every((v, i) => Object.is(v, b[i]));
  const hooks = {
    useState(initial) { const i = cursor++; slots[i] ??= { value: typeof initial === "function" ? initial() : initial }; return [slots[i].value, v => { if (disposed) writesAfterDispose++; slots[i].value = typeof v === "function" ? v(slots[i].value) : v; }]; },
    useRef(value) { const i = cursor++; slots[i] ??= { current: value }; return slots[i]; },
    useMemo(factory, deps) { const i = cursor++; if (!equal(slots[i]?.deps, deps)) slots[i] = { deps, value: factory() }; return slots[i].value; },
    useEffect(effect, deps) { const i = cursor++; if (!slots[i] || !equal(slots[i].deps, deps)) { const prior = slots[i]; slots[i] = { deps }; effects.push(() => { prior?.cleanup?.(); slots[i].cleanup = effect(); }); } },
  };
  const calls = []; const patchCalls = []; const blameCalls = []; const notices = []; let held; let blameResponder;
  let patchResponder = () => ({ patch: "old object bytes", patchTruncated: false });
  const entry = { oid: revision ?? "b".repeat(40), shortOid: "aaaaaaa", path: "中文 空格.txt", oldPath: null, status: "A", subject: "old object", author: "fixture", authorTimeMs: 0 };
  const page = { entries: [entry], anchor: JSON.stringify([entry.oid]), hasMore: true, nextSkip: 1 };
  const api = {
    fileHistory: async (worktree, request) => { calls.push({ worktree, ...request }); return structuredClone(await (held ?? page)); },
    fileHistoryPatch: async (worktree, request) => { patchCalls.push({ worktree, ...request }); return patchResponder(request); },
    fileBlame: async (worktree, request) => { blameCalls.push({ worktree, ...request }); return blameResponder
      ? blameResponder(request) : { path: request.path, revision: request.revision, lines: [] }; },
  };
  const texts = path => new Proxy(() => path, { get: (_t, p) => texts(path ? path + "." + String(p) : String(p)), apply: () => path });
  const jsx = (type, props) => ({ type, props });
  const PatchDiffView = () => null;
  const require = id => id === "react" ? hooks : id === "react/jsx-runtime" ? { jsx, jsxs: jsx } : id === "@/lib/ipc" ? { ipc: api }
    : id === "@/lib/i18n/index" ? { messages: texts(""), getLocale: () => "en" } : id === "@/lib/error" ? { formatAppError: error => error.code, toAppError: e => e }
      : id.includes("PatchDiffView") ? { PatchDiffView } : id.includes("codicon") ? { Codicon: () => null } : id.includes("Dialogs") ? { Modal: () => null } : {};
  const module = { exports: {} };
  const code = ts.transpileModule(fs.readFileSync(new URL("../src/features/m6/FileHistoryDialog.tsx", import.meta.url), "utf8"), {
    compilerOptions: { jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  vm.runInNewContext(code, { module, exports: module.exports, require, console, Set, Promise });
  const props = { worktreeId: "owned-fixture", path: entry.path, revision, showFileNames: true, onClose() {}, onJumpCommit() {}, onNotify(notice) { notices.push(notice); } };
  const nodes = () => { const found = []; const walk = node => { if (!node || typeof node !== "object") return; if (Array.isArray(node)) return node.forEach(walk); found.push(node); walk(node.props?.children); }; walk(tree); return found; };
  const render = () => { cursor = 0; tree = module.exports.FileHistoryDialog(props); };
  const flush = async () => { for (let n = 0; n < 4; n++) { render(); const current = effects; effects = []; current.forEach(effect => effect()); await new Promise(resolve => setImmediate(resolve)); } };
  const button = text => nodes().find(n => n.type === "button" && typeof n.props.children === "function" && n.props.children() === text);
  const all = value => nodes().find(n => n.type === "input" && n.props.type === "checkbox").props.onChange({ target: { checked: value } });
  const select = subject => {
    const row = nodes().find(n => n.type === "div" && n.props.onClick &&
      n.props.children[0]?.props.children.some(child => child.props.title === subject));
    assert.ok(row, subject); row.props.onClick();
  };
  return { props, calls, patchCalls, blameCalls, notices, nodes, render, flush, button, all, page, select,
    dispose() { disposed = true; for (const slot of slots) slot?.cleanup?.(); },
    get writesAfterDispose() { return writesAfterDispose; },
    get selectedSubject() { return nodes().find(n => n.type === "span" && n.props.className?.includes("font-semibold"))?.props.children; },
    get loadingPatch() { return nodes().some(n => n.props.role === "status" && n.props.children() === "fileHistory.loading"); },
    get preview() { return nodes().find(n => n.type === PatchDiffView)?.props; },
    set held(value) { held = value; }, set blameResponder(value) { blameResponder = value; },
    set patchResponder(value) { patchResponder = value; } };
}

test("旧日志对象的首查、刷新、分页和初次blame保留固定提交", async () => {
  const f = historyFixture(); await f.flush();
  assert.equal(f.calls[0].anchor, f.props.revision); assert.equal(f.calls[0].worktree, f.props.worktreeId);
  assert.equal(f.preview.patch, "old object bytes");
  f.button("fileHistory.loadMore").props.onClick(); await f.flush();
  assert.equal(f.calls.at(-1).anchor, f.page.anchor); assert.equal(f.calls.at(-1).skip, 1);
  f.nodes().find(n => n.type === "button" && n.props.title?.() === "fileHistory.refresh").props.onClick(); await f.flush();
  assert.equal(f.calls.at(-1).anchor, f.props.revision); assert.equal(f.calls.at(-1).skip, 0);
  f.button("fileHistory.blame").props.onClick(); await f.flush();
  assert.equal(f.blameCalls.at(-1).revision, f.props.revision); assert.equal(f.blameCalls.at(-1).path, f.props.path);
});

test("工作文件默认HEAD；显式所有分支后切回旧版本，迟到回复不能换对象", async () => {
  const working = historyFixture(null); await working.flush(); assert.equal(working.calls[0].anchor, undefined);
  const f = historyFixture(); await f.flush(); let finish;
  f.held = new Promise(resolve => { finish = resolve; }); f.all(true); await f.flush();
  assert.equal(f.calls.at(-1).allBranches, true); assert.equal(f.calls.at(-1).anchor, undefined);
  f.held = null; f.all(false); await f.flush(); assert.equal(f.calls.at(-1).anchor, f.props.revision);
  finish({ ...f.page, entries: [{ ...f.page.entries[0], subject: "late unrelated object" }] }); await f.flush();
  assert.ok(!f.nodes().some(n => n.props.title === "late unrelated object"));
  assert.equal(f.preview.patch, "old object bytes");
});


function pendingPatches() {
  const f = historyFixture();
  const pending = [];
  f.patchResponder = () => new Promise((resolve, reject) => pending.push({ resolve, reject }));
  return { f, pending };
}

test("满页列表先显示，翻页保留选择，A→C→A只接受最后一次读取", async () => {
  const { f, pending } = pendingPatches();
  const first = f.page.entries[0];
  const entries = Array.from({ length: 40 }, (_, index) => {
    const oid = (index + 1).toString(16).padStart(40, "0");
    return { ...first, oid, shortOid: oid.slice(-7), subject: `commit ${index + 1}` };
  });
  entries[2] = { ...entries[2], status: "R", oldPath: "旧路径.txt" };
  Object.assign(f.page, { entries, nextSkip: 40 });
  await f.flush();
  assert.equal(f.nodes().filter(n => n.props.onDoubleClick).length, 40);
  assert.equal(f.selectedSubject, entries[0].subject);
  assert.equal(f.loadingPatch, true);
  assert.deepEqual(f.patchCalls.map(call => call.oid), [entries[0].oid]);

  f.held = { ...f.page, entries: [{ ...first, subject: "commit 41" }], hasMore: false, nextSkip: 41 };
  f.button("fileHistory.loadMore").props.onClick();
  await f.flush();
  assert.equal(f.nodes().filter(n => n.props.onDoubleClick).length, 41);
  assert.equal(f.selectedSubject, entries[0].subject);
  assert.equal(f.calls.at(-1).skip, 40);
  assert.equal(f.patchCalls.length, 1);

  f.select(entries[2].subject);
  f.render();
  assert.equal(f.selectedSubject, entries[2].subject);
  assert.equal(f.loadingPatch, true);
  await f.flush();
  assert.deepEqual(f.patchCalls[1], {
    worktree: f.props.worktreeId, oid: entries[2].oid, path: entries[2].path, oldPath: entries[2].oldPath,
  });
  f.select(entries[0].subject);
  await f.flush();
  assert.deepEqual(f.patchCalls.map(call => call.oid), [entries[0].oid, entries[2].oid, entries[0].oid]);
  pending[0].resolve({ patch: "first A", patchTruncated: true });
  await f.flush();
  assert.equal(f.loadingPatch, true);
  assert.equal(f.preview, undefined);
  pending[2].resolve({ patch: "latest A", patchTruncated: true });
  await f.flush();
  pending[1].reject({ code: "late error" });
  await f.flush();
  assert.equal(f.preview.patch, "latest A");
  assert.ok(f.nodes().some(n => n.type === "span" && typeof n.props.children === "function" && n.props.children() === "fileHistory.patchTruncated"));
  assert.ok(!f.nodes().some(n => n.props.role === "alert"));
});

test("当前条目错误留在预览区，切换到另一条后加载并显示空 diff", async () => {
  const { f, pending } = pendingPatches();
  f.page.entries.push({ ...f.page.entries[0], oid: "2".repeat(40), subject: "empty object" });
  await f.flush();
  pending[0].reject({ code: "errors.fileHistory.patchUnavailable" });
  await f.flush();
  assert.ok(f.nodes().some(n => n.props.role === "alert" && n.props.children === "errors.fileHistory.patchUnavailable"));
  f.select("empty object");
  f.render();
  assert.equal(f.selectedSubject, "empty object");
  assert.equal(f.loadingPatch, true);
  assert.ok(!f.nodes().some(n => n.props.role === "alert"));
  await f.flush();
  pending[1].resolve({ patch: "", patchTruncated: false });
  await f.flush();
  assert.ok(f.nodes().some(n => n.type === "p" && typeof n.props.children === "function" && n.props.children() === "fileHistory.noPatch"));
});

for (const [key, value] of [["worktreeId", "other-worktree"], ["path", "other-file"], ["revision", "c".repeat(40)]]) {
  test(`变更 ${key} 后只读取新列表里的条目，关闭后忽略在途响应`, async () => {
    const { f, pending } = pendingPatches();
    await f.flush();
    let finishList;
    f.held = new Promise(resolve => { finishList = resolve; });
    f.props[key] = value;
    f.render();
    assert.equal(f.selectedSubject, undefined);
    await f.flush();
    pending[0].resolve({ patch: "from previous view", patchTruncated: false });
    await f.flush();
    assert.equal(f.patchCalls.length, 1);
    assert.equal(f.preview, undefined);
    finishList(f.page);
    await f.flush();
    assert.equal(f.loadingPatch, true);
    assert.equal(f.patchCalls.length, 2);
    assert.equal(f.patchCalls[1].worktree, f.props.worktreeId);

    let failNextPage;
    f.held = new Promise((_, reject) => { failNextPage = reject; });
    f.button("fileHistory.loadMore").props.onClick();
    await f.flush();
    f.dispose();
    pending[1].reject({ code: "late patch error" });
    failNextPage({ code: "late page error" });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(f.writesAfterDispose, 0);
    assert.equal(f.notices.length, 0);
  });
}

test("同OID不同历史路径逐行选中、刷新保留和blame使用被点击的文件", async () => {
  const f = historyFixture();
  const real = JSON.parse(fs.readFileSync(process.env.REVIA_M6_ERROR_FIXTURE + ".history-collision.json", "utf8"));
  const roots = real.page.entries.filter(entry => entry.status === "A");
  assert.equal(roots.length, 2); assert.equal(roots[0].oid, roots[1].oid);
  const patchFor = entry => {
    const match = real.patches.find(({ request }) => request.oid === entry.oid && request.path === entry.path && request.oldPath === entry.oldPath);
    assert.ok(match); return match.result;
  };
  f.patchResponder = patchFor;
  Object.assign(f.page, real.page); await f.flush();
  for (const entry of roots) {
    const row = f.nodes().find(n => n.type === "div" && n.props.onClick && n.props.children.some?.(n => n?.props?.title === entry.path));
    assert.ok(row); row.props.onClick(); await f.flush();
    assert.equal(f.preview.path, entry.path); assert.equal(f.preview.patch, patchFor(entry).patch);
    assert.equal(f.nodes().filter(n => n.type === "div" && n.props.onClick && n.props.className.includes("bg-[var(--list-inactiveselectionbackground)]")).length, 1);
    f.nodes().find(n => n.type === "button" && n.props.title?.() === "fileHistory.refresh").props.onClick(); await f.flush();
    assert.equal(f.preview.path, entry.path);
    f.button("fileHistory.blameThis").props.onClick(); await f.flush();
    assert.equal(f.blameCalls.at(-1).path, entry.path); assert.equal(f.blameCalls.at(-1).revision, entry.oid);
    f.button("fileHistory.history").props.onClick(); await f.flush();
  }
});

function pendingBlameFixture() {
  const real = JSON.parse(fs.readFileSync(process.env.REVIA_M6_ERROR_FIXTURE + ".blame-options.json", "utf8"));
  const f = historyFixture(real.revision); const pending = [];
  Object.assign(f.page, real.history);
  const resultFor = request => {
    const actual = real.variants.find(item => ["path", "revision", "ignoreWhitespace", "detectMoves", "detectCopies"]
      .every(key => item.request[key] === request[key]));
    assert.ok(actual, `missing real Git response: ${JSON.stringify(request)}`); return actual.result;
  };
  f.blameResponder = request => new Promise((resolve, reject) => pending.push({ request, reject, finish: () => resolve(resultFor(request)) }));
  const checkbox = index => f.nodes().filter(n => n.type === "input" && n.props.type === "checkbox")[index];
  const refresh = () => f.nodes().find(n => n.type === "button" && n.props.title?.() === "fileHistory.refresh").props.onClick();
  return { f, real, pending, checkbox, refresh };
}

for (const [index, key, checked] of [[0, "ignoreWhitespace", false], [1, "detectMoves", true], [2, "detectCopies", true]]) {
  test(`首次blame在途切换${key}立即重读并拒绝旧参数响应`, async () => {
    const { f, real, pending, checkbox } = pendingBlameFixture(); await f.flush();
    f.button("fileHistory.blame").props.onClick(); await f.flush(); assert.equal(pending.length, 1);
    checkbox(index).props.onChange({ target: { checked } }); await f.flush(); assert.equal(pending.length, 2);
    assert.equal(pending[1].request[key], checked); assert.equal(pending[1].request.revision, real.revision);
    assert.equal(pending[1].request.path, real.path);
    pending[1].finish(); await f.flush(); pending[0].finish(); await f.flush();
    const expected = key === "ignoreWhitespace" ? real.revision : real.base;
    assert.equal(checkbox(index).props.checked, checked);
    assert.ok(f.nodes().some(n => n.type === "button" && n.props.title === expected));
    assert.equal(f.notices.length, 0);
  });
}

test("切换blame版本期间的选项读取绑定新目标，旧版本迟到不能覆盖", async () => {
  const { f, real, pending, checkbox, refresh } = pendingBlameFixture(); await f.flush();
  f.button("fileHistory.blame").props.onClick(); await f.flush(); pending[0].finish(); await f.flush();
  refresh(); await f.flush();
  f.button("fileHistory.history").props.onClick(); await f.flush();
  const base = real.history.entries.find(entry => entry.oid === real.base);
  const row = f.nodes().find(n => n.type === "div" && n.props.onClick
    && n.props.children[0]?.props?.children?.some?.(child => child?.props?.children === base.shortOid));
  assert.ok(row); row.props.onClick(); await f.flush();
  f.button("fileHistory.blameThis").props.onClick(); await f.flush();
  checkbox(0).props.onChange({ target: { checked: false } }); await f.flush();
  assert.equal(pending.length, 4); assert.equal(pending[3].request.revision, real.base);
  assert.equal(pending[3].request.ignoreWhitespace, false);
  pending[3].finish(); await f.flush(); pending[2].finish(); pending[1].finish(); await f.flush();
  assert.ok(f.nodes().some(n => n.type === "button" && n.props.title === real.base));
  assert.ok(f.nodes().some(n => n.type === "td" && n.props.children === "alpha"));
  assert.ok(!f.nodes().some(n => n.type === "td" && n.props.children === "  alpha"));
});

test("blame失败统一通知，重新读取成功后清除原错误", async () => {
  const { f, real, pending, refresh } = pendingBlameFixture(); await f.flush();
  f.button("fileHistory.blame").props.onClick(); await f.flush(); pending[0].reject(real.missingError); await f.flush();
  assert.equal(f.notices.length, 1); assert.ok(f.nodes().some(n => n.props?.role === "alert"));
  refresh(); await f.flush(); pending[1].finish(); await f.flush();
  assert.ok(!f.nodes().some(n => n.props?.role === "alert")); assert.equal(f.notices.length, 1);
});

test("关闭文件历史后在途blame失败不再发通知", async () => {
  const { f, real, pending } = pendingBlameFixture(); await f.flush();
  f.button("fileHistory.blame").props.onClick(); await f.flush(); f.dispose();
  pending[0].reject(real.missingError); await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.notices.length, 0);
});
