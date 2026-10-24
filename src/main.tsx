import ReactDOM from "react-dom/client";
import { App } from "./App";
import { ErrorBoundary } from "./components/app/ErrorBoundary";
import { reportCritical } from "./features/diff/log";
import { getErrorMessage } from "./lib/error";
// Inter 与 JetBrains Mono 随应用打包（Q107）：只带拉丁子集，中文回退到系统字体。
import "@fontsource/inter/latin-400.css";
import "@fontsource/inter/latin-500.css";
import "@fontsource/inter/latin-600.css";
import "@fontsource/jetbrains-mono/latin-400.css";
import "./index.css";

// 渲染异常与未处理的 Promise 都记进后端日志（只在测量会话里真正写入）。
window.addEventListener("error", (event) => reportCritical(`前端错误：${event.message}`));
window.addEventListener("unhandledrejection", (event) => reportCritical(`未处理的 Promise：${getErrorMessage(event.reason)}`));
reportCritical("前端模块已载入");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <ErrorBoundary>
    <App />
  </ErrorBoundary>,
);
