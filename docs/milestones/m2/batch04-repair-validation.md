# 整批B04返修检查与第四轮复核

日期2026-10-25，固定 `e1ab47d10f1753722c4f2e61b1f71786ccf63470`，基点 `f92c968689b7ac312c30b6446e74b54c717dcfe6`，m2独立worktree。这不是最终M2验收。

原件 `/private/tmp/revia-b04-QMqsIw`完整无损归档，见[清单](./evidence/batch04-repair/manifest.json)。Node150通过、Rust匹配exit_guard测试3通过，Rust lib103通过；tsc/lint/build各退出0。首次diff-check退出2（ts-rs绑定行尾空白），规范化后的diff-check与受影响前端/Node复验退出0，失败原件保留。没有重复执行本轮测试。

六份代码/测试的检查前后SHA相同，与固定提交及磁盘匹配。`tested-source-manifest.txt`只列72条路径，没有对应72份检查前后哈希，不能声称完整树逐字证明。runner列出cwd和命令，但第二条Rust写的是session_flow，与实际`rust-lib.out`的103项lib输出不一致；缺少每次调用的精确argv/环境记录。这些原件均保留，不补造、不将runner当精确执行日志。

Rust新增三个测试复制了版本接收算法，不调用生产`AppState::apply_exit_guard`；它们的通过不算真实生产接收端回归。实际生产版本锁静态有改善，待最小接缝修复后再验证。

前轮切worktree卸载监听、晚Wait按钮消失、保护失败未收尾的直接路径已静态闭合。独立两轴仍未全通过，见[任务08最新Comments](./issues/08-safe-close-and-exit.md#comments)：Standards另发现异步退出已放行后，旧回调可越过后续Stay；Spec指出真实Rust回归缺失。两个轴保持独立计数，不将同一验证缺口合并成额外运行时缺陷。

本轮未Release/GUI/D06/CI/长测/推送/合并。UI深色及浅色监视故障提示已在独立m2-ui留证，不据此放行任务08或完整M2；主目录用户改动保持原样。
