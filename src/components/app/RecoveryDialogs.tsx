import { useEffect, useRef, useState, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { messages } from "@/lib/i18n/zh-CN";
import { ipc } from "@/lib/ipc";
import { getErrorMessage } from "@/lib/error";
import type { RecoveryInspection } from "@/types/bindings/RecoveryInspection";
import type { RecoverySummary } from "@/types/bindings/RecoverySummary";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { RecoveryState } from "@/types/bindings/RecoveryState";
import type { RecoveryMaintenance } from "@/types/bindings/RecoveryMaintenance";
import type { RestoreAttempt } from "@/types/bindings/RestoreAttempt";
import type { RestoreOutcome } from "@/types/bindings/RestoreOutcome";

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

/**
 * 回收区面板。
 *
 * 两个代际保证异步结果不串：
 * - `panelEpoch`：每次返回列表 / 换记录 / 刷新都会递增，旧响应直接丢弃；
 * - `requestEpoch`：同一记录内每次发起 diff/inspect/restore 递增，旧请求不得覆盖新结果。
 * 恢复确认绑定记录的 `version`（记录内容 + 丢弃回执 + 恢复历史），刷新后旧确认自动失效。
 */
export function RecoveryDialog({
  worktreeId,
  isWriting,
  snapshotRevision,
  onRestore,
  onClose,
}: {
  worktreeId: string | null;
  isWriting: boolean;
  snapshotRevision: number;
  onRestore: (request: {
    worktreeId: string;
    recordId: string;
    expectedVersion: string;
    expectedPaths: string[];
    expectedState: RecoveryState;
  }) => Promise<RestoreOutcome>;
  onClose: () => void;
}) {
  const [records, setRecords] = useState<RecoverySummary[] | null>(null);
  const [maintenance, setMaintenance] = useState<RecoveryMaintenance | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const [selected, setSelected] = useState<RecoveryInspection | null>(null);
  const [diff, setDiff] = useState<{ path: string; text: string | null; error: string | null } | null>(null);
  const [confirming, setConfirming] = useState<{ version: string; snapshotRevision: number } | null>(null);
  const [confirmingClear, setConfirmingClear] = useState(false);
  const [busy, setBusy] = useState(false);
  const panelEpoch = useRef(0);
  const requestEpoch = useRef(0);

  useEffect(() => {
    const epoch = ++panelEpoch.current;
    requestEpoch.current += 1;
    setRecords(null);
    setError(null);
    void ipc.listRecovery(worktreeId)
      .then((result) => { if (epoch === panelEpoch.current) setRecords(result); })
      .catch((caught) => { if (epoch === panelEpoch.current) setError(getErrorMessage(caught)); });
    void ipc.recoveryMaintenance()
      .then((result) => { if (epoch === panelEpoch.current) setMaintenance(result); })
      .catch(() => {});
    return () => { panelEpoch.current += 1; };
  }, [reload, worktreeId]);

  // 主界面快照刷新后，用户看到的状态已变：旧确认一律失效（Q89）。
  useEffect(() => {
    setConfirming(null);
  }, [snapshotRevision]);

  const backToList = () => {
    panelEpoch.current += 1;
    requestEpoch.current += 1;
    setSelected(null);
    setDiff(null);
    setConfirming(null);
  };

  const refreshList = () => {
    backToList();
    setNotice(null);
    setReload((value) => value + 1);
  };

  const inspect = (id: string) => {
    if (!worktreeId) return;
    const panel = ++panelEpoch.current;
    const request = ++requestEpoch.current;
    setBusy(true);
    setError(null);
    setDiff(null);
    setConfirming(null);
    void ipc.inspectRecovery(worktreeId, id)
      .then((inspection) => {
        if (panel !== panelEpoch.current || request !== requestEpoch.current) return;
        if (inspection.id !== id) return;
        setSelected(inspection);
      })
      .catch((caught) => { if (panel === panelEpoch.current && request === requestEpoch.current) setError(getErrorMessage(caught)); })
      .finally(() => { if (panel === panelEpoch.current && request === requestEpoch.current) setBusy(false); });
  };

  const showDiff = (path: string) => {
    if (!selected || !worktreeId) return;
    const panel = panelEpoch.current;
    const recordId = selected.id;
    const request = ++requestEpoch.current;
    setDiff({ path, text: null, error: null });
    void ipc.recoveryTextDiff({ worktreeId, recordId, path })
      .then((text) => {
        // 迟到的 R1 响应不能显示在已切到 R2 的面板里。
        if (panel !== panelEpoch.current || request !== requestEpoch.current) return;
        if (selectedRef.current?.id !== recordId) return;
        setDiff({ path, text, error: null });
      })
      .catch((caught) => {
        if (panel !== panelEpoch.current || request !== requestEpoch.current) return;
        if (selectedRef.current?.id !== recordId) return;
        setDiff({ path, text: null, error: getErrorMessage(caught) });
      });
  };

  const selectedRef = useRef<RecoveryInspection | null>(null);
  selectedRef.current = selected;

  const restore = () => {
    if (!selected || !worktreeId || !confirming) return;
    const panel = panelEpoch.current;
    const request = ++requestEpoch.current;
    const recordId = selected.id;
    setBusy(true);
    setError(null);
    void onRestore({
      worktreeId,
      recordId,
      expectedVersion: confirming.version,
      expectedPaths: selected.paths,
      expectedState: selected.state,
    })
      .then((outcome) => {
        if (panel !== panelEpoch.current) return;
        if (outcome.inspection.id !== recordId) return;
        setConfirming(null);
        setSelected(outcome.inspection);
        setNotice(outcome.attempt?.state === "completed"
          ? messages.recovery.restoreDone
          : messages.recovery.restoreNeedsCheck);
      })
      .catch((caught) => {
        const err = getErrorMessage(caught);
        if (panel !== panelEpoch.current) return;
        setConfirming(null);
        setError(err);
        if (/待核对/.test(err)) setNotice(messages.recovery.restoreNeedsCheck);
      })
      .finally(() => { if (panel === panelEpoch.current && request === requestEpoch.current) setBusy(false); });
  };

  const saveAs = async (path: string) => {
    if (!selected) return;
    const picked = await saveDialog({ title: messages.recovery.saveAsPrompt });
    if (typeof picked !== "string") return;
    setBusy(true);
    setError(null);
    try {
      const bytes = await ipc.saveRecoveryAs({ worktreeId, recordId: selected.id, path, destination: picked });
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

  const attemptLabel = (attempt: RestoreAttempt) => ({
    pending: messages.recovery.attemptPending,
    completed: messages.recovery.attemptCompleted,
    needsVerification: messages.recovery.attemptNeedsCheck,
    rejected: messages.recovery.attemptRejected,
  })[attempt.state];

  return (
    <Modal title={messages.recovery.title} onClose={onClose}>
      <p className="mb-3 text-xs text-muted-foreground">{messages.recovery.viewOnly}</p>
      {maintenance?.error ? (
        <p role="alert" className="mb-3 text-xs text-destructive">
          {`${messages.recovery.maintenanceFailed}：${maintenance.error}`}
        </p>
      ) : null}
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
          {selected.restore ? (
            <p role="status" className={selected.restore.state === "needsVerification" ? "text-destructive" : "text-muted-foreground"}>
              {`${messages.recovery.attemptLabel}：${attemptLabel(selected.restore)} · ${new Date(selected.restore.atMs).toLocaleString()}`}
              {selected.restore.message ? ` · ${selected.restore.message}` : ""}
            </p>
          ) : null}
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
                    <Button variant="ghost" size="sm" disabled={busy}
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
              onClick={() => setConfirming({ version: selected.version, snapshotRevision })}
              title={messages.recovery.restoreFullFile}
            >
              {messages.recovery.restore}
            </Button>
          </div>
          {confirming ? (
            <div role="dialog" aria-label={messages.recovery.confirmRestore} className="rounded border border-destructive/40 p-3">
              <p>{messages.recovery.confirmRestore}</p>
              <div className="mt-2 flex justify-end gap-2">
                <Button variant="outline" size="sm" onClick={() => setConfirming(null)}>{messages.recovery.cancel}</Button>
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
                {record.restore ? (
                  <p className={record.restore.state === "needsVerification" ? "text-destructive" : "text-muted-foreground"}>
                    {`${messages.recovery.attemptLabel}：${attemptLabel(record.restore)} · ${new Date(record.restore.atMs).toLocaleString()}`}
                  </p>
                ) : null}
                <p className="break-all text-muted-foreground">{record.worktreePath}</p>
                <p className="break-all font-mono text-muted-foreground">{record.paths.join(" · ")}</p>
                {!record.identityConfirmed ? <p className="text-muted-foreground">{messages.recovery.otherWorktree}</p> : null}
                <Button variant="outline" size="sm" disabled={busy} onClick={() => inspect(record.id)}>
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
