/**
 * 提交 diff 的语法高亮（M5 L05）。
 *
 * 复用任务 01 的只读高亮命令：内容由 Rust 按对象 ID 读取，前端只给已拿到的
 * `HistoryFileTarget`、版本与行范围。类别索引到 CSS 类的映射与工作区 diff 一致，
 * 因此浅色 / 深色主题共用同一份结果。
 */

import type { FileVersion } from "@/types/bindings/FileVersion";
import type { HistoryFileTarget } from "@/types/bindings/HistoryFileTarget";
import { ipc, type HighlightLine } from "@/lib/ipc";

/** 语法类别名（按索引顺序）；整个应用只需取一次。 */
let categoriesPromise: Promise<string[]> | null = null;

export function syntaxCategories(): Promise<string[]> {
  categoriesPromise ??= ipc.syntaxCategories().catch(() => [] as string[]);
  return categoriesPromise;
}

/** 类别索引 → CSS 类（`tok-<name>`，与工作区 diff 同一套变量）。 */
export function categoryClass(categories: string[], index: number): string {
  const name = categories[index] ?? "plain";
  return `tok-${name}`;
}

/** 一次请求的行数。 */
export const HIGHLIGHT_CHUNK = 300;

export interface HighlightRange {
  from: number;
  to: number;
  lines: HighlightLine[];
}

/**
 * 读取某个版本的一段高亮（行范围 0 基、左闭右开）。
 *
 * 失败时返回空数组：高亮是可选增强，不能因为它挡住正文显示。
 */
export async function loadHighlight(
  targetId: string,
  file: HistoryFileTarget,
  version: FileVersion,
  fromLine: number,
  toLine: number,
): Promise<Map<number, HighlightLine>> {
  const result = new Map<number, HighlightLine>();
  const from = Math.max(0, fromLine);
  const to = Math.max(from, toLine);
  if (to <= from) return result;
  try {
    for (let start = from; start < to; start += HIGHLIGHT_CHUNK) {
      const end = Math.min(to, start + HIGHLIGHT_CHUNK);
      const highlighted = await ipc.historyFileHighlight({
        targetId,
        file,
        version,
        fromLine: start,
        toLine: end,
      });
      if (highlighted.kind !== "highlighted") continue;
      highlighted.lines.forEach((line, index) => {
        if (line) result.set(start + index, line);
      });
    }
  } catch {
    return new Map();
  }
  return result;
}
