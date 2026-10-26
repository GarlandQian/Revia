# 照 Rebased 自研，不 fork Rebased

Revia 的功能和界面以 Rebased（JetBrains Git 功能单独发布的客户端）为照抄对象（Q117–Q119），但继续用 Tauri + React、由 Rust 层调用系统 Git 自研，而不是 fork Rebased 或直接改用它。Rebased 基于 IntelliJ 平台和 JVM，启动和内存开销远大于 Tauri 应用；Revia 已经实现的审阅标记、回收区、写前 diff 指纹校验和结果待核对，若迁到 IntelliJ 插件体系需要全部重做，M0–M3 的成果也会作废。代价是 Rebased 的 Git 功能要逐项重写，进度远慢于 fork；外观的一比一要靠与 Rebased 并排对照来维持，Rebased 升级后不会自动跟随。

## Consequences

- 界面与交互的验收以本机安装的 Rebased 1.1.22 为基准：每一屏与它并排截图对照（Q117、Q125）。
- 配色、尺寸和图标直接取自 IntelliJ 社区版的开源主题与图标（Apache 2.0），按其许可保留版权与许可声明。
- 技术栈、Git 读写由 Rust 层调用系统 Git 命令行（ADR-0002）等既有决定不变。
- 以后若想改为基于 IntelliJ 平台，需要先重新评估本决定。
