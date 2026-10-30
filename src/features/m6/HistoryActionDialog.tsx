import type { NoticeText } from "@/components/app/Notifications";
/**
 * 历史改写对话框（M6 H01–H05）。
 *
 * 目标提交与提交清单由日志视图传入；这里不自行刷新日志，执行完成后由 App
 * 统一刷新工作区、日志和引用。reset 不自动恢复任何改动，hard / keep 的
 * 未提交内容由 Rust 在破坏前写入回收区。
 */
import { useEffect, useMemo, useState } from "react";
import type { HistoryCommit } from "@/types/bindings/HistoryCommit";
import type { DirtyStrategy } from "@/types/bindings/DirtyStrategy";
import type { RebaseAction } from "@/types/bindings/RebaseAction";
import type { ResetMode } from "@/types/bindings/ResetMode";
import type { RewritePreflight } from "@/types/bindings/RewritePreflight";
import type { RewriteOutcome } from "@/types/bindings/RewriteOutcome";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { RemoteTarget } from "@/features/branches/remoteContinuation";
import { ipc } from "@/lib/ipc";
import { formatAppError, toAppError } from "@/lib/error";
import { messages } from "@/lib/i18n/index";
import { Codicon } from "@/components/ui/codicon";
import { Modal } from "@/components/app/Dialogs";

export type HistoryAction = "cherryPick" | "revert" | "reset" | "reword" | "interactive";

export interface HistoryActionRequest {
  /** 打开对话框时冻结的目标；迟到确认不得落到切换后的 worktree。 */
  origin: RemoteTarget;
  action: HistoryAction;
  commit: HistoryCommit;
  /** Rust 按当前 HEAD 读取的固定祖先范围（新到旧），不受日志筛选影响。 */
  commits: HistoryCommit[];
  /** 当前多选（新到旧）；cherry-pick / revert 按选择集执行。 */
  selection: HistoryCommit[];
  /** 当前 HEAD，用于执行前确认。 */
  head: string | null;
}

const RESET_MODE_KEY = "revia.history.resetMode";

function rememberedResetMode(): ResetMode {
  try {
    const value = window.localStorage.getItem(RESET_MODE_KEY);
    // Hard 会丢弃工作区改动，不作为下次默认，回到 Mixed。
    if (value === "soft" || value === "mixed" || value === "keep") return value;
  } catch {
    // localStorage may be unavailable in hardened webviews; the native
    // default (Mixed) is still valid.
  }
  return "mixed";
}

function actionText(action: HistoryAction): string {
  switch (action) {
    case "cherryPick": return messages.historyActions.cherryPick;
    case "revert": return messages.historyActions.revert;
    case "reset": return messages.historyActions.reset;
    case "reword": return messages.historyActions.reword;
    case "interactive": return messages.historyActions.interactive;
  }
}

function rebaseActionText(action: RebaseAction): string {
  switch (action) {
    case "pick": return messages.historyActions.pick;
    case "reword": return messages.historyActions.rewordAction;
    case "edit": return messages.historyActions.edit;
    case "squash": return messages.historyActions.squash;
    case "fixup": return messages.historyActions.fixup;
    case "drop": return messages.historyActions.drop;
  }
}

export function HistoryActionDialog({
  request,
  initialMessage,
  cherryPickKeepEmpty,
  cherryPickIncludeSource,
  busy,
  onClose,
  onRun,
  onNotify,
}: {
  request: HistoryActionRequest;
  initialMessage: string;
  cherryPickKeepEmpty: boolean;
  cherryPickIncludeSource: boolean;
  busy: boolean;
  onClose: () => void;
  onRun: (run: () => Promise<RewriteOutcome>) => Promise<void>;
  onNotify: (notice: { tone: "success" | "warning" | "error"; title: NoticeText; detail?: NoticeText }) => void;
}) {
  const { action, commit, commits, selection, head } = request;
  const worktreeId = request.origin.worktreeId;
  const [preflight, setPreflight] = useState<RewritePreflight | null>(null);
  const [strategy, setStrategy] = useState<DirtyStrategy>("stash");
  const [resetMode, setResetMode] = useState<ResetMode>(rememberedResetMode);
  const [message, setMessage] = useState(initialMessage);
  const [rangeEntries, setRangeEntries] = useState<Array<{ oid: string; action: RebaseAction; message: string | null }>>([]);
  const [error, setError] = useState<AppErrorPayload | { validation: "messageRequired" | "emptyRange" | "rewordMessageRequired" | "firstCannotSquash" | "rootCommitUnsupported" } | null>(null);
  const [loading, setLoading] = useState(false);
  const destructive = action === "reset" && resetMode === "hard";

  const range = useMemo(() => {
    if (action !== "interactive") return [];
    const start = commits.findIndex((entry) => entry.oid === commit.oid);
    if (start < 0) return [commit];
    return commits.slice(0, start + 1).reverse();
  }, [action, commit, commits]);

  const orderedSelection = useMemo(() => {
    const selected = selection.length > 0 ? selection : [commit];
    return action === "cherryPick" ? [...selected].reverse() : selected;
  }, [action, commit, selection]);

  const preflightOids = useMemo(() => {
    if (action === "interactive") return range.map((entry) => entry.oid);
    if (action === "reword") return commits.map((entry) => entry.oid);
    if (action === "cherryPick" || action === "revert") return orderedSelection.map((entry) => entry.oid);
    return [commit.oid];
  }, [action, commit.oid, commits, orderedSelection, range]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void ipc.rewritePreflight(worktreeId, preflightOids, action === "reset" ? commit.oid : null)
      .then((result) => {
        if (cancelled) return;
        setPreflight(result);
        setStrategy("stash");
      })
      .catch((caught) => {
        if (!cancelled) setError(toAppError(caught));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // The backend preflight is rebound whenever the selected commit set changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [preflightOids.join("\0"), worktreeId, action]);

  useEffect(() => {
    if (action !== "interactive") return;
    setRangeEntries(range.map((entry) => ({ oid: entry.oid, action: "pick" as RebaseAction, message: null })));
  }, [action, range]);

  const move = (index: number, delta: number) => {
    const next = [...rangeEntries];
    const target = index + delta;
    if (target < 0 || target >= next.length) return;
    [next[index], next[target]] = [next[target], next[index]];
    setRangeEntries(next);
  };

  const updateEntry = (index: number, patch: Partial<{ oid: string; action: RebaseAction; message: string | null }>) => {
    setRangeEntries((current) => current.map((entry, at) => at === index ? { ...entry, ...patch } : entry));
  };

  const submit = () => {
    setError(null);
    (async () => {
      if (!preflight) return;
      if (action === "cherryPick" || action === "revert") {
        if (!preflight) return;
        if (!preflight.requiresChoice) {
          // 没有会覆盖的改动时仍走后端，后端按真实状态再判断一次。
        }
        await onRun(async () => await (action === "cherryPick"
          ? ipc.rewriteCherryPick(worktreeId, {
            oids: orderedSelection.map((entry) => entry.oid),
            expectedHead: head ?? "",
            expectedContext: preflight.contextVersion,
            expectedState: preflight.overwriteFingerprint,
            dirtyStrategy: strategy,
            keepEmpty: cherryPickKeepEmpty,
            includeSource: cherryPickIncludeSource,
          })
          : ipc.rewriteRevert(worktreeId, {
            oids: orderedSelection.map((entry) => entry.oid),
            expectedHead: head ?? "",
            expectedContext: preflight.contextVersion,
            expectedState: preflight.overwriteFingerprint,
            dirtyStrategy: strategy,
          })));
        return;
      }
      if (action === "reset") {
        if (!preflight) return;
        try {
          window.localStorage.setItem(RESET_MODE_KEY, resetMode);
        } catch {
          // Persisting the remembered mode is a convenience, not a gate.
        }
        await onRun(() => ipc.rewriteReset(worktreeId, {
          target: commit.oid,
          mode: resetMode,
          expectedHead: head ?? "",
          expectedContext: preflight.contextVersion,
          expectedState: preflight.resetFingerprint,
        }));
        return;
      }
      if (action === "reword") {
        if (!message.trim()) {
          setError({ validation: "messageRequired" });
          return;
        }
        await onRun(() => ipc.rewriteReword(worktreeId, {
          target: commit.oid,
          expectedHead: head ?? "",
          expectedContext: preflight.contextVersion,
          expectedState: preflight.overwriteFingerprint,
          message,
        }));
        return;
      }
      if (rangeEntries.length === 0) {
        setError({ validation: "emptyRange" });
        return;
      }
      const rewordEntry = rangeEntries.find((entry) => entry.action === "reword" && !entry.message?.trim());
      if (rewordEntry) {
        setError({ validation: "rewordMessageRequired" });
        return;
      }
      if (rangeEntries[0]?.action === "squash" || rangeEntries[0]?.action === "fixup") {
        setError({ validation: "firstCannotSquash" });
        return;
      }
      const upstream = range[0]?.parents[0] ?? "";
      if (!upstream) {
        setError({ validation: "rootCommitUnsupported" });
        return;
      }
      await onRun(() => ipc.rewriteInteractive(worktreeId, {
        upstream,
        expectedHead: head ?? "",
        expectedContext: preflight.contextVersion,
        expectedState: preflight.overwriteFingerprint,
        entries: rangeEntries,
      }));
    })().catch((caught) => {
      const failure = toAppError(caught);
      setError(failure);
      if (failure.kind === "needsVerification") {
        onNotify({ tone: "warning", title: () => (messages.errors.needsVerification), detail: () => (formatAppError(failure)) });
      }
    });
  };

  const dirty = preflight?.requiresChoice ?? false;

  return (
    <Modal title={actionText(action)} onClose={onClose} width={710}>
      <div className="space-y-3">
        {orderedSelection.length > 1 ? null : (
          <div className="rounded border border-panel-border bg-muted/30 px-2 py-1.5">
            <p className="truncate font-semibold">{commit.subject}</p>
            <p className="font-mono text-[11px] text-descriptionforeground">{commit.oid.slice(0, 12)}</p>
          </div>
        )}

        {(action === "reword" || action === "interactive" || action === "reset") && preflight?.published ? (
          <p className="rounded border border-[var(--editorwarning-foreground)]/50 px-2 py-1.5 text-[12px] text-[var(--editorwarning-foreground)]">
            {messages.historyActions.publishedWarning}
          </p>
        ) : null}

        {action === "cherryPick" || action === "revert" ? (
          <>
            <p className="text-[12.5px] leading-relaxed text-descriptionforeground">
              {orderedSelection.length > 1
                ? (action === "cherryPick" ? messages.historyActions.cherryPickHintMany : messages.historyActions.revertHintMany)
                : (action === "cherryPick" ? messages.historyActions.cherryPickHint : messages.historyActions.revertHint)}
            </p>
            {orderedSelection.length > 1 ? (
              <p className="text-[11.5px] text-descriptionforeground">
                {messages.historyActions.selectedCount(orderedSelection.length)}
              </p>
            ) : null}
            <div className="rounded border border-panel-border bg-muted/20 px-2 py-1">
              <p className="mb-1 text-[11.5px] font-semibold">{messages.historyActions.selectedCommits}</p>
              <ul className="max-h-24 overflow-y-auto font-mono text-[11px] text-descriptionforeground">
                {orderedSelection.map((entry) => (
                  <li key={entry.oid} className="truncate" title={entry.subject}>
                    {entry.oid.slice(0, 10)} {entry.subject}
                  </li>
                ))}
              </ul>
            </div>
            {action === "cherryPick" ? (
              <p className="text-[11.5px] text-descriptionforeground">
                {cherryPickKeepEmpty ? messages.historyActions.emptyKeepHint : messages.historyActions.emptySkipHint}
              </p>
            ) : null}
            {loading ? <p className="text-[12px] text-descriptionforeground">{messages.historyActions.checking}</p> : null}
            {dirty ? (
              <div className="space-y-2 rounded border border-[var(--editorwarning-foreground)]/50 p-2">
                <p className="font-semibold text-[12.5px]">{messages.historyActions.dirtyTitle}</p>
                <ul className="max-h-28 overflow-y-auto font-mono text-[11px] text-descriptionforeground">
                  {preflight?.hasStagedChanges ? <li>{messages.historyActions.indexDirty}</li> : null}
                  {preflight?.paths.map((path) => <li key={path} className="truncate" title={path}>{path}</li>)}
                </ul>
                {preflight?.hasStagedChanges ? <p className="text-[11.5px] text-descriptionforeground">{messages.historyActions.indexDirtyHint}</p> : null}
                <label className="flex items-start gap-2 text-[12.5px]">
                  <input type="radio" className="mt-0.5" checked={strategy === "stash"} onChange={() => setStrategy("stash")} />
                  <span>
                    <span className="block">{messages.historyActions.strategyStash}</span>
                    <span className="block text-[11.5px] text-descriptionforeground">{messages.historyActions.strategyStashHint}</span>
                  </span>
                </label>
                <label className="flex items-start gap-2 text-[12.5px]">
                  <input type="radio" className="mt-0.5" checked={strategy === "force"} onChange={() => setStrategy("force")} />
                  <span>
                    <span className="block">{messages.historyActions.strategyForce}</span>
                    <span className="block text-[11.5px] text-descriptionforeground">{messages.historyActions.strategyForceHint}</span>
                  </span>
                </label>
              </div>
            ) : preflight ? (
              <p className="text-[12px] text-descriptionforeground">{messages.historyActions.cleanWorktree}</p>
            ) : null}
          </>
        ) : null}

        {action === "reset" ? (
          <>
            <p className="text-[12.5px] leading-relaxed text-descriptionforeground">{messages.historyActions.resetHint}</p>
            {(["soft", "mixed", "hard", "keep"] as ResetMode[]).map((mode) => (
              <label key={mode} className="flex items-start gap-2 text-[12.5px]">
                <input type="radio" className="mt-0.5" checked={resetMode === mode} onChange={() => setResetMode(mode)} />
                <span>
                  <span className="block font-mono">{mode}</span>
                  <span className="block text-[11.5px] text-descriptionforeground">
                    {mode === "soft" ? messages.historyActions.resetSoft
                      : mode === "mixed" ? messages.historyActions.resetMixed
                        : mode === "hard" ? messages.historyActions.resetHard
                          : messages.historyActions.resetKeep}
                  </span>
                </span>
              </label>
            ))}
            {destructive ? <p className="text-[12px] text-destructive">{messages.historyActions.resetHardWarning}</p> : null}
          </>
        ) : null}

        {action === "reword" ? (
          <label className="block text-[12.5px]">
            <span className="mb-1 block">{messages.historyActions.newMessage}</span>
            <textarea
              className="settings-input min-h-36 w-full resize-y font-mono text-[12px]"
              value={message}
              spellCheck={false}
              onChange={(event) => setMessage(event.target.value)}
            />
          </label>
        ) : null}

        {action === "interactive" ? (
          <div className="space-y-2">
            <p className="text-[12.5px] text-descriptionforeground">{messages.historyActions.interactiveHint}</p>
            <p className="text-[11.5px] text-descriptionforeground">{messages.historyActions.interactiveOldestFirst}</p>
            <div className="max-h-64 overflow-y-auto rounded border border-panel-border">
              {rangeEntries.map((entry, index) => {
                const source = range.find((commit) => commit.oid === entry.oid);
                return (
                  <div key={entry.oid} className="flex flex-wrap items-center gap-1 border-b border-panel-border px-2 py-1 last:border-b-0">
                    <span className="w-5 shrink-0 text-right font-mono text-[10.5px] text-descriptionforeground">{index + 1}</span>
                    <select
                      className="settings-input h-6 text-[11.5px]"
                      value={entry.action}
                      onChange={(event) => {
                        const next = event.target.value as RebaseAction;
                        updateEntry(index, { action: next, message: next === "reword" ? (entry.message ?? source?.subject ?? "") : null });
                      }}
                    >
                      {(["pick", "reword", "edit", "squash", "fixup", "drop"] as RebaseAction[]).map((value) => (
                        <option key={value} value={value}>{rebaseActionText(value)}</option>
                      ))}
                    </select>
                    <span className="min-w-0 flex-1 truncate font-mono text-[11px]" title={source?.subject}>{source?.subject}</span>
                    <span className="font-mono text-[10.5px] text-descriptionforeground">{entry.oid.slice(0, 8)}</span>
                    <button type="button" className="icon-action size-[20px]" title={messages.historyActions.moveUp} disabled={index === 0} onClick={() => move(index, -1)}>
                      <Codicon name="arrow-up" size={12} />
                    </button>
                    <button type="button" className="icon-action size-[20px]" title={messages.historyActions.moveDown} disabled={index === rangeEntries.length - 1} onClick={() => move(index, 1)}>
                      <Codicon name="arrow-down" size={12} />
                    </button>
                    {entry.action === "reword" ? (
                      <input
                        className="settings-input mt-1 w-full font-mono text-[11.5px]"
                        value={entry.message ?? ""}
                        placeholder={messages.historyActions.rewordMessageRequired}
                        onChange={(event) => updateEntry(index, { message: event.target.value })}
                      />
                    ) : null}
                  </div>
                );
              })}
            </div>
          </div>
        ) : null}

        {error ? <p role="alert" className="text-[12px] text-destructive">{"validation" in error ? ({ messageRequired: messages.historyActions.messageRequired, emptyRange: messages.historyActions.emptyRange, rewordMessageRequired: messages.historyActions.rewordMessageRequired, firstCannotSquash: messages.historyActions.firstCannotSquash, rootCommitUnsupported: messages.historyActions.rootCommitUnsupported })[error.validation] : formatAppError(error)}</p> : null}
        <div className="flex justify-end gap-2">
          <button type="button" className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-3 py-1" onClick={onClose}>
            {messages.historyActions.cancel}
          </button>
          <button
            type="button"
            autoFocus
            className={`rounded-[4px] px-3 py-1 disabled:opacity-50 ${destructive ? "bg-destructive text-destructive-foreground" : "bg-[var(--button-background)] text-[var(--button-foreground)]"}`}
            disabled={busy || loading || (action !== "interactive" && action !== "reword" && action !== "reset" && !preflight)}
            onClick={submit}
          >
            {busy ? messages.historyActions.running : actionText(action)}
          </button>
        </div>
      </div>
    </Modal>
  );
}
