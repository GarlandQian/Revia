import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { t } from "@/lib/i18n";
import { formatAppError, toAppError } from "@/lib/error";
import type { ChangelistView } from "@/types/bindings/ChangelistView";
import type { ChangelistSelection } from "@/types/bindings/ChangelistSelection";

export const selectionOf = (view: ChangelistView): ChangelistSelection => ({ identity: view.identity, revision: view.revision });
type Action = { kind: "create"; name: string } | { kind: "rename"; group: string; name: string }
  | { kind: "delete" | "default"; group: string } | { kind: "move"; group: string; items: string[] }
  | { kind: "select"; items: string[]; selected: boolean };

export function Changelists({ worktreeId, revision, busy, onView }: {
  worktreeId: string; revision: number; busy: boolean; onView: (view: ChangelistView | null) => void;
}) {
  const [view, setView] = useState<ChangelistView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [name, setName] = useState("");
  const epoch = useRef(0);
  useEffect(() => {
    const current = ++epoch.current;
    onView(null);
    if (busy) return;
    setPending(true);
    void invoke<ChangelistView>("m7_changelist_view", { request: { worktreeId } }).then((next) => {
      if (epoch.current !== current) return;
      setView(next); onView(next); setError(null);
    }).catch((caught) => { if (epoch.current === current) setError(formatAppError(toAppError(caught))); })
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
    } catch (caught) { if (epoch.current === current) setError(formatAppError(toAppError(caught))); }
    finally { if (epoch.current === current) setPending(false); }
  };
  return <div className="min-h-0 flex-1 overflow-auto px-2 py-1 text-[12px]" aria-label={t("changelists.title")}>
    {error && <div role="alert" className="mb-2 whitespace-pre-wrap text-errorforeground">{error}</div>}
    <form className="mb-2 flex gap-1" onSubmit={(event) => { event.preventDefault(); void change({ kind: "create", name }); }}>
      <input className="settings-input min-w-0 flex-1" value={name} aria-label={t("changelists.name")} placeholder={t("changelists.name")} onChange={(event) => setName(event.target.value)} maxLength={256} disabled={busy || pending} />
      <button className="toolbar-button" type="submit" disabled={busy || pending || !name.trim()}>{t("changelists.create")}</button>
    </form>
    {!view && <p>{t("changes.loading")}</p>}
    {view?.groups.map((group) => {
      const items = view.items.filter((item) => item.group === group.id);
      const active = items.filter((item) => !item.stale);
      const paths = [...new Set(items.map((item) => item.path))];
      return <section key={group.id} className="mb-2 border-b border-panelborder pb-1">
        <div className="flex items-center gap-1">
          <input type="checkbox" aria-label={group.name || t("changelists.default")} checked={active.length > 0 && active.every((item) => item.selected)} disabled={busy || pending || !active.length} onChange={(event) => void change({ kind: "select", items: active.map((i) => i.id), selected: event.target.checked })} />
          <input key={`${group.id}:${group.name}`} className="min-w-0 flex-1 bg-transparent font-semibold" aria-label={t("changelists.rename")} defaultValue={group.name || t("changelists.default")} maxLength={256} disabled={busy || pending} onBlur={(event) => { if (event.target.value.trim() && event.target.value !== (group.name || t("changelists.default"))) void change({ kind: "rename", group: group.id, name: event.target.value }); }} />
          <button className="toolbar-button" disabled={busy || pending || group.id === view.defaultGroup} onClick={() => void change({ kind: "default", group: group.id })} title={t("changelists.makeDefault")}>{group.id === view.defaultGroup ? "★" : "☆"}</button>
          <button className="toolbar-button" disabled={busy || pending || view.groups.length === 1} onClick={() => void change({ kind: "delete", group: group.id })}>{t("changelists.delete")}</button>
        </div>
        {paths.map((path) => {
          const entries = items.filter((item) => item.path === path);
          const usable = entries.filter((item) => !item.stale);
          // 整文件移动须涵盖其他组内同一路径的块，保持单一归属。
          const allFileItems = view.items.filter((item) => item.path === path);
          return <div key={path} className="ml-2 py-1">
            <div className="flex items-center gap-1">
              <input type="checkbox" aria-label={path} checked={usable.length > 0 && usable.every((i) => i.selected)} disabled={busy || pending || !usable.length} onChange={(event) => void change({ kind: "select", items: usable.map((i) => i.id), selected: event.target.checked })} />
              <span className="min-w-0 flex-1 truncate" title={path}>{path}</span>
              <select className="min-w-0 max-w-24 bg-dropdownbackground" aria-label={t("changelists.moveFile")} value={group.id} disabled={busy || pending} onChange={(event) => void change({ kind: "move", items: allFileItems.map((i) => i.id), group: event.target.value })}>
                {view.groups.map((g) => <option value={g.id} key={g.id}>{g.name || t("changelists.default")}</option>)}
              </select>
            </div>
            {entries.map((item) => <details key={item.id} className="ml-3 mt-1">
              <summary className="flex cursor-pointer items-center gap-1">
                <input type="checkbox" aria-label={t("changelists.selectBlock")} checked={item.selected} disabled={busy || pending || item.stale} onClick={(event) => event.stopPropagation()} onChange={(event) => void change({ kind: "select", items: [item.id], selected: event.target.checked })} />
                <span className="min-w-0 flex-1 truncate">{item.stale ? t("changelists.stale") : item.hunk === null ? t("changelists.wholeFile") : item.preview.split("\n")[0]}</span>
                <select className="max-w-24 bg-dropdownbackground" aria-label={t("changelists.moveBlock")} value={item.group} disabled={busy || pending} onClick={(event) => event.stopPropagation()} onChange={(event) => void change({ kind: "move", items: [item.id], group: event.target.value })}>
                  {view.groups.map((g) => <option value={g.id} key={g.id}>{g.name || t("changelists.default")}</option>)}
                </select>
              </summary>
              <pre className="max-h-72 overflow-auto bg-editorbackground p-2 font-mono text-editorforeground">{item.preview}</pre>
            </details>)}
          </div>;
        })}
      </section>;
    })}
  </div>;
}
