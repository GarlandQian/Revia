/**
 * 渲染错误边界：把前端渲染异常写进后端日志，并显示一段可读信息，
 * 而不是让整个窗口变成空白。
 */

import { Component, type ErrorInfo, type ReactNode } from "react";
import { reportCritical } from "@/features/diff/log";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { formatAppError, toAppError, getErrorMessage } from "@/lib/error";
import { messages } from "@/lib/i18n/index";

interface Props {
  children: ReactNode;
}

interface State {
  message: AppErrorPayload | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { message: null };

  static getDerivedStateFromError(error: unknown): State {
    return { message: toAppError(error) };
  }

  componentDidCatch(error: unknown, info: ErrorInfo): void {
    reportCritical(`渲染错误：${error instanceof Error ? `${error.message}\n${error.stack}` : getErrorMessage(error)}\n${info.componentStack}`);
  }

  render(): ReactNode {
    if (this.state.message) {
      return (
        <div className="p-4 text-sm text-destructive">
          <p className="font-medium">{messages.errors.app.renderTitle}</p>
          <pre className="mt-2 whitespace-pre-wrap font-mono text-xs">{formatAppError(this.state.message)}</pre>
        </div>
      );
    }
    return this.props.children;
  }
}
