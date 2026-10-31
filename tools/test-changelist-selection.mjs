import assert from "node:assert/strict";
import test from "node:test";
import { VisibleChangelistSelection, visibleChangelistPaths } from "../src/lib/m7/visibleChangelistSelection.ts";

test("range follows visible b/c/a groups rather than hidden a/b/c index order", () => {
  const selection = new VisibleChangelistSelection(); selection.setRows(["default:b", "default:c", "later:a"]);
  selection.activate("default:b"); selection.activate("later:a", { extend: true });
  assert.deepEqual(selection.ids(), ["default:b", "default:c", "later:a"]);
});
test("shift arrows and primary toggles follow the same visible order", () => {
  const selection = new VisibleChangelistSelection(); selection.setRows(["b", "c", "a"]);
  selection.activate("b"); assert.equal(selection.move(1, true), "c"); assert.deepEqual(selection.ids(), ["b", "c"]);
  selection.move(1, true); assert.deepEqual(selection.ids(), ["b", "c", "a"]);
  selection.activate("c", {toggle:true}); assert.deepEqual(selection.ids(), ["b", "a"]);
});
test("same path in different lists has independent visible row identities", () => {
  const selection = new VisibleChangelistSelection(); selection.setRows(["first:file", "first:other", "second:file"]);
  selection.activate("first:file"); selection.activate("second:file", {extend:true});
  assert.deepEqual(selection.ids(), ["first:file", "first:other", "second:file"]);
});
test("refresh removes vanished rows and select all uses the latest order", () => {
  const selection = new VisibleChangelistSelection(); selection.setRows(["a", "b", "c"]); selection.selectAll();
  selection.setRows(["c", "a"]); assert.deepEqual(selection.ids(), ["c", "a"]);
  selection.setRows(["new", "c", "a"]); selection.selectAll(); assert.deepEqual(selection.ids(), ["new", "c", "a"]);
});

test("diff file order follows visible groups and deduplicates split file membership", () => {
  assert.deepEqual(visibleChangelistPaths([{id:"default"},{id:"later"}], [{group:"later",path:"a"},{group:"default",path:"b"},{group:"default",path:"c"},{group:"later",path:"b"}]), ["b","c","a"]);
});
