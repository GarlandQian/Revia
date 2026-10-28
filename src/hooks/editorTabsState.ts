import type { EditorTab } from "@/components/app/EditorTabs";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";

export interface MergeTabTarget {
  worktreeId: string;
  path: string;
}

export function activeMergeEntry(
  target: MergeTabTarget | null,
  snapshot: Pick<ChangesSnapshot, "worktreeId" | "entries"> | null,
): ChangeEntry | null {
  if (!target || !snapshot || target.worktreeId !== snapshot.worktreeId) return null;
  // 内容变化不自动丢弃草稿；应用时仍用打开合并页签时签发的指纹。
  return snapshot.entries.find((entry) => entry.path === target.path && entry.unmerged) ?? null;
}

export function retainEditorTabs(
  tabs: EditorTab[],
  snapshot: Pick<ChangesSnapshot, "entries">,
): EditorTab[] {
  let changed = false;
  const next: EditorTab[] = [];
  for (const tab of tabs) {
    const entry = snapshot.entries.find((entry) => entry.path === tab.path
      && (tab.kind === "merge" ? entry.unmerged : entry.changes === tab.changes));
    if (!entry) {
      changed = true;
    } else if (entry.changes !== tab.changes) {
      changed = true;
      next.push({ ...tab, changes: entry.changes });
    } else {
      next.push(tab);
    }
  }
  return changed ? next : tabs;
}

export interface ClosedEditorTabs {
  tabs: EditorTab[];
  /** undefined 保持当前编辑器，null 清空当前编辑器。 */
  activate: EditorTab | null | undefined;
}

export function closeEditorTab(
  tabs: EditorTab[],
  activeKey: string | null,
  closingKey: string | null,
): ClosedEditorTabs {
  if (closingKey === null) return { tabs: [], activate: null };
  const index = tabs.findIndex((tab) => tab.key === closingKey);
  if (index < 0) return { tabs, activate: undefined };
  const next = tabs.filter((tab) => tab.key !== closingKey);
  return {
    tabs: next,
    activate: closingKey === activeKey ? next[Math.min(index, next.length - 1)] ?? null : undefined,
  };
}

export function showDiffTab(tabs: EditorTab[], target: EditorTab): EditorTab[] {
  const existing = tabs.find((tab) => tab.key === target.key);
  if (existing) {
    return existing.preview && !target.preview
      ? tabs.map((tab) => tab.key === target.key ? { ...tab, preview: false } : tab)
      : tabs;
  }
  const next = [...tabs];
  if (target.preview) {
    const previewIndex = next.findIndex((tab) => tab.preview);
    if (previewIndex >= 0) next.splice(previewIndex, 1);
  }
  return [...next, target];
}
