import type { OperationState } from "@/types/bindings/OperationState";
import type { AutostashPending } from "@/types/bindings/AutostashPending";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { toAppError } from "@/lib/error";

export interface M6ReadState {
  operation: OperationState | null;
  pending: AutostashPending | null;
  readErrors: AppErrorPayload[];
}

/** Git's operation and Revia's association are independent sources of state. */
export async function readM6State(
  operationRead: Promise<OperationState | null>,
  pendingRead: Promise<AutostashPending | null>,
): Promise<M6ReadState> {
  const [operation, pending] = await Promise.allSettled([operationRead, pendingRead]);
  return {
    operation: operation.status === "fulfilled" ? operation.value : null,
    pending: pending.status === "fulfilled" ? pending.value : null,
    readErrors: [operation, pending].flatMap((result) =>
      result.status === "rejected" ? [toAppError(result.reason)] : []),
  };
}
