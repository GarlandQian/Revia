import assert from "node:assert/strict";
import { existsSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import vm from "node:vm";
import React from "react";
import * as jsxRuntime from "react/jsx-runtime";
import { renderToStaticMarkup } from "react-dom/server";
import { registerHooks } from "node:module";
import test from "node:test";

const sourceRoot = new URL("../src/", import.meta.url);
const imports = registerHooks({
  load(url, context, nextLoad) {
    // Decorative icons use Vite's asset glob; they are outside this SSR text
    // check. Keep the production banner/dialog and formatting implementations.
    if (url === new URL("components/app/FileTypeIcon.tsx", sourceRoot).href) {
      return { format: "module", shortCircuit: true, source: "export function FileTypeIcon() { return null; }" };
    }
    if (url.startsWith(sourceRoot.href) && url.endsWith(".tsx")) {
      const code = readFileSync(new URL(url), "utf8");
      return { format: "module", shortCircuit: true, source: ts.transpileModule(code, {
        compilerOptions: { module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ESNext },
      }).outputText };
    }
    return nextLoad(url, context);
  },
  resolve(specifier, context, nextResolve) {
    let source;
    if (specifier.startsWith("@/")) {
      source = new URL(specifier.slice(2), sourceRoot);
    } else if ((specifier.startsWith("./") || specifier.startsWith("../"))
      && context.parentURL?.startsWith(sourceRoot.href)) {
      source = new URL(specifier, context.parentURL);
    }
    if (source) {
      if (!/\.[^/]+$/.test(source.pathname)) {
        const path = fileURLToPath(source);
        source.pathname += existsSync(path) && statSync(path).isDirectory() ? "/index.ts" : existsSync(path + ".tsx") ? ".tsx" : ".ts";
      }
      return nextResolve(source.href, context);
    }
    return nextResolve(specifier, context);
  },
});
const { messages, setLocale, t } = await import("../src/lib/i18n/index.ts");
const { closeGuardFailure, commitOutputListenerFailure, formatAppError, getErrorMessage, localizedError, resultNeedsVerification, toAppError } = await import("../src/lib/error.ts");
const { MOD_SYMBOLS, COMMIT_SHORTCUT_LABEL } = await import("../src/lib/platform.ts");
const { commitWarningText, stashFileStatusText, operationKindText } = await import("../src/lib/i18n/text.ts");
const { commitResultNotice } = await import("../src/hooks/useNotifications.ts");
const { NotificationCard } = await import("../src/components/app/Notifications.tsx");
const { LogPager } = await import("../src/features/log/logPager.ts");
const { HistoryRowsReader } = await import("../src/features/log/historyRows.ts");
const { updateNotice, pushNotice } = await import("../src/features/branches/remoteWrite.ts");
const { PatchRows } = await import("../src/features/diff/patchRows.ts");
const { readM6State } = await import("../src/features/m6/readState.ts");
const { OperationBanner } = await import("../src/features/m6/OperationBanner.tsx");
const { CloseCoordinator, CloseGuardGate, WriteGuardOrchestrator, RefreshCoordinator } = await import("../src/lib/refreshGate.ts");
const { runGuardedWrite } = await import("../src/lib/guardedWrite.ts");
const { savedAutostashNotice, rewriteOutcomeDetail, rewriteFailureNotice } = await import("../src/features/m6/notices.ts");
const { StatusBar } = await import("../src/components/app/Shell.tsx");
const { RowHeights } = await import("../src/features/diff/rowHeights.ts");
imports.deregister();

const fixture = process.env.REVIA_M6_ERROR_FIXTURE;

test("原生菜单的生产定义中英切换，保留系统编辑角色与受保护关闭入口", () => {
  const source = readFileSync(new URL("../src/lib/nativeMenu.ts", import.meta.url), "utf8");
  for (const isMac of [true, false]) {
    const module = { exports: {} }; const code = ts.transpileModule(source, {
      compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
    }).outputText;
    vm.runInNewContext(code, { module, exports: module.exports,
      require: name => name.includes("i18n") ? { t } : { isMac } });
    let closed = 0;
    for (const locale of ["zh-CN", "en"]) {
      setLocale(locale); const options = module.exports.nativeMenuOptions(() => { closed++; });
      const groups = options.items; const file = groups.find(item => item.text === t("nativeMenu.file"));
      const edit = groups.find(item => item.text === t("nativeMenu.edit"));
      assert.ok(file); assert.ok(edit); assert.equal(groups.some(item => item.text === "Revia"), isMac);
      assert.deepEqual(Array.from(edit.items, item => item.item), ["Undo", "Redo", "Separator", "Cut", "Copy", "Paste", "SelectAll"]);
      assert.equal(file.items[0].text, t("nativeMenu.closeWindow")); file.items[0].action();
      if (isMac) { assert.equal(file.items[1].text, t("nativeMenu.closeAll")); file.items[1].action(); }
      const labels = groups.flatMap(item => [item.text, ...item.items.map(child => child.text).filter(Boolean)]).join(" ");
      if (locale === "zh-CN") assert.match(labels, /文件.*关闭窗口.*编辑.*撤销/);
      else assert.doesNotMatch(labels, /[\u3400-\u4dbf\u4e00-\u9fff]/u);
    }
    assert.equal(closed, isMac ? 4 : 2);
  }
});

test("生产App写门禁等待自动读取并阻止写入阶段自刷新，释放后补读", async () => {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const start = app.indexOf("  const refreshCoordinator = useMemo(");
  const end = app.indexOf("\n  useEffect(() => () => refreshCoordinator.dispose()", start);
  const guardStart = app.indexOf("  const writeOrchestratorRef = useRef<");
  const guardEnd = app.indexOf("\n  const performWrite =", guardStart);
  const resume = app.split("\n").find(line => line.includes("useEffect(() => refreshCoordinator.resume()"));
  assert.ok(start >= 0 && end > start && guardStart >= 0 && guardEnd > guardStart && resume);
  let reads = 0; let release; let protectedWrites = 0;
  const writing = new Set(); const firstRead = new Promise(resolve => { release = resolve; });
  const coordinator = new CloseCoordinator();
  const context = { RefreshCoordinator, WriteGuardOrchestrator, localizedError,
    useMemo: factory => factory(), useRef: current => ({ current }), useEffect: effect => effect(),
    gateRef: { current: { current: () => ({ worktreeId: "owned-fixture" }) } },
    writingWorktreesRef: { current: writing }, writingWorktrees: writing, worktreeId: "owned-fixture",
    readAndAccept: async () => { if (++reads === 1) await firstRead; }, setAppError: assert.fail,
    closeDriver: { canStartWrite: () => true }, closeGuardsRef: { current: { ready: true } },
    closeCoordinatorRef: { current: coordinator }, reconcileCloseRef: { current: null },
    syncExitBlocked: async blocked => { assert.equal(blocked, true); protectedWrites++; },
  };
  vm.createContext(context);
  const execute = code => vm.runInContext(ts.transpileModule(code, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText, context);
  execute(app.slice(start, end) + app.slice(guardStart, guardEnd) + "\nglobalThis.flow = { refreshCoordinator, writeOrchestrator };");
  context.flow.refreshCoordinator.request(); writing.add("owned-fixture");
  let started = false; const begin = context.flow.writeOrchestrator.begin("owned-fixture").then(result => { started = result.ok; });
  context.flow.refreshCoordinator.request(); await Promise.resolve(); assert.equal(started, false);
  release(); await begin; assert.equal(started, true); assert.equal(protectedWrites, 1); assert.equal(reads, 1);
  context.flow.refreshCoordinator.request(); await Promise.resolve(); assert.equal(reads, 1);
  context.flow.writeOrchestrator.end("owned-fixture", false); writing.delete("owned-fixture"); execute(resume);
  await context.flow.refreshCoordinator.whenIdle(); assert.equal(reads, 2); assert.equal(coordinator.hasWriting(), false);
});

test("真实审阅同步错误在生产formatter中保留结构化原因并即时切换语言", () => {
  const payload = JSON.parse(readFileSync(fixture + ".review-sync.json", "utf8"));
  assert.equal(payload.code, "errors.review.failedSyncReviewMarksCause");
  assert.equal(payload.cause.code, "errors.error.io");
  for (const locale of ["en", "zh-CN", "en"]) {
    setLocale(locale); const text = formatAppError(payload);
    assert.ok(text.includes(payload.cause.params.error));
    if (locale === "en") {
      assert.match(text, /Failed to sync review marks: IO error:/);
      assert.doesNotMatch(text, /[\u3400-\u4dbf\u4e00-\u9fff]/u);
    } else assert.match(text, /同步审阅标记失败：IO 错误/);
  }
});

test("真实历史批次异常保留已完成失败待处理清单和双语通知", () => {
  const cases = JSON.parse(readFileSync(fixture + ".partial-history.json", "utf8"));
  assert.equal(cases.length, 4);
  for (const { outcome } of cases) {
    assert.equal(outcome.completed, false); assert.equal(resultNeedsVerification(outcome), true);
    for (const locale of ["en", "zh-CN", "en"]) {
      setLocale(locale); const detail = rewriteOutcomeDetail(outcome); const title = formatAppError(outcome.error);
      for (const oid of [...outcome.applied, outcome.failedOid, ...outcome.pending]) assert.ok(detail.includes(oid.slice(0, 8)));
      const html = renderToStaticMarkup(React.createElement(NotificationCard, {
        notice: { id: "real-history-receipt", tone: "warning", title, detail, persistent: true }, onClose() {},
      }));
      if (locale === "en") { assert.match(html, /Completed.*Stopped at.*Not processed yet/s); assert.doesNotMatch(html, /[\u3400-\u4dbf\u4e00-\u9fff]/u); }
      else assert.match(html, /已完成.*停在此提交.*尚未处理/s);
    }
  }
});

test("真实续接的回收收尾失败保留完成与跳过清单并显示双语错误", () => {
  const cases = JSON.parse(readFileSync(fixture + ".continue-receipts.json", "utf8"));
  assert.equal(cases.length, 4);
  for (const { outcome } of cases) {
    assert.equal(outcome.completed, false); assert.equal(resultNeedsVerification(outcome), true);
    const receipt = outcome.continuation;
    assert.equal(receipt.error.kind, "needsVerification");
    for (const locale of ["en", "zh-CN", "en"]) {
      setLocale(locale);
      const html = renderToStaticMarkup(React.createElement(NotificationCard, {
        notice: { id: "continue-receipt", tone: "warning", persistent: true,
          title: () => formatAppError(receipt.error), detail: () => rewriteOutcomeDetail(receipt) }, onClose() {},
      }));
      for (const oid of [...receipt.applied, ...receipt.skipped]) assert.ok(html.includes(oid.slice(0, 8)));
      if (locale === "en") { assert.match(html, /Completed:/); assert.doesNotMatch(html, /[\u3400-\u4dbf\u4e00-\u9fff]/u); }
      else assert.match(html, /已完成/);
    }
  }
});

test("生产外部工具启动回调独立报告迟到的设置保存失败且归属原目标", async () => {
  const { error, outcome } = JSON.parse(readFileSync(fixture + ".external-settings.json", "utf8"));
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const source = ts.createSourceFile("App.tsx", app, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  let callback;
  function visit(node) {
    if (ts.isJsxSelfClosingElement(node) && node.tagName.getText(source) === "ExternalToolDialog") {
      callback = node.attributes.properties.find(prop => prop.name?.getText(source) === "onLaunch")?.initializer.expression;
    }
    ts.forEachChild(node, visit);
  }
  visit(source); assert.ok(callback && ts.isArrowFunction(callback));
  const code = ts.transpileModule("globalThis.launch = " + callback.getText(source), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  for (const kind of ["diff", "merge"]) {
    const origin = { worktreeId: "owned-A", activation: 1 }; const notices = []; const patches = [];
    let rejectSave; let current = { origin }; let runs = 0;
    const context = { externalTool: { origin, kind }, gateRef: { current: { token: () => origin } },
      remoteTargetIsCurrent: () => true, toAppError, formatAppError, messages,
      ipc: { updateSettings: patch => { patches.push(patch); return new Promise((_, reject) => { rejectSave = reject; }); } },
      performM6Write: async (id, run) => { assert.equal(id, "owned-A"); return run(); },
      setExternalTool: update => { current = typeof update === "function" ? update(current) : update; },
      notifyM6: (target, notice) => notices.push({ target, notice }),
    };
    vm.createContext(context); vm.runInContext(code, context);
    await context.launch(async () => { runs++; current = { origin: { worktreeId: "owned-B", activation: 2 } }; return outcome; }, "fixturetool");
    assert.equal(runs, 1); assert.equal(patches.length, 1);
    assert.equal(patches[0][kind === "diff" ? "diffTool" : "mergeTool"], "fixturetool");
    assert.equal(notices[0].notice.tone, "success"); assert.equal(current.origin.worktreeId, "owned-B");
    rejectSave(error); await Promise.resolve(); await Promise.resolve();
    const warning = notices.find(item => item.notice.tone === "warning");
    assert.ok(warning); assert.equal(warning.target, "owned-A"); assert.equal(warning.notice.persistent, true);
    for (const locale of ["en", "zh-CN", "en"]) {
      setLocale(locale); const title = warning.notice.title();
      assert.ok(title.includes(error.params.error));
      if (locale === "en") { assert.match(title, /Failed to write settings/); assert.doesNotMatch(title, /[\u3400-\u4dbf\u4e00-\u9fff]/u); }
      else assert.match(title, /写入设置失败/);
    }
  }
});

test("生产历史与续接入口在真实写后重读错误下保留回执原归属和待核对保护", async () => {
  const readbacks = JSON.parse(readFileSync(fixture + ".history-readbacks.json", "utf8"));
  const continuations = JSON.parse(readFileSync(fixture + ".continue-receipts.json", "utf8"));
  assert.equal(readbacks.length, 2);
  const cases = [
    ...readbacks.map(item => ({ entry: "rewrite", result: item.outcome, readError: item.readError })),
    ...continuations.map(item => ({ entry: "operation", result: item.outcome, readError: readbacks[0].readError })),
  ];
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const workStart = app.indexOf("  async function performM6Write<");
  const operationStart = app.indexOf("\n  const performOperationAction", workStart);
  const operationEnd = app.indexOf("\n  const performAutostashRestore", operationStart);
  const rewriteStart = app.indexOf("  const runHistoryRewrite = useCallback(");
  const rewriteEnd = app.indexOf("\n\n  /** 从文件历史", rewriteStart);
  assert.ok(workStart >= 0 && operationStart > workStart && operationEnd > operationStart && rewriteEnd > rewriteStart);
  const code = ts.transpileModule(app.slice(workStart, operationEnd) + app.slice(rewriteStart, rewriteEnd)
    + "\nglobalThis.flow = { runHistoryRewrite, performOperationAction };", {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  for (const { entry, result, readError } of cases) {
    for (const mode of ["target-false", "target-throw", "operation-throw"]) {
      const origin = { worktreeId: "owned-A", activation: 1 }; const other = { worktreeId: "owned-B", activation: 2 };
      const notices = []; const errors = []; let writes = 0; let token = origin; let dialog = { origin }; let busy;
      const coordinator = new CloseCoordinator();
      const writeOrchestrator = new WriteGuardOrchestrator({ closeDriver: { canStartWrite: () => true }, closeBlockedReason: "unused",
        guardsReady: () => true, guardBlockedReason: () => null, guard: async () => {},
        register: id => coordinator.beginWrite(id), rollback: id => coordinator.rollbackWrite(id),
        endWrite: (id, pending) => coordinator.endWrite(id, pending), reconcile() {} });
      const operation = { kind: result.kind, metadataVersion: "owned-fixture-metadata", confirmationState: "owned-fixture-confirmation" };
      const reply = async () => { writes++; token = other; dialog = { origin: other }; return result; };
      const context = { writeOrchestrator, writingWorktreesRef: { current: new Set() },
        setWritingWorktrees() {}, setM6Busy: value => { busy = value; },
        setAppError: (failure, target) => errors.push({ failure, target }),
        refreshTarget: async id => {
          assert.equal(id, "owned-A");
          if (mode === "target-throw") throw readError;
          if (mode === "target-false") { errors.push({ failure: readError, target: id }); return false; }
          return true;
        },
        refreshM6State: async id => { assert.equal(id, "owned-A"); if (mode === "operation-throw") throw readError; },
        bumpRefsRevision() {}, setStashRevision() {}, useCallback: fn => fn,
        toAppError, localizedError, resultNeedsVerification, rewriteFailureNotice, rewriteOutcomeDetail,
        messages, formatAppError, operationKindText, gateRef: { current: { token: () => token } },
        remoteTargetIsCurrent: (expected, current) => expected.worktreeId === current.worktreeId && expected.activation === current.activation,
        operationState: operation, ipc: { operationExecute: reply },
        notifyAutostashSaved() {}, reportAutostash: () => false,
        setHistoryAction: update => { dialog = typeof update === "function" ? update(dialog) : update; },
        notifyM6: (target, notice) => notices.push({ target, notice }),
      };
      vm.createContext(context); vm.runInContext(code, context);
      if (entry === "rewrite") await context.flow.runHistoryRewrite(origin, reply);
      else await context.flow.performOperationAction(result.action, origin, operation);
      assert.equal(writes, 1); assert.equal(busy, false); assert.equal(context.writingWorktreesRef.current.size, 0);
      assert.deepEqual(coordinator.pendingWorktrees(), ["owned-A"]); assert.equal(dialog.origin.worktreeId, "owned-B");
      assert.equal(errors.at(-1).failure.kind, "needsVerification"); assert.equal(errors.at(-1).target, "owned-A");
      assert.equal(notices.length, 1); assert.equal(notices[0].target, "owned-A");
      const notice = notices[0].notice; assert.equal(notice.tone, "warning"); assert.equal(notice.persistent, true);
      const receipt = entry === "rewrite" ? result : result.continuation;
      const oids = [...receipt.applied, ...receipt.skipped, ...receipt.skippedEmpty, ...receipt.pending, ...(receipt.failedOid ? [receipt.failedOid] : [])];
      for (const locale of ["en", "zh-CN", "en"]) {
        setLocale(locale); const html = renderToStaticMarkup(React.createElement(NotificationCard, {
          notice: { ...notice, id: "real-write-readback" }, onClose() {},
        }));
        for (const oid of oids) assert.ok(html.includes(oid.slice(0, 8)));
        if (receipt.error) assert.ok(notice.detail().includes(formatAppError(receipt.error)));
        if (locale === "en") assert.doesNotMatch(html, /[\u3400-\u4dbf\u4e00-\u9fff]/u);
        else assert.match(html, /结果待核对/);
      }
    }
  }
});

test("真实stash部分清理失败经生产App收尾保留待核对关闭保护和双语原因", async () => {
  const cases = JSON.parse(readFileSync(fixture + ".stash-save-partial.json", "utf8"));
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const start = app.indexOf("  async function performM6Write<");
  const end = app.indexOf("\n  const performOperationAction", start);
  assert.ok(start >= 0 && end > start);
  const code = ts.transpileModule(app.slice(start, end), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  for (const { error } of cases) {
    const coordinator = new CloseCoordinator(); const errors = []; const refreshed = [];
    const flow = new WriteGuardOrchestrator({ closeDriver: { canStartWrite: () => true }, closeBlockedReason: "unused",
      guardsReady: () => true, guardBlockedReason: () => null, guard: async () => {},
      register: id => coordinator.beginWrite(id), rollback: id => coordinator.rollbackWrite(id),
      endWrite: (id, pending) => coordinator.endWrite(id, pending), reconcile() {} });
    const context = { writeOrchestrator: flow, writingWorktreesRef: { current: new Set() },
      setWritingWorktrees() {}, setM6Busy() {}, setAppError: (failure, owner) => errors.push({ failure, owner }),
      refreshTarget: async id => { refreshed.push(id); return true; }, refreshM6State: async () => {},
      toAppError, localizedError, resultNeedsVerification };
    vm.createContext(context); vm.runInContext(code, context);
    let writes = 0;
    await assert.rejects(context.performM6Write("owned-stash-fixture", async () => { writes++; throw error; }),
      failure => failure.kind === "needsVerification");
    assert.equal(writes, 1); assert.deepEqual(refreshed, ["owned-stash-fixture"]);
    assert.deepEqual(coordinator.pendingWorktrees(), ["owned-stash-fixture"]); assert.equal(coordinator.hasWriting(), false);
    const failure = errors.at(-1).failure; assert.equal(errors.at(-1).owner, "owned-stash-fixture");
    for (const locale of ["zh-CN", "en"]) {
      setLocale(locale); const text = formatAppError(failure);
      assert.ok(text.includes(error.cause.params.detail));
      if (locale === "en") assert.match(text, /Verify the result: Failed to save stash:/);
      else assert.match(text, /结果待核对.*保存 stash 失败/s);
    }
  }
});

test("真实原生监视失败在状态栏保留详情并即时切换同一错误的语言", () => {
  const status = JSON.parse(readFileSync(fixture + ".watch.json", "utf8"));
  assert.equal(status.error.code, "errors.watch.unavailable");
  const props = { branch: "owned-fixture", ahead: 0, behind: 0, watchError: status.error,
    reviewed: 0, total: 0, onRefresh() {}, onOpenCommandLog() {}, autoFetchError: null };
  let chinese;
  for (const locale of ["zh-CN", "en"]) {
    setLocale(locale); const message = formatAppError(status.error);
    const escaped = renderToStaticMarkup(React.createElement("span", { title: message })).match(/title="([^"]*)"/)[1];
    const html = renderToStaticMarkup(React.createElement(StatusBar, props));
    assert.ok(html.includes(`title="${escaped}"`)); assert.doesNotMatch(html, /\[object Object\]/);
    if (locale === "zh-CN") { assert.match(message, /自动刷新不可用/); chinese = message; }
    else { assert.match(message, /Auto-refresh is unavailable/); assert.notEqual(message, chinese); assert.doesNotMatch(message, /[\u3400-\u4dbf\u4e00-\u9fff]/u); }
  }
});

test("真实已发布引用变化拒绝载荷能在同一错误上切换中英", () => {
  const payload = JSON.parse(readFileSync(fixture + ".publication.json", "utf8"));
  assert.equal(payload.kind, "rejected");
  setLocale("zh-CN"); const chinese = formatAppError(payload); assert.match(chinese, /变化|确认/);
  setLocale("en"); const english = formatAppError(payload); assert.match(english, /changed|confirm/i);
  assert.doesNotMatch(english, /[\u3400-\u4dbf\u4e00-\u9fff]/u); assert.notEqual(chinese, english);
});

test("静止统一diff的memo行即时切语言且保留缓存、回调和滚动", () => {
  const slots = []; const scheduled = []; let cursor = 0;
  const equal = (a, b) => a && b && a.length === b.length && a.every((value, index) => Object.is(value, b[index]));
  const memoValue = (make, deps) => { const index = cursor++; if (!slots[index] || !equal(slots[index].deps, deps)) slots[index] = { value: make(), deps }; return slots[index].value; };
  const hooks = {
    memo: React.memo,
    useRef(value) { const index = cursor++; return slots[index] ??= { current: value }; },
    useState(initial) { const index = cursor++; slots[index] ??= { value: typeof initial === "function" ? initial() : initial }; return [slots[index].value, value => { slots[index].value = typeof value === "function" ? value(slots[index].value) : value; }]; },
    useMemo: memoValue, useCallback: (callback, deps) => memoValue(() => callback, deps),
    useEffect() { cursor++; }, useLayoutEffect() { cursor++; },
    useSyncExternalStore(_subscribe, snapshot) { cursor++; return snapshot(); },
    useImperativeHandle(ref, create) { cursor++; if (ref) ref.current = create(); },
  };
  const require = name => name === "react" ? hooks : name === "react-dom" ? { flushSync: run => run() }
    : name === "react/jsx-runtime" ? jsxRuntime
    : name === "@/lib/i18n/index" ? { messages, getLocale: () => locale }
    : name === "@/lib/theme" ? { codeFontStack: () => "monospace" }
    : name === "./rowHeights" ? { RowHeights }
    : name === "./useFontRevision" ? { useFontRevision: () => 0 }
    : name === "./hunkAction" ? { matchesHunkAction: () => true, segmentActionDisabled: () => false }
    : name.includes("codicon") ? { Codicon: () => null } : {};
  const module = { exports: {} };
  const code = ts.transpileModule(readFileSync(new URL("../src/features/diff/DiffView.tsx", import.meta.url), "utf8"), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  vm.runInNewContext(code, { module, exports: module.exports, require, console,
    requestAnimationFrame: callback => scheduled.push(callback), cancelAnimationFrame() {},
    window: { setTimeout: callback => scheduled.push(callback), clearTimeout() {} } });
  const action = { changes: "unstaged" };
  const cache = Array.from({ length: 100 }, (_, index) => index === 1
    ? { kind: "hunk", section: "@@ fixed @@", hunkAction: action }
    : { kind: "line", lineKind: "addition", oldLine: null, newLine: index, text: "cached bytes", segmentAction: index === 2 ? action : null });
  const snapshot = { version: 1, error: null };
  const rows = { rowCountValue: cache.length, epochValue: 1, subscribe() {}, noteDirection() {}, getSnapshot: () => snapshot, getRow: index => cache[index] };
  const highlights = { subscribe() {}, getSnapshot: () => snapshot, get() {} };
  const noop = () => {}; const handleRef = { current: null };
  const props = { worktreeId: "owned-fixture", entry: { path: "file", changes: "unstaged" }, view: "unified", fontSize: 13,
    rows, highlights, handleRef, onHunkAction: noop, onHunkDiscard: noop, onSegmentAction: noop, onSegmentDiscard: noop };
  let locale = "zh-CN";
  const renderRows = () => { cursor = 0; const tree = module.exports.DiffView(props); return tree.props.children[0].props.children.filter(Boolean); };
  setLocale(locale); renderRows(); handleRef.current.setScrollTop(19);
  while (scheduled.length) scheduled.shift()();
  for (const segmentActionsEnabled of [true, false]) {
    props.segmentActionsEnabled = segmentActionsEnabled;
    locale = "zh-CN"; setLocale(locale); const chinese = renderRows();
    const again = renderRows();
    assert.deepEqual(Object.keys(chinese[0].props).filter(key => !Object.is(chinese[0].props[key], again[0].props[key])), []);
    const before = chinese.map(row => renderToStaticMarkup(row)).join("");
    assert.match(before, /暂存/);
    locale = "en"; setLocale(locale); const english = renderRows();
    assert.equal(handleRef.current.getScrollTop(), 19);
    for (let index = 0; index < chinese.length; index++) {
      const old = chinese[index], next = english[index];
      assert.equal(old.type, next.type); assert.equal(old.type.compare, null); // React.memo's default shallow comparison.
      assert.deepEqual(Object.keys(old.props).filter(key => !Object.is(old.props[key], next.props[key])), ["locale"]);
      assert.equal(next.props.row, cache[next.props.index]);
    }
    const after = english.map(row => renderToStaticMarkup(row)).join("");
    assert.match(after, /Stage/); assert.doesNotMatch(after, /暂存|丢弃/);
    const buttons = english.flatMap(row => {
      const tree = row.type.type(row.props); const children = [tree]; const found = [];
      while (children.length) { const node = children.pop(); if (node?.type === "button") found.push(node); const nested = node?.props?.children; children.push(...(Array.isArray(nested) ? nested : [nested]).filter(Boolean)); }
      return found;
    });
    assert.ok(buttons.length >= 2);
    for (const button of buttons) assert.doesNotMatch(String(button.props.title ?? "") + String(button.props["aria-label"] ?? ""), /暂存|丢弃/);
  }
});

test("真实提交失败同时保留快照收尾原因并随语言呈现", () => {
  const outcome = JSON.parse(readFileSync(fixture + ".snapshot.json", "utf8"));
  assert.equal(outcome.success, false); assert.equal(outcome.needsVerification, true);
  assert.equal(outcome.verificationNote.code, "errors.commitFlow.snapshotFinishFailed");
  assert.ok(outcome.verificationNote.cause);
  setLocale("en"); const english = formatAppError(outcome.verificationNote);
  assert.match(english, /snapshot/i); assert.doesNotMatch(english, /[\u3400-\u4dbf\u4e00-\u9fff]/u);
  setLocale("zh-CN"); assert.match(formatAppError(outcome.verificationNote), /快照/);
});

test("真实历史编码限制与stash部分结果保留结构化说明", () => {
  const entry = JSON.parse(readFileSync(fixture + ".patch.json", "utf8"));
  assert.equal(entry.patch, ""); assert.equal(entry.patchError.code, "errors.fileHistory.patchUnavailable");
  const association = JSON.parse(readFileSync(fixture + ".association.json", "utf8"));
  const checkout = JSON.parse(readFileSync(fixture + ".checkout.json", "utf8"));
  assert.equal(resultNeedsVerification({ error: checkout }), true);
  setLocale("en");
  assert.match(formatAppError(entry.patchError), /lossless UTF-8/);
  assert.match(formatAppError(association), /not automatically restored/);
  assert.match(formatAppError(checkout), /cleanup of stash/);
  for (const payload of [entry.patchError, association, checkout]) {
    assert.doesNotMatch(formatAppError(payload), /[\u3400-\u4dbf\u4e00-\u9fff]/u);
  }
  setLocale("zh-CN"); assert.match(formatAppError(association), /未自动恢复/);
});

test("真实stash预览保留文件列表和可切换语言的编码限制", () => {
  const result = JSON.parse(readFileSync(fixture + ".stash-patch.json", "utf8"));
  assert.equal(result.patch, ""); assert.equal(result.files.length, 1);
  assert.equal(result.patchError.code, "errors.stash.patchUnavailable");
  setLocale("en"); assert.match(formatAppError(result.patchError), /stash diff is not lossless UTF-8/);
  setLocale("zh-CN"); assert.match(formatAppError(result.patchError), /仅保留文件列表/);
});

test("真实历史准备阶段中断保留待核对原因并可切换语言", () => {
  for (const action of ["cherry-pick", "revert"]) {
    const payload = JSON.parse(readFileSync(fixture + `.prepare-${action}.json`, "utf8"));
    assert.equal(payload.kind, "needsVerification");
    assert.equal(payload.cause.code, "errors.rewrite.stateChanged");
    assert.equal(resultNeedsVerification({ error: payload }), true);
    setLocale("en"); const english = formatAppError(payload);
    assert.match(english, /changed/); assert.doesNotMatch(english, /[\u3400-\u4dbf\u4e00-\u9fff]/u);
    setLocale("zh-CN"); assert.match(formatAppError(payload), /变化/);
  }
});

test("真实 stash 新分支部分回执按当前语言说明并保留待核对语义", () => {
  const result = JSON.parse(readFileSync(fixture + ".branch.json", "utf8"));
  assert.equal(result.branch, "partial-applied"); assert.equal(result.applied, false);
  assert.equal(resultNeedsVerification(result), true);
  setLocale("en"); assert.match(formatAppError(result.error), /Created and switched to branch partial-applied/);
  assert.doesNotMatch(formatAppError(result.error), /[\u3400-\u4dbf\u4e00-\u9fff]/u);
  setLocale("zh-CN"); assert.match(formatAppError(result.error), /已创建并切换分支/);
});

test("stash 文件状态和无归属通知不会固定为英文或当前目标", () => {
  const statuses = ["added", "deleted", "renamed", "copied", "typeChanged", "modified"];
  setLocale("zh-CN"); const chinese = statuses.map(stashFileStatusText); assert.ok(chinese.every(text => /[\u4e00-\u9fff]/u.test(text)));
  setLocale("en"); assert.ok(statuses.map(stashFileStatusText).every((text, i) => text !== chinese[i]));
  const original = "/owned/worktree A";
  const notice = { id: "late-a", tone: "success", title: () => messages.historyActions.completed, worktreePath: original };
  const html = renderToStaticMarkup(React.createElement(NotificationCard, { notice, onClose() {} }));
  assert.ok(html.includes(original)); assert.ok(html.includes('title="' + original + '"'));
});

test("真实 hook 失败保留路径数据，已有错误可即时切换中英说明", () => {
  const payload = JSON.parse(readFileSync(fixture + ".hook.json", "utf8"));
  assert.equal(payload.related[0].code, "errors.commitFlow.hookChangedFiles");
  assert.equal(payload.related[0].params.worktree, "file.txt");
  setLocale("en");
  assert.match(formatAppError(payload), /Files changed by hooks/);
  assert.doesNotMatch(formatAppError(payload), /[\u3400-\u4dbf\u4e00-\u9fff]/u);
  setLocale("zh-CN");
  assert.match(formatAppError(payload), /hooks 修改过的文件/);
});

test("关联异常独立呈现，不屏蔽真实 Git 操作；缺失 stash 禁用恢复", async () => {
  const { operation, pending } = JSON.parse(readFileSync(fixture + ".pending.json", "utf8"));
  const next = await readM6State(Promise.resolve(operation), Promise.reject(pending.error));
  assert.deepEqual(next.operation, operation);
  assert.equal(next.pending, null);
  assert.equal(next.readErrors[0].code, pending.error.code);
  const inverse = await readM6State(Promise.reject(pending.error), Promise.resolve(pending));
  assert.deepEqual(inverse.pending, pending);
  setLocale("en");
  const html = renderToStaticMarkup(React.createElement(OperationBanner, {
    state: operation, origin: null, autostash: pending.link, autostashError: pending.error,
    canRestoreAutostash: !!pending.confirmationState, readErrors: next.readErrors, busy: false,
    onAction() {}, onRestoreAutostash() {}, onViewStash() {}, onKeepStash() {},
  }));
  assert.ok(html.includes(messages.operation.inProgress(messages.operation.kinds.merge)));
  assert.match(html, /role="alert"/);
  assert.ok(html.includes('disabled="">' + messages.operation.restoreAutostash + '</button>'));
});

test("platform shortcuts have their actual modifiers after each language switch", () => {
  for (const locale of ["zh-CN", "en"]) {
    setLocale(locale);
    const text = [messages.changes.shortcuts.stageFile, messages.changes.shortcuts.review,
      messages.changes.noSelectionDetail, messages.diff.previousHunkShortcut, messages.settings.fontSizeHint, messages.settings.uiZoomHint].join("\n");
    assert.doesNotMatch(text, /\{(?:shortcut|MOD_SYMBOLS|primary|shift|control|alt|commitShortcut|reviewShortcut)\}/u);
    assert.ok(messages.diff.previousHunkShortcut.includes(MOD_SYMBOLS.shift + "F7"));
    assert.ok(messages.changes.noSelectionDetail.includes(COMMIT_SHORTCUT_LABEL));
    assert.ok(messages.settings.uiZoomHint.includes(MOD_SYMBOLS.control + MOD_SYMBOLS.alt + "="));
  }
});

test("real Rust related failures remain translatable and mark structured write receipts uncertain", () => {
  const { related, nested, git } = JSON.parse(readFileSync(fixture, "utf8"));
  assert.equal(related.related.length, 1);
  for (const locale of ["zh-CN", "en"]) {
    setLocale(locale);
    const formatted = formatAppError(toAppError(related));
    assert.ok(formatted.includes(formatAppError(git)));
    if (locale === "en") assert.doesNotMatch(formatted, /[\u3400-\u4dbf\u4e00-\u9fff]/u);
  }
  assert.equal(resultNeedsVerification({ cleanupError: nested }), true);
  assert.equal(resultNeedsVerification({ error: nested }), true);
  assert.equal(resultNeedsVerification({ error: git }), false);
});

test("真实 Git 失败经生产结构载荷在当前语言下渲染", () => {
  assert.ok(fixture, "Run the real Git Rust fixture first and set REVIA_M6_ERROR_FIXTURE");
  const { git: payload, nested } = JSON.parse(readFileSync(fixture, "utf8"));
  assert.equal(payload.code, "errors.fileHistory.failedReadBlame");
  assert.match(payload.params.detail, /fatal/);
  assert.match(payload.message, /读取 blame 失败/);

  setLocale("en");
  const english = formatAppError(payload);
  assert.match(english, /^Failed to read blame: /);
  assert.match(formatAppError(nested), /^The operation may have run/);
  assert.doesNotMatch(formatAppError(nested), /[\u3400-\u4dbf\u4e00-\u9fff]/u);
  assert.doesNotMatch(english, /[\u3400-\u4dbf\u4e00-\u9fff]/u);

  setLocale("zh-CN");
  assert.match(formatAppError(payload), /^读取 blame 失败：/);
});


test("已有提交结果通知在语言切换后重新渲染标题、嵌套错误与快照说明", () => {
  const { nested } = JSON.parse(readFileSync(fixture, "utf8"));
  setLocale("zh-CN");
  const notice = commitResultNotice({ worktreeId: "fixture", title: () => messages.commit.resultNeedsVerification,
    detail: () => messages.commit.resultDraftKept,
    outcome: { success: false, error: nested, draftCleared: false }, needsVerification: true,
    output: [], hookChanges: null, snapshotRecordId: "record-fixture", snapshotPaths: ["file.txt"] });
  const render = () => renderToStaticMarkup(React.createElement(NotificationCard, { notice, onClose() {} }));
  assert.match(render(), /结果待核对/);
  setLocale("en");
  const english = render();
  assert.match(english, /The operation may have run/);
  assert.match(english, /record-fixture/);
  assert.doesNotMatch(english, /[\u3400-\u4dbf\u4e00-\u9fff]/u);
  setLocale("zh-CN");
  assert.match(render(), /结果待核对/);
});


test("真实暂存区的CRLF、非法文件名与游离HEAD警告按当前语言展示", () => {
  const { warnings } = JSON.parse(readFileSync(fixture, "utf8"));
  assert.ok(warnings.length >= 3);
  setLocale("en");
  for (const warning of warnings) {
    const text = commitWarningText(warning);
    assert.ok(text.length > 0);
    assert.doesNotMatch(text, /[\u3400-\u4dbf\u4e00-\u9fff]/u);
    assert.doesNotMatch(text, /\[object Object\]/);
  }
  setLocale("zh-CN");
  assert.ok(warnings.some((warning) => /[\u3400-\u4dbf\u4e00-\u9fff]/u.test(commitWarningText(warning))));
});


test("real persisted recovery error uses current language after reloading its receipt", () => {
  const payload = JSON.parse(readFileSync(`${fixture}.recovery.json`, "utf8"));
  assert.ok(payload.code && payload.cause);
  setLocale("en");
  const english = formatAppError(payload);
  assert.match(english, /before recovery|before restoring|precheck/i);
  assert.doesNotMatch(english, /[\u3400-\u4dbf\u4e00-\u9fff]/u);
  setLocale("zh-CN");
  assert.match(formatAppError(payload), /恢复前/);
});


test("real history errors and environment checks keep current-language presentation", async () => {
  const { log, environment } = JSON.parse(readFileSync(fixture, "utf8"));
  const pager = new LogPager({ targetId: "real-history-error", loader: async () => { throw log; } });
  pager.reset("invalid-regex"); await new Promise((resolve) => setImmediate(resolve));
  const reader = new HistoryRowsReader(async () => { throw log; });
  const rows = await reader.read({ targetId: "real-history-error", file: {}, view: "unified", ignoreWhitespace: "off", from: 0, to: 10 });
  for (const locale of ["en", "zh-CN"]) {
    setLocale(locale);
    const text = formatAppError(pager.snapshot().error);
    assert.equal(text, formatAppError(log)); assert.equal(formatAppError(rows.error), text);
    if (locale === "en") {
      assert.match(text, /^Git command failed/); assert.doesNotMatch(text, /Git 命令失败/);
      const checks = environment.flatMap((check) => [check.detail, check.hint].filter(Boolean)).map(formatAppError).join("\n");
      assert.doesNotMatch(checks, /毫秒|需要|登录/); assert.match(checks, /ms/);
    } else assert.match(text, /^Git 命令失败/);
  }
});

test("existing remote notices translate lazily and retain the conflict paths", () => {
  setLocale("zh-CN");
  const notice = updateNotice({ status: "mergeConflict", conflicts: ["中文 name.txt", "second.txt"] });
  const pushed = pushNotice({ kind: "pushed", remote: "origin", branch: "main", commits: 2 });
  setLocale("en");
  assert.match(notice.title(), /conflict/i); assert.doesNotMatch(notice.title(), /冲突/);
  assert.ok(notice.detail().includes("中文 name.txt")); assert.ok(pushed.title().includes("origin/main"));
  setLocale("zh-CN"); assert.match(notice.title(), /冲突/);
});

test("function messages preserve list parameters, empty placeholders and truncation", () => {
  for (const locale of ["zh-CN", "en"]) {
    setLocale(locale);
    const hook = messages.commit.resultHookChanges(["中文 path.txt", "second.txt"], []);
    assert.ok(hook.includes("中文 path.txt, second.txt")); assert.ok(hook.includes(messages.common.none));
    assert.doesNotMatch(hook, /\{\?\}/);
    const paths = Array.from({ length: 9 }, (_, index) => `file-${index}.txt`);
    const text = messages.remote.conflictFiles(paths);
    assert.ok(text.includes(paths[7])); assert.ok(!text.includes(paths[8])); assert.ok(text.endsWith(" …"));
    assert.doesNotMatch(text, /\{\?\}/);
  }
});

test("Git patch rows preserve headers and line numbers without worktree write credentials", () => {
  const patch = "diff --git a/file b/file\n--- a/file\n+++ b/file\n@@ -4,2 +4,2 @@ section\n context\n-old\n+new\n\\ No newline at end of file\n";
  const rows = new PatchRows(patch);
  assert.equal(rows.getRow(4).oldLine, 4); assert.equal(rows.getRow(5).oldLine, 5);
  assert.equal(rows.getRow(6).newLine, 5); assert.equal(rows.getRow(6).text, "new");
  assert.equal(rows.getRow(7).text, "\\ No newline at end of file");
  for (let at = 0; at < rows.rowCountValue; at++) {
    assert.equal(rows.getRow(at).hunkAction, null); assert.equal(rows.getRow(at).segmentAction, null);
  }
});


test("every function message retains the named arguments from its public contract", () => {
  const source = ts.createSourceFile("build.ts", readFileSync(new URL("../src/lib/i18n/build.ts", import.meta.url), "utf8"), ts.ScriptTarget.Latest, true);
  let definitions;
  const visit = (node) => {
    if (ts.isVariableDeclaration(node) && node.name.getText(source) === "FUNCTIONS") definitions = node.initializer;
    ts.forEachChild(node, visit);
  };
  visit(source); assert.ok(definitions && ts.isObjectLiteralExpression(definitions));
  for (const locale of ["zh-CN", "en"]) {
    const raw = JSON.parse(readFileSync(new URL(`../locales/${locale}.json`, import.meta.url), "utf8"));
    for (const property of definitions.properties) {
      const key = property.name.text;
      const text = key.split(".").reduce((node, segment) => node[segment], raw);
      const names = property.initializer.elements.map((node) => node.text);
      for (const name of names) assert.ok(text.includes(`{${name}}`), `${locale}:${key} lost parameter ${name}`);
      assert.doesNotMatch(text, /\{\?\}/, `${locale}:${key}`);
    }
  }
});

test("设置搜索保留双语语义关键词，不使用数组下标冒充词语", () => {
  for (const locale of ["zh-CN", "en"]) {
    setLocale(locale);
    for (const value of Object.values(messages.settings.keywords)) {
      assert.ok(value.split("|").some(word => /[A-Za-z\u3400-\u9fff]/u.test(word)));
      assert.ok(!value.split("|").some(word => /^\d+$/.test(word)));
    }
    assert.ok(messages.settings.keywords.gitTools.toLowerCase().includes("stash"));
    assert.ok(messages.settings.stashComparisonHint.length > 10);
  }
});


test("英文计数按ICU数量变化，Rust字符串计数与普通参数保持数据身份", () => {
  setLocale("en");
  for (const [render, one, two] of [
    [messages.changes.fileCount, "1 file", "2 files"],
    [messages.log.branchCount, "1 branch", "2 branches"],
    [messages.merge.conflictCount.some, "1 conflict remains", "2 conflicts remain"],
    [messages.close.writing, "1 write operation is running", "2 write operations are running"],
  ]) {
    assert.equal(render(1), one); assert.equal(render(2), two);
    assert.doesNotMatch(render(0), /plural|other \{/);
  }
  const failure = { kind: "rejected", code: "errors.commitFlow.filesUnresolvedConflictsCommit", params: { "0": "1" }, message: "fallback" };
  assert.equal(formatAppError(failure), "1 file has unresolved conflicts; cannot commit");
  const target = "<tag>{count, plural, one {not evaluated} other {bytes}}</tag>";
  assert.equal(messages.remote.pushed(1, target), "Pushed 1 commit to " + target);
  setLocale("zh-CN"); assert.equal(messages.changes.fileCount(1), "1 个文件");
  setLocale("en"); assert.equal(messages.changes.fileCount(1), "1 file");
});


test("关闭监听失败和后续所有写拒绝保留可即时翻译的语义，未发起写入", async () => {
  for (const which of ["window", "exit"]) {
    setLocale("zh-CN"); const failure = closeGuardFailure(which, { kind: "environment", message: "native failure bytes" });
    const gate = new CloseGuardGate(); gate.markFailed(failure);
    let writes = 0;
    const flow = new WriteGuardOrchestrator({ closeDriver: { canStartWrite: () => true }, closeBlockedReason: "unused",
      guardsReady: () => gate.ready, guardBlockedReason: () => gate.blockedReason,
      register() { throw new Error("must not register an unprotected write"); }, guard: async () => {}, rollback() {}, reconcile() {}, endWrite() {} });
    const blocked = await flow.begin("owned-fixture"); assert.equal(blocked.ok, false); assert.equal(blocked.reason, failure);
    assert.match(formatAppError(failure), /关闭保护/);
    setLocale("en");
    const english = formatAppError(blocked.reason); assert.match(english, /Close protection/); assert.match(english, /native failure bytes/);
    assert.doesNotMatch(english, /[\u3400-\u4dbf\u4e00-\u9fff]/u);
    await assert.rejects(runGuardedWrite({ writing: new Set(), publishWriting() {}, begin: id => flow.begin(id), end() {},
      beginOperation() {}, canRecord: () => true, execute: async () => { writes++; }, refreshTarget: async () => {}, reportFailure() {},
      toError: toAppError, busyMessage: "unused", guardUnavailable: () => "must not flatten structured failure" }, { worktreeId: "owned-fixture" }), error => error === failure);
    assert.equal(writes, 0);
  }
});

function failedOutputListener(caught) {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const start = app.indexOf("  /** 实时输出的传输与展示");
  const end = app.indexOf("\n  /**", start + 4);
  assert.ok(start >= 0 && end > start);
  const code = ts.transpileModule(app.slice(start, end), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  const errors = []; let cleanup;
  vm.runInNewContext(code, {
    useEffect(effect) { cleanup = effect(); }, listen: () => Promise.reject(caught),
    COMMIT_OUTPUT_EVENT: "fixture-output-event", commitOutputs: {},
    setCommitLiveOutput() {}, setCommitResults() {},
    setAppError(error, owner) { errors.push({ error, owner }); },
    commitOutputListenerFailure, localizedError, getErrorMessage, messages,
  });
  return { errors, dispose: () => cleanup() };
}

test("生产实时输出监听失败回调保留语义和原因，同一错误能双向切语言", async () => {
  const native = JSON.parse(readFileSync(fixture + ".watch.json", "utf8")).error;
  for (const initial of ["zh-CN", "en"]) {
    for (const caught of [native, new Error("native listener rejection")]) {
      setLocale(initial); const f = failedOutputListener(caught);
      await new Promise(resolve => setImmediate(resolve)); assert.equal(f.errors.length, 1);
      const { error, owner } = f.errors[0]; assert.equal(owner, null);
      assert.equal(error.code, "errors.commitOutput.listenerFailed");
      assert.equal(error.params.what, undefined); assert.equal(error.cause.code, toAppError(caught).code);
      for (const locale of ["en", "zh-CN"]) {
        setLocale(locale); const text = formatAppError(error);
        if (locale === "en") { assert.match(text, /live output listener/); assert.doesNotMatch(text, /[\u3400-\u4dbf\u4e00-\u9fff]/u); }
        else { assert.match(text, /实时输出监听/); assert.doesNotMatch(text, /live output listener/); }
        assert.ok(text.includes(formatAppError(toAppError(caught))));
      }
      f.dispose();
    }
  }
});

test("生产实时输出监听卸载后的迟到失败不再更新界面", async () => {
  const f = failedOutputListener(new Error("late listener failure")); f.dispose();
  await new Promise(resolve => setImmediate(resolve)); assert.equal(f.errors.length, 0);
});

test("成功自动stash已清关联仍从真实回执产生保存通知与固定查看动作", () => {
  const result = JSON.parse(readFileSync(fixture + ".autostash-removed.json", "utf8"));
  assert.ok(result.restored && result.removed); let opened;
  const action = { label: () => messages.operation.viewStash, run: () => { opened = result.link.stashOid; } };
  const notice = savedAutostashNotice(result.link, action, result.removed);
  setLocale("zh-CN"); assert.match(notice.title(), /自动保存/);
  setLocale("en"); assert.match(notice.title(), /autostashed before cherry-pick/i); assert.match(notice.detail(), /restored/);
  const html = renderToStaticMarkup(React.createElement(NotificationCard, { notice: { ...notice, id: "receipt" }, onClose() {} }));
  assert.match(html, /View stash/i); assert.doesNotMatch(html, /[\u3400-\u4dbf\u4e00-\u9fff]/u);
  notice.action.run(); assert.equal(opened, result.link.stashOid);
  const outcome = { applied: ["a".repeat(40)], skippedEmpty: [], skipped: [], failedOid: "b".repeat(40), pending: ["c".repeat(40)] };
  const english = rewriteOutcomeDetail(outcome); assert.ok(english.includes("bbbbbbbb"));
  setLocale("zh-CN"); const chinese = rewriteOutcomeDetail(outcome); assert.ok(chinese.includes("bbbbbbbb")); assert.notEqual(chinese, english);
});
