# Git 操作只通过 Git CLI 执行

Revia 的所有 Git 读写都由 Rust 层调用系统的 Git 命令行完成，不引入 git2（libgit2）或 gix。这样可以直接沿用用户已有的 Git 配置、hooks、SSH 和凭证工具，行为与终端一致，也不必维护多套实现之间的差异。读取状态一律使用机器可读格式（如 `git status --porcelain=v2 -z`）；前端只能调用 Rust 层提供的固定、带类型的操作，不能传入任意 Git 参数或 shell 命令。

## Consequences

- 每次调用都有启动进程的开销，大仓库下的状态读取速度需要实测；不要为了性能悄悄引入第二套 Git 实现。
- 从 Finder 启动的应用不继承登录 shell 的环境变量，Git 及其 hooks 的运行环境需要 Revia 专门解析。
- Revia 不自带 Git，使用用户登录 shell `PATH` 中的 Git。自带一份 Git 会与用户终端中的版本和系统级配置不一致，违背本决定的初衷。
