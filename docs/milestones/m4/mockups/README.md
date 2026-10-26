# M4 效果图：Rebased 的布局，VS Code 的外观

用户于 2026-10-26 确认（Q131–Q134）。同一屏按 Rebased 的布局（主工具栏、工具窗口图标条、提交工具窗口、diff 页签、状态栏）画，外观照 VS Code，分别套用四套内置颜色主题：

| 截图 | 颜色主题 | 用途 |
| --- | --- | --- |
| [light-2026.png](./shots/light-2026.png) | VS Code Light 2026 | 浅色可选 |
| [one-light.png](./shots/one-light.png) | Atom One Light | 浅色默认 |
| [dark-2026.png](./shots/dark-2026.png) | VS Code Dark 2026 | 深色可选 |
| [one-dark-pro.png](./shots/one-dark-pro.png) | One Dark Pro | 深色默认 |

[sheet.png](./shots/sheet.png) 是四张的对比图。`index.html` 是效果图源文件，`?theme=<名字>` 切换主题；`themes.css` 由 `gen-themes.py` 从下列主题文件生成。

## 来源与许可

| 内容 | 来源 | 许可 |
| --- | --- | --- |
| VS Code Light 2026、Dark 2026 | VS Code 1.143.1 自带的 `extensions/theme-defaults`（源码仓库 microsoft/vscode） | MIT，[licenses/vscode-MIT.txt](./licenses/vscode-MIT.txt) |
| One Dark Pro | VS Code 扩展 zhuangtongfa.material-theme 3.20.3（Binaryify/OneDark-Pro） | MIT，[licenses/one-dark-pro-MIT.txt](./licenses/one-dark-pro-MIT.txt) |
| Atom One Light | akamud/vscode-theme-onelight | MIT，[licenses/atom-one-light-MIT.txt](./licenses/atom-one-light-MIT.txt) |
| 界面图标 `codicon.ttf`、`codicon.css` | @vscode/codicons 0.0.46-24 | 图标 CC BY 4.0，[licenses/codicons-CC-BY-4.0.txt](./licenses/codicons-CC-BY-4.0.txt) |
| 文件类型图标 `icons/` | VS Code 扩展 pkief.material-icon-theme 5.39.0 | MIT，[licenses/material-icon-theme-MIT.txt](./licenses/material-icon-theme-MIT.txt) |

效果图中的“暂存此 hunk”“丢弃此 hunk”画于 Q135 之前；实现时写“暂存此块”“丢弃此块”。
