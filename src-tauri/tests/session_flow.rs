//! 集成测试：在 fixture 仓库上跑通「登记 → 打开会话 → 读取状态 → 取行 → 高亮」。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use revia_lib::git::highlight::HighlightRequest;
use revia_lib::git::rows::ViewMode;
use revia_lib::git::status::ChangesKind;
use revia_lib::git::CommandLog;
use revia_lib::session::{FileVersion, WorktreeSession};

fn fixture() -> Option<PathBuf> {
    let path = std::env::var("REVIA_M1_FIXTURE").unwrap_or_else(|_| "/tmp/revia-m1-fixtures/diffs".into());
    let path = PathBuf::from(path);
    path.is_dir().then_some(path)
}

#[test]
fn open_session_and_read_rows() {
    let Some(path) = fixture() else {
        eprintln!("跳过：没有 fixture 仓库");
        return;
    };
    let home = revia_lib::shell::home_dir().unwrap();
    let started = std::time::Instant::now();
    let env = revia_lib::shell::resolve(&home, &path);
    println!(
        "登录 shell 解析：{} ms（回退 {}）",
        env.info.elapsed_ms, env.info.used_fallback
    );
    let session = WorktreeSession::open("test", &path, env, Arc::new(CommandLog::default()))
        .expect("打开会话失败");
    println!("git：{}（{}）", session.info.git_version, session.info.git_path);
    println!("会话打开：{} ms", started.elapsed().as_millis());

    let started = std::time::Instant::now();
    let snapshot = session.refresh().expect("读取状态失败");
    println!(
        "list_changes：{} ms，{} 个文件（未暂存 {} / 已暂存 {} / 未跟踪 {}）",
        started.elapsed().as_millis(),
        snapshot.entries.len(),
        snapshot.unstaged_count,
        snapshot.staged_count,
        snapshot.untracked_count
    );
    assert!(snapshot.entries.iter().any(|entry| entry.path == "src/generated.ts"));

    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == "src/generated.ts")
        .expect("应有 generated.ts");
    println!(
        "generated.ts：split {} 行 / unified {} 行 / hunk {}",
        entry.row_count_split, entry.row_count_unified, entry.hunk_count
    );

    let started = std::time::Instant::now();
    let slice = session
        .rows("src/generated.ts", ChangesKind::Unstaged, ViewMode::Split, 0, 500)
        .expect("取行失败");
    println!(
        "取 500 行：{} ms（共 {} 行，返回 {} 行）",
        started.elapsed().as_millis(),
        slice.row_count,
        slice.rows.len()
    );
    assert_eq!(slice.rows.len(), 500);

    let started = std::time::Instant::now();
    let highlight = session
        .highlight("src/generated.ts", ChangesKind::Unstaged, FileVersion::New, 1, 400)
        .expect("高亮失败");
    println!(
        "高亮 400 行：{} ms（引擎 {} ms，解析 {} 行）",
        started.elapsed().as_millis(),
        highlight.elapsed_ms,
        highlight.parsed_lines
    );
    assert_eq!(highlight.lines.len(), 400);

    // 分块与回跳
    let _ = session
        .highlight("src/generated.ts", ChangesKind::Unstaged, FileVersion::New, 2000, 2400)
        .expect("第二块失败");
    let started = std::time::Instant::now();
    let back = session
        .highlight("src/generated.ts", ChangesKind::Unstaged, FileVersion::New, 100, 200)
        .expect("回跳失败");
    println!("回跳高亮 100 行：{} ms", started.elapsed().as_millis());
    // 1 基、两端包含：[100, 200] => 101 行
    assert_eq!(back.lines.len(), 101);
}

#[test]
fn rows_cold_and_warm_cost() {
    let Some(path) = fixture() else { return };
    let home = revia_lib::shell::home_dir().unwrap();
    let env = revia_lib::shell::resolve(&home, &path);
    let session = WorktreeSession::open("test", &path, env, Arc::new(CommandLog::default())).unwrap();
    session.refresh().unwrap();

    for (label, view) in [("并排", ViewMode::Split), ("统一", ViewMode::Unified)] {
        let started = std::time::Instant::now();
        let slice = session
            .rows("src/generated.ts", ChangesKind::Unstaged, view, 0, 500)
            .expect("取行失败");
        println!("{label} 首次取 0..500：{} ms（共 {} 行）", started.elapsed().as_millis(), slice.row_count);
        let started = std::time::Instant::now();
        let _ = session
            .rows("src/generated.ts", ChangesKind::Unstaged, view, 4000, 4500)
            .expect("取行失败");
        println!("{label} 再次取 4000..4500：{} ms", started.elapsed().as_millis());
    }

    // 高亮：冷启动 + 第二块
    let started = std::time::Instant::now();
    let first = session
        .highlight("src/generated.ts", ChangesKind::Unstaged, FileVersion::New, 1, 400)
        .expect("高亮失败");
    println!("高亮首块（1..400）：{} ms", started.elapsed().as_millis());
    let started = std::time::Instant::now();
    let _ = session
        .highlight("src/generated.ts", ChangesKind::Unstaged, FileVersion::New, 401, 800)
        .expect("高亮失败");
    println!("高亮第二块（401..800）：{} ms", started.elapsed().as_millis());
    let started = std::time::Instant::now();
    let _ = session
        .highlight("src/generated.ts", ChangesKind::Unstaged, FileVersion::Old, 1, 400)
        .expect("高亮失败");
    println!("旧版本首块：{} ms", started.elapsed().as_millis());
    assert!(!first.lines.is_empty());
}

#[test]
fn highlight_request_shape_matches_frontend() {
    // 前端只会请求可见范围；这里确认范围语义与 0 基一致。
    let Some(path) = fixture() else { return };
    let home = revia_lib::shell::home_dir().unwrap();
    let env = revia_lib::shell::resolve(&home, &path);
    let session = WorktreeSession::open("test", &path, env, Arc::new(CommandLog::default())).unwrap();
    session.refresh().unwrap();
    // 1 基、两端包含：第 1 到第 3 行 => 3 行
    let result = session
        .highlight("src/generated.ts", ChangesKind::Unstaged, FileVersion::Old, 1, 3)
        .expect("高亮失败");
    assert_eq!(result.from_line, 1);
    assert_eq!(result.lines.len(), 3);
    let _ = HighlightRequest {
        document_id: "x".into(),
        generation: 1,
        language: "ts".into(),
        from_line: 0,
        to_line: 0,
        content: None,
    };
}
