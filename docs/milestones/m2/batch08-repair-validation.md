# 整批B08返修检查与第八轮复核

日期2026-10-25，固定`41504336171c2adf3238f45be6dd098e95e2e262`，基点`34b2d5148a3530cb4168fad43317d5ae2fd0362d`。原件`/private/tmp/revia-b08-1yRxyJ`与双轴复核无损归档，见[清单](./evidence/batch08-repair/manifest.json)。不是最终M2验收。

最终5条检查退出0：Node177、tsc/lint/build/diff-check。79份受测生产/测试/配置/锁文件before/after/committed清单一致，规划方核对固定提交与当前磁盘逐份匹配；排除generated bindings、dist/target、文档和验收原件。未重复跑检查，本轮Rust未变未重跑。

记录器保留真实argv/cwd、非敏感显式环境覆盖、start/end/exit/stdout/stderr。早期Paseo诊断曾见失败和局部输出，提供目录没有其完整失败原件，不补造；上述退出0只对应最终5条记录。

Standards2项P2、Fowler建议0；Spec缺失1/错误1项P2、范围扩张0，仍不通过。上轮两个单次时序有静态改善，但earlyVerdict的跨claim消费会使后续新关闭悬挂，canStartWrite未被任何生产写入口调用。两轴分别给出旧IPC仍挂起和旧IPC已结束的确定交错，见[任务08最新Comments](./issues/08-safe-close-and-exit.md#comments)。新的自动测试没有覆盖实际写门禁和新请求最终裁决，不能据Node177放行。

当前保留在可接手检查点：任务08为ready-for-agent，原Pi已idle，复核后未派发下一轮；Q105规则未修改，规划方未擅自接手生产实现。UI仍在m2-ui，main和用户改动未动。未新Release/桌面/D06/CI/Q98长测/推送/合并，任务10/11/M2未完成。
