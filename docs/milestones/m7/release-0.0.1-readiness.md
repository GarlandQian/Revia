# 0.0.1 Actions 打包与发布准备

日期：2026-11-01。用户指定版本 `0.0.1`，并再次明确由 GitHub Actions 打包。构建产物以 Actions 为准；打包成功与完整 M7 验收分别记录。

## 固定版本与检查

- `33fac65ebc328aa2067a0a736e9e47e2b6a1e434` 将 package.json、tauri.conf.json、Cargo.toml 及 Cargo.lock 中 revia 本身的版本统一为 `0.0.1`；依赖版本未变，发布文档示例同步。
- `node tools/check-release-version.mjs --tag v0.0.1 --repository GarlandQian/Revia` exit 0，版本一致，渠道 stable。该命令只校验标签字符串，不能证明远端构建或更新可用。
- `cargo metadata --manifest-path src-tauri/Cargo.toml --no-deps --offline --locked --format-version 1` exit 0，revia 版本为 `0.0.1`；git diff --check 通过。未为版本配置新增测试。
- 工作区原有6个未提交文件逐字节保留，不纳入本次 Actions 候选。Actions 只检出标签指向的已提交源码；不把本地含未提交性能修改的构建结果移用于该候选。

## GitHub 接线与打包

2026-11-01 实际只读核对 GarlandQian/Revia：现有 TAURI_SIGNING_PRIVATE_KEY、TAURI_SIGNING_PRIVATE_KEY_PASSWORD 两个 secret 名称，以及 REVIA_UPDATE_PUBKEY、REVIA_GITHUB_CLIENT_ID 两个 variable 名称。未读取或输出私钥值。配置存在不证明值有效、密钥配对或正式登录成功，后续以实际 CI 和桌面验收为准；前序“用户尚未准备”的记录不再用于判断当前配置是否存在。

现有 `.github/workflows/release.yml` 由 `v*` 标签触发，构建 macOS Apple Silicon 安装包、更新包、签名、SHA256 清单和 latest.json，并上传 Release 草稿；公开草稿仍是单独的发布步骤。版本标签同时触发既有 macOS / Windows 编译检查。当前准备将已提交 m7 候选送入此流程，运行和产物结果在取得后追加。

## 尚未通过的发布验收

- D06 最近一次完整记录首屏238ms，超过150ms；工作区内后续性能修改尚未完成固定候选复验。见 [原生验收记录](native-acceptance.md)。
- Q202 镜片中间帧、真实桌面合成、深色及辅助功能等完整验证仍有缺口。见 [第二批玻璃集成验证](glass-round2-integration.md)。
- 真实 GitHub 登录 / 保护规则、正式来源更新安装、草稿安装包首次下载启动等尚未通过本轮完整验收；CI 配置存在不能替代这些结果。

任务14要求“账号 / 更新正式接线或真实桌面仍缺时整包保持未完成”；当前不将 Actions 打包视为任务01 / 14 done，也不据此公开 Release 或合入 main。3项镜片 P3 仍为非阻断 backlog，不与上述未完成门槛混为一谈。

## 原始证据与本地偏离记录

证据目录 `/private/tmp/revia-0.0.1-preparation-v1634ypr` 保存 before.json、原始文件、补丁、版本校验和 Cargo metadata 原输出。规划方曾偏离已确认流程，先做本地打包：类型 / Vite / Rust Release 和0.0.1应用生成完成，但 `pnpm tauri build --bundles app,dmg` 最终 exit 1，因 Finder AppleEvent 超时（-1712）；保留 build.json / stdout / stderr。仅磁盘映像重试使用生成脚本的 --skip-jenkins 选项，exit 0，见 bundle-retry.json；该本地产物不充当 Actions 产物或正式发布证据。用户纠正后停止沿本地打包交付，转回既定 Actions 流程。
