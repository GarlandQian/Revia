// M11：AI 设置的真实对话框、真实 ipc 封装与 App.updateSettings 切片回归。
// 只使用本机 mockIPC 与受控 Promise；不访问网络、系统凭据库或真实桌面。
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import ts from "typescript";
import test, { afterEach } from "node:test";
import { registerHooks } from "node:module";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";

globalThis.window = { setTimeout, clearTimeout, crypto: globalThis.crypto };

const sourceRoot = new URL("../src/", import.meta.url);
const imports = registerHooks({
  load(url, context, nextLoad) {
    if (url.startsWith(sourceRoot.href) && (url.endsWith(".ts") || url.endsWith(".tsx"))) {
      const code = fs.readFileSync(new URL(url), "utf8");
      return {
        format: "module",
        shortCircuit: true,
        source: ts.transpileModule(code, {
          compilerOptions: {
            module: ts.ModuleKind.ESNext,
            jsx: ts.JsxEmit.ReactJSX,
            target: ts.ScriptTarget.ESNext,
          },
        }).outputText,
      };
    }
    return nextLoad(url, context);
  },
  resolve(specifier, context, nextResolve) {
    let source;
    if (specifier.startsWith("@/")) source = new URL(specifier.slice(2), sourceRoot);
    else if ((specifier.startsWith("./") || specifier.startsWith("../")) && context.parentURL?.startsWith(sourceRoot.href)) {
      source = new URL(specifier, context.parentURL);
    }
    if (source) {
      if (!/\.[^/]+$/.test(source.pathname)) {
        const path = new URL(source.href);
        const candidate = path.pathname.endsWith("/") ? path : path;
        if (fs.existsSync(new URL(candidate.pathname + ".tsx", candidate))) candidate.pathname += ".tsx";
        else if (fs.existsSync(new URL(candidate.pathname + ".ts", candidate))) candidate.pathname += ".ts";
        else candidate.pathname += "/index.ts";
        source = candidate;
      }
      return nextResolve(source.href, context);
    }
    return nextResolve(specifier, context);
  },
});
const { ipc } = await import("../src/lib/ipc.ts");
const errorModule = await import("../src/lib/error.ts");
const i18n = await import("../src/lib/i18n/index.ts");
const aiSettingsModule = await import("../src/lib/aiSettings.ts");
imports.deregister();
const { t } = i18n;

afterEach(() => clearMocks());

const valuesFixture = {
  appearanceMode: "system",
  themeLight: "light-2026",
  themeDark: "dark-2026",
  codeFontFamily: "",
  fontSize: 12,
  gitPath: "",
  mergeTool: "",
  diffTool: "",
  language: "auto",
  cherryPickKeepEmpty: false,
  cherryPickIncludeSource: false,
  sidebarWidth: 300,
  uiZoom: 100,
  treeGroupByDirectory: false,
  diffContextLines: 3,
  diffFoldUnchanged: false,
  diffSyncScroll: false,
  diffHighlight: "word",
  diffShowWhitespace: false,
  diffSoftWrap: false,
  diffJumpToNextFile: false,
  diffIgnoreWhitespace: 0,
  changelistMode: false,
  commitSubjectLimit: 72,
  commitBlankLineRequired: true,
  commitBodyLineLimit: 72,
  commitShowRuler: true,
  commitWarnCrlf: true,
  commitWarnDetached: true,
  commitWarnLargeFileBytes: 1024 * 1024,
  commitWarnInvalidFilename: true,
  notify: {},
  mergeAutoApply: false,
  logColumns: { author: true, date: true, hash: true },
  fileHistoryShowNames: true,
  stashCompareLocal: false,
  logLongEdges: true,
  logFirstParent: false,
  logPreviewPosition: "right",
  updateMode: "merge",
  fetchTagMode: "auto",
  fetchAutomatically: true,
  checkRemote: true,
  autoUpdateOnPushRejection: false,
  pushDialogOnCommit: true,
  pushDialogProtectedOnly: false,
  protectedBranches: "",
  detectedGitPath: null,
  appUpdateMode: "manual",
  allowPrerelease: false,
  shortcuts: {},
};

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

function loadUpdateSettings() {
  const app = fs.readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const marker = "  const updateSettings = useCallback(";
  const endMarker = "\n\n  // 设置对话框关闭时会被卸载；AI 控制器与写入队列必须留在 App。";
  const start = app.indexOf(marker);
  const end = app.indexOf(endMarker, start);
  assert.ok(start >= 0, "App.updateSettings marker missing");
  assert.ok(end > start, "App.updateSettings end marker missing");
  assert.equal(app.indexOf(marker, start + marker.length), -1, "App.updateSettings marker duplicated");
  const source = app.slice(start, end) + "\nglobalThis.updateSettings = updateSettings;";
  const code = ts.transpileModule(source, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  const noop = () => {};
  const context = {
    useCallback: (fn) => fn,
    ipc,
    toAppError: errorModule.toAppError,
    resolveLocale: () => "zh-CN",
    setLocale: noop,
    setLanguage: noop,
    applyShortcutOverrides: noop,
    setShortcutsSnapshot: noop,
    m7: { updateStatus: async () => ({ available: null }) },
    updateChannelEpoch: { current: 0 },
    changelistModeRef: { current: false },
    acceptChangelistView: noop,
    refresh: async () => {},
    setAppUpdateMode: noop,
    setAllowPrerelease: noop,
    setUpdateStatus: noop,
    setUpdateBannerVersion: noop,
    setAppError: noop,
    setAppearanceMode: noop,
    setThemeLight: noop,
    setThemeDark: noop,
    setCodeFontFamily: noop,
    setFontSize: noop,
    setGitPath: noop,
    setMergeTool: noop,
    setDiffTool: noop,
    setCherryPickKeepEmpty: noop,
    setCherryPickIncludeSource: noop,
    setSidebarWidth: noop,
    setView: noop,
    setUiZoom: noop,
    setTreeGroupByDirectory: noop,
    setDiffOptions: noop,
    setChangelistMode: noop,
    setCommitChecks: noop,
    setNotifyKinds: noop,
    setMergeAutoApply: noop,
    setFileHistoryShowNames: noop,
    setStashCompareLocal: noop,
    setLogSettings: noop,
    setPushSettings: noop,
  };
  vm.createContext(context);
  vm.runInContext(code, context);
  return context.updateSettings;
}

const appUpdateSettings = loadUpdateSettings();

const dialogCode = ts.transpileModule(
  fs.readFileSync(new URL("../src/components/app/SettingsDialog.tsx", import.meta.url), "utf8"),
  {
    compilerOptions: {
      jsx: ts.JsxEmit.ReactJSX,
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
    },
  },
).outputText;

function createHooks() {
  const slots = [];
  let cursor = 0;
  let effects = [];
  const equal = (a, b) => a && b && a.length === b.length && a.every((value, index) => Object.is(value, b[index]));
  const hooks = {
    useState(initial) {
      const index = cursor++;
      slots[index] ??= { value: typeof initial === "function" ? initial() : initial };
      return [slots[index].value, (next) => {
        slots[index].value = typeof next === "function" ? next(slots[index].value) : next;
      }];
    },
    useRef(value) {
      const index = cursor++;
      slots[index] ??= { current: value };
      return slots[index];
    },
    useEffect(effect, deps) {
      const index = cursor++;
      if (!slots[index] || !equal(slots[index].deps, deps)) {
        const prior = slots[index];
        slots[index] = { deps };
        effects.push(() => {
          prior?.cleanup?.();
          slots[index].cleanup = effect();
        });
      }
    },
    useMemo(compute) {
      return compute();
    },
  };
  return {
    hooks,
    resetCursor() { cursor = 0; },
    takeEffects() { const current = effects; effects = []; return current; },
    dispose() { for (const slot of slots) slot?.cleanup?.(); },
  };
}

function textOf(node) {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (node && typeof node === "object") return textOf(node.props?.children);
  return "";
}

function createEnvironment(initial = {}) {
  const state = {
    baseUrl: "",
    model: "",
    language: "repo",
    prompt: "",
    defaultPrompt: "默认提示词",
    keySet: false,
    models: ["m1", "m2"],
    ...initial,
  };
  const requests = [];
  const statusCalls = [];
  const listCalls = [];
  const keyCalls = [];
  const clearCalls = [];
  const statusQueue = [];
  const listQueue = [];
  let saveGate = null;
  let keyGate = null;
  let failAddress = null;

  const buildStatus = () => ({
    configured: state.baseUrl !== "" && state.model !== "",
    baseUrl: state.baseUrl,
    model: state.model,
    language: state.language,
    prompt: state.prompt || state.defaultPrompt,
    defaultPrompt: state.defaultPrompt,
    keySet: state.keySet,
    confirmed: false,
    insecure: false,
  });
  const apply = (request) => {
    if ("aiBaseUrl" in request) state.baseUrl = request.aiBaseUrl;
    if ("aiModel" in request) state.model = request.aiModel;
    if ("aiLanguage" in request) state.language = request.aiLanguage;
    if ("aiPrompt" in request) state.prompt = request.aiPrompt;
    return {};
  };

  mockIPC((command, args) => {
    switch (command) {
      case "m9_measure_state":
        return { measure: false, commit: "" };
      case "ai_status":
        statusCalls.push({});
        if (statusQueue.length) return statusQueue.shift().promise;
        return buildStatus();
      case "update_settings": {
        const request = args.request;
        requests.push(request);
        if (request.aiBaseUrl && request.aiBaseUrl === failAddress) {
          return Promise.reject({
            kind: "rejected",
            code: "errors.ai.serviceChanged",
            params: {},
            message: "服务地址已变化，请重新操作",
          });
        }
        if (saveGate) {
          const gate = saveGate;
          saveGate = null;
          return gate.promise.then(() => apply(request));
        }
        return apply(request);
      }
      case "ai_set_key": {
        keyCalls.push(args.request);
        if (args.request.baseUrl !== state.baseUrl) {
          return Promise.reject({
            kind: "rejected",
            code: "errors.ai.serviceChanged",
            params: {},
            message: "服务地址已变化，请重新操作",
          });
        }
        const finish = () => { state.keySet = args.request.key.trim() !== ""; return buildStatus(); };
        if (keyGate) {
          const gate = keyGate;
          keyGate = null;
          return gate.promise.then(finish);
        }
        return finish();
      }
      case "ai_clear_key": {
        clearCalls.push(args.request);
        const finish = () => { state.keySet = false; return buildStatus(); };
        if (keyGate) {
          const gate = keyGate;
          keyGate = null;
          return gate.promise.then(finish);
        }
        return finish();
      }
      case "ai_list_models":
        listCalls.push(args.request);
        if (listQueue.length) {
          const next = listQueue.shift();
          return typeof next === "function" ? next(args.request) : next.promise;
        }
        return { baseUrl: args.request.baseUrl, models: state.models };
      default:
        throw new Error(`unexpected IPC command ${command}`);
    }
  });

  return {
    state, requests, statusCalls, listCalls, keyCalls, clearCalls,
    gateSave() { saveGate = deferred(); return saveGate; },
    gateKey() { keyGate = deferred(); return keyGate; },
    deferStatus() { const item = deferred(); statusQueue.push(item); return item; },
    deferList() {
      const item = deferred();
      listQueue.push(() => item.promise);
      return item;
    },
    failNextList(error) {
      const item = deferred();
      listQueue.push(() => item.promise);
      item.reject(error);
    },
    setFailAddress(value) { failAddress = value; },
  };
}

function createController(save) {
  return new aiSettingsModule.AiSettings({
    transport: {
      status: () => ipc.aiStatus(),
      save,
      setKey: (key, baseUrl) => ipc.aiSetKey(key, baseUrl),
      clearKey: (baseUrl) => ipc.aiClearKey(baseUrl),
      listModels: (baseUrl) => ipc.aiListModels(baseUrl),
    },
  });
}

function createDialogInstance(env, aiSettings, onChange) {
  const hooks = createHooks();
  const module = { exports: {} };
  const require = (id) => {
    switch (id) {
      case "react":
        return hooks.hooks;
      case "react/jsx-runtime":
        return { jsx: (type, props) => ({ type, props }), jsxs: (type, props) => ({ type, props }), Fragment: Symbol.for("react.fragment") };
      case "@/lib/ipc":
        return { ipc };
      case "@/lib/error":
        return errorModule;
      case "@/lib/i18n/index":
        return i18n;
      case "@/lib/i18n/text":
        return { highlightLevelText: (value) => value, notifyKindText: (value) => value };
      case "@/lib/theme":
        return { themesOfKind: () => [] };
      case "@/components/ui/app-icon":
        return { AppIcon: () => null };
      case "@/features/m7/settings":
        return {
          M7GithubSettings: () => null,
          M7ShortcutSettings: () => null,
          M7ThemeSettings: () => null,
          M7UpdateSettings: () => null,
        };
      case "@/hooks/useDialogOpen":
        return { useModalDialog: () => {} };
      case "@/lib/aiSettings":
        return aiSettingsModule;
      default:
        throw new Error(`unexpected import ${id}`);
    }
  };
  vm.runInNewContext(dialogCode, { module, exports: module.exports, require, console });
  const SettingsDialog = module.exports.SettingsDialog;
  let props = {
    open: true,
    values: valuesFixture,
    onClose: () => {},
    onChange,
    aiSettings,
    onReloadEnvironment: async () => null,
    worktreeId: null,
    updateStatus: null,
  };
  let tree = null;

  const render = () => {
    hooks.resetCursor();
    tree = SettingsDialog(props);
    for (const effect of hooks.takeEffects()) effect();
  };
  const flush = async (rounds = 10) => {
    for (let round = 0; round < rounds; round++) {
      render();
      await new Promise((resolve) => setImmediate(resolve));
    }
  };
  const nodes = () => {
    const found = [];
    const walk = (node) => {
      if (!node || typeof node !== "object") return;
      if (Array.isArray(node)) return node.forEach(walk);
      found.push(node);
      walk(node.props?.children);
    };
    walk(tree);
    return found;
  };
  const inputs = () => nodes().filter((node) => node.type === "input");
  const baseInput = () => inputs().find((node) => node.props?.["data-ai-field"] === "baseUrl");
  const modelInput = () => inputs().find((node) => node.props?.["data-ai-field"] === "model");
  const keyInput = () => inputs().find((node) => node.props.type === "password");
  const promptInput = () => nodes().find((node) => node.type === "textarea");
  const languageSelect = () => nodes().find((node) => node.type === "select" && node.props?.["data-ai-field"] === "language");
  const listSelect = () => nodes().find((node) => node.type === "select" && node.props["aria-label"] === t("settings.ai.modelsChoose"));
  const button = (text) => nodes().find((node) => node.type === "button" && textOf(node) === text);
  const aiButton = () => nodes().find((node) => node.type === "button" && node.props?.["data-settings-page"] === "ai");
  const navButton = (key) => nodes().find((node) => node.type === "button" && node.props?.["data-settings-page"] === key);
  const openAi = async () => {
    assert.ok(aiButton(), "AI settings navigation button missing");
    aiButton().props.onClick();
    await flush();
  };
  const openPage = async (key) => {
    const target = navButton(key);
    assert.ok(target, `settings navigation button ${key} missing`);
    target.props.onClick();
    await flush();
  };
  const setOpen = async (open) => {
    props = { ...props, open };
    await flush();
  };

  return {
    render, flush, nodes, inputs, baseInput, modelInput, keyInput, promptInput,
    languageSelect, listSelect, button, openAi, openPage, setOpen,
    dispose() { hooks.dispose(); },
    get props() { return props; },
  };
}

function dialogFixture(initial = {}) {
  const env = createEnvironment(initial);
  const results = [];
  const onChange = async (patch) => {
    const result = await appUpdateSettings(patch);
    results.push({ patch, result });
    return result;
  };
  const aiSettings = createController(onChange);
  const dialog = createDialogInstance(env, aiSettings, onChange);
  return {
    ...env, ...dialog, results, aiSettings,
    dispose() { dialog.dispose(); },
  };
}

const useAiSettingsCode = ts.transpileModule(
  fs.readFileSync(new URL("../src/lib/useAiSettings.ts", import.meta.url), "utf8"),
  {
    compilerOptions: {
      jsx: ts.JsxEmit.ReactJSX,
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
    },
  },
).outputText;

function loadUseAiSettings(hooks) {
  const module = { exports: {} };
  const require = (id) => {
    switch (id) {
      case "react":
        return hooks.hooks;
      case "@/lib/ipc":
        return { ipc };
      case "@/lib/aiSettings":
        return aiSettingsModule;
      default:
        throw new Error(`unexpected owner import ${id}`);
    }
  };
  vm.runInNewContext(useAiSettingsCode, { module, exports: module.exports, require, console });
  return module.exports.useAiSettings;
}

function ownerFixture(initial = {}) {
  const env = createEnvironment(initial);
  const results = [];
  const onChange = async (patch) => {
    const result = await appUpdateSettings(patch);
    results.push({ patch, result });
    return result;
  };
  const hooks = createHooks();
  const useAiSettings = loadUseAiSettings(hooks);
  let aiSettings;
  const render = () => {
    hooks.resetCursor();
    aiSettings = useAiSettings(onChange);
  };
  render();
  return {
    env, results, onChange, render,
    get aiSettings() { return aiSettings; },
    dialog() { return createDialogInstance(env, aiSettings, onChange); },
    dispose() { hooks.dispose(); },
  };
}

test("四个 AI 字段经真实对话框、App.updateSettings 与真实 ipc 落盘，普通 patch 不回归", async () => {
  const fixture = dialogFixture();
  await fixture.flush();
  await fixture.openAi();
  assert.ok(fixture.statusCalls.length >= 1);
  assert.equal(fixture.listCalls.length, 0, "空地址不应请求模型列表");

  fixture.baseInput().props.onChange({ target: { value: "https://api.example.com/v1" } });
  fixture.baseInput().props.onBlur();
  await fixture.flush();
  assert.deepEqual(fixture.requests.at(-1), { aiBaseUrl: "https://api.example.com/v1" });
  assert.equal(fixture.listCalls.at(-1).baseUrl, "https://api.example.com/v1");

  fixture.modelInput().props.onChange({ target: { value: "gpt-4o" } });
  fixture.modelInput().props.onBlur();
  await fixture.flush();
  assert.deepEqual(fixture.requests.at(-1), { aiModel: "gpt-4o" });

  fixture.languageSelect().props.onChange({ target: { value: "english" } });
  await fixture.flush();
  assert.deepEqual(fixture.requests.at(-1), { aiLanguage: "english" });

  fixture.promptInput().props.onChange({ target: { value: "自定义提示词" } });
  fixture.promptInput().props.onBlur();
  await fixture.flush();
  assert.deepEqual(fixture.requests.at(-1), { aiPrompt: "自定义提示词" });
  assert.ok(fixture.results.every((item) => item.result === null));

  const normal = await appUpdateSettings({ commitSubjectLimit: 80 });
  assert.equal(normal, null);
  assert.deepEqual(fixture.requests.at(-1), { commitSubjectLimit: 80 });

  await fixture.setOpen(false);
  await fixture.setOpen(true);
  await fixture.flush();
  assert.equal(fixture.baseInput().props.value, "https://api.example.com/v1");
  assert.equal(fixture.modelInput().props.value, "gpt-4o");
  assert.equal(fixture.promptInput().props.value, "自定义提示词");
  assert.equal(fixture.languageSelect().props.value, "english");
  assert.equal(fixture.keyInput().props.value, "");
  fixture.dispose();
});

test("保存串行：save(B) 未完成时改回 A，仍会按队列补发 A 而不是停在 B", async () => {
  const fixture = dialogFixture({ baseUrl: "https://a.example/v1" });
  await fixture.flush();
  await fixture.openAi();
  const gate = fixture.gateSave();

  fixture.baseInput().props.onChange({ target: { value: "https://b.example/v1" } });
  fixture.baseInput().props.onBlur();
  await fixture.flush();
  assert.deepEqual(fixture.requests.at(-1), { aiBaseUrl: "https://b.example/v1" });
  assert.equal(fixture.state.baseUrl, "https://a.example/v1", "保存应停在受控 Promise");

  fixture.baseInput().props.onChange({ target: { value: "https://a.example/v1" } });
  fixture.baseInput().props.onBlur();
  await fixture.flush();
  assert.equal(fixture.requests.length, 1, "第二次保存应排队等待第一次完成");

  gate.resolve();
  await fixture.flush(16);
  const urls = fixture.requests.filter((request) => "aiBaseUrl" in request).map((request) => request.aiBaseUrl);
  assert.deepEqual(urls, ["https://b.example/v1", "https://a.example/v1"]);
  assert.equal(fixture.state.baseUrl, "https://a.example/v1");
  assert.equal(fixture.baseInput().props.value, "https://a.example/v1");
  fixture.dispose();
});

test("关闭重开先等已排队写入，再读取权威状态", async () => {
  const fixture = dialogFixture({ baseUrl: "https://a.example/v1" });
  await fixture.flush();
  await fixture.openAi();
  const before = fixture.statusCalls.length;
  const gate = fixture.gateSave();

  fixture.baseInput().props.onChange({ target: { value: "https://b.example/v1" } });
  fixture.baseInput().props.onBlur();
  await fixture.flush();
  await fixture.setOpen(false);
  await fixture.setOpen(true);
  await fixture.flush();
  assert.equal(fixture.statusCalls.length, before, "写入未完成前不应读取状态");

  gate.resolve();
  await fixture.flush(16);
  assert.ok(fixture.statusCalls.length > before, "写入完成后应读取状态");
  assert.equal(fixture.state.baseUrl, "https://b.example/v1");
  assert.equal(fixture.baseInput().props.value, "https://b.example/v1");
  fixture.dispose();
});

test("地址草稿为 B 时，迟到的初始状态 A 不启动也不发布 A 的模型列表", async () => {
  const fixture = dialogFixture({ baseUrl: "https://a.example/v1", models: ["a-model"] });
  await fixture.flush();
  const status = fixture.deferStatus();
  await fixture.openAi();
  fixture.baseInput().props.onChange({ target: { value: "https://b.example/v1" } });
  await fixture.flush();
  status.resolve({
    configured: false,
    baseUrl: "https://a.example/v1",
    model: "",
    language: "repo",
    prompt: "默认提示词",
    defaultPrompt: "默认提示词",
    keySet: false,
    confirmed: false,
    insecure: false,
  });
  await fixture.flush(16);
  assert.equal(fixture.listCalls.length, 0);
  assert.equal(fixture.listSelect(), undefined);
  assert.equal(fixture.baseInput().props.value, "https://b.example/v1");
  fixture.dispose();
});

test("旧地址保存完成时地址草稿已改，不启动旧地址列表；保存新地址后才读取", async () => {
  const fixture = dialogFixture({ baseUrl: "https://a.example/v1", models: ["a-model"] });
  await fixture.flush();
  await fixture.openAi();
  assert.deepEqual(fixture.listCalls.map((call) => call.baseUrl), ["https://a.example/v1"]);

  const gate = fixture.gateSave();
  fixture.baseInput().props.onChange({ target: { value: "https://b.example/v1" } });
  fixture.baseInput().props.onBlur();
  await fixture.flush();
  fixture.baseInput().props.onChange({ target: { value: "https://c.example/v1" } });
  await fixture.flush();
  gate.resolve();
  await fixture.flush(16);
  assert.deepEqual(fixture.listCalls.map((call) => call.baseUrl), ["https://a.example/v1"]);

  fixture.baseInput().props.onBlur();
  await fixture.flush(16);
  assert.equal(fixture.listCalls.at(-1).baseUrl, "https://c.example/v1");
  assert.equal(fixture.baseInput().props.value, "https://c.example/v1");
  fixture.dispose();
});

test("地址末尾斜杠在保存前规范化，密钥按规范化地址写入", async () => {
  const fixture = dialogFixture({ baseUrl: "https://a.example/v1" });
  await fixture.flush();
  await fixture.openAi();

  fixture.baseInput().props.onChange({ target: { value: "  https://b.example/v1/  " } });
  fixture.keyInput().props.onChange({ target: { value: "sk-test" } });
  await fixture.flush();
  fixture.button(t("settings.ai.saveKey")).props.onClick();
  await fixture.flush(16);
  assert.deepEqual(fixture.requests.at(-1), { aiBaseUrl: "https://b.example/v1" });
  assert.equal(fixture.keyCalls.at(-1).baseUrl, "https://b.example/v1");
  assert.equal(fixture.baseInput().props.value, "https://b.example/v1");
  assert.equal(fixture.state.keySet, true);
  fixture.dispose();
});

test("密钥保存捕获点击时的地址：等待地址保存期间改地址则失效", async () => {
  const fixture = dialogFixture({ baseUrl: "https://a.example/v1" });
  await fixture.flush();
  await fixture.openAi();
  const gate = fixture.gateSave();

  fixture.baseInput().props.onChange({ target: { value: "https://b.example/v1" } });
  fixture.keyInput().props.onChange({ target: { value: "sk-test" } });
  await fixture.flush();
  fixture.button(t("settings.ai.saveKey")).props.onClick();
  await fixture.flush();
  assert.deepEqual(fixture.requests.at(-1), { aiBaseUrl: "https://b.example/v1" });

  fixture.baseInput().props.onChange({ target: { value: "https://c.example/v1" } });
  await fixture.flush();
  gate.resolve();
  await fixture.flush(16);
  assert.equal(fixture.keyCalls.length, 0, "地址在等待期间变化，密钥写入必须失效");
  assert.equal(fixture.state.keySet, false);

  fixture.button(t("settings.ai.saveKey")).props.onClick();
  await fixture.flush(16);
  assert.equal(fixture.keyCalls.at(-1).baseUrl, "https://c.example/v1");
  assert.equal(fixture.state.keySet, true);
  fixture.dispose();
});

test("地址保存失败时密钥不写入，重试成功后先落地址再写密钥", async () => {
  const fixture = dialogFixture({ baseUrl: "https://a.example/v1" });
  await fixture.flush();
  await fixture.openAi();
  fixture.setFailAddress("https://b.example/v1");

  fixture.baseInput().props.onChange({ target: { value: "https://b.example/v1" } });
  fixture.keyInput().props.onChange({ target: { value: "sk-test" } });
  await fixture.flush();
  fixture.button(t("settings.ai.saveKey")).props.onClick();
  await fixture.flush(16);
  assert.deepEqual(fixture.requests.at(-1), { aiBaseUrl: "https://b.example/v1" });
  assert.equal(fixture.keyCalls.length, 0);
  assert.equal(fixture.state.baseUrl, "https://a.example/v1");
  assert.ok(fixture.nodes().some((node) => node.props?.role === "alert" && textOf(node) === t("errors.ai.serviceChanged")));

  fixture.setFailAddress(null);
  fixture.button(t("settings.ai.saveKey")).props.onClick();
  await fixture.flush(16);
  assert.equal(fixture.state.baseUrl, "https://b.example/v1");
  assert.equal(fixture.keyCalls.at(-1).baseUrl, "https://b.example/v1");
  assert.equal(fixture.state.keySet, true);
  fixture.dispose();
});

test("关闭重开使等待中的密钥操作失效，不按新会话地址写入", async () => {
  const fixture = dialogFixture({ baseUrl: "https://a.example/v1" });
  await fixture.flush();
  await fixture.openAi();
  const gate = fixture.gateSave();

  fixture.baseInput().props.onChange({ target: { value: "https://b.example/v1" } });
  fixture.keyInput().props.onChange({ target: { value: "sk-test" } });
  await fixture.flush();
  fixture.button(t("settings.ai.saveKey")).props.onClick();
  await fixture.flush();
  await fixture.setOpen(false);
  await fixture.setOpen(true);
  await fixture.flush();
  gate.resolve();
  await fixture.flush(16);
  assert.equal(fixture.keyCalls.length, 0);
  fixture.dispose();
});

test("密钥保存回调不清空等待期间新输入的密钥", async () => {
  const fixture = dialogFixture({ baseUrl: "https://a.example/v1" });
  await fixture.flush();
  await fixture.openAi();
  const gate = fixture.gateKey();

  fixture.keyInput().props.onChange({ target: { value: "sk-first" } });
  await fixture.flush();
  fixture.button(t("settings.ai.saveKey")).props.onClick();
  await fixture.flush();
  assert.equal(fixture.keyCalls.length, 1);

  fixture.keyInput().props.onChange({ target: { value: "sk-second" } });
  await fixture.flush();
  gate.resolve();
  await fixture.flush(16);
  assert.equal(fixture.keyInput().props.value, "sk-second");
  assert.equal(fixture.state.keySet, true);
  fixture.dispose();
});

test("模型列表：加载、选择、空、失败重试与旧地址迟到响应失效", async () => {
  const fixture = dialogFixture({ baseUrl: "https://a.example/v1", model: "m1", models: ["m1", "m2"] });
  await fixture.flush();
  const initialList = fixture.deferList();
  await fixture.openAi();
  assert.ok(fixture.nodes().some((node) => node.props?.role === "status" && textOf(node) === t("settings.ai.modelsLoading")));
  initialList.resolve({ baseUrl: "https://a.example/v1", models: ["m1", "m2"] });
  await fixture.flush(16);
  assert.ok(fixture.listSelect());
  assert.equal(fixture.modelInput().props.value, "m1", "列表刷新不擅自替换已有模型");

  fixture.listSelect().props.onChange({ target: { value: "m2" } });
  await fixture.flush(16);
  assert.deepEqual(fixture.requests.at(-1), { aiModel: "m2" });
  assert.equal(fixture.modelInput().props.value, "m2");

  const empty = fixture.deferList();
  fixture.button(t("settings.ai.modelsRefresh")).props.onClick();
  await fixture.flush(16);
  empty.resolve({ baseUrl: "https://a.example/v1", models: [] });
  await fixture.flush(16);
  assert.ok(fixture.nodes().some((node) => textOf(node) === t("settings.ai.modelsEmpty")));
  assert.equal(fixture.modelInput().props.value, "m2");

  fixture.failNextList({
    kind: "rejected",
    code: "errors.ai.modelsUnavailable",
    params: {},
    message: "无法获取模型列表",
  });
  fixture.button(t("settings.ai.modelsRetry")).props.onClick();
  await fixture.flush(16);
  assert.ok(fixture.nodes().some((node) => node.props?.role === "alert" && textOf(node) === t("errors.ai.modelsUnavailable")));
  assert.equal(fixture.modelInput().props.value, "m2");

  fixture.button(t("settings.ai.modelsRetry")).props.onClick();
  await fixture.flush(16);
  assert.ok(fixture.listSelect());
  assert.equal(fixture.modelInput().props.value, "m2");

  const stale = fixture.deferList();
  fixture.button(t("settings.ai.modelsRefresh")).props.onClick();
  await fixture.flush(16);
  fixture.state.models = ["b1"];
  fixture.baseInput().props.onChange({ target: { value: "https://b.example/v1" } });
  fixture.baseInput().props.onBlur();
  await fixture.flush(16);
  assert.equal(fixture.listCalls.at(-1).baseUrl, "https://b.example/v1");
  stale.resolve({ baseUrl: "https://a.example/v1", models: ["a-late"] });
  await fixture.flush(16);
  assert.ok(fixture.listSelect());
  assert.ok(fixture.nodes().some((node) => node.type === "option" && node.props.value === "b1"));
  assert.equal(fixture.modelInput().props.value, "m2");
  fixture.dispose();
});

test("App 级控制器跨对话框真实卸载：旧保存未完成时新实例的编辑仍按同一队列执行", async () => {
  const owner = ownerFixture({ baseUrl: "https://a.example/v1" });
  const first = owner.dialog();
  await first.flush();
  await first.openAi();
  const gate = owner.env.gateSave();

  first.baseInput().props.onChange({ target: { value: "https://b.example/v1" } });
  first.baseInput().props.onBlur();
  await first.flush();
  assert.equal(owner.env.requests.length, 1, "第一次保存应已发出并停在受控 Promise");

  // 真实卸载：effect cleanup 关闭页面，但 App 持有的控制器与队列不销毁。
  first.dispose();
  const second = owner.dialog();
  await second.flush();
  await second.openAi();
  const statusBefore = owner.env.statusCalls.length;

  second.baseInput().props.onChange({ target: { value: "https://c.example/v1" } });
  second.baseInput().props.onBlur();
  await second.flush();
  assert.equal(owner.env.requests.length, 1, "新实例的保存应排在旧保存之后");
  assert.equal(owner.env.statusCalls.length, statusBefore, "重开后读取状态也要等旧写入完成");

  gate.resolve();
  await second.flush(24);
  const urls = owner.env.requests
    .filter((request) => "aiBaseUrl" in request)
    .map((request) => request.aiBaseUrl);
  assert.deepEqual(urls, ["https://b.example/v1", "https://c.example/v1"]);
  assert.equal(owner.env.state.baseUrl, "https://c.example/v1");
  assert.equal(second.baseInput().props.value, "https://c.example/v1");

  const controller = owner.aiSettings;
  owner.render();
  assert.equal(owner.aiSettings, controller, "App 重新渲染不得换控制器或队列");

  second.dispose();
  owner.dispose();
});

test("地址改走又改回或只加末尾斜杠，失焦恢复当前服务的模型列表", async () => {
  const fixture = dialogFixture({ baseUrl: "https://a.example/v1", models: ["a-model"] });
  await fixture.flush();
  await fixture.openAi();
  assert.deepEqual(fixture.listCalls.map((call) => call.baseUrl), ["https://a.example/v1"]);
  const requestsBefore = fixture.requests.length;

  fixture.baseInput().props.onChange({ target: { value: "https://b.example/v1" } });
  fixture.baseInput().props.onChange({ target: { value: "https://a.example/v1" } });
  fixture.baseInput().props.onBlur();
  await fixture.flush(16);
  assert.equal(fixture.requests.length, requestsBefore, "改回已保存地址不应写设置");
  assert.equal(fixture.listCalls.at(-1).baseUrl, "https://a.example/v1");
  assert.ok(fixture.listSelect(), "列表应恢复当前服务");

  fixture.baseInput().props.onChange({ target: { value: "https://a.example/v1/" } });
  fixture.baseInput().props.onBlur();
  await fixture.flush(16);
  assert.equal(fixture.requests.length, requestsBefore, "只加末尾斜杠不应写设置");
  assert.equal(fixture.listCalls.at(-1).baseUrl, "https://a.example/v1");
  assert.ok(fixture.listSelect());
  fixture.dispose();
});

test("密钥意图立即失效旧模型列表和旧错误，不等队列前面的设置写入结束", async () => {
  const fixture = dialogFixture({ baseUrl: "https://a.example/v1", models: ["a-model"] });
  await fixture.flush();
  await fixture.openAi();
  assert.ok(fixture.listSelect());

  const gate = fixture.gateSave();
  fixture.modelInput().props.onChange({ target: { value: "m2" } });
  fixture.modelInput().props.onBlur();
  await fixture.flush();
  assert.ok(fixture.listSelect(), "模型保存排队时旧列表仍在");

  fixture.failNextList({
    kind: "rejected",
    code: "errors.ai.modelsUnavailable",
    params: {},
    message: "无法获取模型列表",
  });
  fixture.button(t("settings.ai.modelsRefresh")).props.onClick();
  await fixture.flush(16);
  assert.ok(fixture.nodes().some((node) => node.props?.role === "alert" && textOf(node) === t("errors.ai.modelsUnavailable")));

  fixture.keyInput().props.onChange({ target: { value: "sk-test" } });
  await fixture.flush();
  fixture.button(t("settings.ai.saveKey")).props.onClick();
  await fixture.flush();
  assert.equal(fixture.listSelect(), undefined, "密钥意图应立即失效旧列表");
  assert.equal(
    fixture.nodes().some((node) => node.props?.role === "alert" && textOf(node) === t("errors.ai.modelsUnavailable")),
    false,
    "密钥意图应立即失效旧错误",
  );

  gate.resolve();
  await fixture.flush(24);
  assert.equal(fixture.state.keySet, true);
  fixture.dispose();
});

test("切页再返回后重新输入同一密钥，旧完成回调不清空新输入", async () => {
  const fixture = dialogFixture({ baseUrl: "https://a.example/v1" });
  await fixture.flush();
  await fixture.openAi();
  const gate = fixture.gateKey();

  fixture.keyInput().props.onChange({ target: { value: "sk-same" } });
  await fixture.flush();
  fixture.button(t("settings.ai.saveKey")).props.onClick();
  await fixture.flush();
  assert.equal(fixture.keyCalls.length, 1);

  await fixture.openPage("appearance");
  await fixture.openPage("ai");
  fixture.keyInput().props.onChange({ target: { value: "sk-same" } });
  await fixture.flush();
  assert.equal(fixture.keyInput().props.value, "sk-same");

  gate.resolve();
  await fixture.flush(24);
  assert.equal(fixture.keyInput().props.value, "sk-same", "旧页面回调不得清掉切页后重新输入的同一字符串");
  assert.equal(fixture.state.keySet, true);
  fixture.dispose();
});

test("真实关闭重开后重新输入同一密钥，旧完成回调不清空新输入", async () => {
  const owner = ownerFixture({ baseUrl: "https://a.example/v1" });
  const first = owner.dialog();
  await first.flush();
  await first.openAi();
  const gate = owner.env.gateKey();

  first.keyInput().props.onChange({ target: { value: "sk-same" } });
  await first.flush();
  first.button(t("settings.ai.saveKey")).props.onClick();
  await first.flush();
  assert.equal(owner.env.keyCalls.length, 1);

  first.dispose();
  const second = owner.dialog();
  await second.flush();
  await second.openAi();
  second.keyInput().props.onChange({ target: { value: "sk-same" } });
  await second.flush();
  assert.equal(second.keyInput().props.value, "sk-same");

  gate.resolve();
  await second.flush(24);
  assert.equal(second.keyInput().props.value, "sk-same", "旧页面回调不得清掉重开后重新输入的同一字符串");
  assert.equal(owner.env.state.keySet, true);

  second.dispose();
  owner.dispose();
});

test("密钥意图也使队列等待期间到达的旧状态错误失效", async () => {
  const fixture = dialogFixture({ baseUrl: "https://a.example/v1" });
  await fixture.flush();
  await fixture.openAi();
  await fixture.openPage("appearance");
  const status = fixture.deferStatus();
  await fixture.openPage("ai");

  const gate = fixture.gateSave();
  fixture.modelInput().props.onChange({ target: { value: "m2" } });
  fixture.modelInput().props.onBlur();
  await fixture.flush();

  fixture.keyInput().props.onChange({ target: { value: "sk-test" } });
  await fixture.flush();
  fixture.button(t("settings.ai.saveKey")).props.onClick();
  await fixture.flush();

  status.reject({
    kind: "rejected",
    code: "errors.ai.server",
    params: {},
    message: "服务错误",
  });
  await fixture.flush(16);
  assert.equal(
    fixture.nodes().some((node) => node.props?.role === "alert"),
    false,
    "密钥意图之后到达的旧状态错误不得显示",
  );

  gate.resolve();
  await fixture.flush(24);
  assert.equal(fixture.state.keySet, true);
  fixture.dispose();
});

for (const operation of ["saveKey", "clearKey"]) {
  test(`地址保存后等待 ${operation} 完成期间不获取模型，全部密钥操作结束后才刷新`, async () => {
    const fixture = dialogFixture({ baseUrl: "https://a.example/v1", keySet: true });
    await fixture.flush();
    await fixture.openAi();
    const previousList = fixture.deferList();
    fixture.button(t("settings.ai.modelsRefresh")).props.onClick();
    await fixture.flush();
    const listCount = fixture.listCalls.length;
    const address = fixture.gateSave();
    const key = fixture.gateKey();
    fixture.baseInput().props.onChange({ target: { value: "https://b.example/v1" } });
    fixture.baseInput().props.onBlur();
    fixture.keyInput().props.onChange({ target: { value: "fixture-pending-key" } });
    await fixture.flush();
    fixture.button(t(operation === "saveKey" ? "settings.ai.saveKey" : "settings.ai.clearKey")).props.onClick();
    address.resolve();
    await fixture.flush(24);
    assert.equal(fixture.state.baseUrl, "https://b.example/v1");
    assert.equal((operation === "saveKey" ? fixture.keyCalls : fixture.clearCalls).length, 1);
    assert.equal(fixture.listCalls.length, listCount, "地址保存不能用尚未更新的凭据抢先拉模型");
    if (operation === "saveKey") {
      previousList.resolve({ baseUrl: "https://a.example/v1", models: ["obsolete-model"] });
    } else {
      previousList.reject({ kind: "rejected", code: "errors.ai.auth", params: {}, message: "fixture obsolete error" });
    }
    await fixture.flush(16);
    assert.equal(fixture.listSelect(), undefined);
    assert.equal(fixture.nodes().some((node) => node.props?.role === "alert"), false);
    // 第二个密钥意图仍排队时，第一个操作完成也不能抢先刷新。
    fixture.keyInput().props.onChange({ target: { value: "fixture-final-key" } });
    await fixture.flush();
    fixture.button(t("settings.ai.saveKey")).props.onClick();
    const finalKey = fixture.gateKey();
    key.resolve();
    await fixture.flush(24);
    assert.equal(fixture.listCalls.length, listCount);
    finalKey.resolve();
    await fixture.flush(24);
    assert.equal(fixture.listCalls.length, listCount + 1);
    assert.equal(fixture.listCalls.at(-1).baseUrl, "https://b.example/v1");
    assert.ok(fixture.listSelect());
    fixture.dispose();
  });
}
