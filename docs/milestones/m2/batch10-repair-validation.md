# B10：原生退出修复复核与 m2-ui 集成检查点

日期：2026-10-25。固定修复提交 `2a51b160d1c3871b9c67fee81ae6f082910bc1f8`，基点 `5f632d661ce5dc37b237c27f60205c8d41fcf86c`。本记录区分静态通过、真实桌面局部结果和未完成门槛；不是 M2 完成报告。

## 修复与静态复核

旧集成版的真实 Cmd-Q 在 `unstage_file` 已执行、响应尚未交付时直接退出，未显示退出询问。该失败仍保留，不改写为 Git 被杀或数据丢失。

B10 在仅 macOS 的平台模块向原 Tao delegate 添加公开的 `applicationShouldTerminate:`，把原生退出转交既有 CloseDriver；保留 Wait/Stay、待核对、原关闭或退出动作及 Rust 最后保护。检查类与继承方法，不覆盖已有实现，不替换菜单或委托对象；转发错误和 panic 均不放行终止。仅补充已锁定的 macOS 依赖，不升级依赖版本。具体实现与旧失败见任务08 Comments。

| 检查范围 | 结果与版本边界 |
| --- | --- |
| 5f 同步版前端、既有 Node/Python、完整 Rust、绑定清理及空白检查 | 13条原始命令退出0；仍归属5f，不能说2a51重新跑过整套 |
| B10 修复后的完整 Rust | 271 passed / 0 failed / 1 ignored；ignored 为既有桌面 fixture 准备入口 |
| B10 绑定清理及空白检查 | 退出0；前端未改，不机械重复刚通过的前端检查 |
| 固定 Release 构建 | 退出0；二进制 SHA256 `fd5ffc86131aebbe77850487192005166a63c1a98f9e97aa72b3ddaa1d96988d` |
| 源码原字节 | Rust检查前后154份相同；构建前后2277份tracked文件相同；两种清单作用域不混用 |

### Standards

固定 `5f632d6...2a51b16`，独立静态复核：确定规范违反0项，Fowler判断建议0项，无最高严重度。

### Spec

相同固定增量，另一独立静态复核：缺失/部分0项、范围扩张0项、实现错误0项，无最高严重度。

两轴分别保留，不以静态0项替代原生桌面验收。任务08回到 `ready-for-human`，不标 `done`。

静态原件见[归档清单](./evidence/batch10-repair-static/manifest.json)：103件原始记录，包括旧5f检查/构建、新B10检查/构建、独立报告和旧失败事后说明，逐件gzip解压及原路径字节/SHA核对一致。旧构建没有完整精确环境记录，不补造；依赖解析前后只改变 `Cargo.lock`，不能算额外产品测试。归档内历史审计的 C05/run02 文字仅表示其写成时的边界，不代表当前状态。

## 已集成，但尚未完成验收

用户于本轮明确要求“m2-ui合并下，后续如果没问题，开始m3”。已执行 `git merge --ff-only --no-stat m2-ui`：

- m2 从 `ababf47ff59e1afe8b4db293d64ebf0a58070709` 快进到 `2a51b160d1c3871b9c67fee81ae6f082910bc1f8`，没有冲突或新增合并代码。
- 合并后 m2/m2-ui 的 tree 均为 `572064fb2c8859959ec4ffebc271c5b83c40550f`，当时两棵工作树干净，已有固定 Release 的 SHA 未变。
- main 仍为 `800ac01f90386d0149fd7e0ae6a96453687d7367`，其 status 原字节及已有修改文件SHA前后一致；没有提交用户改动。
- 这是内部集成，不是 squash main，不豁免任务10/11、D06、Q98及其他剩余门槛。

合并、正常推送和本次CI原始记录见[集成原件清单](./evidence/m2-ui-integration-20261025/manifest.json)。

## 真实桌面阶段性结果

下列原件已按旧5f与新2a51分组收录于[桌面归档](./evidence/task13-desktop04/manifest.json)，1551件逐份解压/SHA及源文件核对一致。31张原名为`.png`的截图实际为JPEG，保持原名和原字节，清单明确实际MIME，不转码、不冒称PNG。

- run01 同一新 Release：stage后窗口关闭/Stay、unstage后真实Cmd-Q/Stay均在显式释放后仍保持窗口；原生Revia菜单Quit/Wait在真实响应显式交付后正常退出0，源码及二进制未变。
- Dock样本仅取得工具超时，没有执行Dock Quit；响应自动超时 `explicit:false`，样本无效，不算Dock或长测通过。
- run02的真实shell失败全局提示、文本只读对比、二进制信息，以及重新打开详情后的后续变化禁止覆盖均已观察。原生另存生成全新6字节文件，当前工作区文件未被覆盖。
- 另存响应C06最终自动超时释放，虽已观察Cmd-Q询问及Stay，不把它计为显式释放闭环通过。
- 用户明确允许临时仓库恢复覆盖后，C05实际执行 `restore_recovery`。临时目录权限故障导致恢复准备失败，生产结果为 `NeedsVerification`、记录 `written=[]`；权限随后恢复0755。窗口Close/Wait后显式释放真实错误，先显示“有结果待核对”并阻止自动关闭，Stay仍可留在应用。HEAD、index、两侧diff、文件字节和模式均与操作前相同。
- 用户随后明确允许点击“我已核对，继续关闭”。真实点击后原PID55553正常退出0，源码和二进制不变；最终快照与C05操作前的HEAD、status、index、两侧diff、文件字节及模式完全一致。Stay已撤销先前的关闭请求，最终核对确认来自新的Close请求；`C05-final-verdict.json`保留这个区别，不描述为恢复成功、杀掉Git或写后丢失。C05完整闭环已完成，不再列为待许可。

字号增大的真实键事件、剩余代表性恢复/丢弃/审阅外改与拒绝提示等仍按原任务补验；不把已有局部成功外推为全部场景通过，也不机械重演所有历史桌面流程。

## CI 失败、修复与最终实际结果

按Q97仅正常推送m2到现有SSH origin，触发[Compile Check #40172283582](https://github.com/GarlandQian/Revia/actions/runs/40172283582)，对应2a51。最终结果为failure：macOS作业success；Windows前端通过，但Rust的 `review_marks` 28通过/1失败，后续Release编译skipped。

失败发生于 `deinitialized_submodule_is_not_confirmed_and_cannot_be_claimed` 内的 `git commit -m two`：克隆出的临时子模块没有提交身份，报 `Author identity unknown`。父仓库和源仓库的local config不会随clone继承；测试尚未到达后续deinit/审阅安全断言，不能称为Windows产品行为失败。

按Q105交原Pi，`d952a85`仅给自建临时子模块设置本地身份并禁签名，保留所有安全断言；本机受影响的review_marks套件31通过。固定增量的独立Standards发现2项（最高P2）、Spec发现2项（最高P3），均为交付记录超出原件，不是代码缺陷。Pi在`f10fc971`仅更正文档，规划方复核两处已准确限定：无有效“修复版+useConfigOnly”正对照原件，notify离线失败也仅是此前描述、对应原件缺失。保留无效外层配置对照及既有删除披露，不补造，不称重跑两轴0项。

仅正常推送m2到现有SSH origin后，[Compile Check #40174661706](https://github.com/GarlandQian/Revia/actions/runs/40174661706)已实际完成，head为`f10fc971c14e556c239cb4648cabfbbfc28ad4b4`：

- macOS Apple Silicon：类型检查、lint、前端构建、完整Rust271通过及Release编译全部成功。
- Windows x64：类型检查、lint、前端构建、完整Rust243通过及Release编译全部成功；review_marks为29通过，先前失败的子模块用例实际通过。
- 双平台均保留1个既有的桌面fixture准备ignored，未增加跳过断言；平台项数不同来自既有条件编译。
- 不推main、不强推、不打标签、不创建PR、不发布。与2a51相比只有测试及任务10文档变化，当前受测Release仍明确记录为2a51构建，不假标为f10重新构建。

修复、更正、原始独立评审及新CI原件见[CI修复归档](./evidence/task10-ci-fixture-final01/manifest.json)。旧CI失败不改写为成功。

最终D06四组已完成；Q98首轮78.54秒失败，第二轮1223.25墙秒因vmmap错误中断且Rust现有样本触发+10MiB门槛，均未通过。详情见[最终测量记录](./final-measure-20261025.md)和[最新矩阵](./issues/10-integration-matrix.md)。后续仍需必要桌面补验、问题定位和新的完整Q98；不直接重跑挑通过。M3目前只有已确认方向，没有专属spec/issues；M2最终通过并合main后，先补M3详细访谈，再按常规流程写规格、拆任务并派Pi。本次未启动M3实施。
