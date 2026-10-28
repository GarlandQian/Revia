import type { FileVersion } from "@/types/bindings/FileVersion";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { RenderRow } from "@/types/bindings/RenderRow";
import type { RowIndex, Side } from "./rowIndex";
import type { RowHeights } from "./rowHeights";
import { filePositionAtY } from "./sideGeometry";

export interface FileAnchor {
  line: number;
  version: FileVersion;
}

export interface AnchorTarget {
  worktreeId: string;
  entry: Pick<ChangeEntry, "path" | "changes" | "fingerprint" | "snapshotRevision">;
  selectionEpoch: number;
  activation: number;
  optionsKey: string;
}

export function sameAnchorTarget(a: AnchorTarget, b: AnchorTarget): boolean {
  return a.worktreeId === b.worktreeId && a.entry.path === b.entry.path
    && a.entry.changes === b.entry.changes && a.entry.fingerprint === b.entry.fingerprint
    && a.entry.snapshotRevision === b.entry.snapshotRevision
    && a.selectionEpoch === b.selectionEpoch && a.activation === b.activation
    && a.optionsKey === b.optionsKey;
}

/** Skip headers and unloaded rows; context lines use their new-version number. */
export function unifiedFileAnchor(
  getRow: (index: number) => RenderRow | null,
  from: number,
  to: number,
): FileAnchor | null {
  for (let index = from; index < to; index += 1) {
    const row = getRow(index);
    if (!row || row.kind !== "line") continue;
    if (row.lineKind === "deletion" && row.oldLine != null) {
      return { line: row.oldLine, version: "old" };
    }
    if (row.newLine != null) return { line: row.newLine, version: "new" };
  }
  return null;
}

export function sideFileAnchor(side: Side, rows: RowIndex, heights: RowHeights, top: number): FileAnchor | null {
  if (rows.lineCount === 0) return null;
  return {
    line: Math.min(rows.lineCount, Math.floor(filePositionAtY(rows, heights, top)) + 1),
    version: side === "left" ? "old" : "new",
  };
}
