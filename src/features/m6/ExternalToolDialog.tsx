import type { NoticeText } from "@/components/app/Notifications";
/**
 * 外部 diff / merge 工具启动确认（M6 T01–T03）。
 *
 * 工具名只来自 `git difftool --tool-help` / `git mergetool --tool-help` 的
 * 解析结果，用户可以改用设置里的自定义路径。此时不经过 shell。
 */
import { useEffect, useState } from "react";
import type { ExternalToolChoices } from "@/types/bindings/ExternalToolChoices";
import type { ExternalToolKind } from "@/types/bindings/ExternalToolKind";
import type { ExternalToolOutcome } from "@/types/bindings/ExternalToolOutcome";
import type { ChangesKind } from "@/types/bindings/ChangesKind";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { RemoteTarget } from "@/features/branches/remoteContinuation";
import { ipc } from "@/lib/ipc";
import { formatAppError, toAppError } from "@/lib/error";
import { messages } from "@/lib/i18n/index";
import { Modal } from "@/components/app/Dialogs";

export interface ExternalToolTarget {
  /** 打开对话框时冻结的目标；延迟启动不得落到切换后的 worktree。 */
  origin: RemoteTarget;
  kind: ExternalToolKind;
  path: string;
  revision: string | null;
  changesKind: ChangesKind | null;
  expectedFingerprint?: string | null;
}

export function ExternalToolDialog({
  target,
  busy,
  onClose,
  onLaunch,
  onNotify,
}: {
  target: ExternalToolTarget;
  busy: boolean;
  onClose: () => void;
  onLaunch: (run: () => Promise<ExternalToolOutcome>, tool: string) => Promise<void>;
  onNotify: (notice: { tone: "success" | "warning" | "error"; title: NoticeText; detail?: NoticeText }) => void;
}) {
  const worktreeId = target.origin.worktreeId;
  const [choices, setChoices] = useState<ExternalToolChoices | null>(null);
  const [tool, setTool] = useState("");
  const [context, setContext] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<AppErrorPayload | null>(null);

  useEffect(() => {
    let cancelled = false;
    void ipc.externalToolChoices(worktreeId)
      .then((setup) => {
        const result = setup.choices;
        if (cancelled) return;
        setChoices(result);
        setContext(setup.context);
        const preferred = target.kind === "diff" ? result.configuredDiff : result.configuredMerge;
        const available = target.kind === "diff" ? result.diff : result.merge;
        setTool(preferred ?? available[0] ?? "");
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
  }, [target.kind, worktreeId]);

  const configured = choices ? (target.kind === "diff" ? choices.configuredDiff : choices.configuredMerge) : null;
  const available = choices
    ? [...new Set([...(target.kind === "diff" ? choices.diff : choices.merge), ...(configured ? [configured] : [])])]
    : [];

  return (
    <Modal
      title={target.kind === "diff" ? messages.externalTools.diffTitle : messages.externalTools.mergeTitle}
      onClose={onClose}
      width={520}
    >
      <p className="truncate font-mono text-[12px]">{target.path}</p>
      {target.revision ? <p className="mt-0.5 font-mono text-[11px] text-descriptionforeground">{target.revision.slice(0, 12)}</p> : null}
      {loading ? (
        <p className="mt-3 text-[12px] text-descriptionforeground">{messages.externalTools.loading}</p>
      ) : available.length === 0 ? (
        <p className="mt-3 text-[12px] text-descriptionforeground">{messages.externalTools.none}</p>
      ) : (
        <label className="mt-3 block text-[12.5px]">
          <span className="mb-1 block">{messages.externalTools.tool}</span>
          <select className="settings-input w-full" value={tool} onChange={(event) => setTool(event.target.value)}>
            {available.map((name) => <option key={name} value={name}>{name}</option>)}
          </select>
        </label>
      )}
      <p className="mt-3 text-[11.5px] leading-relaxed text-descriptionforeground">
        {target.kind === "diff" ? messages.externalTools.diffHint : messages.externalTools.mergeHint}
      </p>
      {target.kind === "merge" ? (
        <p className="mt-1 text-[11.5px] text-[var(--editorwarning-foreground)]">{messages.externalTools.mergeRecovery}</p>
      ) : null}
      {error ? <p role="alert" className="mt-2 text-[12px] text-destructive">{formatAppError(error)}</p> : null}
      <div className="mt-4 flex justify-end gap-2">
        <button type="button" className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-3 py-1" onClick={onClose}>
          {messages.externalTools.cancel}
        </button>
        <button
          type="button"
          autoFocus
          className="rounded-[4px] bg-[var(--button-background)] px-3 py-1 text-[var(--button-foreground)] disabled:opacity-50"
          disabled={busy || loading || !tool || !context}
          onClick={() => {
            if (!context) return;
            setError(null);
            void onLaunch(async () => await ipc.externalToolLaunch(worktreeId, {
              expectedContext: context,
              kind: target.kind,
              path: target.path,
              tool,
              revision: target.revision,
              changesKind: target.changesKind,
              expectedFingerprint: target.expectedFingerprint,
            }), tool).catch((caught) => {
              const failure = toAppError(caught);
              setError(failure);
              onNotify({
                tone: failure.kind === "needsVerification" ? "warning" : "error",
                title: () => (formatAppError(failure)),
              });
            });
          }}
        >
          {busy ? messages.externalTools.launching : messages.externalTools.launch}
        </button>
      </div>
    </Modal>
  );
}
