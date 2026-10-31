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

### Codex / review05 Standards（2026-10-31）

固定增量 b5caf5a…ffaa343；以下为独立正式报告原文，完整附录 `/private/tmp/revia-m7-batch-g5scyw0r/review-05-n9a_z26m/standards-evidence.md`。

范围：仅 `b5caf5a...ffaa343` 的 task13 增量；23 路径，单提交。源码、规则与原始证据散列已核对。

硬性规则违反：

- **[P2] 绕过只读类型限制** — `src-tauri/src/m7/changelists.rs:351–364,513–517`：符号链接被建为可选条目，准备提交直接调用 `stage_file` / `apply_index_patch`，没有既有 `ReadonlyReason` 校验；子模块也未过滤。违反 `docs/product-direction.md:59–62`“符号链接、子模块只读；只读限制优先”。提交前应复用类型限制，并向界面返回原因。
- **[P2] 重新整份传输、原生滚动 diff** — 同文件 `:478,494` 将全部原始补丁写入每项 `preview`，整体 View 跨 IPC 返回；`src/features/m7/Changelists.tsx:84` 用 `overflow-auto` 的 `<pre>` 全量渲染。折叠不减少传输。违反 `docs/technical-direction.md:112`“按可见范围分批获取”和 `docs/adr/0008-diff-view-owns-scrolling.md:8–9`；应接入既有分批 diff 读取与滚动实现。
- **[P2] 错误冻结为旧语言** — `src/features/m7/Changelists.tsx:17,29,41,45` 保存 `formatAppError(...)` 字符串；切换语言后已有错误不会重新翻译。违反 `docs/adr/0013-localize-structured-user-errors-at-presentation.md:3` 与 `docs/ui-direction.md:191` 的即时切换要求。应保存结构化错误，在渲染时格式化。

P3 / backlog：

- **文档规则** — `src/features/m7/Changelists.tsx:60,72` 使用 Unicode 星号，文件名统一默认色；不符合 `docs/ui-direction.md:12–14,64` 的 Codicons 与 Git 状态色约定。
- **判断：可能的 Duplicated Code** — `src-tauri/src/m7/changelists.rs:203–205,217–219` 两次出现 `name.trim()` / `name.is_empty() || name.len() > 256`。可共享名称校验；不是硬性违反，不扩大本轮重构。

已核对 Rust03 的 42 项通过，build/lint/x18n 原件 exit0（x18n 76 警告）。未新增或重跑检查；未验证本增量真实桌面或 Windows。合计：3 项 P2、2 项 P3，无 P1；明确余项由主 Codex 接管。


### Codex / review05 Spec（2026-10-31）

固定增量 b5caf5a…ffaa343；以下为独立正式报告原文，完整附录 `/private/tmp/revia-m7-batch-g5scyw0r/review-05-n9a_z26m/spec-evidence.md`。

Spec：仅审固定 `b5caf5a…ffaa343` 的任务13增量。6项P2、1项P3；以下均为静态结论，未动态复现。

(a) 遗漏／部分实现：

- **[P2，S05-01] 块选择粒度错误。** `docs/milestones/m7/spec.md:133`要求“移动整文件与 Git 原始块”，`GLOSSARY.md:64`明确“一个 hunk 可能含几块”。`src-tauri/src/m7/changelists.rs:482–495,359–364`却每个hunk只建一个条目并应用整hunk；相隔少量未改行的两块不能分别归组／勾选。
- **[P2，S05-02] 新列表无法打开文件diff。** `docs/milestones/m4/spec.md:51`要求“单击文件：选中并在预览页签（标题斜体）中显示 diff”。`src/features/m7/Changelists.tsx:13–15,71,84`只有路径span与原始patch预览；`src/App.tsx:5858`未接文件激活／固定入口。用户无法从新列表切换到所点文件的既有并排diff及编辑器。
- **[P2，S05-03] 多文件丢弃缺失。** `docs/README.md:155`：“两种模式都支持多选、全部暂存和多个文件一起丢弃”。`src/App.tsx:5817`在更改列表模式显式禁用丢弃；新组件无替代入口，勾选也未接入既有批量丢弃目标。

(b) 越出能力边界：

- **[P2，S05-04] 只读类型可被新路径暂存提交。** `docs/product-direction.md:59`要求符号链接／子模块“无，只读并说明限制原因”。`changelists.rs:513–545`将未跟踪符号链接生成普通可选条目，`:353–355`直接调用Git暂存，绕过`session.rs:3191–3203`的只读门禁；已跟踪子模块也未过滤。

(c) 看似实现但错误：

- **[P2，S05-05] 仅改信息的amend被拒。** `docs/milestones/m3/spec.md:38`：“amend仅修改信息等合法情况按真实Git语义处理”。`src/App.tsx:2572`和`changelists.rs:311`强制至少勾选一项，即使勾选amend也不能只改提交信息。
- **[P2，S05-06] rebase编辑停点被误封。** `docs/README.md:203`：“在明确的 rebase‘编辑’停点允许 amend 和新建提交”。`changelists.rs:299–303`却拒绝所有`operation.is_some()`，未沿用现有允许停点的门禁；无冲突merge收尾也受影响。
- **[P3，S05-07，backlog] 保存无关设置后勾选状态脱节。** `docs/README.md:155`：“勾选的内容即提交内容”。`src/App.tsx:4578`每次保存设置都清空父selection；新组件`:21–32`未因该清空重新加载，仍显示勾选，提交却被`:2572/5886`拦住，须手动刷新或重选。

原件核对：7884项源码、1113项证据散列及diff一致；末轮Rust42通过，frontend/lint/x18n均exit0。未启动Revia、未实跑Windows、未宣称桌面通过。详见`spec-evidence.md`。


### Codex：review05明确问题处理与验证（2026-10-31）

- 两轴只读能力问题：更改列表复用status能力表并补工作区符号链接/嵌套仓库检查；返回结构化只读原因，前端禁勾选，后端写前再次拒绝。真实本地子模块、已跟踪/未跟踪符号链接回归通过。非UTF8路径仅生成只读行，不隐藏其他有效文件。
- Standards全量diff传输：移除每项整份patch正文与原生滚动pre；列表只传至多200字符的摘要，点击交给既有按范围读取/自绘滚动的diff与编辑器。
- Standards语言：错误保存AppErrorPayload，渲染时按当前语言格式化。
- S05-01：从同一次系统Git hunk按既有segment划分独立块；同hunk多个已选块合成一次原始上下文补丁，支持分别归组/选择。近邻两块独立提交、三块选其中两块的真实Git断言通过。
- S05-02/03：接回文件预览/固定页签、鼠标与键盘多选、现有丢弃确认和逐文件回收入口；不在新模式禁用批量丢弃。
- S05-05/06：沿用现有operation.commit_allowed门禁，允许真实rebase编辑停点amend/新建提交与无冲突merge收尾；空选择amend只改信息且保留其他原暂存，空paths绝不执行全index reset。
- 在相同行内接线中同时恢复Codicons/Git状态色；保存无关设置不再清空父勾选凭据。两项先前P3已按实际修复记录，名称校验重复的smell仍留backlog，不做扩展重构。

原件：`task13-review05-rust-01` 50通过（新18+既有commit12/文件暂存20）；`task13-review05-segments-01` 49通过（hunk24/segment25）；`task13-review05-node-01` 8通过。最后新增非UTF8只读分支后，`task13-review05-rust-03` 本机18通过；其他生产相关源码未再变化。新增非法字节文件名用例在02轮创建fixture时被macOS APFS以EILSEQ拒绝（18通过/1失败），保留全部stdout/stderr/源码散列；该文件名用例现仅在支持它的非macOS Unix编译，本机不能冒称实跑。总相关Rust回归99项跨上述轮次核实，不称单一全套执行。

前端首轮因鼠标事件使用键盘修饰键类型报TS2345，原失败保留；改用ChangesTree既有isPrimaryClick。最后`task13-review05-frontend-03`与`task13-review05-lint-03`均exit0。locale键集/译文未改，既有task13-x18n-01为0错误/76警告，所有新增UI使用已查询键。生成绑定原件task13-review05-bindings-01。Windows和本修复的真实Revia桌面尚未运行。下一步仅新增修复范围独立两轴，审修后重新构建Release；不复用旧二进制验收。

### Codex / review06 Standards（2026-10-31）

固定增量5839e56…2869c81；以下独立正式报告原文，细证据 `/private/tmp/revia-m7-batch-g5scyw0r/review-06-w57zfr6a/standards-evidence.md`。

范围：仅固定 `5839e56c…2869c81` 的 review05 修复增量，9 路径；未重审旧轮次。

硬性规则违反：

- **[P2，H06-01] 预览与待提交块使用不同基线。** `src-tauri/src/m7/changelists.rs:549–555` 将未匹配已暂存签名的 HEAD→工作区块标为 `unstaged`；`src/features/m7/Changelists.tsx:85,104–107` 随即打开既有暂存区→工作区 diff。同一位置先暂存 A→B、再编辑为 C 时，勾选和提交使用 A→C，完整预览却是 B→C。违反 `docs/adr/0004-hunks-come-from-git.md:3`：“补丁就由这同一个 hunk 生成”“保证用户操作的就是看到的、将要暂存的”。应让按范围读取的预览沿用选择项的 HEAD→工作区模型及指纹；仅切换 `source` 无法表达该比较。静态结论，未动态复现。

review05 的只读门禁、整份补丁跨 IPC／原生 diff 滚动、错误随语言切换问题在此增量已修复；图标／状态色亦已接回。未发现新增的基线 smell；旧名称校验重复维持原 backlog，不重复计数。

已核对 7,884 项源码、1,173 项证据散列与固定 diff。原件：Rust01 50 通过、segments01 49 通过；最终 Rust03 本机18通过，frontend03／lint03 exit0；Node01 8通过，既有 x18n 0错误／76警告且语言源未改。Rust02 的 APFS EILSEQ 与 frontend01 的 TS2345 失败原件均保留，未算作通过。未新增或重跑产品检查，未启动桌面，未实跑 Windows 或非 macOS 原字节文件名用例。

合计：1 项 P2 硬违反；P1、P3、新 smell 均为0。明确余项交主 Codex 处理。


### Codex / review06 Spec（2026-10-31）

固定增量5839e56…2869c81；以下独立正式报告原文，细证据 `/private/tmp/revia-m7-batch-g5scyw0r/review-06-w57zfr6a/spec-evidence.md`。

Spec：固定 `5839e56…2869c81`，仅 review05 修复增量；2项P2，均为静态结论，未动态复现。

(a) 遗漏／部分实现：未发现下述接线错误以外的独立遗漏。

(b) 范围外行为：未发现 scope creep。

(c) 实现错误：

- **[P2，S06-01] 多选按隐藏的暂存区顺序计算。** `docs/milestones/m4/spec.md:52`要求“Shift 点选连续范围”“⇧↑ / ⇧↓ 扩展”；`docs/README.md:155`明确“两种模式都支持多选”。新接线 `src/App.tsx:5858`直接复用按`treeOrder(snapshot)`计算的回调（`src/hooks/useTreeSelection.ts:68–74,92–106`），但列表按更改列表顺序显示（`src/features/m7/Changelists.tsx:69–81,92`）。例如三份未暂存文件a/b/c，把a移到第二列表，可见顺序b/c/a；单击b再Shift单击a只选中a/b，漏选中间的c；箭头导航也偏离可见顺序，批量丢弃目标随之错误。

- **[P2，S06-02] 相同改动内容使未暂存块打开错误版本。** `docs/milestones/m4/spec.md:51`要求“单击文件：选中并在预览页签……中显示 diff”；`docs/README.md:214`要求“diff 工作区一侧用 ⌘S 明确保存”。`src-tauri/src/m7/changelists.rs:549–555`仅凭增删文本签名存在于已暂存集合就标记`staged`，签名不含位置（`src-tauri/src/git/segments.rs:648–675`）。同文件两处相同替换，先暂存第一处再修改第二处，会把第二处也标记已暂存；`Changelists.tsx:85,104–107`因此打开第一处的已暂存diff，实际点击的工作区变化及其编辑入口缺失（`src/App.tsx:6046`）。应按实际位置／版本关联，不能只比较改动正文。

原件核对：7,884项源码、1,173项证据散列及diff一致。末轮Rust18通过，frontend/lint exit0；其余既有检查及源码映射见`spec-evidence.md`。本修复Revia桌面、Windows和非UTF8路径用例仍未实跑；不重启既有Rebased、不复测已过检查。


### Codex：review06预览基线与可见顺序修复（2026-10-31）

H06-01/S06-02：新增明确的 `ChangesKind::Changelist` 只读比较源。启用更改列表的会话额外缓存系统Git HEAD→工作区模型；原staged/unstaged/untracked条目及指纹保留供既有安全写入口。列表、分页行、并排文本与高亮的旧侧均取HEAD、新侧取工作区，选择补丁也调用同一 `Git::diff_changelist`；删除按正文签名猜staged的分支。此源不签发暂存/hunk/segment写凭据，工作区一侧编辑仍复用既有编辑会话。模式切换只改变应用设置与读取模型，不改变真实index。

S06-01：行身份为group:path，点击范围、Command/Ctrl多选、Shift箭头及全选都使用实际可见顺序；文件跨组显示有独立行身份。主diff文件位置/导航按同一可见顺序去重文件。批量/键盘丢弃把实际选中文件投影到原unstaged/untracked条目，继续沿用原确认、指纹和回收流程；不借内部比较源改写Git。

`task13-review06-rust-01` 六targets共134项通过（commit12、文件20、hunk24、更改列表21、审阅32、segment25）；`task13-review06-session-01`会话/刷新隔离18项通过，共152相关Rust检查。新增真实A→B暂存→C预览、相同正文两位置、普通/更改列表模式切换断言，均通过生产WorktreeSession入口读取实际Git内容。`task13-review06-node-02`13通过（可见顺序与导航5、既有提交8），`task13-review06-frontend-04`与`task13-review06-lint-03` exit0。后续前端类型/导航修正未改Rust源码，未重复已过Rust。locale键集/译文不变，复用已查询的HEAD/工作区标签；既有x18n为0错误/76警告。

前端01/02先后暴露新增类型映射遗漏，TS错误与原始源码hash/argv/exit/stdout/stderr全部保留，未算通过。所有原件位于codex-checks/task13-review06-*。非法字节文件名用例仍仅非macOS Unix条件，本机与Windows不冒称实跑。未启动本候选Revia、未构建新版Release；下一步仅此新增量独立两轴，收齐报告后才固定新版Release与真实桌面。

### Codex / review07 Standards（2026-10-31）

固定增量5f70d9c…150b274；以下正式报告原文，细证据 `/private/tmp/revia-m7-batch-g5scyw0r/review-07-wx8l6b6k/standards-evidence.md`。

范围：仅 `5f70d9c…150b274` 的 review06 修复增量，18 路径；不重审旧轮次。

硬性规则违反：

- **[P2，H07-01] 刷新会撤掉新的预览基线。** 新 `session.rs:569–584` 保留原条目并追加 `changelist`，但 `App.tsx:874–885` 仍经仅含旧三组的 `getDisplayOrderEntries` 选择刷新目标。打开 A→C 后刷新，即回到同路径的 `unstaged` B→C；勾选补丁仍是 A→C。违反 `docs/ui-direction.md:70`：“同文件在原组仍有改动则留在原位置”，及 ADR-0004:3：“保证用户操作的就是看到的、将要暂存的”。应使刷新保留模式对应读源。
- **[P2，H07-02] 新可见顺序未接入主 diff 导航。** `App.tsx:475–488,5486` 只接树选择和计数；`focusNextFile:1643–1647` 仍遍历含隐藏旧条目的 `snapshot.entries`，`reviewNextFile:3008` 仍只遍历旧三组。可见顺序 b/a/c 时，标记 b 后跳到隐藏顺序中的 c；上一文件还可跳回暂存区读源。违反 `docs/ui-direction.md:81`：“标记后按当前文件列表顺序跳到下一个未审阅文件”。这些入口应共用模式对应的文件顺序。

判断项：**[P3，可能 Duplicated Code]** `App.tsx:480` 与 `Changelists.tsx:36,110` 重复 `find(changelist) ?? find(untracked)`；可共用读源查找。非硬违反，按 Q174 留 backlog。

核对 7,886 项源码、1,253 项证据散列及固定 diff 一致。原件 Rust134＋会话18、Node13 通过，最终 frontend04/lint03 exit0；前端01/02失败保留。locale无新增，既有 x18n 0错误/76警告。本轴仅静态核对，未重跑检查、启动桌面或实跑 Windows/非法字节路径。

合计：2 项 P2 硬违反、1 项 P3 smell；P1=0。明确余项交主 Codex；未改产品。


### Codex / review07 Spec（2026-10-31）

固定增量5f70d9c…150b274；以下正式报告原文，细证据 `/private/tmp/revia-m7-batch-g5scyw0r/review-07-wx8l6b6k/spec-evidence.md`。

Spec：固定 `5f70d9c…150b274`，仅 review06 修复增量，18 路径、1 提交；3 项 P2，均为静态结论，未动态／桌面复现。

(a) 遗漏／部分实现：以下三处消费入口未接完整，未确认独立遗漏。

(b) 范围外行为：未发现新增 scope creep。

(c) 实现错误：

- **[P2，S07-01] 刷新后恢复错误预览基线。** ADR-0004:3 要求“保证用户操作的就是看到的、将要暂存的”；`docs/ui-direction.md:70`要求“同文件在原组仍有改动则留在原位置”。本轮新增 `changelist` 源（`src-tauri/src/session.rs:572–584`），但 `src/App.tsx:874–885` 仍经 `:6835–6839` 的旧过滤器选预览，排除所有该源，随后改开同路径 unstaged／staged。HEAD=A、index=B、工作区=C时，点击列表显示A→C，任一监视或手动刷新又变B→C，勾选提交仍是A→C。应让刷新保留当前模式的预览源。

- **[P2，S07-02] 文件导航仍走隐藏顺序／版本。** `docs/ui-direction.md:81`要求“按当前文件列表顺序跳到下一个未审阅文件”；M4规格:135要求“能切到上一个、下一个文件”。新 `entryOrder` 只接树选择和位置计数（`src/App.tsx:475–486,5486`）；实际工具条入口 `:6036–6037` 仍到 `:1640–1647`，直接索引含旧源的原始快照；审阅导航 `:3008` 也仍用旧过滤器。可见b/c/a时，从c点下一文件停住，从a点上一文件可打开隐藏的unstaged:c；审阅导航也会离开HEAD→工作区源。应让这些入口复用实际可见文件顺序。

- **[P2，S07-03] 更改列表内外部diff全部被拒。** M6规格:146保留“外部 diff 与合并为可选设置”。`src/features/m7/Changelists.tsx:110`现固定选择changelist，`src/App.tsx:6042`原样传给外部diff；`src-tauri/src/external_tools.rs:228,247–257`却要求该kind存在于原始Git status，而其解析器不生成changelist。菜单仍启用，所有已跟踪行均在启动前被拒。应支持该比较源及其HEAD基线，并保留既有保护。

已核对7,886项源码、1,253项原证据散列及diff；Rust152相关通过，末轮Node13通过，frontend04／lint03 exit0；失败原件保留。未重跑产品检查、未启动桌面或Windows。详见spec-evidence.md。

