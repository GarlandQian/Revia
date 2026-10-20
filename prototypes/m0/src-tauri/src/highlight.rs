//! Rust 侧语法高亮：syntect + two-face（bat 的语法与主题集）。
//!
//! 高亮对象是**完整文件版本**，不是孤立的 diff 行：多行注释、模板字符串等跨行结构必须
//! 有完整上下文才能标对颜色。前端按行号把结果映射到 diff 行。
//!
//! 性能：一次把整份文件的全部 token 传回前端会产生数 MB 的 JSON，IPC 成为瓶颈。
//! 因此高亮在专用线程里执行：解析状态从第 0 行开始累积并按文件缓存，每次只把**请求的
//! 那段行**回传前端。前端按可见区域优先的顺序分块请求，“完整文件上下文”的语义不变。

use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, FontStyle, Style, Theme};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::error::{AppError, Result};

static SYNTAX_SET: std::sync::LazyLock<SyntaxSet> = std::sync::LazyLock::new(two_face::syntax::extra_newlines);
static THEME_SET: std::sync::LazyLock<two_face::theme::EmbeddedLazyThemeSet> =
    std::sync::LazyLock::new(two_face::theme::extra);

fn theme_name(requested: Option<&str>) -> two_face::theme::EmbeddedThemeName {
    use two_face::theme::EmbeddedThemeName;
    match requested {
        Some("dark") | Some("Solarized (dark)") => EmbeddedThemeName::SolarizedDark,
        _ => EmbeddedThemeName::SolarizedLight,
    }
}

fn theme(name: two_face::theme::EmbeddedThemeName) -> &'static Theme {
    &THEME_SET[name]
}

fn hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightRequest {
    pub language: String,
    pub content: String,
    #[serde(default)]
    pub theme: Option<String>,
    /// 请求高亮的行范围（0 基、左闭右开）。
    #[serde(default)]
    pub line_range: Option<LineRange>,
    /// 文件标识：同一文件的多次分块请求复用解析状态。
    #[serde(default)]
    pub document_id: Option<String>,
    /// 内容变更时递增，用于丢弃过期的解析状态。
    #[serde(default)]
    pub generation: u64,
}

/// 行范围的线协议：`"0-500"` 表示 `[0, 500)`，`"200-"` 表示从 200 到文件末尾。
/// 用字符串而不是 JSON 对象，避免不同绑定层对可选数字的处理差异。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct LineRange(pub String);

impl LineRange {
    pub fn unlimited() -> Self {
        Self("0-".into())
    }

    pub fn new(from: usize, to: usize) -> Self {
        if to == usize::MAX {
            Self(format!("{from}-"))
        } else {
            Self(format!("{from}-{to}"))
        }
    }

    pub fn bounds(&self) -> (usize, usize) {
        let Some((from, to)) = self.0.split_once('-') else {
            return (0, 0);
        };
        let from = from.trim().parse().unwrap_or(0);
        let to = to.trim();
        if to.is_empty() {
            return (from, usize::MAX);
        }
        (from, to.parse().unwrap_or(usize::MAX))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightToken {
    pub text: String,
    pub color: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

pub type HighlightLine = Option<Vec<HighlightToken>>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightResult {
    /// 本次返回的第一行在文件中的下标。
    pub from: usize,
    /// 只包含请求范围内的行。
    pub lines: Vec<HighlightLine>,
    pub line_count: usize,
    pub syntax: String,
    pub theme: String,
    pub foreground: String,
    pub background: String,
    /// 本次调用耗时。
    pub elapsed_ms: u64,
    /// 从第 0 行解析到本次范围末尾的累计耗时（第一次请求最慢）。
    pub parse_ms: u64,
    /// 已经完成解析的行数（用于显示进度）。
    pub highlighted_through: usize,
    /// 文件是否需要完整解析一遍（分块请求里只有负责“探到底”的那次为 true）。
    pub need_full_scan: bool,
    /// 语言是否被识别；未识别时前端按纯文本渲染。
    pub supported: bool,
}

fn token(style: Style, text: &str) -> HighlightToken {
    HighlightToken {
        text: text.to_string(),
        color: hex(style.foreground),
        bold: style.font_style.contains(FontStyle::BOLD),
        italic: style.font_style.contains(FontStyle::ITALIC),
        underline: style.font_style.contains(FontStyle::UNDERLINE),
    }
}

/// 按扩展名解析语法；找不到时回退到纯文本（不报错，界面显示无高亮）。
fn syntax_for<'a>(set: &'a SyntaxSet, language: &str) -> (&'a SyntaxReference, bool) {
    let language = language.trim().trim_start_matches('.').to_ascii_lowercase();
    if !language.is_empty() && language != "txt" && language != "text" {
        if let Some(syntax) = set.find_syntax_by_extension(&language) {
            return (syntax, true);
        }
        if let Some(syntax) = set.find_syntax_by_name(&language) {
            return (syntax, true);
        }
    }
    (set.find_syntax_plain_text(), false)
}

/// syntect 可用的语法扩展名，用于报告和核对。
pub fn supported_extensions() -> Vec<String> {
    let mut names: Vec<String> = SYNTAX_SET
        .syntaxes()
        .iter()
        .flat_map(|syntax| syntax.file_extensions.iter().cloned())
        .collect();
    names.sort();
    names.dedup();
    names
}

pub fn syntax_name_for(language: &str) -> String {
    syntax_for(&SYNTAX_SET, language).0.name.clone()
}

/// 单个文件的增量解析状态：只保存解析器自己的状态，不保存已产出的 token。
struct Document {
    id: String,
    generation: u64,
    line_count: usize,
    highlighter: HighlightLines<'static>,
    /// 下一行待解析的索引。
    cursor: usize,
    syntax: String,
    theme: String,
    foreground: String,
    background: String,
    /// 首次解析到请求末尾的等待耗时。
    first_parse_ms: Option<u64>,
}

impl Document {
    /// 把解析推进到 `to`，返回 `[from, to)` 范围的 token。
    fn highlight_range(&mut self, lines: &[String], from: usize, to: usize) -> std::result::Result<Vec<HighlightLine>, String> {
        let to = to.min(self.line_count);
        let mut out: Vec<HighlightLine> = Vec::new();
        if from >= self.line_count {
            return Ok(out);
        }
        let started = Instant::now();
        // 往前推进解析状态；只有在请求范围内的行才产出 token。
        while self.cursor < to {
            let text = &lines[self.cursor];
            let ranges = self
                .highlighter
                .highlight_line(text, &SYNTAX_SET)
                .map_err(|error| format!("syntect 高亮失败：{error}"))?;
            if self.cursor >= from {
                let tokens: Vec<HighlightToken> = ranges
                    .iter()
                    .filter(|(_, piece)| !piece.is_empty())
                    .map(|(style, piece)| token(*style, piece))
                    .collect();
                out.push((!tokens.is_empty()).then_some(tokens));
            }
            self.cursor += 1;
        }
        if self.first_parse_ms.is_none() {
            self.first_parse_ms = Some(started.elapsed().as_millis() as u64);
        }
        Ok(out)
    }
}

/// 高亮工作线程：串行处理请求，独占 `HighlightLines`（它既不是 Send 也不是 Sync）。
pub struct Highlighter {
    sender: Sender<Job>,
    generation: std::sync::Arc<Mutex<u64>>,
}

struct Job {
    request: HighlightRequest,
    reply: Sender<Result<HighlightResult>>,
}

impl Highlighter {
    pub fn start() -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("revia-m0-highlight".into())
            .spawn(move || {
                let mut document: Option<Document> = None;
                while let Ok(job) = receiver.recv() {
                    let result = run_job(&mut document, job.request);
                    let _ = job.reply.send(result);
                }
            })
            .expect("启动高亮线程失败");
        Self { sender, generation: std::sync::Arc::new(Mutex::new(0)) }
    }

    /// 复制一份句柄：请求仍然由同一个线程串行处理，解析状态共享。
    pub fn clone_handle(&self) -> Self {
        Self { sender: self.sender.clone(), generation: self.generation.clone() }
    }

    /// 同步返回；调用方（Tauri 命令）会把它放到阻塞线程池里执行。
    pub fn highlight(&self, mut request: HighlightRequest) -> Result<HighlightResult> {
        if request.generation == 0 {
            let mut generation = self
                .generation
                .lock()
                .map_err(|_| AppError::internal("高亮状态锁失败"))?;
            *generation += 1;
            request.generation = *generation;
        }
        let (reply, receiver) = mpsc::channel();
        self.sender
            .send(Job { request, reply })
            .map_err(|_| AppError::internal("高亮线程已退出"))?;
        receiver
            .recv()
            .map_err(|_| AppError::internal("高亮线程没有返回结果"))?
    }
}

fn run_job(document: &mut Option<Document>, request: HighlightRequest) -> Result<HighlightResult> {
    let started = Instant::now();
    let name = theme_name(request.theme.as_deref());
    let theme = theme(name);
    let (syntax, matched) = syntax_for(&SYNTAX_SET, &request.language);
    let foreground = hex(theme.settings.foreground.unwrap_or(Color { r: 0x2a, g: 0x2a, b: 0x2a, a: 0xff }));
    let background = hex(theme.settings.background.unwrap_or(Color { r: 0xff, g: 0xff, b: 0xff, a: 0xff }));

    // 与旧实现保持一致的切行方式：末尾换行不额外产生一行。
    let lines: Vec<String> = request
        .content
        .split_inclusive('\n')
        .map(|line| line.trim_end_matches('\n').to_string())
        .collect();
    let line_count = lines.len();

    if !matched {
        return Ok(HighlightResult {
            from: 0,
            lines: Vec::new(),
            line_count,
            syntax: syntax.name.clone(),
            theme: name.as_name().to_string(),
            foreground,
            background,
            elapsed_ms: started.elapsed().as_millis() as u64,
            parse_ms: 0,
            highlighted_through: 0,
            need_full_scan: false,
            supported: false,
        });
    }

    let id = request
        .document_id
        .clone()
        .unwrap_or_else(|| format!("{}:{}", request.language, line_count));

    let reuse = document
        .as_ref()
        .map(|current| {
            current.id == id
                && current.generation == request.generation
                && current.line_count == line_count
                && current.syntax == syntax.name
        })
        .unwrap_or(false);
    if !reuse {
        *document = Some(Document {
            id: id.clone(),
            generation: request.generation,
            line_count,
            highlighter: HighlightLines::new(syntax, theme),
            cursor: 0,
            syntax: syntax.name.clone(),
            theme: name.as_name().to_string(),
            foreground: foreground.clone(),
            background: background.clone(),
            first_parse_ms: None,
        });
    }

    let current = document.as_mut().ok_or_else(|| AppError::internal("高亮状态丢失"))?;
    let scan_all = request
        .line_range
        .as_ref()
        .map(|range| range.0.trim_end().ends_with('-'))
        .unwrap_or(true);
    // open-ended 范围由 `highlight_range` clamp 到文件总行数。
    let (from, to) = request
        .line_range
        .as_ref()
        .map(|range| range.bounds())
        .unwrap_or((0, usize::MAX));
    let tokens = current
        .highlight_range(&lines, from, to)
        .map_err(AppError::internal)?;

    Ok(HighlightResult {
        from: from.min(current.line_count),
        lines: tokens,
        line_count: current.line_count,
        syntax: current.syntax.clone(),
        theme: current.theme.clone(),
        foreground: current.foreground.clone(),
        background: current.background.clone(),
        elapsed_ms: started.elapsed().as_millis() as u64,
        parse_ms: current.first_parse_ms.unwrap_or(0),
        highlighted_through: current.cursor,
        need_full_scan: scan_all,
        supported: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(language: &str, content: &str, range: Option<(usize, usize)>) -> HighlightRequest {
        HighlightRequest {
            language: language.into(),
            content: content.into(),
            theme: None,
            line_range: range.map(|(from, to)| LineRange::new(from, to)),
            document_id: Some("test-doc".into()),
            generation: 1,
        }
    }

    fn flat(result: &HighlightResult) -> String {
        result
            .lines
            .iter()
            .map(|line| {
                line.as_ref()
                    .map(|tokens| tokens.iter().map(|token| token.text.as_str()).collect::<String>())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn line_range_parses_both_forms() {
        assert_eq!(LineRange("0-500".into()).bounds(), (0, 500));
        assert_eq!(LineRange("200-".into()).bounds(), (200, usize::MAX));
        assert_eq!(LineRange::new(10, usize::MAX).0, "10-");
        assert_eq!(LineRange::new(10, 20).0, "10-20");
    }

    #[test]
    fn token_text_reassembles_the_source() {
        let source = "fn main() {\n    let x = 1;\n}\n";
        let highlighter = Highlighter::start();
        let result = highlighter.highlight(request("rs", source, None)).unwrap();
        assert_eq!(flat(&result), source.trim_end_matches('\n'));
        assert_eq!(result.lines.len(), 3);
        assert!(result.supported);
    }

    #[test]
    fn multi_line_comment_keeps_coloring_inside_the_comment() {
        // 评审要求：不能逐行孤立高亮，否则多行注释第二行会被当成代码。
        let source = "/* 注释开始\n这行仍在注释里\n注释结束 */\nlet x = 1;\n";
        let highlighter = Highlighter::start();
        let result = highlighter
            .highlight(request("js", source, None))
            .unwrap();
        let comment_color = result.lines[0].as_ref().unwrap()[0].color.clone();
        let second = result.lines[1].as_ref().unwrap();
        assert_eq!(second[0].color, comment_color, "多行注释第二行应与第一行同色");
        let code = result.lines[3].as_ref().unwrap();
        assert_ne!(code[0].color, comment_color, "注释结束后应恢复为代码颜色");
    }

    #[test]
    fn template_string_spans_lines_as_one_token_family() {
        let source = "const message = `第一行\n第二行 ${name}\n第三行`;\n";
        let highlighter = Highlighter::start();
        let result = highlighter.highlight(request("ts", source, None)).unwrap();
        let first = result.lines[0].as_ref().unwrap();
        let string_color = first
            .iter()
            .find(|token| token.text.contains("第一行"))
            .map(|token| token.color.clone())
            .expect("模板字符串内容的 token");
        let second = result.lines[1].as_ref().unwrap();
        assert!(
            second.iter().any(|token| token.color == string_color),
            "模板字符串第二行应包含字符串颜色；实际 {second:?}"
        );
    }

    #[test]
    fn chunked_requests_match_a_single_full_request() {
        // 分块请求（可见区域优先）与一次性全量解析必须得到相同的颜色。
        let source = "/* 注释\n仍注释 */\nconst a = `模板\n第二行`;\nlet b = 2;\n";
        let highlighter = Highlighter::start();
        let full = highlighter.highlight(request("js", source, None)).unwrap();
        let full_flat = flat(&full);

        let chunked = Highlighter::start();
        let first = chunked.highlight(request("js", source, Some((0, 2)))).unwrap();
        let second = chunked.highlight(request("js", source, Some((2, usize::MAX)))).unwrap();
        assert_eq!(first.from, 0);
        assert_eq!(second.from, 2);
        assert_eq!(format!("{}\n{}", flat(&first), flat(&second)), full_flat);
        assert_eq!(second.lines[0].as_ref().unwrap()[0].color, full.lines[2].as_ref().unwrap()[0].color);
    }

    #[test]
    fn new_generation_resets_the_cached_state() {
        let source = "let a = 1;\nlet b = 2;\n";
        let highlighter = Highlighter::start();
        let first = highlighter.highlight(request("ts", source, Some((0, 1)))).unwrap();
        assert_eq!(first.highlighted_through, 1);
        let mut second_request = request("ts", source, Some((0, 2)));
        second_request.generation = 99;
        let second = highlighter.highlight(second_request).unwrap();
        assert_eq!(second.highlighted_through, 2);
        assert_eq!(second.lines.len(), 2, "重新解析应覆盖请求的范围");
    }

    #[test]
    fn vue_single_file_component_is_recognized() {
        let source = "<template>\n  <div>{{ count }}</div>\n</template>\n\n<script setup lang=\"ts\">\nconst count = 1;\n</script>\n";
        let highlighter = Highlighter::start();
        let result = highlighter.highlight(request("vue", source, None)).unwrap();
        let colors: Vec<String> = result
            .lines
            .iter()
            .flatten()
            .flat_map(|tokens| tokens.iter().map(|token| token.color.clone()))
            .collect();
        let unique: std::collections::HashSet<&String> = colors.iter().collect();
        assert!(unique.len() >= 3, "Vue 组件应出现多种颜色，实际 {unique:?}");
    }

    #[test]
    fn swift_is_recognized() {
        let source = "import SwiftUI\n\nstruct ContentView: View {\n    var body: some View {\n        Text(\"你好\")\n    }\n}\n";
        let highlighter = Highlighter::start();
        let result = highlighter.highlight(request("swift", source, None)).unwrap();
        assert!(result.syntax.contains("Swift"), "语法名：{}", result.syntax);
        assert!(result.lines[0].is_some());
    }

    #[test]
    fn unknown_extension_falls_back_to_plain_text() {
        let highlighter = Highlighter::start();
        let result = highlighter.highlight(request("weird-ext", "some text\n", None)).unwrap();
        assert!(!result.supported);
        assert!(result.lines.is_empty(), "纯文本不产生 token");
        assert_eq!(result.syntax, "Plain Text");
    }

    #[test]
    #[ignore]
    fn bench_large_typescript() {
        let path = std::env::var("REVIA_M0_BENCH_FILE").expect("需要 REVIA_M0_BENCH_FILE");
        let content = std::fs::read_to_string(&path).expect("读取基准文件失败");
        let line_count = content.split_inclusive('\n').count();
        println!("文件 {path}：{line_count} 行");
        let highlighter = Highlighter::start();

        // 可见区域优先：前 400 行
        let started = Instant::now();
        let visible = highlighter
            .highlight(request("ts", &content, Some((0, 400))))
            .unwrap();
        println!(
            "可见区域 400 行：总 {} ms，其中解析 {} ms，token 行 {}",
            started.elapsed().as_millis(),
            visible.parse_ms,
            visible.lines.len()
        );

        // 剩余分块
        let mut from = 400;
        let mut total_ms = 0u128;
        let mut max_chunk = 0u128;
        while from < line_count {
            let to = (from + 2000).min(line_count);
            let started = Instant::now();
            let chunk = highlighter.highlight(request("ts", &content, Some((from, to)))).unwrap();
            let elapsed = started.elapsed().as_millis();
            total_ms += elapsed;
            max_chunk = max_chunk.max(elapsed);
            assert_eq!(chunk.from, from);
            from = to;
        }
        println!("剩余分块合计 {total_ms} ms，最慢一块 {max_chunk} ms");
        println!("逐行解析总耗时（parse_ms）：{} ms", highlighter.highlight(request("ts", &content, Some((0, 1)))).unwrap().parse_ms);
    }
}
