# M4 整包交接（给 Pi）

按 Q136，你在 `m4` worktree 中一次实现整份 [M4 规格](./spec.md)（F01–F09），全部做完再交付；规划方在你交付后统一审查与验收，中间不会逐项派发或逐屏确认。

## 工作位置与边界

- 工作目录：`/Volumes/T7 Shield/project/Revia-m4`，分支 `m4`（基于 main）。只在这里改代码、只提交到 `m4`。
- 不推送、不合并、不开 PR，不动主目录 `/Volumes/T7 Shield/project/Revia`。
- 安全边界照旧：写前校验 diff 指纹；丢弃先存回收区；块（hunk）来自 Git；不删 `index.lock`；不改用户的 Git 配置；Git 调用只放在 Rust 层（AGENTS.md“规则”）。
- 不做 M5 及以后的功能，也不放它们的入口或占位按钮（规格“不在 M4”）。

## 先读

1. `AGENTS.md`、`GLOSSARY.md`（界面用词以它为准：hunk 写“块”，颜色主题不叫皮肤）。
2. `docs/README.md` 的 Q117–Q136。
3. [M4 规格](./spec.md)、[UI 方向](../../ui-direction.md)、[产品方向的安全边界](../../product-direction.md#安全边界)。
4. 效果图：[mockups/](./mockups/) 下的 `index.html` 与 `shots/*.png`。这是用户确认过的外观，布局、尺寸、配色以它为准。

## 素材

| 素材 | 位置 | 许可 |
| --- | --- | --- |
| 四套颜色主题的原始文件 | [mockups/theme-sources/](./mockups/theme-sources/)：`2026-light.json` 依次 include `light_modern.json`、`light_plus.json`、`light_vs.json`；`2026-dark.json` 同理；`OneLight.json`；`OneDark-Pro.json` | MIT，见 `mockups/licenses/` |
| 主题键到界面的对应、缺省值 | `mockups/gen-themes.py`、`mockups/themes.css`（效果图用的做法，可以照着实现） | — |
| 界面图标 | npm 包 `@vscode/codicons`（效果图用的是 `mockups/codicon.ttf` 与 `codicon.css`） | 图标 CC BY 4.0 |
| 文件类型图标 | `~/.vscode/extensions/pkief.material-icon-theme-5.39.0/icons/`，扩展名到图标的对应在同一扩展的 `dist/material-icons.json` | MIT |

随应用打包的主题、图标都要保留许可文件，并在“关于 Revia”中注明（规格 F01）。

## 建议顺序

1. F09 后端：多文件暂存、取消暂存、丢弃与逐文件结果；worktree 只读状态。补 Rust 测试。
2. F01 视觉基础：颜色主题的加载、合并 include、设计变量、跟随系统切换与持久化；系统界面字体与可设置的代码字体；Codicons 与文件类型图标。
3. F02 主窗口框架 → F03 提交工具窗口 → F04 提交区 → F05 diff 页签 → F06 回收区页签 → F07 提示与通知 → F08 设置。
4. 更新测量与长测脚手架（它们按 DOM 找元素），保证 D06 测量和长测仍能运行。

每完成一个 F 提交一次，提交说明写“M4 F0x：<内容>”，结尾加 `Co-Authored-By` 行（见 AGENTS.md）。

## 交付前自查

在 `m4` worktree 中运行（命令见 AGENTS.md“开发与构建命令”）：

```bash
pnpm install
pnpm exec tsc -b
pnpm lint
pnpm build
/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml
for t in tools/test-*.mjs; do node "$t"; done
```

改了 Rust 类型后运行 `pnpm bindings` 重新生成 TS 绑定。

## 交付

- 写 `docs/milestones/m4/report.md`：每个 F 的实现位置、自查命令与结果（失败的如实写）、没做或与规格不同的地方及原因、规格没覆盖而你自己做的取舍。
- 遇到规格没写到的取舍，不要停下等人：选最接近效果图外观、Rebased 布局的做法，写进报告。
- 桌面截图、D06 实测、与 Rebased 的并排对照由规划方验收时做，你不需要做。
