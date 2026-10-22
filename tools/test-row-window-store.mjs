import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";
import { setImmediate as nextTurn } from "node:timers/promises";

const sourceRoot = new URL("../src/", import.meta.url);
// Resolve production TypeScript imports only; do not replace store or IPC implementations.
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
const { RowWindowStore } = await import("../src/features/diff/rowWindowStore.ts");
const { matchesHunkAction } = await import("../src/features/diff/hunkAction.ts");
const { messages } = await import("../src/lib/i18n/zh-CN.ts");
const { isLoggingEnabled } = await import("../src/features/diff/log.ts");
imports.deregister();
assert.equal(isLoggingEnabled(), false, "The default-disabled logger must not invoke Tauri");

const ROW_COUNT = 64;

function target(overrides = {}) {
  return {
    worktreeId: "worktree-a", path: "src/example.ts", changes: "unstaged", view: "unified",
    fingerprint: "file-version-1", snapshotRevision: "snapshot-1", ...overrides,
  };
}

function entryFor(current, overrides = {}) {
  return {
    path: current.path, originalPath: null, changes: current.changes, status: "modified",
    binary: false, added: 1, deleted: 1, hunkCount: 1,
    rowCountUnified: ROW_COUNT, rowCountSplit: ROW_COUNT, language: "ts",
    collapsed: false, unmerged: false, metricsLoaded: true, readonlyReason: null,
    hunks: [{ index: 0, header: "@@ -1,3 +1,3 @@", section: "example", rowUnified: 0, rowSplit: 0 }],
    fingerprint: current.fingerprint, snapshotRevision: current.snapshotRevision, ...overrides,
  };
}

function actionFor(current, entry) {
  return {
    worktreeId: current.worktreeId, path: entry.path, changes: entry.changes,
    fingerprint: entry.fingerprint, snapshotRevision: entry.snapshotRevision,
    hunkId: `issued-hunk/${entry.snapshotRevision}/${entry.fingerprint}`,
  };
}

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((accept, fail) => { resolve = accept; reject = fail; });
  return { promise, resolve, reject };
}

function fixture(current = target()) {
  const calls = [];
  const store = new RowWindowStore((request) => {
    const pending = deferred();
    calls.push({ request, ...pending });
    return pending.promise;
  });
  const entry = entryFor(current);
  store.setTarget(current, ROW_COUNT, entry);
  return { store, calls, target: current, entry };
}

function requestWindow(f) {
  const start = f.calls.length;
  assert.equal(f.store.ensure(0, 1), false);
  const calls = f.calls.slice(start);
  const slice = calls.find(({ request }) => request.from === 0 && request.to < ROW_COUNT);
  const chunk = calls.find(({ request }) => request.from === 0 && request.to === ROW_COUNT);
  assert.ok(slice, "The visible window must have a range request");
  assert.ok(chunk, "The same window must have a full chunk request");
  return { calls, slice, chunk };
}

function responseFor(call, entry, marker = "current") {
  const { request } = call;
  const rows = Array.from({ length: request.to - request.from }, (_, offset) => {
    const index = request.from + offset;
    return {
      kind: index === 0 ? "hunk" : "line", hunkIndex: 0,
      hunkAction: index === 0 ? actionFor(request, entry) : null,
      section: index === 0 ? `${marker} hunk` : null,
      oldLine: index + 1, newLine: index + 1, lineKind: index === 0 ? null : "context",
      text: index === 0 ? null : `${marker} line ${index}`, left: null, right: null,
    };
  });
  return { rows, rowCount: ROW_COUNT, entry };
}

async function resolveAll(calls, entry, marker = "current") {
  for (const call of calls) call.resolve(responseFor(call, entry, marker));
  await nextTurn();
}

test("matching rows preserve the hunk credentials issued with the response", async () => {
  const f = fixture();
  const requested = requestWindow(f);
  await resolveAll(requested.calls, f.entry);
  assert.equal(f.store.isCovered(0, ROW_COUNT), true);
  assert.deepEqual(f.store.getRow(0).hunkAction, actionFor(f.target, f.entry));
  assert.equal(f.store.latestEntry, f.entry);
  assert.equal(f.store.getSnapshot().stale, false);
  assert.equal(f.store.getSnapshot().error, null);
  assert.equal(f.store.getSnapshot().pendingChunks, 0);
});

const mismatchedFields = {
  path: "src/other.ts", changes: "staged",
  fingerprint: "other-file-version", snapshotRevision: "other-snapshot",
};

for (const requestKind of ["slice", "chunk"]) {
  for (const [field, value] of Object.entries(mismatchedFields)) {
    test(`${requestKind} rejects mismatched ${field} before accessing or caching response rows`, async () => {
      const f = fixture();
      const requested = requestWindow(f);
      const wrong = responseFor(requested[requestKind], entryFor(f.target, { [field]: value }), "wrong");
      const payloadRows = wrong.rows;
      let rowReads = 0;
      Object.defineProperty(wrong, "rows", { get() { rowReads += 1; return payloadRows; } });
      const publishedRows = [];
      const unsubscribe = f.store.subscribe(() => publishedRows.push(f.store.getRow(0)));
      requested[requestKind].resolve(wrong);
      await nextTurn();

      assert.equal(rowReads, 0, "Version validation must precede reading the row payload");
      assert.equal(f.store.getSnapshot().stale, true);
      assert.equal(f.store.getSnapshot().error, messages.changes.fileChanged);
      assert.equal(f.store.getSnapshot().loadedChunks, 0);
      assert.equal(f.store.getSnapshot().pendingChunks, 0);
      assert.equal(f.store.getRow(0), null);
      assert.equal(f.store.latestEntry, null);
      assert.ok(publishedRows.length > 0);
      assert.ok(publishedRows.every((row) => row === null));

      const stopped = f.store.getSnapshot();
      await resolveAll(requested.calls, f.entry, "late-old");
      assert.equal(f.store.getSnapshot(), stopped, "Other in-flight responses cannot revive this target");
      unsubscribe();
    });
  }
}

for (const outcome of ["resolved", "rejected"]) {
  test(`old-epoch ${outcome} responses cannot change the newer cache or its error state`, async () => {
    const f = fixture();
    const old = requestWindow(f);
    const nextTarget = target({ snapshotRevision: "snapshot-2" });
    const nextEntry = entryFor(nextTarget);
    f.store.setTarget(nextTarget, ROW_COUNT, nextEntry);
    const current = requestWindow(f);
    await resolveAll(current.calls, nextEntry, "new");
    const accepted = f.store.getSnapshot();
    const acceptedRow = f.store.getRow(0);

    if (outcome === "resolved") await resolveAll(old.calls, f.entry, "old");
    else {
      for (const call of old.calls) call.reject({ kind: "rejected", message: "old target disappeared" });
      await nextTurn();
    }
    assert.equal(f.store.getSnapshot(), accepted);
    assert.equal(f.store.getRow(0), acceptedRow);
    assert.equal(f.store.getRow(1).text, "new line 1");
    assert.equal(f.store.latestEntry, nextEntry);
  });
}

for (const [field, value] of Object.entries({ snapshotRevision: "snapshot-2", fingerprint: "file-version-2" })) {
  test(`changing only ${field} clears cached text and hunk actions even when path and row count match`, async () => {
    const f = fixture();
    await resolveAll(requestWindow(f).calls, f.entry, "old");
    const oldAction = f.store.getRow(0).hunkAction;
    const nextTarget = target({ [field]: value });
    const nextEntry = entryFor(nextTarget);
    f.store.setTarget(nextTarget, ROW_COUNT, nextEntry);

    assert.equal(f.store.rowCountValue, ROW_COUNT);
    assert.equal(f.store.isCurrent(f.target), false);
    assert.equal(f.store.isCurrent(nextTarget), true);
    assert.equal(f.store.getRow(0), null);
    assert.equal(f.store.getRow(1), null);
    assert.equal(f.store.getSnapshot().loadedChunks, 0);
    assert.equal(matchesHunkAction(oldAction, nextTarget.worktreeId, nextEntry), false);

    await resolveAll(requestWindow(f).calls, nextEntry, "new");
    assert.notEqual(f.store.getRow(0).hunkAction.hunkId, oldAction.hunkId);
    assert.equal(matchesHunkAction(f.store.getRow(0).hunkAction, nextTarget.worktreeId, nextEntry), true);
  });
}

test("isCurrent compares every target identity and version field", () => {
  const f = fixture();
  assert.equal(f.store.isCurrent({ ...f.target }), true);
  for (const [field, value] of Object.entries({
    worktreeId: "worktree-b", path: "other.ts", changes: "staged", view: "split",
    fingerprint: "file-version-2", snapshotRevision: "snapshot-2",
  })) assert.equal(f.store.isCurrent({ ...f.target, [field]: value }), false, field);
});

for (const requestKind of ["slice", "chunk"]) {
  test(`${requestKind} rejected missing target stops retries until a new target is bound`, async () => {
    const f = fixture();
    const old = requestWindow(f);
    old[requestKind].reject({ kind: "rejected", message: "No change content exists for this file" });
    await nextTurn();
    assert.equal(f.store.getSnapshot().stale, true);
    assert.equal(f.store.getSnapshot().error, messages.changes.fileChanged);
    const issued = f.calls.length;
    for (let index = 0; index < 100; index += 1) {
      f.store.setTarget({ ...f.target }, ROW_COUNT, f.entry);
      assert.equal(f.store.ensure(0, 1), false);
      f.store.prioritize(10, 20);
    }
    await nextTurn();
    assert.equal(f.calls.length, issued);
    await resolveAll(old.calls, f.entry);
    assert.equal(f.store.getRow(0), null);

    const nextTarget = target({ snapshotRevision: "snapshot-2" });
    const nextEntry = entryFor(nextTarget);
    f.store.setTarget(nextTarget, ROW_COUNT, nextEntry);
    assert.equal(f.store.getSnapshot().stale, false);
    assert.equal(f.store.getSnapshot().error, null);
    await resolveAll(requestWindow(f).calls, nextEntry, "recovered");
    assert.equal(f.store.isCovered(0, ROW_COUNT), true);
    assert.equal(f.store.getRow(1).text, "recovered line 1");
  });
}

test("a legacy fileChanged rejection string also disables the target", async () => {
  const f = fixture();
  const requested = requestWindow(f);
  requested.slice.reject(messages.changes.fileChanged);
  await nextTurn();
  await resolveAll(requested.calls, f.entry);
  const issued = f.calls.length;
  assert.equal(f.store.ensure(0, 1), false);
  assert.equal(f.calls.length, issued);
  assert.equal(f.store.getSnapshot().stale, true);
});

for (const changes of ["unstaged", "staged"]) {
  test(`hunk action accepts complete matching ${changes} credentials without rewriting them`, () => {
    const current = target({ changes });
    const entry = Object.freeze(entryFor(current));
    const action = Object.freeze(actionFor(current, entry));
    const original = { ...action };
    assert.equal(matchesHunkAction(action, current.worktreeId, entry), true);
    assert.deepEqual(action, original);
  });
}

for (const [field, value] of Object.entries({ worktreeId: "worktree-b", ...mismatchedFields })) {
  test(`hunk action rejects a mismatched ${field} credential`, () => {
    const current = target();
    const entry = entryFor(current);
    const action = { ...actionFor(current, entry), [field]: value };
    assert.equal(matchesHunkAction(action, current.worktreeId, entry), false);
  });
}

test("hunk action rejects readonly, binary, conflicted and untracked entries even with matching credentials", () => {
  for (const change of [{ readonlyReason: "symlink" }, { binary: true }, { unmerged: true }, { changes: "untracked" }]) {
    const current = target();
    const entry = entryFor(current, change);
    const action = actionFor(current, entry);
    assert.equal(matchesHunkAction(action, current.worktreeId, entry), false, JSON.stringify(change));
  }
});
