# M7 任务图

Status: ready-for-human

全部交给同一个 Paseo / Pi 实施会话，按依赖推进；整包固定源码后由 Codex 审查。任务 13 的事实门槛未满足，任务 14 整包验收保持阻塞。

| 任务 | 交付 | Blocked by | 初始状态 |
| --- | --- | --- | --- |
| [01](./01-liquid-glass.md) | 采用第四版玻璃外观 | None | ready-for-human |
| [02](./02-theme-shortcuts.md) | 导入颜色主题并自定义快捷键 | None | ready-for-human |
| [03](./03-workspace-editing.md) | 编辑工作区 diff 并恢复草稿 | None | ready-for-human |
| [04](./04-terminal.md) | 运行归属 worktree 的内置终端 | None | ready-for-human |
| [05](./05-clone.md) | 克隆并打开仓库 | None | ready-for-human |
| [06](./06-github-auth.md) | 登录并管理多个 GitHub 账号 | None | ready-for-human |
| [07](./07-pr-read-checkout.md) | 查看 PR 并检出来源分支 | 06 | ready-for-human |
| [08](./08-pr-write.md) | 创建 PR、提交审阅与确认合并 | 07 | ready-for-human |
| [09](./09-branch-protection.md) | 同步现有 GitHub 受保护分支 | 06 | ready-for-human |
| [10](./10-diagnostics.md) | 预览并导出脱敏诊断 | None | ready-for-human |
| [11](./11-app-update.md) | 确认下载并安装应用更新 | 03, 04 | ready-for-human |
| [12](./12-actions-package.md) | 标签打包至 Release 草稿 | None | ready-for-human |
| [13](./13-changelists.md) | 按更改列表选择并提交 | None | needs-info |
| [14](./14-integrate-acceptance.md) | 整包审查与固定 Release 桌面验收 | 01, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13 | needs-info |

任务 01–12 的验收项限定为实现及相关检查；需要真实桌面的整体验收集中在任务 14。技术依赖必须满足任务所列验收后才能标 done；有未完成项则如实保留状态。Pi 可继续处理不受阻塞的任务，不能据整包目标擅自编造任务 13。

Codex 2026-10-31T03:14:30.306434+08:00：首轮两轴未通过；01–12进入正式返修，未完成项不能按旧勾选宣称通过。两轴原文及证据分别见各任务 Comments 与任务14；13/14事实门槛保持。
