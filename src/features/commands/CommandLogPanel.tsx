/**
 * Git 命令日志面板（M5 B07、F04）。
 *
 * 底部“Git”工具窗口里的“控制台”：本次运行执行过的 Git 命令，显示时间、所属 worktree、
 * 命令与参数、退出码、耗时与输出（Rust 侧已打码，单条最多 64 KB）。只在内存里保留
 * 最近 1000 条；面板打开时轮询，随新命令更新。
 *
 * 与提交日志（`features/log`）完全不同：这里是 Revia 自己执行的 Git 命令。
 */

import { useEffect, useRef, useState } from "react";
import type { CommandRecordPayload } from "@/lib/ipc";
import { ipc } from "@/lib/ipc";
import { messages } from "@/lib/i18n/zh-CN";
import { Codicon } from "@/components/ui/codicon";

/** 面板打开时的刷新间隔（毫秒）。 */
export const COMMAND_LOG_POLL_MS = 800;

export function CommandLogPanel({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const [records, setRecords] = useState<CommandRecordPayload[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);
  const lastSequence = useRef(0);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    const poll = () => {
      void ipc
        .commandLog()
        .then((next) => {
          if (cancelled) return;
          const newest = next.length ? next[next.length - 1].sequence : 0;
          if (newest !== lastSequence.current) {
            lastSequence.current = newest;
            setRecords(next);
            setError(null);
          }
        })
        .catch((caught: unknown) => {
          if (!cancelled) setError(String((caught as { message?: string })?.message ?? caught));
        });
    };
    poll();
    const timer = window.setInterval(poll, COMMAND_LOG_POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [open]);

  const active = selected === null ? records[records.length - 1] ?? null : records.find((record) => record.sequence === selected) ?? null;

  const copy = (text: string) => {
    void navigator.clipboard?.writeText(text).catch(() => undefined);
  };

  /** 单条命令的完整文本（复制用）。 */
  const recordText = (record: CommandRecordPayload): string =>
    [
      `#${record.sequence} ${new Date(record.startedAtMs).toLocaleString()}`,
      `${record.program} ${record.args.join(" ")}`,
      `exit=${record.exitCode} time=${record.elapsedMs}ms${record.timedOut ? " (timeout)" : ""}`,
      record.directory,
      record.output,
    ].join("\n");

  if (!open) return null;

  return (
    <section
      aria-label={messages.commandLog.title}
      className="flex h-[260px] shrink-0 flex-col overflow-hidden border-t border-panel-border bg-[var(--panel-background)]"
    >
      <div className="flex h-[26px] shrink-0 items-center gap-2 border-b border-panel-border px-2 text-[11.5px]">
        <Codicon name="terminal" size={13} />
        <span className="font-medium">{messages.commandLog.title}</span>
        <span className="text-descriptionforeground">{messages.commandLog.count(records.length)}</span>
        <span className="flex-1" />
        <button
          type="button"
          className="icon-action size-[20px]"
          title={messages.commandLog.copyAll}
          onClick={() => copy(records.map(recordText).join("\n\n"))}
        >
          <Codicon name="copy" size={12} />
        </button>
        <button type="button" className="icon-action size-[20px]" title={messages.recovery.close} onClick={onClose}>
          <Codicon name="close" size={12} />
        </button>
      </div>
      {error ? <p className="px-2 py-1 text-[11.5px] text-destructive">{error}</p> : null}
      <div className="flex min-h-0 flex-1">
        <div ref={listRef} className="min-h-0 w-[58%] shrink-0 overflow-auto">
          {records.length === 0 ? (
            <p className="px-2 py-2 text-[11.5px] text-descriptionforeground">{messages.commandLog.empty}</p>
          ) : null}
          {[...records].reverse().map((record) => {
            const activeRow = active?.sequence === record.sequence;
            return (
              <button
                key={record.sequence}
                type="button"
                className={`command-row w-full ${activeRow ? "selected" : ""}`}
                onClick={() => setSelected(record.sequence)}
                title={record.directory}
              >
                <span className={`w-[16px] shrink-0 ${record.exitCode === 0 ? "text-[var(--gitdecoration-addedresourceforeground)]" : "text-destructive"}`}>
                  <Codicon name={record.exitCode === 0 ? "pass" : record.timedOut ? "watch" : "error"} size={11} />
                </span>
                <span className="w-[54px] shrink-0 text-descriptionforeground tabular-nums">
                  {new Date(record.startedAtMs).toLocaleTimeString()}
                </span>
                <span className="min-w-0 flex-1 truncate font-mono text-[11px]">
                  {record.args.join(" ")}
                </span>
                <span className="w-[44px] shrink-0 text-right text-descriptionforeground tabular-nums">
                  {record.elapsedMs}ms
                </span>
                <span className="w-[36px] shrink-0 text-right text-descriptionforeground tabular-nums">
                  {record.exitCode}
                </span>
              </button>
            );
          })}
        </div>
        <div className="flex min-h-0 min-w-0 flex-1 flex-col border-l border-panel-border">
          {active ? (
            <>
              <div className="flex shrink-0 items-center gap-2 border-b border-panel-border px-2 py-1 text-[11px] text-descriptionforeground">
                <span className="min-w-0 flex-1 truncate" title={active.directory}>
                  {active.worktreeId ? `${active.worktreeId} · ` : ""}
                  {active.directory}
                </span>
                {active.outputTruncated ? <span title={messages.commandLog.truncated}>{messages.commandLog.truncatedShort}</span> : null}
                <button
                  type="button"
                  className="icon-action size-[20px]"
                  title={messages.commandLog.copyOne}
                  onClick={() => copy(recordText(active))}
                >
                  <Codicon name="copy" size={12} />
                </button>
              </div>
              <pre className="min-h-0 flex-1 overflow-auto px-2 py-1 font-mono text-[11px] whitespace-pre-wrap break-words">
                <span className="text-descriptionforeground">{`${active.program} ${active.args.join(" ")}\n`}</span>
                {active.output || messages.commandLog.noOutput}
              </pre>
            </>
          ) : (
            <p className="px-2 py-2 text-[11.5px] text-descriptionforeground">{messages.commandLog.selectRecord}</p>
          )}
        </div>
      </div>
    </section>
  );
}
