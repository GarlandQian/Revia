import type { NoticeText } from "@/components/app/Notifications";
/**
 * 文件历史与 Blame（M6 F01–F04）。
 *
 * 历史只读取 Git 的 --follow 路径；Blame 的路径与版本都带进请求，
 * 不把行号当身份。上一版本由 Rust 解析父提交，前端不假设第一父提交。
 */
import { useEffect, useMemo, useRef, useState } from "react";
import type { FileHistoryEntry } from "@/types/bindings/FileHistoryEntry";
import type { FileHistoryPage } from "@/types/bindings/FileHistoryPage";
import type { FileHistoryPatch } from "@/types/bindings/FileHistoryPatch";
import type { FileHistoryPatchRequest } from "@/types/bindings/FileHistoryPatchRequest";
import type { BlameResult } from "@/types/bindings/BlameResult";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { ipc } from "@/lib/ipc";
import { formatAppError, toAppError } from "@/lib/error";
import { getLocale, messages } from "@/lib/i18n/index";
import { PatchDiffView } from "@/features/diff/PatchDiffView";
import { AppIcon } from "@/components/ui/app-icon";
import { Modal } from "@/components/app/Dialogs";

const PAGE_SIZE = 40;
const FILE_STATUS_CLASS: Record<string, string> = {
  A: "status-added", M: "status-modified", D: "status-deleted",
  R: "status-renamed", C: "status-added", T: "status-typeChanged",
};

type HistoryList = {
  view: string;
  page: FileHistoryPage | null;
  loading: boolean;
  error: AppErrorPayload | null;
};

type PatchResponse = {
  request: FileHistoryPatchRequest;
  result?: FileHistoryPatch;
  error?: AppErrorPayload;
};

function entryKey(entry: FileHistoryEntry): string {
  return `${entry.oid}:${entry.path}`;
}

export function FileHistoryDialog({
  worktreeId,
  path,
  revision,
  showFileNames,
  onClose,
  onJumpCommit,
  onNotify,
}: {
  worktreeId: string;
  path: string;
  /** Full commit selected in the log; null opens the working file. */
  revision: string | null;
  showFileNames: boolean;
  onClose: () => void;
  onJumpCommit: (oid: string, subject: string) => void;
  onNotify: (notice: { tone: "success" | "warning" | "error"; title: NoticeText; detail?: NoticeText }) => void;
}) {
  const [tab, setTab] = useState<"history" | "blame">("history");
  const [allBranches, setAllBranches] = useState(false);
  const [list, setList] = useState<HistoryList | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [patchResponse, setPatchResponse] = useState<PatchResponse | null>(null);
  const [blameError, setBlameError] = useState<AppErrorPayload | null>(null);
  const [blameResult, setBlame] = useState<BlameResult | null>(null);
  const [blameTarget, setBlameTarget] = useState<{ view: string; path: string; revision: string | null } | null>(null);
  const [ignoreWhitespace, setIgnoreWhitespace] = useState(true);
  const [detectMoves, setDetectMoves] = useState(false);
  const [detectCopies, setDetectCopies] = useState(false);
  const [blameLoading, setBlameLoading] = useState(false);
  const historySeq = useRef(0);
  const blameSeq = useRef(0);
  const viewIdentity = JSON.stringify([worktreeId, path, revision]);
  const listIdentity = JSON.stringify([worktreeId, path, revision, allBranches]);
  const history = list?.view === listIdentity ? list : null;
  const entries = history?.page?.entries ?? [];
  const loading = history?.loading ?? true;
  const activeBlameTarget = blameTarget?.view === viewIdentity ? blameTarget : null;
  const blame = activeBlameTarget ? blameResult : null;
  const blamePath = activeBlameTarget?.path ?? path;
  const blameRevision = activeBlameTarget ? activeBlameTarget.revision : revision;

  const load = async (append: boolean) => {
    const sequence = ++historySeq.current;
    const previous = history?.page ?? null;
    setList({ view: listIdentity, page: previous, loading: true, error: null });
    try {
      const page = await ipc.fileHistory(worktreeId, {
        path, allBranches, skip: append && previous ? previous.nextSkip : 0, limit: PAGE_SIZE,
        anchor: append ? previous?.anchor : allBranches ? undefined : revision ?? undefined,
      });
      if (sequence !== historySeq.current) return;
      setList({
        view: listIdentity, loading: false, error: null,
        page: append && previous ? { ...page, entries: [...previous.entries, ...page.entries] } : page,
      });
      if (!append) {
        setSelected(current => page.entries.some(entry => entryKey(entry) === current)
          ? current : page.entries[0] ? entryKey(page.entries[0]) : null);
      }
    } catch (caught) {
      if (sequence === historySeq.current) {
        setList({ view: listIdentity, page: previous, loading: false, error: toAppError(caught) });
      }
    }
  };

  useEffect(() => {
    void load(false);
    return () => { ++historySeq.current; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [listIdentity]);

  const selectedEntry = entries.find(entry => entryKey(entry) === selected);
  const patchRequest = useMemo(() => {
    if (tab !== "history" || !selectedEntry) return null;
    const { oid, path, oldPath } = selectedEntry;
    return { oid, path, oldPath };
  }, [tab, selectedEntry]);
  const preview = patchResponse?.request === patchRequest ? patchResponse : null;

  useEffect(() => {
    if (!patchRequest) return;
    let current = true;
    void ipc.fileHistoryPatch(worktreeId, patchRequest).then(
      result => { if (current) setPatchResponse({ request: patchRequest, result }); },
      caught => { if (current) setPatchResponse({ request: patchRequest, error: toAppError(caught) }); },
    );
    return () => { current = false; };
  }, [worktreeId, patchRequest]);

  const runBlame = (revision: string | null, nextPath: string) => {
    ++blameSeq.current;
    setBlame(null);
    setBlameLoading(true);
    setTab("blame");
    // Intent belongs to the request, even while its first result is pending.
    setBlameTarget({ view: viewIdentity, path: nextPath, revision });
  };

  const updateBlameOption = (setOption: (value: boolean) => void, value: boolean) => {
    ++blameSeq.current;
    setBlame(null);
    setOption(value);
  };

  useEffect(() => {
    const sequence = ++blameSeq.current;
    setBlame(null);
    setBlameError(null);
    if (!blameTarget || blameTarget.view !== viewIdentity) {
      setBlameLoading(false);
      return;
    }
    let disposed = false;
    const isCurrent = () => !disposed && sequence === blameSeq.current;
    // One lifecycle serves initial reads, versions, refresh and all options.
    void (async () => {
      setBlameLoading(true);
      try {
        const result = await ipc.fileBlame(worktreeId, {
          path: blameTarget.path,
          revision: blameTarget.revision,
          ignoreWhitespace,
          detectMoves,
          detectCopies,
        });
        if (!isCurrent()) return;
        setBlame(result);
      } catch (caught) {
        if (isCurrent()) {
          const failure = toAppError(caught);
          setBlameError(failure);
          onNotify({ tone: "error", title: () => formatAppError(failure) });
        }
      } finally {
        if (isCurrent()) setBlameLoading(false);
      }
    })();
    return () => { disposed = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [worktreeId, viewIdentity, blameTarget, ignoreWhitespace, detectMoves, detectCopies]);

  const previous = () => {
    const target = blame?.previous;
    if (!target) {
      onNotify({ tone: "warning", title: () => messages.fileHistory.noPrevious });
      return;
    }
    runBlame(target.oid, target.path);
  };

  const error = tab === "history" ? history?.error : activeBlameTarget ? blameError : null;

  return (
    <Modal title={messages.fileHistory.title(path)} onClose={onClose} width={960}>
      <div className="flex min-h-[52vh] flex-col">
        <div className="mb-2 flex items-center gap-1 border-b border-[var(--border-subtle)]">
          <button
            type="button"
            className={`tab-button h-7 px-2.5 ${tab === "history" ? "tab-button-active" : ""}`}
            onClick={() => setTab("history")}
          >
            {messages.fileHistory.history}
          </button>
          <button
            type="button"
            className={`tab-button h-7 px-2.5 ${tab === "blame" ? "tab-button-active" : ""}`}
            onClick={() => {
              setTab("blame");
              if (!activeBlameTarget) runBlame(revision, path);
            }}
          >
            {messages.fileHistory.blame}
          </button>
          <button
            type="button"
            className="ml-auto icon-action size-[22px]"
            title={messages.fileHistory.refresh}
            onClick={() => {
              if (tab === "blame") void runBlame(blameRevision, blamePath);
              else void load(false);
            }}
          >
            <AppIcon name="refresh" size={14} />
          </button>
        </div>

        {error ? <p role="alert" className="mb-1 text-[12px] text-destructive">{formatAppError(error)}</p> : null}

        {tab === "history" ? (
          <div className="flex min-h-0 flex-1 gap-2">
            <div className="flex w-[42%] min-w-[280px] flex-col rounded border border-[var(--border-subtle)]">
              <label className="flex items-center gap-2 border-b border-[var(--border-subtle)] px-2 py-1 text-[11.5px]">
                <input type="checkbox" checked={allBranches} onChange={(event) => setAllBranches(event.target.checked)} />
                {messages.fileHistory.allBranches}
                <span className="ml-auto text-[var(--text-secondary)]">{entries.length}</span>
              </label>
              <div className="min-h-0 flex-1 overflow-y-auto">
                {loading && entries.length === 0 ? (
                  <p className="px-2 py-2 text-[12px] text-[var(--text-secondary)]">{messages.fileHistory.loading}</p>
                ) : entries.length === 0 ? (
                  <p className="px-2 py-2 text-[12px] text-[var(--text-secondary)]">{messages.fileHistory.empty}</p>
                ) : entries.map((entry) => (
                  <div
                    key={entryKey(entry)}
                    className={`cursor-default border-b border-[var(--border-subtle)]/60 px-2 py-1 ${entryKey(entry) === selected ? "bg-[var(--surface-active-neutral)]" : "hover:bg-[var(--surface-hover)]"}`}
                    onClick={() => setSelected(entryKey(entry))}
                    onDoubleClick={() => onJumpCommit(entry.oid, entry.subject)}
                  >
                    <div className="flex items-center gap-1.5">
                      <span className="min-w-0 flex-1 truncate text-[12px]" title={entry.subject}>{entry.subject}</span>
                      <span className="shrink-0 font-mono text-[10.5px] text-[var(--text-secondary)]">{entry.shortOid}</span>
                    </div>
                    <p className="truncate text-[10.5px] text-[var(--text-secondary)]">
                      {entry.author} · {new Date(entry.authorTimeMs).toLocaleString(getLocale())}
                      {entry.oldPath ? ` · ${entry.oldPath}` : ""}
                    </p>
                    {showFileNames ? (
                      <p className={`truncate font-mono text-[10.5px] ${FILE_STATUS_CLASS[entry.status] ?? "text-[var(--text-secondary)]"}`} title={entry.path}>
                        {entry.oldPath ? `${entry.oldPath} → ${entry.path}` : entry.path}
                      </p>
                    ) : null}
                  </div>
                ))}
              </div>
              {history?.page?.hasMore ? (
                <button
                  type="button"
                  className="border-t border-[var(--border-subtle)] py-1 text-[11.5px] hover:bg-[var(--surface-hover)] disabled:opacity-50"
                  disabled={loading}
                  onClick={() => void load(true)}
                >
                  {loading ? messages.fileHistory.loading : messages.fileHistory.loadMore}
                </button>
              ) : null}
            </div>
            <div className="flex min-w-0 flex-1 flex-col rounded border border-[var(--border-subtle)]">
              {selectedEntry ? (
                <>
                  <div className="flex items-center gap-2 border-b border-[var(--border-subtle)] px-2 py-1 text-[11.5px]">
                    <span className="truncate font-semibold">{selectedEntry.subject}</span>
                    {preview?.result?.patchTruncated ? (
                      <span className="text-warningforeground">{messages.fileHistory.patchTruncated}</span>
                    ) : null}
                    <span className="ml-auto flex gap-1">
                      <button type="button" className="revia-btn revia-btn-secondary h-6 px-2" onClick={() => void runBlame(selectedEntry.oid, selectedEntry.path)}>
                        {messages.fileHistory.blameThis}
                      </button>
                      <button type="button" className="revia-btn revia-btn-secondary h-6 px-2" onClick={() => onJumpCommit(selectedEntry.oid, selectedEntry.subject)}>
                        {messages.fileHistory.jumpLog}
                      </button>
                    </span>
                  </div>
                  <div className="min-h-0 flex-1">
                    {!preview ? <p role="status" className="p-2 text-[11px] text-[var(--text-secondary)]">{messages.fileHistory.loading}</p>
                      : preview.error ? <p role="alert" className="p-2 text-[11px] text-[var(--text-secondary)]">{formatAppError(preview.error)}</p>
                      : preview.result?.patch ? <PatchDiffView patch={preview.result.patch} path={selectedEntry.path}
                        identity={JSON.stringify([worktreeId, selectedEntry.oid, selectedEntry.path])} />
                      : <p className="p-2 text-[11px] text-[var(--text-secondary)]">{messages.fileHistory.noPatch}</p>}
                  </div>
                </>
              ) : (
                <p className="p-2 text-[12px] text-[var(--text-secondary)]">{messages.fileHistory.selectHint}</p>
              )}
            </div>
          </div>
        ) : (
          <div className="flex min-h-0 flex-1 flex-col">
            <div className="flex flex-wrap items-center gap-2 border-b border-[var(--border-subtle)] pb-1.5 text-[11.5px]">
              <span className="min-w-0 truncate font-mono">{blamePath}</span>
              <span className="text-[var(--text-secondary)]">
                {blameRevision ? blameRevision.slice(0, 12) : messages.fileHistory.workingCopy}
              </span>
              <label className="flex items-center gap-1">
                <input type="checkbox" checked={ignoreWhitespace} onChange={(event) => updateBlameOption(setIgnoreWhitespace, event.target.checked)} />
                {messages.fileHistory.ignoreWhitespace}
              </label>
              <label className="flex items-center gap-1">
                <input type="checkbox" checked={detectMoves} onChange={(event) => updateBlameOption(setDetectMoves, event.target.checked)} />
                {messages.fileHistory.detectMoves}
              </label>
              <label className="flex items-center gap-1">
                <input type="checkbox" checked={detectCopies} onChange={(event) => updateBlameOption(setDetectCopies, event.target.checked)} />
                {messages.fileHistory.detectCopies}
              </label>
              <span className="ml-auto flex items-center gap-1">
                {/* 禁用按钮在 WebView 里不显示 title，根或不可读取版本的原因直接写出来（F02）。 */}
                {blame && !blame.previous && !blameLoading ? (
                  <span className="text-[12px] text-[var(--text-secondary)]">{messages.fileHistory.noPrevious}</span>
                ) : null}
                <button
                  type="button"
                  className="revia-btn revia-btn-secondary h-6 px-2"
                  disabled={!blame?.previous || blameLoading}
                  title={!blame?.previous ? messages.fileHistory.noPrevious : messages.fileHistory.previous}
                  onClick={() => void previous()}
                >
                  {messages.fileHistory.previous}
                </button>
                <button type="button" className="revia-btn revia-btn-secondary h-6 px-2" onClick={() => void runBlame(null, path)}>
                  {messages.fileHistory.workingCopy}
                </button>
              </span>
            </div>
            <div className="min-h-0 flex-1 overflow-auto rounded border border-[var(--border-subtle)]">
              {blameLoading && !blame ? (
                <p className="p-2 text-[12px] text-[var(--text-secondary)]">{messages.fileHistory.loading}</p>
              ) : !blame || blame.lines.length === 0 ? (
                <p className="p-2 text-[12px] text-[var(--text-secondary)]">{messages.fileHistory.noBlame}</p>
              ) : (
                <table className="w-full border-collapse font-mono text-[11px]">
                  <thead className="sticky top-0 bg-panel-background">
                    <tr className="text-left text-[var(--text-secondary)]">
                      <th className="w-12 px-1 py-0.5 font-normal">{messages.fileHistory.line}</th>
                      <th className="w-20 px-1 py-0.5 font-normal">{messages.fileHistory.commit}</th>
                      <th className="w-32 px-1 py-0.5 font-normal">{messages.fileHistory.author}</th>
                      <th className="w-36 px-1 py-0.5 font-normal">{messages.fileHistory.time}</th>
                      <th className="px-1 py-0.5 font-normal">{messages.fileHistory.content}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {blame.lines.map((line) => (
                      <tr key={line.line} className="border-t border-[var(--border-subtle)]/40 hover:bg-[var(--surface-hover)]">
                        <td className="px-1 text-right tabular-nums text-[var(--text-secondary)]">{line.line}</td>
                        <td className="px-1">
                          <button
                            type="button"
                            className="font-mono hover:underline"
                            title={line.oid}
                            disabled={line.uncommitted}
                            onClick={() => {
                              onJumpCommit(line.oid, line.summary);
                              onClose();
                            }}
                          >
                            {line.uncommitted ? messages.fileHistory.uncommitted : line.shortOid}
                          </button>
                        </td>
                        <td className="max-w-32 truncate px-1" title={line.author}>{line.author}</td>
                        <td className="px-1 text-[var(--text-secondary)]">{new Date(line.authorTimeMs).toLocaleDateString(getLocale())}</td>
                        <td className="whitespace-pre-wrap break-all px-1">{line.content}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          </div>
        )}
      </div>
    </Modal>
  );
}
