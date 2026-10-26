import type { JumpDomProbe } from "./jumpDomProbe";

const FRAME_BUDGET_MS = 16.7;
const COLD_TEXT_BUDGET_MS = 100;
const TIMEOUT_MS = 4000;

type ProbeSource = "synchronous" | "mutation" | "animation-frame" | "timeout";

interface FirstJumpFrame {
  callbackMs: number;
  checkedMs: number;
  /** rAF 的帧时间可能早于跳转起点；门槛只使用实际核验完成时间。 */
  rafTimestampMs: number;
  hidden: boolean;
  dom: JumpDomProbe;
}

export interface JumpFrameResult {
  scrollCallMs: number;
  /** 从跳转开始到同步 DOM 核验结束，包含 scrollCallMs。 */
  immediateDomMs: number;
  immediate: JumpDomProbe;
  textDomMs: number | null;
  textDomSource: ProbeSource | null;
  colorDomMs: number | null;
  final: JumpDomProbe;
  firstFrame: FirstJumpFrame | null;
  /** 正常渲染机会前的 DOM 就绪证据，不是 GPU 扫描或硬件上屏时间。 */
  frameReady: boolean;
  textWithinBudget: boolean;
  textBudgetMs: number;
  frameBudgetMs: number;
  completed: boolean;
  timedOut: boolean;
  timeoutMs: number;
  hidden: boolean;
  hiddenTransitions: number;
  observer: { callbacks: number; records: number; firstCallbackMs: number | null };
}

/** 先登记观察器与首个 rAF，再计时并执行真实跳转；不插手取数或渲染调度。 */
export function probeJumpFrame(options: {
  root: HTMLElement;
  jump: () => void;
  inspect: () => JumpDomProbe;
  colorsRestored: () => boolean;
  targetWasCached: boolean;
  checkColors: boolean;
}): Promise<JumpFrameResult> {
  return new Promise((resolve, reject) => {
    let started = 0;
    let scrollCallMs = 0;
    let immediateDomMs = 0;
    let immediate: JumpDomProbe;
    let final!: JumpDomProbe; // capture assigns this before any finish/read path.
    let textDomMs: number | null = null;
    let textDomSource: ProbeSource | null = null;
    let colorDomMs: number | null = null;
    let firstFrame: FirstJumpFrame | null = null;
    let hidden = document.hidden || document.visibilityState !== "visible";
    let hiddenTransitions = 0;
    let finished = false;
    let raf: number | null = null;
    let timer: number | null = null;
    const observerStats = { callbacks: 0, records: 0, firstCallbackMs: null as number | null };
    const textBudgetMs = options.targetWasCached ? FRAME_BUDGET_MS : COLD_TEXT_BUDGET_MS;

    const cleanup = (removeVisibility = true) => {
      observer.disconnect();
      if (raf != null) cancelAnimationFrame(raf);
      if (timer != null) window.clearTimeout(timer);
      if (removeVisibility) document.removeEventListener("visibilitychange", onVisibility);
      raf = null;
      timer = null;
    };
    const fail = (error: unknown) => {
      if (finished) return;
      finished = true;
      cleanup();
      reject(error);
    };
    const finish = (timedOut: boolean) => {
      if (finished) return;
      finished = true;
      cleanup(false);
      // 不在 rAF 的 Promise 微任务中启动下一次跳转，避免同一渲染机会被回跳覆盖。
      timer = window.setTimeout(() => {
        hidden ||= document.hidden || document.visibilityState !== "visible";
        cleanup();
        const completed = !timedOut && !hidden && firstFrame != null && final.canvasReady && final.textMatched
          && (!options.checkColors || colorDomMs != null);
        const frameReady = completed && firstFrame != null && !firstFrame.hidden
          && firstFrame.checkedMs <= FRAME_BUDGET_MS && firstFrame.dom.canvasReady
          && (firstFrame.dom.textMatched || (!options.targetWasCached && firstFrame.dom.placeholderExposed));
        resolve({
          scrollCallMs, immediateDomMs, immediate, textDomMs, textDomSource, colorDomMs, final,
          firstFrame, frameReady, textWithinBudget: completed && textDomMs != null && textDomMs <= textBudgetMs,
          textBudgetMs, frameBudgetMs: FRAME_BUDGET_MS, completed, timedOut, timeoutMs: TIMEOUT_MS, hidden, hiddenTransitions,
          observer: { ...observerStats },
        });
      }, 0);
    };
    const capture = (source: ProbeSource) => {
      final = options.inspect();
      const checkedMs = performance.now() - started;
      hidden ||= document.hidden || document.visibilityState !== "visible";
      if (textDomMs == null && final.textMatched) {
        textDomMs = checkedMs;
        textDomSource = source;
      }
      if (options.checkColors && colorDomMs == null && final.colorsMatched && options.colorsRestored()) {
        colorDomMs = performance.now() - started;
      }
      return checkedMs;
    };
    const maybeFinish = () => {
      if (performance.now() - started >= TIMEOUT_MS) {
        finish(true);
      } else if (hidden || (firstFrame != null && final.textMatched && (!options.checkColors || colorDomMs != null))) {
        finish(false);
      }
    };
    const onVisibility = () => {
      if (document.hidden || document.visibilityState !== "visible") {
        hidden = true;
        hiddenTransitions += 1;
        finish(false);
      }
    };
    const observer = new MutationObserver((records) => {
      if (finished) return;
      try {
        observerStats.callbacks += 1;
        observerStats.records += records.length;
        observerStats.firstCallbackMs ??= performance.now() - started;
        capture("mutation");
        maybeFinish();
      } catch (error) { fail(error); }
    });
    const onFrame = (timestamp: number) => {
      raf = null;
      if (finished) return;
      try {
        const callbackMs = performance.now() - started;
        const checkedMs = capture("animation-frame");
        firstFrame ??= { callbackMs, checkedMs, rafTimestampMs: timestamp - started, hidden, dom: final };
        maybeFinish();
        if (!finished) raf = requestAnimationFrame(onFrame);
      } catch (error) { fail(error); }
    };

    try {
      observer.observe(options.root, { childList: true, subtree: true, characterData: true, attributes: true });
      document.addEventListener("visibilitychange", onVisibility);
      raf = requestAnimationFrame(onFrame);
      timer = window.setTimeout(() => {
        if (finished) return;
        try {
          capture("timeout");
          finish(true);
        } catch (error) { fail(error); }
      }, TIMEOUT_MS);
      started = performance.now();
      options.jump();
      scrollCallMs = performance.now() - started;
      immediateDomMs = capture("synchronous");
      immediate = final;
      maybeFinish();
    } catch (error) { fail(error); }
  });
}
