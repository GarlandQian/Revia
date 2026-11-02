import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";
import type { ChangelistView } from "@/types/bindings/ChangelistView";
/** 文件行身份是 group:path，范围和箭头只按真实可见顺序计算。 */
export class VisibleChangelistSelection {
  private rows: string[] = [];
  private selected = new Set<string>();
  private anchor: string | null = null;
  private focus: string | null = null;
  setRows(rows: string[]) {
    this.rows = [...rows];
    this.selected = new Set([...this.selected].filter((id) => rows.includes(id)));
    if (this.anchor && !rows.includes(this.anchor)) this.anchor = null;
    if (this.focus && !rows.includes(this.focus)) this.focus = null;
  }
  ids() { return this.rows.filter((id) => this.selected.has(id)); }
  activate(id: string, { toggle = false, extend = false }: { toggle?: boolean; extend?: boolean } = {}) {
    const to = this.rows.indexOf(id);
    if (to < 0) return;
    const from = this.anchor ? this.rows.indexOf(this.anchor) : -1;
    if (extend && from >= 0) this.selected = new Set(this.rows.slice(Math.min(from, to), Math.max(from, to) + 1));
    else if (toggle) { this.selected.has(id) ? this.selected.delete(id) : this.selected.add(id); this.anchor = id; }
    else { this.selected = new Set([id]); this.anchor = id; }
    this.focus = id;
  }
  move(direction: 1 | -1, extend: boolean) {
    if (!this.rows.length) return null;
    const current = this.focus ? this.rows.indexOf(this.focus) : -1;
    const next = this.rows[Math.max(0, Math.min(this.rows.length - 1, current < 0 ? 0 : current + direction))];
    this.activate(next, { extend }); return next;
  }
  selectAll() { this.selected = new Set(this.rows); this.anchor ??= this.rows[0] ?? null; this.focus ??= this.anchor; }
}

/** 主diff导航按可见列表顺序去重文件；同文件跨组仍是同一份HEAD→工作区预览。 */
export function visibleChangelistPaths(groups: readonly {id: string}[], items: readonly {group: string; path: string}[]): string[] {
  return [...new Set(groups.flatMap((group) => items.filter((item) => item.group === group.id).map((item) => item.path)))];
}

/** 所有更改列表消费入口共用严格读源；任何等待/刷新都不能回退到index基线。 */
export function changelistEntryForPath(snapshot: Pick<ChangesSnapshot, "entries">, path: string): ChangeEntry | undefined {
  return snapshot.entries.find((entry) => entry.path === path && entry.changes === "changelist") ?? snapshot.entries.find((entry) => entry.path === path && entry.changes === "untracked");
}
export function changelistEntriesForView(snapshot: Pick<ChangesSnapshot, "worktreeId" | "entries">, view: Pick<ChangelistView, "worktreeId" | "groups" | "items"> | null): ChangeEntry[] {
  const available = snapshot.entries.filter((entry) => entry.changes === "changelist" || entry.changes === "untracked");
  const paths = view?.worktreeId === snapshot.worktreeId ? visibleChangelistPaths(view.groups, view.items) : [];
  const uniquePaths = [...new Set([...paths, ...available.map((entry) => entry.path)])];
  return uniquePaths.flatMap((path) => { const entry = changelistEntryForPath(snapshot, path); return entry ? [entry] : []; });
}
