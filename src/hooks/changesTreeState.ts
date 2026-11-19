import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";
import type { ChangesKind } from "@/types/bindings/ChangesKind";

export type TreeGroupKey = Exclude<ChangesKind, "changelist"> | "conflicts";
export type TreeGroupOpen = Record<TreeGroupKey, boolean>;
export const allGroupsOpen: TreeGroupOpen = { conflicts: true, staged: true, unstaged: true, untracked: true };
const groups = ["conflicts", "staged", "unstaged", "untracked"] as const;

export function entryKey(entry: Pick<ChangeEntry, "path" | "changes">): string {
  return `${entry.changes}:${entry.path}`;
}

function groupOf(entry: ChangeEntry) {
  return entry.unmerged && entry.changes !== "changelist" ? "conflicts" : entry.changes;
}

export function treeOrder(snapshot: ChangesSnapshot): ChangeEntry[] {
  return groups.flatMap((group) => snapshot.entries.filter((entry) => groupOf(entry) === group));
}

export type TreeItem =
  | { type: "header"; kind: TreeGroupKey; entries: ChangeEntry[] }
  | { type: "dir"; kind: TreeGroupKey; dir: string }
  | { type: "row"; entry: ChangeEntry };

interface Directory { children: Map<string, Directory>; rows: ChangeEntry[] }

function appendDirectories(items: TreeItem[], entries: ChangeEntry[], kind: TreeGroupKey, dirOpen: Set<string>) {
  const root: Directory = { children: new Map(), rows: [] };
  for (const entry of entries) {
    const parts = entry.path.split("/");
    let node = root;
    for (const part of parts.slice(0, -1)) {
      let child = node.children.get(part);
      if (!child) {
        child = { children: new Map(), rows: [] };
        node.children.set(part, child);
      }
      node = child;
    }
    node.rows.push(entry);
  }

  const walk = (node: Directory, prefix: string) => {
    const dirs = [...node.children].sort(([a], [b]) => a.localeCompare(b));
    for (const [name, child] of dirs) {
      const dir = prefix ? `${prefix}/${name}` : name;
      items.push({ type: "dir", kind, dir });
      if (dirOpen.has(dir)) walk(child, dir);
    }
    for (const entry of node.rows) items.push({ type: "row", entry });
  };
  walk(root, "");
}

/** 渲染和 Shift 使用同一份展开结果；虚拟滚动只截取渲染窗口。 */
export function treeLayout(entries: ChangeEntry[], open: TreeGroupOpen, byDirectory: boolean, dirOpen: Set<string>) {
  const items: TreeItem[] = [];
  for (const kind of groups) {
    const members = entries.filter((entry) => groupOf(entry) === kind);
    if (members.length === 0) continue;
    items.push({ type: "header", kind, entries: members });
    if (!open[kind]) continue;
    if (byDirectory) appendDirectories(items, members, kind, dirOpen);
    else for (const entry of members) items.push({ type: "row", entry });
  }
  return { items, rows: items.flatMap((item) => item.type === "row" ? [item.entry] : []) };
}

export function selectedDirectoryCounts(entries: ChangeEntry[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const entry of entries) {
    let slash = entry.path.lastIndexOf("/");
    while (slash > 0) {
      const key = `${groupOf(entry)}:${entry.path.slice(0, slash)}`;
      counts.set(key, (counts.get(key) ?? 0) + 1);
      slash = entry.path.lastIndexOf("/", slash - 1);
    }
  }
  return counts;
}

export interface TreeSelection {
  worktreeId: string;
  keys: Set<string>;
  anchor: string | null;
  focus: string | null;
}

export function initialSelection(worktreeId: string, entry: ChangeEntry | null): TreeSelection {
  const key = entry && entryKey(entry);
  return { worktreeId, keys: new Set(key ? [key] : []), focus: key, anchor: key };
}

export function retainSelection(current: TreeSelection, entries: ChangeEntry[]): TreeSelection {
  const available = new Set(entries.map(entryKey));
  const keys = new Set([...current.keys].filter((key) => available.has(key)));
  const anchor = current.anchor && available.has(current.anchor) ? current.anchor : null;
  const focus = current.focus && available.has(current.focus) ? current.focus : null;
  if (keys.size === current.keys.size && anchor === current.anchor && focus === current.focus) return current;
  return { ...current, keys, anchor, focus };
}

export type TreeModifiers = { toggle: boolean; extend: boolean };

export function selectTreeRow(current: TreeSelection, entry: ChangeEntry, modifiers: TreeModifiers, rows: ChangeEntry[]): TreeSelection {
  const key = entryKey(entry);
  if (modifiers.extend) {
    const from = rows.findIndex((row) => entryKey(row) === current.anchor);
    const to = rows.findIndex((row) => entryKey(row) === key);
    if (from >= 0 && to >= 0) {
      const range = rows.slice(Math.min(from, to), Math.max(from, to) + 1);
      return { ...current, keys: new Set(range.map(entryKey)), focus: key };
    }
  } else if (modifiers.toggle) {
    const keys = new Set(current.keys);
    if (keys.has(key)) keys.delete(key);
    else keys.add(key);
    return { ...current, keys, anchor: key, focus: key };
  }
  return initialSelection(current.worktreeId, entry);
}

export function nextTreeRow(rows: ChangeEntry[], focus: string | null, direction: 1 | -1) {
  const index = rows.findIndex((entry) => entryKey(entry) === focus);
  return rows[Math.max(0, Math.min(rows.length - 1, index + direction))];
}

export function inlineTargets(entry: ChangeEntry, selection: ChangeEntry[]) {
  if (!selection.some((target) => entryKey(target) === entryKey(entry))) return [entry];
  return selection.filter((target) => target.changes === entry.changes);
}

/** 快捷键与树中的空格共用写入目标。混合组串行通过已有的整文件写门。 */
export async function runFileSelectionAction(
  action: "stageFile" | "discardFile",
  entries: ChangeEntry[],
  write: (kind: "stage" | "unstage", entries: ChangeEntry[]) => Promise<void>,
  confirmDiscard: (entries: ChangeEntry[]) => void,
  isCurrent: () => boolean,
) {
  if (!entries.length || !isCurrent()) return;
  if (action === "discardFile") {
    confirmDiscard(entries);
    return;
  }
  const unstaged = entries.filter((entry) => entry.changes !== "staged");
  const staged = entries.filter((entry) => entry.changes === "staged");
  if (unstaged.length) await write("stage", unstaged);
  if (staged.length && isCurrent()) await write("unstage", staged);
}
