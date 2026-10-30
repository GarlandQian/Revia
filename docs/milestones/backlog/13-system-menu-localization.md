# 13：macOS 系统自动加入的菜单项跟随界面语言

**What to build:** 中文界面下，“编辑”菜单中的 AutoFill、Start Dictation…、Emoji & Symbols 与 Revia 菜单的 Quit and Keep Windows 显示为中文。

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] 应用包声明支持的本地化（如 CFBundleLocalizations 含 zh-Hans、en），系统项随界面语言或系统语言显示
- [ ] 不影响 M6 已实现的原生菜单双语与即时切换

## 背景

M6 桌面验收（2026-10-30）：Revia 自己的菜单项已双语，上述几项由 macOS 自动加入，因应用包未声明中文本地化而保持英文。属于打包配置，不阻断 M6。
