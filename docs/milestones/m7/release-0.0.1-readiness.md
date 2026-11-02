# 0.0.1 Actions 打包与发布准备

日期：2026-11-01。用户指定版本 `0.0.1`，并再次明确由 GitHub Actions 打包。构建产物以 Actions 为准；打包成功与完整 M7 验收分别记录。

## 固定版本与检查

- `33fac65ebc328aa2067a0a736e9e47e2b6a1e434` 将 package.json、tauri.conf.json、Cargo.toml 及 Cargo.lock 中 revia 本身的版本统一为 `0.0.1`；依赖版本未变，发布文档示例同步。
- `node tools/check-release-version.mjs --tag v0.0.1 --repository GarlandQian/Revia` exit 0，版本一致，渠道 stable。该命令只校验标签字符串，不能证明远端构建或更新可用。
- `cargo metadata --manifest-path src-tauri/Cargo.toml --no-deps --offline --locked --format-version 1` exit 0，revia 版本为 `0.0.1`；git diff --check 通过。未为版本配置新增测试。
- 工作区原有6个未提交文件逐字节保留，不纳入本次 Actions 候选。Actions 只检出标签指向的已提交源码；不把本地含未提交性能修改的构建结果移用于该候选。

## GitHub 接线与打包

2026-11-01 实际只读核对 GarlandQian/Revia：现有 TAURI_SIGNING_PRIVATE_KEY、TAURI_SIGNING_PRIVATE_KEY_PASSWORD 两个 secret 名称，以及 REVIA_UPDATE_PUBKEY、REVIA_GITHUB_CLIENT_ID 两个 variable 名称。未读取或输出私钥值。配置存在不证明值有效、密钥配对或正式登录成功，后续以实际 CI 和桌面验收为准；前序“用户尚未准备”的记录不再用于判断当前配置是否存在。

现有 `.github/workflows/release.yml` 由 `v*` 标签触发，构建 macOS Apple Silicon 安装包、更新包、签名、SHA256 清单和 latest.json，并上传 Release 草稿；公开草稿仍是单独的发布步骤。版本标签同时触发既有 macOS / Windows 编译检查。当前准备将已提交 m7 候选送入此流程，运行和产物结果在取得后追加。

## 尚未通过的发布验收

- D06 最近一次完整记录首屏238ms，超过150ms；工作区内后续性能修改尚未完成固定候选复验。见 [原生验收记录](native-acceptance.md)。
- Q202 镜片中间帧、真实桌面合成、深色及辅助功能等完整验证仍有缺口。见 [第二批玻璃集成验证](glass-round2-integration.md)。
- 真实 GitHub 登录 / 保护规则、正式来源更新安装、草稿安装包首次下载启动等尚未通过本轮完整验收；CI 配置存在不能替代这些结果。

任务14要求“账号 / 更新正式接线或真实桌面仍缺时整包保持未完成”；当前不将 Actions 打包视为任务01 / 14 done，也不据此公开 Release 或合入 main。3项镜片 P3 仍为非阻断 backlog，不与上述未完成门槛混为一谈。

## 原始证据与本地偏离记录

证据目录 `/private/tmp/revia-0.0.1-preparation-v1634ypr` 保存 before.json、原始文件、补丁、版本校验和 Cargo metadata 原输出。规划方曾偏离已确认流程，先做本地打包：类型 / Vite / Rust Release 和0.0.1应用生成完成，但 `pnpm tauri build --bundles app,dmg` 最终 exit 1，因 Finder AppleEvent 超时（-1712）；保留 build.json / stdout / stderr。仅磁盘映像重试使用生成脚本的 --skip-jenkins 选项，exit 0，见 bundle-retry.json；该本地产物不充当 Actions 产物或正式发布证据。用户纠正后停止沿本地打包交付，转回既定 Actions 流程。


## Actions 首轮与测试兼容修正

用户已明确允许推送分支和标签并生成草稿。SSH 短时断连后沿原渠道重试，m7 / v0.0.1 原子推送成功，候选固定为 `90166485279d5ae3df5b128d07538f6fdec40d80`。实际运行：[Release Draft 41284315737](https://github.com/GarlandQian/Revia/actions/runs/41284315737)、[Compile Check 41284315681](https://github.com/GarlandQian/Revia/actions/runs/41284315681)。

标签编译检查失败：macOS 的350项lib测试通过，branch_remote的31项中29通过、2条只匹配旧Git报错文本的断言失败；日志确认检出和推送实际均被正确拒绝。Git 2.55 输出为“already used by worktree”与“[rejected] ... (fetch first)”，调整现有断言接受这些形式。Windows测试编译因batch模块单元测试无条件调用std::os::unix::fs::symlink而失败，将该Unix符号链接测试限定于unix。修改均在现有测试内，不改运行期业务逻辑或Git安全门。

安装包任务与编译检查分别运行，当前仍在构建。测试兼容修正提交到m7后，使用现有Compile Check的workflow_dispatch重新检查两平台；不重写v0.0.1标签，不把补跑结果混称为原标签检查通过。完整原件在compile-macos-job-log.txt、compile-failed-all.stdout及后续CI状态记录。


## Actions 打包已完成，编译修正待授权补跑

Release Draft运行41284315737已success，[Revia 0.0.1草稿](https://github.com/GarlandQian/Revia/releases/tag/untagged-7b898b97e55c8af0abb9)仍为draft。6项资产齐全：Revia-0.0.1-macos-arm64.dmg、Revia.app.tar.gz、其.sig、macos-SHA256SUMS.txt、latest.json和latest-prerelease.json。下载自Actions草稿后，三项SHA256均与清单一致；tar内应用短版本和bundle版本均为0.0.1，更新清单固定到v0.0.1，签名资产与清单一致，可信注释包含version:0.0.1。DMG SHA256为`40d4775f5e119fbfd3b7a402a5416b2b471989edbb9f86f037e6d8e083f09225`。未安装或运行该下载包，未将元数据匹配称为独立密码学验签或正式更新安装通过。原件见draft-release.json、actions-download.json、actions-assets/及actions-assets-verification.json。

测试修正后，本机Git2.56.0下branch_remote 31/31与Unix batch单项1/1通过，真实exit0；精确argv、原输出和源码散列在ci-fix-local-source.json与ci-fix-*.json/stdout/stderr。此本地源码含原有6个未提交文件，未将本地结果冒称为标签或Windows CI通过。

自动审批对后续m7修正推送及手动补跑作了拒绝，认为用户前次授权仅覆盖固定9016648；被拒脚本没有运行。现完成不涉及远端写入的本地相关检查与修复提交，再请求新提交及本轮必要CI修正的明确推送/补跑授权。v0.0.1标签和草稿均保持原状态，原Compile Check失败没有抹除。


## 当前收尾：用户选择保留草稿

测试兼容修正已固定为`33d3f3f2bda1648342e6eedb63a8a7856c2bd1a7`。用户随后明确允许本轮必要修复推送与CI补跑，前述自动审批授权缺口已解决；实际推送在现有SSH渠道失败：1Password代理可列出GitHub公钥，但两次签名均报`communication with agent failed` / `Permission denied (publickey)`。未更换HTTPS、加载密钥或修改配置。原件在ci-fix-authorization.json、ci-fix-push.*和ci-fix-push-retry.*。

用户对解锁1Password的请求选择“暂不处理，保留当前草稿”。据此停止后续推送和补跑，不再重复尝试。远端m7及v0.0.1仍对应9016648；本地m7包含测试修正，尚未推送，Compile Check未补跑。Release Draft打包成功与资产校验结论保留，草稿未公开；原标签Compile Check仍失败，整包验收未完成。原有6个未提交文件再次逐字节核对一致。


## 解锁后恢复推送的诊断（2026-11-01）

用户明确恢复：“1Password 解锁了，继续推送修复并补跑 CI，草稿先别公开。”当前已授权继续正常推送和补跑；本次从68f2ce4恢复，原有6个未提交文件核对不变。实际推送仍在1Password签名阶段失败，未到远端更新或workflow_dispatch。SSH客户端只读诊断确认GitHub已接受所配置公钥、代理列出3个身份，随后签名报communication with agent failed，ssh真实exit255。没有更改SSH地址、密钥或持久配置。原件在`/private/tmp/revia-0.0.1-preparation-v1634ypr/resume-ci-i3nqlii7`。

自动审批拒绝打开1Password当前界面，理由是可能暴露私密项目、原推送授权未明确涵盖界面访问。未读取凭据库，改用SSH客户端协议摘要定位；现等用户选择自行处理下一次SSH授权弹窗，或明确允许查看当前界面处理SSH提示。修复尚未推送、CI未触发，v0.0.1及Release草稿不变。


## 手动允许 SSH 后的实际补跑与下一轮修正

用户选择自行处理1Password弹窗后，现有SSH推送成功，远端m7推进到4f96c12；[Compile Check 41299292456](https://github.com/GarlandQian/Revia/actions/runs/41299292456)按该SHA实际运行，原标签不变。该轮两平台均未通过：Windows已越过Unix符号链接编译问题，但lib目标338通过/5失败，两个测试夹具按冒号拆PATH而无法定位Git；macOS的m6_flows目标132通过/1失败，实际补丁中文路径可读，断言却要求可选的b/显示前缀。原件保存在manual-ssh-retry-ma7a000u/{run-initial.*,compile-recheck-run-latest.json,all-failures.*,windows-failure.*}。

本轮仅修改测试/CI：batch和worktree_status夹具改用std::env::split_paths，元数据用fixture而非虚构zsh；中文路径断言保留真实路径和无八进制乱码要求，去掉a/b前缀假设。Compile Check加--no-fail-fast，让失败后仍执行其余测试目标，失败退出码保留。集中本地验证batch5项、worktree_status2项、中文路径1项均通过，真实exit0；原输出与源码散列在round2-*.json/stdout/stderr及round2-local-source.json。该本地结果包含原有6个未提交文件，不替代Windows CI。下一轮继续固定提交后补跑，Release保持草稿。


## 完整目标补跑后的 Windows 修正（2026-11-01）

固定684ac0d的[Compile Check 41300741222](https://github.com/GarlandQian/Revia/actions/runs/41300741222)已完整结束：macOS 853 passed、1 ignored，Release binary构建成功；Windows 677 passed、85 failed、1 ignored，另两个integration test exe以0xc0000139在入口前退出，共7个失败target。原始日志及逐例失败摘要在manual-ssh-retry-ma7a000u/round2-*；没有把macOS成功称为两平台成功。

本轮修正Windows夹具的外部Git路径表示、file URL、合法文件名、portable sequence editor，以及batch夹具的换行配置。Rust身份路径仍保留canonical结果，只在传给Git时复用platform::external_path；控制字符文件名样例在Unix保留，Windows用合法特殊文件名，字面路径负例与提交字段断言仍执行。两个Tauri mock-runtime集成测试按[官方同类问题](https://github.com/tauri-apps/tauri/issues/13419#issuecomment-3398457618)补Common Controls v6 manifest，仅作用于Windows MSVC integration test链接；不禁用测试、不关闭Tauri功能，最终修复效果待真实Windows CI确认。

另有一处运行期修正：stash本地比较包含第三父树时，临时GIT_INDEX_FILE也需通过external_path传给Git，否则Windows的verbatim路径导致index.lock Invalid argument。仍使用本次独占临时目录，不触碰用户暂存区；现有stash_local_comparison_includes_third_parent_without_touching_index覆盖结果及暂存区保持。此轮不能沿用上一轮“只有测试/CI、运行期源码不变”的结论。

本地集中验证：batch 5/5、branch_remote 31/31、commit_flow 12/12、history_readonly 43/43通过；m6_flows首次132通过、1失败，既有user_refresh_during_autostash_invalidates_the_unstarted_history_write等待hook信号15秒超时。保留首次exit101与全部原件，单独复查该用例1/1通过、exit0，未扩大等待时间或放宽断言。不称本轮完整M6单次通过。源码清单、精确argv、真实退出码和stdout/stderr见windows-round3-xc16lc4j/；本地仍包含6个原有未提交文件，均逐字节核对不变，固定源码结果由后续Actions确认。

本轮继续正常推送m7与补跑Compile Check的用户授权有效；v0.0.1标签、Release资产及草稿状态保持不变，不合main。增量审查基准固定为684ac0d，随后记录实际审查和CI结果。


## 第三轮固定源码结果与继续修复

用户自行处理SSH弹窗后，2e5f702已沿现有SSH origin正常推送到m7；[Compile Check 41306160331](https://github.com/GarlandQian/Revia/actions/runs/41306160331)实际运行该SHA。macOS整组成功，853 passed、0 failed、1 ignored，Release binary构建成功。Windows 761 passed、4 failed、1 ignored，3个target失败，Release编译因此未执行；不能称两平台已通过。

Windows已执行完整102项M6流程测试并全部通过，确认上一轮stash临时索引修正和平台文件名夹具有效；两个Tauri集成测试exe也均正常进入测试，m7_update_package的2项通过。剩余失败：branch_remote智能检出自动stash清理因独立归档的canonical Windows路径直接传Git失败；history_readonly模式用例在Windows没有构造权限变化、特殊路径夹具含Windows非法文件名；m7_updater_metadata传Node的脚本路径含verbatim前缀，报EISDIR lstat C:。本轮原始日志、每target结果、精确命令及退出码见windows-round3-xc16lc4j/，此前失败原件保留。

以684ac0d为基准、2e5f702为候选的fresh context双轴增量审查已完成，Standards硬违反0/possible smell 0、Spec 0；原文记录在任务12 Comments。审查没有把本地含旧6文件的验证或静态结论冒称Windows CI成功。后续按项目流程用新的Paseo/Pi会话修复上述4项，规划方负责复核、固定提交和继续补跑。

只读核对远端注解标签668c94b仍解析到9016648，Release v0.0.1保持draft、6项资产未变；没有重写标签、更新草稿资产、公开发布或合入main。


## 第四轮最小修复与本地验证

修复独立stash归档创建和恢复时传给Git的路径，均复用external_path；身份、outside保护和Rust存储路径保持原逻辑。现有“原对象GC后从归档恢复且不改变index/工作文件”回归改用canonical应用数据目录，覆盖Windows verbatim路径的创建与fetch边界。

history_readonly的模式用例在所有平台用真实Git update-index --chmod=+x及直接commit构造100644→100755，保留全部模式断言；特殊路径Unix样例原样保留，Windows用合法的选项形名称、方括号、前导/内部空格、replacement character与中文样例，逐文件路径/内容断言仍执行。m7_updater_metadata四处传Node的脚本或产物路径仅在外部参数边界转换，公钥全文/base64、坏公钥拒绝和真实下载验签断言未放宽。

集中本地检查branch_remote 31/31、history_readonly 43/43、m7_updater_metadata 1/1、M6 stash_archive相关8/8均通过，两个cargo命令真实exit0，共83项。原始stdout/stderr、精确argv、源码清单和旧6文件逐字节核对在windows-round4-yoz_1whi/。本地仍含旧6文件，后续固定Actions才是两平台结论；本轮无新增测试框架、无跳过测试。

流程纠正：report.md已明确70c59ac是Q113唯一正式返修窗口，后续明确缺陷应由Codex直接接手。本轮误重复派给Pi造成等待；已停止两个临时会话并保留日志，保留Pi的归档路径改动，由Codex完成其他修复和验证。没有模型渠道错误，不伪称按Q137四模型失败接管。后续CI明确问题沿用Q113直接接手，不再重复返修派发。Release草稿、v0.0.1和main不动。


## 继续发布：Actions runner 退役与下载包复验（2026-11-01）

用户再次要求“版本号0.0.1继续发布”。续接 m7，三处应用版本及 Cargo.lock 已是0.0.1；主目录main、v0.0.1固定标签、现有草稿资产和七个已有未提交文件均未改动。

固定2fa097a的Compile Check 41312653943第一轮：Windows测试和Release编译均成功；macOS只有merge目标的chunk_grouping_agrees_with_git_on_random_edits失败，case90返回不一致，其他目标继续执行。新独立诊断确认随机seed固定，两轮CI Git均2.55.0，合并源码未变；本机Git2.56.0及Apple Git2.54.0原单项均通过，仅为含既有未提交文件的补充证据，尚不能认定根因或宣布修复。原件在 `/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-merge-ci-diag-73gfeds_`。

只补跑失败macOS任务后，attempt2在分配runner前被GitHub取消。check-run124885019526注解明确为macOS14 runner计划停用；[官方公告](https://github.com/actions/runner-images/issues/13518)确认2026-11-01 14:00–24:00 UTC停用演练、11月2日正式退役。因此compile-check.yml与release.yml仅将macos-14改成macos-15，触发条件、架构、检查门槛和draft发布方式不变。原v0.0.1不重写；新CI仅证明新固定提交。

本次重新下载现有Actions草稿六项资产：均与GitHub资产digest一致，三项SHA256清单一致，DMG完整性检查通过；tar内版本0.0.1，更新清单URL固定到原tag且签名与.sig一致。使用本地既有minisign-verify库及仓库正式公开公钥独立验签，包签名和受信任version:0.0.1均通过；未读取私钥。

从DMG只读挂载后复制到独占临时安装目录，arm64二进制在独立appdata中正常启动。真实窗口添加本次自建fixture、查看两栏diff、暂存并提交成功，提交36d7d39d04c6e3cb64390d4e0c11f77618b5620d，工作区与index均无未提交改动。未覆盖用户应用或操作用户仓库。codesign --verify --deep --strict返回1（code has no resources but signature indicates they must be present）；当前没有Apple开发者签名/公证，这次直接启动成功不能替代浏览器下载后的Gatekeeper首次打开验证。

本轮证据根 `/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-0.0.1-publish-tv7bc1ag`，保存远端原始日志、下载记录、资产散列、正式更新验签和DMG检查命令/退出码、隔离启动PID与数据目录。D06、玻璃完整验证、真实GitHub授权及正式桌面更新安装原缺口仍保留；不据本次冒烟宣布M7完成。


## 本轮CI与公开前状态（2026-11-02）

[Compile Check 41325765305](https://github.com/GarlandQian/Revia/actions/runs/41325765305)固定51844d73c6b5b2c88d96621b27cd841d48ac3458，macOS15 arm64和Windows x64均success，类型检查、lint、前端构建、完整Rust目标与Release编译均通过。macOS 853 passed / 0 failed / 1 ignored；Windows 765 passed / 0 failed / 1 ignored。既有忽略项、lint和Actions Node运行时提示保留，未把成功写成零警告。此前macOS分块case90失败在此固定CI未复现，根因仍未确认；保留原失败及诊断，不放宽断言。

下载版正式更新检查在真实窗口完成，显示当前版本0.0.1、已是最新版本；草稿不公开时该结果符合渠道约定，但不等于真实升级安装通过。DMG与验签后的updater中4个普通应用文件逐字节一致。隔离应用正常退出exit0，fixture的工作区/index干净且HEAD内容严格符合已提交变更。

已将[发布说明](release-0.0.1-notes.md)写入远端现有草稿，包含安装、校验值、固定源码与已知限制；复读确认正文逐字一致、stable标记、6资产与原tag9016648不变。当前isDraft=true。已向用户说明AGENTS.md不擅自变更已确认验收门槛的要求，并询问是否允许CI通过后带现有D06、真实账号/更新安装、Gatekeeper等限制公开；等待答复，不把“继续发布”解释为自动豁免所有M7验收。M7不标完成，不合main。原有6个性能/验收文件逐字节保持。

最终原件：本轮证据根的macos15-ci.log、windows-ci.log、ci-final.json、ci-final-counts.json、dmg-updater-equivalence.json、smoke-proof.json、app-exit.json、publication-preflight.json。

收尾说明：上述记录与发布说明已提交为ff06a1a；随后正常SSH推送因1Password代理communication with agent failed / Permission denied (publickey)失败，未改SSH渠道或凭据。远端m7保持已通过CI的51844d7，本地验收文档提交尚未推送。公开限制确认仍待用户答复，Release保持草稿；本段为本地交接记录。


## 0.0.1 已公开（2026-11-02）

用户明确选择“CI通过后公开，保留已知限制”，授权本次带已披露验收缺口公开；不改变M7整包尚未完成的结论。公开前再次核对Compile Check 41325765305两平台success、固定tag9016648、六资产digest和已知限制正文一致。`gh release edit v0.0.1 --repo GarlandQian/Revia --draft=false --latest`真实exit0。

[Revia 0.0.1](https://github.com/GarlandQian/Revia/releases/tag/v0.0.1)于2026-11-02 05:20:26（Asia/Shanghai）公开，draft=false、prerelease=false，设为latest。安装包、原标签和发布说明保持原固定内容。

公开后无认证下载六项资产，散列全部与发布前及GitHubdigest一致；稳定入口`releases/latest/download/latest.json`与v0.0.1清单逐字节一致，version0.0.1、darwin-aarch64下载URL与签名对应。第一轮三条下载遇到curl52连接空响应，原件保留；仅重试失败下载后全部通过。独立子代理另以无认证公开API/清单复核发布状态、六资产和已知限制，结论一致。

证据根：`/private/var/folders/1h/cc658t3j6cb51lr_q807z5v40000gn/T/revia-0.0.1-publication-ofr_10zt`，包含授权文字、公开前固定源码/CI/资产、真实发布命令与exit、公开元数据、下载首轮和重试记录及最终校验。六个已有性能/验收未提交文件逐字节保留，未合main，未重写tag，未替换包或改变签名身份。
