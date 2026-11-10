# 0.0.4 发布证据

执行状态与未完成项目见[发布记录](../../release-0.0.4.md)。本目录保存可审计摘要；原始命令、退出码、日志、失败下载残片和完整应用包保留在记录所列的独占证据目录。

- `release-source.json`、`tag-update-authorization.json`：最终源码、标签对象与本轮标签更新的授权范围。
- `main-ci.json`、`tag-ci.json`：固定源码的双平台测试和编译结果。
- `first-draft-assets.json`、`final-draft-assets.json`：首轮与重建后资产的身份、大小和摘要。
- `artifact-task-summary.json`、`artifact-verification.json`：构建来源、11 项资产、版本和架构的检查结果；保留验证器修正和下载中断的说明。
- `signatures-and-checksums.json`、`latest.json`、`updater-public-key.txt`：真实更新签名、SHA256、最终更新清单，以及本次验签所用的公开公钥。
- `windows-installed-app.json`、`7zip-provenance.json`：安装器内实际程序的资源树结果与解包工具来源。
- `desktop-package.json`：新 DMG 内应用、独立应用数据和启动进程；此文件只证明包与启动来源，桌面验收结论以发布记录为准。
- `retained-ci-log-index.json`：已保留的原始 CI 日志位置及 SHA256。
- `pre-publication-old-tags.txt`：公开前的旧版标签对象与固定源码。
- `draft-preparation.json`：草稿说明更新后的版本、状态与资产一致性；尚未公开。
