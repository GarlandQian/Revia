import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { HunkAction } from "@/types/bindings/HunkAction";
import type { SegmentAction } from "@/types/bindings/SegmentAction";

/** 操作凭据只能来自当前显示行，不能用新文件版本补齐旧 hunk 的目标。 */
export function matchesHunkAction(
  action: HunkAction,
  worktreeId: string,
  entry: ChangeEntry,
): boolean {
  return matchesActionTarget(action, worktreeId, entry);
}

/** 正文段首行只有段凭据，不能用标题行的 hunk 凭据判断按钮是否可用。 */
export function segmentActionDisabled(
  action: SegmentAction | null,
  worktreeId: string,
  entry: ChangeEntry,
  blocked: boolean,
): boolean {
  return blocked || !action || action.ignoreWhitespace === "allAndBlankLines"
    || !matchesActionTarget(action, worktreeId, entry);
}

function matchesActionTarget(
  action: HunkAction | SegmentAction,
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
