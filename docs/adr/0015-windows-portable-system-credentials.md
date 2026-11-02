# Windows 便携版的 GitHub 凭据使用系统凭据库

2026-11-02，用户明确要求 Windows 安装版和便携版都使用 Windows Credential Manager（Q204）。这为 Q58 的便携数据隔离增加一个窄范围例外：GitHub 访问令牌、刷新令牌及其轮换信息保存在当前 Windows 用户的系统凭据库，其余 Revia 持久数据和 WebView2 用户数据仍在 exe 旁的 `data`。

与仅会话保存或另加口令加密相比，这沿用系统凭据保护并保留重启后的登录。代价是 GitHub 登录不随便携目录迁移，换 Windows 用户或电脑需重新登录；不能把系统凭据同步到普通配置、明文文件、诊断或日志。该例外不改变 Git 的 credential helper、ssh-agent 或提交作者。
