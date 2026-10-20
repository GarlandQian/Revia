// 不使用 StrictMode
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  // 原型是测量工具：关掉 StrictMode，避免开发期的双调用干扰测量与合成帧调度。
  <App />,
);
