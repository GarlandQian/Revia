import { useEffect, useRef, useState, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { messages } from "@/lib/i18n/zh-CN";
import { ipc } from "@/lib/ipc";
import { getErrorMessage } from "@/lib/error";
import type { RecoveryInspection } from "@/types/bindings/RecoveryInspection";
import type { RecoverySummary } from "@/types/bindings/RecoverySummary";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

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
  const [notice, setNotice] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const [selected, setSelected] = useState<RecoveryInspection | null>(null);
  const [confirmingRestore, setConfirmingRestore] = useState(false);
  const [confirmingClear, setConfirmingClear] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let active = true;
    setRecords(null);
    setError(null);
    void ipc.listRecovery(worktreeId).then((result) => { if (active) setRecords(result); })
      .catch((caught) => { if (active) setError(getErrorMessage(caught)); });
    return () => { active = false; };
  }, [reload, worktreeId]);

  const refreshList = () => { setSelected(null); setReload((value) => value + 1); };

  const inspect = (id: string) => {
    if (!worktreeId) return;
    setBusy(true);
    setError(null);
    void ipc.inspectRecovery(worktreeId, id)
      .then((inspection) => setSelected(inspection))
      .catch((caught) => setError(getErrorMessage(caught)))
      .finally(() => setBusy(false));
  };

  const restore = () => {
    if (!selected || !worktreeId) return;
    setBusy(true);
    setError(null);
    void ipc.restoreRecovery({
      worktreeId,
      recordId: selected.id,
      expectedPaths: selected.paths,
      expectedState: selected.state,
    })
      .then((inspection) => { setConfirmingRestore(false); setSelected(inspection); setNotice(messages.recovery.restore); })
      .catch((caught) => { setConfirmingRestore(false); setError(getErrorMessage(caught)); })
      .finally(() => setBusy(false));
  };

  const saveAs = async (path: string) => {
    if (!selected) return;
    const picked = await openDialog({ directory: false, multiple: false, title: messages.recovery.saveAsPrompt });
    if (typeof picked !== "string") return;
    setBusy(true);
    setError(null);
    try {
      const bytes = await ipc.saveRecoveryAs({ recordId: selected.id, path, destination: picked });
      setNotice(`${messages.recovery.savedAs}：${bytes} ${messages.recovery.bytes}`);
    } catch (caught) {
      setError(getErrorMessage(caught));
    } finally {
      setBusy(false);
    }
  };

  const clear = () => {
    setBusy(true);
    setError(null);
    void ipc.clearRecovery()
      .then((count) => { setConfirmingClear(false); setNotice(`${messages.recovery.cleared}：${count}`); refreshList(); })
      .catch((caught) => { setConfirmingClear(false); setError(getErrorMessage(caught)); })
      .finally(() => setBusy(false));
  };

  const purge = () => {
    setBusy(true);
    setError(null);
    void ipc.purgeRecovery()
      .then((count) => { setNotice(`${messages.recovery.purged}：${count}`); refreshList(); })
      .catch((caught) => setError(getErrorMessage(caught)))
      .finally(() => setBusy(false));
  };

  const stateLabel = (state: RecoveryInspection["state"]) => ({
    ready: messages.recovery.stateReady,
    laterChanges: messages.recovery.stateLaterChanges,
    noBaseline: messages.recovery.stateNoBaseline,
    identityMismatch: messages.recovery.stateIdentityMismatch,
  })[state];

  return (
    <Modal title={messages.recovery.title} onClose={onClose}>
      <p className="mb-3 text-xs text-muted-foreground">{messages.recovery.viewOnly}</p>
      {error ? <p role="alert" className="mb-3 text-xs text-destructive">{error}</p> : null}
      {notice ? <p role="status" className="mb-3 text-xs text-muted-foreground">{notice}</p> : null}
      {selected ? (
        <div className="space-y-3 text-xs">
          <Button variant="ghost" size="sm" onClick={() => { setSelected(null); setNotice(null); }}>
            ← {messages.recovery.back}
          </Button>
          <h3 className="break-all font-mono font-semibold">{selected.target}</h3>
          <p>{new Date(selected.createdAtMs).toLocaleString()} · {selected.worktreePath}</p>
          <p>{selected.source === "discardHunk" ? messages.recovery.sourceHunk : messages.recovery.sourceFile}</p>
          <p className="text-muted-foreground">{messages.recovery.restoreFullFile}</p>
          <p role="status">{stateLabel(selected.state)}</p>
          <div className="max-h-[40vh] space-y-2 overflow-y-auto">
            {selected.files.map((file) => (
              <section key={file.path} className="rounded border p-2">
                <h4 className="break-all font-mono font-semibold">{file.path}</h4>
                <p className="text-muted-foreground">
                  {messages.recovery.savedVersion}：{file.saved.exists
                    ? `${file.saved.bytes} ${messages.recovery.bytes}${file.saved.executable ? " · 可执行" : ""}`
                    : messages.recovery.noContent}
                </p>
                <p className="text-muted-foreground">
                  {messages.recovery.currentVersion}：{file.current.exists
                    ? `${file.current.bytes} ${messages.recovery.bytes}${file.current.executable ? " · 可执行" : ""}`
                    : messages.recovery.noContent}
                </p>
                {file.saved.binary || file.current.binary ? (
                  <p className="text-muted-foreground">{messages.recovery.binaryInfo}</p>
                ) : (
                  <div className="mt-1 grid grid-cols-2 gap-2">
                    <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/40 p-2 font-mono">
                      {file.saved.text ?? messages.recovery.noContent}
                    </pre>
                    <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/40 p-2 font-mono">
                      {file.current.text ?? messages.recovery.noContent}
                    </pre>
                  </div>
                )}
                {file.saved.exists ? (
                  <Button variant="outline" size="sm" className="mt-1" disabled={busy} onClick={() => void saveAs(file.path)}>
                    {messages.recovery.saveAs}
                  </Button>
                ) : null}
              </section>
            ))}
          </div>
          {selected.state === "laterChanges" ? (
            <p className="text-muted-foreground">{messages.recovery.laterChangesHint}</p>
          ) : null}
          <div className="flex justify-end gap-2">
            <Button variant="outline" size="sm" onClick={() => { setSelected(null); setNotice(null); }}>
              {messages.recovery.back}
            </Button>
            <Button
              size="sm"
              disabled={busy || selected.state !== "ready"}
              onClick={() => setConfirmingRestore(true)}
              title={messages.recovery.restoreFullFile}
            >
              {messages.recovery.restore}
            </Button>
          </div>
          {confirmingRestore ? (
            <div role="dialog" aria-label={messages.recovery.confirmRestore} className="rounded border border-destructive/40 p-3">
              <p>{messages.recovery.confirmRestore}</p>
              <div className="mt-2 flex justify-end gap-2">
                <Button variant="outline" size="sm" onClick={() => setConfirmingRestore(false)}>{messages.recovery.cancel}</Button>
                <Button variant="destructive" size="sm" disabled={busy} onClick={restore}>{messages.recovery.confirm}</Button>
              </div>
            </div>
          ) : null}
        </div>
      ) : (
        <>
          <div className="max-h-[55vh] space-y-3 overflow-y-auto text-xs">
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
                <Button variant="outline" size="sm" disabled={busy || !worktreeId} onClick={() => inspect(record.id)}>
                  {messages.recovery.details}
                </Button>
              </article>
            ))}
          </div>
          <p className="mt-3 text-xs text-muted-foreground">{messages.recovery.keepDays}</p>
          <div className="mt-2 flex flex-wrap justify-end gap-2">
            <Button variant="ghost" size="sm" disabled={busy} onClick={purge}>{messages.recovery.purge}</Button>
            <Button variant="outline" size="sm" disabled={busy} onClick={() => setConfirmingClear(true)}>{messages.recovery.clear}</Button>
            <Button variant="outline" size="sm" onClick={() => setReload((value) => value + 1)}>{messages.recovery.refresh}</Button>
            <Button autoFocus size="sm" onClick={onClose}>{messages.recovery.close}</Button>
          </div>
          {confirmingClear ? (
            <div role="dialog" aria-label={messages.recovery.confirmClear} className="mt-2 rounded border border-destructive/40 p-3 text-xs">
              <p>{messages.recovery.confirmClear}</p>
              <div className="mt-2 flex justify-end gap-2">
                <Button variant="outline" size="sm" onClick={() => setConfirmingClear(false)}>{messages.recovery.cancel}</Button>
                <Button variant="destructive" size="sm" disabled={busy} onClick={clear}>{messages.recovery.confirm}</Button>
              </div>
            </div>
          ) : null}
        </>
      )}
    </Modal>
  );
}
