use std::time::{Duration, Instant};

use gpui::{BenchAppContext, Entity, VisualContext};
use oxideterm_gpui_terminal::{TerminalPane, TerminalPlaybackUpdateTimings, TerminalUiPreferences};

const BENCHMARK_COLS: usize = 120;
const BENCHMARK_ROWS: usize = 40;
const INITIAL_LINES: usize = 400;
const OUTPUT_LINES_PER_FRAME: usize = 8;
const IDLE_STARTUP_SETTLE: Duration = Duration::from_millis(150);
const PROFILE_SAMPLE_CAPACITY: usize = 16_384;

fn benchmark_semantic_history(
    cx: &mut BenchAppContext<'_, '_>,
    history: usize,
    enabled: bool,
    warm: bool,
    scrollback: Option<usize>,
) {
    let mut preferences = TerminalUiPreferences::default();
    preferences.cursor_blink = false;
    preferences.semantic_coloring = enabled;
    preferences.command_marks_enabled = true;
    if let Some(scrollback) = scrollback {
        preferences.scrollback_lines = scrollback;
    }
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
            .expect("history benchmark terminal should initialize")
        })
        .expect("history benchmark window should remain open");
    let row = b"-rwxr-xr-x 1 alice staff 154208 Sep 4 12:30 app\r\n";
    let mut output = Vec::new();
    for _ in 0..OUTPUT_LINES_PER_FRAME {
        output.extend_from_slice(row);
    }
    terminal.update(cx, |terminal, cx| {
        terminal.enable_benchmark_performance_metrics();
        for _ in 0..history {
            terminal.feed_benchmark_output(
                b"\x1b]133;A\x07$ ls\r\n\x1b]133;C;cmdline_url=ls\x07old\r\n\x1b]133;D;0\x07",
                cx,
            );
        }
        terminal.feed_benchmark_output(b"\x1b]133;A\x07$ ls\r\n\x1b]133;C;cmdline_url=ls\x07", cx);
        for _ in 0..BENCHMARK_ROWS {
            terminal.feed_benchmark_output(row, cx);
        }
        assert!(terminal.command_facts().len() >= history.max(1));
    });
    cx.run_until_idle();
    let mut snapshot = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut layout = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut scene = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut updates = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut cache_hits = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut roles = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut highlights = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut command_lines = 0usize;
    let mut previous_frame = false;
    cx.bench_renderer(terminal, |terminal, _window, cx| {
        if previous_frame {
            let (snapshot_us, _, _, layout_us, scene_us) = terminal.benchmark_render_stage_micros();
            if !warm {
                snapshot.push(Duration::from_micros(snapshot_us));
            }
            layout.push(Duration::from_micros(layout_us));
            scene.push(Duration::from_micros(scene_us));
            cache_hits.push(terminal.benchmark_layout_cache_hit_percent());
            let (roles_us, highlights_us, commands) = terminal.benchmark_semantic_stage_micros();
            command_lines += commands;
            roles.push(Duration::from_micros(roles_us));
            highlights.push(Duration::from_micros(highlights_us));
        }
        previous_frame = true;
        let started = Instant::now();
        if warm {
            cx.notify();
        } else {
            terminal.feed_benchmark_output(&output, cx);
        }
        updates.push(started.elapsed());
    });
    eprintln!(
        "command-role rows per frame: {:.1}",
        command_lines as f64 / cache_hits.len().max(1) as f64
    );
    cx.record_stage_samples("snapshot", snapshot);
    cx.record_stage_samples("semantic role lookup", roles);
    cx.record_stage_samples("highlight cache and generation", highlights);
    cx.record_stage_samples("layout including semantic lookup", layout);
    cx.record_stage_samples("scene construction", scene);
    cx.record_stage_samples("output update including parse and events", updates);
    if !cache_hits.is_empty() {
        eprintln!(
            "layout cache hits: mean {:.1}%",
            cache_hits
                .iter()
                .map(|value| f64::from(*value))
                .sum::<f64>()
                / cache_hits.len() as f64
        );
    }
}

#[gpui::bench(fps = 120)]
fn semantic_history_stream_small(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_semantic_history(cx, 0, true, false, None);
}
#[gpui::bench(fps = 120)]
fn semantic_history_stream_2000(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_semantic_history(cx, 2000, true, false, None);
}
#[gpui::bench(fps = 120)]
fn semantic_history_warm_small(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_semantic_history(cx, 0, true, true, None);
}
#[gpui::bench(fps = 120)]
fn semantic_history_warm_2000(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_semantic_history(cx, 2000, true, true, None);
}
#[gpui::bench(fps = 120)]
fn semantic_history_disabled_2000(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_semantic_history(cx, 2000, false, false, None);
}

#[gpui::bench(fps = 120)]
fn semantic_history_stream_2000_untrimmed(cx: &mut BenchAppContext<'_, '_>) {
    // Keep the same command history while avoiding coordinate reuse during the measured run.
    benchmark_semantic_history(cx, 2000, true, false, Some(100_000));
}

fn benchmark_semantic_output(cx: &mut BenchAppContext<'_, '_>, enabled: bool) {
    let mut preferences = TerminalUiPreferences::default();
    preferences.cursor_blink = false;
    preferences.semantic_coloring = enabled;
    preferences.command_marks_enabled = true;
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
            .expect("semantic benchmark terminal should initialize")
        })
        .expect("semantic benchmark window should remain open");
    // Both cases replay identical plain output and command marks through real text shaping.
    let mut chunks = Vec::new();
    for (command, line) in [
        ("ls", "-rwxr-xr-x 1 alice staff 154208 Sep 4 12:30 app"),
        ("df", "/dev/sda1 100G 90G 10G 90% /"),
        (
            "ping",
            "64 bytes from 1.1.1.1: icmp_seq=1 ttl=57 time=12.4 ms",
        ),
    ] {
        let mut chunk =
            format!("\x1b]133;A\x07$ {command}\r\n\x1b]133;C;cmdline_url={command}\x07")
                .into_bytes();
        for _ in 0..OUTPUT_LINES_PER_FRAME {
            chunk.extend_from_slice(line.as_bytes());
            chunk.extend_from_slice(b"\r\n");
        }
        chunk.extend_from_slice(b"\x1b]133;D;0\x07");
        chunks.push(chunk);
    }
    terminal.update(cx, |terminal, cx| {
        for _ in 0..INITIAL_LINES / OUTPUT_LINES_PER_FRAME {
            terminal.feed_benchmark_output(&chunks[0], cx);
        }
        assert!(
            terminal
                .command_marks()
                .iter()
                .any(|mark| mark.command.as_deref() == Some("ls")),
            "semantic benchmark requires the replayed command role"
        );
    });
    cx.run_until_idle();
    let mut index = 0;
    cx.bench_renderer(terminal, move |terminal, _window, cx| {
        terminal.feed_benchmark_output(&chunks[index], cx);
        index = (index + 1) % chunks.len();
    });
}

#[gpui::bench(fps = 120)]
fn terminal_semantic_output_disabled(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_semantic_output(cx, false);
}

#[gpui::bench(fps = 120)]
fn terminal_semantic_output_enabled(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_semantic_output(cx, true);
}

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
    semantic_history_stream_small,
    semantic_history_stream_2000,
    semantic_history_warm_small,
    semantic_history_warm_2000,
    semantic_history_disabled_2000,
    semantic_history_stream_2000_untrimmed,
    terminal_semantic_output_disabled,
    terminal_semantic_output_enabled,
    terminal_warm_cache_redraw_frame,
    terminal_playback_output_frame,
    terminal_playback_output_pipeline,
    terminal_idle_no_frames
);
gpui::bench_main!(terminal_render);
