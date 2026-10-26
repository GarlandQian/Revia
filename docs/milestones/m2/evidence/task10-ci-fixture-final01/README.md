# review_marks CI 修复原件归档

此包只封存已经结束的检查、敏感性对照、审查、更正和 CI 原件。归档期间不运行产品检查、构建、GUI 或新的 CI，不访问 Q98 活动日志，不修改仓库、原件、fixture 或应用数据。

## 内容与版本

- `repair-records/`：基于 `2a51b160d1c3871b9c67fee81ae6f082910bc1f8` 的修复工作记录。现存根目录测试输出是成功记录，不是曾描述的 notify 离线失败日志。
- `final-local-run/`：`d952a857b3faef2a4101ab5a6582217bd5a7c8e0` 关联的最终本机 review_marks 检查，31 passed / 0 failed；测试源码前后 SHA 及提交清单一致。哈希清单作用域并不完全相同，不能称整棵源码原字节不变。
- `sensitivity/`：保留旧代码在外层配置下通过的无效对照、fixture 内启用 useConfigOnly 后实际身份失败的原件、修复源码快照和隔离配置。不把失败改写为成功，不把普通 31 项通过当缺失的同条件正对照。
- `independent-review/` 与 `correction-dispatch/`：保留 `2a51...d952` 的两轴原始发现及更正派发；这些不是 `f10` 后的新审查，也没有倒写为零发现。
- `documentation-correction/`：仅文档提交 `f10fc971c14e556c239cb4648cabfbbfc28ad4b4` 的记录和 `diff-check exit=0`；未以此冒充产品重测。
- `terminal-ci/`：同一 `f10fc971...` 的正常 push、首次 API 连接失败、历史 in_progress 查询、终态 run `40174661706`、完整原始日志和派生计数。Windows 为 **243 passed / 0 failed / 1 ignored**，review_marks 29 passed；macOS 为 **271 / 0 / 1**，review_marks 31 passed。两个 job 的前端检查、Rust tests 和 Release binary 步骤均成功。

## 保留的边界

1. 修复后在相同 useConfigOnly 条件下通过的独立原件缺失。最终本机普通测试及实际 CI 成功都不能补造这一历史正对照，也不据缺件断言该运行从未发生。
2. 未隔离 HOME 的 notify offline 失败（101）仅有此前实施方描述，对应原件缺失。现存两组成功不冒充该失败。已披露的早期删除事实不被归档抹去。
3. 原记录脚本使用 `text=True`，保存环境时省略 PATH。本包保证现有文件字节原样，不提升为原进程未经文本转换的二进制捕获，也不补写缺失环境。
4. 文档更正以及 CI 查询/下载缺少独立精确 argv、完整环境和起止时间的部分，不按推测补造。push 单独保有命令、cwd 和起止时间。
5. CI 日志包含其原有 BOM、ANSI、warnings、runner 路径及平台掩码，均原样保留。归档不构成完整敏感信息审计。
6. CI Release 指编译二进制，不代表发布安装包、Windows 真实桌面验收、D06/Q98 或整个 M2 完成。旧审查的“Windows 待 CI”保留当时语境，终态另层关联。

## 完整性

每件原件单独以 gzip 封存；`manifest.json` 保存原绝对路径、版本关联、类别、状态、局限、原字节大小/SHA256 及压缩件大小/SHA256。`archive.py` 在封包前后核原件状态与字节；`verify.py` 逐件解压，核对压缩 SHA、解压 SHA，并可直接对比仍存在的源文件。空输出也是原件，不省略。

`archive.py`、`verify.py` 和本说明是新建归档工具/说明，不是历史运行原件。不得把这些脚本当作新的项目测试入口；历史 recorder 仅作为 gzip 原件保存，从未执行。

离线核包：`python3 verify.py`。原件仍在时可额外执行 `python3 verify.py --check-sources`。均仅核静态文件，不启动产品或测试。

明确排除完整 fixture、home、isolated-home、cargo、target、node_modules、.git、__pycache__、.app、二进制与应用数据。Q98 首轮时序小报告独立保存，不混入此 CI 包。
