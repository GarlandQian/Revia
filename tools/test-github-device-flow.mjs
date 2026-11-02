import assert from "node:assert/strict";
import test from "node:test";
import { GithubDeviceFlow } from "../src/lib/m7/githubDeviceFlow.ts";

const device = { flowId: "fixture-flow", userCode: "TEST-CODE", verificationUri: "https://github.com/login/device", expiresInMs: 30000, intervalMs: 5000 };
const account = { login: "fixture-account" };
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const flush = async () => { for (let i = 0; i < 5; i++) await Promise.resolve(); };

function fixture(overrides = {}) {
  let now = 0, nextId = 0;
  const timers = new Map(), calls = [], views = [], errors = [], ended = [], authorized = [];
  const api = {
    githubDeviceStart: async () => { calls.push("start"); return device; },
    githubDevicePoll: async (id) => { calls.push(["poll", id]); return { kind: "pending", intervalMs: 5000, expiresInMs: 20000 }; },
    githubDeviceCancel: async (id) => { calls.push(["cancel", id]); },
    githubDeviceOpen: async () => { calls.push("open"); },
    ...overrides,
  };
  const flow = new GithubDeviceFlow(api, {
    changed: (view) => views.push(view),
    failed: (error) => errors.push(error),
    ended: (reason) => ended.push(reason),
    authorized: (value) => authorized.push(value),
  }, {
    now: () => now,
    later: (callback, delay) => { const id = ++nextId; timers.set(id, { callback, at: now + delay }); return id; },
    clear: (id) => timers.delete(id),
  });
  return { flow, calls, views, errors, ended, authorized, timers,
    async advance(ms) {
      now += ms;
      const due = Array.from(timers).filter(([, timer]) => timer.at <= now);
      for (const [id, timer] of due) { timers.delete(id); timer.callback(); }
      await flush();
    },
  };
}

test("browser login starts once and waits for the server polling interval", async () => {
  const pending = deferred();
  const f = fixture({ githubDeviceStart: () => { f.calls.push("start"); return pending.promise; } });
  const started = f.flow.start();
  await f.flow.start();
  assert.deepEqual(f.calls, ["start"]);
  pending.resolve(device); await started;
  assert.deepEqual(f.calls, ["start", "open"]);
  assert.equal(f.views.at(-1).device.userCode, "TEST-CODE");
  await f.advance(4999); assert.equal(f.calls.length, 2);
  await f.advance(1); assert.deepEqual(f.calls.at(-1), ["poll", "fixture-flow"]);
});

test("late start after cancellation or unmount is cancelled without opening a browser", async () => {
  for (const notify of [true, false]) {
    const pending = deferred();
    const f = fixture({ githubDeviceStart: () => pending.promise });
    const started = f.flow.start(); f.flow.cancel(notify);
    const count = f.views.length;
    pending.resolve(device); await started;
    assert.deepEqual(f.calls, [["cancel", "fixture-flow"]]);
    assert.equal(f.views.length, count);
    assert.equal(f.timers.size, 0);
  }
});

test("cancelled authorization reply cannot refresh accounts or replace a new attempt", async () => {
  const pending = deferred();
  const f = fixture({ githubDevicePoll: () => pending.promise });
  await f.flow.start(); await f.advance(5000);
  f.flow.cancel(); await f.flow.start();
  pending.resolve({ kind: "authorized", account }); await flush();
  assert.deepEqual(f.authorized, []);
  assert.equal(f.views.at(-1).phase, "waiting");
  assert.equal(f.timers.size, 1);
});

test("unmount clears the waiting timer and cancels the backend flow", async () => {
  const f = fixture(); await f.flow.start();
  const count = f.views.length; f.flow.cancel(false);
  await f.advance(30000);
  assert.equal(f.timers.size, 0);
  assert.equal(f.views.length, count);
  assert.deepEqual(f.calls, ["start", "open", ["cancel", "fixture-flow"]]);
});

test("browser launch failure keeps code and polling; reopening can succeed", async () => {
  let attempts = 0;
  const f = fixture({ githubDeviceOpen: async () => { if (++attempts === 1) throw Error("fixture browser failure"); } });
  await f.flow.start(); await flush();
  assert.equal(f.errors.length, 1);
  assert.equal(f.views.at(-1).phase, "waiting");
  assert.equal(f.timers.size, 1);
  await f.flow.openBrowser(); assert.equal(attempts, 2);
});

test("slow-down interval and expiration are enforced without extra requests", async () => {
  const f = fixture({ githubDevicePoll: async () => { f.calls.push("poll"); return { kind: "pending", intervalMs: 10000, expiresInMs: 12000 }; } });
  await f.flow.start(); await f.advance(5000);
  await f.advance(9999); assert.equal(f.calls.filter((c) => c === "poll").length, 1);
  await f.advance(1); assert.equal(f.calls.filter((c) => c === "poll").length, 2);
  await f.advance(2000);
  assert.deepEqual(f.ended, ["expired"]);
  assert.equal(f.calls.filter((c) => c === "poll").length, 2);
  assert.equal(f.timers.size, 0);
});

test("authorization refreshes the account once and ends polling", async () => {
  const f = fixture({ githubDevicePoll: async () => ({ kind: "authorized", account }) });
  await f.flow.start(); await f.advance(5000); await f.advance(30000);
  assert.deepEqual(f.authorized, [account]);
  assert.equal(f.views.at(-1).phase, "idle");
  assert.equal(f.timers.size, 0);
});

test("denial, expiration and network errors are visible and release the flow", async () => {
  for (const kind of ["denied", "expired", "network"]) {
    const f = fixture({ githubDevicePoll: async () => {
      if (kind === "network") throw Error("fixture network failure");
      return { kind };
    } });
    await f.flow.start(); await f.advance(5000);
    assert.equal(f.views.at(-1).phase, "idle");
    assert.equal(f.timers.size, 0);
    assert.deepEqual(f.calls.at(-1), ["cancel", "fixture-flow"]);
    if (kind === "network") assert.equal(f.errors.length, 1);
    else assert.deepEqual(f.ended, [kind]);
  }
});
