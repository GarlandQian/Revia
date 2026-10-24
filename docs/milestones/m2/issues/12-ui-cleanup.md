# 12：界面清理

**What to build:** 平时使用的界面不再出现测量和调试用的元素，工具栏只保留日常操作；测量、冒烟与长测工具在测量模式下照常可用。

**Blocked by:** None

**Status:** ready-for-human

依据：[M2 规格](../spec.md)实现决策 9、D06、D08；Q39、Q99–Q102、Q104。

- [ ] 测量面板（测量记录、导出 JSON、清空）只在自动测量、长测等测量模式下显示，平时不出现。
- [ ] 工具栏不显示 Git 版本和文件数；找不到 Git、版本过低等环境问题仍按原规则在窗口顶部全局提示条显示。
- [ ] 工具栏去掉 A−／A+；⌘+／⌘− 调整字号照常可用并被记住。
- [ ] 只做以上三项，不改工具栏其他按钮、侧栏、文件列表、提示条和 diff 区的外观（留给任务 13），不新增功能。
- [ ] 自动测量、冒烟检查与长测在测量模式下照常运行并产出结果；D06 不退化。
- [ ] 类型检查、lint、build 与完整 Rust 测试通过；真实桌面截图确认普通启动时三项均已消失。

## Comments

### 2026-10-24 / Pi 实现：任务12 界面清理

只做已确认的三项；未改工具栏其他按钮、侧栏、文件列表、提示条与 diff 布局，未提前做 13 的图标/美化/半透明。

#### 1. 测量面板只在测量模式显示（Q99、D08）

- 后端 `MeasureConfig` 新增 `measurement_mode`：`REVIA_MEASURE_LOG` / `REVIA_MEASURE` / `REVIA_SMOKE` / `REVIA_SOAK_SECONDS` **任一**成立即为 true。
  普通隔离的 acceptance 实例只设 `REVIA_ACCEPTANCE(_DIR)`，不设这些变量，因此仍是普通模式、不显示测量面板。
- 前端新增 `measurementMode` 状态，`<MeasurePanel>` 只在它为 true 时渲染。
- **只控制界面**：`loggingEnabled`、`setMeasurements`、`runAutoMeasurement` / `runSmoke` / `runSoak` 与所有 `write_measurement` IPC 流程都保持原样，未因隐藏 UI 关闭任何采集。
- 顺带把原先只写不读的 `status` 接到测量面板标题栏（测量/长测诊断仍在写它），因此普通界面看不到它、测量模式下仍可见。

#### 2. 工具栏不再显示 Git 版本与文件数（Q100）

删掉工具栏末尾的 `gitVersion · status` 文本与对应的 `environment` / `status` props。
环境问题仍按原规则：`openWorktree` 里 `sessionInfo.checks` 失败时写入 `kind: "environment"` 的 App 错误 → 顶部 `ErrorBanner`，本轮未改动这条路径。

#### 3. 去掉 A−／A+（Q101）

删除工具栏两个字号按钮及 `onFontSize` prop。`⌘+` / `⌘−` 快捷键与 `changeFontSize` → `ipc.setDiffFontSize` 的持久化未改动，字号仍被记住。

#### 实际执行的检查（改完后集中一次）

日志目录（实际路径）：`/private/tmp/revia-task12-zyZuUB`

- 基线：`ff2fc640d3a6fa771d1d6b8a5514a3d7471ab347`
- 检查前源码 SHA-256（检查后复核未变）：

  - `c3859baca864f55ed84564d8d3ab91e1d5ecdb6ec8da27e1b595e2afe3e3542f  src/App.tsx`
  - `fe2dd84edeb474eb143a01136ab842e739125f9e3ff227d082352eebd3da4a7b  src/lib/ipc.ts`
  - `d823d89b04993c9684980863900a30898c015cabfcb99b9168cf3072b4fbf407  src-tauri/src/commands.rs`

| 命令 | 退出码 | 原始输出 |
| --- | --- | --- |
| `pnpm exec tsc -b` | 0 | `tsc.out` / `tsc.err` / `tsc.exit` |
| `pnpm lint` | 0 | `lint.out` / `lint.err` / `lint.exit` |
| `pnpm build` | 0 | `build.out` / `build.err` / `build.exit` |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 0 | `cargo-test.out` / `cargo-test.err` / `cargo-test.exit` |

Rust 合计 **247 项**全部通过（lib 99、`discard_recovery` 71 含 1 ignored 桌面夹具、`file_staging` 20、`hunk_staging` 24、`refresh_isolation` 15、`review_marks` 15、`session_flow` 3）。本轮只改 UI 与一个配置字段，未新增 Rust 回归。

#### 未验证（必须保留）

- **真实桌面未验证**：普通启动时测量面板、Git 版本/文件数、A−／A+ 是否真的都不出现；测量模式下测量工具是否照常可见可用；字号记忆是否仍生效。
- **D06 未复测**：本轮未跑任何性能测量，不能声称未退化。
- 未构建 Release、未启动 GUI、未跑 D06/双平台 CI/长测。

### 2026-10-24 / 规划方：任务12首轮固定两轴复核

固定 `ff2fc640d3a6fa771d1d6b8a5514a3d7471ab347...48482656b7bf46223e371e44d1d1dddb6f60903b`，唯一实施提交4848265。

#### Standards

0项确定违规或缺陷，Fowler判断性建议0项。普通模式/测量模式分离，Git调用边界、全局环境错误、字号快捷键及持久记忆没有发现确定回归；不因低风险UI清理引入新的测试体系。

#### Spec

0项发现：(a)未见缺失/部分实现；(b)未见范围扩张；(c)未见确定行为错误。对照spec实现决策9和D08，测量面板仅受测量配置控制；普通启动及仅acceptance配置关闭，采集/冒烟/长测/写结果链未改。工具栏移除诊断信息和字号按钮，环境错误条、字号快捷键及记忆保留，没有提前实现13。

#### 检查与待验收

- [16份检查原件](../evidence/task12-review01/manifest.json)已归档，3份检查前源码SHA独立匹配4848265；tsc/lint/build及完整Rust退出0（247通过、1桌面夹具ignored）。只是固定版本实施检查，不代替动态验收。
- 保持 `ready-for-human`，验收项暂未勾：待07所有返修复核通过，统一固定新版Release与最新双平台CI，再由规划方核普通启动、测量/冒烟/长测产出、字号记忆、环境错误和D06。未Release/GUI/D06/新Windows CI/长测，不能声称这些已通过或将任务done。
