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
    /// 展示所用的原始 diff 文本切片，用于计算与展示严格一致的文件级指纹（Q80、ADR-0009）。
    pub raw_patch: String,
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

/// 解析完整的 `git diff` 输出。
pub fn parse(patch: &str) -> DiffParseResult {
    let mut result = DiffParseResult::default();
    let mut current_file: Option<FileChange> = None;
    let mut current_hunk: Option<Hunk> = None;
    let mut old_line: u32 = 0;
    let mut new_line: u32 = 0;

    for line in lines(patch) {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            finish_hunk(&mut current_file, &mut current_hunk);
            finish_file(&mut result, &mut current_file);
            current_file = Some(file_from_git_header(rest));
            continue;
        }

        let Some(file) = current_file.as_mut() else { continue };
        file.raw_patch.push_str(line);
        file.raw_patch.push('\n');

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
            file.old_path = Some(rest.to_string());
            file.status = FileStatus::Renamed;
            continue;
        }
        if let Some(rest) = line.strip_prefix("copy from ") {
            file.old_path = Some(rest.to_string());
            file.status = FileStatus::Renamed;
            continue;
        }
        if line.starts_with("rename to ") || line.starts_with("copy to ") {
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
            finish_hunk(&mut current_file, &mut current_hunk);
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

        let (kind, text) = match line.as_bytes().first() {
            Some(b'+') => (LineKind::Addition, &line[1..]),
            Some(b'-') => (LineKind::Deletion, &line[1..]),
            Some(b' ') => (LineKind::Context, &line[1..]),
            None => (LineKind::Context, ""),
            Some(_) => {
                // 未知前缀，按上下文处理，避免丢掉 Git 给出的内容。
                (LineKind::Context, line)
            }
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

    finish_hunk(&mut current_file, &mut current_hunk);
    finish_file(&mut result, &mut current_file);
    result
}

fn finish_hunk(file: &mut Option<FileChange>, hunk: &mut Option<Hunk>) {
    if let (Some(file), Some(hunk)) = (file.as_mut(), hunk.take()) {
        file.hunks.push(hunk);
    }
}

fn finish_file(result: &mut DiffParseResult, file: &mut Option<FileChange>) {
    if let Some(mut file) = file.take() {
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
        result.files.push(file);
    }
}

/// 从 `diff --git a/x b/y` 里取一个初始路径，后续由 `+++` / `---` 覆盖。
fn file_from_git_header(rest: &str) -> FileChange {
    let mut paths = rest.split(' ');
    let first = paths.next().unwrap_or_default();
    let second = paths.next().unwrap_or_default();
    let path = strip_prefix_path(second)
        .or_else(|| strip_prefix_path(first))
        .unwrap_or_default();
    let mut raw_patch = String::from("diff --git ");
    raw_patch.push_str(rest);
    raw_patch.push('\n');
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
        raw_patch,
    }
}

/// 去掉 `a/`、`b/` 前缀；`/dev/null` 返回 None。
fn strip_prefix_path(text: &str) -> Option<String> {
    let text = text.trim();
    if text == "/dev/null" {
        return None;
    }
    let path = text
        .strip_prefix("a/")
        .or_else(|| text.strip_prefix("b/"))
        .unwrap_or(text);
    Some(unquote(path))
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

/// 按 `\n` 切行；末尾空段不算行，内容里的空行在 diff 中带前缀，不会丢失。
fn lines(patch: &str) -> impl Iterator<Item = &str> {
    let mut text = patch;
    if let Some(stripped) = text.strip_suffix('\n') {
        text = stripped;
    }
    text.split('\n')
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
}
