# M7 GitHub：访谈前事实

核对日期：2026-10-30。仓库基点 `8b9548774054c64921e2a1f57214b711f51c5f72`；调查只读，未访问账号、凭据或改动远端。下述事实不代替账号、登录和保护同步的产品决定。

## 登录

GitHub 支持 device flow：应用取得验证码，用户在浏览器打开 GitHub 授权页输入验证码，应用取得用户访问令牌。这一流程要求先注册应用获得 `client_id`，并在应用设置中启用 device flow；其令牌交换不需要 `client_secret`。普通 OAuth 网页授权码交换仍列 `client_secret` 为必填，不能因为支持 PKCE 就假定可以省略。来源：[GitHub OAuth 授权文档](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)。

据此，浏览器验证码授权可作为不把 client secret 放进安装包的桌面登录方案；这是依据接口要求得出的实现判断。是否采用该方案、采用 OAuth App 还是 GitHub App、是否提供手动令牌入口，尚未由用户确认。GitHub API 登录与 Q153 的 Git 传输凭据规则分别处理。

## 分支保护

`List branches` 的 `protected=true` 会返回受 classic branch protection 或 rulesets 保护的分支。这可以支撑同步“现有分支是否受保护”，但不等于复制完整保护策略、通配模式或旁路权限。来源：[GitHub 列分支接口](https://docs.github.com/en/rest/branches/branches#list-branches)。

`Get rules for a branch` 可以查询尚未创建的分支名，返回适用的活动 ruleset 规则，包括仓库和组织层面的规则；不返回 `evaluate` 或 `disabled` 的 rulesets。它与 classic protection 并存，不能据 rulesets 结果为空就宣称不存在任何分支保护。来源：[GitHub 分支规则接口](https://docs.github.com/en/rest/repos/rules#get-rules-for-a-branch)。

候选产品方案是只读同步现有保护分支，与本地保护列表取并集；成功刷新后才替换远端缓存，失败保留上次结果并标记可能过期。这仍是建议，须在下一轮访谈确认；不会据此自动修改 GitHub 上的规则。

## 原件与限制

上述三个官方页面的 Markdown 正文、URL、响应状态和 SHA256 保存在 `/private/tmp/revia-m7-planning-fslVoN9y/` 的 `github-oauth-authorizing`、`github-branch-list`、`github-rules` 文件组中。

独立事实代理 `/root/m7_github_auth_facts` 还核对了 PAT、组织限制、PR 权限与 Enterprise 差异。GrokSearch 搜索超时、抓取未返回，内置网页工具不可用，最后使用直接读取的 GitHub 官方公开 Markdown 正文；本文件中的三个关键页面也已由主代理直接读取。未做真实账号授权或 API 写入，功能验收仍待实施阶段。
