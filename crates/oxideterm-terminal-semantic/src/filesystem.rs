// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Structured file metadata selected from ls, stat, and getfacl command scopes.

use crate::{
    SemanticClass, SemanticLineRole,
    command::ParsedCommand,
    permissions,
    scheme::Candidate,
    tokens::{ranges, text_at},
};

const FILE_FIELD_PRIORITY: u8 = 94;

pub(crate) fn listing_role(command: &ParsedCommand<'_>) -> SemanticLineRole {
    let mut owner = true;
    let mut group = true;
    for arg in command
        .arguments()
        .take_while(|arg| !matches!(*arg, "--" | "|" | "||" | "&&" | ";"))
    {
        if let Some(flags) = arg
            .strip_prefix('-')
            .filter(|flags| !flags.starts_with('-'))
        {
            owner &= !flags.contains('g');
            group &= !flags.contains('o');
        }
        group &= arg != "--no-group";
    }
    match (owner, group) {
        (true, true) => SemanticLineRole::FileListingOutput,
        (true, false) => SemanticLineRole::FileListingOwnerOutput,
        (false, true) => SemanticLineRole::FileListingGroupOutput,
        (false, false) => SemanticLineRole::FileListingAnonymousOutput,
    }
}

pub(crate) fn line_candidates(
    text: &str,
    role: SemanticLineRole,
    allows_class: impl Fn(SemanticClass) -> bool,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    match role {
        SemanticLineRole::FileListingOutput
        | SemanticLineRole::FileListingOwnerOutput
        | SemanticLineRole::FileListingGroupOutput
        | SemanticLineRole::FileListingAnonymousOutput => {
            push_listing_fields(text, role, &allows_class, &mut candidates);
        }
        SemanticLineRole::FileStatOutput => {
            push_stat_fields(text, &allows_class, &mut candidates);
        }
        SemanticLineRole::FileAclOutput => {
            push_acl_fields(text, &allows_class, &mut candidates);
        }
        _ => {}
    }
    candidates
}

fn push_listing_fields(
    text: &str,
    role: SemanticLineRole,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let Some(mode_range) = permissions::line_mode_range(text.as_bytes()) else {
        return;
    };
    let mut fields = ranges(text);
    let Some(mode_field) = fields.next() else {
        return;
    };
    let Some(link_count) = fields.next() else {
        return;
    };
    // The command determines omitted identity columns; numeric owners cannot be inferred from text.
    let owner = matches!(
        role,
        SemanticLineRole::FileListingOutput | SemanticLineRole::FileListingOwnerOutput
    )
    .then(|| fields.next())
    .flatten();
    let group = matches!(
        role,
        SemanticLineRole::FileListingOutput | SemanticLineRole::FileListingGroupOutput
    )
    .then(|| fields.next())
    .flatten();
    let Some(mut size) = fields.next() else {
        return;
    };
    let device = matches!(text.as_bytes()[mode_range.start], b'b' | b'c');
    if device {
        let major = &text[size.clone()];
        if let Some(major) = major.strip_suffix(',') {
            let Some(minor) = fields.next() else {
                return;
            };
            if !is_unsigned_integer(major) || !is_unsigned_integer(&text[minor.clone()]) {
                return;
            }
            size.end = minor.end;
        } else if !major
            .split_once(',')
            .is_some_and(|(major, minor)| is_unsigned_integer(major) && is_unsigned_integer(minor))
            && !major.strip_prefix("0x").is_some_and(|value| {
                !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        {
            return;
        }
    }
    if mode_field.start != mode_range.start
        || !text_at(text, &link_count).is_some_and(is_unsigned_integer)
        || (!device && !text_at(text, &size).is_some_and(is_file_size))
    {
        return;
    }

    if allows_class(SemanticClass::Keyword) {
        candidates.push(Candidate::new(
            mode_range.start..mode_range.start + 1,
            SemanticClass::Keyword,
            FILE_FIELD_PRIORITY,
        ));
    }
    if allows_class(SemanticClass::Number) {
        candidates.push(Candidate::new(
            link_count,
            SemanticClass::Number,
            FILE_FIELD_PRIORITY,
        ));
        candidates.push(Candidate::new(
            size.clone(),
            SemanticClass::Number,
            FILE_FIELD_PRIORITY,
        ));
    }
    if allows_class(SemanticClass::Variable) {
        for identity in [owner, group].into_iter().flatten() {
            candidates.push(Candidate::new(
                identity,
                SemanticClass::Variable,
                FILE_FIELD_PRIORITY,
            ));
        }
    }
    if text.as_bytes().get(mode_range.start) == Some(&b'l')
        && allows_class(SemanticClass::Operator)
        && let Some(offset) = text.get(size.end..).and_then(|rest| rest.find(" -> "))
    {
        let arrow_start = size.end + offset + 1;
        candidates.push(Candidate::new(
            arrow_start..arrow_start + 2,
            SemanticClass::Operator,
            FILE_FIELD_PRIORITY,
        ));
    }
}

fn push_stat_fields(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    // BSD stat starts with device and inode numbers, followed by a complete mode field.
    let mut fields = ranges(text);
    if let (Some(device), Some(inode), Some(mode)) = (fields.next(), fields.next(), fields.next())
        && is_unsigned_integer(&text[device])
        && is_unsigned_integer(&text[inode])
        && permissions::push_mode_characters(candidates, text, mode)
    {
        let (Some(links), Some(owner), Some(group), Some(_rdev), Some(size)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            return;
        };
        if is_unsigned_integer(&text[links.clone()]) && is_unsigned_integer(&text[size.clone()]) {
            for (range, class) in [
                (links, SemanticClass::Number),
                (size, SemanticClass::Number),
                (owner, SemanticClass::Variable),
                (group, SemanticClass::Variable),
            ] {
                if allows_class(class) {
                    candidates.push(Candidate::new(range, class, FILE_FIELD_PRIORITY));
                }
            }
        }
        return;
    }
    if allows_class(SemanticClass::Keyword) {
        for range in ranges(text) {
            let Some(token) = text_at(text, &range) else {
                continue;
            };
            let label = token.trim_end_matches(':');
            if token.ends_with(':')
                && matches!(
                    label,
                    "File"
                        | "Size"
                        | "Blocks"
                        | "Block"
                        | "Device"
                        | "Inode"
                        | "Links"
                        | "Access"
                        | "Uid"
                        | "Gid"
                        | "Modify"
                        | "Change"
                        | "Birth"
                )
            {
                candidates.push(Candidate::new(
                    range.start..range.start + label.len(),
                    SemanticClass::Keyword,
                    FILE_FIELD_PRIORITY,
                ));
            }
        }
    }

    let trimmed = text.trim_start();
    if trimmed.starts_with("Access:")
        && let Some(mode_start) = text.find('/')
    {
        let mode_start = mode_start + 1;
        permissions::push_mode_characters(
            candidates,
            text,
            mode_start..mode_start + permissions::MODE_FIELD_BYTES,
        );
    }
}

fn push_acl_fields(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let trimmed = text.trim_start();
    let leading = text.len() - trimmed.len();
    if let Some(metadata) = trimmed.strip_prefix("# ") {
        let Some((label, value)) = metadata.split_once(':') else {
            return;
        };
        if !matches!(label, "file" | "owner" | "group") {
            return;
        }
        if allows_class(SemanticClass::Keyword) {
            candidates.push(Candidate::new(
                leading + 2..leading + 2 + label.len(),
                SemanticClass::Keyword,
                FILE_FIELD_PRIORITY,
            ));
        }
        let value = value.trim_start();
        if value.is_empty() {
            return;
        }
        let value_start = text.len() - value.len();
        let class = if label == "file" {
            SemanticClass::Path
        } else {
            SemanticClass::Variable
        };
        if allows_class(class) {
            candidates.push(Candidate::new(
                value_start..text.len(),
                class,
                FILE_FIELD_PRIORITY,
            ));
        }
        return;
    }

    let mut fields = trimmed.split_whitespace();
    let Some(entry) = fields.next() else {
        return;
    };
    let entry_body = entry.strip_prefix("default:").unwrap_or(entry);
    let mut parts = entry_body.split(':');
    let (Some(kind), Some(identity), Some(bits), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return;
    };
    if !matches!(kind, "user" | "group" | "mask" | "other")
        || (matches!(kind, "mask" | "other") && !identity.is_empty())
        || !valid_acl_bits(bits)
    {
        return;
    }
    let last_colon = entry.len() - bits.len() - 1;
    let permission_start = leading + last_colon + 1;
    let permission_end = permission_start + bits.len();
    permissions::push_permission_characters(candidates, text, permission_start..permission_end);
    if let Some(effective) = fields
        .next()
        .and_then(|field| field.strip_prefix("#effective:"))
        && valid_acl_bits(effective)
        && let Some(offset) = trimmed[entry.len()..].find("#effective:")
    {
        let start = leading + entry.len() + offset + "#effective:".len();
        permissions::push_permission_characters(candidates, text, start..start + effective.len());
    }

    let mut segment_start = leading;
    let identity_column = if entry.starts_with("default:") { 2 } else { 1 };
    for (column, segment) in trimmed[..last_colon].split(':').enumerate() {
        let segment_end = segment_start + segment.len();
        if !segment.is_empty() {
            let class = if column == identity_column {
                SemanticClass::Variable
            } else {
                SemanticClass::Keyword
            };
            if allows_class(class) {
                candidates.push(Candidate::new(
                    segment_start..segment_end,
                    class,
                    FILE_FIELD_PRIORITY,
                ));
            }
        }
        segment_start = segment_end + 1;
    }
}

fn valid_acl_bits(bits: &str) -> bool {
    matches!(bits.as_bytes(), [b'r' | b'-', b'w' | b'-', b'x' | b'-'])
}

fn is_unsigned_integer(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_file_size(value: &str) -> bool {
    let number_end = value
        .bytes()
        .position(|byte| !byte.is_ascii_digit() && !matches!(byte, b'.' | b','))
        .unwrap_or(value.len());
    value.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && matches!(
            &value[number_end..],
            "" | "B"
                | "K"
                | "KB"
                | "KiB"
                | "M"
                | "MB"
                | "MiB"
                | "G"
                | "GB"
                | "GiB"
                | "T"
                | "TB"
                | "TiB"
                | "P"
                | "PB"
                | "PiB"
                | "E"
                | "EB"
                | "EiB"
        )
}
