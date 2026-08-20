// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use oxideterm_terminal_graphics::{
    GraphicsCursor, GraphicsIngress, GraphicsOptions, TerminalGraphicsSegment,
};

const PAYLOAD_BYTES: usize = 1024 * 1024;

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
}

criterion_group!(benches, benchmark_graphics_ingress);
criterion_main!(benches);
