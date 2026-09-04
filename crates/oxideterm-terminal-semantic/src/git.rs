// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Git status and diff semantics kept out of unrelated terminal output.

use std::ops::Range;

use crate::{SemanticClass, SemanticLineRole, command::ParsedCommand, scheme::Candidate};

const GIT_STRUCTURE_PRIORITY: u8 = 96;
const GIT_CONTENT_PRIORITY: u8 = 88;

pub(crate) fn output_role_for_command(command: &ParsedCommand<'_>) -> Option<SemanticLineRole> {
    if command.executable() != "git" {
        return None;
    }

    let mut option_value = false;
    for token in command.arguments() {
        if option_value {
            option_value = false;
            continue;
        }
        if matches!(token, "-C" | "-c" | "--git-dir" | "--work-tree") {
            option_value = true;
            continue;
        }
        if token.starts_with('-') {
            continue;
        }
        return match token {
            "status" => Some(SemanticLineRole::GitStatusOutput),
            "diff" | "show" => Some(SemanticLineRole::GitDiffOutput),
            _ => None,
        };
    }
    None
}

pub(crate) fn line_candidates(
    text: &str,
    role: SemanticLineRole,
    allows_class: impl Fn(SemanticClass) -> bool,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    match role {
        SemanticLineRole::GitStatusOutput => {
            push_porcelain_status(text, &allows_class, &mut candidates);
            push_human_status(text, &allows_class, &mut candidates);
        }
        SemanticLineRole::GitDiffOutput => {
            push_diff_line(text, &allows_class, &mut candidates);
        }
        _ => {}
    }
    candidates
}

fn push_porcelain_status(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let bytes = text.as_bytes();
    if bytes.len() < 4 || bytes[2] != b' ' {
        return;
    }
    let Some(status) = text.get(..2) else {
        return;
    };
    let Some(class) = git_status_class(status) else {
        return;
    };
    if allows_class(class) {
        candidates.push(Candidate::new(0..2, class, GIT_STRUCTURE_PRIORITY));
    }
    push_rename_paths(text, 3..text.len(), allows_class, candidates);
}

fn git_status_class(status: &str) -> Option<SemanticClass> {
    if status
        .chars()
        .any(|character| matches!(character, 'U' | 'D'))
    {
        Some(SemanticClass::Error)
    } else if status.contains('A') {
        Some(SemanticClass::Success)
    } else if status
        .chars()
        .any(|character| matches!(character, 'M' | 'T'))
    {
        Some(SemanticClass::Warning)
    } else if status
        .chars()
        .any(|character| matches!(character, '?' | 'R' | 'C'))
    {
        Some(SemanticClass::Info)
    } else {
        None
    }
}

fn push_human_status(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let trimmed = text.trim_start();
    let leading = text.len() - trimmed.len();
    for (prefix, class) in [
        ("modified:", SemanticClass::Warning),
        ("deleted:", SemanticClass::Error),
        ("new file:", SemanticClass::Success),
        ("renamed:", SemanticClass::Info),
        ("copied:", SemanticClass::Info),
    ] {
        let Some(rest) = trimmed.strip_prefix(prefix) else {
            continue;
        };
        if allows_class(class) {
            candidates.push(Candidate::new(
                leading..leading + prefix.len() - 1,
                class,
                GIT_STRUCTURE_PRIORITY,
            ));
        }
        let path_start = text.len() - rest.trim_start().len();
        let path_end = text.trim_end().len();
        push_rename_paths(text, path_start..path_end, allows_class, candidates);
        return;
    }

    if let Some(branch) = trimmed.strip_prefix("On branch ")
        && allows_class(SemanticClass::Variable)
    {
        let branch_start = text.len() - branch.len();
        candidates.push(Candidate::new(
            branch_start..text.len(),
            SemanticClass::Variable,
            GIT_STRUCTURE_PRIORITY,
        ));
    }
}

fn push_diff_line(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    if let Some(paths) = text.strip_prefix("diff --git ") {
        if allows_class(SemanticClass::Keyword) {
            candidates.push(Candidate::new(
                0..10,
                SemanticClass::Keyword,
                GIT_STRUCTURE_PRIORITY,
            ));
        }
        let paths_start = text.len() - paths.len();
        for (start, path) in token_ranges(paths) {
            push_path(
                paths_start + start..paths_start + start + path.len(),
                allows_class,
                candidates,
            );
        }
        return;
    }
    if let Some(path) = text.strip_prefix("+++ ") {
        push_diff_header(path, SemanticClass::Success, allows_class, candidates);
        return;
    }
    if let Some(path) = text.strip_prefix("--- ") {
        push_diff_header(path, SemanticClass::Error, allows_class, candidates);
        return;
    }
    if text.starts_with("@@")
        && let Some(closing) = text[2..].find("@@")
    {
        if allows_class(SemanticClass::Info) {
            candidates.push(Candidate::new(
                0..closing + 4,
                SemanticClass::Info,
                GIT_CONTENT_PRIORITY,
            ));
        }
        return;
    }
    let class = match text.as_bytes().first() {
        Some(b'+') => Some(SemanticClass::Success),
        Some(b'-') => Some(SemanticClass::Error),
        _ => None,
    };
    if let Some(class) = class
        && text.len() > 1
        && allows_class(class)
    {
        candidates.push(Candidate::new(0..text.len(), class, GIT_CONTENT_PRIORITY));
    }
}

fn push_diff_header(
    path: &str,
    class: SemanticClass,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    if allows_class(SemanticClass::Operator) {
        candidates.push(Candidate::new(
            0..3,
            SemanticClass::Operator,
            GIT_STRUCTURE_PRIORITY,
        ));
    }
    let path = path.split('\t').next().unwrap_or(path);
    if path != "/dev/null" && allows_class(class) {
        let path_start = 4;
        candidates.push(Candidate::new(
            path_start..path_start + path.len(),
            class,
            GIT_STRUCTURE_PRIORITY,
        ));
    }
}

fn push_rename_paths(
    text: &str,
    range: Range<usize>,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let selected = &text[range.clone()];
    if let Some(arrow) = selected.find(" -> ") {
        push_path(range.start..range.start + arrow, allows_class, candidates);
        if allows_class(SemanticClass::Operator) {
            candidates.push(Candidate::new(
                range.start + arrow + 1..range.start + arrow + 3,
                SemanticClass::Operator,
                GIT_STRUCTURE_PRIORITY,
            ));
        }
        push_path(range.start + arrow + 4..range.end, allows_class, candidates);
    } else {
        push_path(range, allows_class, candidates);
    }
}

fn push_path(
    range: Range<usize>,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    if !range.is_empty() && allows_class(SemanticClass::Path) {
        candidates.push(Candidate::new(
            range,
            SemanticClass::Path,
            GIT_STRUCTURE_PRIORITY,
        ));
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
