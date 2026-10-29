use super::*;
use gpui::{AvailableSpace, TestAppContext, size};
use std::time::Instant;

#[cfg(target_os = "macos")]
#[gpui::test]
#[ignore = "manual dense-search and shared-snapshot allocation benchmark"]
fn editor_search_snapshot_memory(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| {
        TextEditorView::new(
            "word word word word\n".repeat(250_000),
            &oxideterm_theme::default_tokens(),
            cx,
        )
    });
    // Separate search storage from the existing document row index.
    editor.update(cx, |editor, _| {
        let _ = editor.display_rows();
    });
    let before = allocator_bytes();
    let started = Instant::now();
    editor.update(cx, |editor, cx| editor.set_find_query("word", cx));
    let dispatch_ms = started.elapsed().as_secs_f64() * 1000.0;
    cx.run_until_parked();
    let complete_ms = started.elapsed().as_secs_f64() * 1000.0;
    let retained = allocator_bytes().saturating_sub(before);
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.find_matches.len(), 1_000_000);
        let last = editor.find_matches.iter().last().unwrap();
        assert_eq!(last.range.start.0..last.range.end.0, 4_999_995..4_999_999);
    });
    eprintln!(
        "EDITOR_SEARCH retained={retained} dispatch_ms={dispatch_ms:.3} complete_ms={complete_ms:.3}"
    );
    drop(editor);
    cx.run_until_parked();

    let mut buffer = TextBuffer::new("");
    buffer
        .replace_selection(
            Selection::caret(BufferOffset(0)),
            "x".repeat(8 * 1024 * 1024),
        )
        .unwrap();
    let snapshot = buffer.shared_snapshot();
    let before = allocator_bytes();
    let started = Instant::now();
    buffer
        .replace_selection(Selection::caret(BufferOffset(buffer.len())), "z")
        .unwrap();
    let append_ms = started.elapsed().as_secs_f64() * 1000.0;
    let retained = allocator_bytes().saturating_sub(before);
    assert_eq!(snapshot.len(), 8 * 1024 * 1024);
    assert_eq!(
        buffer
            .slice(TextRange::new(
                BufferOffset(buffer.len() - 2),
                BufferOffset(buffer.len())
            ))
            .unwrap(),
        "xz"
    );
    buffer.undo().unwrap();
    assert_eq!(buffer.len(), snapshot.len());
    buffer.redo().unwrap();
    assert_eq!(buffer.len(), snapshot.len() + 1);
    eprintln!("EDITOR_SNAPSHOT_APPEND retained={retained} append_ms={append_ms:.3}");

    let mut buffer = TextBuffer::new("x".repeat(8 * 1024 * 1024));
    let snapshot = buffer.shared_snapshot();
    let before = allocator_bytes();
    let started = Instant::now();
    buffer
        .replace_selection(
            Selection::new(BufferOffset(1), BufferOffset(buffer.len() - 1)),
            "",
        )
        .unwrap();
    let delete_ms = started.elapsed().as_secs_f64() * 1000.0;
    let retained = allocator_bytes().saturating_sub(before);
    assert_eq!(buffer.text(), "xx");
    buffer.undo().unwrap();
    assert_eq!(buffer.len(), snapshot.len());
    assert!(buffer.with_temporary_text(|text| text.bytes().all(|byte| byte == b'x')));
    buffer.redo().unwrap();
    assert_eq!(buffer.text(), "xx");
    eprintln!("EDITOR_SNAPSHOT_DELETE retained={retained} delete_ms={delete_ms:.3}");
}

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
            crate::grammar_fixture::install_rust(cx);
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
#[ignore = "manual long-line and wrap allocation benchmark"]
fn editor_layout_memory(cx: &mut TestAppContext) {
    for (name, source, wrap) in [
        ("long-ascii", "0123456789 abcdef ".repeat(16_384), false),
        ("long-unicode", "中文 e\u{301} 👨‍👩‍👧‍👦 ".repeat(4096), false),
        (
            "wrapped",
            "let value = 42; // wrap measurement\n".repeat(16_384),
            true,
        ),
    ] {
        let baseline = allocator_bytes();
        let started = Instant::now();
        let (editor, cx) = cx.add_window_view(|_, cx| {
            let mut editor = TextEditorView::new(source, &oxideterm_theme::default_tokens(), cx);
            // Integral test advances isolate layout costs from the no-op
            // shaper's synthetic floating-point accumulation drift.
            editor.apply_runtime_settings(
                &oxideterm_theme::default_tokens(),
                "monospace".into(),
                10.0,
                1.2,
                wrap,
                false,
                cx,
            );
            editor
        });
        cx.simulate_resize(size(px(1000.0), px(700.0)));
        editor.update(cx, |editor, _| editor.settings.soft_wrap = wrap);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let first_ms = started.elapsed().as_secs_f64() * 1000.0;
        let opened = allocator_bytes();
        let started = Instant::now();
        editor.update(cx, |editor, cx| editor.insert_text("z", cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let edit_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        cx.simulate_resize(size(px(600.0), px(700.0)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let resize_ms = started.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "EDITOR_LAYOUT name={name} baseline={baseline} opened={opened} final={} first_ms={first_ms:.3} edit_ms={edit_ms:.3} resize_ms={resize_ms:.3}",
            allocator_bytes()
        );
        cx.update(|window, _| window.remove_window());
        drop(editor);
        cx.cx.update(|_| {});
        cx.run_until_parked();
    }
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
            crate::grammar_fixture::install_rust(cx);
            editor.set_language(language, cx);
            editor
        });
        cx.simulate_resize(size(px(1000.0), px(700.0)));
        let draw = |cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| window.draw(cx).clear(cx));
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
        let mut scroll_frames = Vec::new();
        for page in (0..128).chain((0..128).rev()) {
            editor.update(cx, |editor, cx| {
                editor.viewport.scroll_y_px = editor.metrics.line_height * (page * 48) as f32;
                cx.notify();
            });
            let started = Instant::now();
            draw(cx);
            scroll_frames.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        let traversed = allocator_bytes();
        let started = Instant::now();
        editor.update(cx, |editor, cx| {
            editor.set_find_case_sensitive(true, cx);
            editor.set_find_query("zzzz", cx);
        });
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert_eq!(
                editor
                    .find_matches
                    .iter()
                    .map(|hit| hit.range.start.0..hit.range.end.0)
                    .collect::<Vec<_>>(),
                (3..32).map(|start| start..start + 4).collect::<Vec<_>>()
            );
        });
        let search_ms = started.elapsed().as_secs_f64() * 1000.0;
        draw(cx);
        let searched = allocator_bytes();
        editor.update(cx, |editor, cx| editor.set_find_query("", cx));
        draw(cx);
        let search_cleared = allocator_bytes();
        scroll_frames.sort_by(f64::total_cmp);
        eprintln!(
            "EDITOR_SCROLL_SEARCH workload={name} traversed={traversed} searched={searched} search_cleared={search_cleared} search_ms={search_ms:.3} scroll_p50_ms={:.3} scroll_p95_ms={:.3} scroll_max_ms={:.3}",
            scroll_frames[scroll_frames.len() / 2],
            scroll_frames[scroll_frames.len() * 95 / 100],
            scroll_frames.last().unwrap(),
        );
        if name == "rust" {
            // Release each owner immediately before teardown to measure its retained allocation.
            editor.update(cx, |editor, _| {
                let before = allocator_bytes();
                drop(editor.syntax.take());
                let syntax = allocator_bytes();
                drop(std::mem::take(&mut editor.highlight_spans));
                let highlights = allocator_bytes();
                drop(std::mem::take(&mut editor.bracket_index));
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
                crate::grammar_fixture::install_rust(cx);
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
                let syntax = crate::grammar_fixture::parse(LanguageId::Rust, &expected).unwrap();
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

#[gpui::test]
#[ignore = "manual coalesced input benchmark"]
fn editor_burst_performance(cx: &mut TestAppContext) {
    let source = "// burst benchmark\nfn sample() { let value = 42; }\n".repeat(22000);
    let editor =
        cx.new(|cx| TextEditorView::new(source.clone(), &oxideterm_theme::default_tokens(), cx));
    editor.update(cx, |editor, cx| {
        crate::grammar_fixture::install_rust(cx);
        editor.set_language(Some(LanguageId::Rust), cx)
    });
    cx.run_until_parked();
    let mut expected = source;
    for run in 0..6 {
        let started = Instant::now();
        let input_ms = editor.update(cx, |editor, cx| {
            editor
                .cursor
                .set_selection(Selection::caret(BufferOffset(3)));
            let started = Instant::now();
            for _ in 0..64 {
                editor.insert_text("z", cx);
            }
            started.elapsed().as_secs_f64() * 1000.0
        });
        cx.run_until_parked();
        let ready_ms = started.elapsed().as_secs_f64() * 1000.0;
        expected.insert_str(3, &"z".repeat(64));
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.buffer.text(), expected);
            assert_eq!(editor.syntax_version, Some(editor.buffer.version()));
            assert_eq!(
                editor
                    .highlight_spans
                    .spans_in_range(0..expected.len())
                    .collect::<Vec<_>>(),
                crate::grammar_fixture::parse(LanguageId::Rust, &expected)
                    .unwrap()
                    .highlight_spans(&expected)
            );
        });
        eprintln!("EDITOR_BURST run={run} input_ms={input_ms:.3} ready_ms={ready_ms:.3}");
    }
}
