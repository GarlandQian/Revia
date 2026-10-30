import { useMemo } from "react";
import { DiffView } from "./DiffView";
import type { DiffHighlights, HighlightSnapshot } from "./highlightStore";
import { PatchRows } from "./patchRows";

// Raw patches can contain several file languages. Keep their Git-provided
// text and diff tones; they have no single version to request syntax tokens for.
const snapshot: HighlightSnapshot = { version: 0, loadedLines: 0, pending: 0,
  supported: false, error: null, lastRoundTripMs: null };
const plain: DiffHighlights = { subscribe: () => () => {}, getSnapshot: () => snapshot,
  get: () => null, ensure: () => {}, categoryClass: () => "" };

export function PatchDiffView({ patch, identity, path, fontSize = 11 }: {
  patch: string; identity: string; path: string; fontSize?: number;
}) {
  const rows = useMemo(() => new PatchRows(patch), [patch]);
  return <DiffView key={identity} history={{ key: identity, path }} view="unified"
    rows={rows} highlights={plain} fontSize={fontSize} softWrap />;
}
