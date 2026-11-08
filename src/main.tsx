import ReactDOM from "react-dom/client";
import { App } from "./App";
import { ErrorBoundary } from "./components/app/ErrorBoundary";
import { reportCritical } from "./features/diff/log";
import { getErrorMessage } from "./lib/error";
import { startNativeGlass } from "./features/m7/glass/nativeGlass";
import { isMac } from "./lib/platform";
import { perfRecorder } from "./lib/perfRecorder";
// 界面字体用系统字体（Q131）；代码字体默认随应用打包的 JetBrains Mono（Q133）。
import "@fontsource/jetbrains-mono/latin-400.css";
// 界面图标用 Codicons（Q130）：字体与图标类由 npm 包提供。
import "@vscode/codicons/dist/codicon.css";
import "./index.css";

import { ipc } from "./lib/ipc";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { messages, subscribeLocale } from "./lib/i18n/index";

// 渲染异常与未处理的 Promise 都记进后端日志（只在测量会话里真正写入）。
window.addEventListener("error", (event) => reportCritical(`前端错误：${event.message}`));
window.addEventListener("unhandledrejection", (event) => reportCritical(`未处理的 Promise：${getErrorMessage(event.reason)}`));
reportCritical("前端模块已载入");

document.documentElement.dataset.platform = isMac ? "macos" : "windows";

// M9 测量版对照外观（S4、Q210）：朴素对照下窗口完全不透明，不走原生玻璃与材质。
const measure = await ipc.measureState().catch(() => null);
if (measure?.plainAppearance) {
  document.documentElement.dataset.plainAppearance = "true";
  document.documentElement.classList.remove("window-material", "native-glass");
  // 窗口标题注明对照状态，跟随界面语言（Q164）。
  const applyTitle = () => {
    void getCurrentWindow().setTitle(messages.measure.plainAppearanceTitle).catch(() => undefined);
  };
  applyTitle();
  subscribeLocale(applyTitle);
} else {
  // 原生玻璃在首帧前决定，避免先画一帧 CSS 材质再切换。
  await startNativeGlass();
}
// 性能记录只在交互期间采样（M9 任务 01）；启动时安装一次监听。
perfRecorder.start();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <ErrorBoundary>
    <App />
  </ErrorBoundary>,
);
