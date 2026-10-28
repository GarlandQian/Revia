/**
 * 日志的按 worktree 状态（M5 L01）。
 *
 * 每个 worktree 各自保留筛选条件、选中提交、滚动位置与 merge 分组开关；本次运行内
 * 切换 worktree 再回来时恢复。状态不写磁盘（Rebased 同样按打开的工作区保留），
 * 也不随工作区刷新重置。
 *
 * 纯状态容器 + React 绑定；真正的取数在 `LogPager` 与 `LogView`。
 */

import { useCallback, useRef, useState } from "react";
import { defaultFilter, type LogFilterState } from "./logModel";

export interface LogWorkspaceState {
  filter: LogFilterState;
  selectedCommit: string | null;
  scrollTop: number;
  /** merge 提交是否按父提交分组列出改动（L05 的开关）。 */
  mergeByParent: boolean;
}

export function defaultWorkspaceState(): LogWorkspaceState {
  return { filter: defaultFilter(), selectedCommit: null, scrollTop: 0, mergeByParent: false };
}

export interface LogWorkspace {
  state: LogWorkspaceState;
  update: (patch: Partial<LogWorkspaceState>) => void;
}

export class LogWorkspaceStore {
  private readonly states = new Map<string, LogWorkspaceState>();

  read(key: string): LogWorkspaceState {
    let state = this.states.get(key);
    if (!state) {
      state = defaultWorkspaceState();
      this.states.set(key, state);
    }
    return state;
  }

  update(key: string, patch: Partial<LogWorkspaceState>): void {
    this.states.set(key, { ...this.read(key), ...patch });
  }
}

export function useLogWorkspace(worktreeId: string | null): LogWorkspace {
  const store = useRef(new LogWorkspaceStore());
  const key = worktreeId ?? "";
  const [, setVersion] = useState(0);

  // 旧视图的清理或节流回调仍归原 worktree，不能写入切换后的当前目标。
  const update = useCallback((patch: Partial<LogWorkspaceState>) => {
    store.current.update(key, patch);
    setVersion((value) => value + 1);
  }, [key]);

  return { state: store.current.read(key), update };
}
