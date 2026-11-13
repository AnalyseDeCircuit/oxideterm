use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::time::{Duration, Instant};

use gpui::{
    AppContext, BenchAppContext, Context, Entity, IntoElement, ParentElement, Render, Styled,
    VisualContext, Window, div,
};
use oxideterm_gpui_terminal::{TerminalPane, TerminalPlaybackUpdateTimings, TerminalUiPreferences};

const BENCHMARK_COLS: usize = 120;
const BENCHMARK_ROWS: usize = 40;
const INITIAL_LINES: usize = 400;
const OUTPUT_LINES_PER_FRAME: usize = 8;
const IDLE_STARTUP_SETTLE: Duration = Duration::from_millis(150);
const PROFILE_SAMPLE_CAPACITY: usize = 16_384;

struct TrackedAllocator;
static LIVE_HEAP: AtomicUsize = AtomicUsize::new(0);

// Requested Rust heap only: native font objects and GPU allocations are not counted.
unsafe impl GlobalAlloc for TrackedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            LIVE_HEAP.fetch_add(layout.size(), Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_HEAP.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(ptr, layout) };
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(ptr, layout, size) };
        if !result.is_null() {
            LIVE_HEAP.fetch_sub(layout.size(), Relaxed);
            LIVE_HEAP.fetch_add(size, Relaxed);
        }
        result
    }
}
#[global_allocator]
static ALLOCATOR: TrackedAllocator = TrackedAllocator;

struct TerminalTabs {
    panes: Vec<Entity<TerminalPane>>,
    selected: usize,
}

impl TerminalTabs {
    fn select(&mut self, selected: usize, cx: &mut Context<Self>) {
        for (index, pane) in self.panes.iter().enumerate() {
            pane.update(cx, |pane, cx| {
                pane.set_render_cache_visible(index == selected, cx)
            });
        }
        self.selected = selected;
        cx.notify();
    }
}

impl Render for TerminalTabs {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.panes[self.selected].clone())
    }
}

#[gpui::bench(fps = 120)]
fn terminal_tab_switch_frame(cx: &mut BenchAppContext<'_, '_>) {
    let before = LIVE_HEAP.load(Relaxed);
    let tabs = cx
        .add_empty_window()
        .replace_root_view(|window, cx| {
            let panes = (0..8)
                .map(|_| {
                    cx.new(|cx| {
                        let mut preferences = TerminalUiPreferences::default();
                        preferences.cursor_blink = false;
                        TerminalPane::new_recording_playback(
                            BENCHMARK_COLS,
                            BENCHMARK_ROWS,
                            preferences,
                            window,
                            cx,
                        )
                        .unwrap()
                    })
                })
                .collect();
            TerminalTabs { panes, selected: 0 }
        })
        .unwrap();
    cx.settle();
    for index in 0..8 {
        tabs.update(cx, |tabs, cx| tabs.select(index, cx));
        cx.settle();
        for chunk in 0..32 {
            let output = terminal_corpus_from((index * 32 + chunk) * 16, 16);
            tabs.update(cx, |tabs, cx| {
                tabs.panes[index].update(cx, |pane, cx| pane.feed_recording_output(&output, cx));
            });
            cx.settle();
        }
    }
    std::thread::sleep(IDLE_STARTUP_SETTLE);
    cx.settle();
    static REPORTED: AtomicBool = AtomicBool::new(false);
    if !REPORTED.swap(true, Relaxed) {
        eprintln!(
            "8 tabs, 7 hidden: requested_heap_bytes={}",
            LIVE_HEAP.load(Relaxed).saturating_sub(before)
        );
    }
    cx.bench_renderer(tabs, |tabs, _, cx| {
        tabs.select((tabs.selected + 1) % tabs.panes.len(), cx);
    });
}

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
    preferences.command_folding_enabled =
        std::env::var("OXIDE_BENCH_COMMAND_FOLDING").as_deref() != Ok("0");
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
    cx.settle();
    std::thread::sleep(IDLE_STARTUP_SETTLE);
    cx.settle();
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
    cx.settle();
    // Painting schedules the real viewport resize on the next frame and its debounce timer.
    // Let that resize finish before calibration so each sample renders the same settled grid.
    std::thread::sleep(IDLE_STARTUP_SETTLE);
    cx.settle();
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
fn terminal_box_drawing_redraw_frame(cx: &mut BenchAppContext<'_, '_>) {
    let terminal = benchmark_terminal(cx, false);
    terminal.update(cx, |terminal, cx| {
        let rows = "┌────┬────┐ ╔════╦════╗ ░▒▓█▁▂▃▄▅▆▇\r\n│    │    │ ║    ║    ║ ▖▗▘▙▚▛▜▝▞▟\r\n├────┼────┤ ╠════╬════╣ ┄┅┆┇┈┉┊┋\r\n└────┴────┘ ╚════╩════╝ ╭─╮ ╲╱\r\n".repeat(10);
        terminal.feed_recording_output(rows.as_bytes(), cx);
    });
    cx.run_until_idle();
    cx.bench_renderer(terminal, |_terminal, _window, cx| cx.notify());
}

#[gpui::bench(fps = 120)]
fn terminal_warm_cache_redraw_frame(cx: &mut BenchAppContext<'_, '_>) {
    let terminal = benchmark_terminal(cx, false);
    let grid = benchmark_grid_size(cx, &terminal);
    cx.bench_renderer(terminal.clone(), |_terminal, _window, cx| {
        // Force the same visible terminal through prepaint and paint to measure warm-cache cost.
        cx.notify();
    });
    assert_eq!(
        benchmark_grid_size(cx, &terminal),
        grid,
        "viewport resize must settle before measurement"
    );
}

fn benchmark_generated_background(
    cx: &mut BenchAppContext<'_, '_>,
    kind: oxideterm_gpui_background::GeneratedEffectKind,
) {
    benchmark_background(cx, Some(kind), None, false, &format!("{kind:?}"));
}

fn benchmark_background(
    cx: &mut BenchAppContext<'_, '_>,
    kind: Option<oxideterm_gpui_background::GeneratedEffectKind>,
    media: Option<std::path::PathBuf>,
    reading_overlay: bool,
    preview_name: &str,
) {
    let terminal = benchmark_terminal(cx, false);
    let composed = media.is_some();
    let video = media
        .as_ref()
        .is_some_and(|path| path.extension().is_some_and(|extension| extension == "mp4"));
    terminal.update(cx, |terminal, cx| {
        let theme = TerminalUiPreferences::default().theme;
        let effect = kind.map(
            |kind| oxideterm_gpui_background::GeneratedEffectPreferences {
                kind,
                strength: if composed { 0.35 } else { 0.65 },
                sheen: if composed { 0.0 } else { 1.0 },
                max_fps: None,
                colors: [0x8db7d8, 0x80cabc],
                speed: 1.0,
                size: 1.0,
                brightness: 0.6,
                roughness: 0.6,
                direction: 25.0,
                particle_count: 12,
                preview_viewport: None,
            },
        );
        let readability = reading_overlay.then_some(oxideterm_gpui_background::ReadingOverlay {
            color: theme.background,
            opacity: 0.18,
        });
        terminal.set_appearance(
            theme,
            Some(oxideterm_gpui_background::BackgroundPreferences {
                path: media.unwrap_or_default(),
                opacity: 0.65,
                blur: 0.0,
                fit: oxideterm_gpui_background::BackgroundFit::Cover,
                alignment: (0.5, 0.5),
                limits: Default::default(),
                on_failure: None,
                scene: Default::default(),
                readability,
                effect,
            }),
            false,
            cx,
        );
    });
    cx.settle();
    if composed {
        let started = Instant::now();
        let (window, mut app) = cx.update(|cx| {
            (
                cx.windows()
                    .last()
                    .copied()
                    .expect("media benchmark window"),
                cx.to_async(),
            )
        });
        cx.run_until(|| {
            let visible = app
                .update_window(window, |_, window, _| {
                    media_frame_visible(
                        &window.render_to_image().expect("capture media startup"),
                        video,
                    )
                })
                .unwrap();
            if visible {
                Some(())
            } else {
                assert!(
                    started.elapsed() < Duration::from_secs(5),
                    "native media first frame did not appear"
                );
                None
            }
        });
    }
    cx.bench_renderer(terminal, |_, _, cx| cx.notify());
    let directory = std::env::var_os("OXIDE_BACKGROUND_PREVIEW_DIRECTORY");
    if directory.is_some() || composed {
        cx.update(|cx| {
            let window = cx
                .windows()
                .last()
                .copied()
                .expect("background benchmark window");
            let image = cx
                .update_window(window, |_, window, _| window.render_to_image())
                .unwrap()
                .expect("capture native background renderer");
            if composed {
                assert!(
                    media_frame_visible(&image, video),
                    "media must remain visible beneath overlays"
                );
            }
            if let Some(directory) = directory {
                image
                    .save(std::path::PathBuf::from(directory).join(format!("{preview_name}.png")))
                    .expect("save background preview");
            }
        });
    }
}

fn media_frame_visible(image: &image::RgbaImage, video: bool) -> bool {
    let x = if video {
        image.width() * 4 / 5
    } else {
        image.width() * 19 / 20
    };
    let pixel = image.get_pixel(x, image.height() / 2);
    let red = u16::from(pixel[0]);
    let blue = u16::from(pixel[2]);
    if video {
        // The fixture alternates red and blue; either color proves media survived composition.
        (red > 80 && red > blue * 2) || (blue > 80 && blue > red * 2)
    } else {
        // The bundled image has an orange right edge; the effects alone cannot produce it.
        red > 70 && red * 10 > blue * 13
    }
}

#[gpui::bench(fps = 120)]
fn terminal_mineral_background(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_generated_background(cx, oxideterm_gpui_background::GeneratedEffectKind::Mineral);
}

#[gpui::bench(fps = 120)]
fn terminal_fog_background(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_generated_background(cx, oxideterm_gpui_background::GeneratedEffectKind::Fog);
}

#[gpui::bench(fps = 120)]
fn terminal_tide_background(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_generated_background(cx, oxideterm_gpui_background::GeneratedEffectKind::Tide);
}

#[gpui::bench(fps = 120)]
fn terminal_meteor_background(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_generated_background(cx, oxideterm_gpui_background::GeneratedEffectKind::Meteor);
}

fn benchmark_image_background(
    cx: &mut BenchAppContext<'_, '_>,
    kind: Option<oxideterm_gpui_background::GeneratedEffectKind>,
) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../oxideterm-gpui-app/resources/backgrounds/oxide-ambient-v1.png");
    let name = kind.map_or_else(|| "Image".into(), |kind| format!("Image-{kind:?}"));
    benchmark_background(cx, kind, Some(path), kind.is_some(), &name);
}

#[gpui::bench(fps = 120)]
fn terminal_image_background(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_image_background(cx, None);
}

#[gpui::bench(fps = 120)]
fn terminal_image_mineral_overlay(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_image_background(
        cx,
        Some(oxideterm_gpui_background::GeneratedEffectKind::Mineral),
    );
}

#[gpui::bench(fps = 120)]
fn terminal_image_fog_overlay(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_image_background(
        cx,
        Some(oxideterm_gpui_background::GeneratedEffectKind::Fog),
    );
}

#[gpui::bench(fps = 120)]
fn terminal_image_tide_overlay(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_image_background(
        cx,
        Some(oxideterm_gpui_background::GeneratedEffectKind::Tide),
    );
}

#[gpui::bench(fps = 120)]
fn terminal_image_meteor_overlay(cx: &mut BenchAppContext<'_, '_>) {
    benchmark_image_background(
        cx,
        Some(oxideterm_gpui_background::GeneratedEffectKind::Meteor),
    );
}

#[gpui::bench(fps = 120)]
fn terminal_video_fog_overlay(cx: &mut BenchAppContext<'_, '_>) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../oxideterm-background-media/tests/fixtures/red-blue.mp4");
    benchmark_background(
        cx,
        Some(oxideterm_gpui_background::GeneratedEffectKind::Fog),
        Some(path),
        true,
        "Video-Fog",
    );
}

fn benchmark_grid_size(
    cx: &BenchAppContext<'_, '_>,
    terminal: &Entity<TerminalPane>,
) -> (usize, usize) {
    let snapshot = cx.read(|app| terminal.read(app).ai_screen_snapshot());
    (snapshot.cols, snapshot.rows)
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

// Change eight rows in one terminal through the production shaping and glyph paint paths.
fn benchmark_internal_text(cx: &mut BenchAppContext<'_, '_>, sample: &str) {
    let terminal = benchmark_terminal(cx, true);
    let grid = benchmark_grid_size(cx, &terminal);
    let chunks = (0..256)
        .map(|index| {
            format!("{index:03} {sample}\r\n")
                .repeat(OUTPUT_LINES_PER_FRAME)
                .into_bytes()
        })
        .collect::<Vec<_>>();
    // Resolve this workload's fonts and first glyphs outside the measured loop;
    // otherwise italic/CJK font discovery is mixed into steady drawing costs.
    terminal.update(cx, |terminal, cx| {
        terminal.feed_recording_output(&chunks[0], cx);
    });
    cx.settle();
    let mut index = 0;
    let mut parse = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut snapshot = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut snapshot_state = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut links = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut layout = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut paint = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut previous = false;
    cx.bench_renderer(terminal.clone(), |terminal, _window, cx| {
        if previous {
            let (_, backend_us, state_us, layout_us, paint_us) =
                terminal.benchmark_render_stage_micros();
            snapshot.push(Duration::from_micros(backend_us));
            snapshot_state.push(Duration::from_micros(state_us));
            links.push(Duration::from_micros(
                terminal.benchmark_link_stage_micros(),
            ));
            layout.push(Duration::from_micros(layout_us));
            paint.push(Duration::from_micros(paint_us));
        }
        previous = true;
        parse.push(
            terminal
                .feed_recording_output_profiled(&chunks[index], cx)
                .parse_and_grid,
        );
        index = (index + 1) % chunks.len();
    });
    assert_eq!(benchmark_grid_size(cx, &terminal), grid);
    cx.record_stage_samples("parse and grid", parse);
    cx.record_stage_samples("backend snapshot", snapshot);
    cx.record_stage_samples("snapshot pane state", snapshot_state);
    cx.record_stage_samples("links within line layout", links);
    cx.record_stage_samples("line layout", layout);
    cx.record_stage_samples("scene construction", paint);
}

fn benchmark_native_text(
    criterion: &mut criterion::Criterion<gpui::BenchMeasurement>,
    name: &'static str,
    benchmark: impl Fn(&mut BenchAppContext<'_, '_>),
) {
    // Windows headless platforms use NoopTextSystem. Acquire DirectWrite from a
    // native platform, but keep benchmark windows on TestPlatform (no visible UI).
    let native = gpui_platform::current_platform(!cfg!(target_os = "windows"));
    let text_system = native.text_system();
    assert!(
        !text_system.all_font_names().is_empty(),
        "requires native fonts"
    );
    let report = gpui::BenchReport::with_fps(120);
    criterion.bench_function(name, |bencher| {
        let mut cx = BenchAppContext::new_with_platform_and_report(
            gpui::bench_platform(
                Some(Box::new(gpui_platform::current_headless_renderer)),
                text_system.clone(),
            ),
            Some(name),
            bencher,
            report.clone(),
        );
        benchmark(&mut cx);
        cx.teardown();
    });
    report.print(name);
}

fn terminal_internal_plain(criterion: &mut criterion::Criterion<gpui::BenchMeasurement>) {
    benchmark_native_text(criterion, "terminal_internal_plain", |cx| {
        benchmark_internal_text(
            cx,
            "The quick brown fox jumps over the lazy dog. cargo check completed successfully.",
        );
    });
}

fn terminal_internal_styled(criterion: &mut criterion::Criterion<gpui::BenchMeasurement>) {
    benchmark_native_text(criterion, "terminal_internal_styled", |cx| {
        benchmark_internal_text(
            cx,
            "\x1b[1;31mERROR\x1b[0m src/main.rs:42 \x1b[3;4;32mrender pipeline\x1b[0m \x1b[9mold value\x1b[0m -> \x1b[38;5;117mnew value\x1b[0m",
        );
    });
}

fn terminal_internal_unicode(criterion: &mut criterion::Criterion<gpui::BenchMeasurement>) {
    benchmark_native_text(criterion, "terminal_internal_unicode", |cx| {
        benchmark_internal_text(
            cx,
            "终端绘制 中文字体 fallback e\u{301} 日本語 한글 مرحبا بالعالم ABC 123",
        );
    });
}

fn terminal_internal_semantic_warm(criterion: &mut criterion::Criterion<gpui::BenchMeasurement>) {
    benchmark_native_text(criterion, "terminal_internal_semantic_warm", |cx| {
        benchmark_semantic_history(cx, 200, true, true, None);
    });
}

fn terminal_internal_warm(criterion: &mut criterion::Criterion<gpui::BenchMeasurement>) {
    benchmark_native_text(criterion, "terminal_internal_warm", |cx| {
        let terminal = benchmark_terminal(cx, true);
        let grid = benchmark_grid_size(cx, &terminal);
        let mut layout = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
        let mut scene = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
        let mut previous = false;
        cx.bench_renderer(terminal.clone(), |terminal, _window, cx| {
            if previous {
                let (_, _, _, layout_us, scene_us) = terminal.benchmark_render_stage_micros();
                layout.push(Duration::from_micros(layout_us));
                scene.push(Duration::from_micros(scene_us));
            }
            previous = true;
            cx.notify();
        });
        assert_eq!(benchmark_grid_size(cx, &terminal), grid);
        cx.record_stage_samples("warm line layout", layout);
        cx.record_stage_samples("warm scene construction", scene);
    });
}

#[derive(Clone, Copy)]
enum IncrementalWorkload {
    Input,
    Selection,
    Scroll,
    Sparse,
}

fn benchmark_incremental(cx: &mut BenchAppContext<'_, '_>, workload: IncrementalWorkload) {
    let terminal = benchmark_terminal(cx, true);
    let grid = benchmark_grid_size(cx, &terminal);
    let word = "alpha beta gamma delta ";
    let prefix = word.repeat(grid.0.saturating_sub(16) / word.len());
    assert!(prefix.len() >= word.len());
    let mut chunks = Vec::new();
    if !matches!(workload, IncrementalWorkload::Scroll) {
        let mut setup = String::from("\x1b[2J\x1b[H\x1b[?25l");
        let rows = if matches!(workload, IncrementalWorkload::Selection) {
            grid.1
        } else {
            1
        };
        for row in 0..rows {
            setup.push_str(&format!("\x1b[{};1H{prefix}00000000", row + 1));
        }
        setup.push_str("\x1b[H");
        terminal.update(cx, |terminal, cx| {
            terminal.set_selection_highlighting_override(Some(false), cx);
            terminal.feed_recording_output(setup.as_bytes(), cx);
        });
        // More distinct revisions than the row cache can retain prevent the
        // input fixture from becoming a two-state warm-cache redraw benchmark.
        for index in 0..2048 {
            let output = if matches!(workload, IncrementalWorkload::Sparse) {
                (0..grid.1)
                    .map(|row| format!("\x1b[{};1H\x1b[2K{:08}", row + 1, index * grid.1 + row))
                    .collect::<String>()
            } else {
                format!("\x1b[1;{}H{index:08}", prefix.len() + 1)
            };
            chunks.push(output.into_bytes());
        }
    } else {
        let offset = terminal.update(cx, |terminal, cx| terminal.benchmark_scroll_pixels(64., cx));
        assert!(
            offset > 0,
            "precise scroll fixture must start inside history"
        );
    }
    cx.settle();
    let mut index = 0;
    let mut previous = false;
    let mut layout = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut paint = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    cx.bench_renderer(terminal.clone(), |terminal, _, cx| {
        if previous {
            let (_, _, _, layout_us, paint_us) = terminal.benchmark_render_stage_micros();
            layout.push(Duration::from_micros(layout_us));
            paint.push(Duration::from_micros(paint_us));
        }
        previous = true;
        match workload {
            IncrementalWorkload::Input | IncrementalWorkload::Sparse => {
                terminal.feed_recording_output(&chunks[index % chunks.len()], cx)
            }
            IncrementalWorkload::Selection => {
                let head = index % (grid.0 * grid.1);
                terminal.benchmark_select_cells(head / grid.0, head % grid.0, cx);
            }
            IncrementalWorkload::Scroll => {
                let delta = if index % 1024 < 512 { 0.5 } else { -0.5 };
                terminal.benchmark_scroll_pixels(delta, cx);
            }
        }
        index += 1;
    });
    assert_eq!(benchmark_grid_size(cx, &terminal), grid);
    cx.record_stage_samples("line layout", layout);
    cx.record_stage_samples("scene construction", paint);
}

fn terminal_incremental_input(c: &mut criterion::Criterion<gpui::BenchMeasurement>) {
    benchmark_native_text(c, "terminal_incremental_input", |cx| {
        benchmark_incremental(cx, IncrementalWorkload::Input)
    });
}
fn terminal_incremental_selection(c: &mut criterion::Criterion<gpui::BenchMeasurement>) {
    benchmark_native_text(c, "terminal_incremental_selection", |cx| {
        benchmark_incremental(cx, IncrementalWorkload::Selection)
    });
}
fn terminal_incremental_scroll(c: &mut criterion::Criterion<gpui::BenchMeasurement>) {
    benchmark_native_text(c, "terminal_incremental_scroll", |cx| {
        benchmark_incremental(cx, IncrementalWorkload::Scroll)
    });
}
fn terminal_incremental_sparse(c: &mut criterion::Criterion<gpui::BenchMeasurement>) {
    benchmark_native_text(c, "terminal_incremental_sparse", |cx| {
        benchmark_incremental(cx, IncrementalWorkload::Sparse)
    });
}
fn terminal_incremental_semantic_warm(c: &mut criterion::Criterion<gpui::BenchMeasurement>) {
    benchmark_native_text(c, "terminal_incremental_semantic_warm", |cx| {
        benchmark_semantic_history(cx, 2000, true, true, None)
    });
}
fn terminal_incremental_semantic_stream(c: &mut criterion::Criterion<gpui::BenchMeasurement>) {
    benchmark_native_text(c, "terminal_incremental_semantic_stream", |cx| {
        benchmark_semantic_history(cx, 2000, true, false, None)
    });
}

fn benchmark_decoration_redraw(cx: &mut BenchAppContext<'_, '_>, decorated: bool) {
    let terminal = benchmark_terminal(cx, true);
    let grid = benchmark_grid_size(cx, &terminal);
    let style = if decorated { "\x1b[4;9m" } else { "\x1b[0m" };
    let content =
        format!("{style}render pipeline 0123456789 abcdefghijklmnopqrstuvwxyz\x1b[0m\r\n")
            .repeat(grid.1);
    terminal.update(cx, |terminal, cx| {
        terminal.feed_recording_output(content.as_bytes(), cx);
    });
    cx.settle();
    let mut paint = Vec::with_capacity(PROFILE_SAMPLE_CAPACITY);
    let mut previous = false;
    cx.bench_renderer(terminal.clone(), |terminal, _window, cx| {
        if previous {
            let (_, _, _, _, paint_us) = terminal.benchmark_render_stage_micros();
            paint.push(Duration::from_micros(paint_us));
        }
        previous = true;
        cx.notify();
    });
    assert_eq!(benchmark_grid_size(cx, &terminal), grid);
    cx.record_stage_samples("scene construction", paint);
}

fn terminal_internal_decorated_warm(criterion: &mut criterion::Criterion<gpui::BenchMeasurement>) {
    benchmark_native_text(criterion, "terminal_internal_decorated_warm", |cx| {
        benchmark_decoration_redraw(cx, true);
    });
}

fn terminal_internal_undecorated_warm(
    criterion: &mut criterion::Criterion<gpui::BenchMeasurement>,
) {
    benchmark_native_text(criterion, "terminal_internal_undecorated_warm", |cx| {
        benchmark_decoration_redraw(cx, false);
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
    terminal_internal_semantic_warm,
    terminal_incremental_input,
    terminal_incremental_selection,
    terminal_incremental_scroll,
    terminal_incremental_sparse,
    terminal_incremental_semantic_warm,
    terminal_incremental_semantic_stream,
    terminal_internal_plain,
    terminal_internal_styled,
    terminal_internal_unicode,
    terminal_internal_warm,
    terminal_internal_decorated_warm,
    terminal_internal_undecorated_warm,
    terminal_tab_switch_frame,
    semantic_history_stream_small,
    semantic_history_stream_2000,
    semantic_history_warm_small,
    semantic_history_warm_2000,
    semantic_history_disabled_2000,
    semantic_history_stream_2000_untrimmed,
    terminal_semantic_output_disabled,
    terminal_semantic_output_enabled,
    terminal_warm_cache_redraw_frame,
    terminal_mineral_background,
    terminal_fog_background,
    terminal_tide_background,
    terminal_meteor_background,
    terminal_image_background,
    terminal_image_mineral_overlay,
    terminal_image_fog_overlay,
    terminal_image_tide_overlay,
    terminal_image_meteor_overlay,
    terminal_video_fog_overlay,
    terminal_box_drawing_redraw_frame,
    terminal_playback_output_frame,
    terminal_playback_output_pipeline,
    terminal_idle_no_frames
);
gpui::bench_main!(allocator = none; terminal_render);
