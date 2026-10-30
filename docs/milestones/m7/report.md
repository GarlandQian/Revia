# M7 实施与验收报告

Status: ready-for-review

实施方：Paseo / Pi（同会话续接；渠道中断与续接记录见文末）。规划方 / 验收方：Codex。
分支 `m7`（worktree `/Volumes/T7 Shield/project/Revia-m7`），规划基点 `581d1527651e981018f2b0830d994bca49580254`。
独占证据根：`/private/tmp/revia-m7-batch-g5scyw0r`（日志、冒烟 JSON、fixture 均在此，不覆盖）。

## 实施摘要

任务 01–12 的实现已全部落在 m7 工作区（139 个文件，未合 main）。核心交付：

- **01 玻璃外观**：React Bits 三组件原样复用（`src/components/m7/glass/`，含 LICENSE.md）；参数统一收口 `src/features/m7/glass/material.ts`（第四版 mount.tsx 数值）。`GlassLayer` 提供整窗 / 浮层材质层；`GlassButton` 统一按钮材质与状态。用户"不是全 liquid glass"的纠正及处理过程见任务 01 Comments。
- **02 主题 / 快捷键**：VS Code JSONC 主题导入（`m7/theme.rs`，include 链、非法值拒收）、快捷键自定义（`src/lib/shortcuts.ts` + `settings.rs` 规范化 / 冲突替换）、设置页新增"扩展"分类。
- **03 工作区编辑**：`m7/edit.rs`（2MB 上限、符号链接 / 二进制 / 子模块 / 未合并只读、SHA256 写前核验、CRLF/BOM/权限保留、草稿跨重启）；前端 `EditableWorkspace`。
- **04 终端**：`m7/terminal.rs` portable-pty 登录 shell，输入来自前端字节，200KB 回滚，页签跨重启恢复（不恢复进程、不重放输入）；面板 `TerminalPanel`。
- **05 克隆**：`m7/clone.rs` 独立 argv、进度流、取消不删残留目录；`CloneDialog`。
- **06–09 GitHub**：设备码 / Token 登录、钥匙串存储（测试走 REVIA_GITHUB_TOKEN_STORE 适配）、PR 列表 / 详情 / 检出（refs/revia/pr/N）、审阅草稿、合并回执、保护分支 24h 缓存；错误全部结构化 code。
- **10 诊断**：白名单导出 + 脱敏（HOME/用户名/URL 凭据/Token），预览与导出同源；`DiagnosticsDialog`。
- **11 应用更新**：tauri-plugin-updater，未配置时给稳定原因键（NoPlugin/NoPubkey/NoEndpoints），回执落盘，安装前 flush 草稿；`UpdatePanel` / `UpdateBanner`。
- **12 打包**：`.github/workflows/release.yml`（仅 v* 标签、draft release、minisign 公钥注入）、`compile-check.yml` 保留 Windows 编译检查、`tools/check-release-version.mjs`、`tools/write-updater-config.mjs`、`docs/release.md`。

## 材质覆盖（任务 01，按区域）

| 区域 | 材质 | 证据 |
| --- | --- | --- |
| 整窗外壳 | `GlassLayer--window`（GlassSurface，折射开启，第四版参数）+ `glass-host` 渐变底 | `smoke-final2.json` ui.glass.windowLayer=true |
| 顶栏 | 玻璃上渐变半透明层（`--glass-titlebar-tint`），不再是独立 blur 层 | 同上 panelLayers.toolbar=true |
| 侧栏 / 活动栏 / 状态栏 / 页签行 | 半透明层 + `--glass-line` 分隔（`window-status` 类并入状态栏） | 同上 sidebar/status=true |
| 浮层（菜单 / commit 弹层） | `GlassLayer--popover`（半径 7、折射开启） | `src/components/ui/popover.tsx` |
| 对话框（通用 / 设置 / 克隆 / 诊断 / 更新）与通知 | `glass-isolate` + `GlassLayer--popover` | 对应组件 diff |
| 主提交按钮 | SpecularButton `variant=primary`，tint #c3d2dd/.16（dark）、#405b71/.12（light） | `CommitComposer.tsx` |
| 工具按钮 / 图标按钮 / 对话框按钮 | `GlassButton` + toolbar-widget / icon-action / dialog-button 统一 CSS 材质状态 | `src/index.css` 玻璃段 |
| 顶栏工具簇 | GlassIcons（5 入口，主题同步） | `smoke-final2.json` tools=5 |
| 代码 / diff | 保持不透明 `--editor-background`，行号 / 增删 / 光标不受材质影响 | index.css 明确注释 + smoke sbs.canvasPainted=true |

## 真实验证 / 降级

- `cargo test`：**768 通过 0 失败**（m6_flows 1 例 15s 超时为负载抖动，单独重跑通过；`logs/m6-retest.exit=0`）。含新增 `tests/m7_edit.rs`（5 例：真实 Git 保存 / 外部修改拒写 / CRLF·BOM·权限 / 只读原因 / 草稿重启恢复）与 `tests/m7_terminal.rs`（2 例：真实 PTY 读写·resize·关闭·错误码、双 worktree 独立）。
- `pnpm exec tsc -b` 0 错；`pnpm lint` exit=0；`pnpm build` 成功；`x18n check` 0 错 65 预存警告。
- 隔离冒烟（独占 HOME/appdata/fixture，release 二进制 sha256 `7599baa2acc6f9f659092d2d4c5e0cb2e8fe6c79c3611c2470f9df2054a41220`）：`logs/smoke-final2.json` ok=true、titlebar y=0、并排色带 canvasPainted=true、视图切换锚点保持、M7 IPC（主题/更新状态/终端/诊断/可编辑文件）全部真实返回。
- **强折射未验**：WKWebView（macOS 真机）上 SVG/WebGL 折射路径的实际渲染证据未取得；`refractionEnabled=1` 只证明组件走了非 fallback 路径，不等于桌面视觉通过。留任务 14 桌面核对。减少透明度 / 高对比度 / 减少动效的降级为 CSS 媒体查询 + `useFlatGlass`，浏览器预览已按 adapt.css 语义实现，真机降级未逐一实测。
- 冒烟布局回归（材质层占 grid 轨道把顶栏挤下）在 smoke-glass4..7 原件中保留完整证据，修复为 `.glass-layer.glass-surface { position:absolute !important }`，vendor 非 layered 样式覆盖问题已写入任务 01 Comments。

## 逐任务状态

| 任务 | 状态 | 说明 |
| --- | --- | --- |
| 01 玻璃外观 | **done** | 整窗 / 浮层 / 按钮统一材质；强折射真机证据留 14 |
| 02 主题 / 快捷键 | **done** | 单元测试覆盖解析 / 冲突 / 规范化；真机主题切换留 14 |
| 03 工作区编辑 | **done** | 新增真实 Git 集成测试 5 例；真机交互留 14 |
| 04 终端 | **done** | 新增真实 PTY 集成测试 2 例；"写入期间显示忙碌"以现有 `has_running` + 列表运行态呈现，未做独立横幅，如审查认为不足返修 |
| 05 克隆 | **done** | 单元测试覆盖注入 / 非空目录 / 进度解析；local remote 全链路（成功 / 失败 / 取消 / 子模块）仅部分由单元测试覆盖，完整 e2e 未跑，如实标注 |
| 06 GitHub 登录 | **done** | 设备码状态机 / Token 校验有测试；真实账号 / 钥匙串留 14 |
| 07 PR 读 / 检出 | **done** | 分支名校验、head 核验、检出流程实现；HTTP fixture 级 e2e 未跑 |
| 08 PR 写 | **done** | 合并确认 / 回执 / 草稿实现并有单元测试；HTTP fixture 级 e2e 未跑 |
| 09 保护分支 | **done** | 缓存 / 404 / 权限不足按"未知受保护"处理；分页完整性（完整分页才替换缓存）按单分支 protection API 实现，非分支列表分页，如审查口径不同请指出 |
| 10 诊断 | **done** | 脱敏 / 白名单 / 预览一致性有测试；7 天过期策略在 store 层实现 |
| 11 应用更新 | **done** | 状态机 / 回执 / 未配置原因有测试；实际签名更新与首启下载留 14 / 外部接线 |
| 12 打包 | **done** | 工作流 YAML 校验通过、版本核对脚本 stable/prerelease 实测（`logs/release-version-*.log`）、updater overlay 生成实测；真实 CI 运行留最终验收 |
| 13 更改列表 | **needs-info（不变）** | Rebased 1.1.22 终端已有暂存内容实测仍未取得，不猜 index 语义 |
| 14 整包验收 | **blocked（不变）** | 依赖 13 及真实桌面（Codex）；本报告不构成桌面通过 |

## 已知缺口与如实声明

1. 任务 05/07/08 的受控 HTTP fixture e2e（请求目标 / 次数 / 版本变化断言）未建；现有覆盖为单元级。若 Codex 认为达不到验收项口径，请在 Comments 标注，返修窗口内补。
2. 任务 09 的"完整分页"按 GitHub `GET /branches/{branch}/protection` 单分支语义实现（无分页概念）；规格若指 branch-list 分页需返修确认。
3. 任务 04"写入期间显示所属 worktree 忙碌"当前通过终端列表运行态 + worktree 写锁（Git 写排队）体现，没有独立忙碌横幅。
4. minisign 签名验证由 tauri-plugin-updater 内部执行（配置 pubkey 后强制验签）；`minisign-verify` 依赖当前未直接调用，好坏签名对拍测试未建。
5. 强折射（SVG 滤镜在 WKWebView 的实际表现）、Windows WebView2 外观、真实 CI、真实账号 / 钥匙串、D06 长测：均未执行，留任务 14 / 外部接线。

## 会话与渠道记录

- 会话：Paseo `8a5673e4-a7bb-4361-8307-6f523c81ee31` / Pi `01a18ce0-67c3-74fa-9c20-585fd434e10e`。
- 2026-10-30 GLM-5.3-Flash 渠道三次 502，原件存批次 `flash-channel-error-kedbp795`；随后同会话续接（setAgentModel 同模型），formalRepairUsed=false。
- 用户"全 liquid glass"纠正原文见 `/private/tmp/revia-m7-batch-g5scyw0r/user-glass-scope-feedback.md`，已转录任务 01 Comments。

## 审查与验收

由 Codex 执行首轮全范围两轴审查（对照 spec.md 与 issues/），随后固定 Release 真实桌面验收（任务 14，含任务 13 事实门槛）。实施方不自行宣告整包通过。
