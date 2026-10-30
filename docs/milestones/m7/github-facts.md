# M7 GitHub：事实核对

核对日期：2026-10-30。仓库基点 `8b9548774054c64921e2a1f57214b711f51c5f72`；调查只读，未访问账号、凭据或改动远端。下述事实不代替账号、登录和保护同步的产品决定。

## 登录

GitHub 支持 device flow：应用取得验证码，用户在浏览器打开 GitHub 授权页输入验证码，应用取得用户访问令牌。这一流程要求先注册应用获得 `client_id`，并在应用设置中启用 device flow；其令牌交换不需要 `client_secret`。普通 OAuth 网页授权码交换仍列 `client_secret` 为必填，不能因为支持 PKCE 就假定可以省略。来源：[GitHub OAuth 授权文档](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)。

据此，浏览器验证码授权可作为不把 client secret 放进安装包的桌面登录方案；这是依据接口要求得出的实现判断。是否采用该方案、采用 OAuth App 还是 GitHub App、是否提供手动令牌入口，尚未由用户确认。GitHub API 登录与 Q153 的 Git 传输凭据规则分别处理。

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

对 Revia 的建议仍待 Q184 确认：提供浏览器登录与手动 Token 备用入口；若浏览器入口采用此前的验证码授权，需注册 Revia 自己的应用并启用 device flow，无需借用 JetBrains 的应用身份或中转服务。不能将该建议表述为与 Rebased 相同的底层授权流程。

## 分支保护

`List branches` 的 `protected=true` 会返回受 classic branch protection 或 rulesets 保护的分支。这可以支撑同步“现有分支是否受保护”，但不等于复制完整保护策略、通配模式或旁路权限。来源：[GitHub 列分支接口](https://docs.github.com/en/rest/branches/branches#list-branches)。

`Get rules for a branch` 可以查询尚未创建的分支名，返回适用的活动 ruleset 规则，包括仓库和组织层面的规则；不返回 `evaluate` 或 `disabled` 的 rulesets。它与 classic protection 并存，不能据 rulesets 结果为空就宣称不存在任何分支保护。来源：[GitHub 分支规则接口](https://docs.github.com/en/rest/repos/rules#get-rules-for-a-branch)。

第二轮 Q185 已确认只读同步现有保护分支，与本地保护列表取并集；成功刷新后才替换远端缓存，失败保留上次结果并标记可能过期，不修改 GitHub 上的规则。账号范围与认证方式仍待 Q183、Q184 确认。

## 原件与限制

上述三个官方页面的 Markdown 正文、URL、响应状态和 SHA256 保存在 `/private/tmp/revia-m7-planning-fslVoN9y/` 的 `github-oauth-authorizing`、`github-branch-list`、`github-rules` 文件组中。

独立事实代理 `/root/m7_github_auth_facts` 还核对了 PAT、组织限制、PR 权限与 Enterprise 差异。GrokSearch 搜索超时、抓取未返回，内置网页工具不可用，最后使用直接读取的 GitHub 官方公开 Markdown 正文；本文件中的三个关键页面也已由主代理直接读取。未做真实账号授权或 API 写入，功能验收仍待实施阶段。

本轮 Rebased 核对的网页工具原件在 `/private/tmp/revia-m7-round2-_ekzr32j/`：仓库与发布页、IntelliJ 说明，以及 Rebased 专属帮助页 404、两个猜测源码路径 404 和 GitHub API 目录查询 403 的原始结果均保留。当前客户端无 GrokSearch 入口，改用可用网页工具读取上述原页。

本机静态证据在 `/private/tmp/revia-m7-rebased-auth.W9rayw/`：`metadata.json` 记录版本、来源和 SHA256，`class-method-references.json` 记录方法引用与字节偏移，`raw/` 保存安装资源原件。主代理核对了 27 份资源原件与 3 个来源 JAR 的散列及关键调用链；未读取账号配置、钥匙串或用户凭据。
