// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use oxideterm_terminal_semantic::{
    SemanticLineRole, classify_line, semantic_output_role_for_command,
};

const LINES_PER_ITERATION: u64 = 1_024;

fn benchmark_line(
    criterion: &mut Criterion,
    name: &str,
    line: &'static str,
    role: SemanticLineRole,
) {
    let mut group = criterion.benchmark_group(name);
    group.throughput(Throughput::Elements(LINES_PER_ITERATION));
    group.bench_function("classify", |bencher| {
        bencher.iter(|| {
            for _ in 0..LINES_PER_ITERATION {
                black_box(classify_line(black_box(line), black_box(role)));
            }
        });
    });
    group.finish();
}

fn benchmark_semantic_classifier(criterion: &mut Criterion) {
    benchmark_line(
        criterion,
        "semantic_classifier/hash_line",
        "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  artifact.tar",
        SemanticLineRole::Output,
    );
    benchmark_line(
        criterion,
        "semantic_classifier/container_line",
        "d7a8f90b1234 nginx:latest Up 5 minutes 80/tcp web",
        SemanticLineRole::ContainerOutput,
    );
    benchmark_line(
        criterion,
        "semantic_classifier/session_id_line",
        "Session ID: 01a0ad3f-3b2d-7c72-9518-8ff3f1cb012a",
        SemanticLineRole::Output,
    );
    benchmark_line(
        criterion,
        "semantic_classifier/identifier_like_line",
        "worker-thread-name request-id=01234567-89ab-cdef-0123-456789abcdeg build-id=12345678-1234-1234-1234-1234567890123",
        SemanticLineRole::Output,
    );
    benchmark_line(
        criterion,
        "semantic_classifier/ordinary_long_line",
        "application worker completed request processing successfully with no permission field near the beginning and enough trailing output to expose accidental full-line scans",
        SemanticLineRole::Output,
    );
    benchmark_line(
        criterion,
        "semantic_classifier/unix_permission_line",
        "-rwsr-xr--+ 1 alice developers 16384 Sep 4 12:30 executable-with-capabilities",
        SemanticLineRole::Output,
    );
    benchmark_line(
        criterion,
        "semantic_classifier/file_metadata_line",
        "lrwxr-xr-x@ 1 alice staff 11K Sep 4 12:30 current -> /opt/app",
        SemanticLineRole::FileListingOutput,
    );
    benchmark_line(
        criterion,
        "semantic_classifier/resource_usage_line",
        "/dev/nvme0n1p2 100G 91G 9G 91% /",
        SemanticLineRole::DiskUsageOutput,
    );
    benchmark_line(
        criterion,
        "semantic_classifier/network_diagnostic_line",
        "64 bytes from 1.1.1.1: icmp_seq=1 ttl=57 time=12.4 ms",
        SemanticLineRole::PingOutput,
    );

    let mut group = criterion.benchmark_group("semantic_classifier/command_role");
    group.throughput(Throughput::Elements(LINES_PER_ITERATION));
    for (name, command) in [
        ("ordinary", "printf '%s\\n' ready"),
        ("file_listing", "ls -lah /var/log"),
        ("resource_usage", "df -h"),
        ("network", "ip address show"),
    ] {
        group.bench_function(name, |bencher| {
            bencher.iter(|| {
                for _ in 0..LINES_PER_ITERATION {
                    black_box(semantic_output_role_for_command(black_box(command)));
                }
            });
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = benchmark_semantic_classifier
}
criterion_main!(benches);
