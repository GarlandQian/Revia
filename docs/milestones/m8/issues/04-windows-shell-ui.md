# 04：Windows 外壳、快捷键与输入

Status: ready-for-human
Blocked by: None（可立即开始）

## What to build

共享界面在 Windows 使用原生窗口与正确按键，同时保留清晰材质回退和终端输入。

## Acceptance criteria

- [x] 原生标题栏、系统菜单、最大化、拖动与贴靠接线正确，macOS 外壳不回归。
- [x] Ctrl/Alt/Shift 提示、匹配与菜单一致，输入框 Ctrl+Alt 与 AltGr 不触发后台业务操作。
- [x] 沿 Windows 11 Mica、Windows 10 CSS 与 Q202 镜片/浮动卡片，修复发现的缺口。
- [x] 高对比度、减少透明度/动态效果有稳定回退；已有焦点、弹层和关闭协调保持。
- [x] 通过相关现有检查；Windows DPI、IME、像素观感与滚动性能列入任务 07，未实测不勾通过。

## Comments

实施说明（`a30b435`、`af869f9`）：

- Windows 窗口配置保持 `decorations: true` / `shadow: true` / `transparent: true`，原生标题栏；
  macOS 仍由 Tauri 按原配置建窗，前端构建与既有测试通过。
- 修复非 macOS 快捷键文案（`Ctrl+Shift+K` 而不是 `Ctrl++K`），抽出纯格式化函数并在
  Node 回归中同时断言 macOS / Windows 文案。
- AltGr（Ctrl+Alt 字符输入）与输入法组合不再触发全局业务快捷键；日志视图 Ctrl+F / Ctrl+D
  也加入保护。Mica 与 CSS 回退沿用既有接线，未发现需修改的缺口。
- 真实 Windows 的 DPI、IME、Mica 观感、镜片与滚动性能未执行，待任务 07。
