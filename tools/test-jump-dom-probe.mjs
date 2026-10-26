import assert from "node:assert/strict";
import test from "node:test";
import { inspectJumpDom } from "../src/lib/jumpDomProbe.ts";

// Minimal semantic DOM fixtures for the production read-only probe, not a renderer or layout engine.
class ProbeElement {
  constructor(tag, className, children = [], text = "") {
    this.tagName = tag.toUpperCase();
    this.className = className;
    this.children = children;
    this.childNodes = children;
    this.text = text;
    this.isConnected = true;
    this.style = {};
    this.classList = { contains: (name) => this.className.split(" ").includes(name) };
  }
  get firstElementChild() { return this.children[0]; }
  get textContent() { return this.text + this.children.map((child) => child.textContent).join(""); }
  matches(selector) {
    const [tag, ...classes] = selector.split(".");
    return (!tag || tag.toUpperCase() === this.tagName)
      && classes.every((name) => this.classList.contains(name));
  }
}
globalThis.Element = ProbeElement;
globalThis.HTMLElement = ProbeElement;
const span = (className, text = "", children = []) => new ProbeElement("span", className, children, text);
const marker = (kind) => kind === "addition" ? "+" : kind === "deletion" ? "-" : " ";
const cell = (value, tone) => value == null ? span("cell empty") : span(
  `cell cell-${value.kind}${tone ? ` cell-${tone}` : ""}`, "",
  [span("marker", marker(value.kind)), span("tok-plain", value.text)],
);

function inspect(view, kind = "context", { missing = false, mutate = () => {} } = {}) {
  const value = { kind, text: "const value = 2;", line: 7 };
  const added = kind === "addition";
  const deleted = kind === "deletion";
  const left = added ? null : value;
  const right = deleted ? null : value;
  const row = { kind: view === "split" ? "pair" : "line", modified: !added && !deleted,
    lineKind: kind, oldLine: left?.line ?? null, newLine: right?.line ?? null,
    text: value.text, left, right };
  const tone = added ? "ins" : deleted ? "del" : null;
  const children = view === "split"
    ? [span("gutter", String(left?.line ?? "")), cell(left, tone), span("mid"),
      span("gutter", String(right?.line ?? "")), cell(right, tone)]
    : [span("gutter", String(row.oldLine ?? "")), span("gutter", String(row.newLine ?? "")), cell(value, tone)];
  const node = new ProbeElement("div", `row row-${view}`, children);
  node.style.top = "0px";
  mutate(node);
  const canvas = new ProbeElement("div", `diff-canvas diff-canvas-${view}`, missing ? [] : [node]);
  canvas.style = { height: "18px", transform: "translateY(0px)" };
  const root = new ProbeElement("div", "diff-scroll", [canvas]);
  return inspectJumpDom({ root, view, from: 0, to: 1, rowHeight: 18, getRow: () => row,
    highlights: { get: () => [{ text: value.text, category: "plain" }], categoryClass: () => "tok-plain" },
    preparation: { matches: () => true },
  });
}

for (const view of ["split", "unified"]) {
  for (const kind of ["context", "addition", "deletion"]) {
    test(`${view} ${kind}: exact semantic cells, line numbers, markers and tokens match`, () => {
      const result = inspect(view, kind);
      assert.equal(result.textMatched, true);
      assert.equal(result.colorsMatched, true);
      assert.equal(result.matchedRows, 1);
    });
  }
  test(`${view}: wrong text or token classes are still rejected`, () => {
    const position = view === "split" ? 1 : 2;
    assert.equal(inspect(view, "context", { mutate(node) {
      node.children[position].children[1].text = "stale text";
    } }).textMatched, false);
    const result = inspect(view, "context", { mutate(node) {
      node.children[position].children[1].className = "tok-keyword";
    } });
    assert.equal(result.textMatched, true);
    assert.equal(result.colorsMatched, false);
  });
  test(`${view}: an actually absent row is a placeholder, never ready text`, () => {
    const result = inspect(view, "context", { missing: true });
    assert.equal(result.textMatched, false);
    assert.equal(result.placeholderExposed, true);
    assert.equal(result.placeholderRows, 1);
  });
}

test("split: the new middle separator cannot impersonate the right gutter or cell", () => {
  assert.equal(inspect("split", "context", { mutate(node) {
    node.children[2].className = "gutter";
  } }).textMatched, false);
  assert.equal(inspect("split", "context", { mutate(node) {
    node.children[3].text = "99";
  } }).textMatched, false);
  assert.equal(inspect("split", "context", { mutate(node) {
    node.children[4].children[0].text = "+";
  } }).textMatched, false);
});
