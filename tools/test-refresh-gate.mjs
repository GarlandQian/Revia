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
const { SnapshotGate, RefreshCoordinator, OperationResultGate, ReviewOverlay, SelectionGate, WatchEventGate, applyReviewSuccess, applyReviewSuccessAndCompose, commitReviewResult, mergePublishedReview, planReviewResult, publishReviewSuccess, readThroughReview, reviewedSetForNavigation } = await import("../src/lib/refreshGate.ts");
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
  const merged = read.deliver(reviewSnapshot());
  assert.equal(reviewedOf(merged, "probe.txt"), true);
  assert.equal(merged.review.reviewedCount, 3);
});

test("读取发起后才成功的取消，不会被这次读取勾回", () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  const read = overlay.beginRead();
  overlay.record("wt-a", "b.txt", bVersions, false, nextSeq());
  const merged = read.deliver(reviewSnapshot());
  assert.equal(reviewedOf(merged, "b.txt"), false);
  assert.equal(merged.review.reviewedCount, 1);
});

test("文件版本真的变化时本地记录不生效，标记仍按后端结果失效", () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  const read = overlay.beginRead();
  overlay.record("wt-a", "probe.txt", { head: null, worktree: "w-old" }, true, nextSeq());
  const merged = read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-new" },
    ],
  }));
  assert.equal(reviewedOf(merged, "probe.txt"), false);
  assert.equal(merged.review.reviewedCount, 2);
});

test("分界之前成功的变更不重复叠加（该读取本来就已经包含它）", () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  overlay.record("wt-a", "probe.txt", probe, true, nextSeq());
  const read = overlay.beginRead();
  const merged = read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: true, headVersion: null, worktreeVersion: "w-p" },
    ],
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt", "b.txt", "probe.txt"], total: 3, reviewedCount: 3 },
  }));
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
  const merged = read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p" },
    ],
  }));
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
  const merged = await pending;
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
  const merged = await pending;
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
  const merged = read.deliver(reviewSnapshot());
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
  const merged = read.deliver(reviewSnapshot());
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
  const merged = overlay.mergeEntry("wt-a", loaded, 0, current, 1);
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
  const merged = overlay.mergeEntry("wt-a", loaded, 0, current, 1);
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
  const published = await pending;
  assert.equal(reviewedOf(published, "probe.txt"), true, "重开不能直接发布 rawSnapshot");
});

test("不同 worktree 的本地记录互不影响", () => {
  const overlay = new ReviewOverlay();
  let seq = 0;
  const nextSeq = () => ++seq;
  const read = overlay.beginRead();
  overlay.record("wt-b", "probe.txt", probe, true, nextSeq());
  const merged = read.deliver(reviewSnapshot());
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
  const gateIndex = app.indexOf("if (!published.view) return null;");
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
  const merged = read.deliver(reviewSnapshot());
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
  const merged = read.deliver(reviewSnapshot());
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
  const merged = read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: false, headVersion: "h-b", worktreeVersion: "w-b-v1" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p" },
    ],
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt"], total: 3, reviewedCount: 1 },
  }));
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
  const merged = read.deliver(reviewSnapshot());
  assert.equal(reviewedOf(merged, "b.txt"), true);
  assert.equal(merged.review.reviewedCount, 3);
});

test("契约后果：若把后端确认的 v2 改记成 v1，就会错误勾上 v1（证明版本必须来自后端）", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  // 故意按错误来源记账：把后端确认的 B(v2) 改记成 v1。
  overlay.record("wt-a", "b.txt", { head: "h-b", worktree: "w-b-v1" }, true, 1);
  const merged = read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: false, headVersion: "h-b", worktreeVersion: "w-b-v1" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p" },
    ],
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt"], total: 3, reviewedCount: 1 },
  }));
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
  const merged = read.deliver({
    worktreeId: "wt-a",
    entries: [
      { path: "a.txt", reviewed: false, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "a.txt", reviewed: false, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p" },
    ],
    review: { paths: ["a.txt", "probe.txt"], reviewed: [], total: 2, reviewedCount: 0 },
  });
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
  const gateIndex = app.indexOf("if (!published.view) return null;");
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
  const merged = read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: true, headVersion: null, worktreeVersion: "w-p" },
    ],
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt", "b.txt", "probe.txt"], total: 3, reviewedCount: 3, seq: 1 },
  }));
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
  const merged = read.deliver(notReviewed);
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
  const merged = read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p" },
    ],
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt", "b.txt"], total: 3, reviewedCount: 2, seq: 1 },
  }));
  assert.equal(reviewedOf(merged, "probe.txt"), true, "比读取水位更新的成功必须生效");
});

test("读取水位：版本变化后旧成功同样不生效", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  applyReviewSuccess({
    overlay, worktreeId: "wt-a", reviewed: ["a.txt", "b.txt", "probe.txt"],
    marks: marksOf(["a.txt", "h-a", "w-a"], ["b.txt", "h-b", "w-b"], ["probe.txt", null, "w-old"]), seq: 2,
  });
  const merged = read.deliver(reviewSnapshot({
    entries: [
      { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a" },
      { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b" },
      { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-new" },
    ],
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt", "b.txt"], total: 3, reviewedCount: 2, seq: 1 },
  }));
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
  const merged = overlay.mergeEntry("wt-a", loaded, 2, current, 2);
  assert.equal(merged.reviewed, false, "水位 2 已超越 seq=1，旧成功不能复活");
  assert.equal(merged.metricsLoaded, true, "元数据本身仍要补上");
});

test("getRows 水位：比读取水位更新的成功仍然生效", () => {
  const overlay = new ReviewOverlay();
  // 更晚的成功 seq=3，后端这次读到的是水位 2（未审）。
  overlay.record("wt-a", "U", { head: null, worktree: "w-v1" }, true, 3);
  const loaded = { path: "U", reviewed: false, headVersion: null, worktreeVersion: "w-v1", metricsLoaded: true };
  const current = { path: "U", reviewed: false, headVersion: null, worktreeVersion: "w-v1", metricsLoaded: false };
  const merged = overlay.mergeEntry("wt-a", loaded, 2, current, 2);
  assert.equal(merged.reviewed, true, "比读取水位更新的成功必须生效");
});

test("getRows 用后端读取水位：后端已读到已审时，旧界面未审不能覆盖它（ST07-R12-2）", () => {
  const overlay = new ReviewOverlay();
  // 列表 L 在 seq=0 捕获 B 未审；B 标记成功 seq=1（回执暂缓）。
  overlay.record("wt-a", "B", { head: null, worktree: "w-b" }, true, 1);
  // getRows 从后端读到 **已审**、水位 1（后端真值），界面仍是未审（水位 0）。
  const loaded = { path: "B", reviewed: true, headVersion: null, worktreeVersion: "w-b", metricsLoaded: true };
  const current = { path: "B", reviewed: false, headVersion: null, worktreeVersion: "w-b", metricsLoaded: false };
  const merged = overlay.mergeEntry("wt-a", loaded, 1, current, 0);
  assert.equal(merged.reviewed, true, "后端读取到已审，旧界面未审不能覆盖它");
  assert.equal(merged.metricsLoaded, true);
});

test("getRows 用后端读取水位：界面更新时仍以水位大的一侧为准", () => {
  const overlay = new ReviewOverlay();
  // 界面水位 2 已审，后端这次读到的是水位 1 未审（更旧）。
  const loaded = { path: "B", reviewed: false, headVersion: null, worktreeVersion: "w-b", metricsLoaded: true };
  const current = { path: "B", reviewed: true, headVersion: null, worktreeVersion: "w-b", metricsLoaded: false };
  const merged = overlay.mergeEntry("wt-a", loaded, 1, current, 2);
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
  const merged = overlay.mergeEntry("wt-a", loaded, 1, current, 0);
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
  const merged = overlay.mergeEntry("wt-a", loaded, 2, current, 2);
  assert.equal(merged.reviewed, false, "水位 2 已超越旧成功 seq1");
});

test("getRows 真实读取水位：回执尚未到达时也必须以后端读取为准（ST07-R12-2 无 override 变体）", () => {
  const overlay = new ReviewOverlay();
  // 列表 L 在 seq=0 捕获 B 未审并 hold。
  // B 的标记已在后端成功（seq=1），但**回执还没到达前端**，所以叠加层里没有记录。
  // getRows 从后端读到已审、`reviewSeq = 1`；界面仍是水位 0 的未审。
  const loaded = { path: "B", reviewed: true, headVersion: null, worktreeVersion: "w-b", metricsLoaded: true };
  const current = { path: "B", reviewed: false, headVersion: null, worktreeVersion: "w-b", metricsLoaded: false };
  const merged = overlay.mergeEntry("wt-a", loaded, 1, current, 0);
  assert.equal(merged.reviewed, true,
    "后端读取水位更大，旧界面未审不能覆盖它（否则界面与磁盘长期不一致）");
});
