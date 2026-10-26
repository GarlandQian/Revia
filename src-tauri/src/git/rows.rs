//! 显示行索引：把 Git 给出的 hunk 排成界面上的行。
//!
//! 这里只做排版，不重新计算差异：
//! - 统一视图：一行一个 diff 行，加上 hunk 头；
//! - 并排视图：把相邻的删除块与新增块按行配对，缺哪边就留空。
//! hunk 的划分始终来自 Git（ADR-0004）。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::git::diff::{FileChange, LineKind};
use crate::git::status::ChangesKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ViewMode {
    Split,
    Unified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RowKind {
    /// hunk 头（`@@ ... @@`）。
    Hunk,
    /// 统一视图的内容行。
    Line,
    /// 并排视图的一对（可能只有一侧）。
    Pair,
}

/// 并排视图里的一格。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RowCell {
    /// 该格在对应版本里的行号（1 基）。
    pub line: u32,
    pub kind: LineKind,
    pub text: String,
}

/// 与显示 hunk 同批签发的操作目标；前端只能原样使用，不能按序号重建。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HunkAction {
    pub worktree_id: String,
    pub path: String,
    pub changes: ChangesKind,
    pub fingerprint: String,
    pub snapshot_revision: String,
    pub hunk_id: String,
}

/// 界面上的一个显示行。统一视图用 `lineKind`/`text`，并排视图用 `left`/`right`。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RenderRow {
    pub kind: RowKind,
    pub hunk_index: u32,
    pub hunk_action: Option<HunkAction>,
    /// hunk 头显示的内容。
    pub section: Option<String>,
    /// 统一视图：旧行号 / 新行号；并排视图：配对两侧的行号。
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub line_kind: Option<LineKind>,
    pub text: Option<String>,
    pub left: Option<RowCell>,
    pub right: Option<RowCell>,
    /// 该行属于“先删后增”的修改块：同一块里既有删除行又有新增行。界面据此把修改与纯新增、纯删除分开着色。
    pub modified: bool,
}

/// hunk 在某个视图里的位置，供 F7 / ⇧F7 跳转使用。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HunkAnchor {
    pub index: u32,
    pub header: String,
    pub section: String,
    /// 统一视图里的行下标。
    pub row_unified: u32,
    /// 并排视图里的行下标。
    pub row_split: u32,
}

/// 缓存只保存源行位置，文本和操作目标仍由当前快照持有。
#[derive(Clone, Copy)]
enum LayoutRow {
    Hunk { hunk_index: usize },
    Line { hunk_index: usize, line: usize, modified: bool },
    Pair { hunk_index: usize, left: Option<usize>, right: Option<usize>, modified: bool },
}

impl LayoutRow {
    fn hunk_index(self) -> usize {
        match self {
            Self::Hunk { hunk_index }
            | Self::Line { hunk_index, .. }
            | Self::Pair { hunk_index, .. } => hunk_index,
        }
    }

    fn line_number(self, file: &FileChange, old: bool) -> Option<u32> {
        let hunk = &file.hunks[self.hunk_index()];
        let index = match self {
            Self::Hunk { .. } => return None,
            Self::Line { line, .. } => line,
            Self::Pair { left, right, .. } => (if old { left } else { right })?,
        };
        let line = &hunk.lines[index];
        if old { line.old_line } else { line.new_line }
    }

    fn render(self, file: &FileChange) -> RenderRow {
        let hunk_index = self.hunk_index();
        let hunk = &file.hunks[hunk_index];
        let mut row = RenderRow {
            kind: RowKind::Hunk,
            hunk_index: hunk_index as u32,
            hunk_action: None,
            section: None,
            old_line: None,
            new_line: None,
            line_kind: None,
            text: None,
            left: None,
            right: None,
            modified: false,
        };
        match self {
            Self::Hunk { .. } => {
                let section = if hunk.section.is_empty() { &hunk.header } else { &hunk.section };
                row.section = Some(section.clone());
                row.old_line = Some(hunk.old_start);
                row.new_line = Some(hunk.new_start);
            }
            Self::Line { line, modified, .. } => {
                let line = &hunk.lines[line];
                row.kind = RowKind::Line;
                row.old_line = line.old_line;
                row.new_line = line.new_line;
                row.line_kind = Some(line.kind);
                row.text = Some(line.text.clone());
                row.modified = modified;
            }
            Self::Pair { left, right, modified, .. } => {
                let cell = |index: Option<usize>, old: bool| {
                    let line = &hunk.lines[index?];
                    let number = (if old { line.old_line } else { line.new_line })?;
                    Some(RowCell { line: number, kind: line.kind, text: line.text.clone() })
                };
                row.kind = RowKind::Pair;
                row.left = cell(left, true);
                row.right = cell(right, false);
                row.old_line = row.left.as_ref().map(|cell| cell.line);
                row.new_line = row.right.as_ref().map(|cell| cell.line);
                row.modified = modified;
            }
        }
        row
    }
}

/// 只遍历 Git hunk 的配对关系；计数、定位和索引构造共用，不复制文本。
fn visit_layout(file: &FileChange, view: ViewMode, mut visit: impl FnMut(LayoutRow)) {
    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
        visit(LayoutRow::Hunk { hunk_index });
        let mut cursor = 0;
        while cursor < hunk.lines.len() {
            let line = &hunk.lines[cursor];
            if !matches!(line.kind, LineKind::Deletion | LineKind::Addition) {
                visit(match view {
                    ViewMode::Unified => LayoutRow::Line { hunk_index, line: cursor, modified: false },
                    ViewMode::Split => LayoutRow::Pair {
                        hunk_index,
                        left: line.old_line.map(|_| cursor),
                        right: line.new_line.map(|_| cursor),
                        modified: false,
                    },
                });
                cursor += 1;
                continue;
            }

            let start = cursor;
            while cursor < hunk.lines.len() && hunk.lines[cursor].kind == LineKind::Deletion {
                cursor += 1;
            }
            let additions_start = cursor;
            while cursor < hunk.lines.len() && hunk.lines[cursor].kind == LineKind::Addition {
                cursor += 1;
            }
            let deleted = additions_start - start;
            let added = cursor - additions_start;
            let modified = deleted > 0 && added > 0;
            match view {
                ViewMode::Unified => {
                    for line in start..cursor {
                        visit(LayoutRow::Line { hunk_index, line, modified });
                    }
                }
                ViewMode::Split => {
                    for offset in 0..deleted.max(added) {
                        visit(LayoutRow::Pair {
                            hunk_index,
                            left: (offset < deleted).then_some(start + offset),
                            right: (offset < added).then_some(additions_start + offset),
                            modified,
                        });
                    }
                }
            }
        }
    }
}

/// 仅供所属 FileChange 快照使用的排版索引，不拥有整份 RenderRow/文本副本。
pub(crate) struct RowLayout {
    rows: Vec<LayoutRow>,
}

impl RowLayout {
    pub(crate) fn new(file: &FileChange, view: ViewMode) -> Self {
        let mut rows = Vec::with_capacity(row_count(file, view));
        visit_layout(file, view, |row| rows.push(row));
        Self { rows }
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn render_range(&self, file: &FileChange, range: std::ops::Range<usize>) -> Vec<RenderRow> {
        self.rows[range].iter().map(|row| row.render(file)).collect()
    }
}

/// 按视图生成完整显示行；常规取行只通过 RowLayout 生成请求范围。
pub fn layout(file: &FileChange, view: ViewMode) -> Vec<RenderRow> {
    let layout = RowLayout::new(file, view);
    layout.render_range(file, 0..layout.len())
}

pub fn row_count(file: &FileChange, view: ViewMode) -> usize {
    let mut count = 0;
    visit_layout(file, view, |_| count += 1);
    count
}

/// 每个 hunk 在两种视图里的行下标。
pub fn hunk_anchors(file: &FileChange) -> Vec<HunkAnchor> {
    let mut anchors: Vec<HunkAnchor> = file.hunks.iter().enumerate().map(|(index, hunk)| HunkAnchor {
        index: index as u32,
        header: hunk.header.clone(),
        section: hunk.section.clone(),
        row_unified: 0,
        row_split: 0,
    }).collect();
    for view in [ViewMode::Unified, ViewMode::Split] {
        let mut index = 0;
        visit_layout(file, view, |row| {
            if let LayoutRow::Hunk { hunk_index } = row {
                match view {
                    ViewMode::Unified => anchors[hunk_index].row_unified = index,
                    ViewMode::Split => anchors[hunk_index].row_split = index,
                }
            }
            index += 1;
        });
    }
    anchors
}

/// 行号所属的显示行下标。先精确匹配，再找第一条 >= 目标的行；均不构造文本行。
pub fn row_index_for_line(file: &FileChange, view: ViewMode, line: u32, old: bool) -> u32 {
    let mut index = 0;
    let mut exact = None;
    let mut nearest = None;
    visit_layout(file, view, |row| {
        if let Some(candidate) = row.line_number(file, old) {
            if candidate == line && exact.is_none() {
                exact = Some(index);
            }
            if candidate >= line && nearest.is_none() {
                nearest = Some(index);
            }
        }
        index += 1;
    });
    exact.or(nearest).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::diff;

    const PATCH: &str = "\
diff --git a/src/app.ts b/src/app.ts
--- a/src/app.ts
+++ b/src/app.ts
@@ -1,4 +1,5 @@ 函数
 const a = 1;
-const b = 2;
+const b = 3;
+const c = 4;
 
@@ -10,2 +11,2 @@
 tail();
-removed();
+added();
";

    fn file() -> FileChange {
        diff::parse(PATCH).files.into_iter().next().unwrap()
    }

    #[test]
    fn unified_layout_matches_diff_lines_plus_headers() {
        let rows = layout(&file(), ViewMode::Unified);
        // hunk 头 2 行 + 内容行 5 + 3
        assert_eq!(rows.len(), 10);
        assert_eq!(rows[0].kind, RowKind::Hunk);
        assert_eq!(rows[0].section.as_deref(), Some("函数"));
        assert_eq!(rows[1].kind, RowKind::Line);
        assert_eq!(rows[1].text.as_deref(), Some("const a = 1;"));
        assert_eq!(rows[2].line_kind, Some(LineKind::Deletion));
        assert_eq!(rows[2].old_line, Some(2));
        assert_eq!(rows[2].new_line, None);
    }

    #[test]
    fn split_layout_pairs_deletions_with_additions() {
        let rows = layout(&file(), ViewMode::Split);
        // 第一段：1 行上下文 + 1 删除配 2 新增（2 行）+ 1 行上下文 = 5 行；加上 hunk 头 2 行
        let first = rows
            .iter()
            .find(|row| row.left.as_ref().is_some_and(|cell| cell.text == "const b = 2;"))
            .expect("找到删除行");
        assert_eq!(first.kind, RowKind::Pair);
        assert_eq!(first.left.as_ref().unwrap().line, 2);
        assert_eq!(first.right.as_ref().unwrap().text, "const b = 3;");
        // 第二条新增行左侧为空
        let second = rows
            .iter()
            .find(|row| row.right.as_ref().is_some_and(|cell| cell.text == "const c = 4;"))
            .expect("找到第二条新增行");
        assert!(second.left.is_none(), "没有配对的删除行时左侧为空");
    }

    #[test]
    fn modified_marks_only_blocks_with_both_kinds() {
        let patch = "\
diff --git a/x.ts b/x.ts
--- a/x.ts
+++ b/x.ts
@@ -1,5 +1,5 @@
 keep();
-old();
+new();
+extra();
 keep();
-gone();
@@ -20,1 +20,2 @@
 keep();
+only();
";
        let file = diff::parse(patch).files.into_iter().next().unwrap();
        let unified = layout(&file, ViewMode::Unified);
        let flag = |text: &str| unified.iter().find(|row| row.text.as_deref() == Some(text)).unwrap().modified;
        assert!(flag("old();") && flag("new();") && flag("extra();"), "先删后增的整块都算修改");
        assert!(!flag("keep();"), "上下文行不算修改");
        assert!(!flag("gone();"), "纯删除不算修改");
        assert!(!flag("only();"), "纯新增不算修改");
        assert!(unified.iter().filter(|row| row.kind == RowKind::Hunk).all(|row| !row.modified));

        let split = layout(&file, ViewMode::Split);
        let pair = |text: &str| {
            split
                .iter()
                .find(|row| [&row.left, &row.right].into_iter().flatten().any(|cell| cell.text == text))
                .unwrap()
                .modified
        };
        assert!(pair("old();") && pair("extra();"), "并排视图与统一视图对同一块给出相同判断");
        assert!(!pair("gone();") && !pair("only();"));
    }

    #[test]
    fn hunk_anchors_point_at_header_rows() {
        let file = file();
        let anchors = hunk_anchors(&file);
        assert_eq!(anchors.len(), 2);
        assert_eq!((anchors[0].row_unified, anchors[0].row_split), (0, 0));
        assert_eq!((anchors[1].row_unified, anchors[1].row_split), (6, 5));
        for (view, expected_count) in [(ViewMode::Unified, 10), (ViewMode::Split, 8)] {
            assert_eq!(row_count(&file, view), expected_count);
            let rows = layout(&file, view);
            assert_eq!(rows.len(), expected_count);
            for anchor in &anchors {
                let index = match view {
                    ViewMode::Unified => anchor.row_unified,
                    ViewMode::Split => anchor.row_split,
                };
                assert_eq!(rows[index as usize].kind, RowKind::Hunk);
                assert_eq!(rows[index as usize].hunk_index, anchor.index);
            }
        }
    }

    #[test]
    fn row_index_for_line_finds_the_row() {
        let file = file();
        let rows = layout(&file, ViewMode::Unified);
        let index = row_index_for_line(&file, ViewMode::Unified, 3, false);
        assert_eq!(rows[index as usize].line_kind, Some(LineKind::Addition));
        assert_eq!(rows[index as usize].new_line, Some(3));
        // 旧行号 3 是空上下文行
        let context = row_index_for_line(&file, ViewMode::Unified, 3, true);
        assert_eq!(rows[context as usize].line_kind, Some(LineKind::Context));
        for (view, second_content) in [(ViewMode::Unified, 7), (ViewMode::Split, 6)] {
            // 两个 hunk 之间的行号退化到下一条内容行，不选 hunk 头。
            assert_eq!(row_index_for_line(&file, view, 4, true), second_content);
            assert_eq!(row_index_for_line(&file, view, 5, false), second_content);
            assert_eq!(row_index_for_line(&file, view, 10, true), second_content);
            assert_eq!(row_index_for_line(&file, view, 11, false), second_content);
            assert_eq!(row_index_for_line(&file, view, 100, true), 0);
            assert_eq!(row_index_for_line(&file, view, 100, false), 0);
        }
    }

    #[test]
    fn split_uses_left_for_old_and_right_for_new() {
        let rows = layout(&file(), ViewMode::Split);
        let first_cell = rows
            .iter()
            .find_map(|row| row.left.as_ref())
            .expect("并排视图应有左侧内容");
        assert_eq!(first_cell.line, 1);
        let right_cell = rows
            .iter()
            .find_map(|row| row.right.as_ref())
            .expect("并排视图应有右侧内容");
        assert_eq!(right_cell.line, 1);
    }

    #[test]
    fn compact_layout_renders_requested_range_across_hunks() {
        let file = file();
        for (view, range) in [(ViewMode::Unified, 4..8), (ViewMode::Split, 3..7)] {
            let index = RowLayout::new(&file, view);
            let rows = index.render_range(&file, range);
            assert_eq!(rows.len(), 4);
            assert_eq!(rows.iter().map(|row| row.hunk_index).collect::<Vec<_>>(), [0, 0, 1, 1]);
            assert_eq!(rows[0].new_line, Some(3));
            assert!(rows[0].modified);
            match view {
                ViewMode::Unified => assert_eq!(rows[0].text.as_deref(), Some("const c = 4;")),
                ViewMode::Split => {
                    assert!(rows[0].left.is_none());
                    assert_eq!(rows[0].right.as_ref().unwrap().text, "const c = 4;");
                }
            }
            assert_eq!((rows[1].old_line, rows[1].new_line), (Some(3), Some(4)));
            assert_eq!(rows[2].kind, RowKind::Hunk);
            assert_eq!(rows[2].section.as_deref(), Some("@@ -10,2 +11,2 @@"));
            assert_eq!((rows[3].old_line, rows[3].new_line), (Some(10), Some(11)));
            assert!(rows.iter().all(|row| row.hunk_action.is_none()));
            assert!(index.render_range(&file, index.len()..index.len()).is_empty());
        }
    }

    #[test]
    fn no_eol_markers_keep_row_positions_and_separate_change_blocks() {
        let patch = "\
diff --git a/x.txt b/x.txt
--- a/x.txt
+++ b/x.txt
@@ -1 +1 @@
-old
\\ No newline at end of file
+new
\\ No newline at end of file
";
        let file = diff::parse(patch).files.into_iter().next().unwrap();
        for view in [ViewMode::Unified, ViewMode::Split] {
            let index = RowLayout::new(&file, view);
            assert_eq!(row_count(&file, view), 5);
            assert_eq!(index.len(), 5);
            let rows = index.render_range(&file, 1..5);
            assert!(rows.iter().all(|row| !row.modified));
            assert_eq!((rows[0].old_line, rows[0].new_line), (Some(1), None));
            assert_eq!((rows[2].old_line, rows[2].new_line), (None, Some(1)));
            for marker in [&rows[1], &rows[3]] {
                assert_eq!((marker.old_line, marker.new_line), (None, None));
                match view {
                    ViewMode::Unified => {
                        assert_eq!(marker.line_kind, Some(LineKind::NoEol));
                        assert_eq!(marker.text.as_deref(), Some("No newline at end of file"));
                    }
                    ViewMode::Split => {
                        assert!(marker.left.is_none() && marker.right.is_none());
                    }
                }
            }
            assert_eq!(row_index_for_line(&file, view, 1, true), 1);
            assert_eq!(row_index_for_line(&file, view, 1, false), 3);
        }
    }

    #[test]
    fn empty_diff_has_no_layout_or_anchors() {
        let mut file = file();
        file.hunks.clear();
        assert!(hunk_anchors(&file).is_empty());
        for view in [ViewMode::Unified, ViewMode::Split] {
            let index = RowLayout::new(&file, view);
            assert_eq!(row_count(&file, view), 0);
            assert_eq!(index.len(), 0);
            assert!(index.render_range(&file, 0..0).is_empty());
            assert_eq!(row_index_for_line(&file, view, 1, true), 0);
            assert_eq!(row_index_for_line(&file, view, 1, false), 0);
        }
    }
}
