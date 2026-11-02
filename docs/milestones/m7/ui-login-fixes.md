# 提交区贴底与浏览器登录修复

2026-11-02，Codex 根据用户两张截图直接修复。范围是“提交区应该固定在最下面”和“浏览器登录呢”，沿 Q184 / Q193；不改变 M7 整体验收结论或发布版本。

## 修改

- 提交区根节点增加 `mt-auto`，空列表与加载占位时也贴侧栏底部。文件列表仍占剩余空间并独立滚动，短窗口沿用提交区的最大高度和内部滚动。
- GitHub 账号设置增加浏览器设备码登录；PR 面板复用同一组件。获得验证码后打开系统默认浏览器，支持复制验证码、再次打开和取消。Token 备用入口与未配置提示保留，已有账号仍可添加账号。
- 新增固定类型 IPC `m7_github_device_open`。平台层只打开固定 `https://github.com/login/device`，不接受前端或远端返回的任意 URL，不传验证码或令牌。
- 共用 controller 按服务端间隔轮询，处理过期、拒绝、网络错误；取消、卸载和重试使迟到响应失效。浏览器打开失败保留验证码和轮询，便于重开。
- 双语文案通过 x18n CLI 更新。复制失败复用已有 `commit.copyFailed` 结构化翻译。

## 固定源码与隔离

基线：`6c3bd93e39f52286bb0e271a52bee940d55d0235`，工作目录 `/Volumes/T7 Shield/project/Revia-m7`，分支 `m7`。验收源码为基线加本次补丁及工作区原有增量，逐文件 hash 在 `native-source-02.json`；二进制 hash、argv、进程与隔离变量在 `desktop-process-02.json`。

证据根：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-ui-login-fix-9l6uwu9c`。保留初次失败的 build / lint 原件；补齐 JSX 闭合标签与测试 lint 后重跑受影响检查。

原有六个未提交文件（任务 14、native-acceptance、git/mod.rs、SideBySideView、sideGeometry、test-sync-scroll）的 hash 与 `before.json` 一致，不计入本次提交。桌面二进制包含这些既有工作区增量，不能当作纯净发布构建的验收。主目录 main 与已公开 v0.0.1 未改变。

桌面使用本次新建的 60 文件 fixture、独立 appdata、独占凭据文件路径和 loopback HTTP。HTTP 只返回假设备码及 pending；没有提交假码到 GitHub，没有进行真实账号授权或访问真实钥匙串。验收应用正常退出，exit 0。

## 验证

| 检查 | 结果与原件前缀 |
| --- | --- |
| `pnpm build` | 通过，`frontend-build-03`；既有 Vite configLoader / chunk size 提示保留 |
| 定向 oxlint | 通过，`changed-lint-02`、复制错误修正后 `component-lint-03` |
| `node --test tools/test-github-device-flow.mjs tools/test-github-requests.mjs` | 16/16 通过，`github-node-02`；设备码 8 项，既有请求代际 8 项 |
| `cargo test --manifest-path src-tauri/Cargo.toml --test m7_github_auth --locked` | 通过，`rust-auth`；顶层 1 项，内部 4 个隔离场景，不重复累计嵌套 harness |
| `pnpm exec x18n check --json` | 0 errors / 75 warnings，`x18n-03`；不删除本次无关的未使用键 |
| `cargo build --manifest-path src-tauri/Cargo.toml --locked` | macOS debug 原生构建通过，`native-build-02`，嵌入最新前端 |
| 真实 macOS 桌面 | 空列表贴底，1280×521 短窗口长列表滚到 file-59 时提交区仍完整可见；设置页和 PR 面板均有设备码入口，点击实际打开 Safari 的 GitHub 设备授权页；复制后粘贴得到 `DEMO-CODE`；关闭设置或取消后无后续轮询 |

截图：`empty-bottom.png`、`empty-short.png`、`long-short-scrolled.png`、`github-settings.png`、`github-device-code.png`、`pr-device-code.png`。这些文件保留 CUA 返回的原始 JPEG 字节，历史文件名虽用 .png，未转换图像。浏览器 URL 与复制结果另有 `browser-open.ax.txt`、`copied-code.ax.txt`。轮询仅两次 start / 26 次 poll，原始路径和时戳在 `desktop-http.jsonl`；关闭设置后的空闲观察超过 108 秒，见 `after-settings-close.json`。取消后仍无新增请求，见 `after-cancel.json`。

横向缩窗手势未改变 1280px 宽度，因此不宣称本轮已完成 900px 窄窗验收。未在 Windows 真机检查浏览器打开；真实 GitHub 授权成功、钥匙串、正式更新安装和 Gatekeeper 等发布限制继续保留。

## Standards

独立增量审查发现 1 项 P3：复制验证码失败时原样展示英文 DOMException。已改为 `localizedError("environment", "commit.copyFailed")`，复用中英文翻译，构建和 lint 通过。未发现 P1/P2，Fowler 判断项 0。原报告 `review-standards-result.md` 保留。

## Spec

独立增量审查 0 项发现。提交区贴底、设置与 PR 共用浏览器登录、Token 备用、轮询与取消均符合本次要求。原报告 `review-spec-result.md` 保留。

两轴结果：Standards 1 项 P3 已修复，无未处理发现；Spec 0 项。修复只提交本地 m7，不推送、不重写已公开 v0.0.1，不生成新的分发安装包。
