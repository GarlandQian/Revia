//! 解析 `git diff` 输出：hunk 一律来自 Git（ADR-0004），前端只接收解析结果。
//!
//! 解析器只做“读取”：把 patch 拆成文件、hunk 和带行号的行，不重新计算差异。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LineKind {
    Context,
    Deletion,
    Addition,
    /// `\ No newline at end of file`。
    NoEol,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    /// 去掉前缀（` ` / `+` / `-`）之后的内容。
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    /// `@@ -a,b +c,d @@ section` 里的 section 部分。
    pub section: String,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    /// 原始 hunk 头，用于报告与核对。
    pub header: String,
    pub lines: Vec<DiffLine>,
}

use crate::git::status::FileStatus;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub binary: bool,
    pub hunks: Vec<Hunk>,
    /// 新增行数与删除行数（列表显示用）。
    pub added: u32,
    pub deleted: u32,
    /// 旧版本与新版本参与高亮的行数。
    pub old_lines: u32,
    pub new_lines: u32,
    /// 原始 diff 字节切片，用于计算与展示严格一致的文件级指纹（Q80、ADR-0009）。
    pub raw_patch: Vec<u8>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffParseResult {
    pub files: Vec<FileChange>,
}

impl DiffParseResult {
    pub fn line_count(&self) -> usize {
        self.files.iter().map(|file| file.hunks.iter().map(|hunk| hunk.lines.len()).sum::<usize>()).sum()
    }

    pub fn hunk_count(&self) -> usize {
        self.files.iter().map(|file| file.hunks.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// 解析完整的 `git diff` 输出（以原始字节切片解析，保证与 Git 单文件输出字节 100% 一致）。
pub fn parse_bytes(bytes: &[u8]) -> DiffParseResult {
    let mut result = DiffParseResult::default();
    for slice in split_file_slices(bytes) {
        if let Some(file) = parse_single_file(slice) {
            result.files.push(file);
        }
    }
    result
}

/// 解析完整的 `git diff` 字符串输出。
pub fn parse(patch: &str) -> DiffParseResult {
    parse_bytes(patch.as_bytes())
}

fn split_file_slices(bytes: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let header = b"diff --git ";
    if bytes.starts_with(header) {
        starts.push(0);
    }
    for (i, window) in bytes.windows(header.len() + 1).enumerate() {
        if window[0] == b'\n' && &window[1..] == header {
            starts.push(i + 1);
        }
    }
    if starts.is_empty() {
        return Vec::new();
    }
    let mut slices = Vec::new();
    for i in 0..starts.len() {
        let start = starts[i];
        let end = if i + 1 < starts.len() {
            starts[i + 1]
        } else {
            bytes.len()
        };
        slices.push(&bytes[start..end]);
    }
    slices
}

fn parse_single_file(slice: &[u8]) -> Option<FileChange> {
    let mut lines = byte_lines(slice);
    let first_line = lines.next()?;
    let rest_bytes = first_line.strip_prefix(b"diff --git ")?;
    let rest_str = String::from_utf8_lossy(rest_bytes);
    let mut file = file_from_git_header(&rest_str);
    file.raw_patch = slice.to_vec();

    let mut current_hunk: Option<Hunk> = None;
    let mut old_line: u32 = 0;
    let mut new_line: u32 = 0;

    for line_bytes in lines {
        let line = String::from_utf8_lossy(line_bytes);

        if let Some(rest) = line.strip_prefix("--- ") {
            if let Some(path) = strip_prefix_path(rest) {
                if path != file.path {
                    file.old_path = Some(path);
                }
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("+++ ") {
            if let Some(path) = strip_prefix_path(rest) {
                file.path = path;
            }
            continue;
        }

        if line.starts_with("old mode ") || line.starts_with("new mode ") {
            continue;
        }
        if line.starts_with("new file mode ") {
            file.status = FileStatus::Added;
            continue;
        }
        if line.starts_with("deleted file mode ") {
            file.status = FileStatus::Deleted;
            continue;
        }
        if let Some(rest) = line.strip_prefix("rename from ") {
            file.old_path = Some(unquote(rest.trim()));
            file.status = FileStatus::Renamed;
            continue;
        }
        if let Some(rest) = line.strip_prefix("copy from ") {
            file.old_path = Some(unquote(rest.trim()));
            file.status = FileStatus::Renamed;
            continue;
        }
        if let Some(rest) = line.strip_prefix("rename to ") {
            file.path = unquote(rest.trim());
            continue;
        }
        if let Some(rest) = line.strip_prefix("copy to ") {
            file.path = unquote(rest.trim());
            continue;
        }
        if line.starts_with("similarity index ") || line.starts_with("dissimilarity index ") {
            continue;
        }
        if line.starts_with("index ") {
            continue;
        }

        if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
            file.binary = true;
            continue;
        }

        if let Some(rest) = line.strip_prefix("@@ ") {
            if let Some(hunk) = current_hunk.take() {
                file.hunks.push(hunk);
            }
            let header = format!("@@ {rest}");
            if let Some(hunk) = parse_hunk_header(&header) {
                old_line = hunk.old_start;
                new_line = hunk.new_start;
                current_hunk = Some(hunk);
            }
            continue;
        }

        let Some(hunk) = current_hunk.as_mut() else { continue };

        if let Some(text) = line.strip_prefix('\\') {
            hunk.lines.push(DiffLine {
                kind: LineKind::NoEol,
                old_line: None,
                new_line: None,
                text: text.trim().to_string(),
            });
            continue;
        }

        let (kind, text) = match line_bytes.first() {
            Some(b'+') => (LineKind::Addition, &line[1..]),
            Some(b'-') => (LineKind::Deletion, &line[1..]),
            Some(b' ') => (LineKind::Context, &line[1..]),
            None => (LineKind::Context, ""),
            Some(_) => (LineKind::Context, &line[..]),
        };

        let (old_number, new_number) = match kind {
            LineKind::Addition => {
                let number = new_line;
                new_line += 1;
                (None, Some(number))
            }
            LineKind::Deletion => {
                let number = old_line;
                old_line += 1;
                (Some(number), None)
            }
            _ => {
                let old_number = old_line;
                let new_number = new_line;
                old_line += 1;
                new_line += 1;
                (Some(old_number), Some(new_number))
            }
        };

        hunk.lines.push(DiffLine {
            kind,
            old_line: old_number,
            new_line: new_number,
            text: text.to_string(),
        });
    }

    if let Some(hunk) = current_hunk {
        file.hunks.push(hunk);
    }

    let mut added = 0;
    let mut deleted = 0;
    let mut old_lines = 0;
    let mut new_lines = 0;
    for hunk in &file.hunks {
        for line in &hunk.lines {
            match line.kind {
                LineKind::Addition => {
                    added += 1;
                    new_lines += 1;
                }
                LineKind::Deletion => {
                    deleted += 1;
                    old_lines += 1;
                }
                LineKind::Context => {
                    old_lines += 1;
                    new_lines += 1;
                }
                LineKind::NoEol => {}
            }
        }
    }
    file.added = added;
    file.deleted = deleted;
    file.old_lines = old_lines;
    file.new_lines = new_lines;

    Some(file)
}

fn byte_lines(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    let trimmed = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    trimmed.split(|&b| b == b'\n').map(|line| {
        line.strip_suffix(b"\r").unwrap_or(line)
    })
}

/// 从 `diff --git a/x b/y` 里取一个初始路径，后续由 `+++` / `---` 覆盖。
fn file_from_git_header(rest: &str) -> FileChange {
    let (first_tok, remainder) = next_header_token(rest.trim());
    let (second_tok, _) = next_header_token(remainder.trim());
    let path = second_tok
        .as_deref()
        .and_then(strip_prefix_path)
        .or_else(|| first_tok.as_deref().and_then(strip_prefix_path))
        .unwrap_or_default();
    FileChange {
        path,
        old_path: None,
        status: FileStatus::Modified,
        binary: false,
        hunks: Vec::new(),
        added: 0,
        deleted: 0,
        old_lines: 0,
        new_lines: 0,
        raw_patch: Vec::new(),
    }
}

fn next_header_token(s: &str) -> (Option<String>, &str) {
    let s = s.trim_start();
    if s.is_empty() {
        return (None, "");
    }
    if s.starts_with('"') {
        let mut escaped = false;
        let mut end_idx = None;
        for (idx, ch) in s.char_indices().skip(1) {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                end_idx = Some(idx);
                break;
            }
        }
        if let Some(idx) = end_idx {
            let token = &s[..=idx];
            let remainder = &s[idx + 1..];
            return (Some(token.to_string()), remainder);
        }
    }
    if let Some(space_idx) = s.find(' ') {
        (Some(s[..space_idx].to_string()), &s[space_idx + 1..])
    } else {
        (Some(s.to_string()), "")
    }
}

/// 先反引号，再去 `a/`、`b/` 前缀；`/dev/null` 返回 None（审查意见 4）。
fn strip_prefix_path(text: &str) -> Option<String> {
    let text = text.trim();
    if text == "/dev/null" {
        return None;
    }
    let unquoted = unquote(text);
    if unquoted == "/dev/null" {
        return None;
    }
    let path = unquoted
        .strip_prefix("a/")
        .or_else(|| unquoted.strip_prefix("b/"))
        .unwrap_or(&unquoted);
    Some(path.to_string())
}

fn unquote(text: &str) -> String {
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        let inner = &text[1..text.len() - 1];
        let mut result = String::with_capacity(inner.len());
        let mut chars = inner.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != '\\' {
                result.push(ch);
                continue;
            }
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some(digit) if digit.is_digit(8) => {
                    let mut value = digit.to_digit(8).unwrap_or(0);
                    let mut count = 1;
                    while count < 3 {
                        match chars.peek().and_then(|ch| ch.to_digit(8)) {
                            Some(next) => {
                                value = value * 8 + next;
                                chars.next();
                                count += 1;
                            }
                            None => break,
                        }
                    }
                    if let Some(ch) = char::from_u32(value) {
                        result.push(ch);
                    }
                }
                Some(other) => result.push(other),
                None => {}
            }
        }
        result
    } else {
        text.to_string()
    }
}

fn parse_hunk_header(header: &str) -> Option<Hunk> {
    // @@ -a,b +c,d @@ section
    let body = header.strip_prefix("@@ ")?;
    let (ranges, tail) = body.split_once(" @@")?;
    let mut sections = ranges.split(' ');
    let old = sections.next()?.strip_prefix('-')?;
    let new = sections.next()?.strip_prefix('+')?;
    let (old_start, old_count) = parse_range(old)?;
    let (new_start, new_count) = parse_range(new)?;
    Some(Hunk {
        section: tail.trim().to_string(),
        old_start,
        old_count,
        new_start,
        new_count,
        header: header.to_string(),
        lines: Vec::new(),
    })
}

fn parse_range(text: &str) -> Option<(u32, u32)> {
    match text.split_once(',') {
        Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
        None => Some((text.parse().ok()?, 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
diff --git a/src/app.ts b/src/app.ts
index 1111111..2222222 100644
--- a/src/app.ts
+++ b/src/app.ts
@@ -1,4 +1,5 @@ export function run
 const a = 1;
-const b = 2;
+const b = 3;
+const c = 4;
 
 export default a;
@@ -10,3 +11,2 @@
 tail();
-removed();
 last();
diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+hello
+world
diff --git a/logo.png b/logo.png
index aaaa..bbbb 100644
Binary files a/logo.png and b/logo.png differ
";

    #[test]
    fn parses_multiple_files_and_hunks() {
        let result = parse(SAMPLE);
        assert_eq!(result.files.len(), 3);
        assert_eq!(result.hunk_count(), 3);

        let app = &result.files[0];
        assert_eq!(app.path, "src/app.ts");
        assert_eq!(app.status, FileStatus::Modified);
        assert_eq!(app.hunks[0].section, "export function run");
        assert_eq!(app.hunks[0].old_start, 1);
        assert_eq!(app.hunks[0].new_count, 5);
    }

    #[test]
    fn assigns_old_and_new_line_numbers() {
        let result = parse(SAMPLE);
        let hunk = &result.files[0].hunks[0];
        let lines = &hunk.lines;
        assert_eq!((lines[0].kind, lines[0].old_line, lines[0].new_line), (LineKind::Context, Some(1), Some(1)));
        assert_eq!((lines[1].kind, lines[1].old_line, lines[1].new_line), (LineKind::Deletion, Some(2), None));
        assert_eq!((lines[2].kind, lines[2].old_line, lines[2].new_line), (LineKind::Addition, None, Some(2)));
        assert_eq!((lines[3].kind, lines[3].old_line, lines[3].new_line), (LineKind::Addition, None, Some(3)));
        // 空上下文行
        assert_eq!((lines[4].kind, lines[4].old_line, lines[4].new_line), (LineKind::Context, Some(3), Some(4)));
        assert_eq!(lines[4].text, "");
        // 第二个 hunk 的起始行号
        let second = &result.files[0].hunks[1];
        assert_eq!(second.lines[0].old_line, Some(10));
        assert_eq!(second.lines[0].new_line, Some(11));
    }

    #[test]
    fn detects_new_file_and_binary() {
        let result = parse(SAMPLE);
        assert_eq!(result.files[1].status, FileStatus::Added);
        assert_eq!(result.files[1].path, "new.txt");
        assert!(result.files[2].binary);
        assert!(result.files[2].hunks.is_empty());
    }

    #[test]
    fn detects_rename_and_no_newline_marker() {
        let patch = "\
diff --git a/old.ts b/new.ts
similarity index 90%
rename from old.ts
rename to new.ts
index 111..222 100644
--- a/old.ts
+++ b/new.ts
@@ -1,2 +1,2 @@
 keep();
-change();
\\ No newline at end of file
+changed();
";
        let result = parse(patch);
        let file = &result.files[0];
        assert_eq!(file.status, FileStatus::Renamed);
        assert_eq!(file.path, "new.ts");
        assert_eq!(file.old_path.as_deref(), Some("old.ts"));
        assert_eq!(file.hunks[0].lines.len(), 4);
        assert_eq!(file.hunks[0].lines[2].kind, LineKind::NoEol);
    }

    #[test]
    fn hunk_without_count_defaults_to_one() {
        let result = parse("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -5 +5 @@\n-a\n+b\n");
        let hunk = &result.files[0].hunks[0];
        assert_eq!((hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count), (5, 1, 5, 1));
    }

    #[test]
    fn empty_patch_has_no_files() {
        assert!(parse("").is_empty());
        assert_eq!(parse("\n").files.len(), 0);
    }

    #[test]
    fn parses_quoted_paths_and_special_characters() {
        let patch = r#"diff --git "a/q\"x.txt" "b/q\"x.txt"
new file mode 100644
index 0000000..b39eb90
--- /dev/null
+++ "b/q\"x.txt"
@@ -0,0 +1 @@
+quote
diff --git "a/back\\slash.txt" "b/back\\slash.txt"
new file mode 100644
index 0000000..8f57a77
--- /dev/null
+++ "b/back\\slash.txt"
@@ -0,0 +1 @@
+backslash
diff --git "a/old\" name.txt" "b/new\" name.txt"
similarity index 100%
rename from "old\" name.txt"
rename to "new\" name.txt"
"#;
        let result = parse(patch);
        assert_eq!(result.files.len(), 3);
        assert_eq!(result.files[0].path, "q\"x.txt");
        assert_eq!(result.files[0].status, FileStatus::Added);
        assert_eq!(result.files[1].path, "back\\slash.txt");
        assert_eq!(result.files[1].status, FileStatus::Added);
        assert_eq!(result.files[2].path, "new\" name.txt");
        assert_eq!(result.files[2].old_path.as_deref(), Some("old\" name.txt"));
        assert_eq!(result.files[2].status, FileStatus::Renamed);
    }
}
