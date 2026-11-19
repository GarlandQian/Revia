import { useCallback, useMemo, useState } from "react";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";
import {
  allGroupsOpen, entryKey, initialSelection, inlineTargets, nextTreeRow, retainSelection,
  selectTreeRow, selectedDirectoryCounts, treeLayout, treeOrder,
  type TreeGroupKey, type TreeGroupOpen, type TreeModifiers, type TreeSelection,
} from "./changesTreeState";

const emptyKeys = new Set<string>();

export function useTreeSelection(
  snapshot: ChangesSnapshot | null,
  groupByDirectory: boolean,
  openEntry: (entry: ChangeEntry) => void,
) {
  const [selection, setSelection] = useState<TreeSelection | null>(null);
  const [groupOpen, setGroupOpen] = useState<TreeGroupOpen>({ ...allGroupsOpen, untracked: false });
  const [dirOpen, setDirOpen] = useState<Set<string>>(new Set());
  const { items, rows } = useMemo(
    () => treeLayout(snapshot?.entries ?? [], groupOpen, groupByDirectory, dirOpen),
    [snapshot?.entries, groupOpen, groupByDirectory, dirOpen],
  );
  const keys = selection && selection.worktreeId === snapshot?.worktreeId ? selection.keys : emptyKeys;
  const allEntries = useMemo(() => snapshot ? treeOrder(snapshot) : [], [snapshot]);
  const selectedEntries = useMemo(() => allEntries.filter((entry) => keys.has(entryKey(entry))), [allEntries, keys]);
  const directoryCounts = useMemo(() => selectedDirectoryCounts(groupByDirectory ? selectedEntries : []), [groupByDirectory, selectedEntries]);

  // 只有打开仓库或首次出现改动时初始化；刷新只能淘汰已消失的条目。
  const receiveTreeSnapshot = useCallback((next: ChangesSnapshot | null, first: ChangeEntry | null, opened = false) => {
    setSelection((current) => {
      if (!next) return null;
      if (!opened && current?.worktreeId === next.worktreeId) return retainSelection(current, next.entries);
      return first ? initialSelection(next.worktreeId, first) : null;
    });
  }, []);

  const activateTreeEntry = useCallback((entry: ChangeEntry, modifiers: TreeModifiers) => {
    if (!snapshot) return;
    setSelection((current) => selectTreeRow(
      current?.worktreeId === snapshot.worktreeId ? current : initialSelection(snapshot.worktreeId, null),
      entry, modifiers, rows,
    ));
    openEntry(entry);
  }, [snapshot, rows, openEntry]);

  const moveTreeSelection = (direction: 1 | -1, extend: boolean) => {
    const next = nextTreeRow(rows, selection?.focus ?? null, direction);
    if (next) activateTreeEntry(next, { toggle: false, extend });
  };

  const selectAllEntries = () => {
    if (!snapshot) return;
    setSelection((current) => ({
      ...(current?.worktreeId === snapshot.worktreeId ? current : initialSelection(snapshot.worktreeId, allEntries[0] ?? null)),
      keys: new Set(allEntries.map(entryKey)),
    }));
    setGroupOpen(allGroupsOpen);
  };

  const toggleGroup = (kind: TreeGroupKey) => {
    setGroupOpen((current) => ({ ...current, [kind]: !current[kind] }));
  };

  const toggleDir = (dir: string) => {
    setDirOpen((current) => {
      const next = new Set(current);
      if (next.has(dir)) next.delete(dir);
      else next.add(dir);
      return next;
    });
  };

  const expandToSelection = useCallback((entry: ChangeEntry) => inlineTargets(entry, selectedEntries), [selectedEntries]);

  return {
    items, keys, directoryCounts, selectedEntries, receiveTreeSnapshot,
    groupOpen, setGroupOpen, dirOpen, expandToSelection, activateTreeEntry, moveTreeSelection,
    selectAllEntries, toggleGroup, toggleDir,
  };
}
