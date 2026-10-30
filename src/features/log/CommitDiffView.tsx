import { formatAppError } from "@/lib/error";
/** Fixed-object history data uses the same controlled renderers as worktree diffs. */
import { useEffect, useMemo, useSyncExternalStore } from "react";
import type { HistoryFileTarget } from "@/types/bindings/HistoryFileTarget";
import type { IgnoreWhitespace } from "@/types/bindings/IgnoreWhitespace";
import type { ViewMode } from "@/types/bindings/ViewMode";
import type { DiffOptions } from "@/features/diff/SideBySideView";
import { DiffView } from "@/features/diff/DiffView";
import { ipc } from "@/lib/ipc";
import { messages } from "@/lib/i18n/index";
import { Codicon } from "@/components/ui/codicon";
import { HistoryRowsStore, historyRowsKey } from "./historyRows";
import { HistoryHighlightStore } from "./historyHighlightStore";
import { HistorySideDiffView } from "./HistorySideDiffView";

export interface CommitDiffViewProps {
  targetId: string;
  file: HistoryFileTarget;
  title: string;
  view: ViewMode;
  ignoreWhitespace: IgnoreWhitespace;
  softWrap: boolean;
  fontSize: number;
  fontFamily: string | null;
  options: DiffOptions;
  onClose: () => void;
}

export function CommitDiffView(props: CommitDiffViewProps) {
  const { targetId, file, view, options, fontSize, fontFamily } = props;
  const identityKey = historyRowsKey(props);
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-[24px] shrink-0 items-center gap-2 border-b border-panel-border px-2 text-[11.5px] text-descriptionforeground">
        <Codicon name="eye" size={12} />
        <span className="shrink-0">{messages.history.readonly}</span>
        <span className="min-w-0 flex-1 truncate" title={props.title}>{props.title}</span>
        <span className="shrink-0">{view === "split" ? messages.history.split : messages.history.unified}</span>
        <button type="button" className="icon-action size-[20px]" title={messages.recovery.close} onClick={props.onClose}>
          <Codicon name="close" size={12} />
        </button>
      </div>
      <div className="min-h-0 flex-1">
        {view === "split" ? (
          <HistorySideDiffView key={identityKey} targetId={targetId} file={file}
            options={options} fontSize={fontSize} fontFamily={fontFamily} />
        ) : <UnifiedHistoryDiff key={identityKey} {...props} />}
      </div>
    </div>
  );
}

function UnifiedHistoryDiff(props: CommitDiffViewProps) {
  const { targetId, file, ignoreWhitespace, fontSize, fontFamily, softWrap } = props;
  const identityKey = historyRowsKey(props);
  const rows = useMemo(() => new HistoryRowsStore(
    { targetId, file, view: "unified", ignoreWhitespace }, ipc.historyFileRows,
  ), [file, ignoreWhitespace, targetId]);
  const highlights = useMemo(() => new HistoryHighlightStore(
    { targetId, file, ignoreWhitespace },
    { highlight: ipc.historyFileHighlight, categories: ipc.syntaxCategories },
  ), [file, ignoreWhitespace, targetId]);
  const snapshot = useSyncExternalStore(rows.subscribe, rows.getSnapshot);
  const state = snapshot.details;
  useEffect(() => {
    rows.ensure(0, 60);
    return () => { rows.invalidate(); highlights.dispose(); };
  }, [highlights, rows]);
  const emptyText = (snapshot.error ? formatAppError(snapshot.error) : null)
    ?? (state?.binary ? messages.history.binary
      : state?.modeOnly ? messages.history.modeOnly
        : state?.emptyReason === "emptyFile" ? messages.history.emptyFile
          : state && state.rowCount === 0 ? messages.history.noRows : null);
  if (emptyText) {
    return <div className="flex h-full items-center justify-center p-6 text-center text-[12.5px] text-descriptionforeground">{emptyText}</div>;
  }
  return <DiffView history={{ key: identityKey, path: file.path }} view="unified" rows={rows}
    highlights={highlights} fontSize={fontSize} fontFamily={fontFamily} softWrap={softWrap} />;
}
