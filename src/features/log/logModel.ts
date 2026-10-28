/**
 * 日志的筛选与显示模型（M5 L02、L04，纯函数）。
 *
 * 前端只把结构化条件交给 Rust：这里负责把界面状态整理成 `HistoryFilter`、
 * 生成查询起点与筛选标识，并把提交日期 / 引用标签格式化成界面文字。
 * 不拼 Git 参数，也不决定哪些提交可见（那是 Rust 的事）。
 *
 * 纯模块：`tools/test-log-model.mjs` 直接测这些函数。
 */

import type { HistoryCommit } from "@/types/bindings/HistoryCommit";
import type { HistoryDateFilter } from "@/types/bindings/HistoryDateFilter";
import type { HistoryFilter } from "@/types/bindings/HistoryFilter";
import type { HistoryRefLabel } from "@/types/bindings/HistoryRefLabel";
import type { HistoryStart } from "@/types/bindings/HistoryStart";
import type { HistoryRefKind } from "@/types/bindings/HistoryRefKind";

export function logVisibleEnd(scrollTop: number, viewport: number, rowHeight: number): number {
  return Math.ceil((scrollTop + viewport) / rowHeight);
}

/** An unloaded page is not evidence that the selected commit left the query. */
export function selectedLogCommit(commits: HistoryCommit[], selected: string | null, complete: boolean): string | null {
  if (selected && (!complete || commits.some((commit) => commit.oid === selected))) return selected;
  return commits[0]?.oid ?? null;
}

/** 日期筛选的界面预设（Rebased 的最近 1 / 7 / 30 天与自定义区间）。 */
export type DatePreset = "all" | "last1" | "last7" | "last30" | "range";

/** 日志筛选的界面状态；按 worktree 保留（L01）。 */
export interface LogFilterState {
  text: string;
  caseSensitive: boolean;
  regex: boolean;
  /** 选中的分支 / 标签完整引用名；空数组表示“全部”。 */
  branches: string[];
  /** 选中的作者标识（`名字 <邮箱>`）。 */
  authors: string[];
  date: DatePreset;
  /** 自定义起止日期（`YYYY-MM-DD`），仅 `date === "range"` 时使用。 */
  since: string;
  until: string;
  /** 选中的路径（字面匹配）。 */
  paths: string[];
}

export function defaultFilter(): LogFilterState {
  return {
    text: "",
    caseSensitive: false,
    regex: false,
    branches: [],
    authors: [],
    date: "all",
    since: "",
    until: "",
    paths: [],
  };
}

/** 输入是否看起来像哈希前缀（与 Rust 侧同一个判定：至少 4 位十六进制）。 */
export function looksLikeHash(text: string): boolean {
  return text.trim().length >= 4 && /^[0-9a-f]+$/i.test(text.trim());
}

/** 把界面状态整理成后端筛选条件（空项一律省略）。 */
export function toHistoryFilter(state: LogFilterState, now: number = Date.now()): HistoryFilter {
  const text = state.text.trim();
  const date = dateFilterOf(state, now);
  return {
    text: text.length > 0 ? text : null,
    caseSensitive: state.caseSensitive,
    regex: state.regex,
    branches: [...state.branches],
    authors: [...state.authors],
    paths: state.paths.map((path) => path.trim()).filter((path) => path.length > 0),
    date,
  };
}

/** 日期预设 → 结构化日期筛选（最近 N 天由 Rust 在查询开始时换算成固定时间点）。 */
export function dateFilterOf(state: LogFilterState, now: number = Date.now()): HistoryDateFilter | null {
  switch (state.date) {
    case "last1":
      return { kind: "lastDays", days: 1 };
    case "last7":
      return { kind: "lastDays", days: 7 };
    case "last30":
      return { kind: "lastDays", days: 30 };
    case "range": {
      if (!isDateText(state.since) || !isDateText(state.until)) return null;
      return { kind: "range", since: state.since, until: state.until };
    }
    default:
      void now;
      return null;
  }
}

/** `YYYY-MM-DD`（只做形状检查；真正的校验在 Rust）。 */
export function isDateText(text: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(text.trim());
}

/**
 * 查询起点：日志总是以“全部引用的固定快照”开始（L04 默认分支筛选）。
 * 起始引用列表与 `filter.branches` 一致，Rust 侧以它作为唯一来源。
 */
export function filterStart(_state: LogFilterState): HistoryStart {
  return { kind: "refs", fullNames: [] };
}

/** 筛选标识：变化时必须重新查询（游标不能跨筛选复用）。 */
export function filterKey(state: LogFilterState): string {
  return JSON.stringify({
    text: state.text.trim(),
    caseSensitive: state.caseSensitive,
    regex: state.regex,
    branches: [...state.branches].sort(),
    authors: [...state.authors].sort(),
    date: state.date,
    since: state.since,
    until: state.until,
    paths: [...state.paths].sort(),
  });
}

/** 有哪几项筛选在生效（界面用来显示“重置筛选”与筛选数量）。 */
export function activeFilterCount(state: LogFilterState): number {
  let count = 0;
  if (state.text.trim().length > 0) count += 1;
  if (state.branches.length > 0) count += 1;
  if (state.authors.length > 0) count += 1;
  if (state.date !== "all") count += 1;
  if (state.paths.length > 0) count += 1;
  return count;
}

export function hasActiveFilter(state: LogFilterState): boolean {
  return activeFilterCount(state) > 0;
}

/** 提交哈希的短形式（7 位，与页签标题一致）。 */
export function shortOid(oid: string): string {
  return oid.slice(0, 7);
}

const DAY_MS = 86_400_000;

function pad(value: number): string {
  return value < 10 ? `0${value}` : String(value);
}

/**
 * 作者时间在本地时区的显示（L02）。
 *
 * 今天 / 昨天显示“今天 14:22”“昨天 09:05”，其余显示“2026/10/27 14:22”。
 * 只依赖本机时区，不重新解释提交里记录的时区（与 Rebased 的本地时间显示一致）。
 */
export function formatCommitDate(unixSeconds: number, now: number = Date.now()): string {
  if (!Number.isFinite(unixSeconds) || unixSeconds <= 0) return "";
  const date = new Date(unixSeconds * 1000);
  const current = new Date(now);
  const time = `${pad(date.getHours())}:${pad(date.getMinutes())}`;
  const startOfToday = new Date(current.getFullYear(), current.getMonth(), current.getDate()).getTime();
  const day = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const diffDays = Math.round((startOfToday - day) / DAY_MS);
  if (diffDays === 0) return `今天 ${time}`;
  if (diffDays === 1) return `昨天 ${time}`;
  return `${date.getFullYear()}/${pad(date.getMonth() + 1)}/${pad(date.getDate())} ${time}`;
}

export interface RefChip {
  kind: HistoryRefKind;
  label: string;
  fullName: string;
  /** 当前分支：突出显示。 */
  current: boolean;
}

/**
 * 提交上的引用标签排序与突出显示（L02）。
 *
 * 顺序：HEAD > 当前分支 > 本地分支 > 远端分支 > 标签 > 其他；同组按名字。
 */
export function refChips(refs: HistoryRefLabel[], currentBranch: string | null): RefChip[] {
  const currentRef = currentBranch ? `refs/heads/${currentBranch}` : null;
  const weight = (kind: HistoryRefKind): number => {
    switch (kind) {
      case "branch":
        return 2;
      case "remoteBranch":
        return 3;
      case "tag":
        return 4;
      default:
        return 5;
    }
  };
  return refs
    .map((ref) => ({
      kind: ref.kind,
      label: ref.shortName,
      fullName: ref.fullName,
      current: currentRef !== null && ref.fullName === currentRef,
    }))
    .sort((left, right) => {
      const leftHead = left.label === "HEAD" ? 0 : left.current ? 1 : weight(left.kind);
      const rightHead = right.label === "HEAD" ? 0 : right.current ? 1 : weight(right.kind);
      if (leftHead !== rightHead) return leftHead - rightHead;
      return left.label.localeCompare(right.label);
    });
}

/** 作者筛选候选项的标识（`名字 <邮箱>`；与 Git 的 `--author` 匹配同一串文字）。 */
export function authorKey(commit: Pick<HistoryCommit, "author">): string {
  const name = commit.author.name.trim();
  const email = commit.author.email.trim();
  if (name && email) return `${name} <${email}>`;
  return name || email;
}

/** 作者名字的显示（表格里的“作者”列）。 */
export function authorLabel(commit: Pick<HistoryCommit, "author">): string {
  return commit.author.name.trim() || commit.author.email.trim();
}

/** 作者颜色：按名字稳定取色，方便在长列表里区分（Rebased 同样按作者着色）。 */
export function authorColorIndex(seed: string, buckets: number): number {
  let hash = 0;
  for (let index = 0; index < seed.length; index += 1) {
    hash = (hash * 31 + seed.charCodeAt(index)) % 100_000;
  }
  return hash % Math.max(1, buckets);
}

/** 分支筛选的候选项：本地分支、远端分支、标签（来自历史目标打开时的引用快照）。 */
export interface BranchFilterOption {
  fullName: string;
  label: string;
  kind: HistoryRefKind;
  group: "local" | "remote" | "tag";
}

export function branchFilterOptions(
  refs: Array<{ fullName: string; shortName: string; kind: HistoryRefKind }>,
): BranchFilterOption[] {
  const options: BranchFilterOption[] = [];
  for (const ref of refs) {
    if (ref.kind === "branch") {
      options.push({ fullName: ref.fullName, label: ref.shortName, kind: ref.kind, group: "local" });
    } else if (ref.kind === "remoteBranch") {
      options.push({ fullName: ref.fullName, label: ref.shortName, kind: ref.kind, group: "remote" });
    } else if (ref.kind === "tag") {
      options.push({ fullName: ref.fullName, label: ref.shortName, kind: ref.kind, group: "tag" });
    }
  }
  options.sort((left, right) => left.label.localeCompare(right.label));
  return options;
}

/** 提交 diff 页签的键：同一提交同一文件只开一个页签（L01）。 */
export function commitTabKey(commit: string, path: string): string {
  return `commit:${commit}:${path}`;
}

/** 提交 diff 页签标题：“文件名（短哈希）”（L01）。 */
export function commitTabTitle(path: string, commit: string): string {
  const slash = path.lastIndexOf("/");
  const name = slash >= 0 ? path.slice(slash + 1) : path;
  return `${name}（${shortOid(commit)}）`;
}
