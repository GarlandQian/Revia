import type { HistoryFileTarget } from "@/types/bindings/HistoryFileTarget";
import type { HistoryRowSlice } from "@/types/bindings/HistoryRowSlice";
import type { IgnoreWhitespace } from "@/types/bindings/IgnoreWhitespace";
import type { ViewMode } from "@/types/bindings/ViewMode";
import type { RenderRow } from "@/types/bindings/RenderRow";
import type { DiffRows, RowWindowSnapshot } from "@/features/diff/rowWindowStore";
import { getErrorMessage } from "@/lib/error";

export interface HistoryRowsIdentity {
  targetId: string;
  file: HistoryFileTarget;
  view: ViewMode;
  ignoreWhitespace: IgnoreWhitespace;
}

export interface HistoryRowsRequest extends HistoryRowsIdentity {
  from: number;
  to: number;
}

export interface HistoryRowsState {
  key: string;
  from: number;
  to: number;
  rows: HistoryRowSlice["rows"];
  rowCount: number;
  binary: boolean;
  modeOnly: boolean;
  emptyReason: HistoryRowSlice["emptyReason"];
  error: string | null;
}

export const EMPTY_HISTORY_ROWS: HistoryRowsState = {
  key: "", from: 0, to: 0, rows: [], rowCount: 0,
  binary: false, modeOnly: false, emptyReason: null, error: null,
};

export function historyRowsKey({ targetId, file, view, ignoreWhitespace }: HistoryRowsIdentity): string {
  return JSON.stringify([
    targetId, file.diffId, file.commit, file.parent, file.path, file.originalPath,
    file.oldOid, file.newOid, file.status, view, ignoreWhitespace,
  ]);
}

const CHUNK = 400;

// Include both ends of the visible interval, plus one forward prefetch block.
export function historyRowsWindow(visibleStart: number, visibleEnd: number): { from: number; to: number } {
  const from = Math.floor(Math.max(0, visibleStart) / CHUNK) * CHUNK;
  const to = Math.ceil(Math.max(visibleStart + 1, visibleEnd) / CHUNK) * CHUNK + CHUNK;
  return { from, to };
}

/** One bounded window; superseded requests must never populate its cache. */
export class HistoryRowsReader {
  private generation = 0;
  private cached: HistoryRowsState | null = null;
  private readonly loader: (request: HistoryRowsRequest) => Promise<HistoryRowSlice>;

  constructor(loader: (request: HistoryRowsRequest) => Promise<HistoryRowSlice>) {
    this.loader = loader;
  }

  invalidate(): void {
    this.generation += 1;
    this.cached = null;
  }

  async read(request: HistoryRowsRequest): Promise<HistoryRowsState | null> {
    const generation = ++this.generation;
    const key = historyRowsKey(request);
    const cached = this.cached;
    if (cached?.key === key && cached.from <= request.from
      && cached.to >= (cached.error ? request.to : Math.min(request.to, cached.rowCount))) {
      return cached;
    }
    try {
      const slice = await this.loader(request);
      if (generation !== this.generation) return null;
      if (slice.targetId !== request.targetId || slice.diffId !== request.file.diffId
        || slice.view !== request.view || slice.from !== Math.min(request.from, slice.rowCount)) {
        throw new Error("提交 diff 响应与读取目标不一致");
      }
      this.cached = { ...slice, key, to: request.to, error: null };
    } catch (caught) {
      if (generation !== this.generation) return null;
      this.cached = { ...EMPTY_HISTORY_ROWS, key, from: request.from, to: request.to, error: getErrorMessage(caught) };
    }
    return this.cached;
  }
}

export interface HistoryRowsSnapshot extends RowWindowSnapshot {
  details: HistoryRowsState | null;
}

/** History data for the existing controlled renderer, without worktree credentials. */
export class HistoryRowsStore implements DiffRows {
  private readonly identity: HistoryRowsIdentity;
  private readonly reader: HistoryRowsReader;
  private readonly rows = new Map<number, RenderRow>();
  private readonly listeners = new Set<() => void>();
  private generation = 0;
  private pending: { from: number; to: number } | null = null;
  private epoch = 0;
  private visible = { from: 0, to: 1 };
  private direction = 1;
  private snapshot: HistoryRowsSnapshot = {
    version: 0, rowCount: 0, chunkCount: 0, loadedChunks: 0,
    pendingChunks: 0, error: null, stale: false, details: null,
  };

  constructor(identity: HistoryRowsIdentity, loader: (request: HistoryRowsRequest) => Promise<HistoryRowSlice>) {
    this.identity = identity;
    this.reader = new HistoryRowsReader(loader);
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): HistoryRowsSnapshot => this.snapshot;
  get rowCountValue(): number { return this.snapshot.rowCount; }
  get epochValue(): number { return this.epoch; }
  getRow(index: number): RenderRow | null { return this.rows.get(index) ?? null; }
  noteDirection(delta: number): void { this.direction = delta >= 0 ? 1 : -1; }

  uncovered(from: number, to: number): number {
    let count = 0;
    for (let index = Math.max(0, from); index < Math.min(to, this.rowCountValue); index += 1) {
      if (!this.rows.has(index)) count += 1;
    }
    return count;
  }

  isCovered(from: number, to: number): boolean {
    return this.snapshot.details !== null && this.uncovered(from, to) === 0;
  }

  ensure(from: number, to: number, prefetchScreens = 2): boolean {
    this.visible = { from, to };
    if (this.snapshot.error) return false;
    const covered = this.isCovered(from, to);
    const margin = Math.max(1, to - from) * prefetchScreens;
    const window = historyRowsWindow(Math.max(0, from - margin), to + margin);
    if (!this.snapshot.details || !this.isCovered(window.from, window.to)) {
      if (!this.pending || this.pending.from > window.from || this.pending.to < Math.min(window.to, this.rowCountValue)) {
        void this.load(window);
      }
    } else {
      this.prefetch();
    }
    return covered;
  }

  invalidate(): void {
    this.generation += 1;
    this.epoch += 1;
    this.pending = null;
    this.reader.invalidate();
  }

  private async load(window: { from: number; to: number }): Promise<void> {
    const generation = ++this.generation;
    this.pending = window;
    this.publish();
    const state = await this.reader.read({ ...this.identity, ...window });
    if (!state || generation !== this.generation) return;
    this.pending = null;
    for (const [index, row] of state.rows.entries()) {
      this.rows.set(state.from + index, { ...row, hunkAction: null, segmentAction: null });
    }
    if (state.rowCount > 20_000 && this.rows.size > 24_000) {
      const center = this.visible.from;
      const farthest = [...this.rows.keys()].sort((a, b) => Math.abs(b - center) - Math.abs(a - center));
      for (const index of farthest.slice(0, this.rows.size - 24_000)) this.rows.delete(index);
    }
    this.snapshot = { ...this.snapshot, details: state, rowCount: state.rowCount, error: state.error };
    this.publish();
    this.prefetch();
  }

  private prefetch(): void {
    if (this.pending || this.snapshot.error || !this.snapshot.details) return;
    if (!this.isCovered(this.visible.from, this.visible.to)) return;
    if (this.rowCountValue <= 20_000) {
      for (let from = 0; from < this.rowCountValue; from += CHUNK) {
        const to = Math.min(from + CHUNK, this.rowCountValue);
        if (!this.isCovered(from, to)) { void this.load({ from, to }); return; }
      }
    } else {
      const from = this.direction > 0 ? this.visible.to : Math.max(0, this.visible.from - CHUNK);
      const window = historyRowsWindow(from, from + CHUNK);
      if (!this.isCovered(window.from, window.to)) void this.load(window);
    }
  }

  private publish(): void {
    this.snapshot = {
      ...this.snapshot, version: this.snapshot.version + 1,
      chunkCount: Math.ceil(this.rowCountValue / CHUNK), loadedChunks: Math.ceil(this.rows.size / CHUNK),
      pendingChunks: this.pending ? 1 : 0,
    };
    for (const listener of this.listeners) listener();
  }
}
