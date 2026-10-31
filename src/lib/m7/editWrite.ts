/** 工作区编辑保存复用现有写门、退出保护和按原目标报告结果的通道。 */
import { runGuardedWrite, type GuardedWriteDeps } from "@/lib/guardedWrite";
import { localizedError, toAppError } from "@/lib/error";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { EditSaved } from "@/types/bindings/EditSaved";
import type { EditSavePayload } from "@/lib/m7/api";
export type EditWriteDeps<Token> = Omit<GuardedWriteDeps<Token>, "execute"> & {
  execute: (request: EditSavePayload) => Promise<EditSaved>;
};
export async function runEditWrite<Token>(deps: EditWriteDeps<Token>, request: EditSavePayload): Promise<EditSaved> {
  let saved!: EditSaved;
  let refreshFailure: AppErrorPayload | null = null;
  await runGuardedWrite({ ...deps,
    execute: async () => { saved = await deps.execute(request); },
    refreshTarget: async (id) => {
      try {
        if (await deps.refreshTarget(id) === false) throw localizedError("needsVerification", "errors.recovery.writeResultNeedsVerification");
      } catch (caught) {
        refreshFailure = { ...localizedError("needsVerification", "errors.recovery.writeResultNeedsVerification"), cause: toAppError(caught) };
        throw refreshFailure;
      }
    },
    end: (id, needsVerification) => deps.end(id, needsVerification || refreshFailure !== null),
  }, request);
  if (refreshFailure) { deps.reportFailure(request.worktreeId, refreshFailure); throw refreshFailure; }
  return saved;
}
