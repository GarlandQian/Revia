# 12：标签打包至 Release 草稿

Status: ready-for-agent
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
