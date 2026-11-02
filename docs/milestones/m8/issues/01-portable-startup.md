# 01：安装版与便携版启动及数据隔离

Status: ready-for-agent
Blocked by: None（可立即开始）

## What to build

两种构建形态从首次建窗起选择正确数据位置，便携版可改名或移动且其余数据仍随目录保留。

## Acceptance criteria

- [x] 安装版使用标准当前用户目录，NSIS 配置 currentUser；便携构建可稳定识别，不靠偶然 exe 名或用户环境变量决定。
- [x] 便携业务、缓存、WebView2 用户数据都在 exe 旁 data，首次建窗前接线；改名不改变模式。
- [x] 目录不可写明确失败，便携模式不回退用户目录；保留测量/验收隔离。
- [x] 两种版本共用 Windows Credential Manager，令牌与刷新仍原子轮换；说明凭据不随便携目录移动。
- [ ] 沿系统 WebView2，便携缺失时有可理解的准备指引；不把运行时或秘密塞进普通数据文件。
- [x] 通过现有测试接缝验证数据选择、凭据后端选择和失败行为；真机启动在任务 07。

## Comments

实施说明（`a30b435`、`af869f9`）：

- 形态由 Cargo 特性 `portable` 决定；`distribution::plan` 的单元测试覆盖安装版标准目录、
  便携版 exe 旁 `data` / `data/webview2`、改名/移动后仍为便携，以及 `data` 不可写时
  返回 `errors.distribution.portableDataDirUnwritable`。
- Windows 主窗口在 `setup` 中建窗，WebView2 数据目录在建窗前传入；macOS 建窗路径未改。
- `TokenStore::backend()` 单元测试确认安装版与便携版都使用系统凭据库；验收文件后端只在
  `REVIA_ACCEPTANCE=1` 时启用。令牌轮换沿用既有 `save_credentials` 原子条目。
- WebView2 缺失检查与原生提示代码已实现，但未在无运行时的 Windows 上执行，故该项保持未勾选。

### Codex 首轮审查 · 2026-11-02

X3 · P2：显式 Windows 验收/测量的安装版也须隔离 WebView2 数据；先选覆盖路径再创建/探测，避免触碰正常便携 data。 详见 [review01](../review01.md) 与 [repair01](../repair01.md)。

### 正式返修 · a3abb92

X3 已修：`distribution::plan` 改为纯路径计算，新增 `with_isolated_data_dir`；`lib.rs`
先选定显式验收/测量覆盖目录，再创建/探测。安装版与便携版的覆盖都使用
`<覆盖目录>/webview2`；正常启动（安装版默认位置、便携版 exe 旁 data）保持不变，
macOS 建窗路径未改。单元测试覆盖两种形态的最终目录、覆盖时不触碰标准目录/便携正常
`data`，以及正常便携 `data` 被占用时覆盖仍先成功。证据见
[repair01-report](../repair01-report.md)；真实 Windows WebView2 落点仍待任务 07。
