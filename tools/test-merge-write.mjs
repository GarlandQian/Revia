import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
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
const { CloseCoordinator, CloseDriver, CloseGuardGate, OperationResultGate, SnapshotGate, WriteGuardOrchestrator } =
  await import("../src/lib/refreshGate.ts");
const { runMergeWrite } = await import("../src/features/merge/mergeWrite.ts");
imports.deregister();

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  return { promise, resolve, reject };
}

const flush = () => new Promise((resolve) => setImmediate(resolve));

/**
 * 与 App 相同的生产协调器：关闭驱动、写入编排、操作结果通道。
 * IPC 与重读用可控的 deferred 代替，记录调用以便断言。
 */
function makeEnv() {
  const coordinator = new CloseCoordinator();
  const guards = new CloseGuardGate();
  guards.markInstalled("window");
  guards.markInstalled("exit");
  const native = [];
  let guardImpl = async () => {};
  const driver = new CloseDriver({
    coordinator,
    releaseExitGuard: async () => {},
    closeWindow: async () => { native.push("close"); },
    exitApp: async () => { native.push("exit"); },
    onError: () => {},
    onAsk: () => {},
    onSettled: () => {},
  });
  const orchestrator = new WriteGuardOrchestrator({
    closeDriver: driver,
    closeBlockedReason: "closing",
    guardsReady: () => guards.ready,
    guardBlockedReason: () => guards.blockedReason,
    register: (id) => coordinator.beginWrite(id),
    guard: () => guardImpl(),
    rollback: (id) => coordinator.rollbackWrite(id),
    reconcile: () => driver.notifyStateChanged(),
    endWrite: (id, needsVerification) => coordinator.endWrite(id, needsVerification),
  });
  const gate = new SnapshotGate();
  const operations = new OperationResultGate(gate);
  const writing = new Set();
  const executed = [];
  const pendingExecutes = [];
  const refreshed = [];
  const reports = [];
  const ends = [];
  let refreshImpl = async () => true;
  const deps = {
    writing,
    publishWriting: () => {},
    begin: (id) => orchestrator.begin(id),
    end: (id, needsVerification) => {
      ends.push({ id, needsVerification });
      orchestrator.end(id, needsVerification);
    },
    beginOperation: (id) => operations.begin(id),
    canRecord: (token) => operations.canRecord(token),
    execute: (request) => {
      executed.push(request);
      const call = deferred();
      pendingExecutes.push(call);
      return call.promise;
    },
    refreshTarget: (id) => {
      refreshed.push(id);
      return refreshImpl(id);
    },
    reportFailure: (id, error) => reports.push({ id, error }),
    toError: (caught) =>
      caught && typeof caught === "object" && "kind" in caught
        ? { kind: caught.kind, message: String(caught.message) }
        : { kind: "internal", message: String(caught?.message ?? caught) },
    busyMessage: "busy",
    guardUnavailable: (reason) => `guard:${reason}`,
  };
  return {
    coordinator, driver, guards, gate, operations, writing, native, deps,
    executed, pendingExecutes, refreshed, reports, ends,
    setGuard(fn) { guardImpl = fn; },
    setRefresh(fn) { refreshImpl = fn; },
  };
}

const apply = (worktreeId) => ({ kind: "apply", worktreeId, path: "a.txt", result: "merged\n", fingerprint: "fp" });
const acceptSide = (worktreeId) => ({
  kind: "acceptSide",
  worktreeId,
  targets: [{ path: "a.txt", fingerprint: "fp" }],
  side: "ours",
});

test("同一 worktree 已有写入时拒绝，不发第二次 IPC", async () => {
  const env = makeEnv();
  const first = runMergeWrite(env.deps, apply("wt-a"));
  await flush();
  await assert.rejects(runMergeWrite(env.deps, acceptSide("wt-a")), { kind: "rejected", message: "busy" });
  assert.equal(env.executed.length, 1);
  assert.deepEqual(env.coordinator.writingWorktrees(), ["wt-a"]);
  env.pendingExecutes[0].resolve({ path: "a.txt", recoveryRecordId: "r1" });
  await first;
  assert.equal(env.writing.size, 0);
  assert.equal(env.coordinator.hasWriting(), false);
  assert.deepEqual(env.ends, [{ id: "wt-a", needsVerification: false }]);
});

test("关闭/退出保护失败时不发 IPC，门禁与登记都撤回", async () => {
  const env = makeEnv();
  env.setGuard(async () => { throw new Error("保护建立失败"); });
  await assert.rejects(runMergeWrite(env.deps, apply("wt-a")), { kind: "internal", message: "保护建立失败" });
  assert.equal(env.executed.length, 0);
  assert.equal(env.writing.size, 0);
  assert.equal(env.coordinator.hasWriting(), false);
  assert.deepEqual(env.ends, [], "没有开始的写入不收尾");

  // 监听没有就绪（例如已卸载）：不建立保护、不发 IPC。
  env.setGuard(async () => {});
  env.guards.dispose();
  assert.equal(env.guards.ready, false);
  await assert.rejects(runMergeWrite(env.deps, apply("wt-a")), { kind: "internal", message: /^guard:/ });
  assert.equal(env.executed.length, 0);
  assert.equal(env.writing.size, 0);
  assert.equal(env.coordinator.hasWriting(), false);
});

test("写入期间关闭窗口先询问，写完前不执行关闭", async () => {
  const env = makeEnv();
  const running = runMergeWrite(env.deps, apply("wt-a"));
  await flush();
  assert.equal(env.driver.handleRequest("close").kind, "ask");
  assert.deepEqual(env.native, []);
  env.pendingExecutes[0].resolve({ path: "a.txt", recoveryRecordId: null });
  await running;
  assert.equal(env.coordinator.hasWriting(), false);
  assert.deepEqual(env.refreshed, ["wt-a"]);
});

test("失败与重读归原 worktree，另一 worktree 的写入互不影响", async () => {
  const env = makeEnv();
  env.gate.activate("wt-a");
  const onA = runMergeWrite(env.deps, apply("wt-a"));
  await flush();
  // 用户切到 B，并在 B 上开始另一笔合并写入。
  env.gate.activate("wt-b");
  const onB = runMergeWrite(env.deps, acceptSide("wt-b"));
  await flush();
  assert.equal(env.executed.length, 2);
  env.pendingExecutes[0].reject({ kind: "rejected", message: "指纹已变化" });
  env.pendingExecutes[1].resolve([{ path: "a.txt", recoveryRecordId: "r2" }]);
  await assert.rejects(onA, { kind: "rejected", message: "指纹已变化" });
  await onB;
  assert.deepEqual(env.reports.map((report) => report.id), ["wt-a"]);
  assert.deepEqual([...env.refreshed].sort(), ["wt-a", "wt-b"]);
  assert.equal(env.writing.size, 0);
});

test("较新的操作让旧结果失去记录资格", async () => {
  const env = makeEnv();
  const running = runMergeWrite(env.deps, apply("wt-a"));
  await flush();
  env.operations.begin("wt-a");
  env.pendingExecutes[0].reject({ kind: "git", message: "写入失败" });
  await assert.rejects(running, { kind: "git" });
  assert.deepEqual(env.reports, []);
  assert.deepEqual(env.refreshed, ["wt-a"], "仍然真实重读原目标");
});

test("调用方不再等待（组件已卸载）时照常收尾，待核对阻止静默退出", async () => {
  const env = makeEnv();
  const running = runMergeWrite(env.deps, apply("wt-a"));
  running.catch(() => {});
  await flush();
  env.pendingExecutes[0].reject({ kind: "needsVerification", message: "写后核对不一致" });
  await flush();
  await flush();
  assert.deepEqual(env.ends, [{ id: "wt-a", needsVerification: true }], "收尾只做一次");
  assert.equal(env.writing.size, 0);
  assert.deepEqual(env.reports.map((report) => report.error.kind), ["needsVerification"]);
  assert.deepEqual(env.coordinator.pendingWorktrees(), ["wt-a"]);
  assert.equal(env.driver.handleRequest("exit").kind, "ask");
  assert.deepEqual(env.native, []);
});

test("重读失败不覆盖写入结果", async () => {
  const env = makeEnv();
  env.setRefresh(async () => { throw new Error("重读失败"); });
  const failed = runMergeWrite(env.deps, apply("wt-a"));
  await flush();
  env.pendingExecutes[0].reject({ kind: "rejected", message: "指纹已变化" });
  await assert.rejects(failed, { kind: "rejected", message: "指纹已变化" });

  const succeeded = runMergeWrite(env.deps, apply("wt-a"));
  await flush();
  env.pendingExecutes[1].resolve({ path: "a.txt", recoveryRecordId: null });
  await succeeded;
  assert.deepEqual(env.ends.map((end) => end.needsVerification), [false, false]);
  assert.equal(env.writing.size, 0);
});

test("三个界面入口都经 App 的受保护写入，不直接调用合并写 IPC", () => {
  const read = (path) => readFileSync(new URL(path, sourceRoot), "utf8");
  const mergeView = read("features/diff/MergeView.tsx");
  const dialog = read("components/app/ConflictDialog.tsx");
  const app = read("App.tsx");
  for (const [name, source] of [["MergeView", mergeView], ["ConflictDialog", dialog]]) {
    assert.doesNotMatch(source, /\.mergeApply\b|\.mergeAcceptSide\b/, `${name} 不直接发写 IPC`);
  }
  // 合并编辑器（应用、接受一侧）与只能接受一侧的视图各一处，冲突对话框一处。
  assert.equal(mergeView.match(/await onWrite\(/g)?.length, 2);
  assert.equal(dialog.match(/await onWrite\(/g)?.length, 1);
  assert.equal(app.match(/onWrite=\{performMergeWrite\}/g)?.length, 2);
  assert.match(app, /const performMergeWrite = useCallback\(\(request: MergeWriteRequest\) => runMergeWrite</);
});
