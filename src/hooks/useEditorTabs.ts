import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { mergeTabKey, tabKey, type EditorTab } from "@/components/app/EditorTabs";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";
import { closeEditorTab, retainEditorTabs, showDiffTab, type MergeTabTarget } from "./editorTabsState";

interface EditorTabsOptions {
  snapshot: ChangesSnapshot | null;
  snapshotRef: RefObject<ChangesSnapshot | null>;
  selectedRef: RefObject<Pick<ChangeEntry, "path" | "changes"> | null>;
  worktreeIdRef: RefObject<string | null>;
  openEntry: (entry: ChangeEntry) => void;
  selectMergeEntry: (entry: ChangeEntry) => void;
  clearSelection: () => void;
}

export function useEditorTabs({
  snapshot, snapshotRef, selectedRef, worktreeIdRef, openEntry, selectMergeEntry, clearSelection,
}: EditorTabsOptions) {
  const [tabs, setTabs] = useState<EditorTab[]>([]);
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;
  const [mergeTabPath, setMergeTabPath] = useState<string | null>(null);
  const mergeTabPathRef = useRef(mergeTabPath);
  mergeTabPathRef.current = mergeTabPath;
  const mergeTabTargetRef = useRef<MergeTabTarget | null>(null);

  const replaceTabs = useCallback((next: EditorTab[]) => {
    tabsRef.current = next;
    setTabs(next);
  }, []);
  const selectMergePath = useCallback((path: string | null) => {
    mergeTabTargetRef.current = path && worktreeIdRef.current
      ? { worktreeId: worktreeIdRef.current, path }
      : null;
    mergeTabPathRef.current = path;
    setMergeTabPath(path);
  }, [worktreeIdRef]);

  useEffect(() => {
    if (!snapshot) return;
    const current = tabsRef.current;
    const next = retainEditorTabs(current, snapshot);
    if (next !== current) replaceTabs(next);
    const mergePath = mergeTabPathRef.current;
    if (mergePath && !next.some((tab) => tab.key === mergeTabKey(mergePath))) selectMergePath(null);
  }, [snapshot, replaceTabs, selectMergePath]);

  const enterDiff = useCallback(() => selectMergePath(null), [selectMergePath]);

  const resetMergeTabs = useCallback(() => {
    replaceTabs(tabsRef.current.filter((tab) => tab.kind !== "merge"));
    if (mergeTabPathRef.current) {
      selectMergePath(null);
      clearSelection();
    }
  }, [clearSelection, replaceTabs, selectMergePath]);

  const recordDiffTab = useCallback((entry: ChangeEntry, pin: boolean) => {
    replaceTabs(showDiffTab(tabsRef.current, {
      key: tabKey(entry.path, entry.changes), path: entry.path, changes: entry.changes, preview: !pin,
    }));
  }, [replaceTabs]);

  const openMergeTab = useCallback((entry: ChangeEntry) => {
    if (!worktreeIdRef.current) return;
    const key = mergeTabKey(entry.path);
    if (!tabsRef.current.some((tab) => tab.key === key)) {
      replaceTabs([...tabsRef.current, {
        key, path: entry.path, changes: entry.changes, preview: false, kind: "merge",
      }]);
    }
    selectMergePath(entry.path);
    selectMergeEntry(entry);
  }, [replaceTabs, selectMergeEntry, selectMergePath, worktreeIdRef]);

  const activateTab = useCallback((tab: EditorTab) => {
    if (!worktreeIdRef.current) return;
    const entry = snapshotRef.current?.entries.find(
      (candidate) => candidate.path === tab.path
        && (tab.kind === "merge" ? candidate.unmerged : candidate.changes === tab.changes),
    );
    if (!entry) return;
    if (tab.kind === "merge") {
      selectMergePath(tab.path);
      selectMergeEntry(entry);
    } else {
      openEntry(entry);
    }
  }, [openEntry, selectMergeEntry, selectMergePath, snapshotRef, worktreeIdRef]);

  const closeByKey = useCallback((key: string | null) => {
    const selected = selectedRef.current;
    const activeKey = mergeTabPathRef.current
      ? mergeTabKey(mergeTabPathRef.current)
      : selected ? tabKey(selected.path, selected.changes) : null;
    const result = closeEditorTab(tabsRef.current, activeKey, key);
    replaceTabs(result.tabs);
    // 导航放在状态更新外；关闭 diff 后相邻页签仍按自己的 kind 激活。
    if (result.activate === null) {
      selectMergePath(null);
      clearSelection();
    } else if (result.activate) {
      activateTab(result.activate);
    }
  }, [activateTab, clearSelection, replaceTabs, selectedRef, selectMergePath]);

  const closeTab = useCallback((tab: EditorTab) => closeByKey(tab.key), [closeByKey]);
  const closeMergeTab = useCallback((path: string) => closeByKey(mergeTabKey(path)), [closeByKey]);
  const closeAllTabs = useCallback(() => closeByKey(null), [closeByKey]);

  return {
    tabs, mergeTabPath, mergeTabPathRef, mergeTabTargetRef, enterDiff, recordDiffTab, resetMergeTabs,
    openMergeTab, activateTab, closeTab, closeMergeTab, closeAllTabs,
  };
}
