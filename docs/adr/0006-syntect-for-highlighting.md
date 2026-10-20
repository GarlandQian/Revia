# 语法高亮在 Rust 侧使用 syntect，不用 Shiki

M0 原型对比了前端 Shiki 和 Rust 侧 syntect，最终选择 syntect：高亮在 Rust 层完成，按可见区域优先分块返回给界面，界面只负责渲染。Shiki 的语法定义来自 VS Code 生态，引擎本身也更快（同一份数据约 0.8 秒，syntect 约 1.7–1.9 秒），但它默认的 JavaScript 正则引擎在 Tauri 的 WebView（WKWebView）里处理一个 8 千行的文件，35 秒都没有返回；能用的 Oniguruma WASM 引擎又要多带约 1.9 MB 的产物，还要单独维护一个 Web Worker 线程模型。syntect 的着色是异步进行的，不影响首屏和滚动，而且和 Git 解析放在同一侧，与 [ADR-0004](./0004-hunks-come-from-git.md) 的思路一致。

## Consequences

- 着色总耗时比 Shiki 长，所以必须保留“先显示纯文本、再异步着色、可见区域优先”的做法，不能让高亮进入首屏路径。
- 高亮必须基于旧、新两个版本的完整文件进行，再映射到 diff 行；只高亮 diff 片段会把多行注释、模板字符串标错颜色。
- 以后如果要改用 Shiki，必须使用 Oniguruma 引擎，不能用它的 JavaScript 正则引擎。
