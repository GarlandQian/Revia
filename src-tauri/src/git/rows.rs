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

/// 按视图把 hunk 排成显示行。
pub fn layout(file: &FileChange, view: ViewMode) -> Vec<RenderRow> {
    let mut rows: Vec<RenderRow> = Vec::new();
    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
        let section = if hunk.section.is_empty() {
            hunk.header.clone()
        } else {
            hunk.section.clone()
        };
        rows.push(RenderRow {
            kind: RowKind::Hunk,
            hunk_index: hunk_index as u32,
            hunk_action: None,
            section: Some(section),
            old_line: Some(hunk.old_start),
            new_line: Some(hunk.new_start),
            line_kind: None,
            text: None,
            left: None,
            right: None,
        });

        match view {
            ViewMode::Unified => {
                for line in &hunk.lines {
                    rows.push(RenderRow {
                        kind: RowKind::Line,
                        hunk_index: hunk_index as u32,
                        hunk_action: None,
                        section: None,
                        old_line: line.old_line,
                        new_line: line.new_line,
                        line_kind: Some(line.kind),
                        text: Some(line.text.clone()),
                        left: None,
                        right: None,
                    });
                }
            }
            ViewMode::Split => {
                let mut cursor = 0;
                while cursor < hunk.lines.len() {
                    let line = &hunk.lines[cursor];
                    if line.kind != LineKind::Deletion && line.kind != LineKind::Addition {
                        rows.push(RenderRow {
                            kind: RowKind::Pair,
                            hunk_index: hunk_index as u32,
                            hunk_action: None,
                            section: None,
                            old_line: line.old_line,
                            new_line: line.new_line,
                            line_kind: None,
                            text: None,
                            left: line
                                .old_line
                                .map(|number| RowCell { line: number, kind: line.kind, text: line.text.clone() }),
                            right: line
                                .new_line
                                .map(|number| RowCell { line: number, kind: line.kind, text: line.text.clone() }),
                        });
                        cursor += 1;
                        continue;
                    }

                    let mut deletions = Vec::new();
                    while cursor < hunk.lines.len() && hunk.lines[cursor].kind == LineKind::Deletion {
                        deletions.push(cursor);
                        cursor += 1;
                    }
                    let mut additions = Vec::new();
                    while cursor < hunk.lines.len() && hunk.lines[cursor].kind == LineKind::Addition {
                        additions.push(cursor);
                        cursor += 1;
                    }

                    let count = deletions.len().max(additions.len());
                    for pair in 0..count {
                        let left_line = deletions.get(pair).map(|index| &hunk.lines[*index]);
                        let right_line = additions.get(pair).map(|index| &hunk.lines[*index]);
                        rows.push(RenderRow {
                            kind: RowKind::Pair,
                            hunk_index: hunk_index as u32,
                            hunk_action: None,
                            section: None,
                            old_line: left_line.and_then(|line| line.old_line),
                            new_line: right_line.and_then(|line| line.new_line),
                            line_kind: None,
                            text: None,
                            left: left_line.and_then(|line| {
                                line.old_line.map(|number| RowCell {
                                    line: number,
                                    kind: line.kind,
                                    text: line.text.clone(),
                                })
                            }),
                            right: right_line.and_then(|line| {
                                line.new_line.map(|number| RowCell {
                                    line: number,
                                    kind: line.kind,
                                    text: line.text.clone(),
                                })
                            }),
                        });
                    }
                }
            }
        }
    }
    rows
}

/// 每个 hunk 在两种视图里的行下标。
pub fn hunk_anchors(file: &FileChange) -> Vec<HunkAnchor> {
    let unified = layout(file, ViewMode::Unified);
    let split = layout(file, ViewMode::Split);
    let mut anchors = Vec::with_capacity(file.hunks.len());
    for (index, hunk) in file.hunks.iter().enumerate() {
        let index = index as u32;
        let row_unified = unified
            .iter()
            .position(|row| row.kind == RowKind::Hunk && row.hunk_index == index)
            .unwrap_or(0) as u32;
        let row_split = split
            .iter()
            .position(|row| row.kind == RowKind::Hunk && row.hunk_index == index)
            .unwrap_or(0) as u32;
        anchors.push(HunkAnchor {
            index,
            header: hunk.header.clone(),
            section: hunk.section.clone(),
            row_unified,
            row_split,
        });
    }
    anchors
}

/// 行号所属的显示行下标。`old` 为真时按旧版本行号查找，否则按新版本。
pub fn row_index_for_line(file: &FileChange, view: ViewMode, line: u32, old: bool) -> u32 {
    let rows = layout(file, view);
    for (index, row) in rows.iter().enumerate() {
        let candidate = match row.kind {
            RowKind::Hunk => None,
            RowKind::Line => {
                if old {
                    row.old_line
                } else {
                    row.new_line
                }
            }
            RowKind::Pair => {
                if old {
                    row.left.as_ref().map(|cell| cell.line)
                } else {
                    row.right.as_ref().map(|cell| cell.line)
                }
            }
        };
        if candidate == Some(line) {
            return index as u32;
        }
    }
    // 退化为就近取：找第一条行号大于等于目标的行。
    for (index, row) in rows.iter().enumerate() {
        let candidate = match row.kind {
            RowKind::Hunk => None,
            RowKind::Line => {
                if old {
                    row.old_line
                } else {
                    row.new_line
                }
            }
            RowKind::Pair => {
                if old {
                    row.left.as_ref().map(|cell| cell.line)
                } else {
                    row.right.as_ref().map(|cell| cell.line)
                }
            }
        };
        if candidate.is_some_and(|value| value >= line) {
            return index as u32;
        }
    }
    0
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
    fn hunk_anchors_point_at_header_rows() {
        let file = file();
        let anchors = hunk_anchors(&file);
        assert_eq!(anchors.len(), 2);
        let unified = layout(&file, ViewMode::Unified);
        for anchor in &anchors {
            assert_eq!(unified[anchor.row_unified as usize].kind, RowKind::Hunk);
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
}
