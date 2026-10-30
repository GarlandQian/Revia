/**
 * 需要用户选择或确认的对话框（M4 F07、Q119）：
 * 丢弃确认（可含多个文件）、写入期间关闭或退出。
 */

import { useEffect, useRef, type ReactNode } from "react";
import type { ChangesKind } from "@/types/bindings/ChangesKind";
import type { CommitWarning } from "@/types/bindings/CommitWarning";
import { messages } from "@/lib/i18n/index";
import { commitWarningText } from "@/lib/i18n/text";
import { Codicon } from "@/components/ui/codicon";
import { GlassLayer } from "@/features/m7/glass/GlassLayer";
import { FileTypeIcon } from "./FileTypeIcon";

export function Modal({
  title,
  children,
  onClose,
  width,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  width?: number;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current;
    dialog?.showModal();
    return () => dialog?.close();
  }, []);
  return (
    <dialog
      ref={ref}
      aria-label={title}
      className="app-dialog glass-isolate overflow-auto p-0 text-[13px]"
      style={{ width: `min(${width ?? 520}px, calc(100vw - 40px))`, maxHeight: "calc(100vh - 40px)" }}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <GlassLayer variant="popover" />
      <div className="flex items-center gap-2 border-b border-panel-border px-4 py-2.5">
        <h2 className="flex-1 text-[13.5px] font-semibold">{title}</h2>
        <button type="button" className="icon-action" aria-label={messages.recovery.close} onClick={onClose}>
          <Codicon name="close" size={16} />
        </button>
      </div>
      <div className="p-4">{children}</div>
    </dialog>
  );
}

export interface DiscardDialogTarget {
  path: string;
  changes: ChangesKind;
  /** 只丢弃一个块时的块标题。 */
  section?: string;
}

/** 丢弃确认：一次列出所有将丢弃的文件，说明会先存入回收区（Q120）。 */
export function DiscardDialog({
  targets,
  worktree,
  valid,
  onConfirm,
  onClose,
}: {
  targets: DiscardDialogTarget[];
  worktree: string;
  valid: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const single = targets.length === 1;
  const hunk = single && targets[0].section !== undefined;
  return (
    <Modal title={messages.recovery.discardTitle} onClose={onClose} width={560}>
      <div className="grid grid-cols-[28px_minmax(0,1fr)] gap-3">
        <Codicon name="warning" size={26} className="text-[var(--editorwarning-foreground)]" />
        <div className="min-w-0 space-y-2">
          <p className="font-semibold">
            {hunk
              ? messages.recovery.confirmHunk
              : single
                ? messages.recovery.confirmFile
                : messages.recovery.confirmFiles(targets.length)}
          </p>
          <p className="truncate font-mono text-[11.5px] text-descriptionforeground" title={worktree}>
            {worktree}
          </p>
          <ul className="max-h-56 overflow-y-auto rounded border border-panel-border">
            {targets.map((target) => (
              <li key={`${target.changes}:${target.path}`} className="flex items-center gap-1.5 px-2 py-1">
                <FileTypeIcon path={target.path} />
                <span className="min-w-0 flex-1 truncate font-mono text-[11.5px]">{target.path}</span>
                <span className="shrink-0 text-[11px] text-descriptionforeground">
                  {target.changes === "untracked" ? messages.changes.untracked : messages.changes.unstaged}
                </span>
              </li>
            ))}
          </ul>
          {hunk ? (
            <pre className="max-h-24 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/50 p-2 font-mono text-[11px]">
              {targets[0].section}
            </pre>
          ) : null}
          <p className="text-[12.5px] leading-relaxed text-descriptionforeground">{messages.recovery.safety}</p>
          {!valid ? (
            <p role="alert" className="text-[12.5px] text-destructive">
              {messages.recovery.changed}
            </p>
          ) : null}
        </div>
      </div>
      <div className="mt-4 flex justify-end gap-2">
        <button type="button" autoFocus className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-3 py-1 hover:bg-toolbar-hoverbackground" onClick={onClose}>
          {messages.recovery.cancel}
        </button>
        <button
          type="button"
          className="rounded-[4px] bg-destructive px-3 py-1 text-white disabled:opacity-50"
          disabled={!valid}
          onClick={onConfirm}
        >
          {messages.recovery.confirm}
        </button>
      </div>
    </Modal>
  );
}

/**
 * 写入期间的关闭/退出询问（任务 08，Q96）。
 *
 * 有在途写操作或待核对结果时保留窗口：等待完成后关闭/退出，或留在应用；
 * 不提供取消写操作或杀掉 Git 的入口。
 */
export function CloseRequestDialog({
  intent,
  writing,
  pending,
  onWait,
  onStay,
  onPendingReviewed,
}: {
  intent: "close" | "exit";
  writing: string[];
  pending: string[];
  onWait: () => void;
  onStay: () => void;
  onPendingReviewed: () => void;
}) {
  const hasPending = pending.length > 0;
  return (
    <Modal title={hasPending ? messages.close.pendingTitle : messages.close.title} onClose={onStay} width={460}>
      <div className="space-y-3 text-[12.5px]">
        <p className="leading-relaxed">{hasPending ? messages.close.pendingExplain : messages.close.explain}</p>
        {writing.length > 0 ? (
          <p className="break-all text-descriptionforeground">{messages.close.writing(writing.length)}</p>
        ) : null}
        {hasPending ? (
          <p role="alert" className="break-all text-destructive">
            {messages.close.pending(pending.length)}
          </p>
        ) : null}
      </div>
      <div className="mt-5 flex justify-end gap-2">
        <button type="button" autoFocus className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-3 py-1 hover:bg-toolbar-hoverbackground" onClick={onStay}>
          {messages.close.pendingStay}
        </button>
        {hasPending ? (
          <button type="button" className="rounded-[4px] bg-primary px-3 py-1 text-primary-foreground" onClick={onPendingReviewed}>
            {messages.close.pendingReviewed}
          </button>
        ) : (
          <button type="button" className="rounded-[4px] bg-primary px-3 py-1 text-primary-foreground" onClick={onWait}>
            {intent === "exit" ? messages.close.waitExit : messages.close.wait}
          </button>
        )}
      </div>
    </Modal>
  );
}

/** 提交时警告（返修 E.3）：列出检查结果，可以继续提交或取消。 */
export function CommitWarningDialog({
  warnings,
  onContinue,
  onCancel,
}: {
  warnings: CommitWarning[];
  onContinue: () => void;
  onCancel: () => void;
}) {
  return (
    <Modal title={messages.commit.warningsTitle} onClose={onCancel} width={560}>
      <p className="text-[12.5px] leading-relaxed text-descriptionforeground">{messages.commit.warningsDetail}</p>
      <ul className="mt-2 max-h-[280px] space-y-1 overflow-y-auto text-[12.5px]">
        {warnings.map((warning, index) => (
          <li key={`${warning.kind}-${index}`} className="flex items-start gap-1.5">
            <Codicon name="warning" size={13} className="mt-0.5 shrink-0 text-[var(--editorwarning-foreground)]" />
            <span className="min-w-0 break-words">{commitWarningText(warning)}</span>
          </li>
        ))}
      </ul>
      <div className="mt-4 flex justify-end gap-2">
        <button type="button" className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-3 py-1 hover:bg-toolbar-hoverbackground" onClick={onCancel}>
          {messages.commit.warningsCancel}
        </button>
        <button type="button" className="rounded-[4px] bg-primary px-3 py-1 text-primary-foreground" onClick={onContinue}>
          {messages.commit.warningsContinue}
        </button>
      </div>
    </Modal>
  );
}
