# M8 实施报告：Windows x64 安装版与便携版

Status: ready-for-human
日期：2026-11-02
实施方：Pi（Paseo）
固定源码：`a3abb92`（正式返修；含主体实现 `a30b435`、`af869f9`），基线文档 `e7f4c25`，main `b9cf807`
证据根：`/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m8-evidence.Vij0gB`（首轮）
返修证据根：`/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m8-repair01.9VOhvU`

## 结论

任务 01–06 的代码、配置、工作流与本机可行的回归已完成；首轮审查发现的 X1、X2/S1、X3、
R1、R2 已由唯一一次正式返修（`a3abb92`）处理，详见
[repair01-report](./repair01-report.md)。任务 07 的本机部分已完成，Windows 真实桌面验收
尚未执行。按规格保持 **ready-for-human**，不合并 main，不推送、不打标签、不创建或公开 Release。

本机可以证明：Rust 类型检查与完整现有测试目标通过，前端类型、lint、构建、x18n 检查
与相关 Node 回归通过，CRLF / `.gitattributes` 的真实 Git 行为通过。不能证明：Windows
编译、安装/卸载/升级、便携启动、Credential Manager 真实登录、PowerShell profile 与
hooks、Mica / DPI / IME / WebView2 性能；这些都需要真实 Windows 环境（任务 07）。

## 实现摘要

### 01 安装版与便携版启动及数据隔离（`src-tauri/src/distribution.rs`、`lib.rs`）

- 分发形态由编译特性 `portable` 决定（`Cargo.toml`），不看 exe 名、目录标记或用户环境变量；
  改名、移动目录不改变模式。
- 便携版业务数据、缓存与 WebView2 用户数据都在 exe 旁的 `data` / `data/webview2`；
  安装版继续使用 Tauri 解析的当前用户标准目录，NSIS 配置 `installMode: currentUser`。
- Windows 主窗口改为在 `setup` 中显式创建（Windows 配置 `create: false`），WebView2 数据
  目录在**第一个窗口创建前**交给 Tauri；macOS 仍由 Tauri 按配置建窗，外壳行为不变。
- 便携数据目录先 `create_dir_all` 再真实写入探测；不可写时原生消息框提示并拒绝启动，
  不回退用户目录。测量 / 验收覆盖业务目录时，便携版 WebView2 目录一起隔离。
- 便携版在建窗前用 `tauri::webview_version()` 检查 WebView2；缺失时给出安装指引后退出。
- 凭据继续使用系统存储（Windows Credential Manager / macOS 钥匙串，Q204、ADR-0015）；
  验收文件后端只在 `REVIA_ACCEPTANCE=1` 时启用，与分发形态无关。令牌与刷新仍作为同一
  钥匙串条目原子轮换，不写入便携 data。
- `distribution_info` 命令返回形态、实际数据目录、WebView2 目录与凭据后端，供界面与诊断使用。

### 02 PowerShell 环境、hooks 与终端（`shell.rs`、`shell/windows_env.rs`、`m7/terminal.rs`）

- Windows 环境捕获改为：PowerShell 7（`pwsh.exe`，PATH / Program Files / WindowsApps）
  → Windows PowerShell（`System32\WindowsPowerShell\v1.0`），与终端共用同一选择。
- 在目标 worktree 启动 PowerShell，显式加载四个用户 profile；profile 报错写入 stderr
  诊断但不丢弃其余环境；加载完成后再次进入 worktree，触发 fnm 等目录钩子。
- 环境通过临时文件以 UTF-8、NUL 分隔的 `KEY=VALUE` 转储，stdout 为空；值可含 `=`、
  换行与 Unicode。Windows 变量名按不区分大小写合并 / 查找。
- 5 秒硬超时；启动失败、超时或空环境回退到进程环境并标记 `used_fallback` / 诊断。
  缓存仍按 worktree，`reload_environment` 继续强制失效。
- 内置终端使用同一 PowerShell 选择与 profile 行为（`-NoLogo`），保留真实 PTY、会话归属、
  尺寸与关闭协调。
- Git 后台进程（普通调用、stash 窄例外、提交、克隆）在 Windows 使用
  `CREATE_NO_WINDOW`，不额外弹控制台。

### 03 Windows 路径与换行完整性（`platform.rs`、`tests/windows_line_endings.rs`）

- 抽出 `external_path_text` 并补 verbatim / UNC 纯文本回归：`\\?\C:\…` 去前缀，
  `\\?\UNC\server\share` 还原为 `\\server\share`，普通盘符与 UNC 不变。
- 新增真实 Git 集成回归（隔离 HOME / 全局配置 / 临时仓库）：
  - `.gitattributes` `*.txt text eol=crlf` 下暂存：工作区字节不变，index 归一为 LF；
  - 丢弃：恢复提交时的 CRLF 字节，并先把被覆盖内容完整存入回收区；
  - 编辑保存：保留 CRLF 换行。
- 既有中文 / 空格路径暂存、身份与指纹、写锁、回收区语义未放宽；完整测试目标通过。

### 04 Windows 外壳、快捷键与输入（`src/lib/platform.ts`、`shortcuts.ts`、`App.tsx`、`LogView.tsx`）

- 修复非 macOS 快捷键展示：`Ctrl+Shift+K` / `Ctrl+Alt+A`，不再出现 `Ctrl++K`；
  抽出 `formatAcceleratorFor` 以便在 macOS 回归 Windows 文案。
- AltGr 视为字符输入：`isPrimaryModifier`、`matchesAccelerator`、快捷键捕获与全局
  handler 均不把它当业务快捷键；输入法组合（`isComposing` / keyCode 229）同样跳过。
- 日志视图的 Ctrl+F / Ctrl+D 也加入 AltGr / 组合输入保护。
- 原生标题栏、装饰、阴影与 Mica 接线保持既有实现；Windows 11 用 Mica，Windows 10 /
  辅助功能设置继续走既有 CSS 回退。macOS 建窗路径未改。

### 05 安装版安全更新与便携提示（`m7/update.rs`、`UpdatePanel.tsx`）

- 前后端均识别便携形态：后端在下载、安装、重启入口拒绝便携版（即使直接调用命令），
  前端只显示“检查更新 + 下载便携版”，不出现下载/安装/重启按钮。
- 便携下载地址由配置的 GitHub Release 仓库与 Q65 命名约定在 Rust 侧生成
  （`Revia-<版本>-windows-x64-portable.zip`），经 `open_url` 只允许 https://github.com。
- 安装前先把“待安装版本”写入本地状态，再调用安装器；Windows 安装器结束进程后，
  下一次启动按真实版本核对：达到版本才写安装回执，未达到则保留待安装状态供核对/重试。
  这使回执明确区分“待安装”与“新进程核对成功”，不假设退出后代码继续运行。
- 安装仍只使用已下载、已验签的原字节；macOS 更新、稳定/预发布、HTTPS 与权限约束未改。

### 06 Actions 双平台产物与统一清单（`.github/workflows/`、`tools/`）

- Release 工作流拆为 `build-macos`、`build-windows` 与汇总 `publish`：两平台先独立构建，
  再统一下载产物生成同时含 `darwin-aarch64` 与 `windows-x86_64` 的 `latest.json` /
  `latest-prerelease.json`，避免并发覆盖。
- Windows 构建：
  - NSIS currentUser 安装版 `Revia-<版本>-windows-x64-setup.exe`；
  - `--features portable --no-bundle` 的单 exe `Revia-<版本>-windows-x64-portable.exe`；
  - zip 内为 `Revia/Revia.exe` 文件夹（Q66）；
  - `windows-SHA256SUMS.txt`。
- `createUpdaterArtifacts: true` 的 Windows updater 包按官方 updater 文档使用
  `setup.exe` + `setup.exe.sig`（不是 v1Compatible 的 `.nsis.zip`）；清单生成器同时保留
  `.nsis.zip` 兼容分支。
- 便携构建不混入安装包，不打包 Git / WebView2；依赖系统 WebView2。
- 新增 `.cargo/config.toml` 的 `x86_64-pc-windows-msvc` `+crt-static`：Rust MSVC 默认
  动态 CRT，静态链接后便携单 exe 不要求用户安装 VC++ Redistributable。CI 在 Windows
  runner 上对最终 exe 运行 `dumpbin /dependents`，由 `tools/check-windows-deps.mjs`
  拒绝 `vcruntime*` / `msvcp*` / `ucrtbase` / `api-ms-win-crt-*` / 动态 `WebView2Loader.dll`。
- 保留 main 来源门禁、版本一致性、签名密钥/公钥/endpoint 缺失失败、稳定/预发布与
  macOS 产物；本任务未执行任何推送、打标签或 Release 操作。

## 本机检查与结果

所有命令在 `/Volumes/T7 Shield/project/Revia-m8` 执行；日志、精确命令与退出码在证据根
`checks/` 下。下表退出码均为真实值。

| 检查 | 命令 | 退出码 | 结果 / 证据 |
| --- | --- | --- | --- |
| 前端类型 | `pnpm exec tsc -b` | 0 | `checks/final-tsc.log` |
| Lint | `pnpm lint` | 0 | `checks/final-lint.log`；警告仅来自既有 `docs/milestones/m7/mockups` 预览文件 |
| 前端构建 | `pnpm build` | 0 | `checks/final-build.log`；仅 Vite 分块体积提示 |
| x18n check | `pnpm i18n:check` | 0 | `checks/final-x18n-check.log`；0 errors，75 个 unused-key 警告按 Q171 保留（未删除键） |
| 写死中文扫描 | `pnpm i18n:extract` | 0 | `checks/final-x18n-extract.log`；78 个候选，0 个含中文 |
| 相关 Node 回归 | `node --test tools/test-shortcuts.mjs tools/test-update-lifecycle.mjs tools/test-release-manifest.mjs tools/test-windows-deps.mjs tools/test-release-source.mjs tools/test-ui-input.mjs tools/test-terminal-sessions.mjs` | 0 | `checks/final-node-tests.log`；38 tests，38 pass，0 fail |
| Rust 完整测试 | `/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml --no-fail-fast` | 0 | `checks/final-cargo-test.log`；32 个测试目标 ok，0 failed |
| 便携特性检查 | `/opt/homebrew/bin/cargo check --features portable --manifest-path src-tauri/Cargo.toml` | 0 | `checks/final-cargo-check-portable.log` |
| 增量修复后单元测试 | `/opt/homebrew/bin/cargo test --lib --manifest-path src-tauri/Cargo.toml` | 0 | `checks/final-cargo-test-lib.log`；368 passed，0 failed（`af869f9` 增加便携 WebView2 目录可写检查后重跑） |
| 便携特性单元测试 | `/opt/homebrew/bin/cargo test --features portable --lib --manifest-path src-tauri/Cargo.toml` | 0 | `checks/cargo-test-portable-lib.log`；367 passed，0 failed |
| 绑定生成 | `pnpm bindings`（含 `tools/strip-binding-whitespace.mjs`） | 0 | `checks/final-bindings-strip.log`；清理 102 个生成文件的行尾空白 |

补充：

- 固定路径 `/tmp/revia-*.txt`、`/tmp/revia-cargo-test.log` 的旧输出已复制到
  `pre-existing/`，原文件未删除；其中第一次 `cargo test` 因规划方消息中断（工具报
  `Command aborted`），未取得有效退出码，已在证据根完整重跑并以上表为准。见
  `pre-existing/README.md`。
- `pnpm bindings` 会重新生成全部绑定；为只提交真实变更，除 `AppUpdateStatus.ts` 与三个
  新绑定外，其余绑定已恢复到 HEAD，避免纯空白变化进入提交。`git diff --check` 通过。
- 提交前源码状态：HEAD `e7f4c25`，工作区 diff SHA-256
  `6d664ecef01982723a58f313ccfa10fa0526fde8784ea839a06e2dec91b65c35`，见
  `final-source-state.txt`；实现提交 `a30b435` 与 `af869f9`。
- 证据根由 `mktemp -d` 独占创建；测试仓库、应用数据与 profile 都在 `tempfile` 临时目录中，
  未读写真实用户仓库、凭据或系统配置。

## 未验证与原因

1. **Windows 编译**：本机只有 macOS 工具链，无 `x86_64-pc-windows-msvc` target；未把
   `cfg(windows)` 代码声明为已通过 Windows。工作流已加入 Windows 编译检查，但本轮未推送，
   未执行远端 Actions。
2. **Windows 安装 / 卸载 / 升级 / 便携启动**：没有 Windows 桌面；安装包执行、WebView2
   运行时准备、便携目录实际落点与改名/移动均待任务 07。
3. **Credential Manager 真实登录与重启**：只在单元测试验证了后端选择；没有写入真实凭据。
4. **PowerShell 7 / Windows PowerShell / profile / hooks 真机行为**：纯协议与合并逻辑有
   单元测试，真实 PowerShell、fnm use-on-cd、执行策略与超时行为待 Windows。
5. **Mica、原生标题栏、DPI、IME / AltGr、减少透明度 / 高对比度、WebView2 diff 首屏 /
   跳转 / 滚动**：只在代码与既有 CSS 层验证，视觉与性能必须真实可见窗口验收。
6. **静态 CRT 最终二进制**：`+crt-static` 配置与 `dumpbin` 检查已加入工作流，但未在
   Windows runner 实际运行；不能把 CI 定义当作通过。
7. **Windows updater 包格式**：按 Tauri 官方 updater 文档核对为 `setup.exe` + `.sig`
   （`createUpdaterArtifacts: true`），但未在 Windows 实际构建/安装验证。

## 遗留与风险

- 工作流未执行：所有 Actions 结论需 Codex 在取得 Windows 环境/权限后确认；本任务未推送。
- `.cargo/config.toml` 的 `+crt-static` 若在 Windows 构建中发现与某个原生依赖冲突，
  需保留证据再调整；不得把动态 CRT 的便携包当作“解压即用”。
- 便携版更新只打开下载地址；下载后替换 exe 的用户流程需在验收中确认数据与登录保留。
- Windows 安装器结束进程的行为已按“待安装 → 新进程核对”处理，真实升级重启需验收。
- 既有 M4/M5/M6/M7 未达标项不因本任务改变；未新增 backlog 条目。
- 实施方未修改 ADR / 决策文档；ADR 文件名和链接由规划方核对正常。

## 交付

- 实现提交：`a30b435`（主体）、`af869f9`（便携 WebView2 目录检查）、`a3abb92`（正式返修），均只在 m8。
- 报告与验收步骤：本文件与 [`windows-acceptance.md`](./windows-acceptance.md)。
- 状态：任务 01–07 `ready-for-human`；唯一一次正式返修已完成，等待 Codex 增量复核与真实 Windows 验收。

## 正式返修（2026-11-02）

按 Q113 集中处理首轮审查的 X1、X2/S1、X3、R1、R2，S2 转 backlog 17，不扩大范围。
修复提交 `a3abb92`，完整证据与逐项说明见 [repair01-report](./repair01-report.md)。

- X1：Windows `fallback_windows` 的 `timed_out` 改为真实 bool；超时、profile、Set-Location
  与一般捕获失败分别映射，不再伪装成同一原因。仍未运行 Windows 编译。
- X2/S1：profile / Set-Location 失败整次捕获失败并回退进程环境；会话检查使用结构化错误键
  与独立原始详情，x18n 中英文键已加入。
- X3：显式验收/测量的安装版与便携版都使用隔离 WebView2 目录；先选覆盖路径再创建/探测，
  正常启动与 macOS 建窗路径保持不变。
- R1：stderr 改用有界 `OutputPipe` 读取，进程回收后限定排空窗口；真实子进程持有写端与
  超限输出的回归通过。
- R2：Windows 安装前明确“安装并重启”确认，可取消；确认后仍走既有草稿、写入与终端协调；
  macOS 安装后单独确认重启保留。

返修后本机检查（新证据根）：前端类型、lint、构建、x18n、写死中文扫描、40 个相关 Node
测试均通过；Rust 完整套件以 `--test-threads=4` 运行，32 个目标 ok、lib 377 passed；
便携特性检查通过。默认并发完整测试曾出现失败（原因尚未确认），日志保留；逐项重跑与
4 线程完整套件通过。Windows 编译、真实安装器与真机行为仍未验证。

## Codex 首轮独立复核（2026-11-02）

完整意见见 review01。代码阶段发现 Windows 类型错误、profile 失败回退与诊断、验收 WebView2 隔离、环境捕获硬超时和安装重启确认问题，集中交 Pi 唯一正式返修。上述“本机通过”只证明所运行平台与固定源码，不代表 Windows 分支可编译。未修前本报告主体结论仅为实施方初始交付声明；最终状态以独立复核和修复证据为准。

## Codex 最终阶段结论（2026-11-02）

代码候选固定为 `83d4070`：Pi 主体与一次返修之后，Codex 按 Q113 接管 S3/X4，将非终止 PowerShell profile 错误纳入失败回退，并补 Windows 真实进程回归。两轴审查见 review01/review02，规划方证据与检查范围见 [verification](./verification.md)。

代码和当前 Mac 上可行的验证已完成；Windows 编译、打包、安装升级、真实凭据、PowerShell 与 WebView2 桌面仍未验证，不能据此宣称 M8 全部验收通过。backlog 17/18 两项 P3 保留；m8 未合 main，未推送或发布。默认并发两轮失败与 4 线程完整通过都保留，尚无证据确定其根因。

## 后续合并与发布授权（2026-11-02）

用户在上述交付后明确授权合并 main、发布 0.0.2 并删除现有旧 Release（Q207）。实际过程见 [发布记录](./release-0.0.2-readiness.md)；本报告历史未验收项继续保留，代码交付与发布不等于 Windows 真机验收通过。


## 0.0.2 公开后的状态（2026-11-02）

M8 与发布前两项用户反馈补修已合入 main，0.0.2 已公开。前文未执行的 CI/打包描述是交付时记录；后续固定应用源码的双平台完整 CI、安装版/便携版打包、验签和公开下载结果见 [发布记录](./release-0.0.2-readiness.md)。原始失败记录保留；Windows 真实桌面验收继续 ready-for-human，没有因发布改写为通过。
