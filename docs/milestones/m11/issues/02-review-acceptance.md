# 02：审查与桌面验收

Status: done
Blocked by: 01

## What to build

规划方按项目规范与 M11 规格两轴审查，修正后固定源码构建，在独立数据和本机模拟 AI 服务上完成真实 macOS 验收。

## Acceptance criteria

- [x] Standards / Spec 两轴审查，问题写入任务 Comments。
- [x] 固定实现 SHA、原始检查结果与失败记录。
- [x] 桌面地址可填写，模型自动列出并可选择，手填兼容。
- [x] 关闭重开保留地址、模型、语言和提示词，英文状态正常。
- [x] 明确真实 AI 服务和 Windows 未验范围。

## Comments

2026-11-11 规划方：固定源码 8587039 完成最终增量两轴复核、Release 构建及真实 macOS 桌面验收。结果与未验范围见 ../report.md；本机应用已更新，旧应用备份保留。
