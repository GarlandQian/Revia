/**
 * 高亮缓存：按行范围请求 Rust 侧的 syntect 结果，按语法类别渲染（Q50，ADR-0006）。
 *
 * - 只请求可见范围（含预取），不把整份文件的 token 拉到前端；
 * - 类别名由 Rust 的 `syntax_categories` 命令给出，前端映射到 CSS 类，
 *   切换浅色 / 深色不需要重新高亮；
 * - 旧、新两个版本各自维护（旧 = HEAD 或暂存区，新 = 暂存区或工作区）。
 */

import type { ChangesKind } from "@/types/bindings/ChangesKind";
import type { FileVersion } from "@/types/bindings/FileVersion";
import { ipc, type HighlightLine } from "@/lib/ipc";
import { getErrorMessage } from "@/lib/error";

/** 一次请求覆盖的行数。 */
export const HIGHLIGHT_CHUNK_LINES = 400;
const MAX_IN_FLIGHT = 1;

export interface HighlightTarget {
  worktreeId: string;
  path: string;
  changes: ChangesKind;
  fingerprint: string;
}

export interface HighlightSnapshot {
  version: number;
  loadedLines: number;
  pending: number;
  supported: boolean;
  error: string | null;
  /** 最近一次请求的往返耗时（毫秒），用于测量。 */
  lastRoundTripMs: number | null;
}

export class HighlightStore {
  private target: HighlightTarget | null = null;
  private epoch = 0;
  private version = 0;
  private snapshot: HighlightSnapshot = {
    version: 0,
    loadedLines: 0,
    pending: 0,
    supported: true,
    error: null,
    lastRoundTripMs: null,
  };
  private listeners = new Set<() => void>();
  private lines: Record<FileVersion, Map<number, HighlightLine>> = { old: new Map(), new: new Map() };
  private requested = new Set<string>();
  private pending = new Set<string>();
  private categories: string[] = [];

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): HighlightSnapshot => this.snapshot;

  /** 类别名表（由 Rust 提供），用于把类别索引映射到 CSS 类名。 */
  async loadCategories(): Promise<void> {
    if (this.categories.length > 0) return;
    this.categories = await ipc.syntaxCategories();
  }

  categoryClass(index: number): string {
    const name = this.categories[index] ?? "plain";
    return `tok-${name}`;
  }

  setTarget(target: HighlightTarget): void {
    const same =
      this.target &&
      this.target.worktreeId === target.worktreeId &&
      this.target.path === target.path &&
      this.target.changes === target.changes &&
      this.target.fingerprint === target.fingerprint;
    if (same) return;
    this.epoch += 1;
    this.target = { ...target };
    this.lines = { old: new Map(), new: new Map() };
    this.requested.clear();
    this.pending.clear();
    this.snapshot = { ...this.snapshot, error: null, lastRoundTripMs: null };
    this.publish(null, true);
  }

  /** 取某一版本某一行的 token；null 表示该行还没有高亮结果。 */
  get(version: FileVersion, line: number): HighlightLine | undefined {
    return this.lines[version].get(line);
  }

  /** 保证某个版本的行范围有结果（后台补齐，不阻塞渲染）。 */
  ensure(version: FileVersion, fromLine: number, toLine: number): void {
    if (!this.target || !this.snapshot.supported) return;
    const first = Math.floor(Math.max(0, fromLine - 1) / HIGHLIGHT_CHUNK_LINES);
    const last = Math.floor(Math.max(fromLine - 1, toLine - 1) / HIGHLIGHT_CHUNK_LINES);
    for (let chunk = first; chunk <= last; chunk += 1) {
      const key = `${version}:${chunk}`;
      if (this.requested.has(key) || this.pending.has(key)) continue;
      void this.requestChunk(version, chunk, key);
    }
  }

  private async requestChunk(version: FileVersion, chunk: number, key: string): Promise<void> {
    const target = this.target;
    if (!target) return;
    if (this.pending.size >= MAX_IN_FLIGHT) {
      // 简单背压：稍后再试（由下一次 ensure 触发）。
      return;
    }
    const epoch = this.epoch;
    this.pending.add(key);
    this.publish(null);
    const fromLine = chunk * HIGHLIGHT_CHUNK_LINES + 1;
    const toLine = fromLine + HIGHLIGHT_CHUNK_LINES - 1;
    const startedAt = performance.now();
    try {
      const result = await ipc.highlightRange({
        worktreeId: target.worktreeId,
        path: target.path,
        changes: target.changes,
        version,
        fromLine,
        toLine,
      });
      if (epoch !== this.epoch) return;
      if (result.kind === "plainText") {
        this.requested.add(key);
        this.publish(null, false);
        return;
      }
      const store = this.lines[version];
      result.lines.forEach((line: HighlightLine, index: number) => {
        store.set(result.fromLine + index, line);
      });
      this.requested.add(key);
      this.snapshot = { ...this.snapshot, lastRoundTripMs: performance.now() - startedAt };
      this.publish(null, true);
    } catch (error) {
      if (epoch !== this.epoch) return;
      this.requested.add(key);
      this.publish(getErrorMessage(error), true);
    } finally {
      if (epoch === this.epoch) {
        this.pending.delete(key);
        this.publish(null);
      }
    }
  }

  private publish(error: string | null, supported = this.snapshot.supported): void {
    this.version += 1;
    this.snapshot = {
      version: this.version,
      loadedLines: this.lines.old.size + this.lines.new.size,
      pending: this.pending.size,
      supported,
      error: error ?? this.snapshot.error,
      lastRoundTripMs: this.snapshot.lastRoundTripMs,
    };
    for (const listener of this.listeners) listener();
  }
}
