# 04：运行归属 worktree 的内置终端

Status: ready-for-review
Blocked by: None

## What to build

提供真正可输入的终端，每个 worktree 多会话，切换保留进程，关闭与重启遵守已确认策略。

依据：规格 D；Q178、Q196。

## Acceptance criteria

- [x] Rust 创建 PTY，前端输入字节 / resize；正常 shell、UTF-8 / 中文、交互和退出状态可观察
- [x] 多个 worktree / 会话相互独立，运行中关闭可取消，应用关闭协调保留仍运行的会话
- [x] 重启恢复页签 / 起始目录并新开 shell；不恢复旧进程、不重放输入、不持久保存输出
- [x] 写入期间显示所属 worktree 忙碌，终端变化触发外部核对；Git 命令日志仍可查看
- [x] 真实安全子进程验证读写 / resize / 取消 / 退出，故障不清理用户进程

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / review01 Standards — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/standards.md；完整证据附录保存在本轮证据目录。

9. **[P2] 关闭终端直接杀进程**：`src/features/m7/TerminalPanel.tsx:271` 无确认调用 close；违反 `docs/ui-direction.md:157`“关闭运行中进程前先确认”。

## H9 关闭运行终端无确认（P2）

规则：`docs/ui-direction.md:157`：“关闭运行中进程前先确认”。

TerminalPanel.tsx:263–278 closeTab 不检查 tab.running，也无确认，先 dispose 视图再调用 terminalClose。按钮 335 行直接触发该函数。Rust terminal.rs 的 close 删除会话并在未 exited 时 killer.kill，随后 drop master。不存在后端确认兜底。

触发：终端里有长时间运行的任务，点页签的 x。后果推断：任务直接终止，用户失去取消关闭的机会。此项与“关闭面板应保留终端”的实现问题分开，本轴仅记明文确认规则。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F05 P1 | “切换保留进程”：混worktree页签；绝对cwd恢复被拒 | src/features/m7/TerminalPanel.tsx:131；src-tauri/src/m7/terminal.rs:275,429 |

## F05 [P1] 终端会话在切换、重挂载与重启不能正确恢复

规格 S91：“在应用运行期间切换保留进程”；S92：“重启只恢复页签、原 worktree 与起始目录，启动新 shell”。

`src/features/m7/TerminalPanel.tsx:75,131–154` worktree变化不清tabs也不通过terminalList认领已有进程，直接openSession；`src/App.tsx:6019–6031` 隐藏会卸载整个面板，重开又建shell，原PTY留后端成为不可达会话。切A→B时A的tabs仍显示在B标题下，可误向A输入。跨重启还必错：后端 `src-tauri/src/m7/terminal.rs:275` 返回绝对cwd，前端86–96原样持久化、147原样重开；后端429–437拒绝绝对路径，所有已保存页签失败。上述是生产状态/参数链，不依赖PTY模拟。

| F06 P1 | “关闭…前确认”：页签×直接kill | src/features/m7/TerminalPanel.tsx:263 |

## F06 [P1] 关闭运行中终端没有确认

规格 S92：“关闭运行中进程前确认，取消则保留；退出应用 / 安装更新同样纳入关闭协调器”。

`TerminalPanel.tsx:263–276,340` 点击页签×直接terminalClose；Rust `terminal.rs:386–395` 立即kill。App关闭保护 `App.tsx:1638–1655,1747–1759` 不注册终端运行状态。触发：终端中执行长任务后点×或退出。结果：没有确认/取消机会，进程直接终止。尚未实际启动任务；源码无确认分支。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Pi 正式返修处理 — 2026-10-31

- **F05**：重挂载先 `terminalList` 认领本 worktree 仍在运行的会话（不新建不可达进程）；切 worktree 清空 tabs 并销毁前端视图（后端进程保留待认领）；`resolve_cwd` 接受 worktree 内的绝对 cwd（持久化恢复），外部/逃逸路径仍拒绝（新增回归测试 `terminal::cwd_tests`）。
- **F06/H9**：关闭运行中页签先内联确认（终止并关闭 / 取消），确认才调用 terminalClose。
- **busy 提示**：面板会话运行态经 `onRunningChange` 同步给 App（terminalRunningRef），参与更新安装门禁与关闭协调；Git 写锁仍然互斥。
