# 04：语法高亮对比（Shiki 与 syntect）

**What to build:** 同一个 diff 视图分别用前端 Shiki 和 Rust 侧 syntect 着色，高亮基于旧、新版本的完整文件，不进入首屏路径；对比速度、正确性、体积和工程复杂度。

**Blocked by:** 03

**Status:** done

- [x] 多行注释、模板字符串着色正确，有单元测试
- [x] Vue 单文件组件与 Swift 着色正确
- [x] 记录两种方案的耗时、token 数与产物体积
