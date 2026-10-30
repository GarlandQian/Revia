# M7 分发与更新：官方事实核对

核对日期：2026-10-30。产品决定见 Q62、Q63、Q65、Q181、Q182、Q188；本文件记录技术事实，不另行决定发布方式。

## 没有 Apple 证书时的路径

Tauri updater 要求校验更新包签名，且不能关闭；公私钥可用 Tauri CLI 的 `signer generate` 自行生成。公钥随应用分发，私钥用于构建端签署更新包。私钥需要安全保留，丢失后无法继续为持有原公钥的已安装应用签署更新。这套密钥不需要 Apple 证书。[Tauri updater 官方文档](https://v2.tauri.app/plugin/updater/)

Apple 的应用代码签名、公证是独立流程。缺少这套身份验证的下载应用仍可能遇到 macOS 的安全提示；更新签名不能代替它。结合两份官方文档，M7 可沿既定 Q63 路径先打包试用，并使用独立的更新签名密钥；真正下载后的首次打开与应用更新仍须实测，不能仅凭构建成功宣布可分发。[Tauri macOS 代码签名文档](https://v2.tauri.app/distribute/sign/macos/)

## GitHub Actions

Tauri 官方提供通过 `tauri-action` 在 GitHub Actions 构建、上传产物及创建 GitHub Release 的流程，支持版本标签触发，也提供 Release 草稿配置。官方多平台示例不改变 Revia 的平台范围；本轮不锁定示例中 action 的版本，也未执行发布。[官方 GitHub Actions 指南](https://v2.tauri.app/distribute/pipelines/github/)

Revia 沿 Q62 在 main 推送时只检查，在版本标签时生成分发包。M7 只发布 macOS Apple Silicon，Windows 发布在 M8。第二轮 Q188 已确认先生成带安装包的 Release 草稿，确认后公开，公开后客户端才收到更新。已有 `.github/workflows/compile-check.yml` 仅编译检查，没有安装包上传或发布步骤。

更新产物与用户下载的 `.dmg` 不同：官方 updater 为 macOS 生成 `.app.tar.gz` 与相应 `.sig`。Revia 的具体更新产物命名在实现时确定，安装包仍沿 Q65 命名。下载与安装可以分开调用，支持 Q181 由用户确认下载、处理当前工作后再安装的交互。[Tauri updater 官方文档](https://v2.tauri.app/plugin/updater/)

## 来源原件与限制

本轮工具取得的官方正文与失败结果保存在独占目录 `/private/tmp/revia-m7-planning-fslVoN9y/`：

- `tauri-updater.json`、`tauri-github-actions.json`、`tauri-macos-signing.json`：成功读取的完整工具正文与来源 URL。
- `failed-tauri-macos-overview.json`：分发总页 `https://v2.tauri.app/distribute/macos/` 读取超时；已改用上面的签名专页。
- `failed-apple-open.json`：Apple 首次打开说明页 `https://support.apple.com/en-us/102445` 读取失败；后续内置网页工具也不可用，未将该页作为已核实来源。

正文只支持机制说明；本轮未生成密钥、读取用户凭据、配置 GitHub Secrets、打标签、上传产物或创建 Release。
