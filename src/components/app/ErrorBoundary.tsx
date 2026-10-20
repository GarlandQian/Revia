/**
 * 渲染错误边界：把前端渲染异常写进后端日志，并显示一段可读信息，
 * 而不是让整个窗口变成空白。
 */

import { Component, type ErrorInfo, type ReactNode } from "react";
import { reportCritical } from "@/features/diff/log";

interface Props {
  children: ReactNode;
}

interface State {
  message: string | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { message: null };

  static getDerivedStateFromError(error: unknown): State {
    return { message: error instanceof Error ? error.message : String(error) };
  }

  componentDidCatch(error: unknown, info: ErrorInfo): void {
    reportCritical(`渲染错误：${error instanceof Error ? `${error.message}\n${error.stack}` : String(error)}\n${info.componentStack}`);
  }

  render(): ReactNode {
    if (this.state.message) {
      return (
        <div className="p-4 text-sm text-destructive">
          <p className="font-medium">界面渲染出错</p>
          <pre className="mt-2 whitespace-pre-wrap font-mono text-xs">{this.state.message}</pre>
        </div>
      );
    }
    return this.props.children;
  }
}
