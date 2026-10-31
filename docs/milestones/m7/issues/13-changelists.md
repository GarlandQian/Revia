# 13：按更改列表选择并提交

Status: ready-for-agent
Blocked by: None

## What to build

在 Rebased 实测解阻后，提供互斥的更改列表模式并保证勾选内容与真实提交一致。

依据：规格 I；Q120、Q197、ADR-0003。

## Acceptance criteria

- [x] 先取得 Rebased 1.1.22 在独占 fixture 中的实际观察：预先暂存、同文件部分暂存、切模式、只选其他块提交、完成后 index / 工作区；保存原 argv / exit / 输出与截图
- [x] 规划方把实测语义补入规格和 Comments 后才能写 index / 提交路径；不能从静态实现、推测或 mock 补全
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

### Codex：Rebased 1.1.22 原生事实解阻（2026-10-31）

实际宿主 ChatGPT 获用户辅助功能授权后，提升执行上下文 AX=true、截图=true。只操作本批独占 Rebased PID80019；HOME、config/system/plugins/log 均由实际 idea.log 核对隔离。未操作用户仓库、配置或凭据。

1. `existing-staged`：预先暂存 existing.txt，仅在真实 Commit 界面勾选 other.txt 提交，生成 `99e0f8bcfd6482b8c0ea4e806de3044aeb7e2466`。提交只含 other.txt；existing.txt 原暂存保留，工作区未变。
2. `same-file-partial`：mixed.txt line4 已暂存、line30 未暂存；真实 diff 只勾 line30（“2 differences, 1 included”）后提交 `d7e0e5bc03a9540a642a1d204009699365ba7f5d`。提交只含 line30；该路径 index 对齐新 HEAD，原先未选 line4 内容保留在工作区并变为未暂存。other.txt 未提交且内容保留。
3. `mode-switch`：通过真实 Settings → Version Control → Git 的 Enable staging area，切到 Staged/Unstaged 再切回 Changes。前后 HEAD、index 条目、cached/working diff 与三个测试文件字节完全相同。切换本身不改 index/工作区。

原件根 `/private/tmp/revia-m7-batch-g5scyw0r/desktop-acceptance-myntirt_`：fixture-manifest.json、task13-observations.md、mode-switch-comparison.json；逐命令 argv/exit/stdout/stderr 与原字节在 evidence/{existing-staged,same-file-partial,mode-switch}，所有真实截图在 evidence。启动原件 task13-rebased-launch-01 仍是运行中进程，不伪造最终退出码。其余命令原件在批次 codex-checks。

据以上真实事实补入规格 I 后，解除任务13的事实前置；尚未把任务13实现或任务14验收记为通过。由 Codex 直接实现，不再次派 Pi；formalRepairUsed=true。

### Codex：任务13实现与集中检查（2026-10-31）

基于上方三组真实 Rebased 事实实现更改列表；设置默认仍为暂存区。应用数据按 worktree 身份保存默认列表、创建/重命名/删除、整文件/原始 Git 块单一归属与勾选；切换不删组。失配块保留原组和预览，取消勾选。文件与块均可移动，提交成功只移除本次已提交条目。

选择提交使用系统 Git 生成的 HEAD→工作区原始块建立独立 index，执行前再次核对真实 HEAD/index/操作状态/文件版本及 worktree 身份；沿用现有提交输出、快照、hooks、草稿和退出保护。成功后只用 Git reset 更新所选路径的真实 index，未触及路径的原暂存不混入提交且保留，工作区不被写入。原 index entries/binary patch、选项及独立 index 原件保存在应用数据 changelists/commit-evidence；失败不自动续跑、不删 index.lock。收尾失败不清草稿，报告待核对。所选文件的编辑草稿经现有入口处理；不要求处理未选文件的草稿。

`task13-rust-03`：42通过（新更改列表10、既有commit_flow12、file_staging20），覆盖不同路径已有暂存、同文件部分暂存、二进制/未跟踪/删除、归组持久化与默认组、外部修改失配、锁保留、hook失败、初始提交和仓库身份替换。此前task13-rust-01为40通过，02为42通过；所有轮次保留。`task13-frontend-01`、`task13-lint-01`、`task13-x18n-01`均exit0，x18n 0错误/76既有unused-key警告；lint有既有非阻断提示。x18n首次沙箱缓存写入EPERM的原件在task13-locale-query-01，提升后查询/维护通过。绑定生成task13-bindings-01通过。

后两次产品变化仅Rust收尾与命令相关编辑范围，已重跑上述Rust组；前端与locale检查后无对应源码变化。不把这些CLI检查称作桌面通过，Windows未实跑。下一步固定本增量、按Q174新两轴审查，再重建Release和任务14真实桌面；旧Release不适用于本次新源码。
