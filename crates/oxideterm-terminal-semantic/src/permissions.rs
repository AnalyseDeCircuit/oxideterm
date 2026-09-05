// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

use crate::{SemanticClass, SemanticLineRole, scheme::Candidate};

pub(crate) const MODE_FIELD_BYTES: usize = 10;
const MAX_LEADING_WHITESPACE_BYTES: usize = 8;
const PERMISSION_PRIORITY: u8 = 118;

pub(crate) fn push_line_candidates(
    candidates: &mut Vec<Candidate>,
    text: &str,
    role: SemanticLineRole,
) {
    if role == SemanticLineRole::Command {
        return;
    }

    let bytes = text.as_bytes();
    let Some(mode_range) = line_mode_range(bytes) else {
        return;
    };
    push_mode_characters(candidates, text, mode_range);
}

pub(crate) fn line_mode_range(bytes: &[u8]) -> Option<Range<usize>> {
    let mut mode_start = 0;
    // Keep rejection cost constant even for pathological output with long indentation.
    while mode_start < bytes.len()
        && mode_start < MAX_LEADING_WHITESPACE_BYTES
        && bytes[mode_start].is_ascii_whitespace()
    {
        mode_start += 1;
    }
    if mode_start == MAX_LEADING_WHITESPACE_BYTES
        && bytes.get(mode_start).is_some_and(u8::is_ascii_whitespace)
    {
        return None;
    }

    let mode_range = mode_start..mode_start + MODE_FIELD_BYTES;
    let mode = bytes.get(mode_range.clone())?;
    // Fixed POSIX columns avoid coloring incidental rwx text elsewhere in terminal output.
    if !is_unix_mode(mode) {
        return None;
    }

    let mut field_end = mode_start + MODE_FIELD_BYTES;
    if bytes
        .get(field_end)
        .is_some_and(|marker| matches!(marker, b'+' | b'.' | b'@'))
    {
        field_end += 1;
    }
    if !bytes.get(field_end).is_some_and(u8::is_ascii_whitespace) {
        return None;
    }
    Some(mode_range)
}

pub(crate) fn push_mode_characters(
    candidates: &mut Vec<Candidate>,
    text: &str,
    mode_range: Range<usize>,
) -> bool {
    let Some(mode) = text.as_bytes().get(mode_range.clone()) else {
        return false;
    };
    if !is_unix_mode(mode) {
        return false;
    }
    push_permission_characters(candidates, text, mode_range.start + 1..mode_range.end);
    true
}

pub(crate) fn push_permission_characters(
    candidates: &mut Vec<Candidate>,
    text: &str,
    range: Range<usize>,
) -> bool {
    let Some(permission_bytes) = text.as_bytes().get(range.clone()) else {
        return false;
    };
    if !permission_bytes.iter().copied().all(is_permission_byte) {
        return false;
    }

    candidates.reserve(permission_bytes.len());
    for (offset, byte) in permission_bytes.iter().copied().enumerate() {
        let class = match byte {
            b'r' => SemanticClass::PermissionRead,
            b'w' => SemanticClass::PermissionWrite,
            b'x' => SemanticClass::PermissionExecute,
            b's' | b'S' | b't' | b'T' => SemanticClass::PermissionSpecial,
            b'-' => continue,
            _ => unreachable!("mode field was validated before classification"),
        };
        let start = range.start + offset;
        candidates.push(Candidate::new(start..start + 1, class, PERMISSION_PRIORITY));
    }
    true
}

fn is_unix_mode(mode: &[u8]) -> bool {
    mode.len() == MODE_FIELD_BYTES
        && matches!(mode[0], b'-' | b'd' | b'l' | b'b' | b'c' | b'p' | b's')
        && matches!(mode[1], b'r' | b'-')
        && matches!(mode[2], b'w' | b'-')
        && matches!(mode[3], b'x' | b's' | b'S' | b'-')
        && matches!(mode[4], b'r' | b'-')
        && matches!(mode[5], b'w' | b'-')
        && matches!(mode[6], b'x' | b's' | b'S' | b'-')
        && matches!(mode[7], b'r' | b'-')
        && matches!(mode[8], b'w' | b'-')
        && matches!(mode[9], b'x' | b't' | b'T' | b'-')
}

fn is_permission_byte(byte: u8) -> bool {
    matches!(byte, b'r' | b'w' | b'x' | b's' | b'S' | b't' | b'T' | b'-')
}
