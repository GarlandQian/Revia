/**
 * 提交区（M3 F01–F06，Q107 岛屿布局）。
 *
 * 放在现有变更工具窗口下方：不新建网站式布局，沿用既有 Inter / JetBrains Mono
 * 与主题变量。组件只负责“显示与输入”，所有判断（门禁、预期、指纹、写门）
 * 都由 App 与 Rust 层决定：
 * - 提交按钮的可用性来自门禁摘要 + 输入，但真正执行时后端会重新校验（F01.3）；
 * - 审阅进度只显示，不阻止提交（F06.5）；
 * - 常驻提示条说明进行中操作与需到终端处理（F01.2）。
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, GitCommitHorizontal, Loader2, TriangleAlert } from "lucide-react";
import { Button } from "@/components/ui/button";
import { messages } from "@/lib/i18n/zh-CN";
import { COMMIT_SHORTCUT_LABEL, isCommitShortcut } from "@/lib/platform";
import type { CommitContext } from "@/types/bindings/CommitContext";
import type { CommitHookChanges } from "@/types/bindings/CommitHookChanges";
import type { CommitOutcome } from "@/types/bindings/CommitOutcome";

/** 首行字符数建议上限（按字符计数，不按 UTF-8 字节，F02.2）。 */
export const FIRST_LINE_LIMIT = 72;

/** 首行字符数：只数首行，按 Unicode 码点计数。 */
export function firstLineLength(message: string): number {
  const newline = message.indexOf("\n");
  const firstLine = newline === -1 ? message : message.slice(0, newline);
  return Array.from(firstLine).length;
}

/** 输入是否可以提交（空白信息不能提交，F02.2）。 */
export function messageSubmittable(message: string): boolean {
  return message.trim().length > 0;
}

/** 门禁提示：进行中操作的中文说明（F01.2）。 */
export function operationHint(context: CommitContext): string | null {
  const gates = context.gates;
  if (gates.operationKind === null && !gates.hasConflicts) return null;
  const base = (() => {
    switch (gates.operationKind) {
      case "merge":
        return messages.commit.operationMerge;
      case "rebase":
        return messages.commit.operationRebase;
      case "cherryPick":
        return messages.commit.operationCherryPick;
      case "revert":
        return messages.commit.operationRevert;
      case null:
        return "";
    }
  })();
  const sequencer = gates.operationSequencerPending ? messages.commit.operationSequencer : "";
  const conflicts = gates.hasConflicts
    ? messages.commit.conflictBlocked(gates.conflictCount)
    : "";
  return [base + sequencer, conflicts, messages.commit.operationTerminal]
    .filter((part) => part.length > 0)
    .join(" ");
}

/** 提交结果面板的数据（成功/失败/待核对都保留在发起它的 worktree 上）。 */
export interface CommitResultView {
  worktreeId: string;
  /** 结果的标题文案。 */
  title: string;
  /** 结果正文（错误信息或成功说明）。 */
  detail: string | null;
  outcome: CommitOutcome | null;
  /** 实时输出（可能来自事件流，也可能是结果里的聚合尾部）。 */
  output: string[];
  /** 是否为待核对结果。 */
  needsVerification: boolean;
  /** hooks 修改过的文件（成功或失败后重新读取的真实对照）。 */
  hookChanges: CommitHookChanges | null;
  /** 本次提交前保存的快照记录 ID。 */
  snapshotRecordId: string | null;
}

/** 提交结果区：可展开、可复制的 Git 输出（F04.1、F06.4）。 */
export function CommitResultPanel({
  result,
  expanded,
  onToggle,
  onCopy,
  copied,
  onClose,
  activeWorktreeId,
}: {
  result: CommitResultView;
  expanded: boolean;
  onToggle: () => void;
  onCopy: () => void;
  copied: boolean;
  onClose: () => void;
  activeWorktreeId: string | null;
}) {
  const other = activeWorktreeId !== null && activeWorktreeId !== result.worktreeId;
  const output = result.output.join("\n");
  return (
    <section
      role={result.needsVerification ? "alert" : "status"}
      aria-label={messages.commit.resultTitle}
      data-commit-result
      className="mt-1.5 rounded border border-edge bg-island/60 p-2 text-xs"
    >
      <header className="flex items-center gap-1.5">
        {result.needsVerification ? (
          <TriangleAlert aria-hidden className="size-3.5 shrink-0 text-[var(--banner-warn-icon)]" strokeWidth={1.8} />
        ) : (
          <GitCommitHorizontal aria-hidden className="size-3.5 shrink-0 text-icon" strokeWidth={1.8} />
        )}
        <span className="min-w-0 flex-1 truncate font-semibold" title={result.title}>{result.title}</span>
        <button type="button" className="banner-link" onClick={onCopy}>
          {copied ? messages.commit.resultCopied : messages.commit.resultCopy}
        </button>
        <button type="button" className="banner-link" onClick={onToggle}>
          {expanded ? messages.commit.resultCollapse : messages.commit.resultExpand}
        </button>
        <button type="button" className="banner-link" onClick={onClose}>
          {messages.commit.resultClose}
        </button>
      </header>
      {result.detail ? (
        <p className="mt-1 whitespace-pre-wrap break-words text-fg-2">{result.detail}</p>
      ) : null}
      {result.outcome?.error ? (
        <p className="mt-1 whitespace-pre-wrap break-words text-fg-2">{result.outcome.error.message}</p>
      ) : null}
      {other ? <p className="mt-1 text-fg-3">{messages.commit.outputOtherWorktree}</p> : null}
      {result.hookChanges
        && (result.hookChanges.worktree.length > 0 || result.hookChanges.index.length > 0) ? (
        <p className="mt-1 whitespace-pre-wrap break-words text-fg-2">
          {messages.commit.resultHookChanges(result.hookChanges.worktree, result.hookChanges.index)}
        </p>
      ) : null}
      {result.snapshotRecordId ? (
        <p className="mt-1 break-all font-mono text-fg-3">
          {messages.commit.resultSnapshot(result.snapshotRecordId)}
        </p>
      ) : null}
      {result.outcome && !result.outcome.draftCleared && result.outcome.success ? (
        <p className="mt-1 text-fg-3">{messages.commit.resultDraftKept}</p>
      ) : null}
      {result.outcome?.draftError ? (
        <p className="mt-1 text-[var(--banner-warn-icon)]">
          {messages.commit.resultDraftError(result.outcome.draftError)}
        </p>
      ) : null}
      <p className="mt-1 flex items-center gap-2 text-fg-3">
        <span>{messages.commit.resultLines(result.output.length)}</span>
        {result.output.length === 0 ? <span>{messages.commit.outputWaiting}</span> : null}
      </p>
      {expanded ? (
        output.length > 0 ? (
          <div>
            <p className="mt-1 text-fg-3">{messages.commit.resultOutput}</p>
            <pre
              data-commit-output
              className="mt-1 max-h-56 overflow-auto whitespace-pre-wrap break-words rounded bg-muted/40 p-2 font-mono text-[11.5px] leading-relaxed"
            >
              {output}
            </pre>
          </div>
        ) : (
          <p className="mt-1 text-fg-3">{messages.commit.resultNoOutput}</p>
        )
      ) : null}
    </section>
  );
}

/** 提交区主体：信息框、amend、跳过 hooks、模板与最近信息、门禁提示、提交按钮。 */
export function CommitComposer({
  context,
  message,
  amend,
  skipHooks,
  busy,
  submitting,
  reviewedCount,
  totalFiles,
  result,
  resultExpanded,
  copied,
  onMessage,
  onAmend,
  onSkipHooks,
  onSubmit,
  onPickRecent,
  onToggleResult,
  onCopyResult,
  onCloseResult,
  activeWorktreeId,
  draftNotice,
  templateError,
  amendConfirm,
  onAmendConfirm,
  onAmendCancel,
}: {
  context: CommitContext | null;
  message: string;
  amend: boolean;
  skipHooks: boolean;
  busy: boolean;
  submitting: boolean;
  reviewedCount: number;
  totalFiles: number;
  result: CommitResultView | null;
  resultExpanded: boolean;
  copied: boolean;
  onMessage: (value: string) => void;
  onAmend: (value: boolean) => void;
  onSkipHooks: (value: boolean) => void;
  onSubmit: () => void;
  onPickRecent: (value: string) => void;
  onToggleResult: () => void;
  onCopyResult: () => void;
  onCloseResult: () => void;
  activeWorktreeId: string | null;
  /** 草稿相关提示（恢复成功、保存失败等），持久化失败必须可见（F02.5）。 */
  draftNotice: string | null;
  /** 模板读取失败的原因：可见但不阻断提交（F02.3）。 */
  templateError: string | null;
  /** 需要二次确认的 amend（HEAD 已包含于本地上游，F06.2）。 */
  amendConfirm: { upstreamName: string } | null;
  onAmendConfirm: () => void;
  onAmendCancel: () => void;
}) {
  const [recentOpen, setRecentOpen] = useState(false);
  const firstLine = firstLineLength(message);
  const overLimit = firstLine > FIRST_LINE_LIMIT;
  const submittable = messageSubmittable(message);
  const gates = context?.gates ?? null;
  const hint = context ? operationHint(context) : null;
  // 硬门禁：未解决冲突或禁止提交的进行中操作（F01.2）。
  const blockedByOperation = gates !== null && (gates.hasConflicts || gates.operationBlocksCommit);
  // 无冲突 merge 收尾允许「index 与 HEAD 相同」的普通提交（F03.4）。
  const hasCommittableContent = gates !== null
    && (gates.hasStagedChanges || (!amend && gates.allowsEmptyDiffCommit));
  const blockedByInput = !submittable || (gates !== null && !hasCommittableContent && !amend);
  const amendBlocked = gates !== null && !gates.amendAvailable;
  const disabled = context === null || submitting || busy || blockedByOperation || blockedByInput || (amend && amendBlocked);
  const recent = context?.recentMessages ?? [];

  return (
    <section
      aria-label={messages.commit.section}
      data-commit-composer
      className="shrink-0 border-t border-border px-2.5 pt-2 pb-2"
    >
      {templateError ? (
        <p role="status" className="mb-1.5 text-[11.5px] leading-relaxed text-[var(--banner-warn-icon)]">
          {messages.commit.templateFailed(templateError)}
        </p>
      ) : null}
      {hint ? (
        <p role="status" className="mb-1.5 flex items-start gap-1.5 text-[11.5px] leading-relaxed text-[var(--banner-warn-icon)]">
          <TriangleAlert aria-hidden className="mt-0.5 size-3.5 shrink-0" strokeWidth={1.8} />
          <span className="min-w-0">{hint}</span>
        </p>
      ) : null}
      <label className="sr-only" htmlFor="commit-message">{messages.commit.messageLabel}</label>
      <textarea
        id="commit-message"
        data-commit-message
        className="commit-message"
        rows={4}
        spellCheck={false}
        placeholder={messages.commit.placeholder}
        value={message}
        disabled={submitting}
        onChange={(event) => onMessage(event.target.value)}
        onKeyDown={(event) => {
          // 信息框内的 ⌘K / Ctrl+K 直接提交；不穿透到全局快捷键（F06.5）。
          if (isCommitShortcut(event)) {
            event.preventDefault();
            if (!disabled && !document.querySelector("[role='dialog'], dialog[open]")) onSubmit();
          }
        }}
      />
      <div className="mt-1 flex items-center gap-2 text-[11px] text-fg-3">
        <span className={overLimit ? "text-[var(--banner-warn-icon)]" : undefined}>
          {overLimit ? messages.commit.firstLineHint(firstLine) : `${firstLine} / ${FIRST_LINE_LIMIT}`}
        </span>
        {draftNotice ? (
          <span role="status" className="min-w-0 flex-1 truncate" title={draftNotice}>{draftNotice}</span>
        ) : null}
      </div>
      <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11.5px]">
        <label className="inline-flex items-center gap-1.5" title={amendBlocked
          ? (gates?.operationBlocksAmend ? messages.commit.amendUnavailableOperation
            : gates?.amendAvailable === false && !gates.hasConflicts
              ? messages.commit.amendUnavailableNoHead
              : messages.commit.amendUnavailable)
          : messages.commit.amend}>
          <input
            type="checkbox"
            checked={amend}
            disabled={amendBlocked || submitting}
            onChange={(event) => onAmend(event.target.checked)}
          />
          {messages.commit.amend}
        </label>
        <label className="inline-flex items-center gap-1.5" title={messages.commit.skipHooksDetail}>
          <input
            type="checkbox"
            checked={skipHooks}
            disabled={submitting}
            onChange={(event) => onSkipHooks(event.target.checked)}
          />
          {messages.commit.skipHooks}
        </label>
        {skipHooks ? (
          <span className="text-[11px] text-fg-3">{messages.commit.skipHooksOnce}</span>
        ) : null}
        <span className="flex-1" />
        {context?.upstream ? <UpstreamNote context={context} /> : null}
      </div>
      <div className="mt-1.5 flex items-center gap-2">
        {recent.length > 0 ? (
          <div className="relative">
            <button
              type="button"
              className="banner-link inline-flex items-center gap-0.5"
              aria-expanded={recentOpen}
              onClick={() => setRecentOpen((value) => !value)}
            >
              {messages.commit.recent}
              <ChevronDown aria-hidden className="size-3" strokeWidth={2} />
            </button>
            {recentOpen ? (
              <ul
                role="listbox"
                aria-label={messages.commit.recent}
                className="absolute bottom-6 left-0 z-10 max-h-56 w-[min(420px,60vw)] overflow-y-auto rounded border border-edge bg-popover p-1 shadow-xl"
              >
                {recent.map((value, index) => (
                  <li key={`${index}-${value.slice(0, 24)}`}>
                    <button
                      type="button"
                      role="option"
                      aria-selected={false}
                      title={messages.commit.recentPick}
                      className="w-full rounded px-2 py-1 text-left font-mono text-[11.5px] whitespace-pre-wrap break-words hover:bg-hover"
                      onClick={() => {
                        setRecentOpen(false);
                        onPickRecent(value);
                      }}
                    >
                      {value.split("\n")[0]}
                    </button>
                  </li>
                ))}
              </ul>
            ) : null}
          </div>
        ) : (
          <span className="text-[11px] text-fg-3">{messages.commit.recentEmpty}</span>
        )}
        <span className="flex-1" />
        <span
          className="text-[11px] whitespace-nowrap text-fg-2 tabular-nums"
          title={messages.commit.progressDetail}
        >
          {messages.commit.progress(reviewedCount, totalFiles)}
        </span>
        <Button
          size="sm"
          disabled={disabled}
          title={messages.commit.submitHint}
          onClick={onSubmit}
        >
          {submitting ? <Loader2 aria-hidden className="animate-spin" /> : <GitCommitHorizontal aria-hidden />}
          {submitting ? messages.commit.submitting : messages.commit.submit}
          {!submitting ? <span className="ml-0.5 text-[11px] text-primary-foreground/70">{COMMIT_SHORTCUT_LABEL}</span> : null}
        </Button>
      </div>
      {!submittable ? (
        <p className="mt-1 text-[11px] text-fg-3">{messages.commit.emptyMessage}</p>
      ) : blockedByInput ? (
        <p className="mt-1 text-[11px] text-fg-3">
          {amend ? messages.commit.amendUnavailable : messages.commit.noStaged}
        </p>
      ) : context && !context.gates.hasStagedChanges
        && context.expectation.index.some((entry) => entry.intentToAdd) ? (
        <p className="mt-1 text-[11px] text-fg-3">{messages.commit.intentOnly}</p>
      ) : submitting ? (
        <p className="mt-1 text-[11px] text-fg-3">{messages.commit.blockedWriting}</p>
      ) : null}
      {amendConfirm ? (
        <AmendConfirm
          upstreamName={amendConfirm.upstreamName}
          unknown={context?.upstream === "unknown"}
          onConfirm={onAmendConfirm}
          onCancel={onAmendCancel}
        />
      ) : null}
      {result ? (
        <CommitResultPanel
          result={result}
          expanded={resultExpanded}
          copied={copied}
          onToggle={onToggleResult}
          onCopy={onCopyResult}
          onClose={onCloseResult}
          activeWorktreeId={activeWorktreeId}
        />
      ) : null}
    </section>
  );
}

/** 上游状态提示：只按本地记录，不联网（F06.3）。 */
function UpstreamNote({ context }: { context: CommitContext }) {
  const note = useMemo(() => {
    switch (context.upstream) {
      case "contained":
        return messages.commit.upstreamWarning(context.upstreamName ?? "");
      case "notContained":
        return messages.commit.upstreamBoundary;
      case "unknown":
        return messages.commit.upstreamUnknown;
      case "noUpstream":
        return messages.commit.upstreamNone;
    }
  }, [context.upstream, context.upstreamName]);
  const warn = context.upstream === "contained";
  return (
    <span
      className={`min-w-0 max-w-[60%] truncate text-[11px] ${warn ? "text-[var(--banner-warn-icon)]" : "text-fg-3"}`}
      title={note}
    >
      {note}
    </span>
  );
}

/** amend 二次确认：绑定 worktree/HEAD/暂存区/信息/选项，任何变化都使旧确认失效（F06.2）。 */
function AmendConfirm({
  upstreamName,
  unknown,
  onConfirm,
  onCancel,
}: {
  upstreamName: string;
  unknown: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const title = unknown ? messages.commit.amendUnknownConfirmTitle : messages.commit.amendConfirmTitle;
  const body = unknown ? messages.commit.amendUnknownConfirmBody : messages.commit.amendConfirmBody(upstreamName);
  useEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>("button")?.focus();
  }, []);
  return (
    <div
      ref={ref}
      role="dialog"
      aria-label={title}
      className="mt-1.5 rounded border border-[var(--banner-warn-icon)]/40 p-2 text-xs"
    >
      <p className="font-semibold">{title}</p>
      <p className="mt-1 text-fg-2">{body}</p>
      <p className="mt-1 text-fg-3">{messages.commit.upstreamBoundary}</p>
      <div className="mt-2 flex justify-end gap-2">
        <Button variant="outline" size="sm" onClick={onCancel}>{messages.recovery.cancel}</Button>
        <Button variant="destructive" size="sm" onClick={onConfirm}>{messages.commit.amendConfirmAction}</Button>
      </div>
    </div>
  );
}
