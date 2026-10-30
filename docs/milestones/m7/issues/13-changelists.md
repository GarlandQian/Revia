# 13：按更改列表选择并提交

Status: needs-info
Blocked by: None

## What to build

在 Rebased 实测解阻后，提供互斥的更改列表模式并保证勾选内容与真实提交一致。

依据：规格 I；Q120、Q197、ADR-0003。

## Acceptance criteria

- [ ] 先取得 Rebased 1.1.22 在独占 fixture 中的实际观察：预先暂存、同文件部分暂存、切模式、只选其他块提交、完成后 index / 工作区；保存原 argv / exit / 输出与截图
- [ ] 规划方把实测语义补入规格和 Comments 后才能写 index / 提交路径；不能从静态实现、推测或 mock 补全
- [ ] 按 Q197 实现默认列表、单一归属、文件 / Git 块移动、删组回默认与提交后保留未提交部分
- [ ] 块失配保留分组信息、取消失效勾选，切模式保留列表；写前校验和数据保护门槛不变
- [ ] 真实 Git 断言提交树、index 与剩余工作区完全对应；实现细节另经审查

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

- Codex，2026-10-30：当前无桌面操控工具，AXIsProcessTrusted() 为 false，Rebased 事实前提尚未取得。此任务保持 needs-info，Pi 不猜测提交语义；任务 01–12 可推进。
