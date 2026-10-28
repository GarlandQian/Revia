/**
 * 受保护的写操作编排（Q90、Q91、Q96、M5 第二批 B01）。
 *
 * 所有写操作（暂存、合并、检出、新建分支、fetch、更新、推送）共用同一条边界：
 * - 目标 worktree 在发起时确定，之后切换 worktree 也不改变结果与刷新归属；
 * - 同步占用该 worktree 的全局写门禁，已有写操作时直接拒绝，不发 IPC；
 * - 关闭 / 退出保护建立后才发 IPC，保护失败时不写；
 * - 成功或失败都真实重读原目标，重读失败不覆盖写入结果；
 * - 收尾只做一次；结果待核对时登记到关闭协调器，退出前先展示待核对。
 *
 * 纯模块：App 与 Node 回归使用同一份实现（`tools/test-remote-write.mjs`
 * 与 `tools/test-merge-write.mjs`）。
 */

import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";

export interface GuardedWriteDeps<Token> {
  /** 全局写门禁（App 的 `writingWorktreesRef.current`）。 */
  writing: Set<string>;
  /** 门禁变化后同步到界面状态。 */
  publishWriting: () => void;
  /** 共享异步编排（`WriteGuardOrchestrator`）。 */
  begin: (worktreeId: string) => Promise<{ ok: true } | { ok: false; reason: string }>;
  end: (worktreeId: string, needsVerification: boolean) => void;
  /** 操作结果通道（`OperationResultGate`）：较新的操作会让旧结果失去记录资格。 */
  beginOperation: (worktreeId: string) => Token;
  canRecord: (token: Token) => boolean;
  /** 真正的写命令（IPC）。 */
  execute: (request: { worktreeId: string }) => Promise<unknown>;
  /** 对指定 worktree 真实重读一次（界面是否接收由它自己判断）。 */
  refreshTarget: (worktreeId: string) => Promise<unknown>;
  /** 失败按原目标报告（不随组件卸载消失）。 */
  reportFailure: (worktreeId: string, error: AppErrorPayload) => void;
  toError: (caught: unknown) => AppErrorPayload;
  busyMessage: string;
  guardUnavailable: (reason: string) => string;
}

export interface GuardedWriteRequest {
  worktreeId: string;
}

/** 执行一次受保护的写入；失败时抛出规范化后的错误。 */
export async function runGuardedWrite<TRequest extends GuardedWriteRequest, Token>(
  deps: GuardedWriteDeps<Token>,
  request: TRequest,
): Promise<void> {
  const worktreeId = request.worktreeId;
  if (deps.writing.has(worktreeId)) {
    const busy: AppErrorPayload = { kind: "rejected", message: deps.busyMessage };
    throw busy;
  }
  deps.writing.add(worktreeId);
  deps.publishWriting();
  let started: { ok: true } | { ok: false; reason: string };
  try {
    started = await deps.begin(worktreeId);
  } catch (caught) {
    deps.writing.delete(worktreeId);
    deps.publishWriting();
    throw deps.toError(caught);
  }
  if (!started.ok) {
    deps.writing.delete(worktreeId);
    deps.publishWriting();
    const unavailable: AppErrorPayload = { kind: "internal", message: deps.guardUnavailable(started.reason) };
    throw unavailable;
  }

  const token = deps.beginOperation(worktreeId);
  let failure: AppErrorPayload | null = null;
  try {
    try {
      await deps.execute(request);
    } catch (caught) {
      failure = deps.toError(caught);
      if (deps.canRecord(token)) deps.reportFailure(worktreeId, failure);
    }
    try {
      await deps.refreshTarget(worktreeId);
    } catch {
      // 重读失败由 refreshTarget 自己报告，不覆盖写入结果。
    }
  } finally {
    deps.writing.delete(worktreeId);
    deps.publishWriting();
    deps.end(worktreeId, failure?.kind === "needsVerification");
  }
  if (failure) throw failure;
}
