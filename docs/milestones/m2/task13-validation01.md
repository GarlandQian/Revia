# 任务13：接手修复与本地检查

日期：2026-10-24。当前仅在 `m2-ui` 收尾；用户同意先复核、真实桌面和 D06，通过后合入 `m2`。后续界面以已确认 rev3 / Q107 为准，不另起视觉方案。

## 范围

首轮限定两轴复核固定 `be02cb7...35da167`，完整意见见 [13 Comments](./issues/13-ui-polish.md#comments)。Standards 3 项（最严重 P2，Fowler 判断建议 0）、Spec 2 项（最严重 P2，范围扩张 0）。

- 视图下拉进入生产焦点谓词的隔离范围，防止审阅等全局快捷键穿透。
- 保留新 diff 色调，同时恢复语义单元格类；独立跳转探针对齐并排五节点结构，仍严格检查文字、行号、marker 和 token，不放宽匹配。
- 图标颜色迁入 CSS 变量，保持既定外观；状态字母颜色使用生产枚举后缀。
- 任务及规格残留外观描述对齐 Q107，保留旧意见和旧效果图作为历史。

## 实测检查

[原始记录与逐文件清单](./evidence/task13-review01/manifest.json) 包含命令、退出码、stdout/stderr、源码前后哈希、产品 diff 和驱动脚本。所有检查完成于同一组未提交修复，最终源码与首轮检查前逐文件 SHA256 完全相同。

| 检查 | 真实结果 |
| --- | --- |
| bindings / `tsc -b` / lint / build | 均退出 0；lint 保留现有测试工具的 6 条 unused 警告 |
| Node 内置回归 | 178 通过：IPC 11、jump DOM 11、refresh gate 115、row store 25、UI input 3、visible highlight 13 |
| Python 工具回归 | 20 通过 |
| 完整 Rust | 265 通过 / 1 ignored；lib 101、discard 71、file 20、hunk 24、refresh 15、review 31、session 3 |
| 修复敏感性 | 同两份新回归分别读取旧 `35da167` 模块，均按预期退出 1；不是原生 GUI 复现 |
| 初次差异检查 | 真实退出 2：完整 Rust 后 ts-rs 又生成绑定行尾空白，原错误保留 |
| 受影响复验 | 现有 strip-binding-whitespace 工具规范化后，类型检查、两项新回归、diff-check 均退出 0；全部源码哈希回到首轮检查前原字节，不重跑完整 Rust |

归档暂存检查另发现原始敏感性 stdout 自带行尾空白；改为无损 gzip 存储并核验解压 SHA，不改写原始输出。

新回归使用 Node 内置设施与最小 DOM / HTMLElement 语义夹具，直接调用生产探针和焦点谓词；不是实际 React 渲染、布局引擎或原生键盘输入。真实桌面和 D06 仍是独立门槛。

## 尚未完成

本轮修复尚待固定提交的两轴限定复核；没有新 Release、真实桌面浅深截图、D06、Windows CI 或合并。旧模拟 WebKit 截图不替代真实应用；本页不能作为任务13或整个 M2 的完成结论。08 的既有关闭问题仍另行返修，不归咎本UI增量，也不因本地检查通过而忽略。
