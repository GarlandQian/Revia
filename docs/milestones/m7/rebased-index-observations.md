# Rebased 既有暂存内容实测

### Codex：Rebased 1.1.22 原生事实解阻（2026-10-31）

实际宿主 ChatGPT 获用户辅助功能授权后，提升执行上下文 AX=true、截图=true。只操作本批独占 Rebased PID80019；HOME、config/system/plugins/log 均由实际 idea.log 核对隔离。未操作用户仓库、配置或凭据。

1. `existing-staged`：预先暂存 existing.txt，仅在真实 Commit 界面勾选 other.txt 提交，生成 `99e0f8bcfd6482b8c0ea4e806de3044aeb7e2466`。提交只含 other.txt；existing.txt 原暂存保留，工作区未变。
2. `same-file-partial`：mixed.txt line4 已暂存、line30 未暂存；真实 diff 只勾 line30（“2 differences, 1 included”）后提交 `d7e0e5bc03a9540a642a1d204009699365ba7f5d`。提交只含 line30；该路径 index 对齐新 HEAD，原先未选 line4 内容保留在工作区并变为未暂存。other.txt 未提交且内容保留。
3. `mode-switch`：通过真实 Settings → Version Control → Git 的 Enable staging area，切到 Staged/Unstaged 再切回 Changes。前后 HEAD、index 条目、cached/working diff 与三个测试文件字节完全相同。切换本身不改 index/工作区。

原件根 `/private/tmp/revia-m7-batch-g5scyw0r/desktop-acceptance-myntirt_`：fixture-manifest.json、task13-observations.md、mode-switch-comparison.json；逐命令 argv/exit/stdout/stderr 与原字节在 evidence/{existing-staged,same-file-partial,mode-switch}，所有真实截图在 evidence。启动原件 task13-rebased-launch-01 仍是运行中进程，不伪造最终退出码。其余命令原件在批次 codex-checks。

据以上真实事实补入规格 I 后，解除任务13的事实前置；尚未把任务13实现或任务14验收记为通过。由 Codex 直接实现，不再次派 Pi；formalRepairUsed=true。
