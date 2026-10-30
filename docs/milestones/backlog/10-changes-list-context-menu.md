# 10：改动列表右键提供应用菜单

**What to build:** 在改动列表的文件和分组上右键，弹出 Revia 自己的菜单（暂存 / 取消暂存 / 丢弃 / 文件历史 / 用外部工具查看 diff 等已有动作），不再出现 WebView 自带的 Look Up、Translate、Search with DuckDuckGo 等系统项。

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] 文件行与分组行右键都显示应用菜单，动作与 Rebased 的改动列表右键对照
- [ ] 菜单动作复用现有写门禁与确认，不新增未确认的能力
- [ ] 其他非输入区域不再弹出 WebView 默认菜单

## 背景

M6 桌面验收（2026-10-30）发现。属于 M4 改动列表范围，不阻断 M6。
