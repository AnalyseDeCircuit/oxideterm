use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use oxideterm_terminal_encoding::{
    EncodingMismatchDetector, TerminalEncoding, TerminalOutputDecoder,
};
use std::time::Duration;

// Include the private scanner directly without expanding the production API.
#[allow(unused_imports)]
#[path = "../src/privilege_prompt.rs"]
mod privilege_prompt;

use privilege_prompt::TerminalPrivilegePromptStream;

const BENCHMARK_CORPUS_BYTES: usize = 1024 * 1024;
const TERMINAL_OUTPUT_CHUNK_BYTES: usize = 8 * 1024;

fn repeated_corpus(line: &[u8]) -> Vec<u8> {
    let mut corpus = Vec::with_capacity(BENCHMARK_CORPUS_BYTES + line.len());
    while corpus.len() < BENCHMARK_CORPUS_BYTES {
        corpus.extend_from_slice(line);
    }
    corpus
}

fn benchmark_privilege_prompt_stream(criterion: &mut Criterion) {
    let workloads = [
        (
            "plain",
            repeated_corpus(b"OxideTerm prompt scanner plain text 0123456789\r\n"),
        ),
        (
            "ansi",
            repeated_corpus(b"\x1b[1;38;2;72;183;255mOxideTerm ANSI workload\x1b[0m\r\n"),
        ),
        (
            "unicode",
            repeated_corpus(
                "OxideTerm Unicode workload: 中文 日本語 한국어 Δοκιμή 🚀\r\n".as_bytes(),
            ),
        ),
        (
            "colon-heavy",
            repeated_corpus(b"time:12:34:56 level:info module:terminal message:healthy\r\n"),
        ),
        ("long-line", vec![b'x'; BENCHMARK_CORPUS_BYTES]),
        (
            "osc-title",
            repeated_corpus(format!("\x1b]2;{}\x07", "terminal title ".repeat(128)).as_bytes()),
        ),
        (
            "osc-links",
            repeated_corpus(b"\x1b]8;;https://example.com/builds/123456789/logs\x1b\\linked output\x1b]8;;\x1b\\\r\n"),
        ),
    ];

    let mut group = criterion.benchmark_group("privilege_prompt_stream");
    for (name, corpus) in &workloads {
        group.throughput(Throughput::Bytes(corpus.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            &corpus,
            |bencher, corpus| {
                bencher.iter_batched(
                    TerminalPrivilegePromptStream::default,
                    |mut stream| {
                        for chunk in corpus.chunks(TERMINAL_OUTPUT_CHUNK_BYTES) {
                            black_box(stream.observe(black_box(chunk)));
                        }
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();

    // Measure the production scanner stages without SSH or PTY delivery noise.
    // Unicode is kept in complete codepoints, as it is after the live UTF-8 residual guard.
    let mut group = criterion.benchmark_group("shell_outer_scanners");
    for (name, corpus) in &workloads {
        let text = std::str::from_utf8(corpus).unwrap();
        let mut chunks = Vec::new();
        let mut start = 0;
        while start < corpus.len() {
            let mut end = (start + TERMINAL_OUTPUT_CHUNK_BYTES).min(corpus.len());
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            chunks.push(&corpus[start..end]);
            start = end;
        }
        group.throughput(Throughput::Bytes(corpus.len() as u64));
        for combined in [false, true] {
            group.bench_with_input(
                BenchmarkId::new(if combined { "combined" } else { "encoding" }, name),
                &chunks,
                |bencher, chunks| {
                    bencher.iter_batched(
                        || {
                            (
                                EncodingMismatchDetector::new(TerminalEncoding::Utf8),
                                TerminalOutputDecoder::new(TerminalEncoding::Utf8),
                                TerminalPrivilegePromptStream::default(),
                            )
                        },
                        |(mut detector, mut decoder, mut prompt)| {
                            for chunk in chunks {
                                black_box(detector.observe(black_box(chunk)));
                                if combined {
                                    let decoded = decoder.decode_to_utf8_bytes(chunk);
                                    black_box(prompt.observe(decoded.as_ref()));
                                }
                            }
                            black_box(detector);
                        },
                        BatchSize::SmallInput,
                    );
                },
            );
        }
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = benchmark_privilege_prompt_stream
}
criterion_main!(benches);
