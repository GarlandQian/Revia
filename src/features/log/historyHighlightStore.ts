import type { HighlightLine, ipc } from "@/lib/ipc";
import type { HighlightSnapshot } from "@/features/diff/highlightStore";
import type { HistoryFileTarget } from "@/types/bindings/HistoryFileTarget";
import type { IgnoreWhitespace } from "@/types/bindings/IgnoreWhitespace";
import type { FileVersion } from "@/types/bindings/FileVersion";
import { getErrorMessage } from "@/lib/error";

const CHUNK = 400;
const MAX_CHUNKS = 16;
const MAX_IN_FLIGHT = 2;

interface HistoryHighlightTarget {
  targetId: string;
  file: HistoryFileTarget;
  ignoreWhitespace: IgnoreWhitespace;
}

interface HistoryHighlightApi {
  highlight: typeof ipc.historyFileHighlight;
  categories: typeof ipc.syntaxCategories;
}

/** A bounded, fixed-history counterpart to the worktree HighlightStore. */
export class HistoryHighlightStore {
  private readonly target: HistoryHighlightTarget;
  private readonly api: HistoryHighlightApi;
  private generation = 0;
  private listeners = new Set<() => void>();
  private pending = new Set<string>();
  private chunks = new Map<string, { version: FileVersion; from: number; count: number }>();
  private lines: Record<FileVersion, Map<number, HighlightLine>> = { old: new Map(), new: new Map() };
  private categories: string[] = [];
  private categoriesPending = false;
  private snapshot: HighlightSnapshot = {
    version: 0, loadedLines: 0, pending: 0, supported: true, error: null, lastRoundTripMs: null,
  };

  constructor(target: HistoryHighlightTarget, api: HistoryHighlightApi) {
    this.target = { ...target, file: { ...target.file } };
    this.api = api;
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): HighlightSnapshot => this.snapshot;

  categoryClass(index: number): string {
    return `tok-${this.categories[index] ?? "plain"}`;
  }

  get(version: FileVersion, line: number): HighlightLine | undefined {
    return this.lines[version].get(line);
  }

  ensure(version: FileVersion, fromLine: number, toLine: number): void {
    this.loadCategories();
    if (!this.snapshot.supported || toLine < fromLine || toLine < 1) return;
    const first = Math.floor(Math.max(0, fromLine - 1) / CHUNK);
    const last = Math.floor(Math.max(0, toLine - 1) / CHUNK);
    for (let chunk = first; chunk <= last; chunk += 1) {
      const key = `${version}:${chunk}`;
      if (this.chunks.has(key) || this.pending.has(key)) continue;
      if (this.pending.size >= MAX_IN_FLIGHT) return;
      void this.load(version, chunk, key);
    }
  }

  dispose(): void {
    this.generation += 1;
    this.pending.clear();
    this.chunks.clear();
    this.lines = { old: new Map(), new: new Map() };
    this.categoriesPending = false;
    this.snapshot = { ...this.snapshot, loadedLines: 0, pending: 0, error: null, supported: true };
  }

  private loadCategories(): void {
    if (this.categories.length || this.categoriesPending) return;
    this.categoriesPending = true;
    const generation = this.generation;
    void this.api.categories().then((categories) => {
      if (generation !== this.generation) return;
      this.categories = categories;
      this.publish();
    }).catch(() => {}).finally(() => {
      if (generation === this.generation) this.categoriesPending = false;
    });
  }

  private async load(version: FileVersion, chunk: number, key: string): Promise<void> {
    const generation = this.generation;
    const fromLine = chunk * CHUNK + 1;
    this.pending.add(key);
    this.publish();
    const started = performance.now();
    try {
      const result = await this.api.highlight({ ...this.target, version, fromLine, toLine: fromLine + CHUNK - 1 });
      if (generation !== this.generation) return;
      // History requests are one-based, but its HighlightResult retains the parser's zero-based origin.
      const firstLine = result.fromLine + 1;
      this.chunks.set(key, { version, from: firstLine, count: result.lines.length });
      if (result.kind === "highlighted") {
        result.lines.forEach((line, index) => this.lines[version].set(firstLine + index, line));
      } else {
        this.snapshot = { ...this.snapshot, supported: false };
      }
      this.snapshot = { ...this.snapshot, lastRoundTripMs: performance.now() - started };
    } catch (error) {
      if (generation !== this.generation) return;
      this.chunks.set(key, { version, from: fromLine, count: 0 });
      this.snapshot = { ...this.snapshot, error: getErrorMessage(error) };
    } finally {
      if (generation === this.generation) {
        this.pending.delete(key);
        while (this.chunks.size > MAX_CHUNKS) {
          const oldest = this.chunks.entries().next().value!;
          this.chunks.delete(oldest[0]);
          for (let index = 0; index < oldest[1].count; index += 1) this.lines[oldest[1].version].delete(oldest[1].from + index);
        }
        this.publish();
      }
    }
  }

  private publish(): void {
    this.snapshot = {
      ...this.snapshot, version: this.snapshot.version + 1,
      loadedLines: this.lines.old.size + this.lines.new.size, pending: this.pending.size,
    };
    for (const listener of this.listeners) listener();
  }
}
