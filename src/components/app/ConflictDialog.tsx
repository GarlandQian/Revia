import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
/**
 * 冲突对话框（Q143）：照 Rebased 的 Conflicts 对话框。
 *
 * 左侧是文件列表（你的版本状态、对方的版本状态），右侧三个按钮：
 * 接受你的版本、接受对方的版本、合并…（打开合并页签）。可以多选后一起接受某一侧；
 * 每个文件带列表签发的指纹，写前逐个核对（Q143、merge-design 第 4 节）。
 * 接受一侧由 App 经全局写门禁与关闭/退出保护执行（`runMergeWrite`）。
 */

import { useEffect, useMemo, useState } from "react";
import type { ConflictFile } from "@/types/bindings/ConflictFile";
import { ipc } from "@/lib/ipc";
import type { MergeWriteRequest } from "@/features/merge/mergeWrite";
import { useMountedRef } from "@/hooks/useMountedRef";
import { messages } from "@/lib/i18n/index";
import { conflictCountText, mergeUnavailableText } from "@/lib/i18n/text";
import { formatAppError, toAppError } from "@/lib/error";
import { Codicon } from "@/components/ui/codicon";
import { Modal } from "./Dialogs";
import { FileTypeIcon } from "./FileTypeIcon";

function sideLabel(state: ConflictFile["ours"]): string {
  switch (state) {
    case "modified":
      return messages.merge.modified;
    case "deleted":
      return messages.merge.deleted;
    case "added":
      return messages.merge.added;
  }
}

export function ConflictDialog({
  worktreeId,
  onClose,
  onOpenMerge,
  onWrite,
}: {
  worktreeId: string;
  onClose: () => void;
  onOpenMerge: (path: string) => void;
  onWrite: (request: MergeWriteRequest) => Promise<void>;
}) {
  const [files, setFiles] = useState<ConflictFile[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [busy, setBusy] = useState(false);
  const mounted = useMountedRef();

  useEffect(() => {
    let cancelled = false;
    void ipc
      .listConflicts(worktreeId)
      .then((list) => {
        if (cancelled) return;
        setFiles(list);
        setSelected(new Set(list.map((file) => file.path)));
      })
      .catch((caught: unknown) => {
        if (cancelled) return;
        setError(toAppError(caught));
        setFiles([]);
      });
    return () => {
      cancelled = true;
    };
  }, [worktreeId]);

  const chosen = useMemo(() => (files ?? []).filter((file) => selected.has(file.path)), [files, selected]);

  const accept = async (side: "ours" | "theirs") => {
    if (chosen.length === 0 || busy) return;
    setBusy(true);
    setError(null);
    try {
      await onWrite({
        kind: "acceptSide",
        worktreeId,
        targets: chosen.map((file) => ({ path: file.path, fingerprint: file.fingerprint })),
        side,
      });
      if (mounted.current) onClose();
    } catch (caught) {
      if (mounted.current) setError(toAppError(caught));
    } finally {
      if (mounted.current) setBusy(false);
    }
  };

  return (
    <Modal title={messages.merge.conflictDialogTitle} onClose={onClose} width={720}>
      <p className="text-[12.5px] text-descriptionforeground">{messages.merge.conflictDialogHint}</p>
      {error ? (
        <p role="alert" className="mt-2 rounded border border-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_32%,transparent)] bg-[color-mix(in_srgb,var(--gitdecoration-conflictingresourceforeground)_10%,var(--editor-background))] px-2 py-1 text-[12px]">
          {formatAppError(error)}
        </p>
      ) : null}
      <div className="mt-2 flex min-h-[220px] gap-3">
        <ul className="min-w-0 flex-1 overflow-y-auto rounded border border-panel-border" data-conflict-list>
          {files === null ? <li className="px-2 py-1 text-[12px] text-descriptionforeground">{messages.changes.loading}</li> : null}
          {files?.length === 0 ? <li className="px-2 py-1 text-[12px] text-descriptionforeground">{conflictCountText(0)}</li> : null}
          {(files ?? []).map((file) => (
            <li key={file.path}>
              <button
                type="button"
                className="flex w-full items-center gap-2 px-2 py-1 text-left hover:bg-toolbar-hoverbackground"
                aria-pressed={selected.has(file.path)}
                onClick={() => {
                  setSelected((current) => {
                    const next = new Set(current);
                    if (next.has(file.path)) next.delete(file.path);
                    else next.add(file.path);
                    return next;
                  });
                }}
                onDoubleClick={() => onOpenMerge(file.path)}
              >
                <span className="w-4 shrink-0">
                  {selected.has(file.path) ? <Codicon name="check" size={12} /> : null}
                </span>
                <FileTypeIcon path={file.path} />
                <span className="min-w-0 flex-1 truncate font-mono text-[11.5px]">{file.path}</span>
                <span className="shrink-0 text-[11px] text-descriptionforeground">
                  {messages.merge.ourVersion}: {sideLabel(file.ours)}
                </span>
                <span className="shrink-0 text-[11px] text-descriptionforeground">
                  {messages.merge.theirVersion}: {sideLabel(file.theirs)}
                </span>
                {file.unavailable ? (
                  <span className="shrink-0 text-[11px] text-descriptionforeground" title={mergeUnavailableText(file.unavailable)}>
                    {messages.merge.onlyAcceptSide}
                  </span>
                ) : null}
              </button>
            </li>
          ))}
        </ul>
        <div className="flex w-[150px] shrink-0 flex-col gap-1.5">
          <button
            type="button"
            className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-2 py-1 text-[12px] hover:bg-toolbar-hoverbackground disabled:opacity-50"
            disabled={busy || chosen.length === 0}
            onClick={() => void accept("ours")}
          >
            {messages.merge.acceptYourVersion}
          </button>
          <button
            type="button"
            className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-2 py-1 text-[12px] hover:bg-toolbar-hoverbackground disabled:opacity-50"
            disabled={busy || chosen.length === 0}
            onClick={() => void accept("theirs")}
          >
            {messages.merge.acceptTheirVersion}
          </button>
          <button
            type="button"
            className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-2 py-1 text-[12px] hover:bg-toolbar-hoverbackground disabled:opacity-50"
            disabled={busy || chosen.length === 0}
            onClick={() => {
              const target = chosen[0];
              if (target) onOpenMerge(target.path);
            }}
          >
            {messages.merge.mergeButton}
          </button>
        </div>
      </div>
    </Modal>
  );
}
