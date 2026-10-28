/**
 * 分支与远端写操作的前端编排（M5 B01）。
 *
 * 所有写操作都交给 `runGuardedWrite`：全局写门禁 → 关闭 / 退出保护 → 发 IPC →
 * 真实重读原目标 → 收尾一次；结果待核对时阻止静默退出。这里是纯模块，
 * App 与 Node 回归使用同一份实现。
 *
 * 另外提供几个可以单独测试的决策函数：
 * - 提交并推送时是否弹出推送对话框（B06、B08 的两个设置）；
 * - 推送被拒后是否自动「更新后再推送」（只做一次）；
 * - 更新 / 推送结果如何转成通知文案。
 */

import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { CheckoutStrategy } from "@/types/bindings/CheckoutStrategy";
import type { CheckoutTarget } from "@/types/bindings/CheckoutTarget";
import type { CreateBranchRequest } from "@/types/bindings/CreateBranchRequest";
import type { PushRequest } from "@/types/bindings/PushRequest";
import type { PushResult } from "@/types/bindings/PushResult";
import type { UpdateOutcome } from "@/types/bindings/UpdateOutcome";
import { messages } from "@/lib/i18n/zh-CN";
import { runGuardedWrite, type GuardedWriteDeps } from "@/lib/guardedWrite";

export type RemoteWriteIntent =
  | { kind: "checkout"; worktreeId: string; target: CheckoutTarget; strategy: CheckoutStrategy }
  | { kind: "createBranch"; worktreeId: string; request: CreateBranchRequest }
  | { kind: "fetch"; worktreeId: string }
  | { kind: "update"; worktreeId: string }
  | { kind: "push"; worktreeId: string; request: PushRequest };

export interface RemoteWriteDeps<Token> extends Omit<GuardedWriteDeps<Token>, "execute"> {
  /** 执行真正的 IPC，返回后端结果。 */
  executeIntent: (intent: RemoteWriteIntent) => Promise<unknown>;
  /** 成功结果（不含失败：失败由 `reportFailure` 按原目标报告）。 */
  onSuccess: (intent: RemoteWriteIntent, result: unknown) => void;
}

/** 已建立写入/关闭保护的多阶段操作，把同一占用借给后续阶段而不重复登记。 */
export interface RemoteWriteLease {
  readonly worktreeId: string;
  active: boolean;
  executing: boolean;
  needsVerification: boolean;
}

/**
 * 新建分支失败时对话框里显示的文字（审查第 6 项）。
 *
 * 统一写编排会把错误规范化后交给对话框：Git 的原话（例如 `check-ref-format` 的
 * 拒绝原因）优先，只有完全没有消息时才退回通用失败提示。
 */
export function branchFailureDetail(error: AppErrorPayload | null | undefined): string {
  const message = error?.message?.trim();
  return message && message.length > 0 ? message : messages.branches.createFailed;
}

/** 执行一次受保护的分支 / 远端写操作。 */
export async function runRemoteWrite<Token>(
  deps: RemoteWriteDeps<Token>,
  intent: RemoteWriteIntent,
  lease?: RemoteWriteLease,
): Promise<{ ok: true; result: unknown } | { ok: false; error: AppErrorPayload }> {
  let captured: unknown;
  if (lease) {
    if (!lease.active || lease.executing || lease.worktreeId !== intent.worktreeId
      || !deps.writing.has(intent.worktreeId)) {
      return { ok: false, error: { kind: "rejected", message: deps.busyMessage } };
    }
    lease.executing = true;
    const token = deps.beginOperation(intent.worktreeId);
    let failure: AppErrorPayload | null = null;
    try {
      try {
        captured = await deps.executeIntent(intent);
      } catch (caught) {
        failure = deps.toError(caught);
        lease.needsVerification ||= failure.kind === "needsVerification";
        if (deps.canRecord(token)) deps.reportFailure(intent.worktreeId, failure);
      }
      try {
        await deps.refreshTarget(intent.worktreeId);
      } catch {
        // 重读自行报告；不能把已经完成的写入改报成未执行。
      }
    } finally {
      lease.executing = false;
    }
    if (failure) return { ok: false, error: failure };
    deps.onSuccess(intent, captured);
    return { ok: true, result: captured };
  }
  const guarded: GuardedWriteDeps<Token> = {
    ...deps,
    execute: async () => {
      captured = await deps.executeIntent(intent);
      return captured;
    },
  };
  try {
    await runGuardedWrite(guarded, { worktreeId: intent.worktreeId });
  } catch (caught) {
    const error = deps.toError(caught);
    return { ok: false, error };
  }
  deps.onSuccess(intent, captured);
  return { ok: true, result: captured };
}

/**
 * 提交并推送时是否显示推送对话框（B06）。
 *
 * 默认显示；关闭后直接推送；“只在受保护分支显示”打开时，只有受保护分支才显示。
 */
export function shouldShowPushDialog(options: {
  pushDialogOnCommit: boolean;
  pushDialogProtectedOnly: boolean;
  protectedBranch: boolean;
}): boolean {
  if (!options.pushDialogOnCommit) return false;
  if (options.pushDialogProtectedOnly) return options.protectedBranch;
  return true;
}

/**
 * 推送被拒后的下一步（B05）。
 *
 * 设置“推送被拒时自动更新”开启、且这次还没有尝试过更新时，自动做一次“更新后再推送”；
 * 更新后仍失败只报告，不再重试（不循环）。
 */
export function planAfterPushRejected(options: {
  autoUpdate: boolean;
  alreadyAttempted: boolean;
}): "update-then-push" | "stop" {
  if (!options.autoUpdate || options.alreadyAttempted) return "stop";
  return "update-then-push";
}

/** 更新结果的通知文案（B04）。 */
export function updateNotice(outcome: UpdateOutcome): {
  tone: "success" | "warning" | "error";
  title: string;
  detail?: string;
} {
  switch (outcome.status) {
    case "upToDate":
      return { tone: "success", title: messages.remote.upToDate };
    case "updated":
      return {
        tone: "success",
        title: messages.remote.updated(outcome.commitsMerged),
        detail: outcome.upstream,
      };
    case "mergeConflict":
      return {
        tone: "warning",
        title: messages.remote.mergeConflict,
        detail: outcome.conflicts.length ? messages.remote.conflictFiles(outcome.conflicts) : undefined,
      };
    case "rebaseConflict":
      return {
        tone: "warning",
        title: messages.remote.rebaseConflict,
        detail: outcome.conflicts.length ? messages.remote.conflictFiles(outcome.conflicts) : undefined,
      };
    case "autostashConflict":
      return {
        tone: "warning",
        title: messages.remote.autostashConflict,
        detail: outcome.stashKept ? messages.remote.stashKept(outcome.stashKept) : undefined,
      };
    default:
      return { tone: "warning", title: messages.remote.unknownUpdate };
  }
}

/** 推送结果的通知文案（B05）。 */
export function pushNotice(result: PushResult): {
  tone: "success" | "warning" | "error";
  title: string;
  detail?: string;
} {
  if (result.kind === "pushed") {
    return {
      tone: "success",
      title: messages.remote.pushed(result.commits, `${result.remote}/${result.branch}`),
    };
  }
  return {
    tone: "warning",
    title: messages.remote.pushRejected,
    detail: result.reason,
  };
}
