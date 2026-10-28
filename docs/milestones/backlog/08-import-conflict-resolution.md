# 08：自动解决 import 冲突

Status: needs-triage
Blocked by: 无；尚未排期

## 背景与范围

Q139 曾将此项列在 M6 的设置清单。2026-10-28 用户在 Q167 明确：M6 不做，只放 backlog，不写进 M7。
本机 Rebased 通过语言解析与 import 范围 provider 判断能力；现有资料不能证明它对 Vue、TypeScript、Swift 等具体语言的支持，见 [M6 核对记录](../m6/references.md)。

后续规划时先确定支持的语言、安全规则和真实冲突样例，再决定是否实施。不得简单拼接 import 文本、误报冲突已解决，或先放一个不起作用的设置。
M6 继续使用已有三方合并；本条未完成不属于 M6 缺项，也不代表已纳入 M7。
