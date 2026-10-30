# 12：标签打包至 Release 草稿

Status: done
Blocked by: None

## What to build

增加版本标签构建 macOS Apple Silicon 安装包与 updater 产物的工作流，先保存为草稿。

依据：规格 H；Q62、Q63、Q65、Q182、Q188。

## Acceptance criteria

- [x] main 推送仅检查；标签触发构建 / 分发产物，Release 始终 draft，保留 Windows 编译检查
- [x] 应用版本、标签、架构、安装包命名、更新 URL 和签名对应；稳定 / 预发布渠道正确
- [x] 更新私钥经 GitHub secret 注入，不进源码 / 日志；无 Apple 证书仍按既定分发方式
- [x] 补准确的 App 注册 / 更新签名 / secret / 首次打开说明，验证配置和本地产物；不推送、打标签或创建实际 Release
- [x] 工作流存在 / 本地构建不称为 Actions 实际运行，真实 CI 与分发证据留最终验收

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments
