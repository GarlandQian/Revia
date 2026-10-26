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

按钮在窄栏的块首行上；悬停时显示中文提示，并把这一块的两边和色带一起加亮。

| 文件所在组 | 右侧 « | 左侧 » |
| --- | --- | --- |
| 未暂存 | 暂存此块 | 丢弃此块（确认后先存回收区） |
| 已暂存 | 无 | 取消暂存此块 |
| 未跟踪 | 无（整文件操作） | 无 |

- 一个 Git hunk（`--unified=3`）里可能有几段改动。每段画一条色带，但按钮只在 hunk 的第一段出现一次，作用于整个 hunk；悬停时同一 hunk 的各段一起加亮。不要改用 `-U0` 细分。
- 忽略空白的三档照 Q140 处理。“同时忽略空行”一档下按钮变灰。
- “暂存此块”等文字按钮不再占一整行。原来的 hunk 标题行去掉，键盘与菜单入口保留。

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
  - `tools/test-sync-scroll.mjs` 覆盖 `mapLine`：段前、段内、段后、零长度段、首尾夹紧、左右互逆误差 ≤ 1 行。
- 统一视图、整文件收起、回收区的只读对比保持原样，不受影响。

## 6. 做不到时

照 Q141：Pi 做完后规划方验收这一块。交互或观感明显不如 Rebased，或 D06 不达标时，由规划方直接接手修改。Pi 在报告里如实写出哪些做到了、哪些没做到、测到的数据。
