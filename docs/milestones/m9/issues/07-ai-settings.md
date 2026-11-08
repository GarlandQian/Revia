# 07：设置“AI”页与 AI 服务配置

Status: done
Blocked by: None（可立即开始；按 Q208 排在性能任务之后实施）

## What to build

设置的“提交”分类下新增“AI”页：填写服务地址、模型名、密钥和生成语言，并能改写提示词、恢复默认。密钥存系统凭据库，并与服务地址绑定。见规格 E 节。

## Acceptance criteria

- [x] “AI”页照现有 Loop 式设置页写法，包括服务地址（带三个示例）、模型名、密钥、生成语言（跟随仓库、中文、English）、完整的默认提示词编辑区和“恢复默认”。
- [x] 服务地址、模型名、语言、自定义提示词、已确认发送的地址存进 Revia 设置；密钥只存 macOS 钥匙串或 Windows Credential Manager（两种 Windows 形态都是），复用 GitHub 凭据的 `keyring` 写法，用独立的 service 名。
- [x] 前端只能写入或清除密钥、查询是否已设置，读不到密钥原文；设置文件、日志、诊断信息里都没有密钥。
- [x] 改了服务地址后，旧密钥从凭据库删除，界面要求重新填写；已确认发送的地址只对原地址有效。
- [x] 服务地址或模型名为空即视为未配置，提供给提交信息框判断是否显示“生成”。
- [x] 默认提示词写清：只输出提交信息、首行简洁、需要时写正文说明原因、参考最近的提交写法。固定规则（语言、首行上限、空行、只输出提交信息）不放在可编辑的提示词里。
- [x] 新文案照 x18n 流程，中英文齐全。补凭据替身回归测试：写入、清除、改地址后删除旧密钥。

## Comments

- `src/components/app/SettingsDialog.tsx`：实现“提交”分类下的“AI”页，支持示例提示、模型名称、生成语言（跟随仓库、中文、英文）、密码型密钥输入、提示词大文本框及“恢复默认”。
- `src-tauri/src/ai/mod.rs` & `src-tauri/src/settings.rs`：服务地址、模型、语言与提示词保存在 `settings.json`；密钥存储使用 `io.github.garlandqian.revia.ai` 独立服务名写入钥匙串/凭据管理器，前端只能操作 set/clear/status；服务地址变更时自动清除旧地址密钥和确认记录。
- 补全基于内存替身的凭据测试 `credential_mock_write_clear_and_url_swap`，测试通过。中英文文案已由 x18n 统一管理并通过 `x18n check`。

### 规划方首轮审查 · 2026-11-08

不通过。本任务相关：S8（配置后立即刷新“生成”是否显示）、T4（`SettingsDialog.tsx` 的 `useEffect` 在提前返回之后）；未使用的 `settings.ai.*` 键。详见 [review01](../review01.md)。正式返修只有一次（Q113）。
