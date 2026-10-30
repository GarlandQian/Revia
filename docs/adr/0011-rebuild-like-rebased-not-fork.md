# 照 Rebased 自研，不 fork Rebased

Revia 的布局和功能以 Rebased（基于 IntelliJ 平台的开源 Git 客户端）为参照（Q117–Q119），继续用 Tauri + React、由 Rust 层调用系统 Git 自研，而不是 fork Rebased 或直接改用它。M4 外观照 VS Code（Q131）；M7 按用户新要求改为 Liquid Glass，颜色主题继续沿用，代码与 diff 清晰度优先（Q189、Q190）。迁到 IntelliJ 插件体系需要重做审阅标记、回收区、写前 diff 指纹校验和结果待核对，也会引入 JVM 运行时；继续自研的代价是逐项实现 Git 功能，并与 Rebased 对照布局和交互，升级后不会自动跟随。

## Consequences

- 布局与交互的验收以本机安装的 Rebased 1.1.22 为基准：每一屏与它并排截图对照（Q117、Q125）。
- 外层采用 Liquid Glass，代码区保持清晰、不透明的表面；配色来自 VS Code 颜色主题（Q132、Q189、Q190），界面图标用 Codicons，文件类型图标用 Material Icon Theme（Q130）。React Bits 可按需用于材质层，按实际引入源码的许可保留声明；它不替代已有功能控件。
- 技术栈、Git 读写由 Rust 层调用系统 Git 命令行（ADR-0002）等既有决定不变。
- 以后若想改为基于 IntelliJ 平台，需要先重新评估本决定。
