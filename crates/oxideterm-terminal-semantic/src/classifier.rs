// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[cfg(feature = "shell-syntax")]
use crate::syntax;
use crate::{
    CompiledSemanticScheme, SemanticLineRole, SemanticScheme, SemanticShellDialect, SemanticSpan,
    ps, scheme,
};

pub fn semantic_output_role_for_command(command: &str) -> SemanticLineRole {
    ps::output_role_for_command(command)
}

pub fn classify_line(text: &str, role: SemanticLineRole) -> Vec<SemanticSpan> {
    classify_line_with_scheme(text, role, SemanticScheme::default())
}

pub fn classify_line_with_scheme(
    text: &str,
    role: SemanticLineRole,
    semantic_scheme: SemanticScheme,
) -> Vec<SemanticSpan> {
    let mut candidates = scheme::candidates(text, role, semantic_scheme);
    let structural = structural_candidates(text);
    candidates.extend(ps::line_candidates(text, role, |class| {
        semantic_scheme.includes(class)
    }));
    accept_candidates_with_structural_variants(&mut candidates, structural)
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
    let structural = structural_candidates(text);
    candidates.extend(ps::line_candidates(text, role, |class| {
        semantic_scheme.contains_rule_class(class)
    }));
    #[cfg(feature = "shell-syntax")]
    candidates.extend(syntax::shell_syntax_candidates(text, role, shell));
    #[cfg(not(feature = "shell-syntax"))]
    let _ = shell;
    accept_candidates_with_structural_variants(&mut candidates, structural)
}

fn structural_candidates(text: &str) -> Vec<scheme::Candidate> {
    const BRACKET_PAIR_PRIORITY: u8 = 55;

    let mut stack = Vec::new();
    let mut candidates = Vec::new();
    let mut token_start = None;
    let mut quoted_by = None;
    let mut escaped = false;
    let mut previous = None;
    let mut chars = text.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        let next = chars.peek().map(|(_, ch)| *ch);
        if escaped {
            escaped = false;
            previous = Some(ch);
            continue;
        }
        if ch == '\\' && quoted_by != Some('\'') {
            token_start.get_or_insert(index);
            escaped = true;
            previous = Some(ch);
            continue;
        }
        if is_quote_delimiter(ch, previous, next) {
            token_start.get_or_insert(index);
            if quoted_by == Some(ch) {
                quoted_by = None;
            } else if quoted_by.is_none() {
                quoted_by = Some(ch);
            }
            previous = Some(ch);
            continue;
        }
        if ch.is_whitespace() && quoted_by.is_none() {
            if let Some(start) = token_start.take() {
                push_standalone_operator(text, start..index, &mut candidates);
            }
        } else {
            token_start.get_or_insert(index);
        }
        if quoted_by.is_some() {
            previous = Some(ch);
            continue;
        }

        if matching_closing_bracket(ch).is_some() {
            let depth = u8::try_from(stack.len()).unwrap_or(u8::MAX);
            stack.push((ch, index, depth));
            previous = Some(ch);
            continue;
        }
        let Some(expected_opening) = matching_opening_bracket(ch) else {
            previous = Some(ch);
            continue;
        };
        let Some((opening, opening_index, depth)) = stack.pop() else {
            previous = Some(ch);
            continue;
        };
        if opening != expected_opening {
            // A mismatched close makes the current nesting ambiguous.
            stack.clear();
            previous = Some(ch);
            continue;
        }
        candidates.push(scheme::Candidate::new_with_style_variant(
            opening_index..opening_index + opening.len_utf8(),
            crate::SemanticClass::Operator,
            BRACKET_PAIR_PRIORITY,
            depth,
        ));
        candidates.push(scheme::Candidate::new_with_style_variant(
            index..index + ch.len_utf8(),
            crate::SemanticClass::Operator,
            BRACKET_PAIR_PRIORITY,
            depth,
        ));
        previous = Some(ch);
    }
    if let Some(start) = token_start {
        push_standalone_operator(text, start..text.len(), &mut candidates);
    }

    candidates
}

fn push_standalone_operator(
    text: &str,
    range: std::ops::Range<usize>,
    candidates: &mut Vec<scheme::Candidate>,
) {
    const STANDALONE_OPERATOR_PRIORITY: u8 = 56;

    if text
        .get(range.clone())
        .is_some_and(|token| matches!(token, "|" | "||" | "=" | "==" | "*" | "-" | "--"))
    {
        candidates.push(scheme::Candidate::new(
            range,
            crate::SemanticClass::Operator,
            STANDALONE_OPERATOR_PRIORITY,
        ));
    }
}

fn is_quote_delimiter(ch: char, previous: Option<char>, next: Option<char>) -> bool {
    if ch == '\''
        && previous.is_some_and(char::is_alphanumeric)
        && next.is_some_and(char::is_alphanumeric)
    {
        return false;
    }
    matches!(ch, '\'' | '"' | '`')
}

fn matching_closing_bracket(opening: char) -> Option<char> {
    match opening {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '<' => Some('>'),
        '（' => Some('）'),
        '［' => Some('］'),
        '｛' => Some('｝'),
        '＜' => Some('＞'),
        '【' => Some('】'),
        '〔' => Some('〕'),
        '〖' => Some('〗'),
        '〘' => Some('〙'),
        '〚' => Some('〛'),
        '〈' => Some('〉'),
        '《' => Some('》'),
        '「' => Some('」'),
        '『' => Some('』'),
        '⟨' => Some('⟩'),
        '⟦' => Some('⟧'),
        '⦃' => Some('⦄'),
        '⌈' => Some('⌉'),
        '⌊' => Some('⌋'),
        _ => None,
    }
}

fn matching_opening_bracket(closing: char) -> Option<char> {
    match closing {
        ')' => Some('('),
        ']' => Some('['),
        '}' => Some('{'),
        '>' => Some('<'),
        '）' => Some('（'),
        '］' => Some('［'),
        '｝' => Some('｛'),
        '＞' => Some('＜'),
        '】' => Some('【'),
        '〕' => Some('〔'),
        '〗' => Some('〖'),
        '〙' => Some('〘'),
        '〛' => Some('〚'),
        '〉' => Some('〈'),
        '》' => Some('《'),
        '」' => Some('「'),
        '』' => Some('『'),
        '⟩' => Some('⟨'),
        '⟧' => Some('⟦'),
        '⦄' => Some('⦃'),
        '⌉' => Some('⌈'),
        '⌋' => Some('⌊'),
        _ => None,
    }
}

fn accept_candidates_with_structural_variants(
    candidates: &mut Vec<scheme::Candidate>,
    structural: Vec<scheme::Candidate>,
) -> Vec<SemanticSpan> {
    let mut bracket_spans = structural
        .iter()
        .filter(|candidate| candidate.span.style_variant.is_some())
        .map(|candidate| candidate.span.clone())
        .collect::<Vec<_>>();
    bracket_spans.sort_by_key(|span| span.range.start);
    candidates.extend(structural);
    let accepted = accept_candidates(candidates);
    overlay_structural_variants(accepted, bracket_spans)
}

fn overlay_structural_variants(
    accepted: Vec<SemanticSpan>,
    bracket_spans: Vec<SemanticSpan>,
) -> Vec<SemanticSpan> {
    let mut result = Vec::with_capacity(accepted.len() + bracket_spans.len());
    let mut brackets = bracket_spans.into_iter().peekable();

    for span in accepted {
        while brackets
            .peek()
            .is_some_and(|bracket| bracket.range.end <= span.range.start)
        {
            result.push(brackets.next().expect("peeked bracket must exist"));
        }

        if matches!(
            span.class,
            crate::SemanticClass::String
                | crate::SemanticClass::Comment
                | crate::SemanticClass::Link
                | crate::SemanticClass::Path
                | crate::SemanticClass::Address
        ) {
            // Keep atomic structured values intact; bracket depth applies only
            // where punctuation is part of the surrounding expression.
            while brackets
                .peek()
                .is_some_and(|bracket| bracket.range.start < span.range.end)
            {
                brackets.next();
            }
            result.push(span);
            continue;
        }

        let mut cursor = span.range.start;
        while brackets
            .peek()
            .is_some_and(|bracket| bracket.range.start < span.range.end)
        {
            let bracket = brackets.next().expect("peeked bracket must exist");
            if cursor < bracket.range.start {
                let mut prefix = span.clone();
                prefix.range = cursor..bracket.range.start;
                result.push(prefix);
            }
            cursor = bracket.range.end;
            result.push(bracket);
        }
        if cursor < span.range.end {
            let mut suffix = span;
            suffix.range.start = cursor;
            result.push(suffix);
        }
    }
    result.extend(brackets);
    result
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
