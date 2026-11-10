import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test, { afterEach } from "node:test";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";

const sourceRoot = new URL("../src/", import.meta.url);
const imports = registerHooks({
  resolve(specifier, context, nextResolve) {
    let source;
    if (specifier.startsWith("@/")) source = new URL(specifier.slice(2), sourceRoot);
    else if ((specifier.startsWith("./") || specifier.startsWith("../"))
      && context.parentURL?.startsWith(sourceRoot.href)) source = new URL(specifier, context.parentURL);
    if (source) {
      if (!/\.[^/]+$/.test(source.pathname)) source.pathname += ".ts";
      return nextResolve(source.href, context);
    }
    return nextResolve(specifier, context);
  },
});
const { CommitDraftPersistence } = await import("../src/lib/commitDraftPersistence.ts");
const { CommitComposerState, draftToInput } = await import("../src/lib/commitDraft.ts");
const { CommitOutputStore } = await import("../src/lib/commitOutput.ts");
const { CommitMessageGeneration } = await import("../src/lib/commitMessageGeneration.ts");
imports.deregister();

globalThis.window = { setTimeout, clearTimeout, crypto: globalThis.crypto };
afterEach(clearMocks);
const tick = () => new Promise((resolve) => setImmediate(resolve));
const deferred = () => {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
};
const write = (worktreeId, message, revision = message, generation = 1) => ({
  worktreeId, revision, generation, input: { message, amend: false, skipHooks: true },
});
const event = (worktreeId, clientNonce, line) => ({
  worktreeId, clientNonce, line, operationId: "operation", stderr: false,
});

test("生成期间切换：恢复草稿按原目标保存，A→B→A 不留下半截生成稿", async () => {
  const drafts = new Map([
    ["A", { message: "A 原文", amend: false, skipHooks: false, revision: "a", savedAtMs: 0 }],
    ["B", { message: "B 原文", amend: false, skipHooks: false, revision: "b", savedAtMs: 0 }],
  ]);
  mockIPC((command, args) => {
    if (command === "save_commit_draft") {
      drafts.set(args.request.worktreeId, args.request.draft);
    } else if (command === "clear_commit_draft") {
      drafts.delete(args.request.worktreeId);
    } else assert.fail(`unexpected IPC: ${command}`);
  });
  const queue = new CommitDraftPersistence(() => {});
  const state = new CommitComposerState();
  state.reset("A");
  state.edit(draftToInput(drafts.get("A")));
  let visibleWorktree = "A";
  let mirror = state.current().input;
  const writes = [];
  const generation = new CommitMessageGeneration({
    state,
    transport: { generate: async () => {}, cancel: async () => true },
    events: { attach: () => () => {} },
    onInput: (write) => {
      writes.push(write);
      queue.schedule({ ...write, revision: `generation-${write.generation}` });
      if (visibleWorktree === write.worktreeId) mirror = write.input;
    },
    onChange: () => {},
    newRequestId: () => "switch-request",
  });
  const requestId = generation.begin({ target: generation.captureTarget("A"), mode: { kind: "staged" } });
  generation.delta(requestId, "半截");
  await queue.flush("A");
  assert.equal(drafts.get("A").message, "半截");

  // React 已 render B，旧输入状态仍是 A：生产 resetTarget 必须恢复 A 后才重置。
  visibleWorktree = "B";
  mirror = draftToInput(drafts.get("B"));
  generation.resetTarget("B");
  await queue.flush();
  assert.equal(writes.at(-1).worktreeId, "A");
  assert.equal(writes.at(-1).input.message, "A 原文");
  assert.equal(mirror.message, "B 原文");
  assert.equal(drafts.get("A").message, "A 原文");
  assert.equal(drafts.get("B").message, "B 原文");
  state.edit(mirror);
  generation.delta(requestId, "迟到");
  generation.done(requestId, "迟到完成", false);
  generation.fail(requestId, { kind: "rejected", message: "迟到失败" });
  assert.equal(state.current().input.message, "B 原文");

  visibleWorktree = "A";
  generation.resetTarget("A");
  const loaded = state.applyLoaded(state.ticket("A", 3), 3, drafts.get("A"));
  assert.equal(loaded.applied, true);
  assert.equal(loaded.input.message, "A 原文");
  assert.equal(generation.view().canUndo, false);
});

test("draft flush with no writes terminates without invoking IPC", async () => {
  mockIPC(() => assert.fail("unexpected IPC"));
  const queue = new CommitDraftPersistence(() => {});
  await queue.flush("missing");
  await queue.flush();
});

test("drafts serialize per worktree, permit other worktrees and never persist skipHooks", async () => {
  const gate = deferred();
  const calls = [];
  mockIPC(async (command, { request }) => {
    calls.push([command, request]);
    if (request.draft?.message === "first") await gate.promise;
  });
  const queue = new CommitDraftPersistence(() => {});
  queue.save(write("a", "first"));
  queue.save(write("a", "second"));
  queue.save(write("b", "other"));
  await tick();
  assert.deepEqual(calls.map(([, request]) => request.draft.message), ["first", "other"]);
  gate.resolve();
  await queue.flush();
  assert.deepEqual(calls.map(([, request]) => request.draft.message), ["first", "other", "second"]);
  for (const [command, request] of calls) {
    assert.equal(command, "save_commit_draft");
    assert.equal(request.draft.skipHooks, false);
  }
});

test("flush includes new edits queued while waiting and worktree switches save pending input", async () => {
  const gate = deferred();
  const calls = [];
  mockIPC(async (_, { request }) => {
    calls.push(`${request.worktreeId}:${request.draft.message}`);
    if (request.draft.message === "first") await gate.promise;
  });
  const queue = new CommitDraftPersistence(() => {});
  queue.schedule(write("a", "first"));
  const flushing = queue.flush("a");
  await tick();
  queue.schedule(write("a", "latest"));
  queue.schedule(write("b", "other"));
  gate.resolve();
  await flushing;
  assert.deepEqual(calls, ["a:first", "a:latest"]);
  await queue.flush();
  assert.deepEqual(calls, ["a:first", "a:latest", "b:other"]);
});

test("draft failures remain visible, are not automatically retried and explicit saves recover", async () => {
  let failed = true;
  const calls = [];
  const settled = [];
  mockIPC((command) => {
    calls.push(command);
    if (failed) throw { kind: "environment", message: "disk unavailable" };
  });
  const queue = new CommitDraftPersistence((result) => settled.push(result));
  queue.schedule(write("a", "keep local input"));
  await assert.rejects(queue.flush("a"), /disk unavailable/);
  await assert.rejects(queue.flush("a"), /disk unavailable/);
  assert.equal(calls.length, 1);
  assert.equal(queue.failureFor("a"), "disk unavailable");
  assert.equal(settled[0].write.input.message, "keep local input");
  failed = false;
  await queue.save(write("a", "new explicit save"));
  await queue.flush();
  assert.equal(queue.failureFor("a"), null);
  await queue.save(write("a", "   "));
  assert.equal(calls.at(-1), "clear_commit_draft");
});

test("old draft settlements cannot show a stale success or failure on a new edit", async () => {
  const gate = deferred();
  const settled = [];
  mockIPC(async (_, { request }) => {
    if (request.worktreeId === "a") { await gate.promise; throw new Error("old failure"); }
  });
  const queue = new CommitDraftPersistence((result) => settled.push(result));
  queue.save(write("a", "old", "r1", 1));
  queue.save(write("b", "new", "r2", 2));
  await tick();
  gate.resolve();
  await queue.waitFor("a");
  assert.deepEqual(settled.map(({ write }) => write.worktreeId), ["b"]);
  assert.equal(queue.failureFor("a"), "old failure");
});

test("composer reads bind to worktree, activation and input generation", () => {
  const state = new CommitComposerState();
  const draft = { message: "saved", amend: true, skipHooks: true, savedAtMs: 1, revision: "r1" };
  state.reset("a");
  const old = state.ticket("a", 1);
  state.edit(write("a", "local input").input);
  assert.equal(state.applyLoaded(old, 1, draft).applied, false);
  assert.equal(state.current().input.message, "local input");
  state.reset("b");
  assert.equal(state.applyLoaded(old, 1, draft).reason, "worktree");
  state.reset("a");
  const ticket = state.ticket("a", 3);
  assert.equal(state.applyLoaded(ticket, 4, draft).reason, "activation");
  state.setSubmitting(true);
  assert.equal(state.applyLoaded(ticket, 3, draft).reason, "submitting");
  state.setSubmitting(false);
  assert.deepEqual(state.applyLoaded(ticket, 3, draft), { applied: true, input: draftToInput(draft) });
  assert.equal(state.current().input.skipHooks, false);
  const submitted = state.current().input;
  state.edit({ ...submitted, message: "new draft" });
  assert.equal(state.shouldClearAfterCommit("a", submitted), false);
  assert.equal(state.recordSaved("a", ticket.generation, submitted), false);
});

test("output is nonce-owned, UTF-8 byte bounded, and sealed only by a terminal result", () => {
  const store = new CommitOutputStore();
  const a = store.start("a", "one");
  store.start("b", "two");
  assert.equal(store.accept(event("a", "wrong", "foreign")), null);
  for (let i = 0; i < 100; i += 1) store.accept(event("a", "one", "你".repeat(2048)));
  const text = store.snapshot(a).join("\n");
  assert.ok(Buffer.byteLength(text) <= 256 * 1024);
  assert.ok(!text.includes("�"));
  const before = store.snapshot(a);
  store.accept(event("a", "one", "late but valid"));
  assert.notDeepEqual(store.snapshot(a), before, "snapshot after a transport error does not seal output");
  assert.deepEqual(store.finish(a, "final\noutput"), ["final", "output"]);
  assert.equal(store.accept(event("a", "one", "stale event")), null);
  assert.deepEqual(store.snapshot(a), ["final", "output"]);
  assert.deepEqual(store.finish(a, "second terminal"), ["final", "output"]);
});

test("old releases and terminal replies cannot clear a later submission", () => {
  const store = new CommitOutputStore();
  const old = store.start("a", "old");
  const fresh = store.start("a", "new");
  store.accept(event("a", "new", "new output"));
  store.release(old);
  assert.deepEqual(store.finish(old, "old output"), []);
  assert.deepEqual(store.snapshot(fresh), ["new output"]);
  store.release(fresh);
  assert.equal(store.accept(event("a", "new", "after release")), null);
});
