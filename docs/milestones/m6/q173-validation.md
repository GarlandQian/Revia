# Q173 stash 删除事务验证

2026-10-29；实现候选通过本轮自动化，独立审查与桌面验收尚未完成。用户授权范围仍以 Q173 / ADR-0014 为准，其他 Git 操作继续使用 Rust 系统 Git CLI。

## 实际机制

- 先通过现有 CLI 完成独立对象闭包归档，之后才能进入删除事务。
- 删除单条且仍有条目时，使用锁定的 `git2 0.21.0` / `libgit2 1.9.7`：先持有共享 `refs/stash.lock`，再读取真实引用和 reflog，要求完整 OID 唯一且说明一致，修改精确条目并保持其他条目的签名、时间和说明。只写 loose ref，不调用 libgit2 的引用删除或 packed refs 重写。
- 清空和删除最后一条通过系统 Git 2.56.0 的 `update-ref --no-deref --stdin`：`start`、固定引用及完整旧 OID、`prepare`；收到真实准备回执后，在其引用锁与 packed refs 锁内核对完整有序 reflog 集合，再明确 `commit` 或 `abort`。不是在解锁后另删 reflog。
- Git 提交后还核对引用和 reflog 路径的真实状态；提交失败、部分 reflog / ref 写入、超时和丢回执均保留归档并报 `NeedsVerification`，不自动重试。只回收本进程自己的子进程，不删除其他进程的锁。
- 当前写边界只支持 SHA-1 / files 仓库；SHA-256、reftable 和非空 `GIT_NAMESPACE` 保守拒绝删除，原条目及归档保留。不将该边界称为这些格式的写支持。

## 源码核实

实际依赖及库源码副本、原始哈希、x18n get / apply 原件和 Git 官方源码均保存在 `/private/tmp/revia-m6-batch-ig_nawgt/q173-validation-bnscs3s7/`。Cargo.lock 固定的是 `libgit2-sys 0.18.8+1.9.7`，不是早期评估的 1.9.3。关闭 git2 默认 transport features，仅此窄边界使用 vendored libgit2。

核实 [`transaction.c`](https://github.com/libgit2/libgit2/blob/v1.9.7/src/libgit2/transaction.c) 和 [`refdb_fs.c`](https://github.com/libgit2/libgit2/blob/v1.9.7/src/libgit2/refdb_fs.c)：提交先写 reflog 再写 ref，可能部分执行；其 packed 删除路径存在先读缓存再取 packed 锁的边界，因此不用于本实现的引用删除。

核实 Git 2.56.0 [`files-backend.c`](https://github.com/git/git/blob/v2.56.0/refs/files-backend.c)、[`refs.c`](https://github.com/git/git/blob/v2.56.0/refs.c)、[`update-ref.c`](https://github.com/git/git/blob/v2.56.0/builtin/update-ref.c)、[`hook.h`](https://github.com/git/git/blob/v2.56.0/hook.h)：prepare 持有相关锁，finish 在锁内删 log / packed / loose ref；hook 输出转入 stderr，不能伪造 stdout 事务回执。reflog unlink 失败只有警告，所以生产实现另核对删除结果。

## 真实验证与失败原件

原始 argv、源码 HEAD / diff SHA-256 / 逐文件哈希、退出码和 stdout / stderr：`/private/tmp/revia-m6-batch-ig_nawgt/codex-checks-yf3rkhgl/`。

- `q173-m6-02`：43/44；重复 OID fixture 只向当前头写回相同值，Git 未追加 reflog，测试构造失败。原件保留。修正为先保存不同 OID 再追加原 OID，并断言真实三条 reflog；`q173-duplicate-03` 通过，没有降低生产断言。
- `q173-final-rust`：603 passed / 0 failed / 1 ignored（既有忽略项），M6 44/44，15 组含 doctest。原封闭导致失败的四条删除 / 自动清理回归全部恢复通过。
- 新回归覆盖：外部追加后仍选精确中间 OID；顶 / 中 / 末删除；metadata 保留；ABA 清空拒绝；跨 linked worktree 的真实引用锁；真实终端 stash / pack-refs 竞争；其他引用并发写入保留；重复 OID 歧义拒绝；别人创建的锁不被删除；prepared hook 拒绝和 stdout 伪造；提交前中断、提交后丢回执；真实 libgit2 部分提交；Git reflog unlink 警告；packed stash 删除；独立归档恢复。
- 最后只调整命令日志总长度上限，受影响 stash 生产入口在 `q173-log-cap-stash` 重跑；整套通过结果对应的源码与该单点后续差异均有记录。
- `q173-final-frontend`（含 tsc）、`q173-final-lint`、Node `q173-final-node` 435/435 通过；`q173-fresh-error-chain` 用本轮 Rust 实际 Git 失败载荷，3/3 通过。
- `q173-final-x18n`：1646 键，两语言完整，0 errors / 63 unused-key warnings；未删除键、未降级检查。`q173-final-extract`：57 候选 / 0 中文。新错误键均经 x18n CLI 查询和 apply。
- Rust 导出绑定造成的纯行尾空白差异先保存原件，再逐行去尾比较；没有语义变化，已恢复与 HEAD 一致。没有修改无关格式。

这些是自动化与源码核实结果，不代替新的 Standards / Spec 独立审查、固定 Release、真实桌面 / Rebased / 中英文窄窗验收；未合 main、未推送、未开 PR、未删 worktree。
