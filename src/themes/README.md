# 颜色主题与图标

M4 起外观照 VS Code（Q131）。本目录保存随应用打包的颜色主题与图标素材及其许可。

## 颜色主题

`themes.css` 由 `tools/gen-themes.mjs` 从 `sources/` 里的 VS Code 主题文件生成，
每套主题一个 `[data-theme="<id>"]` 块：

| id | 名称 | 外观 | 来源 |
| --- | --- | --- | --- |
| `one-light` | Atom One Light | 浅色（默认） | akamud/vscode-theme-onelight（MIT） |
| `light-2026` | VS Code Light 2026 | 浅色 | VS Code 1.143.1 `extensions/theme-defaults`（MIT） |
| `one-dark-pro` | One Dark Pro | 深色（默认） | zhuangtongfa.material-theme 3.20.3 / Binaryify/OneDark-Pro（MIT） |
| `dark-2026` | VS Code Dark 2026 | 深色 | VS Code 1.143.1 `extensions/theme-defaults`（MIT） |

`sources/` 里还保留了 VS Code 默认主题的 include 链（`light_modern.json`、
`light_plus.json`、`light_vs.json` 等）。许可文件在 `licenses/`。

## 图标

| 用途 | 来源 | 许可 |
| --- | --- | --- |
| 界面图标 | `@vscode/codicons` 0.0.46-24（npm 依赖） | 图标 CC BY 4.0，见 `licenses/codicons-CC-BY-4.0.txt` |
| 文件类型图标 | pkief.material-icon-theme 5.39.0，复制到 `src/assets/material-icons/` | MIT，见同目录 `LICENSE.txt` |

文件类型图标由 `tools/sync-material-icons.mjs` 从本机扩展同步，只复制映射里用到的 SVG。
