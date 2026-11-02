//! 导入本地 VS Code 颜色主题（M7 任务 02、Q198）。
//!
//! 只解析数据：读取 JSON / JSONC，按 VS Code 的 `include` 链合并 `colors`，
//! 把键映射成 Revia 的 CSS 变量名后保存到自己的数据目录。
//! 不执行主题扩展、远程脚本或任何代码；`tokenColors` 不参与（语法高亮继续用内置主题）。
//!
//! 缺失的颜色由前端回退到对应深浅色的内置主题：这里只保存用户实际提供的键，
//! 不凭空补值，避免把一套主题的部分颜色伪装成完整主题。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, ErrorKind, Result};

/// 主题适用于浅色还是深色（决定回退到哪套内置主题）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ThemeKind {
    Light,
    Dark,
}

/// 一套导入的颜色主题。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportedTheme {
    /// 稳定 id（`imported-<slug>-<hash>`）；设置里保存的就是它。
    pub id: String,
    pub label: String,
    pub kind: ThemeKind,
    /// CSS 变量名（不含前导 `--`）→ 颜色值。
    pub colors: BTreeMap<String, String>,
    /// 来源文件名（只用于展示，不保存完整路径）。
    pub source_name: String,
    pub imported_at_ms: u64,
}

/// 导入主题的存储目录。
pub struct ThemeStore {
    directory: PathBuf,
}

/// 主题文件上限：VS Code 主题远小于这个值，超过说明不是主题文件。
const MAX_THEME_BYTES: u64 = 4 * 1024 * 1024;
/// `include` 链的最大深度（防止循环引用与病态文件）。
const MAX_INCLUDE_DEPTH: u8 = 8;
/// 单套主题最多保存的颜色键数。
const MAX_COLORS: usize = 512;

impl ThemeStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            directory: data_dir.join("themes"),
        }
    }

    pub fn list(&self) -> Result<Vec<ImportedTheme>> {
        let mut themes = Vec::new();
        let entries = match std::fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(themes),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            match std::fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<ImportedTheme>(&bytes).ok())
            {
                Some(theme) => themes.push(theme),
                // 损坏的文件不阻止其余主题加载；它仍在设置里显示为不可用（resolve 时回退）。
                None => continue,
            }
        }
        themes.sort_by(|left, right| {
            (left.kind as u8, left.label.to_lowercase())
                .cmp(&(right.kind as u8, right.label.to_lowercase()))
        });
        Ok(themes)
    }

    pub fn load(&self, id: &str) -> Result<ImportedTheme> {
        let bytes = std::fs::read(self.path_for(id)?)?;
        serde_json::from_slice(&bytes).map_err(|error| {
            AppError::coded(
                ErrorKind::Internal,
                "errors.m7.themeFileUnreadable",
                "导入的颜色主题文件已损坏：{error}",
                &[("error", error.to_string())],
            )
        })
    }

    /// 导入一个主题文件。`kind` 为 `None` 时按主题的 `type` 字段判断。
    pub fn import(
        &self,
        path: &Path,
        kind: Option<ThemeKind>,
        label: Option<&str>,
    ) -> Result<ImportedTheme> {
        let metadata = std::fs::metadata(path).map_err(|error| {
            AppError::coded(
                ErrorKind::Rejected,
                "errors.m7.themeFileMissing",
                "找不到主题文件：{error}",
                &[("error", error.to_string())],
            )
        })?;
        if !metadata.is_file() {
            return Err(rejected("errors.m7.themePathFile", "主题路径不是文件"));
        }
        if metadata.len() > MAX_THEME_BYTES {
            return Err(rejected(
                "errors.m7.themeFileTooLarge",
                "主题文件过大（上限 4 MB）",
            ));
        }
        let (colors, declared_kind, declared_name) = parse_theme_chain(path, 0)?;
        if colors.is_empty() {
            return Err(rejected(
                "errors.m7.themeHasNoColors",
                "主题文件里没有可用的颜色（需要 VS Code 的 colors 字段）",
            ));
        }
        let kind = kind
            .or(declared_kind)
            .unwrap_or(ThemeKind::Light);
        let source_name = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "theme.json".into());
        let label = label
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or(declared_name)
            .unwrap_or_else(|| {
                path.file_stem()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Imported".into())
            });
        let digest = super::sha256_hex(source_name.as_bytes());
        let id = format!(
            "imported-{}-{}",
            super::safe_slug(&label),
            &digest[..8]
        );
        let theme = ImportedTheme {
            id,
            label,
            kind,
            colors,
            source_name,
            imported_at_ms: super::now_ms(),
        };
        let bytes = serde_json::to_vec_pretty(&theme)?;
        super::atomic_write(&self.path_for(&theme.id)?, &bytes)?;
        Ok(theme)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        match std::fs::remove_file(self.path_for(id)?) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn path_for(&self, id: &str) -> Result<PathBuf> {
        if !id.starts_with("imported-")
            || !id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        {
            return Err(rejected(
                "errors.m7.unknownImportedTheme",
                "未知的导入主题标识",
            ));
        }
        Ok(self.directory.join(format!("{id}.json")))
    }
}

fn rejected(code: &'static str, message: &'static str) -> AppError {
    AppError::coded(ErrorKind::Rejected, code, message, &[])
}

/// 解析主题文件（含 `include` 链），返回颜色、主题类型与名称。
fn parse_theme_chain(
    path: &Path,
    depth: u8,
) -> Result<(BTreeMap<String, String>, Option<ThemeKind>, Option<String>)> {
    if depth > MAX_INCLUDE_DEPTH {
        return Err(rejected(
            "errors.m7.themeIncludeTooDeep",
            "主题的 include 链过深或循环引用",
        ));
    }
    let text = std::fs::read_to_string(path).map_err(|error| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.themeFileUnreadable",
            "读取主题文件失败：{error}",
            &[("error", error.to_string())],
        )
    })?;
    let value = parse_jsonc(&text)?;
    let object = value.as_object().ok_or_else(|| {
        rejected("errors.m7.themeNotObject", "主题文件必须是 JSON 对象")
    })?;

    let mut colors = BTreeMap::new();
    let mut inherited_kind = None;
    if let Some(include) = object.get("include").and_then(|value| value.as_str()) {
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        let parent = base.join(include);
        // include 只允许同目录及其子目录，且必须是文件：不跟随绝对路径到别处。
        if include.starts_with('/') || include.contains("..") {
            return Err(rejected(
                "errors.m7.themeIncludeOutsideDirectory",
                "主题的 include 只能指向同目录下的文件",
            ));
        }
        let (parent_colors, parent_kind, _) = parse_theme_chain(&parent, depth + 1)?;
        colors = parent_colors;
        inherited_kind = parent_kind;
    }

    if let Some(map) = object.get("colors").and_then(|value| value.as_object()) {
        for (key, value) in map {
            let Some(color) = value.as_str() else { continue };
            let Some(name) = css_variable_name(key) else {
                continue;
            };
            let Some(color) = normalize_color(color) else {
                continue;
            };
            if colors.len() >= MAX_COLORS {
                break;
            }
            colors.insert(name, color);
        }
    }

    // 只给了编辑器前景色的主题，把它同时用作界面前景色，避免与导入的背景不搭。
    if !colors.contains_key("ui-foreground") {
        if let Some(color) = colors.get("editor-foreground").cloned() {
            colors.insert("ui-foreground".into(), color);
        }
    }

    let kind = object
        .get("type")
        .and_then(|value| value.as_str())
        .and_then(|value| match value {
            "light" | "hcLight" | "hclight" => Some(ThemeKind::Light),
            "dark" | "hcDark" | "hcdark" => Some(ThemeKind::Dark),
            _ => None,
        })
        .or(inherited_kind)
        // 没写 `type` 的主题按背景亮度判断，避免深色主题被当成浅色。
        .or_else(|| infer_kind(&colors));
    let name = object
        .get("name")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    Ok((colors, kind, name))
}

/// 按 `editor-background` 的亮度判断深浅色（无法判断时返回 `None`）。
fn infer_kind(colors: &BTreeMap<String, String>) -> Option<ThemeKind> {
    let value = colors.get("editor-background")?;
    let hex = value.strip_prefix('#')?;
    let (red, green, blue) = match hex.len() {
        3 | 4 => {
            let digit = |index: usize| {
                u8::from_str_radix(&hex[index..index + 1], 16)
                    .ok()
                    .map(|value| value * 17)
            };
            (digit(0)?, digit(1)?, digit(2)?)
        }
        6 | 8 => {
            let digit = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).ok();
            (digit(0)?, digit(2)?, digit(4)?)
        }
        _ => return None,
    };
    // 近似相对亮度：人眼对绿色最敏感。
    let luminance = 299 * red as u32 + 587 * green as u32 + 114 * blue as u32;
    Some(if luminance >= 128_000 {
        ThemeKind::Light
    } else {
        ThemeKind::Dark
    })
}

/// VS Code 颜色键 → Revia 的 CSS 变量名（与 `tools/gen-themes.mjs` 一致）。
fn css_variable_name(key: &str) -> Option<String> {
    let lowered = key.trim().to_ascii_lowercase();
    if lowered.is_empty() || lowered.len() > 80 {
        return None;
    }
    let name = if lowered == "foreground" {
        // 与 shadcn 的语义色同名，主题侧改名为 `ui-foreground`（避免自引用）。
        "ui-foreground".to_string()
    } else {
        lowered.replace('.', "-")
    };
    if name
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        Some(name)
    } else {
        None
    }
}

/// 只接受纯颜色值：`#rgb` / `#rrggbb` / `#rrggbbaa`，或 rgb/hsl 函数形式。
///
/// 拒绝 `var(...)`、`url(...)`、`expression(...)` 等能把外部内容带进样式表的值。
fn normalize_color(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return None;
    }
    if let Some(hex) = trimmed.strip_prefix('#') {
        if matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Some(trimmed.to_string());
        }
        return None;
    }
    let lowered = trimmed.to_ascii_lowercase();
    if (lowered.starts_with("rgb(") || lowered.starts_with("rgba(")
        || lowered.starts_with("hsl(") || lowered.starts_with("hsla("))
        && lowered.ends_with(')')
        && lowered.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '(' | ')' | ',' | '.' | '%' | ' ' | '/' | '+' | '-')
        })
    {
        return Some(trimmed.to_string());
    }
    None
}

/// 去掉 JSONC 的注释与尾逗号后解析（VS Code 主题文件是 JSONC）。
///
/// 按字符处理：主题名和注释里可能有中文，不能按字节切。
fn parse_jsonc(text: &str) -> Result<serde_json::Value> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                out.push(ch);
            }
            '/' if chars.peek() == Some(&'/') => {
                while let Some(&next) = chars.peek() {
                    if next == '\n' {
                        break;
                    }
                    chars.next();
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
            }
            _ => out.push(ch),
        }
    }
    let trimmed = strip_trailing_commas(&out);
    serde_json::from_str(&trimmed).map_err(|error| {
        AppError::coded(
            ErrorKind::Rejected,
            "errors.m7.themeJsonInvalid",
            "主题文件不是有效的 JSON / JSONC：{error}",
            &[("error", error.to_string())],
        )
    })
}

fn strip_trailing_commas(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    let mut pending: Vec<char> = Vec::new();
    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            // 字符串也是「有意义的字符」：先把挂起的逗号与空白放回去。
            if !pending.is_empty() {
                out.extend(pending.drain(..));
            }
            in_string = true;
            out.push(ch);
            continue;
        }
        if ch == ',' {
            pending.push(ch);
            continue;
        }
        if matches!(ch, '}' | ']') {
            pending.clear();
            out.push(ch);
            continue;
        }
        if ch.is_whitespace() {
            // 逗号与空白一起挂在 pending 上，等下一个有意义的字符决定去留。
            if pending.is_empty() {
                out.push(ch);
            } else {
                pending.push(ch);
            }
            continue;
        }
        if !pending.is_empty() {
            out.extend(pending.drain(..));
        }
        out.push(ch);
    }
    out.extend(pending.drain(..));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn imports_jsonc_with_include_chain_and_rejects_unsafe_values() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "base.json",
            r##"{ "type": "dark", "colors": { "editor.background": "#101010", "foreground": "#dddddd" } }"##,
        );
        let path = write(
            dir.path(),
            "child.jsonc",
            r##"{
              // 注释与尾逗号都是合法的 JSONC
              "name": "My Theme",
              "include": "./base.json",
              "colors": {
                "editor.foreground": "#eeeeee",
                "bad.key": "url(http://example.com/x)",
                "weird.value": "var(--x)",
              },
            }"##,
        );
        let store = ThemeStore::new(dir.path());
        let theme = store.import(&path, None, None).unwrap();
        assert_eq!(theme.kind, ThemeKind::Dark);
        assert_eq!(theme.label, "My Theme");
        assert_eq!(theme.colors.get("editor-background").map(String::as_str), Some("#101010"));
        assert_eq!(theme.colors.get("editor-foreground").map(String::as_str), Some("#eeeeee"));
        assert_eq!(theme.colors.get("ui-foreground").map(String::as_str), Some("#dddddd"));
        assert!(!theme.colors.values().any(|value| value.contains("url(")));
        assert!(!theme.colors.values().any(|value| value.contains("var(")));
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, theme.id);
        store.remove(&theme.id).unwrap();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn rejects_include_outside_directory_and_broken_json() {
        let dir = tempfile::tempdir().unwrap();
        let outside = write(
            dir.path(),
            "escape.json",
            r#"{ "include": "../../etc/passwd", "colors": {} }"#,
        );
        let store = ThemeStore::new(dir.path());
        assert!(store.import(&outside, None, None).is_err());
        let broken = write(dir.path(), "broken.json", "{ colors: ");
        assert!(store.import(&broken, None, None).is_err());
    }

    #[test]
    fn color_values_are_restricted_to_plain_colors() {
        assert_eq!(normalize_color("#abc").as_deref(), Some("#abc"));
        assert_eq!(normalize_color("rgb(1, 2, 3)").as_deref(), Some("rgb(1, 2, 3)"));
        assert!(normalize_color("url(x)").is_none());
        assert!(normalize_color("red; }").is_none());
        assert!(normalize_color("").is_none());
    }
}
