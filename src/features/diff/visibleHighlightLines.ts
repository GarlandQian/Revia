import type { FileVersion } from "@/types/bindings/FileVersion";
import type { RenderRow } from "@/types/bindings/RenderRow";
import type { ViewMode } from "@/types/bindings/ViewMode";

/** Match RowView's token sources without spanning gaps between versions or hunks. */
export function visibleHighlightLines(
  getRow: (index: number) => RenderRow | null,
  from: number,
  to: number,
  view: ViewMode,
): Record<FileVersion, number[]> {
  const lines: Record<FileVersion, number[]> = { new: [], old: [] };
  for (let index = from; index < to; index += 1) {
    const row = getRow(index);
    if (!row || row.kind === "hunk") continue;
    if (view === "split") {
      if (row.right != null) lines.new.push(row.right.line);
      if (row.left != null) lines.old.push(row.left.line);
    } else {
      const added = row.lineKind === "addition";
      const line = added ? row.newLine : row.oldLine;
      if (line != null) lines[added ? "new" : "old"].push(line);
    }
  }
  return lines;
}
