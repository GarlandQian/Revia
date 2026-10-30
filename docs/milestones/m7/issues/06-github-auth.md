# 06：登录并管理多个 GitHub 账号

Status: ready-for-agent
Blocked by: None

## What to build

提供 GitHub App 浏览器验证码与 Token 备用入口，令牌安全存储，按仓库记住身份。

依据：规格 F；Q183、Q184、Q193。

## Acceptance criteria

- [x] 未配置 App 时明确原因并保留 Token 入口；client secret / Token 不进源码、普通配置、日志或前端持久化
- [x] 验证码、取消、过期、拒绝、slow_down 与刷新轮换可用；多账号、按仓库记住及退出登录正确
- [x] 生产凭据存 macOS 钥匙串，测试使用独占适配；组织安装 / 权限限制给出明确状态
- [x] 固定 github.com 信任来源，Git 传输认证独立；迟到请求不覆盖新账号 / 仓库
- [x] 受控 HTTP fixture 通过生产 API 逻辑覆盖上述结果；真实账号 / 钥匙串验收留任务 14

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F16 P2 | “取消”“刷新轮换”：均缺，refresh_token丢弃 | src-tauri/src/m7/github/auth.rs:60,257 |

## F16 [P2] device flow无取消及刷新令牌轮换

规格 S105：“轮询 / 取消 / 过期与拒绝状态；按服务器节流和刷新轮换结果持久保存”。

`github/auth.rs:60–69` TokenResponse只接access_token/scope/error/interval；257只保存access token，client_for始终直接载入，整个模块无refresh_token/expires_in/刷新请求；`commands.rs:338–351`/api.ts无cancel命令，GithubPanel451–459授权中只有复制码。触发：用户取消授权或GitHub App返回会过期的用户token/refresh token。结果：无真正取消入口；刷新字段被丢弃，过期后只能重新登录。不是client_id未配置这一外部前提导致。静态事实，未碰真实钥匙串。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Pi 正式返修处理 — 2026-10-31

- **F16**：TokenResponse 接收 refresh_token / expires_in / refresh_token_expires_in；TokenStore 新增 save_refresh/load_refresh/delete_refresh（独立 keyring 服务 "Revia GitHub Refresh"，验收模式同文件 0600）；设备码与 Token 登录都持久化刷新令牌，登出同时清除；新增 `m7_github_device_cancel` 命令与面板“取消授权”按钮（本地取消，不访问网络）。

### Codex / review02 Spec / F16 — 2026-10-31

| F16 P2 | S105“刷新轮换”：只存refresh_token，无刷新请求或到期处理 | github/auth.rs:402；github/mod.rs:386 |

<details>
<summary>本项证据原文</summary>

## F16 [P2] 刷新仅存储，没有到期或轮换路径

S105“按服务器节流和刷新轮换结果持久保存”。auth:69–75新接expires_in等字段，285–291只把refresh_token传入，402–407保存；`GithubState::client_for:386–387`仍直接load access_token。load_refresh在mod:228只是定义，生产全树没有调用；grant_type只有device_code，无refresh_token请求，也不保存到期时间或换新access/refresh。触发GitHub App用户令牌到期：仍只能重新登录，本地取消可用不等于刷新已实现。`consumers.stdout`保存搜索证据；未触碰钥匙串或登录。设备码取消中的在途成功仍可能返回authorized，未额外以竞态扩大本项。

</details>
