# 目标平台：macOS Apple Silicon 与 Windows x64

Revia 最终提供三种产物：macOS Apple Silicon 版、Windows x64 安装版、Windows x64 便携版。首版只发布 macOS，Windows 放在首版验证通过之后的 M5。明确不做 Intel Mac、ARM 版 Windows 和 Linux：Tauri 虽然都能打包，但每多一个平台，就多一份 hooks 运行环境、路径、换行符、WebView 性能和安装方式要验证；个人项目的精力应该集中在开发者自己和目标试用者实际使用的平台上。ARM 版 Windows 设备可以通过系统转译运行 x64 版。

## Consequences

- 从 M2 起，平台相关的代码集中放在少数模块中，其他代码不写死 macOS 假设；GitHub Actions 每次推送都在 Windows 上编译检查，避免到 M5 才发现大量需要改写的地方。
- 界面采用“共享设计 + 平台外壳”：布局、密度和配色各平台一致，只有窗口按钮、字体、侧栏材质、修饰键等跟随系统。
- 以后如果要增加平台，需要先重新评估本决定，而不是直接在构建配置里多加一个目标。
