// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use oxideterm_modem_transfer::{ModemConsumer, ModemConsumerEvent};

const PAYLOAD_BYTES: usize = 1024 * 1024;
const CHUNK_BYTES: usize = 8 * 1024;

fn repeated_payload(pattern: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(PAYLOAD_BYTES + pattern.len());
    while payload.len() < PAYLOAD_BYTES {
        payload.extend_from_slice(pattern);
    }
    payload.truncate(PAYLOAD_BYTES);
    payload
}

fn benchmark_modem_consumer(criterion: &mut Criterion) {
    let workloads = [
        (
            "plain",
            repeated_payload(b"OxideTerm modem consumer plain text 0123456789\r\n"),
        ),
        (
            "ansi",
            repeated_payload(b"\x1b[1;38;2;72;183;255mOxideTerm ANSI workload\x1b[0m\r\n"),
        ),
        (
            "candidate-heavy",
            repeated_payload(b"CACHE CONFIG CHECK COMPLETE\r\n"),
        ),
    ];
    let mut group = criterion.benchmark_group("modem_consumer");
    group.throughput(Throughput::Bytes(PAYLOAD_BYTES as u64));

    for (name, payload) in &workloads {
        group.bench_with_input(
            BenchmarkId::new("ordinary-output", name),
            payload,
            |bencher, payload| {
                bencher.iter(|| {
                    let mut consumer = ModemConsumer::new();
                    let mut terminal_bytes = 0usize;
                    for chunk in payload.chunks(CHUNK_BYTES) {
                        for event in consumer.process_server_output(black_box(chunk)) {
                            if let ModemConsumerEvent::WriteTerminal(bytes) = event {
                                terminal_bytes += black_box(bytes).len();
                            }
                        }
                    }
                    black_box(terminal_bytes);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_modem_consumer);
criterion_main!(benches);
