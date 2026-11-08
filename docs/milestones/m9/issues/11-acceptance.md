# 11：集中复核与两个平台验收

Status: ready-for-human
Blocked by: 07, 08, 09, 10

## What to build

按第一轮结论完成调整，并完成生成提交信息后，出验收版：用户在 Windows 上做最终确认，规划方在 macOS 上验收；然后写 report，按工作流审查与合并。

## Acceptance criteria

- [ ] Pi 保存固定源码、精确命令、退出码和原始输出：完整 Rust 测试、`tsc -b`、lint、build、x18n check、`cargo check --features measure`。
- [ ] 规划方首轮按两轴审查全部改动，意见写进对应任务的 Comments；Pi 正式返修最多一次（Q113）。
- [ ] 删除对照开关和 `measure` 特性中仅供测量的部分，性能记录保留。
- [ ] 规划方推送 `m9` 出验收版；用户在 Windows 上确认 Q211 各项门槛：空闲 CPU、滚动帧时间、拖动缩放跟手、交互 100ms、打开与切换 0.3 秒。
- [ ] 规划方在 macOS 可见窗口中复测打开与切换门槛；确认 Windows 降级没有影响 macOS 外观。
- [ ] 生成提交信息在两种提交模式和 amend 下，用本机模拟服务完成端到端验收；真实服务由用户自己填密钥后确认。
- [ ] `report.md` 写明实测数据、验收结论、未验证项及原因、遗留问题。Windows 未经用户确认之前，保持 ready-for-human，不合并 main。

## Comments
