# Liquid Glass 组件（M7 任务 01）

来源：`docs/milestones/m7/mockups/react-bits/vendor/`（React Bits，David Haz）。
许可见同目录 `LICENSE.md`（MIT + Commons Clause v1.0），随组件一起保留。

- `GlassSurface`：毛玻璃容器。没有 backdrop 元素时走 CSS 磨砂回退，不启用 WebGL。
- `GlassIcons`：玻璃图标组，用于空状态与快捷入口。
- `SpecularButton`：带高光扫过的主操作按钮。

使用约束（Q189、Q190）：

- 只用于外壳、操作区与对话框；代码与 diff 的背景保持不透明。
- 三个组件保持原实现，不改成纯 CSS 替代品；尺寸与颜色通过 props / className 适配真实布局。
