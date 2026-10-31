# M7 原生玻璃合并与日志列验证

日期：2026-11-01。规划方：Codex。状态：已合入 m7，命令检查和局部真实桌面验证通过；整套材质验收未完成。任务 01 / 14 不据本记录改为 done。

## 固定源码与现有工作

- 接手基准：m7 `b13b6a7229d9d9c990d89e8b26429740b07450c9`，m7-glass 从 `88320fa5dc687e53acb7b40b0e2b8c2856c096bb` 分出。
- m7-glass 的 38 个未提交文件已备份并分别保存为 `6206d44`（Q200 文档同步）、`262bd47`（原生玻璃与 Mica 实现）；`95d0448` 无冲突合入 m7。
- `24f7410` 修复辅助功能回退与设置页退出确认；`6e29f6346803fd2fce60e60f3afa88145c8950eb` 按用户 Q201 把完整引用标签放入独立列，完整绘制分支图，整表横向滚动。
- 本轮 Release 使用 `6e29f63` 加接手时已有的 6 个未提交文件，不能冒称只由 HEAD 生成。这 6 个文件在合并前后及最终检查中逐字节一致，均未纳入本轮提交：任务 14、native-acceptance.md、git/mod.rs、SideBySideView.tsx、sideGeometry.ts、test-sync-scroll.mjs。完整源码逐文件散列、状态和 SHA 在 `build-source-2.json`；构建结束后再次核对，无源码变化。
- 备份及证据根：`/private/tmp/revia-m7-glass-integration-z3i4cgvx`，包含 before.json、两份 before.patch、before/ 原始文件、命令 stdout / stderr / exit JSON。未推送，未合 main，保留两个 worktree。

## 命令检查

| 命令 | 结果 | 原始证据前缀 |
| --- | --- | --- |
| `pnpm lint` | exit 0；997 个既有告警，本轮所有变动文件零告警。告警来自预览 bundle、terminalSessions.ts、visibleChangelistSelection.ts、test-refresh-gate.mjs、write-updater-config.mjs | lint |
| `node --test tools/test-log-model.mjs tools/test-sync-scroll.mjs` | 72 / 72 通过；含接手时已有滚动优化 | node |
| `/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml --lib export_bindings_glass` | 6 / 6 通过；之后只清理新增 Glass*.ts 行尾空白，未改旧绑定 | glass-bindings |
| `pnpm exec oxlint src/features/log/LogView.tsx src/features/m7/glass/nativeGlass.ts` | 最终修复 exit 0，无告警 | log-lint |
| `node --test tools/test-log-model.mjs` | 最终日志增量 49 / 49 通过 | log-node |
| `pnpm tauri build --bundles app` | 最终 exit 0；包含 `tsc -b`、Vite 前端构建、Rust Release 编译、Revia.app 打包；构建前 touch lib.rs 强制嵌入最新前端。仅已有大 chunk 提示 | release-2 |

本轮是材质与布局修改，复用现有检查，未另建测试框架，也未扩大重跑整包业务测试。最终 Rust 源码与玻璃绑定受测版本对应，之后的产品变化仅为前端两项修复和日志列布局。Windows 工具链 / 真机、完整 clippy / 全量 Rust 套件未运行；Windows 真机按 Q200 在 M8。锁定 windows-version 0.1.7 的 `OsVersion::current().build` 与[该版本文档](https://docs.rs/windows-version/0.1.7/windows_version/struct.OsVersion.html)核对一致，该只读核对不等同 Windows 编译通过。

## 两轴审查

Standards 与 Spec 独立审查，固定 SHA，初轮仅 m7-glass 新增量，后续按 Q174 只审修复新增量，未重新全范围审 M7。

| 范围 | Standards | Spec |
| --- | --- | --- |
| `b13b6a7...262bd47`（merge-base 88320fa） | 1 项 P2：新系统材质规则覆盖减少透明度实底 / 无模糊降级 | 2 项 P2：同一降级缺口，以及设置页隐藏后续草稿 / 终端退出确认 |
| `95d0448...24f7410` | 0 项 | 0 项；两项原 P2 静态修复成立 |
| `24f7410...6e29f63`（Q201 日志列） | 0 项 | 0 项 |

完整原文分别保存在证据根的 review-standards.md、review-spec.md、review-delta-standards.md、review-delta-spec.md、review-log-standards.md、review-log-spec.md；修复与依据已追加任务 01 Comments。两轴计数独立保留，不能把静态通过写成材质像素通过。

## 真实桌面：已观察

本轮只操作独占 fixture、独立 appdata。`tools/fixtures/setup.sh` 在证据根下生成 fixtures；另建 graph 仓库，含 9 个提交、分叉 / 合并、4 段线性历史、本地分支、远端引用、短标签与 `release/complete-reference-label-without-ellipsis-v0.1.0` 长标签。没有执行 push / fetch / clone / GitHub 写入，未操作真实仓库内容。

验收复制最终 Release 为独立 bundle ID 的 `Revia Native Acceptance.app`、`Revia Css Acceptance.app`，仅改临时副本 Info.plist 并 ad hoc 签名，生产构建未改；副本二进制签名改变后的散列与源二进制散列均记在 native-launch.json / css-launch.json。使用 `REVIA_ACCEPTANCE=1`、独立 native-data / css-data；CSS 额外 `REVIA_GLASS=css`。旧 Revia-glass 调试实例已打开用户真实仓库，保持未改、未退出；本轮仅按精确 PID 和可执行路径关闭自己的 v1 副本，旧包及日志保留。

- 原生模式浅色窗口中，工具栏三组胶囊、独立设置入口、分支图和引用色可见；仓库 / worktree 下拉实际打开、关闭，矩形边界与内容对齐。现有窗口截图只证明结构，不能证明桌面背景折射。分支菜单、窗口 resize 和完整原生区域生命周期尚未全部复验。
- 设置页铺满整窗、主界面隐藏。保留本次 fixture 的活跃终端后打开设置，再按 ⌘Q：终端退出确认完整可见，点击“取消”后设置恢复可操作；由此动态复验了不可见模态修复。未发送任何终端命令。对应截图和 AX 在本轮电脑工具记录；此场景未另存 PNG。
- Q201：分支图左缘圆点完整，线性、分叉、合并线连续；引用独立列，HEAD / main / 远端分支 / 标签均保留名称和胶囊边缘。长标签实际横向滚动后完整显示，未出现省略号；滚到右侧可见主题、作者、日期，表头与内容对齐。截图 native-log-100.png、native-long-labels.png。
- 隔离实例实际切至 150% 界面缩放并回到 100%；两端泳道空间未因缩放改变，表格通过横向滚动容纳。150% 下主区域因详情面板和侧栏而很窄，截图只覆盖首几行 / 部分横向范围，不能称整屏可同时呈现所有列；完整标签布局由 100% 长标签及横向滚动证据支持。native-log-150.png 保存该原始状态，未裁图掩盖限制。
- 原生与 CSS 回退模式均通过真实文件列表打开 base.ts 并排 diff，正文、行号、语法色、增删底色可见；native-diff.png、css-diff.png。只观察 fixture 的实际文本，不据此宣称亮色壁纸上的背景字形已压住。
- 工具输出截图为单窗口，不包含窗口后面的合成桌面。电脑连接两次出现长时间未返回（约 56 分钟、15 分钟），timeout 参数未限制该等待；这些不计应用耗时，不将遮挡 / 未返回画面当应用渲染失败。

## 尚未验证

- 整窗玻璃折射桌面、强亮背景 / 背后有文字窗口时的 --code-veil 遮蔽、整体深色外观，以及系统减少透明度 / 高对比度 / 减少动态效果的真实切换。当前工具仅有单窗口截图，无法补齐合成桌面证据。
- 所有浮动面板（克隆、诊断、更新、PR）、分支菜单、窗口 resize 及各原生玻璃区域卸载 / 恢复的全组验证；本轮只完成仓库 / worktree 菜单和设置退出确认的局部验证。
- 900px 窄窗、Windows 编译与 Mica / Windows 10 / DPI 实机、D06 性能和长测。现有未提交性能工作继续保留，不因本次材质合入而宣称通过。

当前可以复查本地 m7 Release 和上述截图；任务 01 保持 ready-for-human，未完成整套材质与任务 14 验收，未进入 main 合并。
