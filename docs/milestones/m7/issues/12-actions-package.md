# 12：标签打包至 Release 草稿

Status: ready-for-human
Blocked by: None

## What to build

增加版本标签构建 macOS Apple Silicon 安装包与 updater 产物的工作流，先保存为草稿。

依据：规格 H；Q62、Q63、Q65、Q182、Q188。

## Acceptance criteria

- [x] main 推送仅检查；标签触发构建 / 分发产物，Release 始终 draft，保留 Windows 编译检查
- [x] 应用版本、标签、架构、安装包命名、更新 URL 和签名对应；稳定 / 预发布渠道正确
- [x] 更新私钥经 GitHub secret 注入，不进源码 / 日志；无 Apple 证书仍按既定分发方式
- [x] 补准确的 App 注册 / 更新签名 / secret / 首次打开说明，验证配置和本地产物；不推送、打标签或创建实际 Release
- [x] 工作流存在 / 本地构建不称为 Actions 实际运行，真实 CI 与分发证据留最终验收

## Validation

在生产入口和独占 fixture 中核对实际可观察结果；原始证据保存到交接指定目录。相关改动完成后集中验证，遵守本地验证规则。产品真实桌面由任务 14 统一完成。

## Comments

### Codex / review01 Standards — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/standards.md；完整证据附录保存在本轮证据目录。

10. **[P3，非阻断] 发布约定**：`.github/workflows/release.yml:118` 原名复制 dmg、未生成 SHA256 清单，违反 `docs/technical-direction.md:150,153` 的命名与校验文件约定。

## H10 发布命名与校验文件（P3，非阻断）

规则：`docs/technical-direction.md:150,153` 指定 `Revia-<版本>-macos-arm64.dmg`、`macos-SHA256SUMS.txt`。

release.yml:118–120 直接复制 Tauri 原文件名，131–143 仅上传 dmg/tar.gz/sig，无重命名或 SHA256 步骤；docs/release.md 也改写成 `Revia_<version>_aarch64.dmg`。CI 未运行，本项不声称已生成实际发布包；从工作流可确定没有实现约定的重命名/校验文件。该项为明确标准违反，但按 Q174 作为非阻断 backlog。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Codex / review01 Spec — 2026-10-31T03:14:30.306434+08:00

固定候选 `46574071b591b138a31d51be4403c29ba578a0d1`，本轴正式意见与证据摘录按原文转录。来源：/private/tmp/revia-m7-batch-g5scyw0r/review-01-oim6d9nj/spec.md；完整证据附录保存在本轮证据目录。

| F14 P1 | “URL…产物…对应”：Release不生成/上传latest.json | .github/workflows/release.yml:118,140 |

## F14 [P1] Release不产出客户端请求的更新清单

规格 S126：“版本、平台、架构、URL、签名与产物必须对应”。

`tools/check-release-version.mjs:53–58` 指向latest.json；`.github/workflows/release.yml:118–143` 只收集/上传dmg、app.tar.gz、sig，没有生成或上传latest.json；`tools/write-updater-config.mjs:48` 只开启打包产物，并未生成供updater读取的清单。触发：依该workflow构建并公开草稿。结果：配置的清单URL404，客户端无法发现更新。`release-endpoint.stdout` 为实际版本脚本exit0；`manifest-generation.stdout` 全仓工作流/工具来源搜索无生成器。未运行Actions或发布任何内容。

| F15 P1 | “签名校验”：公钥原文误传要求base64的插件 | tools/write-updater-config.mjs:27,40 |

## F15 [P1] 更新公钥配置编码与锁定插件不兼容

规格 S124：“使用 Tauri updater 的强制签名校验”；任务12：“补准确的…更新签名…说明”。

`tools/write-updater-config.mjs:27–40` 只接受含 `untrusted comment:` 的原始minisign公钥全文，并原样写pubkey；`docs/release.md:48–51`也指导填全文。锁定 `tauri-plugin-updater 2.14.1` 本地源码 `updater.rs:1901–1902,1970` 先STANDARD base64解码再PublicKey::decode。触发：按文档正确配置公钥，客户端下载合法签名更新。结果：原始带空格/冒号公钥不是base64输入，验签前必失败。以锁定crate源码核对，未下载或安装更新。插件来源hash及原件摘录在supporting-source.txt。

Codex 核对：以上为固定源码及原证据支持的审查发现；未执行的动态复现须在返修时补充，不能写成桌面通过。P1/P2 属本次正式返修范围；P3/构造边界登记 backlog 15，非阻断且不要求额外重构。原勾选项需在返修后逐项按真实证据重新核对，当前不得据旧勾选宣称 done。

### Pi 正式返修处理 — 2026-10-31

- **F14**：新增 tools/generate-latest-json.mjs（Static Format 清单：version/notes/pub_date/platforms["darwin-aarch64"]/{url,signature}；signature 为 .sig 全文的 STANDARD base64，与锁定插件 verify_signature 的解码一致）；release.yml 新增“Generate updater manifests”步骤，生成 latest.json 与 latest-prerelease.json（本地 fixture 实测生成与 JSON 校验通过，原件 repair/latest-json-test.out）。
- **F15**：write-updater-config.mjs 接受 minisign 全文或纯 base64，统一转为插件要求的 base64 编码（真实格式 fixture 实测：全文 0、base64 0、坏输入 exit 1，原件 repair/f15-*.log）。
- **H10**（P3 顺带，自然消除）：产物重命名为 Revia-<版本>-macos-arm64.dmg 并生成 macos-SHA256SUMS.txt（shasum -a 256 --tag）。
- 真实 CI 运行仍属最终验收，未执行。

### Codex / review02 Spec / F14 — 2026-10-31

| F14 P1 | S126“签名…对应”：清单将已编码.sig再次编码，合法包验签失败 | generate-latest-json.mjs:40 |

<details>
<summary>本项证据原文</summary>

## F14 [P1] 新清单生成器把签名重复编码

S126“版本、平台、架构、URL、签名与产物必须对应”。新 `tools/generate-latest-json.mjs:40–41`读取`.sig`后再base64，54把其作为manifest.signature。Tauri生成的.sig已是插件所需内容；[Tauri官方更新文档](https://v2.tauri.app/plugin/updater/#static-json-file)要求signature直接使用生成.sig内容。锁定本地 `tauri-plugin-updater-2.14.1/src/updater.rs:1903–1904`只base64解码一次，然后Signature::decode需要minisign全文；重复编码后得到的仍是base64串，decode失败。

动态证据：用`tempfile.mkdtemp`独占 `spec-probes-pgo9x6xd`，取本地minisign-verify-0.3.0公开测试数据`test`/公开签名，写入标准Tauri编码形态.sig；真实生产generate-latest-json脚本exit0，但manifest_signature_equals_input_sig=false，解码一次不以untrusted comment开头，见`signature-comparison.json`、`manifest.stdout/.stderr`。它验证生成器编码错误，未执行插件安装或真实CI。官方原件`tauri-updater-official.txt`，锁定源码及散列另见`upstream-source.txt/json`。

</details>

### Codex / review02 Spec / F15 — 2026-10-31

| F15 P1 | S124“签名校验”：按发布说明输入正常公钥全文仍exit1 | write-updater-config.mjs:39 |

<details>
<summary>本项证据原文</summary>

## F15 [P1] 发布说明要求的正常公钥全文仍不接受

S124强制验签，任务12“补准确的…更新签名…说明”。`docs/release.md:39–46,56`要求`minisign -G`的.pub全文含untrusted comment。新normalizePubkey `write-updater-config.mjs:39–45`却把第二行密钥体base64解码后要求其仍以untrusted comment开头；正常第二行解码是原始密钥字节，必失败。正确的base64包裹全文输入:49–55可以通过，但文档与兼容分支依然错误。

动态：将生产脚本逐字复制到自建fixture/tools（SHA与生产相同），配置/输出仅在fixture/src-tauri。使用minisign-verify公开的合法公钥全文，`pubkey-raw` exit1、stderr明确second line does not decode；同一公钥全文外包base64后`pubkey-base64` exit0，见各stdout/stderr及commands.json。没有生成生产签名身份，没有写产品tauri.release.conf。实施旧f15-full.log也保存同类失败，报告“真实格式exit0”并不能证明raw全文兼容。

</details>


### Codex 直接修复 F13/F22 — 2026-10-31

已下载的真实包字节、散列和签名清单保存在应用数据目录；安装用锁定 Tauri updater 2.14.1 的 restore_update / verify 恢复并再次验签，直接安装保存的字节，不重新下载。签名必须绑定版本，旧的无版本签名不可恢复。公共状态补 downloadedVersion / restartRequired，历史回执不遮住新版本，重启后的旧回执不重复要求重启。下载回包同步全局状态，重新打开面板的较早读取不能覆盖操作结果。

安装和重启分别进入生产 CloseDriver / WriteGuard / CloseCoordinator，同步处理未保存编辑、Git 写入 / 待核对和全局终端；取消不安装、不重启。合并编辑仍归原 CodeMirror 实例，未应用内容先通过原有应用 / 取消入口处理；生命周期保留窗口与退出保护。安装成功回到应用，重启需要独立明确操作；失败取消本次更新意图，不自动重发。

渠道从构建配置绑定仓库的公开 Releases 完整分页中选取最高合适 semver；默认稳定，主动开启才含预发布，草稿排除。两类客户端使用同一仓库发现入口，再读取所选 tag 的清单；包 URL 固定到自身 tag，beta 客户端不再锁在自身旧标签。发布覆盖配置要求 requireSignedVersion，Tauri CLI 2.12.3 本机帮助确认 build 自动绑定签名版本。未读取真实私钥、未运行 Actions、未发布；docs/release.md 已同步新流程。

最终原件在批次 codex-checks：Rust 全部 23 个 integration targets 及 lib/main 已逐目标完成；首轮 codex-final-rust-01 真实 exit101，故不能称单轮全过。其前半 targets 通过，GitHub auth fixture accept 后读取 WouldBlock 导致失败；仅给隔离 HTTP fixture 的 accepted socket 明确 blocking，未改产品网络实现。codex-final-rust-continuation-01 后被用户中断，command 无真实 exit（保留 null 与 interruption.json），不得补造退出码。continuation-02 的剩余 11 targets exit0；另为 auth / checkout 补独立真实 exit。codex-final-doc-01 exit0、实际无 doc 用例。discard_recovery 保留原 1 ignored；不重复叠加授权子进程内外计数。

最终 codex-final-node-04 的 8 个相关生产模块入口 98 项通过、0 skipped；codex-final-frontend-build-02、codex-final-lint-03、codex-final-x18n-02 exit0，翻译 0 errors / 76 既有 unused-key warnings。原 updater-lifecycle-node-01 发现失败意图仍保留，修正后共享回归通过。另一次 codex-final-node-03 误用通配符，包含需要专用 fixture 参数的旧 M6 脚本，494 通过 / 8 失败、exit1；原件保留，按正确相关入口重跑通过，没有改旧 M6 范围以消除该调用错误。源码散列、精确 argv、真实 exit、分流 stdout/stderr 与全部失败原件均保留；生成绑定只清理本次空白，受影响前端检查在清理后执行。

update package 新 3 项实测下载验签、保存、关闭 fixture HTTP、新 updater 离线恢复并实际安装，只替换独占 fixture .app；另 3 项渠道与 1 项公开旧签名元数据回归通过。含坏包、版本 / 公钥 / 渠道篡改及原子落盘失败。上述均不代表真实 Revia 桌面更新、真实账号 / 钥匙串、发布 CI 或正式签名来源已验。任务13仍缺 Rebased 1.1.22 既有暂存内容真实桌面观察，14最终桌面受阻；不合 main、不删除 worktree、不删除监控。


### 2026-10-31：发布包 GitHub App Client ID 接线

按用户提供的发布接线说明，`release.yml` 在 build-macos 整个任务环境注入公开仓库变量 `REVIA_GITHUB_CLIENT_ID`，前置检查在其缺失或为空时直接失败。Rust现有运行时/编译时读取链路已核对，无需写死Client ID或使用client secret。`docs/release.md`同步变量表和缺失处理。只读GitHub元数据确认两个仓库变量均非空、两个签名Secret名称存在；没有读取密钥值或修改远端设置。

`release-client-id-config-01`真实exit0：生产YAML逐字复制到独占fixture解析，确认任务级注入与检查顺序；bash语法通过，正常配置通过，Client ID空/缺失及原有私钥/公钥/地址缺失五个拒绝场景符合预期，原stdout/stderr和各exit保留。全部输入为fixture占位值，未签名、构建安装包、运行Actions或发布；不把仓库变量存在等同于GitHub登录/更新安装验收通过。用户另由Claude处理Liquid Glass，本增量不修改其UI工作。


### review17：发布 Client ID 接线独立双轴审查 — 2026-10-31

固定范围 `51db991de514d912e44824479c407c2cd71b0389...88320fa5dc687e53acb7b40b0e2b8c2856c096bb`，只审此次三路径发布接线，不包含此前性能未提交修改或Claude的Liquid Glass工作。以下两轴分别原文追加。

#### Standards（原文）

未发现本增量违反仓库书面规范的硬问题，也未发现值得提出的 Fowler smell；两类发现均为 0，无新增 backlog 候选。

审查固定范围为 `git diff 51db991de514d912e44824479c407c2cd71b0389...88320fa5dc687e53acb7b40b0e2b8c2856c096bb`，仅三条路径。`.github/workflows/release.yml:53,107–110` 从公开仓库 variable 注入 Client ID，在构建前拒绝缺失/空值，不写死身份、不读取 client secret；两处文档同步接线和验证边界，任务仍是 `ready-for-human`。符合 `AGENTS.md` 的增量审查/安全范围、`docs/technical-direction.md:140–159,165` 的分发与认证边界，以及 `docs/agents/issue-tracker.md` 的 Comments 记录约定。

已核对 `release-client-id-config-01` 原始命令、源码清单及独占 fixture 的六组 stdout/stderr/exit：正常配置 exit0，Client ID 空/缺失与三个既有缺失案例均 exit1。fixture 工作流 SHA256 与固定提交相同。本轴没有重跑检查、构建、应用或远端动作；这不是 Actions、GitHub 登录、签名更新或安装验收通过的证据（`docs/agents/local-validation.md:9,16`；`docs/release.md:98,103`）。未审查或改动另六条性能修改及 Claude 的 Liquid Glass 工作。详细规范来源、hunk 判断和证据见 `standards-evidence.md`。

#### Spec（原文）

未发现本增量的规格问题（0 项）。固定范围：`51db991…88320fa`，仅审发布接线。

- **(a) 缺漏或部分实现：无。** 用户说明第二节第 4 步要求在 `build-macos.env` 注入 `REVIA_GITHUB_CLIENT_ID: ${{ vars.REVIA_GITHUB_CLIENT_ID }}`，并在指定前置步骤中“变量为空时直接失败”。工作流第 53 行、第 107–109 行均落实，检查早于第 116 行实际构建；既有 Rust `client_id()` 第 29–37 行包含编译时读取。
- **(b) 范围扩张：无。** 差异仅涉及工作流、发布配置说明和任务 12 Comments，符合用户说明“只有第二节第 4 步是代码改动”，未触及 Liquid Glass、客户端、版本或发布操作。
- **(c) 看似满足但实现错误：无。** 任务级环境覆盖 Tauri 构建，空值及缺失均以 exit 1 拒绝，保留原有三项守卫；未写死 Client ID，未引入 client secret。

已核对真实验证原件：工作流副本与固定提交 SHA256 一致，原检查 exit 0，正常配置通过、五项拒绝场景退出符合预期。本轮未重跑检查。按规格 H 第 125 行及任务 12 第 18 行，这仅证明配置与守卫，不代表真实 Actions、GitHub 登录或更新安装验收通过。详见 `spec-evidence.md`。

Codex直接核对固定源码与原始证据：工作流、发布文档字节与检查输入一致，YAML/bash及六场景退出符合预期；任务12 Comments在检查后追加，未冒称完整工作树散列固定通过。Standards硬违反0/判断0，Spec0，两轴完成即结束。接线已完成；真实Actions、GitHub登录、更新签名包安装仍未验。不推送、不打标签、不发布。


### Codex：0.0.1 Actions 实际打包（2026-11-01）

用户指定0.0.1并要求Actions打包；明确授权固定9016648的m7分支及v0.0.1标签推送、生成草稿。[Release Draft 41284315737](https://github.com/GarlandQian/Revia/actions/runs/41284315737)真实success，6项安装/更新/签名/校验/清单资产齐全。下载后SHA256、tar内应用0.0.1版本及清单/签名元数据核对一致，未安装或公开草稿。完整结果见[发布准备记录](../release-0.0.1-readiness.md)。

原标签Compile Check失败：macOS两条Git报错文本断言、Windows一个Unix专用测试未加平台条件。修正33d3f3f本地31+1通过，后续推送受1Password SSH签名通信失败阻断；用户选择暂不处理、保留草稿，因此未推送修正或补跑。任务14完整发布验收仍未完成，本任务不据打包成功改done。


### Codex：Windows CI 修正双轴增量审查（2026-11-01）

固定范围 `684ac0d492f399132e5829f6b1f1bfb8488b4b09...2e5f70282061c433e7e347d3d5513b0f80aeb223`，两个fresh context审查完成，分别记录如下。不包含原有6个未提交文件。


#### Standards

范围：`684ac0d492f399132e5829f6b1f1bfb8488b4b09...2e5f70282061c433e7e347d3d5513b0f80aeb223`，仅本轮 9 个文件；未审查或改动原有 6 个未提交文件。

结论：未发现有据可报的硬性标准违反或 Fowler possible smell。

- `src-tauri/src/stash.rs:252` 仅在外部 Git 参数边界复用 `platform::external_path`，内部 canonical 路径与独占临时目录保持原语义；没有触碰用户 index 或锁文件。符合 AGENTS.md 的 Q55、安全边界及 ADR-0002 的 CLI 约束。
- 测试夹具继续调用真实 Git，路径表示转换集中复用平台函数；Windows 合法文件名分支保留原业务断言。`build.rs` 的 manifest 接线限定 Windows MSVC 测试链接，没有扩大发布平台或禁用测试，符合 ADR-0007。
- `docs/milestones/m7/release-0.0.1-readiness.md:78` 明确区分首次失败、单例复查及包含旧 6 文件的本地源码；与原始命令退出码和测试摘要一致，符合 `docs/agents/local-validation.md` 第 3 条及完成条件。

依据同时包括 CLAUDE.md、`docs/agents/issue-tracker.md`、`docs/technical-direction.md`、ADR-0012、ADR-0014；未发现本增量与这些规则冲突。

本次只做静态审查并核对已有本地证据，未运行构建或测试。候选提交的 Windows 执行效果仍待固定 SHA 的 Compile Check 确认。


#### Spec

未发现本增量的规格问题（0 项）。固定范围为 `684ac0d492f399132e5829f6b1f1bfb8488b4b09...2e5f70282061c433e7e347d3d5513b0f80aeb223`，仅审 9 条提交路径；原有 6 个未提交文件未纳入审查、未修改。

- **(a) 缺漏或部分实现：0。** 对照发布准备记录第 74–80 行，本轮覆盖外部 Git 路径、file URL、Windows 合法文件名、sequence editor、autocrlf、测试 manifest 和 stash 临时索引路径。Windows 最终效果明确留给真实 CI，未冒称已通过。
- **(b) 未要求的范围扩张：0。** 差异限于必要测试 / 构建修正、stash 本地比较修复和对应记录；未改标签、Release、主分支或发布工作流，符合本轮授权。M7 规格第 135 行要求“保留现有 Windows 编译检查”，本轮未禁用测试或关闭 Tauri 功能。
- **(c) 看似实现但实现错误：0。** `stash.rs:252–254` 只对独占临时索引的外部 Git 表示使用 `external_path`，符合 M6 S01“内容为只读”及 C02“不删除 index.lock，不修改用户 Git 配置”。`m6_flows.rs` 的 Unix 控制字符样例仍保留，Windows 字面路径负例、提交字段、文件和 index 断言仍执行，符合 M6 V 的完整性要求。

已核对现有原始日志与真实退出码，8 条代码 / manifest 路径的 SHA256 均与候选提交一致。batch 5、branch 31、commit 12、history 43 通过；M6 首轮 132/133，既有 hook 等待超时单项复查通过。本地包含原有 6 文件，不等于固定 Windows CI 成功。按要求未重跑测试 / 构建，未推送、未使用真实用户仓库或凭据。


Standards: 0 hard violations / 0 possible smells; Spec: 0 findings.


### Codex：Windows CI 剩余修复双轴增量审查（2026-11-01）

固定范围 `2e5f70282061c433e7e347d3d5513b0f80aeb223...2fa097a18a09b79ea5d808571d5fcedf23b4c4af`，仅6条提交路径；两个fresh context独立完成，原有6个未提交文件不纳入本轮。该候选已正常推送，[Compile Check 41312653943](https://github.com/GarlandQian/Revia/actions/runs/41312653943)实际检查固定2fa097a，不重写v0.0.1或公开草稿。


#### Standards

范围：`2e5f70282061c433e7e347d3d5513b0f80aeb223...2fa097a18a09b79ea5d808571d5fcedf23b4c4af`，提交 `2fa097a` 的 6 条路径（4 个代码/测试、2 个文档）。原有 6 个未提交文件未纳入审查，未修改工作树。

结论：**0 项硬性标准违反，0 项 Fowler possible smell。**

- `src-tauri/src/stash_archive.rs:166,364` 仅在 Git argv 边界复用 `platform::external_path`，身份校验、独立归档和 Rust 内部路径保持原语义；符合 AGENTS.md 的 Q55、ADR-0002 的 Rust/Git CLI 边界及 ADR-0012/0014 的独立归档约束。`m7_updater_metadata.rs:102,104,125,179` 同样复用现有平台函数，没有分散实现平台路径规则。
- `history_readonly.rs:677,728` 通过真实 Git 构造模式变化，Windows 使用合法文件名并继续逐文件核对路径和内容，Unix 原样例保留；`m6_flows.rs:245` 用自建数据目录的 canonical 路径覆盖归档创建与恢复。符合 `docs/agents/local-validation.md` 第 2 条及 `docs/technical-direction.md` 的真实 Git、隔离夹具要求。
- 两份文档如实区分本地证据和固定 Windows CI，记录 Q113 返修窗口已用尽后的流程纠正，未改写 Q137，也未改已确认决策或提前标记完成；符合 AGENTS.md、`docs/agents/issue-tracker.md` 和本地验证第 3 条。

已核对两个命令原件均 exit 0，31+43+1+8 共 83 项通过；4 个代码/测试文件的 SHA256 与候选提交一致。本地源码仍含旧 6 文件，不能据此声称固定 Windows CI 通过。只做静态审查与既有证据核对，未运行测试或构建。


#### Spec

未发现本增量的规格问题（0 项）。固定范围为 `2e5f70282061c433e7e347d3d5513b0f80aeb223...2fa097a18a09b79ea5d808571d5fcedf23b4c4af`，仅审 6 条提交路径；原有 6 个未提交文件未纳入审查、未修改。

- **(a) 缺漏或部分实现：0。** 发布准备记录第 87、96–100 行列出的四个 Windows 失败均有对应修复；`history_readonly.rs:680–720` 用真实 Git 构造并断言 `100644→100755`，第 729–763 行保留 Unix 特殊路径及逐文件内容断言，Windows 使用合法名称，没有跳过测试或放宽断言。
- **(b) 未要求的范围扩张：0。** 差异限于必要修复、回归夹具和过程记录，符合授权“允许本轮必要修复推送与 CI 补跑”。M7 spec 第 135 行“先进入 Release 草稿”“保留现有 Windows 编译检查”及任务12第 18 行“真实 CI 与分发证据留最终验收”均未被改变；没有修改标签、资产或发布工作流。
- **(c) 看似实现但实现错误：0。** `stash_archive.rs:166、364` 仅转换 Git argv，身份和 outside 保护保持；`m6_flows.rs:247–288` 的 canonical 数据目录覆盖创建与恢复，仍验证 GC 后取回及 index/工作文件不变，符合 M6 S03 第 77–80 行“完整对象”“不直接改工作区或暂存区”和 ADR-0012 第 7–8 行。`m7_updater_metadata.rs:104、106、127、181` 仅转换 Node 路径，公钥全文/base64、坏公钥拒绝及下载验签断言仍执行，符合 M7 spec 第 133 行“强制签名校验”。

已核对四条代码路径的 SHA256 与候选提交一致，原始记录为 31+43+1+8=83 项通过、两个命令 exit0。本地包含旧 6 文件，不能替代固定 Windows CI；本审查未运行测试/构建，未执行远端、真实用户仓库或凭据操作。


Standards: 0 hard violations / 0 possible smells; Spec: 0 findings.


### Codex：macOS runner 退役迁移双轴增量审查（2026-11-02）

固定范围 `2fa097a18a09b79ea5d808571d5fcedf23b4c4af...51844d73c6b5b2c88d96621b27cd841d48ac3458`，仅3条已提交路径；两个独立fresh context代理只读审查，原有7条未提交文件不纳入。

#### Standards

0项硬性违反、0项Fowler possible smell。两个workflow唯一变化是macos-14→macos-15；目标架构、Windows检查、测试门槛、签名配置和Release草稿语义保留，符合技术方向140–159与ADR-0007。GitHub官方公告确认停用时间、11月2日退役和macos-15迁移，runner文档确认该标签为arm64。发布准备记录如实区分原CI失败、本地补充检查、下载包验证和未验项目，未将旧tag包冒称新提交产物，符合local-validation第3条。已核对既有验签、DMG检查原件exit0及codesign exit1；初始7文件哈希保留。未运行测试或构建、无远端写操作；新runner结果由固定Actions证明。

#### Spec

缺漏/部分0、范围扩张0、实现错误0。两个workflow均迁移macos-15，main检查、标签构建、draft:true、Windows检查和签名配置保持，符合spec.md:133–136；没有改变版本标签或增加发布动作。官方公告和runner文档支持当前arm64迁移。原始验签、DMG、arm64二进制、隔离fixture提交及codesign失败证据与记录一致；Gatekeeper、D06、真实账号和正式更新安装缺口保留，没有把冒烟称为M7完成。未运行测试或构建、未修改文件。

Standards: 0 hard violations / 0 possible smells; Spec: 0 findings.

迁移提交已沿现有SSH正常推送m7，[Compile Check 41325765305](https://github.com/GarlandQian/Revia/actions/runs/41325765305)固定检查51844d7。仅推送本轮工作流和发布记录，未提交原有6个性能/验收文件；v0.0.1及资产保持原固定版本。


### Codex：0.0.1公开完成（2026-11-02）

用户明确接受已知限制并要求CI通过后公开。两平台Compile Check 41325765305成功后，既有v0.0.1正式公开为latest，发布时间2026-11-02 05:20:26（Asia/Shanghai）；六资产无认证下载及散列、稳定更新清单均通过。原tag9016648和Actions安装包未重打，发布说明保留D06、真实账号/更新安装与Gatekeeper等未验限制；本次不宣布任务14或M7整体done。发布命令exit0和公开后验证原件见[发布准备记录](../release-0.0.1-readiness.md)最新章节。
