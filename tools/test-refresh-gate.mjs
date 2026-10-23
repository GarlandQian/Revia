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
const { SnapshotGate, RefreshCoordinator, OperationResultGate, SelectionGate, WatchEventGate } = await import("../src/lib/refreshGate.ts");
imports.deregister();
const { subscribeToForeground } = await import("../src/lib/foregroundRefresh.ts");

test("原生前台事件进入真实刷新协调器，失焦与卸载后的事件不刷新", async () => {
  let focus;
  let stopped = 0;
  let reads = 0;
  const errors = [];
  const coordinator = new RefreshCoordinator(async () => { reads += 1; });
  const dispose = subscribeToForeground(async (handler) => {
    focus = handler;
    return () => { stopped += 1; };
  }, () => coordinator.request(), (error) => errors.push(error));
  await Promise.resolve();
  focus(false);
  assert.equal(reads, 0);
  focus(true);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(reads, 1);
  dispose();
  dispose();
  focus(true);
  assert.equal(reads, 1);
  assert.equal(stopped, 1);
  assert.deepEqual(errors, []);
});

test("原生焦点监听安装晚于清理时立即释放，不泄漏旧目标监听", async () => {
  let installed;
  let focus;
  let stopped = 0;
  let refreshed = 0;
  const dispose = subscribeToForeground((handler) => {
    focus = handler;
    return new Promise((resolve) => { installed = resolve; });
  }, () => { refreshed += 1; }, assert.fail);
  dispose();
  focus(true);
  installed(() => { stopped += 1; });
  await Promise.resolve();
  assert.equal(refreshed, 0);
  assert.equal(stopped, 1);
});

test("原生焦点监听安装失败可见于调用方，清理后不再汇报旧错误", async () => {
  const failure = new Error("native focus subscription failed");
  const errors = [];
  subscribeToForeground(async () => { throw failure; }, assert.fail, (error) => errors.push(error));
  const dispose = subscribeToForeground(async () => { throw failure; }, assert.fail, (error) => errors.push(error));
  dispose();
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(errors, [failure]);
});

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

test("写结果走接收门：晚到的 r2 不能覆盖已接受的 r3", () => {
  const gate = new SnapshotGate();
  const ops = new OperationResultGate(gate);
  gate.activate("wt-a");
  const token = ops.begin("wt-a");

  // 自动刷新先接受了 r3。
  assert.equal(gate.accept(gate.token(), { worktreeId: "wt-a", revision: 3 }), true);

  // 之前发起的写操作结果 r2 迟到返回：必须被拒绝。
  assert.equal(ops.acceptSuccess(token, { worktreeId: "wt-a", revision: 2 }), false);
  assert.equal(gate.acceptedRevision("wt-a"), 3);

  // 更新的 r4 写结果可以接受。
  assert.equal(ops.acceptSuccess(token, { worktreeId: "wt-a", revision: 4 }), true);
  assert.equal(gate.acceptedRevision("wt-a"), 4);
});

test("写结果 A→B→A：第一次 A 的写结果被拒绝", () => {
  const gate = new SnapshotGate();
  const ops = new OperationResultGate(gate);
  gate.activate("wt-a");
  const tokenA1 = ops.begin("wt-a");
  gate.activate("wt-b");
  gate.activate("wt-a");

  assert.equal(ops.acceptSuccess(tokenA1, { worktreeId: "wt-a", revision: 9 }), false);
  // 失败结果同样不得作用到新的 A 界面。
  assert.equal(ops.acceptFailure(tokenA1), false);
});

test("失败结果只在发起目标仍激活时生效", () => {
  const gate = new SnapshotGate();
  const ops = new OperationResultGate(gate);
  gate.activate("wt-a");
  const token = ops.begin("wt-a");
  assert.equal(ops.acceptFailure(token), true);
  gate.activate("wt-b");
  assert.equal(ops.acceptFailure(token), false, "切走后旧失败不得污染当前界面");
});

test("切走后原操作结果仍可保存，不能覆盖该 worktree 更新的操作", () => {
  const gate = new SnapshotGate();
  const ops = new OperationResultGate(gate);
  gate.activate("wt-a");
  const a1 = ops.begin("wt-a");
  gate.activate("wt-b");
  const b = ops.begin("wt-b");
  assert.equal(ops.canRecord(a1), true, "非当前 worktree 的结果必须保留归属");
  assert.equal(ops.acceptFailure(a1), false, "但不能刷新当前 B");
  gate.activate("wt-a");
  const a2 = ops.begin("wt-a");
  assert.equal(ops.canRecord(a1), false, "旧失败不能覆盖新的 A 操作结果");
  assert.equal(ops.canRecord(a2), true);
  assert.equal(ops.canRecord(b), true, "A 的新操作不作废 B 的结果记录");
});

test("切换未完成时按真实写目标登记，不借用新激活目标的操作序号", () => {
  const gate = new SnapshotGate();
  const ops = new OperationResultGate(gate);
  gate.activate("wt-a");
  gate.activate("wt-b");
  // B 的 open_worktree 未完成，旧侧栏仍允许对 A 发起操作。
  const a = ops.begin("wt-a");
  const b = ops.begin("wt-b");
  assert.equal(a.worktreeId, "wt-a");
  assert.equal(ops.canRecord(a), true, "B 的后续操作不能吞掉 A 的错误或待核对结果");
  assert.equal(ops.canRecord(b), true);
  assert.equal(ops.acceptFailure(a), false, "A 的结果不能触发当前 B 的刷新");
  assert.equal(ops.acceptSuccess(a, { worktreeId: "wt-a", revision: 10 }), false);
  gate.activate("wt-a");
  assert.equal(ops.canRecord(a), true, "切回后仍保留原目标的结果提示");
  assert.equal(ops.acceptSuccess(a, { worktreeId: "wt-a", revision: 10 }), false,
    "切回 A 仍不能复活旧激活的快照");
  ops.begin("wt-a");
  assert.equal(ops.canRecord(a), false, "同目标更新的操作才能淘汰旧结果");
});

test("真实通知消费路径：首个、连续事件触发刷新，重复与旧激活通知忽略", async () => {
  const notifications = new WatchEventGate();
  let runs = 0;
  const coordinator = new RefreshCoordinator(async () => { runs += 1; });
  const poll = async (activation, changedActivation, eventSequence) => {
    if (notifications.consume(activation, { changedActivation, eventSequence })) coordinator.request();
    await new Promise((resolve) => setImmediate(resolve));
  };
  await poll(1, null, 0);
  assert.equal(runs, 0);
  await poll(1, 1, 1);
  assert.equal(runs, 1, "首个事件不能只记作基线后吞掉");
  await poll(1, 1, 2);
  assert.equal(runs, 2, "相同 activation 的后续事件也必须驱动刷新");
  await poll(1, 1, 2);
  await poll(2, 1, 3);
  assert.equal(runs, 2);
  await poll(2, 2, 4);
  assert.equal(runs, 3);
});

test("选择门：同 worktree U→V 时 U 的迟到元数据被作废", () => {
  const gate = new SelectionGate();
  const u = gate.begin("wt-a", "u.txt", "untracked", 1);
  const v = gate.begin("wt-a", "v.txt", "unstaged", 1);

  assert.equal(gate.isValid(u, 1), false, "U 已被 V 取代");
  assert.equal(gate.isValid(v, 1), true);
});

test("选择门：A→B→A 时第一次 A 的选择被作废", () => {
  const gate = new SelectionGate();
  const a1 = gate.begin("wt-a", "a.txt", "unstaged", 1);
  gate.begin("wt-b", "b.txt", "unstaged", 2);
  gate.begin("wt-a", "a.txt", "unstaged", 3);

  assert.equal(gate.isValid(a1, 1), false, "旧激活代号的同路径结果也不得生效");
  assert.equal(gate.isValid(a1, 3), false);
});

test("选择门：清空选择后迟到结果不得重新选中", () => {
  const gate = new SelectionGate();
  const context = gate.begin("wt-a", "a.txt", "unstaged", 1);
  gate.clear();
  assert.equal(gate.isValid(context, 1), false);
});

test("选择门：当前激活代号变化后旧上下文失效", () => {
  const gate = new SelectionGate();
  const context = gate.begin("wt-a", "a.txt", "unstaged", 1);
  assert.equal(gate.isValid(context, 1), true);
  assert.equal(gate.isValid(context, 2), false, "外部激活代号已变，旧上下文必须失效");
});
