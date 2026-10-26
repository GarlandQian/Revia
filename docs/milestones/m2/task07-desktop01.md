# 任务07首轮真实桌面：旧刷新覆盖新审阅状态，未通过

2026-10-24；本记录是规划方实际观察，不是计划或实施方叙述。

## 固定版本与范围

- 源码：`8f95c26c33a4c2883043a0d4041b87c00c763cc6`；产品源码逐项匹配已复核的`86b26aa`。
- Release：`pnpm tauri build --bundles app`，原始退出0；二进制SHA256 `bed7f32947ef4d897918c164b788b108a3095d95c309e7f7bd91d5ec6ac41e5f`。
- 普通隔离实例PID24560，数据`/private/tmp/revia-task05-desktop-9ixADj/revia-data`。启动环境仅acceptance，不设REVIA_MEASURE_LOG/MEASURE/SMOKE/SOAK_SECONDS；未运行自动测量或D06。
- fixture经现有ignored生产准备入口新建，5条回收记录由生产丢弃入口产生。准备通过不等于桌面恢复通过。
- GUI仅CUA。截图在工具会话中，没有生成本地PNG，不在证据清单伪造截图文件。

## 实际观察

1. 普通模式首屏没有测量面板、工具栏Git版本/文件数或A-/A+，审阅进度仍在文件列表中。这只覆盖12初始普通呈现；字号、重启、测量模式、环境错误等未执行，不关闭12。
2. 首次点击另一文件`src/partial.ts`右侧勾选后，持久review.json已有该记录，但界面仍0/2；手动刷新后为1/2。此单次现象没有证明原生焦点是根因，单独保留，不冒称完整A/B或独立稳定复现。
3. 窗口已聚焦、手动刷新完成后，勾选当前`src/hunk.ts`立即到2/2；当前diff仍是hunk.ts。
4. 为受控时序只在本次fixture用独占新文件创建`src/review-ui-probe.ts`（内容`export const reviewProbe = 1;`）；生产watcher自动显示3文件、2/3，展开未跟踪组。没有改动原5个文件或暂存区。
5. 在本次独立数据目录布置`R07UI03ListBeforeMark`，只捕获真实`list_changes`结果，worktreeId为`e10cb2d2ac073b54`。通过真实刷新按钮发起，capture收据显示PID24560、revision9、outcome=ok。
6. 旧刷新仍等待交付时，用真实勾选框审阅probe文件；界面到3/3，该勾选为1，review.json含3个真实标记。
7. 显式创建release收据后，旧刷新交付；界面回退2/3、probe勾选为0。review.json逐字节不变，probe记录仍在，说明是界面回退，不是已证明的持久标记丢失。截图和AX结果均实际观察。
8. 前后HEAD、语义index列表、原5文件的字节/存在/可执行位及全部5条回收记录（含嵌套文件）相同；唯一工作区新增为步骤4声明的probe。`verification-ui03.json`逐项为true。
9. 所有hold显式释放，deliver明确`explicit=true`，无errors.log、无超时兜底。已正常Cmd-Q退出，CLI核PID24560不存在；未强杀。

## 返修 R07-D1 / P2

旧刷新在标记前捕获、标记成功后才交付时，仍可覆盖更新的审阅状态。需保护审阅变更与快照读取的先后关系，旧刷新不得回退已成功的同版本标记或重新勾回已取消的标记；文件版本真的变化时仍正常失效。不能按路径盲合并旧标记，也不能丢掉内容刷新、跨worktree/activation隔离、既有导航或错误代际保护。

通过原Pi限定修复，复核后再重建、真实补验；旧版结果不倒填为新版通过。05恢复/清空、07完整导航/重启/交错、12剩余项目、D06及最终长测未完成。最新CI若通过仅证明该版编译/现有测试，不消除此桌面缺陷。


[本轮固定构建、夹具、快照与显式收据](./evidence/task07-desktop01/manifest.json)。
