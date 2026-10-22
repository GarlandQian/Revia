# 09：平台边界与 Windows 编译检查

**What to build:** 平台相关行为集中管理，macOS 现有应用保持可用，并可在 Windows x64 的 CI 环境实际编译正式工程，提前暴露平台耦合。

**Blocked by:** 01

**Status:** ready-for-agent

依据：[M2 规格](../spec.md)实现决策 8、D07；Q55、Q62、Q97，ADR-0007。

- [x] 登录 shell、窗口外观、快捷键修饰键等平台逻辑集中，核心 Git／回收／审阅代码不散布 macOS 假设。
- [x] macOS 现有构建与行为不退化；不提前实现 Windows 安装版、便携版、完整外壳或 hooks 体验。
- [x] 提供 main 与 m2 的仅编译 CI，覆盖既定 macOS Apple Silicon 与 Windows x64 编译方向；不生成发布安装包或触发发布。
- [ ] 由规划方按 Q97 正常推送 m2 至现有 origin 后，取得 Windows x64 实际编译结果与对应提交；不以只编写工作流或 Mac 交叉编译冒充通过。
- [ ] 不推 main、不强推、不打标签、不创建 PR、不发布；权限或运行环境问题如实记录，未通过时不能勾选验收。
- [ ] 记录本任务编译成功对应的代码版本；最终集成后的再次编译由任务 10、11 验收，早期成功不替代最终验收。

## Comments

### 2026-10-23 / Pi 实现

1. **集中平台逻辑：**
   - **Rust 侧（`src-tauri/`）**：
     - `src-tauri/src/shell.rs`：将登录 shell 环境解析彻底解耦并集中到平台模块。Unix 平台保留通过 `/bin/zsh -l -i -c` 运行转储脚本与 5 秒超时的完整逻辑；Windows 平台（`#[cfg(windows)]`）合理降级为使用进程当前环境变量（`std::env::vars()`），不调用外部 shell，为 Windows CI 与本地编译提供稳健支持；
     - `src-tauri/src/shell.rs`：`split_path` 改用标准库 `std::env::split_paths`，跨平台兼容 Unix `:` 与 Windows `;`，并兼容 Windows 大小写不敏感的 `PATH`/`Path` 键名；`path_string` 使用 `std::env::join_paths`；
     - `src-tauri/src/shell.rs`：`which_in` 在 Windows 平台上支持附带 `.exe`/`.cmd`/`.bat` 扩展名的自动寻找（定位 `git` 时能正确找到 `git.exe`）；
     - `src-tauri/src/shell.rs`：`home_dir()` 支持 Unix 读取 `HOME` 与 Windows 读取 `USERPROFILE`/`HOMEDRIVE+HOMEPATH`；
     - `src-tauri/src/lib.rs`：长测窗口辅助方法 `window.set_visible_on_all_workspaces(true)` 加上 `#[cfg(target_os = "macos")]` 保护；
     - `src-tauri/tests/file_staging.rs`：顶层 `OsStrExt` 与 POSIX 专有的 `t03_stage_and_unstage_mode_change` 加上 `#[cfg(unix)]` 标注；`open_session` 使用 `std::env::split_paths` 从宿主环境动态获取 PATH，确保 Windows 下集成测试能定位系统 Git；
     - 核心代码（`git/diff.rs`, `git/status.rs`, `git/mod.rs`, `git/highlight.rs`, `session.rs`）严格保持平台无关。
   - **前端侧（`src/`）**：
     - `src/lib/platform.ts`：集中管理所有平台判断。`isMac`（环境探测）、`isPrimaryModifier`（macOS ⌘ 与 Windows/Linux Ctrl 严格互斥）、`isAltModifier`、`isStageFileShortcut`（物理键 `KeyA` 避免 Option 产生字符死键）、`isTextInputOrDialog`（输入控件与弹窗让出快捷键避免穿透）；
     - 所有界面与业务组件均引用 `platform.ts`，无任何散落的平台判断或 macOS 硬编码假设。

2. **各平台预期行为：**
   - **macOS**：保持首版设计全量特性。通过交互式登录 zsh 解析完整 PATH 与 worktree 工具链（支持 Homebrew、fnm、nvm、husky 等），窗口使用原生 Overlay 标题栏，快捷键使用 ⌘ 与 ⌥；
   - **Windows**：作为 M5 前期编译检查与合理降级。环境变量直接使用父进程环境，支持 Windows 路径与可执行后缀，快捷键使用 Ctrl 与 Alt；明确不提前实现 Windows 安装版、便携版、完整外壳或 hooks 体验（留待 M5）。

3. **仅编译 CI 工作流（`.github/workflows/compile-check.yml`）：**
   - **名称**：`Compile Check`；
   - **触发条件**：推送到 `main`、`m2` 分支，以及 `workflow_dispatch` 手动触发。不包含任何其他里程碑分支；
   - **作业设置**：
     - `compile-macos`：在 `macos-14`（原生 Apple Silicon M1 arm64 架构）上运行，安装 Node 22、pnpm 10、Rust stable，执行 `pnpm install --frozen-lockfile`、`pnpm exec tsc -b`、`pnpm lint`、`pnpm build`、`cargo test`、`cargo build --release`；
     - `compile-windows`：在 `windows-latest`（Windows x64 架构）上运行，安装 Node 22、pnpm 10、Rust stable (msvc)，执行 `pnpm install --frozen-lockfile`、`pnpm exec tsc -b`、`pnpm lint`、`pnpm build`、`cargo test`、`cargo build --release`；
   - **严格约束**：仅做编译和测试检查，不生成安装包（不执行 `--bundles`），不上传产物，不触发发布，不需要代码签名密钥。

4. **本机验证检查结果：**
   - **工作流语法校验**：YAML 格式、缩进与结构检查通过；
   - **前端检查**：`pnpm exec tsc -b`、`pnpm lint`、`pnpm build` 全部通过；
   - **Rust 测试**：`cargo test --manifest-path src-tauri/Cargo.toml` **88 个测试全部通过**（67 个单元测试 + 18 个 file_staging 集成测试 + 3 个 session_flow 集成测试）；
   - **自动化回归工具**：44 项基线与诊断测试全部通过（`test_bench_long_running.py`, `test-ipc-diagnostics.mjs`, `test-visible-highlight-lines.mjs`）；
   - **macOS 打包**：`pnpm tauri build --bundles app` 打包成功；
   - **D06 性能回归实测**（Release 构建，可见窗口，60 rAF/s）：
     - 并排视图：首屏 `fromListChangesMs`: **85ms**（<150ms ✅），已缓存跳转 `warmAppearMs`: **8ms**（≤16.7ms ✅），滚动 60 屏未覆盖帧: **0**；
     - 统一视图：首屏 `fromListChangesMs`: **83ms**（<150ms ✅），已缓存跳转 `warmAppearMs`: **6ms**（≤16.7ms ✅），滚动 60 屏未覆盖帧: **0**；
   - **冒烟检查**：`REVIA_SMOKE=1` 验证 7 个文件全开成功，视图切换行锚点 3322 完好保留。

5. **遗留问题与交接：**
   - 本机为 macOS Apple Silicon 环境，无 Windows 交叉编译工具链，不以本地交叉检查冒充通过；
   - 待规划方按 Q97 正常推送 `m2` 分支到 GitHub 现有 origin 后，在 GitHub Actions 取得实际 Windows x64 编译结果与对应 commit，并在验收项中确认。


### 2026-10-23 / Claude 审查与首次 CI

代码审查通过：登录 shell 环境解析集中在 `shell.rs`，macOS 与 Windows 分别实现，Windows 无登录 shell 时使用进程自身环境；工作流 `Compile Check` 在推送 main、m2 和手动触发时，于 macos-14（Apple Silicon）与 windows-latest（x64）上执行前端类型检查、lint、构建以及 `cargo test`、`cargo build --release`，不打包、不发布。`pnpm-lock.yaml` 为 9.0 格式，与工作流的 pnpm 10 兼容。

按 Q97，规划方于 2026-10-23 把 `m2`（`2a34c9b`）正常推送到 origin（未推 main、未强推、未打标签、未建 PR），运行 [Compile Check #39847227656](https://github.com/GarlandQian/Revia/actions/runs/39847227656)：

- macOS Apple Silicon：通过（6 分 56 秒）。
- Windows x64：**失败**。前端各步骤通过；整个 crate 与测试在 Windows 上编译成功，`cargo test` 65 项通过、1 项失败：`shell::tests::split_path_drops_empty_entries` 在测试里写死了 `:` 分隔的 PATH（`"/a::/b:"`），Windows 的分隔符是 `;`。这是测试本身的平台假设，不是产品代码问题。

**必须修改**：该测试按平台分隔符构造 PATH（例如用 `std::env::join_paths` 或按 `cfg` 选择分隔符），本机通过后由规划方再次推送并取得 Windows x64 通过的结果。另有一条 GitHub 提示：所用 action 的 Node.js 20 运行时已弃用，可在修改时把 `actions/checkout`、`actions/setup-node`、`actions/cache`、`pnpm/action-setup` 升到当前版本（不阻塞）。

“取得 Windows x64 实际编译结果”一项暂不勾选。

