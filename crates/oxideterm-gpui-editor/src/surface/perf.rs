// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::{Duration, Instant};

use gpui::{ScrollDelta, ScrollWheelEvent, TestAppContext, point, px, size};
use oxideterm_editor_syntax::LanguageId;
use oxideterm_theme::default_tokens;

use super::TextEditorView;

const BENCHMARK_VIEWPORT_WIDTH: f32 = 1280.0;
const BENCHMARK_VIEWPORT_HEIGHT: f32 = 720.0;
const BENCHMARK_WARMUP_EVENTS: usize = 64;
const BENCHMARK_SAMPLES: usize = 60;
const BENCHMARK_EVENTS_PER_SAMPLE: usize = 8;
const BENCHMARK_SCROLL_DELTA_PX: f32 = -24.0;

#[gpui::test]
#[ignore = "manual release performance baseline"]
fn scroll_performance_baseline(cx: &mut TestAppContext) {
    for line_count in [10_000, 100_000] {
        let source = rust_fixture(line_count);
        let (editor, cx) =
            cx.add_window_view(|_, cx| TextEditorView::new(source, &default_tokens(), cx));
        cx.simulate_resize(size(
            px(BENCHMARK_VIEWPORT_WIDTH),
            px(BENCHMARK_VIEWPORT_HEIGHT),
        ));
        editor.update(cx, |editor, cx| {
            editor.set_language(Some(LanguageId::Rust), cx);
            editor.reveal_line_column((line_count / 2) as u32, 1, cx);
        });

        // Two initial draws stabilize bounds, font metrics, syntax caches, and
        // the visible-row cache before any measurement begins.
        draw(cx);
        draw(cx);
        for _ in 0..BENCHMARK_WARMUP_EVENTS {
            scroll_once(cx);
        }

        let mut per_event = Vec::with_capacity(BENCHMARK_SAMPLES);
        for _ in 0..BENCHMARK_SAMPLES {
            let started = Instant::now();
            for _ in 0..BENCHMARK_EVENTS_PER_SAMPLE {
                scroll_once(cx);
            }
            per_event.push(started.elapsed() / BENCHMARK_EVENTS_PER_SAMPLE as u32);
        }
        per_event.sort_unstable();
        let median = percentile(&per_event, 0.50);
        let p95 = percentile(&per_event, 0.95);
        println!(
            "{{\"fixture_lines\":{line_count},\"viewport\":\"1280x720\",\"samples\":{},\"events_per_sample\":{},\"median_us_per_event\":{:.1},\"p95_us_per_event\":{:.1}}}",
            BENCHMARK_SAMPLES,
            BENCHMARK_EVENTS_PER_SAMPLE,
            median.as_secs_f64() * 1_000_000.0,
            p95.as_secs_f64() * 1_000_000.0,
        );
    }
}

fn scroll_once(cx: &mut gpui::VisualTestContext) {
    cx.simulate_event(ScrollWheelEvent {
        position: point(
            px(BENCHMARK_VIEWPORT_WIDTH / 2.0),
            px(BENCHMARK_VIEWPORT_HEIGHT / 2.0),
        ),
        delta: ScrollDelta::Pixels(point(px(0.0), px(BENCHMARK_SCROLL_DELTA_PX))),
        ..Default::default()
    });
    draw(cx);
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
}

fn percentile(sorted: &[Duration], percentile: f64) -> Duration {
    let index = ((sorted.len() as f64 * percentile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len().saturating_sub(1));
    sorted[index]
}

fn rust_fixture(line_count: usize) -> String {
    const LINES_PER_BLOCK: usize = 5;
    let block_count = line_count.div_ceil(LINES_PER_BLOCK);
    let mut source = String::with_capacity(block_count * 96);
    for index in 0..block_count {
        source.push_str(&format!(
            "fn item_{index}() {{\n    if item_{index}_enabled() {{\n        run_item({index});\n    }}\n}}\n"
        ));
    }
    source
}
