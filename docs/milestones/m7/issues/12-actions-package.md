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
