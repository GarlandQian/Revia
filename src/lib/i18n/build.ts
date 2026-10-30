/**
 * 由 x18n 管理的 JSON 资源构造运行时字典（M6 L01）。
 *
 * JSON 里函数型文案存成带占位符的模板（如 `"{count} 处改动"`）；这里按
 * `FUNCTIONS` 还原成函数。运行期状态到完整字面量键的映射见 `text.ts`。
 */
import { COMMIT_SHORTCUT_LABEL, MOD_SYMBOLS, isMac } from "@/lib/platform";
import { format } from "./format";
import type { Messages } from "./types";

/** 平台相关示例路径：只影响占位符的取值，文案本身在资源里。 */
const GIT_PATH_EXAMPLE = isMac ? "/opt/homebrew/bin/git" : "C:\\Program Files\\Git\\cmd\\git.exe";

/** 函数型文案：路径 → 参数名（与 JSON 占位符一致）。 */
const FUNCTIONS: Record<string, readonly string[]> = {
  "toolbar.operation": ["label"],
  "changes.hunkCount": ["count"],
  "changes.segmentAt": ["action", "line"],
  "changes.fileCount": ["count"],
  "changes.batchStaged": ["count"],
  "changes.batchUnstaged": ["count"],
  "changes.batchDiscarded": ["count"],
  "changes.batchPartial": ["done", "failed"],
  "review.progress": ["reviewed", "total"],
  "recovery.confirmFiles": ["count"],
  "diff.foldLines": ["count"],
  "diff.changeCount": ["count"],
  "diff.filePosition": ["index", "total"],
  "merge.tabTitle": ["name"],
  "merge.sideTitle.withRole": ["fallback", "roleName"],
  "merge.sideTitle.headRole": ["fallback"],
  "merge.sideTitle.plain": ["fallback"],
  "merge.sideTitle.withRoleAndName": ["fallback", "roleName", "name"],
  "merge.conflictCount.some": ["count"],
  "merge.confirm.apply.bodyConflicts": ["conflicts"],
  "notices.expandOutput": ["lines"],
  "commit.firstLineHint": ["length"],
  "commit.subjectTooLong": ["length", "limit"],
  "commit.bodyLineTooLong": ["line", "length", "limit"],
  "commit.templateFailed": ["detail"],
  "commit.draftFailed": ["detail"],
  "commit.draftLoadFailed": ["detail"],
  "commit.outputTitle": ["worktree"],
  "commit.outputExpand": ["count"],
  "commit.resultSuccess": ["head"],
  "commit.resultAmend": ["head"],
  "commit.resultGitFailed": ["exitCode"],
  "commit.conflictBlocked": ["count"],
  "commit.operationHintSentence": ["text"],
  "commit.upstreamWarning": ["name"],
  "commit.amendConfirmBody": ["name"],
  "commit.progress": ["reviewed", "total"],
  "commit.resultHookChanges": ["worktree", "index"],
  "commit.resultSnapshot": ["id"],
  "commit.resultSnapshotFiles": ["paths"],
  "commit.resultDraftError": ["detail"],
  "commit.resultLines": ["count"],
  "commit.closeFlushFailed": ["detail"],
  "log.branchCount": ["count"],
  "log.authorCount": ["count"],
  "log.pathCount": ["count"],
  "log.fileCount": ["count"],
  "log.parentGroup": ["index", "oid"],
  "log.today": ["time"],
  "log.yesterday": ["time"],
  "log.spanLimitHint": ["limit"],
  "branches.checkoutTracked": ["name"],
  "branches.checkoutAsLocal": ["name"],
  "branches.updateHint": ["shortcut"],
  "branches.pushHint": ["shortcut"],
  "branches.checkoutBlockedDetail": ["target"],
  "branches.commitsToPush": ["count"],
  "branches.forceConfirmHint": ["remote", "branch"],
  "remote.fetchDone": ["remotes"],
  "remote.updated": ["count"],
  "remote.stashKept": ["name"],
  "remote.conflictFiles": ["files"],
  "remote.pushed": ["count", "target"],
  "remote.branchCreated": ["name"],
  "remote.branchCreatedCheckedOut": ["name"],
  "remote.checkoutDone": ["name"],
  "history.parent": ["oid"],
  "history.commit": ["oid"],
  "history.parent1": ["oid"],
  "history.parent2": ["oid"],
  "historyActions.selectedCount": ["count"],
  "historyActions.appliedList": ["oids"],
  "historyActions.skippedList": ["oids"],
  "historyActions.failedList": ["oid"],
  "historyActions.pendingList": ["oids"],
  "operation.confirmAbort": ["kind"],
  "operation.confirmSkip": ["kind"],
  "operation.autostashSaved": ["kind"],
  "operation.conflicts": ["count"],
  "operation.stopped": ["kind", "count"],
  "operation.inProgress": ["kind"],
  "externalTools.finished": ["tool"],
  "externalTools.failedExit": ["tool", "code"],
  "fileHistory.title": ["path"],
  "stash.archives": ["count"],
  "stash.otherArchives": ["count"],
  "stash.clearConfirm": ["count"],
  "stash.expires": ["expires"],
  "stash.files": ["count"],
  "commandLog.count": ["count"],  "close.writing": ["count"],
  "close.pending": ["count"],
  "close.listenerFailed": ["what", "detail"],
  "close.guardUnavailable": ["detail"],
};

type Raw = Record<string, any>;

/** 取原始资源里的字符串（组合文案用）。 */
function at(raw: Raw, path: string): string {
  let cursor: any = raw;
  for (const segment of path.split(".")) cursor = cursor?.[segment];
  return typeof cursor === "string" ? cursor : path;
}

/**
 * 把资源树构造成字典：函数型文案还原为函数，其余保留结构。
 */
function materialize(node: any, path: string[], locale: "zh-CN" | "en", emptyList: string): any {
  if (Array.isArray(node)) return node.map((item) => materialize(item, path, locale, emptyList));
  if (node && typeof node === "object") {
    const out: Record<string, any> = {};
    for (const [key, value] of Object.entries(node)) out[key] = materialize(value, [...path, key], locale, emptyList);
    return out;
  }
  if (typeof node === "string") {
    const key = path.join(".");
    const params = FUNCTIONS[key];
    if (!params) return format(node, { MOD_SYMBOLS: MOD_SYMBOLS.primary, primary: MOD_SYMBOLS.primary, shift: MOD_SYMBOLS.shift, control: MOD_SYMBOLS.control, alt: MOD_SYMBOLS.alt }, locale);
    return (...args: unknown[]) =>
      format(node, Object.fromEntries(params.map((name, index) => {
        const value = args[index];
        if (!Array.isArray(value)) return [name, value as string | number];
        if (key === "commit.resultHookChanges") return [name, value.join(", ") || emptyList];
        const separator = locale === "zh-CN" ? "、" : ", ";
        if (key === "remote.conflictFiles") return [name, value.slice(0, 8).join(separator) + (value.length > 8 ? " …" : "")];
        return [name, value.join(separator)];
      })), locale);
  }
  return node;
}

/** 由资源构造字典；`locale` 只用于让调用方明确指出语言。 */
export function buildMessages(raw: Raw, locale: "zh-CN" | "en"): Messages {
  const tree = materialize(raw, [], locale, at(raw, "common.none")) as Raw;
  const stageShortcut = MOD_SYMBOLS.alt + MOD_SYMBOLS.primary;
  for (const key of ["stageFile", "unstageFile", "discardFile", "review", "reviewNext"]) {
    tree.changes.shortcuts[key] = format(tree.changes.shortcuts[key], { shortcut: stageShortcut });
  }
  tree.changes.noSelectionDetail = format(tree.changes.noSelectionDetail, { commitShortcut: COMMIT_SHORTCUT_LABEL, reviewShortcut: stageShortcut, primary: MOD_SYMBOLS.primary });
  // Function parameters remain lazy; fixed platform values are materialized.
  tree.changes.shortcuts.commit = format(at(raw, "changes.shortcuts.commit"), {
    shortcut: COMMIT_SHORTCUT_LABEL,
  });
  tree.commit.submitHint = format(at(raw, "commit.submitHint"), { shortcut: COMMIT_SHORTCUT_LABEL });
  tree.settings.gitPathPlaceholder = format(at(raw, "settings.gitPathPlaceholder"), {
    path: GIT_PATH_EXAMPLE,
  });
  return tree as Messages;
}
