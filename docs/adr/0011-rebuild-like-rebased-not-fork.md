# 参照 Rebased 能力自研，不 fork Rebased

Revia 的核心功能集合与 Git 工作流最初以 Rebased（基于 IntelliJ 平台的开源 Git 客户端）为参照（Q117–Q119），继续用 Tauri + React、由 Rust 层调用系统 Git 自研，而不是 fork Rebased 或直接改用它。M4 外观曾照 VS Code（Q131），M7 曾采用 Liquid Glass（Q189、Q200、Q202）。M13 起松绑与 Rebased 1.1.22 的一比一布局与视觉绑定（Q235–Q244），保留已实现的全部功能与顺手交互，改为自研克制的现代桌面设计系统（见 [ADR-0016](./0016-decoupled-design-system-and-solid-surfaces.md)）。迁到 IntelliJ 插件体系仍会失去审阅标记、回收区、写前 diff 指纹校验和结果待核对，并引入 JVM 运行时；继续自研能保持轻量与独立演进。

## Consequences

- 不再要求界面布局或视觉与 Rebased 1.1.22 逐屏一比一对照；验收以 Revia 自身的视觉设计稿与可用性标准为准（Q236、Q245）。
- 界面外壳采用 Revia 精调的浅/深色实色设计系统，不再使用 Liquid Glass、React Bits 材质组件或玻璃镜片；代码与 diff 区域继续沿用内置或导入的 VS Code 颜色主题（Q235、Q237、ADR-0016）。
- 界面控件图标改用 Lucide，文件类型图标继续保留 Material Icon Theme（Q238）。
- 技术栈、Git 读写由 Rust 层调用系统 Git 命令行（ADR-0002）等既有决定不变。
- 以后若想改为基于 IntelliJ 平台，需要先重新评估本决定。
