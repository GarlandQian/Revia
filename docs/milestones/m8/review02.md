# M8 返修增量两轴审查

日期：2026-11-02
固定范围：`git diff 290ef9b...50ffce7`，代码 a3abb92，报告 50ffce7。
两轴独立、只读审查；未重审无变化的首轮实现。backlog 17 保持原非阻断结论。

## Standards

S3 · P2（S1/X2 残留）：windows_env.rs:49 只在 catch 中退出，但脚本仍设置 ErrorActionPreference=Continue。profile 中 Write-Error、模块加载失败等非终止错误会继续转储环境，Rust 接受成功结果并丢弃 stderr，用户仍可能拿到部分初始化环境且看不到回退。违反 Q205 的失败提示与回退要求。应把 profile 初始化错误纳入失败判定，并补真实 PowerShell 回归。该语义已核对 [Microsoft 文档](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_preference_variables?view=powershell-7.5#erroractionpreference)。

S4 · P3，建议记 backlog：windows_env.rs:179 等新增失败路径仍拼接“创建环境捕获临时目录失败”等中文产品文案，再由 session.rs:336 原样放进 detail。英文界面遇到临时目录、启动或环境转储失败时，详情仍夹中文。违反 ADR-0013“不能把原来的整段中文错误塞进参数”；应将失败阶段结构化，只把原始系统/profile 输出作为详情。此项是新增环境提示链路，未重开 backlog 17 的启动语言问题。

未改文件、未运行测试；实施方报告未视为独立验收证据。未发现其他实质规范违规或需报告的代码异味。

## Spec

X4 · P2 · X2 仍未覆盖 profile 的非终止错误。规格 spec.md:55：“失败、超时或策略阻止时显式标出回退及诊断”；repair01.md:11：“不得接受部分初始化环境”。windows_env.rs:44 新增的 catch 只处理终止错误，而 :33 仍设置 ErrorActionPreference=Continue。profile 中 Import-Module 加载失败或 Write-Error 等非终止错误会继续执行，最终转储部分环境并成功退出；shell.rs:333 随后固定 used_fallback=false、清空诊断。用户仍会看到环境检查成功，hooks 可能使用初始化不完整的 Node 环境。应识别 profile 错误并走结构化回退，同时保留独立原始详情。[Microsoft 的错误处理说明](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_preference_variables?view=powershell-7.5#erroractionpreference)确认 Continue 会显示非终止错误后继续执行。

X1 的类型错误已修复；X3 的覆盖目录选择及 WebView2 隔离、R1 的 stderr 有界读取、R2 的可取消确认均已接入。未发现其他确定的新问题或范围扩大，未重开 backlog 17。

运行 `node --test tools/test-update-lifecycle.mjs`：11/11 通过。未运行 Rust 全套、Windows 编译或真机验收；这些验证边界未作为缺陷。

## 规划方接管

Standards 2 项（P2 1、P3 1），Spec 1 项 P2；S3/X4 为同一问题。按 Q113 不再交 Pi 返修，Codex 将生产捕获脚本的 ErrorActionPreference 改为 Stop，使非终止 profile 错误进入既有 exit 13 → 结构化回退链路。

补 Windows 专属真实进程回归：以 -NoProfile 启动系统 PowerShell，只把本测试的临时 profile 放进 PROFILE 属性；该 profile 先改变环境再 Write-Error，断言退出码 13、原始错误详情保留、环境转储不存在。进程等待 15 秒有界，测试不读用户 profile、不改执行策略。当前 Mac 没有 PowerShell，故该用例留待 Windows 执行；本机仅运行可行的环境模块检查，不声称已实测 Windows。

S4 依 Q174 转 [backlog 18](../backlog/18-m8-shell-error-details-language.md)，非本轮阻断。S2/backlog 17 同样保持。Windows 编译与真机验收仍待取得相应环境，m8 保留独立分支，不合 main。
