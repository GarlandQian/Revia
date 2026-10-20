//! 集成测试：确认 Rust 高亮线程 + 分块协议在大文件上能正常返回。
//!
//! 这是 syntect 通路的回归测试：窗口不可见时的 GUI 测量依赖它能在大文件上快速返回。

use revia_m0_lib::highlight::{HighlightRequest, Highlighter, LineRange};

fn request(language: &str, content: &str, from: usize, to: usize) -> HighlightRequest {
    HighlightRequest {
        language: language.into(),
        content: content.into(),
        theme: None,
        line_range: Some(LineRange::new(from, to)),
        document_id: Some("bench".into()),
        generation: 0,
    }
}

#[test]
fn chunked_highlighting_large_file_terminates() {
    let path = std::env::var("REVIA_M0_BENCH_FILE")
        .unwrap_or_else(|_| "/tmp/revia-m0-fixtures/diffs/src/generated.ts".into());
    let Ok(content) = std::fs::read_to_string(&path) else {
        eprintln!("跳过：找不到基准文件 {path}");
        return;
    };
    let total = content.split_inclusive('\n').count();
    let highlighter = Highlighter::start();
    let started = std::time::Instant::now();

    let visible = highlighter
        .highlight(request("ts", &content, 0, 400))
        .expect("可见区域高亮失败");
    assert_eq!(visible.from, 0);
    assert_eq!(visible.lines.len(), 400);
    println!("可见区域 400 行：{} ms", started.elapsed().as_millis());

    let mut from = 400;
    let mut chunks = 1;
    while from < total {
        let to = (from + 2000).min(total);
        let result = highlighter
            .highlight(request("ts", &content, from, to))
            .expect("分块高亮失败");
        assert_eq!(result.from, from);
        chunks += 1;
        from = to;
    }
    println!("{chunks} 块全部完成：{} ms", started.elapsed().as_millis());
}
