# 任务 05 第五轮限定返修（R05-R5）：实现方原始检查证据

固定源码：`7e5a2c81190461e375ccc02fc6c29b63955d8085`（`05：第五轮限定返修（R05-R5 外部另存结果留存）`），基线 `186e6de495a83e9829eeefd545977cb108dff7fe`。

## 这份证据是什么

`raw/` 是实施方本轮检查留下的原始文件，**逐字节**复制，未修改、未重写、未从摘要重建：

| 文件 | 内容 |
| --- | --- |
| `MANIFEST.md` | 实施方当时的检查清单（含 commit 行，由实施方在提交后追加） |
| `base-sha.txt` | 检查时的仓库 HEAD（基线 `186e6de`） |
| `source-sha256-before-check.txt` | 检查前被测源码哈希 |
| `pre-change.diff` | 改动前的完整 `git diff` |
| `tsc.out` / `tsc.err` / `tsc.exit` | `pnpm exec tsc -b` 的 stdout / stderr / 退出码 |
| `lint.out` / `lint.err` / `lint.exit` | `pnpm lint` 的 stdout / stderr / 退出码 |
| `build.out` / `build.err` / `build.exit` | `pnpm build` 的 stdout / stderr / 退出码 |

`manifest.json` 记录每个文件的原始路径、字节数、原始 SHA-256、存储 SHA-256，以及是否逐字节相同。

## 关于报告里的临时路径

交付说明里写的目录是 `/private/tmp/revia-task05-r5-1792802843`，**该路径不存在**；实际存在的是 `/private/tmp/revia-task05-r5-1792800590`。两个名字只差末尾数字，属实施方笔误。本次复制的是真实存在的那个目录，`manifest.json` 的 `sourceDirectoryNote` 有同样记录。

## 检查前源码哈希与固定源码的对应关系

- 检查前记录：`9e455a51cee6a7558d7b3adf1f87066694c24d851b62c7d20faf9a08083f760a  src/App.tsx`
- 固定源码 `7e5a2c8` 中 `src/App.tsx` 的 blob：`3e27c8d1077124897aff3f6e51e19e143b992ce4`，其内容 SHA-256 同样是 `9e455a51…760a`
- 当前工作区 `src/App.tsx` 也仍是这个哈希

因此本轮 `tsc/lint/build` 所测的正是 `7e5a2c8` 里的那份 `App.tsx`，不存在「测完再改」的情况。

## 边界

- 本轮只跑过 `tsc -b`、`lint`、`build` 三项；Rust 测试、Node/Python 工具测试、Release 构建、GUI、D06、CI、长测均未执行。
- 上述三项的真实退出码见 `raw/*.exit`（均为 `0`），原始输出见 `raw/*.out` 与 `raw/*.err`。
- 证据只覆盖实施方自己的运行；不构成规划方的独立复核，也不代替桌面、D06 或 CI 验收。
