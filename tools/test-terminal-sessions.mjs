import assert from "node:assert/strict";
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
const { TerminalSessions } = await import("../src/lib/m7/terminalSessions.ts");
const { CloseCoordinator, CloseDriver } = await import("../src/lib/refreshGate.ts");
imports.deregister();
function deferred() { let resolve, reject; const promise = new Promise((ok, fail) => { resolve = ok; reject = fail; }); return { promise, resolve, reject }; }
const tick = () => new Promise((resolve) => setImmediate(resolve));
const info = (id, worktreeId, cwd = `/${worktreeId}`) => ({ sessionId: id, worktreeId, worktreePath: `/${worktreeId}`, cwd, title: `shell-${worktreeId}`, shell: "/fixture/shell", rows: 24, cols: 80, running: true, exitCode: null });
const record = (worktreeId, cwd = `/${worktreeId}`, order = 0) => ({ worktreeId, cwd, title: `shell-${worktreeId}`, order });
function fixture(records = [], live = []) {
  const sessions = new Map(live.map((s) => [s.sessionId, s])), calls = [], killed = [], writes = [], reported = [], activity = [], saves = [], busy = new Set();
  let next = 100, stored = records, open = async (id, cwd) => info(next++, id, cwd ?? `/${id}`), save = async (tabs) => { stored = tabs; saves.push(tabs); }, close = async (id) => { killed.push(id); sessions.delete(id); };
  const deps = { list: async () => [...sessions.values()], loadTabs: async () => stored, saveTabs: (tabs) => save(tabs),
    open: async (id, cwd) => { calls.push({ id, cwd }); const result = await open(id, cwd); sessions.set(result.sessionId, result); return result; },
    close: (id) => close(id), write: async (id, data) => { writes.push({ id, data }); }, resize: async () => {},
    blocked: (id) => busy.has(id), report: (id, error) => reported.push({ id, error }), activity: (id) => activity.push(id) };
  return { registry: new TerminalSessions(deps), deps, sessions, calls, killed, writes, reported, activity, saves, busy,
    get stored() { return stored; }, setOpen(fn) { open = fn; }, setSave(fn) { save = fn; }, setClose(fn) { close = fn; } };
}
test("A在途打开晚于B完成，永远归A；持久化保留B和未显示的C页签", async () => {
  const f = fixture([record("C")]), a = deferred();
  f.setOpen((id) => id === "A" ? a.promise : Promise.resolve(info(2, id)));
  const pending = f.registry.open("A", null); await tick(); await f.registry.open("B", null);
  a.resolve(info(1, "A")); await pending; await f.registry.flush();
  assert.deepEqual(f.registry.list("A").map((s) => s.sessionId), [1]); assert.deepEqual(f.registry.list("B").map((s) => s.sessionId), [2]);
  assert.deepEqual(f.stored.map((s) => s.worktreeId).sort(), ["A", "B", "C"]); assert.equal(f.registry.needsExitGuard, true);
});
test("重复恢复认领既有真实会话而不另开，按cwd补齐缺少的页签", async () => {
  const f = fixture([record("A", "/A/one"), record("A", "/A/two", 1), record("B")], [info(1, "A", "/A/two")]);
  await Promise.all([f.registry.restore("A"), f.registry.restore("A")]);
  assert.deepEqual(f.calls, [{ id: "A", cwd: "/A/one" }]); assert.equal(f.registry.list("A").length, 2);
  assert.ok(f.stored.some((s) => s.worktreeId === "B")); await f.registry.restore("A"); assert.equal(f.calls.length, 1);
});
test("隐藏A和B仍共同参与退出确认；取消不杀，准备成功也等最终执行才关闭", async () => {
  const f = fixture([], [info(1, "A"), info(2, "B")]); await f.registry.initialize();
  assert.equal(await f.registry.prepareShutdown(async (sessions) => { assert.deepEqual(sessions.map((s) => s.worktreeId), ["A", "B"]); return false; }), false);
  assert.deepEqual(f.killed, []); assert.equal(f.registry.isClosing, false);
  assert.equal(await f.registry.prepareShutdown(async () => true), true); assert.deepEqual(f.killed, []);
  f.registry.cancelShutdown(); assert.deepEqual(f.killed, [], "Git关闭确认取消后，原终端仍保留");
  assert.equal(await f.registry.prepareShutdown(async () => true), true); await f.registry.finishShutdown();
  assert.deepEqual(f.killed, [1, 2]);
});
test("退出准备等待已发打开并冻结新终端，晚回A仍在确认清单中", async () => {
  const f = fixture(), opened = deferred(); f.setOpen(() => opened.promise);
  const pending = f.registry.open("A", null); await tick();
  const shutdown = f.registry.prepareShutdown(async (sessions) => { assert.deepEqual(sessions.map((s) => s.sessionId), [7]); return false; });
  assert.equal(await f.registry.open("B", null), null); assert.equal(f.calls.length, 1);
  opened.resolve(info(7, "A")); await pending; assert.equal(await shutdown, false); assert.deepEqual(f.killed, []);
});
test("运行中页签必须显式确认才能关闭；后端关闭失败保留原页签与目标错误", async () => {
  const f = fixture([], [info(1, "A")]); await f.registry.initialize();
  await assert.rejects(f.registry.close(1), { kind: "rejected" }); assert.deepEqual(f.killed, []);
  f.setClose(async () => { throw Error("fixture kill failed"); });
  await assert.rejects(f.registry.close(1, true)); assert.equal(f.registry.list("A").length, 1);
  assert.equal(f.reported.at(-1).id, "A");
});
test("所属A忙碌只阻止A输入和新建，不误阻B；错误与输出活动保留原目标", async () => {
  const f = fixture([], [info(1, "A"), info(2, "B")]); await f.registry.initialize(); f.busy.add("A");
  await assert.rejects(f.registry.input(1, "user input"), { kind: "rejected" }); assert.equal(f.reported.at(-1).id, "A");
  assert.equal(await f.registry.open("A", null), null); await f.registry.input(2, "B input");
  assert.deepEqual(f.writes, [{ id: 2, data: "B input" }]);
  f.registry.output({ sessionId: 1, data: "fixture output" }); assert.deepEqual(f.activity, ["A"]);
});
test("退出事件先于打开回执仍保留真实已退出状态，旧list不复活已关闭会话", async () => {
  const f = fixture(), gate = deferred(); f.setOpen(() => gate.promise);
  const pending = f.registry.open("A", null); await tick();
  f.registry.exitedSession({ sessionId: 8, exitCode: 3 }); gate.resolve(info(8, "A")); await pending;
  assert.equal(f.registry.list("A")[0].running, false); assert.equal(f.registry.list("A")[0].exitCode, 3);
  await f.registry.close(8); f.sessions.set(8, info(8, "A")); await f.registry.refresh();
  assert.equal(f.registry.list("A").length, 0);
});
test("恢复或持久化失败不以空列表覆盖旧页签，最终关闭flush失败不杀进程", async () => {
  const f = fixture([record("A")]); f.setOpen(async () => { throw Error("fixture shell unavailable"); });
  await f.registry.restore("A"); assert.deepEqual(f.stored, [record("A")]); assert.equal(f.saves.length, 0);
  f.setOpen(async () => info(1, "A")); f.setSave(async () => { throw Error("fixture appdata unavailable"); });
  await f.registry.open("A", null); assert.ok(f.registry.errorFor("A"));
  assert.equal(await f.registry.prepareShutdown(async () => true), true);
  await assert.rejects(f.registry.finishShutdown()); assert.deepEqual(f.killed, []);
});
test("确认退出保留页签元信息，新manager只新开shell，不重放旧输入或输出", async () => {
  const f = fixture(); await f.registry.open("A", "/A/nested"); await f.registry.input(100, "user-only input");
  assert.equal(await f.registry.prepareShutdown(async () => true), true); await f.registry.finishShutdown();
  assert.deepEqual(f.stored, [record("A", "/A/nested")]);
  const restarted = fixture(f.stored); await restarted.registry.restore("A");
  assert.deepEqual(restarted.calls, [{ id: "A", cwd: "/A/nested" }]); assert.deepEqual(restarted.writes, []);
  assert.ok(!JSON.stringify(f.stored).includes("user-only input"));
});

test("最后页签关闭后的持久化仍受原生退出保护，成功落盘后才解除", async () => {
  const f = fixture([], [{ ...info(1, "A"), running: false, exitCode: 0 }]); await f.registry.initialize();
  const saving = deferred(); f.setSave(() => saving.promise);
  const closing = f.registry.close(1); await tick();
  assert.equal(f.registry.list().length, 0); assert.equal(f.registry.needsExitGuard, true);
  saving.resolve(); await closing; await f.registry.flush(); assert.equal(f.registry.needsExitGuard, false);
});

test("较早list不能删除刚打开的B；新的真实空列表解除已消失进程的运行态", async () => {
  const f = fixture([], [info(1, "A")]); await f.registry.initialize();
  const list = deferred(); f.deps.list = () => list.promise;
  const refreshing = f.registry.refresh(); await f.registry.open("B", null);
  list.resolve([]); await refreshing; assert.equal(f.registry.list("B").length, 1);
  f.sessions.delete(1); f.deps.list = async () => [...f.sessions.values()]; await f.registry.refresh();
  assert.equal(f.registry.list("A").length, 0); assert.equal(f.registry.list("B").length, 1);
});
test("终端确认后仍由生产CloseDriver等Git写完成；Stay取消不会提前结束进程", async () => {
  const f = fixture([], [info(1, "A")]); await f.registry.initialize();
  const coordinator = new CloseCoordinator(), native = [];
  let driver;
  driver = new CloseDriver({ coordinator, releaseExitGuard: async () => {},
    closeWindow: async () => { await f.registry.finishShutdown(); native.push("close"); },
    exitApp: async () => { await f.registry.finishShutdown(); native.push("exit"); },
    onError: (error) => { throw error; }, onAsk: () => {}, onSettled: () => { if (driver.currentPhase() === "idle") f.registry.cancelShutdown(); } });
  coordinator.beginWrite("A");
  assert.equal(await f.registry.prepareShutdown(async () => true), true);
  assert.equal(driver.handleRequest("exit").kind, "ask"); driver.handleChoice("stay");
  assert.deepEqual(f.killed, []); assert.equal(f.registry.isClosing, false);
  assert.equal(await f.registry.prepareShutdown(async () => true), true);
  driver.handleRequest("exit"); driver.handleChoice("wait"); assert.deepEqual(f.killed, []);
  coordinator.endWrite("A", false); driver.notifyStateChanged(); await tick();
  assert.deepEqual(f.killed, [1]); assert.deepEqual(native, ["exit"]);
});
