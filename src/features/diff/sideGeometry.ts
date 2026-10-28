import type { RowHeights } from "./rowHeights";
import type { RowIndex, Side } from "./rowIndex";
import { clampLine, mapLine, type SyncSegment } from "./syncScroll";

export interface SideGeometry {
  rows: RowIndex;
  heights: RowHeights;
  viewportHeight: number;
}

/** 像素位置 → 0 基文件位置，包含行内比例；折叠占位按隐藏的文件区间插值。 */
export function filePositionAtY(rows: RowIndex, heights: RowHeights, y: number): number {
  const position = heights.rowAtFraction(y);
  const row = Math.floor(position);
  const item = rows.rows[row];
  if (!item) return rows.lineCount;
  const fraction = position - row;
  return "line" in item
    ? item.line - 1 + fraction
    : item.start + fraction * (item.end - item.start);
}

/** 0 基文件位置 → 像素位置；EOF 在末行之后，折叠区里的位置落在占位行内。 */
export function yAtFilePosition(rows: RowIndex, heights: RowHeights, position: number): number {
  if (rows.rows.length === 0) return 0;
  const at = Math.min(rows.lineCount, Math.max(0, Number.isNaN(position) ? 0 : position));
  if (at === rows.lineCount) return heights.total();
  let low = 0;
  let high = rows.rows.length - 1;
  let found = 0;
  while (low <= high) {
    const mid = (low + high) >> 1;
    const item = rows.rows[mid];
    const start = "line" in item ? item.line - 1 : item.start;
    if (start <= at) {
      found = mid;
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }
  const item = rows.rows[found];
  const fraction = "line" in item
    ? at - (item.line - 1)
    : (at - item.start) / Math.max(1, item.end - item.start);
  return heights.yAt(found + fraction);
}

/** 按主动栏中线求跟随栏可达的 scrollTop；两边都折叠时按同一个 fold id 对齐。 */
export function syncTargetTop(
  from: Side,
  sourceTop: number,
  source: SideGeometry,
  follower: SideGeometry,
  segments: SyncSegment[],
): number {
  const centerY = clampScrollTop(sourceTop, source) + source.viewportHeight / 2;
  const position = source.heights.rowAtFraction(centerY);
  const row = Math.floor(position);
  const item = source.rows.rows[row];
  const followerFold = item && "fold" in item ? follower.rows.foldRows.get(item.fold) : undefined;
  const mappedY = followerFold !== undefined
    ? follower.heights.yAt(followerFold + position - row)
    : yAtFilePosition(
      follower.rows,
      follower.heights,
      mapLine(from, filePositionAtY(source.rows, source.heights, centerY), segments),
    );
  return clampScrollTop(mappedY - follower.viewportHeight / 2, follower);
}

/** 同一主动方向下的逻辑位置误差；首尾夹紧不是同步失败，不计不可达的位置。 */
export function syncPositionError(
  from: Side,
  sourceTop: number,
  followerTop: number,
  source: SideGeometry,
  follower: SideGeometry,
  segments: SyncSegment[],
): number {
  const expectedTop = syncTargetTop(from, sourceTop, source, follower, segments);
  const center = follower.viewportHeight / 2;
  const expected = filePositionAtY(follower.rows, follower.heights, expectedTop + center);
  const actual = filePositionAtY(follower.rows, follower.heights, followerTop + center);
  return Math.abs(expected - actual);
}

function clampScrollTop(top: number, geometry: SideGeometry): number {
  return clampLine(top, Math.max(0, geometry.heights.total() - geometry.viewportHeight));
}
