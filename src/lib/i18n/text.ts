/**
 * 运行期状态到完整字面量键的映射（Q171）。
 *
 * 组件和构建函数都不拼接翻译键；这里集中用显式 `t("完整.键")` 查表，
 * 让 x18n 的静态扫描能看到真实使用位置。
 */
import type { CommitWarning } from "@/types/bindings/CommitWarning";
import type { InvalidFilenameReason } from "@/types/bindings/InvalidFilenameReason";
import type { ConflictSideState } from "@/types/bindings/ConflictSideState";
import type { MergeSideRole } from "@/types/bindings/MergeSideRole";
import type { MergeUnavailable } from "@/types/bindings/MergeUnavailable";
import type { OperationKind } from "@/types/bindings/OperationKind";
import type { ReadonlyReason } from "@/types/bindings/ReadonlyReason";
import type { DiffHighlight } from "@/features/diff/SideBySideView";
import type { DatePreset } from "@/features/log/logModel";
import type { MergeSideName } from "@/features/merge/mergeModel";
import { t } from "./index";

export function stashFileStatusText(status: string): string {
  switch (status) {
    case "added": return t("merge.added");
    case "deleted": return t("merge.deleted");
    case "renamed": return t("changes.renamed");
    case "copied": return t("changes.copied");
    case "typeChanged": return t("changes.typeChanged");
    default: return t("merge.modified");
  }
}

export function readonlyReasonText(reason: ReadonlyReason): string {
  switch (reason) {
    case "symlink":
      return t("changes.readonlyReasons.symlink");
    case "submodule":
      return t("changes.readonlyReasons.submodule");
    case "nestedRepo":
      return t("changes.readonlyReasons.nestedRepo");
    case "conflict":
      return t("changes.readonlyReasons.conflict");
    case "nonUtf8Path":
      return t("changes.readonlyReasons.nonUtf8Path");
    case "unreadable":
      return t("changes.readonlyReasons.unreadable");
  }
}

export function renamedFrom(dir: string, original: string): string {
  return dir
    ? t("changes.renamedFromWithDir", { dir, original })
    : t("changes.renamedFromOnly", { original });
}

export function sideLabels(
  changes: "unstaged" | "staged" | "untracked",
  conflict: boolean,
): { left: string; right: string } {
  if (conflict) {
    return {
      left: t("diff.sideLabels.conflict.left"),
      right: t("diff.sideLabels.conflict.right"),
    };
  }
  switch (changes) {
    case "unstaged":
      return {
        left: t("diff.sideLabels.unstaged.left"),
        right: t("diff.sideLabels.unstaged.right"),
      };
    case "staged":
      return {
        left: t("diff.sideLabels.staged.left"),
        right: t("diff.sideLabels.staged.right"),
      };
    case "untracked":
      return {
        left: t("diff.sideLabels.untracked.left"),
        right: t("diff.sideLabels.untracked.right"),
      };
  }
}

export function conflictCountText(count: number): string {
  return count > 0
    ? t("merge.conflictCount.some", { count })
    : t("merge.conflictCount.none");
}

export function mergeSideTitle(
  side: MergeSideName,
  role: MergeSideRole,
  name: string | null,
): string {
  const fallback = side === "ours" ? t("merge.sideTitle.ours") : t("merge.sideTitle.theirs");
  switch (role) {
    case "ours":
    case "theirs":
      return fallback;
    case "head":
      return t("merge.sideTitle.headRole", { fallback });
    case "currentBranch":
      return roleTitle(t("merge.sideTitle.roles.currentBranch"), fallback, name);
    case "mergedIn":
      return roleTitle(t("merge.sideTitle.roles.mergedIn"), fallback, name);
    case "upstream":
      return roleTitle(t("merge.sideTitle.roles.upstream"), fallback, name);
    case "yourCommit":
      return roleTitle(t("merge.sideTitle.roles.yourCommit"), fallback, name);
    case "appliedCommit":
      return roleTitle(t("merge.sideTitle.roles.appliedCommit"), fallback, name);
  }
}

function roleTitle(roleName: string, fallback: string, name: string | null): string {
  return name
    ? t("merge.sideTitle.withRoleAndName", { fallback, roleName, name })
    : t("merge.sideTitle.withRole", { fallback, roleName });
}

export function mergeConfirm(
  kind: "apply" | "cancel" | "ours" | "theirs",
  conflicts: number,
): { title: string; body: string; stay: string; go: string } {
  if (kind === "apply") {
    return {
      title: t("merge.confirm.apply.title"),
      body:
        conflicts > 0
          ? t("merge.confirm.apply.bodyConflicts", { conflicts })
          : t("merge.confirm.apply.bodyMarkers"),
      stay: t("merge.confirm.apply.stay"),
      go: t("merge.confirm.apply.go"),
    };
  }
  if (kind === "cancel") {
    return {
      title: t("merge.confirm.cancel.title"),
      body: t("merge.confirm.cancel.body"),
      stay: t("merge.confirm.cancel.stay"),
      go: t("merge.confirm.cancel.go"),
    };
  }
  if (kind === "ours") {
    return {
      title: t("merge.confirm.acceptOurs.title"),
      body: t("merge.confirm.acceptOurs.body"),
      stay: t("merge.confirm.acceptOurs.stay"),
      go: t("merge.confirm.acceptOurs.go"),
    };
  }
  return {
    title: t("merge.confirm.acceptTheirs.title"),
    body: t("merge.confirm.acceptTheirs.body"),
    stay: t("merge.confirm.acceptTheirs.stay"),
    go: t("merge.confirm.acceptTheirs.go"),
  };
}

export function mergeAcceptSideState(
  side: MergeSideName,
  state: ConflictSideState,
): string {
  if (side === "ours") {
    return state === "deleted"
      ? t("merge.acceptSide.oursDeleted")
      : t("merge.acceptSide.ours");
  }
  return state === "deleted"
    ? t("merge.acceptSide.theirsDeleted")
    : t("merge.acceptSide.theirs");
}

export function mergeUnavailableText(reason: MergeUnavailable): string {
  switch (reason) {
    case "bothDeleted":
      return t("merge.unavailable.bothDeleted");
    case "oursDeleted":
      return t("merge.unavailable.oursDeleted");
    case "theirsDeleted":
      return t("merge.unavailable.theirsDeleted");
    case "binary":
      return t("merge.unavailable.binary");
    case "symlink":
      return t("merge.unavailable.symlink");
    case "submodule":
      return t("merge.unavailable.submodule");
    case "inconsistent":
      return t("merge.unavailable.inconsistent");
  }
}

export function dateLabel(preset: DatePreset): string {
  switch (preset) {
    case "all":
      return t("log.dateLabels.all");
    case "last1":
      return t("log.dateLabels.last1");
    case "last7":
      return t("log.dateLabels.last7");
    case "last30":
      return t("log.dateLabels.last30");
    case "range":
      return t("log.dateLabels.range");
  }
}

export function columnLabel(column: "author" | "date" | "hash"): string {
  switch (column) {
    case "author":
      return t("log.columnLabels.author");
    case "date":
      return t("log.columnLabels.date");
    case "hash":
      return t("log.columnLabels.hash");
  }
}

export function highlightLevelText(level: DiffHighlight): string {
  switch (level) {
    case "word":
      return t("diff.highlightLevels.word");
    case "line":
      return t("diff.highlightLevels.line");
    case "char":
      return t("diff.highlightLevels.char");
    case "none":
      return t("diff.highlightLevels.none");
  }
}

export function notifyKindText(kind: "stage" | "discard" | "commit" | "recovery"): string {
  switch (kind) {
    case "stage":
      return t("settings.notifyKinds.stage");
    case "discard":
      return t("settings.notifyKinds.discard");
    case "commit":
      return t("settings.notifyKinds.commit");
    case "recovery":
      return t("settings.notifyKinds.recovery");
  }
}

export function operationKindText(kind: OperationKind): string {
  switch (kind) {
    case "merge":
      return t("operation.kinds.merge");
    case "rebase":
      return t("operation.kinds.rebase");
    case "cherryPick":
      return t("operation.kinds.cherryPick");
    case "revert":
      return t("operation.kinds.revert");
  }
}

/** 提交前警告：Rust 只给机器可读数据，这里按完整字面量键翻译。 */
export function commitWarningText(warning: CommitWarning): string {
  const detail = warning.detail;
  const path = warning.path ?? "";
  switch (detail.kind) {
    case "detachedHead":
      return t("commit.warnings.detachedHead");
    case "rebaseInProgress":
      return t("commit.warnings.rebaseInProgress");
    case "crlf":
      return t("commit.warnings.crlf", { path });
    case "largeFile":
      return t("commit.warnings.largeFile", {
        path,
        sizeMb: detail.sizeMb,
        limitMb: detail.limitMb,
      });
    case "invalidFilename":
      return t("commit.warnings.invalidFilename", {
        path,
        reason: invalidFilenameReasonText(detail.reason),
      });
  }
}

function invalidFilenameReasonText(reason: InvalidFilenameReason): string {
  switch (reason.kind) {
    case "character":
      return t("commit.warnings.invalidReason.character", { character: reason.character });
    case "controlCharacter":
      return t("commit.warnings.invalidReason.controlCharacter");
    case "trailingSpaceOrDot":
      return t("commit.warnings.invalidReason.trailingSpaceOrDot");
    case "reservedName":
      return t("commit.warnings.invalidReason.reservedName", { component: reason.component });
  }
}


export function checkoutNoteText(note: import("@/types/bindings/CheckoutNote").CheckoutNote): string {
  switch (note.kind) {
    case "existingTracking": return t("remote.notes.existingTracking", { remote: note.remote, branch: note.branch });
    case "createdTracking": return t("remote.notes.createdTracking", { remote: note.remote, branch: note.branch });
    case "stashRestoreConflicted": return t("remote.notes.stashRestoreConflicted");
  }
}

/** 更新通道未配置的原因（后端只给稳定的翻译键，Q168）。 */
export function updateReasonText(reason: string): string {
  switch (reason) {
    case "errors.m7.updateReasonNoPlugin":
      return t("errors.m7.updateReasonNoPlugin");
    case "errors.m7.updateReasonNoPubkey":
      return t("errors.m7.updateReasonNoPubkey");
    case "errors.m7.updateReasonNoEndpoints":
      return t("errors.m7.updateReasonNoEndpoints");
    default:
      return t("update.unconfigured");
  }
}
