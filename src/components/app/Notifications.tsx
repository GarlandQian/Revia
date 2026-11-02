/**
 * 通知气泡（M4 F07、Q90、Q91、Q119）：窗口右下角。
 *
 * - 一次操作的结果（暂存、丢弃、提交、恢复）用通知展示；
 * - 成功的约 10 秒后自动消失，失败与结果待核对的一直保留到手动关闭；
 * - 提交输出可以展开、复制；提交前快照与 hooks 改动的说明也在这里。
 */

import { messages } from "@/lib/i18n/index";
import { useEffect, useRef, useState } from "react";
import { Codicon } from "@/components/ui/codicon";
import { GlassLayer } from "@/features/m7/glass/GlassLayer";

export type NoticeTone = "info" | "success" | "warning" | "error";
export type NoticeText = string | (() => string | null | undefined);

export interface NoticeView {
  id: string;
  tone: NoticeTone;
  title: NoticeText;
  detail?: NoticeText | null;
  action?: { label: NoticeText; run: () => void };
  /** 可展开的输出（例如 Git / hooks 输出）。 */
  output?: string[];
  /** 输出区标题。 */
  outputLabel?: string;
  /** 失败与待核对的通知不自动消失。 */
  persistent?: boolean;
  /** Frozen originating path, so late results remain identifiable after switching. */
  worktreePath?: string;
}

const TONE_ICON: Record<NoticeTone, string> = {
  info: "info",
  success: "pass",
  warning: "warning",
  error: "error",
};

const TONE_COLOR: Record<NoticeTone, string> = {
  info: "var(--editorinfo-foreground)",
  success: "var(--gitdecoration-addedresourceforeground)",
  warning: "var(--editorwarning-foreground)",
  error: "var(--gitdecoration-conflictingresourceforeground)",
};

/** 单条通知卡片；成功/信息类在 10 秒后自动关闭。 */
export function NotificationCard({
  notice,
  onClose,
}: {
  notice: NoticeView;
  onClose: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const output = notice.output ?? [];
  const title = typeof notice.title === "function" ? notice.title() : notice.title;
  const detail = typeof notice.detail === "function" ? notice.detail() : notice.detail;
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    if (notice.persistent) return;
    // 只按通知身份与持久性计时：父组件因其它状态重渲染时不重置 10 秒。
    const timer = window.setTimeout(() => closeRef.current(), 10_000);
    return () => window.clearTimeout(timer);
  }, [notice.id, notice.persistent]);
  const copy = () => {
    void navigator.clipboard.writeText(output.join("\n")).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    });
  };
  return (
    <article
      role={notice.tone === "error" ? "alert" : "status"}
      className="notification glass-isolate pointer-events-auto flex gap-2 p-3"
      data-tone={notice.tone}
    >
      <GlassLayer variant="popover" />
      <Codicon
        name={TONE_ICON[notice.tone]}
        size={16}
        className="mt-0.5"
        style={{ color: TONE_COLOR[notice.tone] }}
      />
      <div className="min-w-0 flex-1">
        {notice.worktreePath ? <p className="mb-1 truncate text-[11px] text-descriptionforeground" title={notice.worktreePath}>{notice.worktreePath}</p> : null}
        <div className="flex items-start gap-2">
          <p className="min-w-0 flex-1 text-[12.5px] font-semibold break-words">{title}</p>
          <button
            type="button"
            className="icon-action size-5 shrink-0"
            aria-label={messages.notices.close}
            title={messages.notices.close}
            onClick={onClose}
          >
            <Codicon name="close" size={14} />
          </button>
        </div>
        {detail ? (
          <p className="mt-1 text-[12px] leading-relaxed break-words whitespace-pre-wrap text-descriptionforeground">
            {detail}
          </p>
        ) : null}
        {notice.action ? (
          <button type="button" className="banner-link mt-1 text-[12px]" onClick={notice.action.run}>
            {typeof notice.action.label === "function" ? notice.action.label() : notice.action.label}
          </button>
        ) : null}
        {output.length > 0 ? (
          <div className="mt-1.5">
            <div className="flex items-center gap-3 text-[11.5px]">
              <button type="button" className="banner-link" onClick={() => setExpanded((value) => !value)}>
                {expanded ? messages.notices.collapseOutput : messages.notices.expandOutput(output.length)}
              </button>
              <button type="button" className="banner-link" onClick={copy}>
                {copied ? messages.notices.copied : messages.notices.copyOutput}
              </button>
            </div>
            {expanded ? (
              <pre
                data-commit-output
                className="mt-1 max-h-52 overflow-auto rounded bg-muted/50 p-2 font-mono text-[11px] leading-relaxed whitespace-pre-wrap break-words"
              >
                {output.join("\n")}
              </pre>
            ) : null}
          </div>
        ) : null}
      </div>
    </article>
  );
}

/** 右下角通知栈；没有通知时不占位。 */
export function NotificationStack({
  notices,
  onDismiss,
}: {
  notices: NoticeView[];
  onDismiss: (id: string) => void;
}) {
  if (notices.length === 0) return null;
  return (
    <div className="pointer-events-none fixed right-3 bottom-7 z-[60] flex flex-col gap-2">
      {notices.map((notice) => (
        <NotificationCard key={notice.id} notice={notice} onClose={() => onDismiss(notice.id)} />
      ))}
    </div>
  );
}
