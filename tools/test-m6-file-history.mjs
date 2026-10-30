// Component hook regression; real Git and desktop acceptance remain separate.
import fs from "node:fs";
import vm from "node:vm";
import ts from "typescript";
import assert from "node:assert/strict";
import test from "node:test";

function historyFixture(revision = "a".repeat(40)) {
  const slots = []; let cursor = 0; let effects = []; let tree;
  const equal = (a, b) => a && b && a.length === b.length && a.every((v, i) => Object.is(v, b[i]));
  const hooks = {
    useState(initial) { const i = cursor++; slots[i] ??= { value: typeof initial === "function" ? initial() : initial }; return [slots[i].value, v => { slots[i].value = typeof v === "function" ? v(slots[i].value) : v; }]; },
    useRef(value) { const i = cursor++; slots[i] ??= { current: value }; return slots[i]; },
    useEffect(effect, deps) { const i = cursor++; if (!slots[i] || !equal(slots[i].deps, deps)) { const prior = slots[i]; slots[i] = { deps }; effects.push(() => { prior?.cleanup?.(); slots[i].cleanup = effect(); }); } },
  };
  const calls = []; const blameCalls = []; const notices = []; let held; let blameResponder;
  const entry = { oid: revision ?? "b".repeat(40), shortOid: "aaaaaaa", path: "中文 空格.txt", status: "A", subject: "old object", author: "fixture", authorTimeMs: 0, patch: "old object bytes" };
  const page = { entries: [entry], anchor: JSON.stringify([entry.oid]), hasMore: true, nextSkip: 1 };
  const api = {
    fileHistory: async (worktree, request) => { calls.push({ worktree, ...request }); return held ?? page; },
    fileBlame: async (worktree, request) => { blameCalls.push({ worktree, ...request }); return blameResponder
      ? blameResponder(request) : { path: request.path, revision: request.revision, lines: [] }; },
  };
  const texts = path => new Proxy(() => path, { get: (_t, p) => texts(path ? path + "." + String(p) : String(p)), apply: () => path });
  const jsx = (type, props) => ({ type, props });
  const PatchDiffView = () => null;
  const require = id => id === "react" ? hooks : id === "react/jsx-runtime" ? { jsx, jsxs: jsx } : id === "@/lib/ipc" ? { ipc: api }
    : id === "@/lib/i18n/index" ? { messages: texts(""), getLocale: () => "en" } : id === "@/lib/error" ? { formatAppError: () => "", toAppError: e => e }
      : id.includes("PatchDiffView") ? { PatchDiffView } : id.includes("codicon") ? { Codicon: () => null } : id.includes("Dialogs") ? { Modal: () => null } : {};
  const module = { exports: {} };
  const code = ts.transpileModule(fs.readFileSync(new URL("../src/features/m6/FileHistoryDialog.tsx", import.meta.url), "utf8"), {
    compilerOptions: { jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  vm.runInNewContext(code, { module, exports: module.exports, require, console, Set, Promise });
  const props = { worktreeId: "owned-fixture", path: entry.path, revision, showFileNames: true, onClose() {}, onJumpCommit() {}, onNotify(notice) { notices.push(notice); } };
  const nodes = () => { const found = []; const walk = node => { if (!node || typeof node !== "object") return; if (Array.isArray(node)) return node.forEach(walk); found.push(node); walk(node.props?.children); }; walk(tree); return found; };
  const flush = async () => { for (let n = 0; n < 4; n++) { cursor = 0; tree = module.exports.FileHistoryDialog(props); const current = effects; effects = []; current.forEach(effect => effect()); await new Promise(resolve => setImmediate(resolve)); } };
  const button = text => nodes().find(n => n.type === "button" && typeof n.props.children === "function" && n.props.children() === text);
  const all = value => nodes().find(n => n.type === "input" && n.props.type === "checkbox").props.onChange({ target: { checked: value } });
  return { props, calls, blameCalls, notices, nodes, flush, button, all, page,
    dispose() { for (const slot of slots) slot?.cleanup?.(); },
    get preview() { return nodes().find(n => n.type === PatchDiffView)?.props; },
    set held(value) { held = value; }, set blameResponder(value) { blameResponder = value; } };
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
  finish({ ...f.page, entries: [{ ...f.page.entries[0], patch: "late unrelated object" }] }); await f.flush();
  assert.equal(f.preview.patch, "old object bytes");
});


test("同OID不同历史路径逐行选中、刷新保留和blame使用被点击的文件", async () => {
  const f = historyFixture();
  const real = JSON.parse(fs.readFileSync(process.env.REVIA_M6_ERROR_FIXTURE + ".history-collision.json", "utf8"));
  const roots = real.entries.filter(entry => entry.status === "A");
  assert.equal(roots.length, 2); assert.equal(roots[0].oid, roots[1].oid);
  Object.assign(f.page, real); await f.flush();
  for (const entry of roots) {
    const row = f.nodes().find(n => n.type === "div" && n.props.onClick && n.props.children.some?.(n => n?.props?.title === entry.path));
    assert.ok(row); row.props.onClick(); await f.flush();
    assert.equal(f.preview.path, entry.path); assert.equal(f.preview.patch, entry.patch);
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
