/**
 * 测量工具：首屏、跳转、滚动期间的覆盖情况与帧间隔。
 *
 * 注意：窗口不可见时 macOS 会停摆合成器（`requestAnimationFrame` 不再触发），
 * 因此每个等待都有基于 `MessageChannel` 的兜底，并记录窗口可见性与 rAF 频率，
 * 避免把无合成的会话数据当作验收结论。
 */

export interface FirstScreenSample {
  /** 从 `list_changes` 发起算起（与 M0 口径一致）。 */
  fromListChangesMs: number | null;
  /** 从 `open_worktree` 发起算起（含会话与环境解析）。 */
  fromOpenWorktreeMs: number | null;
  /** `list_changes` 往返（含 git diff 与解析）。 */
  listChangesMs: number | null;
  /** 首次取可见行的往返。 */
  firstRowsMs: number | null;
  /** 数据到齐到提交渲染。 */
  renderMs: number | null;
  rowCount: number | null;
  fileCount: number | null;
  /** 是否真的拿到了合成帧。 */
  rafPainted: boolean;
  viewportHeight: number | null;
  devicePixelRatio: number | null;
}

export interface JumpSample {
  fromRow: number;
  toRow: number;
  /** 跳转到目标窗口内容提交的耗时。 */
  appearMs: number;
  /** 其中取数据（Rust + IPC）的耗时。 */
  fetchMs: number;
  /** 目标窗口是否在 1 帧内齐备（16.7ms）。 */
  withinOneFrame: boolean;
  /** 热态跳转：跳回刚看过的位置（块已在缓存里），到内容提交的耗时。 */
  warmAppearMs: number | null;
  /** 热态跳转是否在 1 帧内提交。 */
  warmWithinOneFrame: boolean | null;
  /** 热态跳转后，已缓存的语法颜色恢复完毕的耗时（毫秒）。 */
  colorRestoreMs: number | null;
}

export interface ScrollSample {
  mode: "frame" | "step";
  screens: number;
  /** 采样帧数。 */
  frames: number;
  /** 可见区域里没有数据的行数统计。 */
  uncoveredFrames: number;
  uncoveredMax: number;
  uncoveredP95: number;
  frameP50Ms: number;
  frameP95Ms: number;
  frameMaxMs: number;
  droppedFrames: number;
  scrollDistancePx: number;
  seconds: number;
}

export interface VisibilitySample {
  state: string;
  hidden: boolean;
  viewportWidth: number;
  viewportHeight: number;
  devicePixelRatio: number;
  rafPerSecond: number;
}

export interface Measurement {
  id: number;
  label: string;
  startedAt: string;
  view: "split" | "unified";
  path: string;
  visibility: VisibilitySample;
  firstScreen: FirstScreenSample;
  jump: JumpSample | null;
  scroll: ScrollSample | null;
  highlight: { roundTrips: number; lastRoundTripMs: number | null; loadedLines: number } | null;
}

export function percentile(values: number[], fraction: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.max(0, Math.round((sorted.length - 1) * fraction)));
  return sorted[index];
}

/** 等一次渲染提交：优先合成帧，窗口不可见时用消息任务兜底。 */
export function nextPaint(timeoutMs = 800): Promise<{ at: number; rafPainted: boolean }> {
  return new Promise((resolve) => {
    let settled = false;
    const channel = new MessageChannel();
    const finish = (rafPainted: boolean) => {
      if (settled) return;
      settled = true;
      channel.port1.close();
      resolve({ at: performance.now(), rafPainted });
    };
    requestAnimationFrame(() => requestAnimationFrame(() => finish(true)));
    setTimeout(() => channel.port1.postMessage(null), 30);
    setTimeout(() => finish(false), timeoutMs);
    channel.port2.onmessage = () => finish(false);
  });
}

/**
 * 让出一个任务。用 MessageChannel 而不是 setTimeout：窗口不可见时定时器会被
 * 节流到秒级，消息任务不受影响。
 */
export function nextTask(): Promise<void> {
  return new Promise((resolve) => {
    const channel = new MessageChannel();
    channel.port1.onmessage = () => {
      channel.port1.close();
      resolve();
    };
    channel.port2.postMessage(null);
  });
}

/** 一秒内的 rAF 次数；0 表示窗口不可见或合成器停摆。 */
export function probeRaf(seconds = 1): Promise<number> {
  return new Promise((resolve) => {
    let frames = 0;
    const start = performance.now();
    const tick = () => {
      frames += 1;
      if (performance.now() - start >= seconds * 1000) {
        resolve(frames / seconds);
        return;
      }
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
    setTimeout(() => resolve(frames / seconds), seconds * 1000 + 250);
  });
}

export function visibilitySample(rafPerSecond = 0): VisibilitySample {
  return {
    state: document.visibilityState,
    hidden: document.hidden,
    viewportWidth: window.innerWidth,
    viewportHeight: window.innerHeight,
    devicePixelRatio: window.devicePixelRatio,
    rafPerSecond,
  };
}

/**
 * 滚动测量：每次推进一屏，记录帧间隔与「可见区域里还没有数据的行数」。
 * 有合成帧时按 rAF 推进；没有时用 0ms 定时器推进，保证在有图形会话与无图形会话都能得到数据。
 */
/** 测量用的滚动接口：diff 视图自己接管滚动（ADR-0008），不能直接改元素的 scrollTop。 */
export interface Scroller {
  getScrollTop(): number;
  setScrollTop(top: number): void;
  getViewportHeight(): number;
  getContentHeight(): number;
}

export async function measureScroll(options: {
  scroller: Scroller;
  screens: number;
  uncovered: () => number;
  hasCompositor: boolean;
  onProgress?: (index: number) => void;
}): Promise<ScrollSample> {
  const { scroller, screens, uncovered, hasCompositor, onProgress } = options;
  const intervals: number[] = [];
  const uncoveredCounts: number[] = [];
  const scrollStart = scroller.getScrollTop();
  const viewport = Math.max(scroller.getViewportHeight(), 200);
  const started = performance.now();
  let index = 0;
  let last = started;

  const waitFrame = async (): Promise<void> => {
    if (!hasCompositor) {
      await nextTask();
      return;
    }
    // 有合成帧时等真正的下一帧，帧间隔才是 T4 要的数据；100ms 兜底，防止窗口中途被遮住。
    await new Promise<void>((resolve) => {
      const frame = requestAnimationFrame(() => {
        window.clearTimeout(timer);
        resolve();
      });
      const timer = window.setTimeout(() => {
        cancelAnimationFrame(frame);
        resolve();
      }, 100);
    });
  };

  while (index < screens) {
    if (scroller.getScrollTop() + scroller.getViewportHeight() >= scroller.getContentHeight() - viewport) break;
    // 隐藏窗口下定时器会被节流，这里用短的墙钟预算兜底。
    if (performance.now() - started > 12_000) break;
    const before = performance.now();
    scroller.setScrollTop(scroller.getScrollTop() + viewport);
    intervals.push(before - last);
    last = before;
    uncoveredCounts.push(uncovered());
    onProgress?.(index);
    index += 1;
    await waitFrame();
  }

  const elapsed = (performance.now() - started) / 1000;
  return {
    mode: hasCompositor ? "frame" : "step",
    screens: index,
    frames: index,
    uncoveredFrames: uncoveredCounts.filter((value) => value > 0).length,
    uncoveredMax: uncoveredCounts.length > 0 ? Math.max(...uncoveredCounts) : 0,
    uncoveredP95: percentile(uncoveredCounts, 0.95),
    frameP50Ms: percentile(intervals, 0.5),
    frameP95Ms: percentile(intervals, 0.95),
    frameMaxMs: intervals.length > 0 ? Math.max(...intervals) : 0,
    droppedFrames: intervals.filter((value) => value > 20).length,
    scrollDistancePx: Math.abs(scroller.getScrollTop() - scrollStart),
    seconds: elapsed,
  };
}

export function nowStamp(): string {
  const date = new Date();
  const pad = (value: number, size = 2) => String(value).padStart(size, "0");
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}.${pad(date.getMilliseconds(), 3)}`;
}

export function formatMs(value: number | null | undefined): string {
  if (value == null) return "—";
  return `${value.toFixed(1)} ms`;
}
