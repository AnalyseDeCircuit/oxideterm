use std::time::{Duration, Instant};

use alacritty_terminal::{
    event::VoidListener,
    grid::Dimensions,
    term::{Config, Term},
    vte::{
        Params, Parser, Perform,
        ansi::{Processor, StdSyncHandler},
    },
};
use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use oxideterm_terminal::{GraphicsOptions, TerminalSession};

const BENCHMARK_ROWS: usize = 40;
const BENCHMARK_COLS: usize = 120;
const BENCHMARK_SCROLL_DELTA: i32 = 1;
const INPUT_CORPUS_BYTES: usize = 256 * 1024;
const CRLF_BENCHMARK_LINE_BYTES: usize = 64;

#[derive(Clone, Copy)]
struct BenchmarkSize;

impl Dimensions for BenchmarkSize {
    fn total_lines(&self) -> usize {
        BENCHMARK_ROWS
    }

    fn screen_lines(&self) -> usize {
        BENCHMARK_ROWS
    }

    fn columns(&self) -> usize {
        BENCHMARK_COLS
    }
}

#[derive(Default)]
struct ParserActionCount {
    actions: usize,
}

impl ParserActionCount {
    fn record(&mut self) {
        self.actions += 1;
    }
}

impl Perform for ParserActionCount {
    fn print(&mut self, _character: char) {
        self.record();
    }

    fn print_text(&mut self, text: &str) {
        // Byte progress keeps batch callbacks observable without reintroducing scalar decoding.
        self.actions += text.len();
    }

    fn print_text_lines(&mut self, text: &str, _line_count: usize, _max_line_length: usize) {
        self.actions += text.len();
    }

    fn execute(&mut self, _byte: u8) {
        self.record();
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {
        self.record();
    }

    fn put(&mut self, _byte: u8) {
        self.record();
    }

    fn unhook(&mut self) {
        self.record();
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {
        self.record();
    }

    fn csi_dispatch(
        &mut self,
        _params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
        self.record();
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {
        self.record();
    }
}

fn repeated_input(pattern: &[u8]) -> Vec<u8> {
    let repetitions = INPUT_CORPUS_BYTES.div_ceil(pattern.len());
    let mut corpus = Vec::with_capacity(repetitions * pattern.len());
    for _ in 0..repetitions {
        corpus.extend_from_slice(pattern);
    }
    corpus
}

fn fixed_size_input(pattern: &[u8]) -> Vec<u8> {
    let mut corpus = repeated_input(pattern);
    corpus.truncate(INPUT_CORPUS_BYTES);
    corpus
}

fn crlf_ascii_input() -> Vec<u8> {
    let mut corpus = vec![b'x'; INPUT_CORPUS_BYTES];
    for line_end in (CRLF_BENCHMARK_LINE_BYTES..=corpus.len()).step_by(CRLF_BENCHMARK_LINE_BYTES) {
        corpus[line_end - 2] = b'\r';
        corpus[line_end - 1] = b'\n';
    }
    corpus
}

fn terminal_input_corpora() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (
            "plain",
            repeated_input(
                b"oxideterm benchmark plain output cargo check completed successfully 0123456789\r\n",
            ),
        ),
        (
            "ansi",
            repeated_input(
                b"\x1b[38;5;42moxideterm benchmark colored output\x1b[0m cargo check\r\n",
            ),
        ),
        (
            "unicode",
            repeated_input("OxideTerm 中文输出 e\u{301} Rust 🦀 终端基准测试\r\n".as_bytes()),
        ),
        (
            "long-csi",
            repeated_input(b"\x1b[1;2;3;4;5;7;8;9;22;23;24;25;27;28;29;38;5;42mX\x1b[0m"),
        ),
        ("wrapped-ascii", fixed_size_input(b"x")),
        ("crlf-ascii", crlf_ascii_input()),
    ]
}

fn benchmark_term() -> Term<VoidListener> {
    let mut config = Config::default();
    config.scrolling_history = 20_000;
    Term::new(config, &BenchmarkSize, VoidListener)
}

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
    let mut output_scroll_terminal = populated_terminal(20_000);
    let mut output_scroll_snapshot = output_scroll_terminal.snapshot();
    criterion.bench_function("snapshot_output_scroll_incremental_120x40", |bencher| {
        bencher.iter_custom(|iterations| {
            let mut measured = Duration::ZERO;
            for _ in 0..iterations {
                output_scroll_terminal.feed_recording_output(black_box(b"next line\r\n"));
                let started = Instant::now();
                output_scroll_snapshot =
                    output_scroll_terminal.snapshot_incremental(black_box(&output_scroll_snapshot));
                measured += started.elapsed();
            }
            black_box(output_scroll_snapshot.generation);
            measured
        });
    });
    let search_source = terminal
        .search_source()
        .expect("recording playback sessions expose a background search source");
    criterion.bench_function("search_chunked_20000_lines", |bencher| {
        bencher.iter(|| black_box(search_source.search_matches(black_box("cargo"), &|| false)));
    });
}

fn benchmark_terminal_input_breakdown(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("terminal_input_breakdown");

    for (name, corpus) in terminal_input_corpora() {
        group.throughput(Throughput::Bytes(corpus.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("parser", name),
            &corpus,
            |bencher, corpus| {
                bencher.iter_batched(
                    Parser::new,
                    |mut parser| {
                        let mut performer = ParserActionCount::default();
                        parser.advance(&mut performer, black_box(corpus));
                        black_box(performer.actions)
                    },
                    BatchSize::SmallInput,
                );
            },
        );
        group.bench_with_input(
            BenchmarkId::new("grid", name),
            &corpus,
            |bencher, corpus| {
                bencher.iter_batched(
                    || (Processor::<StdSyncHandler>::new(), benchmark_term()),
                    |(mut parser, mut term)| {
                        parser.advance(&mut term, black_box(corpus));
                        black_box(term)
                    },
                    BatchSize::SmallInput,
                );
            },
        );
        if matches!(name, "plain" | "crlf-ascii") {
            let mut scalar_dispatch_corpus = corpus.clone();
            scalar_dispatch_corpus.push(b'x');
            group.bench_with_input(
                BenchmarkId::new("grid-scalar-dispatch", name),
                &scalar_dispatch_corpus,
                |bencher, corpus| {
                    bencher.iter_batched(
                        || (Processor::<StdSyncHandler>::new(), benchmark_term()),
                        |(mut parser, mut term)| {
                            parser.advance(&mut term, black_box(corpus));
                            black_box(term)
                        },
                        BatchSize::SmallInput,
                    );
                },
            );
        }
        group.bench_with_input(
            BenchmarkId::new("pipeline", name),
            &corpus,
            |bencher, corpus| {
                bencher.iter_batched(
                    || {
                        TerminalSession::recording_playback(
                            BENCHMARK_COLS,
                            BENCHMARK_ROWS,
                            GraphicsOptions::default(),
                            20_000,
                        )
                    },
                    |mut terminal| {
                        terminal.feed_recording_output(black_box(corpus));
                        black_box(terminal)
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn snapshot_benchmark_session() -> TerminalSession {
    let mut terminal = TerminalSession::recording_playback(
        BENCHMARK_COLS,
        BENCHMARK_ROWS,
        GraphicsOptions::default(),
        20_000,
    );
    terminal.feed_recording_output(&terminal_corpus(5_000));
    terminal
}

fn benchmark_terminal_snapshots(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("terminal_snapshots");
    group.throughput(Throughput::Elements(
        (BENCHMARK_ROWS * BENCHMARK_COLS) as u64,
    ));

    group.bench_function("full_visible_grid", |bencher| {
        let terminal = snapshot_benchmark_session();
        bencher.iter(|| black_box(terminal.snapshot()));
    });

    group.bench_function("incremental_unchanged", |bencher| {
        let terminal = snapshot_benchmark_session();
        let previous = terminal.snapshot();
        let previous = terminal.snapshot_incremental(&previous);
        bencher.iter(|| black_box(terminal.snapshot_incremental(&previous)));
    });

    group.bench_function("incremental_one_row", |bencher| {
        let mut terminal = snapshot_benchmark_session();
        let previous = terminal.snapshot();
        let mut previous = terminal.snapshot_incremental(&previous);
        let mut revision = 0u64;
        bencher.iter_custom(|iterations| {
            let mut measured = Duration::ZERO;
            for _ in 0..iterations {
                let row = format!(
                    "\rupdated terminal snapshot row revision {revision:08} with ASCII output"
                );
                terminal.feed_recording_output(black_box(row.as_bytes()));
                revision = revision.wrapping_add(1);
                let started = Instant::now();
                previous = black_box(terminal.snapshot_incremental(&previous));
                measured += started.elapsed();
            }
            measured
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_terminal_pipeline,
    benchmark_terminal_input_breakdown,
    benchmark_terminal_snapshots
);
criterion_main!(benches);
