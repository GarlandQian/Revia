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
const { activeMergeEntry, closeEditorTab, retainEditorTabs, showDiffTab } =
  await import("../src/hooks/editorTabsState.ts");
imports.deregister();

function diff(path, changes = "unstaged", preview = false) {
  return Object.freeze({ key: `${changes}:${path}`, path, changes, preview });
}

function merge(path, changes = "unstaged") {
  return Object.freeze({ key: `merge:${path}`, path, changes, preview: false, kind: "merge" });
}

function entry(path, patch = {}) {
  return Object.freeze({
    path, changes: "unstaged", unmerged: true,
    fingerprint: "original", snapshotRevision: "revision-1", ...patch,
  });
}

test("关闭当前 diff 后按相邻页签的 merge 类型激活，不把它当普通 diff", () => {
  const current = diff("src/a.ts");
  const next = merge("src/b.ts");
  const tabs = Object.freeze([current, next]);
  const result = closeEditorTab(tabs, current.key, current.key);
  assert.deepEqual(result.tabs, [next]);
  assert.equal(result.activate, next);
  assert.equal(result.activate.kind, "merge");
  assert.deepEqual(tabs, [current, next], "不修改原页签数组");
});

test("关闭当前 merge 优先激活右邻页签，末尾则激活左邻页签", () => {
  const first = diff("src/a.ts");
  const current = merge("src/b.ts");
  const last = diff("src/c.ts");
  const middle = closeEditorTab(Object.freeze([first, current, last]), current.key, current.key);
  assert.equal(middle.activate, last);
  assert.deepEqual(middle.tabs, [first, last]);
  const end = closeEditorTab(Object.freeze([first, current]), current.key, current.key);
  assert.equal(end.activate, first);
  assert.deepEqual(end.tabs, [first]);
});

test("关闭最后一个 merge 清空激活目标", () => {
  const current = merge("src/a.ts");
  assert.deepEqual(closeEditorTab(Object.freeze([current]), current.key, current.key), {
    tabs: [], activate: null,
  });
});

test("关闭非当前页签不切换编辑器，同路径 diff 与 merge 也不混淆", () => {
  const sameFileDiff = diff("src/a.ts");
  const current = merge("src/a.ts");
  const result = closeEditorTab(Object.freeze([sameFileDiff, current]), current.key, sameFileDiff.key);
  assert.deepEqual(result.tabs, [current]);
  assert.equal(result.activate, undefined);
});

test("关闭全部始终返回清空激活目标，不留下合并页签状态", () => {
  const first = diff("src/a.ts");
  const current = merge("src/b.ts");
  const tabs = Object.freeze([first, current]);
  for (const active of [current.key, first.key, null]) {
    assert.deepEqual(closeEditorTab(tabs, active, null), { tabs: [], activate: null });
  }
  assert.deepEqual(closeEditorTab([], null, null), { tabs: [], activate: null });
});

test("关闭不存在的页签无副作用", () => {
  const current = merge("src/a.ts");
  const tabs = Object.freeze([current]);
  const result = closeEditorTab(tabs, current.key, "missing");
  assert.equal(result.tabs, tabs);
  assert.equal(result.activate, undefined);
});

test("新预览仅替换已有预览，保留固定 diff 与 merge 的顺序", () => {
  const pinned = diff("src/pinned.ts");
  const oldPreview = diff("src/old.ts", "unstaged", true);
  const merging = merge("src/conflict.ts");
  const nextPreview = diff("src/next.ts", "unstaged", true);
  const tabs = Object.freeze([pinned, oldPreview, merging]);
  assert.deepEqual(showDiffTab(tabs, nextPreview), [pinned, merging, nextPreview]);
  assert.deepEqual(tabs, [pinned, oldPreview, merging]);
});

test("新固定页签不会抢走已有预览", () => {
  const preview = diff("src/preview.ts", "unstaged", true);
  const pinned = diff("src/pinned.ts");
  assert.deepEqual(showDiffTab(Object.freeze([preview]), pinned), [preview, pinned]);
});

test("双击固定已有预览时保留位置且不创建重复页签", () => {
  const first = merge("src/a.ts");
  const preview = diff("src/b.ts", "unstaged", true);
  const last = diff("src/c.ts");
  const tabs = Object.freeze([first, preview, last]);
  assert.deepEqual(showDiffTab(tabs, diff(preview.path)), [first, { ...preview, preview: false }, last]);
  assert.equal(preview.preview, true, "不直接修改旧页签对象");
});

test("单击重开固定页签不会降级为预览，也不重复添加", () => {
  const pinned = diff("src/a.ts");
  const tabs = Object.freeze([pinned]);
  assert.equal(showDiffTab(tabs, diff(pinned.path, pinned.changes, true)), tabs);
  assert.equal(showDiffTab(tabs, pinned), tabs);
});

test("同路径的不同暂存组与合并页签以各自 key 区分", () => {
  const staged = diff("src/a.ts", "staged");
  const merging = merge("src/a.ts");
  const unstaged = diff("src/a.ts");
  assert.deepEqual(showDiffTab(Object.freeze([staged, merging]), unstaged), [staged, merging, unstaged]);
});

test("无关刷新和冲突指纹变化都保留当前合并目标，不触发重新打开", () => {
  const target = Object.freeze({ worktreeId: "worktree-a", path: "src/conflict.ts" });
  const original = entry(target.path);
  const changed = entry(target.path, { fingerprint: "external-change", snapshotRevision: "revision-2" });
  const unrelated = entry("src/other.ts", { unmerged: false });
  assert.equal(activeMergeEntry(target, { worktreeId: target.worktreeId, entries: [unrelated, original] }), original);
  assert.equal(activeMergeEntry(target, { worktreeId: target.worktreeId, entries: [unrelated, changed] }), changed);
  assert.equal(original.fingerprint, "original", "判断保留目标不会改写打开时的凭证");
});

test("不同 worktree 的同路径冲突不能继承当前合并编辑", () => {
  const target = { worktreeId: "worktree-a", path: "src/conflict.ts" };
  assert.equal(activeMergeEntry(target, { worktreeId: "worktree-b", entries: [entry(target.path)] }), null);
  assert.equal(activeMergeEntry(null, { worktreeId: target.worktreeId, entries: [entry(target.path)] }), null);
  assert.equal(activeMergeEntry(target, null), null);
});

test("冲突已解决或路径消失时不再保留当前合并编辑", () => {
  const target = { worktreeId: "worktree-a", path: "src/conflict.ts" };
  const resolved = entry(target.path, { unmerged: false });
  assert.equal(activeMergeEntry(target, { worktreeId: target.worktreeId, entries: [resolved] }), null);
  assert.equal(activeMergeEntry(target, { worktreeId: target.worktreeId, entries: [entry("src/other.ts")] }), null);
  assert.equal(activeMergeEntry(target, { worktreeId: target.worktreeId, entries: [] }), null);
});

test("同路径有多个列表条目时按未解决冲突识别当前合并目标", () => {
  const target = { worktreeId: "worktree-a", path: "src/conflict.ts" };
  const normal = entry(target.path, { unmerged: false, changes: "staged" });
  const conflict = entry(target.path);
  assert.equal(activeMergeEntry(target, { worktreeId: target.worktreeId, entries: [normal, conflict] }), conflict);
});

test("只改变冲突内容指纹和快照版本不替换页签对象", () => {
  const merging = merge("src/conflict.ts");
  const normal = diff("src/other.ts");
  const tabs = Object.freeze([merging, normal]);
  const entries = [
    entry(merging.path, { fingerprint: "changed", snapshotRevision: "revision-2" }),
    entry(normal.path, { unmerged: false }),
  ];
  assert.equal(retainEditorTabs(tabs, { entries }), tabs);
});

test("解决冲突后移除 merge，但同文件仍有改动时保留普通 diff", () => {
  const merging = merge("src/conflict.ts");
  const normal = diff(merging.path);
  const tabs = Object.freeze([merging, normal]);
  const next = retainEditorTabs(tabs, { entries: [entry(merging.path, { unmerged: false })] });
  assert.deepEqual(next, [normal]);
  assert.equal(next[0], normal);
  assert.deepEqual(tabs, [merging, normal]);
});

test("刷新移除消失的页签，空快照清空全部", () => {
  const kept = diff("src/kept.ts");
  const missing = diff("src/missing.ts");
  const missingMerge = merge("src/conflict.ts");
  const tabs = Object.freeze([kept, missing, missingMerge]);
  assert.deepEqual(retainEditorTabs(tabs, { entries: [entry(kept.path, { unmerged: false })] }), [kept]);
  assert.deepEqual(retainEditorTabs(tabs, { entries: [] }), []);
});

test("冲突条目换组时保留 merge 的身份并更新分组，普通 diff 不跨组继承", () => {
  const merging = merge("src/conflict.ts");
  const oldDiff = diff(merging.path);
  const tabs = Object.freeze([merging, oldDiff]);
  const next = retainEditorTabs(tabs, { entries: [entry(merging.path, { changes: "staged" })] });
  assert.deepEqual(next, [{ ...merging, changes: "staged" }]);
  assert.equal(next[0].key, merging.key);
  assert.equal(next[0].kind, "merge");
  assert.equal(next[0].preview, false);
  assert.equal(merging.changes, "unstaged");
});
