import type { SideLine } from "@/types/bindings/SideLine";
import type { DisplayRow, Side } from "./rowIndex";

const CHUNK = 400;

/** File line ranges are one-based and inclusive; display row ranges are half-open. */
export interface SideLineRange {
  from: number;
  to: number;
}

export function lineRangesForRows(rows: readonly DisplayRow[], from: number, to: number): SideLineRange[] {
  const ranges: SideLineRange[] = [];
  for (let index = Math.max(0, Math.floor(from)); index < Math.min(rows.length, Math.ceil(to)); index += 1) {
    const row = rows[index];
    if (!("line" in row)) continue;
    const previous = ranges.at(-1);
    if (previous && previous.to + 1 === row.line) previous.to = row.line;
    else ranges.push({ from: row.line, to: row.line });
  }
  return ranges;
}

interface WindowRequest {
  visible: SideLineRange[];
  buffered: SideLineRange[];
  lineCount: number;
}

interface Loader {
  load: (side: Side, from: number, to: number) => Promise<SideLine[]>;
  isCurrent: () => boolean;
  publish: () => void;
}

/** Two visible windows have priority over either column's full-block prefetch. */
export class SideLineWindow {
  readonly old = new Map<number, SideLine>();
  readonly new = new Map<number, SideLine>();
  private windows: Partial<Record<Side, WindowRequest>> = {};
  private pending = new Map<string, { side: Side; from: number; to: number; slice: boolean }>();

  readonly key: string;

  constructor(key: string) {
    this.key = key;
  }

  get pendingRequests(): number {
    return this.pending.size;
  }

  ensure(side: Side, window: WindowRequest, loader: Loader): void {
    if (!loader.isCurrent()) return;
    this.windows[side] = window;
    for (const range of window.visible) {
      if (!this.covered(side, range)) this.request(side, range, true, loader);
    }

    // Wait for both columns, including the empty side of an addition/deletion.
    if (!this.windows.left || !this.windows.right) return;
    for (const current of ["left", "right"] as const) {
      if (this.windows[current]!.visible.some((range) => !this.covered(current, range))) return;
    }
    for (const current of ["left", "right"] as const) {
      const wanted = this.windows[current]!;
      for (const range of wanted.buffered) {
        for (let from = Math.floor((range.from - 1) / CHUNK) * CHUNK + 1; from <= range.to; from += CHUNK) {
          const block = { from, to: Math.min(from + CHUNK - 1, wanted.lineCount, 1_000_000) };
          if (block.to >= block.from && !this.covered(current, block)) this.request(current, block, false, loader);
        }
      }
    }
  }

  private covered(side: Side, range: SideLineRange): boolean {
    const lines = side === "left" ? this.old : this.new;
    for (let line = range.from; line <= range.to; line += 1) {
      if (!lines.has(line)) return false;
    }
    return true;
  }

  private request(side: Side, range: SideLineRange, slice: boolean, loader: Loader): void {
    const key = `${slice ? "slice" : "block"}/${side}/${range.from}/${range.to}`;
    if (this.pending.has(key)) return;
    if (slice && [...this.pending.values()].some((pending) => pending.slice && pending.side === side
      && pending.from <= range.from && pending.to >= range.to)) return;
    this.pending.set(key, { side, ...range, slice });
    let accepted = false;
    void loader.load(side, range.from, range.to)
      .then((received) => {
        if (!loader.isCurrent()) return;
        const lines = side === "left" ? this.old : this.new;
        for (const line of received) {
          // Same-snapshot overlap is identical; preserve row/word identities for memoized rows.
          if (lines.has(line.line)) continue;
          lines.set(line.line, line);
          accepted = true;
        }
      })
      .catch(() => {
        // Keep the existing loading background; failure must not drive a render/retry loop.
      })
      .finally(() => {
        this.pending.delete(key);
        if (accepted && loader.isCurrent()) loader.publish();
      });
  }
}
