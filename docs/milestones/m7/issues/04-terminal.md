# 04：运行归属 worktree 的内置终端

Status: ready-for-agent
Blocked by: None

## What to build

提供真正可输入的终端，每个 worktree 多会话，切换保留进程，关闭与重启遵守已确认策略。

依据：规格 D；Q178、Q196。

## Acceptance criteria

- [ ] Rust 创建 PTY，前端输入字节 / resize；正常 shell、UTF-8 / 中文、交互和退出状态可观察
- [ ] 多个 worktree / 会话相互独立，运行中关闭可取消，应用关闭协调保留仍运行的会话
- [ ] 重启恢复页签 / 起始目录并新开 shell；不恢复旧进程、不重放输入、不持久保存输出
- [ ] 写入期间显示所属 worktree 忙碌，终端变化触发外部核对；Git 命令日志仍可查看
- [ ] 真实安全子进程验证读写 / resize / 取消 / 退出，故障不清理用户进程

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments
