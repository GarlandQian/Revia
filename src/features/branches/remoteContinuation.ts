import type { ActivationContext } from "@/lib/refreshGate";
import type { PushPreview } from "@/types/bindings/PushPreview";
import type { PushRequest } from "@/types/bindings/PushRequest";
import type { PushResult } from "@/types/bindings/PushResult";
import { planAfterPushRejected, shouldShowPushDialog, type RemoteWriteLease } from "./remoteWrite";

/** 后续阶段沿用发起时的 worktree；确认还必须属于同一次激活。 */
export interface RemoteTarget {
  worktreeId: string;
  activation: number;
}

export function captureRemoteTarget(
  current: ActivationContext,
  worktreeId: string | null = current.worktreeId,
): RemoteTarget | null {
  return worktreeId && current.worktreeId === worktreeId
    ? { worktreeId, activation: current.activation }
    : null;
}

export function remoteTargetIsCurrent(target: RemoteTarget, current: ActivationContext): boolean {
  return target.worktreeId === current.worktreeId && target.activation === current.activation;
}

/** 对话框的确认不能把旧目标套到当前 worktree，A -> B -> A 也须重新预览。 */
export function confirmRemoteAction<T>(
  target: RemoteTarget,
  current: ActivationContext,
  run: (target: RemoteTarget) => T,
): T | undefined {
  if (!remoteTargetIsCurrent(target, current)) return undefined;
  return run(target);
}

export interface BoundPushPreview {
  origin: RemoteTarget;
  preview: PushPreview;
}

export async function preparePush(
  origin: RemoteTarget,
  deps: {
    preview: (worktreeId: string) => Promise<PushPreview>;
    current: () => ActivationContext;
    showDialog: (preview: PushPreview) => boolean;
    onConfirmationRequired?: () => void;
  },
): Promise<
  | ({ kind: "dialog" } & BoundPushPreview)
  | { kind: "push"; origin: RemoteTarget; request: PushRequest }
  | { kind: "stale"; origin: RemoteTarget }
> {
  const preview = await deps.preview(origin.worktreeId);
  if (deps.showDialog(preview)) {
    deps.onConfirmationRequired?.();
    return remoteTargetIsCurrent(origin, deps.current())
      ? { kind: "dialog", origin, preview }
      : { kind: "stale", origin };
  }
  return {
    kind: "push", origin,
    request: { remote: preview.remote, branch: preview.targetBranch, tags: false, force: "none" },
  };
}

export function commitPushNeedsDialog(
  preview: PushPreview,
  settings: { pushDialogOnCommit: boolean; pushDialogProtectedOnly: boolean },
): boolean {
  return shouldShowPushDialog({ ...settings, protectedBranch: preview.protected });
}

/** 更新与唯一一次重试都使用原目标和原推送参数，不读取当前界面的 worktree。 */
export async function runPushSequence(
  origin: RemoteTarget,
  request: PushRequest,
  alreadyAttempted: boolean,
  deps: {
    push: (worktreeId: string, request: PushRequest) => Promise<PushResult | null>;
    update: (worktreeId: string) => Promise<boolean>;
    autoUpdate: () => boolean;
    onResult: (result: PushResult) => void;
  },
): Promise<"done" | "offer-update" | "update-failed"> {
  const result = await deps.push(origin.worktreeId, request);
  if (!result) return "done";
  deps.onResult(result);
  if (result.kind !== "rejected" || alreadyAttempted) return "done";
  if (planAfterPushRejected({ autoUpdate: deps.autoUpdate(), alreadyAttempted }) === "stop") {
    return "offer-update";
  }
  if (!await deps.update(origin.worktreeId)) return "update-failed";
  const retried = await deps.push(origin.worktreeId, { ...request, force: "none" });
  if (retried) deps.onResult(retried);
  return "done";
}

/** 提交与自动推送共用写入生命周期；待用户确认或整个自动链结束后才放行关闭。 */
export async function finishCommitAndContinue(
  origin: RemoteTarget,
  pushAfterCommit: boolean,
  release: (needsVerification: boolean) => void,
  continuePush: (origin: RemoteTarget, lease: RemoteWriteLease) => Promise<void>,
): Promise<void> {
  const lease: RemoteWriteLease = {
    worktreeId: origin.worktreeId, active: true, executing: false, needsVerification: false,
  };
  try {
    if (pushAfterCommit) await continuePush(origin, lease);
  } finally {
    lease.active = false;
    release(lease.needsVerification);
  }
}

export function remoteShortcutAction(
  event: { code: string; altKey: boolean; shiftKey: boolean; defaultPrevented: boolean; repeat: boolean; isComposing: boolean },
  focus: { textOrDialog: boolean; button: boolean; commitMessage: boolean; dialogOpen: boolean },
): "push" | "commitAndPush" | "update" | null {
  if (event.defaultPrevented || event.repeat || event.isComposing || focus.dialogOpen || focus.button) return null;
  const action = event.code === "KeyK" && event.altKey && !event.shiftKey
    ? "commitAndPush"
    : event.code === "KeyK" && event.shiftKey && !event.altKey
      ? "push"
      : event.code === "KeyT" && !event.shiftKey && !event.altKey ? "update" : null;
  if (focus.textOrDialog && !(action === "commitAndPush" && focus.commitMessage)) return null;
  return action;
}
