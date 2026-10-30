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
const { runRemoteWrite, shouldShowPushDialog, planAfterPushRejected, updateNotice, pushNotice, branchFailureDetail } =
  await import("../src/features/branches/remoteWrite.ts");
const { runGuardedWrite } = await import("../src/lib/guardedWrite.ts");
const { IncomingState } = await import("../src/features/branches/incomingState.ts");
const {
  captureRemoteTarget, confirmRemoteAction, finishCommitAndContinue, preparePush,
  remoteShortcutAction, remoteTargetIsCurrent, runPushSequence,
} = await import("../src/features/branches/remoteContinuation.ts");
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

/** 与 App 相同的生产协调器（关闭驱动、写入编排、操作结果通道）。 */
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
  const pending = [];
  const refreshed = [];
  const reports = [];
  const ends = [];
  const successes = [];
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
      pending.push(call);
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
    coordinator, driver, guards, gate, operations, orchestrator, writing, native, deps,
    executed, pending, refreshed, reports, ends, successes,
    setGuard(fn) { guardImpl = fn; },
    setRefresh(fn) { refreshImpl = fn; },
  };
}

/** 远端写入的依赖：执行的是 `executeIntent`，成功结果单独回调。 */
function remoteDeps(env) {
  const intents = [];
  const deps = {
    ...env.deps,
    executeIntent: (intent) => {
      intents.push(intent);
      const call = deferred();
      env.pending.push(call);
      return call.promise;
    },
    onSuccess: (intent, result) => env.successes.push({ intent, result }),
  };
  return { deps, intents };
}

const checkoutIntent = (worktreeId, strategy = "plain") => ({
  kind: "checkout",
  worktreeId,
  target: { kind: "local", name: "main" },
  strategy,
});

test("远端写入走同一套门禁：同一 worktree 拒绝第二个写，不发 IPC", async () => {
  const env = makeEnv();
  const { deps, intents } = remoteDeps(env);
  const first = runRemoteWrite(deps, checkoutIntent("wt-a"));
  await flush();
  const second = await runRemoteWrite(deps, { kind: "fetch", worktreeId: "wt-a" });
  assert.equal(second.ok, false);
  assert.equal(second.error.kind, "rejected");
  assert.equal(intents.length, 1, "第二次不发 IPC");
  env.pending[0].resolve({ kind: "done", branch: "main" });
  await first;
  assert.equal(env.writing.size, 0);
  assert.deepEqual(env.ends, [{ id: "wt-a", needsVerification: false }]);
});

test("关闭/退出保护失败时不写仓库", async () => {
  const env = makeEnv();
  const { deps, intents } = remoteDeps(env);
  env.setGuard(async () => { throw new Error("保护建立失败"); });
  const result = await runRemoteWrite(deps, { kind: "update", worktreeId: "wt-a" });
  assert.equal(result.ok, false);
  assert.equal(result.error.kind, "internal");
  assert.equal(intents.length, 0);
  assert.equal(env.writing.size, 0);

  env.setGuard(async () => {});
  env.guards.dispose();
  const blocked = await runRemoteWrite(deps, { kind: "push", worktreeId: "wt-a", request: { remote: "origin", branch: "main", tags: false, force: "none" } });
  assert.equal(blocked.ok, false);
  assert.equal(intents.length, 0);
});

test("结果与重读归原 worktree，切换后仍按发起目标报告", async () => {
  const env = makeEnv();
  const { deps } = remoteDeps(env);
  env.gate.activate("wt-a");
  const onA = runRemoteWrite(deps, { kind: "fetch", worktreeId: "wt-a" });
  await flush();
  env.gate.activate("wt-b");
  env.pending[0].reject({ kind: "rejected", message: "远端拒绝" });
  const outcome = await onA;
  assert.equal(outcome.ok, false);
  assert.deepEqual(env.reports.map((report) => report.id), ["wt-a"]);
  assert.deepEqual(env.refreshed, ["wt-a"]);
  assert.equal(env.writing.size, 0);
});

test("成功结果交给调用方，失败不收尾信息但收尾只一次", async () => {
  const env = makeEnv();
  const { deps } = remoteDeps(env);
  const running = runRemoteWrite(deps, { kind: "update", worktreeId: "wt-a" });
  await flush();
  env.pending[0].resolve({ status: "updated", commitsMerged: 2 });
  const outcome = await running;
  assert.deepEqual(outcome, { ok: true, result: { status: "updated", commitsMerged: 2 } });
  assert.equal(env.successes.length, 1);
  assert.equal(env.successes[0].result.status, "updated");
  assert.deepEqual(env.ends, [{ id: "wt-a", needsVerification: false }]);
});

test("结果待核对时登记到关闭协调器，退出前先询问", async () => {
  const env = makeEnv();
  const { deps } = remoteDeps(env);
  const running = runRemoteWrite(deps, { kind: "push", worktreeId: "wt-a", request: { remote: "origin", branch: "main", tags: false, force: "none" } });
  running.catch(() => {});
  await flush();
  env.pending[0].reject({ kind: "needsVerification", message: "网络超时" });
  const outcome = await running;
  assert.equal(outcome.ok, false);
  assert.deepEqual(env.ends, [{ id: "wt-a", needsVerification: true }]);
  assert.deepEqual(env.coordinator.pendingWorktrees(), ["wt-a"]);
  assert.equal(env.driver.handleRequest("exit").kind, "ask");
  assert.deepEqual(env.native, []);
});

test("通用编排与合并写入共用一份实现", () => {
  const read = (path) => readFileSync(new URL(path, sourceRoot), "utf8");
  const merge = read("features/merge/mergeWrite.ts");
  assert.match(merge, /runGuardedWrite/, "合并写入委托给共用编排");
  const app = read("App.tsx");
  // 所有分支 / 远端入口都走 performRemoteWrite：提交并推送、更新、推送、检出、fetch。
  assert.match(app, /const performRemoteWrite = useCallback/);
  assert.match(app, /runRemoteWrite<OperationContext>/);
  for (const method of ["checkoutBranch", "createBranch", "fetchRemotes", "updateProject", "pushBranch"]) {
    assert.ok(app.includes(`ipc.${method}(`), `${method} 通过 executeRemoteIntent 调用`);
  }
  assert.doesNotMatch(app, /ipc\.pushBranch\(\{\s*worktreeId[^}]*\}\);\n\s*notify/s, "推送不绕过编排");
});

test("提交并推送：按两项设置决定是否显示推送对话框", () => {
  // 默认：显示对话框。
  assert.equal(shouldShowPushDialog({ pushDialogOnCommit: true, pushDialogProtectedOnly: false, protectedBranch: false }), true);
  // 关闭：直接推送。
  assert.equal(shouldShowPushDialog({ pushDialogOnCommit: false, pushDialogProtectedOnly: false, protectedBranch: true }), false);
  // 只在受保护分支显示。
  assert.equal(shouldShowPushDialog({ pushDialogOnCommit: true, pushDialogProtectedOnly: true, protectedBranch: true }), true);
  assert.equal(shouldShowPushDialog({ pushDialogOnCommit: true, pushDialogProtectedOnly: true, protectedBranch: false }), false);
  // 两项都关：不显示。
  assert.equal(shouldShowPushDialog({ pushDialogOnCommit: false, pushDialogProtectedOnly: true, protectedBranch: true }), false);
});

const pushRequest = { remote: "origin", branch: "topic", tags: true, force: "withLease" };
const pushPreview = { remote: "origin", targetBranch: "topic", protected: false };
const rejectedPush = { kind: "rejected", remote: "origin", branch: "topic", reason: "非快进", output: "" };

test("远端确认绑定发起激活，切到 B 或 A→B→A 均不能确认旧检出/推送", async () => {
  const env = makeEnv();
  const { deps, intents } = remoteDeps(env);
  env.gate.activate("wt-a");
  const origin = captureRemoteTarget(env.gate.token());
  assert.deepEqual(origin, { worktreeId: "wt-a", activation: 1 });
  assert.equal(captureRemoteTarget(env.gate.token(), "wt-b"), null);
  assert.equal(captureRemoteTarget({ worktreeId: null, activation: 0 }), null);
  env.gate.activate("wt-b");
  assert.equal(remoteTargetIsCurrent(origin, env.gate.token()), false);
  for (const strategy of ["smart", "force"]) {
    assert.equal(confirmRemoteAction(origin, env.gate.token(), (target) =>
      runRemoteWrite(deps, checkoutIntent(target.worktreeId, strategy))), undefined);
  }
  env.gate.activate("wt-a");
  assert.equal(confirmRemoteAction(origin, env.gate.token(), (target) =>
    runRemoteWrite(deps, { kind: "push", worktreeId: target.worktreeId, request: pushRequest })), undefined);
  assert.deepEqual(intents, [], "过期确认不发任何写 IPC");
  const fresh = captureRemoteTarget(env.gate.token());
  const checkout = confirmRemoteAction(fresh, env.gate.token(), (target) =>
    runRemoteWrite(deps, checkoutIntent(target.worktreeId, "smart")));
  await flush();
  assert.equal(intents[0].worktreeId, "wt-a");
  env.pending[0].resolve({ kind: "done", branch: "main" });
  assert.equal((await checkout).ok, true);
});

test("迟到推送预览不会在新 worktree 弹确认，自动推送仍使用原目标", async () => {
  const gate = new SnapshotGate();
  gate.activate("wt-a");
  const origin = captureRemoteTarget(gate.token());
  const preview = deferred();
  const readTargets = [];
  const deps = {
    preview: (id) => { readTargets.push(id); return preview.promise; },
    current: () => gate.token(),
    showDialog: () => true,
  };
  const confirmation = preparePush(origin, deps);
  const direct = preparePush(origin, { ...deps, showDialog: () => false });
  gate.activate("wt-b");
  preview.resolve(pushPreview);
  assert.deepEqual(await confirmation, { kind: "stale", origin });
  assert.deepEqual(await direct, {
    kind: "push", origin,
    request: { remote: "origin", branch: "topic", tags: false, force: "none" },
  });
  assert.deepEqual(readTargets, ["wt-a", "wt-a"]);
});

test("推送→自动更新→重试始终归原 worktree，保留标签且不循环", async () => {
  const env = makeEnv();
  const { deps, intents } = remoteDeps(env);
  env.gate.activate("wt-a");
  const origin = captureRemoteTarget(env.gate.token());
  const notices = [];
  const run = runRemoteWrite({
    ...deps,
    executeIntent: () => runPushSequence(origin, pushRequest, false, {
      push: (worktreeId, request) => deps.executeIntent({ kind: "push", worktreeId, request }),
      update: async (worktreeId) => {
        const result = await deps.executeIntent({ kind: "update", worktreeId });
        return result.status === "updated";
      },
      autoUpdate: () => true,
      onResult: (result) => notices.push(result),
    }),
  }, { kind: "push", worktreeId: origin.worktreeId, request: pushRequest });
  await flush();
  env.driver.handleRequest("exit");
  env.driver.handleChoice("wait");
  env.gate.activate("wt-b");
  env.pending[0].resolve(rejectedPush);
  await flush();
  assert.equal(intents[1].kind, "update");
  assert.deepEqual(env.native, [], "自动更新期间保留原写入生命周期");
  env.gate.activate("wt-c");
  env.pending[1].resolve({ status: "updated" });
  await flush();
  assert.deepEqual(intents[2].request, { ...pushRequest, force: "none" });
  assert.deepEqual(env.native, [], "重试期间不放行等待的退出");
  env.pending[2].resolve(rejectedPush);
  assert.deepEqual(await run, { ok: true, result: "done" });
  await flush();
  assert.deepEqual(intents.map(({ kind, worktreeId }) => [kind, worktreeId]), [
    ["push", "wt-a"], ["update", "wt-a"], ["push", "wt-a"],
  ]);
  assert.deepEqual(env.refreshed, ["wt-a"]);
  assert.equal(notices.length, 2);
  assert.equal(env.writing.size, 0);
  assert.deepEqual(env.native, ["exit"]);
});

test("更新冲突/失败不续推，手动更新后失败不再次安排自动更新", async () => {
  const origin = { worktreeId: "wt-a", activation: 1 };
  const calls = [];
  const deps = {
    push: async (id) => { calls.push(["push", id]); return rejectedPush; },
    update: async (id) => { calls.push(["update", id]); return false; },
    autoUpdate: () => true,
    onResult: () => {},
  };
  assert.equal(await runPushSequence(origin, pushRequest, false, deps), "update-failed");
  assert.deepEqual(calls, [["push", "wt-a"], ["update", "wt-a"]]);
  calls.length = 0;
  assert.equal(await runPushSequence(origin, pushRequest, true, deps), "done");
  assert.deepEqual(calls, [["push", "wt-a"]]);
  calls.length = 0;
  assert.equal(await runPushSequence(origin, pushRequest, false, { ...deps, autoUpdate: () => false }), "offer-update");
  assert.deepEqual(calls, [["push", "wt-a"]]);
});

test("提交并推送复用原写门禁，等待退出不会在提交与推送之间放行", async () => {
  const env = makeEnv();
  const { deps, intents } = remoteDeps(env);
  env.gate.activate("wt-a");
  const origin = captureRemoteTarget(env.gate.token());
  env.writing.add("wt-a");
  assert.equal((await env.orchestrator.begin("wt-a")).ok, true);
  assert.equal(env.driver.handleRequest("exit").kind, "ask");
  env.driver.handleChoice("wait");
  env.gate.activate("wt-b");
  const preview = deferred();
  let activeLease;
  const finished = finishCommitAndContinue(origin, true, (needsVerification) => {
    env.writing.delete("wt-a");
    env.orchestrator.end("wt-a", needsVerification);
  }, async (target, lease) => {
    activeLease = lease;
    assert.equal(env.writing.has("wt-a"), true);
    assert.equal(env.coordinator.hasWriting(), true);
    const next = await preparePush(target, {
      preview: async (id) => { assert.equal(id, "wt-a"); return preview.promise; },
      current: () => env.gate.token(), showDialog: () => false,
    });
    const result = await runRemoteWrite(deps, {
      kind: "push", worktreeId: next.origin.worktreeId, request: next.request,
    }, lease);
    assert.equal(result.ok, true, "不能被自己的在途提交拒绝");
  });
  await flush();
  assert.deepEqual(env.native, [], "等待预览期间也不能放行退出");
  assert.equal(env.writing.has("wt-a"), true);
  preview.resolve(pushPreview);
  await flush();
  assert.equal(intents[0].worktreeId, "wt-a");
  assert.deepEqual(env.native, [], "推送仍在途时不能退出");
  env.pending[0].resolve({ kind: "pushed" });
  await finished;
  await flush();
  assert.deepEqual(env.coordinator.writingWorktrees(), []);
  assert.deepEqual(env.native, ["exit"]);
  assert.equal(activeLease.active, false);
  const late = await runRemoteWrite(deps, { kind: "fetch", worktreeId: "wt-a" }, activeLease);
  assert.equal(late.ok, false, "释放后不能继续复用原写入租约");
  assert.equal(intents.length, 1);
});

test("续推超时把待核对交回原提交生命周期，不能静默退出", async () => {
  const env = makeEnv();
  const { deps, intents } = remoteDeps(env);
  const origin = { worktreeId: "wt-a", activation: 1 };
  env.writing.add("wt-a");
  await env.orchestrator.begin("wt-a");
  env.driver.handleRequest("exit");
  env.driver.handleChoice("wait");
  const finished = finishCommitAndContinue(origin, true, (needsVerification) => {
    env.writing.delete("wt-a");
    env.orchestrator.end("wt-a", needsVerification);
  }, async (_target, lease) => {
    const running = runRemoteWrite(deps, { kind: "push", worktreeId: "wt-a", request: pushRequest }, lease);
    const wrongTarget = await runRemoteWrite(deps, { kind: "fetch", worktreeId: "wt-b" }, lease);
    assert.equal(wrongTarget.ok, false);
    const concurrent = await runRemoteWrite(deps, { kind: "fetch", worktreeId: "wt-a" }, lease);
    assert.equal(concurrent.ok, false, "租约内部同样不并发或积压点击");
    assert.equal((await running).ok, false);
  });
  await flush();
  env.pending[0].reject({ kind: "needsVerification", message: "网络超时" });
  await finished;
  await flush();
  assert.equal(intents.length, 1);
  assert.deepEqual(env.refreshed, ["wt-a"]);
  assert.deepEqual(env.coordinator.pendingWorktrees(), ["wt-a"]);
  assert.deepEqual(env.native, []);
});

test("提交并推送需确认时保留窗口，不用提交完成越过待确认操作退出", async () => {
  for (const intent of ["close", "exit"]) {
    const env = makeEnv();
    env.gate.activate("wt-a");
    const origin = captureRemoteTarget(env.gate.token());
    env.writing.add("wt-a");
    await env.orchestrator.begin("wt-a");
    env.driver.handleRequest(intent);
    env.driver.handleChoice("wait");
    let dialog;
    await finishCommitAndContinue(origin, true, (needsVerification) => {
      env.writing.delete("wt-a");
      env.orchestrator.end("wt-a", needsVerification);
    }, async (target) => {
      dialog = await preparePush(target, {
        preview: async () => pushPreview,
        current: () => env.gate.token(), showDialog: () => true,
        onConfirmationRequired: () => env.driver.handleChoice("stay"),
      });
    });
    await flush();
    assert.equal(dialog.kind, "dialog");
    assert.deepEqual(env.native, []);
    assert.equal(env.coordinator.pendingIntent(), null);
    assert.equal(env.writing.size, 0, "不把写锁跨用户确认无限持有");
    assert.equal(env.driver.currentPhase(), "idle");
  }
  const app = readFileSync(new URL("App.tsx", sourceRoot), "utf8");
  assert.match(app, /onConfirmationRequired: \(\) => \{\s*if \(afterCommit && closeCoordinatorRef\.current\.pendingIntent\(\) !== null\)/);
  assert.match(app, /applyEffect\(closeDriver\.handleChoice\("stay"\)\)/);
});

test("失败、取消或普通提交没有共享的待推送标记，不会续到另一笔提交", async () => {
  const calls = [];
  const push = async (origin) => calls.push(["push", origin.worktreeId]);
  await finishCommitAndContinue({ worktreeId: "wt-a", activation: 1 }, false, () => calls.push(["release", "wt-a"]), push);
  await finishCommitAndContinue({ worktreeId: "wt-b", activation: 2 }, false, () => calls.push(["release", "wt-b"]), push);
  await finishCommitAndContinue({ worktreeId: "wt-c", activation: 3 }, true, () => calls.push(["release", "wt-c"]), push);
  assert.deepEqual(calls, [["release", "wt-a"], ["release", "wt-b"], ["push", "wt-c"], ["release", "wt-c"]]);
  const app = readFileSync(new URL("App.tsx", sourceRoot), "utf8");
  assert.doesNotMatch(app, /commitAndPushRequestedRef|pushUpdatedOnceRef/);
  assert.match(app, /await finishCommitAndContinue\(origin, pushAfterCommit/);
  assert.match(app, /pushAfter: pending\.pushAfter/);
});

test("远端快捷键不穿透输入、按钮或模态，只为提交信息框保留提交并推送", () => {
  const normalFocus = { textOrDialog: false, button: false, commitMessage: false, dialogOpen: false };
  const normalEvent = { code: "KeyK", altKey: false, shiftKey: false, defaultPrevented: false, repeat: false, isComposing: false };
  for (const [event, action] of [
    [{ ...normalEvent, shiftKey: true }, "push"],
    [{ ...normalEvent, altKey: true }, "commitAndPush"],
    [{ ...normalEvent, code: "KeyT" }, "update"],
  ]) {
    assert.equal(remoteShortcutAction(event, normalFocus), action);
    assert.equal(remoteShortcutAction(event, { ...normalFocus, textOrDialog: true }), null);
    assert.equal(remoteShortcutAction(event, { ...normalFocus, button: true }), null);
    assert.equal(remoteShortcutAction(event, { ...normalFocus, dialogOpen: true }), null);
    assert.equal(remoteShortcutAction({ ...event, defaultPrevented: true }, normalFocus), null);
    assert.equal(remoteShortcutAction({ ...event, isComposing: true }, normalFocus), null);
    assert.equal(remoteShortcutAction(event, { ...normalFocus, textOrDialog: true, commitMessage: true }),
      action === "commitAndPush" ? action : null);
    assert.equal(remoteShortcutAction(event, { ...normalFocus, textOrDialog: true, commitMessage: true, dialogOpen: true }), null);
  }
  // Option+K 的 event.key 在 macOS 可能是特殊字符，物理 KeyK 仍然识别。
  assert.equal(remoteShortcutAction({ ...normalEvent, key: "˚", altKey: true }, normalFocus), "commitAndPush");
  const app = readFileSync(new URL("App.tsx", sourceRoot), "utf8");
  assert.match(app, /remoteShortcutAction\(event, \{/);
  assert.match(app, /textOrDialog: isTextInputOrDialog\(event\.target\)/);
  assert.match(app, /dialogOpen: document\.querySelector/);
});

test("推送被拒：自动更新只做一次，之后不再重试", () => {
  assert.equal(planAfterPushRejected({ autoUpdate: false, alreadyAttempted: false }), "stop");
  assert.equal(planAfterPushRejected({ autoUpdate: true, alreadyAttempted: false }), "update-then-push");
  assert.equal(planAfterPushRejected({ autoUpdate: true, alreadyAttempted: true }), "stop");
});

test("更新与推送结果的通知文案覆盖每种状态", () => {
  const base = {
    mode: "merge", upstream: "refs/remotes/origin/main", commitsMerged: 0,
    conflicts: [], recoveryRecordId: null, stashKept: null, output: "",
  };
  assert.equal(updateNotice({ ...base, status: "upToDate" }).tone, "success");
  assert.equal(updateNotice({ ...base, status: "updated", commitsMerged: 3 }).tone, "success");
  assert.equal(updateNotice({ ...base, status: "mergeConflict", conflicts: ["a.txt"] }).tone, "warning");
  assert.equal(updateNotice({ ...base, status: "rebaseConflict", conflicts: ["a.txt"] }).tone, "warning");
  assert.match(updateNotice({ ...base, status: "autostashConflict", stashKept: "stash@{0}" }).title(), /stash|冲突/);
  assert.equal(pushNotice({ kind: "pushed", remote: "origin", branch: "main", commits: 2, upstreamRecorded: true, output: "" }).tone, "success");
  assert.equal(pushNotice({ kind: "rejected", remote: "origin", branch: "main", reason: "非快进", output: "" }).tone, "warning");
});

test("关闭编排与结果通道与合并写入共用同一实现", async () => {
  const env = makeEnv();
  const token = env.operations.begin("wt-a");
  const running = runGuardedWrite(
    { ...env.deps, execute: () => env.deps.execute({ worktreeId: "wt-a" }) },
    { worktreeId: "wt-a" },
  );
  await flush();
  // 较新的操作让旧结果失去记录资格，但仍真实重读。
  env.operations.begin("wt-a");
  assert.equal(env.operations.canRecord(token), false);
  env.pending[0].reject({ kind: "git", message: "失败" });
  await assert.rejects(running, { kind: "git" });
  assert.deepEqual(env.reports, []);
  assert.deepEqual(env.refreshed, ["wt-a"]);
});

test("新建分支失败时对话框显示后端（Git）给出的原因", () => {
  // 名字不合法：原样显示 Git 的拒绝原因。
  assert.equal(
    branchFailureDetail({ kind: "rejected", message: "分支名不合法：fatal: 'bad..name' is not a valid branch name" }),
    "分支名不合法：fatal: 'bad..name' is not a valid branch name",
  );
  // 没有消息时才退回通用提示。
  assert.match(branchFailureDetail({ kind: "internal", message: "   " }), /新建分支失败/);
  assert.match(branchFailureDetail(null), /新建分支失败/);
});

test("新建分支对话框的错误来自写编排的返回，不是通用提示", () => {
  const app = readFileSync(new URL("App.tsx", sourceRoot), "utf8");
  assert.match(app, /setNewBranchError\(outcome\.error\)/);
  assert.match(app, /error=\{newBranchError \? branchFailureDetail\(newBranchError\) : null\}/);
  // 统一编排把错误原样返回给调用方（对话框与通知共用同一个原因）。
  assert.match(app, /Promise<\{ ok: true; result: unknown \} \| \{ ok: false; error: AppErrorPayload \}>/);
});

test("检查远端只在开启 / 切换 worktree 时跑，引用变化不再联网", () => {
  const hook = readFileSync(new URL("features/branches/useBranches.ts", sourceRoot), "utf8");
  // 检查远端的 effect 不再依赖引用版本。
  assert.match(hook, /\}, \[checkRemote, worktreeId\]\);/);
  assert.doesNotMatch(hook, /\[checkRemote, refreshKey, worktreeId\]/);
});

function branchOverview(worktreeId, branch = "main", upstream = "refs/remotes/origin/main", commit = "aaa") {
  const headRef = branch ? `refs/heads/${branch}` : null;
  return {
    worktreeId, currentBranch: branch, headRef, detached: !branch,
    shortHead: branch ? null : commit, headCommit: commit,
    local: branch ? [{
      name: branch, fullName: headRef, commit, isCurrent: true, upstream,
      upstreamShort: upstream?.replace("refs/remotes/", "") ?? null,
      ahead: 0, behind: 0, upstreamGone: false, worktrees: [],
    }] : [],
    remotes: [], remoteNames: [], operation: null, user: { name: null, email: null },
  };
}

function observeBranches(state, overview) {
  assert.equal(state.observeOverview(state.capture(overview.worktreeId), overview), true);
}

test("远端标记：首次概览不使已发起的 fetch/update 清除凭据失效", async () => {
  const state = new IncomingState();
  state.select("wt-a", true);
  const completion = state.capture("wt-a");
  observeBranches(state, branchOverview("wt-a"));
  assert.equal(await state.check(async () => true), true);
  assert.equal(state.incoming, true);
  const pending = deferred();
  const checking = state.check(() => pending.promise);

  assert.equal(state.clear(completion), true, "首次身份建立不是切换分支");
  assert.equal(state.incoming, false);
  pending.resolve(true);
  assert.equal(await checking, false, "清除前的在途检查不能重新点亮标记");
  assert.equal(state.incoming, false);
});

test("远端标记：首次概览前的凭据仍在后续真实分支变化时失效", async () => {
  const state = new IncomingState();
  state.select("wt-a", true);
  const completion = state.capture("wt-a");
  observeBranches(state, branchOverview("wt-a"));
  const pending = deferred();
  const checking = state.check(() => pending.promise);

  observeBranches(state, branchOverview("wt-a", "topic", "refs/remotes/origin/topic"));
  assert.equal(await state.check(async () => true), true);
  assert.equal(state.clear(completion), false, "旧完成凭据不能清除新分支标记");
  pending.resolve(false);
  assert.equal(await checking, false);
  assert.equal(state.incoming, true);
  observeBranches(state, branchOverview("wt-a"));
  assert.equal(await state.check(async () => true), true);
  assert.equal(state.clear(completion), false, "切回原分支也不恢复旧凭据资格");
  assert.equal(state.incoming, true);
});

test("远端标记：旧 worktree 的检查与清除都不能改变当前 worktree", async () => {
  const state = new IncomingState();
  state.select("wt-a", true);
  observeBranches(state, branchOverview("wt-a"));
  const clearA = state.capture("wt-a");
  const pending = deferred();
  const checkA = state.check((id) => {
    assert.equal(id, "wt-a");
    return pending.promise;
  });

  state.select("wt-b", true);
  observeBranches(state, branchOverview("wt-b"));
  assert.equal(await state.check(async () => true), true);
  assert.equal(state.incoming, true);
  assert.equal(state.capture("wt-a"), null);
  assert.equal(state.clear(clearA), false);
  pending.resolve(false);
  assert.equal(await checkA, false);
  assert.equal(state.incoming, true, "A 的迟到响应和完成回调都不能清掉 B");
  assert.equal(await state.check(async () => false), true);
  assert.equal(state.incoming, false, "当前目标的正常检查仍可清除标记");
});

test("远端标记：分支与上游变化取消旧检查，提交变化不改变检查身份", async () => {
  const state = new IncomingState();
  state.select("wt-a", true);
  observeBranches(state, branchOverview("wt-a"));
  assert.equal(await state.check(async () => true), true);
  const clearMain = state.capture("wt-a");
  const main = deferred();
  const checkingMain = state.check(() => main.promise);

  observeBranches(state, branchOverview("wt-a", "topic", "refs/remotes/origin/topic"));
  assert.equal(state.incoming, false);
  assert.equal(state.clear(clearMain), false);
  main.resolve(true);
  assert.equal(await checkingMain, false);
  assert.equal(state.incoming, false);

  const topic = deferred();
  const checkingTopic = state.check(() => topic.promise);
  observeBranches(state, branchOverview("wt-a", "topic", "refs/remotes/other/topic"));
  topic.resolve(true);
  assert.equal(await checkingTopic, false, "分支名相同而上游改变也应失效");

  const sameBranch = deferred();
  const checkingSameBranch = state.check(() => sameBranch.promise);
  observeBranches(state, branchOverview("wt-a", "topic", "refs/remotes/other/topic", "bbb"));
  sameBranch.resolve(true);
  assert.equal(await checkingSameBranch, true, "只改变 HEAD 提交不改变分支身份");
  assert.equal(state.incoming, true);
});

test("远端标记：fetch/update 完成清除后，在途检查不能重新点亮标记", async () => {
  const state = new IncomingState();
  state.select("wt-a", true);
  observeBranches(state, branchOverview("wt-a"));
  assert.equal(await state.check(async () => true), true);
  const completion = state.capture("wt-a");
  const pending = deferred();
  const checking = state.check(() => pending.promise);
  assert.equal(state.clear(completion), true);
  assert.equal(state.incoming, false);
  pending.resolve(true);
  assert.equal(await checking, false);
  assert.equal(state.incoming, false);
  assert.equal(await state.check(async () => true), true, "下一轮正常检查仍可更新标记");
  assert.equal(state.incoming, true);
});

test("远端标记：离开再返回同一 worktree 不恢复旧代际资格", async () => {
  const state = new IncomingState();
  state.select("wt-a", true);
  observeBranches(state, branchOverview("wt-a"));
  const old = state.capture("wt-a");
  const pending = deferred();
  const checking = state.check(() => pending.promise);
  state.select("wt-b", true);
  state.select("wt-a", true);
  assert.equal(state.observeOverview(old, branchOverview("wt-a")), false, "旧概览也不能复活");
  observeBranches(state, branchOverview("wt-a"));
  assert.equal(await state.check(async () => true), true);
  assert.equal(state.clear(old), false);
  pending.resolve(false);
  assert.equal(await checking, false);
  assert.equal(state.incoming, true);
});

test("远端标记：同一 worktree 切走再切回原分支不接受旧分支响应", async () => {
  const state = new IncomingState();
  state.select("wt-a", true);
  observeBranches(state, branchOverview("wt-a"));
  const old = state.capture("wt-a");
  const pending = deferred();
  const checking = state.check(() => pending.promise);
  observeBranches(state, branchOverview("wt-a", "topic", "refs/remotes/origin/topic"));
  observeBranches(state, branchOverview("wt-a"));
  pending.resolve(true);
  assert.equal(await checking, false);
  assert.equal(state.clear(old), false);
  assert.equal(state.incoming, false);
});

test("远端标记：关闭检查或卸载时取消请求，未就绪与无上游时不联网", async () => {
  const state = new IncomingState();
  let calls = 0;
  const run = async () => { calls += 1; return true; };
  state.select("wt-a", true);
  assert.equal(await state.check(run), false);
  observeBranches(state, branchOverview("wt-a", "main", null));
  assert.equal(await state.check(run), false);
  observeBranches(state, branchOverview("wt-a", null, null));
  assert.equal(await state.check(run), false);
  assert.equal(calls, 0);

  observeBranches(state, branchOverview("wt-a"));
  const disabled = deferred();
  const checkingDisabled = state.check(() => disabled.promise);
  state.select("wt-a", false);
  state.select("wt-a", true);
  disabled.resolve(true);
  assert.equal(await checkingDisabled, false);
  const cancelled = deferred();
  const checkingCancelled = state.check(() => cancelled.promise);
  state.cancelChecks();
  cancelled.resolve(true);
  assert.equal(await checkingCancelled, false);
  assert.equal(state.incoming, false);
});

test("Git 命令日志只有状态栏入口，顶栏没有“视图”下拉", () => {
  const shell = readFileSync(new URL("components/app/Shell.tsx", sourceRoot), "utf8");
  assert.doesNotMatch(shell, /viewMenu/, "顶栏不再有视图菜单（与 Rebased 一致）");
  // 状态栏入口保留。
  assert.match(shell, /onOpenCommandLog/);
  assert.match(shell, /messages\.commandLog\.open/);
  const app = readFileSync(new URL("App.tsx", sourceRoot), "utf8");
  assert.doesNotMatch(app, /viewMenu/);
  assert.match(app, /onOpenCommandLog=\{\(\) => setCommandLogOpen/);
});

test("共用写编排的注释指向实际存在的回归文件", () => {
  const source = readFileSync(new URL("lib/guardedWrite.ts", sourceRoot), "utf8");
  assert.match(source, /tools\/test-remote-write\.mjs/);
  assert.doesNotMatch(source, /test-guarded-write/);
  assert.ok(readFileSync(new URL("./test-remote-write.mjs", import.meta.url), "utf8").length > 0);
});
