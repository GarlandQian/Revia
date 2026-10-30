// Component wiring regression only; real Git and desktop acceptance are separate.
import fs from "node:fs";
import vm from "node:vm";
import ts from "typescript";
import assert from "node:assert/strict";
import test from "node:test";

function componentFixture() {
  const slots = []; let cursor = 0; let effects = []; let tree;
  const equal = (a, b) => a && b && a.length === b.length && a.every((v, i) => Object.is(v, b[i]));
  const hooks = {
    useState(initial) { const i = cursor++; slots[i] ??= { value: typeof initial === "function" ? initial() : initial }; return [slots[i].value, v => { slots[i].value = typeof v === "function" ? v(slots[i].value) : v; }]; },
    useRef(value) { const i = cursor++; slots[i] ??= { current: value }; return slots[i]; },
    useCallback(value, deps) { const i = cursor++; if (!slots[i] || !equal(slots[i].deps, deps)) slots[i] = { value, deps }; return slots[i].value; },
    useEffect(effect, deps) { const i = cursor++; if (!slots[i] || !equal(slots[i].deps, deps)) { const prior = slots[i]; slots[i] = { deps }; effects.push(() => { prior?.cleanup?.(); slots[i].cleanup = effect(); }); } },
  };
  const entry = { key: "version:0", oid: "a".repeat(40), message: "fixture stash", index: 0, timeMs: 0, branch: "main" };
  let patch = "first local comparison"; let calls = 0; let held; let entries = [entry]; let inspected; let archives = [];
  const api = { stashList: async () => entries, stashArchives: async () => archives, stashPreflight: async () => "token",
    stashInspect: async (_worktree, selected) => { calls++; inspected = selected; return held ? held : { entry: selected, files: [], patch, truncated: false, comparedWithLocal: true }; } };
  const texts = path => new Proxy(() => path, { get: (_t, p) => texts(path ? path + "." + String(p) : String(p)), apply: () => path });
  const jsx = (type, props) => ({ type, props });
  const PatchDiffView = () => null;
  const require = id => id === "react" ? hooks : id === "react/jsx-runtime" ? { jsx, jsxs: jsx } : id === "@/lib/ipc" ? { ipc: api }
    : id === "@/lib/i18n/index" ? { messages: texts(""), getLocale: () => "en" } : id === "@/lib/error" ? { formatAppError: () => "", toAppError: e => e }
      : id.includes("PatchDiffView") ? { PatchDiffView } : id.includes("codicon") ? { Codicon: () => null } : id.includes("Dialogs") ? { Modal: () => null } : {};
  const module = { exports: {} };
  const code = ts.transpileModule(fs.readFileSync(new URL("../src/features/m6/StashPanel.tsx", import.meta.url), "utf8"), {
    compilerOptions: { jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  vm.runInNewContext(code, { module, exports: module.exports, require, console, Set, Promise });
  const props = { compareLocal: false, onCompareLocal: async value => { props.compareLocal = value; return null; }, worktreeId: "fixture", activation: 1, refreshKey: "1", busy: false, focusOid: null, onNotify() {}, onRefresh() {} };
  const nodes = () => { const found = []; const walk = node => { if (!node || typeof node !== "object") return; if (Array.isArray(node)) return node.forEach(walk); found.push(node); walk(node.props?.children); }; walk(tree); return found; };
  const flush = async () => { for (let n = 0; n < 4; n++) { cursor = 0; tree = module.exports.StashPanel(props); const current = effects; effects = []; current.forEach(effect => effect()); await new Promise(resolve => setImmediate(resolve)); } };
  const preview = () => nodes().find(n => n.type === PatchDiffView)?.props;
  const refresh = () => nodes().find(n => n.type === "button" && n.props.title?.() === "stash.refresh").props.onClick();
  return { props, nodes, flush, preview, refresh, entry, set archives(value) { archives = value; }, set entries(value) { entries = value; }, get inspected() { return inspected; }, get calls() { return calls; }, set patch(value) { patch = value; }, set held(value) { held = value; } };
}

test("终端变更刷新和面板刷新均重读所选 stash 本地比较", async () => {
  const f = componentFixture(); await f.flush();
  const label = f.nodes().find(n => n.type === "label" && n.props.children.some(child => typeof child === "function" && child() === "stash.compareLocal"));
  label.props.children.find(n => n?.type === "input").props.onChange({ target: { checked: true } });
  await f.flush(); const before = f.calls;
  f.patch = "second comparison after terminal edit"; f.props.refreshKey = "2"; await f.flush();
  assert.ok(f.calls > before); assert.equal(f.preview().patch, "second comparison after terminal edit");
  f.patch = "third comparison after panel refresh"; f.refresh(); await f.flush();
  assert.equal(f.preview().patch, "third comparison after panel refresh");
});

test("刷新取消旧比较回复，迟到结果不能覆盖新 patch", async () => {
  const f = componentFixture(); await f.flush(); let finish;
  f.held = new Promise(resolve => { finish = resolve; }); f.props.refreshKey = "2"; await f.flush();
  f.held = null; f.patch = "new comparison"; f.props.refreshKey = "3"; await f.flush();
  finish({ entry: f.entry, files: [], patch: "late old comparison", truncated: false }); await f.flush();
  assert.equal(f.preview().patch, "new comparison");
});


test("相同OID的两行按具体出现项选择，比较方式由持久设置属性驱动", async () => {
  const f = componentFixture();
  f.entries = [{ ...f.entry, key: "v:0" }, { ...f.entry, key: "v:1" }];
  await f.flush();
  const rows = f.nodes().filter(n => n.type === "div" && n.props.onClick);
  assert.equal(rows.length, 2);
  rows[1].props.onClick(); await f.flush();
  assert.equal(f.inspected.key, "v:1");
  assert.equal(f.nodes().filter(n => n.type === "div" && n.props.onClick && n.props.className.includes("bg-[var(--list-inactiveselectionbackground)]")).length, 1);
  const label = f.nodes().find(n => n.type === "label" && n.props.children.some(child => typeof child === "function" && child() === "stash.compareLocal"));
  await label.props.children.find(n => n?.type === "input").props.onChange({ target: { checked: true } });
  await f.flush(); assert.equal(f.props.compareLocal, true);
  assert.ok(f.preview().identity.endsWith(":true"));
});


test("已恢复删除的自动stash查看动作打开并准确突出同OID归档", async () => {
  const f = componentFixture(); f.props.focusOid = f.entry.oid; await f.flush();
  assert.equal(f.inspected.oid, f.entry.oid);
  f.entries = []; f.archives = [
    { id: "other", stashOid: "b".repeat(40), identityConfirmed: true, message: "other archive", createdAtMs: 1, expiresAtMs: 2 },
    { id: "saved", stashOid: f.entry.oid, identityConfirmed: true, message: "saved archive", createdAtMs: 1, expiresAtMs: 2 },
  ];
  f.props.refreshKey = "after-cleanup"; await f.flush();
  const highlighted = f.nodes().filter(n => n.type === "li" && n.props["aria-current"] === "true");
  assert.equal(highlighted.length, 1); assert.ok(highlighted[0].props.children[0].props.children.some(n => n?.props?.title === "saved archive"));
});
