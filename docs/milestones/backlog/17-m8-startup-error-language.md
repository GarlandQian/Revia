# M8 启动前原生错误按语言呈现

Status: ready-for-agent
日期：2026-11-02
来源：M8 review01 Standards S2，P3，按 Q174 非阻断记录。

便携数据目录不可写或缺少 WebView2 时，distribution::startup_failure_message 直接显示中文 error.message 并拼接中英双语指引。英文语言资源不会参与这条启动前原生路径。后续按已保存语言/系统语言格式化同一错误语义，保留无 WebView2 时也能看见提示的原生入口；从真实启动错误入口验证语言，不能只测试资源存在。

本项不阻塞 M8 正式返修与 Windows 桌面验收；尚未修复。
