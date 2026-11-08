# 08：生成提交信息的发送内容与 AI 请求

Status: done
Blocked by: 07

## What to build

在 Rust 层按提交模式整理出下一次提交将包含的改动，连同最近的提交标题和固定规则，以 OpenAI 兼容的流式请求发给 AI 服务；支持取消，并把结果清理后逐段交给前端。见规格 E、F 节。

## Acceptance criteria

- [x] 暂存区模式取已暂存改动；更改列表模式在私有 `GIT_INDEX_FILE` 上取勾选内容，复用提交准备独立 index 的逻辑，用户 index 前后一致；amend 相对原提交父提交取 diff（根提交时相对空树），并附上原提交信息。
- [x] diff 固定使用 `--no-ext-diff --no-textconv --no-color` 和 `-c core.quotePath=false`；二进制文件、规格列出的 lock 文件、子模块只写路径和状态。
- [x] 单个文件截断到 6,000 个字符、总量截断到 24,000 个字符，标注“已截断”；始终附上文件清单（超过 300 项时写明另有几个文件）和最近 10 条提交标题（只取首行）。
- [x] 固定规则追加在用户的提示词之后：生成语言、首行不超过 `commit_subject_limit`、首行与正文空一行、只输出提交信息。
- [x] 请求发往 `{服务地址}/chat/completions`（地址已以该路径结尾时原样使用），`stream: true`；没有密钥时不带 Authorization；不发送 `temperature`。
- [x] 连接超时 15 秒，两段数据间隔超过 60 秒视为超时；取消后关闭连接，不再推送数据。
- [x] 去掉思考字段、`<think>…</think>` 段和包裹全文的代码围栏；结果为空按失败处理。错误分为认证失败、模型不存在、限流、网络、超时、服务端、空结果，以结构化双语错误返回，并去掉其中可能出现的密钥。
- [x] 检查目标地址是否已确认发送，未确认时拒绝请求，由前端先弹确认。请求和返回内容不进 Git 命令日志和诊断信息。
- [x] 补真实 Git 回归测试，覆盖规格 Testing Decisions 列出的发送内容场景；补本机 TcpListener 模拟 SSE 服务的回归测试，覆盖请求、清理、取消、错误映射、间隔超时。测试不访问真实服务。

## Comments

- 发送内容生成已在 `src-tauri/src/ai/payload.rs` 实现：覆盖暂存区模式、更改列表私有独立 index 模式、amend 相对父提交（或空树）模式，自动过滤 18 种 lock 文件与二进制文件，自动截断大文件（6000 字符）与总内容（24000 字符），附带最近 10 条提交标题与固定规则。
- HTTP 流式请求实现在 `src-tauri/src/ai/commands.rs`：使用 reqwest 流式调用 `/chat/completions`，支持无密钥模式，不发送 temperature，通过 `StreamFilter` 清洗 `<think>` 标签和代码块围栏，通过 `AiManager` 注册与取消请求。
- 补全真实 Git 回归测试（`src-tauri/src/ai/payload.rs`）和基于 `TcpListener` 的本机模拟 SSE 单元测试（`src-tauri/src/ai/commands.rs`），覆盖了请求格式、授权头、思考清洗、中途取消、错误状态码映射及分段超时等全部场景，测试全部通过。

### 规划方首轮审查 · 2026-11-08

不通过。本任务相关：S5（固定 diff 前缀、结构化判断二进制/子模块/lock）、S6（发送内容不得进 Git 命令日志）、S7（更改列表 amend 用勾选内容建私有 index）、S12（请求 ID 先注册再发请求）、S13（缺少的发送内容与请求测试）；P3（标题严格取首行）；`stream_inner` 参数合并、重复的错误与判断。详见 [review01](../review01.md)。正式返修只有一次（Q113）。
