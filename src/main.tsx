import ReactDOM from "react-dom/client";
import { App } from "./App";
import { ErrorBoundary } from "./components/app/ErrorBoundary";
import { reportCritical } from "./features/diff/log";
import { getErrorMessage } from "./lib/error";
import { startNativeGlass } from "./features/m7/glass/nativeGlass";
import { isMac } from "./lib/platform";
// 界面字体用系统字体（Q131）；代码字体默认随应用打包的 JetBrains Mono（Q133）。
import "@fontsource/jetbrains-mono/latin-400.css";
// 界面图标用 Codicons（Q130）：字体与图标类由 npm 包提供。
import "@vscode/codicons/dist/codicon.css";
import "./index.css";

// 渲染异常与未处理的 Promise 都记进后端日志（只在测量会话里真正写入）。
window.addEventListener("error", (event) => reportCritical(`前端错误：${event.message}`));
window.addEventListener("unhandledrejection", (event) => reportCritical(`未处理的 Promise：${getErrorMessage(event.reason)}`));
reportCritical("前端模块已载入");

document.documentElement.dataset.platform = isMac ? "macos" : "windows";
// 原生玻璃在首帧前决定，避免先画一帧 CSS 材质再切换。
await startNativeGlass();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <ErrorBoundary>
    <App />
  </ErrorBoundary>,
);
