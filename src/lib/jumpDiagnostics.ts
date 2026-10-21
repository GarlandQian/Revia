import type { DiffViewHandle } from "@/features/diff/DiffView";
import type { HighlightStore } from "@/features/diff/highlightStore";
import type { RowWindowStore } from "@/features/diff/rowWindowStore";
import type { ViewMode } from "@/types/bindings/ViewMode";
import { inspectJumpDom, type JumpDomProbe } from "./jumpDomProbe";
import { nextTask } from "./measure";
import { waitForSoak } from "./soak";

interface DomJumpSample {
  fromRow: number;
  toRow: number;
  targetWasCached: boolean;
  missingRowsBefore: number;
  colorsCachedBefore: boolean;
  scrollCallMs: number;
  /** Elapsed from jump start through the synchronous DOM check, including scrollCallMs. */
  immediateDomMs: number;
  immediate: JumpDomProbe;
  textDomMs: number | null;
  colorDomMs: number | null;
  final: JumpDomProbe;
}

export interface JumpDomDiagnostics {
  sourceReady: boolean;
  sourcePreparation: "explicit-cache-and-normal-scroll";
  preparationError: string | null;
  outward: DomJumpSample;
  return: DomJumpSample;
}

/** Separate probes leave the original M1 timing sequence intact; DOM readiness is not paint time. */
export async function diagnoseJumpDom(options: {
  view: ViewMode;
  rowHeight: number;
  rows: RowWindowStore;
  highlights: HighlightStore;
  getScroller: () => DiffViewHandle | null;
}): Promise<JumpDomDiagnostics> {
  const { view, rowHeight, rows, highlights, getScroller } = options;
  const scroller = () => {
    const current = getScroller();
    if (!current || current.getViewportHeight() <= 0) throw new Error("跳转诊断缺少有效视口");
    return current;
  };
  const inspect = (from: number) => inspectJumpDom({
    root: document.querySelector<HTMLElement>(".diff-scroll"),
    view,
    from,
    to: Math.min(rows.rowCountValue, from + Math.ceil(scroller().getViewportHeight() / rowHeight)),
    rowHeight,
    getRow: (index) => rows.getRow(index),
    highlights,
  });
  const until = async (predicate: () => boolean, timeoutMs: number) => {
    const deadline = performance.now() + timeoutMs;
    while (performance.now() < deadline) {
      if (predicate()) return true;
      await nextTask();
    }
    return predicate();
  };

  scroller().scrollToRow(0);
  const sourceRows = Math.ceil(scroller().getViewportHeight() / rowHeight);
  const maxTarget = Math.max(0, rows.rowCountValue - sourceRows);
  const target = Math.min(maxTarget, Math.floor(rows.rowCountValue * 0.8));
  if (target <= 0) throw new Error("跳转诊断没有可用的远端目标");
  // Explicit preparation is outside all timings; this tests restoration of already-cached colors.
  const sourceCached = await waitForSoak(() => {
    for (let index = 0; index < sourceRows; index += 1) {
      const row = rows.getRow(index);
      if (!row || row.kind === "hunk") continue;
      const oldLine = view === "split" ? row.left?.line : row.lineKind === "addition" ? null : row.oldLine;
      const newLine = view === "split" ? row.right?.line : row.lineKind === "addition" ? row.newLine : null;
      if (oldLine != null) highlights.ensure("old", oldLine, oldLine);
      if (newLine != null) highlights.ensure("new", newLine, newLine);
    }
    return inspect(0).colorsCached;
  }, 15_000);
  if (sourceCached) {
    scroller().scrollToRow(Math.min(maxTarget, sourceRows * 3));
    scroller().scrollToRow(0);
  }
  const sourceReady = sourceCached && await until(() => {
    const sample = inspect(0);
    return sample.textMatched && sample.colorsMatched && !scroller().isCompact();
  }, 2000);

  const jump = async (to: number, checkColors: boolean): Promise<DomJumpSample> => {
    const handle = scroller();
    const fromRow = Math.floor(handle.getScrollTop() / rowHeight);
    const end = Math.min(rows.rowCountValue, to + Math.ceil(handle.getViewportHeight() / rowHeight));
    const missingRowsBefore = rows.uncovered(to, end);
    const colorsCachedBefore = inspect(to).colorsCached;
    const started = performance.now();
    handle.scrollToRow(to);
    const returnedAt = performance.now();
    const immediate = inspect(to);
    const immediateAt = performance.now();
    const textReady = immediate.textMatched || await until(() => inspect(to).textMatched, 4000);
    const textDomMs = textReady ? performance.now() - started : null;
    let colorDomMs: number | null = null;
    if (checkColors && colorsCachedBefore) {
      const restored = await until(() => inspect(to).colorsMatched && !scroller().isCompact(), 2000);
      if (restored) colorDomMs = performance.now() - started;
    }
    return {
      fromRow,
      toRow: to,
      targetWasCached: missingRowsBefore === 0,
      missingRowsBefore,
      colorsCachedBefore,
      scrollCallMs: returnedAt - started,
      immediateDomMs: immediateAt - started,
      immediate,
      textDomMs,
      colorDomMs,
      final: inspect(to),
    };
  };

  const outward = await jump(target, false);
  const back = await jump(0, sourceReady);
  return {
    sourceReady,
    sourcePreparation: "explicit-cache-and-normal-scroll",
    preparationError: sourceReady ? null : "源位置文字与双侧颜色未就绪；颜色恢复样本不适用",
    outward,
    return: back,
  };
}
