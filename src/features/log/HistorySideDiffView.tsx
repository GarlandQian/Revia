import { useEffect, useMemo } from "react";
import { SideBySideView, type DiffOptions } from "@/features/diff/SideBySideView";
import type { HistoryFileTarget } from "@/types/bindings/HistoryFileTarget";
import { ipc } from "@/lib/ipc";
import { createHistorySideSource } from "./historySideSource";
import { HistoryHighlightStore } from "./historyHighlightStore";

export interface HistorySideDiffViewProps {
  targetId: string;
  file: HistoryFileTarget;
  fontSize: number;
  fontFamily: string | null;
  options: DiffOptions;
}

export function HistorySideDiffView({ targetId, file, fontSize, fontFamily, options }: HistorySideDiffViewProps) {
  const source = useMemo(() => createHistorySideSource(targetId, file, options, ipc),
    [targetId, file, options.ignoreWhitespace, options.contextLines, options.highlight]);
  const highlights = useMemo(() => new HistoryHighlightStore({ targetId, file, ignoreWhitespace: options.ignoreWhitespace }, {
    highlight: ipc.historyFileHighlight, categories: ipc.syntaxCategories,
  }), [targetId, file, options.ignoreWhitespace]);
  useEffect(() => () => highlights.dispose(), [highlights]);
  return <SideBySideView history={source} fontSize={fontSize} fontFamily={fontFamily} options={options} highlights={highlights} />;
}
