# 第四版实际组件预览

[打开设计稿](../liquid-glass-v4.html)。本目录只服务 M7 第 1 步的独立外观预览，未接入产品、账号或 Git，不表示产品实施已开始。

## 来源

2026-10-30 读取用户提供的 [React Bits 仓库](https://github.com/DavidHDev/react-bits)，再固定源码提交 `e12c22192ac7b4b452c5e2feb9b586d74765d696`。[upstream.json](./upstream.json) 记录来源 URL、时间和原始 SHA256。`vendor/` 内三个组件的 TSX / CSS 与下载原件逐字相同：

| 组件 | 在设计稿中承担的部分 |
| --- | --- |
| [Specular Button](https://reactbits.dev/components/specular-button) | 提交、提交并推送、最近信息、提交选项、设置和审阅，共 6 个按钮 |
| [Glass Icons](https://reactbits.dev/components/glass-icons) | 左侧提交、日志、GitHub PR、终端，共 4 个入口；图形由既有 Codicons 传入 |
| [Glass Surface](https://reactbits.dev/components/glass-surface) | 窗口背景、外观浮层、最近信息浮层、提交选项浮层，共 4 个表面 |

这三个组件并非完整 Git 客户端组件库。文件类型图标、代码编辑区、列表语义与原生窗口按钮继续按 Revia 的要求处理。预览只说明这一组实际组件的搭配，并未声称所有控件均来自 React Bits。

## 适配

[mount.tsx](./mount.tsx) 挂载组件并同步主题、平台与降低透明度；[adapt.css](./adapt.css) 统一尺寸、边框、低饱和配色和焦点样式。提交按钮深色使用浅灰蓝 16% 填充，浅色使用深灰蓝 12% 填充；图标内容保持清晰，代码底层不透明。组件的默认大尺寸、自动居中和装饰层不直接决定业务布局。

Glass Icons 的 `icon` 属性接收调用方提供的图形，不自带分支、提交与文件扩展名的完整图标集。Specular Button 的边缘灯光使用 WebGL2，Glass Surface 有 SVG、WebGL 与磨砂路径；本轮 Chromium 使用 SVG 表面路径，不因此承诺 macOS WKWebView 或 Windows WebView2 已通过。降低透明度和强制颜色模拟关闭装饰滤镜并提供清晰边框。

原始许可完整保存在 [vendor/LICENSE.md](./vendor/LICENSE.md)，为 MIT + Commons Clause，保留其应用内使用和组件再分发条件，不标为普通 MIT。预览 bundle 使用工程已安装的 React、React DOM、Scheduler，版本见 [runtime.json](./runtime.json)，许可见 [React](./REACT-LICENSE.txt)、[React DOM](./REACT-DOM-LICENSE.txt)、[Scheduler](./SCHEDULER-LICENSE.txt)。本目录是 Revia 设计稿的一部分，不作为独立组件包发布。

## 复现

在仓库根目录运行以下命令，只构建独立预览，无需产品构建：

```sh
node docs/milestones/m7/mockups/react-bits/build.mjs
```

[build.mjs](./build.mjs) 使用工程已有 Vite / React 插件，把组件与运行时输出为本地 [JS](./bundle/preview.js) 和 [CSS](./bundle/preview.css)。打开 HTML 不需 CDN，也不需启动开发服务器；未修改工程依赖或锁文件。

## 验证

原件目录 `/private/tmp/revia-m7-reactbits-cljxi6_0`：包含旧稿、上游原件及哈希、构建命令、退出码、原 stdout / stderr、检查源码与截图。末轮 `check-t8wkYO` 为 16 / 16 通过：

- 两平台浅深色，1440×920 / 900×780，Windows 125% / 150% CSS 缩放、降低透明度与强制颜色模拟。
- 实际挂载 6 个 Specular Button、4 个 Glass Icons 入口与 4 个 Glass Surface；资源无加载错误，页面无错误。
- 提交区入口、居中、最近信息右对齐、弹层位置、草稿跨平台及主题切换保留、Escape 焦点返回。
- 按钮默认 / 悬停 / 按下 / 焦点的计算文字对比度：普通浅色最低约 5.79:1，深色约 5.60:1；按透明填充与底层合成并取黑白背后颜色的最差值，不声称覆盖任意主题或所有折射像素。
- 代码与行号无滤镜，代码底层不透明；原有代码色对比度、滚动和布局边界检查继续通过。

首轮 16 项通过后，人工看图发现组件 CSS 把最近信息的自动右对齐覆盖，已在适配层修正并补查右边界，再完成末轮；初版脚本、源码与截图均保留。人工另查两平台浅深色、窄窗、150% 浮层、强制颜色与降低透明度截图。

这些是独立 Chromium 预览检查，使用降低动态效果的浏览器环境。真实鼠标跟随动画、GPU 性能、原生窗口、系统 DPI、WKWebView / WebView2 与真实桌面验收仍留待产品实施阶段。
