import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { HunkAction } from "@/types/bindings/HunkAction";

/** 操作凭据只能来自当前显示行，不能用新文件版本补齐旧 hunk 的目标。 */
export function matchesHunkAction(
  action: HunkAction,
  worktreeId: string,
  entry: ChangeEntry,
): boolean {
  return !entry.readonlyReason && !entry.binary && !entry.unmerged
    && entry.changes !== "untracked"
    && action.worktreeId === worktreeId
    && action.path === entry.path
    && action.changes === entry.changes
    && action.fingerprint === entry.fingerprint
    && action.snapshotRevision === entry.snapshotRevision;
}
