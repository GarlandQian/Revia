# 01：Worktree 激活与刷新

Status: done
Blocked by: None（可立即开始）

## What to build

交付[规格](../spec.md)中与Worktree 激活与刷新对应的完整行为。前三项由同一个 Pi 会话实施，04、05 由规划方接手；无依赖项可独立验证，实施顺序优先 01 → 02 → 03。

## Acceptance criteria

- [x] 打开、手动刷新、环境就绪、重载和写后核对使用共同生产 interface，调用方不再分散同步激活与监视。
- [x] 刷新及环境重读后仍能接收监视事件；A→B→A 不接收旧快照或旧错误。
- [x] 写后始终核对原目标，只在资格仍有效时发布到当前界面；审阅标记与选中项语义保留。
- [x] 保留渐进加载、写期间合并刷新与监视失败降级。
- [x] Node 行为测试执行同一生产 module，覆盖规格列出的刷新时序；不靠源码字符串证明。

## Comments

### 2026-11-09 · Spec 审查（af0a609...d8babd7）

- SP7 / P2：worktreeRefresh.readInto 经 acceptTarget 拒绝激活代次改变；对原目标核对的读取期间切换界面会返回 false，影响写入/关闭收尾。必须区分“真实读取核对成功”和“可否发布当前界面”，当前激活改变不应使原目标读取失败。补生产 interface 回归：发起 verifyWrite 后切换，再交付真实快照，返回成功且新界面不受影响。

### 2026-11-09 · 返修（实施方，SP7）

- `SnapshotGate.acceptTarget` 不再比较激活代号，只按该 worktree 自己的版本单调性判定“真实读取是否核对成功”；是否画到当前界面由 `WorktreeRefresh.readInto` 另行判定（仍是当前激活目标 **且** 激活代号未变）。
- 因此写后核对期间切换 worktree 不再让 `verifyWrite` 失败；A→B→A 后同一份核对结果也不会被画到新界面。
- 回归（生产 module，非源码字符串）：`tools/test-worktree-refresh.mjs` 新增“写后核对原目标：核对期间切换 worktree 仍算核对成功，但不发布到新界面”，含 A→B→A 与版本倒退两段；`tools/test-log-model.mjs` 中对应的接线断言随签名更新，行为由前者覆盖。
