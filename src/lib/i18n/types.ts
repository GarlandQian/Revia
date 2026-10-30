/**
 * 文案资源的类型（M6 L01）。
 *
 * `Messages` 由 `locales/zh-CN.json` 的形状推导；函数型文案（例如
 * `messages.log.authorCount(3)`）按 `FunctionSignatures` 还原为函数签名，
 * 保证组件里的调用在编译期检查参数。
 */
import type zhRaw from "../../../locales/zh-CN.json";
import type { ConflictSideState } from "@/types/bindings/ConflictSideState";
import type { MergeSideRole } from "@/types/bindings/MergeSideRole";

type Resource = typeof zhRaw;

/** 返回值是字符串的函数型文案（参数名对应 JSON 里的占位符）。 */
type FunctionSignatures = {
  "toolbar.operation": [label: string];
  "changes.hunkCount": [count: number];
  "changes.segmentAt": [action: string, line: number];
  "changes.fileCount": [count: number];
  "changes.batchStaged": [count: number];
  "changes.batchUnstaged": [count: number];
  "changes.batchDiscarded": [count: number];
  "changes.batchPartial": [done: string, failed: number];
  "review.progress": [reviewed: number, total: number];
  "recovery.confirmFiles": [count: number];
  "diff.foldLines": [count: number];
  "diff.changeCount": [count: number];
  "diff.filePosition": [index: number, total: number];
  "merge.tabTitle": [name: string];
  "merge.conflictCount": [count: number];
  "merge.sideTitle": [side: "ours" | "theirs", role: MergeSideRole, name: string | null];
  "merge.acceptSideState": [side: "ours" | "theirs", state: ConflictSideState];
  "notices.expandOutput": [lines: number];
  "commit.firstLineHint": [length: number];
  "commit.subjectTooLong": [length: number, limit: number];
  "commit.bodyLineTooLong": [line: number, length: number, limit: number];
  "commit.templateFailed": [detail: string];
  "commit.draftFailed": [detail: string];
  "commit.draftLoadFailed": [detail: string];
  "commit.outputTitle": [worktree: string];
  "commit.outputExpand": [count: number];
  "commit.resultSuccess": [head: string];
  "commit.resultAmend": [head: string];
  "commit.resultGitFailed": [exitCode: number];
  "commit.conflictBlocked": [count: number];
  "commit.operationHintSentence": [text: string];
  "commit.upstreamWarning": [name: string];
  "commit.warnings.crlf": [path: string];
  "commit.warnings.largeFile": [path: string, sizeMb: number, limitMb: number];
  "commit.warnings.invalidFilename": [path: string, reason: string];
  "commit.warnings.invalidReason.character": [character: string];
  "commit.warnings.invalidReason.reservedName": [component: string];
  "commit.amendConfirmBody": [name: string];
  "commit.progress": [reviewed: number, total: number];
  "commit.resultHookChanges": [worktree: string[], index: string[]];
  "commit.resultSnapshot": [id: string];
  "commit.resultSnapshotFiles": [paths: string[]];
  "commit.resultDraftError": [detail: string];
  "commit.resultLines": [count: number];
  "commit.closeFlushFailed": [detail: string];
  "log.branchCount": [count: number];
  "log.authorCount": [count: number];
  "log.pathCount": [count: number];
  "log.fileCount": [count: number];
  "log.parentGroup": [index: number, oid: string];
  "log.today": [time: string];
  "log.yesterday": [time: string];
  "log.spanLimitHint": [limit: number];
  "branches.checkoutTracked": [name: string];
  "branches.checkoutAsLocal": [name: string];
  "branches.updateHint": [shortcut: string];
  "branches.pushHint": [shortcut: string];
  "branches.checkoutBlockedDetail": [target: string];
  "branches.commitsToPush": [count: number];
  "branches.forceConfirmHint": [remote: string, branch: string];
  "remote.fetchDone": [remotes: string];
  "remote.updated": [count: number];
  "remote.stashKept": [name: string];
  "remote.conflictFiles": [files: string[]];
  "remote.pushed": [count: number, target: string];
  "remote.branchCreated": [name: string];
  "remote.branchCreatedCheckedOut": [name: string];
  "remote.checkoutDone": [name: string];
  "history.parent": [oid: string];
  "history.commit": [oid: string];
  "history.parent1": [oid: string];
  "history.parent2": [oid: string];
  "commandLog.count": [count: number];
  "close.writing": [count: number];
  "close.pending": [count: number];
  "close.listenerFailed": [what: string, detail: string];
  "close.guardUnavailable": [detail: string];
  "historyActions.selectedCount": [count: number];
  "historyActions.appliedList": [oids: string];
  "historyActions.skippedList": [oids: string];
  "historyActions.failedList": [oid: string];
  "historyActions.pendingList": [oids: string];
  "operation.confirmAbort": [kind: string];
  "operation.confirmSkip": [kind: string];
  "operation.autostashSaved": [kind: string];
  "operation.conflicts": [count: number];
  "operation.stopped": [kind: string, count: number];
  "operation.inProgress": [kind: string];
  "externalTools.finished": [tool: string];
  "externalTools.failedExit": [tool: string, code: number];
  "fileHistory.title": [path: string];
  "stash.archives": [count: number];
  "stash.otherArchives": [count: number];
  "stash.clearConfirm": [count: number];
  "stash.expires": [expires: string];
  "stash.files": [count: number];
};

type FunctionOverrides = Record<never, never>;

type FnFor<P extends string> = P extends keyof FunctionOverrides
  ? FunctionOverrides[P]
  : P extends keyof FunctionSignatures
    ? (...args: FunctionSignatures[P]) => string
    : never;

type IsFunctionPath<P extends string> = P extends keyof FunctionOverrides
  ? true
  : P extends keyof FunctionSignatures
    ? true
    : false;

type ApplyFunctions<T, Prefix extends string = ""> = T extends readonly string[]
  ? string[]
  : T extends string
    ? string
    : {
        [K in keyof T]: K extends string
          ? IsFunctionPath<`${Prefix}${K}`> extends true
            ? FnFor<`${Prefix}${K}`>
            : ApplyFunctions<T[K], `${Prefix}${K}.`>
          : never;
      };

export type Messages = ApplyFunctions<Resource>;
