/**
 * 两条高亮通路的前端封装：
 * - `ShikiHighlighter`：Web Worker 里的 Shiki，对完整文件一次性高亮；
 * - `SyntectHighlighter`：Rust 侧 syntect，带解析状态缓存，按行范围分块返回。
 *
 * 两者接口一致：给定文件内容和语言，返回按 1 基行号索引的 token 表。
 * 结果按“文件 + 版本”缓存，滚动时直接复用。
 */

import { invoke } from "@tauri-apps/api/core";

/** 把关键步骤写进后端日志，便于排查（与 App 里的同名函数一致）。 */
function debugLog(message: string) {
  void invoke("debug_log", { message: `${new Date().toISOString()} ${message}` }).catch(() => {});
}
import type { HighlightLine, HighlightedDocument, HighlightToken } from "../diff/types";

export interface HighlightRequest {
  path: string;
  language: string;
  content: string;
  /** 0 基、左闭右开；不传表示整份文件。 */
  range?: { from: number; to: number | null };
  signal?: AbortSignal;
}

export interface HighlightOutcome {
  /** 按 1 基行号索引的 token 表。 */
  byLine: Map<number, HighlightToken[]>;
  /** 本次覆盖的 1 基行号区间（含两端）。 */
  covered: { from: number; to: number; supported: boolean };
  engine: "shiki" | "syntect";
  /** 从请求发出到结果可用。 */
  elapsedMs: number;
  /** 引擎自身报告的耗时（syntect 来自 Rust，shiki 来自 Worker）。 */
  engineMs: number | null;
  /** Rust 侧从第 0 行解析到本次范围末尾的累计耗时。 */
  parseMs: number | null;
  syntax: string | null;
  theme: string | null;
}

export interface HighlightedFile {
  document: HighlightedDocument;
  oldMs: number;
  newMs: number;
  chunkTimings: { side: "old" | "new"; from: number; to: number | null; wallMs: number }[];
}

export interface Highlighter {
  readonly mode: "shiki" | "syntect";
  highlight(request: HighlightRequest): Promise<HighlightOutcome>;
}

let worker: Worker | null = null;
let nextRequestId = 1;

function getWorker(): Worker {
  if (!worker) {
    worker = new Worker(new URL("./shiki.worker.ts", import.meta.url), { type: "module" });
  }
  return worker;
}

class ShikiHighlighter implements Highlighter {
  readonly mode = "shiki" as const;

  highlight(request: HighlightRequest): Promise<HighlightOutcome> {
    return new Promise((resolve, reject) => {
      const id = nextRequestId++;
      const instance = getWorker();
      const started = performance.now();
      const supported = request.language !== "txt" && request.language !== "text";

      // Worker 可能分片返回：逐片合并，最后一片到达才算完成。
      const collected = new Map<number, HighlightLine>();
      let collectedCount = 0;
      let expected = Number.POSITIVE_INFINITY;
      const onMessage = (event: MessageEvent) => {
        const data = event.data as {
          id: number;
          ok: boolean;
          error?: string;
          tookMs?: number;
          lines?: HighlightLine[];
          from?: number;
          more?: boolean;
          supported?: boolean;
          log?: string;
          syntax?: string;
          theme?: string;
        };
        if (data.id !== id) return;
        if (data.log) {
          debugLog(`shiki ${data.log}`);
          return;
        }
        if (!data.ok) {
          instance.removeEventListener("message", onMessage);
          reject(new Error(data.error ?? "Shiki 高亮失败"));
          return;
        }
        const from = data.from ?? 0;
        const lines = data.lines ?? [];
        for (let index = 0; index < lines.length; index += 1) {
          collected.set(from + index, lines[index]);
        }
        collectedCount += lines.length;
        expected = from + lines.length + (data.more ? 0 : 0);
        if (data.more) {
          // 继续等下一片。
          return;
        }
        instance.removeEventListener("message", onMessage);
        if (expected === 0) {
          expected = collectedCount;
        }
        const merged: HighlightLine[] = [];
        const limit = Math.max(...collected.keys(), -1) + 1;
        for (let index = 0; index < limit; index += 1) {
          merged.push(collected.get(index) ?? null);
        }
        resolve(
          buildOutcome(
            "shiki",
            merged,
            0,
            performance.now() - started,
            data.tookMs ?? null,
            null,
            data.supported ?? supported,
            data.syntax ?? request.language,
            data.theme ?? null,
          ),
        );
      };

      instance.addEventListener("message", onMessage);
      request.signal?.addEventListener("abort", () => {
        instance.removeEventListener("message", onMessage);
        reject(new DOMException("已取消", "AbortError"));
      });
      instance.postMessage({ id, language: request.language, content: request.content });
    });
  }
}

/** Rust 侧 `highlight_syntect` 的返回结构。 */
interface SyntectResult {
  from: number;
  lines: HighlightLine[];
  lineCount: number;
  syntax: string;
  theme: string;
  elapsedMs: number;
  parseMs: number;
  highlightedThrough: number;
  needFullScan: boolean;
  supported: boolean;
}

class SyntectHighlighter implements Highlighter {
  readonly mode = "syntect" as const;
  private generation = 0;

  async highlight(request: HighlightRequest): Promise<HighlightOutcome> {
    const started = performance.now();
    const from = request.range?.from ?? 0;
    const to = request.range?.to ?? null;
    // 行范围用字符串传：`"0-400"` / `"200-"`，与 Rust 侧的 LineRange 对齐。
    const lineRange = to == null ? `${from}-` : `${from}-${to}`;
    debugLog(`syntect 请求 ${request.path} 行 ${lineRange}`);
    // 诊断：把 invoke 和超时赛跑，避免“永远等待”看不出原因。
    const result = await Promise.race([
      invoke<SyntectResult>("highlight_syntect", {
      request: {
        language: request.language,
        content: request.content,
        theme: "light",
        lineRange,
        documentId: request.path,
        generation: this.generation,
      },
      }),
      new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error(`highlight_syntect 超过 60 秒未返回（${lineRange}）`)), 60_000),
      ),
    ]);
    if (result.needFullScan) {
      // 下一次请求同一文件的新版本时重新开始解析。
      this.generation += 1;
    }
    debugLog(`syntect 返回 ${request.path} 行 ${lineRange}：${result.lines.length} 行，${result.elapsedMs} ms`);
    return buildOutcome(
      "syntect",
      result.lines,
      result.from,
      performance.now() - started,
      result.elapsedMs,
      result.parseMs,
      result.supported,
      result.syntax,
      result.theme,
    );
  }
}

function buildOutcome(
  engine: "shiki" | "syntect",
  lines: HighlightLine[],
  fromLine: number,
  elapsedMs: number,
  engineMs: number | null,
  parseMs: number | null,
  supported: boolean,
  syntax: string | null = null,
  theme: string | null = null,
): HighlightOutcome {
  const byLine = new Map<number, HighlightToken[]>();
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    // 统一用 1 基行号，和 diff 行号保持一致。
    if (line && line.length > 0) byLine.set(fromLine + index + 1, line);
  }
  return {
    byLine,
    covered: { from: fromLine + 1, to: fromLine + lines.length, supported },
    engine,
    elapsedMs,
    engineMs,
    parseMs,
    syntax,
    theme,
  };
}

export function createHighlighter(mode: "shiki" | "syntect"): Highlighter {
  return mode === "shiki" ? new ShikiHighlighter() : new SyntectHighlighter();
}

/**
 * 分块顺序：先可见区域，再向前补齐，再向后补齐。
 * 每个分块都携带完整文件内容，由 Rust 侧的解析状态保证跨行结构正确。
 */
export function chunkRanges(
  totalLines: number,
  visibleFrom: number,
  visibleTo: number,
  chunkSize: number,
): { from: number; to: number | null }[] {
  const ranges: { from: number; to: number | null }[] = [];
  const from = Math.min(Math.max(visibleFrom, 0), totalLines);
  const to = Math.min(Math.max(visibleTo, from), totalLines);
  if (to > from) {
    ranges.push({ from, to });
  }
  for (let end = from; end > 0; ) {
    const start = Math.max(0, end - chunkSize);
    ranges.push({ from: start, to: end });
    end = start;
  }
  for (let start = to; start < totalLines; ) {
    const end = Math.min(totalLines, start + chunkSize);
    ranges.push({ from: start, to: end >= totalLines ? null : end });
    start = end;
  }
  return ranges;
}

/**
 * 对旧、新两个版本做高亮。`syntect` 会按分块请求逐步补齐；`shiki` 一次性完成。
 * `progress` 在每次分块完成后回调，便于界面显示进度。
 */
export async function highlightVersions(
  highlighter: Highlighter,
  requests: { old: HighlightRequest | null; new: HighlightRequest | null },
  options: {
    visibleFrom: number;
    visibleTo: number;
    chunkSize: number;
    totalLines: { old: number; new: number };
    onProgress?: (lines: number, total: number) => void;
    signal?: AbortSignal;
  },
): Promise<HighlightedFile> {
  const oldMap = new Map<number, HighlightToken[]>();
  const newMap = new Map<number, HighlightToken[]>();
  // 覆盖范围按“请求过的行区间”统计：空行没有 token，不能按 token 行数判断完整性。
  const covered = { old: 0, new: 0 };
  const chunkTimings: HighlightedFile["chunkTimings"] = [];
  const total = options.totalLines.old + options.totalLines.new;
  let oldMs = 0;
  let newMs = 0;
  let engineMs = 0;
  let syntax: string | null = null;
  let theme: string | null = null;

  const sides: { side: "old" | "new"; request: HighlightRequest | null; lines: number }[] = [
    { side: "old", request: requests.old, lines: options.totalLines.old },
    { side: "new", request: requests.new, lines: options.totalLines.new },
  ];

  for (const { side, request, lines } of sides) {
    if (!request) continue;
    const ranges =
      highlighter.mode === "syntect"
        ? chunkRanges(lines, options.visibleFrom, options.visibleTo, options.chunkSize)
        : [{ from: 0, to: null as number | null }];
    for (const range of ranges) {
      if (options.signal?.aborted) throw new DOMException("已取消", "AbortError");
      const started = performance.now();
      const outcome = await highlighter.highlight({ ...request, range });
      const wallMs = performance.now() - started;
      for (const [line, tokens] of outcome.byLine) {
        const target = side === "old" ? oldMap : newMap;
        if (!target.has(line)) target.set(line, tokens);
      }
      if (!outcome.covered.supported) {
        // 语言没有被识别：不继续请求剩余分块。
        break;
      }
      const span = Math.max(0, outcome.covered.to - outcome.covered.from + 1);
      const limit = side === "old" ? options.totalLines.old : options.totalLines.new;
      if (side === "old") covered.old += Math.min(span, limit);
      else covered.new += Math.min(span, limit);
      if (side === "old") oldMs += wallMs;
      else newMs += wallMs;
      engineMs += outcome.engineMs ?? 0;
      syntax = outcome.syntax ?? syntax;
      theme = outcome.theme ?? theme;
      chunkTimings.push({ side, from: range.from, to: range.to, wallMs });
      options.onProgress?.(oldMap.size + newMap.size, total);
    }
  }

  const complete =
    highlighter.mode === "shiki" ||
    syntax === "Plain Text" ||
    (covered.old >= options.totalLines.old && covered.new >= options.totalLines.new);

  return {
    document: {
      old: oldMap,
      new: newMap,
      engine: highlighter.mode,
      elapsedMs: oldMs + newMs,
      engineMs,
      syntax,
      theme,
      complete,
      highlightedLines: covered.old + covered.new,
      totalLines: total,
    },
    oldMs,
    newMs,
    chunkTimings,
  };
}

/** 文件扩展名 -> 两种引擎都能识别的语言名。 */
export function languageForPath(path: string): string {
  const name = path.split("/").pop() ?? path;
  const lower = name.toLowerCase();
  if (lower.endsWith(".lock") || lower === "pnpm-lock.yaml") return "yaml";
  const dot = lower.lastIndexOf(".");
  const extension = dot >= 0 ? lower.slice(dot + 1) : lower;
  const table: Record<string, string> = {
    ts: "ts",
    tsx: "tsx",
    mts: "ts",
    cts: "ts",
    js: "js",
    jsx: "jsx",
    mjs: "js",
    cjs: "js",
    vue: "vue",
    swift: "swift",
    json: "json",
    yaml: "yaml",
    yml: "yaml",
    md: "markdown",
    sh: "shellscript",
    zsh: "shellscript",
    rs: "rust",
    py: "python",
    css: "css",
    html: "html",
    log: "txt",
    txt: "txt",
  };
  return table[extension] ?? "txt";
}

/** 默认收起：lock 文件、二进制文件与超大文件。 */
export function isCollapsedByDefault(path: string, lineCount: number, binary: boolean): boolean {
  if (binary) return true;
  const name = (path.split("/").pop() ?? path).toLowerCase();
  if (name.endsWith(".lock") || name === "pnpm-lock.yaml" || name === "package-lock.json" || name === "yarn.lock") {
    return true;
  }
  return lineCount > LARGE_FILE_LINES;
}

export const LARGE_FILE_LINES = 2000;
