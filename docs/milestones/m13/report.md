# M13 里程碑报告：UI 全面重构与历史遗留清理

更新日期：2026-11-21。状态：已完成实施、功能审查、历史遗留清理与自动化验证，并压成单提交合入本地 `main`（未推送远程）。

## 1. 目标与已确认决策（Q234–Q245）

- **告别 Liquid Glass 与 Rebased 一比一布局约束**（[ADR-0011](../../adr/0011-rebuild-like-rebased-not-fork.md)、[ADR-0016](../../adr/0016-decoupled-design-system-and-solid-surfaces.md)）：采用克制、清晰的实色现代桌面设计系统（浅色外壳 `#F5F6F8 / #FFFFFF`，深色外壳 `#141518 / #1A1C20`，钴蓝强调色 `#2563EB / #3B82F6`，40px 顶栏 / 36px 页签栏 / 24px 状态栏 / 26px 列表行高），保留全部已实现 Git 功能与安全边界。
- **外壳与代码颜色主题解耦**：外壳完全由 Revia 浅/深色 Design Tokens 驱动，新增**外观模式**（`appearanceMode`: `system | light | dark`，默认 `system`）；代码 diff、三方合并与 blame 表面继续由 4 套内置及用户导入的 VS Code 颜色主题驱动。
- **高保真视觉稿先行**：浅/深色交互式设计稿见 [revia-ui-v1.html](./mockups/revia-ui-v1.html)，经用户确认定稿后按模块落地。
- **取消双 agent 并行与强制 worktree**（Q245）：后续默认在当前仓库切出本地分支实施并压成单提交合入 `main`。

## 2. 核心重构内容

- **窗口壳与布局重整**：
  - 移除单按钮 `ActivityBar`，侧栏开关移入 40px 顶栏左缘；顶栏瘦身为 `[侧栏开关] [仓库 / Worktree / 分支] ... [更新 / 推送] [PR / 终端] [设置（带新版本提示点）]`；克隆归入仓库下拉底部与欢迎页，诊断与应用更新直达入口归入设置页。
  - 新增 `src/components/app/SidebarShell.tsx`，统一承载「提交 / Stash / 回收区」三段切换、侧栏宽度拖拽与改动树/提交区上下分隔拖拽。
  - 重绘 `ChangesTree`：26px 行高、中性文件名（删除文件加删除线）、行尾等宽单字母状态（`M/A/D/R/?/!/T`）悬停平滑切换为行内快捷操作。
  - 重绘 `EditorTabs`：36px 页签栏（常驻「提交日志」+ 提交 diff / 工作区 diff / 三方合并 + GitHub PR 主区页签）与单条 38px `.diff-context-bar`（三方合并页签自动隐藏）。
  - 新增 `src/components/app/BottomDrawer.tsx`，统一承载「内置终端」与「Git 命令日志」，切换页签时保留终端挂载且随外观模式实时切色。
  - 重绘 `SettingsDialog`：居中两栏卡片模态窗（上限 `920×640`），从 17 个碎片页合并为 3 组 12 页，统一 `.setting-card-list` / `.setting-row` / `.setting-switch` / `.kbd-cap` 键帽样式，并在「应用更新」页提供直达 `UpdatePanel` 入口。
- **前端样式与图标体系**：
  - 原 3,000+ 行单文件 `src/index.css` 拆分为 `src/styles/{tokens,shell,workspace,diff,overlays}.css`。
  - 界面图标全面迁移至 `lucide-react`（1.65px，`src/components/ui/app-icon.tsx` 导出 `AppIcon`），文件类型图标继续保留 Material Icon Theme。
- **Rust 设置持久化**：
  - `src-tauri/src/settings.rs` 新增 `AppearanceMode` 枚举并默认回退 `System`（兼容旧版 `settings.json`），新增单元测试覆盖持久化与旧配置回退。
  - `src-tauri/tauri.conf.json` 更新 macOS 红绿灯坐标适配 40px 顶栏；`src-tauri/tauri.windows.conf.json` 恢复 `transparent: false` 实色窗口。

## 3. 历史遗留彻底清理

- **玻璃材质与旧平台代码**：删除 `src-tauri/src/platform/{macos_glass,window_glass}.rs`、`src/components/m7/glass/*`、`src/features/m7/glass/*`、`src/types/bindings/Glass*.ts`、`src/hooks/useGlassPrefs.ts`，移除 `m7_glass_support` / `m7_glass_layout` IPC、`Cargo.toml` 的 `NSGlassEffectView` 特性、`platform.ts` 的玻璃辅助函数以及 `App.tsx` / `perfRecorder.ts` 中的旧玻璃与旧布局 DOM 探测。
- **旧版 CSS 兼容层与样板组件**：
  - `tools/gen-themes.mjs` 与 `src/themes/themes.css` 剥离全部旧 VS Code 外壳变量，仅保留代码/diff/语法色键。
  - `src/styles/tokens.css`、`src/index.css`、`src/styles/shell.css` 移除所有旧 VS Code 外壳别名与 `.toolbar-widget`；全站 27 个 `.tsx` 组件统一迁移至 M13 语义令牌与 `.revia-btn` / `.dialog-button`。
  - 重命名 `codicon.tsx` → `app-icon.tsx` 并移除 `Codicon` 别名；移除 `TerminalPanel` / `CommandLogPanel` 的旧版非嵌入式兼容分支；删除 9 个零引用 shadcn 样板组件、`src/lib/utils.ts`、`components.json`、`tools/extract-rebased-theme.py` 及 4 个无用 npm 依赖（`class-variance-authority`、`cn`、`shadcn`、`tw-animate-css`）；清理 14 个 M13 已淘汰的旧 UI 翻译键。
- **Git 分支与 worktree 收尾**：压成单提交合入本地 `main` 后，清理失效的 `m12` worktree 记录并删除全部已合入的历史本地分支（`m13`、`m12`、`m11`、`m10`、`codex/selection-design`、`release-0.0.4-fix`、`release-0.0.4-windows-fix`），本地仅保留 `main`。

## 4. 验证记录

- `pnpm build`（`tsc -b && vite build`）：通过。
- `pnpm lint`（`oxlint`）：通过，0 error。
- `pnpm i18n:check` & `pnpm i18n:extract`：通过，0 error、0 硬编码中文。
- 32 个独立前端 Node 测试脚本（`node --test tools/test-*.mjs`）：**569 / 569 通过**。
- Rust `cargo test --lib settings`：**13 / 13 通过**；全库 `cargo test --lib`：**433 / 447 通过**（其余 14 项为 `ai::*` 依赖本地 `127.0.0.1` 回环 HTTP 的测试，沙箱禁网下不可达）。
- Rust `cargo build`（嵌入最新 `dist/` 的 debug 二进制）：通过。

### 未验证范围

- 依赖外部 fixture 环境变量的 3 个脚本（`test-edit-session.mjs`、`test-m6-file-history.mjs`、`test-m6-i18n.mjs`）及 14 个依赖本地回环网络的 Rust AI 单元测试未在沙箱外执行。
- macOS 与 Windows 真机桌面的手工交互与视觉复核待用户实测。
