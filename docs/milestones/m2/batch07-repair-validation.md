# 整批B07返修检查与第七轮复核

日期2026-10-25，固定`520f134ff995010d87d885594d99d3518213e725`，基点`332097b6c0e95b77327f55d1894787545a4cacf2`。原件目录`/private/tmp/revia-b07-opWp3M`无损归档，见[清单](./evidence/batch07-repair/manifest.json)。本页不是最终M2验收。

最终5条真实执行记录均退出0：Node172、tsc/lint/build/diff-check。79份生产/测试/配置/锁文件的检查前后/committed清单相同，规划方逐份核对固定提交与磁盘SHA匹配；排除generated bindings、dist/target、文档与验收原件。没有重复跑这些检查，本轮Rust未改未重跑。

记录器保存真实argv/cwd、显式非敏感环境覆盖、start/end/exit/stdout/stderr。Paseo此前诊断活动出现Node失败及过滤后的错误输出，提供目录没有这些完整失败原件，不补造，也不声称所有尝试都成功。Pi所述runCloseExecution“已删除”不准确：它仍是未使用导出，只是App改用CloseDriver。

独立Standards2/Spec2均P2，Fowler建议0、范围扩张0，仍不通过。同步递归与立即重投已静态闭合；剩余为Stay后已投递原生关闭未被阻止，以及早到blocked裁决被晚delivered覆盖，原请求永久等待。详见[任务08最新Comments](./issues/08-safe-close-and-exit.md#comments)，完整双轴报告在清单review组。

App监听仍保留原生放行分支，测试native仅记投递后手动裁决；Stay用例未到“close已发出”，早裁决用例未继续验证B完成后的原请求，因此Node172不能作上述链通过证据。08回ready-for-agent；未新Release/桌面/D06/CI/长测/推送/合并，UI分支和主目录未改。
