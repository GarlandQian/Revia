/**
 * 生成提交信息的目标归属（M10 任务 03）。
 *
 * 同一个 module 持有：
 * - **请求生命周期**：requestId、进行中状态、流式文字、取消、失败、完成与一步撤销；
 * - **目标资格**：worktree + 提交输入代次。生成的每一次写入都经 `CommitComposerState.edit`
 *   推进代次（沿用既有草稿保存语义），module 记住自己**最后一次写入**的代次；
 *   用户手动编辑、切换 worktree、清空输入都会让这个代次失配，迟到事件与撤销随
 *   即失去资格；
 * - **迟到事件**：delta / done / error / invoke 拒绝 / 异步确认回包都先核对归属，
 *   不属于当前请求的一律丢弃；
 * - **取消顺序**：先在本地失效资格，再请求后端取消；后端取消失败也不会让迟到结果生效。
 *
 * 显示层只表达意图（开始、取消、撤销），不再自己维护 requestId、生成前文字或流式缓冲。
 */
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { ChangelistSelection } from "@/types/bindings/ChangelistSelection";
import type { ComposerInput } from "./commitDraft";
import { CommitComposerState } from "./commitDraft";

/** 与 `src/lib/ipc.ts` 的 `AiCommitMode` 结构一致（只依赖结构，避免把 IPC 拖进测试）。 */
export type GenerationMode =
  | { kind: "staged" }
  | { kind: "changelist"; selection: ChangelistSelection; amend?: boolean }
  | { kind: "amend" };

/** 发起生成时的目标输入。 */
export interface GenerationTarget {
  worktreeId: string;
  /** 用户发起生成时（或确认回包前）的输入代次。 */
  generation: number;
}

/**
 * 一次「生成」点击的完整意图（M10 返修 SP5）。
 *
 * 点击时就把目标、模式与当时的文字一起定下来：等待配置、替换确认与首次发送确认期间
 * 的任何变化（切换 worktree、编辑输入、改模式或勾选）都会让这份意图失效，
 * 不会用点击时的旧文字去判定新输入，也不会把旧点击发到新 worktree 上。
 */
export interface GenerationIntent {
  target: GenerationTarget;
  mode: GenerationMode;
  /** 点击那一刻的输入文字（只用于「是否要替换确认」判定）。 */
  message: string;
}

/** 传输与事件 adapter：生产接现有 IPC 与 Tauri 事件，测试接受控实现。 */
export interface GenerationTransport {
  generate(request: { requestId: string; worktreeId: string; mode: GenerationMode }): Promise<void>;
  cancel(requestId: string): Promise<unknown>;
}

export interface GenerationHandlers {
  onDelta: (event: { requestId: string; text: string }) => void;
  onDone: (event: { requestId: string; text: string; cancelled: boolean }) => void;
  onError: (event: { requestId: string; error: AppErrorPayload }) => void;
}

export interface GenerationEventSource {
  /** 安装监听；返回卸载函数。监听本身安装一次，事件按请求归属过滤。 */
  attach(handlers: GenerationHandlers): () => void;
}

export interface GenerationDeps {
  /** 与提交输入共享的目标状态（唯一真值）。 */
  state: CommitComposerState;
  transport: GenerationTransport;
  events: GenerationEventSource;
  /** 生成写入后同步界面镜像与草稿保存（代次已由 module 推进）。 */
  onInput: (write: GeneratedInput) => void;
  /** 生成状态变化时通知显示层重新取快照。 */
  onChange: () => void;
  /** 新请求 ID；测试可注入确定性实现。 */
  newRequestId?: () => string;
}

/** 草稿保存使用生成实际写入的目标，不能从当前界面重新推断归属。 */
export interface GeneratedInput {
  worktreeId: string;
  generation: number;
  input: ComposerInput;
}

export interface GenerationView {
  requestId: string | null;
  generating: boolean;
  error: AppErrorPayload | null;
  canUndo: boolean;
}

/**
 * 显示层使用的控制面：由 module 生成一次，界面重新渲染不会换引用。
 *
 * 生命周期只由显式意图（开始、取消、撤销、目标失效）驱动，不受呈现对象引用变化影响。
 */
export interface GenerationControl {
  view(): GenerationView;
  /** 点击生成时捕获完整意图；没有可用目标时返回 null。 */
  captureIntent(mode: GenerationMode, message: string): GenerationIntent | null;
  /** 异步确认回包后核对：目标与模式必须仍是点击时那一份。 */
  intentStillValid(intent: GenerationIntent, mode: GenerationMode): boolean;
  begin(intent: GenerationIntent): string | null;
  cancel(): string | null;
  undo(): boolean;
  dismissError(): void;
}

/** 模式的深比较：勾选内容相同即视为同一模式（对象身份不重要）。 */
export function sameGenerationMode(a: GenerationMode, b: GenerationMode): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind !== "changelist" || b.kind !== "changelist") return true;
  return (
    (a.amend ?? false) === (b.amend ?? false) &&
    a.selection.revision === b.selection.revision &&
    a.selection.identity === b.selection.identity
  );
}

interface UndoBaseline {
  worktreeId: string;
  /** 完成写入后的输入代次：之后的任何编辑都让撤销失效。 */
  generation: number;
  message: string;
}

interface Ownership {
  worktreeId: string;
  /** module 最后一次写入后的输入代次。 */
  generation: number;
}

export class CommitMessageGeneration {
  private readonly deps: GenerationDeps;
  private requestId: string | null = null;
  /** 本次生成的输入归属（module 最后一次写入后的 worktree + 代次）。 */
  private owned: Ownership | null = null;
  /** 发起生成前的文字：取消、失败与撤销都恢复到它。 */
  private preMessage = "";
  private streamed = "";
  private error: AppErrorPayload | null = null;
  private undo: UndoBaseline | null = null;
  private detach: (() => void) | null = null;
  /** 稳定控制面（M10 返修 SP1）：界面重新渲染不换引用。 */
  private controlFace: GenerationControl | null = null;
  private readonly newRequestId: () => string;

  constructor(deps: GenerationDeps) {
    this.deps = deps;
    this.newRequestId = deps.newRequestId ?? (() => crypto.randomUUID());
  }

  /** 安装 IPC 事件监听；返回卸载函数，忽略重复调用。 */
  attach(): () => void {
    if (this.detach) return this.detach;
    const detach = this.deps.events.attach({
      onDelta: (event) => {
        this.delta(event.requestId, event.text);
      },
      onDone: (event) => {
        this.done(event.requestId, event.text, event.cancelled);
      },
      onError: (event) => {
        this.fail(event.requestId, event.error);
      },
    });
    this.detach = () => {
      detach();
      this.detach = null;
    };
    return this.detach;
  }

  view(): GenerationView {
    return {
      requestId: this.requestId,
      generating: this.requestId !== null,
      error: this.error,
      canUndo: this.canUndo(),
    };
  }

  /** 捕获当前目标：异步确认回包后必须用同一份目标发起（切换/编辑即失配）。 */
  captureTarget(worktreeId: string | null): GenerationTarget | null {
    if (!worktreeId) return null;
    const current = this.deps.state.current();
    if (current.worktreeId !== worktreeId) return null;
    return { worktreeId, generation: current.generation };
  }

  /** 点击生成时捕获完整意图（目标 + 模式 + 当时的文字）。 */
  captureIntent(worktreeId: string | null, mode: GenerationMode, message: string): GenerationIntent | null {
    const target = this.captureTarget(worktreeId);
    return target ? { target, mode, message } : null;
  }

  /** 异步确认回包后核对：目标仍归属当前输入，且模式（含勾选）没有变过。 */
  intentStillValid(intent: GenerationIntent, mode: GenerationMode): boolean {
    const target = this.captureTarget(intent.target.worktreeId);
    if (!target || target.generation !== intent.target.generation) return false;
    return sameGenerationMode(intent.mode, mode);
  }

  /**
   * 开始生成：登记请求、清空输入框（生成前文字留作恢复）、向后端发起请求。
   * 目标资格在捕获与真正发起之间必须一致，否则返回 `null` 且不改动界面。
   */
  begin(input: { target: GenerationTarget; mode: GenerationMode }): string | null {
    const current = this.deps.state.current();
    if (current.worktreeId !== input.target.worktreeId) return null;
    if (current.generation !== input.target.generation) return null;
    // 新请求接替旧请求：旧请求先失去资格，再让后端停掉。
    const previous = this.requestId;
    this.requestId = null;
    this.owned = null;
    this.undo = null;
    if (previous) void this.deps.transport.cancel(previous).catch(() => undefined);

    this.preMessage = current.input.message;
    this.streamed = "";
    this.error = null;
    const requestId = this.newRequestId();
    this.requestId = requestId;
    // 先写空输入框：这一刻起「生成前文字」已由本次请求托管。
    this.write("");
    this.deps.onChange();
    void this.deps.transport
      .generate({ requestId, worktreeId: input.target.worktreeId, mode: input.mode })
      .catch((caught: unknown) => {
        this.fail(requestId, caught as AppErrorPayload);
      });
    return requestId;
  }

  /** 流式增量：只接受当前请求且目标归属仍一致。 */
  delta(requestId: string, text: string): boolean {
    if (!this.accepts(requestId)) return false;
    this.streamed += text;
    this.write(this.streamed);
    this.deps.onChange();
    return true;
  }

  /** 完成：成功内容落地并开放一步撤销；取消则恢复生成前文字。 */
  done(requestId: string, text: string, cancelled: boolean): boolean {
    if (!this.accepts(requestId)) return false;
    this.requestId = null;
    if (cancelled) {
      this.restorePreMessage();
      this.undo = null;
      this.deps.onChange();
      return true;
    }
    this.write(text);
    const owned = this.owned;
    this.undo = owned
      ? { worktreeId: owned.worktreeId, generation: owned.generation, message: this.preMessage }
      : null;
    this.deps.onChange();
    return true;
  }

  /** 失败：恢复生成前文字，并展示原因；迟到的失败不能覆盖新目标。 */
  fail(requestId: string, error: AppErrorPayload): boolean {
    if (!this.accepts(requestId)) return false;
    this.requestId = null;
    this.restorePreMessage();
    this.undo = null;
    this.error = error;
    this.deps.onChange();
    return true;
  }

  /**
   * 用户取消：先在本地失效资格（迟到 delta / done / error 立刻无权改写），
   * 再请求后端取消；后端取消的成败不影响本地已作出的结论。
   */
  cancel(): string | null {
    const requestId = this.requestId;
    if (!requestId) return null;
    this.requestId = null;
    this.restorePreMessage();
    this.undo = null;
    this.deps.onChange();
    void this.deps.transport.cancel(requestId).catch(() => undefined);
    return requestId;
  }

  /** 一步撤销：只撤销仍归属于这次生成、且之后没有被编辑过的输入。 */
  undoLast(): boolean {
    const undo = this.undo;
    if (!undo) return false;
    const current = this.deps.state.current();
    if (current.worktreeId !== undo.worktreeId || current.generation !== undo.generation) {
      this.undo = null;
      this.deps.onChange();
      return false;
    }
    this.write(undo.message);
    this.undo = null;
    this.deps.onChange();
    return true;
  }

  /**
   * 目标失效：先恢复仍归属本次生成的原文，再丢弃资格并让后端停下。
   * 任何仍在途的回包都不会再改写界面。
   */
  invalidate(): void {
    const requestId = this.requestId;
    this.requestId = null;
    if (requestId) this.restorePreMessage();
    this.owned = null;
    this.undo = null;
    this.error = null;
    if (requestId) void this.deps.transport.cancel(requestId).catch(() => undefined);
    this.deps.onChange();
  }

  /** 切换提交输入：恢复与保存旧目标先于状态重置，调用方无需协调先后。 */
  resetTarget(worktreeId: string | null): void {
    this.invalidate();
    this.deps.state.reset(worktreeId);
  }

  /** 清除错误提示（用户关闭或开始新一次生成）。 */
  dismissError(): void {
    if (this.error === null) return;
    this.error = null;
    this.deps.onChange();
  }

  dispose(): void {
    this.invalidate();
    this.detach?.();
  }

  /** 生成一次稳定的控制面：重复调用返回同一对象，生命周期只受显式意图驱动。 */
  control(worktreeId: () => string | null): GenerationControl {
    if (!this.controlFace) {
      this.controlFace = {
        view: () => this.view(),
        captureIntent: (mode, message) => this.captureIntent(worktreeId(), mode, message),
        intentStillValid: (intent, mode) => this.intentStillValid(intent, mode),
        begin: (intent) => this.begin({ target: intent.target, mode: intent.mode }),
        cancel: () => this.cancel(),
        undo: () => this.undoLast(),
        dismissError: () => this.dismissError(),
      };
    }
    return this.controlFace;
  }

  private canUndo(): boolean {
    const undo = this.undo;
    if (!undo) return false;
    const current = this.deps.state.current();
    return current.worktreeId === undo.worktreeId && current.generation === undo.generation;
  }

  /**
   * 请求与目标归属核对：requestId 必须是当前请求，且 module 最后一次写入的代次
   * 仍是当前输入代次。用户在生成期间编辑、切换或清空输入都会让后者失配。
   */
  private accepts(requestId: string): boolean {
    if (this.requestId !== requestId) return false;
    if (!this.ownsTarget()) {
      // 目标已被用户改写：放弃本请求资格，并让后端停掉（不覆盖新输入）。
      this.requestId = null;
      this.owned = null;
      void this.deps.transport.cancel(requestId).catch(() => undefined);
      this.deps.onChange();
      return false;
    }
    return true;
  }

  private ownsTarget(): boolean {
    const owned = this.owned;
    if (!owned) return false;
    const current = this.deps.state.current();
    return current.worktreeId === owned.worktreeId && current.generation === owned.generation;
  }

  /** 经共享的提交输入状态写入一次生成内容（推进代次，沿用草稿保存语义）。 */
  private write(message: string): void {
    const state = this.deps.state;
    const current = state.current();
    if (current.worktreeId === null) return;
    state.edit({ ...current.input, message });
    const next = state.current();
    this.owned = { worktreeId: next.worktreeId!, generation: next.generation };
    this.deps.onInput({ ...this.owned, input: next.input });
  }

  /** 取消 / 失败恢复到生成前文字：只在目标仍归属本次生成时执行。 */
  private restorePreMessage(): void {
    if (!this.ownsTarget()) {
      this.owned = null;
      return;
    }
    this.write(this.preMessage);
    this.owned = null;
  }
}
