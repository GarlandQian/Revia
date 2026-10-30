import { useCallback, type Dispatch, type RefObject, type SetStateAction } from "react";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";
import type { BatchTarget } from "@/types/bindings/BatchTarget";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import type { OperationContext, OperationResultGate, ReviewOverlay, ReviewRecoveryBatchOutcome, WriteGuardOrchestrator } from "@/lib/refreshGate";
import { ipc } from "@/lib/ipc";
import { messages } from "@/lib/i18n/index";
import { readonlyReasonText } from "@/lib/i18n/text";
import { closeGuardBlockedError, formatAppError, toAppError } from "@/lib/error";
import { logToBackend as debugLog } from "@/features/diff/log";
import type { NoticeInput } from "./useNotifications";

interface BatchWriteOptions {
  worktreeIdRef: RefObject<string | null>;
  writingWorktreesRef: RefObject<Set<string>>;
  setWritingWorktrees: Dispatch<SetStateAction<Set<string>>>;
  writeOrchestrator: WriteGuardOrchestrator<AppErrorPayload>;
  operationGateRef: RefObject<OperationResultGate>;
  reviewOverlayRef: RefObject<ReviewOverlay>;
  acceptSnapshot: (token: OperationContext, snapshot: ChangesSnapshot) => boolean;
  scheduleReviewRecovery: (worktreeId: string) => Promise<ReviewRecoveryBatchOutcome>;
  refreshTarget: (worktreeId: string) => Promise<boolean>;
  setAppError: (error: AppErrorPayload | null, worktreeId?: string | null) => void;
  notify: (notice: NoticeInput) => void;
  expandToSelection: (entry: ChangeEntry) => ChangeEntry[];
}

/** 整批复用 App 的写门和退出保护，不创建第二套锁或结果代际。 */
export function useBatchWrite({
  worktreeIdRef, writingWorktreesRef, setWritingWorktrees, writeOrchestrator,
  operationGateRef, reviewOverlayRef, acceptSnapshot, scheduleReviewRecovery,
  refreshTarget, setAppError, notify, expandToSelection,
}: BatchWriteOptions) {
  const performBatchWrite = useCallback(async (
    kind: "stage" | "unstage" | "discard",
    entries: ChangeEntry[],
    options: { snapshotRevision?: string } = {},
  ) => {
    const targetWorktreeId = worktreeIdRef.current;
    if (!targetWorktreeId || entries.length === 0) return;
    if (writingWorktreesRef.current.has(targetWorktreeId)) return;
    const writable = entries.filter((entry) => !entry.readonlyReason);
    const readonly = entries.filter((entry) => entry.readonlyReason);
    const readonlyLines = () => readonly.map(
      (entry) => `${entry.path}: ${readonlyReasonText(entry.readonlyReason!)}`,
    );
    if (writable.length === 0) {
      notify({
        tone: "warning",
        title: () => (messages.changes.batchNothing),
        detail: () => readonlyLines().join("\n"),
        persistent: true,
      });
      return;
    }
    writingWorktreesRef.current.add(targetWorktreeId);
    setWritingWorktrees(new Set(writingWorktreesRef.current));
    try {
      const started = await writeOrchestrator.begin(targetWorktreeId);
      if (!started.ok) {
        writingWorktreesRef.current.delete(targetWorktreeId);
        setWritingWorktrees(new Set(writingWorktreesRef.current));
        setAppError(closeGuardBlockedError(started.reason), targetWorktreeId);
        return;
      }
    } catch (caught) {
      writingWorktreesRef.current.delete(targetWorktreeId);
      setWritingWorktrees(new Set(writingWorktreesRef.current));
      setAppError(toAppError(caught), targetWorktreeId);
      return;
    }
    setAppError(null, targetWorktreeId);
    const opToken = operationGateRef.current.begin(targetWorktreeId);
    let needsVerification = false;
    // 写结果的审阅分界必须在请求发起前取（与单项写入口一致）。
    const reviewRead = reviewOverlayRef.current.beginRead();
    try {
      const targets: BatchTarget[] = writable.map((entry) => ({
        path: entry.path,
        changes: entry.changes,
        fingerprint: entry.fingerprint,
      }));
      const outcome = kind === "stage"
        ? await ipc.stageFiles({ worktreeId: targetWorktreeId, targets })
        : kind === "unstage"
          ? await ipc.unstageFiles({ worktreeId: targetWorktreeId, targets })
          : await ipc.discardFiles({
            worktreeId: targetWorktreeId,
            targets,
            snapshotRevision: options.snapshotRevision ?? "",
          });
      if (outcome.snapshot) {
        const written = reviewRead.deliver(outcome.snapshot);
        if (written.ok) {
          operationGateRef.current.acceptSuccess(opToken, outcome.snapshot);
          acceptSnapshot(opToken, written.snapshot);
        } else {
          // x18n-ignore
          debugLog(`批量写结果读取不可信，重新核对：${written.worktreeId}@${written.observedSeq}`);
          await scheduleReviewRecovery(written.worktreeId);
        }
      }
      needsVerification = outcome.needsVerification;
      const ok = outcome.results.filter((result) => result.ok).length;
      const failed = outcome.results.filter((result) => !result.ok && !result.skipped);
      const skipped = outcome.results.filter((result) => result.skipped);
      const lines = () => [
        ...failed.map((result) => `${result.path}: ${result.error ? formatAppError(result.error) : messages.changes.notExecuted}`),
        ...skipped.map((result) => `${result.path}: ${result.skipped ? formatAppError(result.skipped) : messages.changes.notExecuted}`),
        ...readonlyLines(),
      ];
      const doneTitle = () => kind === "stage"
        ? messages.changes.batchStaged(ok)
        : kind === "unstage"
          ? messages.changes.batchUnstaged(ok)
          : messages.changes.batchDiscarded(ok);
      if (needsVerification) {
        notify({
          tone: "warning",
          title: () => (messages.changes.batchNeedsCheck),
          detail: () => lines().join("\n") || undefined,
          persistent: true,
        });
      } else if (failed.length > 0) {
        notify({
          tone: "warning",
          title: () => (messages.changes.batchPartial(doneTitle(), failed.length)),
          detail: () => lines().join("\n"),
          persistent: true,
        });
      } else {
        notify({ tone: "success", kind: kind === "discard" ? "discard" : "stage", title: doneTitle, detail: () => lines().join("\n") || undefined });
      }
      if (operationGateRef.current.canRecord(opToken)) setAppError(null, targetWorktreeId);
    } catch (caught) {
      const err = toAppError(caught);
      needsVerification = err.kind === "needsVerification";
      if (operationGateRef.current.canRecord(opToken)) setAppError(err, targetWorktreeId);
      notify({ tone: "error", title: () => (messages.changes.batchFailedTitle), detail: () => (formatAppError(err)), persistent: true });
      void refreshTarget(targetWorktreeId);
    } finally {
      writingWorktreesRef.current.delete(targetWorktreeId);
      setWritingWorktrees(new Set(writingWorktreesRef.current));
      writeOrchestrator.end(targetWorktreeId, needsVerification);
    }
  }, [acceptSnapshot, notify, operationGateRef, refreshTarget, reviewOverlayRef, scheduleReviewRecovery, setAppError, setWritingWorktrees, worktreeIdRef, writeOrchestrator, writingWorktreesRef]);

  const stageFile = useCallback((entry: ChangeEntry) => {
    if (!worktreeIdRef.current || entry.readonlyReason || entry.changes === "staged") return;
    return performBatchWrite("stage", expandToSelection(entry));
  }, [expandToSelection, performBatchWrite, worktreeIdRef]);

  const unstageFile = useCallback((entry: ChangeEntry) => {
    if (!worktreeIdRef.current || entry.readonlyReason || entry.changes !== "staged") return;
    return performBatchWrite("unstage", expandToSelection(entry));
  }, [expandToSelection, performBatchWrite, worktreeIdRef]);

  return { performBatchWrite, stageFile, unstageFile };
}
