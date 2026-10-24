import assert from "node:assert/strict";
import test from "node:test";
import { isTextInputOrDialog } from "../src/lib/platform.ts";

// Only the production focus predicate is exercised; native key dispatch is checked on desktop.
class FocusTarget {
  constructor(tag, { type = "text", dialog = false, editable = false } = {}) {
    this.tagName = tag.toUpperCase();
    this.type = type;
    this.isContentEditable = editable;
    this.dialog = dialog;
  }
  closest(selector) {
    return this.dialog && ["dialog", "[role='dialog']"].includes(selector) ? this : null;
  }
}
globalThis.HTMLElement = FocusTarget;

test("view and worktree SELECT controls keep keyboard input out of global actions", () => {
  assert.equal(isTextInputOrDialog(new FocusTarget("select")), true);
});

test("text inputs, editable content and dialogs remain isolated", () => {
  for (const target of [new FocusTarget("input"), new FocusTarget("textarea"),
    new FocusTarget("div", { editable: true }), new FocusTarget("button", { dialog: true })]) {
    assert.equal(isTextInputOrDialog(target), true);
  }
});

test("non-text targets retain their separate button and file-list keyboard paths", () => {
  for (const target of [null, {}, new FocusTarget("div"), new FocusTarget("button"),
    new FocusTarget("input", { type: "checkbox" })]) {
    assert.equal(isTextInputOrDialog(target), false);
  }
});
