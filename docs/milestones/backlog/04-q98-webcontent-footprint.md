# 04：长测中 WebContent 压力后占用超出 Q98 门槛

**What to build:** 30 分钟正式长测中，内存压力后的 WebContent footprint 热身后最大增长不超过 10 MiB、趋势不超过每分钟 0.5 MiB（Q98）。

**Blocked by:** None (can start immediately)

**Status:** needs-triage

- [ ] 找出压力后仍在增长的内存来源，写进报告
- [ ] 正式长测按 Q98 原门槛通过，Rust 与 malloc allocated 两项不退化

## 背景

M5 `7681f9f` 正式长测：footprint 最大增长 10.2 MiB（超 0.2 MiB）、趋势 +0.27 MiB/分钟；此前第七包为 24.0 MiB、+1.00。
剩余增长主要是 vmmap 的 Untagged 区域（约每 5 分钟 8 MiB 虚拟、2 MiB dirty），各类单项诊断和 M2 通过的长测里都有，来源未定。
数据与对照见 [M5 报告](../m5/report.md) 22.14、22.16、22.18、22.19。2026-10-28 随 Q154 转入此处。
