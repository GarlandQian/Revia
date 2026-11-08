/**
 * 前端性能采样（M9 任务 01，Q210）。
 *
 * 只记录数值，不读文件内容、diff 文本、路径或终端输出。两条硬约束：
 * - 空闲时不跑 requestAnimationFrame 循环：只在滚轮、拖动、键盘导航、窗口缩放期间采样帧，
 *   最后一次输入后 1 秒停止；
 * - 采样结果按场景聚合成固定字段（count / p50 / p95 / max / 超 16ms / 超 33ms），
 *   内存占用与运行时长无关。
 *
 * 长任务优先用 Chromium 的 PerformanceObserver("longtask")；WebKit 没有该类型时，
 * 用采样期间超过 100ms 的帧间隔代替。点击与按键的响应延迟用 Event Timing（Chromium），
 * 取输入到下一帧的 duration。
 */

import { ipc } from "@/lib/ipc";
import { defaultCssMaterial } from "@/lib/platform";

export type FrameScenario = "diff-scroll" | "log-scroll" | "window-resize" | "other";
export type InteractionCategory = "tab" | "file" | "popover" | "other";

/** 送给 Rust 的固定结构（与 `m7/perf.rs` 的 `ClientReport` 一一对应）。 */
export type ClientReport =
  | {
      kind: "frames";
      scenarios: {
        scenario: string; count: number; p50Ms: number; p95Ms: number; maxMs: number;
        over16Ms: number; over33Ms: number;
      }[];
    }
  | {
      kind: "interactions";
      categories: { category: string; count: number; p95Ms: number; maxMs: number }[];
    }
  | { kind: "longTasks"; count: number; totalMs: number; longestMs: number[]; fromFrameGaps: boolean }
  | {
      kind: "display";
      screenWidth: number;
      screenHeight: number;
      devicePixelRatio: number;
      windowMaterial: string;
      webviewPlatform: string;
    }
  | {
      kind: "repoPaints";
      worktreeId: string;
      listDataMs: number;
      listPaintMs: number;
      logPaintMs: number | null;
      changedFiles: number;
      triggerMs?: number;
    };

/** 测量版状态（`m9_measure_state` 的返回）。 */
export type MeasureState = {
  measure: boolean;
  build: string;
  commit: string;
  plainAppearance: boolean;
};

/** 环境进度（`environment_status` 的返回）。 */
export type EnvStatus = {
  ready: boolean;
  provisional: boolean;
  readyAtMs: number | null;
  error: { message: string; detail?: string | null } | null;
};

const FRAME_SCENARIOS: FrameScenario[] = ["diff-scroll", "log-scroll", "window-resize", "other"];
const INTERACTION_CATEGORIES: InteractionCategory[] = ["tab", "file", "popover", "other"];
/** 最后一次输入之后继续采样的时间。 */
const IDLE_STOP_MS = 1000;
/** 采样直方图上限（毫秒）；更大的值计入溢出桶。 */
const HISTOGRAM_MAX_MS = 200;
/** WebKit 没有 longtask 时，超过这个帧间隔算一次长阻塞。 */
const FRAME_GAP_LONG_MS = 100;
const LONG_TASK_KEEP = 10;
const REPORT_INTERVAL_MS = 15_000;

/** 定宽直方图：p50 / p95 是 1ms 粒度的近似值，内存固定（重命名消除与 distribution 混淆）。 */
class Histogram {
  count = 0;
  max = 0;
  over16 = 0;
  over33 = 0;
  private buckets = new Uint32Array(HISTOGRAM_MAX_MS + 1);
  private overflow = 0;

  add(value: number): void {
    if (!Number.isFinite(value) || value < 0) return;
    this.count += 1;
    if (value > this.max) this.max = value;
    if (value > 16.7) this.over16 += 1;
    if (value > 33.4) this.over33 += 1;
    const bucket = Math.round(value);
    if (bucket > HISTOGRAM_MAX_MS) this.overflow += 1;
    else this.buckets[bucket] += 1;
  }

  percentile(fraction: number): number {
    if (this.count === 0) return 0;
    const target = Math.max(1, Math.ceil(this.count * fraction));
    let seen = 0;
    for (let index = 0; index <= HISTOGRAM_MAX_MS; index += 1) {
      seen += this.buckets[index];
      if (seen >= target) return index;
    }
    return this.max;
  }
}

export type RepoOpenTrace = {
  worktreeId: string;
  /** 文件列表已画出（快照已应用）。triggerMs 是点击到发出命令的排队时间。 */
  listReady: (listDataMs: number, changedFiles: number, triggerMs?: number) => void;
  /** 日志首屏已画出；没有日志时由超时兜底。 */
  logReady: () => void;
};

class PerfRecorder {
  private frames = new Map<FrameScenario, Histogram>();
  private interactions = new Map<InteractionCategory, Histogram>();
  private longTasks = { count: 0, totalMs: 0, longest: [] as number[] };
  private frameGaps = { count: 0, totalMs: 0, longest: [] as number[] };
  private sampling = false;
  private lastInput = 0;
  private lastFrame: number | null = null;
  private scenario: FrameScenario = "other";
  private installed = false;
  private reportedAt = 0;
  private logTaskObserver: PerformanceObserver | null = null;
  private interactionObserver: PerformanceObserver | null = null;
  private usingFrameGaps = false;
  /** 最近一次点击或按键的事件时刻（与 performance.now() 同一时间基准）。 */
  private lastTriggerAt = 0;

  /** 在应用启动时调用一次；重复调用无副作用。 */
  start(): void {
    if (this.installed || typeof window === "undefined") return;
    this.installed = true;
    for (const scenario of FRAME_SCENARIOS) this.frames.set(scenario, new Histogram());
    for (const category of INTERACTION_CATEGORIES) this.interactions.set(category, new Histogram());

    // 记下用户触发的时刻：打开仓库的计时从点击或按键那一刻开始（S11）。
    const noteTrigger = (event: Event) => {
      this.lastTriggerAt = event.timeStamp;
    };
    window.addEventListener("pointerdown", noteTrigger, { capture: true, passive: true });
    window.addEventListener("keydown", noteTrigger, { capture: true, passive: true });
    window.addEventListener("wheel", (event) => this.noteActivity(this.scenarioOf(event.target)), { capture: true, passive: true });
    window.addEventListener("touchmove", (event) => this.noteActivity(this.scenarioOf(event.target)), { capture: true, passive: true });
    window.addEventListener("scroll", (event) => this.noteActivity(this.scenarioOf(event.target)), { capture: true, passive: true });
    window.addEventListener("pointermove", (event) => {
      // 只有按住拖动才算交互，悬停不采样。
      if (event.buttons !== 0) this.noteActivity(this.scenarioOf(event.target));
    }, { capture: true, passive: true });
    window.addEventListener("keydown", (event) => {
      if (event.key.startsWith("Arrow") || event.key === "PageDown" || event.key === "PageUp"
        || event.key === "Home" || event.key === "End" || event.key === " ") {
        this.noteActivity(this.scenarioOf(event.target));
      }
    }, { capture: true });
    window.addEventListener("resize", () => this.noteActivity("window-resize"));

    this.observeLongTasks();
    this.observeInteractions();
    this.reportDisplay();
  }

  /**
   * 本次操作的触发时刻：1 秒内有点击或按键就取它（含主线程排队的时间），
   * 否则（程序发起的打开）取 `fallback`。
   */
  triggerTimestamp(fallback: number): number {
    const at = this.lastTriggerAt;
    return at > 0 && at <= fallback && fallback - at < 1000 ? at : fallback;
  }

  /** 打开 / 切换 worktree 的分段耗时；一次打开只上报一条（S11：从用户触发瞬间开始计时）。 */
  beginRepoOpen(worktreeId: string, triggerTimestamp?: number): RepoOpenTrace {
    const startedAt = triggerTimestamp ?? performance.now();
    let listDataMs = 0;
    let listPaintMs: number | null = null;
    let logPaintMs: number | null = null;
    let changedFiles = 0;
    let triggerMs: number | undefined;
    let sent = false;
    let timer = 0;

    const send = () => {
      if (sent || listPaintMs === null) return;
      sent = true;
      window.clearTimeout(timer);
      void ipc.reportPerformance({
        kind: "repoPaints",
        worktreeId,
        listDataMs,
        listPaintMs,
        logPaintMs,
        changedFiles,
        triggerMs,
      }).catch(() => undefined);
    };

    return {
      worktreeId,
      listReady: (dataMs, files, trigger) => {
        listDataMs = dataMs;
        if (trigger !== undefined) triggerMs = trigger;
        changedFiles = files;
        // React 提交在下一帧之前；取一帧之后的时刻，避免量到提交之前的绘制。
        requestAnimationFrame(() => {
          listPaintMs = performance.now() - startedAt;
          // 日志迟迟不来时也要上报，logPaintMs 记为 null。
          timer = window.setTimeout(send, 2000);
          if (logPaintMs !== null) send();
        });
      },
      logReady: () => {
        if (logPaintMs !== null) return;
        requestAnimationFrame(() => {
          logPaintMs = performance.now() - startedAt;
          send();
        });
      },
    };
  }

  /** 把累计值送给 Rust；诊断预览与退出前各调用一次。 */
  async flush(): Promise<void> {
    const frames = [...this.frames.entries()].map(([scenario, stats]) => ({
      scenario,
      count: stats.count,
      p50Ms: stats.percentile(0.5),
      p95Ms: stats.percentile(0.95),
      maxMs: stats.max,
      over16Ms: stats.over16,
      over33Ms: stats.over33,
    }));
    const interactions = [...this.interactions.entries()].map(([category, stats]) => ({
      category,
      count: stats.count,
      p95Ms: stats.percentile(0.95),
      maxMs: stats.max,
    }));
    await Promise.allSettled([
      ipc.reportPerformance({ kind: "frames", scenarios: frames }),
      ipc.reportPerformance({ kind: "interactions", categories: interactions }),
      ipc.reportPerformance({
        kind: "longTasks",
        count: this.longTasks.count,
        totalMs: this.longTasks.totalMs,
        longestMs: this.longTasks.longest,
        fromFrameGaps: this.usingFrameGaps,
      }),
      ipc.reportPerformance({ kind: "display", ...this.displayInfo() }),
    ]);
    this.reportedAt = performance.now();
  }

  private displayInfo() {
    const root = document.documentElement;
    const material = !root.classList.contains("window-material")
      ? "solid"
      : root.classList.contains("native-glass")
        ? "native"
        : defaultCssMaterial();
    return {
      screenWidth: Math.round(window.screen?.width ?? 0),
      screenHeight: Math.round(window.screen?.height ?? 0),
      devicePixelRatio: Math.round(window.devicePixelRatio * 100) / 100,
      windowMaterial: material,
      webviewPlatform: (navigator.platform || navigator.userAgent).slice(0, 32),
    };
  }

  private reportDisplay(): void {
    void ipc.reportPerformance({ kind: "display", ...this.displayInfo() }).catch(() => undefined);
  }

  private noteActivity(scenario: FrameScenario): void {
    this.scenario = scenario;
    this.lastInput = performance.now();
    if (this.sampling) return;
    this.sampling = true;
    this.lastFrame = null;
    requestAnimationFrame(this.tick);
  }

  private tick = (timestamp: number) => {
    if (this.lastFrame !== null) {
      const delta = timestamp - this.lastFrame;
      this.frames.get(this.scenario)?.add(delta);
      // S10：仅在不支持原生 longtask（WebKit）时才用帧间隔推算，不覆盖 Chromium 的原生 longtask。
      if (delta > FRAME_GAP_LONG_MS && !this.logTaskObserver) this.noteFrameGap(delta);
    }
    this.lastFrame = timestamp;
    if (performance.now() - this.lastInput > IDLE_STOP_MS) {
      this.sampling = false;
      void this.flushIfDue();
      return;
    }
    requestAnimationFrame(this.tick);
  };

  private async flushIfDue(): Promise<void> {
    if (performance.now() - this.reportedAt < REPORT_INTERVAL_MS) return;
    await this.flush();
  }

  private noteFrameGap(delta: number): void {
    // WebKit 没有 longtask：用帧间隔代替，标记来源，避免和 Chromium 的统计混在一起。
    this.usingFrameGaps = true;
    this.frameGaps.count += 1;
    this.frameGaps.totalMs += delta;
    this.frameGaps.longest = [...this.frameGaps.longest, delta].sort((a, b) => b - a).slice(0, LONG_TASK_KEEP);
    this.longTasks.count = this.frameGaps.count;
    this.longTasks.totalMs = this.frameGaps.totalMs;
    this.longTasks.longest = this.frameGaps.longest;
  }

  private observeLongTasks(): void {
    if (typeof PerformanceObserver === "undefined") return;
    if (!PerformanceObserver.supportedEntryTypes?.includes("longtask")) return;
    this.logTaskObserver = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        this.longTasks.count += 1;
        this.longTasks.totalMs += entry.duration;
        this.longTasks.longest = [...this.longTasks.longest, entry.duration].sort((a, b) => b - a).slice(0, LONG_TASK_KEEP);
      }
    });
    try {
      this.logTaskObserver.observe({ entryTypes: ["longtask"] });
    } catch {
      this.logTaskObserver = null;
    }
  }

  private observeInteractions(): void {
    if (typeof PerformanceObserver === "undefined") return;
    if (!PerformanceObserver.supportedEntryTypes?.includes("event")) return;
    this.interactionObserver = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        const event = entry as PerformanceEntry & { name: string; target?: EventTarget | null };
        if (!["click", "keydown", "pointerdown", "pointerup"].includes(event.name)) continue;
        if (event.duration <= 0) continue;
        this.interactions.get(this.categoryOf(event.target ?? null))?.add(event.duration);
      }
    });
    try {
      this.interactionObserver.observe({ type: "event", durationThreshold: 16, buffered: true } as PerformanceObserverInit);
    } catch {
      this.interactionObserver = null;
    }
  }

  private scenarioOf(target: EventTarget | null): FrameScenario {
    const element = target instanceof Element ? target : null;
    if (element?.closest(".diff-scroll, .diff-canvas, .sbs-column")) return "diff-scroll";
    if (element?.closest(".log-view, .log-table, .log-scroll")) return "log-scroll";
    return "other";
  }

  private categoryOf(target: EventTarget | null): InteractionCategory {
    const element = target instanceof Element ? target : null;
    if (!element) return "other";
    if (element.closest(".editor-tab, .settings-nav, .log-view, .main-view-tabs, [role='tab']")) return "tab";
    if (element.closest(".changes-sidebar, .tree-row, .file-row, .changes-tree")) return "file";
    if (element.closest("[role='menu'], .revia-popper, .glass-popover, [role='dialog'], .app-dialog")) return "popover";
    return "other";
  }
}

export const perfRecorder = new PerfRecorder();
