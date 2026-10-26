# 整批B06返修检查与第六轮复核

日期2026-10-25，固定`ff6ba782a3e5a2e30960856e60bc49bb2bdca876`，基点`95d37df24f95e159c7175069b0ace6fabc287a56`，m2独立worktree。本页不是最终M2验收。

`/private/tmp/revia-b06-jQhG5C`全部28份原件与独立双轴报告、固定diff/provenance等无损归档，见[清单](./evidence/batch06-repair/manifest.json)。六条实际命令退出0：Node165、Rust exit_guard3、tsc/lint/build/diff-check。未重复运行这些检查；本轮生产Rust未改，匹配Rust测试仅为实施方附加检查。

79份受测生产/测试/构建配置/锁文件的before/after/committed逐字一致，规划方核对固定提交与当前磁盘SHA全部匹配；不含generated bindings、dist/target、文档与验收原件。记录器真实保存每条argv/cwd、显式非敏感环境覆盖、开始/结束/退出/stdout/stderr，不声称完整继承环境。

独立复核仍不通过：Standards3（P1一、P2二，Fowler建议0）；Spec实现错误1（P1，缺失/范围扩张0）。前轮立即Quit与失败取消授权有静态改善，但排他claim接入App后仍同步递归，投递后缺等待裁决状态，旧原生事件未关联请求代际。两个轴按各自根因/后果计数，不强求一致。见[任务08最新Comments](./issues/08-safe-close-and-exit.md#comments)。

Node165的绿色不证明App闭环：新增测试仍把reconcile简化成计数，native为fake结果，没有驱动App完整重入/裁决接线。成批交原Pi收敛到App与测试共享的生产驱动；不以额外测试手动调用掩盖漏接线。

任务08回ready-for-agent。未新Release/GUI/D06/CI/长测/推送/合并；UI仍在m2-ui独立验收，主目录用户改动保持原样，任务10/11/M2不提前完成。
