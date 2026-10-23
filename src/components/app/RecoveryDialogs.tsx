import { useEffect, useRef, useState, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { messages } from "@/lib/i18n/zh-CN";
import { ipc } from "@/lib/ipc";
import { getErrorMessage } from "@/lib/error";
import type { RecoverySummary } from "@/types/bindings/RecoverySummary";

function Modal({ title, children, onClose }: { title: string; children: ReactNode; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current;
    dialog?.showModal();
    return () => dialog?.close();
  }, []);
  return (
    <dialog ref={ref} aria-label={title} className="recovery-dialog rounded-lg border bg-background p-5 text-foreground shadow-xl"
      onCancel={(event) => { event.preventDefault(); onClose(); }}>
      <h2 className="mb-4 text-sm font-semibold">{title}</h2>
      {children}
    </dialog>
  );
}

export function DiscardDialog({ path, worktree, section, valid, onConfirm, onClose }: {
  path: string; worktree: string; section?: string; valid: boolean; onConfirm: () => void; onClose: () => void;
}) {
  return (
    <Modal title={section !== undefined ? messages.recovery.confirmHunk : messages.recovery.confirmFile} onClose={onClose}>
      <div className="space-y-3 text-xs">
        <p className="break-all font-mono font-semibold">{path}</p>
        <p className="break-all text-muted-foreground">{worktree}</p>
        {section !== undefined ? <pre className="whitespace-pre-wrap break-all font-mono">{section}</pre> : null}
        <p className="leading-relaxed text-muted-foreground">{messages.recovery.safety}</p>
        {!valid ? <p role="alert" className="text-destructive">{messages.recovery.changed}</p> : null}
      </div>
      <div className="mt-5 flex justify-end gap-2">
        <Button autoFocus variant="outline" size="sm" onClick={onClose}>{messages.recovery.cancel}</Button>
        <Button variant="destructive" size="sm" disabled={!valid} onClick={onConfirm}>{messages.recovery.confirm}</Button>
      </div>
    </Modal>
  );
}

export function RecoveryDialog({ worktreeId, onClose }: { worktreeId: string | null; onClose: () => void }) {
  const [records, setRecords] = useState<RecoverySummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  useEffect(() => {
    let active = true;
    setRecords(null);
    setError(null);
    void ipc.listRecovery(worktreeId).then((result) => { if (active) setRecords(result); })
      .catch((caught) => { if (active) setError(getErrorMessage(caught)); });
    return () => { active = false; };
  }, [reload, worktreeId]);
  return (
    <Modal title={messages.recovery.title} onClose={onClose}>
      <p className="mb-4 text-xs text-muted-foreground">{messages.recovery.viewOnly}</p>
      <div className="max-h-[60vh] space-y-3 overflow-y-auto text-xs">
        {error ? <p role="alert" className="text-destructive">{error}</p> : null}
        {!records && !error ? <p>{messages.recovery.loading}</p> : null}
        {records?.length === 0 ? <p>{messages.recovery.empty}</p> : null}
        {records?.map((record) => (
          <article key={record.id} className="space-y-1 rounded border p-3">
            <h3 className="break-all font-mono font-semibold">{record.target}</h3>
            <p>{new Date(record.createdAtMs).toLocaleString()} · {record.bytes} {messages.recovery.bytes}</p>
            <p>{record.source === "discardHunk" ? messages.recovery.sourceHunk : messages.recovery.sourceFile}</p>
            <p>{messages.recovery[record.status]}</p>
            <p className="break-all text-muted-foreground">{record.worktreePath}</p>
            <p className="break-all font-mono text-muted-foreground">{record.paths.join(" · ")}</p>
            {!record.identityConfirmed ? <p className="text-muted-foreground">{messages.recovery.otherWorktree}</p> : null}
          </article>
        ))}
      </div>
      <div className="mt-5 flex justify-end gap-2">
        <Button variant="outline" size="sm" onClick={() => setReload((value) => value + 1)}>{messages.recovery.refresh}</Button>
        <Button autoFocus size="sm" onClick={onClose}>{messages.recovery.close}</Button>
      </div>
    </Modal>
  );
}
