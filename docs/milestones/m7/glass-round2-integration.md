# M7 第二批玻璃镜片与卡片集成验证

日期：2026-11-01。规划方：Codex。第二批 Q202 已合入 m7；构建、相关检查和局部真实桌面验证通过，完整材质验收未完成。任务 01 / 14 不据本记录改为 done。第一批 Q200 / Q201 结果见 [原生玻璃合并验证](glass-integration.md)。

## 固定源码与工作区

- 接手基准 m7 `3f279cc3de2c7103bc09452f47989132a8f53d7d`；m7-glass 第二批 9 个文件固定为 `7c74dbdebdd9b65414123e603a8d7c779c3c8284`，在 `0784e9b244411ad4e3c624112d1c09462f68634b` 合入 m7。
- 唯一冲突为 docs/README.md 在 Q200 后插行，保留 Q201 与 Q202 两行及各自决策树条目。Q201 的独立引用列、标签完整宽度、连续分支图和整表横向滚动保留。
- `3101564c965eab3e56c89d26f03366d75d684f4a` 同步规格、任务与 P3 记录，并修复 macOS CSS 材质误选 SVG 背景滤镜；`429d2f6608099871824e9bce931b99b3d745e426` 只调整现有测试对 JSX 空白的断言。
- 最终 Release 来自 `429d2f6` 加接手时已有 6 个未提交文件，不能称为仅由 HEAD 构建。原有任务 14、native-acceptance.md、git/mod.rs、SideBySideView.tsx、sideGeometry.ts、test-sync-scroll.mjs 的 SHA256 与接手时逐字节一致，未纳入本轮提交。
- 独占证据目录：`/private/tmp/revia-m7-glass-round2-jej71x5e`。before.json、before/、两份 before.patch 保存原件；release-source.json 保存构建 HEAD、未提交状态和 7901 个 tracked 文件的散列，构建后及写本记录前核对均无变化。最终收尾记录另存 final-check.json。

## 检查结果

| 命令 | 结果 | 原始证据前缀 |
| --- | --- | --- |
| `pnpm tauri build --bundles app` | exit 0，184.9 秒；包含 tsc -b、Vite、Rust Release 和 .app 打包；构建前 touch lib.rs，嵌入最新前端 | release |
| `pnpm exec oxlint src/components/app/EditorTabs.tsx src/components/app/SettingsDialog.tsx src/components/app/CommitComposer.tsx src/components/ui/popover.tsx src/features/m7/glass/glassLens.ts src/features/m7/glass/GlassLayer.tsx` | exit 0，零告警 | lint |
| `node --test tools/test-log-model.mjs tools/test-editor-tabs.mjs tools/test-ui-input.mjs tools/test-shortcuts.mjs` | 初次 73/74 通过；唯一失败为旧断言要求 Positioner 与 className 同行，与 JSX 格式变化有关 | node |
| `node --test tools/test-log-model.mjs` | 将旧断言改为允许换行后，受影响日志套件 49/49 通过；其他 25 项此前已通过，未重复整个套件 | log-retest |

本轮无 Rust 产品修改、无翻译键或语言文件修改，未重复第一批玻璃绑定测试、全量 Rust、全量 lint 或引入新测试框架。记录保留首次失败原件。Windows 编译与真机未运行，按 Q202 留 M8；D06 不在本轮局部材质检查中冒称通过。

## 审查

使用 code-review 的 Standards / Spec 两轴；Q174 规定只审本批增量，P3 登记 backlog。

| 固定范围 | Standards | Spec |
| --- | --- | --- |
| `3f279cc...7c74dbd`，merge-base `262bd47` | 硬标准违反 0、smell 0；另 1 项 P3 | P1/P2 0，无范围扩张；2 项 P3 |
| `0784e9b...3101564` | 独立增量审查 0 发现 | 独立子代理未实际完成；由根代理直接复核，未发现新增遗漏、范围扩张或实现错误，不记作独立 Spec 通过 |
| `3101564..429d2f6` | 根代理核对仅为现有测试空白断言修正 | 产品代码无变化 |

初轮原文在 review-standards.md / review-spec.md；Standards 增量原返回转录在 review-delta-standards.md。根代理 Spec 复核依据为 Q200 / Q202、规格 A 与交接中的 WKWebView 回退问题：GlassLayer 复用既有 isMac，在 macOS 令 refraction=false 走 vendor 原有 frosted 路径，仍保留 flat 条件；Windows 路径和使用普通 filter:url() 的独立镜片均未改。规格与任务仅同步已确认内容，新增桌面验收保持未勾选。此复核是源码结论，不是背景模糊像素已验收。

3 项 P3 已分别登记 [backlog 16](../backlog/16-m7-glass-lens-details.md)：动画启动后切辅助功能偏好可能最多残留约 680ms、设置镜片快照内距偏差 2px、搜索期间短暂残留旧导航快照。三项均为静态观察，不声称已在真实桌面复现。

## 真实桌面已观察

仅操作本轮自建 graph fixture 与独立 native-data / css-data。没有执行真实仓库操作、终端命令或网络写入。生产 Release 复制为独立 bundle ID 的临时 Revia Native Round2.app，临时副本 ad hoc 签名；源二进制及签名后副本散列分别在 native-launch.json，CSS 模式和 PID 在 css-launch.json。

- 原生浅色模式：侧栏和编辑区浮动圆角卡片、卡片间空隙、顶部胶囊与卡片底部的状态栏可见；日志中独立引用列和连续分支图保留。证据 native-layout.png / .ax.txt。
- 提交选项实际在按钮上方弹出，留有窗口底部空间；未切换跳过 hooks 等选项。证据 native-commit-options.png / .ax.txt。单次截图不证明所有窗口位置都满足 10px，也未据此勾选完整焦点恢复验收。
- 设置页原生模式铺满窗口，新平铺图标导航、页面大标题和内容卡片可见；切到语言页成功。搜索“快捷键”后只保留匹配导航，实际可进入快捷键页；未改语言或快捷键。证据 native-settings.png、native-settings-switch.png、native-settings-search.png 及相应 AX。
- 原生模式 base.ts 并排 diff 的正文、行号和增删色可辨认。证据 native-diff.png / .ax.txt；未取得亮色壁纸和背后文字窗口的合成画面，不能证明卡片已压住所有背景字形。
- `REVIA_GLASS=css` 下启动与日志布局正常，仓库弹层可打开；设置页仍是居中对话框，平铺图标导航显示正常，切语言页及关闭返回日志成功。证据 css-layout.png、css-popover.png、css-settings.png、css-settings-switch.png 和 css-settings-closed.ax.txt。应用名称仍为 Revia Native Round2，实际 CSS 模式以 css-launch.json 为准；截图不证明精确背景模糊像素。
- 原生导航切换截图主要捕获落定状态，未充分捕获放大、弯折、色散的中间帧；不将实施方此前调试版逐帧报告或独立 WKWebView 实验转写成本轮产品验收通过。

以上均为单窗口截图，不能据此评价窗口背后桌面的真实玻璃合成。

## 未完成的验收

- 页签与设置导航镜片的完整中间帧、快速连续切换与当前固定 Release 的光学效果。
- 系统深色、减少透明度 / 减少动态效果 / 强制颜色的实际切换。此前已请求临时更改系统偏好并恢复的许可，尚未收到答复，因此没有切换系统设置。
- 真实桌面合成、强亮背景与背后文字的遮蔽、窗口 resize、窄窗以及完整焦点 / 键盘和全部浮层生命周期；第一批记录里的未验项未被本批局部验证替代。
- Windows M8、D06 和完整长测；发布接线待决项也不因本次合入完成。

当前结论为已集成、相关检查通过、局部桌面行为已核对；任务 01 保持 ready-for-human，任务 14 的原有未提交状态保留。未推送、未开 PR、未合 main。
