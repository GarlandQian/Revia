import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";

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
  if (error && typeof error === "object") {
    if ("message" in error && typeof (error as { message: unknown }).message === "string") {
      return (error as { message: string }).message;
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
    const payload = caught as AppErrorPayload;
    return { kind: payload.kind, message: String(payload.message) };
  }
  return { kind: "internal", message: getErrorMessage(caught) };
}
