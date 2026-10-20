# diff 的 hunk 一律来自 Git，diff 视图只读

界面显示的 hunk 由 Rust 层解析 `git diff` 的输出得到。暂存、取消暂存或丢弃某个 hunk 时，补丁就由这同一个 hunk 生成。前端不用 JS diff 库重新计算差异，也不提供“忽略空白”等会改变 hunk 划分的显示选项，以此保证用户看到的就是将要暂存的。diff 视图只读，不能在 Revia 中编辑文件。

## Consequences

- diff 渲染组件必须能渲染外部给定的 hunk。以“传入两份文本、组件自己计算差异”为核心的方案（如 Monaco、CodeMirror 的 diff 组件）不能直接使用。
- 以后做按行暂存时，要在 Git 给出的 hunk 上改写补丁，而不是另算一份差异。
