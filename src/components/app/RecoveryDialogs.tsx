import { useEffect, useRef, useState, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { messages } from "@/lib/i18n/zh-CN";
import { ipc } from "@/lib/ipc";
import { getErrorMessage } from "@/lib/error";
import type { RecoveryInspection } from "@/types/bindings/RecoveryInspection";
import type { RecoverySummary } from "@/types/bindings/RecoverySummary";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { RecoveryState } from "@/types/bindings/RecoveryState";

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

export function RecoveryDialog({
  worktreeId,
  isWriting,
  onRestore,
  onClose,
}: {
  worktreeId: string | null;
  isWriting: boolean;
  onRestore: (request: {
    worktreeId: string;
    recordId: string;
    expectedPaths: string[];
    expectedState: RecoveryState;
  }) => Promise<RecoveryInspection>;
  onClose: () => void;
}) {
  const [records, setRecords] = useState<RecoverySummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const [selected, setSelected] = useState<RecoveryInspection | null>(null);
  const [diff, setDiff] = useState<{ path: string; text: string | null; error: string | null } | null>(null);
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

  const refreshList = () => { setSelected(null); setDiff(null); setConfirmingRestore(false); setReload((value) => value + 1); };

  // 任何会让旧确认失效的动作都清掉待确认状态（Q89）。
  const backToList = () => {
    setSelected(null);
    setDiff(null);
    setConfirmingRestore(false);
    setNotice(null);
  };

  const inspect = (id: string) => {
    if (!worktreeId) return;
    setBusy(true);
    setError(null);
    setDiff(null);
    setConfirmingRestore(false);
    void ipc.inspectRecovery(worktreeId, id)
      .then((inspection) => setSelected(inspection))
      .catch((caught) => setError(getErrorMessage(caught)))
      .finally(() => setBusy(false));
  };

  const showDiff = (path: string) => {
    if (!selected || !worktreeId) return;
    setDiff({ path, text: null, error: null });
    void ipc.recoveryTextDiff({ worktreeId, recordId: selected.id, path })
      .then((text) => setDiff({ path, text, error: null }))
      .catch((caught) => setDiff({ path, text: null, error: getErrorMessage(caught) }));
  };

  const restore = () => {
    if (!selected || !worktreeId) return;
    setBusy(true);
    setError(null);
    void onRestore({
      worktreeId,
      recordId: selected.id,
      expectedPaths: selected.paths,
      expectedState: selected.state,
    })
      .then((inspection) => {
        setConfirmingRestore(false);
        setSelected(inspection);
        setNotice(messages.recovery.restoreDone);
      })
      .catch((caught) => {
        setConfirmingRestore(false);
        const err = getErrorMessage(caught);
        setError(err);
        if (/待核对/.test(err)) setNotice(messages.recovery.restoreNeedsCheck);
      })
      .finally(() => setBusy(false));
  };

  const saveAs = async (path: string) => {
    if (!selected) return;
    // 保存对话框允许输入尚不存在的新文件名；后端仍用 create_new 拒绝已存在目标。
    const picked = await saveDialog({ title: messages.recovery.saveAsPrompt });
    if (typeof picked !== "string") return;
    setBusy(true);
    setError(null);
    try {
      const bytes = await ipc.saveRecoveryAs({ recordId: selected.id, path, destination: picked });
      setNotice(`${messages.recovery.saveAsDone}：${bytes} ${messages.recovery.bytes}`);
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

  const stateLabel = (state: RecoveryState) => ({
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
          <Button variant="ghost" size="sm" onClick={backToList}>← {messages.recovery.back}</Button>
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
                    ? `${file.saved.bytes} ${messages.recovery.bytes}${file.saved.executable ? ` · ${messages.recovery.executable}` : ""}`
                    : messages.recovery.noContent}
                </p>
                {file.currentUnavailable || !file.current ? (
                  <p className="text-muted-foreground">{messages.recovery.currentUnavailable}</p>
                ) : (
                  <p className="text-muted-foreground">
                    {messages.recovery.currentVersion}：{file.current.exists
                      ? `${file.current.bytes} ${messages.recovery.bytes}${file.current.executable ? ` · ${messages.recovery.executable}` : ""}`
                      : messages.recovery.noContent}
                  </p>
                )}
                {file.saved.binary || file.current?.binary ? (
                  <p className="text-muted-foreground">{messages.recovery.binaryInfo}</p>
                ) : (
                  <div className="mt-1 space-y-1">
                    <Button variant="ghost" size="sm" disabled={busy || !file.current || file.currentUnavailable}
                      onClick={() => showDiff(file.path)}>
                      {messages.recovery.diffTitle}
                    </Button>
                    {diff?.path === file.path ? (
                      diff.error ? (
                        <p role="alert" className="text-destructive">{`${messages.recovery.diffUnavailable}：${diff.error}`}</p>
                      ) : diff.text === null ? (
                        <p className="text-muted-foreground">{messages.recovery.loading}</p>
                      ) : (
                        <pre className="max-h-60 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/40 p-2 font-mono">
                          {diff.text}
                        </pre>
                      )
                    ) : null}
                    <div className="grid grid-cols-2 gap-2">
                      <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/40 p-2 font-mono">
                        {file.saved.text ?? messages.recovery.noContent}
                      </pre>
                      <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/40 p-2 font-mono">
                        {file.current?.text ?? messages.recovery.currentUnavailable}
                      </pre>
                    </div>
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
            <Button variant="outline" size="sm" onClick={backToList}>{messages.recovery.back}</Button>
            <Button
              size="sm"
              disabled={busy || isWriting || selected.state !== "ready"}
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
                <Button variant="destructive" size="sm" disabled={busy || isWriting} onClick={restore}>{messages.recovery.confirm}</Button>
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
