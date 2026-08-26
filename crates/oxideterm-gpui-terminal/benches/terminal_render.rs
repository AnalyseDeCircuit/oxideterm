use std::time::Duration;

use gpui::{BenchAppContext, Entity, VisualContext};
use oxideterm_gpui_terminal::{TerminalPane, TerminalPlaybackUpdateTimings, TerminalUiPreferences};

const BENCHMARK_COLS: usize = 120;
const BENCHMARK_ROWS: usize = 40;
const INITIAL_LINES: usize = 400;
const OUTPUT_LINES_PER_FRAME: usize = 8;
const IDLE_STARTUP_SETTLE: Duration = Duration::from_millis(150);
const PROFILE_SAMPLE_CAPACITY: usize = 16_384;

fn terminal_corpus(lines: usize) -> Vec<u8> {
    terminal_corpus_from(0, lines)
}

fn terminal_corpus_from(first_line: usize, lines: usize) -> Vec<u8> {
    let mut corpus = Vec::with_capacity(lines * 96);
    for line in first_line..first_line + lines {
        corpus.extend_from_slice(
            format!(
                "\x1b[38;5;{}moxideterm render benchmark line {line} cargo check\x1b[0m\r\n",
                line % 256
            )
            .as_bytes(),
        );
    }
    corpus
}

fn benchmark_terminal(
    cx: &mut BenchAppContext<'_, '_>,
    collect_render_stages: bool,
) -> Entity<TerminalPane> {
    let mut preferences = TerminalUiPreferences::default();
    // Cursor animation would add timer-driven frames unrelated to the measured invalidation.
    preferences.cursor_blink = false;
    preferences.show_performance_overlay = false;

    let mut window = cx.add_empty_window();
    let terminal = window
        .replace_root_view(|window, cx| {
            TerminalPane::new_recording_playback(
                BENCHMARK_COLS,
                BENCHMARK_ROWS,
                preferences,
                window,
                cx,
            )
            .expect("benchmark playback terminal should initialize")
        })
        .expect("benchmark terminal window should remain open");
    terminal.update(cx, |terminal, cx| {
        if collect_render_stages {
            terminal.enable_benchmark_performance_metrics();
        }
        terminal.feed_recording_output(&terminal_corpus(INITIAL_LINES), cx);
    });
    cx.run_until_idle();
    terminal
}

fn playback_output_chunks() -> Vec<Vec<u8>> {
    // Prebuild every chunk so benchmark iterations measure terminal work, not corpus formatting.
    (0..256)
        .map(|chunk| {
            terminal_corpus_from(
                INITIAL_LINES + chunk * OUTPUT_LINES_PER_FRAME,
                OUTPUT_LINES_PER_FRAME,
            )
        })
        .collect()
}

#[gpui::bench(fps = 120)]
fn terminal_warm_cache_redraw_frame(cx: &mut BenchAppContext<'_, '_>) {
    let terminal = benchmark_terminal(cx, false);
    cx.bench_renderer(terminal, |_terminal, _window, cx| {
        // Force the same visible terminal through prepaint and paint to measure warm-cache cost.
        cx.notify();
    });
}

#[gpui::bench(fps = 120)]
fn terminal_playback_output_frame(cx: &mut BenchAppContext<'_, '_>) {
    let terminal = benchmark_terminal(cx, false);
    let output_chunks = playback_output_chunks();
    let mut chunk_index = 0;
    cx.bench_renderer(terminal, move |terminal, _window, cx| {
        terminal.feed_recording_output(&output_chunks[chunk_index], cx);
        chunk_index = (chunk_index + 1) % output_chunks.len();
    });
}

#[gpui::bench(fps = 120)]
fn terminal_playback_output_pipeline(cx: &mut BenchAppContext<'_, '_>) {
    let terminal = benchmark_terminal(cx, true);
    let output_chunks = playback_output_chunks();
    let mut chunk_index = 0;
    let mut update_samples =
        Vec::<TerminalPlaybackUpdateTimings>::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut snapshot_samples = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut backend_snapshot_samples = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut snapshot_state_samples = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut layout_samples = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut scene_build_samples = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut has_previous_render = false;
    cx.bench_renderer(terminal, |terminal, _window, cx| {
        if has_previous_render {
            let (
                snapshot_micros,
                backend_snapshot_micros,
                snapshot_state_micros,
                layout_micros,
                scene_build_micros,
            ) = terminal.benchmark_render_stage_micros();
            snapshot_samples.push(Duration::from_micros(snapshot_micros));
            backend_snapshot_samples.push(Duration::from_micros(backend_snapshot_micros));
            snapshot_state_samples.push(Duration::from_micros(snapshot_state_micros));
            layout_samples.push(Duration::from_micros(layout_micros));
            scene_build_samples.push(Duration::from_micros(scene_build_micros));
        }
        has_previous_render = true;
        update_samples
            .push(terminal.feed_recording_output_profiled(&output_chunks[chunk_index], cx));
        chunk_index = (chunk_index + 1) % output_chunks.len();
    });

    cx.record_stage_samples(
        "terminal lock",
        update_samples.iter().map(|sample| sample.terminal_lock),
    );
    cx.record_stage_samples(
        "PTY parse + grid",
        update_samples.iter().map(|sample| sample.parse_and_grid),
    );
    cx.record_stage_samples(
        "event extraction",
        update_samples.iter().map(|sample| sample.event_extraction),
    );
    cx.record_stage_samples(
        "GPUI state update",
        update_samples.iter().map(|sample| sample.gpui_state_update),
    );
    cx.record_stage_samples("incremental snapshot", snapshot_samples);
    cx.record_stage_samples("backend snapshot", backend_snapshot_samples);
    cx.record_stage_samples("snapshot pane state", snapshot_state_samples);
    cx.record_stage_samples("line layout", layout_samples);
    cx.record_stage_samples("scene construction", scene_build_samples);
}

#[gpui::bench(fps = 120)]
fn terminal_idle_no_frames(cx: &mut BenchAppContext<'_, '_>) {
    let _terminal = benchmark_terminal(cx, false);
    // Drain startup sizing and the scheduler's initial maintenance deadline before measuring the
    // steady idle state. Cursor blinking is disabled by the fixture.
    std::thread::sleep(IDLE_STARTUP_SETTLE);
    cx.run_until_idle();
    cx.bench_iter(|cx| cx.run_until_idle());
    cx.assert_no_rendered_frames();
}

gpui::bench_group!(
    terminal_render,
    terminal_warm_cache_redraw_frame,
    terminal_playback_output_frame,
    terminal_playback_output_pipeline,
    terminal_idle_no_frames
);
gpui::bench_main!(terminal_render);
