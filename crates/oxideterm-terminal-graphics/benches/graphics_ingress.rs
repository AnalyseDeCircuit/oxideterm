// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::hint::black_box;

use alacritty_terminal::{
    event::VoidListener,
    grid::Dimensions,
    index::Line,
    term::{Config, Term},
    vte::ansi::{Processor, StdSyncHandler},
};
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use oxideterm_terminal_graphics::{
    GraphicsCursor, GraphicsIngress, GraphicsOptions, TerminalGraphicsSegment,
};

const PAYLOAD_BYTES: usize = 1024 * 1024;

struct GridSize;

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        40
    }
    fn screen_lines(&self) -> usize {
        40
    }
    fn columns(&self) -> usize {
        120
    }
}

fn benchmark_cursor() -> GraphicsCursor {
    GraphicsCursor {
        line: 0,
        row: 0,
        col: 0,
        cols: 120,
        rows: 40,
        cell_width: 10,
        cell_height: 20,
    }
}

fn repeated_payload(pattern: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(PAYLOAD_BYTES + pattern.len());
    while payload.len() < PAYLOAD_BYTES {
        payload.extend_from_slice(pattern);
    }
    payload.truncate(PAYLOAD_BYTES);
    payload
}

fn benchmark_graphics_ingress(criterion: &mut Criterion) {
    let workloads = [
        (
            "plain",
            repeated_payload(b"OxideTerm graphics ingress plain text 0123456789\r\n"),
        ),
        (
            "ansi",
            repeated_payload(b"\x1b[1;38;2;72;183;255mOxideTerm ANSI workload\x1b[0m\r\n"),
        ),
        (
            "unicode",
            repeated_payload(
                "OxideTerm Unicode workload: 中文 日本語 한국어 Δοκιμή 🚀\r\n".as_bytes(),
            ),
        ),
        (
            "long-csi",
            repeated_payload(
                b"\x1b[1;2;3;4;5;7;8;9;22;23;24;25;27;28;29;38;2;72;183;255mworkload\x1b[0m\r\n",
            ),
        ),
        (
            "osc-title",
            repeated_payload(format!("\x1b]2;{}\x07", "terminal title ".repeat(128)).as_bytes()),
        ),
        (
            "osc-links",
            repeated_payload(b"\x1b]8;id=build;https://example.com/builds/123456789/logs\x1b\\linked output\x1b]8;;\x1b\\\r\n"),
        ),
    ];
    let mut group = criterion.benchmark_group("graphics_ingress");
    group.throughput(Throughput::Bytes(PAYLOAD_BYTES as u64));

    for (name, payload) in &workloads {
        group.bench_with_input(
            BenchmarkId::new("ordered", name),
            payload,
            |bencher, payload| {
                bencher.iter(|| {
                    let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
                    let mut terminal_bytes = 0usize;
                    let mut graphics_events = 0usize;
                    ingress.advance_ordered(
                        black_box(payload),
                        |segment| match segment {
                            TerminalGraphicsSegment::Terminal(bytes) => {
                                terminal_bytes += black_box(bytes).len();
                            }
                            TerminalGraphicsSegment::Event(event) => {
                                black_box(event);
                                graphics_events += 1;
                            }
                        },
                        benchmark_cursor,
                    );
                    black_box((terminal_bytes, graphics_events));
                });
            },
        );
    }

    group.finish();

    // Disabled ingress is a stricter reference than the old scanner: it skips
    // graphics scanning entirely, while both variants execute the real ANSI grid.
    let mut group = criterion.benchmark_group("terminal_output");
    group.throughput(Throughput::Bytes(PAYLOAD_BYTES as u64));
    for (name, payload) in &workloads {
        for enabled in [false, true] {
            group.bench_with_input(
                BenchmarkId::new(if enabled { "graphics" } else { "direct" }, name),
                payload,
                |bencher, payload| {
                    bencher.iter_batched(
                        || {
                            let mut term = Term::new(
                                Config {
                                    scrolling_history: 1000,
                                    ..Config::default()
                                },
                                &GridSize,
                                VoidListener,
                            );
                            let mut ingress = GraphicsIngress::new(GraphicsOptions {
                                enabled,
                                ..GraphicsOptions::default()
                            });
                            ingress.attach_grid(term.graphics_events());
                            (
                                std::cell::RefCell::new(term),
                                Processor::<StdSyncHandler>::new(),
                                ingress,
                            )
                        },
                        |(term, mut parser, mut ingress)| {
                            for chunk in payload.chunks(32 * 1024) {
                                ingress.advance_ordered(
                                    black_box(chunk),
                                    |segment| {
                                        if let TerminalGraphicsSegment::Terminal(bytes) = segment {
                                            parser.advance(&mut *term.borrow_mut(), &bytes);
                                        }
                                    },
                                    || {
                                        let term = term.borrow();
                                        term.capture_graphics_placeholders();
                                        term.capture_sixel_background();
                                        benchmark_cursor()
                                    },
                                );
                            }
                            black_box(
                                term.borrow().grid()[Line(0)]
                                    .cell(alacritty_terminal::index::Column(0))
                                    .c,
                            );
                        },
                        BatchSize::SmallInput,
                    );
                },
            );
        }
    }
    group.finish();

    // Omit raster dimensions to exercise the decoder's growing-canvas path.
    let mut sixel = b"\x1bP9;1q#1;2;100;0;0".to_vec();
    for _ in 0..100 {
        sixel.extend_from_slice(b"!800~-");
    }
    sixel.extend_from_slice(b"\x1b\\");
    criterion.bench_function("sixel/800x600", |bencher| {
        bencher.iter(|| {
            let mut ingress = GraphicsIngress::new(GraphicsOptions::default());
            black_box(ingress.advance(black_box(&sixel), benchmark_cursor()));
        });
    });
}

criterion_group!(benches, benchmark_graphics_ingress);
criterion_main!(benches);
