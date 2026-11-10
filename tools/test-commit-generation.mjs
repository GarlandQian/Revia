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
const { CommitMessageGeneration } = await import("../src/lib/commitMessageGeneration.ts");
const { CommitComposerState } = await import("../src/lib/commitDraft.ts");
imports.deregister();

const STAGED = { kind: "staged" };

/**
 * 受控 harness：传输、事件与提交输入状态都经生产 module。
 * `mirror` 相当于 App 镜像到界面的输入；`inputs` 记录 module 每次写入后的草稿同步。
 */
function createHarness(initial = { message: "旧信息", amend: false, skipHooks: false }) {
  const state = new CommitComposerState();
  state.reset("A");
  state.edit(initial);
  const generate = [];
  const cancel = [];
  const inputs = [];
  let handlers = null;
  let mirror = state.current().input;
  let seq = 0;
  const generation = new CommitMessageGeneration({
    state,
    transport: {
      generate: (request) => new Promise((resolve, reject) => {
        generate.push({ ...request, resolve, reject });
      }),
      cancel: (requestId) => {
        cancel.push(requestId);
        return Promise.resolve(true);
      },
    },
    events: {
      attach: (next) => {
        handlers = next;
        return () => {
          handlers = null;
        };
      },
    },
    onInput: (write) => {
      inputs.push(write);
      mirror = write.input;
    },
    onChange: () => undefined,
    newRequestId: () => `req-${++seq}`,
  });
  generation.attach();
  return {
    state,
    generation,
    generate,
    cancel,
    inputs,
    handlers: () => handlers,
    emit: {
      delta: (requestId, text) => handlers.onDelta({ requestId, text }),
      done: (requestId, text, cancelled) => handlers.onDone({ requestId, text, cancelled }),
      error: (requestId, error) => handlers.onError({ requestId, error }),
    },
    mirror: () => mirror,
    /** 模拟切换 worktree：重置提交输入并重新镜像（与 App 一致）。 */
    reset(worktreeId) {
      generation.resetTarget(worktreeId);
      mirror = state.current().input;
    },
    /** 模拟用户在界面上编辑：App 会推进同一份提交输入状态。 */
    edit(message) {
      const next = { ...state.current().input, message };
      state.edit(next);
      mirror = next;
    },
  };
}

test("流式完成：增量逐次落地，完成后可一步撤销到生成前文字", () => {
  const h = createHarness();
  const target = h.generation.captureTarget("A");
  assert.ok(target);
  const requestId = h.generation.begin({ target, mode: STAGED });
  assert.equal(requestId, "req-1");
  assert.equal(h.generate.length, 1);
  assert.equal(h.generate[0].worktreeId, "A");
  assert.deepEqual(h.generate[0].mode, STAGED);
  assert.equal(h.mirror().message, "", "开始生成时清空输入框");
  assert.equal(h.generation.view().generating, true);

  h.emit.delta(requestId, "feat");
  assert.equal(h.mirror().message, "feat");
  h.emit.delta(requestId, ": add");
  assert.equal(h.mirror().message, "feat: add");

  h.emit.done(requestId, "feat: add undo", false);
  assert.equal(h.mirror().message, "feat: add undo");
  assert.equal(h.generation.view().generating, false);
  assert.equal(h.generation.view().canUndo, true);

  // 每次写入只推进一代输入：草稿保存按代次排队，不能被 module 与界面各推一次。
  for (let index = 1; index < h.inputs.length; index += 1) {
    assert.equal(h.inputs[index].generation, h.inputs[index - 1].generation + 1);
  }

  assert.equal(h.generation.undoLast(), true);
  assert.equal(h.mirror().message, "旧信息");
  assert.equal(h.generation.view().canUndo, false);
});

test("取消：先失效本地资格，再请求后端；迟到事件不能改写已恢复的文字", () => {
  const h = createHarness();
  const target = h.generation.captureTarget("A");
  const requestId = h.generation.begin({ target, mode: STAGED });
  h.emit.delta(requestId, "半截");
  assert.equal(h.mirror().message, "半截");

  assert.equal(h.generation.cancel(), requestId);
  assert.deepEqual(h.cancel, [requestId]);
  assert.equal(h.mirror().message, "旧信息");
  assert.equal(h.generation.view().generating, false);

  // 取消之后才到的增量 / 完成 / 错误都无权改写。
  h.emit.delta(requestId, "迟到");
  h.emit.done(requestId, "迟到完成", false);
  h.emit.error(requestId, { kind: "rejected", message: "迟到错误" });
  assert.equal(h.mirror().message, "旧信息");
  assert.equal(h.generation.view().canUndo, false);
});

test("失败：恢复生成前文字并展示原因；迟到的完成不覆盖", () => {
  const h = createHarness();
  const target = h.generation.captureTarget("A");
  const requestId = h.generation.begin({ target, mode: STAGED });
  h.emit.delta(requestId, "半截");
  const failure = { kind: "environment", message: "AI 不可用" };

  h.emit.error(requestId, failure);
  assert.equal(h.mirror().message, "旧信息");
  assert.deepEqual(h.generation.view().error, failure);
  assert.equal(h.generation.view().generating, false);
  h.emit.done(requestId, "迟到完成", false);
  assert.equal(h.mirror().message, "旧信息");

  h.generation.dismissError();
  assert.equal(h.generation.view().error, null);
});

test("请求本身失败（invoke 拒绝）同样恢复并展示原因", async () => {
  const h = createHarness();
  const target = h.generation.captureTarget("A");
  const requestId = h.generation.begin({ target, mode: STAGED });
  const failure = new Error("invoke failed");
  h.generate[0].reject(failure);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(h.mirror().message, "旧信息");
  assert.equal(h.generation.view().generating, false);
  assert.equal(h.generation.view().error, failure);
  h.emit.delta(requestId, "迟到");
  assert.equal(h.mirror().message, "旧信息");
});

test("新请求接替旧请求：旧请求先被取消，旧事件随即失去资格", () => {
  const h = createHarness();
  const first = h.generation.begin({ target: h.generation.captureTarget("A"), mode: STAGED });
  h.emit.delta(first, "旧");
  const second = h.generation.begin({ target: h.generation.captureTarget("A"), mode: { kind: "amend" } });
  assert.notEqual(second, first);
  assert.deepEqual(h.cancel, [first]);
  h.emit.delta(first, "旧迟到");
  h.emit.done(first, "旧完成", false);
  assert.equal(h.mirror().message, "");
  h.emit.delta(second, "新");
  assert.equal(h.mirror().message, "新");
});

test("取消在请求登记前发生：后端仍收到取消，迟到的完成不生效", () => {
  const h = createHarness();
  const requestId = h.generation.begin({ target: h.generation.captureTarget("A"), mode: STAGED });
  assert.equal(h.generation.cancel(), requestId);
  assert.deepEqual(h.cancel, [requestId]);
  // 后端此时才把请求登记完并返回。
  h.generate[0].resolve();
  h.emit.done(requestId, "迟到完成", false);
  assert.equal(h.mirror().message, "旧信息");
});

test("手动输入后：旧事件与撤销都失效，不覆盖用户后来的编辑", () => {
  const h = createHarness();
  const requestId = h.generation.begin({ target: h.generation.captureTarget("A"), mode: STAGED });
  h.emit.done(requestId, "生成结果", false);
  assert.equal(h.generation.view().canUndo, true);

  h.edit("用户后来写的");
  assert.equal(h.generation.undoLast(), false);
  assert.equal(h.mirror().message, "用户后来写的");
  assert.equal(h.generation.view().canUndo, false);
});

test("生成期间手动编辑：目标失配，迟到事件丢弃并请求后端取消", () => {
  const h = createHarness();
  const requestId = h.generation.begin({ target: h.generation.captureTarget("A"), mode: STAGED });
  h.emit.delta(requestId, "生成中");
  h.edit("用户接管");

  h.emit.delta(requestId, "迟到");
  assert.deepEqual(h.cancel, [requestId]);
  assert.equal(h.mirror().message, "用户接管");
  assert.equal(h.generation.view().generating, false);
});

test("A→B→A：切换后的迟到回包不能写进新目标，重新发起仍可用", () => {
  const h = createHarness();
  const requestId = h.generation.begin({ target: h.generation.captureTarget("A"), mode: STAGED });
  h.emit.delta(requestId, "A 的内容");

  // App 切换 worktree 时先让请求失效，再重置提交输入（并重新镜像到界面）。
  h.generation.invalidate();
  h.reset("B");
  h.reset("A");

  h.emit.done(requestId, "迟到完成", false);
  assert.equal(h.mirror().message, "");
  assert.equal(h.generation.view().canUndo, false);

  const again = h.generation.begin({ target: h.generation.captureTarget("A"), mode: STAGED });
  assert.ok(again);
  h.emit.delta(again, "新目标");
  assert.equal(h.mirror().message, "新目标");
});

test("异步确认回包时目标已失效：不再发起生成", () => {
  const h = createHarness();
  const target = h.generation.captureTarget("A");
  assert.ok(target);
  // 确认对话框打开期间切换了 worktree。
  h.reset("B");
  assert.equal(h.generation.begin({ target, mode: STAGED }), null);
  assert.equal(h.generate.length, 0);
  assert.equal(h.mirror().message, "", "未发起生成时保持切换后的界面不变");
});

test("取消与失败只恢复仍归属本次生成的输入", () => {
  const h = createHarness();
  const requestId = h.generation.begin({ target: h.generation.captureTarget("A"), mode: STAGED });
  h.emit.delta(requestId, "生成中");
  // 用户在生成期间手动编辑（同一 worktree，但输入代次已变）。
  h.edit("手动");
  // 此时迟到的失败没有资格恢复，不能把用户的手动输入改回生成前文字。
  h.emit.error(requestId, { kind: "rejected", message: "失败" });
  assert.equal(h.mirror().message, "手动");
  assert.equal(h.generation.view().error, null);
});

test("切换前已手动接管输入：resetTarget 不恢复旧生成原文", () => {
  const h = createHarness();
  const requestId = h.generation.begin({ target: h.generation.captureTarget("A"), mode: STAGED });
  h.emit.delta(requestId, "生成中");
  h.edit("用户接管");
  const count = h.inputs.length;
  h.reset("B");
  assert.equal(h.inputs.length, count, "不能把生成前文字重新交给草稿保存");
  assert.deepEqual(h.cancel, [requestId]);
});

test("控制面引用稳定：重新渲染与取视图不取消在途请求（SP1）", () => {
  const h = createHarness();
  const control = h.generation.control(() => "A");
  // App 在每次渲染都取一次控制面；模块保证返回同一个对象。
  assert.equal(h.generation.control(() => "A"), control, "control face must be stable");
  const intent = control.captureIntent(STAGED, "");
  assert.ok(intent);
  control.begin(intent);
  assert.equal(control.view().generating, true);

  // 模拟生成期间的多次重新渲染：取快照、重新取控制面都不会取消请求。
  for (let index = 0; index < 5; index += 1) {
    control.view();
    h.generation.control(() => "A");
  }
  assert.deepEqual(h.cancel, [], "re-rendering must not cancel the generation");
  assert.equal(control.view().generating, true);
  assert.equal(h.mirror().message, "", "still generating into the composer");

  // 生命周期只由显式意图驱动：用户取消仍然生效。
  control.cancel();
  assert.deepEqual(h.cancel, ["req-1"]);
  assert.equal(control.view().generating, false);
  assert.equal(h.mirror().message, "旧信息", "cancel restores the pre-generation text");
});

test("点击时捕获完整意图：等待与确认期间的目标、文字与模式变化都使点击失效（SP5）", () => {
  const h = createHarness();
  const control = h.generation.control(() => "A");
  const selection = { revision: "r1", identity: "cl-1" };
  const mode = { kind: "changelist", selection, amend: false };

  const intent = control.captureIntent(mode, h.mirror().message);
  assert.ok(intent);
  assert.equal(intent.message, "旧信息", "the click-time text is captured");
  assert.deepEqual(intent.mode, mode);

  // 等待 aiStatus 期间用户清空输入：替换确认按点击时的文字判定，但这次点击已失效。
  h.edit("");
  assert.equal(control.intentStillValid(intent, mode), false, "edited input invalidates the click");
  // 用户又改回相同文字也一样：代次已经推进。
  h.edit("旧信息");
  assert.equal(control.intentStillValid(intent, mode), false);

  // 重新捕获后，切换 worktree 使意图失效。
  const fresh = control.captureIntent(mode, "");
  assert.ok(fresh);
  h.reset("B");
  assert.equal(control.intentStillValid(fresh, mode), false, "worktree switch invalidates the click");

  // 回到 A 重新捕获：模式（含勾选内容）变了也失效。
  h.reset("A");
  const again = control.captureIntent(mode, "");
  assert.ok(again);
  const otherSelection = { revision: "r2", identity: "cl-2" };
  assert.equal(
    control.intentStillValid(again, { kind: "changelist", selection: otherSelection, amend: false }),
    false,
    "changed changelist selection invalidates the click",
  );
  assert.equal(
    control.intentStillValid(again, { kind: "changelist", selection: { ...selection }, amend: false }),
    true,
    "an equal selection (new object) keeps the click valid",
  );
  assert.equal(control.intentStillValid(again, STAGED), false, "changed mode invalidates the click");

  // 确认回包时仍有效：用捕获的模式发起，输入代次与点击时一致。
  const requestId = control.begin(again);
  assert.equal(requestId, "req-1");
  assert.deepEqual(h.generate[0].mode, mode, "generation uses the captured mode");
});
