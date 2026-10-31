import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { registerHooks } from "node:module";
import test from "node:test";
const sourceRoot = new URL("../src/", import.meta.url);
const imports = registerHooks({ resolve(specifier, context, nextResolve) {
  let source;
  if (specifier.startsWith("@/")) source = new URL(specifier.slice(2), sourceRoot);
  else if ((specifier.startsWith("./") || specifier.startsWith("../")) && context.parentURL?.startsWith(sourceRoot.href)) source = new URL(specifier, context.parentURL);
  if (source) { if (!/\.[^/]+$/.test(source.pathname)) source.pathname += ".ts"; return nextResolve(source.href, context); }
  return nextResolve(specifier, context);
}});
const { EditSessions } = await import("../src/lib/m7/editSession.ts");
const { runEditWrite } = await import("../src/lib/m7/editWrite.ts");
const { formatAppError, toAppError } = await import("../src/lib/error.ts");
const { setLocale } = await import("../src/lib/i18n/index.ts");
const { CloseCoordinator, CloseDriver, WriteGuardOrchestrator } = await import("../src/lib/refreshGate.ts");
imports.deregister();
function deferred() {
  let resolve, reject;
  const promise = new Promise((ok, fail) => { resolve = ok; reject = fail; });
  return { promise, resolve, reject };
}
const tick = () => new Promise((resolve) => setImmediate(resolve));
const target = (worktreeId = "A", path = "a.txt") => ({ worktreeId, path });
const key = (value) => JSON.stringify([value.worktreeId, value.path]);
function fixture() {
  const disks = new Map(), drafts = new Map(), reports = [], requests = [];
  const file = (t) => disks.get(key(t)) ?? { ...t, worktreeIdentity: `identity-${t.worktreeId}`, text: "disk\n", baseSha256: "sha-disk", readonlyReason: null, executable: false, newline: "lf", hasBom: false, sizeBytes: 5 };
  let persist = async (draft) => { drafts.set(key(draft), { ...draft }); };
  let list = async (id) => [...drafts.values()].filter((d) => d.worktreeId === id);
  let save = async (request) => {
    requests.push(request);
    const opened = file(request);
    if (request.baseSha256 !== opened.baseSha256) throw { kind: "rejected", message: "disk changed", code: "errors.m7.editFileChangedOnDisk" };
    const sha256 = `saved-${requests.length}`;
    disks.set(key(request), { ...opened, text: request.text, baseSha256: sha256 });
    return { path: request.path, sha256, bytes: request.text.length, executable: false, newline: "lf" };
  };
  const registry = new EditSessions({ open: async (t) => file(t), loadDraft: async (t) => drafts.get(key(t)) ?? null,
    listDrafts: (id) => list(id), persist: (d) => persist(d), clear: async (t) => { drafts.delete(key(t)); },
    save: (r) => save(r), blocked: () => false, report: (t, error) => reports.push({ target: t, error }) });
  return { registry, drafts, reports, requests,
    setDisk(t, text, sha, identity) { disks.set(key(t), { ...file(t), text, baseSha256: sha, ...(identity ? { worktreeIdentity: identity } : {}) }); },
    setPersist(fn) { persist = fn; }, setList(fn) { list = fn; }, setSave(fn) { save = fn; } };
}
test("恢复外部变化前的真草稿，保留旧基线与两版；磁盘二次变化必须再次核对", async () => {
  const f = fixture(), t = target();
  f.drafts.set(key(t), { ...t, worktreeIdentity: "identity-A", text: "my draft\n", baseSha256: "original", updatedAtMs: 1 });
  const session = f.registry.get(t); await session.open();
  assert.equal(session.snapshot().text, "my draft\n"); assert.equal(session.snapshot().diskText, "disk\n");
  assert.equal(session.snapshot().baseSha, "original"); assert.equal(session.snapshot().conflict, true);
  assert.equal(await session.save(), false); assert.equal(f.requests.length, 0);
  f.setDisk(t, "newer disk\n", "newer");
  assert.equal(await session.acceptDiskBaseline(), false); assert.equal(session.snapshot().diskText, "newer disk\n");
  assert.equal(session.snapshot().text, "my draft\n"); assert.equal(session.snapshot().baseSha, "original");
  assert.equal(await session.acceptDiskBaseline(), true); assert.equal(f.requests.length, 0, "核对不是自动覆盖");
  assert.equal(await session.save(), true); assert.equal(f.requests[0].baseSha256, "newer");
  assert.equal(f.requests[0].text, "my draft\n"); assert.equal(session.hasWork, false);
});
test("在途草稿写之后放弃且卸载冲刷，不得把已放弃内容重建", async () => {
  const f = fixture(), session = f.registry.get(target()); await session.open(); session.change("draft\n");
  const gate = deferred(); let persisted = 0;
  f.setPersist(async (draft) => { persisted++; await gate.promise; f.drafts.set(key(draft), draft); });
  const flushing = session.flushDraft(); await tick();
  const discarding = session.discard(); const unmountFlush = session.flushDraft();
  gate.resolve(); await flushing; assert.equal(await discarding, true); await unmountFlush;
  await session.flushDraft(); assert.equal(persisted, 1); assert.equal(f.drafts.size, 0); assert.equal(session.hasWork, false);
});
test("隐藏A仍受保护，B干净不解除A；保存改变磁盘通过saved让旧暂存动作停止", async () => {
  const f = fixture(), a = f.registry.get(target()), b = f.registry.get(target("B"));
  await a.open(); await b.open(); a.change("new A\n");
  assert.deepEqual(await f.registry.resolve(["B"], null, async () => { throw Error("B不应询问"); }), { ok: true, saved: false });
  assert.equal(f.registry.hasWork, true); assert.deepEqual(f.registry.worktreeIds, ["A", "B"]);
  const cancel = await f.registry.resolve(["A"], null, async (session) => { assert.equal(session, a); return false; });
  assert.equal(cancel.ok, false); assert.equal(a.snapshot().text, "new A\n");
  const saved = await f.registry.resolve(["A"], ["a.txt"], (session) => session.save());
  assert.deepEqual(saved, { ok: true, saved: true }); assert.equal(f.requests.length, 1);
});
test("持久化失败在原目标可见，决策可明确放弃；不能先无限重发失败写", async () => {
  const f = fixture(), t = target(), a = f.registry.get(t); await a.open(); a.change("important\n");
  let attempts = 0; f.setPersist(async () => { attempts++; throw Error("fixture storage unavailable"); });
  await assert.rejects(a.flushDraft()); assert.equal(f.reports[0].target.worktreeId, "A");
  assert.equal(a.hasWork, true); assert.equal(a.snapshot().text, "important\n");
  assert.deepEqual(await f.registry.resolve(["A"], null, (session) => session.discard()), { ok: true, saved: false });
  assert.equal(attempts, 1); assert.equal(a.hasWork, false);
});
test("持久化但未挂载的草稿也必须决策，取消保留原文", async () => {
  const f = fixture(), t = target("hidden");
  f.drafts.set(key(t), { ...t, worktreeIdentity: "identity-hidden", text: "stored\n", baseSha256: "sha-disk", updatedAtMs: 1 });
  let calls = 0;
  const result = await f.registry.resolve(["hidden"], null, async (session) => { calls++; assert.equal(session.snapshot().text, "stored\n"); return false; });
  assert.equal(calls, 1); assert.equal(result.ok, false); assert.equal(f.drafts.get(key(t)).text, "stored\n");
});
test("决策后等待磁盘列表时的新输入仍阻止退出，不能被handled漏掉", async () => {
  const f = fixture(), session = f.registry.get(target()); await session.open(); session.change("first\n");
  const gate = deferred(); f.setList(() => gate.promise);
  const result = f.registry.resolve(["A"], null, (value) => value.discard());
  await tick(); session.change("typed after decision\n"); gate.resolve([]);
  assert.equal((await result).ok, false); assert.equal(session.snapshot().text, "typed after decision\n");
});
test("同路径身份替换不能继承或保存旧草稿，仍可明确放弃旧身份", async () => {
  const f = fixture(), t = target(), session = f.registry.get(t); await session.open(); session.change("old worktree\n");
  await session.flushDraft(); f.setDisk(t, "replacement\n", "replacement", "identity-replaced"); await session.open();
  assert.equal(session.snapshot().identityValid, false); assert.equal(session.snapshot().text, "old worktree\n");
  assert.equal(await session.save(), false); assert.equal(f.requests.length, 0);
  assert.equal(await session.discard(), true); assert.equal(f.drafts.size, 0);
});
function writeFixture(execute) {
  const coordinator = new CloseCoordinator(); const native = []; const reports = []; const refreshes = [];
  const driver = new CloseDriver({ coordinator, releaseExitGuard: async () => {}, closeWindow: async () => native.push("close"),
    exitApp: async () => native.push("exit"), onError: () => {}, onAsk: () => {}, onSettled: () => {} });
  const orchestrator = new WriteGuardOrchestrator({ closeDriver: driver, closeBlockedReason: "closing", guardsReady: () => true,
    guardBlockedReason: () => "guard", register: (id) => coordinator.beginWrite(id), guard: async () => {},
    rollback: (id) => coordinator.rollbackWrite(id), reconcile: () => driver.notifyStateChanged(), endWrite: (id, pending) => coordinator.endWrite(id, pending) });
  let refresh = async () => true;
  const deps = { writing: new Set(), publishWriting: () => {}, begin: (id) => orchestrator.begin(id), end: (id, pending) => orchestrator.end(id, pending),
    beginOperation: (id) => ({ id }), canRecord: () => true, execute,
    refreshTarget: (id) => { refreshes.push(id); return refresh(id); }, reportFailure: (id, error) => reports.push({ id, error }),
    toError: toAppError, busyMessage: "busy", guardUnavailable: (reason) => reason };
  return { coordinator, driver, deps, native, reports, refreshes, setRefresh(fn) { refresh = fn; } };
}
test("实际保存通过现有写门登记原A，关闭可等待；迟到结果不改成B", async () => {
  const f = fixture(), gate = deferred(), called = [];
  const w = writeFixture((request) => { called.push(request); return gate.promise; });
  f.setSave((request) => runEditWrite(w.deps, request));
  const a = f.registry.get(target()); await a.open(); a.change("saved\n");
  const saving = a.save(); await tick();
  assert.deepEqual(w.coordinator.writingWorktrees(), ["A"]); assert.equal(w.driver.handleRequest("close").kind, "ask");
  const b = f.registry.get(target("B")); await b.open();
  gate.resolve({ path: "a.txt", sha256: "saved", bytes: 6, executable: false, newline: "lf" });
  assert.equal(await saving, true); assert.equal(called.length, 1); assert.equal(called[0].worktreeId, "A");
  assert.deepEqual(w.refreshes, ["A"]); assert.equal(b.snapshot().text, "disk\n"); assert.equal(w.coordinator.hasWriting(), false);
});
test("NeedsVerification保留真草稿与原目标，全局待核对阻止直接退出且不重发", async () => {
  const f = fixture(); let writes = 0;
  const w = writeFixture(async () => { writes++; throw { kind: "needsVerification", message: "fixture readback failed" }; });
  f.setSave((request) => runEditWrite(w.deps, request));
  const session = f.registry.get(target()); await session.open(); session.change("preserved\n");
  assert.equal(await session.save(), false); assert.equal(session.snapshot().text, "preserved\n");
  assert.equal(f.drafts.get(key(target())).text, "preserved\n"); assert.equal(session.snapshot().needsVerification, true);
  assert.deepEqual(w.coordinator.pendingWorktrees(), ["A"]); assert.equal(w.driver.handleRequest("exit").kind, "ask");
  assert.equal(await session.save(), false); assert.equal(writes, 1); assert.deepEqual(w.native, []);
});
test("保存后刷新返回false或抛错，均保留草稿并登记待核对", async () => {
  for (const failure of [false, Error("fixture refresh unavailable")]) {
    const f = fixture(), w = writeFixture(async () => ({ path: "a.txt", sha256: "written", bytes: 5, executable: false, newline: "lf" }));
    w.setRefresh(async () => { if (failure instanceof Error) throw failure; return failure; });
    f.setSave((request) => runEditWrite(w.deps, request));
    const session = f.registry.get(target()); await session.open(); session.change("draft\n");
    assert.equal(await session.save(), false); assert.equal(session.snapshot().needsVerification, true);
    assert.equal(f.drafts.size, 1); assert.deepEqual(w.coordinator.pendingWorktrees(), ["A"]);
  }
});
test("编辑cause按实际formatter双语显示原因", () => {
  for (const locale of ["zh-CN", "en"]) {
    setLocale(locale);
    for (const code of ["errors.m7.editWriteFailedKeepDraft", "errors.m7.editWriteUnverifiedCause"]) {
      const text = formatAppError({ kind: "needsVerification", code, params: {}, message: "fallback", cause: { kind: "environment", message: "fixture I/O failure" } });
      assert.ok(text.includes("fixture I/O failure")); assert.doesNotMatch(text, /\{(?:error|cause)\}/);
    }
  }
  setLocale("zh-CN");
});

test("启动发现持久草稿即建立全局保护，冻结后拒绝新输入", async () => {
  const f = fixture(), t = target("hidden");
  f.drafts.set(key(t), { ...t, worktreeIdentity: "identity-hidden", text: "stored\n", baseSha256: "sha-disk", updatedAtMs: 1 });
  await f.registry.discover(["hidden"]);
  assert.equal(f.registry.hasWork, true); assert.deepEqual(f.registry.worktreeIds, ["hidden"]);
  assert.equal((await f.registry.resolve(["hidden"], null, (session) => {
    assert.equal(session.snapshot().loading, false); assert.ok(session.snapshot().file, "发现的草稿须先打开，才能选择保存");
    return session.discard();
  })).ok, true);
  const session = f.registry.get(t); await session.open();
  f.registry.setClosing(true); session.change("late input");
  assert.equal(session.snapshot().text, "disk\n"); assert.equal(await session.save(), false);
});

test("真实Rust保存权限失败载荷经生产formatter中英均包含内层原因", { skip: !process.env.REVIA_EDIT_ERROR_EVIDENCE }, () => {
  const raw = readFileSync(process.env.REVIA_EDIT_ERROR_EVIDENCE, "utf8");
  const payload = JSON.parse(raw.match(/^EDIT_IO_PAYLOAD_JSON=(.+)$/m)[1]);
  assert.equal(payload.kind, "needsVerification"); assert.ok(payload.cause);
  for (const locale of ["zh-CN", "en"]) {
    setLocale(locale);
    const actual = formatAppError(payload), cause = formatAppError(payload.cause);
    assert.ok(actual.includes(cause)); assert.doesNotMatch(actual, /\{(?:error|cause)\}/);
    assert.notEqual(actual, payload.code); assert.ok(cause.length > 0);
  }
  setLocale("zh-CN");
});
