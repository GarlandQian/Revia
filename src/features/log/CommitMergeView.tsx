/**
 * merge 提交的三栏只读视图（M5 L05、Q147）。
 *
 * 父提交 1 | 合并结果 | 父提交 2，三栏同步滚动，没有任何写入按钮、块操作或审阅标记。
 * 内容来自 `history_merge_view`（按对象 ID 从 Git 读取，不读工作区）。
 *
 * 说明：M4 的 `MergeView` 是**可编辑**的合并编辑器（CodeMirror + 块状态 + 应用按钮），
 * 本视图只借用它的三栏布局与配色，不复用它的编辑与写入路径（报告里记作实现选择）。
 */

import { useEffect, useRef, useState } from "react";
import type { HistoryMergeView } from "@/types/bindings/HistoryMergeView";
import { ipc } from "@/lib/ipc";
import { messages } from "@/lib/i18n/zh-CN";
import { getErrorMessage } from "@/lib/error";
import { Codicon } from "@/components/ui/codicon";

export interface CommitMergeViewProps {
  targetId: string;
  commit: string;
  path: string;
  title: string;
  fontSize: number;
  fontFamily: string | null;
  onClose: () => void;
}

export function CommitMergeView(props: CommitMergeViewProps) {
  const { targetId, commit, path, fontSize, fontFamily } = props;
  const [layout, setLayout] = useState<HistoryMergeView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const panes = useRef<Array<HTMLDivElement | null>>([]);
  const syncing = useRef(false);

  useEffect(() => {
    let cancelled = false;
    setLayout(null);
    setError(null);
    void ipc
      .historyMergeView({ targetId, commit, path })
      .then((next) => {
        if (!cancelled) setLayout(next);
      })
      .catch((caught: unknown) => {
        if (!cancelled) setError(getErrorMessage(caught));
      });
    return () => {
      cancelled = true;
    };
  }, [commit, path, targetId]);

  const syncScroll = (source: number) => {
    if (syncing.current) return;
    syncing.current = true;
    const from = panes.current[source];
    const ratio = from && from.scrollHeight > from.clientHeight
      ? from.scrollTop / (from.scrollHeight - from.clientHeight)
      : 0;
    panes.current.forEach((pane, index) => {
      if (!pane || index === source) return;
      pane.scrollTop = ratio * Math.max(0, pane.scrollHeight - pane.clientHeight);
    });
    syncing.current = false;
  };

  const unavailable = error ?? layout?.unavailable ?? null;

  return (
    <div className="flex h-full min-h-0 flex-col" style={{ fontSize, fontFamily: fontFamily ?? undefined }}>
      <div className="flex h-[24px] shrink-0 items-center gap-2 border-b border-panel-border px-2 text-[11.5px] text-descriptionforeground">
        <Codicon name="git-merge" size={12} />
        <span className="shrink-0">{messages.history.mergeReadonly}</span>
        <span className="min-w-0 flex-1 truncate" title={props.title}>{props.title}</span>
        <button type="button" className="icon-action size-[20px]" title={messages.recovery.close} onClick={props.onClose}>
          <Codicon name="close" size={12} />
        </button>
      </div>
      {unavailable ? (
        <div className="flex min-h-0 flex-1 items-center justify-center p-6 text-center text-[12.5px] text-descriptionforeground">
          {unavailable}
        </div>
      ) : !layout ? (
        <div className="flex min-h-0 flex-1 items-center justify-center p-6 text-[12.5px] text-descriptionforeground">
          {messages.history.loadingMerge}
        </div>
      ) : (
        <div className="grid min-h-0 flex-1 grid-cols-3 divide-x divide-[var(--diff-gutter-border)]">
          <MergePane
            label={messages.history.parent1(layout.sides[0]?.parent ?? "")}
            text={layout.sides[0]?.text.text ?? ""}
            missing={layout.sides[0]?.text.missing ?? true}
            binary={layout.sides[0]?.text.binary ?? false}
            readonly
            registerRef={(element) => {
              panes.current[0] = element;
            }}
            onScroll={() => syncScroll(0)}
          />
          <MergePane
            label={messages.history.mergeResult}
            text={layout.result.text}
            missing={layout.result.missing}
            binary={layout.result.binary}
            readonly
            registerRef={(element) => {
              panes.current[1] = element;
            }}
            onScroll={() => syncScroll(1)}
          />
          <MergePane
            label={messages.history.parent2(layout.sides[1]?.parent ?? "")}
            text={layout.sides[1]?.text.text ?? ""}
            missing={layout.sides[1]?.text.missing ?? true}
            binary={layout.sides[1]?.text.binary ?? false}
            readonly
            registerRef={(element) => {
              panes.current[2] = element;
            }}
            onScroll={() => syncScroll(2)}
          />
        </div>
      )}
    </div>
  );
}

function MergePane({
  label,
  text,
  missing,
  binary,
  readonly,
  registerRef,
  onScroll,
}: {
  label: string;
  text: string;
  missing: boolean;
  binary: boolean;
  readonly: boolean;
  registerRef: (element: HTMLDivElement | null) => void;
  onScroll: () => void;
}) {
  return (
    <div className="flex min-h-0 min-w-0 flex-col">
      <div className="flex h-[22px] shrink-0 items-center gap-1.5 border-b border-panel-border px-2 font-sans text-[11px] text-descriptionforeground">
        <span className="min-w-0 flex-1 truncate" title={label}>{label}</span>
        {readonly ? <Codicon name="lock" size={11} /> : null}
      </div>
      <div
        ref={registerRef}
        onScroll={onScroll}
        className="min-h-0 flex-1 overflow-auto bg-[var(--diff-bg)] font-mono"
      >
        {binary ? (
          <p className="p-3 font-sans text-[12px] text-descriptionforeground">{messages.history.binary}</p>
        ) : missing ? (
          <p className="p-3 font-sans text-[12px] text-descriptionforeground">{messages.history.missingSide}</p>
        ) : (
          <table className="border-collapse">
            <tbody>
              {text.split("\n").map((line, index) => (
                <tr key={index} className="history-merge-row">
                  <td className="history-merge-line">{index + 1}</td>
                  <td className="history-merge-cell">{line === "" ? " " : line}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
