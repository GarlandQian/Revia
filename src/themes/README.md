# 颜色主题与图标

M13 起界面外壳改用 Revia 自研中性灰阶 Design Tokens（[ADR-0016](../../docs/adr/0016-decoupled-design-system-and-solid-surfaces.md)），代码 diff、三方合并与 blame 表面继续沿用本目录保存的 VS Code 颜色主题。

## 代码颜色主题

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
| 界面图标 | `lucide-react`（npm 依赖，Q238） | ISC 许可 |
| 文件类型图标 | pkief.material-icon-theme 5.39.0，复制到 `src/assets/material-icons/` | MIT，见同目录 `LICENSE.txt` |

文件类型图标由 `tools/sync-material-icons.mjs` 从本机扩展同步，只复制映射里用到的 SVG。
