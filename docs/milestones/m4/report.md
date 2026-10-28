# M4 界面重做报告

实施：Pi（GLM-5.3-Flash，整包交接按 Q136）。分两个会话完成：前一会话提交 8207efa（F09）、7be4dd0（F01）、7aa92a7（F02–F08），留下 7 个文件的收尾改动未提交；后一会话（本报告作者）接手，核对了整份规格，补齐缺漏后随遗留改动一起提交为 4a835b4。分支 `m4`，共 4 个提交，未推送、未合并。

## 各项实现位置与结论

### F01 视觉基础 — 已完成（7be4dd0）

- 颜色主题：`tools/gen-themes.mjs` 从 `src/themes/sources/` 的四套 VS Code 主题文件（含 include 链）生成 `src/themes/themes.css`，变量按主题键命名（`sideBar.background`、`gitDecoration.*`、`diffEditor.*`、`tokenColors` → `--syn-*` 等）；`src/lib/theme.ts` 管浅色/深色各选一套、随系统切换（`matchMedia` 监听）与默认值（浅色 Atom One Light、深色 One Dark Pro）。来源、版本与许可记录在 `src/themes/README.md`，许可文件在 `src/themes/licenses/` 与 `src/assets/material-icons/LICENSE.txt`。Q107 时期的设计变量已全部替换。
- 字体：界面用系统字体；代码字体默认随应用打包的 JetBrains Mono（`@fontsource/jetbrains-mono`，`src/main.tsx` 引入），设置中可改字体与字号，本机没有该字体时 CSS 回退（`src/lib/theme.ts` `applyCodeFont`）。尺寸照效果图：行高 22、页签高 35、状态栏高 22（`src/index.css`）。
- 图标：界面图标全部换成 `@vscode/codicons`（lucide-react 已移除依赖），文件类型图标换成 Material Icon Theme（`tools/sync-material-icons.mjs` 从本机扩展同步用到的 SVG 到 `src/assets/material-icons/`）。“关于 Revia”注明两者版权与许可（`src/components/app/SettingsDialog.tsx`）。
- 文件名按状态上色，颜色取主题 `gitDecoration.*`（`src/index.css` 的 `--status-*`），行内不显示状态字母（Q128）。

### F02 主窗口框架 — 已完成（7aa92a7）

- 主工具栏（高 38，`src/components/app/Shell.tsx` `TitleBar`）：macOS 红绿灯留白（`MAIN_TOOLBAR_INSET`）、仓库组件（点开是仓库列表 + “添加仓库…”）、worktree 组件（点开是 worktree 列表，每行显示分支、路径、有无未提交改动、进行中操作、领先落后数，F09.2 的数据）、右侧设置按钮。
- 左侧图标条（`ActivityBar`）：只有“提交”一个图标，带未提交文件数徽标（去重按路径计数），点击切换提交工具窗口显示/隐藏。
- 编辑器页签区：没有页签时显示空状态与常用快捷键提示（`messages.changes.noSelection`：⌘K 提交、F7 下一处改动、⌥⌘R 标记已审阅、⌘O 添加仓库）。
- 状态栏（`StatusBar`）：当前分支、领先/落后数并注明“截至上次 fetch”、自动刷新不可用提示（点击可手动刷新）、审阅进度。
- 欢迎界面（`WelcomeScreen`）：已添加仓库列表、“添加仓库”按钮、⌘O、拖入文件夹（`onDragDropEvent`，Q35）。

### F03 提交工具窗口 — 已完成（7aa92a7 + 4a835b4 补缺）

- 标题行“提交”“回收区”两个页签与工具条：刷新、丢弃所选、展开全部、收起全部、更多（更多里是全选、关闭全部页签、刷新）。
- 改动树（`src/components/app/ChangesTree.tsx`）：已暂存、未暂存、未跟踪三组，组标题显示文件数；未跟踪默认收起；同一文件可同时出现在两组。每行是文件类型图标、按状态上色的文件名、灰色所在目录（重命名时“原 …”）。
- 单击选中并在预览页签（斜体）显示 diff，单击另一个文件替换预览页签；双击或 Enter 固定为普通页签（`data-preview` 冒烟可查）。
- 多选：Shift 连续、⌘ 增减、⇧↑/⇧↓ 扩展、⌘A 全选当前树。暂存（⌥⌘A）、取消暂存、丢弃（⌥⌘Z）作用于全部选中项；行内按钮按组显示；组标题右侧“全部暂存”/“全部取消暂存”（只读条目自动排除）。**接手时发现未跟踪行没有丢弃按钮**（`showDiscard` 只写了 `unstaged`），4a835b4 已补上，与后端 `discard_untracked_locked`（先存回收区再删除，Q120）对齐。
- 只读文件不参与批量操作，跳过原因在结果通知中逐条列出（符号链接、子模块、嵌套仓库、非 UTF-8 路径、不可读，Q81–Q83）。
- 丢弃多个文件：一次确认对话框列出全部文件并说明先存回收区；部分失败时通知如实列出已完成/未执行（`performBatchWrite` 的 `failed`/`skipped` 分支）。
- 已审阅文件行尾小图标；⌥⌘R 行为不变（Q94）。

### F04 提交区 — 已完成（7aa92a7 + 4a835b4 补收尾）

- 位于“提交”页签底部（`src/components/app/CommitComposer.tsx`）：修改上一次提交勾选框（右侧最近提交信息按钮）、提交信息框、“提交”按钮与齿轮按钮、审阅进度。齿轮打开提交选项弹层，内含“跳过 hooks”，默认不勾，提交后复原（`finally` 里按草稿 revision 复原）。
- M3 行为保留：进行中操作门禁、模板、最近 20 条、按 worktree 草稿、amend 已在上游时的确认、⌘K 提交。
- 提交进行中显示进度，hooks 实时输出经 `COMMIT_OUTPUT_EVENT` 流式进入通知的输出区；完成后用通知报告结果。**接手时未提交的收尾改动正是这里：结果通知里没有报告“提交前快照存了哪些文件”**（规格 F04.3 明确要求）。后端 `CommitOutcome` 增加 `snapshotPaths`（快照记录的文件路径列表，`commit_flow.rs`），通知文案 `messages.commit.resultSnapshotFiles` 列出路径，随 4a835b4 提交；`pnpm bindings` 重新生成验证与手工改动一致。

### F05 diff 页签 — 已完成（7aa92a7）

- 页签标题是文件名 + “未暂存/已暂存/未跟踪”，页签下方路径导航行（`Breadcrumbs`）。工具条：上一处/下一处改动（⇧F7/F7）、并排与统一切换（选择存 `localStorage`，Q17）、“标记为已审阅”（切换不跳转，Q121）、更多（关闭页签、关闭全部、刷新）。
- 块级入口照 Rebased：按钮写“暂存此块”“取消暂存此块”“丢弃此块”（Q135），操作对象仍是 Git 生成的 hunk，执行前校验指纹（ADR-0004/0005/0009，M2 逻辑未动）。
- lock 文件、二进制和超大文件默认收起，编辑器提示条说明并可展开（Q32）。
- 渲染与滚动沿用 M2 的 D06 实现，本次实测数据见下文。

### F06 回收区页签 — 已完成（7aa92a7）

列表、详情、恢复、另存、清空移到“回收区”页签（`src/components/app/RecoveryPanel.tsx`），规则不变（Q29、Q85–Q87、ADR-0010）；确认与只读说明改用统一对话框与提示条。

### F07 提示与通知 — 已完成（7aa92a7 + 4a835b4 修细节）

- 文件相关状态（默认收起、只读、冲突、写前校验失败）：diff 页签顶部提示条（`EditorBanner`）。
- 操作结果（暂存、丢弃、提交、恢复）：右下角通知气泡（`src/components/app/Notifications.tsx`）；成功约 10 秒自动消失，失败与结果待核对保留到手动关闭。
- 需要选择的情况（丢弃确认、写入期间关闭/退出）：对话框（`src/components/app/Dialogs.tsx`）。
- 4a835b4 修了一个计时器问题：通知组件此前依赖父组件传入的 `onClose`，父组件因其它状态重渲染会重置 10 秒计时；改为按通知身份与持久性计时。

### F08 设置 — 已完成（7aa92a7）

设置对话框（`src/components/app/SettingsDialog.tsx`）：左侧分类（外观 / Git / 关于 Revia）、右侧内容。“外观”有浅色主题、深色主题（Q132）、代码字体与字号（Q133）；“Git”有 Git 程序路径、合并工具、重新加载环境；“关于 Revia”是版本与图标、主题的版权许可。修改立即保存（输入框失焦或回车提交）。回收区的查看与清空在“回收区”页签，未放进设置。

### F09 后端 — 已完成（8207efa）

- 多文件暂存、取消暂存、丢弃（`src-tauri/src/batch.rs`）：整批一次写锁，串行执行；每个文件按真实状态核对指纹（ADR-0009），只读条目返回跳过原因；丢弃逐个先存回收区（未跟踪文件走 `discard_untracked_locked`）；返回逐文件结果（成功/跳过/失败）与批次后快照。Rust 测试覆盖混合成功失败、未跟踪删除等。
- worktree 只读状态（`src-tauri/src/worktree_status.rs`）：分支、脏/净、进行中操作、领先/落后（本地记录 `rev-list --left-right --count HEAD...@{upstream}`）、路径可访问性；单个 worktree 读取失败不影响整个列表。命令 `list_worktree_status`，前端在切换 worktree 与刷新后更新。
- 未新增 M5 以后的命令（命令清单核对过，与 M2/M3 相比只多了批量操作、worktree 状态与设置类命令）。

## 测量与长测脚手架

- 冒烟（`runSmoke`）按新 DOM 重写：页签预览/固定行为（`data-preview`）、改动树行数、组标题数、提交区与信息框存在性、图标条徽标、状态栏文本、布局尺寸（标题栏 38、图标条 44、侧栏 330、页签 35、状态栏 22）与四套主题的实际取值都在报告中输出。4a835b4 让它优先选择 `REVIA_MEASURE_REPO` 指定的仓库，避免共享数据目录里其它仓库排在首位导致测量目标错误。
- D06 自动测量与长测（soak）沿用 M2 的取数/跳转/滚动探针，入口与新界面兼容，无需改动探针本身。

## 交付前自查结果

命令按 AGENTS.md“开发与构建命令”与交接文件运行，均为最终代码（4a835b4）的结果：

| 命令 | 结果 |
| --- | --- |
| `pnpm install` | 通过（无变化） |
| `pnpm exec tsc -b` | 通过 |
| `pnpm lint` | 通过（仅 tools/test-refresh-gate.mjs 的 5 条既有 unused-vars 警告，M2 遗留，非本次引入） |
| `pnpm build` | 通过（产物约 1.0 MB，chunk 大小警告与 M2 相同） |
| `pnpm bindings` | 通过，与手工改动一致（0 diff 后置空） |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 317 通过、0 失败、1 忽略（同 M2/M3 的忽略项） |
| `for t in tools/test-*.mjs; do node "$t"; done` | 7 个脚本全部通过 |
| `python3 tools/test_bench_long_running.py` | 46 个用例通过 |
| fixture `tools/fixtures/setup.sh /tmp/revia-m4-fixtures` | 通过（注意：脚本是 bash，直接 `python3` 运行会报语法错误；用 `bash` 运行） |

D06 实测（release 构建，fixture `/tmp/revia-m4-fixtures/diffs`，目标 `src/generated.ts`，8309 行，两视图各跑一次；数据目录 `REVIA_MEASURE_DATA_DIR=/tmp/revia-m4-data`）：

| 视图 | 首屏（listChanges 起） | 首屏含合成帧 | 跳转 appear | 滚动未覆盖帧 |
| --- | --- | --- | --- | --- |
| split | 100 ms | 是（rafPainted=true） | 23 ms | 0 |
| unified | 110 ms | 是 | 16 ms | 0 |

对照 M2 门槛（首屏 150 ms 内、未缓存跳转 100 ms 内有文字、快速滚动不露白）：两项实测均达标。第一次 unified 运行因测量途中窗口失焦被判“前台可见性无效”而失败，重跑通过；数据如实记录。冒烟检查（`REVIA_SMOKE=1`）通过：布局尺寸、页签预览/固定、徽标与状态栏文案符合预期。

## 规格没写到而自行做的取舍

按交接文件“选最接近效果图外观、Rebased 布局的做法”处理，全部在此列出：

1. **冒烟测量选择仓库**（4a835b4）：指定 `REVIA_MEASURE_REPO` 时优先选它对应的仓库，而不是固定取列表第一项。起因是开发机共享数据目录里第一个登记仓库指向已清理的 fixture，导致冒烟在打开 worktree 时失败。不影响普通界面。
2. **“更多”菜单的内容**：规格只说“更多”按钮，未列菜单项；提交工具窗口的“更多”放了全选、关闭全部页签、刷新（全选也是 ⌘A 的可见入口），diff 页签的“更多”放了关闭页签、关闭全部、刷新。
3. **未提交文件数徽标的计数口径**：按路径去重（同一文件同时出现在已暂存与未暂存两组时只算一个），与“有未提交改动的文件数”一致。
4. **状态栏右侧追加应用名**：效果图状态栏右侧有远端/同步类信息的位置，M4 没有 fetch 功能可显示，放了应用名占位；布局与 Rebased 对齐，不提供任何动作。
5. **空状态快捷键提示文案**：规格要求“写常用快捷键提示”，具体条目自选了 ⌘K、F7、⌥⌘R、⌘O 四条（提交、下一处改动、标记已审阅、添加仓库），都是 M4 范围内的核心操作。
6. **提交结果通知的 `snapshotPaths`**（上一位遗留改动的收尾）：快照文件列表并入快照记录通知的正文（“快照包含：…”），不单独开一条通知。
7. **设置输入框提交时机**：主题下拉即选即存；文本输入（字体、Git 路径、合并工具）与数字输入（字号）在失焦或回车时提交，避免每次按键写设置。

## 已知限制与未做项

- 规格中的交互先按书面描述实现（Q136），未与本机 Rebased 1.1.22 并排对照；布局、交互细节的偏差（页签关闭按钮位置、下拉样式等）留给验收时按 Rebased 修正。
- 与 Rebased 的并排截图、四套主题截图、D06 与 M2/M3 界面验收的重做、Windows 编译检查（Q55）按分工由规划方验收时进行；本会话未做桌面截图。
- `pnpm lint` 的 5 条 unused-vars 警告与前端 chunk 大小警告为 M2 遗留，未在本次处理。

---

# 返修（Q113 唯一一次，模型 deepseek-v4.1-flash）

范围：`docs/milestones/m4/spec.md` 末尾 `## Comments` 的 A–F 六节，加上此前两位实施方
已完成的提交（c561c4b、52b492d、4d2ff44、7c5defe）。本段逐条写处理结果。

## A. 用户新确认的决定

### A1 左右对照（Q138、Q141、Q142）

后端（`src-tauri/src/git/segments.rs`、`session.rs`、`commands.rs`）：

- `side_by_side_layout`：一次返回两侧名称、两侧行数、改动段、折叠区、每段的操作目标
  （`SegmentAction`）、忽略空白档位与指纹。两侧全文与段按 `<changes>/<path>/<档位>/<上下文>`
  缓存（最多 16 份），指纹仍按普通 diff 计算。
- `side_lines`：按需取一侧的行，标出改动行与词级区间（按词 / 按字符 / 按行三档）；
  词级区间只对与请求范围相交的段计算（与可见行成正比）。
- `stage_segment` / `unstage_segment` / `discard_segment`：补丁由缓存的同一个 Git hunk
  按段切开（同 `git add -p`），写前按真实状态核对指纹；丢弃先存回收区。
- 冲突文件：左右对照只读显示（左 HEAD、右工作区含冲突标记），不签发段按钮（3.2 节）。
- 统一视图的显示行也带 `segmentIndex` / `segmentAction`：每段首行一个按钮，hunk 标题行
  只显示 `@@` 信息（设计第 2 节）；忽略空白档位随 `get_rows` 传入，段编号与那份 diff 一致。
- 测试：`tests/segment_staging.rs` 新增 7 例（布局标签/行数/段/折叠区、已暂存与未跟踪
  标签、词级区间与按行档位、统一视图每段一个首行、`-w` 过滤缩进改动、空行档位禁用段按钮）。

前端（`src/features/diff/SideBySideView.tsx`、`syncScroll.ts`、`DiffView.tsx`）：

- 两栏各自连续显示整个文件，不插空行；中间 96px 窄栏：两列行号 + canvas 弧形色带 +
  每段一个 » / « 按钮；栏头写两侧名称与路径；改动块用 `diffEditor.*` 淡底色、改动字词
  用实色（与语法颜色叠加）；折叠未改动部分（点击展开）；同步滚动（主动栏中线 →
  `mapLine` → 跟随栏，两栏各自维护滚动位置与自定义滚动条）。
- 工具条：上一处 / 下一处改动（并排视图按段跳转）、← 第 n / m 个文件 →、“N 处改动”、
  并排 / 统一、齿轮菜单（diff 显示选项与设置同步）、标记为已审阅、更多。
- 统一视图按段操作；`tools/test-sync-scroll.mjs` 覆盖 `mapLine`（段前、段内、段后、
  零长度段、互逆误差、首尾夹紧）。

未做到 / 与设计的差异（见“未做到”一节）：窄栏按钮用 DOM 而非 canvas 绘制文字；
并排视图的软换行未实现；测量脚手架已接好但本次无法实测（见“实测”）。

### A2 改动树按目录分组

已由 4d2ff44 完成（按目录分层展开、工具条“更多”的查看选项、选择存设置）。本次复核
无改动。

## B. 实测确认的缺陷

| 项 | 处理结果 |
| --- | --- |
| B1 单文件丢弃不可用 | 已由 c561c4b 修复（确认只绑定 worktree 激活与文件指纹）。本次复核：丢弃对话框打开即失效的问题未再出现。 |
| B2 弹层被裁掉 | 已由 c561c4b 换成 Base UI Popover（Portal + Positioner，翻转与贴边）。本次新增的齿轮菜单、冲突对话框也走同一套组件。 |
| B3 自引用变量 | 已由 c561c4b 修复。 |
| B4 侧栏宽度拖不动 | 已由 c561c4b 完成（220–720，宽度存设置）。 |
| B5 页签栏滚动条露出 | 已由 c561c4b 修复。 |
| B6 徽标无圆形底色 | 已由 c561c4b 修复。 |
| B7 界面仍有 hunk | 已由 c561c4b 改为“块”；本次新增文案也统一用“块”。 |
| B8 重复说明文字 | 本次修：默认收起只在提示条说明一次；欢迎界面去掉重复的拖入提示；设置里两个颜色主题的相同说明合成一条。 |
| B9 页签无当前态 | 已由 c561c4b 完成。 |
| B10 状态栏 Revia 占位 | 本次修：状态栏右侧的占位已删除（冒烟检查的状态栏文案不再出现 “Revia”）。 |
| B11 过渡 | 已由 c561c4b 完成（控件与弹层 100–120ms）。 |
| B12 控件形状 | 已由 c561c4b 完成。 |

## C. 审查发现

C1–C14 已由 52b492d、c561c4b 处理（未跟踪文件丢弃前复核、`--no-optional-locks`、
写操作期间拒绝重载环境、⌥⌘A 不再当 ⌘A、混合组串行、跳过原因逐项报告、多选时行内按钮
作用于全部选中项、hooks 实时输出放在提交区、写结果改通知、另存失败标题、平台化修饰键、
视图选择并入设置、dirty 用“未提交改动”、设置第三类“关于 Revia”）。本次复核这些代码
路径仍然成立，未回退。

C15 清理：本次做了其中可安全验证的部分——`performWrite` 去掉走不到的整文件分支并把
写目标类型改成“整块或按段”二选一；`DiffView` 去掉未用的 `modified` 参数；`batch.rs`
的重复数组加注说明。**未做**：Q107 时期 CSS 别名（frame / island / edge / fg-2 等）改成
VS Code 键名（涉及 20 多个组件的类名，本次环境无法逐屏截图验证，风险大于收益）；
App.tsx 按主题与字体、通知、页签、树多选、批量写拆成 hook（4700 行，改动面太大，
无法在本轮验证完）。

## D. 返修要求

- 自查命令：见“自查结果”。
- release 版运行一律带 `REVIA_SMOKE=1` 或 `REVIA_MEASURE=1` 与独立数据目录，并加超时。
- 只在 `/Volumes/T7 Shield/project/Revia-m4` 工作、只提交到 `m4`；未在主目录执行任何
  git 命令；未推送、未合并、未 rebase、未 reset。
- 本报告末尾的“返修”一节即 D 要求的逐条记录。

## E. 设置（Q139、Q140）

- E.1 设置对话框照 Rebased 重做：左上角搜索（按设置项文字过滤）、左侧分类可展开、
  右侧顶部“分类 › 页名”；分类为外观、提交、diff、通知、Git、关于 Revia。只放 M4 项。
- E.2 外观：界面缩放 80%–150%，快捷键 ⌃⌥= / ⌃⌥- / ⌃⌥0，选择存设置。
- E.3 提交：首行长度上限（默认 72）、首行与正文空行、正文每行上限、右边界线；问题在
  提交信息框里标出但不阻止提交。提交时警告由 Rust 层检查（新模块
  `src-tauri/src/commit_warnings.rs`：CRLF、游离 HEAD 或 rebase、大文件阈值、其他系统上
  不合法的文件名），提交前弹“继续提交 / 取消”的提示。
- E.4 diff：上下文行数、折叠、同步滚动、高亮方式、显示空白、软换行、最后一处后跳到
  下一个文件、忽略空白三档；工具条齿轮菜单与设置对话框共用同一份选项，改一处两处同步。
- E.5 忽略空白前端：档位切换；空行档位下块按钮变灰并显示说明提示条；整文件操作照常。
- E.6 通知：成功通知开关（失败与待核对始终弹出）。
- E.7 新设置项全部存进 `settings.rs` 并生成绑定（`mergeAutoApply` 一并加上），前端不用
  localStorage 保存设置。

## F. 内置三方合并（Q143）

- 冲突组：改动树最上方“冲突”组，组标题右侧“解决…”；冲突文件不再出现在已暂存、未暂存组；
  双击或 Enter 打开合并页签。
- 冲突对话框：文件列表 + 两侧状态（已修改 / 已删除 / 已新增），可多选后接受一侧，
  或打开合并页签。
- 合并页签：三栏（两侧只读 + 中栏可编辑）、两条窄栏冲突色带、» / « 把一侧的一段放进结果、
  上一处 / 下一处冲突、冲突计数、“应用所有不冲突的改动”（设置关闭自动应用时可用）、
  全部接受左边 / 右边、应用与取消。栏头按进行中的操作命名（merge / rebase /
  cherry-pick / revert）。不能三栏合并时只显示说明与“接受一侧”。
- Rust：`merge_open` / `merge_lines` / `merge_apply` / `merge_accept_side` / `list_conflicts`；
  三份内容写到 Revia 数据目录下的临时目录、`git merge-file -p --diff3` 生成初始结果、
  `git diff --no-index -U0` 求段表，用完删除临时目录；“应用”先校验指纹、存回收区
  （新来源 `mergeApply`）、按你的版本换行风格写入、`git add`、写后核对（仍冲突或暂存区
  不符则标结果待核对）。
- 测试：`src-tauri/tests/merge.rs` 12 例（真实 Git 仓库）：内容冲突、两边都新增、
  一边删除一边修改、二进制、rebase 栏头、关闭自动应用、应用前外部修改被拒、应用先存
  回收区、应用后不再是未解决冲突、接受一侧、接受“删除”的一侧（`git rm`）、CRLF 保持、
  不改 Git 配置、临时目录清理。
- fixture：`tools/fixtures/setup.sh` 增加 `conflicts` 仓库（内容冲突 + 一边删除一边修改 +
  一个普通未暂存改动），冒烟检查新增冲突组、合并页签与并排视图三栏/canvas 检查。

## 自查结果

| 命令 | 结果 |
| --- | --- |
| `pnpm exec tsc -b` | 通过 |
| `pnpm lint` | 通过（7 条 M2 遗留的 unused-vars 警告，非本次引入） |
| `pnpm build` | 通过（chunk 大小警告与 M2 相同） |
| `pnpm bindings` | 通过（新增 SideBySideLayout、SegmentAction、SideLine、MergeLayout、MergeConflict、MergeSegment、MergeOutcome、MergeSide、ConflictFile、ConflictSideState、CommitWarning 等绑定） |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 370 通过、0 失败、2 忽略（与之前相同的忽略项） |
| `for t in tools/test-*.mjs; do node "$t"; done` | 8 个脚本全部通过（新增 test-sync-scroll.mjs：`mapLine` 6 例 + 显示行索引/折叠换算 4 例） |
| `bash tools/fixtures/setup.sh /tmp/revia-m4-fixtures2` | 通过（新增 conflicts 仓库） |

## 实测

冒烟检查（`REVIA_SMOKE=1`，独立数据目录，fixture `conflicts`）在返修中途成功跑完一次，
记录到 `/tmp/smoke2.json`：

| 项 | 数据 |
| --- | --- |
| 打开仓库/读取状态 | 2 个文件、24 ms |
| 合并页签 | 打开成功；中栏 12 行、含冲突标记；“还有 1 处冲突”；改动树 1 个分组（冲突） |
| 页签 | 单击产生预览页签、双击固定；合并页签 `merge:src/app.ts` 为当前页 |
| 布局尺寸 | 标题栏 38、图标条 44、侧栏 330、页签 35、状态栏 22 |
| 状态栏文案 | `main 0 0 截至上次 fetch 已审阅 0/2`（不再有 Revia 占位） |

**D06（split、unified）与并排视图冒烟未能重测**：本次会话期间机器屏幕被锁定
（`CGSSessionScreenIsLocked`）。窗口被遮挡后 WebKit 会挂起页面，`MessageChannel` 任务与
定时器都不再执行：冒烟跑到约 6 秒（打开最后一个文件后）就停住，进程与 WebContent 的 CPU
都是 0，直到超时被杀；D06 的自动测量另有“前台可见且有合成帧”的门槛，同样无法满足。
这是环境限制，不是代码路径的问题（同一份构建在屏幕未锁时跑完过一次冲突仓库的冒烟，
数据见上表）。测量脚手架已按新结构接好：

- `REVIA_MEASURE_ONLY=split` 走新的并排视图分支：记录跳转（冷 / 热）、滚动 60 帧的未覆盖
  行数、帧间隔 p50/p95，以及两栏中线同步误差（`split.syncErrorMax` / `syncErrorLast`），
  写进同一份测量日志。
- 冒烟新增 `sideBySide`（左栏/窄栏/右栏数量、两列行号、窄栏 canvas 是否画了内容）与
  `merge`（合并页签、中栏行数、是否含冲突标记、冲突计数文案）。

屏幕解锁后重跑这两条命令即可取到数据：

```bash
REVIA_MEASURE=1 REVIA_MEASURE_ONLY=split REVIA_MEASURE_DATA_DIR=/tmp/revia-d06-data \
REVIA_MEASURE_REPO=/tmp/revia-m4-fixtures2/diffs REVIA_MEASURE_PATH=src/generated.ts \
REVIA_MEASURE_LOG=/tmp/d06-split.json src-tauri/target/release/revia
REVIA_SMOKE=1 REVIA_MEASURE_DATA_DIR=/tmp/revia-smoke-data \
REVIA_MEASURE_REPO=/tmp/revia-m4-fixtures2/conflicts REVIA_MEASURE_PATH=src/app.ts \
REVIA_MEASURE_LOG=/tmp/smoke-conflicts.json src-tauri/target/release/revia
```

## 未做到 / 与规格或设计不同的地方

1. **D06 与并排视图冒烟没有实测数据**：原因见上（屏幕锁定 + 定时器节流）。代码路径与
   脚手架已完成，命令与预期字段写在上面。
2. **窄栏的 » / « 用 DOM 绘制**（设计 4.3 说行号与按钮用普通 DOM，色带用 canvas）：色带
   按设计用 canvas 画；按钮与行号是 DOM，位置跟随各自一栏的滚动。
3. **并排视图的软换行未实现**：固定行高的虚拟滚动与软换行冲突，`diff.softWrap` 目前只
   影响统一视图（统一视图也没有实现换行，选项已存进设置供后续使用）。
4. **中栏编辑器用 `<textarea>`**，不是 CodeMirror 6：本机离线环境无法安装新依赖；
   textarea 提供撤销、重做与编辑，但没有语法高亮与段内标记。设计里“推荐 CodeMirror”
   不是硬要求，差异全部来自 Git 这一点不受影响。
5. **合并页签的两条窄栏只有冲突色带**，没有左右对照那样的弧形贝塞尔连接：冲突位置以
   结果栏行号为准画在窄栏上，跟随中栏滚动。
6. **“应用”前还有冲突时的确认用 `window.confirm`**，不是自绘对话框；文案是“还有 N 处
   冲突未解决，仍要保存当前结果吗？”，两个按钮是“确定 / 取消”，语义与设计一致。
7. **C15 的两项没做**：CSS 别名改 VS Code 键名、App.tsx 拆 hook（理由见 C 节）。
8. **`stage_files` / `unstage_files` 仍是两个命令**：会话层的实现已合并成一个带
   `is_stage` 的方法，命令层保留两个入口是因为前端需要两种明确操作；合并成一个带布尔
   参数的命令只会让调用点更难读。
9. **并排视图没有做“按目录分组”等其它交互的联动验证**（无法截图）。

# 规划方接手（Q141–Q143）

返修后审查（spec.md Comments“返修后审查”G1–G4）发现按段写入、三方合并有安全与交互问题，规划方按 Q141–Q143 直接接手，提交 2aa5c53、96b9423、f664da4。

## 按段写入（G1.1–G1.7）

- 段编号统一由一张“每行属于第几段”的表生成，统一视图、左右对照、按段切补丁三处一致；`\ No newline at end of file` 跟着所属行，不再把末行修改拆成两段。
- 段区间按行推进，多行段完整；折叠区不再盖住改动，文件头尾不多留上下文。
- 按段切补丁直接改写 Git 输出的原始字节（每行保留自己的行尾），重算 hunk 头时保留下来完整一侧的起点照搬原 hunk。去掉“按显示行号取基线文本”的做法：实测 `git apply --ignore-whitespace` 会保留目标文件原有的空白，原做法在前一个 hunk 改变行数后取错行、应用失败。
- 段操作凭证（`SegmentAction.credential`）绑定签发时的忽略空白档位（Q140“指纹包含所用档位”），档位不符拒绝；前端原样传回凭证与档位。
- 暂存、取消暂存一段后按同一档位重读 diff：其余段不变才算成功，否则“结果待核对”（设计 3.2）。改名文件拒绝按段写。
- 左右对照两侧全文与 diff 逐行核对，对不上（期间文件被改）不缓存。

## 三方合并（G1.8–G1.10、G2）

- 块由 `git diff --no-index -U0` 求共同祖先到两侧的改动，按 Git 合并的规则（两侧改动在共同祖先里重叠或相接即同一处）分组，再与 `git merge-file --diff3` 逐行核对，对不上时只提供接受一侧。两者固定 myers 算法，不受用户 `diff.algorithm`、外部 diff、缩进启发式配置影响；随机生成的 150 组三方内容全部与 Git 一致。
- 二进制、符号链接、子模块、一边删除一边修改不能三栏合并；关闭“自动应用无冲突改动”时结果从共同祖先开始，每一块待处理。
- 应用与整文件接受一侧：写前校验指纹（冲突列表与合并页签签发）→ 存回收区 → 保存后再核对 → 写入（按你的版本的换行与模式）→ `git add` / `git rm` → 写后核对（不再冲突、暂存区对象等于按路径属性计算的工作区对象）。
- 合并页签改用 CodeMirror 6 的基础编辑（未加载它的 diff、merge 扩展）：三栏同步滚动、两条窄栏弧形色带、» / « / ✕、F7 冲突导航、应用所有不冲突的改动、全部接受左边 / 右边、未解决时自绘确认对话框；块状态随撤销、重做一起恢复；语法高亮走 Rust 的 syntect。

## 左右对照与其他（G3、G4）

- 色带改为贝塞尔弧形，改动底色穿过两侧行号列，修改块从删除色渐变到新增色；去掉栏内多出的按钮；悬停块按钮时两边与色带加亮；冲突文件只读对照 HEAD 与工作区。
- 通知按暂存、丢弃、提交、回收区四类开关；F7 在最后一处后按设置打开下一个文件；Git 设置页去掉外部合并工具（M6），补上“自动应用无冲突改动”开关；组件里写死的文案移进文案表。

## 自查

| 命令 | 结果 |
| --- | --- |
| `pnpm exec tsc -b` | 通过 |
| `pnpm lint` | 通过（7 条已有的 unused-vars 警告） |
| `cargo test` | 389 通过、0 失败、1 忽略 |
| `tools/test-*.mjs` | 9 个脚本全部通过（新增 `test-merge-model.mjs`） |

## 仍未做

- 软换行（E.4）：设置项已存在，但两种 diff 视图都还没有实现换行显示。
- C15 的 CSS 别名改名与 App.tsx 拆分（判断题，不影响验收）。
- D06、冒烟与真实桌面验收：机器锁屏期间无法进行，待解锁后由规划方完成。

# Codex 接手检查点（2026-10-27，未验收）

用户在本次接手期间确认合并页签也跟随软换行（Q144），随后要求直接开始 M5。
因此这里保留 M4 的实现与检查点，不继续打包、桌面验收、D06 或压缩合并，M4 不标为完成。
实施过程见 [软换行计划](./soft-wrap-plan.md)。

## 已实现但未做桌面验收

- 在交接遗留的统一视图改动上补齐左右对照软换行：两侧测量行高、按逻辑行与行内比例
  同步、折叠区映射、文件末尾插入点、两侧改动跳转、行号与弧形色带几何。
- 合并三栏通过 CodeMirror `Compartment` 动态重配软换行、字体与字号，不重建文档或撤销历史。
- C15：旧颜色别名改为现有 VS Code 主题键，保留派生色混色值；App 拆出主题与字体、通知、
  页签、树多选、批量写五个 hook。
- 修复关闭页签时误把合并页签当 diff 激活，以及无关刷新把正在编辑的合并页签切走的问题。
- 测量探针按左右两栏实际可见范围检查覆盖，不再只检查左栏。

## 本次检查

源码：`eba7446` 加本检查点同一提交内的改动；原始日志与接手前补丁保存在
`/private/tmp/revia-m4-codex.6O3W9y/`。检查在用户转向 M5 前已启动，以下只记录真实结果。

| 检查 | 结果 |
| --- | --- |
| `pnpm exec tsc -b` | 通过 |
| `pnpm lint` | 通过，7 条原有 `test-refresh-gate.mjs` unused-vars 警告 |
| 逐个 `node --test tools/test-*.mjs` | 12 个脚本中 11 个通过；`test-merge-appearance.mjs` 导入失败 |
| `/opt/homebrew/bin/cargo test --manifest-path src-tauri/Cargo.toml` | 389 通过，0 失败，1 忽略 |
| 前端 build / `.app` 重打包 | 未运行 |
| 桌面 / D06 / 冒烟 / Windows 编译 | 未运行，未验收 |

测试导入失败原因：`editorAppearance.ts` 后续复用了 `@/lib/theme` 的字体栈函数，
`test-merge-appearance.mjs` 尚未像其他 Node 脚本注册别名解析器；不能把该脚本记为通过。

## 审查未解决项

用户转向 M5 时停止了审查扩展。以下结论来自静态调用链和几何检查，不代表完整验收：

- **P1，规范轴与规格轴共同确认：合并写入绕过应用写入/退出保护。** `MergeView` 的应用、
  `AcceptOnly` 与 `ConflictDialog` 的接受一侧只设置局部 busy，直接调用合并 IPC，
  没有接入 App 的 `writingWorktrees` / `WriteGuardOrchestrator` / `CloseCoordinator`。
  关闭窗口可能中断写入；切走组件后的失败与“结果待核对”缺少全局保留。违反 Q90/Q91/Q96。
  修复时复用原 worktree 的共享编排与全局结果通知，不能仅增加局部 busy。
- **P2，统一软换行可见窗口：** 在同一很长的逻辑行内部滚动时，顶部行号未变可能不触发渲染，
  但视口底端已变化；可见末行需要参与窗口更新，尤其 compact 阶段。
- **P2，左右对照同帧行号：** 行号/按钮仍靠下一次 rAF 取 scrollTop，不与正文同步提交。
- **P2，左右对照键盘聚焦：** 预渲染的折叠/段按钮被 Tab 聚焦时，浏览器可能原生滚动容器；
  尚缺统一视图已有的原生滚动复位与焦点揭示处理。

M5 可以先开发已确认的只读功能，但以上 P1 仍是交付前阻塞项，不能因进入新里程碑而消失。
Rebased 偏好是否恢复尚未获得用户确认，本次未修改；旧 worktree 与旧 Pi 会话均保留。

## 规划方修复：合并写入的全局保护（2026-10-27）

针对上节 P1。合并页签的“应用”、接受一侧，只能接受一侧的视图，以及冲突对话框的接受一侧，
三个入口都改为调用 App 的 `performMergeWrite`，由新模块 `src/features/merge/mergeWrite.ts`
（`runMergeWrite`）执行。流程照 `performRestore`：

- 目标 worktree 在发起时确定；同步占用 `writingWorktrees`，已有写操作时拒绝且不发 IPC；
- 经 `WriteGuardOrchestrator` 建立关闭/退出保护后才发 IPC，保护失败或监听未就绪时不写；
- 成功或失败都对原目标 `refreshTarget`，重读失败不覆盖写入结果；
- 失败按原目标发持久通知（返修 C9），组件关闭后仍保留；`OperationResultGate` 防止旧结果覆盖新结果；
- 收尾只做一次，“结果待核对”登记到 `CloseCoordinator`，退出前先询问。

组件只在仍挂载时关闭页签或更新状态（`src/hooks/useMountedRef.ts`），避免写完时误关另一 worktree 的同名合并页签。
回归 `tools/test-merge-write.mjs`（8 个）：重复写拒绝、保护失败与监听未就绪不发 IPC、写入期间关闭先询问、
跨 worktree 失败归属、较新操作夺取记录资格、调用方不再等待时照常收尾且待核对阻止退出、重读失败不覆盖结果、
三个界面入口的接线检查。

检查（`/Volumes/T7 Shield/project/Revia-m4`）：`tsc -b` 通过；`lint` 通过（原有警告）；`pnpm build` 通过；
13 个 Node 脚本 12 个通过，`test-merge-appearance.mjs` 仍是上节记录的别名导入失败（m5 已修，合并 m4 后消失）。
Rust 未改动，未重跑。桌面验收未做，界面上的入口行为待真实桌面确认。

## 规划方修复：审查未解决项中的三个 P2（2026-10-27）

- **统一软换行可见末行**：`DiffView` 在软换行时另记视口底部所在的行，顶部行不变而底部进入新行时也重新渲染并补取行；
  不换行的固定行高路径不变（D06 测量路径不受影响）。
- **左右对照行号同帧**：窄栏行号与段按钮不再每帧轮询滚动位置。各栏写入滚动位移时同步移动窄栏（`onPaint` → `place`），
  只有行窗口变化才经 React 重新渲染（`onWindow` → `setWindow`），与本栏行窗口同一次提交。
- **左右对照键盘聚焦**：两栏与窄栏出现原生滚动立即复位；Tab 聚焦预渲染的段按钮或折叠行时，经该栏的自绘滚动入口
  （`reveal`）把它带进可见区，并照常同步另一栏。

检查：`tsc -b`、`lint`（原有 7 条警告）、`pnpm build` 通过；Node 脚本同上节（仅 `test-merge-appearance.mjs` 的已知别名问题）。
屏幕锁定，三项都未在真实桌面上确认，列入桌面验收。

## 真实桌面验收（第一轮，2026-10-27，规划方）

屏幕已解锁。用 `REVIA_ACCEPTANCE=1` 与独立数据目录启动 `e9344f7` 构建（之后为下面的修复重打包），测试仓库为规划方自建的 merge-rich、conflicts、diffs。本轮只用后台操作完成；需要接管屏幕的项目（弹层、悬停、与 Rebased 并排）两次申请全屏控制都没有得到确认，留到下一轮。

### 通过

| 项目 | 结果 |
| --- | --- |
| 合并页签 | “解决冲突”直接打开三栏页签并定位到第一处冲突；» / « / ✕ 逐块处理，计数从“还有 2 处冲突”到“没有未处理的冲突”；“应用”后页签关闭、文件进入“已暂存”，Git 核对：无冲突标记、两处分别取了左 / 右、MERGE_HEAD 保留、回收区新增应用前记录 |
| 冲突对话框 | 布局同 Rebased（文件、两侧状态、三个按钮）；delete/modify 的 gone.ts 标“只能接受一侧”；只选它接受对方版本后进入“已暂存”（新增），回收区新增记录 |
| 左右对照 | 两栏连续、窄栏两列行号与弧形色带、按词高亮；点 « 只暂存一段，`git diff --cached` 只有这一段；软换行时长行折成多行、色带与行号跟随；外部修改当前文件后自动刷新 |
| 设置对话框 | 搜索、可展开分类、“分类 › 页名”；通知四类开关；diff 显示各项 |
| 颜色主题 | One Dark Pro、VS Code Dark 2026 正常；浅色两套需系统浅色模式，未验收 |
| 合并写入保护（`6553772`） | 应用、对话框接受一侧都经全局写门禁完成，结果与重读归原 worktree |

### D06 与冒烟（修复后构建，窗口可见、聚焦）

| 视图 | 首屏 | 未缓存跳转 | 已缓存跳转 | 滚动 60 帧 |
| --- | --- | --- | --- | --- |
| 统一 | 104 / 105 / 105 ms | 22 / 16 / 14 ms | 1 帧内（3–10 ms） | 未覆盖 0 |
| 左右对照 | 211（新包首次冷启动）/ 151 / 128 / 135 ms | 38–57 ms | 1 帧内（同步检查） | 未覆盖 0，两栏同步误差 0 |

左右对照首屏卡在 150 ms 门槛附近（4 次中 2 次超出），其余各项达标。冒烟（conflicts）通过：并排两栏与色带已绘制，合并页签三栏、“还有 1 处冲突”、结果 6 行。

### 本轮修复（m4）

- 左右对照的跳转测量补同步的 1 帧判定（`jumpWithinOneFrame` / `warmJumpWithinOneFrame`），口径同统一视图；原先只记录含两次重绘等待的耗时，33 ms 是测量本身的开销。
- 设置 › Git 页“合并时自动应用无冲突改动”重复出现两次，删掉一个。
- 复选框的勾改用 `checkbox.foreground`（缺省为编辑器前景色）；原先用边框色，在 One Dark Pro 中几乎看不见。
- 提交按钮禁用态照 VS Code：透明度 0.4、默认光标。
- 提交区“进行中操作”提示几句之间补句号。
- 冒烟的合并页签检查改读 CodeMirror 编辑器状态，并给合并页签补回 `data-merge-view`、`data-merge-conflicts` 标记。

### 发现但不属于 M4

- 系统菜单栏仍是 Tauri 默认英文菜单，未按 Q38 做中文原生菜单；M4 规格没有要求，记入 [backlog 03](../backlog/03-native-menu-bar.md)。

### 仍未验收

- 需要接管屏幕：弹层是否被遮挡、圆角、色带悬停加亮、忽略空白三档的下拉选择、与本机 Rebased 并排对照（Rebased 的信任对话框只接受前台点击）。
- 浅色两套颜色主题（需系统浅色模式）。
- 左右对照首屏 150 ms 门槛的余量。
- M2、M3 与界面相关的验收、Windows 编译。

## 真实桌面验收（第二轮，进行中，2026-10-27）

全屏控制已获批准。**发现 P1：所有弹层都打不开**（仓库、分支下拉，diff 选项，提交选项，页签菜单）。原因：`src/components/ui/popover.tsx` 只在 Base UI 通知“关闭”时调用 `onClose`，“打开”通知被忽略；而 7 个调用点的触发按钮都没有自己的 `onClick`，受控的 `open` 永远是 false。M5 第二批（`m5-batch2`）的 Pi 已按同一原因修好（`Popover` 增加 `onOpen`，调用点传入）；m4 需同样修复后重打包，再继续本轮：弹层遮挡与圆角、色带悬停、忽略空白三档、与本机 Rebased 并排对照。


## Codex 接手：恢复弹层与准备第二轮验收（2026-10-27）

基点 `329f962`；源码为本节同一提交内的修复。未放宽 D06 的 150 ms 首屏门槛，未合并或推送。

- `Popover` 新增必填 `onOpen`，同时转发 Base UI 的打开与关闭通知；仓库、worktree、diff 选项、页签更多、最近提交信息、提交选项和变更更多的 7 个调用点全部接入。
- 修复 `tools/test-merge-appearance.mjs` 的 Node 别名与 TypeScript 扩展解析，保留现有生产外观模块测试。
- 两轴独立静态复核未发现本次 M4 修复的新问题；不能据此代替弹层真实桌面验收。

集中验证：`git diff --check`、`pnpm lint`、app/node 两个 TypeScript 项目通过；
`node --test tools/test-merge-appearance.mjs` **2/2**；`pnpm build` 退出码 **0**。
原有 lint 警告保留。Rust 源码未改，本次不重复跑整套 Rust 测试。

原始证据（包含命令、退出码、stdout/stderr、源码差异）：

- `/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-takeover-checks-p5howaqm`
- `/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-typecheck-iudquxzn`
- `/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-scoped-build-8uto0sb9`

此前构建失败是当前会话沙箱拒绝写 worktree 的 `node_modules/.tmp/*.tsbuildinfo`（TS5033），
不是类型错误。用户授权仅这两个 worktree 与 Paseo 所需操作后，已按限定命令完成构建；
原失败证据保留在 `revia-m4-takeover-sq650c4l`，不覆盖。

下一步从本修复固定提交重建 Release `.app`，使用本次独占 fixture 和应用数据目录继续第二轮。
弹层边界、圆角、色带悬停、忽略空白三档、四套主题、Rebased 对照和 D06 仍以实际桌面结果为准。

## Codex 第二轮桌面验收与 D06 性能接手（2026-10-27）

### 弹层与原始性能样本

固定源码 `a0ba41513c0d520c79370a52e314642230d493db`，离线重建真正的 Release `.app`；
二进制 SHA256 为 `05f0be60810f02231989dddf664ac71c54441820667a2a51cd39d5b33d452039`。
构建证据：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-release-l8ec_f_1`。

本轮独占 fixture / 应用数据根为
`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-desktop-9c5heep6`，含创建记录。
普通实例与测量实例的数据目录分开；Rebased 使用 `fixtures/diffs-rebased` 副本，
不让对照工具写入 D06 使用的 `fixtures/diffs`。

普通模式 `evidence/normal-01` 已逐个打开仓库、worktree、提交选项、最近提交信息、
变更更多、页签更多、diff 显示选项七个弹层；均可见，当前窗口尺寸下未被上下或右边界裁剪，
Escape 可关闭。逐项 PNG 原件保存在该目录。此结论只属于该固定包和已测窗口尺寸，
不代替更小窗口、四套颜色主题或完整 Rebased 对照。Rebased 1.1.22 的信任对话框已处理，
只信任本轮自建副本，未选择信任整个父目录。

| 样本 | 首屏 | listChanges | firstRows |
| --- | ---: | ---: | ---: |
| split-01（新包首进程，无预热） | 227 ms | 124 ms | 102 ms |
| split-02 | 151 ms | 56 ms | 95 ms |
| split-03 | 142 ms | 51 ms | 90 ms |
| unified-01 | 124 ms | 49 ms | 75 ms |

以上样本窗口均可见且聚焦，focusLosses=0。split 三次滚动各 60 帧、uncovered=0、
syncError=0，未缓存跳转约 38–50 ms。unified 该次跳转目标已缓存，不把 14 ms 冒称
未缓存跳转成绩。`result.ok=true` 仅说明采样有效，不代表 D06 达标；
`renderMs` 与 `firstRowsMs` 是同一段计时，不重复相加。原始失败样本全部保留。
**split 未稳定达到 150 ms，M4 仍阻塞，不合入 main。**

### 本次性能修改与集中检查

按 Q141 由规划方接手，基点仍为 `a0ba415`：

- 两栏按真实 RowHeights 可见范围先取小片，跳过折叠占位；两侧可见数据齐备后继续补原有
  400 行块与 overscan。空侧不阻塞预取，过期代际不发布，失败和空响应不驱动渲染重试环。
- Rust 一次汇总两视图行数和 hunk 锚点，纯计数不分配段归属；folds 复用现有 segments，
  side_lines 只标记请求范围内的改动行。保留 NoEol 语义、Git 读取、一致性检查和指纹门禁。
- 未修改滚动几何、测量起点、nextPaint、两侧覆盖判定或 D06 门槛。

#### Standards

独立只读审查：0 项确定规范违规或需要返修的代码异味。

#### Spec

独立只读审查：0 项确定规格缺陷、遗漏或范围扩张。小片加整块增加 IPC 次数，
必须重测快速滚动、未缓存跳转、折叠和软换行；静态通过不能证明性能达标。

集中验证通过：Node **68/68**；Rust rows **10/10**、segments **10/10**、
segment_staging **22/22**；`git diff --check`、`pnpm lint`、`pnpm build` 退出码均为 0。
lint 原有未使用变量警告保留。真实 Git 分页回归对两侧及 word/char/line 三粒度逐字段比较。
原始源码差异、文件 SHA256、命令、退出码、stdout/stderr 保存在
`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-perf-checks-maqwtzga`。

本节提交后重新构建 Release，首个新进程起保留全部测量样本，再决定是否达标。
尚未合并或推送；剩余界面验收、完整门槛仍按实际证据记录。

### 固定包 b612998 复测与第二次性能收敛

Release 构建退出码 0，二进制 SHA256
`0485b0e2e0e87433d6884ef1d935fab88a573e3b322f9cc88e6dfa8737fc0f15`；构建原件在
`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-perf-release-0c2id618`。
同一独占 fixture、各自全新数据目录、独立进程的三次 split 首屏为 **147 / 143 / 152 ms**，
listChanges 为 51 / 47 / 51 ms，firstRows 为 95 / 96 / 101 ms；前台有效、失焦次数 0。
三次未缓存跳转均 49 ms，滚动各 60 帧、未覆盖 0、同步误差 0。
第三次仍超 150 ms，停止继续刷样本，**不判稳定达标、不合 main**。
没有清除操作系统文件缓存，不能把相对上一包冷样本的全部差值归因于代码优化。

unified-02 首屏 127 ms（listChanges 48、firstRows 79），滚动 60 帧、未覆盖 0。
其跳转目标仍已缓存；附加颜色探针部分不匹配，未据此宣称全部 D06 项目通过。
smoke-01 退出后的结果显示左右两栏与窄栏 canvas 已绘制，三栏合并页签存在、结果 6 行、
提示“还有 1 处冲突”。原始 JSON 保留，不将单个 `ok=true` 扩张为所有界面门槛通过。
全部样本在前述桌面证据根的 `evidence/split-04` 至 `split-06`、`unified-02`、`smoke-01`。

继续收敛两项已证实的重复计算（本节提交的源码）：

- 同一快照的重叠取行回包只补缺失行，保留已有行和词区间引用；删除行组件不使用却会
  击穿 memo 的 `highlightVersion` 参数，缺失行使用共享空词区间。实际 tokens 与显示档位
  仍参与 memo，完整缓存键和旧代际拒收不变。
- 派生左右布局时只读借用当前模型，派生成功后才写快照缓存，避免深复制整个 diff；
  相同的段凭据只计算一次。保留 Git 读取、全文一致性核对、缓存界限和错误语义。

Standards 独立审查 0 项；Spec 独立审查 0 项。未改 effect 时机、预取时序、计时边界或门槛。
集中检查：Node **69/69**、真实 Git `segment_staging` **22/22**、diff check、lint、build 通过。
证据：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-memo-checks-20my3h3x`。
新包仍须从首进程开始复测；纯静态与检查通过不代表首屏性能、主题或完整桌面验收完成。


### 固定包 0417078：普通模式体验验收与两项补修

真正 Release 二进制 SHA256 为
`898d306d909b4aec32809de36601fcd0b672b8fe392fdfa7fa86f9e7f411e5c5`；构建原件在
`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-memo-release-ommm2hsd`。
首个测量进程 split-07：首屏 139 ms（listChanges 48、firstRows 91），未缓存跳转 56 ms，
热跳转 27 ms 且在一帧内；滚动 60 帧、未覆盖 0、同步误差 0，前台有效且失焦 0。
当前固定包只有这一个样本，不据此判稳定达标；不改变 150 ms 门槛，也不删除原失败样本。

用户要求优先检查实际操作、普通模式不显示测量面板。normal-03 启动时清除继承的全部
`REVIA_*`，仅设置 `REVIA_ACCEPTANCE=1` 与本轮隔离数据目录；欢迎页、添加仓库、
打开 diff、切换文件、并排/统一切换均未出现“测量 / 导出 JSON / 清空”区域。
普通模式原本就不挂载该面板，本次未改测量门禁。

在约 961 x 521 逻辑点的小窗口中检查七处弹层：仓库、worktree、最近提交信息、提交选项、
变更更多、页签更多、diff 显示选项，均能打开且未被窗口边界裁剪；Escape 可关闭。
长 diff 菜单内部滚动后能到达上下文行数，设置对话框能滚到最底部忽略空白项。
软换行开/关、App.vue 与 Model.swift 切换、大文件展开、滚轮正反滚动、键盘翻页、
差异导航和并排/统一切换均有响应；观察点没有持续空白或卡死。
这些是操作后的真实桌面观察，不冒称连续帧率或完整长测通过。
证据在前述独占桌面根的 `evidence/normal-03/`，D06 的 `fixtures/diffs` 全程只读。

发现并补修（按 Q113 接管，不再交回 Pi）：

1. **P2 页签标题消失**：窄窗口下多页签被 flex 收缩至文件名宽度为 0。
   让页签保持内容宽度，沿用横向滚动；新增/切换活动页签及窗口/侧栏尺寸变化后露出标题。
   滚动使用同一未缩放布局坐标，避免 80%–150% 界面缩放时欠滚或过滚。
2. **P1 统一视图段操作全禁用**：正文段首行只有 segmentAction，却误用要求 hunkAction
   的标题行门禁。独立判断实际段凭据，保留写入中、取行错误、只读档位和完整目标身份校验；
   不修改 Rust 写入、安全校验或签发凭据。删除/替换段的丢弃按钮不再误报“第 0 行起”。

回归加入现有 Node 测试，未引入框架。集中检查 Node **53/53**、真实 Git
segment_staging **22/22**、diff check、lint、前端 build 通过。证据在
`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-normal-ui-checks-9p4s247t`。
审查发现缩放坐标问题后修复，只重跑受影响的 lint/build/diff check，均通过；原件在
`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-normal-ui-recheck-hz6orfyg`。
补修尚须固定版本重新打包，以普通模式复验页签和自建独立 fixture 上的统一视图操作。
M4 整体验收仍未完成，不合 main、不推送。

### 固定包 71ce04c：普通操作复验与紧凑布局、显示导航补修

Release 二进制 SHA256 为
`63ac38f3b3b32000ac159123ac9e5b68313fac6ee165b263d045c3df278c8795`，构建原件在
`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-normal-ui-release-6j52gfhd`。
normal-04 继续普通模式，清除继承的全部测量变量，复用本轮隔离验收数据。
重启、切仓库、并排/统一切换和写操作期间均未出现测量面板。

在本轮新建的 `fixtures/whitespace-ui-sajuqrsc` 中，通过真实界面暂存第 4 行所在块，
只读 Git 核对只有该块进入暂存区，再通过已暂存文件的“取消暂存此块”还原。
HEAD、index blob、工作区 SHA256、暂存/未暂存 diff 和 status 均与操作前逐项一致。
丢弃确认框完整显示后选择“取消”，没有执行丢弃；D06 的 `fixtures/diffs` 始终只读。
证据在前述独占桌面根的 `evidence/normal-04/`，包括 `after-unstage.json` 与截图。

100% 小窗口及 80% 下，活动页签文件名与自动露出行为正常。CUA 的横向手势没有观察到
位移，不据此声称页签手势滚动通过。150% / 961 x 521 逻辑点暴露额外问题：

1. 工具条把页签挤到 0 宽并越过窗口右侧；提交区把文件树挤到近 0 高。
   按编辑区实际容器宽度分行，窄处只隐藏重复计数与长文字，保留图标、名称和操作；
   提交区整体限高并内部滚动，给树保留四行，不删除警告、确认或 hooks 输出。
2. 分隔条实际命中点位于窗口最左边，而不是侧栏右边。补正确定位，拖动按实际缩放
   比例换算；审查进一步发现 720px 固定宽度可压没编辑区，已按容器限宽并保留 240px。
3. 统一视图忽略空白后正文正确显示 4 / 3 / 2 / 1 块，计数却始终为 4；源码确认 F7
   也使用原始 hunk 坐标。新增独立显示导航元数据，与过滤后正文同批派生，分页携带完整
   段首数组；计数、面包屑、工具条和 F7 共用它。软换行使用逻辑行，页尾记住刚跳过的块。
   不覆盖原始 ChangeEntry，不修改指纹、快照或写凭据，旧代际照常拒收。

集中检查：Node **70/70**、Rust rows **10/10**、真实 Git segment_staging **24/24**，
`pnpm bindings`、`git diff --check`、lint、前端 build 均成功。既有 lint 警告保留。
类型生成带来的其它文件纯行尾空白变化已逐一核对并排除，仅保留 RowSlice 契约变更。
原始命令、退出码、stdout/stderr 和源码差异在
`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-compact-checks-e19e2iyt`。
侧栏限宽补修后仅重跑受影响的 diff check、lint、build，均成功，原件在其 `ui-cap-recheck/`。

Standards 最终独立审查 **0 项**；Spec 最终独立审查 **0 项**。前次 Spec 的侧栏限宽 P2
已处理，未将前次发现抹掉。本节修复仍须固定版本重新生成真正的 Release `.app`，在真实
WebView 复验高缩放、侧栏拖动、弹层与过滤后的操作；检查通过不等于桌面或 D06 通过。
没有追加性能刷样本，M4 完整门槛仍未完成，不合 main、不推送。

### 固定包 686a93e：布局、导航复验与高缩放弹层补修

真正 Release 构建成功，二进制 SHA256 为
`177c8031b4dbae3fd514b29f9d9b20e750014a07269480bc9b67ed6b9c3a0663`；构建原件在
`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-compact-release-qvhufgrg`。
普通进程 normal-05 仍只使用本轮隔离数据，不显示测量面板。

真实桌面结果：

- 100% 宽窗保持单行页签工具条；961 x 521 / 150% 下页签、工具图标完整可达，文件树
  保留四行高度并独立滚动，提交区可内部滚动。分隔条向左跟随鼠标移动；向右拖至窗口
  边缘后停止在上限，编辑区仍可见，最右工具换行而不是消失。
- 统一视图忽略行尾空白后计数为 3、忽略所有空白后为 2；连续 F7 到最后真实改动，
  两次 Shift+F7 回到第一处可见段首。全部变化被忽略时显示 0，上一处/下一处禁用。
- 在独立 whitespace fixture 的“忽略所有空白”档位暂存语义块，真实 Git 核对只把
  `value before` 改为 `value after` 纳入暂存区，尾空格、行内空格及新增空行仍留在工作区。
  切已暂存文件后取消暂存同一块，HEAD、index blob、工作区 SHA256、完整 diff、status
  六项全部恢复到操作前。未执行丢弃，未写 D06 fixture。

**另发现 P2，尚不能把弹层验收写为通过：** 150% 下顶部 diff 菜单向右越界；提交区滚动后
打开底部提交选项，AX 中存在 checkbox，但弹层不在可见区域，焦点还会带动整页向上偏移；
Escape 恢复。截图在 `evidence/normal-05/commit-options-150.png` 与
`diff-options-150-before-zoom-fix.png`，保留失败证据。

本地已装依赖源码确认：html CSS zoom 生效，但静态 Portal 下的 absolute Positioner
定位父元素退化为 window，未按 HTMLElement 的缩放比例转换 client rect，坐标再次被
放大。补完整视口 fixed Portal 定位层，让 Floating UI 正确读取比例；定位层不拦截点击，
Positioner 恢复指针事件，弹层内容按可用宽高内部滚动。没有改全局缩放、碰撞避让或焦点
管理，没有用禁用焦点滚动掩盖越界。

仅两文件局部 UI 补修，未引入测试框架或重复运行 Rust 套件。diff check、lint、build
均成功；原始证据在
`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-popover-checks-ev_7lcro`。
Standards 独立审查 0 项，Spec 独立审查 0 项；仍须重新固定打包，复验 80%/100%/150%
顶部、底部、长菜单及外部点击/Escape 关闭。M4 不合 main、不推送。

### 固定包 ca0f2a0：弹层复验通过，补修视图切换丢失阅读位置

2026-10-28，真正 Release `.app` 构建成功；二进制 SHA256 为
`078ba9c5ef7aec3a106bb406c61ace7727e5f2e0e879c76f480e55bd0ea2811f`，构建原件在
`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-popover-release-qd_t741a`。
normal-06 的启动清单与包哈希一致，只设置 acceptance 及本轮独立数据目录，不启用测量。

本轮真实桌面结果（截图在既有独占桌面根的 `evidence/normal-06/`）：

- 961 x 521 逻辑点、150% 缩放下，仓库、worktree、最近提交信息、提交选项、变更更多、
  页签更多和 diff 显示选项七处弹层全部可见，没有越过窗口边界或被上下区域裁剪。
  长 diff 菜单内部滚动可到上下文行数；底部提交菜单不再把整页顶走。外部点击可直接切到
  另一菜单，Escape 可关闭，未出现透明全屏层截住点击。失败原件 normal-05 保留。
- 80% 与 100% 的顶部、底部菜单均完整可见。80% 拖动侧栏边界约 80 截图像素，边界
  随之移动约相同距离。恢复 100% 宽窗口后，页签与工具栏正常显示，测量面板始终未出现。
- App.vue / Model.swift 切换、大文件展开、统一视图正反滚动与并排两栏滚动均有响应；
  观察点正文已绘制，没有持续空白或卡死。这是操作后观察，不冒称连续帧率或长测通过。
- **发现 P2**：大文件统一视图读到 `transform0029` 附近，切并排回到文件开头；反向路径
  也使用已不再绑定的旧 `diffRef`。失败截图为 `view-switch-unified-scrolled.png` 与
  `view-switch-split-reset.png`。两栏首行偶尔相差一行属于既定中线映射与首尾夹紧，
  不为强行对齐首行而改同步算法。

按 Q113 / Q141 接管补修：

- 两视图以带 old/new 版本的文件行锚点交接；统一视图按实际逻辑行找正文，跳过标题及
  缺行，不再用像素除固定行高。并排从主动栏读取文件行，等当前布局与视口就绪后恢复，
  仅展开含锚点的折叠区，沿原有同步规则定位另一栏，软换行量高后保持锚点。
- 锚点捕获时固定 worktree、文件、选择代际、激活、快照、指纹及显示选项。每次异步
  完成后再核对，快速切回只复用同身份的待恢复锚点；迟到的设置保存结果不回写视图。
- 统一视图定位 RPC 增加可选忽略空白档位与期望版本，同一快照锁内校验后复用取行模型；
  定位后主动请求对应行。全文尾部超出统一 diff 时降级到最后可见正文，不再跳回首屏。
  默认请求与原入口保持 Off，Git 差异来源、写凭据、安全门禁和测量边界不变。
- 原冒烟的视图切换也改为真实并排句柄与文件锚点，不能继续用空 `diffRef` 证明通过。

集中检查：Node **72/72**、Rust rows **10/10**、真实 Git segment_staging **25/25**；
diff check、lint、前端 build 均为 0。原始命令、退出码、stdout/stderr 和源码差异在
`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-anchor-recheck-00nxhzq6`。
第一次新回归误把带行号的 hunk 标题当正文，已修测试遍历条件；失败日志保留在
`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-anchor-checks-_mkwx77r`。
既有 lint / Vite 警告仍在，不称零警告。Standards 最终 **0 项**；Spec 最终 **0 项**，
首轮 Spec 的快速切回和全文尾部两项 P2 已补修，未抹掉原意见。

本节锚点补修仍须固定源码重新打包后真实桌面复验。D06 稳定首屏、完整颜色探针与长测、
四主题和完整 Rebased 对照仍未完成；未追加性能刷样本，不合 main，不推送。

### 固定包 ebb3232：普通模式体验与阅读位置复验

2026-10-28，源码固定为 `ebb32325dc238d2fbfd6a1d594ffc50d0730d728`。Release `.app`
二进制 SHA256 为 `8b7359435a865dba5f4a7407d0f3bff8d0882522aa187edc7832d32f5e669e01`；
离线构建成功，原始命令、退出码及输出位于
`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-m4-anchor-release-p4y0vmxk`。
normal-07 进程为 83110，启动清单与实算包哈希一致；只传入 `REVIA_ACCEPTANCE=1` 和本轮
独占数据目录，没有测量、冒烟或长测环境变量。证据位于既有独占桌面根的
`evidence/normal-07/`，本节只补验收记录，不再改源码或重建包。

真实桌面结果：

- 普通启动、大文件展开、切文件、切仓库、切视图、缩放和打开菜单均没有测量面板。
  最后恢复 100%、1280 x 800 逻辑点窗口，在只读 diffs fixture 的 `src/App.vue` 保持
  并排审阅界面；`ordinary-start-no-panel.png` 与 `ordinary-final-no-panel.png` 留证。
- 大文件从并排左栏以 Home / Page Down 固定到旧第 34 行，切统一再切回，均保留该文件行；
  连续两次点击切换也未回文件头。对应 `split-keyboard-position.png`、
  `unified-after-keyboard-switch.png`、`split-keyboard-roundtrip.png` 和
  `rapid-roundtrip.png`。此前滚轮截图受惯性影响，只证明未回首屏，不作为精确行号证据。
- 开启软换行，从并排右栏固定到新第 31 行附近，切统一再切回仍保持这一版本的文件行，
  没有误用左栏旧行号；允许最上方部分可见行对齐到整行，不要求像素偏移完全相同。
  截图为 `softwrap-right-before.png`、`softwrap-right-to-unified.png` 与
  `softwrap-right-roundtrip.png`。
- 独立 whitespace fixture 同时开启“忽略所有空白”和“折叠未改动部分”，计数为 2；
  展开尾部后以 End 滚至全文尾部，切统一留在最后可见 diff 正文，未返回第一处改动；
  再切并排仍在末段，折叠与正文可继续操作。截图为 `whitespace-folded-ignore-all.png`、
  `ignore-all-tail-before.png`、`ignore-all-tail-to-unified.png` 和
  `ignore-all-folded-roundtrip.png`。全文与 diff 长度不同，视口到底时会夹紧，不把这一项
  写成隐藏尾行精确保留。当前样例顶端锚点为新第 53 行，恰为最后可见正文；严格超出最后
  hunk 的回退由前述 Rust 回归覆盖，本轮桌面没有独立覆盖。尝试继续放大时界面已到 150%
  上限，`ignore-all-tail-zoom-limit-attempt.png` 不代表 160% 验收。
- 新包在 961 x 521 / 150% 下抽查顶部 diff 菜单和底部提交选项，弹层均在窗口内完整
  显示，无上下裁剪或整页被焦点顶走；长菜单内部滚动可到上下文行数，Escape 正常关闭。
  截图为 `diff-options-150-final.png`、`diff-options-150-bottom-final.png` 与
  `commit-options-150-final.png`。150% 七入口及 80% / 100% 顶部、底部菜单的既有结论
  仍以 normal-06 为准，不把本次两入口抽查写为重复全量验收。
- normal-07 全程仅浏览，没有暂存、取消暂存、丢弃或提交。只读核对 whitespace fixture
  的 HEAD、index blob、工作区 SHA256、暂存 diff、未暂存 diff、status，与 normal-05
  操作前六项均一致；原始命令与输出在 `final-fixture-readonly-check.json`。D06 fixture
  未执行任何 Git 写操作。

本轮用户关心的普通模式、弹层遮挡及已复现的阅读位置回退，重点项复验通过。集中检查沿用
固定源码的 Node 72/72、Rust rows 10/10、真实 Git segment_staging 25/25，以及成功的
diff check、lint、前端 build；独立证据核对确认构建和 normal-07 对应同一源码与二进制。

边界：普通连续点击不等于注入慢 IPC 的桌面时序测试；未单独桌面验证锚点落入折叠区后
自动展开该区的分支。操作后观察没有卡死或持续空白，不等于连续帧率、完整 D06 或 30 分钟
长测通过。本轮没有新跑 smoke，不沿用旧包性能成绩；色带悬停、横向页签手势、四主题、
完整 Rebased 对照、稳定首屏门槛与颜色探针仍按前文未完成项处理。M4 整体验收尚未完成，
没有合入 main，没有推送。

# 合入 main（2026-10-28）

用户认为 M5 可以了，M4 随 M5 带已知未做项合入 main（[Q154](../../README.md#已确认)）。
上面“已实现但未做桌面验收”的各项与合并写入三个入口的桌面验收转入 [backlog 06](../backlog/06-m4-m5-desktop-acceptance.md)；
D06 与长测的最新结果见 [M5 报告](../m5/report.md) 22.19 与第 23 节。
