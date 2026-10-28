/**
 * 提交区（M4 F04、M3 F01–F06）。
 *
 * 位于“提交”页签底部：修改上一次提交、提交信息框、提交按钮与提交选项、审阅进度。
 * 组件只负责显示与输入；门禁、预期、指纹、写门都在 App 与 Rust 层。
 * 提交结果不在这里展开，改由右下角通知显示（F04.3、F07）。
 */

import { useEffect, useMemo, useRef, useState } from "react";
import type { CommitContext } from "@/types/bindings/CommitContext";
import type { CommitHookChanges } from "@/types/bindings/CommitHookChanges";
import type { CommitOutcome } from "@/types/bindings/CommitOutcome";
import { messages } from "@/lib/i18n/zh-CN";
import { COMMIT_SHORTCUT_LABEL, isCommitShortcut } from "@/lib/platform";
import { Codicon } from "@/components/ui/codicon";
import { Popover } from "@/components/ui/popover";

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
  const conflicts = gates.hasConflicts ? messages.commit.conflictBlocked(gates.conflictCount) : "";
  return `${[base + sequencer, conflicts, messages.commit.operationTerminal]
    .filter((part) => part.length > 0)
    .join("。")}。`;
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
  /** 快照记录保存的文件路径。 */
  snapshotPaths: string[];
}

export function CommitArea({
  context,
  message,
  amend,
  skipHooks,
  busy,
  submitting,
  reviewedCount,
  totalFiles,
  draftNotice,
  templateError,
  amendConfirm,
  liveOutput,
  checks,
  onMessage,
  onAmend,
  onSkipHooks,
  onSubmit,
  onPickRecent,
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
  draftNotice: string | null;
  templateError: string | null;
  amendConfirm: { upstreamName: string } | null;
  /** 提交进行中的实时输出（返修 C8，F04.3）：可展开，显示在提交区。 */
  liveOutput: string[] | null;
  /** 提交信息检查（返修 E.3，Q139）：只标出问题，不阻止提交。 */
  checks: CommitMessageChecks;
  onMessage: (value: string) => void;
  onAmend: (value: boolean) => void;
  onSkipHooks: (value: boolean) => void;
  onSubmit: () => void;
  onPickRecent: (value: string) => void;
  onAmendConfirm: () => void;
  onAmendCancel: () => void;
}) {
  const [recentOpen, setRecentOpen] = useState(false);
  const [optionsOpen, setOptionsOpen] = useState(false);
  const firstLine = firstLineLength(message);
  const subjectLimit = checks.subjectLimit;
  const overLimit = subjectLimit > 0 && firstLine > subjectLimit;
  const issues = commitMessageIssues(message, checks);
  const submittable = messageSubmittable(message);
  const gates = context?.gates ?? null;
  const hint = context ? operationHint(context) : null;
  const blockedByOperation = gates !== null && (gates.hasConflicts || gates.operationBlocksCommit);
  const hasCommittableContent = gates !== null
    && (gates.hasStagedChanges || (!amend && gates.allowsEmptyDiffCommit));
  const blockedByInput = !submittable || (gates !== null && !hasCommittableContent && !amend);
  const amendBlocked = gates !== null && !gates.amendAvailable;
  const disabled = context === null || submitting || busy || blockedByOperation || blockedByInput || (amend && amendBlocked);
  const recent = context?.recentMessages ?? [];
  const upstream = context ? upstreamNote(context) : null;

  return (
    <section aria-label={messages.commit.section} data-commit-composer className="commit-composer shrink-0 border-t border-panel-border px-3 pt-2 pb-2.5">
      {templateError ? (
        <p role="status" className="mb-1.5 text-[11.5px] leading-relaxed text-[var(--editorwarning-foreground)]">
          {messages.commit.templateFailed(templateError)}
        </p>
      ) : null}
      {hint ? (
        <p role="status" className="mb-1.5 flex items-start gap-1.5 text-[11.5px] leading-relaxed text-[var(--editorwarning-foreground)]">
          <Codicon name="warning" size={14} className="mt-0.5" />
          <span className="min-w-0">{hint}</span>
        </p>
      ) : null}

      <div className="mb-1.5 flex items-center gap-2 text-[12.5px]">
        <label
          className="inline-flex min-w-0 items-center gap-1.5"
          title={
            amendBlocked
              ? gates?.operationBlocksAmend
                ? messages.commit.amendUnavailableOperation
                : gates?.amendAvailable === false && !gates.hasConflicts
                  ? messages.commit.amendUnavailableNoHead
                  : messages.commit.amendUnavailable
              : messages.commit.amend
          }
        >
          <input
            type="checkbox"
            className="size-3.5 accent-[var(--checkbox-border)]"
            checked={amend}
            disabled={amendBlocked || submitting}
            onChange={(event) => onAmend(event.target.checked)}
          />
          <span className="truncate">{messages.commit.amend}</span>
        </label>
        <span className="flex-1" />
        <div className="relative">
          <Popover
            open={recentOpen}
            onOpen={() => setRecentOpen(true)}
            onClose={() => setRecentOpen(false)}
            align="right"
            width={380}
            trigger={
              <button
                type="button"
                className="icon-action size-5"
                title={messages.commit.recent}
                aria-label={messages.commit.recent}
                disabled={recent.length === 0}
              >
                <Codicon name="history" size={15} />
              </button>
            }
          >
            <p className="px-2 py-1 text-[11px] text-descriptionforeground">{messages.commit.recent}</p>
            {recent.length === 0 ? (
              <p className="px-2 py-1 text-descriptionforeground">{messages.commit.recentEmpty}</p>
            ) : (
              recent.map((value, index) => (
                <button
                  key={`${index}-${value.slice(0, 24)}`}
                  type="button"
                  role="menuitem"
                  title={messages.commit.recentPick}
                  className="w-full rounded-[3px] px-2 py-1 text-left font-mono text-[11.5px] whitespace-pre-wrap break-words hover:bg-toolbar-hoverbackground"
                  onClick={() => {
                    setRecentOpen(false);
                    onPickRecent(value);
                  }}
                >
                  {value.split("\n")[0]}
                </button>
              ))
            )}
          </Popover>
        </div>
      </div>

      <label className="sr-only" htmlFor="commit-message">
        {messages.commit.messageLabel}
      </label>
      <textarea
        id="commit-message"
        data-commit-message
        className="commit-message"
        style={
          checks.showRuler && subjectLimit > 0
            ? { backgroundImage: `linear-gradient(to right, transparent calc(${subjectLimit}ch + 6px), var(--panel-border) calc(${subjectLimit}ch + 6px), var(--panel-border) calc(${subjectLimit}ch + 7px), transparent calc(${subjectLimit}ch + 7px))` }
            : undefined
        }
        rows={4}
        spellCheck={false}
        placeholder={messages.commit.placeholder}
        value={message}
        disabled={submitting}
        onChange={(event) => onMessage(event.target.value)}
        onKeyDown={(event) => {
          if (isCommitShortcut(event)) {
            event.preventDefault();
            if (!disabled && !document.querySelector("[role='dialog'], dialog[open]")) onSubmit();
          }
        }}
      />

      <div className="mt-1 flex items-center gap-2 text-[11px] text-[color:color-mix(in_srgb,var(--descriptionforeground)_55%,var(--editor-background))]">
        <span className={overLimit ? "text-[var(--editorwarning-foreground)]" : undefined}>
          {subjectLimit > 0
            ? overLimit
              ? messages.commit.firstLineHint(firstLine)
              : `${firstLine} / ${subjectLimit}`
            : `${firstLine}`}
        </span>
        {draftNotice ? (
          <span role="status" className="min-w-0 flex-1 truncate" title={draftNotice}>
            {draftNotice}
          </span>
        ) : null}
      </div>

      {issues.length > 0 ? (
        <ul data-commit-issues className="mt-1 space-y-0.5 text-[11px] leading-relaxed text-[var(--editorwarning-foreground)]">
          {issues.map((issue, index) => (
            <li key={index} className="flex items-start gap-1">
              <Codicon name="warning" size={11} className="mt-0.5 shrink-0" />
              <span className="min-w-0">{issue}</span>
            </li>
          ))}
        </ul>
      ) : null}

      <div className="mt-2 flex items-center gap-1.5">
        <button
          type="button"
          className="inline-flex h-7 flex-1 items-center justify-center gap-1.5 rounded-[4px] bg-primary text-[12.5px] font-medium text-primary-foreground outline-none hover:bg-primary/90 focus-visible:outline-1 focus-visible:outline-offset-1 focus-visible:outline-[var(--focusborder)] disabled:cursor-default disabled:opacity-40"
          disabled={disabled}
          title={messages.commit.submitHint}
          onClick={onSubmit}
        >
          {submitting ? <Codicon name="loading" size={14} spin /> : <Codicon name="check" size={14} />}
          {submitting ? messages.commit.submitting : messages.commit.submit}
          {!submitting ? <span className="text-[11px] opacity-70">{COMMIT_SHORTCUT_LABEL}</span> : null}
        </button>
        <div className="relative">
          <Popover
            open={optionsOpen}
            onOpen={() => setOptionsOpen(true)}
            onClose={() => setOptionsOpen(false)}
            align="right"
            width={260}
            trigger={
              <button
                type="button"
                className="icon-action size-7"
                title={messages.commit.options}
                aria-label={messages.commit.options}
              >
                <Codicon name="gear" size={15} />
              </button>
            }
          >
            <label className="flex items-start gap-2 rounded-[3px] px-2 py-1.5 text-[12px] hover:bg-toolbar-hoverbackground">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={skipHooks}
                disabled={submitting}
                onChange={(event) => onSkipHooks(event.target.checked)}
              />
              <span className="min-w-0">
                <span className="block">{messages.commit.skipHooks}</span>
                <span className="block text-[11px] text-descriptionforeground">{messages.commit.skipHooksDetail}</span>
                {skipHooks ? (
                  <span className="mt-0.5 block text-[11px] text-descriptionforeground">{messages.commit.skipHooksOnce}</span>
                ) : null}
              </span>
            </label>
          </Popover>
        </div>
      </div>

      <div className="mt-1.5 flex items-center gap-1.5 text-[11.5px] text-descriptionforeground" title={messages.commit.progressDetail}>
        <Codicon name={submitting ? "loading" : "pass"} size={13} className={submitting ? "animate-spin" : undefined} />
        <span className="tabular-nums">{submitting ? messages.commit.running : messages.commit.progress(reviewedCount, totalFiles)}</span>
      </div>

      {liveOutput !== null ? <LiveCommitOutput lines={liveOutput} /> : null}

      {upstream ? (
        <p
          className={`mt-1 text-[11px] leading-relaxed ${upstream.warn ? "text-[var(--editorwarning-foreground)]" : "text-[color:color-mix(in_srgb,var(--descriptionforeground)_55%,var(--editor-background))]"}`}
          title={upstream.text}
        >
          {upstream.text}
        </p>
      ) : null}

      {!submittable ? (
        <p className="mt-1 text-[11px] text-[color:color-mix(in_srgb,var(--descriptionforeground)_55%,var(--editor-background))]">{messages.commit.emptyMessage}</p>
      ) : blockedByInput ? (
        <p className="mt-1 text-[11px] text-[color:color-mix(in_srgb,var(--descriptionforeground)_55%,var(--editor-background))]">{amend ? messages.commit.amendUnavailable : messages.commit.noStaged}</p>
      ) : context && !context.gates.hasStagedChanges
        && context.expectation.index.some((entry) => entry.intentToAdd) ? (
        <p className="mt-1 text-[11px] text-[color:color-mix(in_srgb,var(--descriptionforeground)_55%,var(--editor-background))]">{messages.commit.intentOnly}</p>
      ) : submitting ? (
        <p className="mt-1 text-[11px] text-[color:color-mix(in_srgb,var(--descriptionforeground)_55%,var(--editor-background))]">{messages.commit.blockedWriting}</p>
      ) : null}

      {amendConfirm ? (
        <AmendConfirm
          upstreamName={amendConfirm.upstreamName}
          unknown={context?.upstream === "unknown"}
          onConfirm={onAmendConfirm}
          onCancel={onAmendCancel}
        />
      ) : null}
    </section>
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
      className="mt-2 rounded border border-[var(--editorwarning-foreground)]/40 p-2 text-xs"
    >
      <p className="font-semibold">{title}</p>
      <p className="mt-1 text-descriptionforeground">{body}</p>
      <p className="mt-1 text-[color:color-mix(in_srgb,var(--descriptionforeground)_55%,var(--editor-background))]">{messages.commit.upstreamBoundary}</p>
      <div className="mt-2 flex justify-end gap-2">
        <button type="button" className="banner-link" onClick={onCancel}>
          {messages.recovery.cancel}
        </button>
        <button
          type="button"
          className="rounded-[3px] bg-destructive px-2 py-1 text-white"
          onClick={onConfirm}
        >
          {messages.commit.amendConfirmAction}
        </button>
      </div>
    </div>
  );
}

/** 上游状态提示：只按本地记录，不联网（F06.3）。 */
export function upstreamNote(context: CommitContext): { text: string; warn: boolean } {
  switch (context.upstream) {
    case "contained":
      return { text: messages.commit.upstreamWarning(context.upstreamName ?? ""), warn: true };
    case "notContained":
      return { text: messages.commit.upstreamBoundary, warn: false };
    case "unknown":
      return { text: messages.commit.upstreamUnknown, warn: false };
    case "noUpstream":
      return { text: messages.commit.upstreamNone, warn: false };
  }
}

export function useUpstreamNote(context: CommitContext | null) {
  return useMemo(() => (context ? upstreamNote(context) : null), [context]);
}

/**
 * 提交进行中的实时输出（返修 C8，F04.3）：显示在提交区，可展开、自动滚到底。
 * 完成后整个区域随 submitting 结束而消失，结果由右下角通知报告（F07）。
 */
function LiveCommitOutput({ lines }: { lines: string[] }) {
  const [expanded, setExpanded] = useState(true);
  const preRef = useRef<HTMLPreElement>(null);
  useEffect(() => {
    if (expanded) preRef.current?.scrollTo({ top: preRef.current.scrollHeight });
  }, [expanded, lines.length]);
  return (
    <div className="mt-1 rounded border border-panel-border text-[11px]" data-commit-live-output>
      <button
        type="button"
        className="flex w-full items-center gap-1.5 px-2 py-1 text-left text-descriptionforeground hover:bg-toolbar-hoverbackground"
        onClick={() => setExpanded((value) => !value)}
      >
        <Codicon name={expanded ? "chevron-down" : "chevron-right"} size={12} />
        {messages.commit.outputLive}
        {lines.length > 0 ? <span className="ml-auto tabular-nums">{lines.length}</span> : null}
      </button>
      {expanded ? (
        <pre
          ref={preRef}
          className="max-h-32 overflow-auto border-t border-panel-border bg-muted/40 px-2 py-1 font-mono leading-relaxed whitespace-pre-wrap break-words"
        >
          {lines.length > 0 ? lines.join("\n") : messages.commit.outputWaiting}
        </pre>
      ) : null}
    </div>
  );
}

/** 提交信息检查的设置（返修 E.3、Q139）。 */
export interface CommitMessageChecks {
  subjectLimit: number;
  blankLineRequired: boolean;
  bodyLineLimit: number;
  showRuler: boolean;
}

/**
 * 提交信息检查：只列出问题，不阻止提交（E.3）。
 * 首行上限 0 表示不检查；正文行上限 0 表示不检查。
 */
export function commitMessageIssues(message: string, checks: CommitMessageChecks): string[] {
  const issues: string[] = [];
  if (message.trim().length === 0) return issues;
  const lines = message.split("\n");
  const subject = lines[0] ?? "";
  if (checks.subjectLimit > 0 && Array.from(subject).length > checks.subjectLimit) {
    issues.push(messages.commit.subjectTooLong(Array.from(subject).length, checks.subjectLimit));
  }
  const hasBody = lines.slice(1).some((line) => line.trim().length > 0);
  if (checks.blankLineRequired && hasBody && (lines[1] ?? "").trim().length > 0) {
    issues.push(messages.commit.blankLineMissing);
  }
  if (checks.bodyLineLimit > 0) {
    const over = lines
      .slice(1)
      .findIndex((line) => Array.from(line).length > checks.bodyLineLimit);
    if (over >= 0) {
      issues.push(
        messages.commit.bodyLineTooLong(over + 2, Array.from(lines[over + 1]).length, checks.bodyLineLimit),
      );
    }
  }
  return issues;
}
