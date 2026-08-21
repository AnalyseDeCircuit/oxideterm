// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{sync::Arc, time::Duration};

use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use oxideterm_terminal_triggers::{
    CompiledTriggerSet, TERMINAL_TRIGGERS_SCHEMA_VERSION, TerminalTrigger, TerminalTriggerAction,
    TerminalTriggerDispatch, TerminalTriggerMatch, TerminalTriggerMatchMode, TerminalTriggerScope,
    TerminalTriggerStream, TerminalTriggerTiming, TerminalTriggersSnapshot, compile_active,
};

const BENCHMARK_CORPUS_BYTES: usize = 1024 * 1024;
const TERMINAL_OUTPUT_CHUNK_BYTES: usize = 8 * 1024;

fn trigger(id: &str, pattern: &str, mode: TerminalTriggerMatchMode) -> TerminalTrigger {
    TerminalTrigger {
        id: id.to_string(),
        name: id.to_string(),
        description: None,
        enabled: true,
        matcher: TerminalTriggerMatch {
            pattern: pattern.to_string(),
            mode,
            case_sensitive: true,
            whole_word: false,
        },
        action: TerminalTriggerAction::SendText {
            text: "ack".to_string(),
            append_enter: true,
        },
        timing: TerminalTriggerTiming {
            dispatch: TerminalTriggerDispatch::Immediate,
            delay_ms: 0,
            cooldown_ms: 100,
        },
        scope: TerminalTriggerScope::AllTerminals,
        created_at: 1,
        updated_at: 1,
    }
}

fn compile(rules: Vec<TerminalTrigger>) -> Option<Arc<CompiledTriggerSet>> {
    let snapshot = TerminalTriggersSnapshot {
        version: TERMINAL_TRIGGERS_SCHEMA_VERSION,
        triggers: rules,
        updated_at: 1,
    };
    compile_active(&snapshot, 1).unwrap()
}

fn repeated_corpus(line: &[u8]) -> Vec<u8> {
    let mut corpus = Vec::with_capacity(BENCHMARK_CORPUS_BYTES + line.len());
    while corpus.len() < BENCHMARK_CORPUS_BYTES {
        corpus.extend_from_slice(line);
    }
    corpus
}

fn benchmark_case(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    corpus: &[u8],
    rules: Option<Arc<CompiledTriggerSet>>,
) {
    group.throughput(Throughput::Bytes(corpus.len() as u64));
    group.bench_with_input(
        BenchmarkId::from_parameter(name),
        corpus,
        |bencher, corpus| {
            bencher.iter_batched(
                || {
                    rules
                        .as_ref()
                        .map(|rules| TerminalTriggerStream::new(rules.clone()))
                },
                |mut stream| {
                    for chunk in corpus.chunks(TERMINAL_OUTPUT_CHUNK_BYTES) {
                        if let Some(stream) = stream.as_mut() {
                            stream.observe_bytes(black_box(chunk), |event| {
                                black_box(event);
                            });
                        } else {
                            black_box(chunk);
                        }
                    }
                },
                BatchSize::SmallInput,
            );
        },
    );
}

fn benchmark_trigger_stream(criterion: &mut Criterion) {
    let plain = repeated_corpus(b"status=READY terminal output 0123456789\r\n");
    let ansi = repeated_corpus(b"\x1b[1;38;2;72;183;255mstatus=READY\x1b[0m\r\n");
    let unicode = repeated_corpus("状态=READY 中文 日本語 한국어 🚀\r\n".as_bytes());
    let long_csi =
        repeated_corpus(b"\x1b[38;2;123;45;67;1;2;3;4;5;6;7;8;9;10mstatus=READY\x1b[0m\r\n");
    let literal = compile(vec![trigger(
        "literal",
        "status=READY",
        TerminalTriggerMatchMode::Literal,
    )]);
    let regex = compile(vec![trigger(
        "regex",
        r"status=(?P<status>READY|WAITING)",
        TerminalTriggerMatchMode::Regex,
    )]);
    let many = compile(
        (0..16)
            .map(|index| {
                trigger(
                    &format!("rule-{index}"),
                    &format!("candidate-{index}"),
                    TerminalTriggerMatchMode::Literal,
                )
            })
            .collect(),
    );

    let mut group = criterion.benchmark_group("terminal_trigger_stream");
    benchmark_case(&mut group, "disabled_plain", &plain, None);
    benchmark_case(&mut group, "literal_plain", &plain, literal.clone());
    benchmark_case(&mut group, "literal_ansi", &ansi, literal.clone());
    benchmark_case(&mut group, "literal_unicode", &unicode, literal.clone());
    benchmark_case(&mut group, "literal_long_csi", &long_csi, literal);
    benchmark_case(&mut group, "regex_plain", &plain, regex);
    benchmark_case(&mut group, "sixteen_literals_plain", &plain, many);
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = benchmark_trigger_stream
}
criterion_main!(benches);
