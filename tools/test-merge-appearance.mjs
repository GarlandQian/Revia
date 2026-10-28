import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";
import { Compartment, EditorState, StateEffect, StateField, Transaction } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { history, invertedEffects, redo, undo, undoDepth } from "@codemirror/commands";

// 与 test-sync-scroll.mjs 相同的接线：Node 直接加载 `src/` 下的 TS 模块，
// 需要把 `@/` 别名解析到 `src/`，并给相对导入补上 `.ts` 扩展名。
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
const { mergeEditorAppearance } = await import("../src/features/merge/editorAppearance.ts");
imports.deregister();

const initialAppearance = { fontSize: 12, fontFamily: null, softWrap: false };
const appearance = (options) => mergeEditorAppearance(options, 19);
const wraps = (state) => state.facet(EditorView.contentAttributes)
  .some((attrs) => typeof attrs === "object" && attrs.class?.split(" ").includes("cm-lineWrapping"));

test("重配字体、字号和软换行保留手动文本、选区、块状态及撤销重做", () => {
  const compartment = new Compartment();
  const setChunk = StateEffect.define();
  const initialChunk = { from: 0, to: 4, ours: "pending", theirs: "pending", edited: false };
  const chunkField = StateField.define({
    create: () => initialChunk,
    update(value, tr) {
      for (const effect of tr.effects) if (effect.is(setChunk)) return effect.value;
      return value;
    },
  });
  let state = EditorState.create({
    doc: "base\nunchanged",
    extensions: [
      compartment.of(appearance(initialAppearance)),
      history(),
      chunkField,
      invertedEffects.of((tr) => tr.effects
        .filter((effect) => effect.is(setChunk))
        .map(() => setChunk.of(tr.startState.field(chunkField)))),
    ],
  });
  const editedChunk = { from: 0, to: 6, ours: "applied", theirs: "pending", edited: true };
  state = state.update({
    changes: { from: 0, to: 4, insert: "manual" },
    selection: { anchor: 2, head: 5 },
    effects: setChunk.of(editedChunk),
  }).state;
  const doc = state.doc;
  const selection = state.selection;
  for (const options of [
    { fontSize: 12, fontFamily: null, softWrap: true },
    { fontSize: 18, fontFamily: "Menlo", softWrap: true },
    { fontSize: 14, fontFamily: "JetBrains Mono", softWrap: false },
  ]) {
    state = state.update({
      effects: compartment.reconfigure(appearance(options)),
      annotations: Transaction.addToHistory.of(false),
    }).state;
    assert.equal(state.doc, doc);
    assert.equal(state.doc.toString(), "manual\nunchanged");
    assert.equal(state.doc.lines, 2, "软换行不能在文件里插入换行符");
    assert.ok(state.selection.eq(selection));
    assert.equal(state.field(chunkField), editedChunk);
    assert.equal(undoDepth(state), 1);
    assert.equal(wraps(state), options.softWrap);
  }
  const dispatch = (tr) => { state = tr.state; };
  assert.equal(undo({ state, dispatch }), true);
  assert.equal(state.doc.toString(), "base\nunchanged");
  assert.equal(state.field(chunkField), initialChunk);
  assert.equal(redo({ state, dispatch }), true);
  assert.equal(state.doc.toString(), "manual\nunchanged");
  assert.equal(state.field(chunkField), editedChunk);
});

test("三栏共享外观重配不改变两侧只读与结果可编辑的状态", () => {
  const compartment = new Compartment();
  for (const readOnly of [true, false, true]) {
    let state = EditorState.create({
      doc: "a long line that must only wrap visually",
      extensions: [compartment.of(appearance(initialAppearance)), EditorState.readOnly.of(readOnly)],
    });
    state = state.update({
      effects: compartment.reconfigure(appearance({ fontSize: 16, fontFamily: "Menlo", softWrap: true })),
    }).state;
    assert.equal(state.readOnly, readOnly);
    assert.equal(wraps(state), true);
    assert.equal(state.doc.lines, 1);
  }
});
