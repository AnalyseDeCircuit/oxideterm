// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[cfg(feature = "shell-syntax")]
use crate::syntax;
use crate::{
    CompiledSemanticScheme, SemanticLineRole, SemanticScheme, SemanticShellDialect, SemanticSpan,
    scheme,
};

pub fn classify_line(text: &str, role: SemanticLineRole) -> Vec<SemanticSpan> {
    classify_line_with_scheme(text, role, SemanticScheme::default())
}

pub fn classify_line_with_scheme(
    text: &str,
    role: SemanticLineRole,
    semantic_scheme: SemanticScheme,
) -> Vec<SemanticSpan> {
    let mut candidates = scheme::candidates(text, role, semantic_scheme);
    accept_candidates(&mut candidates)
}

pub fn classify_line_with_compiled_scheme(
    text: &str,
    role: SemanticLineRole,
    semantic_scheme: &CompiledSemanticScheme,
) -> Vec<SemanticSpan> {
    classify_line_with_compiled_scheme_and_shell(
        text,
        role,
        semantic_scheme,
        SemanticShellDialect::Auto,
    )
}

pub fn classify_line_with_compiled_scheme_and_shell(
    text: &str,
    role: SemanticLineRole,
    semantic_scheme: &CompiledSemanticScheme,
    shell: SemanticShellDialect,
) -> Vec<SemanticSpan> {
    let mut candidates = scheme::candidates_for_compiled(text, role, semantic_scheme);
    #[cfg(feature = "shell-syntax")]
    candidates.extend(syntax::shell_syntax_candidates(text, role, shell));
    #[cfg(not(feature = "shell-syntax"))]
    let _ = shell;
    accept_candidates(&mut candidates)
}

fn accept_candidates(candidates: &mut Vec<scheme::Candidate>) -> Vec<SemanticSpan> {
    candidates.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.span.range.start.cmp(&right.span.range.start))
            .then_with(|| right.span.range.len().cmp(&left.span.range.len()))
    });

    let mut accepted = Vec::new();
    for candidate in candidates.drain(..) {
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
