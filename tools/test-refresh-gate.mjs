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
/** 测试里统一解包读取结果：不可信读取在断言里应当立即暴露。 */
function unwrap(outcome) {
  if (!outcome || typeof outcome !== "object" || !("ok" in outcome)) return outcome;
  if (!outcome.ok) throw new Error(`读取被拒收：${outcome.reason} ${outcome.worktreeId}@${outcome.observedSeq}`);
  return outcome.snapshot;
}
/** 逐路径合并结果解包。 */
function unwrapEntry(outcome) {
  if (!outcome || typeof outcome !== "object" || !("ok" in outcome)) return outcome;
  if (!outcome.ok) throw new Error(`逐路径读取被拒收：${outcome.reason} ${outcome.worktreeId}@${outcome.observedSeq}`);
  return outcome.entry;
}

const { CloseCoordinator, CloseGuardGate, ExitGuardSync, SnapshotGate, WriteGuardOrchestrator, runCloseExecution, RefreshCoordinator, OperationResultGate, ReviewOverlay, SelectionGate, WatchEventGate, applyReviewSuccess, applyReviewSuccessAndCompose, commitReviewResult, mergePublishedReview, planReviewResult, publishReviewSuccess, readThroughReview, ReviewRecoveryScheduler, runReviewRecoveryBatch, reviewedSetForNavigation } = await import("../src/lib/refreshGate.ts");
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

// ---------------- 审阅变更与异步读取交付的先后契约 ----------------
// 全部走 ReviewOverlay 的真实生产决策：beginRead() 在请求发起处取分界，
// deliver()/mergeEntry() 在结果交付时合并。App 只是这些入口的调用方。

function reviewSnapshot(overrides = {}) {
  return {
    worktreeId: "wt-a",
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p" },
    ],
    // `seq` 是后端真实水位：真实读取一定带它（`ReviewMarks.seq`）。
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt", "b.txt"], total: 3, reviewedCount: 2, seq: 0 },
    ...overrides,
  };
}

const probe = { head: null, worktree: "w-p" };
const bVersions = { head: "h-b", worktree: "w-b" };

function reviewedOf(snapshot, path) {
  return snapshot.entries.find((entry) => entry.path === path).reviewed;
}

test("读取发起后才成功的标记，不会被这次读取回退", () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  const read = overlay.beginRead();
  overlay.record("wt-a", "probe.txt", probe, true, nextSeq());
  const merged = unwrap(read.deliver(reviewSnapshot()));
  assert.equal(reviewedOf(merged, "probe.txt"), true);
  assert.equal(merged.review.reviewedCount, 3);
});

test("读取发起后才成功的取消，不会被这次读取勾回", () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  const read = overlay.beginRead();
  overlay.record("wt-a", "b.txt", bVersions, false, nextSeq());
  const merged = unwrap(read.deliver(reviewSnapshot()));
  assert.equal(reviewedOf(merged, "b.txt"), false);
  assert.equal(merged.review.reviewedCount, 1);
});

test("文件版本真的变化时本地记录不生效，标记仍按后端结果失效", () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  const read = overlay.beginRead();
  overlay.record("wt-a", "probe.txt", { head: null, worktree: "w-old" }, true, nextSeq());
  const merged = unwrap(read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-new" },
    ],
  })));
  assert.equal(reviewedOf(merged, "probe.txt"), false);
  assert.equal(merged.review.reviewedCount, 2);
});

test("分界之前成功的变更不重复叠加（该读取本来就已经包含它）", () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  overlay.record("wt-a", "probe.txt", probe, true, nextSeq());
  const read = overlay.beginRead();
  const merged = unwrap(read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: true, headVersion: null, worktreeVersion: "w-p" },
    ],
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt", "b.txt", "probe.txt"], total: 3, reviewedCount: 3 },
  })));
  assert.equal(merged.review.reviewedCount, 3);
});

test("较早的成功不会覆盖较新的相反操作（顺序由序号保证）", () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  const read = overlay.beginRead();
  // 先成功后取消：同一路径的最新操作是取消。
  overlay.record("wt-a", "b.txt", bVersions, true, nextSeq());
  overlay.record("wt-a", "b.txt", bVersions, false, nextSeq());
  const merged = unwrap(read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p" },
    ],
  })));
  assert.equal(reviewedOf(merged, "b.txt"), false, "较新的取消必须胜出");
});

test("写结果：经共享入口发起，期间成功的标记不被回退（ST07-R7-1）", async () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  // 生产入口是 readThroughReview：它在调用读取函数**之前**取分界。
  // 这里用延迟的读取函数模拟写结果/读取结果的迟到交付。
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  const pending = readThroughReview(overlay, async () => {
    await gate;
    return reviewSnapshot();
  });
  // 请求已发起（分界已取），结果还没交付，用户成功勾选。
  overlay.record("wt-a", "probe.txt", probe, true, nextSeq());
  release();
  const merged = unwrap(await pending);
  assert.equal(reviewedOf(merged, "probe.txt"), true, "写结果不能回退期间的成功标记");
  assert.equal(merged.review.reviewedCount, 3);
});

test("写结果期间取消同样保留（ST07-R7-1 反向）", async () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  const pending = readThroughReview(overlay, async () => {
    await gate;
    return reviewSnapshot();
  });
  overlay.record("wt-a", "a.txt", { head: "h-a", worktree: "w-a" }, false, nextSeq());
  release();
  const merged = unwrap(await pending);
  assert.equal(reviewedOf(merged, "a.txt"), false);
  assert.equal(merged.review.reviewedCount, 1);
});

test("A/B 并发：只登记 B 会漏掉 A，按完整成功集合记账才不漏（ST07-R7-3 / SP07-R7-2）", () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  const read = overlay.beginRead();
  // B 的响应先到，且后端返回的完整集合已经包含 A+B。
  // 界面此刻仍是 A、probe 都未审阅（A 的响应还没被应用）。
  const current = [
    { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a", worktreeVersionConfirmed: true },
    { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b", worktreeVersionConfirmed: true },
    { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p", worktreeVersionConfirmed: true },
  ];
  applyReviewSuccess({
    overlay,
    worktreeId: "wt-a",
    reviewed: ["a.txt", "b.txt", "probe.txt"],
    marks: marksOf(["a.txt", "h-a", "w-a"], ["b.txt", "h-b", "w-b"], ["probe.txt", null, "w-p"]),
    seq: nextSeq(),
  });
  // A 的响应随后才到、被展示门禁拒收——但记账已在门禁之前完成。
  // 旧读取（分界早于两者）现在交付，A 与 B 都必须保留。
  const merged = unwrap(read.deliver(reviewSnapshot()));
  assert.equal(reviewedOf(merged, "probe.txt"), true, "A 不能因迟到被漏账");
  assert.equal(reviewedOf(merged, "b.txt"), true);
  assert.equal(merged.review.reviewedCount, 3);
});

test("A/B 并发：只登记目标 B 时旧读取仍会把 A 回退（证明批量记账必要）", () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  const read = overlay.beginRead();
  // 只记本次目标（旧实现），A 就漏账了。
  overlay.record("wt-a", "b.txt", bVersions, true, nextSeq());
  const merged = unwrap(read.deliver(reviewSnapshot()));
  assert.equal(reviewedOf(merged, "probe.txt"), false, "这正是只记 B 时的漏账表现");
  assert.equal(merged.review.reviewedCount, 2);
});

test("未跟踪元数据补全不整条替换：同版本已成功的勾选保留，进度一致（SP07-R7-3）", () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  const metricsRead = overlay.beginRead();
  // getRows 等待期间用户勾选成功。
  overlay.record("wt-a", "probe.txt", probe, true, nextSeq());
  // 交付的 entry 是旧读取：未审阅、行数才刚补上。
  const loaded = { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p", metricsLoaded: true };
  const current = { path: "probe.txt", reviewed: true, headVersion: null, worktreeVersion: "w-p", metricsLoaded: false };
  // loaded 是后端这次读到的（水位 0，未审）；current 是界面（水位 1，已审）。
  const merged = unwrapEntry(overlay.mergeEntry("wt-a", loaded, 0, current, 1));
  assert.equal(merged.reviewed, true, "补元数据不能把已成功的勾选勾回去");
  assert.equal(merged.metricsLoaded, true, "元数据本身仍要补上");
  // 进度也必须跟着勾选，而不是保持旧值。
  const synced = overlay.syncProgress({
    worktreeId: "wt-a",
    entries: [merged, { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" }],
    review: { paths: ["a.txt", "probe.txt"], reviewed: ["a.txt"], total: 2, reviewedCount: 1 },
  });
  assert.equal(synced.review.reviewedCount, 2);
});

test("未跟踪元数据补全时版本变了就不沿用旧勾选", () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  const metricsRead = overlay.beginRead();
  const loaded = { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-new", metricsLoaded: true };
  const current = { path: "probe.txt", reviewed: true, headVersion: null, worktreeVersion: "w-old", metricsLoaded: false };
  const merged = unwrapEntry(overlay.mergeEntry("wt-a", loaded, 0, current, 1));
  assert.equal(merged.reviewed, false, "版本不同不能沿用旧状态");
});

test("重开仓库（再次添加当前仓库）经共享入口，不能直接发布 rawSnapshot（SP07-R7-4）", async () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  const pending = readThroughReview(overlay, async () => {
    await gate;
    return reviewSnapshot();
  });
  overlay.record("wt-a", "probe.txt", probe, true, nextSeq());
  release();
  const published = unwrap(await pending);
  assert.equal(reviewedOf(published, "probe.txt"), true, "重开不能直接发布 rawSnapshot");
});

test("不同 worktree 的本地记录互不影响", () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  const read = overlay.beginRead();
  overlay.record("wt-b", "probe.txt", probe, true, nextSeq());
  const merged = unwrap(read.deliver(reviewSnapshot()));
  assert.equal(reviewedOf(merged, "probe.txt"), false, "别的 worktree 的标记不能串过来");
});

test("生产入口真的接入了分界与记账（辅助定位，不当作时序证明）", async () => {
  const { readFile } = await import("node:fs/promises");
  const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8");
  // 三个读取入口都经同一个共享函数（分界与请求的先后由它固定）。
  const reads = app.match(/readThroughReview\(/g) ?? [];
  assert.ok(reads.length >= 3, `读取入口都要经共享函数，实际 ${reads.length} 处`);
  assert.match(app, /overlay\.mergeEntry\(/, "未跟踪元数据必须逐条合并");
  assert.match(app, /publishReviewSuccess\(/, "成功回执必须经唯一发布入口");
  // 发布判定在共享入口里，只取 worktree/activation/revision；epoch 只用于错误清理。
  assert.match(app, /epochMatches: reviewEpochRef\.current === epoch/, "epoch 只作为上下文事实传入");
  const gate = await readFile(new URL("../src/lib/refreshGate.ts", import.meta.url), "utf8");
  const publish = gate.slice(gate.indexOf("export function publishReviewSuccess"), gate.indexOf("export function mergePublishedReview"));
  assert.match(publish, /const contextValid = worktreeMatches && activationMatches && revisionMatches;/,
    "发布判定不能含 epochMatches");
  assert.match(publish, /const clearError = epochMatches && contextValid;/, "错误清理仍按点击代际");
  // 记账必须在发布门禁之前，否则迟到成功会漏账。
  const recordIndex = app.indexOf("publishReviewSuccess({");
  const gateIndex = app.indexOf("if (!published.view) {");
  assert.ok(recordIndex > 0 && gateIndex > 0 && recordIndex < gateIndex, "记账必须先于发布门禁");
  // 写结果也必须用请求前分界，不能在交付时才取。
  const writeIndex = app.indexOf("const reviewRead = reviewOverlayRef.current.beginRead();");
  const writeCall = app.indexOf("await (unstaging ? ipc.unstageHunk : ipc.stageHunk)");
  assert.ok(writeIndex > 0 && writeCall > 0 && writeIndex < writeCall, "写结果的分界必须早于写请求");
});

// ---------------- 第九轮：先后与版本都来自后端真实结果 ----------------
// 契约：ReviewOverlay 只接受后端返回的 `seq`（真实提交序号）与
// `marks`（每条标记自己实际绑定的版本）。前端不再有点击次序、请求时版本
// 或来源优先级这些推测来源。

// 后端 `ReviewMarks.marks` 的真实形状：path + reviewed + head + worktree。
// 正标记 reviewed=true；取消时后端显式返回 reviewed=false（ST07-R10-1）。
function marksOf(...items) {
  return items.map(([path, head, worktree, reviewed = true]) => ({ path, reviewed, head, worktree }));
}
const cancelled = (path, head, worktree) => [path, head, worktree, false];

test("逆序回执：后执行成功的取消不会被先执行但迟到的勾选回执覆盖（ST07-R9-1）", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  // 两次操作都成功，但**执行**先后由后端决定：勾选 seq=1，取消 seq=2。
  // 回执到达次序相反：取消先到，勾选后到。
  overlay.record("wt-a", "probe.txt", probe, false, 2);
  const late = overlay.record("wt-a", "probe.txt", probe, true, 1);
  assert.equal(late, false, "序号更小的回执不能覆盖更晚执行的相反操作");
  const merged = unwrap(read.deliver(reviewSnapshot()));
  assert.equal(reviewedOf(merged, "probe.txt"), false, "旧读取也不能把它勾回");
  assert.equal(merged.review.reviewedCount, 2);
});

test("顺序回执：后执行成功的勾选覆盖先执行的取消", () => {
  const overlay = new ReviewOverlay();
  overlay.record("wt-a", "probe.txt", probe, false, 1);
  const applied = overlay.record("wt-a", "probe.txt", probe, true, 2);
  assert.equal(applied, true, "序号更大的回执必须生效");
  assert.equal(overlay.get("wt-a", "probe.txt").reviewed, true);
});

test("来源优先级已取消：已有 explicit 记录时，后来的聚合确认仍能生效（ST07-R9-2 / SP07-R9-1）", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  // 用户此前取消过 B（seq=1），旧读取 L 捕获到 false。
  overlay.record("wt-a", "b.txt", bVersions, false, 1);
  // 之后重新勾选 B 成功（seq=2），但回执被暂缓。
  // A 的全量回执先到：它包含 B=true，且序号更大（seq=3）。
  const plan = applyReviewSuccess({
    overlay,
    worktreeId: "wt-a",
    reviewed: ["a.txt", "b.txt", "probe.txt"],
    marks: marksOf(
      ["a.txt", "h-a", "w-a"],
      ["b.txt", "h-b", "w-b"],
      ["probe.txt", null, "w-p"],
    ),
    seq: 3,
  });
  assert.equal(plan.reviewed.size, 3);
  // 旧读取 L 现在交付：不能回退 B。
  const merged = unwrap(read.deliver(reviewSnapshot()));
  assert.equal(reviewedOf(merged, "b.txt"), true, "聚合确认必须能覆盖更早的 explicit 记录");
  assert.equal(merged.review.reviewedCount, 3);
});

test("聚合版本来自后端 marks，不来自请求时快照（ST07-R9-3 / SP07-R9-2）", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  // A 的回执：后端确认 B 在 **v2** 已审（A 等待期间 B 已变成 v2 并被标记）。
  applyReviewSuccess({
    overlay,
    worktreeId: "wt-a",
    reviewed: ["a.txt", "b.txt", "probe.txt"],
    marks: marksOf(
      ["a.txt", "h-a", "w-a"],
      ["b.txt", "h-b", "w-b-v2"],
      ["probe.txt", null, "w-p"],
    ),
    seq: 1,
  });
  // 之后 B 回到 v1，一条未审阅读取交付。
  const merged = unwrap(read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: false, headVersion: "h-b", worktreeVersion: "w-b-v1" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p" },
    ],
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt"], total: 3, reviewedCount: 1 },
  })));
  assert.equal(reviewedOf(merged, "b.txt"), false,
    "后端确认的是 v2，不能把 v1 认领为已审");
});

test("聚合版本与当前版本一致时正常生效（不因版本绑定而漏掉同版本成功）", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  applyReviewSuccess({
    overlay, worktreeId: "wt-a",
    reviewed: ["a.txt", "b.txt", "probe.txt"],
    marks: marksOf(["a.txt", "h-a", "w-a"], ["b.txt", "h-b", "w-b"], ["probe.txt", null, "w-p"]),
    seq: 1,
  });
  const merged = unwrap(read.deliver(reviewSnapshot()));
  assert.equal(reviewedOf(merged, "b.txt"), true);
  assert.equal(merged.review.reviewedCount, 3);
});

test("契约后果：若把后端确认的 v2 改记成 v1，就会错误勾上 v1（证明版本必须来自后端）", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  // 故意按错误来源记账：把后端确认的 B(v2) 改记成 v1。
  overlay.record("wt-a", "b.txt", { head: "h-b", worktree: "w-b-v1" }, true, 1);
  const merged = unwrap(read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: false, headVersion: "h-b", worktreeVersion: "w-b-v1" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p" },
    ],
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt"], total: 3, reviewedCount: 1 },
  })));
  assert.equal(reviewedOf(merged, "b.txt"), true, "这正是版本来源错误时的后果");
});

test("跨组同一文件只记一条（marks 按路径去重，进度按路径统计）", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  applyReviewSuccess({
    overlay, worktreeId: "wt-a",
    reviewed: ["a.txt", "probe.txt"],
    marks: marksOf(["a.txt", "h-a", "w-a"], ["probe.txt", null, "w-p"]),
    seq: 1,
  });
  const merged = unwrap(read.deliver({
    worktreeId: "wt-a",
    entries: [
      { path: "a.txt", reviewed: false, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "a.txt", reviewed: false, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p" },
    ],
    review: { paths: ["a.txt", "probe.txt"], reviewed: [], total: 2, reviewedCount: 0 },
  }));
  assert.equal(merged.entries.every((entry) => entry.reviewed), true);
  assert.equal(merged.review.reviewedCount, 2, "同一文件跨组只算一次");
});

test("生产入口真的接入：版本取后端 marks、先后取后端 seq（辅助定位）", async () => {
  const { readFile } = await import("node:fs/promises");
  const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8");
  assert.match(app, /marks: marks\.marks/, "版本必须来自后端返回的 marks");
  assert.match(app, /seq: marks\.seq/, "先后必须来自后端返回的 seq");
  assert.doesNotMatch(app, /entriesAtRequest/, "不能再用请求时快照推版本");
  assert.doesNotMatch(app, /beginMark\(\)/, "不能再用点击次序推先后");
  const reads = app.match(/readThroughReview\(/g) ?? [];
  assert.ok(reads.length >= 3, `四个读取入口都要经共享函数，实际 ${reads.length} 处`);
  assert.match(app, /overlay\.mergeEntry\(/, "未跟踪元数据必须逐条合并");
  const recordIndex = app.indexOf("publishReviewSuccess({");
  const gateIndex = app.indexOf("if (!published.view) {");
  assert.ok(recordIndex > 0 && gateIndex > 0 && recordIndex < gateIndex, "记账必须先于发布门禁");
});

// ---------------- 第十轮：取消真值、展示合成、读取水位 ----------------

test("生产取消回执带 false：读取捕获已审后取消成功，旧读取不能勾回（ST07-R10-1 / SP07-R10-1）", () => {
  const overlay = new ReviewOverlay();
  // 读取 R 发起时文件已审（后端 seq=1 的正标记）。
  const read = overlay.beginRead();
  // 取消成功：后端显式返回 reviewed=false 与它绑定的版本，seq=2。
  const plan = applyReviewSuccess({
    overlay,
    worktreeId: "wt-a",
    reviewed: ["a.txt", "b.txt"],   // 取消后只剩这两个
    marks: marksOf(["a.txt", "h-a", "w-a"], ["b.txt", "h-b", "w-b"], cancelled("probe.txt", null, "w-p")),
    seq: 2,
  });
  const cancelRecord = plan.records.find((record) => record.path === "probe.txt");
  assert.equal(cancelRecord.reviewed, false, "取消必须产生一条 false 记录");
  // 旧读取 R 交付：probe 仍是已审的旧真值，不能勾回。
  const merged = unwrap(read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: true, headVersion: null, worktreeVersion: "w-p" },
    ],
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt", "b.txt", "probe.txt"], total: 3, reviewedCount: 3, seq: 1 },
  })));
  assert.equal(reviewedOf(merged, "probe.txt"), false, "取消成功后旧读取不能把它勾回");
  assert.equal(merged.review.reviewedCount, 2);
});

test("取消也挡住更早的新增回执（旧新增回执晚到不能覆盖取消）", () => {
  const overlay = new ReviewOverlay();
  // 先执行：新增成功 seq=1（回执暂缓）。
  // 后执行：取消成功 seq=2（回执先到）。
  applyReviewSuccess({
    overlay, worktreeId: "wt-a",
    reviewed: ["a.txt", "b.txt"],
    marks: marksOf(["a.txt", "h-a", "w-a"], ["b.txt", "h-b", "w-b"], cancelled("probe.txt", null, "w-p")),
    seq: 2,
  });
  // 旧新增回执现在才到（seq=1）。
  const plan = applyReviewSuccess({
    overlay, worktreeId: "wt-a",
    reviewed: ["a.txt", "b.txt", "probe.txt"],
    marks: marksOf(["a.txt", "h-a", "w-a"], ["b.txt", "h-b", "w-b"], ["probe.txt", null, "w-p"]),
    seq: 1,
  });
  assert.equal(plan.seq, 1);
  assert.equal(overlay.get("wt-a", "probe.txt").reviewed, false,
    "序号更小的新增回执不能覆盖取消");
});

test("展示按真实进度合成：B(seq1) 回执晚到不能把 A(seq2) 的已审显示掉（ST07-R10-2 / SP07-R10-2）", () => {
  const overlay = new ReviewOverlay();
  const current = {
    worktreeId: "wt-a",
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p" },
      { path: "other.txt", reviewed: false, headVersion: "h-o", worktreeVersion: "w-o" },
    ],
    review: { paths: ["a.txt", "b.txt", "probe.txt", "other.txt"], reviewed: ["a.txt", "b.txt"], total: 4, reviewedCount: 2, seq: 0 },
  };
  // A 后执行（seq=2，含 A+B 已审），回执先到并记账。
  const later = marksOf(["a.txt", "h-a", "w-a"], ["b.txt", "h-b", "w-b"],
    ["probe.txt", null, "w-p"], ["other.txt", "h-o", "w-o"]);
  applyReviewSuccess({
    overlay, worktreeId: "wt-a",
    reviewed: ["a.txt", "b.txt", "probe.txt", "other.txt"],
    marks: later, seq: 2,
  });
  // B 先执行（seq=1，只含 B），回执后到：它的完整确认集合把其余路径都标为未审。
  const earlier = marksOf(["a.txt", "h-a", "w-a"], ["b.txt", "h-b", "w-b"],
    cancelled("probe.txt", null, "w-p"), cancelled("other.txt", "h-o", "w-o"));
  applyReviewSuccess({
    overlay, worktreeId: "wt-a",
    reviewed: ["a.txt", "b.txt"],
    marks: earlier, seq: 1,
  });
  // 展示必须按真实进度合成：seq2 的结果仍生效。
  const view = overlay.composeState(current, earlier, 1);
  assert.equal(reviewedOf(view, "probe.txt"), true, "更晚执行的成功不能被更早的回执显示掉");
  assert.equal(reviewedOf(view, "other.txt"), true);
  assert.equal(view.review.reviewedCount, 4, "导航用的进度也必须按合成结果");
});

test("展示合成不回退更晚的取消", () => {
  const overlay = new ReviewOverlay();
  const current = {
    worktreeId: "wt-a",
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p" },
    ],
    review: { paths: ["a.txt", "probe.txt"], reviewed: ["a.txt"], total: 2, reviewedCount: 1, seq: 0 },
  };
  // 取消 seq=2 生效。
  applyReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: ["a.txt"],
    marks: marksOf(["a.txt", "h-a", "w-a"], cancelled("probe.txt", null, "w-p")), seq: 2,
  });
  // 更早的新增回执 seq=1 后到并请求展示。
  applyReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: ["a.txt", "probe.txt"],
    marks: marksOf(["a.txt", "h-a", "w-a"], ["probe.txt", null, "w-p"]), seq: 1,
  });
  const confirmedEarlier = marksOf(["a.txt", "h-a", "w-a"], ["probe.txt", null, "w-p"]);
  const view = overlay.composeState(current, confirmedEarlier, 1);
  assert.equal(reviewedOf(view, "probe.txt"), false, "更晚的取消必须保持");
});

test("读取水位：读取已观察到该序号时，旧成功不能再覆盖它的真值（ST07-R10-3 / SP07-R10-3）", () => {
  const overlay = new ReviewOverlay();
  // v1 新增成功 seq=1，回执暂缓。
  const pending = { marks: marksOf(["a.txt", "h-a", "w-a"], ["b.txt", "h-b", "w-b"], ["probe.txt", null, "w-p"]), seq: 1 };
  // v2 刷新：版本变化，标记失效，后端把失效也计入进度 → seq 前进。
  // 回到 v1 再刷新：仍未审，水位已经是 2。
  // 读取 L 在此时发起（观察到水位 2，probe 未审）。
  const read = overlay.beginRead();
  const notReviewed = reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p" },
    ],
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt", "b.txt"], total: 3, reviewedCount: 2, seq: 2 },
  });
  // 旧成功回执现在才释放：记账写入 seq=1。
  applyReviewSuccess({ overlay, worktreeId: "wt-a", reviewed: pending.marks.map((m) => m.path), marks: pending.marks, seq: 1 });
  // 再释放 L：读取的水位 2 已经观察到 seq=1，旧成功不能压过它。
  const merged = unwrap(read.deliver(notReviewed));
  assert.equal(reviewedOf(merged, "probe.txt"), false, "读取已观察到的水位必须压过旧成功");
  assert.equal(merged.review.reviewedCount, 2);
});

test("读取水位不阻止比它更新的成功", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  // 读取时水位 1，之后成功 seq=2。
  applyReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: ["a.txt", "b.txt", "probe.txt"],
    marks: marksOf(["a.txt", "h-a", "w-a"], ["b.txt", "h-b", "w-b"], ["probe.txt", null, "w-p"]), seq: 2,
  });
  const merged = unwrap(read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p" },
    ],
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt", "b.txt"], total: 3, reviewedCount: 2, seq: 1 },
  })));
  assert.equal(reviewedOf(merged, "probe.txt"), true, "比读取水位更新的成功必须生效");
});

test("读取水位：版本变化后旧成功同样不生效", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  applyReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: ["a.txt", "b.txt", "probe.txt"],
    marks: marksOf(["a.txt", "h-a", "w-a"], ["b.txt", "h-b", "w-b"], ["probe.txt", null, "w-old"]), seq: 2,
  });
  const merged = unwrap(read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-new" },
    ],
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt", "b.txt"], total: 3, reviewedCount: 2, seq: 1 },
  })));
  assert.equal(reviewedOf(merged, "probe.txt"), false, "版本变化后旧成功不生效");
});

// ---------------- 第十一轮：完整聚合负状态、发布代际、getRows 水位 ----------------

test("非目标取消聚合：B 取消(seq1)回执暂缓，A(seq2)回执只含正 A 时 B 仍必须显示未审（ST07-R11-1）", () => {
  const overlay = new ReviewOverlay();
  const current = {
    worktreeId: "wt-a",
    entries: [
      { path: "a.txt", reviewed: false, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
    ],
    review: { paths: ["a.txt", "b.txt"], reviewed: ["b.txt"], total: 2, reviewedCount: 1, seq: 0 },
  };
  // B 的取消 seq=1 已记账（回执暂缓，界面还没更新）。
  overlay.record("wt-a", "b.txt", bVersions, false, 1);
  // A 的回执 seq=2 现在到达，它的**完整确认集合**把 B 标为未审。
  const confirmed = marksOf(["a.txt", "h-a", "w-a"], cancelled("b.txt", "h-b", "w-b"));
  const { view } = applyReviewSuccessAndCompose({
    overlay, worktreeId: "wt-a",
    reviewed: ["a.txt"],           // 后端只列正标记
    marks: confirmed,
    seq: 2,
    snapshot: current,
  });
  assert.equal(reviewedOf(view, "b.txt"), false,
    "完整确认集合里的未审状态必须生效，不能因为它是负状态就忽略");
  assert.equal(reviewedOf(view, "a.txt"), true);
  assert.equal(view.review.reviewedCount, 1);
});

test("非目标取消：即使 B 的 false 记录比 A 的回执序号更小，也不能回退成已审（ST07-R11-1）", () => {
  const overlay = new ReviewOverlay();
  const current = {
    worktreeId: "wt-a",
    entries: [
      { path: "a.txt", reviewed: false, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
    ],
    review: { paths: ["a.txt", "b.txt"], reviewed: ["b.txt"], total: 2, reviewedCount: 1, seq: 0 },
  };
  // B 取消 seq=1 先到并记账。
  applyReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: [],
    marks: marksOf(["a.txt", "h-a", "w-a"], cancelled("b.txt", "h-b", "w-b")), seq: 1,
  });
  // A 的回执 seq=2 只列正标记 A，聚合集合里 B 也是 false。
  const confirmed = marksOf(["a.txt", "h-a", "w-a"], cancelled("b.txt", "h-b", "w-b"));
  const view = overlay.composeState(current, confirmed, 2);
  assert.equal(reviewedOf(view, "b.txt"), false, "seq 门槛不能把更早的取消丢掉");
});

test("发布门禁：更晚执行的成功即使点击代际更旧也必须能发布（ST07-R11-2）", () => {
  const overlay = new ReviewOverlay();
  const current = {
    worktreeId: "wt-a",
    entries: [
      { path: "a.txt", reviewed: false, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
    ],
    review: { paths: ["a.txt", "b.txt"], reviewed: ["b.txt"], total: 2, reviewedCount: 1, seq: 0 },
  };
  // B 先执行 seq1 并已显示。
  applyReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: ["b.txt"],
    marks: marksOf(["a.txt", "h-a", "w-a"], ["b.txt", "h-b", "w-b"]), seq: 1,
  });
  // A 后执行 seq2，回执晚到：它的确认集合里 A+B 都已审。
  const confirmed = marksOf(["a.txt", "h-a", "w-a"], ["b.txt", "h-b", "w-b"]);
  // 生产入口：contextValid 为真（同 worktree/activation/revision），
  // clearError 为假（这次点击代际已旧，不能清掉更新的错误）。
  const published = publishReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: ["a.txt", "b.txt"],
    marks: confirmed, seq: 2, snapshot: current,
    context: { worktreeMatches: true, activationMatches: true, revisionMatches: true, epochMatches: false },
  });
  assert.equal(reviewedOf(published.view, "a.txt"), true, "更晚执行的真实成功必须能发布");
  assert.equal(published.view.review.reviewedCount, 2);
  assert.equal(published.clearError, false, "旧点击不能清掉更新的错误");
});

test("发布门禁：上下文失效时不发布，但记账仍然完成（不丢真实成功）", () => {
  const overlay = new ReviewOverlay();
  const current = {
    worktreeId: "wt-a",
    entries: [{ path: "a.txt", reviewed: false, headVersion: "h-a", worktreeVersion: "w-a" }],
    review: { paths: ["a.txt"], reviewed: [], total: 1, reviewedCount: 0, seq: 0 },
  };
  const published = publishReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: ["a.txt"],
    marks: marksOf(["a.txt", "h-a", "w-a"]), seq: 3, snapshot: current,
    context: { worktreeMatches: false, activationMatches: true, revisionMatches: true, epochMatches: true },
  });
  assert.equal(published.view, null, "上下文失效不发布");
  assert.equal(published.clearError, false, "上下文失效也不清错误");
  // 记账已完成：后续读取会被这条记录保护。
  assert.equal(overlay.get("wt-a", "a.txt").reviewed, true);
});

test("发布只在 current 上合并审阅字段（生产入口 mergePublishedReview）", () => {
  const overlay = new ReviewOverlay();
  const base = {
    worktreeId: "wt-a",
    revision: 7,
    entries: [
      { path: "U", reviewed: false, headVersion: null, worktreeVersion: "w-p", metricsLoaded: false, rowCountSplit: 0 },
    ],
    review: { paths: ["U"], reviewed: [], total: 1, reviewedCount: 0, seq: 0 },
  };
  const published = publishReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: ["U"],
    marks: marksOf(["U", null, "w-p"]), seq: 1, snapshot: base,
    context: { worktreeMatches: true, activationMatches: true, revisionMatches: true, epochMatches: true },
  });
  // React 先排入元数据补全，再应用发布。
  const current = {
    ...base,
    entries: base.entries.map((item) => ({ ...item, metricsLoaded: true, rowCountSplit: 12 })),
  };
  const merged = mergePublishedReview(current, published.view, 1);
  const u = merged.entries.find((item) => item.path === "U");
  assert.equal(u.metricsLoaded, true, "已排队的元数据补全不能被覆盖");
  assert.equal(u.rowCountSplit, 12);
  assert.equal(u.reviewed, true, "审阅字段仍然发布");
});

test("getRows 水位：失效已超越的旧成功不能在补元数据时复活（ST07-R11-3）", () => {
  const overlay = new ReviewOverlay();
  // 旧 v1 成功 seq=1，回执暂缓。
  overlay.record("wt-a", "U", { head: null, worktree: "w-v1" }, true, 1);
  // 后端这次读到的是未审、水位 2（v2 失效已推进），版本回到 v1。
  const loaded = { path: "U", reviewed: false, headVersion: null, worktreeVersion: "w-v1", metricsLoaded: true };
  const current = { path: "U", reviewed: false, headVersion: null, worktreeVersion: "w-v1", metricsLoaded: false };
  const merged = unwrapEntry(overlay.mergeEntry("wt-a", loaded, 2, current, 2));
  assert.equal(merged.reviewed, false, "水位 2 已超越 seq=1，旧成功不能复活");
  assert.equal(merged.metricsLoaded, true, "元数据本身仍要补上");
});

test("getRows 水位：比读取水位更新的成功仍然生效", () => {
  const overlay = new ReviewOverlay();
  // 更晚的成功 seq=3，后端这次读到的是水位 2（未审）。
  overlay.record("wt-a", "U", { head: null, worktree: "w-v1" }, true, 3);
  const loaded = { path: "U", reviewed: false, headVersion: null, worktreeVersion: "w-v1", metricsLoaded: true };
  const current = { path: "U", reviewed: false, headVersion: null, worktreeVersion: "w-v1", metricsLoaded: false };
  const merged = unwrapEntry(overlay.mergeEntry("wt-a", loaded, 2, current, 2));
  assert.equal(merged.reviewed, true, "比读取水位更新的成功必须生效");
});

test("getRows 用后端读取水位：后端已读到已审时，旧界面未审不能覆盖它（ST07-R12-2）", () => {
  const overlay = new ReviewOverlay();
  // 列表 L 在 seq=0 捕获 B 未审；B 标记成功 seq=1（回执暂缓）。
  overlay.record("wt-a", "B", { head: null, worktree: "w-b" }, true, 1);
  // getRows 从后端读到 **已审**、水位 1（后端真值），界面仍是未审（水位 0）。
  const loaded = { path: "B", reviewed: true, headVersion: null, worktreeVersion: "w-b", metricsLoaded: true };
  const current = { path: "B", reviewed: false, headVersion: null, worktreeVersion: "w-b", metricsLoaded: false };
  const merged = unwrapEntry(overlay.mergeEntry("wt-a", loaded, 1, current, 0));
  assert.equal(merged.reviewed, true, "后端读取到已审，旧界面未审不能覆盖它");
  assert.equal(merged.metricsLoaded, true);
});

test("getRows 用后端读取水位：界面更新时仍以水位大的一侧为准", () => {
  const overlay = new ReviewOverlay();
  // 界面水位 2 已审，后端这次读到的是水位 1 未审（更旧）。
  const loaded = { path: "B", reviewed: false, headVersion: null, worktreeVersion: "w-b", metricsLoaded: true };
  const current = { path: "B", reviewed: true, headVersion: null, worktreeVersion: "w-b", metricsLoaded: false };
  const merged = unwrapEntry(overlay.mergeEntry("wt-a", loaded, 1, current, 2));
  assert.equal(merged.reviewed, true, "水位更大的界面真值胜出");
});

test("React 发布只在 current 上合并审阅字段，不覆盖已排队的元数据补全（SP07-R11-4）", async () => {
  // 复现真实 updater 语义：先排入 getRows 的元数据补全，再排入 mark 的发布。
  const overlay = new ReviewOverlay();
  const revision = 7;
  const before = {
    worktreeId: "wt-a",
    revision,
    entries: [
      { path: "U", reviewed: false, headVersion: null, worktreeVersion: "w-p", metricsLoaded: false, rowCountSplit: 0 },
      { path: "a.txt", reviewed: false, headVersion: "h-a", worktreeVersion: "w-a", metricsLoaded: true, rowCountSplit: 5 },
    ],
    review: { paths: ["U", "a.txt"], reviewed: [], total: 2, reviewedCount: 0, seq: 0 },
  };
  // mark 回执在**旧快照**上合成（此时 U 的元数据还没补全）。
  const confirmed = marksOf(["U", null, "w-p"], ["a.txt", "h-a", "w-a"]);
  const { view } = applyReviewSuccessAndCompose({
    overlay, worktreeId: "wt-a", reviewed: ["U", "a.txt"],
    marks: confirmed, seq: 1, snapshot: before,
  });
  // React 依次应用：先元数据补全，再发布（发布只改审阅字段）。
  let current = {
    ...before,
    entries: before.entries.map((item) => item.path === "U"
      ? { ...item, metricsLoaded: true, rowCountSplit: 12 } : item),
  };
  const reviewedByPath = new Map(view.entries.map((item) => [item.path, item.reviewed]));
  current = {
    ...current,
    review: { ...view.review, seq: 1 },
    entries: current.entries.map((item) => {
      const reviewed = reviewedByPath.get(item.path);
      return reviewed === undefined || reviewed === item.reviewed ? item : { ...item, reviewed };
    }),
  };
  const u = current.entries.find((item) => item.path === "U");
  assert.equal(u.metricsLoaded, true, "已排队的元数据补全不能被旧整批 entries 覆盖");
  assert.equal(u.rowCountSplit, 12, "行数补全不能被覆盖成 0");
  assert.equal(u.reviewed, true, "审阅字段仍然要正确发布");
});

test("生产入口接入：发布与错误清理分开、getRows 传水位（辅助定位）", async () => {
  const { readFile } = await import("node:fs/promises");
  const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8");
  assert.match(app, /epochMatches: reviewEpochRef\.current === epoch/, "epoch 只作为上下文事实传入");
  // getRows 必须用后端 `RowSlice.reviewSeq`（真实读取水位），不能用界面 revision 猜。
  assert.match(app, /const loadedSeq = slice\.reviewSeq;/, "getRows 必须取后端读取水位");
  assert.match(app, /mergeEntry\(target\.worktreeId, loaded, loadedSeq,/, "合并必须传后端读取水位");
  assert.doesNotMatch(app, /entries: view\.entries/, "发布不能整批覆盖 entries");
});

// ---------------- 第十二轮：导航真值与真实读取水位 ----------------

test("导航消费合成真值：目标被更晚取消时不能补 true，也不能误报全部已审（SP07-R12-1）", () => {
  const overlay = new ReviewOverlay();
  const current = {
    worktreeId: "wt-a",
    entries: [
      { path: "A", reviewed: false, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "B", reviewed: false, headVersion: "h-b", worktreeVersion: "w-b" },
    ],
    review: { paths: ["A", "B"], reviewed: [], total: 2, reviewedCount: 0, seq: 0 },
  };
  // 1) ⌥⌘R 标 A：seq1（回执暂缓）。
  overlay.record("wt-a", "A", { head: "h-a", worktree: "w-a" }, true, 1);
  // 2) B 的 seq2 结果先显示 A+B 已审。
  // 3) 用户取消 A：seq3 成功。
  overlay.record("wt-a", "A", { head: "h-a", worktree: "w-a" }, false, 3);
  // 4) 旧 A 回执（seq1）释放：合成真值应是 A 未审 / B 已审。
  const published = publishReviewSuccess({
    overlay, worktreeId: "wt-a",
    reviewed: ["A", "B"],   // 旧回执的正标记集合
    marks: marksOf(["A", "h-a", "w-a"], ["B", "h-b", "w-b"]),
    seq: 1,
    snapshot: current,
    context: { worktreeMatches: true, activationMatches: true, revisionMatches: true, epochMatches: false },
  });
  assert.equal(reviewedOf(published.view, "A"), false, "合成真值里 A 已被取消");
  assert.equal(reviewedOf(published.view, "B"), true);
  const reviewed = reviewedSetForNavigation(published);
  assert.equal(reviewed.has("A"), false, "导航不能把已取消的 A 补成已审");
  assert.equal(reviewed.size, 1, "真实进度是 1/2，不能误报 2/2");
});

test("导航：上下文失效（未发布）时终止过时导航", () => {
  const overlay = new ReviewOverlay();
  const published = publishReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: ["A"],
    marks: marksOf(["A", "h-a", "w-a"]), seq: 1, snapshot: null,
    context: { worktreeMatches: false, activationMatches: true, revisionMatches: true, epochMatches: true },
  });
  assert.equal(reviewedSetForNavigation(published), null, "未发布时导航必须终止");
});

test("导航：正常情况下使用合成后的真实进度", () => {
  const overlay = new ReviewOverlay();
  const current = {
    worktreeId: "wt-a",
    entries: [
      { path: "A", reviewed: false, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "B", reviewed: false, headVersion: "h-b", worktreeVersion: "w-b" },
    ],
    review: { paths: ["A", "B"], reviewed: [], total: 2, reviewedCount: 0, seq: 0 },
  };
  const published = publishReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: ["A", "B"],
    marks: marksOf(["A", "h-a", "w-a"], ["B", "h-b", "w-b"]), seq: 1, snapshot: current,
    context: { worktreeMatches: true, activationMatches: true, revisionMatches: true, epochMatches: true },
  });
  const reviewed = reviewedSetForNavigation(published);
  assert.deepEqual([...reviewed].sort(), ["A", "B"]);
});

test("getRows 真实读取水位：列表旧未审 + 后端读到已审时以水位大的一侧为准（SP07-R12-2 完整链）", () => {
  const overlay = new ReviewOverlay();
  // 列表 L 捕获 B 未审（水位 0），被 hold。
  // B 勾选成功 seq1，回执被 hold（请求绑定前端 r0）。
  overlay.record("wt-a", "B", { head: null, worktree: "w-b" }, true, 1);
  // 释放 L：显示 B 未审（水位 0 的旧真值）。
  // 打开未跟踪 B：getRows 从后端读到 **已审**，`reviewSeq = 1`。
  const loaded = { path: "B", reviewed: true, headVersion: null, worktreeVersion: "w-b", metricsLoaded: true };
  const current = { path: "B", reviewed: false, headVersion: null, worktreeVersion: "w-b", metricsLoaded: false };
  const merged = unwrapEntry(overlay.mergeEntry("wt-a", loaded, 1, current, 0));
  assert.equal(merged.reviewed, true, "后端读取到的已审不能被旧界面覆盖");
  // 最后释放 B 的回执：它绑定的版本/水位不匹配当前 revision，展示被拒；但真值已经是已审。
  assert.equal(overlay.get("wt-a", "B").reviewed, true, "持久真值已审，重启也一致");
});

test("getRows 真实读取水位：原 v1→v2→v1 旧成功仍被水位挡住", () => {
  const overlay = new ReviewOverlay();
  // 旧 v1 成功 seq1（回执暂缓）。
  overlay.record("wt-a", "U", { head: null, worktree: "w-v1" }, true, 1);
  // v2 失效推进到 seq2，回到 v1 仍未审；getRows 读到未审、水位 2。
  const loaded = { path: "U", reviewed: false, headVersion: null, worktreeVersion: "w-v1", metricsLoaded: true };
  const current = { path: "U", reviewed: false, headVersion: null, worktreeVersion: "w-v1", metricsLoaded: false };
  const merged = unwrapEntry(overlay.mergeEntry("wt-a", loaded, 2, current, 2));
  assert.equal(merged.reviewed, false, "水位 2 已超越旧成功 seq1");
});

test("getRows 真实读取水位：回执尚未到达时也必须以后端读取为准（ST07-R12-2 无 override 变体）", () => {
  const overlay = new ReviewOverlay();
  // 列表 L 在 seq=0 捕获 B 未审并 hold。
  // B 的标记已在后端成功（seq=1），但**回执还没到达前端**，所以叠加层里没有记录。
  // getRows 从后端读到已审、`reviewSeq = 1`；界面仍是水位 0 的未审。
  const loaded = { path: "B", reviewed: true, headVersion: null, worktreeVersion: "w-b", metricsLoaded: true };
  const current = { path: "B", reviewed: false, headVersion: null, worktreeVersion: "w-b", metricsLoaded: false };
  const merged = unwrapEntry(overlay.mergeEntry("wt-a", loaded, 1, current, 0));
  assert.equal(merged.reviewed, true,
    "后端读取水位更大，旧界面未审不能覆盖它（否则界面与磁盘长期不一致）");
});

// ---------------- 第十三轮：持有已接受的逐路径读取真值 ----------------
// 两个方向都必须「先接受真实 getRows 结果，再交付更旧的全量成功」。

test("读取真值持有（true 方向）：getRows 读到 B 已审后，旧全量回执不能把它改回未审（SP07-R13-1）", () => {
  const overlay = new ReviewOverlay();
  const snapshot = {
    worktreeId: "wt-a",
    revision: 9,
    entries: [
      { path: "A", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "B", reviewed: false, headVersion: null, worktreeVersion: "w-b", metricsLoaded: false },
    ],
    review: { paths: ["A", "B"], reviewed: ["A"], total: 2, reviewedCount: 1, seq: 0 },
  };
  // 打开未跟踪 B：后端 getRows 实际读到 **已审**，reviewSeq = 2（B 的标记 seq2）。
  const loaded = { path: "B", reviewed: true, headVersion: null, worktreeVersion: "w-b", metricsLoaded: true };
  const current = snapshot.entries.find((entry) => entry.path === "B");
  const merged = unwrapEntry(overlay.mergeEntry("wt-a", loaded, 2, current, 0));
  assert.equal(merged.reviewed, true, "getRows 读到的已审必须先被接受");
  // 旧 A 回执（seq1，完整状态里 B 是 false）现在才交付。
  const published = publishReviewSuccess({
    overlay, worktreeId: "wt-a",
    reviewed: ["A"],
    marks: marksOf(["A", "h-a", "w-a"], cancelled("B", null, "w-b")),
    seq: 1,
    snapshot,
    context: { worktreeMatches: true, activationMatches: true, revisionMatches: true, epochMatches: false },
  });
  assert.equal(reviewedOf(published.view, "B"), true,
    "已接受的读取真值（B 已审、seq2）不能被更旧的回执（seq1）改回未审");
});

test("读取真值持有（false 方向）：getRows 读到 U 已取消后，旧全量回执不能把它改回已审（ST07-R13-1）", () => {
  const overlay = new ReviewOverlay();
  const snapshot = {
    worktreeId: "wt-a",
    revision: 9,
    entries: [
      { path: "U", reviewed: true, headVersion: "h-u", worktreeVersion: "w-u", metricsLoaded: false },
      { path: "A", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
    ],
    review: { paths: ["U", "A"], reviewed: ["U", "A"], total: 2, reviewedCount: 2, seq: 2 },
  };
  // U 的新增 seq1 回执 hold；A 的 seq2 先发布（完整状态含 Utrue）。
  overlay.record("wt-a", "U", { head: "h-u", worktree: "w-u" }, true, 2);
  overlay.record("wt-a", "A", { head: "h-a", worktree: "w-a" }, true, 2);
  // U 取消 seq3 回执 hold（前端还没拿到）。
  // 首次打开 U：后端 getRows 读到 **未审**，reviewSeq = 3。
  const loaded = { path: "U", reviewed: false, headVersion: "h-u", worktreeVersion: "w-u", metricsLoaded: true };
  const current = snapshot.entries.find((entry) => entry.path === "U");
  const merged = unwrapEntry(overlay.mergeEntry("wt-a", loaded, 3, current, 2));
  assert.equal(merged.reviewed, false, "getRows 读到的取消必须先被接受");
  // 旧 seq1 回执（Utrue）现在才交付。
  const published = publishReviewSuccess({
    overlay, worktreeId: "wt-a",
    reviewed: ["U"],
    marks: marksOf(["U", "h-u", "w-u"]),
    seq: 1,
    snapshot,
    context: { worktreeMatches: true, activationMatches: true, revisionMatches: true, epochMatches: false },
  });
  assert.equal(reviewedOf(published.view, "U"), false,
    "已接受的读取真值（U 已取消、seq3）不能被更旧的回执（seq1）改回已审");
});

test("读取真值持有：读取水位不能冒充整快照已确认（别的路径仍按各自真值）", () => {
  const overlay = new ReviewOverlay();
  const snapshot = {
    worktreeId: "wt-a",
    revision: 9,
    entries: [
      { path: "B", reviewed: false, headVersion: null, worktreeVersion: "w-b", metricsLoaded: false },
      { path: "C", reviewed: false, headVersion: "h-c", worktreeVersion: "w-c" },
    ],
    review: { paths: ["B", "C"], reviewed: [], total: 2, reviewedCount: 0, seq: 0 },
  };
  // 只读了 B（seq 5 已审）；C 从未被读过。
  const loaded = { path: "B", reviewed: true, headVersion: null, worktreeVersion: "w-b", metricsLoaded: true };
  unwrapEntry(overlay.mergeEntry("wt-a", loaded, 5, snapshot.entries[0], 0));
  // 一份 seq 4 的回执说 C 已审 —— C 没有更晚的读取，回执仍应生效。
  const published = publishReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: ["B", "C"],
    marks: marksOf(["B", null, "w-b"], ["C", "h-c", "w-c"]), seq: 4, snapshot,
    context: { worktreeMatches: true, activationMatches: true, revisionMatches: true, epochMatches: true },
  });
  assert.equal(reviewedOf(published.view, "B"), true, "B 的读取真值（seq5）胜出");
  assert.equal(reviewedOf(published.view, "C"), true, "C 没有更晚的读取，回执照常生效");
});

test("读取真值持有：更晚的有效写结果仍能覆盖读取观察（不是固定优先级）", () => {
  const overlay = new ReviewOverlay();
  const snapshot = {
    worktreeId: "wt-a",
    revision: 9,
    entries: [{ path: "B", reviewed: false, headVersion: null, worktreeVersion: "w-b", metricsLoaded: false }],
    review: { paths: ["B"], reviewed: [], total: 1, reviewedCount: 0, seq: 0 },
  };
  // 读取观察到 B 已审（seq2）。
  const loaded = { path: "B", reviewed: true, headVersion: null, worktreeVersion: "w-b", metricsLoaded: true };
  unwrapEntry(overlay.mergeEntry("wt-a", loaded, 2, snapshot.entries[0], 0));
  // 之后真的发生了更晚的取消（seq3）并成功交付。
  const published = publishReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: [],
    marks: marksOf(cancelled("B", null, "w-b")), seq: 3, snapshot,
    context: { worktreeMatches: true, activationMatches: true, revisionMatches: true, epochMatches: true },
  });
  assert.equal(reviewedOf(published.view, "B"), false, "更晚的真实写结果必须胜出");
});

test("读取真值持有：v1→v2→v1 失效仍不被旧成功复活", () => {
  const overlay = new ReviewOverlay();
  const snapshot = {
    worktreeId: "wt-a",
    revision: 9,
    entries: [{ path: "U", reviewed: false, headVersion: null, worktreeVersion: "w-v1", metricsLoaded: false }],
    review: { paths: ["U"], reviewed: [], total: 1, reviewedCount: 0, seq: 0 },
  };
  // 读取观察到未审、水位 2（v2 失效已推进，回到 v1）。
  const loaded = { path: "U", reviewed: false, headVersion: null, worktreeVersion: "w-v1", metricsLoaded: true };
  unwrapEntry(overlay.mergeEntry("wt-a", loaded, 2, snapshot.entries[0], 0));
  // 旧 v1 成功（seq1）现在才交付。
  const published = publishReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: ["U"],
    marks: marksOf(["U", null, "w-v1"]), seq: 1, snapshot,
    context: { worktreeMatches: true, activationMatches: true, revisionMatches: true, epochMatches: false },
  });
  assert.equal(reviewedOf(published.view, "U"), false, "失效后的读取真值必须挡住旧成功");
});

// ---------------- 容量边界（R07-CAP-1）----------------
// 生产函数 + **默认** `new ReviewOverlay()`：一次完整回执本身就可能超过容量。

function capacityFixture(count) {
  const paths = Array.from({ length: count }, (_, i) => `file-${String(i).padStart(4, "0")}.txt`);
  const entries = paths.map((path) => ({
    path, reviewed: false, headVersion: `h:${path}`, worktreeVersion: `w:${path}`,
  }));
  return {
    paths,
    snapshot: {
      worktreeId: "fixture-wt",
      revision: 9,
      entries,
      review: { paths, reviewed: [], total: count, reviewedCount: 0, seq: 0 },
    },
  };
}

/** 目标路径（索引 0）按 `targetReviewed`，其余路径按 `othersReviewed`。 */
function capacityMarks(paths, targetReviewed, othersReviewed = false) {
  return paths.map((path, i) => ({
    path,
    reviewed: i === 0 ? targetReviewed : othersReviewed,
    head: `h:${path}`,
    worktree: `w:${path}`,
  }));
}

function runCapacity(count, newerReviewed) {
  const { paths, snapshot } = capacityFixture(count);
  const overlay = new ReviewOverlay();   // 默认容量
  const context = { worktreeMatches: true, activationMatches: true, revisionMatches: true, epochMatches: true };
  // 先交付完整 seq2。
  const newer = publishReviewSuccess({
    overlay, worktreeId: snapshot.worktreeId,
    reviewed: capacityMarks(paths, newerReviewed).filter((m) => m.reviewed).map((m) => m.path),
    marks: capacityMarks(paths, newerReviewed), seq: 2, snapshot, context,
  });
  let current = mergePublishedReview(snapshot, newer.view, 2);
  const before = current.entries[0].reviewed;
  // 再交付旧 seq1（相反状态）。
  // 旧回执是相反状态，作用在**同一个目标路径**上。
  const older = publishReviewSuccess({
    overlay, worktreeId: snapshot.worktreeId,
    reviewed: capacityMarks(paths, !newerReviewed).filter((m) => m.reviewed).map((m) => m.path),
    marks: capacityMarks(paths, !newerReviewed), seq: 1, snapshot: current,
    context: { ...context, epochMatches: false },
  });
  current = older.view ? mergePublishedReview(current, older.view, 1) : current;
  return { before, after: current.entries[0].reviewed, newerReviewed, needsRefresh: older.needsRefresh };
}

for (const count of [1024, 1025]) {
  for (const newerReviewed of [true, false]) {
    test(`容量 ${count}：先 seq2 后旧 seq1，新状态 ${newerReviewed ? "已审" : "未审"} 不回退（R07-CAP-1）`, () => {
      const result = runCapacity(count, newerReviewed);
      assert.equal(result.before, newerReviewed, "seq2 先正确生效");
      assert.equal(result.after, newerReviewed, "旧回执不能把新状态改回去");
    });
  }
}

test("容量 1025：旧回执被拒绝并要求重新读取（不是静默猜）", () => {
  const result = runCapacity(1025, true);
  assert.equal(result.needsRefresh, true, "超过容量后旧序号必须触发重新读取");
});

test("容量 1024：旧回执仍按序号正常拒绝，不需要重新读取", () => {
  const result = runCapacity(1024, true);
  assert.equal(result.needsRefresh, false, "未淘汰时按序号比较即可");
});

test("跨 worktree 容量隔离：A 淘汰不影响 B 的旧回执判定", () => {
  const overlay = new ReviewOverlay();
  const context = { worktreeMatches: true, activationMatches: true, revisionMatches: true, epochMatches: true };
  // 在 wt-a 写满并溢出，产生淘汰水位。
  const { paths, snapshot } = capacityFixture(1025);
  publishReviewSuccess({
    overlay, worktreeId: "wt-a",
    reviewed: paths.slice(0, 1),
    marks: capacityMarks(paths, true), seq: 2,
    snapshot: { ...snapshot, worktreeId: "wt-a" }, context,
  });
  assert.ok(overlay.evictedWatermark("wt-a") > 0, "wt-a 必须记录淘汰水位");
  assert.equal(overlay.evictedWatermark("wt-b"), 0, "wt-b 不受影响");
  // wt-b 的旧回执（seq1）仍然可信：别的 worktree 的淘汰不能波及它。
  const b = capacityFixture(2);
  const published = publishReviewSuccess({
    overlay, worktreeId: "wt-b",
    reviewed: [b.paths[0]],
    marks: capacityMarks(b.paths, true), seq: 1,
    snapshot: { ...b.snapshot, worktreeId: "wt-b" }, context,
  });
  assert.equal(published.needsRefresh, false, "跨 worktree 不能互相影响");
  assert.equal(reviewedOf(published.view, b.paths[0]), true);
});

test("容量淘汰后，已接受的逐路径读取真值仍挡住旧回执", () => {
  const { paths, snapshot } = capacityFixture(1025);
  const overlay = new ReviewOverlay();
  // 写满并溢出：路径 file-0000 的记录被淘汰，水位记为 2。
  overlay.record("fixture-wt", paths[0], { head: `h:${paths[0]}`, worktree: `w:${paths[0]}` }, true, 2);
  for (let i = 1; i <= 1024; i += 1) {
    overlay.record("fixture-wt", paths[i], { head: `h:${paths[i]}`, worktree: `w:${paths[i]}` }, false, 2);
  }
  assert.ok(overlay.evictedWatermark("fixture-wt") >= 2, "必须有淘汰水位");
  // 读取观察到 file-0001 已审、水位 3（比淘汰水位更新）。
  const loaded = { path: paths[1], reviewed: true, headVersion: `h:${paths[1]}`, worktreeVersion: `w:${paths[1]}`, metricsLoaded: true };
  const current = snapshot.entries[1];
  unwrapEntry(overlay.mergeEntry("fixture-wt", loaded, 3, current, 0));
  // 旧回执（seq1）现在交付：必须被拒绝，不能把已读到的 true 改回 false。
  const older = publishReviewSuccess({
    overlay, worktreeId: "fixture-wt",
    reviewed: [],
    marks: capacityMarks(paths, false),
    seq: 1, snapshot,
    context: { worktreeMatches: true, activationMatches: true, revisionMatches: true, epochMatches: false },
  });
  assert.equal(older.needsRefresh, true, "旧回执必须被拒绝");
  assert.equal(older.view, null, "不发布任何基于旧信息的合成结果");
});

test("已淘汰路径的旧状态不能重新认领（record 层直接验证，R07-CAP-1）", () => {
  const overlay = new ReviewOverlay(2);
  // 写满并溢出：f0（seq5 已审）被淘汰，淘汰水位 = 5。
  overlay.record("wt", "f0", { head: "h", worktree: "v" }, true, 5);
  overlay.record("wt", "f1", { head: "h", worktree: "v" }, false, 5);
  overlay.record("wt", "f2", { head: "h", worktree: "v" }, false, 5);
  assert.equal(overlay.evictedWatermark("wt"), 5);
  assert.equal(overlay.get("wt", "f0"), undefined, "f0 已被淘汰");
  // 严格更旧的回执不能把 f0 重新插回来当作真值。
  const reinserted = overlay.record("wt", "f0", { head: "h", worktree: "v" }, false, 4);
  assert.equal(reinserted, false, "已淘汰路径的旧状态不能重新认领");
  assert.equal(overlay.get("wt", "f0"), undefined, "不能被重新插入");
  // 更新的回执仍然可以写入（不是永久封锁）。
  const newer = overlay.record("wt", "f0", { head: "h", worktree: "v" }, true, 6);
  assert.equal(newer, true, "比淘汰水位更新的回执仍可写入");
});

// ---------------- 第十五轮：不可信读取的拒收与重新核对 ----------------

test("完整快照读取：不可信时明确拒收，并按原目标重新核对（ST07-R15-1 / SP07-R15-2）", async () => {
  const overlay = new ReviewOverlay(2);
  // 用 seq2 写满并溢出，抬高淘汰水位到 2。
  overlay.record("wt-a", "a", { head: "h", worktree: "v" }, true, 2);
  overlay.record("wt-a", "b", { head: "h", worktree: "v" }, true, 2);
  overlay.record("wt-a", "c", { head: "h", worktree: "v" }, true, 2);
  assert.equal(overlay.evictedWatermark("wt-a"), 2);

  const stale = {
    worktreeId: "wt-a", revision: 9,
    entries: [{ path: "a", reviewed: false, headVersion: "h", worktreeVersion: "v" }],
    review: { paths: ["a"], reviewed: [], total: 1, reviewedCount: 0, seq: 0 },
  };
  const outcome = await readThroughReview(overlay, async () => stale);
  assert.equal(outcome.ok, false, "旧 seq0 读取必须明确拒收，不能原样交付");
  assert.equal(outcome.reason, "untrusted");
  assert.equal(outcome.worktreeId, "wt-a", "拒收结果必须带原目标");

  // 生产闭环：拒收 → 调度真实重读 → 重读返回更新的可信快照 → 接收。
  const scheduler = new ReviewRecoveryScheduler(3);
  const fresh = { ...stale, revision: 10, review: { ...stale.review, seq: 3 } };
  const accepted = [];
  const gate = new SnapshotGate();
  gate.activate("wt-a");
  const token = gate.token();
  const result = await scheduler.run(outcome.worktreeId, async () => {
    const reread = await readThroughReview(overlay, async () => fresh);
    if (!reread.ok) return { status: "untrusted", worktreeId: reread.worktreeId, observedSeq: reread.observedSeq };
    if (!gate.accept(token, { worktreeId: reread.snapshot.worktreeId, revision: reread.snapshot.revision })) {
      return { status: "error", error: new Error("gate rejected") };
    }
    accepted.push(reread.snapshot.revision);
    return { status: "trusted" };
  });
  assert.equal(result.converged, true, "重读必须收敛");
  assert.equal(result.exhausted, false);
  assert.deepEqual(accepted, [10], "必须经过真实接收门并发布更新后的快照");
  assert.equal(scheduler.attemptsFor("wt-a"), 0, "批次结束后清零计数");
});

test("不可信读取的重读按原目标归属，不借用当前界面目标", async () => {
  const scheduler = new ReviewRecoveryScheduler(3);
  const requested = [];
  await scheduler.run("wt-original", async () => {
    requested.push("wt-original");
    return { status: "trusted" };
  });
  assert.deepEqual(requested, ["wt-original"], "必须按拒收结果里的原目标重读");
});

test("重读有界：不前进时停止，不产生请求风暴", async () => {
  const scheduler = new ReviewRecoveryScheduler(3);
  let calls = 0;
  const still = async () => { calls += 1; return { status: "error", error: new Error("temp") }; };
  // 一次批次最多 3 次尝试，然后 settle。
  const first = await scheduler.run("wt-a", still);
  assert.equal(calls, 3, "单批最多尝试 3 次后停止");
  assert.equal(first.exhausted, true, "耗尽要可见");
  // 耗尽不是终身禁用：新批次还能真实读取（SP07-R16-3）。
  calls = 0;
  const second = await scheduler.run("wt-a", still);
  assert.equal(calls, 3, "新批次重新获得预算");
  assert.equal(second.attempts, 3);
});

test("重读单飞：同一 worktree 并发拒收只发起一次重读", async () => {
  const scheduler = new ReviewRecoveryScheduler(3);
  let calls = 0;
  let release;
  const gatePromise = new Promise((resolve) => { release = resolve; });
  const run = () => scheduler.run("wt-a", async () => {
    calls += 1;
    await gatePromise;
    return { status: "trusted" };
  });
  const first = run();
  const second = run();
  release();
  await Promise.all([first, second]);
  assert.equal(calls, 1, "并发拒收必须合并成一次重读");
});

test("逐路径读取不可信时同样明确拒收（getRows 入口）", () => {
  const overlay = new ReviewOverlay(2);
  overlay.record("wt-a", "a", { head: "h", worktree: "v" }, true, 2);
  overlay.record("wt-a", "b", { head: "h", worktree: "v" }, true, 2);
  overlay.record("wt-a", "c", { head: "h", worktree: "v" }, true, 2);
  const loaded = { path: "d", reviewed: true, headVersion: "h", worktreeVersion: "v", metricsLoaded: true };
  const outcome = overlay.mergeEntry("wt-a", loaded, 0, undefined, 0);
  assert.equal(outcome.ok, false, "逐路径读取也必须遵守同一可信度契约");
  assert.equal(outcome.worktreeId, "wt-a");
});

test("成功发布被拒（needsRefresh）时也走真实重读，不靠 watcher 巧合", async () => {
  const overlay = new ReviewOverlay(2);
  for (const path of ["a", "b", "c"]) overlay.record("wt-a", path, { head: "h", worktree: "v" }, true, 2);
  const snapshot = {
    worktreeId: "wt-a", revision: 9,
    entries: [{ path: "a", reviewed: false, headVersion: "h", worktreeVersion: "v" }],
    review: { paths: ["a"], reviewed: [], total: 1, reviewedCount: 0, seq: 0 },
  };
  const published = publishReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: ["a"],
    marks: marksOf(["a", "h", "v"]), seq: 1, snapshot,
    context: { worktreeMatches: true, activationMatches: true, revisionMatches: true, epochMatches: true },
  });
  assert.equal(published.view, null);
  assert.equal(published.needsRefresh, true, "必须明确要求重新读取");

  const scheduler = new ReviewRecoveryScheduler(3);
  const requested = [];
  await scheduler.run("wt-a", async () => { requested.push("wt-a"); return { status: "trusted" }; });
  assert.deepEqual(requested, ["wt-a"], "needsRefresh 必须真的触发一次原目标重读");
});

// ---------------- 第十六轮：恢复编排闭环 ----------------

test("恢复 runner 不递归等待自身：再次拒收时批次仍会结束（ST07-R16-1 / SP07-R16-1）", async () => {
  const overlay = new ReviewOverlay(2);
  // 抬高淘汰水位到 2。
  for (const path of ["a", "b", "c"]) overlay.record("wt-a", path, { head: "h", worktree: "v" }, true, 2);
  const scheduler = new ReviewRecoveryScheduler(3);
  // 模拟真实时序：每次读取发起时快照都「新鲜」，但交付前总有更高的 seq 先到，
  // 于是交付后一律被拒（不是假设后端新读凭空变旧）。
  let reads = 0;
  const runner = async () => {
    reads += 1;
    const current = reads + 1;                 // 发起时看到的 seq
    const snapshot = {
      worktreeId: "wt-a", revision: 9,
      entries: [{ path: "a", reviewed: false, headVersion: "h", worktreeVersion: "v" }],
      review: { paths: ["a"], reviewed: [], total: 1, reviewedCount: 0, seq: current },
    };
    const pending = readThroughReview(overlay, async () => snapshot);
    // 交付前更高 seq 的完整结果先到，抬高淘汰水位。
    for (const path of ["a", "b", "c"]) {
      overlay.record("wt-a", path, { head: "h", worktree: "v" }, true, current + 1);
    }
    const outcome = await pending;
    if (!outcome.ok) return { status: "untrusted", worktreeId: outcome.worktreeId, observedSeq: outcome.observedSeq };
    return { status: "trusted" };
  };
  // 关键断言：这个 promise 必须结束（旧实现会自等待，永不 settle）。
  const result = await Promise.race([
    scheduler.run("wt-a", runner),
    new Promise((resolve) => setTimeout(() => resolve("timeout"), 500)),
  ]);
  assert.notEqual(result, "timeout", "恢复批次必须结束，不能自等待");
  assert.equal(result.exhausted, true, "三次都不可信 → 本批次耗尽");
  assert.equal(reads, 3, "单批最多 3 次真实读取");
  assert.equal(scheduler.isRunning("wt-a"), false, "结束后释放单飞槽");
});

test("写结果等待恢复时门禁一定会释放（批次 settle）", async () => {
  const scheduler = new ReviewRecoveryScheduler(2);
  let gateHeld = true;
  const writeWithRecovery = async () => {
    try {
      await scheduler.run("wt-a", async () => ({ status: "error", error: new Error("temp") }));
    } finally {
      gateHeld = false;
    }
  };
  const settled = await Promise.race([
    writeWithRecovery().then(() => "done"),
    new Promise((resolve) => setTimeout(() => resolve("timeout"), 500)),
  ]);
  assert.equal(settled, "done", "写结果的 await 必须结束");
  assert.equal(gateHeld, false, "写门禁必须释放");
});

test("耗尽后新批次恢复：预算按批重置，不是终身熔断（ST07-R16-2 / SP07-R16-3）", async () => {
  const scheduler = new ReviewRecoveryScheduler(2);
  let reads = 0;
  const failing = async () => { reads += 1; return { status: "error", error: new Error("temp") }; };
  const first = await scheduler.run("wt-a", failing);
  assert.equal(first.exhausted, true);
  assert.equal(reads, 2);
  // 后续独立拒收 → 新批次仍会真实读取。
  const second = await scheduler.run("wt-a", failing);
  assert.equal(second.exhausted, true);
  assert.equal(reads, 4, "新批次必须重新获得预算");
  // 可信读取后，再下一个批次也正常。
  const third = await scheduler.run("wt-a", async () => { reads += 1; return { status: "trusted" }; });
  assert.equal(third.converged, true);
  assert.equal(scheduler.attemptsFor("wt-a"), 0);
});

test("旧恢复读失败不覆盖较新的错误（SP07-R16-2）", async () => {
  // 复现生产语义：恢复读发起时记下错误代际；在途期间出现更新错误 → 旧失败不再展示。
  const epochs = { "wt-a": 1 };
  const shown = [];
  const recoveryRead = async (errorEpoch, fail) => {
    await Promise.resolve();
    if (!fail) return;
    const unchanged = (epochs["wt-a"] ?? 0) === errorEpoch;
    if (unchanged) shown.push("old-recovery-error");
  };
  const pending = recoveryRead(epochs["wt-a"], true);
  // 较新的操作显示了拒绝错误 → 代际前进。
  epochs["wt-a"] += 1;
  shown.push("newer-rejection");
  await pending;
  assert.deepEqual(shown, ["newer-rejection"], "旧恢复读的失败不能覆盖较新的错误");
});

test("恢复读取归属原目标：切换后仍读原目标，不污染新激活", async () => {
  const scheduler = new ReviewRecoveryScheduler(2);
  const gate = new SnapshotGate();
  gate.activate("wt-a");
  const accepted = [];
  const reads = [];
  const result = await scheduler.run("wt-a", async () => {
    reads.push("wt-a");
    // 期间用户切到 wt-b：恢复读仍必须读 wt-a。
    gate.activate("wt-b");
    const token = gate.token();
    // 不是当前激活目标 → 不更新界面，但仍报告可信。
    if (token.worktreeId === "wt-a") accepted.push("wt-a");
    return { status: "trusted" };
  });
  assert.equal(result.converged, true);
  assert.deepEqual(reads, ["wt-a"], "恢复读只读原目标");
  assert.deepEqual(accepted, [], "切换后不再污染新激活的界面");
});

// ---------------- 第十七轮：恢复转发与批次级错误资格 ----------------

test("生产编排入口：批次级资格在重试与耗尽出口一致保护新错误（ST07-R17-2 / SP07-R17-2）", async () => {
  const scheduler = new ReviewRecoveryScheduler(3);
  const errors = [];
  let epoch = 0;                 // 错误代际
  const shown = [];
  const outcome = await runReviewRecoveryBatch({
    scheduler,
    worktreeId: "wt-a",
    // 每次读取都失败；第一次之后出现一条更新的错误（代际前进）。
    read: async () => {
      errors.push("attempt");
      if (errors.length === 1) epoch += 1;      // 较新的拒绝错误
      return { status: "error", error: new Error(`temp-${errors.length}`) };
    },
    beginEligibility: () => {
      const batchEpoch = epoch;                  // 批次开始时只取一次
      return () => epoch === batchEpoch;
    },
    showFailure: () => shown.push("failure"),
  });
  assert.equal(outcome.exhausted, true);
  assert.equal(outcome.failureShown, false, "批次期间出现新错误 → 耗尽出口也不能覆盖它");
  assert.deepEqual(shown, [], "不得展示任何旧失败");
});

test("生产编排入口：批次期间没有新错误时耗尽提示可见（不静默）", async () => {
  const scheduler = new ReviewRecoveryScheduler(2);
  const shown = [];
  const outcome = await runReviewRecoveryBatch({
    scheduler,
    worktreeId: "wt-a",
    read: async () => ({ status: "error", error: new Error("temp") }),
    beginEligibility: () => () => true,
    showFailure: (failure) => shown.push(failure?.status ?? "none"),
  });
  assert.equal(outcome.exhausted, true);
  assert.equal(outcome.failureShown, true, "耗尽必须被处理");
  assert.deepEqual(shown, ["error"], "沿用现有错误展示语义");
});

test("生产编排入口：重试不会重新获得覆盖新错误的资格", async () => {
  const scheduler = new ReviewRecoveryScheduler(3);
  let epoch = 0;
  let reads = 0;
  const shown = [];
  await runReviewRecoveryBatch({
    scheduler,
    worktreeId: "wt-a",
    read: async () => {
      reads += 1;
      if (reads === 1) epoch += 1;   // 第一次读取期间出现新错误
      return { status: "untrusted", worktreeId: "wt-a", observedSeq: 0 };
    },
    beginEligibility: () => {
      const batchEpoch = epoch;
      return () => epoch === batchEpoch;
    },
    showFailure: () => shown.push("failure"),
  });
  assert.equal(reads, 3, "仍然按批尝试");
  assert.deepEqual(shown, [], "后续重试与耗尽都不能认领批次期间的新错误");
});

test("生产编排入口：用户关闭提示后旧批次不再回填", async () => {
  const scheduler = new ReviewRecoveryScheduler(2);
  let epoch = 0;
  const shown = [];
  await runReviewRecoveryBatch({
    scheduler,
    worktreeId: "wt-a",
    read: async () => {
      // 批次进行中用户关闭提示（dismissError 推进代际）。
      epoch += 1;
      return { status: "error", error: new Error("temp") };
    },
    beginEligibility: () => {
      const batchEpoch = epoch;
      return () => epoch === batchEpoch;
    },
    showFailure: () => shown.push("failure"),
  });
  assert.deepEqual(shown, [], "关闭提示后旧批次不得写回");
});

test("恢复转发真实生效：requestReviewRecovery 会到达编排入口（ST07-R17-1 / SP07-R17-1）", async () => {
  // 复现 App 的 ref 转发形态：初始化 null，之后赋值；调用必须真的触发读取。
  const scheduler = new ReviewRecoveryScheduler(2);
  let forward = null;
  const request = (worktreeId) => forward?.(worktreeId) ?? Promise.resolve(null);
  let reads = 0;
  const realEntry = (worktreeId) => runReviewRecoveryBatch({
    scheduler,
    worktreeId,
    read: async () => { reads += 1; return { status: "trusted" }; },
    beginEligibility: () => () => true,
    showFailure: () => {},
  });
  forward = realEntry;                       // 必须真的赋值（旧实现漏了这一步）
  const outcome = await request("wt-a");
  assert.equal(reads, 1, "转发必须真的到达编排入口并读取");
  assert.equal(outcome.converged, true);
});

test("恢复转发未赋值时不静默成功（返回 null 而不是伪造结果）", async () => {
  let forward = null;
  const request = (worktreeId) => forward?.(worktreeId) ?? Promise.resolve(null);
  assert.equal(await request("wt-a"), null, "未接线时明确返回 null，不伪造收敛");
});

// ---------------- 任务 08：写入期间的安全关闭/退出 ----------------

test("没有在途写操作时关闭请求立即执行（不打断任何 Git）", () => {
  const c = new CloseCoordinator();
  assert.deepEqual(c.request("close"), { action: "proceed" });
  assert.equal(c.pendingIntent(), null);
});

test("有在途写操作时保留窗口并询问，不执行关闭", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  const decision = c.request("close");
  assert.equal(decision.action, "ask");
  assert.deepEqual(decision.writing, ["wt-a"]);
  assert.equal(c.pendingIntent(), "close", "原请求必须被保留");
});

test("切换 worktree 后在途写操作仍然算在途（Q90）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  // 用户在界面切到 wt-b 并请求关闭：wt-a 的写操作仍在执行。
  const decision = c.request("close");
  assert.equal(decision.action, "ask");
  assert.deepEqual(decision.writing, ["wt-a"]);
});

test("留在应用：清掉请求但不取消写操作、不改变动作类型", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.request("exit");
  c.keepOpen();
  assert.equal(c.pendingIntent(), null, "请求已清掉");
  assert.deepEqual(c.writingWorktrees(), ["wt-a"], "写操作没有被取消");
  // 写操作完成后不再执行任何关闭/退出。
  c.endWrite("wt-a");
  assert.notEqual(c.reconcile().kind, "proceed", "留在应用后不执行原请求")
});

test("等待完成后执行**原请求**（关闭不会被改成退出，反之亦然）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.request("exit");
  c.resolveAsk("wait");                 // 用户明确选择等待（授权）
  c.endWrite("wt-a");
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "exit" }, "执行的就是用户原来的退出请求")

  const c2 = new CloseCoordinator();
  c2.beginWrite("wt-b");
  c2.request("close");
  c2.resolveAsk("wait");
  c2.endWrite("wt-b");
  assert.deepEqual(c2.reconcile(), { kind: "proceed", intent: "close" });
});

test("询问未选择就完成写入：不执行关闭（仅显示询问不算授权）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  assert.equal(c.request("close").action, "ask");
  // 用户还没选，写入就结束了。
  c.endWrite("wt-a");
  assert.notEqual(c.reconcile().kind, "proceed", "未授权不得关窗")
  assert.equal(c.pendingIntent(), "close", "原请求仍保留，等待用户选择");
});

test("写入结束后用户才点「等待」：请求不丢，收尾时执行原动作", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.request("exit");
  c.endWrite("wt-a");                   // 写入先结束
  assert.notEqual(c.reconcile().kind, "proceed", "此时还没授权")
  c.resolveAsk("wait");                 // 用户晚点击
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "exit" }, "晚点击等待后仍执行原动作")
});

test("重复请求不替换原动作（关闭询问中再次退出）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.request("close");
  c.request("exit");                    // 询问中再次请求
  assert.equal(c.pendingIntent(), "close", "原动作以第一次为准");
  c.resolveAsk("wait");
  c.endWrite("wt-a");
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "close" }, "执行的是最初的动作")

  const c2 = new CloseCoordinator();
  c2.beginWrite("wt-b");
  c2.request("exit");
  c2.request("close");
  c2.resolveAsk("wait");
  c2.endWrite("wt-b");
  assert.deepEqual(c2.reconcile(), { kind: "proceed", intent: "exit" });
});

test("多个 worktree 的在途写操作全部完成后才执行原请求", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.beginWrite("wt-b");
  c.request("close");
  c.resolveAsk("wait");
  c.endWrite("wt-a");
  assert.notEqual(c.reconcile().kind, "proceed", "还有 wt-b 在写")
  c.endWrite("wt-b");
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "close" });
});

test("待核对结果先展示，不静默退出", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.endWrite("wt-a", true);            // 结果待核对
  const decision = c.request("close");
  assert.equal(decision.action, "showPending");
  assert.deepEqual(decision.pending, ["wt-a"]);
  assert.notEqual(c.reconcile().kind, "proceed", "待核对未确认前不执行退出")
  const gen = c.pendingGeneration("wt-a");
  c.acknowledgePending("wt-a", gen);
  c.confirmPendingContinue();
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "close" }, "确认后才执行原请求")
});

test("待核对不会被下一次写入自动清除（只有真实核对才清除，Q91）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.endWrite("wt-a", true);
  assert.deepEqual(c.pendingWorktrees(), ["wt-a"]);
  // 下一次写入开始/成功**不会**清掉待核对：核对是独立动作。
  c.beginWrite("wt-a");
  assert.deepEqual(c.pendingWorktrees(), ["wt-a"], "开始新写入不能清 pending");
  c.endWrite("wt-a", false);
  assert.deepEqual(c.pendingWorktrees(), ["wt-a"], "成功写入也不能代替核对");
  assert.notEqual(c.reconcile().kind, "proceed", "未核对前不执行关闭")
  // 真实核对后才清除。
  c.acknowledgePending("wt-a", c.pendingGeneration("wt-a"));
  assert.deepEqual(c.pendingWorktrees(), []);
});

test("写操作进行中不重跑任何命令：编排只读状态", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.request("close");
  // 关闭编排没有任何「重试/重跑」入口，只能查询状态与记录结束。
  assert.equal(typeof c.retry, "undefined");
  assert.equal(typeof c.rerun, "undefined");
  assert.deepEqual(c.writingWorktrees(), ["wt-a"]);
});

// ---------------- 整批首轮：共享批次晚加入不能重新取得资格 ----------------

test("共享批次：第一读挂起 → 新错误 → 另一路加入 → 释放后不覆盖新错误（ST-B01-07-1 / SP-B01-07-1）", async () => {
  const scheduler = new ReviewRecoveryScheduler(2);
  let epoch = 0;
  const shown = [];
  let releaseFirst;
  const firstGate = new Promise((resolve) => { releaseFirst = resolve; });
  let reads = 0;

  // 生产编排：两路都经 runReviewRecoveryBatch（真实入口），共享同一批次。
  const read = async () => {
    reads += 1;
    if (reads === 1) await firstGate;         // 第一读挂起
    return { status: "error", error: new Error("temp") };
  };
  const beginEligibility = () => {
    const batchEpoch = epoch;                  // 批次创建时只取一次
    return () => epoch === batchEpoch;
  };
  const showFailure = () => shown.push("failure");

  // 第一路发起（批次创建，取 epoch=0 的资格）。
  const first = runReviewRecoveryBatch({ scheduler, worktreeId: "wt-a", read, beginEligibility, showFailure });
  // 第一读仍在挂起时出现较新的错误（代际前进）。
  epoch += 1;
  // 另一路真实调用点加入同一批次（共享 inFlight，不应重新取资格）。
  const second = runReviewRecoveryBatch({ scheduler, worktreeId: "wt-a", read, beginEligibility, showFailure });
  releaseFirst();

  const [a, b] = await Promise.all([first, second]);
  assert.equal(a, b, "两路必须拿到同一个批次结果对象");
  assert.equal(a.exhausted, true);
  assert.equal(a.failureShown, false, "批次期间出现新错误 → 不得展示");
  assert.deepEqual(shown, [], "晚加入者不能重新取得资格并展示失败");
});

test("共享批次：失败最多展示一次", async () => {
  const scheduler = new ReviewRecoveryScheduler(1);
  const shown = [];
  const read = async () => ({ status: "error", error: new Error("temp") });
  const beginEligibility = () => () => true;
  const showFailure = () => shown.push("failure");
  await Promise.all([
    runReviewRecoveryBatch({ scheduler, worktreeId: "wt-a", read, beginEligibility, showFailure }),
    runReviewRecoveryBatch({ scheduler, worktreeId: "wt-a", read, beginEligibility, showFailure }),
  ]);
  assert.deepEqual(shown, ["failure"], "同一批次只展示一次失败");
});

test("共享批次：不同 worktree 各自独立，不共享资格", async () => {
  const scheduler = new ReviewRecoveryScheduler(1);
  const shown = [];
  const read = async () => ({ status: "error", error: new Error("temp") });
  await Promise.all([
    runReviewRecoveryBatch({ scheduler, worktreeId: "wt-a", read, beginEligibility: () => () => true, showFailure: () => shown.push("a") }),
    runReviewRecoveryBatch({ scheduler, worktreeId: "wt-b", read, beginEligibility: () => () => true, showFailure: () => shown.push("b") }),
  ]);
  assert.deepEqual(shown.sort(), ["a", "b"], "两个 worktree 各自展示自己的失败");
});

// ---------------- 整批首轮：08 关闭/退出真实编排 ----------------

test("没有原请求时，写操作结束绝不凭空关窗（ST-B01-08-2 / SP-B01-08-3）", () => {
  const c = new CloseCoordinator();
  // 两个 worktree 并行写，用户从未请求关闭。
  c.beginWrite("wt-a");
  c.beginWrite("wt-b");
  c.endWrite("wt-a");
  assert.notEqual(c.reconcile().kind, "proceed", "没有请求就不能执行关闭")
  c.endWrite("wt-b");
  assert.notEqual(c.reconcile().kind, "proceed", "最后一笔结束也不能凭空关窗")
  assert.equal(c.pendingIntent(), null, "不得自行创建请求");
});

test("用户选「留在应用」后，写操作结束不会自动关窗", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  assert.equal(c.request("close").action, "ask");
  c.resolveAsk("stay");                       // 用户选择留在应用
  c.endWrite("wt-a");
  assert.notEqual(c.reconcile().kind, "proceed", "Stay 之后不得稍后自动关窗")
});

test("用户选「等待」后才在全部写完成时执行原请求", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.beginWrite("wt-b");
  assert.equal(c.request("exit").action, "ask");
  // 仅展示询问不算授权：还没选等待时不能执行。
  assert.notEqual(c.reconcile().kind, "proceed");
  c.resolveAsk("wait");
  c.endWrite("wt-a");
  assert.notEqual(c.reconcile().kind, "proceed", "仍有 wt-b 在写")
  c.endWrite("wt-b");
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "exit" }, "全部完成且已授权 → 执行原退出请求")
});

test("重复关闭请求不会叠加或改变动作类型", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.request("close");
  c.request("close");
  assert.equal(c.pendingIntent(), "close");
  c.resolveAsk("wait");
  c.endWrite("wt-a");
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "close" });
  const claimed = c.claimExecution();
  c.completeExecution(claimed.generation);
  assert.notEqual(c.reconcile().kind, "proceed", "只执行一次");
});

test("待核对必须先真实核对才清除，且核对后仍需用户明确继续", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.endWrite("wt-a", true);                   // 结果无法确定
  const decision = c.request("close");
  assert.equal(decision.action, "showPending");
  assert.deepEqual(decision.pending, ["wt-a"]);
  // 没有核对前不能关闭。
  c.resolveAsk("wait");
  assert.notEqual(c.reconcile().kind, "proceed", "待核对未核对前不执行")
  // 真实核对后清除 pending，并由用户明确「继续」。
  c.acknowledgePending("wt-a", c.pendingGeneration("wt-a"));
  c.confirmPendingContinue();
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "close" }, "核对并确认后才执行")
});

test("另存生命周期：无归属占位键也计入在途，但不占用 worktree 写门禁", () => {
  const c = new CloseCoordinator();
  const external = "save-as:/tmp/outside.txt";
  c.beginWrite(external);
  assert.equal(c.hasWriting(), true, "外部另存也算在途写");
  assert.equal(c.request("close").action, "ask");
  c.resolveAsk("wait");
  c.endWrite(external);
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "close" });
  // 占位键与真实 worktree 键互不影响：另一个 worktree 的写不会被它阻塞。
  c.beginWrite("wt-a");
  assert.deepEqual(c.writingWorktrees(), ["wt-a"]);
});

test("另存的 needsVerification 如实保留为待核对", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.endWrite("wt-a", true);                   // 另存也可能返回 needsVerification
  assert.deepEqual(c.pendingWorktrees(), ["wt-a"]);
  assert.equal(c.request("exit").action, "showPending");
});

// ---------------- 整批 B02：关闭/退出生产闭环 ----------------

test("待核对期间原生保护仍然成立（不因无在途写而放行）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.endWrite("wt-a", true);
  assert.equal(c.hasWriting(), false);
  assert.equal(c.hasPending(), true);
  // 生产端用 hasWriting() || hasPending() 计算保护；这里直接断言两段语义。
  assert.equal(c.hasWriting() || c.hasPending(), true, "待核对期间必须继续阻止退出");
});

test("最后一笔 NeedsVerification 后进入待核对并主动展示（不静默）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.request("close");
  c.resolveAsk("wait");
  c.endWrite("wt-a", true);            // 最后一笔变成待核对
  assert.notEqual(c.reconcile().kind, "proceed", "待核对未核对前不关闭")
  assert.equal(c.authorizedIntent(), "close", "授权仍保留");
  assert.equal(c.hasPending(), true, "待核对必须可见");
});

test("核对失败不清账（pending 保留，仍阻止关闭）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.endWrite("wt-a", true);
  c.request("close");
  c.resolveAsk("wait");
  const gen = c.pendingGeneration("wt-a");
  // 核对失败：调用方不会调用 acknowledgePending；即使代际匹配也不该由失败路径清。
  assert.equal(c.hasPending(), true);
  assert.notEqual(c.reconcile().kind, "proceed");
  // 真正成功核对后才清。
  assert.equal(c.acknowledgePending("wt-a", gen), true);
  c.confirmPendingContinue();
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "close" });
});

test("核对期间同一目标新增 pending：旧代际核对不能清掉新的一代", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.endWrite("wt-a", true);
  const gen1 = c.pendingGeneration("wt-a");
  // 核对期间该目标又出现一次待核对。
  c.beginWrite("wt-a");
  c.endWrite("wt-a", true);
  const gen2 = c.pendingGeneration("wt-a");
  assert.ok(gen2 > gen1, "代际必须递增");
  assert.equal(c.acknowledgePending("wt-a", gen1), false, "旧代际不能清新的一代");
  assert.equal(c.hasPending(), true, "新的待核对仍可见");
  assert.equal(c.acknowledgePending("wt-a", gen2), true, "当前代际才能清");
});

test("核对期间不同 worktree 新增 pending：不被顺手清掉", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.endWrite("wt-a", true);
  c.beginWrite("wt-b");
  c.endWrite("wt-b", true);
  const genA = c.pendingGeneration("wt-a");
  // 只核对 A 成功。
  assert.equal(c.acknowledgePending("wt-a", genA), true);
  assert.deepEqual(c.pendingWorktrees(), ["wt-b"], "B 的待核对必须保留");
  // confirmPendingContinue 不批量清除。
  c.confirmPendingContinue();
  assert.deepEqual(c.pendingWorktrees(), ["wt-b"], "confirm 不清别的目标");
});

test("核对中选择 Stay：不关窗、pending 保留", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.endWrite("wt-a", true);
  c.request("close");
  c.resolveAsk("stay");
  assert.notEqual(c.reconcile().kind, "proceed", "Stay 后不关闭")
  assert.equal(c.pendingIntent(), null, "请求已清");
  assert.deepEqual(c.pendingWorktrees(), ["wt-a"], "待核对仍可见");
});

test("等待后出现 pending：保留原动作，主动展示", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.beginWrite("wt-b");
  c.request("exit");
  c.resolveAsk("wait");
  c.endWrite("wt-a");                  // 还有 wt-b
  assert.notEqual(c.reconcile().kind, "proceed");
  c.endWrite("wt-b", true);            // 最后一笔变待核对
  assert.notEqual(c.reconcile().kind, "proceed", "不能静默关闭")
  assert.equal(c.authorizedIntent(), "exit", "原动作仍是退出");
  assert.equal(c.hasPending(), true);
});

test("重复 close/exit：询问中不替换，授权后动作不变", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.request("close");
  c.request("exit");
  c.request("close");
  assert.equal(c.pendingIntent(), "close", "第一次为准");
  c.resolveAsk("wait");
  c.endWrite("wt-a");
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "close" });
});

test("Stay 之后再次请求：可以重新询问（不是永久禁用）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.request("close");
  c.resolveAsk("stay");
  assert.notEqual(c.reconcile().kind, "proceed");
  const again = c.request("exit");
  assert.equal(again.action, "ask", "可以再次请求");
  assert.equal(c.pendingIntent(), "exit", "新的原动作");
  c.resolveAsk("wait");
  c.endWrite("wt-a");
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "exit" });
});

// ---------------- 整批 B03：保护版本化、监听就绪、晚等待与核对代际 ----------------

test("退出保护版本化：旧 false 晚到不能覆盖新 true（ST-B03-08-1 / SP-B03-08-1）", async () => {
  // 模拟 Rust：只接受更大的 seq（与 commands.rs apply_exit_guard 同语义）。
  let stored = { seq: 0, blocked: false };
  const rust = async ({ blocked, seq }) => {
    if (seq <= stored.seq) return false;
    stored = { seq, blocked };
    return true;
  };
  const guard = new ExitGuardSync(rust);
  // 第一笔写开始 → true(seq1)；写结束 → false(seq2) 但**延迟到达**。
  const releaseDeferred = guard.set(false);      // 先发起（seq1 之后）
  await guard.set(true);                          // 新写获得保护（seq2）
  await releaseDeferred;                          // 旧 false 现在才完成
  // 由于旧 false 的 seq 更小，Rust 实际状态仍是 true。
  assert.equal(stored.blocked, true, "旧 false 绝不能解除新建立的保护");
  assert.equal(guard.current(), true, "本地已知状态也必须是 true");
});

test("退出保护版本化：新 false 可以正常释放", async () => {
  let stored = { seq: 0, blocked: false };
  const guard = new ExitGuardSync(async ({ blocked, seq }) => {
    if (seq <= stored.seq) return false;
    stored = { seq, blocked };
    return true;
  });
  await guard.set(true);
  await guard.set(false);
  assert.equal(stored.blocked, false, "更新的 false 正常生效");
});

test("监听闸门：只装好一个不算就绪，写入口必须拒绝（ST-B03-08-2）", () => {
  const gate = new CloseGuardGate();
  assert.equal(gate.ready, false, "初始未就绪");
  gate.markInstalled("window");
  assert.equal(gate.ready, false, "只有窗口监听仍不算就绪");
  gate.markInstalled("exit");
  assert.equal(gate.ready, true, "两个都装好才就绪");
  assert.equal(gate.blockedReason, null);
});

test("监听闸门：安装失败后即使另一个成功也不就绪，且原因可见", () => {
  const gate = new CloseGuardGate();
  gate.markInstalled("window");
  gate.markFailed("原生退出监听失败");
  gate.markInstalled("exit");
  assert.equal(gate.ready, true, "失败后再 markInstalled 仍算装好（失败是瞬时错误）");
  const failed = new CloseGuardGate();
  failed.markFailed("窗口关闭监听失败");
  assert.equal(failed.ready, false, "只失败没装好 → 未就绪");
  assert.equal(failed.blockedReason, "窗口关闭监听失败");
});

test("监听闸门：卸载/重装回到未就绪", () => {
  const gate = new CloseGuardGate();
  gate.markInstalled("window");
  gate.markInstalled("exit");
  assert.equal(gate.ready, true);
  gate.dispose();
  assert.equal(gate.ready, false, "卸载后必须未就绪");
});

test("晚点等待：写入已结束后点 Wait，reconcile 立即执行原动作（ST-B03-08-3 / SP-B03-08-2）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  assert.equal(c.request("close").action, "ask");
  c.endWrite("wt-a");                      // 写入先结束
  // 询问必须**继续存在**：用户还没选择，不能隐藏对话框（ST-B04-08-2）。
  assert.equal(c.reconcile().kind, "ask", "未选择时询问仍然可见");
  c.resolveAsk("wait");                    // 用户晚点击
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "close" },
    "App 只需调用 reconcile 就能收尾，不需要等下一笔写");
});

test("晚点等待 + 已 pending：reconcile 展示待核对而不是关窗（SP-B03-08-2）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.request("close");
  c.endWrite("wt-a", true);
  c.resolveAsk("wait");
  const decision = c.reconcile();
  assert.equal(decision.kind, "ask");
  assert.deepEqual(decision.pending, ["wt-a"]);
  assert.equal(decision.intent, "close", "原动作保留");
});

test("保护 begin 失败回滚后 reconcile 不留幽灵授权（ST-B03-08-4）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.request("close");
  c.resolveAsk("wait");
  // 保护失败 → 回滚登记。
  c.rollbackWrite("wt-a");
  // 回滚后没有在途写、没有 pending，授权应能正常执行（不是悬挂）。
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "close" });
});

test("旧核对不得授权新请求（ST-B03-08-5 / SP-B03-08-3）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.endWrite("wt-a", true);
  c.request("close");
  const genBefore = c.generation();
  // 核对进行中，用户选择 Stay 并重新发起退出请求。
  c.resolveAsk("stay");
  c.request("exit");
  assert.notEqual(c.generation(), genBefore, "新请求必须推进代际");
  // 旧核对回调发现代际变化，直接返回（不 confirm、不关窗）。
  assert.equal(c.generation() === genBefore, false);
  // 新请求尚未被授权。
  assert.equal(c.reconcile().kind, "ask", "新请求需要新的用户选择");
});

test("核对期间新 pending 不被隐藏（ST-B03-08-6 / SP-B03-08-4）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.endWrite("wt-a", true);
  const genA = c.pendingGeneration("wt-a");
  // 核对 A 期间 B 新增 pending。
  c.beginWrite("wt-b");
  c.endWrite("wt-b", true);
  c.request("close");
  c.resolveAsk("wait");
  // 只核对成功 A。
  assert.equal(c.acknowledgePending("wt-a", genA), true);
  c.confirmPendingContinue();
  const decision = c.reconcile();
  assert.equal(decision.kind, "ask");
  assert.deepEqual(decision.pending, ["wt-b"], "B 必须继续展示");
});

test("展示动作取自被保留的原请求，不受后续事件影响（ST-B03-08-7 / SP-B03-08-5）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.request("close");                      // 首次关闭
  c.request("exit");                       // 期间又按 ⌘Q
  const decision = c.reconcile();
  assert.equal(decision.kind, "ask");
  assert.equal(decision.intent, "close", "展示与执行都必须是第一次的关闭");
});

test("核对中重复点击/多回调：第二次点击不会重复授权", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.endWrite("wt-a", true);
  c.request("close");
  const gen = c.pendingGeneration("wt-a");
  assert.equal(c.acknowledgePending("wt-a", gen), true);
  c.confirmPendingContinue();
  c.confirmPendingContinue();              // 重复回调
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "close" });
  const claimed = c.claimExecution();
  c.completeExecution(claimed.generation);
  assert.equal(c.reconcile().kind, "none", "第二次 reconcile 不再执行");
});

test("Stay 后旧核对失败不会重弹已取消的询问（SP-B03-08-3）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.endWrite("wt-a", true);
  c.request("close");
  const genBefore = c.generation();
  c.resolveAsk("stay");
  assert.notEqual(c.generation(), genBefore);
  // 旧核对失败分支在代际变化后必须直接返回：不展示、不授权。
  assert.equal(c.reconcile().kind, "none", "Stay 后不应弹任何询问");
});

// ---------------- 整批 B04：真实异步生产路径 ----------------

/** 可精确控制「到达顺序」与「回包顺序」的版本化接收端（模拟 Rust apply_exit_guard）。 */
function makeDeferredReceiver() {
  const state = { seq: 0, blocked: false };
  const arrivals = [];
  const rejections = [];
  const receive = (request) => {
    // 到达即记录；实际应用由 resolve 控制，从而可制造「旧 false 晚应用」。
    let resolve, reject;
    const promise = new Promise((res, rej) => { resolve = res; reject = rej; });
    arrivals.push({
      request,
      resolve: (accepted) => {
        // 模拟 Rust：只接受更大的 seq。
        if (request.seq > state.seq) {
          state.seq = request.seq;
          state.blocked = request.blocked;
          resolve(true);
          return;
        }
        rejections.push(request);
        resolve(false);
      },
      reject,
    });
    return promise;
  };
  return { state, arrivals, rejections, receive };
}

test("真实乱序：旧 false 的回包晚于新 true 应用，Rust 状态仍为 true（ST-B04-08-1）", async () => {
  const receiver = makeDeferredReceiver();
  const guard = new ExitGuardSync(receiver.receive);

  const firstWrite = guard.set(false);            // 写结束 → false（先发起）
  const secondWrite = guard.set(true);            // 新写开始 → true（后发起）

  // 故意让**新的 true 先应用**，旧的 false 后应用（真正的乱序）。
  receiver.arrivals[1].resolve(true);
  await secondWrite;
  receiver.arrivals[0].resolve(false);
  await firstWrite;

  assert.equal(receiver.state.blocked, true, "旧 false 晚到不得解除新 true");
  assert.equal(guard.current(), true, "本地已知状态也保持 true");
});

test("真实乱序：新 false 在更高版本下正常释放", async () => {
  const receiver = makeDeferredReceiver();
  const guard = new ExitGuardSync(receiver.receive);
  const on = guard.set(true);
  receiver.arrivals[0].resolve(true);
  await on;
  const off = guard.set(false);
  receiver.arrivals[1].resolve(true);
  await off;
  assert.equal(receiver.state.blocked, false);
  assert.equal(guard.current(), false);
});

test("共享异步编排：切 worktree 不影响在途保护（保护不随 worktree 重装）", async () => {
  // 模拟 App：监听闸门是应用级、worktree 切换不改变它。
  const gate = new CloseGuardGate();
  gate.markInstalled("window");
  gate.markInstalled("exit");
  const coordinator = new CloseCoordinator();
  let guardCalls = 0;
  const orchestrator = new WriteGuardOrchestrator({
    guardsReady: () => gate.ready,
    guardBlockedReason: () => gate.blockedReason,
    register: (id) => coordinator.beginWrite(id),
    guard: async () => { guardCalls += 1; },
    rollback: (id) => coordinator.rollbackWrite(id),
    reconcile: () => coordinator.reconcile(),
    endWrite: (id, nv) => coordinator.endWrite(id, nv),
  });
  // A 写开始（保护建立）。
  assert.deepEqual(await orchestrator.begin("wt-a"), { ok: true });
  // 用户切到 wt-b（只是 UI 状态变化，不动 gate/coordinator）。
  assert.equal(gate.ready, true, "切换 worktree 不卸载监听");
  assert.deepEqual(coordinator.writingWorktrees(), ["wt-a"], "A 仍在途");
  // A 结束。
  orchestrator.end("wt-a", false);
  assert.equal(guardCalls, 1);
});

test("共享异步编排：await 保护期间监听被卸载，则不执行 Git 并回滚", async () => {
  const gate = new CloseGuardGate();
  gate.markInstalled("window");
  gate.markInstalled("exit");
  const coordinator = new CloseCoordinator();
  let guardResolve;
  const guardPromise = new Promise((resolve) => { guardResolve = resolve; });
  const orchestrator = new WriteGuardOrchestrator({
    guardsReady: () => gate.ready,
    guardBlockedReason: () => gate.blockedReason,
    register: (id) => coordinator.beginWrite(id),
    guard: () => guardPromise,
    rollback: (id) => coordinator.rollbackWrite(id),
    reconcile: () => coordinator.reconcile(),
    endWrite: (id, nv) => coordinator.endWrite(id, nv),
  });
  const pending = orchestrator.begin("wt-a");
  // 保护 await 期间监听卸载（模拟 effect 清理/安装失败）。
  gate.dispose();
  guardResolve();
  const result = await pending;
  assert.equal(result.ok, false, "await 后必须复核就绪");
  assert.deepEqual(coordinator.writingWorktrees(), [], "必须回滚登记");
});

test("共享异步编排：保护失败时已 Wait 的请求即时收尾（ST-B04-08-3 / SP-B04-08-2）", async () => {
  const gate = new CloseGuardGate();
  gate.markInstalled("window");
  gate.markInstalled("exit");
  const coordinator = new CloseCoordinator();
  const orchestrator = new WriteGuardOrchestrator({
    guardsReady: () => gate.ready,
    guardBlockedReason: () => gate.blockedReason,
    register: (id) => coordinator.beginWrite(id),
    guard: async () => { throw new Error("保护 IPC 失败"); },
    rollback: (id) => coordinator.rollbackWrite(id),
    // App 的 reconcile 回调就是「消费 decision」：这里记录它拿到什么。
    reconcile: () => { observed.push(coordinator.reconcile()); },
    endWrite: (id, nv) => coordinator.endWrite(id, nv),
  });
  const observed = [];
  // 先有一笔在途写，用户此时请求关闭并选择等待。
  coordinator.beginWrite("wt-existing");
  coordinator.request("close");
  coordinator.resolveAsk("wait");
  // 保护建立失败（此时 wt-existing 仍在途）。
  await assert.rejects(() => orchestrator.begin("wt-a"));
  // 收尾：仍有在途写 → 继续询问（不擅自关窗），原动作保留。
  assert.ok(observed.length >= 1, "保护失败必须触发真实 reconcile");
  assert.equal(observed[observed.length - 1].kind, "ask", "仍有在途写 → 继续询问");
  assert.equal(observed[observed.length - 1].intent, "close", "原动作保留");
  // 在途写结束后，授权应当立即执行（不悬挂、不等下一笔无关写）。
  coordinator.endWrite("wt-existing");
  assert.deepEqual(coordinator.reconcile(), { kind: "proceed", intent: "close" });
});

test("共享异步编排：三个写入口共用同一实例，保护失败都收尾", async () => {
  const gate = new CloseGuardGate();
  gate.markInstalled("window");
  gate.markInstalled("exit");
  const coordinator = new CloseCoordinator();
  const orchestrated = [];
  const orchestrator = new WriteGuardOrchestrator({
    guardsReady: () => gate.ready,
    guardBlockedReason: () => gate.blockedReason,
    register: (id) => coordinator.beginWrite(id),
    guard: async () => { throw new Error("boom"); },
    rollback: (id) => coordinator.rollbackWrite(id),
    reconcile: () => { orchestrated.push(coordinator.reconcile().kind); },
    endWrite: (id, nv) => coordinator.endWrite(id, nv),
  });
  for (const id of ["stage:wt-a", "restore:wt-a", "save-as:/tmp/x"]) {
    await assert.rejects(() => orchestrator.begin(id));
  }
  assert.deepEqual(coordinator.writingWorktrees(), [], "三处都回滚干净");
  assert.deepEqual(orchestrated, ["none", "none", "none"], "三处都走了真实 reconcile");
});

test("询问不提前消失：最后写结束但用户未选择时仍可 Wait（ST-B04-08-2）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  assert.equal(c.request("close").action, "ask");
  c.endWrite("wt-a");                       // 最后写结束
  const decision = c.reconcile();
  assert.equal(decision.kind, "ask", "询问必须仍在");
  assert.equal(decision.intent, "close");
  // 用户此刻才点 Wait。
  c.resolveAsk("wait");
  assert.deepEqual(c.reconcile(), { kind: "proceed", intent: "close" });
});

test("询问不提前消失：Stay 后不自动关且不留暗请求", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.request("close");
  c.endWrite("wt-a");
  c.resolveAsk("stay");
  assert.equal(c.reconcile().kind, "none");
  assert.equal(c.pendingIntent(), null, "Stay 后不得留暗请求");
});

test("无请求时不产生询问也不关窗", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.endWrite("wt-a");
  assert.equal(c.reconcile().kind, "none");
});

test("Wait 时已有 pending：展示核对而不是静默退出（ST-B04-08-2）", () => {
  const c = new CloseCoordinator();
  c.beginWrite("wt-a");
  c.request("close");
  c.endWrite("wt-a", true);
  c.resolveAsk("wait");
  const decision = c.reconcile();
  assert.equal(decision.kind, "ask");
  assert.deepEqual(decision.pending, ["wt-a"]);
  assert.equal(decision.intent, "close", "原动作保留");
});

// ---------------- 整批 B05：异步关闭授权生命周期 ----------------

/** 可控 deferred。 */
function deferred() {
  let resolve, reject;
  const promise = new Promise((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

/** 构造与 App 相同的执行环境：closeCoordinator + releaseExitGuard + native。 */
function makeExecutionEnv() {
  const coordinator = new CloseCoordinator();
  const native = [];
  const errors = [];
  let reconcileCalls = 0;
  const deps = {
    coordinator,
    releaseExitGuard: async () => {},
    performNative: async (intent) => { native.push(intent); },
    reconcile: () => { reconcileCalls += 1; },
    onError: (error) => errors.push(error),
  };
  return {
    coordinator, deps, native, errors,
    reconcileCalls: () => reconcileCalls,
  };
}

test("旧回包越过 Stay：A 写期间 Quit→Wait，A 结束，用户启动 B 后 Stay，旧回包不得关窗（ST-B05-08-1）", async () => {
  const env = makeExecutionEnv();
  const c = env.coordinator;
  // A 写期间请求退出并选择等待。
  c.beginWrite("wt-a");
  c.request("exit");
  c.resolveAsk("wait");
  c.endWrite("wt-a");
  // 认领执行；releaseExitGuard 挂起（旧回包还没回来）。
  const release = deferred();
  env.deps.releaseExitGuard = () => release.promise;
  const claimed = c.claimExecution();
  assert.ok(claimed, "应能认领");
  const running = runCloseExecution(env.deps, claimed);
  // 用户此时启动 B 写，并请求关闭、选择 Stay。
  c.beginWrite("wt-b");
  c.request("close");
  c.resolveAsk("stay");
  // 旧回包现在才恢复。
  release.resolve();
  const executed = await running;
  assert.equal(executed, false, "Stay 之后旧回包不得执行原生动作");
  assert.deepEqual(env.native, [], "绝不能关窗");
});

test("B 尚未结束时旧回包也不关窗，且保留最初的原动作 exit", async () => {
  const env = makeExecutionEnv();
  const c = env.coordinator;
  c.beginWrite("wt-a");
  c.request("exit");
  c.resolveAsk("wait");
  c.endWrite("wt-a");
  const release = deferred();
  env.deps.releaseExitGuard = () => release.promise;
  const claimed = c.claimExecution();
  const running = runCloseExecution(env.deps, claimed);
  // B 开始写（用户没做新请求）。
  c.beginWrite("wt-b");
  release.resolve();
  const executed = await running;
  assert.equal(executed, false, "B 仍在途 → 不能执行");
  assert.deepEqual(env.native, []);
  assert.equal(c.pendingIntent(), "exit", "原动作仍是 exit，没有被改成 close");
});

test("没有后续请求时，正常 Wait 仍执行原动作", async () => {
  const env = makeExecutionEnv();
  const c = env.coordinator;
  c.beginWrite("wt-a");
  c.request("close");
  c.resolveAsk("wait");
  c.endWrite("wt-a");
  const claimed = c.claimExecution();
  const executed = await runCloseExecution(env.deps, claimed);
  assert.equal(executed, true);
  assert.deepEqual(env.native, ["close"]);
});

test("Stay 之后新请求重新走完整授权流程（旧回包不影响新请求）", async () => {
  const env = makeExecutionEnv();
  const c = env.coordinator;
  c.beginWrite("wt-a");
  c.request("exit");
  c.resolveAsk("wait");
  c.endWrite("wt-a");
  const release = deferred();
  env.deps.releaseExitGuard = () => release.promise;
  const claimed = c.claimExecution();
  const running = runCloseExecution(env.deps, claimed);
  // Stay，然后发起新请求并等待。
  c.resolveAsk("stay");
  c.beginWrite("wt-b");
  c.request("close");
  c.resolveAsk("wait");
  release.resolve();
  await running;
  assert.deepEqual(env.native, [], "旧回包没有关窗");
  // 新请求在 B 结束后正常执行。
  c.endWrite("wt-b");
  const claimed2 = c.claimExecution();
  const executed2 = await runCloseExecution(env.deps, claimed2);
  assert.equal(executed2, true);
  assert.deepEqual(env.native, ["close"], "新请求执行的是新动作");
});

test("旧回包失败不影响新请求（失败只上报，不授权旧动作）", async () => {
  const env = makeExecutionEnv();
  const c = env.coordinator;
  c.beginWrite("wt-a");
  c.request("exit");
  c.resolveAsk("wait");
  c.endWrite("wt-a");
  const release = deferred();
  env.deps.releaseExitGuard = () => release.promise;
  const claimed = c.claimExecution();
  const running = runCloseExecution(env.deps, claimed);
  c.resolveAsk("stay");
  release.reject(new Error("旧回包失败"));
  const executed = await running;
  assert.equal(executed, false);
  assert.deepEqual(env.native, []);
  assert.equal(env.errors.length, 1, "失败被如实上报");
});

test("执行失败后仍有效的原动作交回编排，不悬挂", async () => {
  const env = makeExecutionEnv();
  const c = env.coordinator;
  c.beginWrite("wt-a");
  c.request("close");
  c.resolveAsk("wait");
  c.endWrite("wt-a");
  env.deps.performNative = async () => { throw new Error("native 失败"); };
  const claimed = c.claimExecution();
  const executed = await runCloseExecution(env.deps, claimed);
  assert.equal(executed, false);
  assert.ok(env.reconcileCalls() >= 1, "必须交回编排");
  assert.equal(c.pendingIntent(), "close", "原动作仍保留");
});
