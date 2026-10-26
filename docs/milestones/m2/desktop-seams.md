# M2 任务 06：隔离桌面验收接缝

本文件仅说明 Q92 的验收工具，不代表桌面场景已经通过。普通应用运行默认不启用接缝；不新增产品设置，不伪造 Git 或 IPC 返回值。

## 启动

同时设置 `REVIA_ACCEPTANCE=1` 与 `REVIA_ACCEPTANCE_DIR=<已存在的独立绝对目录>`。目录不能等于正常应用数据目录；应用实际将它用于 `AppState` 和设置，启动日志的 `data_dir` 可核对。缺少、相对或正常数据目录会拒绝启动，而不是默默使用用户设置。

可另外设置独立 `HOME` 与 `REVIA_MEASURE_LOG`，不要启用 `REVIA_MEASURE`、`REVIA_SMOKE` 或长测开关。先将只包含 fixture 仓库的 `settings.json` 放入验收目录。

```bash
HOME="$FIXTURE_HOME" REVIA_ACCEPTANCE=1 REVIA_ACCEPTANCE_DIR="$DIAG" \
REVIA_MEASURE_LOG="$EVIDENCE/desktop.log" \
src-tauri/target/release/bundle/macos/Revia.app/Contents/MacOS/revia
```

正常构建仍保持原有监视、轮询与滚动行为。性能验收不依赖这些接缝；普通模式只设置 `REVIA_MEASURE_DATA_DIR` 不能隔离应用数据。

## 一次性响应门控

在验收目录写入 `hold.json`。`id` 必须唯一，只接受 1–80 个字母、数字、横线、下划线；`command` 精确匹配。可选 `worktreeId` 和 `path` 精确匹配真实请求，不使用路径子串或通配符猜测目标。推荐先写临时文件，再原子改名为 `hold.json`。

```json
{"id":"R01","command":"list_changes","worktreeId":"实际登记的 ID"}
```

```json
{"id":"R04","command":"get_rows","path":"10-target.ts"}
```

支持 `list_changes`、`get_rows`、`highlight_range`、`row_index_for_line`，以及 `stage_file`、`unstage_file`、`stage_hunk`、`unstage_hunk` 的真实结果。门控位于实际操作完成、会话与写锁释放之后，不阻塞其他 worktree 的真实操作。

- 第一个匹配且已真实完成的响应认领该规则，保存为 `claimed/<id>.json` 并移除自己的 `hold.json`；其他请求正常交付。
- `captured/<id>.json` 出现后才算确认响应已被截住。摘要包含命令、worktree、路径、PID、捕获时间、成功／失败、可用的快照或内容版本，以及真实结果序列化摘要；不保存文件内容。
- 在真实桌面完成目标切换、新快照接收或后续编辑，再创建 `release/<id>` 文件释放。原始结果不经重建，原样返回给调用者。
- `delivered/<id>.json` 记录释放时间和 `explicit`。安全上限为 300 秒，超时仍交付原结果但标记 `explicit=false`；这种运行不能作为受控逆序验收通过的证据。
- ID 不复用，已有归档不覆盖。解析、认领或记录失败写入 `errors.log`，不替换真实结果；出现错误的场景应标记失败或未覆盖，不能推断门控已生效。

仅看到 UI 没有闪回不足以通过：应关联捕获、目标切换／新版本接收、显式释放、释放后截图与 Git 真值，确认需要的完成顺序确实发生。

## 监视失败与恢复

写入验收目录的 `watch-failure.txt`，内容为本次注入原因。下一次监视状态轮询停止当前通知，并持续返回“自动刷新不可用：验收注入：…”；切换 worktree 后仍会按相同原因报告失败。仓库文件、Git、读取和写前指纹校验不被替换或破坏。

监视失败期间，应验证持续提示、手动刷新、回到前台核对和基于新指纹的安全写入。不能以删 `.git`、占用 Git 锁或让仓库不可读来替代仅监视器失败。

移除自己的 `watch-failure.txt` 后，**现有失败状态不自动宣称恢复**；使用真实 UI 切换到其他 worktree 再切回，重新安装成功的 watcher 才清除错误。手动刷新成功本身不清除监视失败提示。

## 收尾

释放全部已捕获响应，记录 PID 和数据目录，正常退出验收实例。保留失败、超时与旧样本，不删除 Git 锁，不对真实用户仓库执行清理。任务状态只能根据真实桌面和对应代码版本的实际证据更新。
