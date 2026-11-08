# 03：Windows 渲染降级

Status: ready-for-agent
Blocked by: None（可立即开始）

## What to build

Windows 上去掉网页绘制的模糊和选中项玻璃镜片，保留卡片和 Mica；Windows 10 与 CSS 材质路径去掉 SVG 折射和 WebGL 特效按钮。macOS 外观不变。见规格 B 节。

## Acceptance criteria

- [ ] Windows 11 Mica 路径：日志吸顶表头、通知、弹层、对话框、PR 面板确认框不再使用 `backdrop-filter`；对话框打开时不再对整个界面做 `filter: blur`。
- [ ] Windows 上不播放选中项玻璃镜片（编辑器页签、设置页导航），直接显示普通选中底。
- [ ] Windows 10 与 `REVIA_GLASS=css`：Windows 上不渲染 SVG 折射，不使用 WebGL 特效提交按钮。
- [ ] 侧栏与编辑区卡片、Mica 窗口底保留；深浅色、减少透明度、高对比度下，文字对比度不低于现状（Q190）。
- [ ] 平台判断集中在现有平台模块或样式条件里，业务组件不散落平台分支（Q55）。
- [ ] macOS 26+ 原生玻璃、macOS CSS 材质路径和镜片的表现不变。规划方在 macOS 上截图对照。
- [ ] 不新增恢复玻璃效果的设置项。

## Comments
