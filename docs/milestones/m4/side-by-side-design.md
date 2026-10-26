# M4 diff 左右对照：实现设计（Q138）

本设计供 Pi 在 M4 返修中实现 Q138，规划方给出数据格式、算法和验收项，实施时照此做，不要另起炉灶。统一视图保持现状，只改并排视图。效果参照 `feedback/2026-10-27-rebased-side-by-side.png`。安全边界不变：块来自 Git，写操作用同一个块生成补丁并校验指纹（ADR-0004、ADR-0009、Q140）。

## 1. 要做成什么样

```
┌ 已暂存  src/App.vue ───────────────┬──────────┬ 工作区 ───────────────────────────┐
│ 1  <template>                      │ 1 ╲  ╱ 1 │ <template>                        │
│ 2    <div class="counter">   ░░░░░ │»2  ╲╱  2«│   <section class="counter">  ░░░░ │
│ 3      <span>{{ label }}</span>    │ 3      3 │     <strong>{{ label }}</strong>  │
│ 4    </div>                  ░░░░░ │ 4  ╱╲  4 │     <button …>加一</button>  ░░░░ │
│ 5  </template>                     │ 5 ╱  ╲ 5 │   </section>                      │
│                                    │        6 │ </template>                       │
└────────────────────────────────────┴──────────┴───────────────────────────────────┘
```

- 左右两栏各自连续显示整个文件，不插空行。未改动的部分可以折叠（Q139 的“折叠未改动部分”），折叠时两边同时折。
- 中间窄栏宽约 96px，从左到右依次是：
  - 左侧行号列，含 »；
  - 弧带区，约 24px，画连接两边对应块的弧形色带；
  - 右侧行号列，含 «。
- 改动块整块用淡底色，块内改动的字词用实色（单词级高亮，Q139 的“差异高亮方式”默认按词）。颜色取颜色主题的 `diffEditor.insertedLineBackground`、`removedLineBackground`、`insertedTextBackground`、`removedTextBackground`；修改块两边各用删除色和新增色。
- 栏头：左右各写一侧是什么，再写路径。对应关系如下：

  | 文件所在组 | 左侧 | 右侧 |
  | --- | --- | --- |
  | 未暂存 | 已暂存 | 工作区 |
  | 已暂存 | HEAD | 已暂存 |
  | 未跟踪 | 空 | 工作区 |

- 页签工具条：
  - 上一处 / 下一处改动（⇧F7 / F7）；
  - ← 第 n / m 个文件 →（按改动树顺序切换）；
  - “N 处改动”；
  - 并排 / 统一；
  - 齿轮菜单，放 diff 显示选项，与设置同步（Q139）；
  - 标记为已审阅；
  - 更多。

## 2. 块按钮的含义

界面上的“块”就是一段连续的改动，即色带连起来的那一段：中间隔着至少一行未改动的，算两块（Q142）。每块一个按钮，放在窄栏里该块的首行上；悬停时显示中文提示，并把这一块的两边和色带一起加亮。

| 文件所在组 | 右侧 « | 左侧 » |
| --- | --- | --- |
| 未暂存 | 暂存此块 | 丢弃此块（确认后先存回收区） |
| 已暂存 | 无 | 取消暂存此块 |
| 未跟踪 | 无（整文件操作） | 无 |

- 一个 Git hunk（`--unified=3`）里可能有几段改动。每段都有自己的按钮，只作用于这一段，补丁由 Git hunk 按段切开得到，见第 3.1 节。不要改用 `-U0`。
- 统一视图同样按段操作：每段首行的行号旁放按钮，原来的 hunk 标题行只显示 `@@` 信息。
- 忽略空白的三档照 Q140 处理：前两档同样按段切开；“同时忽略空行”一档下按钮变灰。
- “暂存此块”等文字按钮不再占一整行，键盘与菜单入口保留，作用于光标或选中所在的那一段。
- 有未解决冲突的文件按第 3.2 节处理，不显示块按钮。

## 3. 数据：Rust 层

新增两条命令；旧的配对行仍供统一视图使用。

```rust
/// 一次取全：两侧行数、改动段、hunk 操作目标、折叠区。
pub struct SideBySideLayout {
    pub left_label: String,            // “已暂存”“HEAD”“空”
    pub right_label: String,           // “工作区”“已暂存”
    pub left_line_count: u32,
    pub right_line_count: u32,
    pub segments: Vec<ChangeSegment>,  // 按位置排序
    pub hunks: Vec<HunkInfo>,          // 含现有的 HunkAction（与显示同批签发）
    pub folds: Vec<FoldRange>,         // 未改动且超过 2×上下文行数的区间，两侧行号各一份
    pub fingerprint: String,
    pub snapshot_revision: String,
}
pub struct ChangeSegment {             // 0 基，左闭右开；纯新增时 left_start == left_end
    pub hunk_index: u32,
    pub left_start: u32, pub left_end: u32,
    pub right_start: u32, pub right_end: u32,
    pub kind: SegmentKind,             // Added | Deleted | Modified
}
pub struct SideLine { pub line: u32, pub text: String, pub changed: bool, pub words: Vec<(u32, u32)> } // words：改动字词的 UTF-16 区间

/// 按需取一侧的若干行（虚拟滚动用），与现有 500 行分块缓存同样的用法。
fn side_lines(target, side: Side, from: u32, to: u32) -> Vec<SideLine>;
```

- 两侧全文：
  - 左侧与右侧都从同一次快照读取（`Git::file_at` 读 blob，工作区读文件），和 diff 输出一起缓存在会话里，指纹覆盖它们；
  - 读取失败或文件被收起（lock、二进制、超大，Q32）时，不进入左右对照。
- 改动段与 hunk：解析同一次 `git diff --unified=3` 的输出（忽略空白时加 Q140 的参数），把每个 hunk 里连续的删除行、新增行合成一段，行号换算到两侧全文。
- 单词级区间：每段里删除行与新增行逐行配对，在 Rust 里按词（字母数字串、单个标点、空白串）做 LCS，得到改动字词的区间。行数不等时多出的行整行算改动。单行超过 2000 字符或单段超过 500 行时不算单词级，只标整行。
- 语法高亮沿用 `highlight.rs`，按侧取。
- 每段的操作目标（`SegmentAction`：worktree、路径、所在组、hunk 编号、段在 hunk 内的序号、指纹、快照版本）与布局同批签发。前端原样传回，不自己拼补丁，也不按序号重建。

### 3.1 按段切开 Git hunk

按段写的补丁由 Rust 层从缓存的同一个 Git hunk 改写得到。这和 `git add -p` 拆块是同一种做法，符合 ADR-0004 “在 Git 给出的 hunk 上改写补丁”的约束：

- **暂存一段**（正向应用到暂存区，`git apply --cached`）：
  - 目标段的删除行、新增行原样保留；
  - 同一 hunk 里其他段的删除行改成上下文行（它们仍在暂存区里），新增行去掉；
  - 上下文行原样保留。
- **取消暂存一段**（`git apply --cached --reverse`）、**丢弃一段**（作用于工作区，`git apply --reverse`）：
  - 目标段原样保留；
  - 其他段的新增行改成上下文行，删除行去掉。
- 改写后重算 hunk 头 `@@ -a,b +c,d @@` 的行数；补丁里只含这一个 hunk。
- “\ No newline at end of file”标记必须跟着它所属的行：所属行被去掉，标记也去掉；所属行改成上下文行，标记保留。
- 忽略空白前两档：段来自 `-w` 等选项的 hunk，应用时加 `--ignore-whitespace`（Q140）。
- 补丁只保留上下文校验的严格应用：不用 `--unidiff-zero`、`-C0`、`--reject`、`--3way` 来放宽或“强行”应用。

### 3.2 冲突与应用失败

- **有未解决冲突的文件**（Git 状态为 unmerged，例如 UU、AA）：
  - Git 对这类文件输出合并格式（`diff --cc`），不能按块写；整文件的暂存、丢弃同样禁用（Q82）。
  - 左右对照只读显示：左侧为 HEAD，右侧为工作区（含冲突标记）。冲突标记之间的区域用颜色主题的冲突色标出，窄栏不放按钮。
  - diff 页签顶部提示条写明“有未解决的冲突，解决后才能暂存”，并提供“用合并工具打开”：
    - 这是 Q82 原定在 M4 的入口；
    - 由 Rust 层运行 `git -c merge.tool=<设置中的合并工具> mergetool --no-prompt -- <路径>`。用 `-c` 临时指定，不改 Git 配置（Q28）；
    - 设置里没有合并工具时，先让用户选一次并存进 Revia 设置；
    - 工具退出后重新读取状态与 diff。
  - M6 改成内置三方合并（三栏），M4 不放 M6 的入口。
- **应用失败**：
  - `git apply` 对一个补丁要么全部应用、要么都不应用；
  - Git 拒绝时（例如写前校验之后文件又被外部改动）什么都不写，用失败通知报告 Git 的原话，并重新读取 diff；
  - 丢弃一段时，回收区里已存的那份保留不动。
- **写前与写后**：
  - 写前照 ADR-0009 校验指纹，快照变了就拒绝并刷新；
  - 写后重新读取 diff：确认目标段已移走，同一文件其他段的内容不变；对不上就标“结果待核对”（Q91）。
- **同一文件既有已暂存又有未暂存的改动时**：
  - 未暂存视图是“已暂存 vs 工作区”，已暂存视图是“HEAD vs 已暂存”，两者都从同一次快照算；
  - 在一个视图里写完，另一个视图随刷新更新，不复用旧的段。

## 4. 前端：两栏、同步滚动、窄栏

### 4.1 结构

- 一个 `SideBySideView` 组件，内含左栏、窄栏、右栏。两栏各自一份 `rowWindowStore` 式的行缓存（按侧取 `side_lines`），行高固定 18px，与现有一致。
- 折叠后的“显示行”与文件行号的换算：在 `layout.folds` 上建前缀和，提供 `lineToRow(side, line)` 和 `rowToLine(side, row)`。两侧折叠区一一对应，所以折叠后两侧的段坐标仍可换算。

### 4.2 同步滚动

把行映射写成纯函数，放在 `src/features/diff/syncScroll.ts`，并写 node 测试 `tools/test-sync-scroll.mjs`：

```ts
// segments 按 leftStart 排序；行号为浮点数（行内位置）。
export function mapLine(from: "left" | "right", line: number, segments: Segment[]): number {
  // 1. 找最后一个 end <= line 的段 s（用二分）：不在任何段内时，按 s 的两侧 end 差值平移，
  //    即 other = line - s.fromEnd + s.toEnd；line 在所有段之前时原样返回。
  // 2. line 落在段 [fromStart, fromEnd) 内：按比例换算，
  //    other = toStart + (line - fromStart) * (toEnd - toStart) / max(1, fromEnd - fromStart)。
  // 3. 一侧长度为 0 的段（纯新增 / 纯删除）：另一侧映射到该段的起点。
}
```

- 主动侧：最近一次接收滚轮、触控板或拖动滚动条的那一栏。跟随侧不监听自己的 scroll 事件，所以不会互相触发。
- 每帧只做一次：
  1. 取主动侧可视区中线所在的行（浮点）`c`；
  2. 跟随侧中线 `= mapLine(c)`；
  3. 换成跟随侧的 scrollTop，并夹在 `[0, max]` 之间；
  4. 两栏位移在同一个 `requestAnimationFrame` 里写入，用现有的自定义滚动（`applyScroll`）。
- 关掉“同步滚动”时，两栏各滚各的，窄栏照样按两边当前位置画色带。
- 跳转（F7、点改动树）：把目标段放到两栏可视区上方约三分之一处，两侧同时定位。

### 4.3 窄栏与色带

- 窄栏用一个 `<canvas>`，按 devicePixelRatio 设置尺寸，在同一帧里重画。
- 只画与任一侧可视区相交的段，用二分找起点：
  - 左边缘 y 区间 `[yL0, yL1]`，右边缘 `[yR0, yR1]`；
  - 上下两条边各是一条三次贝塞尔曲线，控制点在中间 x 处；
  - 用段的底色填充，两条边描 1px 稍深的线；
  - 长度为 0 的一侧画成 2px 的线。
- 行号与 » / « 用普通 DOM 绝对定位，跟各自一栏的位移走，按可见行虚拟化。
- 两栏里的块底色与单词高亮继续用 DOM，即行背景加 `<span>` 包住改动的字词。

## 5. 性能与验收（D06 不变）

- 测试文件用 `/tmp/…/diffs` fixture 的 `src/generated.ts`（8309 行）和一个两侧都有几百处改动的文件。
- 门槛：
  - 首屏 ≤ 150ms；
  - 未缓存跳转 ≤ 100ms 内出现文字；
  - 快速滚动两栏都不露白；
  - 窄栏每帧重画 ≤ 2ms；
  - 快速滚动停下后，两栏中线的对应误差 ≤ 1 行。
- 更新测量脚手架：`REVIA_MEASURE_ONLY=split` 测新的左右对照，记录上述各项，并把滚动同步误差写进测量日志；冒烟检查加一项：并排视图存在左栏、窄栏、右栏，且窄栏 canvas 有内容。
- 测试：
  - Rust 测试覆盖段计算：纯新增、纯删除、修改、一个 hunk 多段、文件头尾、CRLF、忽略空白三档；还要覆盖单词级区间与 `side_lines` 分页；
  - Rust 测试覆盖按段切开，每种都用真实 Git 仓库验证结果：
    - 三段的 hunk 只暂存中间一段；
    - 取消暂存、丢弃一段（反向规则）；
    - 两段之间只隔一行上下文；
    - 段在文件开头、结尾；
    - “\ No newline at end of file”；
    - CRLF；
    - 忽略空白前两档；
    - 应用失败时暂存区和工作区都不变；
    - 未解决冲突的文件拒绝按块和整文件写；
    - 用合并工具打开时不改 Git 配置（检查 `git config --list` 前后一致）；
  - `tools/test-sync-scroll.mjs` 覆盖 `mapLine`：段前、段内、段后、零长度段、首尾夹紧、左右互逆误差 ≤ 1 行。
- 统一视图、整文件收起、回收区的只读对比保持原样，不受影响。

## 6. 做不到时

照 Q141：Pi 做完后规划方验收这一块，包括左右对照、按段切开和冲突处理。交互或观感明显不如 Rebased，D06 不达标，或按段写、冲突处理有错时，由规划方直接接手修改。Pi 在报告里如实写出哪些做到了、哪些没做到、测到的数据。
