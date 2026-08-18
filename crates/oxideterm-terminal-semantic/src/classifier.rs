// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{SemanticLineRole, SemanticSpan, scheme};

pub fn classify_line(text: &str, role: SemanticLineRole) -> Vec<SemanticSpan> {
    let mut candidates = scheme::candidates(text, role);
    candidates.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.span.range.start.cmp(&right.span.range.start))
            .then_with(|| right.span.range.len().cmp(&left.span.range.len()))
    });

    let mut accepted = Vec::new();
    for candidate in candidates {
        if accepted.iter().any(|existing: &SemanticSpan| {
            candidate.span.range.start < existing.range.end
                && candidate.span.range.end > existing.range.start
        }) {
            continue;
        }
        accepted.push(candidate.span);
    }
    accepted.sort_by_key(|span| span.range.start);
    accepted
}
