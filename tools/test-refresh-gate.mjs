import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";

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
const { SnapshotGate, RefreshCoordinator } = await import("../src/lib/refreshGate.ts");
imports.deregister();

test("同 worktree 内旧版本快照被拒绝，新版本被接受", () => {
  const gate = new SnapshotGate();
  gate.activate("wt-a");
  const token = gate.token();

  assert.equal(gate.accept(token, { worktreeId: "wt-a", revision: 5 }), true);
  assert.equal(gate.accept(token, { worktreeId: "wt-a", revision: 4 }), false, "晚到的旧版本必须丢弃");
  assert.equal(gate.accept(token, { worktreeId: "wt-a", revision: 6 }), true);
  assert.equal(gate.acceptedRevision("wt-a"), 6);
});

test("切换 worktree 后旧 worktree 的响应被拒绝（A→B→A）", () => {
  const gate = new SnapshotGate();
  gate.activate("wt-a");
  const tokenA1 = gate.token();

  gate.activate("wt-b");
  const tokenB = gate.token();
  gate.activate("wt-a");
  const tokenA2 = gate.token();

  // 第一次 A 的迟到响应：激活代号已变，必须丢弃，即使 worktreeId 相同。
  assert.equal(gate.accept(tokenA1, { worktreeId: "wt-a", revision: 99 }), false);
  // B 的迟到响应也丢弃。
  assert.equal(gate.accept(tokenB, { worktreeId: "wt-b", revision: 3 }), false);
  // 当前这次 A 的响应可以接受。
  assert.equal(gate.accept(tokenA2, { worktreeId: "wt-a", revision: 1 }), true);
});

test("空列表快照同样参与版本判定", () => {
  const gate = new SnapshotGate();
  gate.activate("wt-a");
  const token = gate.token();
  assert.equal(gate.accept(token, { worktreeId: "wt-a", revision: 2 }), true, "空列表也带版本");
  assert.equal(gate.accept(token, { worktreeId: "wt-a", revision: 1 }), false);
});

test("刷新协调器合并事件：执行中的请求只补一轮", async () => {
  let runs = 0;
  let release;
  const gatePromise = new Promise((resolve) => { release = resolve; });

  const coordinator = new RefreshCoordinator(async () => {
    runs += 1;
    if (runs === 1) await gatePromise;
  });

  coordinator.request();
  await Promise.resolve();
  assert.equal(runs, 1, "第一轮立即开始");

  // 执行期间的事件风暴：不排队，只置脏。
  for (let index = 0; index < 50; index += 1) coordinator.request();
  assert.equal(runs, 1, "执行期间不得并发或多轮排队");

  release();
  await new Promise((resolve) => setImmediate(resolve));
  await new Promise((resolve) => setImmediate(resolve));

  assert.equal(runs, 2, "合并后最多补一轮，而不是 50 轮");
  assert.equal(coordinator.isRunning(), false);
});

test("刷新协调器单轮失败不终止后续刷新", async () => {
  let runs = 0;
  const coordinator = new RefreshCoordinator(async () => {
    runs += 1;
    if (runs === 1) throw new Error("单轮失败");
  });

  coordinator.request();
  await new Promise((resolve) => setImmediate(resolve));
  coordinator.request();
  await new Promise((resolve) => setImmediate(resolve));

  assert.equal(runs, 2, "失败后仍应响应新的刷新请求");
});

test("刷新协调器 dispose 后不再执行", async () => {
  let runs = 0;
  const coordinator = new RefreshCoordinator(async () => {
    runs += 1;
  });
  coordinator.dispose();
  coordinator.request();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(runs, 0);
});
