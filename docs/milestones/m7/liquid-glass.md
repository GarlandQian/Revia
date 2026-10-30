# M7 Liquid Glass：方向与实现核对

更新日期：2026-10-30。这是 M7 第 1 步的设计记录与独立预览，产品源码尚未改动，不是桌面验收报告。

## 用户已确认

- M7 外观改为 Liquid Glass，可以使用 [React Bits](https://github.com/DavidHDev/react-bits)（Q189）。
- 偏强透明、折射与悬浮感，同时代码必须清晰（Q190）。
- Rebased 的布局和功能基准、现有颜色主题、字体、图标与 Git 安全边界继续沿用；详见 [UI 方向](../../ui-direction.md#视觉风格)。

预览默认将强玻璃放在顶栏、侧栏和浮层，代码区使用不透明底色。用户进一步询问全界面玻璃是否可行，因此另加一个对比模式：编辑器也有玻璃边缘和背景材质，但底层采用低透明度，文字、行号和增删标记保持清晰。该询问尚不构成选择代码区透明；两种模式都不让文字参与模糊或位移滤镜。

## 实现边界

| 部分 | 设计处理 |
| --- | --- |
| 顶栏、侧栏、按钮、浮层 | 强玻璃背景、边缘高光与悬浮阴影；文字和控件在清晰的独立内容层 |
| 代码与 diff | 稳定底色、语法色、增删底色与行号；默认不透明，全玻璃对比提案使用厚重的低透明底层 |
| 窄窗 | 保留代码字号，使用滚动或已选的换行方式；不靠缩小代码换取空间 |
| 交互 | 现有组件继续负责焦点、快捷键、弹层定位、拖动及写前校验；材质层不拦截指针 |
| 效果验证 | 实际 WKWebView 分别验浅深色、滚动、弹层及降级状态；仅磨砂不等于强折射已实现 |

## React Bits 核对

官方 [Glass Surface](https://reactbits.dev/components/glass-surface) 提供 liquid glass 材质。[安装说明](https://reactbits.dev/get-started/installation) 按组件交付源码，可选择 JS / TS 与 CSS / Tailwind 形式，手动复制或通过 shadcn CLI 引入。无需因此替换现有 Base UI 功能控件。

当前 [许可](https://github.com/DavidHDev/react-bits/blob/main/LICENSE.md) 是 **MIT + Commons Clause**，包含作为应用或网站一部分使用和分发的条款，同时限制单独销售、转售或重新分发组件本身；引入时保留原版权和完整许可声明，不标为普通 MIT。本轮预览未复制 React Bits 组件源码。

对 2026-10-30 取得的 `main` 源码核对如下；这些是已保存字节的事实，不代表已固定上游 commit：

- [TypeScript + CSS 组件](https://github.com/DavidHDev/react-bits/blob/main/src/ts-default/Components/GlassSurface/GlassSurface.tsx) 有 SVG、WebGL2 和磨砂三条路径。SVG 探测排除匹配 Safari 且不含 Chrome 的 UA 及 Firefox；其余只检查 CSS 属性是否接受，非像素结果验证。
- WebGL2 需要 `backdrop`、开启折射且上下文可用；`backdrop` 仅接受 `img`、`video` 或 `canvas`，不能把普通 DOM 面板自动截成折射纹理。未提供有效源时不能承诺 WebGL 折射。
- 每个 WebGL 实例独立创建上下文和动画循环，实施时应控制材质实例数量，不在每个文件行或 diff 行创建一个。
- 默认尺寸、padding、居中和裁剪不能直接套在 Revia Shell 上；应适配装饰层，保留已有布局和弹层行为。

原件在 `/private/tmp/revia-m7-glass-source.8JI4U0/`，manifest 记录 URL、取得时间与 SHA256：

| 原件 | SHA256 |
| --- | --- |
| `ts-default.tsx` | `3cfd551b02c48aab38418a1de724ebba35c276c27bd11dcf032197f5c6d56888` |
| `ts-default.css` | `90cfc3ad54ad7aa92bb2b700696e2d9f2d8704162ba1b94ebf0b6f8dcecc4e61` |
| `ts-tailwind.tsx` | `5475561bc5a62ce9476e7e7b8f4fd4c5b741b6dd1bc17f6f6fe76f021ba2428e` |

根代理先读取了用户提供的仓库 URL，官网、安装与许可页面的工具原件保存在 `/private/tmp/revia-m7-round3-n3ww6iju/m7_react_bits_*.json`。根代理没有 GrokSearch 入口，使用可用网页读取工具；独立源码核对保留了三个版本的原始字节。

## Revia 的实际起点

本轮基点 `4f9517ceeff84938686e17bf976ccd512be5fea8`：`Cargo.lock` 锁定 `wry 0.57.0`，本机 crate 源码的 macOS 实现使用 WKWebView；`src-tauri/tauri.conf.json` 的标题栏 Overlay 没有设置透明窗口或原生玻璃。不能从 Overlay 推断能够透出桌面壁纸。

`src/index.css` 管理共享样式，`src/lib/theme.ts` 设定颜色主题，`Shell.tsx` 与 App 侧栏是外壳接入点。弹层、通知、对话框有现成集中样式。`--editor-background` 同时支撑 diff 背景与代码行，不应通过把它改透明来开启整窗效果。

本轮没有取得真实 Revia 的 UA，也没有在 WKWebView 中验证 React Bits 三条渲染路径。不要写成“Safari 只能磨砂”或“所有效果已经兼容”。

## 预览与检查

[打开预览说明](./mockups/)；HTML 可切浅深色、浮层与全界面玻璃对比。它使用自绘 CSS / SVG 材质及示例代码，复用已有 M4 图标资源；没有连接 Git、账号或产品 IPC。代码示例用于呈现颜色和密度，不是 M7 实现片段。

预览检查与截图记录在说明中；它们只证明独立设计稿的显示与控制，不代替真实 Revia 的布局、性能、折射或桌面验收。
