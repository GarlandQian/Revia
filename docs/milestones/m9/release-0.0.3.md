# Revia 0.0.3 M9 合并与发布记录

Status: 进行中
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

（发布后补写）
