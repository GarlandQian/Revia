//! Rust 侧语法高亮：syntect 解析 + 语法类别（Q50）。
//!
//! 两条关键约束：
//! 1. **按语法类别返回**，不返回具体颜色：前端用 CSS 变量映射浅色 / 深色两套配色，
//!    切换深浅色不需要重新高亮（Q50）。
//! 2. **基于完整文件解析**：多行注释、模板字符串等跨行结构必须有完整上下文（ADR-0006）。
//!    解析状态按文件缓存，可以按行范围取结果，但解析永远从第 0 行开始累积。
//!
//! 性能：解析在专用线程里串行执行；为了支持来回跳转，解析状态按固定间隔打检查点，
//! 从最近的检查点继续，而不是每次都从头解析。

use std::collections::HashMap;
use std::time::Instant;

use serde::{Deserialize, Serialize, Serializer};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

use crate::error::{AppError, Result};

static SYNTAX_SET: std::sync::LazyLock<SyntaxSet> = std::sync::LazyLock::new(two_face::syntax::extra_newlines);

/// 每隔多少行打一个解析状态检查点（越大越省内存，跳转越慢）。
const CHECKPOINT_INTERVAL: usize = 512;

/// 语法类别。序列化为数字以压缩载荷；名称顺序由 `syntax_categories` 命令暴露给前端。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SyntaxCategory {
    Plain = 0,
    Comment = 1,
    String = 2,
    Number = 3,
    Constant = 4,
    Keyword = 5,
    Function = 6,
    Type = 7,
    Variable = 8,
    Parameter = 9,
    Tag = 10,
    Attribute = 11,
    Punctuation = 12,
    Operator = 13,
    Invalid = 14,
}

impl SyntaxCategory {
    pub fn name(self) -> &'static str {
        match self {
            SyntaxCategory::Plain => "plain",
            SyntaxCategory::Comment => "comment",
            SyntaxCategory::String => "string",
            SyntaxCategory::Number => "number",
            SyntaxCategory::Constant => "constant",
            SyntaxCategory::Keyword => "keyword",
            SyntaxCategory::Function => "function",
            SyntaxCategory::Type => "type",
            SyntaxCategory::Variable => "variable",
            SyntaxCategory::Parameter => "parameter",
            SyntaxCategory::Tag => "tag",
            SyntaxCategory::Attribute => "attribute",
            SyntaxCategory::Punctuation => "punctuation",
            SyntaxCategory::Operator => "operator",
            SyntaxCategory::Invalid => "invalid",
        }
    }

    /// 按索引顺序返回全部类别名，供前端建立 index → CSS 变量的映射。
    pub fn names() -> Vec<&'static str> {
        (0..=SyntaxCategory::Invalid as u8)
            .map(|index| SyntaxCategory::from_index(index).name())
            .collect()
    }

    fn from_index(index: u8) -> Self {
        match index {
            1 => SyntaxCategory::Comment,
            2 => SyntaxCategory::String,
            3 => SyntaxCategory::Number,
            4 => SyntaxCategory::Constant,
            5 => SyntaxCategory::Keyword,
            6 => SyntaxCategory::Function,
            7 => SyntaxCategory::Type,
            8 => SyntaxCategory::Variable,
            9 => SyntaxCategory::Parameter,
            10 => SyntaxCategory::Tag,
            11 => SyntaxCategory::Attribute,
            12 => SyntaxCategory::Punctuation,
            13 => SyntaxCategory::Operator,
            14 => SyntaxCategory::Invalid,
            _ => SyntaxCategory::Plain,
        }
    }
}

impl Serialize for SyntaxCategory {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

/// 一个 token：文本 + 类别（数字）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightToken {
    pub text: String,
    pub category: SyntaxCategory,
}

/// 一行的 token；None 表示这一行没有需要着色的内容。
pub type HighlightLine = Option<Vec<HighlightToken>>;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightRequest {
    /// 文件标识（worktree + 文件 + 改动分组 + 版本），用于复用解析状态。
    pub document_id: String,
    /// 内容代际；变化时丢弃缓存的解析状态。
    pub generation: u64,
    pub language: String,
    /// 请求的行范围（0 基、左闭右开）。
    pub from_line: usize,
    pub to_line: usize,
    /// 首次请求时携带完整内容；之后可以省略。
    #[serde(default)]
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightResult {
    pub from_line: usize,
    pub lines: Vec<HighlightLine>,
    pub kind: HighlightKind,
    /// 需要调用方补发内容（服务端没有这个文件的解析状态）。
    pub need_content: bool,
    pub elapsed_ms: u64,
    /// 本次为满足请求而解析的行数（诊断用）。
    pub parsed_lines: usize,
    pub total_lines: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HighlightKind {
    /// 已按语法着色。
    Highlighted,
    /// 语言无法识别（纯文本），前端按默认颜色渲染。
    PlainText,
}

fn category_from_scope_name(name: &str) -> Option<SyntaxCategory> {
    // 顺序即优先级：更具体的先判断。
    const RULES: &[(&str, SyntaxCategory)] = &[
        ("comment", SyntaxCategory::Comment),
        ("string", SyntaxCategory::String),
        ("constant.numeric", SyntaxCategory::Number),
        ("constant.character.escape", SyntaxCategory::Constant),
        ("constant.language", SyntaxCategory::Constant),
        ("constant.other", SyntaxCategory::Constant),
        ("keyword.operator", SyntaxCategory::Operator),
        ("keyword", SyntaxCategory::Keyword),
        ("storage", SyntaxCategory::Keyword),
        ("entity.name.function", SyntaxCategory::Function),
        ("support.function", SyntaxCategory::Function),
        ("entity.name.type", SyntaxCategory::Type),
        ("entity.name.class", SyntaxCategory::Type),
        ("entity.name.struct", SyntaxCategory::Type),
        ("entity.name.enum", SyntaxCategory::Type),
        ("entity.name.namespace", SyntaxCategory::Type),
        ("entity.other.inherited-class", SyntaxCategory::Type),
        ("support.type", SyntaxCategory::Type),
        ("support.class", SyntaxCategory::Type),
        ("entity.name.tag", SyntaxCategory::Tag),
        ("entity.other.attribute-name", SyntaxCategory::Attribute),
        ("variable.parameter", SyntaxCategory::Parameter),
        ("variable", SyntaxCategory::Variable),
        ("support.variable", SyntaxCategory::Variable),
        ("punctuation", SyntaxCategory::Punctuation),
        ("invalid", SyntaxCategory::Invalid),
        ("markup.heading", SyntaxCategory::Keyword),
        ("markup.bold", SyntaxCategory::Keyword),
        ("markup.italic", SyntaxCategory::Keyword),
        ("markup.raw", SyntaxCategory::String),
    ];
    RULES
        .iter()
        .find(|(prefix, _)| name.starts_with(prefix))
        .map(|(_, category)| *category)
}

/// 解析状态检查点。
struct Checkpoint {
    line: usize,
    parse_state: ParseState,
    scope_stack: ScopeStack,
}

struct Document {
    generation: u64,
    lines: Vec<String>,
    syntax: &'static syntect::parsing::SyntaxReference,
    supported: bool,
    parse_state: ParseState,
    scope_stack: ScopeStack,
    /// 已经解析到第几行（不含）。
    cursor: usize,
    checkpoints: Vec<Checkpoint>,
    /// scope 名字到类别的缓存，避免反复构造字符串。
    category_cache: HashMap<String, SyntaxCategory>,
}

impl Document {
    fn new(generation: u64, content: &str, language: &str) -> Self {
        let (syntax, supported) = syntax_for(language);
        Self {
            generation,
            lines: content
                .split_inclusive('\n')
                .map(|line| line.trim_end_matches('\n').to_string())
                .collect(),
            syntax,
            supported,
            parse_state: ParseState::new(syntax),
            scope_stack: ScopeStack::new(),
            cursor: 0,
            checkpoints: Vec::new(),
            category_cache: HashMap::new(),
        }
    }

    fn total_lines(&self) -> usize {
        self.lines.len()
    }

    /// 把解析推进到 `to`，返回 `[from, to)` 的 token。
    fn highlight_range(&mut self, from: usize, to: usize) -> Result<Vec<HighlightLine>> {
        let to = to.min(self.lines.len());
        if from >= self.lines.len() || !self.supported {
            return Ok(Vec::new());
        }
        if from < self.cursor {
            self.rewind_to(from);
        }
        let mut out = Vec::with_capacity(to.saturating_sub(from));
        while self.cursor < to {
            let line = self.lines[self.cursor].clone();
            let tokens = self.parse_line(&line)?;
            if self.cursor >= from {
                out.push(tokens);
            }
            self.cursor += 1;
            if self.cursor % CHECKPOINT_INTERVAL == 0 {
                self.checkpoints.push(Checkpoint {
                    line: self.cursor,
                    parse_state: self.parse_state.clone(),
                    scope_stack: self.scope_stack.clone(),
                });
            }
        }
        Ok(out)
    }

    /// 回到最近的检查点，从那里继续解析。
    fn rewind_to(&mut self, line: usize) {
        let checkpoint_index = self
            .checkpoints
            .iter()
            .rposition(|checkpoint| checkpoint.line <= line);
        match checkpoint_index {
            Some(index) => {
                let checkpoint = &self.checkpoints[index];
                self.parse_state = checkpoint.parse_state.clone();
                self.scope_stack = checkpoint.scope_stack.clone();
                self.cursor = checkpoint.line;
                self.checkpoints.truncate(index + 1);
            }
            None => {
                self.parse_state = ParseState::new(self.syntax);
                self.scope_stack = ScopeStack::new();
                self.cursor = 0;
                self.checkpoints.clear();
            }
        }
    }

    /// 解析一行，返回按类别切分的 token。
    fn parse_line(&mut self, line: &str) -> Result<HighlightLine> {
        let ops = self
            .parse_state
            .parse_line(line, &SYNTAX_SET)
            .map_err(|error| AppError::internal(format!("syntect 解析失败：{error}")))?;

        let mut tokens: Vec<HighlightToken> = Vec::new();
        let mut cursor = 0usize;
        let mut stack = std::mem::take(&mut self.scope_stack);

        for (offset, op) in &ops {
            let offset = (*offset).min(line.len());
            if offset > cursor {
                let category = classify(&stack, &mut self.category_cache);
                tokens.push(HighlightToken {
                    text: line[cursor..offset].to_string(),
                    category,
                });
                cursor = offset;
            }
            stack
                .apply(op)
                .map_err(|error| AppError::internal(format!("syntect scope 栈异常：{error}")))?;
        }
        if cursor < line.len() {
            let category = classify(&stack, &mut self.category_cache);
            tokens.push(HighlightToken {
                text: line[cursor..].to_string(),
                category,
            });
        }

        self.scope_stack = stack;
        Ok((!tokens.is_empty()).then_some(tokens))
    }
}

/// 从栈顶向下找第一个能识别的 scope。
fn classify(stack: &ScopeStack, cache: &mut HashMap<String, SyntaxCategory>) -> SyntaxCategory {
    for scope in stack.as_slice().iter().rev() {
        let key = scope.build_string();
        if let Some(category) = cache.get(&key) {
            if *category != SyntaxCategory::Plain {
                return *category;
            }
            continue;
        }
        let category = category_from_scope_name(&key).unwrap_or(SyntaxCategory::Plain);
        cache.insert(key, category);
        if category != SyntaxCategory::Plain {
            return category;
        }
    }
    SyntaxCategory::Plain
}

/// 按扩展名解析语法；找不到时回退到纯文本。
fn syntax_for(language: &str) -> (&'static syntect::parsing::SyntaxReference, bool) {
    let language = language.trim().trim_start_matches('.').to_ascii_lowercase();
    if !language.is_empty() && language != "txt" && language != "text" {
        if let Some(syntax) = SYNTAX_SET.find_syntax_by_extension(&language) {
            return (syntax, true);
        }
        if let Some(syntax) = SYNTAX_SET.find_syntax_by_name(&language) {
            return (syntax, true);
        }
    }
    (SYNTAX_SET.find_syntax_plain_text(), false)
}

/// syntect 可用的语法扩展名，用于核对（测试与报告）。
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

/// 高亮工作线程：串行处理请求，独占解析状态。
pub struct Highlighter {
    sender: std::sync::mpsc::Sender<Job>,
}

struct Job {
    request: HighlightRequest,
    reply: std::sync::mpsc::Sender<Result<HighlightResult>>,
}

impl Highlighter {
    pub fn start() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("revia-highlight".into())
            .spawn(move || {
                let mut documents: HashMap<String, Document> = HashMap::new();
                while let Ok(job) = receiver.recv() {
                    let result = run_job(&mut documents, job.request);
                    let _ = job.reply.send(result);
                }
            })
            .expect("启动高亮线程失败");
        Self { sender }
    }

    pub fn clone_handle(&self) -> Self {
        Self { sender: self.sender.clone() }
    }

    /// 同步返回；调用方（Tauri 命令）把它放到阻塞线程池执行。
    pub fn highlight(&self, request: HighlightRequest) -> Result<HighlightResult> {
        let (reply, receiver) = std::sync::mpsc::channel();
        self.sender
            .send(Job { request, reply })
            .map_err(|_| AppError::internal("高亮线程已退出"))?;
        receiver
            .recv()
            .map_err(|_| AppError::internal("高亮线程没有返回结果"))?
    }
}

fn run_job(documents: &mut HashMap<String, Document>, request: HighlightRequest) -> Result<HighlightResult> {
    let started = Instant::now();
    let known = documents
        .get(&request.document_id)
        .is_some_and(|document| document.generation == request.generation);

    if !known {
        let Some(content) = request.content.as_deref() else {
            return Ok(HighlightResult {
                from_line: request.from_line,
                lines: Vec::new(),
                kind: HighlightKind::PlainText,
                need_content: true,
                elapsed_ms: started.elapsed().as_millis() as u64,
                parsed_lines: 0,
                total_lines: 0,
            });
        };
        documents.insert(
            request.document_id.clone(),
            Document::new(request.generation, content, &request.language),
        );
    }

    let document = documents
        .get_mut(&request.document_id)
        .ok_or_else(|| AppError::internal("高亮状态丢失"))?;
    let before = document.cursor;
    let lines = document.highlight_range(request.from_line, request.to_line)?;
    let kind = if document.supported {
        HighlightKind::Highlighted
    } else {
        HighlightKind::PlainText
    };

    Ok(HighlightResult {
        from_line: request.from_line.min(document.total_lines()),
        lines,
        kind,
        need_content: false,
        elapsed_ms: started.elapsed().as_millis() as u64,
        parsed_lines: document.cursor.saturating_sub(before),
        total_lines: document.total_lines(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(content: &str, language: &str, from: usize, to: usize) -> HighlightRequest {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        HighlightRequest {
            // document_id 必须能区分内容：否则换了内容但 ID 相同会错误复用解析状态。
            document_id: format!("{language}-{:x}", hasher.finish()),
            generation: 1,
            language: language.into(),
            from_line: from,
            to_line: to,
            content: Some(content.into()),
        }
    }

    fn categories(result: &HighlightResult) -> Vec<Vec<SyntaxCategory>> {
        result
            .lines
            .iter()
            .map(|line| {
                line.as_ref()
                    .map(|tokens| tokens.iter().map(|token| token.category).collect())
                    .unwrap_or_default()
            })
            .collect()
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
    fn categories_are_exported_in_index_order() {
        let names = SyntaxCategory::names();
        assert_eq!(names[0], "plain");
        assert_eq!(names[1], "comment");
        assert_eq!(names.len(), SyntaxCategory::Invalid as usize + 1);
    }

    #[test]
    fn comments_keywords_and_strings_get_distinct_categories() {
        let highlighter = Highlighter::start();
        let source = "// 注释\nconst name = \"值\";\nlet n = 42;\n";
        let result = highlighter.highlight(request(source, "ts", 0, 3)).unwrap();
        let lines = categories(&result);
        assert!(lines[0].contains(&SyntaxCategory::Comment), "{:?}", lines[0]);
        assert!(lines[1].contains(&SyntaxCategory::Keyword), "{:?}", lines[1]);
        assert!(lines[1].contains(&SyntaxCategory::String), "{:?}", lines[1]);
        assert!(lines[2].contains(&SyntaxCategory::Number), "{:?}", lines[2]);
    }

    #[test]
    fn multi_line_comment_stays_comment() {
        let highlighter = Highlighter::start();
        let source = "/* 开始\n仍在注释里\n结束 */\nconst a = 1;\n";
        let result = highlighter.highlight(request(source, "js", 0, 4)).unwrap();
        let lines = categories(&result);
        assert!(lines[1].contains(&SyntaxCategory::Comment), "第二行仍是注释：{:?}", lines[1]);
        assert!(!lines[3].is_empty());
        assert!(lines[3].contains(&SyntaxCategory::Keyword), "注释结束后恢复代码类别：{:?}", lines[3]);
    }

    #[test]
    fn template_string_spans_lines() {
        let highlighter = Highlighter::start();
        let source = "const message = `第一行\n第二行 ${name}\n第三行`;\n";
        let result = highlighter.highlight(request(source, "ts", 0, 3)).unwrap();
        let lines = categories(&result);
        assert!(lines[0].contains(&SyntaxCategory::String), "{:?}", lines[0]);
        assert!(
            lines[1].contains(&SyntaxCategory::String),
            "模板字符串第二行仍是字符串：{:?}",
            lines[1]
        );
    }

    #[test]
    fn chunked_requests_match_one_full_request() {
        let source = "/* 注释\n仍注释 */\nconst a = `模板\n第二行`;\nlet b = 2;\n";
        let highlighter = Highlighter::start();
        let full = highlighter.highlight(request(source, "js", 0, 5)).unwrap();
        let full_flat = flat(&full);

        let chunked = Highlighter::start();
        let first = chunked
            .highlight(HighlightRequest { document_id: "chunk".into(), ..request(source, "js", 0, 2) })
            .unwrap();
        let second = chunked
            .highlight(HighlightRequest { document_id: "chunk".into(), ..request(source, "js", 2, 5) })
            .unwrap();
        assert_eq!(format!("{}\n{}", flat(&first), flat(&second)), full_flat);
    }

    #[test]
    fn backwards_jump_uses_checkpoints_and_matches_full_parse() {
        // 造一个超过一个检查点的文件，验证回跳后结果一致。
        let mut source = String::new();
        for index in 0..(CHECKPOINT_INTERVAL + 40) {
            source.push_str(&format!("const value{index} = {index};\n"));
        }
        let highlighter = Highlighter::start();
        let tail = highlighter
            .highlight(request(&source, "ts", CHECKPOINT_INTERVAL + 10, CHECKPOINT_INTERVAL + 12))
            .unwrap();
        let head = highlighter
            .highlight(request(&source, "ts", 3, 5))
            .unwrap();
        let fresh = Highlighter::start();
        let expected = fresh.highlight(request(&source, "ts", 3, 5)).unwrap();
        assert_eq!(flat(&head), flat(&expected), "回跳后应与从头解析一致");
        assert_eq!(categories(&head), categories(&expected));
        assert!(!categories(&tail).is_empty());
    }

    #[test]
    fn without_content_reports_need_content() {
        let highlighter = Highlighter::start();
        let result = highlighter
            .highlight(HighlightRequest {
                content: None,
                ..request("let a = 1;\n", "ts", 0, 1)
            })
            .unwrap();
        assert!(result.need_content);
    }

    #[test]
    fn unknown_language_is_plain_text() {
        let highlighter = Highlighter::start();
        let result = highlighter.highlight(request("some text\n", "weird-ext", 0, 1)).unwrap();
        assert_eq!(result.kind, HighlightKind::PlainText);
        assert!(result.lines.is_empty());
    }

    #[test]
    fn vue_and_swift_are_recognized() {
        let highlighter = Highlighter::start();
        let vue = "<template>\n  <div>{{ count }}</div>\n</template>\n";
        let result = highlighter.highlight(request(vue, "vue", 0, 3)).unwrap();
        assert_eq!(result.kind, HighlightKind::Highlighted);
        assert!(categories(&result)[1].contains(&SyntaxCategory::Tag));

        let swift = "import Foundation\n\nstruct A: B {\n    let value: Int\n}\n";
        let result = highlighter.highlight(request(swift, "swift", 0, 5)).unwrap();
        assert_eq!(result.kind, HighlightKind::Highlighted);
        assert!(categories(&result)[0].contains(&SyntaxCategory::Keyword));
    }

    #[test]
    fn supported_extensions_cover_our_languages() {
        let extensions = supported_extensions();
        for language in ["ts", "vue", "swift", "json", "yaml", "md", "rs", "py"] {
            assert!(extensions.contains(&language.to_string()), "缺少语法：{language}");
        }
    }
}
