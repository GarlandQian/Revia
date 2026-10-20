/**
 * 前端诊断日志：只有在设置了 `REVIA_MEASURE_LOG`（测量会话）时才写后端日志，
 * 正常使用时是空操作，不产生额外 IPC。
 */

let enabled = false;

export function setLoggingEnabled(value: boolean): void {
  enabled = value;
}

export function isLoggingEnabled(): boolean {
  return enabled;
}

import { invoke } from "@tauri-apps/api/core";

export function logToBackend(message: string): void {
  if (!enabled) return;
  send(message);
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
