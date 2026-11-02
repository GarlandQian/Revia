# M7 玻璃镜片细节观察

Status: needs-triage
来源：2026-11-01 Codex 两轴审查，固定 `3f279cc...7c74dbd`（merge-base `262bd47`）。按 Q174 登记P3非阻断项，不扩大本轮重构；均为静态发现，未冒称真实像素或时序复现。

| 来源 | P3 观察 | 触发与证据 | 后续建议 |
| --- | --- | --- | --- |
| Standards | 已启动镜片未随系统偏好改变即时停止 | glassLens.ts:45–58 只在选中键变化时检查；动画中切减少动态效果 / 减少透明度 / 强制颜色，现有镜片最多继续约680ms | 媒体变化 / 关闭 / 卸载统一调用清理函数 |
| Spec | 设置镜片副本内距偏差 | glassLens.ts:126–133 的副本失去 index.css 的 `.settings-nav > .overflow-y-auto` 直子规则，10px退回px-2的8px | 副本保留原列表计算内距，实机核对后修正 |
| Spec | 搜索时短暂保留旧导航快照 | 520ms动画内搜索，SettingsDialog实时重排，镜片副本和mask仍使用旧列表 / 旧几何 | 筛选变化取消或刷新镜片，先补真实场景复现 |

依据：Q202 的镜片与辅助功能行为；不影响当前原有设置页、搜索与提交主流程的合并验收。原报告在 `/private/tmp/revia-m7-glass-round2-jej71x5e/review-standards.md`、`review-spec.md`；来源报告与后续关闭记录保留在此文件中。
