# M7 GitHub：事实核对

核对日期：2026-10-30。仓库基点 `8b9548774054c64921e2a1f57214b711f51c5f72`；调查只读，未访问账号、凭据或改动远端。下述事实不代替账号、登录和保护同步的产品决定。

## 登录

GitHub 支持 device flow：应用取得验证码，用户在浏览器打开 GitHub 授权页输入验证码，应用取得用户访问令牌。这一流程要求先注册应用获得 `client_id`，并在应用设置中启用 device flow；其令牌交换不需要 `client_secret`。普通 OAuth 网页授权码交换仍列 `client_secret` 为必填，不能因为支持 PKCE 就假定可以省略。来源：[GitHub OAuth 授权文档](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)。

据此，浏览器验证码授权可作为不把 client secret 放进安装包的桌面登录方案。第三轮 Q184 已确认采用 device flow，并提供手动 Token 备用入口；GitHub API 登录与 Q153 的 Git 传输凭据规则分别处理。后续核对了两种 App 的刷新接口，见下节；Q193 已选择 GitHub App，device flow 与 Token 备用入口继续按 Q184。

## 应用类型、权限与续期

GitHub App 可细分权限并选择安装覆盖的仓库；OAuth App 采用 scopes，私有仓库的日常 PR 功能需要较宽的 `repo` 授权。组织均可能限制或要求批准应用。GitHub App 的用户授权与安装是两件事：用户可以先授权，但受安装约束的组织资源还需要安装覆盖，令牌可访问范围取用户与 App 权限的交集。手动 Token 不会增加账号原本没有的权限，也不能保证绕过组织策略。来源：[App 比较](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/differences-between-github-apps-and-oauth-apps)、[以用户身份认证](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/authenticating-with-a-github-app-on-behalf-of-a-user)、[GitHub App 用户令牌](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-user-access-token-for-a-github-app)。

两种 App 的文档均允许 device flow 初次换取令牌时不携带 `client_secret`，刷新接口也明确给出相同例外。GitHub App 默认用户令牌 8 小时、刷新令牌 6 个月；刷新后需保存轮换后的两种令牌，到期后重新授权。当前 OAuth App 文档也包含到期令牌刷新接口，不能用“只有 GitHub App 支持短期令牌”作为选型理由。来源：[GitHub App 刷新](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/refreshing-user-access-tokens#refreshing-a-user-access-token-with-a-refresh-token)、[OAuth App 刷新](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps#refreshing-an-access-token-with-a-refresh-token)。

据这些接口要求，device flow 的续期不强制 Revia 携带应用密钥或自建认证中转；这是文档推论，未做真实授权验证。Q193 已选择精细权限、需要安装覆盖的 GitHub App；仍需真实注册与授权验证。

## PR 操作事实与待定范围

- 已有 fork 的分支可以创建跨仓 PR，与“自动创建 fork”是不同产品范围；不能从支持 PR 推导为已同意自动加 remote 或推送。[创建 PR](https://docs.github.com/en/rest/pulls/pulls#create-a-pull-request)
- PR 创建与审阅使用 `Pull requests` 写权限；合并的细粒度权限是 `Contents` 写，不能只靠仓库的 push 布尔值开放所有动作。合并可传用户确认的 head `sha`，变化返回 409；不应自动换新 SHA 重试。[PR 接口](https://docs.github.com/en/rest/pulls/pulls)、[审阅接口](https://docs.github.com/en/rest/pulls/reviews)
- 创建 review 时不传 `event` 可以形成服务端 PENDING 草稿，再统一提交多条行评论。是否自动保存到 GitHub、关闭后如何保留仍待产品决定，不能把接口能力记为已确认行为。[创建审阅](https://docs.github.com/en/rest/pulls/reviews#create-a-review-for-a-pull-request)

## Rebased 1.1.22 的登录参照

本机 Rebased 来自社区项目 [DetachHead/rebased](https://github.com/DetachHead/rebased)，是基于 IntelliJ 平台的开源 Git 客户端，不能把它称为当前 JetBrains 官方独立产品。[1.1.22 发布页](https://github.com/DetachHead/rebased/releases/tag/1.1.22) 与本机版本一致；本机 build 为 `IC-262.10968.SNAPSHOT`，GitHub 插件版本为 `262.10968.20261019`。

安装包中的 GitHub 设置接有三个入口：

| 入口 | 静态实现核对 |
| --- | --- |
| 浏览器登录 | `GHAccountsUtil.loginViaBrowser` 打开 OAuth 对话框，经 `GHOAuthService` 启动浏览器授权；授权码与 PKCE 流程使用 JetBrains 的 `https://account.jetbrains.com/github/oauth/intellij` 服务及本机回调 |
| Token 登录 | `GHAccountsUtil.loginViaToken` 打开令牌对话框，站点固定为 github.com；可粘贴已有令牌，也可打开 GitHub 的令牌生成页面 |
| Enterprise 登录 | 独立的令牌入口，允许填写服务器地址 |

`GHAccountsUtil.createAddAccountActionGroup` 的上述三个 action 已接入；所检查的入口未见 Rebased 专属禁用判断。浏览器调用链是授权码流程，并非此前向用户建议的 device flow。GitHub 插件内 2246 个 class 与资源条目检索未发现 device-flow 标记，这只是限定范围的静态证据。

共享平台的 [IntelliJ 官方账号说明](https://www.jetbrains.com/help/idea/set-up-a-github-account.html) 也列出浏览器 OAuth、Token、Enterprise 和多账号；它作为参照，不代替 Rebased 的实际运行验证。本轮没有启动应用或授权账号，不能据安装包接线断言 JetBrains 的服务目前接受 Rebased 登录。

第三轮 Q184 已确认 Revia 提供浏览器验证码授权与手动 Token 备用入口，注册 Revia 自己的应用并启用 device flow，不依赖 JetBrains 的应用身份或中转服务。该选择不等于采用 Rebased 的底层授权流程。

## 分支保护

`List branches` 的 `protected=true` 会返回受 classic branch protection 或 rulesets 保护的分支。这可以支撑同步“现有分支是否受保护”，但不等于复制完整保护策略、通配模式或旁路权限。来源：[GitHub 列分支接口](https://docs.github.com/en/rest/branches/branches#list-branches)。

`Get rules for a branch` 可以查询尚未创建的分支名，返回适用的活动 ruleset 规则，包括仓库和组织层面的规则；不返回 `evaluate` 或 `disabled` 的 rulesets。它与 classic protection 并存，不能据 rulesets 结果为空就宣称不存在任何分支保护。来源：[GitHub 分支规则接口](https://docs.github.com/en/rest/repos/rules#get-rules-for-a-branch)。

第二轮 Q185 已确认只读同步现有保护分支，与本地保护列表取并集；成功刷新后才替换远端缓存，失败保留上次结果并标记可能过期，不修改 GitHub 上的规则。第三轮 Q183、Q184 已确认 github.com 多账号与验证码授权、Token 备用入口。

## 原件与限制

上述三个官方页面的 Markdown 正文、URL、响应状态和 SHA256 保存在 `/private/tmp/revia-m7-planning-fslVoN9y/` 的 `github-oauth-authorizing`、`github-branch-list`、`github-rules` 文件组中。

独立事实代理 `/root/m7_github_auth_facts` 还核对了 PAT、组织限制、PR 权限与 Enterprise 差异。GrokSearch 搜索超时、抓取未返回，内置网页工具不可用，最后使用直接读取的 GitHub 官方公开 Markdown 正文；本文件中的三个关键页面也已由主代理直接读取。未做真实账号授权或 API 写入，功能验收仍待实施阶段。

本轮 Rebased 核对的网页工具原件在 `/private/tmp/revia-m7-round2-_ekzr32j/`：仓库与发布页、IntelliJ 说明，以及 Rebased 专属帮助页 404、两个猜测源码路径 404 和 GitHub API 目录查询 403 的原始结果均保留。当前客户端无 GrokSearch 入口，改用可用网页工具读取上述原页。

本机静态证据在 `/private/tmp/revia-m7-rebased-auth.W9rayw/`：`metadata.json` 记录版本、来源和 SHA256，`class-method-references.json` 记录方法引用与字节偏移，`raw/` 保存安装资源原件。主代理核对了 27 份资源原件与 3 个来源 JAR 的散列及关键调用链；未读取账号配置、钥匙串或用户凭据。

第三轮权限与续期补查的七份官方页面、GrokSearch 提取原件及独立报告在 `/private/tmp/revia-m7-pr-scope-facts.5Y5QBO/`；主代理核对了 `client_secret` 的 device flow 例外。本轮未进行账号授权或创建、评论、审阅、合并等远端写入。
