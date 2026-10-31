import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { entryKey, treeOrder, type TreeGroupKey, type TreeGroupOpen } from "@/components/app/ChangesTree";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";

interface TreeSelectionOptions {
  snapshot: ChangesSnapshot | null;
  snapshotRef: RefObject<ChangesSnapshot | null>;
  selectedRef: RefObject<Pick<ChangeEntry, "path" | "changes"> | null>;
  openEntry: (entry: ChangeEntry) => void;
  order?: (snapshot: ChangesSnapshot) => ChangeEntry[];
}

export function useTreeSelection({ snapshot, snapshotRef, selectedRef, openEntry, order }: TreeSelectionOptions) {
  const orderOf = useCallback((current: ChangesSnapshot) => order ? order(current) : treeOrder(current), [order]);
  const [treeSelection, setTreeSelection] = useState<Set<string>>(new Set());
  const treeSelectionRef = useRef(treeSelection);
  treeSelectionRef.current = treeSelection;
  const [selectionAnchor, setSelectionAnchor] = useState<string | null>(null);
  const selectionAnchorRef = useRef(selectionAnchor);
  selectionAnchorRef.current = selectionAnchor;
  const [groupOpen, setGroupOpen] = useState<TreeGroupOpen>({
    conflicts: true, staged: true, unstaged: true, untracked: false,
  });
  const [dirOpen, setDirOpen] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (!snapshot) return;
    setTreeSelection((current) => {
      const next = new Set(
        [...current].filter((key) => snapshot.entries.some((entry) => entryKey(entry) === key)),
      );
      return next.size === current.size ? current : next;
    });
  }, [orderOf, snapshot]);

  const followActiveEntry = useCallback((entry: ChangeEntry, replace: boolean) => {
    const key = entryKey(entry);
    if (replace) {
      setTreeSelection(new Set([key]));
      setSelectionAnchor(key);
    } else {
      // 页签、刷新与导航跟随当前文件，原有多选仍保留。
      setTreeSelection((current) => (current.has(key) ? current : new Set([key])));
      setSelectionAnchor((current) => current ?? key);
    }
  }, [orderOf, ]);

  const expandToSelection = useCallback((entry: ChangeEntry): ChangeEntry[] => {
    const selection = treeSelectionRef.current;
    const current = snapshotRef.current;
    if (selection.size <= 1 || !selection.has(entryKey(entry)) || !current) return [entry];
    return orderOf(current).filter(
      (item) => selection.has(entryKey(item)) && item.changes === entry.changes,
    );
  }, [orderOf, snapshotRef]);

  const treeSelectedEntries = useCallback((): ChangeEntry[] => {
    const current = snapshotRef.current;
    if (!current || treeSelectionRef.current.size === 0) return [];
    return orderOf(current).filter((entry) => treeSelectionRef.current.has(entryKey(entry)));
  }, [orderOf, snapshotRef]);

  const activateTreeEntry = useCallback((entry: ChangeEntry, modifiers: { toggle: boolean; extend: boolean }) => {
    const key = entryKey(entry);
    setTreeSelection((current) => {
      if (modifiers.extend) {
        const snapshotNow = snapshotRef.current;
        if (!snapshotNow) return new Set([key]);
        const order = orderOf(snapshotNow);
        const anchor = selectionAnchorRef.current ?? key;
        const from = order.findIndex((candidate) => entryKey(candidate) === anchor);
        const to = order.findIndex((candidate) => entryKey(candidate) === key);
        if (from < 0 || to < 0) return new Set([key]);
        const [start, end] = from <= to ? [from, to] : [to, from];
        return new Set(order.slice(start, end + 1).map(entryKey));
      }
      if (modifiers.toggle) {
        const next = new Set(current);
        if (next.has(key)) next.delete(key);
        else next.add(key);
        return next;
      }
      return new Set([key]);
    });
    if (!modifiers.extend) setSelectionAnchor(key);
    else setSelectionAnchor((previous) => previous ?? key);
    openEntry(entry);
  }, [orderOf, openEntry, snapshotRef]);

  const moveTreeSelection = useCallback((direction: 1 | -1, extend: boolean) => {
    const snapshotNow = snapshotRef.current;
    if (!snapshotNow) return;
    const order = orderOf(snapshotNow);
    if (order.length === 0) return;
    const currentKey = selectedRef.current ? entryKey(selectedRef.current) : null;
    const index = currentKey ? order.findIndex((entry) => entryKey(entry) === currentKey) : -1;
    const nextIndex = Math.min(order.length - 1, Math.max(0, index + direction));
    const next = order[nextIndex];
    if (!next) return;
    const key = entryKey(next);
    const group = next.changes === "changelist" ? "unstaged" : next.changes;
    setGroupOpen((current) => (current[group] ? current : { ...current, [group]: true }));
    if (extend) {
      const anchor = selectionAnchorRef.current ?? currentKey ?? key;
      const from = order.findIndex((entry) => entryKey(entry) === anchor);
      const to = nextIndex;
      const [start, end] = from <= to ? [from, to] : [to, from];
      setTreeSelection(new Set(order.slice(start, end + 1).map(entryKey)));
      setSelectionAnchor(anchor);
    } else {
      setTreeSelection(new Set([key]));
      setSelectionAnchor(key);
    }
    openEntry(next);
  }, [orderOf, openEntry, selectedRef, snapshotRef]);

  const selectAllEntries = useCallback(() => {
    const snapshotNow = snapshotRef.current;
    if (!snapshotNow) return;
    setTreeSelection(new Set(orderOf(snapshotNow).map(entryKey)));
    setGroupOpen({ conflicts: true, staged: true, unstaged: true, untracked: true });
  }, [orderOf, snapshotRef]);

  const toggleGroup = useCallback((kind: TreeGroupKey) => {
    setGroupOpen((current) => ({ ...current, [kind]: !current[kind] }));
  }, [orderOf, ]);

  const toggleDir = useCallback((dir: string) => {
    setDirOpen((current) => {
      const next = new Set(current);
      if (next.has(dir)) next.delete(dir);
      else next.add(dir);
      return next;
    });
  }, [orderOf, ]);

  return {
    treeSelection, groupOpen, setGroupOpen, dirOpen, followActiveEntry,
    expandToSelection, treeSelectedEntries, activateTreeEntry, moveTreeSelection,
    selectAllEntries, toggleGroup, toggleDir,
  };
}
