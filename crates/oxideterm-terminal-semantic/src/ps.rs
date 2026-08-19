// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{SemanticClass, SemanticLineRole, scheme::Candidate};

const PS_COLUMN_PRIORITY: u8 = 92;
const PS_COMMAND_PRIORITY: u8 = 110;

pub(crate) fn output_role_for_command(command: &str) -> SemanticLineRole {
    let mut tokens = command.split_whitespace();
    let Some(mut executable) = tokens.next() else {
        return SemanticLineRole::Output;
    };
    if executable == "sudo" {
        let Some(next) = tokens.next() else {
            return SemanticLineRole::Output;
        };
        executable = next;
    }
    let executable = executable.rsplit(['/', '\\']).next().unwrap_or(executable);
    if !executable.eq_ignore_ascii_case("ps") {
        return SemanticLineRole::Output;
    }

    let mut full_format = false;
    for arg in tokens.take_while(|token| !matches!(*token, "|" | "||" | "&&" | ";")) {
        if matches!(arg, "aux" | "-aux") {
            return SemanticLineRole::PsAuxOutput;
        }
        full_format |= arg
            .strip_prefix('-')
            .is_some_and(|flags| flags.chars().any(|flag| flag == 'f'));
    }
    if full_format {
        return SemanticLineRole::PsFullOutput;
    }
    SemanticLineRole::Output
}

pub(crate) fn line_candidates(
    text: &str,
    role: SemanticLineRole,
    allows_class: impl Fn(SemanticClass) -> bool,
) -> Vec<Candidate> {
    let tokens = token_ranges(text);
    let (number_columns, tty_column, state_column, start_column, time_column, command_column) =
        match role {
            SemanticLineRole::PsAuxOutput => (&[1, 2, 3, 4, 5][..], 6, Some(7), 8, 9, 10),
            SemanticLineRole::PsFullOutput => (&[1, 2, 3][..], 5, None, 4, 6, 7),
            _ => return Vec::new(),
        };
    if tokens.len() <= command_column || is_header_line(text, &tokens) {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    if allows_class(SemanticClass::Number) {
        for index in number_columns {
            push_token_candidate(&mut candidates, &tokens, *index, SemanticClass::Number);
        }
    }
    if allows_class(SemanticClass::Info) && token_text(text, &tokens, tty_column) == Some("?") {
        push_token_candidate(&mut candidates, &tokens, tty_column, SemanticClass::Info);
    }
    if allows_class(SemanticClass::Info)
        && let Some(state_column) = state_column
    {
        push_token_candidate(&mut candidates, &tokens, state_column, SemanticClass::Info);
    }
    if allows_class(SemanticClass::Timestamp) {
        push_token_candidate(
            &mut candidates,
            &tokens,
            start_column,
            SemanticClass::Timestamp,
        );
        push_token_candidate(
            &mut candidates,
            &tokens,
            time_column,
            SemanticClass::Timestamp,
        );
    }

    // Process tables can contain many visible rows, so avoid constructing a
    // tree-sitter session for every command column. Generic semantic rules
    // still classify its options, assignments, strings, and paths.
    if allows_class(SemanticClass::Command)
        && let Some(command) = tokens.get(command_column)
    {
        candidates.push(Candidate::new(
            command.start..command.end,
            SemanticClass::Command,
            PS_COMMAND_PRIORITY,
        ));
    }

    candidates
}

#[derive(Clone, Debug)]
struct TokenRange {
    start: usize,
    end: usize,
}

fn token_ranges(text: &str) -> Vec<TokenRange> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(start) = start.take() {
                tokens.push(TokenRange { start, end: index });
            }
        } else {
            start.get_or_insert(index);
        }
    }
    if let Some(start) = start {
        tokens.push(TokenRange {
            start,
            end: text.len(),
        });
    }
    tokens
}

fn is_header_line(text: &str, tokens: &[TokenRange]) -> bool {
    matches!(token_text(text, tokens, 0), Some("USER" | "UID"))
        || token_text(text, tokens, 1) == Some("PID")
}

fn token_text<'a>(text: &'a str, tokens: &[TokenRange], index: usize) -> Option<&'a str> {
    let range = tokens.get(index)?;
    text.get(range.start..range.end)
}

fn push_token_candidate(
    candidates: &mut Vec<Candidate>,
    tokens: &[TokenRange],
    index: usize,
    class: SemanticClass,
) {
    let Some(token) = tokens.get(index) else {
        return;
    };
    candidates.push(Candidate::new(
        token.start..token.end,
        class,
        PS_COLUMN_PRIORITY,
    ));
}
