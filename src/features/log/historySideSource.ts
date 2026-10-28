import type { ipc } from "@/lib/ipc";
import type { HistoryFileTarget } from "@/types/bindings/HistoryFileTarget";
import type { HistorySideLayout } from "@/types/bindings/HistorySideLayout";
import type { DiffOptions } from "@/features/diff/SideBySideView";
import type { Side } from "@/features/diff/rowIndex";

type HistorySideApi = Pick<typeof ipc, "historyFileSideLayout" | "historyFileSideLines">;

export interface HistorySideSource {
  readonly key: string;
  readonly path: string;
  readonly targetId: string;
  readonly file: HistoryFileTarget;
  layout(): Promise<HistorySideLayout>;
  lines(side: Side, from: number, to: number): ReturnType<HistorySideApi["historyFileSideLines"]>;
  matches(layout: HistorySideLayout): boolean;
}

/** Read-only source: no worktree identity, fingerprint or action credential is synthesized. */
export function createHistorySideSource(
  targetId: string,
  file: HistoryFileTarget,
  options: Pick<DiffOptions, "ignoreWhitespace" | "contextLines" | "highlight">,
  api: HistorySideApi,
): HistorySideSource {
  const target = Object.freeze({ ...file });
  const { ignoreWhitespace, contextLines, highlight } = options;
  const request = { targetId, file: target, ignoreWhitespace, contextLines };
  const matches = (layout: HistorySideLayout) => layout.targetId === targetId
    && layout.diffId === target.diffId && layout.ignoreWhitespace === ignoreWhitespace;
  return {
    key: JSON.stringify([
      targetId, target.diffId, target.commit, target.parent, target.path, target.originalPath,
      target.oldOid, target.newOid, target.status, ignoreWhitespace, contextLines, highlight,
    ]),
    path: target.path,
    targetId,
    file: target,
    matches,
    async layout() {
      const layout = await api.historyFileSideLayout(request);
      if (!matches(layout)) throw new Error("提交 diff 布局与读取目标不一致");
      return layout;
    },
    lines: (side, from, to) => api.historyFileSideLines({
      ...request, highlight, version: side === "left" ? "old" : "new", from, to,
    }),
  };
}
