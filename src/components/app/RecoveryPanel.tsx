/**
 * 回收区页签（M4 F06、Q29、Q85–Q87、ADR-0010）。
 *
 * 列表、详情、恢复、另存与清空都放在提交工具窗口的“回收区”页签里，规则不变：
 * 恢复写回完整文件内容、不改变暂存区；有后续修改时只读对比或另存；
 * 原 worktree 身份无法确认时仍可查看。操作结果用右下角通知报告（F07）。
 */

import { useEffect, useRef, useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { RecoveryInspection } from "@/types/bindings/RecoveryInspection";
import type { RecoveryMaintenance } from "@/types/bindings/RecoveryMaintenance";
import type { RecoverySource } from "@/types/bindings/RecoverySource";
import type { RecoveryState } from "@/types/bindings/RecoveryState";
import type { RecoverySummary } from "@/types/bindings/RecoverySummary";
import type { RestoreAttempt } from "@/types/bindings/RestoreAttempt";
import type { RestoreOutcome } from "@/types/bindings/RestoreOutcome";
import { messages } from "@/lib/i18n/zh-CN";
import { getErrorMessage } from "@/lib/error";
import { ipc } from "@/lib/ipc";
import { Codicon } from "@/components/ui/codicon";
import { Modal } from "./Dialogs";
import type { NoticeTone } from "./Notifications";

function sourceLabel(source: RecoverySource): string {
  switch (source) {
    case "discardHunk":
      return messages.recovery.sourceHunk;
    case "commitSnapshot":
      return messages.recovery.sourceCommit;
    case "discardFile":
      return messages.recovery.sourceFile;
    case "mergeApply":
      return messages.recovery.sourceMerge;
    case "checkoutSmart":
      return messages.recovery.sourceCheckoutSmart;
    case "checkoutForce":
      return messages.recovery.sourceCheckoutForce;
    case "updateAutostash":
      return messages.recovery.sourceUpdateAutostash;
  }
}

function statusLabel(source: RecoverySource, status: RecoverySummary["status"]): string {
  const commit = source === "commitSnapshot";
  switch (status) {
    case "completed":
      return commit ? messages.recovery.completedCommit : messages.recovery.completed;
    case "notApplied":
      return commit ? messages.recovery.notAppliedCommit : messages.recovery.notApplied;
    case "needsVerification":
      return commit ? messages.recovery.needsVerificationCommit : messages.recovery.needsVerification;
  }
}

export function RecoveryPanel({
  worktreeId,
  isWriting,
  isWritingFor,
  snapshotRevision,
  activation,
  onRestore,
  onSaveAs,
  onNotify,
}: {
  worktreeId: string | null;
  isWriting: boolean;
  isWritingFor: (worktreeId: string) => boolean;
  snapshotRevision: number;
  activation: number;
  onRestore: (request: {
    worktreeId: string;
    recordId: string;
    expectedVersion: string;
    expectedPaths: string[];
    expectedState: RecoveryState;
  }) => Promise<RestoreOutcome>;
  onSaveAs: (request: { recordId: string; path: string; destination: string }) => Promise<number>;
  onNotify: (notice: { tone: NoticeTone; title: string; detail?: string; persistent?: boolean }) => void;
}) {
  const [records, setRecords] = useState<RecoverySummary[] | null>(null);
  const [maintenance, setMaintenance] = useState<RecoveryMaintenance | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const [selected, setSelected] = useState<RecoveryInspection | null>(null);
  const [diff, setDiff] = useState<{ path: string; text: string | null; error: string | null } | null>(null);
  const [confirmRestore, setConfirmRestore] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const [busy, setBusy] = useState(false);
  const panelEpoch = useRef(0);
  const requestEpoch = useRef(0);
  const busyToken = useRef(0);
  const selectedRef = useRef<RecoveryInspection | null>(null);
  selectedRef.current = selected;

  const endRequest = (token: number) => {
    if (token === busyToken.current) setBusy(false);
  };
  /** 开始一次面板内请求：占住 busy 并返回令牌。 */
  const beginRequest = () => {
    const requestToken = ++busyToken.current;
    setBusy(true);
    return requestToken;
  };
  const abandonRequests = () => {
    busyToken.current += 1;
    setBusy(false);
  };
  const resetPanel = () => {
    panelEpoch.current += 1;
    requestEpoch.current += 1;
    abandonRequests();
    setSelected(null);
    setDiff(null);
    setConfirmRestore(false);
    setConfirmClear(false);
  };

  useEffect(() => {
    const epoch = ++panelEpoch.current;
    requestEpoch.current += 1;
    setRecords(null);
    setError(null);
    setSelected(null);
    setDiff(null);
    abandonRequests();
    void ipc.listRecovery(worktreeId)
      .then((result) => {
        if (epoch === panelEpoch.current) setRecords(result);
      })
      .catch((caught) => {
        if (epoch === panelEpoch.current) setError(getErrorMessage(caught));
      });
    void ipc.recoveryMaintenance()
      .then((result) => {
        if (epoch === panelEpoch.current) setMaintenance(result);
      })
      .catch(() => {});
    return () => {
      panelEpoch.current += 1;
    };
  }, [reload, worktreeId, activation]);

  useEffect(() => {
    setConfirmRestore(false);
  }, [snapshotRevision, activation, worktreeId]);

  const refreshList = () => {
    resetPanel();
    setReload((value) => value + 1);
  };

  const inspect = (id: string) => {
    const panel = ++panelEpoch.current;
    const request = ++requestEpoch.current;
    const token = beginRequest();
    setError(null);
    setDiff(null);
    setConfirmRestore(false);
    void ipc.inspectRecovery(worktreeId ?? "", id)
      .then((inspection) => {
        if (panel !== panelEpoch.current || request !== requestEpoch.current) return;
        if (inspection.id !== id) return;
        setSelected(inspection);
      })
      .catch((caught) => {
        if (panel === panelEpoch.current && request === requestEpoch.current) setError(getErrorMessage(caught));
      })
      .finally(() => endRequest(token));
  };

  const showDiff = (path: string) => {
    if (!selected || !worktreeId) return;
    const panel = panelEpoch.current;
    const recordId = selected.id;
    const request = ++requestEpoch.current;
    setDiff({ path, text: null, error: null });
    void ipc.recoveryTextDiff({ worktreeId, recordId, path })
      .then((text) => {
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

  const restore = () => {
    if (!selected || !worktreeId) return;
    const panel = panelEpoch.current;
    ++requestEpoch.current;
    const recordId = selected.id;
    const token = beginRequest();
    setConfirmRestore(false);
    void onRestore({
      worktreeId,
      recordId,
      expectedVersion: selected.version,
      expectedPaths: selected.paths,
      expectedState: selected.state,
    })
      .then((outcome) => {
        if (panel !== panelEpoch.current) return;
        if (outcome.inspection.id !== recordId) return;
        setSelected(outcome.inspection);
        const done = outcome.attempt?.state === "completed";
        onNotify({
          tone: done ? "success" : "warning",
          title: done ? messages.recovery.restoreDone : messages.recovery.restoreNeedsCheck,
          persistent: !done,
        });
      })
      .catch((caught) => {
        if (panel !== panelEpoch.current) return;
        onNotify({ tone: "error", title: messages.recovery.restoreNeedsCheck, detail: getErrorMessage(caught), persistent: true });
      })
      .finally(() => endRequest(token));
  };

  const saveAs = async (path: string) => {
    if (!selected) return;
    const picked = await saveDialog({ title: messages.recovery.saveAsPrompt });
    if (typeof picked !== "string") return;
    const panel = panelEpoch.current;
    const token = beginRequest();
    try {
      const bytes = await onSaveAs({ recordId: selected.id, path, destination: picked });
      if (panel !== panelEpoch.current) return;
      onNotify({ tone: "success", title: messages.recovery.saveAsDone, detail: `${bytes} ${messages.recovery.bytes}` });
    } catch (caught) {
      if (panel !== panelEpoch.current) return;
      onNotify({ tone: "error", title: messages.recovery.saveAsFailed, detail: getErrorMessage(caught), persistent: true });
    } finally {
      endRequest(token);
    }
  };

  const clear = () => {
    const panel = panelEpoch.current;
    const token = beginRequest();
    setConfirmClear(false);
    void ipc.clearRecovery()
      .then((count) => {
        if (panel !== panelEpoch.current) return;
        onNotify({ tone: "success", title: `${messages.recovery.cleared}：${count}` });
        refreshList();
      })
      .catch((caught) => {
        if (panel !== panelEpoch.current) return;
        onNotify({ tone: "error", title: messages.recovery.clear, detail: getErrorMessage(caught), persistent: true });
      })
      .finally(() => endRequest(token));
  };

  const purge = () => {
    const panel = panelEpoch.current;
    const token = beginRequest();
    void ipc.purgeRecovery()
      .then((count) => {
        if (panel !== panelEpoch.current) return;
        onNotify({ tone: "info", title: `${messages.recovery.purged}：${count}` });
        refreshList();
      })
      .catch((caught) => {
        if (panel === panelEpoch.current) {
          onNotify({ tone: "error", title: messages.recovery.purge, detail: getErrorMessage(caught), persistent: true });
        }
      })
      .finally(() => endRequest(token));
  };

  const stateLabel = (state: RecoveryState) =>
    ({
      ready: messages.recovery.stateReady,
      laterChanges: messages.recovery.stateLaterChanges,
      noBaseline: messages.recovery.stateNoBaseline,
      identityMismatch: messages.recovery.stateIdentityMismatch,
    })[state];

  const attemptLabel = (attempt: RestoreAttempt) =>
    ({
      pending: messages.recovery.attemptPending,
      completed: messages.recovery.attemptCompleted,
      needsVerification: messages.recovery.attemptNeedsCheck,
      rejected: messages.recovery.attemptRejected,
    })[attempt.state];

  return (
    <div className="flex min-h-0 flex-1 flex-col text-[12px]">
      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-2">
        <p className="mb-2 px-1 text-[11.5px] leading-relaxed text-descriptionforeground">{messages.recovery.viewOnly}</p>
        {maintenance?.error ? (
          <p role="alert" className="mb-2 px-1 text-[11.5px] text-destructive">
            {`${messages.recovery.maintenanceFailed}：${maintenance.error}`}
          </p>
        ) : null}
        {error ? (
          <p role="alert" className="mb-2 px-1 text-[11.5px] text-destructive">
            {error}
          </p>
        ) : null}

        {selected ? (
          <div className="space-y-2 px-1">
            <button type="button" className="banner-link inline-flex items-center gap-1" onClick={() => { setSelected(null); setDiff(null); }}>
              <Codicon name="arrow-left" size={13} />
              {messages.recovery.back}
            </button>
            <h3 className="break-all font-mono text-[12px] font-semibold">{selected.target}</h3>
            <p className="text-descriptionforeground">
              {new Date(selected.createdAtMs).toLocaleString()} · {selected.worktreePath}
            </p>
            <p>{sourceLabel(selected.source)}</p>
            <p role="status" className="font-semibold">{stateLabel(selected.state)}</p>
            <p className="text-descriptionforeground">{messages.recovery.restoreFullFile}</p>
            {selected.restore ? (
              <p role="status" className={selected.restore.state === "needsVerification" ? "text-destructive" : "text-descriptionforeground"}>
                {`${messages.recovery.attemptLabel}：${attemptLabel(selected.restore)} · ${new Date(selected.restore.atMs).toLocaleString()}`}
                {selected.restore.message ? ` · ${selected.restore.message}` : ""}
              </p>
            ) : null}
            <div className="space-y-2">
              {selected.files.map((file) => (
                <section key={file.path} className="rounded border border-panel-border p-2">
                  <h4 className="break-all font-mono text-[11.5px] font-semibold">{file.path}</h4>
                  <p className="text-[11.5px] text-descriptionforeground">
                    {messages.recovery.savedVersion}：{file.saved.exists
                      ? `${file.saved.bytes} ${messages.recovery.bytes}${file.saved.executable ? ` · ${messages.recovery.executable}` : ""}`
                      : messages.recovery.noContent}
                  </p>
                  {file.currentUnavailable || !file.current ? (
                    <p className="text-[11.5px] text-descriptionforeground">{messages.recovery.currentUnavailable}</p>
                  ) : (
                    <p className="text-[11.5px] text-descriptionforeground">
                      {messages.recovery.currentVersion}：{file.current.exists
                        ? `${file.current.bytes} ${messages.recovery.bytes}${file.current.executable ? ` · ${messages.recovery.executable}` : ""}`
                        : messages.recovery.noContent}
                    </p>
                  )}
                  {file.saved.binary || file.current?.binary ? (
                    <p className="text-[11.5px] text-descriptionforeground">{messages.recovery.binaryInfo}</p>
                  ) : (
                    <div className="mt-1 space-y-1">
                      <button
                        type="button"
                        className="banner-link text-[11.5px]"
                        disabled={busy || !worktreeId}
                        onClick={() => showDiff(file.path)}
                      >
                        {messages.recovery.diffTitle}
                      </button>
                      {diff?.path === file.path ? (
                        diff.error ? (
                          <p role="alert" className="text-destructive">{`${messages.recovery.diffUnavailable}：${diff.error}`}</p>
                        ) : diff.text === null ? (
                          <p className="text-descriptionforeground">{messages.recovery.loading}</p>
                        ) : (
                          <pre className="max-h-52 overflow-auto rounded bg-muted/50 p-2 font-mono text-[11px] whitespace-pre-wrap break-all">
                            {diff.text}
                          </pre>
                        )
                      ) : null}
                      <pre className="max-h-32 overflow-auto rounded bg-muted/50 p-2 font-mono text-[11px] whitespace-pre-wrap break-all">
                        {file.saved.text ?? messages.recovery.noContent}
                      </pre>
                    </div>
                  )}
                  {file.saved.exists ? (
                    <button
                      type="button"
                      className="mt-1 rounded-[3px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-2 py-0.5 text-[11.5px] hover:bg-toolbar-hoverbackground"
                      disabled={busy}
                      onClick={() => void saveAs(file.path)}
                    >
                      {messages.recovery.saveAs}
                    </button>
                  ) : null}
                </section>
              ))}
            </div>
            {selected.state === "laterChanges" ? (
              <p className="text-[11.5px] text-descriptionforeground">{messages.recovery.laterChangesHint}</p>
            ) : null}
            <div className="flex justify-end gap-2 pt-1">
              <button
                type="button"
                className="rounded-[3px] bg-primary px-2.5 py-1 text-primary-foreground disabled:opacity-50"
                disabled={busy || isWriting || selected.state !== "ready" || !worktreeId || isWritingFor(worktreeId)}
                title={messages.recovery.restoreFullFile}
                onClick={() => setConfirmRestore(true)}
              >
                {messages.recovery.restore}
              </button>
            </div>
          </div>
        ) : (
          <>
            {!records && !error ? <p className="px-1">{messages.recovery.loading}</p> : null}
            {records?.length === 0 ? <p className="px-1 text-descriptionforeground">{messages.recovery.empty}</p> : null}
            <div className="space-y-2">
              {records?.map((record) => (
                <article key={record.id} className="rounded border border-panel-border p-2">
                  <h3 className="break-all font-mono text-[11.5px] font-semibold">{record.target}</h3>
                  <p className="text-[11px] text-descriptionforeground">
                    {new Date(record.createdAtMs).toLocaleString()} · {record.bytes} {messages.recovery.bytes}
                  </p>
                  <p className="text-[11px]">{sourceLabel(record.source)}</p>
                  <p className="text-[11px]">{statusLabel(record.source, record.status)}</p>
                  {record.restore ? (
                    <p className={`text-[11px] ${record.restore.state === "needsVerification" ? "text-destructive" : "text-descriptionforeground"}`}>
                      {`${messages.recovery.attemptLabel}：${attemptLabel(record.restore)} · ${new Date(record.restore.atMs).toLocaleString()}`}
                    </p>
                  ) : null}
                  <p className="break-all text-[11px] text-descriptionforeground">{record.worktreePath}</p>
                  <p className="break-all font-mono text-[11px] text-descriptionforeground">{record.paths.join(" · ")}</p>
                  {!record.identityConfirmed ? (
                    <p className="text-[11px] text-descriptionforeground">{messages.recovery.otherWorktree}</p>
                  ) : null}
                  <button
                    type="button"
                    className="mt-1 rounded-[3px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-2 py-0.5 text-[11.5px] hover:bg-toolbar-hoverbackground"
                    disabled={busy}
                    onClick={() => inspect(record.id)}
                  >
                    {messages.recovery.details}
                  </button>
                </article>
              ))}
            </div>
            <p className="mt-2 px-1 text-[11px] text-descriptionforeground">{messages.recovery.keepDays}</p>
            <div className="mt-2 flex flex-wrap justify-end gap-2 px-1">
              <button type="button" className="banner-link text-[11.5px]" disabled={busy} onClick={purge}>
                {messages.recovery.purge}
              </button>
              <button
                type="button"
                className="rounded-[3px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-2 py-0.5 text-[11.5px] hover:bg-toolbar-hoverbackground"
                disabled={busy}
                onClick={() => setConfirmClear(true)}
              >
                {messages.recovery.clear}
              </button>
              <button
                type="button"
                className="rounded-[3px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-2 py-0.5 text-[11.5px] hover:bg-toolbar-hoverbackground"
                onClick={() => setReload((value) => value + 1)}
              >
                {messages.recovery.refresh}
              </button>
            </div>
          </>
        )}
      </div>

      {confirmRestore ? (
        <Modal title={messages.recovery.confirmRestore} onClose={() => setConfirmRestore(false)} width={420}>
          <p className="text-[12.5px]">{messages.recovery.confirmRestore}</p>
          <div className="mt-4 flex justify-end gap-2">
            <button type="button" className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-3 py-1 hover:bg-toolbar-hoverbackground" onClick={() => setConfirmRestore(false)}>
              {messages.recovery.cancel}
            </button>
            <button
              type="button"
              className="rounded-[4px] bg-destructive px-3 py-1 text-white disabled:opacity-50"
              disabled={busy || isWriting}
              onClick={restore}
            >
              {messages.recovery.confirmRestoreAction}
            </button>
          </div>
        </Modal>
      ) : null}
      {confirmClear ? (
        <Modal title={messages.recovery.confirmClear} onClose={() => setConfirmClear(false)} width={420}>
          <p className="text-[12.5px]">{messages.recovery.confirmClear}</p>
          <div className="mt-4 flex justify-end gap-2">
            <button type="button" className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-3 py-1 hover:bg-toolbar-hoverbackground" onClick={() => setConfirmClear(false)}>
              {messages.recovery.cancel}
            </button>
            <button
              type="button"
              className="rounded-[4px] bg-destructive px-3 py-1 text-white disabled:opacity-50"
              disabled={busy}
              onClick={clear}
            >
              {messages.recovery.confirmClearAction}
            </button>
          </div>
        </Modal>
      ) : null}
    </div>
  );
}
