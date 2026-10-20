# 02：Rust 解析 git diff 为 hunk

**What to build:** Rust 层调用系统 git 取得 diff，解析成文件和 hunk（含新旧行号）交给界面；界面不计算差异。

**Blocked by:** 01

**Status:** done

- [x] 固定参数 `--no-color --no-ext-diff --no-textconv --src-prefix=a/ --dst-prefix=b/`，路径前加 `--`
- [x] 正确识别多文件、重命名、二进制、无换行标记
- [x] 不引入 tauri-plugin-shell，前端没有执行命令的入口
