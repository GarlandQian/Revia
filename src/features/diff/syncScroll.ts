/**
 * 左右对照的滚动同步映射（返修 A1/Q138、设计 4.2）。
 *
 * 两栏各自连续显示整个文件，不插空行；同步滚动只做一件事：
 * 把主动栏可视区中线所在的行（浮点）映射到跟随栏的行，再换成滚动位置。
 *
 * 映射规则（设计 4.2）：
 * 1. 找最后一个 `end <= line` 的段（二分）；不在任何段内时按该段两侧 end 的差值平移；
 *    在所有段之前时原样返回。
 * 2. 行落在段 `[fromStart, fromEnd)` 内时按比例换算。
 * 3. 一侧长度为 0 的段（纯新增 / 纯删除）映射到该段的起点。
 */

/** 一段改动在两侧的行区间（0 基左闭右开，文件全文行号）。 */
export interface SyncSegment {
  leftStart: number;
  leftEnd: number;
  rightStart: number;
  rightEnd: number;
}

export type Side = "left" | "right";

function startOf(segment: SyncSegment, side: Side): number {
  return side === "left" ? segment.leftStart : segment.rightStart;
}

function endOf(segment: SyncSegment, side: Side): number {
  return side === "left" ? segment.leftEnd : segment.rightEnd;
}

/** 把一侧的行号（浮点）映射到另一侧。 */
export function mapLine(from: Side, line: number, segments: SyncSegment[]): number {
  if (!Number.isFinite(line) || segments.length === 0) return line;
  // 二分：最后一个 fromEnd <= line 的段。
  let low = 0;
  let high = segments.length - 1;
  let found = -1;
  while (low <= high) {
    const mid = (low + high) >> 1;
    if (endOf(segments[mid], from) <= line) {
      found = mid;
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }
  const other: Side = from === "left" ? "right" : "left";
  // 段内：二分找到的段之后的那一段可能仍然覆盖 line。
  const inside = segments[found + 1];
  if (inside) {
    const insideStart = startOf(inside, from);
    const insideEnd = endOf(inside, from);
    if (insideStart <= line && line < insideEnd) {
      const span = Math.max(1, insideEnd - insideStart);
      return startOf(inside, other)
        + ((line - insideStart) * (endOf(inside, other) - startOf(inside, other))) / span;
    }
  }
  if (found < 0) return line;
  const segment = segments[found];
  const fromStart = startOf(segment, from);
  const fromEnd = endOf(segment, from);
  // 零长度段（纯新增 / 纯删除）：落在插入点上时映射到该段的起点。
  if (fromEnd === fromStart && line === fromEnd) return startOf(segment, other);
  return line - fromEnd + endOf(segment, other);
}

/** 夹在 `[0, max]` 之间（滚动位置用）。 */
export function clampLine(line: number, max: number): number {
  if (!Number.isFinite(line)) return 0;
  return Math.min(Math.max(0, line), Math.max(0, max));
}
