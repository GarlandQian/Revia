import { useCallback, useRef, useState } from "react";
import type { CommitResultView } from "@/components/app/CommitComposer";
import type { NoticeView } from "@/components/app/Notifications";
import type { NotifyKinds } from "@/lib/ipc";
import { messages } from "@/lib/i18n/index";
import { formatAppError } from "@/lib/error";

export type NoticeInput = Omit<NoticeView, "id"> & { kind?: keyof NotifyKinds };

export function commitResultNotice(result: CommitResultView): NoticeView {
  const detail = () => {
    const detailParts = [
    typeof result.detail === "function" ? result.detail() : result.detail,
    result.outcome?.error ? formatAppError(result.outcome.error) : null,
    result.hookChanges && (result.hookChanges.worktree.length > 0 || result.hookChanges.index.length > 0)
      ? messages.commit.resultHookChanges(result.hookChanges.worktree, result.hookChanges.index)
      : null,
    result.snapshotRecordId ? messages.commit.resultSnapshot(result.snapshotRecordId) : null,
    result.snapshotPaths.length > 0 ? messages.commit.resultSnapshotFiles(result.snapshotPaths) : null,
    result.outcome && result.outcome.success && !result.outcome.draftCleared
      ? messages.commit.resultDraftKept
      : null,
    result.outcome?.draftError ? messages.commit.resultDraftError(formatAppError(result.outcome.draftError)) : null,
  ].filter((part): part is string => typeof part === "string" && part.length > 0);
    return detailParts.length > 0 ? detailParts.join("\n") : null;
  };
  return {
    id: `commit:${result.worktreeId}`,
    tone: result.outcome === null
      ? "info"
      : result.needsVerification
        ? "warning"
        : result.outcome.success
          ? "success"
          : "error",
    title: result.title,
    detail,
    output: result.output,
    persistent: result.outcome === null || !(result.outcome.success && !result.needsVerification),
  };
}

export function useNotifications() {
  const [notifyKinds, setNotifyKinds] = useState<NotifyKinds>({
    stage: true, discard: true, commit: true, recovery: true,
  });
  const notifyKindsRef = useRef(notifyKinds);
  notifyKindsRef.current = notifyKinds;
  const [notices, setNotices] = useState<NoticeView[]>([]);
  const noticeSeq = useRef(0);
  // 提交结果按原 worktree 保存，切换仓库不丢弃输出或错误。
  const [commitResults, setCommitResults] = useState<Record<string, CommitResultView>>({});

  // 回调身份固定；切换通知偏好不重装依赖它的写入与关闭保护。
  const notify = useCallback((notice: NoticeInput) => {
    const quiet = notice.tone === "success" || notice.tone === "info";
    if (quiet && notice.kind && !notifyKindsRef.current[notice.kind]) return;
    noticeSeq.current += 1;
    const id = `n${noticeSeq.current}`;
    setNotices((current) => [...current, { ...notice, id }].slice(-6));
  }, []);

  const dismissAllNotices = useCallback((id: string) => {
    if (id.startsWith("commit:")) {
      const target = id.slice("commit:".length);
      setCommitResults((current) => {
        const next = { ...current };
        delete next[target];
        return next;
      });
    } else {
      setNotices((current) => current.filter((notice) => notice.id !== id));
    }
  }, []);

  const commitNotices = Object.values(commitResults)
    .filter((result) => notifyKinds.commit || !(result.outcome?.success && !result.needsVerification))
    .map(commitResultNotice);

  return {
    notifyKinds, setNotifyKinds, notify, commitResults, setCommitResults,
    allNotices: [...notices, ...commitNotices], dismissAllNotices,
  };
}
