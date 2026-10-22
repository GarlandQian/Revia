/**
 * 显示行窗口：按可见范围分块取数据（Q46 的核心）。
 *
 * 设计：
 * - 每个文件 + 视图的显示行按 500 行一块缓存，缺哪块取哪块；
 * - 可见范围必须齐备，另外沿滚动方向预取；
 * - 可见范围缺数据时（例如拖滚动条跳到没取过的位置），先单独取可见的几十行，
 *   再补整块：小请求传得快，文字能在 Q52 要求的 100ms 内出现；
 * - 可见块的请求不受在途上限限制；跳转前已在途的块返回后照常缓存，不丢弃；
 * - 2 万显示行以内的文件，首屏齐备后在后台把其余的块也取完，快速滑动时数据已在内存里（Q69）；
 * - 切文件 / 切视图时递增代际，过期响应直接丢弃；
 * - 数据没到时返回 null，界面露出 `.diff-canvas` 底图里“加载中的行”，不出现白底。
 */

import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesKind } from "@/types/bindings/ChangesKind";
import type { RenderRow } from "@/types/bindings/RenderRow";
import type { ViewMode } from "@/types/bindings/ViewMode";

export const CHUNK_ROWS = 500;

import { logToBackend } from "./log";
import { getErrorMessage } from "@/lib/error";
/** 预取最多同时在途的块数；可见块的请求不受这个限制。 */
const MAX_IN_FLIGHT = 2;
/** 单独取可见行时，在可见范围上下各多取的行数。 */
const VISIBLE_SLICE_MARGIN = 16;
/** 单独取到的零散行最多保留的行数，超出后清空重来（它们只是整块到达前的过渡）。 */
const MAX_LOOSE_ROWS = CHUNK_ROWS * 4;
/** 沿滚动方向预取的块数。 */
const PREFETCH_CHUNKS = 2;
/** 不超过这个显示行数的文件，首屏齐备后在后台取完全部行（Q69）。 */
const FULL_PREFETCH_MAX_ROWS = 20_000;
/** 块缓存上限（约 24 000 行，能容下整份后台预取的小文件）；超出后丢弃离可见区域最远的块。 */
const MAX_CHUNKS = 48;

export interface RowWindowTarget {
  worktreeId: string;
  path: string;
  changes: ChangesKind;
  view: ViewMode;
}

export interface RowWindowSnapshot {
  version: number;
  rowCount: number;
  chunkCount: number;
  loadedChunks: number;
  pendingChunks: number;
  error: string | null;
}

type FetchRows = (request: {
  worktreeId: string;
  path: string;
  changes: ChangesKind;
  view: ViewMode;
  from: number;
  to: number;
}) => Promise<{ rows: RenderRow[]; rowCount: number; entry: ChangeEntry }>;

export class RowWindowStore {
  private target: RowWindowTarget | null = null;
  private rowCount = 0;
  private chunks = new Map<number, RenderRow[]>();
  private pending = new Map<number, number>();
  /** 整块到达前单独取到的可见行（行下标 → 行）。 */
  private looseRows = new Map<number, RenderRow>();
  /** 同一时刻只发一个可见行请求；返回后若仍有缺口，由下一次 ensure 再发。 */
  private sliceInFlight = false;
  private epoch = 0;
  private version = 0;
  private snapshot: RowWindowSnapshot = {
    version: 0,
    rowCount: 0,
    chunkCount: 0,
    loadedChunks: 0,
    pendingChunks: 0,
    error: null,
  };
  private listeners = new Set<() => void>();
  private direction = 1;
  private visibleFrom = 0;
  private visibleTo = 0;
  private nextKey = 1;
  private entry: ChangeEntry | null = null;

  private readonly fetchRows: FetchRows;

  constructor(fetchRows: FetchRows) {
    this.fetchRows = fetchRows;
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): RowWindowSnapshot => this.snapshot;

  /** 切换文件 / 视图；行数与元数据随第一次取块的结果校正。 */
  setTarget(target: RowWindowTarget, rowCount: number, entry: ChangeEntry | null): void {
    const same =
      this.target &&
      this.target.worktreeId === target.worktreeId &&
      this.target.path === target.path &&
      this.target.changes === target.changes &&
      this.target.view === target.view;
    if (same && this.rowCount === rowCount) return;
    this.epoch += 1;
    this.target = target;
    this.rowCount = rowCount;
    this.chunks.clear();
    this.pending.clear();
    this.looseRows.clear();
    this.sliceInFlight = false;
    this.direction = 1;
    this.publish(null, entry);
  }

  /** 只换视图时保留方向判断，但清空块缓存。 */
  isCurrent(target: RowWindowTarget): boolean {
    return (
      !!this.target &&
      this.target.worktreeId === target.worktreeId &&
      this.target.path === target.path &&
      this.target.changes === target.changes &&
      this.target.view === target.view
    );
  }

  get rowCountValue(): number {
    return this.rowCount;
  }

  getRow(index: number): RenderRow | null {
    if (index < 0 || index >= this.rowCount) return null;
    const chunkIndex = Math.floor(index / CHUNK_ROWS);
    const chunk = this.chunks.get(chunkIndex);
    if (!chunk) return this.looseRows.get(index) ?? null;
    return chunk[index - chunkIndex * CHUNK_ROWS] ?? null;
  }

  /** 范围内有多少行还没有数据。 */
  uncovered(from: number, to: number): number {
    let missing = 0;
    const start = Math.max(0, from);
    const end = Math.min(this.rowCount, to);
    for (let index = start; index < end; index += 1) {
      if (!this.getRow(index)) missing += 1;
    }
    return missing;
  }

  isCovered(from: number, to: number): boolean {
    return this.uncovered(from, to) === 0;
  }

  /** 通知滚动方向，用于决定预取哪一侧。 */
  noteDirection(delta: number): void {
    if (delta > 0) this.direction = 1;
    else if (delta < 0) this.direction = -1;
  }

  /**
   * 保证 `[from, to)` 有数据，并沿滚动方向预取一块。
   * 返回是否已经齐备（不等待网络/进程往返）。
   */
  ensure(from: number, to: number, prefetchScreens = 2): boolean {
    if (!this.target) {
      logToBackend(`ensure 被调用但还没有目标文件（${from}..${to}）`);
      return false;
    }
    const clampedFrom = Math.max(0, Math.min(from, this.rowCount));
    const clampedTo = Math.max(clampedFrom, Math.min(to, this.rowCount));
    this.visibleFrom = clampedFrom;
    this.visibleTo = clampedTo;
    const covered = this.isCovered(clampedFrom, clampedTo);

    // 预取按块对齐：滚动方向上的下 PREFETCH_CHUNKS 块（按行预取会随视口大小抖动）。
    const chunkCount = this.rowCount > 0 ? Math.ceil(this.rowCount / CHUNK_ROWS) : 0;
    const visibleFirst = Math.floor(clampedFrom / CHUNK_ROWS);
    const visibleLast = Math.floor(Math.max(clampedFrom, clampedTo - 1) / CHUNK_ROWS);
    const wantedVisible: number[] = [];
    const wantedPrefetch: number[] = [];
    const push = (target: number[], index: number) => {
      if (index < 0 || index >= chunkCount) return;
      if (this.chunks.has(index) || this.pending.has(index) || target.includes(index)) return;
      target.push(index);
    };
    for (let index = visibleFirst; index <= visibleLast; index += 1) push(wantedVisible, index);
    for (let step = 1; step <= Math.max(1, Math.round(prefetchScreens) * PREFETCH_CHUNKS / 2); step += 1) {
      if (this.direction >= 0) push(wantedPrefetch, visibleLast + step);
      else push(wantedPrefetch, visibleFirst - step);
    }

    // 可见范围有缺口：先单独取可见的几十行，整块随后补上（Q52）。
    if (!covered) void this.requestVisibleSlice(clampedFrom, clampedTo);
    // 可见块必须取，不受在途上限限制；预取才排队。
    for (const chunkIndex of wantedVisible) void this.requestChunk(chunkIndex);
    for (const chunkIndex of wantedPrefetch) {
      if (this.pending.size >= MAX_IN_FLIGHT) break;
      void this.requestChunk(chunkIndex);
    }
    // 小文件：可见范围齐备后，在后台由近及远把其余块取完（不与首屏争抢 IPC）。
    if (covered && this.rowCount <= FULL_PREFETCH_MAX_ROWS) {
      const center = (visibleFirst + visibleLast) / 2;
      const missing: number[] = [];
      for (let index = 0; index < chunkCount; index += 1) {
        if (!this.chunks.has(index) && !this.pending.has(index)) missing.push(index);
      }
      missing.sort((a, b) => Math.abs(a - center) - Math.abs(b - center));
      for (const chunkIndex of missing) {
        if (this.pending.size >= MAX_IN_FLIGHT) break;
        void this.requestChunk(chunkIndex);
      }
    }
    return covered;
  }

  /**
   * 大幅跳转：立即请求目标位置。跳转前已在途的块不作废，返回后照常缓存——
   * 请求发出后无法真正取消，丢弃结果只会浪费已经花掉的取数时间。
   */
  prioritize(from: number, to: number): void {
    this.ensure(from, to, 0);
  }

  /** 单独取可见范围（上下各多取几行），用于整块到达之前先把文字显示出来。 */
  private async requestVisibleSlice(visibleFrom: number, visibleTo: number): Promise<void> {
    const target = this.target;
    if (!target || this.sliceInFlight) return;
    const epoch = this.epoch;
    const from = Math.max(0, visibleFrom - VISIBLE_SLICE_MARGIN);
    const to = Math.min(this.rowCount, visibleTo + VISIBLE_SLICE_MARGIN);
    if (to <= from) return;
    this.sliceInFlight = true;
    try {
      const slice = await this.fetchRows({ ...target, from, to });
      if (epoch !== this.epoch) return;
      if (this.looseRows.size + slice.rows.length > MAX_LOOSE_ROWS) this.looseRows.clear();
      slice.rows.forEach((row, offset) => {
        const index = from + offset;
        if (!this.chunks.has(Math.floor(index / CHUNK_ROWS))) this.looseRows.set(index, row);
      });
      if (slice.rowCount !== this.rowCount) this.rowCount = slice.rowCount;
      this.publish(null, slice.entry);
    } catch (error) {
      if (epoch !== this.epoch) return;
      logToBackend(`取可见行 ${from}-${to} 失败：${getErrorMessage(error)}`);
      this.publish(getErrorMessage(error));
    } finally {
      if (epoch === this.epoch) this.sliceInFlight = false;
    }
  }

  private async requestChunk(chunkIndex: number): Promise<void> {
    const target = this.target;
    if (!target) return;
    const epoch = this.epoch;
    const key = this.nextKey++;
    this.pending.set(chunkIndex, key);
    this.publish(null);
    const from = chunkIndex * CHUNK_ROWS;
    const to = Math.min(from + CHUNK_ROWS, this.rowCount);
    try {
      const slice = await this.fetchRows({ ...target, from, to });
      if (epoch !== this.epoch) return;
      logToBackend(`取块 ${chunkIndex}（${from}-${to}）完成：${slice.rows.length} 行`);
      this.chunks.set(chunkIndex, slice.rows);
      // 整块到了，块内单独取过的行不再需要。
      for (let index = from; index < to; index += 1) this.looseRows.delete(index);
      this.evictFarChunks();
      // 行数可能与列表元数据不同（未跟踪文件在打开时才统计），以服务端为准。
      if (slice.rowCount !== this.rowCount) {
        this.rowCount = slice.rowCount;
      }
      this.publish(null, slice.entry);
    } catch (error) {
      if (epoch !== this.epoch) return;
      logToBackend(`取块 ${chunkIndex} 失败：${getErrorMessage(error)}`);
      this.publish(getErrorMessage(error));
    } finally {
      if (epoch === this.epoch) {
        this.pending.delete(chunkIndex);
        this.publish(null);
      }
    }
  }

  /** 缓存超过上限时，从离可见区域最远的块开始丢。 */
  private evictFarChunks(): void {
    if (this.chunks.size <= MAX_CHUNKS) return;
    const center = this.visibleTo > this.visibleFrom ? (this.visibleFrom + this.visibleTo) / 2 : this.visibleFrom;
    const keepFrom = Math.floor(this.visibleFrom / CHUNK_ROWS);
    const keepTo = Math.floor(Math.max(this.visibleFrom, this.visibleTo - 1) / CHUNK_ROWS);
    const candidates = [...this.chunks.keys()]
      .filter((index) => index < keepFrom || index > keepTo)
      .map((index) => ({ index, distance: Math.abs(index * CHUNK_ROWS + CHUNK_ROWS / 2 - center) }))
      .sort((a, b) => b.distance - a.distance);
    while (this.chunks.size > MAX_CHUNKS && candidates.length > 0) {
      const victim = candidates.shift();
      if (victim) this.chunks.delete(victim.index);
    }
  }

  private publish(error: string | null, entry: ChangeEntry | null = null): void {
    this.version += 1;
    this.snapshot = {
      version: this.version,
      rowCount: this.rowCount,
      chunkCount: this.rowCount > 0 ? Math.ceil(this.rowCount / CHUNK_ROWS) : 0,
      loadedChunks: this.chunks.size,
      pendingChunks: this.pending.size,
      error: error ?? this.snapshot.error,
    };
    if (entry) this.entry = entry;
    for (const listener of this.listeners) listener();
  }

  /** 诊断：把当前窗口状态写进日志。 */
  logWindow(): void {
    logToBackend(
      `窗口：target=${this.target ? `${this.target.path}/${this.target.view}/${this.target.changes}` : "(无)"} ` +
        `rowCount=${this.rowCount} 已加载块=${this.chunks.size} 在途=${this.pending.size}`,
    );
  }

  get latestEntry(): ChangeEntry | null {
    return this.entry;
  }
}
