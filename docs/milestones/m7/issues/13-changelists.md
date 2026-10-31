# 13：按更改列表选择并提交

Status: needs-info
Blocked by: None

## What to build

在 Rebased 实测解阻后，提供互斥的更改列表模式并保证勾选内容与真实提交一致。

依据：规格 I；Q120、Q197、ADR-0003。

## Acceptance criteria

- [ ] 先取得 Rebased 1.1.22 在独占 fixture 中的实际观察：预先暂存、同文件部分暂存、切模式、只选其他块提交、完成后 index / 工作区；保存原 argv / exit / 输出与截图
- [ ] 规划方把实测语义补入规格和 Comments 后才能写 index / 提交路径；不能从静态实现、推测或 mock 补全
- [ ] 按 Q197 实现默认列表、单一归属、文件 / Git 块移动、删组回默认与提交后保留未提交部分
- [ ] 块失配保留分组信息、取消失效勾选，切模式保留列表；写前校验和数据保护门槛不变
- [ ] 真实 Git 断言提交树、index 与剩余工作区完全对应；实现细节另经审查

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

- Codex，2026-10-30：当前无桌面操控工具，AXIsProcessTrusted() 为 false，Rebased 事实前提尚未取得。此任务保持 needs-info，Pi 不猜测提交语义；任务 01–12 可推进。


### Codex 固定Release与真实验收停点 — 2026-10-31

两轴审修已通过后，以源码`be8f9d882b59546a3a1e905812f92a5222dde086`（产品`628fff93cf19ce1f4e61f329857ed165e087b117`）运行`bash tools/build-app.sh release`，`codex-fixed-release-build-01`真实exit0。产物为macOS arm64 Release二进制，固定副本`/private/tmp/revia-m7-batch-g5scyw0r/fixed-release-jwgkkh07/revia`，SHA256 `b1a4c3d7da2840d97ff3b8e8dffe14c136213f2ab915ca9ae8c77ca28ae1bb38`。前端21个文件分别散列并归档；完整元数据`/private/tmp/revia-m7-batch-g5scyw0r/fixed-release-jwgkkh07/release.json`，包含原始构建记录、前端归档散列与源码对应关系。构建后全部源码hash未变。

未启动产品、未生成正式安装包或接触发布签名。当前仍无原生桌面工具，AXIsProcessTrusted=false，原证据`review-03-zika9m9t/desktop-capability.json`。任务13仍须在独占fixture中真实观察Rebased1.1.22既有暂存、部分暂存及提交后index/工作区，再补规格并实施；不能从Git猜测或模拟IPC决定。任务14的真实四主题/双语900px、D06、编辑终端、GitHub/诊断/更新等仍未验。没有将编译或测试称作桌面验收通过。

进入awaiting-native-desktop；当前可独立完成的源码审修和Release固定已完成，13/14保留阻塞。未push/PR/tag/发布、未合main/删除worktree；formalRepairUsed=true、Pi继续idle，监控保留。源码不变时不重复检查或启动代理；取得真实桌面能力后才续接约定验收。
