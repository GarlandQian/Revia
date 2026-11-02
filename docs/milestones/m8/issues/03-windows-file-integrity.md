# 03：Windows 路径与换行完整性

Status: ready-for-human
Blocked by: None（可立即开始）

## What to build

用户在含中文、空格等 Windows 路径与 CRLF 仓库中审阅、暂存、保存和恢复时保留文件与 Git 语义。

## Acceptance criteria

- [x] 核对既有 Windows 文件身份、canonical/external 路径转换与大小写别名，修复真实缺口。
- [x] 中文、空格及特殊字符以独立参数传给 Git；盘符、UNC/verbatim 在正确边界处理。
- [x] 沿真实 Git 测 LF/CRLF、gitattributes/core.autocrlf 下的暂存、丢弃/恢复与编辑保存，不改用户 Git 配置。
- [x] 所有写入口仍核对身份与指纹、锁占用和回收语义；不为兼容而放宽路径边界。
- [x] 复用现有 Rust 集成测试，只增加可回归的真实缺陷与契约检查；不凭主观猜测大范围重构。

## Comments

实施说明（`a30b435`、`af869f9`）：

- 抽出 `platform::external_path_text`，新增 verbatim / UNC / 盘符纯文本回归；Windows 文件
  身份与 canonical 比较沿用既有实现，未放宽任何路径边界。
- 新增 `src-tauri/tests/windows_line_endings.rs`（真实 Git、隔离 HOME / 全局配置）：
  `eol=crlf` 下暂存保持工作区字节且 index 归一 LF；丢弃恢复 CRLF 并先写回收区；
  编辑保存保留 CRLF。中文 + 空格路径在既有 `file_staging` 测试中继续通过。
- 未在真实 Windows 上验证盘符 / UNC / 大小写别名与 IME 路径，待任务 07。
