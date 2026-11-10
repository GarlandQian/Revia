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
const { WorktreeRefresh } = await import("../src/lib/worktreeRefresh.ts");
const { ReviewOverlay } = await import("../src/lib/refreshGate.ts");
imports.deregister();

const TARGET = "/repo";

/** 一份最小快照：读取接收只看 worktreeId、revision 与审阅分界。 */
function snapshot(worktreeId, revision, entries = []) {
  return { worktreeId, revision, entries, review: { seq: 0 } };
}

const tick = () => new Promise((resolve) => setImmediate(resolve));

/**
 * 受控 harness：读取、监视登记与监视轮询都由测试决定何时完成。
 * 全部断言经过生产 module `WorktreeRefresh`，不复制它内部的判定。
 */
function createHarness({ overlay = new ReviewOverlay() } = {}) {
  const reads = [];
  const opened = [];
  const refreshed = [];
  const errors = [];
  const recovered = [];
  const watchCalls = [];
  const watchStates = [];
  const writing = new Set();
  let status = { activation: 0, active: false, error: null, target: TARGET };
  let poll = { status, changedActivation: null, eventSequence: 0 };
  let register = (worktreeId, activation) => {
    watchCalls.push({ worktreeId, activation });
    return Promise.resolve({ active: true, error: null, target: TARGET });
  };
  const refresh = new WorktreeRefresh(
    {
      overlay,
      listChanges: (worktreeId) => new Promise((resolve, reject) => {
        reads.push({ worktreeId, resolve, reject });
      }),
      publishOpened: (snap) => opened.push(snap),
      publishRefreshed: (snap) => refreshed.push(snap),
      onError: (error, worktreeId) => errors.push({ error, worktreeId }),
      recover: (worktreeId) => {
        recovered.push(worktreeId);
        return Promise.resolve();
      },
      registerWatch: (worktreeId, activation) => register(worktreeId, activation),
      watchStatus: () => Promise.resolve(poll),
      onWatchState: (state) => watchStates.push(state),
      isWriting: (worktreeId) => writing.has(worktreeId),
      watchTarget: () => TARGET,
    },
  );
  return {
    refresh,
    reads,
    opened,
    refreshed,
    errors,
    recovered,
    watchCalls,
    watchStates,
    writing,
    setRegister(next) {
      register = next;
    },
    /** 让下一次轮询返回指定的状态 / 变化序号。 */
    setPoll(next) {
      poll = { status: { ...status }, changedActivation: null, eventSequence: 0, ...next };
    },
    setWatchStatus(next) {
      status = { ...status, ...next };
      poll = { ...poll, status };
    },
    activation: () => refresh.token().activation,
  };
}

test("打开 → 手动刷新 → 监视事件：读取、发布与监视登记都绑定同一激活代号", async () => {
  const h = createHarness();
  h.refresh.activate("A");
  const activation = h.activation();

  const open = h.refresh.readOpened();
  void h.refresh.ensureWatch();
  await tick();
  assert.equal(h.reads.length, 1);
  assert.equal(h.reads[0].worktreeId, "A");
  assert.deepEqual(h.watchCalls, [{ worktreeId: "A", activation }]);
  h.reads[0].resolve(snapshot("A", 1, [{ path: "a.ts", reviewed: false }]));
  assert.equal((await open).ok, true);
  assert.equal(h.opened.length, 1);
  assert.equal(h.opened[0].revision, 1);

  // 手动刷新不推进激活代号，因此监视登记继续有效（不会重复登记）。
  const manual = h.refresh.refresh();
  await tick();
  assert.equal(h.reads.length, 2);
  assert.equal(h.watchCalls.length, 1);
  h.reads[1].resolve(snapshot("A", 2));
  assert.equal(await manual, true);
  assert.equal(h.refreshed.length, 1);
  assert.equal(h.refreshed[0].revision, 2);

  // 监视事件经同一个协调器触发自动刷新，仍能接收。
  h.setPoll({ status: { activation, active: true, error: null, target: TARGET }, changedActivation: activation, eventSequence: 7 });
  await h.refresh.pollWatch();
  await tick();
  assert.equal(h.reads.length, 3);
  h.reads[2].resolve(snapshot("A", 3));
  await h.refresh.whenIdle();
  assert.equal(h.refreshed.length, 2);
  assert.equal(h.refreshed[1].revision, 3);
});

test("环境就绪后的重读不推进激活代号，随后的监视事件仍然生效", async () => {
  const h = createHarness();
  h.refresh.activate("A");
  const activation = h.activation();
  void h.refresh.ensureWatch();
  await tick();

  const reread = h.refresh.refresh();
  await tick();
  assert.equal(h.watchCalls.length, 1);
  h.reads[0].resolve(snapshot("A", 1));
  await reread;
  assert.equal(h.refreshed.length, 1);

  h.setPoll({ status: { activation, active: true, error: null, target: TARGET }, changedActivation: activation, eventSequence: 1 });
  await h.refresh.pollWatch();
  await tick();
  assert.equal(h.reads.length, 2);
  h.reads[1].resolve(snapshot("A", 2));
  await h.refresh.whenIdle();
  assert.equal(h.refreshed.length, 2);
});

test("A→B→A：旧目标的快照与错误都不进入新界面", async () => {
  const h = createHarness();
  h.refresh.activate("A");
  const firstA = h.refresh.readOpened();
  await tick();
  assert.equal(h.reads[0].worktreeId, "A");

  h.refresh.activate("B");
  const readB = h.refresh.readOpened();
  await tick();
  assert.equal(h.reads[1].worktreeId, "B");

  h.refresh.activate("A");
  const secondA = h.refresh.readOpened();
  await tick();
  assert.equal(h.reads[2].worktreeId, "A");

  // 第一次 A 与 B 的迟到结果都不能发布。
  h.reads[0].resolve(snapshot("A", 9));
  h.reads[1].reject(new Error("B failed"));
  assert.equal((await firstA).ok, false);
  assert.equal((await readB).ok, false);
  assert.equal(h.opened.length, 0);
  assert.deepEqual(h.errors, []);

  // 当前 A 的读取正常发布；B 的迟到错误也不画到 A 上。
  h.reads[2].resolve(snapshot("A", 1));
  assert.equal((await secondA).ok, true);
  assert.equal(h.opened.length, 1);
  assert.equal(h.opened[0].revision, 1);
});

test("同一目标内更旧的版本不覆盖已发布快照", async () => {
  const h = createHarness();
  h.refresh.activate("A");
  const first = h.refresh.readOpened();
  await tick();
  h.reads[0].resolve(snapshot("A", 5));
  await first;
  const second = h.refresh.refresh();
  await tick();
  h.reads[1].resolve(snapshot("A", 4));
  assert.equal(await second, false);
  assert.equal(h.refreshed.length, 0);
});

test("写后核对始终读原目标，只有它仍是当前激活目标时才发布", async () => {
  const h = createHarness();
  h.refresh.activate("A");
  h.refresh.activate("B");

  const away = h.refresh.verifyWrite("A");
  await tick();
  assert.equal(h.reads[0].worktreeId, "A");
  h.reads[0].resolve(snapshot("A", 3));
  assert.equal(await away, true);
  assert.equal(h.refreshed.length, 0, "切走后不得把原目标结果画到 B");

  const current = h.refresh.verifyWrite("B");
  await tick();
  assert.equal(h.reads[1].worktreeId, "B");
  h.reads[1].resolve(snapshot("B", 1));
  assert.equal(await current, true);
  assert.equal(h.refreshed.length, 1);
  assert.equal(h.refreshed[0].worktreeId, "B");
});

test("写后核对读原目标期间切换：读取仍算核对成功，但不画到新界面", async () => {
  const h = createHarness();
  h.refresh.activate("A");
  const open = h.refresh.readOpened();
  await tick();
  h.reads[0].resolve(snapshot("A", 1));
  await open;

  const verify = h.refresh.verifyWrite("A");
  await tick();
  assert.equal(h.reads[1].worktreeId, "A");
  // 读取还在途时切到 B：原目标核对不得因此失败（写入/关闭收尾依赖它）。
  h.refresh.activate("B");
  h.reads[1].resolve(snapshot("A", 2));
  assert.equal(await verify, true, "切换界面不应使原目标核对失败");
  assert.equal(h.refreshed.length, 0, "切走后不得把原目标结果画到 B");

  // A→B→A：激活代号已变，旧代次的原目标结果也不能画回界面。
  const again = h.refresh.verifyWrite("A");
  await tick();
  assert.equal(h.reads[2].worktreeId, "A");
  h.refresh.activate("B");
  h.refresh.activate("A");
  h.reads[2].resolve(snapshot("A", 3));
  assert.equal(await again, true);
  assert.equal(h.refreshed.length, 0, "A→B→A 的旧代次结果不得发布");
});

test("写期间的自动刷新合并为结束后的补读，只读一次", async () => {
  const h = createHarness();
  h.refresh.activate("A");
  const open = h.refresh.readOpened();
  await tick();
  h.reads[0].resolve(snapshot("A", 1));
  await open;

  h.writing.add("A");
  h.refresh.requestRefresh();
  h.refresh.requestRefresh();
  await tick();
  assert.equal(h.reads.length, 1, "写期间不得发起自动读取");

  h.writing.delete("A");
  h.refresh.resume();
  await tick();
  assert.equal(h.reads.length, 2, "写结束后只补一轮");
  h.reads[1].resolve(snapshot("A", 2));
  await h.refresh.whenIdle();
  assert.equal(h.refreshed.length, 1);
  assert.equal(h.refreshed[0].revision, 2);
});

test("监视登记失败可见且不阻塞打开；手动刷新按当前激活代号重试登记", async () => {
  const h = createHarness();
  let attempts = 0;
  h.setRegister(() => {
    attempts += 1;
    h.watchCalls.push({ worktreeId: "A", activation: h.activation() });
    return attempts === 1
      ? Promise.reject(new Error("watch unavailable"))
      : Promise.resolve({ active: true, error: null, target: TARGET });
  });
  h.refresh.activate("A");
  const open = h.refresh.readOpened();
  await h.refresh.ensureWatch();
  await tick();
  h.reads[0].resolve(snapshot("A", 1));
  assert.equal((await open).ok, true);

  // 第一次登记失败：文件列表仍然发布，状态栏保持未激活。
  assert.equal(h.opened.length, 1);
  assert.equal(h.watchStates.length, 0);

  // 手动刷新重新登记：使用当前激活代号，成功后监视可用。
  const manual = h.refresh.refresh();
  await tick();
  assert.equal(attempts, 2);
  assert.equal(h.watchCalls[1].activation, h.activation());
  h.reads[1].resolve(snapshot("A", 2));
  await manual;
  assert.deepEqual(h.watchStates.at(-1), { active: true, error: null });
});

test("监视降级与旧代号的状态不覆盖当前目标", async () => {
  const h = createHarness();
  h.refresh.activate("A");
  void h.refresh.ensureWatch();
  await tick();
  const activation = h.activation();

  // 属于旧激活代号的轮询结果：丢弃（登记成功那次的 active 状态保持不变）。
  const recorded = h.watchStates.length;
  h.setPoll({ status: { activation: activation - 1, active: false, error: { kind: "rejected", message: "old" } } });
  await h.refresh.pollWatch();
  assert.equal(h.watchStates.length, recorded);

  // 当前代号的失败状态照常展示（监视失效时仍有提示）。
  const failure = { kind: "environment", message: "watch failed" };
  h.setPoll({ status: { activation, active: false, error: failure, target: TARGET } });
  await h.refresh.pollWatch();
  assert.deepEqual(h.watchStates.at(-1), { active: false, error: failure });
});

test("不可信读取按原目标重新核对，不发布旧快照", async () => {
  const overlay = new ReviewOverlay(1);
  overlay.record("A", "x.ts", { head: "h", worktree: "w" }, true, 1);
  overlay.record("A", "y.ts", { head: "h", worktree: "w" }, true, 2);
  const h = createHarness({ overlay });
  h.refresh.activate("A");
  const open = h.refresh.readOpened();
  await tick();
  h.reads[0].resolve(snapshot("A", 1, [{ path: "x.ts", reviewed: false }]));
  const outcome = await open;
  assert.equal(outcome.ok, false);
  assert.equal(outcome.reason, "untrusted");
  assert.deepEqual(h.recovered, ["A"]);
  assert.equal(h.opened.length, 0);
});
