import type { NoticeInput } from "@/hooks/useNotifications";
import type { AutostashLink } from "@/types/bindings/AutostashLink";
import type { RewriteOutcome } from "@/types/bindings/RewriteOutcome";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { formatAppError } from "@/lib/error";
import { messages, t } from "@/lib/i18n/index";
import { operationKindText } from "@/lib/i18n/text";

/** A receipt is enough to report saving, even if its pending link is gone. */
export function savedAutostashNotice(link: AutostashLink, action: NoticeInput["action"], removed = false): NoticeInput {
  const kind = link.operation === "rebase" ? "rebase" : link.operation === "revert" ? "revert" : "cherryPick";
  return {
    tone: "info", title: () => messages.operation.autostashSaved(operationKindText(kind)),
    detail: removed ? () => t("operation.autostashRestoredRemoved") : undefined,
    action,
  };
}

/** Called lazily so existing result receipts follow the selected language. */
export function rewriteOutcomeDetail(outcome: RewriteOutcome | null | undefined): string | undefined {
  if (!outcome) return undefined;
  const short = (oids: string[]) => oids.map(oid => oid.slice(0, 8)).join(", ");
  const parts = [
    outcome.applied.length ? messages.historyActions.appliedList(short(outcome.applied)) : "",
    outcome.skippedEmpty.length ? messages.historyActions.skippedList(short(outcome.skippedEmpty)) : "",
    outcome.skipped.length ? t("historyActions.skippedSelectionList", { oids: short(outcome.skipped) }) : "",
    outcome.failedOid ? messages.historyActions.failedList(outcome.failedOid.slice(0, 8)) : "",
    outcome.pending.length ? messages.historyActions.pendingList(short(outcome.pending)) : "",
  ].filter(Boolean);
  return parts.length ? parts.join("\n") : undefined;
}

/** A failed readback cannot erase the receipt already returned by the write. */
export function rewriteFailureNotice(outcome: RewriteOutcome, failure: AppErrorPayload): NoticeInput {
  return {
    tone: failure.kind === "needsVerification" ? "warning" : "error",
    title: () => formatAppError(failure), persistent: true,
    detail: () => {
      const parts = [rewriteOutcomeDetail(outcome), outcome.error ? formatAppError(outcome.error) : undefined].filter(Boolean);
      return parts.length ? parts.join("\n") : undefined;
    },
  };
}
