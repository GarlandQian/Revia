/**
 * 合并写入的全局保护（Q90、Q91、Q96、Q143）。
 *
 * “应用”合并结果和整文件接受一侧都是写操作，与按块写入、恢复走同一套门禁：
 * - 目标 worktree 在发起时确定，之后切换 worktree 也不改变结果与重读的归属；
 * - 同步占用该 worktree 的写门禁，已有写操作时直接拒绝，不发 IPC；
 * - 关闭/退出保护建立后才发 IPC，保护失败时不写；
 * - 成功或失败都真实重读原目标，重读失败不覆盖写入结果；
 * - 收尾只做一次；结果待核对时登记到关闭协调器，退出前先展示待核对。
 *
 * 不依赖组件生命周期：合并页签或冲突对话框在写入期间被关闭，收尾与失败通知照常完成。
 * 编排本体在 `src/lib/guardedWrite.ts`，合并、分支、远端操作共用同一份实现。
 */

import { runGuardedWrite, type GuardedWriteDeps } from "@/lib/guardedWrite";

export type MergeWriteRequest =
  | { kind: "apply"; worktreeId: string; path: string; result: string; fingerprint: string }
  | {
      kind: "acceptSide";
      worktreeId: string;
      targets: Array<{ path: string; fingerprint: string }>;
      side: "ours" | "theirs";
    };

export type MergeWriteDeps<Token> = Omit<GuardedWriteDeps<Token>, "execute"> & {
  /** 写命令：合并写入的 IPC 参数随请求变化。 */
  execute: (request: MergeWriteRequest) => Promise<unknown>;
};

/** 执行一次受保护的合并写入；失败时抛出规范化后的错误。 */
export async function runMergeWrite<Token>(
  deps: MergeWriteDeps<Token>,
  request: MergeWriteRequest,
): Promise<void> {
  // 合并写入的 IPC 参数随请求变化，统一封装成 `{ worktreeId }` 交给编排。
  const guarded: GuardedWriteDeps<Token> = {
    ...deps,
    execute: () => deps.execute(request),
  };
  return runGuardedWrite(guarded, { worktreeId: request.worktreeId });
}
