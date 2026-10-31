import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { t, messages } from "@/lib/i18n";
import { readonlyReasonText } from "@/lib/i18n/text";
import { isPrimaryClick, isPrimaryModifier, isAltModifier } from "@/lib/platform";
import { Codicon } from "@/components/ui/codicon";
import { FileTypeIcon } from "@/components/app/FileTypeIcon";
import { VisibleChangelistSelection } from "@/lib/m7/visibleChangelistSelection";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { formatAppError, toAppError } from "@/lib/error";
import type { ChangelistView } from "@/types/bindings/ChangelistView";
import type { ChangelistSelection } from "@/types/bindings/ChangelistSelection";

export const selectionOf = (view: ChangelistView): ChangelistSelection => ({ identity: view.identity, revision: view.revision });
type Action = { kind: "create"; name: string } | { kind: "rename"; group: string; name: string }
  | { kind: "delete" | "default"; group: string } | { kind: "move"; group: string; items: string[] }
  | { kind: "select"; items: string[]; selected: boolean };

export interface ChangelistControls { selectAll: () => void; move: (direction: 1 | -1, extend: boolean) => void }

export function Changelists({ worktreeId, revision, busy, onView, snapshot, onActivate, onPin, onDiscard, onSelection, controls }: {
  worktreeId: string; revision: number; busy: boolean; onView: (view: ChangelistView | null) => void;
  snapshot: ChangesSnapshot; controls: RefObject<ChangelistControls | null>;
  onActivate: (entry: ChangeEntry, modifiers: {toggle: boolean; extend: boolean}) => void;
  onPin: (entry: ChangeEntry) => void; onDiscard: (paths: string[]) => void;
  onSelection: (worktreeId: string, paths: string[]) => void;
}) {
  const [view, setView] = useState<ChangelistView | null>(null);
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [pending, setPending] = useState(false);
  const [name, setName] = useState("");
  const selection = useRef(new VisibleChangelistSelection());
  const [, redrawSelection] = useState(0);
  const visibleRows = useMemo(() => view?.groups.flatMap((group) => [...new Set(view.items.filter((item) => item.group === group.id).map((item) => item.path))].map((path) => ({ id: `${group.id}:${path}`, path, entry: snapshot.entries.find((entry) => entry.path === path && entry.changes === "changelist") ?? snapshot.entries.find((entry) => entry.path === path && entry.changes === "untracked") }))) ?? [], [snapshot, view]);
  const selectedPaths = useCallback(() => [...new Set(selection.current.ids().map((id) => visibleRows.find((row) => row.id === id)?.path).filter((path): path is string => !!path))], [visibleRows]);
  const publishSelection = useCallback(() => { redrawSelection((value) => value + 1); onSelection(worktreeId, selectedPaths()); }, [onSelection, selectedPaths, worktreeId]);
  useEffect(() => { selection.current.setRows(visibleRows.map((row) => row.id)); publishSelection(); }, [visibleRows, publishSelection]);
  const activateRow = (id: string, toggle: boolean, extend: boolean) => {
    selection.current.activate(id, {toggle, extend}); publishSelection();
    const entry = visibleRows.find((row) => row.id === id)?.entry;
    if (entry) onActivate(entry, {toggle:false, extend:false});
  };
  useEffect(() => {
    const controller = {
      selectAll: () => { selection.current.selectAll(); publishSelection(); },
      move: (direction: 1 | -1, extend: boolean) => {
        const id = selection.current.move(direction, extend); publishSelection();
        const entry = visibleRows.find((row) => row.id === id)?.entry;
        if (entry) onActivate(entry, {toggle:false, extend:false});
      },
    };
    controls.current = controller;
    return () => { if (controls.current === controller) controls.current = null; };
  }, [controls, onActivate, publishSelection, visibleRows]);
  const epoch = useRef(0);
  useEffect(() => {
    const current = ++epoch.current;
    onView(null);
    if (busy) return;
    setPending(true);
    void invoke<ChangelistView>("m7_changelist_view", { request: { worktreeId } }).then((next) => {
      if (epoch.current !== current) return;
      setView(next); onView(next); setError(null);
    }).catch((caught) => { if (epoch.current === current) setError(toAppError(caught)); })
      .finally(() => { if (epoch.current === current) setPending(false); });
    return () => { epoch.current++; };
  }, [worktreeId, revision, busy, onView]);
  const change = async (action: Action) => {
    if (!view || busy || pending) return;
    const current = ++epoch.current;
    setPending(true); onView(null);
    try {
      const next = await invoke<ChangelistView>("m7_changelist_change", { request: { worktreeId, expected: selectionOf(view), action } });
      if (epoch.current !== current) return;
      setView(next); onView(next); setError(null);
      if (action.kind === "create") setName("");
    } catch (caught) { if (epoch.current === current) setError(toAppError(caught)); }
    finally { if (epoch.current === current) setPending(false); }
  };
  return <div className="min-h-0 flex-1 overflow-auto px-2 py-1 text-[12px]" aria-label={t("changelists.title")} onKeyDown={(event) => {
    const target = event.target as HTMLElement;
    if (["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;
    if (isPrimaryModifier(event) && !isAltModifier(event) && event.code === "KeyA") { event.preventDefault(); controls.current?.selectAll(); }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") { event.preventDefault(); controls.current?.move(event.key === "ArrowDown" ? 1 : -1, event.shiftKey); }
  }}>
    {error && <div role="alert" className="mb-2 whitespace-pre-wrap text-errorforeground">{formatAppError(error)}</div>}
    <form className="mb-2 flex gap-1" onSubmit={(event) => { event.preventDefault(); void change({ kind: "create", name }); }}>
      <input className="settings-input min-w-0 flex-1" value={name} aria-label={t("changelists.name")} placeholder={t("changelists.name")} onChange={(event) => setName(event.target.value)} maxLength={256} disabled={busy || pending} />
      <button className="toolbar-button" type="submit" disabled={busy || pending || !name.trim()}>{t("changelists.create")}</button>
    </form>
    {!view && <p>{t("changes.loading")}</p>}
    {view?.groups.map((group) => {
      const items = view.items.filter((item) => item.group === group.id);
      const active = items.filter((item) => !item.stale && !item.readonlyReason);
      const paths = [...new Set(items.map((item) => item.path))];
      return <section key={group.id} className="mb-2 border-b border-panelborder pb-1">
        <div className="flex items-center gap-1">
          <input type="checkbox" aria-label={group.name || t("changelists.default")} checked={active.length > 0 && active.every((item) => item.selected)} disabled={busy || pending || !active.length} onChange={(event) => void change({ kind: "select", items: active.map((i) => i.id), selected: event.target.checked })} />
          <input key={`${group.id}:${group.name}`} className="min-w-0 flex-1 bg-transparent font-semibold" aria-label={t("changelists.rename")} defaultValue={group.name || t("changelists.default")} maxLength={256} disabled={busy || pending} onBlur={(event) => { if (event.target.value.trim() && event.target.value !== (group.name || t("changelists.default"))) void change({ kind: "rename", group: group.id, name: event.target.value }); }} />
          <button className="toolbar-button" disabled={busy || pending || group.id === view.defaultGroup} onClick={() => void change({ kind: "default", group: group.id })} title={t("changelists.makeDefault")}><Codicon name={group.id === view.defaultGroup ? "star-full" : "star-empty"} size={13} /></button>
          <button className="toolbar-button" disabled={busy || pending || view.groups.length === 1} onClick={() => void change({ kind: "delete", group: group.id })}>{t("changelists.delete")}</button>
        </div>
        {paths.map((path) => {
          const entries = items.filter((item) => item.path === path);
          const usable = entries.filter((item) => !item.stale && !item.readonlyReason);
          // 整文件移动须涵盖其他组内同一路径的块，保持单一归属。
          const allFileItems = view.items.filter((item) => item.path === path);
          const entryFor = () => snapshot.entries.find((entry) => entry.path === path && entry.changes === "changelist") ?? snapshot.entries.find((entry) => entry.path === path && entry.changes === "untracked");
          const rowId = `${group.id}:${path}`;
          const rowSelected = selection.current.ids().includes(rowId);
          const fileEntry = entryFor();
          const readonly = entries.find((item) => item.readonlyReason)?.readonlyReason;
          return <div key={path} className="ml-2 py-1">
            <div className="flex items-center gap-1">
              <input type="checkbox" aria-label={path} checked={usable.length > 0 && usable.every((i) => i.selected)} disabled={busy || pending || !usable.length} onChange={(event) => void change({ kind: "select", items: usable.map((i) => i.id), selected: event.target.checked })} />
              <button className={`flex min-w-0 flex-1 items-center gap-1 text-left ${fileEntry && rowSelected ? "bg-list-activeselectionbackground" : ""}`} title={path} aria-selected={fileEntry ? rowSelected : false}
                onClick={(event) => { if (fileEntry) activateRow(rowId, isPrimaryClick(event), event.shiftKey); }}
                onDoubleClick={() => { if (fileEntry) onPin(fileEntry); }}
                onKeyDown={(event) => { if (event.key === "Enter" && fileEntry) { event.preventDefault(); onPin(fileEntry); } }}>
                <FileTypeIcon path={path} symlink={readonly === "symlink"} /><span className={`min-w-0 truncate status-${fileEntry?.status ?? "modified"}`}>{path}</span>
              </button>
              {fileEntry && (fileEntry.changes === "changelist" || fileEntry.changes === "untracked") && !readonly && <button className="icon-action" disabled={busy || pending} title={messages.changes.discardSelected} onClick={() => onDiscard(rowSelected ? selectedPaths() : [path])}><Codicon name="discard" size={13} /></button>}
              <select className="min-w-0 max-w-24 bg-dropdownbackground" aria-label={t("changelists.moveFile")} value={group.id} disabled={busy || pending} onChange={(event) => void change({ kind: "move", items: allFileItems.map((i) => i.id), group: event.target.value })}>
                {view.groups.map((g) => <option value={g.id} key={g.id}>{g.name || t("changelists.default")}</option>)}
              </select>
            </div>
            {readonly && <div className="ml-4 text-descriptionforeground">{readonlyReasonText(readonly)}</div>}
            {entries.map((item) => {
              const entry = entryFor();
              return <div key={item.id} className="ml-3 mt-1 flex items-center gap-1">
                <input type="checkbox" aria-label={t("changelists.selectBlock")} checked={item.selected} disabled={busy || pending || item.stale || !!item.readonlyReason} onChange={(event) => void change({ kind: "select", items: [item.id], selected: event.target.checked })} />
                <button className="min-w-0 flex-1 truncate text-left" title={item.summary} onClick={(event) => { if (entry) activateRow(rowId,isPrimaryClick(event),event.shiftKey); }} onDoubleClick={() => { if(entry)onPin(entry); }}>
                  {item.stale ? t("changelists.stale") : item.hunk === null ? t("changelists.wholeFile") : item.summary}
                </button>
                <select className="max-w-24 bg-dropdownbackground" aria-label={t("changelists.moveBlock")} value={item.group} disabled={busy || pending} onChange={(event) => void change({ kind: "move", items: [item.id], group: event.target.value })}>
                  {view.groups.map((g) => <option value={g.id} key={g.id}>{g.name || t("changelists.default")}</option>)}
                </select>
              </div>;
            })}
          </div>;
        })}
      </section>;
    })}
  </div>;
}
