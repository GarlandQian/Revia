# M2 桌面补验成果快照

记录时间：2026-10-25T23:57:43.076055+08:00。用户最新要求先提交、准备 M3，当前只实现功能、不再开窗口或测试。因此停止剩余桌面补验；本记录不是 M2 全部验收通过声明。

## 固定版本与原件

- 普通模式 run01/run03：源码 `50018c6`（与受测 `59b925d` 的143份产品源码一致），Release SHA-256 `28da0f0dc4bf542af589c0e73cc6592520860adee0852fa0f0b5da899438e9fd`。
- 自建 fixture、linked worktree、独立应用 data；未对真实用户仓库试写。
- [无损归档清单](./evidence/desktop-closeout-20261025/manifest.json)含原始截图/AX、字节/权限/index/data 快照、操作收据、分析脚本和失败尝试。现场 `/private/tmp/revia-m2-desktop-closeout.K0wQZe` 保留。
- 独立分析 `desktop-analysis.json`，commands07退出0。首版commands06失败来自错误要求stage/unstage后raw index元数据也相同；修正为语义index相同，原失败及v1脚本保留。不是补跑产品测试。

## 实际完成

| 项目 | 实际观察与证据 |
| --- | --- |
| D02 键盘与范围 | Tab/Return激活hunk暂存/取消暂存，`⌥⌘A`整文件往返；`⌥⌘Z`确认范围及弹窗不穿透 |
| D02 hunk丢弃/回收/恢复 | 首hunk `9010→10`，第二hunk `1140`保留；新record `record-1792941412253-10993-5`的saved.before等于丢弃前完整文件，completed.after等于真实丢弃后文件。真实恢复后全部文件、权限、raw index与操作前一致；22/23/24截图、两份integrity JSON |
| D03 旧响应 | 真实A list_changes交付hold，真正切到B后显式释放，B未被旧A覆盖；7条交付收据均explicit:true |
| D03/D08 拒绝及降级 | 显式watch failure下真实旧hunk写入Rejected，工作区/raw index不变；“操作被拒绝”和常驻监视警告同时可见 |
| D04 审阅失效 | 等长工作区 `1010→9010`失效；随后仅HEAD变化、工作区和真实index字节不变时再次失效。HEAD fixture使用alternate index并同时纳入既有partial暂存版本，不声称仅单路径HEAD变化 |
| D05 原Close/Wait | PID9652合法stage_file成功后hold，窗口Close→Wait→显式release，原关闭自然完成exit0 |
| D08 字号 | 真实 `super+KP_Add` 12→13后减回12；其他无效按键变体保留，不归因产品故障 |
| 任务05只读 | 在B查看A记录IdentityMismatch、直接恢复disabled而Git只读diff可用；回A的LaterChanges只读查看；前后文件/权限/index不变 |
| 任务05清空 | 用户最终确认后真实GUI清空6条本轮record，26截图/AX为空列表；文件、权限、raw index及其余全部data字节未变。清空已完成，不重复执行 |

## 保留的失败与边界

- 首次丢弃确认因重新激活失效，21截图与完整性快照证明无写入；取消后重建确认才完成。无效选择/旧AX编号/字号按键尝试保留。
- run01/run03正常退出0；run02沙箱启动立即exit -6且双流空，未确认产品根因。
- 任务05这里只证明非当前worktree身份不匹配，没有真实删除/移动原目录，不冒称该桌面场景完成。相应真实Git自动化既有证据另计。
- Dock API多次timeout，之后Mac锁定；**真实Dock Quit代表样本未执行**。run04-dock为新 `b185f6c` Release（SHA `b7ec64a2ba602196f6fa2bb34fef87cfd9a38955259f9de625f2f92f130bf0a3`），PID17198，未点击暂存，未有该hold的captured。按用户要求撤下尚未触发的规则，不kill、不唤醒、不再开窗口。最后exec72438仍live，未取得自然退出证据；冻结双流只代表归档时点，不是终态日志。
- 本轮成果可提交，但任务08/10/11/13不因暂停测试而自动done，不据此squash main。既有Q98通过保留其版本，不重复长测。
