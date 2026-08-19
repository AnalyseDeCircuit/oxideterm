use criterion::{BatchSize, Criterion, Throughput, black_box, criterion_group, criterion_main};
use oxideterm_terminal::{GraphicsOptions, TerminalSession};

const BENCHMARK_ROWS: usize = 40;
const BENCHMARK_COLS: usize = 120;
const BENCHMARK_SCROLL_DELTA: i32 = 1;

fn terminal_corpus(lines: usize) -> Vec<u8> {
    let mut corpus = Vec::with_capacity(lines * 96);
    for line in 0..lines {
        corpus.extend_from_slice(
            format!(
                "\x1b[38;5;{}moxideterm benchmark line {line} cargo check\x1b[0m\r\n",
                line % 256
            )
            .as_bytes(),
        );
    }
    corpus
}

fn populated_terminal(lines: usize) -> TerminalSession {
    let mut terminal = TerminalSession::recording_playback(
        BENCHMARK_COLS,
        BENCHMARK_ROWS,
        GraphicsOptions::default(),
        20_000,
    );
    terminal.feed_recording_output(&terminal_corpus(lines));
    terminal
}

fn benchmark_terminal_pipeline(criterion: &mut Criterion) {
    let corpus = terminal_corpus(5_000);
    let mut throughput = criterion.benchmark_group("terminal_stream");
    throughput.throughput(Throughput::Bytes(corpus.len() as u64));
    throughput.bench_function("parse_5000_lines", |bencher| {
        bencher.iter_batched(
            || {
                TerminalSession::recording_playback(
                    BENCHMARK_COLS,
                    BENCHMARK_ROWS,
                    GraphicsOptions::default(),
                    20_000,
                )
            },
            |mut terminal| terminal.feed_recording_output(black_box(&corpus)),
            BatchSize::SmallInput,
        );
    });
    throughput.finish();

    let terminal = populated_terminal(20_000);
    let previous_snapshot = terminal.snapshot();
    criterion.bench_function("snapshot_120x40", |bencher| {
        bencher.iter(|| black_box(terminal.snapshot()));
    });
    criterion.bench_function("snapshot_incremental_unchanged_120x40", |bencher| {
        bencher.iter(|| black_box(terminal.snapshot_incremental(black_box(&previous_snapshot))));
    });
    let mut full_scroll_terminal = populated_terminal(20_000);
    let mut full_scroll_delta = BENCHMARK_SCROLL_DELTA;
    criterion.bench_function("snapshot_scroll_full_120x40", |bencher| {
        bencher.iter(|| {
            full_scroll_terminal.scroll_lines(full_scroll_delta);
            let snapshot = full_scroll_terminal.snapshot();
            full_scroll_delta = if snapshot.display_offset == 0 {
                BENCHMARK_SCROLL_DELTA
            } else {
                -BENCHMARK_SCROLL_DELTA
            };
            black_box(snapshot)
        });
    });
    let mut incremental_scroll_terminal = populated_terminal(20_000);
    let mut incremental_scroll_snapshot = incremental_scroll_terminal.snapshot();
    let mut incremental_scroll_delta = BENCHMARK_SCROLL_DELTA;
    criterion.bench_function("snapshot_scroll_incremental_120x40", |bencher| {
        bencher.iter(|| {
            let snapshot = incremental_scroll_terminal.scroll_lines_snapshot_incremental(
                incremental_scroll_delta,
                black_box(&incremental_scroll_snapshot),
            );
            incremental_scroll_delta = if snapshot.display_offset == 0 {
                BENCHMARK_SCROLL_DELTA
            } else {
                -BENCHMARK_SCROLL_DELTA
            };
            incremental_scroll_snapshot = snapshot;
            black_box(incremental_scroll_snapshot.display_offset)
        });
    });
    let search_source = terminal
        .search_source()
        .expect("recording playback sessions expose a background search source");
    criterion.bench_function("search_chunked_20000_lines", |bencher| {
        bencher.iter(|| black_box(search_source.search_matches(black_box("cargo"), &|| false)));
    });
}

criterion_group!(benches, benchmark_terminal_pipeline);
criterion_main!(benches);
