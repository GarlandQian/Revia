/**
 * 进行中操作的顶部提示条（M6 O01）。
 *
 * metadataVersion 来自用户看到的这次状态；raging 时后端会拒绝旧确认。
 * 中止 / 跳过会先保存恢复记录，因此这里显示的是可继续恢复的破坏性动作。
 */
import { useEffect, useState } from "react";
import type { OperationState } from "@/types/bindings/OperationState";
import type { RemoteTarget } from "@/features/branches/remoteContinuation";
import type { OperationAction } from "@/types/bindings/OperationAction";
import type { AutostashLink } from "@/types/bindings/AutostashLink";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { formatAppError } from "@/lib/error";
import { messages } from "@/lib/i18n/index";
import { operationKindText } from "@/lib/i18n/text";
import { Codicon } from "@/components/ui/codicon";
import { Modal } from "@/components/app/Dialogs";

export function OperationBanner({
  state,
  origin,
  autostash,
  autostashError,
  canRestoreAutostash,
  readErrors,
  busy,
  onAction,
  onRestoreAutostash,
  onViewStash,
  onKeepStash,
}: {
  state: OperationState | null;
  origin: RemoteTarget | null;
  autostash: AutostashLink | null;
  autostashError: AppErrorPayload | null;
  canRestoreAutostash: boolean;
  readErrors: AppErrorPayload[];
  busy: boolean;
  onAction: (action: OperationAction, origin: RemoteTarget, operation: OperationState) => void;
  onRestoreAutostash: () => void;
  onViewStash: () => void;
  onKeepStash: () => void;
}) {
  const [confirm, setConfirm] = useState<{ action: OperationAction; origin: RemoteTarget; operation: OperationState } | null>(null);
  useEffect(() => setConfirm(null), [origin?.worktreeId, origin?.activation, state?.kind, state?.metadataVersion, state?.confirmationState]);
  if (!state && !autostash && !readErrors.length) return null;
  const kind = state ? operationKindText(state.kind) : "";
  const autostashKind = autostash
    ? autostash.operation === "rebase"
      ? messages.operation.kinds.rebase
      : autostash.operation === "cherryPick" || autostash.operation === "cherry-pick"
        ? messages.operation.kinds.cherryPick
        : autostash.operation === "revert"
          ? messages.operation.kinds.revert
          : autostash.operation
    : "";
  const confirmedKind = confirm ? operationKindText(confirm.operation.kind) : kind;
  const destructive = !!confirm && !confirm.operation.continuationOnly && (confirm.action === "abort" || confirm.action === "skip");
  return (
    <>
      {readErrors.map((error, index) => (
        <div key={index} role="alert" className="shrink-0 border-b border-panel-border px-3 py-1.5 text-[12px] text-errorforeground">
          {formatAppError(error)}
        </div>
      ))}
      {state ? (
        <div
          role="status"
          className="flex shrink-0 flex-wrap items-center gap-2 border-b border-[var(--editorwarning-foreground)] bg-[color-mix(in_srgb,var(--editorwarning-foreground)_12%,var(--editor-background))] px-3 py-1.5 text-[12.5px]"
        >
          <Codicon name="warning" size={15} className="text-[var(--editorwarning-foreground)]" />
          <span className="font-semibold">{state.continuationOnly ? messages.operation.sequencePending : messages.operation.inProgress(kind)}</span>
          {state.conflictCount > 0 ? (
            <span className="text-descriptionforeground">{messages.operation.conflicts(state.conflictCount)}</span>
          ) : (
            <span className="text-descriptionforeground">{messages.operation.noConflicts}</span>
          )}
          <span className="ml-auto flex items-center gap-1.5">
            <button
              type="button"
              className="toolbar-widget h-6 px-2 text-[12px]"
              disabled={busy || state.conflictCount > 0 || state.needsVerification}
              title={state.conflictCount > 0 ? messages.operation.resolveFirst : messages.operation.continue}
              onClick={() => { if (origin) onAction("continue", origin, state); }}
            >
              {messages.operation.continue}
            </button>
            {state.kind === "merge" || state.continuationOnly ? null : (
              <button
                type="button"
                className="toolbar-widget h-6 px-2 text-[12px]"
                disabled={busy}
                onClick={() => { if (origin) setConfirm({ action: "skip", origin: { ...origin }, operation: { ...state } }); } }
              >
                {messages.operation.skip}
              </button>
            )}
            <button
              type="button"
              className="toolbar-widget h-6 px-2 text-[12px]"
              disabled={busy}
              onClick={() => { if (origin) setConfirm({ action: "abort", origin: { ...origin }, operation: { ...state } }); } }
            >
              {state.continuationOnly ? messages.operation.endSequence : messages.operation.abort}
            </button>
          </span>
        </div>
      ) : null}
      {autostash ? (
        <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-panel-border bg-[var(--statusbar-background)] px-3 py-1.5 text-[12px]">
          <Codicon name="history" size={14} />
          <span className="font-semibold">{autostash.restoration === "pending" ? messages.operation.autostashPending : messages.operation.autostashNeedsCheck}</span>
          <span className="text-descriptionforeground">{messages.operation.autostashSaved(autostashKind)}</span>
          {autostashError ? <span role="alert" className="text-errorforeground">{formatAppError(autostashError)}</span> : null}
          <button
            type="button"
            className="toolbar-widget ml-auto h-6 px-2 text-[12px]"
            disabled={busy}
            onClick={onViewStash}
          >
            {messages.operation.viewStash}
          </button>
          <button
            type="button"
            className="toolbar-widget h-6 px-2 text-[12px]"
            disabled={busy || !!state || !canRestoreAutostash || autostash.restoration !== "pending"}
            onClick={onRestoreAutostash}
          >
            {messages.operation.restoreAutostash}
          </button>
          <button type="button" className="toolbar-widget h-6 px-2 text-[12px]" disabled={busy || !!state} onClick={onKeepStash}>
            {messages.operation.keepStash}
          </button>
        </div>
      ) : null}
      {confirm ? (
        <Modal
          title={!destructive ? messages.operation.confirmTitle : confirm.action === "skip" ? messages.operation.confirmSkipTitle : messages.operation.confirmDestructiveTitle}
          onClose={() => setConfirm(null)}
          width={480}
        >
          <p className="font-semibold">
            {confirm.operation.continuationOnly ? messages.operation.confirmEndSequence : confirm.action === "abort" ? messages.operation.confirmAbort(confirmedKind) : messages.operation.confirmSkip(confirmedKind)}
          </p>
          <p className="mt-2 text-[12.5px] leading-relaxed text-descriptionforeground">
            {confirm.operation.continuationOnly ? messages.operation.endSequenceHint : messages.operation.confirmSafety}
          </p>
          <div className="mt-4 flex justify-end gap-2">
            <button
              type="button"
              className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-3 py-1 hover:bg-toolbar-hoverbackground"
              onClick={() => setConfirm(null)}
            >
              {messages.operation.cancel}
            </button>
            <button
              type="button"
              autoFocus
              className="rounded-[4px] bg-destructive px-3 py-1 text-destructive-foreground"
              onClick={() => {
                const captured = confirm;
                setConfirm(null);
                onAction(captured.action, captured.origin, captured.operation);
              }}
            >
              {confirm.operation.continuationOnly ? messages.operation.endSequence : confirm.action === "abort" ? messages.operation.abort : messages.operation.skip}
            </button>
          </div>
        </Modal>
      ) : null}
    </>
  );
}
