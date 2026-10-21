import assert from "node:assert/strict";
import test from "node:test";
import { startIpcDiagnostics } from "../src/lib/ipcDiagnostics.ts";

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((accept, fail) => {
    resolve = accept;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function fixture(fetch, initialRegistry = new Map()) {
  let time = 0;
  let registry = initialRegistry;
  return {
    host: {
      transport: { fetch },
      now: () => time,
      callbacks: () => registry,
      domCounts: () => ({ elements: 103, diffRows: 40 }),
      resourceTimingCount: () => 17,
    },
    setTime(value) { time = value; },
    setRegistry(value) { registry = value; },
  };
}

const callback = () => { throw new Error("Diagnostics must not invoke callbacks"); };

// All transports are fakes. These tests never open a WebView or perform network requests.
test("non-IPC calls preserve receiver, arguments and the original promise", () => {
  const response = new Response(null);
  const promise = Promise.resolve(response);
  const calls = [];
  const f = fixture(function (...args) {
    calls.push({ receiver: this, args });
    return promise;
  });
  const diagnostic = startIpcDiagnostics(f.host);
  const receiver = {};
  const init = { method: "POST", body: "unchanged" };
  const inputs = ["https://example.invalid/read", new URL("https://example.invalid/url"),
    new Request("https://example.invalid/request"), "ipc://localhost.evil/get_rows"];
  for (const input of inputs) {
    assert.equal(f.host.transport.fetch.call(receiver, input, init), promise);
  }
  for (const [index, call] of calls.entries()) {
    assert.equal(call.receiver, receiver);
    assert.equal(call.args[0], inputs[index]);
    assert.equal(call.args[1], init);
  }
  assert.equal(diagnostic.snapshot().fetch.total.started, 0);
  diagnostic.stop();
});

test("IPC observes headers without changing promise, response or reading its body", async () => {
  const pending = deferred();
  const receiver = {};
  const init = { method: "POST", body: "fixture" };
  const response = new Response("untouched body");
  let bodyReads = 0;
  for (const property of ["body", "bodyUsed", "arrayBuffer", "blob", "bytes", "formData", "json", "text", "clone"]) {
    Object.defineProperty(response, property, {
      get() { bodyReads += 1; throw new Error(`Unexpected response access: ${property}`); },
    });
  }
  const f = fixture(function (...args) {
    assert.equal(this, receiver);
    assert.deepEqual(args, ["ipc://localhost/get_rows", init]);
    return pending.promise;
  });
  f.setTime(10);
  const diagnostic = startIpcDiagnostics(f.host);
  const returned = f.host.transport.fetch.call(receiver, "ipc://localhost/get_rows", init);
  assert.equal(returned, pending.promise);
  const waiting = diagnostic.snapshot();
  assert.equal(waiting.fetch.total.active, 1);
  assert.equal(waiting.fetch.total.headersSettled, 0);
  f.setTime(42);
  pending.resolve(response);
  assert.equal(await returned, response);
  const result = diagnostic.snapshot();
  assert.equal(bodyReads, 0);
  assert.deepEqual(result.fetch.total, {
    started: 1, headersSettled: 1, failed: 0, synchronousThrows: 0,
    active: 0, maxActive: 1, totalHeaderLatencyMs: 32, maxHeaderLatencyMs: 32,
  });
  assert.deepEqual(result.fetch.commands.get_rows, result.fetch.total);
  assert.match(result.fetch.completionBoundary, /headers, not body/);
  assert.deepEqual(result.dom, { elements: 103, diffRows: 40 });
  assert.equal(result.resourceTimingEntries, 17);
  diagnostic.stop();
});

test("synchronous IPC throws retain error identity and close the active counter", () => {
  const reason = new Error("synchronous transport failure");
  const receiver = {};
  const f = fixture(function () {
    assert.equal(this, receiver);
    throw reason;
  });
  const diagnostic = startIpcDiagnostics(f.host);
  assert.throws(() => f.host.transport.fetch.call(receiver, "ipc://localhost/debug_log"),
    (error) => error === reason);
  const result = diagnostic.snapshot().fetch.total;
  assert.equal(result.failed, 1);
  assert.equal(result.synchronousThrows, 1);
  assert.equal(result.active, 0);
  assert.equal(result.headersSettled, 0);
  diagnostic.stop();
});

test("IPC rejection retains promise and reason without a successful-header sample", async () => {
  const pending = deferred();
  const reason = new Error("fetch rejected");
  const f = fixture(() => pending.promise);
  const diagnostic = startIpcDiagnostics(f.host);
  const returned = f.host.transport.fetch("ipc://localhost/highlight_range");
  assert.equal(returned, pending.promise);
  f.setTime(75);
  pending.reject(reason);
  await assert.rejects(returned, (error) => error === reason);
  assert.deepEqual(diagnostic.snapshot().fetch.total, {
    started: 1, headersSettled: 0, failed: 1, synchronousThrows: 0,
    active: 0, maxActive: 1, totalHeaderLatencyMs: 0, maxHeaderLatencyMs: 0,
  });
  diagnostic.stop();
});

test("callback deletion and new IDs use continuous observation ages without mutation", () => {
  const registry = new Map([[1, callback], [2, callback]]);
  const f = fixture(() => Promise.resolve(new Response(null)), registry);
  const diagnostic = startIpcDiagnostics(f.host);
  assert.equal(diagnostic.snapshot().callbacks.baselineStillRegistered, 2);
  registry.delete(1);
  registry.set(3, callback);
  f.setTime(100);
  let observed = diagnostic.snapshot().callbacks;
  assert.equal(observed.registrySize, 2);
  assert.equal(observed.baselineStillRegistered, 1);
  assert.equal(observed.newIdCount, 1);
  assert.equal(observed.oldestObservedAgeMs, 0);
  f.setTime(225);
  assert.equal(diagnostic.snapshot().callbacks.oldestObservedAgeMs, 125);
  registry.delete(3);
  registry.set(4, callback);
  f.setTime(250);
  observed = diagnostic.snapshot().callbacks;
  assert.equal(observed.trackedObservedIds, 1);
  assert.equal(observed.oldestObservedAgeMs, 0);
  assert.equal(observed.countsAreNotRequestCounts, true);
  assert.match(observed.ageMeaning, /lower bound/);
  assert.deepEqual([...registry], [[2, callback], [4, callback]]);
  diagnostic.stop();
});

test("missing callback registry is unavailable, not zero, and breaks age continuity", () => {
  const registry = new Map();
  const f = fixture(() => Promise.resolve(new Response(null)), registry);
  const diagnostic = startIpcDiagnostics(f.host);
  registry.set(3, callback);
  diagnostic.snapshot();
  f.setTime(100);
  assert.equal(diagnostic.snapshot().callbacks.oldestObservedAgeMs, 100);
  f.setRegistry(undefined);
  const missing = diagnostic.snapshot().callbacks;
  assert.equal(missing.available, false);
  assert.equal(missing.registrySize, null);
  assert.equal(missing.oldestObservedAgeMs, null);
  f.setRegistry(registry);
  f.setTime(500);
  assert.equal(diagnostic.snapshot().callbacks.oldestObservedAgeMs, 0);
  diagnostic.stop();
});

test("baseline IDs cap at 512 without truncating registry size or pretending new IDs are known", () => {
  const registry = new Map(Array.from({ length: 600 }, (_, id) => [id, callback]));
  const f = fixture(() => Promise.resolve(new Response(null)), registry);
  const diagnostic = startIpcDiagnostics(f.host);
  const result = diagnostic.snapshot().callbacks;
  assert.equal(result.registrySize, 600);
  assert.equal(result.baselineRegistrySize, 600);
  assert.equal(result.baselineTrackedIds, 512);
  assert.equal(result.baselineOverflow, 88);
  assert.equal(result.baselineStillRegistered, 512);
  assert.equal(result.baselineStillRegisteredIsLowerBound, true);
  assert.equal(result.newIdCount, null);
  assert.equal(result.nonBaselineRegistryCount, 88);
  assert.equal(registry.size, 600);
  diagnostic.stop();
});

test("observed IDs cap at 512 while full new-ID count and overflow remain visible", () => {
  const registry = new Map();
  const f = fixture(() => Promise.resolve(new Response(null)), registry);
  const diagnostic = startIpcDiagnostics(f.host);
  for (let id = 0; id < 600; id += 1) registry.set(id, callback);
  f.setTime(100);
  let result = diagnostic.snapshot().callbacks;
  assert.equal(result.registrySize, 600);
  assert.equal(result.newIdCount, 600);
  assert.equal(result.trackedObservedIds, 512);
  assert.equal(result.observationOverflow, 88);
  for (let id = 0; id < 300; id += 1) registry.delete(id);
  f.setTime(200);
  result = diagnostic.snapshot().callbacks;
  assert.equal(result.registrySize, 300);
  assert.equal(result.newIdCount, 300);
  assert.equal(result.trackedObservedIds, 300);
  assert.equal(result.observationOverflow, 0);
  assert.equal(result.oldestObservedAgeMs, 100);
  diagnostic.stop();
});

test("command labels have a fixed cap plus an overflow bucket and decode plugin names", async () => {
  const response = new Response(null);
  const f = fixture(() => Promise.resolve(response));
  const diagnostic = startIpcDiagnostics(f.host);
  const calls = [f.host.transport.fetch("ipc://localhost/plugin%3Aevent%7Clisten")];
  for (let index = 1; index < 40; index += 1) {
    calls.push(f.host.transport.fetch(`ipc://localhost/command_${index}`));
  }
  await Promise.all(calls);
  let result = diagnostic.snapshot().fetch;
  assert.equal(result.namedCommandLimit, 32);
  assert.equal(Object.keys(result.commands).length, 33);
  assert.equal(result.commands["plugin:event|listen"].started, 1);
  assert.equal(result.commands["<other>"].started, 8);
  assert.equal(result.commandOverflowRequests, 8);
  assert.equal(result.total.started, 40);
  assert.equal(result.total.active, 0);
  assert.equal(result.total.maxActive, 40);
  await f.host.transport.fetch("ipc://localhost/command_39");
  await f.host.transport.fetch("ipc://localhost/plugin%3Aevent%7Clisten");
  result = diagnostic.snapshot().fetch;
  assert.equal(result.commands["<other>"].started, 9);
  assert.equal(result.commandOverflowRequests, 9);
  assert.equal(result.commands["plugin:event|listen"].started, 2);
  assert.equal(Object.keys(result.commands).length, 33);
  diagnostic.stop();
});

test("stop restores its own hook and lets previously started requests settle", async () => {
  const pending = deferred();
  const original = () => pending.promise;
  const f = fixture(original);
  const diagnostic = startIpcDiagnostics(f.host);
  const returned = f.host.transport.fetch("ipc://localhost/get_rows");
  diagnostic.stop();
  assert.equal(f.host.transport.fetch, original);
  pending.resolve(new Response(null));
  await returned;
  const result = diagnostic.snapshot();
  assert.equal(result.stopped, true);
  assert.equal(result.fetchHookStillInstalled, false);
  assert.equal(result.fetch.total.active, 0);
  assert.equal(result.fetch.total.headersSettled, 1);
});

test("stop never overwrites another wrapper and its old wrapper becomes transparent", () => {
  const promise = Promise.resolve(new Response(null));
  const receiver = {};
  const f = fixture(function () { assert.equal(this, receiver); return promise; });
  const diagnostic = startIpcDiagnostics(f.host);
  const diagnosticWrapper = f.host.transport.fetch;
  const otherWrapper = function (...args) { return Reflect.apply(diagnosticWrapper, this, args); };
  f.host.transport.fetch = otherWrapper;
  diagnostic.stop();
  diagnostic.stop();
  assert.equal(f.host.transport.fetch, otherWrapper);
  assert.equal(f.host.transport.fetch.call(receiver, "ipc://localhost/debug_log"), promise);
  assert.equal(diagnostic.snapshot().fetch.total.started, 0);
});
