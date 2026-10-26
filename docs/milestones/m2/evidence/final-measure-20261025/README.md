# M2 最终 D06 / Q98 留证包（2026-10-25）

本包只封存已存在的文件，不运行源目录脚本，不做新的 GUI、构建、测试或测量。建议仓库位置：`docs/milestones/m2/evidence/final-measure-20261025/`。封包不代表 M2 验收通过。

## 来源与版本

- 运行原件根目录：`/private/tmp/revia-m2-final-measure-ex9swy__`。仅纳入该目录根层的全部普通文件；不遍历或归档 `cold/`、`fixtures/`、`home/`、`data-*/`，也不纳入 `.git/`、`target/`、`node_modules/`、应用或二进制。
- 实际构建源码：`2a51b160d1c3871b9c67fee81ae6f082910bc1f8`；实际受测二进制 SHA256：`fd5ffc86131aebbe77850487192005166a63c1a98f9e97aa72b3ddaa1d96988d`。
- 测量时整合提交：`f10fc971c14e556c239cb4648cabfbbfc28ad4b4`；相对构建版只含测试 fixture 与任务10文档变化，产品源码和资源相同。各 launch/exit 文件及源码快照保留原值，不改标为 f10 重新构建。
- 后续 `9434e7aa0421c88d7e1bddf576df8909095b8239` 仅记录 Pi 模型顺序 Q109（AGENTS.md、docs/README.md、paseo-batch.md），不是本次受测构建，也不追溯修改运行时源码快照。
- `analysis/` 组来自三个既有独立分析目录和一次 attempt02 终态复核。其报告、事实汇总及第一轮分析脚本都是事后派生材料，不冒称运行原件；分析脚本仅按原字节压缩，未执行。具体源路径、分类、PID及版本关联见 manifest。

## D06 已观察结果与边界

| 场景 | PID | 标准 / 独立冷负载首屏 ms | 原 appear / warm ms | 独立文字出 / 回 ms | 返回全色 DOM ms |
| --- | ---: | ---: | --- | --- | ---: |
| standard split | 85755 | 137 | 10 / 4 | 2 / 1 | 33 |
| standard unified | 87247 | 124 | **18 / 7** | 1 / 1 | 33 |
| cold split | 87618 | 135 | 11 / 4 | 4 / 2 | 29 |
| cold unified | 87831 | 137 | 13 / 4 | 3 / 1 | 32 |

- 四组各自完成并退出0，binary/source unchanged 均为 true；均为同次运行的独立 PID，不是拼成一个长测。
- **标准统一原 `appearMs=18`、`withinOneFrame=false` 必须保留。** 原目标行8061，独立 jumpDom 目标10748；不能用另一目标探针的1ms/10ms倒填18ms，不能改称所有原跳转都在一帧内。
- 标准首屏137/124低于150ms；cold首屏属于独立大负载，不混入标准150ms门槛。cold确有目标缺35行、未缓存证据，不只依据文件大小推断。
- 八次独立首rAF均35/35文字、行号、marker及几何匹配；这是DOM语义证据，不是GPU像素扫描。首rAF并非所有颜色到齐，返回颜色29–33ms才完整。
- 四组各60真rAF、0fallback、0uncovered；最大帧18/31/48/18ms均保留，不擅增“慢帧清零”门槛。截图是离散测后/滚动后画面，不冒称逐帧录像或普通模式首次静置补色证明。
- cold准备首轮不存在的Python路径错误事后注记、重试的真实调用和输出均按原件保留，不补造未留存的原始错误输出。

## Q98 两轮均未通过

| 轮次 | 驱动 PID | Revia PID | 已记录墙钟秒 | 终态 |
| --- | ---: | ---: | ---: | --- |
| attempt01（文件名 q98，无后缀） | 88350 | 88354 | 78.54 | 可见连续合成中断，maxFrameGapMs=2179；driver退出1 |
| attempt02 | 90683 | 90685 | 1223.25 | vmmap读取WebContent90688失败、缺少前端最终报告；driver退出1 |

- 没有任何一轮完成同进程1800主动秒，不能拼接两轮、续接时长或视为通过。应用退出0、阶段 provisional 成功也不能代替完整长测判定。
- 首轮 focused=false 不是代码的直接失败条件，也不证明窗口被遮挡；vmmap对前端中断的因果关系尚未确认。
- 第二轮18阶段都是 `samplingChecksPassed=true` 且匹配END=true；**不是18次采样失败**。它们全部最终 invalid 的唯一错误是前端报告缺少唯一 staged/split checkpoint。预期 `q98-attempt02.json.frontend.json` 实际缺失，包内不补造此文件。
- 缺失完整前端描述意味着不能补造 `hiddenDuringHold`、主动时长或最终负载；无效压力样本只能作定位线索，不能擅改 valid。正常心跳日志不是完整 visibility 结构的替代品。
- 第二轮已有Rust常规footprint按当前算法最大抬升82MiB，超过+10MiB门槛；整体负斜率不取消该门槛，也不等于已经确定泄漏。WebContent完整有效压力后证据不足；普通RSS不单独用于Q98判失败。
- 常规采样异常路径可能触发driver清理并丢失仅在前端内存保存的进度。现有原件没有清理分支、最后失败瞬间PID状态或退出来源日志；不能确定vmmap根因、断言driver确已发送terminate或指称用户关闭。须保留这个未知边界。

## 布局与完整性

- `artifacts/<来源组>/<原文件名>.gz`：每件原字节独立gzip，空文件也保留，不改名修内容、不转码、不重生成截图或JSON。
- `manifest.json`：每件来源、性质、原始大小/SHA256、gzip大小/SHA256、源文件模式/mtime、运行及版本/PID关联；另列排除目录及真实缺件。
- `manifest.sha256`：manifest校验值；`verification-offline.json`、`verification-sources.json`为封包后校验输出，是本次封包派生记录，不是历史运行原件。
- 包内仅包含gzip载荷、清单、此说明及校验工具/结果。校验只读，不提取到原目录、不执行载荷脚本。gzip的元数据使用固定mtime，原文件名/mtime另存清单。

离线检查，不依赖原目录：

```bash
python3 verify.py
```

原件仍存在时，额外逐字节对照：

```bash
python3 verify.py --check-sources
```

通过必须是所有载荷的压缩SHA、解压SHA/大小及清单一致；`--check-sources`还要求每件解压字节与现存原件完全相同。清单校验保障传输一致性，不是独立签名或对运行结论的再次验收。
