import type { RenderRow } from "@/types/bindings/RenderRow";
import type { DiffRows, RowWindowSnapshot } from "./rowWindowStore";

/** A bounded patch produced by Git, adapted to the existing read-only renderer.
 * Only requested rows become render objects; this never issues write credentials
 * or computes a diff between file contents. Git's headers and text stay intact. */
export class PatchRows implements DiffRows {
  private readonly lines: string[];
  private readonly positions: Array<{ old: number | null; next: number | null; body: boolean }>;
  private readonly snapshot: RowWindowSnapshot;

  constructor(patch: string) {
    this.lines = patch.split("\n");
    if (this.lines.at(-1) === "") this.lines.pop();
    let old = 0, next = 0, body = false;
    this.positions = this.lines.map((line) => {
      const header = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line);
      if (header) { old = Number(header[1]); next = Number(header[2]); body = true; return { old: null, next: null, body: false }; }
      if (line.startsWith("diff --git ")) body = false;
      if (!body || ![" ", "+", "-"].includes(line[0])) return { old: null, next: null, body: false };
      return { old: line[0] === "+" ? null : old++, next: line[0] === "-" ? null : next++, body: true };
    });
    this.snapshot = { version: 1, rowCount: this.lines.length, chunkCount: 1,
      loadedChunks: 1, pendingChunks: 0, error: null, stale: false };
  }

  subscribe = (_listener: () => void) => () => {};
  getSnapshot = () => this.snapshot;
  get rowCountValue() { return this.lines.length; }
  get epochValue() { return 1; }
  getRow(index: number): RenderRow | null {
    if (index < 0 || index >= this.lines.length) return null;
    const line = this.lines[index], position = this.positions[index];
    return { kind: "line", hunkIndex: 0, hunkAction: null, segmentAction: null,
      section: null, oldLine: position.old, newLine: position.next,
      lineKind: position.body ? line[0] === "+" ? "addition" : line[0] === "-" ? "deletion" : "context" : "context",
      text: position.body ? line.slice(1) : line, left: null, right: null,
      modified: false, segmentIndex: null };
  }
  uncovered = () => 0;
  isCovered = () => true;
  noteDirection = () => {};
  ensure = () => true;
}
