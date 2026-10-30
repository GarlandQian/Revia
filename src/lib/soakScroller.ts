// x18n-ignore-file
import type { DiffViewHandle } from "../features/diff/DiffView";
import type { SideBySideHandle } from "../features/diff/SideBySideView";
import type { ViewMode } from "../types/bindings/ViewMode";

/** 长测只操作当前渲染器，不用统一视图的行缓存代替并排视图的就绪状态。 */
export function soakScroller(view: ViewMode, unified: DiffViewHandle | null, split: SideBySideHandle | null) {
  if (view === "split") {
    if (!split) return null;
    return {
      ready() {
        if (split.getViewportHeight() <= 0 || !split.getFileAnchor()) return false;
        let visibleRows = 0;
        for (const side of ["left", "right"] as const) {
          const range = split.getVisibleRange(side);
          visibleRows += range.to - range.from;
          if (!split.isCovered(range.from, range.to, side)) return false;
        }
        return visibleRows > 0;
      },
      settled: () => split.isSettled(),
      scrollToRow(row: number) {
        const anchor = split.getFileAnchor();
        if (!anchor || !split.restoreFileAnchor({ ...anchor, line: Math.max(0, Math.floor(row)) + 1 })) {
          throw new Error("长测并排视图尚未就绪");
        }
      },
      atTop: () => split.getScrollTop() === 0 && split.getFileAnchor()?.line === 1,
    };
  }
  if (!unified) return null;
  return {
    ready() {
      const range = unified.getVisibleRange();
      return unified.getViewportHeight() > 0 && range.to > range.from
        && unified.isCovered(range.from, range.to);
    },
    settled: () => true,
    scrollToRow: (row: number) => unified.scrollToRow(row),
    atTop: () => unified.getScrollTop() === 0,
  };
}
