import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { t } from "@/lib/i18n/index";

/**
 * 按当前界面语言格式化结构化错误（ADR-0013、Q168）。
 *
 * Rust 侧只传 `code`、数据参数和结构化 `cause`；文案在展示时查当前语言。
 * 旧调用点没有 `code` 时退回诊断文本，避免丢失信息。
 */
export function formatAppError(error: AppErrorPayload): string {
  if (!error.code) return error.message;
  const params: Record<string, string> = { ...error.params };
  if (error.cause) params.cause = formatAppError(error.cause);
  const text = t(error.code, params);
  const main = text === error.code ? error.message : text;
  return [main, ...(error.related ?? []).map(formatAppError)].join("\n");
}

/**
 * 统一提取错误文字。
 * 兼容：
 * - 纯字符串
 * - AppErrorPayload ({ kind, message })
 * - 任意包含 string 类型 message 属性的对象
 * - Error 实例
 * - 回退为 String(error)
 */
export function getErrorMessage(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  if (error && typeof error === "object" && "message" in error) {
    const payload = error as Partial<AppErrorPayload>;
    if (typeof payload.message === "string") {
      return typeof payload.code === "string" ? formatAppError(payload as AppErrorPayload) : payload.message;
    }
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

/**
 * 将任意捕获异常规范化为 AppErrorPayload。
 */
export function toAppError(caught: unknown): AppErrorPayload {
  if (caught && typeof caught === "object" && "kind" in caught && "message" in caught) {
    const payload = caught as Partial<AppErrorPayload>;
    return {
      kind: payload.kind ?? "internal",
      code: payload.code,
      params: payload.params,
      cause: payload.cause,
      related: payload.related,
      message: String(payload.message),
    };
  }
  return { kind: "internal", params: {}, message: getErrorMessage(caught) };
}


/** Frontend validation keeps the same code/data contract as Rust errors. */
export function localizedError(
  kind: AppErrorPayload["kind"], code: string, params: Record<string, string | number> = {},
): AppErrorPayload {
  return { kind, code, params: Object.fromEntries(Object.entries(params).map(([key, value]) => [key, String(value)])), message: t(code, params) };
}

/** Listener identity and native failure stay structured across locale changes. */
export function closeGuardFailure(which: "window" | "exit", caught: unknown): AppErrorPayload {
  const listener = localizedError("internal", which === "window"
    ? "errors.closeGuard.windowListener" : "errors.closeGuard.exitListener");
  return { ...localizedError("internal", "errors.closeGuard.unavailable"),
    cause: { ...listener, cause: toAppError(caught) } };
}

export function closeGuardBlockedError(reason: AppErrorPayload | ""): AppErrorPayload {
  return reason || localizedError("internal", "errors.closeGuard.installing");
}

export function commitOutputListenerFailure(caught: unknown): AppErrorPayload {
  return { ...localizedError("internal", "errors.commitOutput.listenerFailed"), cause: toAppError(caught) };
}

/** Structured write outcomes may carry a failure without rejecting Promise. */
export function resultNeedsVerification(value: unknown): boolean {
  if (!value || typeof value !== "object") return false;
  if (Array.isArray(value)) return value.some(resultNeedsVerification);
  const result = value as Record<string, unknown>;
  return result.kind === "needsVerification"
    || ["error", "cleanupError", "autostash", "cause", "related", "continuation"].some((key) => resultNeedsVerification(result[key]));
}
