/** Git并排工作区的可编辑一侧；内容与保存生命周期由EditSession持有。 */
import { useEffect, useRef, useSyncExternalStore, type ReactNode } from "react";
import { Codicon } from "@/components/ui/codicon";
import { Modal } from "@/components/app/Dialogs";
import { formatAppError } from "@/lib/error";
import { t } from "@/lib/i18n/index";
import { isPrimaryModifier } from "@/lib/platform";
import { codeFontStack } from "@/lib/theme";
import { EditSession } from "@/lib/m7/editSession";

const DRAFT_DEBOUNCE_MS = 700;
function ReadonlyNotice({ session }: { session: EditSession }) {
  const reason = session.snapshot().file?.readonlyReason;
  const label = reason === "symlink" ? t("edit.readonly.symlink")
    : reason === "notRegularFile" ? t("edit.readonly.notRegularFile")
    : reason === "submodule" ? t("edit.readonly.submodule")
    : reason === "binary" ? t("edit.readonly.binary")
    : reason === "tooLarge" ? t("edit.readonly.tooLarge")
    : reason === "encoding" ? t("edit.readonly.encoding")
    : reason === "unmerged" ? t("edit.readonly.unmerged")
    : reason === "missing" ? t("edit.readonly.missing")
    : reason === "notInWorktree" ? t("edit.readonly.notInWorktree") : t("edit.readonly.unknown");
  return <p className="p-3 text-[12px] text-descriptionforeground">{label}</p>;
}
export function EditConflict({ session }: { session: EditSession }) {
  const state = useSyncExternalStore(session.subscribe, session.snapshot);
  if (!state.conflict) return null;
  return <div className="space-y-2 border-b border-panel-border p-3 text-[12px]">
    <p>{t("edit.conflict")}</p>
    <details open>
      <summary className="cursor-pointer font-semibold">{t("edit.baseline")}</summary>
      <pre className="max-h-48 overflow-auto whitespace-pre-wrap bg-[var(--editor-background)] p-2 font-mono">{state.diskText}</pre>
    </details>
    <div className="flex gap-2">
      <button type="button" className="toolbar-widget h-6 px-2" disabled={state.saving || session.blocked} onClick={() => void session.discard()}>{t("edit.conflictReload")}</button>
      <button type="button" className="toolbar-widget h-6 px-2" disabled={state.saving || session.blocked} onClick={() => void session.acceptDiskBaseline()}>{t("edit.conflictKeepMine")}</button>
    </div>
  </div>;
}
function EditError({ session }: { session: EditSession }) {
  const state = useSyncExternalStore(session.subscribe, session.snapshot);
  const error = state.error ?? state.draftError;
  return error ? <p role="alert" className="m-2 rounded border border-panel-border px-3 py-2 text-[12px]">{formatAppError(error)}</p> : null;
}

export function EditDecisionDialog({ session, onDone }: { session: EditSession; onDone: (resolved: boolean) => void }) {
  const state = useSyncExternalStore(session.subscribe, session.snapshot);
  return <Modal title={session.target.path} onClose={() => onDone(false)} width={620}>
    <p className="mb-2">{t("edit.closeConfirm")}</p>
    <EditError session={session} />
    <EditConflict session={session} />
    <details className="my-2" open={state.conflict}>
      <summary className="cursor-pointer">{t("edit.editor")}</summary>
      <pre className="max-h-52 overflow-auto whitespace-pre-wrap bg-[var(--editor-background)] p-2 font-mono text-[12px]">{state.text}</pre>
    </details>
    <div className="mt-3 flex justify-end gap-2">
      <button type="button" className="toolbar-widget h-7 px-3" onClick={() => onDone(false)}>{t("edit.closeCancel")}</button>
      <button type="button" className="toolbar-widget h-7 px-3" disabled={state.saving} onClick={() => void session.discard().then((ok) => { if (ok) onDone(true); })}>{t("edit.closeDiscard")}</button>
      <button type="button" className="toolbar-widget h-7 px-3" disabled={session.blocked || state.loading || state.saving || state.conflict || !state.identityValid || state.needsVerification} onClick={() => void session.save().then((ok) => { if (ok) onDone(true); })}>
        {state.saving ? t("edit.saving") : t("edit.closeSave")}
      </button>
    </div>
  </Modal>;
}

export interface EditableWorkspaceProps {
  session: EditSession;
  fontSize: number;
  fontFamily: string | null;
  blocked: boolean;
  onClose: () => void;
  renderDiff: (editor: ReactNode) => ReactNode;
}
export function EditableWorkspace({ session, fontSize, fontFamily, blocked, onClose, renderDiff }: EditableWorkspaceProps) {
  const state = useSyncExternalStore(session.subscribe, session.snapshot);
  const gutter = useRef<HTMLDivElement>(null);
  useEffect(() => {
    void session.open();
    return () => { void session.flushDraft().catch(() => { /* 原目标会话及App已收到结构化失败。 */ }); };
  }, [session]);
  useEffect(() => {
    if (!session.dirty || state.saving || state.loading) return;
    const timer = window.setTimeout(() => void session.flushDraft().catch(() => { /* 会话保留失败与内容。 */ }), DRAFT_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [session, state.text, state.saving, state.loading]);
  const editable = state.file?.text != null && state.file.readonlyReason === null && state.identityValid;
  const lineCount = Math.min(20_000, state.text.split("\n").length);
  const editor = <div className="relative min-h-0 min-w-0 flex-1 overflow-hidden bg-[var(--editor-background)]">
    <div ref={gutter} aria-hidden className="absolute inset-y-0 left-0 w-12 select-none overflow-hidden pr-2 text-right text-descriptionforeground" style={{ fontFamily: codeFontStack(fontFamily), fontSize, lineHeight: 1.45 }}>
      <div className="py-1">{Array.from({ length: lineCount }, (_, line) => <div key={line}>{line + 1}</div>)}</div>
    </div>
    <textarea className="absolute inset-y-0 left-12 right-0 w-[calc(100%-3rem)] resize-none overflow-auto bg-transparent py-1 pl-2 pr-3 outline-none"
      aria-label={t("edit.editor")} style={{ fontFamily: codeFontStack(fontFamily), fontSize, lineHeight: 1.45, tabSize: 4 }}
      value={state.text} disabled={blocked || session.blocked || state.saving || state.loading} spellCheck={false} wrap="off"
      onChange={(event) => session.change(event.target.value)}
      onScroll={(event) => { if (gutter.current) gutter.current.scrollTop = event.currentTarget.scrollTop; }}
      onKeyDown={(event) => {
        if (isPrimaryModifier(event) && event.code === "KeyS") { event.preventDefault(); event.stopPropagation(); void session.save(); return; }
        if (event.key === "Tab" && !event.metaKey && !event.ctrlKey) {
          event.preventDefault(); const input = event.currentTarget; const start = input.selectionStart; const end = input.selectionEnd;
          session.change(`${state.text.slice(0, start)}  ${state.text.slice(end)}`);
          requestAnimationFrame(() => { input.selectionStart = start + 2; input.selectionEnd = start + 2; });
        }
      }} />
  </div>;
  return <div className="flex h-full min-h-0 flex-col bg-[var(--editor-background)]">
    <div className="flex h-8 shrink-0 items-center gap-2 border-b border-panel-border px-3 text-[12px]">
      <Codicon name="edit" size={14} /><span className="truncate font-semibold">{session.target.path}</span>
      <span className="ml-auto shrink-0 text-descriptionforeground">{session.dirty ? t("edit.unsaved") : t("edit.saved")}</span>
      <button type="button" className="toolbar-widget h-6 px-2" disabled={!editable || blocked || state.saving || !session.dirty || state.conflict || state.needsVerification} onClick={() => void session.save()}>{state.saving ? t("edit.saving") : t("edit.save")}</button>
      <button type="button" className="icon-action size-[22px]" aria-label={t("edit.close")} onClick={onClose}><Codicon name="close" size={14} /></button>
    </div>
    <p className="border-b border-panel-border px-3 py-1 text-[11px] text-descriptionforeground">{t("edit.diffDiskVersion")}</p>
    <EditError session={session} /><EditConflict session={session} />
    {state.loading ? <p className="p-3 text-[12px]">{t("edit.loading")}</p> : !editable ? <ReadonlyNotice session={session} /> : renderDiff(editor)}
  </div>;
}
