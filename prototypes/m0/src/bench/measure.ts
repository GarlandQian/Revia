/**
 * 测量：把首屏拆成 Rust 解析、IPC 传输、前端渲染三段，并记录滚动与视图切换表现。
 *
 * 所有时间都来自 `performance.now()`（WebView 里的单调时钟）；Rust 解析耗时由 Rust 自己报告。
 *
 * 注意：窗口不可见时 macOS 会让 `requestAnimationFrame` 停止触发，因此：
 * - 首屏耗时以“数据到达 + 一帧渲染提交完成”为准，不依赖合成器；
 * - 滚动测量优先测“每滚动一屏的重排与渲染耗时”，同时记录真实帧间隔（若 rAF 可用）。
 */

export interface FirstScreenSample {
  /** Rust 侧解析耗时。 */
  parseMs: number | null;
  /** 前端发起 invoke 到拿到结果（含 IPC 传输与反序列化）。 */
  ipcMs: number | null;
  /** 拿到数据到一帧渲染提交完成。 */
  renderMs: number | null;
  /** 从发起请求到渲染提交。 */
  firstScreenMs: number | null;
  /** 渲染提交后是否拿到合成帧（rAF）；窗口不可见时可能为 false。 */
  rafPainted: boolean;
  /** diff 数据规模。 */
  rowCount: number | null;
  fileCount: number | null;
  hunkCount: number | null;
  /** IPC 传输的 JSON 字符数（UTF-16 码元）。 */
  payloadChars: number | null;
  /** 视口高度与窗口缩放。 */
  viewportHeight: number | null;
  devicePixelRatio: number | null;
}

export interface ScrollSample {
  mode: "frame" | "step";
  seconds: number;
  /** frame 模式：帧数；step 模式：推进的屏数。 */
  frames: number;
  frameP50: number;
  frameP95: number;
  frameMax: number;
  droppedFrames: number;
  scrollTopFrom: number;
  scrollTopTo: number;
  /** step 模式：每次推进一屏的渲染耗时统计（毫秒）。 */
  stepRenderP50: number | null;
  stepRenderP95: number | null;
  stepRenderMax: number | null;
  /** 每次推进一屏的实测耗时（含等待），用于判断滚动是否跟得上。 */
  stepWallP50: number | null;
  stepRows: number | null;
}

export interface SwitchSample {
  /** 切换前后的滚动位置是否指向同一批行。 */
  anchorLineBefore: number | null;
  anchorLineAfter: number | null;
  scrollTopBefore: number;
  scrollTopAfter: number;
  layoutMs: number;
}

export interface HighlightSample {
  engine: "shiki" | "syntect";
  totalMs: number;
  oldMs: number;
  newMs: number;
  engineMs: number | null;
  syntax: string | null;
  theme: string | null;
  tokenLines: number;
  tokenCount: number;
  /** 高亮后重新渲染可见区域的一帧耗时。 */
  rerenderMs: number | null;
  /** syntect 分块：是否已经把全部行补齐。 */
  complete: boolean;
  highlightedLines: number;
  totalLines: number;
  /** 分块请求次数。 */
  chunks: number;
}

export interface VisibilitySample {
  state: string;
  hidden: boolean;
  viewportWidth: number | null;
  viewportHeight: number | null;
  devicePixelRatio: number | null;
  /** 一秒内 rAF 回调次数；0 表示窗口不可见或合成器被暂停。 */
  rafPerSecond: number;
}

export interface Measurement {
  id: number;
  label: string;
  startedAt: string;
  view: "split" | "unified";
  highlight: "none" | "shiki" | "syntect";
  visibility: VisibilitySample;
  firstScreen: FirstScreenSample;
  scroll: ScrollSample | null;
  switchView: SwitchSample | null;
  highlightSample: HighlightSample | null;
}

export function percentile(values: number[], fraction: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.max(0, Math.round((sorted.length - 1) * fraction)));
  return sorted[index];
}

/**
 * 等一帧渲染提交完成。优先用合成帧（rAF）；窗口不可见时合成器停摆、定时器也会被节流，
 * 因此再用 `MessageChannel` 兜底（消息任务不受隐藏窗口的定时器节流影响）。
 */
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
    requestAnimationFrame(() => {
      requestAnimationFrame(() => finish(true));
    });
    setTimeout(() => {
      channel.port1.postMessage(null);
    }, 40);
    setTimeout(() => finish(false), timeoutMs);
    channel.port2.onmessage = () => finish(false);
  });
}

/** 一秒内 rAF 触发次数，用来判断窗口是否真的在合成。 */
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

export function visibilitySample(): VisibilitySample {
  return {
    state: document.visibilityState,
    hidden: document.hidden,
    viewportWidth: window.innerWidth,
    viewportHeight: window.innerHeight,
    devicePixelRatio: window.devicePixelRatio,
    rafPerSecond: 0,
  };
}

/** 逐帧滚动（真实合成）：窗口可见时使用。 */
export function measureScrollFrames(element: HTMLElement, durationMs: number, step: number): Promise<ScrollSample> {
  return new Promise((resolve) => {
    const intervals: number[] = [];
    const scrollTopFrom = element.scrollTop;
    let last = performance.now();
    const start = last;

    const tick = () => {
      const now = performance.now();
      intervals.push(now - last);
      last = now;
      element.scrollTop = element.scrollTop + step;
      const finished =
        now - start >= durationMs || element.scrollTop + element.clientHeight >= element.scrollHeight - step;
      if (finished) {
        const dropped = intervals.filter((value) => value > 20).length;
        resolve({
          mode: "frame",
          seconds: (now - start) / 1000,
          frames: intervals.length,
          frameP50: percentile(intervals, 0.5),
          frameP95: percentile(intervals, 0.95),
          frameMax: Math.max(...intervals),
          droppedFrames: dropped,
          scrollTopFrom,
          scrollTopTo: element.scrollTop,
          stepRenderP50: null,
          stepRenderP95: null,
          stepRenderMax: null,
          stepWallP50: null,
          stepRows: null,
        });
        return;
      }
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });
}

/**
 * 按“一屏”推进并逐次计时：每次强制读取 scrollHeight 触发同步布局，
 * 再记录到下一帧（若 rAF 不可用则用 `setTimeout(0)`）为止的耗时。
 * 这样在窗口不可见、rAF 停摆时也能测到渲染成本。
 */
export function measureScrollSteps(element: HTMLElement, screens: number): ScrollSample {
  // 完全同步地推进并测时：不依赖 rAF、timer 或帧调度，
  // 窗口不可见时也能得到“可见区域重建 + 布局”的真实成本。
  const renders: number[] = [];
  const scrollTopFrom = element.scrollTop;
  const viewport = Math.max(element.clientHeight, 200);
  const total = element.scrollHeight;
  const started = performance.now();
  let steps = 0;

  for (let step = 0; step < screens; step += 1) {
    if (element.scrollTop + element.clientHeight >= total - viewport) break;
    if (performance.now() - started > 60_000) break;
    const wallStart = performance.now();
    element.scrollTop += viewport;
    // 读取 scrollHeight 触发同步布局，确保这一屏的行已经重建并排版。
    void element.scrollHeight;
    renders.push(performance.now() - wallStart);
    steps += 1;
  }

  const elapsed = performance.now() - started;
  return {
    mode: "step",
    seconds: elapsed / 1000,
    frames: steps,
    frameP50: percentile(renders, 0.5),
    frameP95: percentile(renders, 0.95),
    frameMax: renders.length > 0 ? Math.max(...renders) : 0,
    droppedFrames: renders.filter((value) => value > 16.7).length,
    scrollTopFrom,
    scrollTopTo: element.scrollTop,
    stepRenderP50: percentile(renders, 0.5),
    stepRenderP95: percentile(renders, 0.95),
    stepRenderMax: renders.length > 0 ? Math.max(...renders) : 0,
    stepWallP50: percentile(renders, 0.5),
    stepRows: steps,
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
