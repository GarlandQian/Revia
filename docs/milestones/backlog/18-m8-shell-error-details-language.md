# M8 环境捕获失败详情的语言边界

Status: ready-for-agent
日期：2026-11-02
来源：M8 review02 Standards S4，P3，按 Q174 非阻断记录。

windows_env 中临时目录、诊断通道、进程启动和转储读取失败仍把中文阶段说明拼入 CaptureFailure.detail，再由 session 的已翻译错误提示直接呈现。应将阶段作为结构化错误语义，只保留原始 OS/profile 输出作为详情，保证英文界面没有 Revia 自行拼接的中文产品文案。沿 ADR-0013/x18n 管理文案，从真实失败入口核对英文结果。

本项不同于 backlog 17 的启动前原生消息框；不阻塞 M8 Windows 真机验收，尚未修复。
