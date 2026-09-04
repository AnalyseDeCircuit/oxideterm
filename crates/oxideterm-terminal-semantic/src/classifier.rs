// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[cfg(feature = "shell-syntax")]
use crate::syntax;
use crate::{
    CompiledSemanticScheme, SemanticClass, SemanticLineRole, SemanticScheme, SemanticShellDialect,
    SemanticSpan, command::ParsedCommand, compiler, container, git, ps, scheme, systemd,
    test_runner,
};

// Severity metadata belongs in the log envelope; bounding it prevents payload text from promotion.
const MAX_SEVERITY_PREFIX_BYTES: usize = 192;
const ERROR_SEVERITY_LABELS: &[&str] = &[
    "error",
    "fatal",
    "critical",
    "panic",
    "错误",
    "錯誤",
    "失败",
    "失敗",
    "エラー",
    "오류",
    "fehler",
    "échec",
    "erreur",
    "fallo",
    "fracaso",
    "errore",
    "falha",
    "erro",
    "lỗi",
];
const WARNING_SEVERITY_LABELS: &[&str] = &[
    "warn",
    "warning",
    "警告",
    "注意",
    "警告あり",
    "경고",
    "주의",
    "avertissement",
    "warnung",
    "advertencia",
    "avviso",
    "aviso",
    "cảnh báo",
    "atenção",
    "achtung",
];

pub fn semantic_output_role_for_command(command: &str) -> SemanticLineRole {
    let Some(command) = ParsedCommand::parse(command) else {
        return SemanticLineRole::Output;
    };
    test_runner::output_role_for_command(&command)
        .or_else(|| compiler::output_role_for_command(&command))
        .or_else(|| git::output_role_for_command(&command))
        .or_else(|| systemd::output_role_for_command(&command))
        .or_else(|| container::output_role_for_command(&command))
        .unwrap_or_else(|| ps::output_role_for_command(&command))
}

/// Returns a line-level treatment only for an explicit leading severity label.
/// Incidental status words remain token highlights instead of tinting an entire output row.
pub fn semantic_line_emphasis(text: &str, role: SemanticLineRole) -> Option<SemanticClass> {
    if role == SemanticLineRole::Command {
        return None;
    }
    if let Some(class) = compiler::line_emphasis(text, role) {
        return Some(class);
    }

    let mut remaining = text.trim_start();
    for _ in 0..4 {
        if let Some(class) = explicit_severity_at_start(remaining) {
            return Some(class);
        }
        let Some(after_metadata) = strip_leading_log_metadata(remaining) else {
            break;
        };
        remaining = after_metadata.trim_start();
    }

    explicit_severity_at_start(remaining)
}

fn explicit_severity_at_start(text: &str) -> Option<SemanticClass> {
    let text = text.trim_start();
    if let Some(close) = text.strip_prefix('[').and_then(|text| text.find(']')) {
        return severity_class(&text[1..close + 1]);
    }
    if let Some(class) = structured_severity_at_start(text) {
        return Some(class);
    }
    if let Some(class) = delimited_severity_at_start(text) {
        return Some(class);
    }

    let token = text.split_whitespace().next()?;
    let explicit_delimiter = token.ends_with(':') || token.contains('[');
    let token = token.trim_end_matches([':', ']']);
    let severity = token
        .split_once('[')
        .map_or(token, |(severity, _)| severity);
    let uppercase_label = severity
        .chars()
        .filter(|character| character.is_alphabetic())
        .all(|character| character.is_uppercase());
    if explicit_delimiter || uppercase_label {
        severity_class(severity)
    } else {
        None
    }
}

fn structured_severity_at_start(text: &str) -> Option<SemanticClass> {
    let prefix = bounded_prefix(text, MAX_SEVERITY_PREFIX_BYTES);
    if prefix.starts_with('{') {
        for key in ["\"level\"", "\"severity\""] {
            let mut search_from = 0;
            while let Some(offset) = prefix[search_from..].find(key) {
                let key_start = search_from + offset;
                let before_key = prefix[..key_start].trim_end();
                let field_start = key_start + key.len();
                let is_object_field = before_key.ends_with('{') || before_key.ends_with(',');
                if is_object_field
                    && let Some(class) = severity_assignment_value(&prefix[field_start..], ':')
                {
                    return Some(class);
                }
                search_from = field_start;
            }
        }
    }

    for key in ["level", "severity", "log.level"] {
        let Some(rest) = strip_ascii_prefix(prefix, key) else {
            continue;
        };
        if let Some(class) =
            severity_assignment_value(rest, '=').or_else(|| severity_assignment_value(rest, ':'))
        {
            return Some(class);
        }
    }
    None
}

fn severity_assignment_value(text: &str, delimiter: char) -> Option<SemanticClass> {
    let value = text.trim_start().strip_prefix(delimiter)?.trim_start();
    let (value, terminator) = match value.chars().next() {
        Some(quote @ ('\'' | '"')) => (&value[quote.len_utf8()..], Some(quote)),
        _ => (value, None),
    };
    let end = value
        .char_indices()
        .find(|(_, character)| {
            terminator.is_some_and(|terminator| *character == terminator)
                || (terminator.is_none()
                    && (character.is_whitespace() || matches!(character, ',' | '}')))
        })
        .map_or(value.len(), |(index, _)| index);
    severity_class(&value[..end])
}

fn delimited_severity_at_start(text: &str) -> Option<SemanticClass> {
    ERROR_SEVERITY_LABELS
        .iter()
        .find_map(|label| severity_label_prefix(text, label).then_some(SemanticClass::Error))
        .or_else(|| {
            WARNING_SEVERITY_LABELS.iter().find_map(|label| {
                severity_label_prefix(text, label).then_some(SemanticClass::Warning)
            })
        })
}

fn severity_label_prefix(text: &str, label: &str) -> bool {
    let Some(candidate) = text.get(..label.len()) else {
        return false;
    };
    let has_delimiter = text[label.len()..]
        .chars()
        .next()
        .is_some_and(|character| matches!(character, ':' | '：'));
    // Ordinary output avoids Unicode case-folding unless it first has an explicit label boundary.
    has_delimiter && (candidate.eq_ignore_ascii_case(label) || candidate.to_lowercase() == label)
}

fn severity_class(label: &str) -> Option<SemanticClass> {
    let label = label.trim().to_lowercase();
    if ERROR_SEVERITY_LABELS.contains(&label.as_str()) {
        Some(SemanticClass::Error)
    } else if WARNING_SEVERITY_LABELS.contains(&label.as_str()) {
        Some(SemanticClass::Warning)
    } else {
        None
    }
}

fn strip_ascii_prefix<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let candidate = text.get(..prefix.len())?;
    candidate
        .eq_ignore_ascii_case(prefix)
        .then_some(&text[prefix.len()..])
}

fn bounded_prefix(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn strip_leading_log_metadata(text: &str) -> Option<&str> {
    // Bound prefix peeling so malformed logs cannot turn classification into an unbounded scan.
    if let Some(rest) = text.strip_prefix('[') {
        let close = rest.find(']')?;
        return (close <= 64).then_some(&rest[close + 1..]);
    }

    let token = text.split_whitespace().next()?;
    let timestamp_like = token.chars().any(|character| character.is_ascii_digit())
        && token.chars().all(|character| {
            character.is_ascii_digit()
                || matches!(character, '-' | '/' | ':' | '.' | ',' | '+' | 'T' | 'Z')
        });
    timestamp_like.then_some(&text[token.len()..])
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
    candidates.extend(compiler::line_candidates(text, role, |class| {
        semantic_scheme.includes(class)
    }));
    candidates.extend(git::line_candidates(text, role, |class| {
        semantic_scheme.includes(class)
    }));
    candidates.extend(systemd::line_candidates(text, role, |class| {
        semantic_scheme.includes(class)
    }));
    candidates.extend(test_runner::line_candidates(text, role, |class| {
        semantic_scheme.includes(class)
    }));
    candidates.extend(container::line_candidates(text, role, |class| {
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
    candidates.extend(compiler::line_candidates(text, role, |class| {
        semantic_scheme.contains_rule_class(class)
    }));
    candidates.extend(git::line_candidates(text, role, |class| {
        semantic_scheme.contains_rule_class(class)
    }));
    candidates.extend(systemd::line_candidates(text, role, |class| {
        semantic_scheme.contains_rule_class(class)
    }));
    candidates.extend(test_runner::line_candidates(text, role, |class| {
        semantic_scheme.contains_rule_class(class)
    }));
    candidates.extend(container::line_candidates(text, role, |class| {
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
