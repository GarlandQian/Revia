//! 左右对照（Q138）与按段操作（Q142）的数据模型。
//!
//! 界面上的“块”是一段连续的改动（中间至少隔一行未改动行）；一个 Git hunk
//! 可以包含几段。段来自同一次 `git diff` 的解析结果（ADR-0004），按段写的
//! 补丁由 Rust 从缓存的同一个 hunk 改写得到（同 `git add -p` 拆块），
//! 前端原样传回段目标，不自己拼补丁。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::git::diff::{FileChange, Hunk, LineKind};
use crate::git::status::ChangesKind;

/// 一段连续的改动（0 基，左闭右开；行号是**文件全文**的行号）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangeSegment {
    /// 所属 Git hunk 在文件改动列表里的下标。
    pub hunk_index: u32,
    /// 段在 hunk 内的序号（按出现顺序，0 基）。
    pub segment_index: u32,
    /// 旧版本（左侧）行区间；纯新增时 start == end。
    pub left_start: u32,
    pub left_end: u32,
    /// 新版本（右侧）行区间；纯删除时 start == end。
    pub right_start: u32,
    pub right_end: u32,
    pub kind: SegmentKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SegmentKind {
    Added,
    Deleted,
    Modified,
}

/// 左右对照里一栏的行。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SideLine {
    /// 文件全文里的行号（1 基）。
    pub line: u32,
    pub text: String,
    /// 这一行是否属于某个改动段。
    pub changed: bool,
    /// 改动字词的区间（UTF-16 码元，0 基），按词高亮（Q139）。
    pub words: Vec<(u32, u32)>,
}

/// 段的操作目标（Q142）：与显示同批签发；前端原样传回，不自己拼补丁。
///
/// `credential` 把文件指纹和签发时的忽略空白档位绑在一起（Q140“指纹包含所用档位”）：
/// 段号只在那一档的 diff 里有意义，写入时档位对不上就拒绝。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SegmentAction {
    pub worktree_id: String,
    pub path: String,
    pub changes: ChangesKind,
    /// 段所属 Git hunk 的下标。
    pub hunk_index: u32,
    /// 段在 hunk 内的序号。
    pub segment_index: u32,
    /// 文件指纹（与改动列表条目相同，界面判断确认是否过期用）。
    pub fingerprint: String,
    /// 签发时的忽略空白档位。
    pub ignore_whitespace: IgnoreWhitespace,
    /// 写入凭证：文件指纹 + 档位，前端不解析。
    pub credential: String,
    pub snapshot_revision: String,
}

/// 生成段的写入凭证。
pub fn segment_credential(file_fingerprint: &str, ignore: IgnoreWhitespace) -> String {
    format!("{file_fingerprint}/ws={}", ignore.as_str())
}

/// 核对写入凭证：档位与请求一致时返回其中的文件指纹。
pub fn credential_fingerprint(credential: &str, ignore: IgnoreWhitespace) -> Option<&str> {
    let (fingerprint, level) = credential.rsplit_once("/ws=")?;
    (level == ignore.as_str()).then_some(fingerprint)
}

/// 折叠区（未改动且超过 2×上下文行数的区间，两侧行号各一份）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FoldRange {
    pub left_start: u32,
    pub left_end: u32,
    pub right_start: u32,
    pub right_end: u32,
}

/// 忽略空白档位（Q140）。前两档可按段操作；同时忽略空行只用于查看。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum IgnoreWhitespace {
    #[default]
    Off,
    /// `--ignore-space-at-eol`。
    Eol,
    /// `-w`。
    All,
    /// `-w` + `--ignore-blank-lines`；只用于查看，块按钮变灰。
    AllAndBlankLines,
}

impl IgnoreWhitespace {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Eol => "eol",
            Self::All => "all",
            Self::AllAndBlankLines => "allAndBlankLines",
        }
    }

    /// 追加到 `git diff` / `git log -p` 之外的 diff 参数。
    pub fn diff_args(&self) -> &'static [&'static str] {
        match self {
            Self::Off => &[],
            Self::Eol => &["--ignore-space-at-eol"],
            Self::All | Self::AllAndBlankLines => &["-w"],
        }
    }

    /// 是否额外忽略空行（只影响 diff 结果，不影响应用）。
    pub fn ignore_blank_lines(&self) -> bool {
        matches!(self, Self::AllAndBlankLines)
    }

    /// 该档位下能否按块（段）操作：同时忽略空行的一档只用于查看（Q140）。
    pub fn allows_segment_actions(&self) -> bool {
        !matches!(self, Self::AllAndBlankLines)
    }
}

/// 每一行属于 hunk 内的第几段（上下文行为 `None`）。
///
/// 连续的删除、新增行是一段，上下文行结束一段；`\ No newline at end of file`
/// 跟着上一行走，不结束段（设计 3.1）。统一视图的块按钮、左右对照的段、
/// 按段切开补丁都用这一张表编号，保证三处一致。
pub fn line_owners(hunk: &Hunk) -> Vec<Option<u32>> {
    let mut owners = Vec::with_capacity(hunk.lines.len());
    let mut next = 0u32;
    let mut open: Option<u32> = None;
    for line in &hunk.lines {
        let owner = match line.kind {
            LineKind::Deletion | LineKind::Addition => {
                let id = *open.get_or_insert_with(|| {
                    next += 1;
                    next - 1
                });
                Some(id)
            }
            LineKind::NoEol => owners.last().copied().flatten(),
            LineKind::Context => {
                open = None;
                None
            }
        };
        owners.push(owner);
    }
    owners
}

/// 把 hunk 里的行分成段，返回每段在**文件全文**里的两侧行区间（0 基，左闭右开）。
pub fn segments_of_hunk(hunk: &Hunk, hunk_index: u32) -> Vec<ChangeSegment> {
    let owners = line_owners(hunk);
    let mut segments: Vec<ChangeSegment> = Vec::new();
    // 两侧的当前位置（0 基）。纯新增 hunk 的旧侧起点按 Git 惯例是“在第 N 行之后”。
    let mut old_line = if hunk.old_count == 0 { hunk.old_start } else { hunk.old_start.saturating_sub(1) };
    let mut new_line = if hunk.new_count == 0 { hunk.new_start } else { hunk.new_start.saturating_sub(1) };
    for (line, owner) in hunk.lines.iter().zip(&owners) {
        if let Some(id) = owner {
            if segments.last().map(|segment| segment.segment_index) != Some(*id) {
                segments.push(ChangeSegment {
                    hunk_index,
                    segment_index: *id,
                    left_start: old_line,
                    left_end: old_line,
                    right_start: new_line,
                    right_end: new_line,
                    kind: SegmentKind::Modified,
                });
            }
        }
        match line.kind {
            LineKind::Deletion => old_line += 1,
            LineKind::Addition => new_line += 1,
            LineKind::Context => {
                old_line += 1;
                new_line += 1;
            }
            LineKind::NoEol => {}
        }
        if let (Some(_), Some(segment)) = (owner, segments.last_mut()) {
            segment.left_end = old_line;
            segment.right_end = new_line;
        }
    }
    for segment in &mut segments {
        segment.kind = if segment.left_start == segment.left_end {
            SegmentKind::Added
        } else if segment.right_start == segment.right_end {
            SegmentKind::Deleted
        } else {
            SegmentKind::Modified
        };
    }
    segments
}

/// 两侧全文是否就是这份 diff 的两端：每个删除行在左侧、每个新增行在右侧的
/// 对应行号上（行尾 `\r` 不计）。全文与 diff 分两次读取，中间文件被改过就对不上。
pub fn texts_match_diff(file: &FileChange, left_text: &str, right_text: &str) -> bool {
    let left: Vec<&str> = display_lines(left_text).collect();
    let right: Vec<&str> = display_lines(right_text).collect();
    fn at<'a>(lines: &[&'a str], number: Option<u32>) -> Option<&'a str> {
        lines.get(number?.checked_sub(1)? as usize).copied()
    }
    file.hunks.iter().flat_map(|hunk| &hunk.lines).all(|line| match line.kind {
        LineKind::Deletion => at(&left, line.old_line) == Some(line.text.as_str()),
        LineKind::Addition => at(&right, line.new_line) == Some(line.text.as_str()),
        _ => true,
    })
}

/// 按行切开全文，去掉每行行尾的 `\r`（显示与比对都不带换行符）。
pub fn display_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line))
}

/// 文件全文的行数（末尾换行不算多一行；空文本为 0）。
pub fn text_line_count(text: &str) -> u32 {
    text.lines().count() as u32
}

/// 段的词级区间（返修 A1/Q138）：把段内两侧的改动行逐行配对，返回
/// **文件全文行号**（1 基）→ 改动字词的 UTF-16 区间。
///
/// `left_lines` / `right_lines` 是两侧全文的行切片（0 基下标），
/// 段的行区间是 0 基左闭右开。
pub fn segment_word_ranges(
    left_lines: &[&str],
    right_lines: &[&str],
    segment: &ChangeSegment,
    granularity: WordGranularity,
) -> (
    std::collections::HashMap<u32, Vec<(u32, u32)>>,
    std::collections::HashMap<u32, Vec<(u32, u32)>>,
) {
    let left_slice = slice_lines(left_lines, segment.left_start, segment.left_end);
    let right_slice = slice_lines(right_lines, segment.right_start, segment.right_end);
    let (left_map, right_map) = word_ranges_with_granularity(&left_slice, &right_slice, granularity);
    let shift = |map: std::collections::HashMap<u32, Vec<(u32, u32)>>, start: u32| {
        map.into_iter()
            .map(|(index, ranges)| (index + start + 1, ranges))
            .collect::<std::collections::HashMap<u32, Vec<(u32, u32)>>>()
    };
    (
        shift(left_map, segment.left_start),
        shift(right_map, segment.right_start),
    )
}

/// 取出 `[start, end)` 范围的行（0 基，越界时夹紧）。
fn slice_lines<'a>(lines: &'a [&'a str], start: u32, end: u32) -> Vec<&'a str> {
    let from = (start as usize).min(lines.len());
    let to = (end as usize).min(lines.len()).max(from);
    lines[from..to].to_vec()
}

/// 从全文行列表里取一段行，标出属于改动段的行并给出词级区间。
pub fn side_lines_from_text(
    lines: &[&str],
    from: u32,
    to: u32,
    changed_lines: &std::collections::HashSet<u32>,
    word_ranges: &std::collections::HashMap<u32, Vec<(u32, u32)>>,
) -> Vec<SideLine> {
    let mut result = Vec::with_capacity((to.saturating_sub(from)).min(lines.len() as u32) as usize);
    for line_no in from..=to {
        let index = (line_no - 1) as usize;
        if index >= lines.len() {
            break;
        }
        result.push(SideLine {
            line: line_no,
            text: lines[index].to_string(),
            changed: changed_lines.contains(&line_no),
            words: word_ranges.get(&line_no).cloned().unwrap_or_default(),
        });
    }
    result
}

/// 计算折叠区（Q139）：两段改动之间未改动的部分，前后各留上下文行数，
/// 中间至少还剩一行才折叠。文件开头与结尾只在改动一侧留上下文。
pub fn compute_folds(
    file: &FileChange,
    left_count: u32,
    right_count: u32,
    context_lines: u32,
) -> Vec<FoldRange> {
    let mut all_segments: Vec<ChangeSegment> = Vec::new();
    for (index, hunk) in file.hunks.iter().enumerate() {
        all_segments.extend(segments_of_hunk(hunk, index as u32));
    }
    compute_folds_from_segments(&all_segments, left_count, right_count, context_lines)
}

/// 复用已按同一 Git diff 算出的段，避免布局缓存再次解析段边界。
pub(crate) fn compute_folds_from_segments(
    segments: &[ChangeSegment],
    left_count: u32,
    right_count: u32,
    context_lines: u32,
) -> Vec<FoldRange> {
    if segments.is_empty() {
        return Vec::new();
    }
    let mut folds = Vec::new();
    let (mut old_pos, mut new_pos) = (0u32, 0u32);
    let mut lead = 0u32; // 文件开头不需要前置上下文
    for segment in segments {
        let left_end = segment.left_start.saturating_sub(context_lines);
        let right_end = segment.right_start.saturating_sub(context_lines);
        let left_start = old_pos + lead;
        let right_start = new_pos + lead;
        if left_end > left_start && right_end > right_start {
            folds.push(FoldRange { left_start, left_end, right_start, right_end });
        }
        old_pos = segment.left_end;
        new_pos = segment.right_end;
        lead = context_lines;
    }
    let (left_start, right_start) = (old_pos + context_lines, new_pos + context_lines);
    if left_count > left_start && right_count > right_start {
        folds.push(FoldRange { left_start, left_end: left_count, right_start, right_end: right_count });
    }
    folds
}

/// 差异高亮粒度（Q139：按词、按行、按字符、不高亮）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordGranularity {
    /// 按词（默认）。
    Word,
    /// 按字符。
    Char,
    /// 按行、不高亮：只标整行，不返回词级区间。
    Line,
}

impl WordGranularity {
    /// 界面上的取值（`settings::DiffHighlightSetting` 的字符串形式）。
    pub fn from_setting(value: &str) -> Self {
        match value {
            "char" => Self::Char,
            "line" | "none" => Self::Line,
            _ => Self::Word,
        }
    }
}

/// 词级区间：按词（字母数字串、单个标点、空白串）分词后对两侧做 LCS，
/// 返回两侧每行里改动字词的 UTF-16 区间（0 基，行号是段内下标）。
pub fn word_ranges_for_segment(
    left_lines: &[&str],
    right_lines: &[&str],
) -> (std::collections::HashMap<u32, Vec<(u32, u32)>>, std::collections::HashMap<u32, Vec<(u32, u32)>>) {
    word_ranges_with_granularity(left_lines, right_lines, WordGranularity::Word)
}

/// 同上，可指定粒度；按行 / 不高亮时不返回区间。
pub fn word_ranges_with_granularity(
    left_lines: &[&str],
    right_lines: &[&str],
    granularity: WordGranularity,
) -> (std::collections::HashMap<u32, Vec<(u32, u32)>>, std::collections::HashMap<u32, Vec<(u32, u32)>>) {
    let mut left_map = std::collections::HashMap::new();
    let mut right_map = std::collections::HashMap::new();
    if granularity == WordGranularity::Line {
        return (left_map, right_map);
    }
    if left_lines.len() > 500 || right_lines.len() > 500 {
        return (left_map, right_map);
    }
    let pairs = left_lines.len().min(right_lines.len());
    for index in 0..pairs {
        let left = left_lines[index];
        let right = right_lines[index];
        if left.bytes().len() > 2000 || right.bytes().len() > 2000 {
            continue;
        }
        let (left_words, right_words) = match granularity {
            WordGranularity::Word => (tokenize(left), tokenize(right)),
            WordGranularity::Char => (tokenize_chars(left), tokenize_chars(right)),
            WordGranularity::Line => continue,
        };
        let (l_changed, r_changed) = lcs_word_diff(&left_words, &right_words);
        if !l_changed.is_empty() {
            left_map.insert(index as u32, l_changed);
        }
        if !r_changed.is_empty() {
            right_map.insert(index as u32, r_changed);
        }
    }
    (left_map, right_map)
}

/// 按字符分词：每个字符一个词（按字符高亮用）。
fn tokenize_chars(text: &str) -> Vec<&str> {
    let mut tokens = Vec::with_capacity(text.len());
    let mut index = 0;
    for ch in text.chars() {
        tokens.push(&text[index..index + ch.len_utf8()]);
        index += ch.len_utf8();
    }
    tokens
}

/// 按词分词：连续字母数字为一词，单个非字母数字字符（含空白）各一词。
fn tokenize(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let bytes = text;
    let mut start = 0;
    let mut in_word = false;
    for (index, ch) in bytes.char_indices() {
        let is_word_char = ch.is_alphanumeric() || ch == '_';
        if is_word_char != in_word && index > start {
            tokens.push(&bytes[start..index]);
            start = index;
        }
        in_word = is_word_char;
        if !is_word_char {
            tokens.push(&bytes[index..index + ch.len_utf8()]);
            start = index + ch.len_utf8();
            in_word = false;
        }
    }
    if start < bytes.len() {
        tokens.push(&bytes[start..]);
    }
    tokens
}

/// 词级 LCS：返回两侧各自“改动了”的词的 UTF-16 区间。
fn lcs_word_diff(
    left: &[&str],
    right: &[&str],
) -> (Vec<(u32, u32)>, Vec<(u32, u32)>) {
    let n = left.len();
    let m = right.len();
    // DP 表：lcs[i][j] = left[i..] 与 right[j..] 的 LCS 长度。
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if left[i] == right[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    // 回溯找出未匹配（= 改动）的词。
    let mut left_changed = Vec::new();
    let mut right_changed = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    let utf16_len = |token: &str| token.encode_utf16().count() as u32;
    // 先算每词的起始偏移。
    let left_offsets: Vec<u32> = {
        let mut offsets = Vec::with_capacity(n + 1);
        let mut acc = 0u32;
        for token in left {
            offsets.push(acc);
            acc += utf16_len(token);
        }
        offsets.push(acc);
        offsets
    };
    let right_offsets: Vec<u32> = {
        let mut offsets = Vec::with_capacity(m + 1);
        let mut acc = 0u32;
        for token in right {
            offsets.push(acc);
            acc += utf16_len(token);
        }
        offsets.push(acc);
        offsets
    };
    let mut run_start_left: Option<usize> = None;
    let mut run_start_right: Option<usize> = None;
    let flush_runs = |left_changed: &mut Vec<(u32, u32)>,
                      right_changed: &mut Vec<(u32, u32)>,
                      run_start_left: &mut Option<usize>,
                      run_start_right: &mut Option<usize>,
                      i: usize,
                      j: usize| {
        if let Some(start) = run_start_left.take() {
            left_changed.push((left_offsets[start], left_offsets[i]));
        }
        if let Some(start) = run_start_right.take() {
            right_changed.push((right_offsets[start], right_offsets[j]));
        }
    };
    while i < n && j < m {
        if left[i] == right[j] {
            flush_runs(&mut left_changed, &mut right_changed, &mut run_start_left, &mut run_start_right, i, j);
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            if run_start_left.is_none() {
                run_start_left = Some(i);
            }
            i += 1;
        } else {
            if run_start_right.is_none() {
                run_start_right = Some(j);
            }
            j += 1;
        }
    }
    flush_runs(&mut left_changed, &mut right_changed, &mut run_start_left, &mut run_start_right, n, m);
    (left_changed, right_changed)
}

/// 按段切开 Git hunk（Q142，同 `git add -p` 拆块）：从 Git 输出的原始字节改写出
/// 只改这一段的补丁，不用显示文本重建（ADR-0004）。
///
/// - 暂存（`forward`）：目标段原样；其他段的删除行改成上下文，新增行去掉；
/// - 取消暂存、丢弃（反向应用）：目标段原样；其他段的新增行改成上下文，删除行去掉。
///
/// `\ No newline at end of file` 跟着它所属的那一行：那一行留下它就留下。
/// 每行原样保留行尾（含 CRLF），只改第一个字节的前缀。上下文保持严格，
/// 不用 `--unidiff-zero`。忽略空白档位下由 `git apply --ignore-whitespace`
/// 匹配上下文，Git 会保留目标文件里原有的空白。
pub fn rewrite_hunk_for_segment(
    file: &FileChange,
    hunk_index: usize,
    segment_index: u32,
    forward: bool,
) -> Option<Vec<u8>> {
    let hunk = file.hunks.get(hunk_index)?;
    let header = file.raw_patch.get(..file.hunks.first()?.raw_range.start)?;
    let body = file.raw_patch.get(hunk.raw_range.clone())?;
    let mut raw_lines = body.split_inclusive(|byte| *byte == b'\n');
    if !raw_lines.next()?.starts_with(b"@@ ") {
        return None;
    }
    let raw_lines: Vec<&[u8]> = raw_lines.collect();
    let owners = line_owners(hunk);
    // 原始行与解析结果一一对应，前缀也必须一致；对不上就不切（拒绝写入）。
    if raw_lines.len() != owners.len() || !owners.contains(&Some(segment_index)) {
        return None;
    }
    let mut out_lines: Vec<u8> = Vec::with_capacity(body.len());
    let (mut old_count, mut new_count) = (0u32, 0u32);
    let mut kept_previous = false;
    for ((raw, line), owner) in raw_lines.iter().zip(&hunk.lines).zip(&owners) {
        let expected = match line.kind {
            LineKind::Context => b' ',
            LineKind::Deletion => b'-',
            LineKind::Addition => b'+',
            LineKind::NoEol => b'\\',
        };
        if raw.first() != Some(&expected) {
            return None;
        }
        let target = *owner == Some(segment_index);
        let prefix = match line.kind {
            LineKind::NoEol => {
                if kept_previous {
                    out_lines.extend_from_slice(raw);
                }
                continue;
            }
            LineKind::Context => b' ',
            LineKind::Deletion if target => b'-',
            LineKind::Addition if target => b'+',
            LineKind::Deletion if forward => b' ',
            LineKind::Addition if !forward => b' ',
            LineKind::Deletion | LineKind::Addition => {
                kept_previous = false;
                continue;
            }
        };
        match prefix {
            b'-' => old_count += 1,
            b'+' => new_count += 1,
            _ => {
                old_count += 1;
                new_count += 1;
            }
        }
        out_lines.push(prefix);
        out_lines.extend_from_slice(&raw[1..]);
        kept_previous = true;
    }
    // 起点：保留下来完整一侧的起点照搬原 hunk（正向是旧侧，反向是新侧），
    // 另一侧按单 hunk 补丁的惯例推出（一侧为空时写“在第 N 行之后”）。
    let derive = |kept_start: u32, kept_count: u32, other_count: u32| -> u32 {
        if kept_count == 0 {
            kept_start + 1
        } else if other_count == 0 {
            kept_start.saturating_sub(1)
        } else {
            kept_start
        }
    };
    let (old_start, new_start) = if forward {
        (hunk.old_start, derive(hunk.old_start, old_count, new_count))
    } else {
        (derive(hunk.new_start, new_count, old_count), hunk.new_start)
    };
    let mut out = Vec::with_capacity(header.len() + out_lines.len() + 64);
    out.extend_from_slice(header);
    // section 为空时不能留尾随空格（git apply 对头行敏感）。
    let header_line = if hunk.section.is_empty() {
        format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@\n")
    } else {
        format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@ {}\n", hunk.section)
    };
    out.extend_from_slice(header_line.as_bytes());
    out.extend_from_slice(&out_lines);
    Some(out)
}

/// 一段改动的内容（删除行、新增行），写后核对用。
pub type SegmentSignature = (Vec<String>, Vec<String>);

/// 文件里全部段的内容，排好序（写后核对按多重集比较）。
pub fn segment_signatures(file: &FileChange) -> Vec<SegmentSignature> {
    let mut signatures = Vec::new();
    for hunk in &file.hunks {
        let owners = line_owners(hunk);
        let mut current: Option<(u32, SegmentSignature)> = None;
        for (line, owner) in hunk.lines.iter().zip(&owners) {
            let Some(id) = owner else { continue };
            if current.as_ref().map(|(open, _)| open) != Some(id) {
                signatures.extend(current.take().map(|(_, signature)| signature));
                current = Some((*id, (Vec::new(), Vec::new())));
            }
            if let Some((_, (deleted, added))) = current.as_mut() {
                match line.kind {
                    LineKind::Deletion => deleted.push(line.text.clone()),
                    LineKind::Addition => added.push(line.text.clone()),
                    _ => {}
                }
            }
        }
        signatures.extend(current.map(|(_, signature)| signature));
    }
    signatures.sort();
    signatures
}

/// 写入一段之后，来源一侧应剩下的段：去掉目标段，其余不变。
pub fn signatures_without(file: &FileChange, hunk_index: usize, segment_index: u32) -> Option<Vec<SegmentSignature>> {
    let hunk = file.hunks.get(hunk_index)?;
    let owners = line_owners(hunk);
    let mut target: SegmentSignature = (Vec::new(), Vec::new());
    for (line, owner) in hunk.lines.iter().zip(&owners) {
        if *owner != Some(segment_index) {
            continue;
        }
        match line.kind {
            LineKind::Deletion => target.0.push(line.text.clone()),
            LineKind::Addition => target.1.push(line.text.clone()),
            _ => {}
        }
    }
    let mut remaining = segment_signatures(file);
    let position = remaining.iter().position(|signature| *signature == target)?;
    remaining.remove(position);
    Some(remaining)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::diff;

    const MULTI_SEGMENT: &str = "\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,7 +1,7 @@ section
 line1
-deleted A
+added A
 context
 context
-deleted B
+added B
 line7
";

    fn file() -> FileChange {
        diff::parse(MULTI_SEGMENT).files.into_iter().next().unwrap()
    }

    #[test]
    fn hunk_with_two_changes_yields_two_segments() {
        let segments = segments_of_hunk(&file().hunks[0], 0);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].kind, SegmentKind::Modified);
        // 文件全文行号（0 基左闭右开）：文件第一行是 line1（行号 0）。
        assert_eq!((segments[0].left_start, segments[0].left_end), (1, 2));
        assert_eq!((segments[0].right_start, segments[0].right_end), (1, 2));
        // 第二段：hunk 头 @@ -1,7 +1,7 @@ 从第 1 行起，两行上下文后是第 4 行（0 基）。
        assert_eq!((segments[1].left_start, segments[1].left_end), (4, 5));
        assert_eq!((segments[1].right_start, segments[1].right_end), (4, 5));
    }

    #[test]
    fn rewrite_keeps_only_target_segment_forward() {
        let file = file();
        // 暂存第二段（added B）：第一段删除行变上下文、新增行去掉。
        let patch = rewrite_hunk_for_segment(&file, 0, 1, true).unwrap();
        let text = String::from_utf8(patch).unwrap();
        assert!(text.contains(" context\n context\n-deleted B\n+added B\n"));
        assert!(!text.contains("-deleted A"));
        assert!(!text.contains("+added A"));
        assert!(text.contains(" line1"));
    }

    #[test]
    fn rewrite_keeps_only_target_segment_reverse() {
        let file = file();
        // 丢弃第二段（反向）：其他段的新增行变上下文、删除行去掉；目标段原样。
        let patch = rewrite_hunk_for_segment(&file, 0, 1, false).unwrap();
        let text = String::from_utf8(patch).unwrap();
        // 其他段（A）：新增行 added A 变上下文，删除行 deleted A 去掉。
        assert!(text.contains(" added A\n"));
        assert!(!text.contains("-deleted A"));
        // 目标段（B）原样保留。
        assert!(text.contains("-deleted B"));
        assert!(text.contains("+added B"));
    }

    #[test]
    fn word_ranges_mark_changed_words_only() {
        let (left, right) = word_ranges_for_segment(&["const count = ref(0);"], &["const count = ref(1);"]);
        // 两侧都标出改动的字词：'0' 与 '1' 各自一个区间，起点一致（同一位置）。
        let lr = left.get(&0).expect("left side should mark the changed word");
        let rr = right.get(&0).expect("right side should mark the changed word");
        assert_eq!(lr.len(), 1);
        assert_eq!(rr.len(), 1);
        assert_eq!(lr[0], rr[0]);
        // 区间长度是 1 个字符（'0'）。
        assert_eq!(lr[0].1 - lr[0].0, 1);
    }

    const MULTI_LINE: &str = "\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,6 +1,5 @@
 keep
-one
-two
-three
+ONE
+TWO
 keep2
 tail
";

    #[test]
    fn multi_line_segment_covers_every_line() {
        let file = diff::parse(MULTI_LINE).files.into_iter().next().unwrap();
        let segments = segments_of_hunk(&file.hunks[0], 0);
        assert_eq!(segments.len(), 1);
        assert_eq!((segments[0].left_start, segments[0].left_end), (1, 4));
        assert_eq!((segments[0].right_start, segments[0].right_end), (1, 3));
    }

    const NO_EOL: &str = "\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,3 +1,3 @@
-first
+FIRST
 middle
-last
\\ No newline at end of file
+LAST
\\ No newline at end of file
";

    #[test]
    fn no_eol_marker_stays_with_its_line() {
        let file = diff::parse(NO_EOL).files.into_iter().next().unwrap();
        let segments = segments_of_hunk(&file.hunks[0], 0);
        assert_eq!(segments.len(), 2, "last 一处修改不能被标记行拆成两段");
        assert_eq!((segments[1].left_start, segments[1].left_end), (2, 3));
        // 暂存第一段：last 改成上下文，并带着它的无换行标记；LAST 与其标记去掉。
        let forward = String::from_utf8(rewrite_hunk_for_segment(&file, 0, 0, true).unwrap()).unwrap();
        assert!(forward.contains("@@ -1,3 +1,3 @@\n-first\n+FIRST\n middle\n last\n\\ No newline at end of file\n"), "{forward}");
        assert!(!forward.contains("LAST"));
        // 丢弃第二段：first 去掉、FIRST 改上下文；目标段两行各带标记。
        let reverse = String::from_utf8(rewrite_hunk_for_segment(&file, 0, 1, false).unwrap()).unwrap();
        assert!(reverse.contains("@@ -1,3 +1,3 @@\n FIRST\n middle\n-last\n\\ No newline at end of file\n+LAST\n\\ No newline at end of file\n"), "{reverse}");
    }

    #[test]
    fn crlf_line_endings_are_copied_from_git_output() {
        let patch = "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1,4 +1,4 @@\n-a\r\n+A\r\n mid\n-b\n+B\n";
        let file = diff::parse(patch).files.into_iter().next().unwrap();
        let out = String::from_utf8(rewrite_hunk_for_segment(&file, 0, 0, true).unwrap()).unwrap();
        // 混合换行：每行原样保留自己的行尾。
        assert!(out.ends_with("-a\r\n+A\r\n mid\n b\n"), "{out:?}");
    }

    #[test]
    fn folds_never_hide_changed_lines() {
        let mut text = String::from("diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -8,26 +8,6 @@\n");
        for line in 8..11 { text.push_str(&format!(" l{line}\n")); }
        for line in 11..31 { text.push_str(&format!("-l{line}\n")); }
        for line in 31..34 { text.push_str(&format!(" l{line}\n")); }
        let file = diff::parse(&text).files.into_iter().next().unwrap();
        let folds = compute_folds(&file, 40, 20, 3);
        let segment = &segments_of_hunk(&file.hunks[0], 0)[0];
        assert_eq!((segment.left_start, segment.left_end), (10, 30));
        for fold in &folds {
            assert!(fold.left_end <= segment.left_start || fold.left_start >= segment.left_end, "{fold:?}");
            // 未改动区两侧长度相等。
            assert_eq!(fold.left_end - fold.left_start, fold.right_end - fold.right_start, "{fold:?}");
        }
        assert_eq!(folds.len(), 2);
    }

    #[test]
    fn precomputed_folds_preserve_context_and_file_edges() {
        let file = diff::parse(MULTI_LINE).files.into_iter().next().unwrap();
        let segments = segments_of_hunk(&file.hunks[0], 0);
        let ranges = |folds: Vec<FoldRange>| folds.into_iter()
            .map(|fold| (fold.left_start, fold.left_end, fold.right_start, fold.right_end))
            .collect::<Vec<_>>();
        for (context, expected) in [
            (0, vec![(0, 1, 0, 1), (4, 6, 3, 5)]),
            (1, vec![(5, 6, 4, 5)]),
            (3, vec![]),
        ] {
            let folds = ranges(compute_folds_from_segments(&segments, 6, 5, context));
            assert_eq!(folds, expected);
            assert_eq!(folds, ranges(compute_folds(&file, 6, 5, context)));
        }
        assert!(compute_folds_from_segments(&[], 6, 5, 0).is_empty());
    }

    #[test]
    fn credential_binds_the_whitespace_level() {
        let credential = segment_credential("abc", IgnoreWhitespace::All);
        assert_eq!(credential_fingerprint(&credential, IgnoreWhitespace::All), Some("abc"));
        assert_eq!(credential_fingerprint(&credential, IgnoreWhitespace::Off), None);
        assert_eq!(credential_fingerprint("abc", IgnoreWhitespace::Off), None);
    }
}
