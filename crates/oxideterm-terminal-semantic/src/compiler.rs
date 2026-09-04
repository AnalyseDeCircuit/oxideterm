// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Structured compiler diagnostics selected only from the originating command mark.

use std::ops::Range;

use crate::{SemanticClass, SemanticLineRole, command::ParsedCommand, scheme::Candidate};

const COMPILER_LOCATION_PRIORITY: u8 = 96;
const COMPILER_SEVERITY_PRIORITY: u8 = 94;
const COMPILER_PHASE_PRIORITY: u8 = 90;

pub(crate) fn output_role_for_command(command: &ParsedCommand<'_>) -> Option<SemanticLineRole> {
    let executable = command.executable();
    if matches!(executable, "cargo" | "rustc" | "rustdoc" | "clippy-driver") {
        Some(SemanticLineRole::RustToolOutput)
    } else if matches!(
        executable,
        "gcc" | "g++" | "clang" | "clang++" | "cc" | "c++"
    ) || ["-gcc", "-g++", "-clang", "-clang++"]
        .iter()
        .any(|suffix| executable.ends_with(suffix))
    {
        Some(SemanticLineRole::CCompilerOutput)
    } else {
        None
    }
}

pub(crate) fn line_candidates(
    text: &str,
    role: SemanticLineRole,
    allows_class: impl Fn(SemanticClass) -> bool,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    match role {
        SemanticLineRole::RustToolOutput | SemanticLineRole::TestOutput => {
            push_rust_diagnostic(text, &allows_class, &mut candidates);
            push_rust_location(text, &allows_class, &mut candidates);
            push_c_style_diagnostic(text, &allows_class, &mut candidates);
            if role == SemanticLineRole::RustToolOutput {
                push_cargo_phase(text, &allows_class, &mut candidates);
            }
        }
        SemanticLineRole::CCompilerOutput => {
            push_c_style_diagnostic(text, &allows_class, &mut candidates);
            push_c_include_location(text, &allows_class, &mut candidates);
        }
        _ => {}
    }
    candidates
}

pub(crate) fn line_emphasis(text: &str, role: SemanticLineRole) -> Option<SemanticClass> {
    if !matches!(
        role,
        SemanticLineRole::RustToolOutput
            | SemanticLineRole::CCompilerOutput
            | SemanticLineRole::TestOutput
    ) {
        return None;
    }
    for (marker, class) in [
        (": fatal error:", SemanticClass::Error),
        (": error:", SemanticClass::Error),
        (": warning:", SemanticClass::Warning),
    ] {
        let Some(marker_start) = text.find(marker) else {
            continue;
        };
        let location = trim_range(text, 0..marker_start);
        if source_location_ranges(text, location).is_some() {
            return Some(class);
        }
    }
    None
}

fn push_rust_diagnostic(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    for (label, class) in [
        ("error", SemanticClass::Error),
        ("warning", SemanticClass::Warning),
        ("note", SemanticClass::Info),
        ("help", SemanticClass::Info),
    ] {
        if !allows_class(class) {
            continue;
        }
        let Some(range) = leading_label_range(text, label, &[':', '[']) else {
            continue;
        };
        candidates.push(Candidate::new(range, class, COMPILER_SEVERITY_PRIORITY));
        break;
    }
}

fn push_rust_location(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let trimmed = text.trim_start();
    let leading = text.len() - trimmed.len();
    let Some(marker) = ["-->", ":::"]
        .iter()
        .find(|marker| trimmed.starts_with(**marker))
    else {
        return;
    };
    if allows_class(SemanticClass::Operator) {
        candidates.push(Candidate::new(
            leading..leading + marker.len(),
            SemanticClass::Operator,
            COMPILER_LOCATION_PRIORITY,
        ));
    }
    let location_start = leading + marker.len();
    let location = trim_range(text, location_start..text.len());
    push_source_location(text, location, allows_class, candidates);
}

fn push_c_style_diagnostic(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let markers = [
        (": fatal error:", "fatal error", SemanticClass::Error),
        (": error:", "error", SemanticClass::Error),
        (": warning:", "warning", SemanticClass::Warning),
        (": note:", "note", SemanticClass::Info),
    ];
    for (marker, label, class) in markers {
        let Some(marker_start) = text.find(marker) else {
            continue;
        };
        let location = trim_range(text, 0..marker_start);
        push_source_location(text, location, allows_class, candidates);
        if allows_class(class) {
            let label_start = marker_start + marker.find(label).expect("marker contains label");
            candidates.push(Candidate::new(
                label_start..label_start + label.len(),
                class,
                COMPILER_SEVERITY_PRIORITY,
            ));
        }
        break;
    }
}

fn push_c_include_location(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let trimmed = text.trim_start();
    let leading = text.len() - trimmed.len();
    let location_start = if let Some(rest) = trimmed.strip_prefix("In file included from ") {
        leading + trimmed.len() - rest.len()
    } else if let Some(rest) = trimmed.strip_prefix("from ") {
        leading + trimmed.len() - rest.len()
    } else {
        return;
    };
    let location = trim_range(text, location_start..text.len());
    push_source_location(text, location, allows_class, candidates);
}

fn push_cargo_phase(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let phases = [
        ("Finished", SemanticClass::Success),
        ("Fresh", SemanticClass::Success),
        ("Compiling", SemanticClass::Info),
        ("Checking", SemanticClass::Info),
        ("Building", SemanticClass::Info),
        ("Downloading", SemanticClass::Info),
        ("Downloaded", SemanticClass::Info),
        ("Running", SemanticClass::Info),
    ];
    for (phase, class) in phases {
        if !allows_class(class) {
            continue;
        }
        let Some(range) = leading_label_range(text, phase, &[' ', '`']) else {
            continue;
        };
        candidates.push(Candidate::new(range, class, COMPILER_PHASE_PRIORITY));
        break;
    }
}

fn push_source_location(
    text: &str,
    location: Range<usize>,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let Some((path, numbers)) = source_location_ranges(text, location) else {
        return;
    };
    if allows_class(SemanticClass::Path) {
        candidates.push(Candidate::new(
            path,
            SemanticClass::Path,
            COMPILER_LOCATION_PRIORITY,
        ));
    }
    if allows_class(SemanticClass::Number) {
        candidates.extend(
            numbers.into_iter().map(|range| {
                Candidate::new(range, SemanticClass::Number, COMPILER_LOCATION_PRIORITY)
            }),
        );
    }
}

fn source_location_ranges(
    text: &str,
    location: Range<usize>,
) -> Option<(Range<usize>, Vec<Range<usize>>)> {
    let location_text = text.get(location.clone())?;
    let location_text = location_text.trim_end_matches([':', ',']);
    let mut relative_end = location_text.len();
    let mut numbers = Vec::new();
    for _ in 0..2 {
        let Some(colon) = location_text[..relative_end].rfind(':') else {
            break;
        };
        let number = &location_text[colon + 1..relative_end];
        if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            break;
        }
        numbers.push(location.start + colon + 1..location.start + relative_end);
        relative_end = colon;
    }
    if numbers.is_empty() {
        return None;
    }
    let path = &location_text[..relative_end];
    if path.is_empty() || (!path.contains('.') && !path.contains('/') && !path.contains('\\')) {
        return None;
    }
    numbers.reverse();
    Some((location.start..location.start + relative_end, numbers))
}

fn leading_label_range(text: &str, label: &str, delimiters: &[char]) -> Option<Range<usize>> {
    let trimmed = text.trim_start();
    let leading = text.len() - trimmed.len();
    let rest = trimmed.strip_prefix(label)?;
    rest.chars()
        .next()
        .is_some_and(|character| delimiters.contains(&character))
        .then_some(leading..leading + label.len())
}

fn trim_range(text: &str, range: Range<usize>) -> Range<usize> {
    let selected = &text[range.clone()];
    let trimmed_start = selected.len() - selected.trim_start().len();
    let trimmed_end = selected.trim_end().len();
    range.start + trimmed_start..range.start + trimmed_end
}
