/**
 * 前端诊断日志：只有在设置了 `REVIA_MEASURE_LOG`（测量会话）时才写后端日志，
 * 正常使用时是空操作，不产生额外 IPC。
 */

let enabled = false;

export function setLoggingEnabled(value: boolean): void {
  enabled = value;
  if (!value) {
    synchronousTimings.length = 0;
    droppedSynchronousTimings = 0;
  }
}

export function isLoggingEnabled(): boolean {
  return enabled;
}

import { invoke } from "@tauri-apps/api/core";

export function logToBackend(message: string): void {
  if (!enabled) return;
  send(message);
}

/** 只在已有测量日志开启时观察真实异步调用；不读取参数或响应正文。 */
export function traceMeasurement<T>(name: string, read: () => Promise<T>, range?: { side: "left" | "right"; from: number; to: number }): Promise<T> {
  if (!enabled) return read();
  const startedAtMs = Date.now();
  const started = performance.now();
  return read().finally(() => {
    logToBackend(`[TIMING_JS] ${JSON.stringify({ name, startedAtMs, elapsedMs: performance.now() - started, ...range })}`);
  });
}

/**
 * 关键信息（启动、渲染错误）：不管开关状态都上报。
 * 日志开关要等前端拿到配置才生效，早期失败必须能看到。
 */
export function reportCritical(message: string): void {
  send(message);
}

function send(message: string): void {
  void invoke("debug_log", { message: `${new Date().toISOString()} ${message}` }).catch(() => {});
}


// Synchronous UI work is buffered until the measured screen is ready, so no logging IPC
// runs inside React publication, layout measurement or canvas painting.
interface SynchronousTiming { name: string; startedAtMs: number; elapsedMs: number }
const synchronousTimings: SynchronousTiming[] = [];
let droppedSynchronousTimings = 0;
const noMeasurement = () => {};

export function beginSynchronousMeasurement(name: string): () => void {
  if (!enabled) return noMeasurement;
  const startedAtMs = Date.now();
  const started = performance.now();
  return () => {
    if (synchronousTimings.length >= 512) { droppedSynchronousTimings += 1; return; }
    synchronousTimings.push({ name, startedAtMs, elapsedMs: performance.now() - started });
  };
}

export function traceSynchronousMeasurement<T>(name: string, read: () => T): T {
  const finish = beginSynchronousMeasurement(name);
  try { return read(); } finally { finish(); }
}

export function flushSynchronousMeasurements(): void {
  if (!enabled || (synchronousTimings.length === 0 && droppedSynchronousTimings === 0)) return;
  const records = synchronousTimings.splice(0);
  const dropped = droppedSynchronousTimings;
  droppedSynchronousTimings = 0;
  logToBackend(`[TIMING_SYNC] ${JSON.stringify({ records, dropped })}`);
}
