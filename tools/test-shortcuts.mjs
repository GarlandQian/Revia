import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";
const sourceRoot = new URL("../src/", import.meta.url);
const imports = registerHooks({ resolve(specifier, context, nextResolve) {
  let source;
  if (specifier.startsWith("@/")) source = new URL(specifier.slice(2), sourceRoot);
  else if ((specifier.startsWith("./") || specifier.startsWith("../")) && context.parentURL?.startsWith(sourceRoot.href)) source = new URL(specifier, context.parentURL);
  if (source) { if (!/\.[^/]+$/.test(source.pathname)) source.pathname += ".ts"; return nextResolve(source.href, context); }
  return nextResolve(specifier, context);
}});
const { replaceShortcut, conflictFor, setShortcutOverrides, acceleratorOf, matchShortcut, shortcutDefs } = await import("../src/lib/shortcuts.ts");
const { isMac } = await import("../src/lib/platform.ts");
imports.deregister();
const event = { code: "KeyK", key: "k", metaKey: isMac, ctrlKey: !isMac, altKey: false, shiftKey: false };
test("确认替换默认快捷键后，真实按键只分派新动作；恢复默认也先解除冲突", () => {
  assert.equal(conflictFor("mod+k", "cloneRepo", {}), "commit");
  const replaced = replaceShortcut({}, "cloneRepo", "mod+k");
  assert.equal(replaced.commit, "");
  setShortcutOverrides(JSON.parse(JSON.stringify(replaced)));
  assert.equal(acceleratorOf("commit"), "");
  assert.equal(matchShortcut(event), "cloneRepo");
  assert.equal(conflictFor("mod+k", "cloneRepo", replaced), null);
  assert.equal(conflictFor("mod+k", "commit", replaced), "cloneRepo");
  const reset = replaceShortcut(replaced, "commit", shortcutDefs.find((def) => def.id === "commit").defaultAccelerator);
  setShortcutOverrides(reset);
  assert.equal(acceleratorOf("cloneRepo"), "");
  assert.equal(matchShortcut(event), "commit");
  setShortcutOverrides({});
  assert.equal(acceleratorOf("cloneRepo"), "mod+shift+o");
  assert.equal(matchShortcut(event), "commit");
});
test("未分配值不互相冲突；同键多个旧绑定都明确解除", () => {
  assert.equal(conflictFor("", "commit", { cloneRepo: "" }), null);
  const next = replaceShortcut({ refresh: "mod+k", cloneRepo: "mod+k" }, "stageFile", "mod+k");
  assert.equal(next.commit, ""); assert.equal(next.cloneRepo, ""); assert.equal(next.refresh, "");
  setShortcutOverrides(next);
  assert.equal(matchShortcut(event), "stageFile");
  setShortcutOverrides({});
});
