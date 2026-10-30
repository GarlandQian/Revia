# 11：确认下载并安装应用更新

Status: ready-for-agent
Blocked by: 03, 04

## What to build

启动检查稳定 / 可选预发布，在明确下载、处理当前工作并再次确认后安装重启。

依据：规格 H；Q181、Q182。

## Acceptance criteria

- [x] 检查 / 下载 / 已下载 / 安装 / 错误状态分开，取消不关闭，离线失败保留当前应用
- [x] 生产来源与公钥受信任，签名必须核验；未配置说明原因，不以测试值冒充生产
- [x] 安装协调草稿 / 合并编辑 / Git 写入 / 结果待核对 / 终端，处理失败不安装
- [x] 独占测试目录验证好坏签名、损坏 / 网络失败、取消与关闭门禁；不替换真实运行应用
- [x] 实际签名更新和首次下载启动交任务 14 / 外部接线，未执行要明确记录

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F13 P1 | “分开”“安装前协调”：下载即安装、绕门禁、重启按钮不可达 | src-tauri/src/m7/update.rs:260,293；src/features/m7/UpdatePanel.tsx:145 |

## F13 [P1] 更新下载即安装并绕过关闭协调，安装后无重启入口

规格 S122：“检查、下载、已下载待安装、安装 / 重启、失败状态分开，用户确认后下载和安装”；S123：“安装前协调未保存编辑、合并编辑、在途 Git 写入、结果待核对与运行中终端，再确认重启”。

`src/features/m7/UpdatePanel.tsx:73–90` 一次点击updateInstall；`src-tauri/src/m7/update.rs:260–278` 直接download_and_install，无下载待安装/取消状态；App5361–5364仅flushM7State并return true，既有CloseDriver没参与；commands691直接app.restart。安装后update.rs293清available，但UpdatePanel145重启按钮位于131 `status.available` 分支内，所以成功后永不可见。触发：正式来源接好后按更新安装；可边Git写入/终端运行边进入。结果：未经第二次确认安装，工作未协调、无法按界面重启。当前未接正式源，未实际安装；属于配置可用后确定的控制流缺陷。

| F22 P2 | “主动开启预发布”：无渠道选择，另增无效automatic方式 | src-tauri/src/settings.rs:123；tools/check-release-version.mjs:53 |

## F22 [P2] 缺稳定/预发布选择，增加无效更新方式

规格 S122：“默认稳定版；用户主动开启预发布”；S126：“公开后才进入所选渠道”。

`src-tauri/src/settings.rs:123–139` 只有Manual/Notify/Automatic；`settings.tsx:379–432` 同样无预发布开关；`update.rs:207,247`始终构建固定配置endpoint，`tools/check-release-version.mjs:53–58` 预发布端点还固定为当前tag，后续beta无法发现。Automatic文案说自动下载但303–328只检查。触发：稳定版用户想主动订阅预发布或选自动下载。结果：没有所需渠道，新增方式无对应行为，偏离已确认取舍。无部署前提能补齐本地缺失字段/分派。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。
