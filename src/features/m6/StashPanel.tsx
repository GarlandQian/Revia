/**
 * Stash 工具窗口（M6 S01–S03）。
 *
 * 列表绑定 reflog 版本与具体出现项，内容绑定完整 OID。写操作走 App 的统一写门禁，
 * 删除前由 Rust 先写入应用数据目录里的独立归档（ADR-0012）。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { StashEntry } from "@/types/bindings/StashEntry";
import type { StashInspection } from "@/types/bindings/StashInspection";
import type { StashArchiveSummary } from "@/types/bindings/StashArchiveSummary";
import type { StashApplyOutcome } from "@/types/bindings/StashApplyOutcome";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { ipc } from "@/lib/ipc";
import { formatAppError, toAppError } from "@/lib/error";
import { getLocale, messages } from "@/lib/i18n/index";
import { stashFileStatusText } from "@/lib/i18n/text";
import { Codicon } from "@/components/ui/codicon";
import { Modal } from "@/components/app/Dialogs";
import type { NoticeText } from "@/components/app/Notifications";
import { PatchDiffView } from "@/features/diff/PatchDiffView";

interface Notice {
  tone: "success" | "warning" | "error";
  title: NoticeText;
  detail?: NoticeText;
}

/**
 * 保存确认只是读取。切换或刷新 worktree 时，同时进行的读取会推进确认代次，
 * 后端因此判这次读取过期；重新读取即可，不把它当成需要用户处理的错误。
 */
async function savePreflight(worktreeId: string): Promise<string> {
  for (let attempt = 1; ; attempt += 1) {
    try {
      return await ipc.stashPreflight(worktreeId);
    } catch (caught) {
      if (attempt >= 3 || toAppError(caught).code !== "errors.rewrite.stateChanged") throw caught;
    }
  }
}

export function StashPanel({
  worktreeId,
  busy,
  activation,
  refreshKey,
  focusOid,
  compareLocal,
  onCompareLocal,
  onSave,
  onApply,
  onDrop,
  onClear,
  onRestoreArchive,
  onRefresh,
  onNotify,
}: {
  worktreeId: string | null;
  busy: boolean;
  activation: number;
  refreshKey: string;
  focusOid: string | null;
  compareLocal: boolean;
  onCompareLocal: (value: boolean) => Promise<AppErrorPayload | null>;
  onSave: (message: string, keepIndex: boolean, expectedState: string) => Promise<void>;
  onApply: (entry: StashEntry, options: {
    restoreIndex: boolean;
    deleteAfter: boolean;
    newBranch: string | null;
    expectedState: string;
  }) => Promise<StashApplyOutcome | void>;
  onDrop: (entry: StashEntry, expectedState: string) => Promise<void>;
  onClear: (oids: string[], expectedState: string) => Promise<void>;
  onRestoreArchive: (archive: StashArchiveSummary, expectedState: string) => Promise<void>;
  onRefresh: () => void;
  onNotify: (notice: Notice) => void;
}) {
  const [entries, setEntries] = useState<StashEntry[]>([]);
  const [archives, setArchives] = useState<StashArchiveSummary[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [inspection, setInspection] = useState<StashInspection | null>(null);
  const [inspectionRevision, setInspectionRevision] = useState(0);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState("");
  const [keepIndex, setKeepIndex] = useState(false);
  const [applyTarget, setApplyTarget] = useState<StashEntry | null>(null);
  const [restoreIndex, setRestoreIndex] = useState(false);
  const [deleteAfter, setDeleteAfter] = useState(false);
  const [newBranch, setNewBranch] = useState("");
  const [confirmDrop, setConfirmDrop] = useState<StashEntry | null>(null);
  const [confirmClear, setConfirmClear] = useState<string[] | null>(null);
  const [confirmArchive, setConfirmArchive] = useState<StashArchiveSummary | null>(null);
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [archivesOpen, setArchivesOpen] = useState(false);
  const [otherArchivesOpen, setOtherArchivesOpen] = useState(false);
  const [focusedArchive, setFocusedArchive] = useState<string | null>(null);
  const archiveListRef = useRef<HTMLUListElement>(null);
  const loadSeq = useRef(0);
  const entriesRef = useRef<StashEntry[]>([]);
  const focused = useRef<string | null>(null);
  const mounted = useRef(true);
  const [saveState, setSaveState] = useState<string | null>(null);
  const [applyState, setApplyState] = useState<string | null>(null);
  const [confirmState, setConfirmState] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    setConfirmState(null);
    if (worktreeId && (confirmDrop || confirmClear || confirmArchive)) {
      void ipc.stashPreflight(worktreeId, confirmDrop, !!confirmClear).then((state) => {
        if (!cancelled && mounted.current) setConfirmState(state);
      }).catch((caught) => { if (!cancelled && mounted.current) setError(toAppError(caught)); });
    }
    return () => { cancelled = true; };
  }, [worktreeId, activation, refreshKey, confirmDrop, confirmClear, confirmArchive]);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; loadSeq.current += 1; };
  }, []);

  // 从自动保存提示跳到 stash 面板时，定位到对应的 OID 条目。
  useEffect(() => {
    if (!focusOid) { focused.current = null; setFocusedArchive(null); return; }
    const entry = entries.find((entry) => entry.oid === focusOid);
    if (entry) {
      const identity = `${worktreeId}:entry:${entry.key}`;
      if (focused.current !== identity) {
        setSelected(entry.key); setFocusedArchive(null); focused.current = identity;
      }
    } else {
      const archive = archives.find(archive => archive.stashOid === focusOid && archive.identityConfirmed);
      const identity = `${worktreeId}:archive:${archive?.id}`;
      if (archive && focused.current !== identity) {
        setFocusedArchive(archive.id); setArchivesOpen(true); focused.current = identity;
      }
    }
  }, [focusOid, worktreeId, entries, archives]);
  useEffect(() => {
    archiveListRef.current?.querySelector('[aria-current="true"]')?.scrollIntoView({ block: "nearest" });
  }, [focusedArchive, archivesOpen, archives]);

  // 切换 worktree 或重新激活时丢弃旧目标的确认框；这些对话框在打开后
  // 可能停留很久，不能让迟到确认写到新 worktree（S1）。
  useEffect(() => {
    setApplyTarget(null);
    setConfirmDrop(null);
    setConfirmClear(null);
    setConfirmArchive(null);
  }, [worktreeId, activation, refreshKey]);

  // 切换 worktree 时旧条目不属于新目标，不能拿它去新 worktree 读取或确认。
  useEffect(() => {
    entriesRef.current = [];
    setEntries([]);
    setSelected(null);
    setInspection(null);
    setSaveState(null);
    setArchives([]);
    setError(null);
  }, [worktreeId]);

  const load = useCallback(async () => {
    if (!mounted.current) return;
    if (!worktreeId) {
      setEntries([]);
      setArchives([]);
      return;
    }
    const sequence = ++loadSeq.current;
    setLoading(true);
    setInspectionRevision((revision) => revision + 1);
    try {
      const [nextEntries, nextArchives, nextSaveState] = await Promise.all([
        ipc.stashList(worktreeId),
        ipc.stashArchives(worktreeId).catch(() => []),
        savePreflight(worktreeId),
      ]);
      if (!mounted.current || sequence !== loadSeq.current) return;
      setSaveState(nextSaveState);
      const previous = entriesRef.current;
      entriesRef.current = nextEntries;
      setEntries(nextEntries);
      setSelected((current) => {
        if (current && nextEntries.some((entry) => entry.key === current)) return current;
        const old = previous.find((entry) => entry.key === current);
        const matches = old ? nextEntries.filter((entry) => entry.oid === old.oid
          && entry.message === old.message && entry.timeMs === old.timeMs) : [];
        return matches.length === 1 ? matches[0].key : nextEntries[0]?.key ?? null;
      });
      setArchives(nextArchives);
      setError(null);
    } catch (caught) {
      if (mounted.current && sequence === loadSeq.current) setError(toAppError(caught));
    } finally {
      if (mounted.current && sequence === loadSeq.current) setLoading(false);
    }
  }, [worktreeId]);

  useEffect(() => {
    if (!worktreeId) return;
    void load();
  }, [worktreeId, activation, refreshKey, load]);

  useEffect(() => {
    if (!worktreeId || !selected) {
      setInspection(null);
      return;
    }
    let cancelled = false;
    setInspection(null);
    void (async () => {
      try {
        const entry = entries.find((entry) => entry.key === selected);
        if (!entry) return;
        const next = await ipc.stashInspect(worktreeId, entry, compareLocal);
        if (!cancelled) setInspection(next);
      } catch (caught) {
        if (!cancelled) setInspection(null);
        if (!cancelled) setError(toAppError(caught));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [worktreeId, selected, entries, compareLocal, inspectionRevision]);

  const ownArchives = archives.filter((archive) => archive.identityConfirmed);
  const otherArchives = archives.filter((archive) => !archive.identityConfirmed);

  const run = async (work: () => Promise<Notice | void>, success: NoticeText) => {
    try {
      setError(null);
      const notice = await work();
      onNotify(notice ?? { tone: "success", title: success });
      if (mounted.current) { await load(); onRefresh(); }
    } catch (caught) {
      const failure = toAppError(caught);
      if (mounted.current) setError(failure);
      onNotify({ tone: failure.kind === "needsVerification" ? "warning" : "error", title: () => (formatAppError(failure)) });
    }
  };

  if (!worktreeId) {
    return <p className="px-3 py-2 text-[12px] text-descriptionforeground">{messages.stash.noWorktree}</p>;
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-1 border-b border-panel-border px-2 py-1">
        <button type="button" className="icon-action size-[22px]" title={messages.stash.refresh} onClick={() => void load()}>
          <Codicon name="refresh" size={15} />
        </button>
        <span className="flex-1 text-[12px] font-semibold">{messages.stash.title}</span>
        <button
          type="button"
          className="icon-action size-[22px]"
          title={messages.stash.clear}
          disabled={busy || entries.length === 0}
          onClick={() => setConfirmClear(entries.map((entry) => entry.oid))}
        >
          <Codicon name="clear-all" size={15} />
        </button>
      </div>
      <div className="shrink-0 space-y-1 border-b border-panel-border p-2">
        <input
          className="settings-input w-full text-[12px]"
          value={message}
          placeholder={messages.stash.messagePlaceholder}
          spellCheck={false}
          onChange={(event) => setMessage(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !busy && !!saveState) {
              void run(async () => { if (!saveState) return; await onSave(message, keepIndex, saveState); if (mounted.current) setMessage(""); }, () => messages.stash.saved);
            }
          }}
        />
        <div className="flex items-center gap-2">
          <label className="flex items-center gap-1.5 text-[11.5px] text-descriptionforeground">
            <input type="checkbox" checked={keepIndex} onChange={(event) => setKeepIndex(event.target.checked)} />
            {messages.stash.keepIndex}
          </label>
          <button
            type="button"
            className="ml-auto rounded-[4px] bg-[var(--button-background)] px-2.5 py-0.5 text-[12px] text-[var(--button-foreground)] disabled:opacity-50"
            disabled={busy || !saveState}
            onClick={() => void run(async () => { if (!saveState) return; await onSave(message, keepIndex, saveState); if (mounted.current) setMessage(""); }, () => messages.stash.saved)}
          >
            {messages.stash.save}
          </button>
        </div>
      </div>
      {error ? <p role="alert" className="shrink-0 px-2 py-1 text-[11.5px] text-destructive">{formatAppError(error)}</p> : null}
      <div className="min-h-0 flex-1 overflow-y-auto">
        {loading && entries.length === 0 ? (
          <p className="px-3 py-2 text-[12px] text-descriptionforeground">{messages.stash.loading}</p>
        ) : entries.length === 0 ? (
          <p className="px-3 py-2 text-[12px] text-descriptionforeground">{messages.stash.empty}</p>
        ) : (
          entries.map((entry) => {
            const active = entry.key === selected;
            return (
              <div
                key={entry.key}
                className={`group flex cursor-default items-start gap-1 px-2 py-1 ${active ? "bg-[var(--list-inactiveselectionbackground)]" : "hover:bg-toolbar-hoverbackground"}`}
                onClick={() => setSelected(entry.key)}
              >
                <Codicon name="archive" size={14} className="mt-0.5 shrink-0" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[12px]" title={entry.message}>{entry.message}</p>
                  <p className="truncate text-[11px] text-descriptionforeground">
                    {entry.branch ? `${entry.branch} · ` : ""}{new Date(entry.timeMs).toLocaleString(getLocale())}
                  </p>
                </div>
                <div className="flex shrink-0 gap-0.5 opacity-0 group-hover:opacity-100">
                  <button
                    type="button"
                    className="icon-action size-[20px]"
                    title={messages.stash.apply}
                    disabled={busy}
                    onClick={(event) => {
                      event.stopPropagation();
                      setApplyState(null);
                      const sequence = loadSeq.current;
                      void ipc.stashPreflight(worktreeId, entry).then((token) => {
                        if (!mounted.current || sequence !== loadSeq.current) return;
                        setApplyState(token);
                        setApplyTarget(entry);
                      }).catch((caught) => { if (mounted.current) setError(toAppError(caught)); });
                      setRestoreIndex(false);
                      setDeleteAfter(false);
                      setNewBranch("");
                    }}
                  >
                    <Codicon name="check" size={13} />
                  </button>
                  <button
                    type="button"
                    className="icon-action size-[20px]"
                    title={messages.stash.drop}
                    disabled={busy}
                    onClick={(event) => {
                      event.stopPropagation();
                      setConfirmDrop(entry);
                    }}
                  >
                    <Codicon name="trash" size={13} />
                  </button>
                </div>
              </div>
            );
          })
        )}
        {inspection ? (
          <div className="border-t border-panel-border p-2">
            <label className="mb-1 flex items-center gap-1.5 text-[11px] text-descriptionforeground">
              <input type="checkbox" checked={compareLocal} onChange={(event) => {
                void onCompareLocal(event.target.checked).then((failure) => { if (failure && mounted.current) setError(failure); });
              }} />
              {messages.stash.compareLocal}
            </label>
            <div className="mb-1 flex items-center gap-1 text-[11px] text-descriptionforeground">
              <span>{messages.stash.files(inspection.files.length)}</span>
              {inspection.truncated ? <span>· {messages.stash.truncated}</span> : null}
            </div>
            <ul className="max-h-28 overflow-y-auto">
              {inspection.files.map((file) => (
                <li key={file.path} className="flex items-center gap-1 text-[11px]">
                  <span className="shrink-0 text-descriptionforeground">{stashFileStatusText(file.status)}</span>
                  <span className="min-w-0 flex-1 truncate font-mono" title={file.path}>{file.path}</span>
                  <span className="shrink-0 tabular-nums text-descriptionforeground">+{file.added} −{file.deleted}</span>
                </li>
              ))}
            </ul>
            <div className="mt-1 h-56 min-h-0 rounded border border-panel-border">
              {inspection.patchError ? <p role="alert" className="p-2 text-[11px] text-descriptionforeground">{formatAppError(inspection.patchError)}</p>
                : <PatchDiffView patch={inspection.patch} path={inspection.entry.message}
                  identity={`${worktreeId}:${inspection.entry.oid}:${compareLocal}`} fontSize={10.5} />}
            </div>
          </div>
        ) : null}
      </div>
      <div className="shrink-0 border-t border-panel-border">
        <button
          type="button"
          className="flex w-full items-center gap-1 px-2 py-1 text-left text-[12px] hover:bg-toolbar-hoverbackground"
          onClick={() => setArchivesOpen((value) => !value)}
        >
          <Codicon name={archivesOpen ? "chevron-down" : "chevron-right"} size={12} />
          {messages.stash.archives(ownArchives.length)}
        </button>
        {archivesOpen ? (
          ownArchives.length === 0 ? (
            <p className="px-4 pb-2 text-[11.5px] text-descriptionforeground">{messages.stash.archiveEmpty}</p>
          ) : (
            <ul ref={archiveListRef} className="max-h-40 overflow-y-auto pb-1">
              {ownArchives.map((archive) => (
                <li key={archive.id} aria-current={archive.id === focusedArchive ? "true" : undefined}
                  className={`flex items-center gap-1 px-2 py-0.5 text-[11.5px] ${archive.id === focusedArchive ? "bg-[var(--list-inactiveselectionbackground)]" : "hover:bg-toolbar-hoverbackground"}`}>
                  <div className="min-w-0 flex-1">
                    <p className="truncate" title={archive.message}>{archive.message}</p>
                    <p className="truncate text-[10.5px] text-descriptionforeground">
                      {new Date(archive.createdAtMs).toLocaleString(getLocale())} · {messages.stash.expires(new Date(archive.expiresAtMs).toLocaleDateString(getLocale()))}
                    </p>
                  </div>
                  <button
                    type="button"
                    className="icon-action size-[20px]"
                    title={messages.stash.restoreArchive}
                    disabled={busy || !archive.identityConfirmed}
                    onClick={() => setConfirmArchive(archive)}
                  >
                    <Codicon name="history" size={13} />
                  </button>
                </li>
              ))}
            </ul>
          )
        ) : null}
        {/* 身份无法确认的归档（其他仓库，或原仓库已移动 / 重建）只读保留，不计入本仓库（S03）。 */}
        {archivesOpen && otherArchives.length > 0 ? (
          <>
            <button
              type="button"
              className="flex w-full items-center gap-1 px-4 py-0.5 text-left text-[11.5px] text-descriptionforeground hover:bg-toolbar-hoverbackground"
              onClick={() => setOtherArchivesOpen((value) => !value)}
            >
              <Codicon name={otherArchivesOpen ? "chevron-down" : "chevron-right"} size={12} />
              {messages.stash.otherArchives(otherArchives.length)}
            </button>
            {otherArchivesOpen ? (
              <ul className="max-h-32 overflow-y-auto pb-1">
                {otherArchives.map((archive) => (
                  <li key={archive.id} className="px-6 py-0.5 text-[11.5px] text-descriptionforeground">
                    <p className="truncate" title={archive.message}>{archive.message}</p>
                    <p className="truncate text-[10.5px]" title={archive.worktreePath}>
                      {archive.worktreePath} · {messages.stash.expires(new Date(archive.expiresAtMs).toLocaleDateString(getLocale()))}
                    </p>
                  </li>
                ))}
              </ul>
            ) : null}
          </>
        ) : null}
      </div>

      {applyTarget ? (
        <Modal title={messages.stash.applyTitle} onClose={() => setApplyTarget(null)} width={500}>
          <p className="truncate font-mono text-[12px]">{applyTarget.message}</p>
          <div className="mt-3 space-y-2">
            <label className="flex items-start gap-2 text-[12.5px]">
              <input type="checkbox" className="mt-0.5" checked={restoreIndex} onChange={(event) => setRestoreIndex(event.target.checked)} />
              <span>
                <span className="block">{messages.stash.restoreIndex}</span>
                <span className="block text-[11.5px] text-descriptionforeground">{messages.stash.restoreIndexHint}</span>
              </span>
            </label>
            <label className="flex items-start gap-2 text-[12.5px]">
              <input type="checkbox" className="mt-0.5" checked={deleteAfter} onChange={(event) => setDeleteAfter(event.target.checked)} />
              <span>{messages.stash.deleteAfter}</span>
            </label>
            <label className="block text-[12.5px]">
              <span className="mb-1 block">{messages.stash.applyNewBranch}</span>
              <input
                className="settings-input w-full"
                value={newBranch}
                placeholder={messages.stash.newBranchPlaceholder}
                spellCheck={false}
                onChange={(event) => setNewBranch(event.target.value)}
              />
            </label>
            <p className="text-[11.5px] text-descriptionforeground">{messages.stash.applyConflictHint}</p>
          </div>
          <div className="mt-4 flex justify-end gap-2">
            <button type="button" className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-3 py-1" onClick={() => setApplyTarget(null)}>
              {messages.stash.cancel}
            </button>
            <button
              type="button"
              className="rounded-[4px] bg-[var(--button-background)] px-3 py-1 text-[var(--button-foreground)] disabled:opacity-50"
              disabled={busy}
              onClick={() => {
                const entry = applyTarget;
                setApplyTarget(null);
                void run(async () => {
                  if (!applyState) return;
                  const outcome = await onApply(entry, { restoreIndex, deleteAfter, newBranch: newBranch.trim() || null, expectedState: applyState });
                  if (outcome?.error) {
                    return { tone: "warning", title: () => formatAppError(outcome.error!) };
                  }
                  if (outcome?.conflicted) {
                    return { tone: "warning", title: () => messages.stash.applyConflicted };
                  }
                  if (outcome?.cleanupError) {
                    return {
                      tone: "warning",
                      title: () => messages.stash.appliedNotDeleted,
                      detail: () => `${messages.stash.appliedNotDeletedHint} ${formatAppError(outcome.cleanupError!)}`,
                    };
                  }
                  return undefined;
                }, () => messages.stash.applied);
              }}
            >
              {messages.stash.apply}
            </button>
          </div>
        </Modal>
      ) : null}

      {confirmDrop ? (
        <Modal title={messages.stash.drop} onClose={() => setConfirmDrop(null)} width={460}>
          <p>{messages.stash.dropConfirm}</p>
          <p className="mt-1 truncate font-mono text-[11.5px] text-descriptionforeground">{confirmDrop.message}</p>
          <p className="mt-2 text-[12px] text-descriptionforeground">{messages.stash.dropSafety}</p>
          <div className="mt-4 flex justify-end gap-2">
            <button type="button" className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-3 py-1" onClick={() => setConfirmDrop(null)}>
              {messages.stash.cancel}
            </button>
            <button
              type="button"
              className="rounded-[4px] bg-destructive px-3 py-1 text-destructive-foreground disabled:opacity-50"
              disabled={busy || !confirmState}
              onClick={() => {
                if (!confirmState) return;
                const entry = confirmDrop;
                setConfirmDrop(null);
                void run(() => onDrop(entry, confirmState), () => messages.stash.dropped);
              }}
            >
              {messages.stash.drop}
            </button>
          </div>
        </Modal>
      ) : null}

      {confirmClear ? (
        <Modal title={messages.stash.clear} onClose={() => setConfirmClear(null)} width={460}>
          <p>{messages.stash.clearConfirm(confirmClear.length)}</p>
          <p className="mt-2 text-[12px] text-descriptionforeground">{messages.stash.dropSafety}</p>
          <div className="mt-4 flex justify-end gap-2">
            <button type="button" className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-3 py-1" onClick={() => setConfirmClear(null)}>
              {messages.stash.cancel}
            </button>
            <button
              type="button"
              className="rounded-[4px] bg-destructive px-3 py-1 text-destructive-foreground disabled:opacity-50"
              disabled={busy || !confirmState}
              onClick={() => {
                if (!confirmState) return;
                const oids = confirmClear;
                setConfirmClear(null);
                void run(() => onClear(oids, confirmState), () => messages.stash.cleared);
              }}
            >
              {messages.stash.clear}
            </button>
          </div>
        </Modal>
      ) : null}

      {confirmArchive ? (
        <Modal title={messages.stash.restoreArchive} onClose={() => setConfirmArchive(null)} width={460}>
          <p>{messages.stash.restoreArchiveConfirm}</p>
          <p className="mt-1 truncate font-mono text-[11.5px] text-descriptionforeground">{confirmArchive.message}</p>
          <p className="mt-2 text-[12px] text-descriptionforeground">{messages.stash.restoreArchiveHint}</p>
          <div className="mt-4 flex justify-end gap-2">
            <button type="button" className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-3 py-1" onClick={() => setConfirmArchive(null)}>
              {messages.stash.cancel}
            </button>
            <button
              type="button"
              className="rounded-[4px] bg-[var(--button-background)] px-3 py-1 text-[var(--button-foreground)] disabled:opacity-50"
              disabled={busy || !confirmState}
              onClick={() => {
                if (!confirmState) return;
                const archive = confirmArchive;
                setConfirmArchive(null);
                void run(() => onRestoreArchive(archive, confirmState), () => messages.stash.archiveRestored);
              }}
            >
              {messages.stash.restoreArchive}
            </button>
          </div>
        </Modal>
      ) : null}
    </div>
  );
}
