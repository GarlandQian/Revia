/**
 * 提交区的输入状态（M3 F02.5、F04.2）。
 *
 * 关键约束：
 * - 草稿的异步读取/保存必须绑定 **worktree 与代际**，旧回包不得覆盖新输入；
 * - 「跳过 hooks」只属于本次命令：不写进持久草稿，也不在切换 worktree 时保留；
 * - 读取落地用「请求发起时」捕获的代际，不是 await 回来后才取，否则旧读取
 *   会被当成新代际而覆盖用户输入。
 */

import type { CommitDraft } from "@/types/bindings/CommitDraft";

export interface ComposerInput {
  message: string;
  amend: boolean;
  /** 仅本次命令有效，不持久化（F04.2）。 */
  skipHooks: boolean;
}

export const EMPTY_INPUT: ComposerInput = { message: "", amend: false, skipHooks: false };

/** 切换 worktree 或提交完成后复位输入，并**总是**清掉本次的跳过 hooks。 */
export function resetInput(): ComposerInput {
  return EMPTY_INPUT;
}

export function inputEquals(a: ComposerInput, b: ComposerInput): boolean {
  return a.message === b.message && a.amend === b.amend && a.skipHooks === b.skipHooks;
}

/**
 * 草稿 → 输入。持久草稿里不含 skipHooks（历史上写进去的也一律忽略），
 * 因此恢复后不会自动跳过 hooks。
 */
export function draftToInput(draft: CommitDraft | null): ComposerInput | null {
  if (!draft) return null;
  return { message: draft.message, amend: draft.amend, skipHooks: false };
}

/** 读取请求发起时的上下文快照：await 回来只按它判断是否还能落地。 */
export interface LoadTicket {
  worktreeId: string;
  /** 请求发起时的输入代际。 */
  generation: number;
  /** 请求发起时的 worktree 激活代号（切换后旧读取作废）。 */
  activation: number;
}

/**
 * 提交区状态机（纯逻辑，无 React 依赖，便于确定性核对）。
 */
export class CommitComposerState {
  private worktreeId: string | null = null;
  /** 输入代际：每次编辑/切换/清空递增。 */
  private generation = 0;
  private input: ComposerInput = EMPTY_INPUT;
  /** 是否已经有本地输入（草稿读取只在没有本地输入时落地）。 */
  private touched = false;
  /** 最近一次成功写入磁盘的输入（判断「提交成功后草稿是否还需要清」）。 */
  private persisted: ComposerInput | null = null;
  /** 用户是否正在提交（期间禁止异步读取覆盖输入）。 */
  private submitting = false;
  /** 模板是否已经初始化过（只在没有草稿时初始化一次，F02.3）。 */
  private templateSeeded = false;

  reset(worktreeId: string | null) {
    this.worktreeId = worktreeId;
    this.generation += 1;
    this.input = EMPTY_INPUT;
    this.touched = false;
    this.persisted = null;
    this.submitting = false;
    this.templateSeeded = false;
  }

  current(): { worktreeId: string | null; generation: number; input: ComposerInput; touched: boolean } {
    return {
      worktreeId: this.worktreeId,
      generation: this.generation,
      input: this.input,
      touched: this.touched,
    };
  }

  /** 用户编辑：推进代际并标记已触碰（此后旧读取一律作废）。 */
  edit(next: ComposerInput) {
    this.input = next;
    this.touched = true;
    this.generation += 1;
  }

  setSubmitting(value: boolean) {
    this.submitting = value;
  }

  isSubmitting(): boolean {
    return this.submitting;
  }

  /** 读取请求发起时取票：绑定 worktree 与**此刻**的代际（不是 await 回来的代际）。 */
  ticket(worktreeId: string, activation: number): LoadTicket {
    return { worktreeId, generation: this.generation, activation };
  }

  /**
   * 草稿读取落地：只有「同一 worktree」「代际未变」「没有本地输入」「不在提交中」
   * 时才应用；否则丢弃这次读取（F02.5）。
   */
  applyLoaded(ticket: LoadTicket, currentActivation: number, draft: CommitDraft | null):
    | { applied: true; input: ComposerInput }
    | { applied: false; reason: "stale" | "touched" | "submitting" | "worktree" | "activation" } {
    if (this.worktreeId !== ticket.worktreeId) return { applied: false, reason: "worktree" };
    if (ticket.activation !== currentActivation) return { applied: false, reason: "activation" };
    if (ticket.generation !== this.generation) return { applied: false, reason: "stale" };
    if (this.touched) return { applied: false, reason: "touched" };
    if (this.submitting) return { applied: false, reason: "submitting" };
    const input = draftToInput(draft);
    if (!input) return { applied: false, reason: "stale" };
    this.input = input;
    this.persisted = input;
    this.generation += 1;
    return { applied: true, input };
  }

  /** 记录一次成功保存；只有仍是当前代际的输入才算「已持久化」。 */
  recordSaved(worktreeId: string, generation: number, input: ComposerInput): boolean {
    if (this.worktreeId !== worktreeId || generation !== this.generation) return false;
    this.persisted = input;
    return true;
  }

  /** 提交成功：只有本地输入仍等于这次提交的输入时才认为可以清除界面。 */
  shouldClearAfterCommit(worktreeId: string, submitted: ComposerInput): boolean {
    if (this.worktreeId !== worktreeId) return false;
    return inputEquals(this.input, submitted);
  }

  /** 清除本地输入（提交成功且草稿已清）。 */
  clear() {
    this.templateSeeded = true;
    this.input = EMPTY_INPUT;
    this.touched = false;
    this.persisted = null;
    this.generation += 1;
  }

  persistedInput(): ComposerInput | null {
    return this.persisted;
  }

  /** 是否还没有被模板初始化过（每个 worktree 会话只初始化一次）。 */
  needsTemplateSeed(): boolean {
    return !this.templateSeeded;
  }

  markTemplateSeeded() {
    this.templateSeeded = true;
  }
}
