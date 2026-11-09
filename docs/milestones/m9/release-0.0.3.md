# Revia 0.0.3 M9 合并与发布记录

Status: 已发布（2026-11-09 公开 [Revia 0.0.3](https://github.com/GarlandQian/Revia/releases/tag/v0.0.3)，为 latest；Windows 实测与验收见任务 10、11）
日期：2026-11-08
依据：Q223

## 授权与范围

2026-11-08 规划方补测 macOS 切换耗时后，用户回复“应该没问题 合并到main 发布0.0.3版本”（Q223）。

- m9（`1a56437`）沿项目约定压成一个提交合入 main，版本由 0.0.2 改为 0.0.3（`package.json`、`tauri.conf.json`、`Cargo.toml`、`Cargo.lock`）；
- 授权正常推送 main 和新标签 `v0.0.3`，由现有 `release.yml` 构建 Release 草稿，再公开；
- 不删除、不改写 0.0.1、0.0.2 的 Release 与标签，不强推；
- main 原有的 `.gitignore` 未提交改动不纳入；
- Windows 第一轮实测（任务 10）与验收（任务 11）尚未完成，按用户判断先发布，未验收项写进[发布说明](./release-0.0.3-notes.md)。

## 合并前检查（m9 `1a56437`）

- `pnpm exec tsc -b`、`pnpm lint`、`x18n check`、`pnpm build`：通过；
- 完整 Rust 测试（`cargo test -- --test-threads=4`）：942 个通过、0 失败、1 个既有忽略；
- 正式构建不含测量功能：对照外观命令与菜单只在 `measure` 特性下生效，`release.yml` 不开启该特性；
- `RELEASE_TAG=v0.0.3 node tools/check-release-version.mjs`：三处版本一致，stable 更新入口正确。

原始日志在规划方 scratchpad 的 `m9-release/`。

## 推送、CI 与发布

1. **合入与首轮 CI**：`9047e48` 推送 main 后，[Windows CI](https://github.com/GarlandQian/Revia/actions/runs/42483169949) 的 `remembered_missing_git_falls_back_to_single_capture` 失败：打开时用到的 Git 来自其他 worktree 的记忆（测试与登记的路径写法不同），失效后只删了当前 worktree 的键。`03bda23` 改为丢弃所有指向这个 Git 的记忆并补单元测试，[双平台 CI](https://github.com/GarlandQian/Revia/actions/runs/42485916531) 通过：macOS 943 项、Windows 848 项，均无失败，各 1 项既有忽略。
2. **标签**：`v0.0.3`（附注标签）指向 `03bda23`。SSH 推送需要 1Password 授权，窗口约 60 秒无人批准即失败；标签在用户授权后推送成功，[标签 CI](https://github.com/GarlandQian/Revia/actions/runs/42547456937) 结果与 main 相同。
3. **首次草稿作废**：[首次安装包构建](https://github.com/GarlandQian/Revia/actions/runs/42547457065) 成功，但核对发现 Windows `setup.exe` 与 `.sig` 和 0.0.2 的资产逐字节相同（签名记录 `file:Revia_0.0.2_x64-setup.exe version:0.0.2`）。原因：缓存恢复的 `src-tauri/target/` 留有 0.0.2 安装包，收集步骤取第一个 `*-setup.exe`；清单生成只检查改名后的文件名，没有发现。macOS 与便携版产物正确。经用户同意，草稿（未公开）已删除，标签不变。
4. **发布流程修正**：`ab88f50` 让两个平台构建前清掉缓存恢复的 bundle 目录；Windows 只收集 `Revia_<版本>_x64-setup.exe`，macOS 要求恰好一个 DMG；收集时解码更新签名，trusted comment 须记录本版本（Windows 还核对文件名）。SSH 再次因无人授权失败后，按用户要求用 gh 登录凭据经 HTTPS 一次性推送，未改远端地址和 Git 配置；提交与标签均使用 GitHub 匿名邮箱。
5. **按原标签重建**：手动触发 [安装包构建](https://github.com/GarlandQian/Revia/actions/runs/42557562343)（工作流 `ab88f50`，应用源码 `03bda23`），新步骤均执行成功。
6. **核对**：11 项资产的大小与 GitHub SHA256 digest 和下载文件一致；两个平台校验文件逐项通过；正式公钥对 macOS 更新包与 Windows 安装包的 payload 和 trusted comment 签名验签成功，签名版本均为 0.0.3，Windows 签名文件名为 `Revia_0.0.3_x64-setup.exe`；stable / prerelease 清单字节相同，含 `darwin-aarch64` 与 `windows-x86_64`，URL 固定在 v0.0.3。DMG 与更新包内应用版本 0.0.3、Mach-O arm64；便携 exe 与安装包的版本资源为 0.0.3；zip 只含 `Revia/Revia.exe`，与单独的 exe 字节一致。未运行 Windows 安装器。
7. **公开**：发布说明使用 [release-0.0.3-notes.md](./release-0.0.3-notes.md)（含构建记录），2026-11-09 公开为 latest；0.0.1、0.0.2 的 Release 与标签未动。公开后无认证读取 `releases/latest/download/latest.json` 与已验清单逐字节一致；DMG、setup.exe、portable.zip 公开链接返回 HTTP 206，首字节与已验产物一致。

证据摘要见 [evidence/release-0.0.3](./evidence/release-0.0.3/)：首次草稿与 0.0.2 安装包摘要对比、重建资产摘要、验签结果、清单、公开 Release 元数据与 CI 运行结果。下载产物与完整日志在规划方 scratchpad 的 `m9-release/`；未读取或提交签名私钥与用户令牌。
