// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Column-aware disk and memory usage parsing selected from df and free command scopes.

use crate::{
    SemanticClass, SemanticLineRole,
    scheme::Candidate,
    tokens::{ranges, text_at},
};

const RESOURCE_FIELD_PRIORITY: u8 = 94;
const DISK_WARNING_PERCENT: u8 = 80;
const DISK_ERROR_PERCENT: u8 = 90;

pub(crate) fn line_candidates(
    text: &str,
    role: SemanticLineRole,
    allows_class: impl Fn(SemanticClass) -> bool,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    match role {
        SemanticLineRole::DiskUsageOutput => {
            if !push_header(text, &allows_class, &mut candidates) {
                push_disk_row(text, &allows_class, &mut candidates);
            }
        }
        SemanticLineRole::MemoryUsageOutput => {
            if !push_header(text, &allows_class, &mut candidates) {
                push_memory_row(text, &allows_class, &mut candidates);
            }
        }
        _ => {}
    }
    candidates
}

fn push_header(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) -> bool {
    let Some(first) = ranges(text).next() else {
        return false;
    };
    let Some(first_text) = text_at(text, &first) else {
        return false;
    };
    if !matches!(first_text, "Filesystem" | "total") {
        return false;
    }
    if !allows_class(SemanticClass::Keyword) {
        return true;
    }

    for range in ranges(text) {
        let Some(token) = text_at(text, &range) else {
            continue;
        };
        if matches!(
            token,
            "Filesystem"
                | "Type"
                | "Size"
                | "Used"
                | "Avail"
                | "Use%"
                | "Mounted"
                | "on"
                | "total"
                | "used"
                | "free"
                | "shared"
                | "buff/cache"
                | "available"
        ) {
            candidates.push(Candidate::new(
                range,
                SemanticClass::Keyword,
                RESOURCE_FIELD_PRIORITY,
            ));
        }
    }
    true
}

fn push_memory_row(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) -> bool {
    let mut fields = ranges(text);
    let Some(label) = fields.next() else {
        return false;
    };
    if !text_at(text, &label).is_some_and(|value| matches!(value, "Mem:" | "Swap:")) {
        return false;
    }
    let Some(first_value) = fields
        .next()
        .filter(|range| text_at(text, range).is_some_and(is_quantity))
    else {
        return false;
    };

    if allows_class(SemanticClass::Keyword) {
        candidates.push(Candidate::new(
            label.start..label.end - 1,
            SemanticClass::Keyword,
            RESOURCE_FIELD_PRIORITY,
        ));
    }
    if allows_class(SemanticClass::Number) {
        candidates.push(Candidate::new(
            first_value,
            SemanticClass::Number,
            RESOURCE_FIELD_PRIORITY,
        ));
        for range in fields
            .take(5)
            .filter(|range| text_at(text, range).is_some_and(is_quantity))
        {
            candidates.push(Candidate::new(
                range,
                SemanticClass::Number,
                RESOURCE_FIELD_PRIORITY,
            ));
        }
    }
    true
}

fn push_disk_row(
    text: &str,
    allows_class: &impl Fn(SemanticClass) -> bool,
    candidates: &mut Vec<Candidate>,
) {
    let mut fields = ranges(text);
    let (
        Some(filesystem),
        Some(mut size),
        Some(mut used),
        Some(mut available),
        Some(mut usage),
        Some(mut mount),
    ) = (
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
    )
    else {
        return;
    };
    // GNU df -T inserts the filesystem type before the three quantity columns.
    let filesystem_type = if !is_quantity(&text[size.clone()]) {
        let Some(next) = fields.next() else {
            return;
        };
        let kind = size;
        size = used;
        used = available;
        available = usage;
        usage = mount;
        mount = next;
        Some(kind)
    } else {
        None
    };
    let Some(percent) = text_at(text, &usage).and_then(parse_percent) else {
        return;
    };
    if ![&size, &used, &available]
        .into_iter()
        .all(|range| text_at(text, range).is_some_and(is_quantity))
    {
        return;
    }
    if let Some(kind) = filesystem_type
        && allows_class(SemanticClass::Keyword)
    {
        candidates.push(Candidate::new(
            kind,
            SemanticClass::Keyword,
            RESOURCE_FIELD_PRIORITY,
        ));
    }
    // BSD df includes inode columns between block usage and the mount point.
    if !text[mount.clone()].starts_with('/') {
        let Some(inodes_available) = fields.next() else {
            return;
        };
        let Some(inode_percent) = fields.next() else {
            return;
        };
        let Some(path) = fields.next() else {
            return;
        };
        if !is_quantity(&text[mount.clone()])
            || !is_quantity(&text[inodes_available])
            || parse_percent(&text[inode_percent]).is_none()
            || !text[path.clone()].starts_with('/')
        {
            return;
        }
        mount = path;
    }
    // Mount points may contain spaces; preserve their complete display value.
    mount.end = text.trim_end().len();

    let filesystem_class = if text_at(text, &filesystem).is_some_and(|value| value.contains('/')) {
        SemanticClass::Path
    } else {
        SemanticClass::Variable
    };
    if allows_class(filesystem_class) {
        candidates.push(Candidate::new(
            filesystem,
            filesystem_class,
            RESOURCE_FIELD_PRIORITY,
        ));
    }
    if allows_class(SemanticClass::Path) {
        candidates.push(Candidate::new(
            mount,
            SemanticClass::Path,
            RESOURCE_FIELD_PRIORITY,
        ));
    }
    if allows_class(SemanticClass::Number) {
        for range in [size, used, available] {
            candidates.push(Candidate::new(
                range,
                SemanticClass::Number,
                RESOURCE_FIELD_PRIORITY,
            ));
        }
    }
    let usage_class = if percent >= DISK_ERROR_PERCENT {
        SemanticClass::Error
    } else if percent >= DISK_WARNING_PERCENT {
        SemanticClass::Warning
    } else {
        // Healthy capacity is informational; green is reserved for explicit success states.
        SemanticClass::Number
    };
    if allows_class(usage_class) {
        candidates.push(Candidate::new(usage, usage_class, RESOURCE_FIELD_PRIORITY));
    }
}

fn parse_percent(value: &str) -> Option<u8> {
    value.strip_suffix('%')?.parse().ok()
}

fn is_quantity(value: &str) -> bool {
    let number_end = value
        .bytes()
        .position(|byte| !byte.is_ascii_digit() && !matches!(byte, b'.' | b','))
        .unwrap_or(value.len());
    value.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && matches!(
            &value[number_end..],
            "" | "B"
                | "k"
                | "K"
                | "KB"
                | "KiB"
                | "Ki"
                | "M"
                | "MB"
                | "MiB"
                | "Mi"
                | "G"
                | "GB"
                | "GiB"
                | "Gi"
                | "T"
                | "TB"
                | "TiB"
                | "Ti"
                | "P"
                | "PB"
                | "PiB"
                | "Pi"
                | "E"
                | "EB"
                | "EiB"
                | "Ei"
        )
}
