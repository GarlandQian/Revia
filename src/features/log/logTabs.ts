/**
 * 提交 diff 页签的状态（M5 L01）。
 *
 * 与工作区的改动 diff 页签分型：提交页签属于某个只读历史目标，标题是
 * “文件名（短哈希）”，同一提交同一文件重复打开时激活已有页签。工作区刷新
 * 不会关闭这些页签，也不会抢走激活状态。
 */

import { useCallback, useState } from "react";
import type { HistoryFileTarget } from "@/types/bindings/HistoryFileTarget";

export type CommitTabKind = "diff" | "merge";

export interface CommitDiffTab {
  /** `${commit}:${path}`。 */
  key: string;
  kind: CommitTabKind;
  worktreeId: string;
  targetId: string;
  commit: string;
  path: string;
  /** 普通提交 diff 的比较父提交；根提交为 null。 */
  parent: string | null;
  title: string;
  /** 普通提交 diff 的固定文件身份（由改动文件列表签发）。 */
  file?: HistoryFileTarget;
}

export interface OpenCommitTabRequest {
  kind: CommitTabKind;
  worktreeId: string;
  targetId: string;
  commit: string;
  path: string;
  parent: string | null;
  title: string;
  file?: HistoryFileTarget;
}

export function commitTabKey(commit: string, path: string): string {
  return `${commit}:${path}`;
}

export function showCommitTab(current: CommitDiffTab[], request: OpenCommitTabRequest): CommitDiffTab[] {
  const tab: CommitDiffTab = { ...request, key: commitTabKey(request.commit, request.path) };
  const index = current.findIndex((item) => item.key === tab.key);
  return index < 0 ? [...current, tab] : current.map((item, position) => position === index ? tab : item);
}

export interface CommitTabs {
  tabs: CommitDiffTab[];
  activeKey: string | null;
  active: CommitDiffTab | null;
  open: (request: OpenCommitTabRequest) => void;
  activate: (key: string) => void;
  close: (key: string) => void;
  /** 历史目标关闭 / 失效时清空该目标的页签。 */
  dropTarget: (targetId: string) => void;
}

export function useCommitTabs(): CommitTabs {
  const [tabs, setTabs] = useState<CommitDiffTab[]>([]);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const open = useCallback((request: OpenCommitTabRequest) => {
    const key = commitTabKey(request.commit, request.path);
    setTabs((current) => showCommitTab(current, request));
    setActiveKey(key);
  }, []);

  const activate = useCallback((key: string) => setActiveKey(key), []);

  const close = useCallback((key: string) => {
    setTabs((current) => {
      const index = current.findIndex((tab) => tab.key === key);
      const next = current.filter((tab) => tab.key !== key);
      setActiveKey((active) => {
        if (active !== key) return active;
        return next[Math.min(index, next.length - 1)]?.key ?? null;
      });
      return next;
    });
  }, []);

  const dropTarget = useCallback((targetId: string) => {
    setTabs((current) => {
      const next = current.filter((tab) => tab.targetId !== targetId);
      setActiveKey((active) => (next.some((tab) => tab.key === active) ? active : null));
      return next;
    });
  }, []);

  const active = tabs.find((tab) => tab.key === activeKey) ?? null;
  return { tabs, activeKey, active, open, activate, close, dropTarget };
}
