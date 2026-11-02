# 0.0.2 打包修正 01

Status: verifying
日期：2026-11-02
应用源码：`v0.0.2` → `4c384ed4b3452ac1713aa3baed5dccaef2213b39`（main 历史，双平台 CI 通过）。

## 问题与依据

Release Draft [41453450255](https://github.com/GarlandQian/Revia/actions/runs/41453450255) 的 macOS job 通过；Windows 便携 exe 构建成功，但依赖 gate 将 `api-ms-win-crt-*` 视为额外运行库，停止后续 NSIS 构建与草稿汇总。失败输出保留在 `/private/tmp/revia-m8-publish-tzrt7vd_/release-failure-01.stdout`，实际 dumpbin 依赖区段为 `portable-dependencies-actual.txt`。

[Microsoft Universal CRT deployment](https://learn.microsoft.com/en-us/cpp/windows/universal-crt-deployment?view=msvc-170)（本轮读取）确认 UCRT 与这些 APISet 属于 Windows 10/11 自带的 OS 组件。Revia 已确认支持这些系统（Q200），不需要额外安装或随包复制 UCRT。此次纠正分类，不改变 Q58/Q204 的便携数据边界，不承诺 Windows 7/8 支持。

## 修正范围

- 保留 Rust `+crt-static`；依赖 gate 允许系统 UCRT，继续拒绝需要额外安装的 `VCRUNTIME*`、`MSVCP*`、`MSVCR*` 与动态 WebView2Loader，补足 `_1`、debug、atomic_wait 等变体。没有有效依赖列表或 dumpbin 非零退出时失败。
- 既有版本标签保持固定。工作流增加手动指定已有标签的重建入口，所有应用 checkout 使用完整 `refs/tags/<release_tag>`；应用来源与工作流/发布工具来源分别校验均属于 main 历史。构建、清单和草稿统一使用该标签的版本。
- 发布工具以工作流事件的固定 SHA 单独 checkout 到 `.release-tooling`；verify job 运行该工具版本的 Node 回归，Windows 用该工具核对实际 exe。只有发布工具可以从已修正的 main 取得，应用代码始终来自原标签。
- 权限不扩大，产物仍只进入草稿，公开与旧 Release 清理由既有 Q207 授权执行。应用源码未变，不再重复触发其完整 Rust 检查；工具增量运行相关检查，随后手动构建将再次执行来源、版本与产物门禁。

GitHub 依据：[checkout v4 的 ref / fetch-depth](https://github.com/actions/checkout/blob/v4/README.md)、[workflow_dispatch 输入与事件 SHA](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#workflow_dispatch)。主线工作流的事件 SHA 固定发布工具，应用的 checkout ref 固定原标签，避免将一次打包修正冒充应用源码更新。

## 验证

- 相关来源、清单与依赖检查 13/13 Node 测试通过，新增系统 UCRT、额外运行库变体与缺失输出回归。
- 用失败 job 保存的实际 dumpbin 32 项依赖运行修正后生产检查器通过，没有额外 VC++ runtime 或 WebView2 loader。
- 工作流 YAML 语法解析通过（4 个 jobs），`git diff --check` 通过；actionlint 当前不可用，没有冒称其检查通过。
- macOS 已下载产物的 3 项 SHA256 通过；DMG 内部校验有效，更新包确认为 arm64 / 0.0.2。此后仍须核对最终草稿的全部产物与签名。
- 后续实际工作流重建结果待补录；Windows 真机验收继续 ready-for-human。
