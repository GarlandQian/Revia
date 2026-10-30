/**
 * 提交草稿的持久化顺序：按 worktree 串行，编辑去抖，提交/关闭前冲刷。
 * 输入与身份归提交区管理；本模块只负责落盘与失败记账，不丢弃本地输入。
 */

import type { ComposerInput } from "@/lib/commitDraft";
import type { CommitDraft } from "@/types/bindings/CommitDraft";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { getErrorMessage, toAppError } from "@/lib/error";
import { ipc } from "@/lib/ipc";

const DRAFT_DEBOUNCE_MS = 400;

export interface DraftWrite {
  worktreeId: string;
  input: ComposerInput;
  revision: string;
  generation: number;
}

interface DraftSettlement {
  write: DraftWrite;
  error: AppErrorPayload | null;
}

export class CommitDraftPersistence {
  private queues = new Map<string, Promise<void>>();
  private failures = new Map<string, AppErrorPayload>();
  private pending: DraftWrite | null = null;
  private timer: number | null = null;
  private saveEpoch = 0;
  private onSettled: (settlement: DraftSettlement) => void;

  constructor(onSettled: (settlement: DraftSettlement) => void) {
    this.onSettled = onSettled;
  }

  /** 只等已排队的写入；读取草稿前可等待，失败记录不阻止读取。 */
  waitFor(worktreeId: string): Promise<void> {
    return this.queues.get(worktreeId) ?? Promise.resolve();
  }

  failureFor(worktreeId: string): string | null {
    const error = this.failurePayloadFor(worktreeId);
    return error ? getErrorMessage(error) : null;
  }

  failurePayloadFor(worktreeId: string): AppErrorPayload | null {
    return this.failures.get(worktreeId) ?? null;
  }

  /** 失败会留在记账中，不拒绝后台队列，也不阻断下一次显式保存。 */
  save(write: DraftWrite): Promise<void> {
    const epoch = ++this.saveEpoch;
    const next = this.waitFor(write.worktreeId).then(async () => {
      let error: AppErrorPayload | null = null;
      try {
        if (inputDirty(write.input)) {
          await ipc.saveCommitDraft({
            worktreeId: write.worktreeId,
            draft: inputToDraft(write.input, write.revision),
          });
        } else {
          await ipc.clearCommitDraft(write.worktreeId);
        }
        this.failures.delete(write.worktreeId);
      } catch (caught) {
        error = toAppError(caught);
        this.failures.set(write.worktreeId, error);
      }
      // 保存提示只属于最新发起的写入；旧失败仍保留，供提交与关闭核对。
      if (epoch === this.saveEpoch) this.onSettled({ write, error });
    });
    this.queues.set(write.worktreeId, next);
    return next;
  }

  schedule(write: DraftWrite): void {
    if (this.pending && this.pending.worktreeId !== write.worktreeId) {
      this.savePending();
    }
    this.pending = write;
    if (this.timer !== null) window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => this.savePending(), DRAFT_DEBOUNCE_MS);
  }

  /**
   * 等到目标写入稳定后再检查失败；等待期间新排队的编辑也不能越过关闭。
   * 不自动重试失败，不冲刷其它 worktree 的去抖输入。
   */
  async flush(worktreeId?: string): Promise<void> {
    while (true) {
      if (this.pending && (worktreeId === undefined || this.pending.worktreeId === worktreeId)) {
        this.savePending();
      }
      const targets = worktreeId === undefined ? [...this.queues.keys()] : [worktreeId];
      const waiting = targets.map((target) => this.queues.get(target));
      await Promise.all(waiting);
      const hasPending = this.pending !== null
        && (worktreeId === undefined || this.pending.worktreeId === worktreeId);
      const changed = targets.some((target, index) => this.queues.get(target) !== waiting[index])
        || (worktreeId === undefined && this.queues.size !== targets.length);
      if (hasPending || changed) continue;
      for (const target of targets) {
        const failure = this.failureFor(target);
        if (failure !== null) throw Object.assign(new Error(failure), this.failurePayloadFor(target));
      }
      return;
    }
  }

  private savePending(): void {
    if (this.timer !== null) window.clearTimeout(this.timer);
    this.timer = null;
    const write = this.pending;
    this.pending = null;
    if (write) void this.save(write);
  }
}

/** 是否需要保存这份输入（全空则不写盘，避免留下空草稿）。 */
function inputDirty(input: ComposerInput): boolean {
  return input.message.trim().length > 0 || input.amend;
}

/** 写盘用的草稿（skipHooks 恒 false，不进入持久状态）。 */
function inputToDraft(input: ComposerInput, revision: string): CommitDraft {
  return { message: input.message, amend: input.amend, skipHooks: false, savedAtMs: 0, revision };
}
