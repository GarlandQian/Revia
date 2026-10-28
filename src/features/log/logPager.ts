/**
 * 日志分页（M5 L02）。
 *
 * 起点与筛选在查询开始时由 Rust 固定；这里只负责：
 * - 滚到接近末尾时再取下一页（不一次读完整个历史）；
 * - 按 OID 去重（拓扑顺序下同一提交不会重复，但错配的响应也不能重复插入）；
 * - 保留“超过本次查询跨度上限”这类明确状态，不把它伪装成“历史末尾”；
 * - 旧筛选 / 旧 target 的晚到响应不能写进新结果。
 *
 * 纯模块：`tools/test-log-model.mjs` 用注入的加载函数测试。
 */

import type { HistoryPage } from "@/types/bindings/HistoryPage";
import type { HistoryCommit } from "@/types/bindings/HistoryCommit";
import type { HistoryStart } from "@/types/bindings/HistoryStart";
import type { HistoryFilter } from "@/types/bindings/HistoryFilter";

/** 一页的条数（Rust 侧上限 200；这里取 100，滚动时增量加载）。 */
export const LOG_PAGE_SIZE = 100;

/** 距离末尾多少行时开始取下一页。 */
export const LOG_PREFETCH_ROWS = 30;

export interface LogPageRequest {
  targetId: string;
  start?: HistoryStart;
  firstParent: boolean;
  limit: number;
  cursor?: string;
  filter: HistoryFilter;
}

export type LogPageLoader = (request: LogPageRequest) => Promise<HistoryPage>;

export interface LogSnapshot {
  commits: HistoryCommit[];
  /** 是否已经确实读到历史末尾。 */
  exhausted: boolean;
  /** 是否因为超过查询跨度上限而停止（与“末尾”不同）。 */
  spanExceeded: boolean;
  loading: boolean;
  /** 加载失败的原因（原始文字）。 */
  error: string | null;
  /** 当前查询的筛选标识：换了筛选就重新开始。 */
  queryKey: string;
}

export interface LogPagerOptions {
  loader: LogPageLoader;
  /** 查询目标（历史目标 ID）；为 null 时不查询。 */
  targetId: string | null;
  /** 每次取一页的条数。 */
  pageSize?: number;
}

/** 分页器：由界面状态驱动，不含 React。 */
export class LogPager {
  private readonly loader: LogPageLoader;
  private targetId: string | null;
  private readonly pageSize: number;
  private commits: HistoryCommit[] = [];
  private cursor: string | null = null;
  private exhausted = false;
  private spanExceeded = false;
  private loading = false;
  private error: string | null = null;
  private queryKey = "";
  /** 每次重置递增：晚到的旧响应据此丢弃。 */
  private generation = 0;
  private listener: ((snapshot: LogSnapshot) => void) | null = null;

  constructor(options: LogPagerOptions) {
    this.loader = options.loader;
    this.targetId = options.targetId;
    this.pageSize = options.pageSize ?? LOG_PAGE_SIZE;
  }

  setListener(listener: ((snapshot: LogSnapshot) => void) | null): void {
    this.listener = listener;
    listener?.(this.snapshot());
  }

  snapshot(): LogSnapshot {
    return {
      commits: this.commits,
      exhausted: this.exhausted,
      spanExceeded: this.spanExceeded,
      loading: this.loading,
      error: this.error,
      queryKey: this.queryKey,
    };
  }

  /**
   * 切换历史目标（仓库 / worktree 变化）：立即清空，不在旧筛选下抢先查询。
   * 随后的 `reset(筛选标识, { force: true })` 会用新目标开始查询。
   */
  setTarget(targetId: string | null): void {
    if (this.targetId === targetId) return;
    this.targetId = targetId;
    this.generation += 1;
    this.queryKey = "";
    this.commits = [];
    this.cursor = null;
    this.exhausted = false;
    this.spanExceeded = false;
    this.loading = false;
    this.error = null;
    this.emit();
  }

  /**
   * 重新开始一次查询：筛选变化、引用变化、用户点“重试”时调用。
   *
   * `queryKey` 相同且已有数据时不动（避免无意义的闪烁与滚动复位）。
   */
  reset(queryKey: string, options: { force?: boolean } = {}): void {
    if (!options.force && queryKey === this.queryKey && (this.commits.length > 0 || this.loading)) {
      return;
    }
    this.generation += 1;
    this.queryKey = queryKey;
    this.commits = [];
    this.cursor = null;
    this.exhausted = false;
    this.spanExceeded = false;
    this.loading = false;
    this.error = null;
    this.emit();
    if (this.targetId) void this.load();
  }

  /** 取下一页；已经到末尾、正在加载或已超出跨度时什么也不做。 */
  async loadMore(): Promise<void> {
    await this.load();
  }

  /** 接近末尾时调用（滚动触发）。 */
  maybeLoadMore(visibleEnd: number): void {
    if (this.commits.length - visibleEnd <= LOG_PREFETCH_ROWS) void this.load();
  }

  private async load(): Promise<void> {
    const targetId = this.targetId;
    if (!targetId || this.loading || this.exhausted || this.spanExceeded) return;
    const token = this.generation;
    this.loading = true;
    this.error = null;
    this.emit();
    try {
      const page = await this.loader({
        targetId,
        start: this.cursor === null ? { kind: "refs", fullNames: [] } : undefined,
        firstParent: this.firstParent,
        limit: this.pageSize,
        cursor: this.cursor ?? undefined,
        filter: this.filter,
      });
      if (token !== this.generation || targetId !== this.targetId) return;
      const known = new Set(this.commits.map((commit) => commit.oid));
      const added: HistoryCommit[] = [];
      for (const commit of page.commits) {
        if (!known.has(commit.oid)) {
          known.add(commit.oid);
          added.push(commit);
        }
      }
      // Published snapshots must not change underneath React's dependency checks.
      if (added.length > 0) this.commits = [...this.commits, ...added];
      this.cursor = page.nextCursor;
      this.exhausted = page.exhausted;
      this.loading = false;
      this.emit();
    } catch (caught) {
      if (token !== this.generation || targetId !== this.targetId) return;
      this.loading = false;
      const message = errorMessage(caught);
      // 超过跨度上限是明确状态：不显示为“历史末尾”，也不自动重试。
      this.spanExceeded = message.includes("上限") || message.includes("跨度");
      this.error = message;
      this.exhausted = true;
      this.emit();
    }
  }

  /** 由界面设置当前查询使用的遍历方式与筛选（供下一次请求使用）。 */
  private filter: HistoryFilter = {
    text: null,
    caseSensitive: false,
    regex: false,
    branches: [],
    authors: [],
    paths: [],
    date: null,
  };
  private firstParent = false;

  setQuery(filter: HistoryFilter, firstParent: boolean): void {
    this.filter = filter;
    this.firstParent = firstParent;
  }

  /** 选中提交变化后，最新列表里是否还有它（L01：仍在结果中时保持选中）。 */
  retains(oid: string | null): boolean {
    return oid === null ? true : this.commits.some((commit) => commit.oid === oid);
  }

  private emit(): void {
    this.listener?.(this.snapshot());
  }
}

/** 把未知错误整理成可显示的文字（不发请求、不做映射）。 */
export function errorMessage(caught: unknown): string {
  if (caught && typeof caught === "object" && "message" in caught) {
    const message = (caught as { message?: unknown }).message;
    if (typeof message === "string" && message.length > 0) return message;
  }
  return String(caught);
}
