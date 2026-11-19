# M12 改动树选区与文件历史

Status: done

应用基线 `81c5ce9`。首次实现 `b00816c`，审查修订及桌面构建源码 `674b438`。原始本机证据目录为 `/private/tmp/revia-m12-evidence.Xvl6wb3v`。

## 实现和审查

- 批量操作使用暂存区模式的改动树选区。折叠目录保留显式选中的文件并显示数量。键盘与鼠标 Shift 范围选择共用展开后的行顺序。
- 文件历史列表只读取元数据。选中条目的 Git diff 由带类型的 Rust 命令单独读取。预览请求按当前 worktree、历史条目和读取次序归属。
- Pi 首两种模型返回配额错误，第三种模型请求超时，第四种模型返回服务不支持。规划方依 Q137 接手实施。
- 首次规范和规格审查均发现，更改列表预览 B 时若仍在读取元数据，快捷键可能误用先前预览的 A。规范审查另发现改动树会截获用户自定义的空格快捷键。修订于 `674b438`，这两种行为均有回归覆盖。详见[任务 01](./issues/01-tree-selection.md)。

## 自动检查

| 检查 | 实际结果 |
| --- | --- |
| 选区、更改列表、页签、输入与快捷键五套 Node 测试 | 修订后 43 项通过，含空选区、跨折叠目录、预览迟到和自定义快捷键。原始输出 `selection-final.log` |
| 文件历史和结构化错误的 Node 测试 | 14 项文件历史测试通过，另有 1 项真实错误载荷的双语测试通过。首次错误套件缺少 CSS 模块加载处理，修正后复跑通过，见 `structured-errors.log` 与 `structured-errors-retry.log` |
| Rust 真实 Git 与绑定 | 文件历史相关 14 项集成与 7 项绑定测试通过，另有 5 项夹具测试通过。首次缺少 Tauri 编译所需的前端产物，生成后复跑通过。命令、结果与源文件摘要见 `history-rust-results.json` |
| 满页历史 Git 调用 | 41 次文件变更的 fixture 中，读取首 40 条、查看首条、翻到下一页 1 条，读取 diff 的 `git show` 总计 1 次，见 `history-fixture.history-full-page.json` |
| 前端与打包 | `pnpm typecheck`、`pnpm lint`、`pnpm i18n:check`、`pnpm i18n:extract`、`pnpm tauri build --bundles app` 均退出 0。lint 保留历史 mockup 警告，Vite 提示入口包体积。macOS 可执行文件 SHA-256 为 `73641208dde9b369010be438a1f41d9b2d1f732cb5726e3619cf44d2f495a3bf` |

## 平台

本轮构建与验收平台为 macOS Apple Silicon。Windows 真实桌面尚待验证。本轮未执行完整 Rust 套件，覆盖范围为修改的文件历史、绑定和受影响的夹具。

## macOS 桌面验收

2026-11-18 首次验收时，macOS 锁屏，指定窗口截图返回 `could not create image from window`。2026-11-19 解锁后，在同一验收应用与独立数据目录中完成操作。测试仓库中的 A、B、C 均有未暂存改动，暂存区起始为空，B 有 40 条文件历史。

| 操作 | 观察结果 |
| --- | --- |
| 点 A，按住 ⌘ 依次点 B、C，再点 B 取消选择 | A、C 仍被选中，右侧显示 B 的未暂存内容 `B preview marker, unstaged`。[预览截图](./evidence/preview-b.png) |
| 在预览 B 时按 ⌥⌘A | `git diff --cached --name-only` 只有 `A.txt`、`C.txt`，`git diff --name-only` 只有 `B.txt`。[暂存后截图](./evidence/staged.png) · [Git 原始结果](./evidence/index-after.json) |
| 从“更多”打开 B 的文件历史 | 列表共 40 条。首条 `B history 40` 的 diff 显示第 39 版改为第 40 版。读历史 diff 的 `git show` 执行 1 次，诊断序号为 1794。[首条截图](./evidence/history-first.png) |
| 直接点击第三条 `B history 38` | 右侧显示第 37 版改为第 38 版，新增 1 次读取，诊断序号为 1966。[第三条截图](./evidence/history-third.png) |

命令计数按实际点击时间和文件历史 diff 的 Git 参数核对。两次读取及验收应用的退出码均为 0。首条截图右侧仍可见“更多”菜单，点选第三条后菜单已收起。原始窗口、辅助功能树和命令快照保存在本机证据根的 `desktop-tzwkh5ah/window-acceptance/`。[桌面验收记录与截图摘要](./evidence/desktop-verification.json)
