# 整批B05返修检查与第五轮复核

日期2026-10-25，固定 `013e1fa631c82e99948efa4d3339e1a79103bad7`，基点 `1dd509ab476a9f279a23074368ec46177b566fb7`，m2独立worktree。这不是最终M2验收。

原件 `/private/tmp/revia-b05-uNREPd`完整无损归档，见[清单](./evidence/batch05-repair/manifest.json)。六条调用均退出0：Node156通过、Rust匹配exit_guard测试3通过、tsc/lint/build/diff-check通过。规划方没有重复运行这些检查。

`record.py`在执行时保存真实argv、cwd、显式非敏感环境覆盖（CI=1/NO_COLOR=1）、开始/结束/退出及stdout/stderr；不声称记录了完整继承环境。79份受测源码检查前后及committed清单逐字相同，规划方逐份核对固定提交与当前磁盘全部匹配；包括生产、测试、构建配置与锁文件，不含generated bindings、dist/target、文档与验收原件。

生产`ExitGuard`已由AppState和Rust测试共用，不再测试复制算法。Pi报告临时删除版本比较后2失败/1通过，但本轮日志未附改动源码及失败原件，仅标为实施方报告，不算独立核验。旧release await后的Stay代际校验已有静态改善。

两个独立轴仍不通过：Standards3项P2、Fowler建议0；Spec缺失/部分1、实现错误2、范围扩张0，最严重P2。详细报告及固定diff/provenance归档在清单的review组；任务[08最新Comments](./issues/08-safe-close-and-exit.md#comments)列ST-B06/SP-B06返修要求。主要边界是原生Quit立即放行吞请求、执行失败自动循环，以及close投递回包并不等于原生关闭裁决。测试仅计数的reconcile与仅记录动作的native没有覆盖真实App闭环；下一轮回归需调用App同源驱动，不在测试补生产漏掉的一步。

未新Release/GUI/D06/CI/长测/推送/合并。任务08回ready-for-agent；UI深色与浅色监视失败提示仍是m2-ui独立证据，不放行08或完整M2。主目录用户改动保持原样。
