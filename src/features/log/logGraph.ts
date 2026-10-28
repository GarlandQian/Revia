/**
 * 分支图（分支线 / 合并与分叉，M5 L03）。
 *
 * 按页增量计算：只向前消费已加载的提交，保留“每条分支线接下来等待哪个提交”的
 * 活跃轨道状态，因此翻页不需要重算整段历史。图在**前端**计算（Rust 只给拓扑顺序的
 * 提交与真实父提交列表），原因见报告：绘图是纯展示，分页数据已经足够。
 *
 * 纯模块：`tools/test-log-model.mjs` 用合成历史检查轨道分配与长边折叠。
 */

export interface GraphCommitInput {
  oid: string;
  parents: string[];
}

/** 一行里的图形：提交点所在轨道、上方/下方轨道、横向连接与占用宽度。 */
export interface GraphRow {
  /** 提交点所在轨道（0 基）。 */
  lane: number;
  /** 这一行上方仍在延续的轨道。 */
  railsAbove: number[];
  /** 这一行下方仍在延续的轨道。 */
  railsBelow: number[];
  /** 从提交点连到某个父提交轨道的横线。 */
  arms: Array<{ from: number; to: number }>;
  /** 这一行最多用到几条轨道（渲染宽度用）。 */
  width: number;
}

export interface GraphState {
  /** 每条轨道接下来等待的提交 OID；null 表示空轨道。 */
  active: Array<string | null>;
}

export function emptyGraphState(): GraphState {
  return { active: [] };
}

function firstFree(active: Array<string | null>): number {
  const index = active.indexOf(null);
  return index >= 0 ? index : active.length;
}

function trim(active: Array<string | null>): void {
  while (active.length > 0 && active[active.length - 1] === null) active.pop();
}

/**
 * 追加一页提交，返回每行的图形与新的轨道状态。
 *
 * `firstParent` 开启时只画第一父边（“只看第一父提交”），父提交列表本身仍由后端
 * 返回真实全部父提交。
 */
export function appendGraphRows(
  state: GraphState,
  commits: GraphCommitInput[],
  options: { firstParent?: boolean } = {},
): { rows: GraphRow[]; state: GraphState } {
  const active = [...state.active];
  const rows: GraphRow[] = [];
  for (const commit of commits) {
    let lane = active.indexOf(commit.oid);
    if (lane < 0) {
      lane = firstFree(active);
      active[lane] = commit.oid;
    }
    const railsAbove: number[] = [];
    for (let index = 0; index < active.length; index += 1) {
      if (index !== lane && active[index] !== null) railsAbove.push(index);
    }
    const parents = options.firstParent ? commit.parents.slice(0, 1) : commit.parents;
    active[lane] = null;
    const arms: Array<{ from: number; to: number }> = [];
    let laneTaken = false;
    for (const parent of parents) {
      const existing = active.indexOf(parent);
      if (existing >= 0) {
        arms.push({ from: lane, to: existing });
        continue;
      }
      let target: number;
      if (!laneTaken && lane < active.length && active[lane] === null) {
        target = lane;
        laneTaken = true;
      } else {
        target = firstFree(active);
      }
      active[target] = parent;
      if (target !== lane) arms.push({ from: lane, to: target });
    }
    trim(active);
    const railsBelow: number[] = [];
    for (let index = 0; index < active.length; index += 1) {
      if (active[index] !== null) railsBelow.push(index);
    }
    const width = Math.max(
      1,
      lane + 1,
      ...railsAbove.map((value) => value + 1),
      ...railsBelow.map((value) => value + 1),
      ...arms.map((arm) => Math.max(arm.from, arm.to) + 1),
    );
    rows.push({ lane, railsAbove, railsBelow, arms, width });
  }
  return { rows, state: { active } };
}

/** 分支线调色板（深浅主题都够对比；与 IntelliJ 的彩色分支线同一思路）。 */
export const LANE_COLORS = [
  "#5b9bd5",
  "#e08a5d",
  "#5fb07a",
  "#c4a03f",
  "#9b7fd4",
  "#4fb3b3",
  "#d4739f",
  "#8fa64a",
];

export function laneColor(lane: number): string {
  return LANE_COLORS[((lane % LANE_COLORS.length) + LANE_COLORS.length) % LANE_COLORS.length];
}

/** 长边折叠的箭头：点击后跳到同一条线的另一端。 */
export interface LongEdgeMarker {
  row: number;
  lane: number;
  /** 箭头方向：`down` 表示另一端在更下面，`up` 表示在更上面。 */
  direction: "down" | "up";
  /** 跳转目标行（已加载行内的下标）。 */
  targetRow: number;
}

export interface LongEdgeLayout {
  /** 需要折叠成短箭头的行与轨道（`row:lane`），这些位置不画连续长线。 */
  collapsed: Set<string>;
  markers: LongEdgeMarker[];
}

/**
 * 计算长边折叠（Rebased 的 Long Edges，默认关）。
 *
 * 一条轨道如果在两次“事件”（提交点或横向连接）之间跨过超过 `threshold` 行，
 * 中间部分折叠，两端各画一个可点击的箭头。开关关闭时返回空结果（画完整长线）。
 */
export function collapseLongEdges(rows: GraphRow[], enabled: boolean, threshold = 24): LongEdgeLayout {
  const collapsed = new Set<string>();
  const markers: LongEdgeMarker[] = [];
  if (!enabled) return { collapsed, markers };
  // 每条轨道的事件行（提交点 / 横向连接端点）。
  const events = new Map<number, number[]>();
  const push = (lane: number, row: number) => {
    const list = events.get(lane) ?? [];
    if (list[list.length - 1] !== row) list.push(row);
    events.set(lane, list);
  };
  rows.forEach((row, index) => {
    push(row.lane, index);
    for (const arm of row.arms) {
      push(arm.from, index);
      push(arm.to, index);
    }
  });
  for (const [lane, list] of events) {
    for (let index = 1; index < list.length; index += 1) {
      const from = list[index - 1];
      const to = list[index];
      if (to - from <= threshold) continue;
      for (let row = from + 1; row < to; row += 1) {
        collapsed.add(`${row}:${lane}`);
      }
      markers.push({ row: from, lane, direction: "down", targetRow: to });
      markers.push({ row: to, lane, direction: "up", targetRow: from });
    }
  }
  return { collapsed, markers };
}
