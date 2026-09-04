// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! systemd state and journal fields selected from systemctl and journalctl command scopes.

use crate::{SemanticClass, SemanticLineRole, command::ParsedCommand, scheme::Candidate};

const SYSTEMD_FIELD_PRIORITY: u8 = 94;

pub(crate) fn output_role_for_command(command: &ParsedCommand<'_>) -> Option<SemanticLineRole> {
    matches!(command.executable(), "systemctl" | "journalctl")
        .then_some(SemanticLineRole::SystemdOutput)
}

pub(crate) fn line_candidates(
    text: &str,
    role: SemanticLineRole,
    allows_class: impl Fn(SemanticClass) -> bool,
) -> Vec<Candidate> {
    if role != SemanticLineRole::SystemdOutput {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    push_status_field(text, "Active:", &allows_class, &mut candidates);
    push_status_field(text, "Loaded:", &allows_class, &mut candidates);
    push_syslog_timestamp(text, &allows_class, &mut candidates);
    push_unit_heading(text, &allows_class, &mut candidates);
    candidates
}

fn push_status_field(
    text: &str,
    field: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let trimmed = text.trim_start();
    let field_start = text.len() - trimmed.len();
    let Some(rest) = trimmed.strip_prefix(field) else {
        return;
    };
    if allows_class(SemanticClass::Keyword) {
        candidates.push(Candidate::new(
            field_start..field_start + field.len() - 1,
            SemanticClass::Keyword,
            SYSTEMD_FIELD_PRIORITY,
        ));
    }
    let value = rest.trim_start();
    let value_start = text.len() - value.len();
    let states = [
        ("active (running)", SemanticClass::Success),
        ("active (exited)", SemanticClass::Success),
        ("active", SemanticClass::Success),
        ("loaded", SemanticClass::Success),
        ("failed", SemanticClass::Error),
        ("not-found", SemanticClass::Error),
        ("inactive (dead)", SemanticClass::Warning),
        ("inactive", SemanticClass::Warning),
        ("deactivating", SemanticClass::Warning),
        ("masked", SemanticClass::Warning),
        ("activating", SemanticClass::Info),
        ("reloading", SemanticClass::Info),
    ];
    for (state, class) in states {
        if value.starts_with(state) && allows_class(class) {
            candidates.push(Candidate::new(
                value_start..value_start + state.len(),
                class,
                SYSTEMD_FIELD_PRIORITY,
            ));
            break;
        }
    }
}

fn push_syslog_timestamp(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    if !allows_class(SemanticClass::Timestamp) {
        return;
    }
    let trimmed = text.trim_start();
    let leading = text.len() - trimmed.len();
    let Some(timestamp) = trimmed.get(..15) else {
        return;
    };
    let bytes = timestamp.as_bytes();
    let valid_month = matches!(
        bytes.get(..3),
        Some(b"Jan")
            | Some(b"Feb")
            | Some(b"Mar")
            | Some(b"Apr")
            | Some(b"May")
            | Some(b"Jun")
            | Some(b"Jul")
            | Some(b"Aug")
            | Some(b"Sep")
            | Some(b"Oct")
            | Some(b"Nov")
            | Some(b"Dec")
    );
    let valid_shape = bytes.get(3) == Some(&b' ')
        && bytes.get(6) == Some(&b' ')
        && bytes.get(9) == Some(&b':')
        && bytes.get(12) == Some(&b':')
        && bytes[4..6]
            .iter()
            .all(|byte| byte.is_ascii_digit() || *byte == b' ')
        && bytes[4..6].iter().any(u8::is_ascii_digit)
        && bytes[7..9].iter().all(u8::is_ascii_digit)
        && bytes[10..12].iter().all(u8::is_ascii_digit)
        && bytes[13..15].iter().all(u8::is_ascii_digit);
    if valid_month && valid_shape {
        candidates.push(Candidate::new(
            leading..leading + timestamp.len(),
            SemanticClass::Timestamp,
            SYSTEMD_FIELD_PRIORITY,
        ));
    }
}

fn push_unit_heading(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let trimmed = text.trim_start();
    let leading = text.len() - trimmed.len();
    let Some(unit) = trimmed.strip_prefix('●').map(str::trim_start) else {
        return;
    };
    let unit_start = text.len() - unit.len();
    let unit_end = unit
        .find(char::is_whitespace)
        .map_or(text.len(), |end| unit_start + end);
    if unit_end > unit_start && allows_class(SemanticClass::Variable) {
        candidates.push(Candidate::new(
            unit_start..unit_end,
            SemanticClass::Variable,
            SYSTEMD_FIELD_PRIORITY,
        ));
    }
    if allows_class(SemanticClass::Info) {
        candidates.push(Candidate::new(
            leading..leading + '●'.len_utf8(),
            SemanticClass::Info,
            SYSTEMD_FIELD_PRIORITY,
        ));
    }
}
