use super::*;
use gpui::{AvailableSpace, TestAppContext, size};
use std::time::Instant;

#[gpui::test]
#[ignore = "manual release-profile editor input and headless paint benchmark"]
fn editor_input_performance(cx: &mut TestAppContext) {
    for (name, bytes, language, pattern) in [
        (
            "rust-small",
            100usize * 1024,
            Some(LanguageId::Rust),
            "// editor benchmark\nfn example() { let value = 42; }\n",
        ),
        (
            "rust-large",
            1024 * 1024,
            Some(LanguageId::Rust),
            "// editor benchmark\nfn example() { let value = 42; }\n",
        ),
        (
            "rust-multiline",
            1024 * 1024,
            Some(LanguageId::Rust),
            "fn example() {\n    if ready {\n        run();\n    }\n}\n",
        ),
        (
            "many-lines",
            16 * 1024 * 1024,
            None,
            "editor benchmark plain text 0123456789\n",
        ),
        ("long-line", 256 * 1024, None, "plain text 0123456789 "),
    ] {
        let text = pattern.repeat(bytes.div_ceil(pattern.len()));
        let bytes = text.len();
        let started = Instant::now();
        let (editor, cx) = cx.add_window_view(move |_, cx| {
            let mut editor = TextEditorView::new(text, &oxideterm_theme::default_tokens(), cx);
            editor.set_language(language, cx);
            editor
        });
        let open_ms = started.elapsed().as_secs_f64() * 1000.0;
        let draw = |cx: &mut gpui::VisualTestContext| {
            cx.draw(
                point(px(0.0), px(0.0)),
                size(
                    AvailableSpace::Definite(px(1000.0)),
                    AvailableSpace::Definite(px(700.0)),
                ),
                |_, _| editor.clone().into_element(),
            );
        };
        cx.run_until_parked();
        draw(cx);
        for run in 0..12 {
            editor.update(cx, |editor, _| {
                editor
                    .cursor
                    .set_selection(Selection::caret(BufferOffset(3)))
            });
            let started = Instant::now();
            let edit_ms = editor.update(cx, |editor, cx| {
                let editing = Instant::now();
                editor.insert_text("z", cx);
                editing.elapsed().as_secs_f64() * 1000.0
            });
            draw(cx);
            let painted_ms = started.elapsed().as_secs_f64() * 1000.0;
            cx.run_until_parked();
            let ready_ms = started.elapsed().as_secs_f64() * 1000.0;
            editor.read_with(cx, |editor, _| {
                assert_eq!(
                    editor
                        .buffer
                        .slice(TextRange::new(BufferOffset(3), BufferOffset(4)))
                        .unwrap(),
                    "z"
                );
                assert_eq!(editor.cursor.selection().head, BufferOffset(4));
            });
            eprintln!(
                "EDITOR_BENCH workload={name} bytes={bytes} run={run} open_ms={open_ms:.3} edit_ms={edit_ms:.3} painted_ms={painted_ms:.3} ready_ms={ready_ms:.3}"
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn allocator_bytes() -> usize {
    #[repr(C)]
    #[derive(Default)]
    struct Statistics {
        blocks_in_use: u32,
        size_in_use: usize,
        max_size_in_use: usize,
        size_allocated: usize,
    }
    unsafe extern "C" {
        fn malloc_zone_statistics(zone: *mut std::ffi::c_void, stats: *mut Statistics);
    }
    let mut stats = Statistics::default();
    // macOS defines a null zone as the aggregate of all malloc zones.
    unsafe {
        malloc_zone_statistics(std::ptr::null_mut(), &mut stats);
    }
    stats.size_in_use
}

#[cfg(target_os = "macos")]
#[gpui::test]
#[ignore = "manual macOS editor live-allocation lifecycle benchmark"]
fn editor_memory_performance(cx: &mut TestAppContext) {
    for (name, bytes, language, pattern) in [
        (
            "rust",
            1024usize * 1024,
            Some(LanguageId::Rust),
            "// memory benchmark\nfn example() { let value = 42; }\n",
        ),
        (
            "rust-multiline",
            1024 * 1024,
            Some(LanguageId::Rust),
            "fn example() {\n    if ready {\n        run();\n    }\n}\n",
        ),
        (
            "plain-16",
            16 * 1024 * 1024,
            None,
            "editor memory benchmark 0123456789\n",
        ),
        (
            "plain-64",
            64 * 1024 * 1024,
            None,
            "editor memory benchmark 0123456789\n",
        ),
    ] {
        let baseline = allocator_bytes();
        let text = pattern.repeat(bytes.div_ceil(pattern.len()));
        let bytes = text.len();
        let (editor, cx) = cx.add_window_view(move |_, cx| {
            let mut editor = TextEditorView::new(text, &oxideterm_theme::default_tokens(), cx);
            editor.set_language(language, cx);
            editor
        });
        let draw = |cx: &mut gpui::VisualTestContext| {
            cx.draw(
                point(px(0.0), px(0.0)),
                size(
                    AvailableSpace::Definite(px(1000.0)),
                    AvailableSpace::Definite(px(700.0)),
                ),
                |_, _| editor.clone().into_element(),
            );
        };
        cx.run_until_parked();
        draw(cx);
        let opened = allocator_bytes();
        for _ in 0..32 {
            editor.update(cx, |editor, cx| {
                editor
                    .cursor
                    .set_selection(Selection::caret(BufferOffset(3)));
                editor.insert_text("z", cx);
            });
        }
        cx.run_until_parked();
        draw(cx);
        let edited = allocator_bytes();
        // Scroll away from the initial viewport without changing the document.
        editor.update(cx, |editor, cx| {
            editor.viewport.scroll_y_px =
                editor.metrics.line_height * (editor.buffer.line_count() / 2) as f32;
            cx.notify();
        });
        draw(cx);
        let scrolled = allocator_bytes();
        if name == "rust" {
            // Release each owner immediately before teardown to measure its retained allocation.
            editor.update(cx, |editor, _| {
                let before = allocator_bytes();
                drop(editor.syntax.take());
                let syntax = allocator_bytes();
                drop(std::mem::take(&mut editor.highlight_spans));
                let highlights = allocator_bytes();
                drop(std::mem::take(&mut editor.bracket_pair_by_caret));
                let brackets = allocator_bytes();
                let lines = allocator_bytes();
                drop(std::mem::take(&mut editor.structure_cache));
                let folds = allocator_bytes();
                drop(editor.display_rows_cache.borrow_mut().take());
                let layout = allocator_bytes();
                eprintln!("EDITOR_OWNERS syntax={} highlights={} brackets={} highlight_lines={} folds_indent={} layout={}", before - syntax, syntax - highlights, highlights - brackets, brackets - lines, lines - folds, folds - layout);
            });
        }
        let weak = editor.downgrade();
        cx.update(|window, _| window.remove_window());
        drop(editor);
        cx.cx.update(|_| {});
        cx.run_until_parked();
        assert!(weak.upgrade().is_none(), "closed editor is still retained");
        let closed = allocator_bytes();
        eprintln!(
            "EDITOR_MEMORY workload={name} bytes={bytes} baseline={baseline} opened={opened} edited={edited} scrolled={scrolled} closed={closed}"
        );
    }
}

#[gpui::test]
#[ignore = "manual disruptive-edit latency benchmark"]
fn editor_disruptive_edit_performance(cx: &mut TestAppContext) {
    let function = "fn example() {\n    let value = 42;\n}\n";
    let statement = "    let value = 42;\n";
    let functions = function.repeat((1024usize * 1024).div_ceil(function.len()));
    let body = statement.repeat((1024usize * 1024).div_ceil(statement.len()));
    for workload in ["long-function", "close-comment", "paste-1m"] {
        for run in 0..6 {
            let (source, range, replacement) = match workload {
                "long-function" => {
                    let source = format!("fn main() {{\n{body}}}\n");
                    let start = source.find("42").unwrap();
                    (source, start..start + 2, "43".to_string())
                }
                "close-comment" => (format!("/*\n{functions}*/\n"), 3..3, "*/\n".to_string()),
                _ => {
                    let source = "fn initial() {}\n".to_string();
                    let end = source.len();
                    (source, end..end, functions.clone())
                }
            };
            let mut expected = source.clone();
            expected.replace_range(range.clone(), &replacement);
            let bytes = expected.len();
            let (editor, cx) = cx.add_window_view(move |_, cx| {
                let mut editor =
                    TextEditorView::new(source, &oxideterm_theme::default_tokens(), cx);
                editor.set_language(Some(LanguageId::Rust), cx);
                editor
            });
            let draw = |cx: &mut gpui::VisualTestContext| {
                cx.draw(
                    point(px(0.0), px(0.0)),
                    size(
                        AvailableSpace::Definite(px(1000.0)),
                        AvailableSpace::Definite(px(700.0)),
                    ),
                    |_, _| editor.clone().into_element(),
                );
            };
            cx.run_until_parked();
            draw(cx);
            let started = Instant::now();
            let edit_ms = editor.update(cx, |editor, cx| {
                let started = Instant::now();
                editor.replace_range_with_caret(
                    TextRange::new(BufferOffset(range.start), BufferOffset(range.end)),
                    replacement,
                    cx,
                );
                started.elapsed().as_secs_f64() * 1000.0
            });
            draw(cx);
            let painted_ms = started.elapsed().as_secs_f64() * 1000.0;
            cx.run_until_parked();
            let ready_ms = started.elapsed().as_secs_f64() * 1000.0;
            editor.read_with(cx, |editor, _| {
                assert_eq!(editor.buffer.text(), expected);
                let syntax = SyntaxSession::parse(LanguageId::Rust, &expected).unwrap();
                assert_eq!(
                    editor
                        .highlight_spans
                        .spans_in_range(0..bytes)
                        .collect::<Vec<_>>(),
                    syntax.highlight_spans(&expected)
                );
            });
            cx.update(|window, _| window.remove_window());
            drop(editor);
            cx.cx.update(|_| {});
            cx.run_until_parked();
            eprintln!(
                "DISRUPTIVE_EDIT workload={workload} bytes={bytes} run={run} edit_ms={edit_ms:.3} painted_ms={painted_ms:.3} ready_ms={ready_ms:.3}"
            );
        }
    }
}
