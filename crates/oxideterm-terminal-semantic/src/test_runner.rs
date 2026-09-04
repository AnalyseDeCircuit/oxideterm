// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cross-runner result markers selected only for known test commands.

use crate::{SemanticClass, SemanticLineRole, command::ParsedCommand, scheme::Candidate};

const TEST_STATUS_PRIORITY: u8 = 95;

pub(crate) fn output_role_for_command(command: &ParsedCommand<'_>) -> Option<SemanticLineRole> {
    let executable = command.executable();
    let mut args = command.arguments();
    let is_test = match executable {
        // Cargo keeps the Rust tool role so compiler diagnostics and test markers can coexist.
        "cargo" => false,
        "go" => args.next() == Some("test"),
        "npm" | "pnpm" | "yarn" | "bun" => args
            .by_ref()
            .take(3)
            .any(|argument| matches!(argument, "test" | "vitest" | "jest")),
        "pytest" | "pytest-3" | "jest" | "vitest" | "mocha" | "ctest" => true,
        "python" | "python3" | "py" => {
            args.next() == Some("-m") && matches!(args.next(), Some("pytest" | "unittest"))
        }
        _ => false,
    };
    is_test.then_some(SemanticLineRole::TestOutput)
}

pub(crate) fn line_candidates(
    text: &str,
    role: SemanticLineRole,
    allows_class: impl Fn(SemanticClass) -> bool,
) -> Vec<Candidate> {
    if !matches!(
        role,
        SemanticLineRole::TestOutput | SemanticLineRole::RustToolOutput
    ) {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    push_prefixed_status(text, &allows_class, &mut candidates);
    push_rust_test_status(text, &allows_class, &mut candidates);
    push_summary_statuses(text, &allows_class, &mut candidates);
    candidates
}

fn push_prefixed_status(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let trimmed = text.trim_start();
    let leading = text.len() - trimmed.len();
    for (prefix, class) in [
        ("--- PASS:", SemanticClass::Success),
        ("--- FAIL:", SemanticClass::Error),
        ("--- SKIP:", SemanticClass::Warning),
        ("PASS ", SemanticClass::Success),
        ("FAIL ", SemanticClass::Error),
        ("SKIP ", SemanticClass::Warning),
        ("ok  ", SemanticClass::Success),
        ("ok\t", SemanticClass::Success),
        ("✓", SemanticClass::Success),
        ("✔", SemanticClass::Success),
        ("✗", SemanticClass::Error),
        ("✘", SemanticClass::Error),
        ("×", SemanticClass::Error),
        ("○", SemanticClass::Warning),
    ] {
        if !trimmed.starts_with(prefix) || !allows_class(class) {
            continue;
        }
        let visible_len = prefix.trim_end_matches([' ', '\t', ':']).len();
        candidates.push(Candidate::new(
            leading..leading + visible_len,
            class,
            TEST_STATUS_PRIORITY,
        ));
        return;
    }
}

fn push_rust_test_status(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let trimmed = text.trim_start();
    if !trimmed.starts_with("test ") {
        return;
    }
    for (suffix, class) in [
        (" ... ok", SemanticClass::Success),
        (" ... FAILED", SemanticClass::Error),
        (" ... ignored", SemanticClass::Warning),
    ] {
        let Some(status_start) = text.rfind(suffix) else {
            continue;
        };
        if allows_class(class) {
            let label_start = status_start + suffix.rfind(' ').expect("status suffix has a label");
            candidates.push(Candidate::new(
                label_start + 1..status_start + suffix.len(),
                class,
                TEST_STATUS_PRIORITY,
            ));
        }
        return;
    }
}

fn push_summary_statuses(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    for (start, token) in token_ranges(text) {
        let label = token.trim_matches(|character: char| !character.is_alphabetic());
        let class = if matches!(label, "passed" | "PASS" | "PASSED" | "ok") {
            Some(SemanticClass::Success)
        } else if matches!(label, "failed" | "FAILED" | "FAIL") {
            Some(SemanticClass::Error)
        } else if matches!(label, "ignored" | "skipped" | "SKIPPED" | "SKIP") {
            Some(SemanticClass::Warning)
        } else {
            None
        };
        let Some(class) = class else {
            continue;
        };
        if allows_class(class) {
            let label_start = start + token.find(label).expect("label belongs to token");
            candidates.push(Candidate::new(
                label_start..label_start + label.len(),
                class,
                TEST_STATUS_PRIORITY,
            ));
        }
    }
}

fn token_ranges(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut search_from = 0;
    text.split_whitespace().filter_map(move |token| {
        let offset = text[search_from..].find(token)?;
        let start = search_from + offset;
        search_from = start + token.len();
        Some((start, token))
    })
}
