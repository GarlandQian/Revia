# Revia D06 原始测量提取

仅提取已记录的值，不产生通过或不通过结论。耗时单位为 ms，除非列名另有标注。

缺失字段显示“缺失”；null、0、false保留原意。valid不从其他字段推导。

focusLosses 保留原值；当前探针是 0/1 失焦标志，不是累计失焦次数。

appear/warm 是 M1 原始总计时，不等同于 DOM 就绪或硬件上屏；colorRestoreMs 不等同于 colorDomMs。

droppedFrames 是原测量中大于 20 ms 的帧间隔数，不是推算的硬件丢帧数。

### 来源与原始状态

| ID | 文件 | ok | valid | error | 读取错误 | 组数 | SHA-256 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| F1 | /private/tmp/revia-task03-frame-split-01-20261023.json | true | 缺失 | 缺失 | 缺失 | 1 | 692c3b807dd73116c67888ce745f84dec034a5fc08573a919fb2b4e26a942161 |
| F2 | /private/tmp/revia-task03-frame-unified-01-20261023.json | true | 缺失 | 缺失 | 缺失 | 1 | ea564b1ca7b04590d0f31ff425b5a8ec2f48291988ef3fa52ac30c2ac16e6dc9 |
| F3 | /private/tmp/revia-task03-frame-cold-split-01-20261023.json | true | 缺失 | 缺失 | 缺失 | 1 | 6953a24f25a0be123aa54b479657ebd810a5fde669c90819b4719aa3507edfa8 |
| F4 | /private/tmp/revia-task03-frame-cold-unified-01-20261023.json | true | 缺失 | 缺失 | 缺失 | 1 | cc8743cd44a80ec187da3e513cfbf1e422b560fbc96226f18b06b054030bf1ff |

### 首屏原始计时

| ID | 标签 | 开始时间 | 显示行数 | 文件数 | fromListChangesMs | fromOpenWorktreeMs | listChangesMs | firstRowsMs | renderMs | rafPainted | 视口高度 | DPR |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| F1/C0 | split / src/generated.ts | 07:52:18.596 | 8309 | 7 | 91 | 235 | 43 | 47 | 47 | true | 718 | 1 |
| F2/C0 | unified / src/generated.ts | 07:53:49.645 | 13435 | 7 | 90 | 245 | 43 | 47 | 47 | true | 718 | 1 |
| F3/C0 | split / cold.ts | 07:55:01.865 | 30001 | 1 | 117 | 268 | 61 | 56 | 56 | true | 718 | 1 |
| F4/C0 | unified / cold.ts | 07:57:54.078 | 60001 | 1 | 106 | 261 | 63 | 43 | 43 | true | 718 | 1 |

### M1 原始跳转计时

| ID | fromRow | toRow | 目标已缓存 | 目标已覆盖 | appearMs | fetchMs | withinOneFrame | 返回已缓存 | 返回已覆盖 | warmAppearMs | warmWithinOneFrame | colorRestoreMs |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| F1/C0 | 0 | 4985 | true | true | 16 | 8 | true | true | true | 8 | true | 21 |
| F2/C0 | 0 | 8061 | true | true | 14 | 10 | true | true | true | 3 | true | 20 |
| F3/C0 | 0 | 18000 | false | true | 25 | 17 | false | true | true | 5 | true | 28 |
| F4/C0 | 0 | 36000 | false | true | 24 | 20 | false | true | true | 5 | true | 27 |

### DOM 跳转准备

| ID | sourceReady | sourcePreparation | preparationError | jumpDomError | 测量 valid |
| --- | --- | --- | --- | --- | --- |
| F1/C0 | true | explicit-cache-and-normal-scroll | null | null | 缺失 |
| F2/C0 | true | explicit-cache-and-normal-scroll | null | null | 缺失 |
| F3/C0 | true | explicit-cache-and-normal-scroll | null | null | 缺失 |
| F4/C0 | true | explicit-cache-and-normal-scroll | null | null | 缺失 |

### DOM 跳转缓存与计时

| ID | 方向 | fromRow | toRow | 目标已缓存 | 事前缺行 | 事前颜色缓存 | scrollCallMs | immediateDomMs | textDomMs | textDomSource | colorDomMs | 首 rAF callbackMs | 首 rAF checkedMs | 首 rAF timestampMs |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| F1/C0 | 去程 | 0 | 6647 | true | 0 | false | 2 | 2 | 2 | synchronous | null | 5 | 5 | 5 |
| F1/C0 | 返回 | 6647 | 0 | true | 0 | true | 1 | 1 | 1 | synchronous | 31 | 12 | 12 | 12 |
| F2/C0 | 去程 | 0 | 10748 | true | 0 | false | 1 | 1 | 1 | synchronous | null | 9 | 9 | 9 |
| F2/C0 | 返回 | 10748 | 0 | true | 0 | true | 0 | 1 | 1 | synchronous | 34 | 14 | 14 | 14 |
| F3/C0 | 去程 | 0 | 24000 | false | 40 | false | 3 | 3 | 5 | mutation | null | 7 | 7 | 7 |
| F3/C0 | 返回 | 24000 | 0 | true | 0 | true | 1 | 1 | 1 | synchronous | 31 | 11 | 11 | 11 |
| F4/C0 | 去程 | 0 | 48000 | false | 40 | false | 3 | 3 | 4 | mutation | null | 11 | 11 | 11 |
| F4/C0 | 返回 | 48000 | 0 | true | 0 | true | 0 | 0 | 0 | synchronous | 33 | 14 | 15 | 14 |

### 同步、首 rAF 与最终 DOM 原始核对

| ID | 方向 | 阶段 | 匹配行 | 需求行 | 文字匹配 | 占位行 | 占位暴露 | 画布就绪 | 画布对齐 | 颜色缓存 | 已着色单元 | 预期着色单元 | 颜色匹配 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| F1/C0 | 去程 | 同步 DOM | 40 | 40 | true | 0 | false | true | true | false | 0 | 80 | false |
| F1/C0 | 去程 | 首 rAF DOM | 40 | 40 | true | 0 | false | true | true | false | 0 | 80 | false |
| F1/C0 | 去程 | 最终 DOM | 40 | 40 | true | 0 | false | true | true | false | 0 | 80 | false |
| F1/C0 | 返回 | 同步 DOM | 40 | 40 | true | 0 | false | true | true | true | 10 | 77 | false |
| F1/C0 | 返回 | 首 rAF DOM | 40 | 40 | true | 0 | false | true | true | true | 10 | 77 | false |
| F1/C0 | 返回 | 最终 DOM | 40 | 40 | true | 0 | false | true | true | true | 77 | 77 | true |
| F2/C0 | 去程 | 同步 DOM | 40 | 40 | true | 0 | false | true | true | false | 0 | 40 | false |
| F2/C0 | 去程 | 首 rAF DOM | 40 | 40 | true | 0 | false | true | true | false | 0 | 40 | false |
| F2/C0 | 去程 | 最终 DOM | 40 | 40 | true | 0 | false | true | true | false | 0 | 40 | false |
| F2/C0 | 返回 | 同步 DOM | 40 | 40 | true | 0 | false | true | true | true | 4 | 39 | false |
| F2/C0 | 返回 | 首 rAF DOM | 40 | 40 | true | 0 | false | true | true | true | 4 | 39 | false |
| F2/C0 | 返回 | 最终 DOM | 40 | 40 | true | 0 | false | true | true | true | 39 | 39 | true |
| F3/C0 | 去程 | 同步 DOM | 0 | 40 | false | 40 | true | true | true | false | 0 | 0 | false |
| F3/C0 | 去程 | 首 rAF DOM | 40 | 40 | true | 0 | false | true | true | false | 0 | 80 | false |
| F3/C0 | 去程 | 最终 DOM | 40 | 40 | true | 0 | false | true | true | false | 0 | 80 | false |
| F3/C0 | 返回 | 同步 DOM | 40 | 40 | true | 0 | false | true | true | true | 0 | 78 | false |
| F3/C0 | 返回 | 首 rAF DOM | 40 | 40 | true | 0 | false | true | true | true | 0 | 78 | false |
| F3/C0 | 返回 | 最终 DOM | 40 | 40 | true | 0 | false | true | true | true | 78 | 78 | true |
| F4/C0 | 去程 | 同步 DOM | 0 | 40 | false | 40 | true | true | true | false | 0 | 0 | false |
| F4/C0 | 去程 | 首 rAF DOM | 40 | 40 | true | 0 | false | true | true | false | 0 | 40 | false |
| F4/C0 | 去程 | 最终 DOM | 40 | 40 | true | 0 | false | true | true | false | 0 | 40 | false |
| F4/C0 | 返回 | 同步 DOM | 40 | 40 | true | 0 | false | true | true | true | 0 | 39 | false |
| F4/C0 | 返回 | 首 rAF DOM | 40 | 40 | true | 0 | false | true | true | true | 0 | 39 | false |
| F4/C0 | 返回 | 最终 DOM | 40 | 40 | true | 0 | false | true | true | true | 39 | 39 | true |

### DOM 有效性相关原始字段

| ID | 方向 | valid | completed | timedOut | timeoutMs | frameReady | textWithinBudget | frameBudgetMs | textBudgetMs | hidden | hiddenTransitions | 首 rAF hidden | 占位样式 ready |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| F1/C0 | 去程 | 缺失 | true | false | 4000 | true | true | 16.7 | 16.7 | false | 0 | false | true |
| F1/C0 | 返回 | 缺失 | true | false | 4000 | true | true | 16.7 | 16.7 | false | 0 | false | true |
| F2/C0 | 去程 | 缺失 | true | false | 4000 | true | true | 16.7 | 16.7 | false | 0 | false | true |
| F2/C0 | 返回 | 缺失 | true | false | 4000 | true | true | 16.7 | 16.7 | false | 0 | false | true |
| F3/C0 | 去程 | 缺失 | true | false | 4000 | true | true | 16.7 | 100 | false | 0 | false | true |
| F3/C0 | 返回 | 缺失 | true | false | 4000 | true | true | 16.7 | 16.7 | false | 0 | false | true |
| F4/C0 | 去程 | 缺失 | true | false | 4000 | true | true | 16.7 | 100 | false | 0 | false | true |
| F4/C0 | 返回 | 缺失 | true | false | 4000 | true | true | 16.7 | 16.7 | false | 0 | false | true |

### 滚动帧与覆盖

| ID | mode | 请求屏数 | 实际屏数 | frames | rAF 帧 | 兜底帧 | 未覆盖帧 | 未覆盖 max | 未覆盖 p95 | 帧间隔 max | 帧间隔 p95 | 帧间隔 p50 | 慢帧 >20ms | 距离 px | 耗时 s |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| F1/C0 | frame | 60 | 60 | 60 | 60 | 0 | 0 | 0 | 0 | 29 | 17 | 17 | 1 | 43080 | 0.997 |
| F2/C0 | frame | 60 | 60 | 60 | 60 | 0 | 0 | 0 | 0 | 18 | 18 | 17 | 0 | 43080 | 0.998 |
| F3/C0 | frame | 60 | 60 | 60 | 60 | 0 | 0 | 0 | 0 | 27 | 17 | 17 | 1 | 43080 | 0.996 |
| F4/C0 | frame | 60 | 60 | 60 | 60 | 0 | 0 | 0 | 0 | 18 | 18 | 17 | 0 | 43080 | 0.997 |

### 可见性原始记录

| ID | 范围 | state | hidden | focused | focusLosses | hiddenTransitions | maxFrameGapMs | rafPerSecond | 视口宽度 | 视口高度 | DPR | valid |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| F1 | 整份记录 | visible | false | true | 缺失 | 0 | 33 | 59.94005994005994 | 1280 | 800 | 1 | 缺失 |
| F1/C0 | 单组测量 | visible | false | 缺失 | 缺失 | 缺失 | 缺失 | 60 | 1280 | 800 | 1 | 缺失 |
| F2 | 整份记录 | visible | false | false | 缺失 | 0 | 29 | 59.94005994005994 | 1280 | 800 | 1 | 缺失 |
| F2/C0 | 单组测量 | visible | false | 缺失 | 缺失 | 缺失 | 缺失 | 60 | 1280 | 800 | 1 | 缺失 |
| F3 | 整份记录 | visible | false | false | 缺失 | 0 | 31 | 59.880239520958085 | 1280 | 800 | 1 | 缺失 |
| F3/C0 | 单组测量 | visible | false | 缺失 | 缺失 | 缺失 | 缺失 | 61 | 1280 | 800 | 1 | 缺失 |
| F4 | 整份记录 | visible | false | false | 缺失 | 0 | 25 | 60 | 1280 | 800 | 1 | 缺失 |
| F4/C0 | 单组测量 | visible | false | 缺失 | 缺失 | 缺失 | 缺失 | 61 | 1280 | 800 | 1 | 缺失 |
