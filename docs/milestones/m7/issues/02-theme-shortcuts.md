# 02：导入颜色主题并自定义快捷键

Status: ready-for-review
Blocked by: None

## What to build

从设置导入本地 JSON / JSONC 颜色主题并按作用范围修改快捷键，保留当前编辑上下文。

依据：规格 B；Q132、Q139、Q198。

## Acceptance criteria

- [x] 浅深色各自选择，缺失颜色回退对应内置主题，非法文件保留当前主题；不运行扩展代码
- [x] 快捷键冲突确认替换，支持单项 / 全部恢复默认，提示和修饰键同步
- [x] 输入框、编辑器、终端、弹层中不重复触发全局动作；设置可持久化并恢复
- [x] 现有设置 / 主题 / 键盘检查通过；数据解析与冲突核心行为有必要验证

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / review01 Standards — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/standards.md；完整证据附录保存在本轮证据目录。

11. **[P3，非阻断] 术语**：`locales/zh-CN.json:2255` 快捷键显示“hunk”，违反 `GLOSSARY.md:64`“界面一律写‘块’”；当前 x18n glossary 未覆盖此词。

## H11 中文界面重新暴露 hunk（P3，非阻断）

规则：GLOSSARY.md:63–65，“界面一律写‘块’”，“Avoid 界面中的 hunk”。

locales/zh-CN.json:2255–2256 新增“下一个 hunk/上一个 hunk”，由 shortcuts.ts 的 shortcutDefs 在设置表中直接显示。x18n.config.ts 的 glossary 未配置 hunk/块规则，因此不是重复报告工具已强制检查的项。改动应由后续实施经 x18n 完成，本审查不修改 locale。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F17 P2 | “确认替换”“提示…同步”：冲突只拒绝，提交框仍旧键 | src/features/m7/settings.tsx:199；src/components/app/CommitComposer.tsx:247 |

## F17 [P2] 快捷键替换缺失且提交框仍用旧绑定

规格 S76：“快捷键按实际作用范围检测冲突，确认替换后才保存”；任务02：“快捷键冲突确认替换…提示和修饰键同步”。

`src/features/m7/settings.tsx:199–203` 遇冲突只报错return，没有确认替换；`src/lib/shortcuts.ts:110–120`不判断scope/平台等价。`src/components/app/CommitComposer.tsx:247–251,296` 仍用isCommitShortcut/COMMIT_SHORTCUT_LABEL，而App2970排除提交框后才自定义分派。触发：把commit从mod+k改mod+j，在提交框按J/K。结果：J不提交，K仍提交且提示旧K；真实纯函数输出见ui-probes.stdout，App对target的静态分派确认这一结果。没有真实键盘桌面验收。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Pi 正式返修处理 — 2026-10-31

- **F17**：冲突不再直接拒绝——记录待确认替换，UI 提供“替换 / 取消”；替换时冲突动作回落默认。提交框（CommitComposer）改用 `matchesAccelerator(event, acceleratorOf("commit"))`，快捷键提示同步 `formatAccelerator(acceleratorOf("commit"))`，跟随用户绑定。
- **H11**：`shortcuts.actions.nextHunk/prevHunk` 中文改“下一个块/上一个块”（经 x18n apply，不直接改 JSON）。
