# 发布与更新签名

用户于 2026-11-02 授权将 M8 合入 main、发布 0.0.2 并删除现有 0.0.1 Release（Q207）。本轮构建、校验与公开状态见 [0.0.2 发布记录](milestones/m8/release-0.0.2-readiness.md)；Windows 真机与 M7 完整验收仍未完成。旧版历史记录见 [0.0.1 发布准备](milestones/m7/release-0.0.1-readiness.md)。

本文说明 Revia 的版本标签、Release 草稿、更新签名与首次打开的处理方式（M7 任务 12，规格 H、Q62、Q63、Q65、Q182、Q188）。这里写的是**已实现的工作流和 GitHub 一次性接线约定**；仓库不保存真实凭据。实际打包运行、配置核对和草稿状态见本轮发布准备记录。

## 工作流分工

| 工作流 | 触发 | 做什么 |
| --- | --- | --- |
| `.github/workflows/compile-check.yml` | 推送 `main` / `m2` / `m3` / `v*` 标签、手动 | 类型检查、lint、前端构建、Rust 测试、release 编译；macOS arm64 与 Windows x64 各一份 |
| `.github/workflows/release.yml` | 推送 `v*` 标签 | 校验 main 来源与版本 → 分别构建 macOS arm64 和 Windows x64 安装版 / 便携版 → 汇总签名更新清单并上传到**草稿** Release |

`release.yml` 只做草稿：`draft: true`。公开草稿是维护者的手动动作。客户端读取构建时配置仓库的公开 Releases 完整分页，在用户选择的渠道中选取较新的最高语义版本，再读取该标签自己的 `latest.json`；草稿不参与更新。

## 版本必须对齐

同一个版本号出现在三处：`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`。标签必须与它们一致，否则 `tools/check-release-version.mjs` 直接失败，避免打出指向错误版本的包。

```bash
RELEASE_TAG=v0.0.1 RELEASE_REPOSITORY=<owner>/<repo> node tools/check-release-version.mjs
# version=0.0.1
# channel=stable
# endpoint=https://github.com/<owner>/<repo>/releases/latest/download/latest.json
```

渠道由标签后缀决定：

| 标签 | 发布渠道 | 该版本清单与安装包 |
| --- | --- | --- |
| `v0.0.1` | stable | `releases/download/v0.0.1/latest.json` 及该标签资产 |
| `v0.0.1-beta.1` | prerelease | `releases/download/v0.0.1-beta.1/latest.json` 及该标签资产 |

两种构建都使用上例同一仓库入口，不能把 beta 客户端固定到自身标签。应用默认只选择稳定版；预发布要在设置里由用户主动开启（规格 H）。清单里的安装包 URL 固定到其自身标签，不能使用 `releases/latest/download` 指向另一版本。

## 更新签名

- 使用 Tauri updater 的强制签名校验（minisign）。公钥随构建注入，私钥只在 CI 里。
- `src-tauri/tauri.conf.json` **不写**公钥和检查地址：没配置时设置页与更新面板明确显示“更新通道未配置”，不会假装能更新。
- 发布构建时 `tools/write-updater-config.mjs` 生成 `src-tauri/tauri.release.conf.json`（构建产物，已在 `.gitignore` 里），内容包含公钥、检查地址、`requireSignedVersion: true` 与 `createUpdaterArtifacts`；私钥不写进覆盖配置。更新签名必须绑定应用版本；Tauri build 自动将当前版本加入签名，缺少版本的旧签名不能用于恢复安装。
- 本机生成一对密钥（只需一次，私钥自己保管，不要提交）：

  ```bash
  cargo install minisign --locked   # 或 brew install minisign
  minisign -G -p revia-updater.pub -s revia-updater.key
  ```

  公钥文本（`revia-updater.pub` 全文，含 `untrusted comment:` 行）填进仓库变量 `REVIA_UPDATE_PUBKEY`；私钥文件内容填进 secret `TAURI_SIGNING_PRIVATE_KEY`。

- macOS 产物：`Revia-<version>-macos-arm64.dmg`、`Revia.app.tar.gz` 与配套 `.sig`。Windows 产物：`Revia-<version>-windows-x64-setup.exe` 与配套 `.sig`，以及 `portable.exe` / `portable.zip`。便携版不原地更新。
- 两平台各生成 SHA256 文件，汇总任务生成同时含 `darwin-aarch64` 与 `windows-x86_64` 的 `latest.json`。更新包 `.sig` 缺失时工作流失败。

## 需要在 GitHub 上配置一次

| 名称 | 类型 | 内容 |
| --- | --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | secret | minisign 私钥文件全文 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | secret | 私钥口令（生成时留空则填空字符串） |
| `REVIA_UPDATE_PUBKEY` | **variable** | minisign 公钥全文 |
| `REVIA_GITHUB_CLIENT_ID` | **variable** | GitHub App 的公开 Client ID（设备码登录） |

公钥是公开信息，用 variable 便于在日志里核对；私钥与口令只用 secret。工作流不会打印私钥内容。

GitHub App（设备码登录）使用公开的 `REVIA_GITHUB_CLIENT_ID`（见 `docs/adr/` 与任务 06）。`release.yml` 从同名仓库 variable 注入整个构建任务，使 Rust 在编译时读取；它为空时，发布前置检查直接失败，避免生成无法设备码登录的安装包。它不属于发布签名私钥，不使用 client secret。

## 发布流程

1. 把三处版本号改成目标版本，提交到 `main`。
2. 打标签并推送：`git tag v0.0.1 && git push origin v0.0.1`（该次打标签与推送须有用户授权）。
3. 等双平台 `Compile Check` 和 `Release Draft` 完成，核对草稿的 macOS / Windows 产物、SHA256、更新包签名与统一 `latest.json`。
4. 在真实 macOS Apple Silicon 机器上安装草稿里的 dmg，确认首屏、更新检查与签名校验。
5. 核对稳定版 / 预发布标记后公开草稿。之后符合用户所选渠道的客户端才会收到这个版本。

## 首次打开（无 Apple 开发者证书）

当前不分发 Apple 证书签名与公证（规格 H 明确不做）。因此首次打开需要用户手动放行：

1. 打开 `.dmg`，把 `Revia.app` 拖到「应用程序」。
2. 第一次双击会被 Gatekeeper 拦下；在「系统设置 → 隐私与安全性」里点「仍要打开」，或右键应用选「打开」。
3. 之后正常双击即可。

升级包同样未经 Apple 签名，但**更新包本身**由 minisign 校验：签名不匹配时 updater 会拒绝安装，应用保持当前版本并显示失败原因。

## 本地验证（不推送、不打标签）

```bash
# 版本与标签对齐
RELEASE_TAG=v0.0.1 RELEASE_REPOSITORY=<owner>/<repo> node tools/check-release-version.mjs

# 生成 updater 覆盖配置（用测试公钥，不要用生产私钥）
RELEASE_VERSION=0.0.1 \
REVIA_UPDATE_ENDPOINT=https://github.com/<owner>/<repo>/releases/latest/download/latest.json \
REVIA_UPDATE_PUBKEY="$(cat /tmp/revia-test.pub)" \
node tools/write-updater-config.mjs

# 构建（会产出 updater 产物；需要 TAURI_SIGNING_PRIVATE_KEY 指向测试私钥）
pnpm tauri build --target aarch64-apple-darwin --bundles app,dmg \
  --config src-tauri/tauri.release.conf.json
```

测试密钥与坏签名只用于独占 fixture，不能当生产发布身份。本地跑通不等于 Actions 跑通：真实 CI 与分发证据留给最终验收（任务 14）。


## 下载与安装状态

确认下载后，验签通过的原始包字节与签名清单保存在独占应用数据目录。确认安装时重新核对签名、版本与包散列，直接离线安装已保存字节，不再次下载。安装与重启分别进入应用的关闭协调流程；编辑、合并编辑、Git 写入、待核对结果或运行中终端未处理时不安装、不重启。macOS 安装后单独确认重启；Windows 安装前明确确认“安装并重启”，先写待安装状态，由新进程核对实际版本后生成回执。便携版只提供新版下载入口。历史回执不会遮住后来发现的新版本。真实生产来源、CI 签名和桌面更新验收仍需部署接线，独占 fixture 检查不能替代。
