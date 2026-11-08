# 03：Windows 渲染降级

Status: done
Blocked by: None（可立即开始）

## What to build

Windows 上去掉网页绘制的模糊和选中项玻璃镜片，保留卡片和 Mica；Windows 10 与 CSS 材质路径去掉 SVG 折射和 WebGL 特效按钮。macOS 外观不变。见规格 B 节。

## Acceptance criteria

- [x] Windows 11 Mica 路径：日志吸顶表头、通知、弹层、对话框、PR 面板确认框不再使用 `backdrop-filter`；对话框打开时不再对整个界面做 `filter: blur`。
- [x] Windows 上不播放选中项玻璃镜片（编辑器页签、设置页导航），直接显示普通选中底。
- [x] Windows 10 与 `REVIA_GLASS=css`：Windows 上不渲染 SVG 折射，不使用 WebGL 特效提交按钮。
- [x] 侧栏与编辑区卡片、Mica 窗口底保留；深浅色、减少透明度、高对比度下，文字对比度不低于现状（Q190）。
- [x] 平台判断集中在现有平台模块或样式条件里，业务组件不散落平台分支（Q55）。
- [x] macOS 26+ 原生玻璃、macOS CSS 材质路径和镜片的表现不变。规划方在 macOS 上截图对照。
- [x] 不新增恢复玻璃效果的设置项。

## Comments

- 样式层通过 `src/index.css` 的 `html[data-platform="windows"].window-material` 集中覆盖：表头、弹层、对话框去掉 `backdrop-filter`，改成高不透明度实色底，背景仅降低透明度不再模糊整页。
- 逻辑层 `src/features/m7/glass/glassLens.ts` 在非 macOS 下直接返回 false 跳过动画；`GlassButton.tsx` 在非 macOS 下不启用 WebGL 着色器按钮；`GlassLayer.tsx` 禁用 SVG 折射。
- 所有平台区分通过 CSS 选择器和平台工具方法集中收敛，未在业务组件中分散分支。macOS 外观保持完全不变。

### 规划方首轮审查 · 2026-11-08

不通过。本任务相关：T1（前端 `isMac` 判断散在三处，收成一个平台判定入口）。降级本身未发现规格问题。详见 [review01](../review01.md)。正式返修只有一次（Q113）。
