# 02：测量构建工作流与朴素外观对照开关

Status: ready-for-agent
Blocked by: None（可立即开始）

## What to build

推送 `m9` 后，GitHub Actions 打出 Windows 安装版、便携版 exe 和 zip，作为产物供用户下载。测量版带一个对照开关，可以一键切到朴素外观。见规格 H 节。

## Acceptance criteria

- [ ] 新增 `.github/workflows/measure-build.yml`：只在推送 `m9`、或手动运行且 ref 为 `m9` 时触发；在 `windows-latest` 上构建三种产物，上传为 Actions 产物，保留 14 天，产物名带 `m9` 和提交短 SHA。
- [ ] 不需要更新签名密钥，不生成更新清单，不创建 Release，不打标签；`release.yml` 和 `compile-check.yml` 的现有行为不变。
- [ ] 新增 `measure` 编译特性：关闭应用更新检查；“关于”和诊断信息显示 `m9` 与提交 SHA。
- [ ] `measure` 下“帮助”菜单多出对照开关：打开后，下次启动时窗口以不透明方式创建，不用 Mica，没有模糊、镜片和半透明；标题注明处于对照状态，诊断信息记录对照状态。
- [ ] 正式构建（不开 `measure`）中没有对照开关的入口，也没有相关代码路径。
- [ ] 本机通过 `cargo check --features measure`；工作流语法用现有工具检查。实施方不推送，也不运行远端 Actions。

## Comments
