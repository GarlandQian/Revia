# 06：登录并管理多个 GitHub 账号

Status: ready-for-agent
Blocked by: None

## What to build

提供 GitHub App 浏览器验证码与 Token 备用入口，令牌安全存储，按仓库记住身份。

依据：规格 F；Q183、Q184、Q193。

## Acceptance criteria

- [ ] 未配置 App 时明确原因并保留 Token 入口；client secret / Token 不进源码、普通配置、日志或前端持久化
- [ ] 验证码、取消、过期、拒绝、slow_down 与刷新轮换可用；多账号、按仓库记住及退出登录正确
- [ ] 生产凭据存 macOS 钥匙串，测试使用独占适配；组织安装 / 权限限制给出明确状态
- [ ] 固定 github.com 信任来源，Git 传输认证独立；迟到请求不覆盖新账号 / 仓库
- [ ] 受控 HTTP fixture 通过生产 API 逻辑覆盖上述结果；真实账号 / 钥匙串验收留任务 14

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments
