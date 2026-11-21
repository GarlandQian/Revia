import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { registerHooks } from "node:module";
import test from "node:test";
import { createElement, useState } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import ts from "typescript";

const src = new URL("../src/", import.meta.url);
const modules = registerHooks({
  resolve(specifier, context, next) {
    if (specifier === "./FileTypeIcon") return next("data:text/javascript,export function FileTypeIcon(){}");
    let source;
    if (specifier.startsWith("@/")) source = new URL(specifier.slice(2), src);
    else if (specifier.startsWith(".") && context.parentURL?.startsWith(src.href)) source = new URL(specifier, context.parentURL);
    if (!source) return next(specifier, context);
    if (!/\.[^/]+$/.test(source.pathname)) source.pathname += source.pathname.endsWith("/app-icon") ? ".tsx" : ".ts";
    return next(source.href, context);
  },
  load(url, context, next) {
    if (!url.endsWith(".tsx")) return next(url, context);
    const source = ts.transpileModule(readFileSync(new URL(url), "utf8"), {
      compilerOptions: { jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
    }).outputText;
    return { format: "module", source, shortCircuit: true };
  },
});
const { ChangesTree, TREE_ROW_HEIGHT } = await import("../src/components/app/ChangesTree.tsx");
const { useTreeSelection } = await import("../src/hooks/useTreeSelection.ts");
const { runFileSelectionAction } = await import("../src/hooks/changesTreeState.ts");
const { changelistShortcutEntries } = await import("../src/lib/m7/visibleChangelistSelection.ts");
const { isMac } = await import("../src/lib/platform.ts");
const { matchShortcut, setShortcutOverrides } = await import("../src/lib/shortcuts.ts");
modules.deregister();

const primary = isMac ? { metaKey: true } : { ctrlKey: true };
const entry = (path, changes = "unstaged") => ({ path, changes, fingerprint: path });
const snapshot = (entries, worktreeId = "fixture") => ({ worktreeId, entries });
const paths = (entries) => entries.map((row) => row.path);
const noop = () => {};

// 每个事件后由 React 重新渲染同一组件，保留生产 hook 的状态。收集整文件入口的目标，
// Git 写入和浏览器原生键盘分派另由固定构建的桌面验收检查。
function exercise(initial, byDirectory, steps) {
  const writes = [];
  const confirms = [];
  function Fixture() {
    const [step, advance] = useState(0);
    const [current, setCurrent] = useState(initial);
    const [preview, open] = useState(null);
    const model = useTreeSelection(current, byDirectory, open);
    const write = async (kind, entries) => { writes.push([kind, ...paths(entries)]); };
    const act = (action) => runFileSelectionAction(action, model.selectedEntries, write,
      (entries) => confirms.push(paths(entries)), () => true);
    const tree = ChangesTree({
      items: model.items, active: preview, selectedKeys: model.keys, directoryCounts: model.directoryCounts,
      open: model.groupOpen, dirOpen: model.dirOpen, onActivate: model.activateTreeEntry,
      onMove: model.moveTreeSelection, onToggleDir: model.toggleDir, onStageSelection: () => act("stageFile"),
      onStageEntry: (row) => write("stage", model.expandToSelection(row)),
    });
    if (step === steps.length) return null;
    const nodes = tree.props.children.props.children;
    const row = (path) => {
      const element = nodes.find((item) => item.props.entry?.path === path);
      assert.ok(element, `rendered row ${path}`);
      return element.type(element.props);
    };
    const findButton = (node, className) => {
      if (!node || typeof node !== "object") return null;
      if (Array.isArray(node)) {
        for (const item of node) {
          const hit = findButton(item, className);
          if (hit) return hit;
        }
        return null;
      }
      if (node.type === "button" && String(node.props?.className ?? "").includes(className)) return node;
      return findButton(node.props?.children, className);
    };
    const directory = (path) => nodes.find((node) => node.key === `dir-unstaged-${path}`).props.children;
    steps[step]({
      writes, confirms, preview, model,
      click: (path, modifiers = {}) => row(path).props.children[0].props.onClick(modifiers),
      stageInline: (path) => findButton(row(path).props.children.slice(1), "mini-icon-btn").props.onClick({ stopPropagation: noop }),
      selection: () => paths(model.selectedEntries),
      directory,
      key: (key, modifiers = {}) => {
        const nativeEvent = { key, code: key === " " ? "Space" : key, ...modifiers };
        tree.props.onKeyDown({ preventDefault: noop, stopPropagation: noop, ...nativeEvent, nativeEvent });
      },
      shortcut: (code) => act(matchShortcut({ key: code, code, altKey: true, ...primary })),
      refresh: (next, first, opened = false) => {
        setCurrent(next);
        open(first);
        model.receiveTreeSnapshot(next, first, opened);
      },
      scroll: (rowIndex) => tree.props.onScroll({ currentTarget: { scrollTop: rowIndex * TREE_ROW_HEIGHT } }),
    });
    advance(step + 1);
    return null;
  }
  renderToStaticMarkup(createElement(Fixture));
}

test("取消 B 保留 B 的预览，批量快捷键只暂存 A、C", () => {
  const first = snapshot(["A", "B", "C"].map((path) => entry(path)));
  exercise(first, false, [
    (f) => f.refresh(first, first.entries[0], true),
    (f) => { assert.deepEqual(f.selection(), ["A"]); f.click("B", primary); },
    (f) => f.click("C", primary),
    (f) => f.click("B", primary),
    (f) => {
      assert.equal(f.preview.path, "B");
      assert.deepEqual(f.selection(), ["A", "C"]);
      f.shortcut("KeyA");
      assert.deepEqual(f.writes, [["stage", "A", "C"]]);
    },
  ]);
});

test("更改列表选 B 的 diff 未载入时，暂存和丢弃均指向新选区", async () => {
  const current = snapshot([entry("A", "untracked"), entry("B", "untracked")]);
  const selection = { worktreeId: current.worktreeId, paths: ["B"] };
  const writes = [], confirmations = [];
  const targets = changelistShortcutEntries(current, selection, "A");
  for (const action of ["stageFile", "discardFile"]) {
    await runFileSelectionAction(action, targets, async (_, rows) => writes.push(paths(rows)),
      (rows) => confirmations.push(paths(rows)), () => true);
  }
  assert.deepEqual(writes, [["B"]]);
  assert.deepEqual(confirmations, [["B"]]);
});

test("自定义刷新占用空格时改动树让按键交给全局动作", () => {
  setShortcutOverrides({ refresh: "space" });
  try {
    const first = snapshot([entry("A")]);
    exercise(first, false, [
      (f) => f.click("A"),
      (f) => {
        assert.equal(matchShortcut({ key: " ", code: "Space" }), "refresh");
        f.key(" ", { stopPropagation: () => assert.fail("custom shortcut was intercepted") });
        assert.deepEqual(f.writes, []);
      },
    ]);
  } finally {
    setShortcutOverrides({});
  }
});

test("空选区经历刷新后保留，空格和两个整文件快捷键均不写入或确认", () => {
  const first = snapshot([entry("A"), entry("B")]);
  const next = snapshot([entry("B"), entry("A")]);
  exercise(first, false, [
    (f) => f.click("A"),
    (f) => f.click("A", primary),
    (f) => { assert.equal(f.preview.path, "A"); f.refresh(next, next.entries[0]); },
    (f) => {
      assert.equal(f.preview.path, "B");
      assert.deepEqual(f.selection(), []);
      f.shortcut("KeyZ"); f.shortcut("KeyA"); f.key(" ");
      assert.deepEqual(f.writes, []); assert.deepEqual(f.confirms, []);
    },
    (f) => f.refresh(snapshot(next.entries, "other"), next.entries[0], true),
    (f) => assert.deepEqual(f.selection(), ["B"]),
  ]);
});

test("选区外 B 的行内按钮只处理 B，写后换组仍可用空格暂存 A、C", () => {
  const first = snapshot(["A", "B", "C"].map((path) => entry(path)));
  const next = snapshot([entry("B", "staged"), entry("A"), entry("C")]);
  exercise(first, false, [
    (f) => f.click("A"),
    (f) => f.click("C", primary),
    (f) => { f.stageInline("B"); assert.deepEqual(f.writes, [["stage", "B"]]); },
    (f) => f.refresh(next, next.entries[0]),
    (f) => {
      assert.deepEqual(f.selection(), ["A", "C"]);
      f.key(" ");
      assert.deepEqual(f.writes.at(-1), ["stage", "A", "C"]);
    },
  ]);
});

test("鼠标与键盘 Shift 均遵循展开顺序，折叠已选目录显示数量且仍可批量写", () => {
  const first = snapshot(["root", "b/hidden", "c/C", "a/A", "a/B"].map((path) => entry(path)));
  exercise(first, true, [
    (f) => f.directory("a").props.onClick(),
    (f) => f.directory("c").props.onClick(),
    (f) => f.click("a/A"),
    (f) => f.click("c/C", { shiftKey: true }),
    (f) => {
      assert.deepEqual(f.selection().sort(), ["a/A", "a/B", "c/C"]);
      f.key("ArrowUp", { shiftKey: true });
    },
    (f) => {
      assert.deepEqual(f.selection().sort(), ["a/A", "a/B"]);
      f.key("ArrowDown", { shiftKey: true });
    },
    (f) => f.directory("a").props.onClick(),
    (f) => {
      assert.equal(f.directory("a").props.children.at(-1).props.children, "已选 2");
      f.shortcut("KeyA");
      assert.deepEqual(f.writes[0].slice(1).sort(), ["a/A", "a/B", "c/C"]);
    },
  ]);
});

test("Shift 包含需滚动才渲染的中间文件", () => {
  const first = snapshot(Array.from({ length: 70 }, (_, i) => entry(`row-${i}`)));
  exercise(first, false, [
    (f) => f.click("row-0"),
    (f) => f.scroll(60),
    (f) => f.click("row-60", { shiftKey: true }),
    (f) => assert.deepEqual(f.selection(), paths(first.entries.slice(0, 61))),
  ]);
});

test("同一路径换组不继承选区，折叠计数区分两组和目录前缀", () => {
  const first = snapshot([entry("src/A"), entry("src/A", "staged"), entry("src2/B")]);
  const afterStage = snapshot([entry("src/A", "staged"), entry("src2/B")]);
  exercise(first, true, [
    (f) => f.refresh(first, first.entries[0], true),
    (f) => {
      assert.equal(f.model.directoryCounts.get("unstaged:src"), 1);
      assert.equal(f.model.directoryCounts.has("staged:src"), false);
      assert.equal(f.model.directoryCounts.has("unstaged:src2"), false);
      f.refresh(afterStage, afterStage.entries[0]);
    },
    (f) => assert.deepEqual(f.selection(), []),
  ]);
});
