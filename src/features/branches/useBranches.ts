/**
 * 分支组件的状态（M5 B02、B03）。
 *
 * - 打开 / 切换 worktree 或引用变化后重新读取分支概览；
 * - 自动 fetch（默认关，每 10 分钟一次，有写操作时跳过本轮）；
 * - 检查远端是否有新提交（默认关，每 10 分钟比较上游引用，不取回对象）。
 *
 * 写操作本身不在这里执行：这里只发出“用户想做某事”的意图，App 用统一的
 * 写编排（`runRemoteWrite`）执行，结果与刷新归原 worktree。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type { BranchOverview } from "@/types/bindings/BranchOverview";
import { ipc } from "@/lib/ipc";
import { getErrorMessage } from "@/lib/error";
import { IncomingState, type IncomingToken } from "./incomingState";

/** 自动 fetch / 检查远端的间隔（Q152）。 */
export const AUTO_FETCH_INTERVAL_MS = 10 * 60 * 1000;

export interface BranchesState {
  overview: BranchOverview | null;
  loading: boolean;
  error: string | null;
  /** 远端有新提交（检查远端的结果）。 */
  incoming: boolean;
}

export interface UseBranchesOptions {
  worktreeId: string | null;
  /** 引用变化（提交、检出、fetch、更新、推送、外部 Git）后重新读取。 */
  refreshKey: number;
  /** 是否开启自动 fetch。 */
  autoFetch: boolean;
  /** 是否开启“检查远端是否有新提交”。 */
  checkRemote: boolean;
  /** 当前是否有写操作（自动 fetch 跳过本轮）。 */
  writing: boolean;
  /** 自动 fetch 的实际执行（走统一写编排）。 */
  runFetch: (worktreeId: string, reason: "auto") => void;
  runCheck: (worktreeId: string) => Promise<boolean>;
}

export function useBranches(
  options: UseBranchesOptions,
): BranchesState & {
  reload: () => void;
  captureIncoming: (worktreeId: string) => IncomingToken | null;
  clearIncoming: (token: IncomingToken | null) => void;
} {
  const { worktreeId, refreshKey, autoFetch, checkRemote } = options;
  const [state, setState] = useState<Omit<BranchesState, "incoming">>({
    overview: null, loading: false, error: null,
  });
  const [, setIncomingRevision] = useState(0);
  const incomingRef = useRef(new IncomingState());
  incomingRef.current.select(worktreeId, checkRemote);
  const requestRef = useRef(0);
  const overviewReadRef = useRef<Promise<void>>(Promise.resolve());
  const optionsRef = useRef(options);
  optionsRef.current = options;

  const reload = useCallback(() => {
    const request = ++requestRef.current;
    if (!worktreeId) {
      overviewReadRef.current = Promise.resolve();
      setState({ overview: null, loading: false, error: null });
      return;
    }
    const token = incomingRef.current.capture(worktreeId);
    setState((previous) => ({ ...previous, loading: true, error: null }));
    overviewReadRef.current = ipc
      .branchesOverview(worktreeId)
      .then((overview) => {
        if (request !== requestRef.current) return;
        if (!incomingRef.current.observeOverview(token, overview)) return;
        setState((previous) => ({ ...previous, overview, loading: false, error: null }));
      })
      .catch((caught: unknown) => {
        if (request !== requestRef.current) return;
        if (!token || incomingRef.current.capture(worktreeId)?.activation !== token.activation) return;
        setState((previous) => ({ ...previous, loading: false, error: getErrorMessage(caught) }));
      });
  }, [worktreeId]);

  useEffect(() => {
    reload();
  }, [reload, refreshKey]);

  // 自动 fetch：每 10 分钟一次；有写操作时跳过本轮（不排队）。
  useEffect(() => {
    if (!worktreeId || !autoFetch) return;
    const timer = window.setInterval(() => {
      const current = optionsRef.current;
      if (current.writing) return;
      current.runFetch(worktreeId, "auto");
    }, AUTO_FETCH_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [autoFetch, worktreeId]);

  // 检查远端（审查第 3 项）：只在开启或切换 worktree 时检查一次，之后每 10 分钟一次。
  // 不再依赖引用版本：提交、检出、fetch 之后不该立刻多跑一次 `ls-remote`；
  // fetch / 更新成功后由调用方清除“远端有新提交”标记。
  useEffect(() => {
    if (!worktreeId || !checkRemote) return;
    let cancelled = false;
    const check = () => {
      // 先取得分支与上游身份；概览刷新只失效旧结果，不触发额外联网。
      void (async () => {
        let reading: Promise<void>;
        do {
          reading = overviewReadRef.current;
          await reading;
          if (cancelled) return false;
        } while (reading !== overviewReadRef.current);
        return incomingRef.current.check((id) => optionsRef.current.runCheck(id));
      })()
        .then((accepted) => {
          if (!cancelled && accepted) setIncomingRevision((value) => value + 1);
        })
        .catch(() => undefined);
    };
    check();
    const timer = window.setInterval(check, AUTO_FETCH_INTERVAL_MS);
    return () => {
      cancelled = true;
      incomingRef.current.cancelChecks();
      window.clearInterval(timer);
    };
  }, [checkRemote, worktreeId]);

  const captureIncoming = useCallback((id: string) => incomingRef.current.capture(id), []);

  /** 只清除原目标的标记，并阻止清除前的检查响应重新点亮它。 */
  const clearIncoming = useCallback((token: IncomingToken | null) => {
    if (incomingRef.current.clear(token)) setIncomingRevision((value) => value + 1);
  }, []);

  return {
    ...state,
    overview: state.overview?.worktreeId === worktreeId ? state.overview : null,
    incoming: incomingRef.current.incoming,
    reload,
    captureIncoming,
    clearIncoming,
  };
}

/** 分支名字校验的前端提示（真正的规则在 Rust 的 `check-ref-format`）。 */
export function quickBranchNameProblem(name: string): string | null {
  const value = name.trim();
  if (!value) return "分支名不能为空";
  if (value.startsWith("-")) return "分支名不能以 - 开头";
  if (value.endsWith(".lock")) return "分支名不能以 .lock 结尾";
  if (/[\s~^:?*[\\]/.test(value)) return "分支名不能包含空格或 ~ ^ : ? * [ \\";
  if (value.includes("..") || value.includes("@{") || value.includes("//")) return "分支名不能包含 .. 、@{ 或连续的 /";
  if (value.startsWith("/") || value.endsWith("/") || value.endsWith(".")) return "分支名的 / 与 . 位置不合法";
  return null;
}
