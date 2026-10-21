import { visibilitySample, type VisibilitySample } from "./measure";

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export async function waitForSoak(predicate: () => boolean, timeoutMs: number): Promise<boolean> {
  const deadline = performance.now() + timeoutMs;
  while (performance.now() < deadline) {
    if (predicate()) return true;
    await sleep(20);
  }
  return predicate();
}

export interface SoakVisibility extends VisibilitySample {
  focused: boolean;
  maxFrameGapMs: number;
  hiddenTransitions: number;
}

/** Measures real compositor callbacks; cancelled callbacks cannot accumulate between probes. */
export function monitorSoakVisibility() {
  let frames = 0;
  let lastFrame = performance.now();
  let maxFrameGapMs = 0;
  let hiddenTransitions = 0;
  let raf = 0;
  const onVisibility = () => {
    if (document.hidden) hiddenTransitions += 1;
  };
  const tick = (now: number) => {
    maxFrameGapMs = Math.max(maxFrameGapMs, now - lastFrame);
    lastFrame = now;
    frames += 1;
    raf = requestAnimationFrame(tick);
  };
  document.addEventListener("visibilitychange", onVisibility);
  raf = requestAnimationFrame(tick);
  return {
    reset() {
      lastFrame = performance.now();
      maxFrameGapMs = 0;
      hiddenTransitions = 0;
    },
    async sample(): Promise<SoakVisibility> {
      const before = frames;
      const started = performance.now();
      await sleep(1000);
      const elapsed = performance.now() - started;
      return {
        ...visibilitySample((frames - before) * 1000 / elapsed),
        focused: document.hasFocus(),
        maxFrameGapMs: Math.round(Math.max(maxFrameGapMs, performance.now() - lastFrame)),
        hiddenTransitions,
      };
    },
    stop() {
      cancelAnimationFrame(raf);
      document.removeEventListener("visibilitychange", onVisibility);
    },
  };
}
