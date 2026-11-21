# 界面外壳与代码颜色主题解耦，采用实色现代桌面设计系统

M7 引入的整窗 Liquid Glass、React Bits 材质层、SVG 玻璃镜片以及由 VS Code 颜色主题 JSON 驱动整套应用外壳的做法，导致界面层次割裂、对比度不稳、macOS 与 Windows 观感分化，并在 Windows WebView2 上引发明显渲染负担与大量 CSS 覆写包袱（Q200、Q202、Q209）。M13 起我们彻底移除前后端所有 Liquid Glass、React Bits 材质与镜片基础设施，将应用外壳与 VS Code 颜色主题解耦（Q235、Q237、Q239）：窗口回归标准不透明原生窗口，外壳（顶栏、侧栏、列表、按钮、弹窗、表单、提示条）统一使用 Revia 自研精调的浅/深色中性灰阶 Design Tokens 与 Lucide 细线图标；代码 diff、三方合并与 blame 区域则完整保留所选或导入的 VS Code 颜色主题（含代码底色、语法高亮与增删行色），通过 1px 细边框与外壳自然隔离。

## Considered Options

- **继续全壳由 VS Code 主题驱动**：保留主题对侧栏、按钮、弹窗的染色能力，但第三方主题的 UI 键覆盖参差不齐，极易造成控件对比度失衡和旧式 IDE 观感。
- **代码底色也强制统一到外壳底色**：整窗色相最一致，但会导致 Solarized、Dracula、One Dark Pro 等主题的语法对比度偏移。
- **保留精简版原生毛玻璃**：macOS 与 Windows 仍需维护两套透明窗口与降级路径，且在代码审阅工具中对可读性增益有限。

## Consequences

- 前后端移除 `src/components/m7/glass/`、`src/features/m7/glass/`、Rust 侧 `macos_glass.rs` / `window_glass.rs` 与 `m7_glass_*` IPC；macOS 仅保留红绿灯融入顶栏的标题栏样式，Windows 保留原生边框，两平台彻底免除昂贵的网页背景模糊。
- 新增“外观模式”（跟随系统 / 浅色 / 深色，默认跟随系统），联动切换外壳浅/深色令牌与对应的浅/深色代码颜色主题；导入的 VS Code 主题 JSON 仅解析代码表面、语法高亮与 diff 相关色键，不再改写外壳控件色。
- 前端样式以语义 Design Tokens 与升级后的 Base UI 基础原语重建，取代层层覆写的单文件 `index.css`。
