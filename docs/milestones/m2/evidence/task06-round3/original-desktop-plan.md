# 任务 06 真实桌面验收计划（只读准备，尚未执行）

编制日期：2026-10-23。规格与现有 UI 固定读取 `82d305e`；不包含 Paseo 正在实施的未提交改动。
依据：`docs/milestones/m2/spec.md` 第 3／6 节、T05／T13、D03／D05／D06，任务 `06-current-worktree-refresh.md`，Q88／Q89／Q90／Q95。

## 1. 执行边界与前置核对

- 仅使用新建 `/private/tmp/revia-task06-*` 下的仓库、linked worktree、HOME、设置、控制文件与证据；不添加真实用户仓库，不改用户 Git 配置，不复用任务 03 现场。
- 本文是待执行计划，不表示已生成 fixture、运行测试、打开 GUI 或通过验收。实施完成后先固定源码 SHA、Release 二进制 SHA、构建时间、PID、实际 `data_dir`，测量中不得重建同一路径二进制。
- 普通模式用隔离 HOME；只设置 `REVIA_MEASURE_DATA_DIR` 不会隔离普通运行。以启动日志中的实际目录为准，不以环境变量名称推断隔离成功。
- 现有 UI：`选择文件夹…`／⌘O、`刷新`、并排／统一、未暂存／未跟踪／已暂存组及整文件／hunk 按钮。使用实际可访问名称定位，不写固定坐标。
- `82d305e` 没有可见 worktree 选择器；`App.tsx` 的 `addRepo` 取第一个非 bare worktree，选择 linked 路径仍可能打开主 worktree。06 交付后先核对实际切换入口，以分支、完整路径及内容哨兵确认目标，不能把重启换设置冒称同进程 A→B→A。
- `82d305e` 没有运行时“延迟指定响应”或“仅监视器失败”的接缝。下文 R／F 组必须先确认实施方现有接缝或另经规划方批准的最小验收适配；缺少接缝就标记未覆盖，不靠概率点击宣布通过，也不为执行本计划擅改生产代码。
- 不提前要求 04 的丢弃确认、05 的审阅标记或 M4 的完整侧栏；本轮“旧确认”主要核对已实现的 hunk／写目标版本，后续确认弹窗留集成验收。

## 2. 最小 fixture

两个独立仓库 A、B，加 A 的 linked worktree L。A/B 用来排除共同 Git 元数据干扰；L 用来检查 `.git` 文件、每 worktree 元数据及共享 Git 元数据。

| 文件／对象 | 用途 |
| --- | --- |
| `00-first.ts`、`05-before.ts`、`10-target.ts`、`20-after.ts`、`99-last.ts` | 5 个改动条目，目标在中部；可区分“附近文件”与错误地总选第一项 |
| `10-target.ts` | 180 行，在第 10／80／140 行有 3 个分离 hunk；A/B/L 的路径、行数、布局相同但有不同内容哨兵 |
| `untracked.txt` | 需要时新建，用于折叠组跨组跟随及迟到的首次元数据加载；不与跟踪文件测试混在一起 |
| A 的 `fixture-alt` 提交 | 仅改变 `20-after.ts`，为 L 的相关共享 ref 变更提供可见差异，不依赖用户历史 |
| 独立 `evidence/` | 放原始状态、哈希、步骤时间线与门控记录，不放进受监视仓库制造额外事件 |

以下准备代码**待执行**；每次创建新目录，不覆盖旧现场：

```bash
python3 - <<'PY'
import json, os, pathlib, shutil, subprocess, tempfile
r = pathlib.Path(tempfile.mkdtemp(prefix="revia-task06-", dir="/tmp")).resolve()
h = r / "home"
h.mkdir()
(r / "evidence").mkdir()
(h / ".gitconfig").write_text('[user]\n name = Revia fixture\n email = revia-fixture@example.invalid\n[init]\n defaultBranch = main\n[commit]\n gpgsign = false\n[core]\n autocrlf = false\n')
env = {k: v for k, v in os.environ.items() if not k.startswith(("GIT_", "REVIA_"))}
env.update(HOME=str(h), GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=str(h / ".gitconfig"))
real_git = shutil.which("git")
assert real_git
def g(w, *args):
    return subprocess.run([real_git, "-C", str(w), *args], env=env, check=True, capture_output=True).stdout
neighbors = ["00-first.ts", "05-before.ts", "20-after.ts", "99-last.ts"]
base = [f"export const value{i:03d} = {i};\n" for i in range(1, 181)]
for name in ("A", "B"):
    w = r / name
    w.mkdir()
    g(w, "init")
    (w / "10-target.ts").write_text("".join(base))
    for p in neighbors:
        (w / p).write_text('export const marker = "HEAD";\n')
    g(w, "add", "--", ".")
    g(w, "commit", "-m", "task06 fixture base")
a = r / "A"
base_oid = g(a, "rev-parse", "HEAD").decode().strip()
g(a, "switch", "-c", "fixture-alt")
(a / "20-after.ts").write_text('export const marker = "ALT_HEAD";\n')
g(a, "add", "--", "20-after.ts")
g(a, "commit", "-m", "task06 shared-ref fixture")
alt_oid = g(a, "rev-parse", "HEAD").decode().strip()
g(a, "switch", "main")
g(a, "worktree", "add", "-b", "task06-linked", str(r / "L"))
for n, name in enumerate(("A", "B", "L"), 1):
    w = r / name
    lines = list(base)
    lines[9] = f"export const value010 = {n * 1000 + 10};\n"
    lines[79] = f'export const value080 = "{name}_v1";\n'
    lines[139] = f"export const value140 = {n * 1000 + 140};\n"
    (w / "10-target.ts").write_text("".join(lines))
    for p in neighbors:
        (w / p).write_text(f'export const marker = "{name}_v1_{p}";\n')
settings = h / "Library/Application Support/io.github.garlandqian.revia/settings.json"
settings.parent.mkdir(parents=True)
settings.write_text(json.dumps({"repos": [str(r / "A"), str(r / "B")], "lastWorktreeId": None, "diffFontSize": 12}))
context = {"root": str(r), "home": str(h), "git": real_git, "A": str(a), "B": str(r / "B"), "L": str(r / "L"), "baseOid": base_oid, "altOid": alt_oid}
(r / "context.json").write_text(json.dumps(context, indent=2))
print(json.dumps(context, indent=2))
PY
```

将输出的真实根路径记为 `$ROOT`；以下命令中的 `$GIT` 取 `context.json` 的 `git`。所有外部 Git 命令在子进程使用上述隔离 HOME，清除继承的 `GIT_*` 目标变量；绝不能在用户仓库运行模板命令。
启动普通 Release 实例时清除 `REVIA_MEASURE`／`REVIA_SMOKE`／`REVIA_SOAK_*`，仅保留隔离 HOME 与独立 `REVIA_MEASURE_LOG` 用于日志；自动测量开关不要开启。

### 同路径同行数修改模板

下列片段仅替换 A 的第 80 行；`true` 可换为 `"A_v3"`，末参可用 `inplace` 或 `atomic`。两种保存方式都需覆盖，避免只验证直接写入而漏掉编辑器原子替换。

```bash
python3 - "$ROOT" A true atomic <<'PY'
import hashlib, json, os, pathlib, sys, tempfile
r = pathlib.Path(sys.argv[1]).resolve()
assert r.name.startswith("revia-task06-") and r.parent == pathlib.Path("/private/tmp")
assert sys.argv[2] in ("A", "B", "L")
p = r / sys.argv[2] / "10-target.ts"
before = p.read_bytes()
lines = before.decode().splitlines(keepends=True)
assert len(lines) == 180
lines[79] = f"export const value080 = {sys.argv[3]};\n"
after = "".join(lines).encode()
if sys.argv[4] == "atomic":
    fd, name = tempfile.mkstemp(prefix=".task06-save-", dir=p.parent)
    with os.fdopen(fd, "wb") as f:
        f.write(after)
    os.chmod(name, p.stat().st_mode & 0o777)
    os.replace(name, p)
else:
    p.write_bytes(after)
print(json.dumps({"path": str(p), "linesBefore": len(before.splitlines()), "linesAfter": len(after.splitlines()), "before": hashlib.sha256(before).hexdigest(), "after": hashlib.sha256(after).hexdigest()}))
PY
```

## 3. 每个步骤必须留的证据

- 逐步记录 `caseId / timestamp / appPid / activeWorktree / selectedPath / selectedGroup / view / externalAction / expected / observed / result`；`result` 仅在核对后填 passed／failed／not-covered。
- 外部真值至少保留 `git status --porcelain=v2 --branch -z`、未暂存／已暂存 `git diff --no-ext-diff --no-textconv -- ...`、`git ls-files --stage -z`、目标工作区字节 SHA／mode、目标暂存区 blob OID；index 哈希可补充，不能替代逻辑内容核对（读取状态可能刷新 index stat 缓存）。
- 每次关键观察保存完整窗口截图或会话截图引用，包含分支／仓库、选中组和新内容哨兵；截图与受测 PID 关联。日志只证明事件／请求顺序，颜色结论来自桌面或语义 DOM 核验，不能从日志推断像素。
- 自动刷新测试期间 Revia 保持前台，外部修改由后台命令完成；不要先切终端再回来，否则“回到前台刷新”会掩盖监视器没有工作。
- 观察等待可用 5 秒、必要时延长并记录实际耗时；这只是操作超时预算，不新增规格没有规定的刷新性能阈值。
- 迟到响应必须记录 requestId、worktreeId、发起／捕获／释放时刻、负载版本摘要及真实完成顺序；“没有看到闪回”而没有确认逆序，不算旧响应隔离证据。

## 4. 常规桌面主线（先不注入故障）

| ID | 可执行步骤 | 必须观察／核对 |
| --- | --- | --- |
| N01 | 打开 A，确认 main、完整路径 A；选择未暂存 `10-target.ts`，并排静置补色；后台直接写第 80 行为 `true` | 不点刷新、不换文件，文本从 `"A_v1"` 变为 `true`；组、路径、180 行、3 hunk、显示行数不变；新 token 颜色正确，无旧字残留 |
| N02 | 切统一，后台原子替换同一路径第 80 行为 `"A_v3"`；再直接写为 `false` | 两次均自动更新，原子替换后下一次保存也能被监视；选中不跳走、文字及高亮不沿用旧缓存 |
| N03 | 在上述最新版本通过 hunk 按钮暂存第 80 行 | Git 暂存区只有最新第 80 行变更，不能写入旧的 `"A_v1"`／`true`；第 10／140 行仍未暂存，工作区字节与模式不变 |
| N04 | 选中未暂存 target，后台 `$GIT -C "$ROOT/A" add -- 10-target.ts` | 未暂存侧消失、已暂存侧出现时跟随同路径到已暂存，显示 HEAD/index diff 与取消暂存按钮；不能跳邻居或显示旧组 |
| N05 | 保持已暂存 target，后台 `$GIT -C "$ROOT/A" restore --staged -- 10-target.ts` | 同路径回到未暂存并被选中；工作区未被改变；两视图至少各完成一次外部 stage／unstage |
| N06 | 外部整文件 stage 后再改工作区第 80 行，令 target 同时出现在两组；先选未暂存，再选已暂存，各外部改一个无关邻居文件 | 每次刷新仍优先保留原组的 target；已暂存侧不能混入工作区新文本，同文件两组不误合并 |
| N07 | 重新形成 5 个未暂存条目，选中中间 target；后台仅把 target index 和工作区恢复为 fixture HEAD | target 完全消失，选中原位置邻近的 `05-before.ts` 或 `20-after.ts`；规格未固定前／后优先，不强制某一个；不能总跳远处 `00-first.ts` |
| N08 | 选中末尾 `99-last.ts`，只清理该 fixture 文件；随后依次清理剩余 fixture 改动 | 尾项消失选附近剩余项；最后无条目时清空 diff、无残留 hunk／写按钮，不显示不存在的旧内容 |
| N09 | 在清洁 A 新建 `untracked.txt`，选中未跟踪项；外部 `git add -- untracked.txt`，再 `git restore --staged -- untracked.txt` | 未跟踪→已暂存→未跟踪均跟随同路径；返回未跟踪时目标组可见／必要时展开，首加载元数据与文字正确 |
| N10 | A 前台静置，后台连续数次修改目标与邻居，最终固定为 `"A_final"`；期间尝试正常滚动／切视图 | 最终列表和文字收敛到真实状态，不卡住、不无限刷新；不强制要求展示每个被合并的中间保存，也不按事件数要求同数刷新 |

N07／N08 的“清理”只对 fixture 指定文件执行 `git restore --source=HEAD --staged --worktree -- <exact-path>`；先校验根路径与仓库身份，不用 `reset --hard`、`clean`、通配符或删除锁。可以重新生成新 fixture 重置整个场景，不覆盖旧证据。
N01／N02 要把 Git `--numstat`、hunk 数和原始 diff 同时存下；“源文件仍 180 行”本身不能证明显示行数没变化。
旧 hunk 凭据拒绝另由真实 Git 集成入口或受控门控验证；点击刷新后的新按钮成功，不能单独证明旧凭据已经失效。06 尚未提供的丢弃确认弹窗不列为本次通过项。

## 5. A→B→A 与迟到响应：确定性协议

### 接缝要求（不是现有开关名称）

优先复用交付代码已经具备的验收接缝。门控只在独立验收实例显式启用，按 worktree／命令／单次 requestId 匹配，记录真实结果后延迟交付；不得伪造成功 payload、替换 Git 状态、重写版本或依靠全局 sleep。
需要区分“真实读取完成但响应尚未交付”和“读取尚未执行”：后者在新状态下才读取，不能证明旧结果被隔离。单次门控必须有上限和显式 release，控制文件放 fixture 外。
若接缝在后端持有整会话锁而使较新请求无法完成，不能声称已制造逆序；应记录实际序列，另选完成边界，不为了通过绕开生产互斥。

| ID | 逆序操作 | 期望及证据 |
| --- | --- | --- |
| R01 | 捕获 A/v1 的 `list_changes` 结果并暂停交付；切 B，确认 B/v1 已显示；释放 A/v1 | B 的列表、选择、分支、diff、错误及 busy 状态均不被 A 覆盖；记录 A 请求已真实返回，不能只有取消未返回 |
| R02 | 捕获 A/v1；切 B；后台改 A→v2；回 A，等待新请求显示 v2；最后释放旧 A/v1 | 相同 worktreeId 仍不得回退，必须识别这次 A 的新激活／快照；新 target/hunk 按钮不能被旧目标替换 |
| R03 | 不切 worktree，在 A 捕获旧刷新结果，再生成 v2 并发起新刷新；新结果完成后释放旧结果 | 最新快照与选择不倒退；若实现有意串行化，保留串行证据并另用 R02 验证激活代际，不编造逆序 |
| R04 | 对 `get_rows`、未跟踪首次补充元数据、`highlight_range` 各重复 R02；让旧／新文本和语法类别可区分 | 旧行、旧 metrics、旧颜色不能进入新缓存；释放后再滚开／滚回检查，不能只看当前一屏；并排双侧／统一至少各覆盖一次 |
| R05 | 滚到远处切视图，捕获旧 `row_index_for_line` 定位结果；再切文件或 A→B→A 到新版本后释放 | 旧锚点不把新文件／新视图滚动到错误位置；记录起止可见行和目标身份，不只记录请求成功 |
| R06 | A 点击暂存第 10 行；真实写入与写后快照已完成，暂缓其返回；切 B；后台改 A 第 80 行→v2；回 A并显示新状态；释放旧写结果 | 写入只影响原 A、第 10 行；A 新文字／组／选择不被旧写快照倒填；同 A 写入口在 pending 期间禁用、不积压；B 仍可浏览，不应被全局写锁锁死 |
| R07 | 延迟 A 的真实失败结果，切 B 后释放；再回 A | 错误属于 A，不污染 B；回 A 能核对真实结果。若失败发生在写前且 Git 未执行，应核对 index 未变；不将未知写入结果称“未执行” |

补充：A→B 切换可先用现有“选择文件夹…”/⌘O选择独立仓库；R02／R06 的返回 A 必须是同进程真实切换，不能通过重启完成。测试延迟读与延迟写分开运行，避免把不同竞态混成一个现象。
可选外部 Git wrapper 只能辅助延迟真实 Git 命令输出：隔离 HOME 的登录 shell PATH 才能生效（只改启动进程 PATH 不可靠）；必须透传真实 stdout/stderr/exit、路径白名单、一次性门控、记录实际 Git 路径。它不能覆盖内存中的 `get_rows` 或全部 IPC 完成边界，不能代替 R04／R05 的证明。

## 6. 监视失败、手动与回到前台

先确认失败来源是真正 watcher 注册／运行失败，而非 Git 读取失败；只令 watcher 失败且保持 fixture 可读写，才可检验 Q95 的独立降级语义。不能用删 `.git`、终止 Git 或 chmod 整个仓库替代。

| ID | 可执行步骤 | 必须观察／核对 |
| --- | --- | --- |
| F01 | 对当前 A 单独注入 watcher 注册失败或运行错误，保持文件／Git 正常；静置、切文件、切视图 | 持续可见“自动刷新不可用”，不能一闪即逝或只写日志；实际恢复监视前不因普通成功操作静默清除 |
| F02 | 保持 F01，后台改 A；点工具栏 `刷新`；再改一次并用当前支持的手动刷新快捷键 | 真值更新、文字／颜色／选择正确，但 watcher 仍失败时降级提示继续存在；不把 ⌥⌘R 当手动刷新入口 |
| F03 | 保持 F01，切到另一应用；后台改 A；只回到 Revia 前台，不点刷新 | 前台核对读取新状态，提示仍独立保留；无 watcher 干预才能独立证明前台刷新，不只验证普通后台自动刷新 |
| F04 | F01 下完成一次合法 hunk／整文件暂存或取消暂存 | 写前真实读取和指纹匹配时允许；操作只改变目标 index，工作区字节／模式不变，watch 提示不被成功结果清掉 |
| F05 | F01 下再注入一次实际写前读取失败，并尝试写入；恢复读取后关闭该操作错误 | 写入拒绝、index／工作区逻辑内容不变；操作错误与 watcher 提示可同时区分，关闭操作错误不能关闭 watcher 降级状态 |
| F06 | A watcher 失败时切 B（正常）再回 A；若随后真实恢复 A watcher，记录恢复边界 | A 的失败不显示成 B 的失败；回 A 按真实监视状态显示；只有确认恢复或重新注册成功后才允许清除降级提示，不规定实现必须自动重试 |

没有“只令 watcher 失败”的可控方式时，F01–F06 记 not-covered，并在报告中明确缺口；不要把普通读失败 banner 当 watcher 失败通过。

## 7. linked worktree 与“仅当前监视”

| ID | 可执行步骤 | 必须观察／核对 |
| --- | --- | --- |
| W01 | 登记 A/B，进入 L；外部记录 `git -C L rev-parse --show-toplevel`、`--absolute-git-dir`、`--git-common-dir` | UI 真正在 L、分支 `task06-linked`、内容 `L_v1`；L 的 `.git` 是指向 A Git 元数据的文件，不误当独立 `.git` 目录 |
| W02 | L 前台，原子替换 L/target；随后外部 stage／unstage L/target | 工作区事件与 linked 的真实 index 事件都刷新；A 的 index 和工作区逻辑状态不变；不能只监视 A/.git/index |
| W03 | L 前台，使用带旧 OID 防护的 `git -C A update-ref refs/heads/task06-linked ALT BASE`（OID 取 context） | 仅 fixture 中 L 当前分支的共享 ref 改变，L 的 HEAD/index 差异新增／变化，应自动更新；取真实 Git diff为准，不能只看分支名没变就判无效 |
| W04 | A 前台，后台改独立 B；再切 B，后台改 A；最后回 A | 当前界面不受非当前目录污染；切入时重新读取最新状态；附 watcher start／stop／active roots 记录，不能仅凭 UI 没变推出“没有监视B” |
| W05 | A→L→B→A 多次切换；每次只对非当前独立仓库修改一次，再对当前修改一次 | 当前修改仍触发，旧 worktree 没有常驻新增订阅；同一逻辑活动目标允许多个必要根目录，不把 worktree + gitdir + commonDir 错判成多 worktree 监视 |

W03 的 `update-ref` 只允许操作新建 fixture 的 `task06-linked`，先校验旧 OID；不移动用户分支、不将其当成 Revia 提供的改写历史功能。若需恢复，反向使用同样的旧 OID 防护。
仅当前监视的最小可观测证据是当前目标与 watcher 生命周期／根目录的有界诊断记录；FSEvents 共享进程、`lsof` 未列出路径或没有 UI 刷新，都不能独立证明订阅已撤销。
相关共享 Git 元数据的监视可能收到其他 linked worktree 的事件；规格约束是不为所有 worktree 常驻创建内容监视，不要求系统事件流绝对没有无关通知。

## 8. 呈现回归与收尾

- 在全新 `$ROOT/perf-fixtures` 准备既有标准 fixture，普通隔离实例分别核对 Vue 并排双侧首次自动补色、Swift 统一首次自动补色；大 TS 两视图各下滚 10 屏、上滚 5 屏、拖远并返回，记录实际行号、颜色恢复和有无整块空白。
- 06 增加自动刷新后，再在滚动中对当前大文件做一次外部修改，停止后核对最新文字、颜色、hunk 与选择；不要求每个中间事件都绘制，也不新增慢帧清零门槛。
- D06 数值沿用现有四份普通／冷负载、两视图、独立进程与前台门槛；固定最终构建，原 appear/warm、DOM/首rAF、颜色、60屏/rAF/兜底/未覆盖、可见性原字段均保留。不同目标探针不能倒填原耗时，测后截图不写成计时期间截图。
- 记录全部实例正常退出、门控已释放、没有针对测试进程的全局按名称终止；不删除任何 Git 锁，不清理旧证据，不误将临时 `.swap` 留在 fixture。
- 最终报告按 N／R／F／W 与 D06 分开列 passed／failed／not-covered；附最终源码／binary SHA、隔离路径、原始 Git 真值、截图归属、事件与门控时间线。只做常规刷新或单向 A→B 不能写成整个任务 06 通过。
