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
const { SnapshotGate, RefreshCoordinator, OperationResultGate, ReviewOverlay, SelectionGate, WatchEventGate, commitReviewResult, planReviewResult, readThroughReview } = await import("../src/lib/refreshGate.ts");
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
    review: { paths: ["a.txt", "b.txt", "probe.txt"], reviewed: ["a.txt", "b.txt"], total: 3, reviewedCount: 2 },
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
  const read = overlay.beginRead();
  overlay.record("wt-a", "probe.txt", probe, true);
  const merged = read.deliver(reviewSnapshot());
  assert.equal(reviewedOf(merged, "probe.txt"), true);
  assert.equal(merged.review.reviewedCount, 3);
});

test("读取发起后才成功的取消，不会被这次读取勾回", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  overlay.record("wt-a", "b.txt", bVersions, false);
  const merged = read.deliver(reviewSnapshot());
  assert.equal(reviewedOf(merged, "b.txt"), false);
  assert.equal(merged.review.reviewedCount, 1);
});

test("文件版本真的变化时本地记录不生效，标记仍按后端结果失效", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  overlay.record("wt-a", "probe.txt", { head: null, worktree: "w-old" }, true);
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
  overlay.record("wt-a", "probe.txt", probe, true);
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
  const read = overlay.beginRead();
  // 先成功后取消：同一路径的最新操作是取消。
  overlay.record("wt-a", "b.txt", bVersions, true);
  overlay.record("wt-a", "b.txt", bVersions, false);
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
  // 生产入口是 readThroughReview：它在调用读取函数**之前**取分界。
  // 这里用延迟的读取函数模拟写结果/读取结果的迟到交付。
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  const pending = readThroughReview(overlay, async () => {
    await gate;
    return reviewSnapshot();
  });
  // 请求已发起（分界已取），结果还没交付，用户成功勾选。
  overlay.record("wt-a", "probe.txt", probe, true);
  release();
  const merged = await pending;
  assert.equal(reviewedOf(merged, "probe.txt"), true, "写结果不能回退期间的成功标记");
  assert.equal(merged.review.reviewedCount, 3);
});

test("写结果期间取消同样保留（ST07-R7-1 反向）", async () => {
  const overlay = new ReviewOverlay();
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  const pending = readThroughReview(overlay, async () => {
    await gate;
    return reviewSnapshot();
  });
  overlay.record("wt-a", "a.txt", { head: "h-a", worktree: "w-a" }, false);
  release();
  const merged = await pending;
  assert.equal(reviewedOf(merged, "a.txt"), false);
  assert.equal(merged.review.reviewedCount, 1);
});

test("A/B 并发：只登记 B 会漏掉 A，按完整成功集合记账才不漏（ST07-R7-3 / SP07-R7-2）", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  // B 的响应先到，且后端返回的完整集合已经包含 A+B。
  // 界面此刻仍是 A、probe 都未审阅（A 的响应还没被应用）。
  const current = [
    { path: "a.txt", reviewed: true, headVersion: "h-a", worktreeVersion: "w-a", worktreeVersionConfirmed: true },
    { path: "b.txt", reviewed: true, headVersion: "h-b", worktreeVersion: "w-b", worktreeVersionConfirmed: true },
    { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p", worktreeVersionConfirmed: true },
  ];
  const plan = planReviewResult({
    worktreeId: "wt-a",
    path: "b.txt",
    versions: bVersions,
    reviewed: ["a.txt", "b.txt", "probe.txt"],
    entries: current,
  });
  commitReviewResult(overlay, "wt-a", plan);
  // A 的响应随后才到、被展示门禁拒收——但记账已在门禁之前完成。
  // 旧读取（分界早于两者）现在交付，A 与 B 都必须保留。
  const merged = read.deliver(reviewSnapshot());
  assert.equal(reviewedOf(merged, "probe.txt"), true, "A 不能因迟到被漏账");
  assert.equal(reviewedOf(merged, "b.txt"), true);
  assert.equal(merged.review.reviewedCount, 3);
});

test("A/B 并发：只登记目标 B 时旧读取仍会把 A 回退（证明批量记账必要）", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  // 只记本次目标（旧实现），A 就漏账了。
  overlay.record("wt-a", "b.txt", bVersions, true);
  const merged = read.deliver(reviewSnapshot());
  assert.equal(reviewedOf(merged, "probe.txt"), false, "这正是只记 B 时的漏账表现");
  assert.equal(merged.review.reviewedCount, 2);
});

test("未跟踪元数据补全不整条替换：同版本已成功的勾选保留，进度一致（SP07-R7-3）", () => {
  const overlay = new ReviewOverlay();
  const metricsRead = overlay.beginRead();
  // getRows 等待期间用户勾选成功。
  overlay.record("wt-a", "probe.txt", probe, true);
  // 交付的 entry 是旧读取：未审阅、行数才刚补上。
  const loaded = { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-p", metricsLoaded: true };
  const current = { path: "probe.txt", reviewed: true, headVersion: null, worktreeVersion: "w-p", metricsLoaded: false };
  const merged = overlay.mergeEntry("wt-a", loaded, current, metricsRead.boundaryOf());
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
  const metricsRead = overlay.beginRead();
  const loaded = { path: "probe.txt", reviewed: false, headVersion: null, worktreeVersion: "w-new", metricsLoaded: true };
  const current = { path: "probe.txt", reviewed: true, headVersion: null, worktreeVersion: "w-old", metricsLoaded: false };
  const merged = overlay.mergeEntry("wt-a", loaded, current, metricsRead.boundaryOf());
  assert.equal(merged.reviewed, false, "版本不同不能沿用旧状态");
});

test("重开仓库（再次添加当前仓库）经共享入口，不能直接发布 rawSnapshot（SP07-R7-4）", async () => {
  const overlay = new ReviewOverlay();
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  const pending = readThroughReview(overlay, async () => {
    await gate;
    return reviewSnapshot();
  });
  overlay.record("wt-a", "probe.txt", probe, true);
  release();
  const published = await pending;
  assert.equal(reviewedOf(published, "probe.txt"), true, "重开不能直接发布 rawSnapshot");
});

test("不同 worktree 的本地记录互不影响", () => {
  const overlay = new ReviewOverlay();
  const read = overlay.beginRead();
  overlay.record("wt-b", "probe.txt", probe, true);
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
  assert.match(app, /commitReviewResult\(/, "成功的标记变更必须记账");
  // 记账必须在展示门禁之前，否则迟到成功会漏账。
  const recordIndex = app.indexOf("commitReviewResult(reviewOverlayRef.current");
  const gateIndex = app.indexOf("if (!isCurrent()) return null;");
  assert.ok(recordIndex > 0 && gateIndex > 0 && recordIndex < gateIndex, "记账必须先于展示门禁");
  // 写结果也必须用请求前分界，不能在交付时才取。
  const writeIndex = app.indexOf("const reviewRead = reviewOverlayRef.current.beginRead();");
  const writeCall = app.indexOf("await (unstaging ? ipc.unstageHunk : ipc.stageHunk)");
  assert.ok(writeIndex > 0 && writeCall > 0 && writeIndex < writeCall, "写结果的分界必须早于写请求");
});
