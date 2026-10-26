# 05：amend、上游二次确认与完整工作流

**What to build:** 实现M3规格F06。

**Blocked by:** 03, 04（实现顺序）

**Status:** ready-for-human

初始Q114批次只实现功能；现按Q115解除执行限制并进行验证，未完成验收不伪标done。

- [x] amend预填与进行中操作禁用
- [x] 本地上游判断和版本绑定二次确认
- [x] 成功/失败/待核对反馈、hooks改动文件、审阅刷新与快捷键
- [ ] 后续验证（Q115已放行，未完成验收不宣称通过）

## Comments

2026-10-26 / Codex：从m2 073ecfe建立独立m3，整包功能实现，禁止启动GUI/唤醒/测试/构建/lint/tsc/CI。

2026-10-26 / Pi（实施方）：功能实现完成，**未验证**（用户 Q114 只实现功能）。

实现位置与要点：

- amend 预填：`App.tsx::toggleAmend` 勾选时读取 `amend_head_message`（最近一次提交完整信息），
  结果绑定代际，用户期间编辑或切换 worktree 时不覆盖新输入；无 HEAD 或任意进行中操作时禁用
  （`CommitGates.amend_available`）。
- 本地上游判断：`commit.rs::upstream_read` 用 `@{upstream}` + `merge-base --is-ancestor`，
  **不 fetch**；没有上游引用时为 `Unknown`（如实"无法判断"，不猜远端状态），
  读取错误直接报错、不当作未发布。`upstream_version` 绑定 HEAD + 分支 + 上游名 + 上游对象名。
- 二次确认：`App.tsx::confirmAmend` 校验 `upstreamVersion`、`upstreamContainsHead` 与当前
  amend 勾选，任何一项变化都使旧确认失效；Rust 执行时用同一读数再校验一次。
- 完成反馈：成功后 `refreshTarget` 刷新列表 / diff / 审阅进度（HEAD 变化按既有规则让旧标记失效），
  结果区显示新 commit ID、Git 输出、hooks 改动文件与快照记录 ID；失败/待核对保留草稿。
- 审阅进度与快捷键：提交按钮旁显示 `已审阅 x/y`（不阻止提交）；⌘K / Ctrl+K 提交，
  信息框内由信息框自己处理，其它输入控件与弹窗内不穿透；修饰键与文案分别在
  `src/lib/platform.ts` 与 `src/lib/i18n/zh-CN.ts`。

未执行：`cargo test` / `cargo check` / build / lint / tsc / GUI / 桌面验收。复选框中的
"后续验证"保持未勾选。

### 2026-10-26 规划方静态审查（唯一一次正式返修）

静态审查：amend二次确认未绑定完整预期/刷新/输入及激活代际；skipHooks失败后不复原；关闭草稿落盘未集成。一次返修补齐，禁止运行验证。


### 2026-10-26 Pi（实施方）返修完成

已按审查意见补齐（未运行任何验证）：

- **二次确认绑定完整预期**：`AmendConfirmation` 绑定 `upstreamVersion + expectationFingerprint
  + 提交信息原文 + amend + skipHooks`；前端确认时校验 worktree 激活、`gateRef` 激活代号、
  预期指纹、信息与选项逐字一致、上游仍包含（或 Unknown 无法判断），任何刷新/切换/编辑都
  使旧确认失效；后端 `verify_amend_confirmation` 写前再核上游读数与预期（`commit_flow.rs`、
  `App.tsx::AmendConfirmationOf`/`confirmAmend`）。
- **skipHooks 完成后复原**：提交完成后（无论结果）仅当输入代际未变时复原 skipHooks 并
  落盘；持久草稿不自动恢复 skip=true，切换 worktree 也复位（`App.tsx`、`commit.rs`、
  `commitDraft.ts`）。
- **关闭草稿落盘集成**：`CloseDriver` 最终 `closeWindow`/`exitApp` 前先
  `flushDraftsBeforeClose`（冲刷全部 pending + 等待所有队列 + 检查失败），失败不销毁
  窗口/退出（`App.tsx`）。
- **上游读数按配置区分**：`read_upstream_config` 用 `branch.<name>.remote/merge` 区分
  未配置 / 已配置但本地无引用（Unknown，界面显示无法判断，不猜测远端）/ 读取错误
  （`commit.rs`、`CommitContext` TS）。

### 2026-10-26 Codex 直接接管收尾

amend 凭据绑定刷新/激活/输入代际与完整预期；统一 platform 提交快捷键，其他弹窗不穿透；原 worktree 结果与草稿只按原 revision 收尾。

功能代码已提交，未经编译/测试；本轮不打开 GUI，不运行任何验证，不勾选后续验证，不将 ready-for-human 当作验收通过。历史实施描述以本段和最终报告为准。


### 2026-10-26 最终静态收尾（Codex，`1ca1e52`）

Spec P2 旧 HEAD 预填竞态已修：请求携带 expectedHead，Rust 固定对象读取并前后复核；前端回包比较 HEAD、上下文 epoch、激活及输入代际。上游未知的二次确认如实说明未知，不称已发布。

仅源码实现及静态阅读；未运行任何编译、测试或 GUI，后续验证项仍未勾选。


### 2026-10-26 Q115 验证恢复（Codex）

用户已解除不测试/不启动限制。固定源码`03736a0`的自动化验证已执行：Node 251项、Rust 304项通过，构建/类型/lint及固定Release只读冒烟通过；失败原件和覆盖范围见[验证记录](../validation-20261026.md)。原生桌面读取超时，未完成真实交互/截图、D06/完整长测及Windows编译；Status与最终验证复选框保持ready-for-human/未勾选，不冒称done。未取消休眠、不回派Pi、不推送或合并。
