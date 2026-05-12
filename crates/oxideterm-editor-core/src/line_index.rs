// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::TextEdit;

pub(crate) fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

pub(crate) fn update_line_starts_after_edits(
    old_starts: &[usize],
    edits: &[TextEdit],
) -> Vec<usize> {
    if edits.is_empty() {
        return old_starts.to_vec();
    }

    let mut starts = Vec::with_capacity(old_starts.len());
    starts.push(0);

    let mut previous_old_offset = 0;
    let mut shift: isize = 0;
    let mut old_start_index = 1;

    for edit in edits {
        while let Some(&line_start) = old_starts.get(old_start_index) {
            if line_start > edit.range.start.0 {
                break;
            }
            if line_start > previous_old_offset {
                push_line_start(&mut starts, apply_line_shift(line_start, shift));
            }
            old_start_index += 1;
        }

        let replacement_base = apply_line_shift(edit.range.start.0, shift);
        for (index, byte) in edit.replacement.bytes().enumerate() {
            if byte == b'\n' {
                push_line_start(&mut starts, replacement_base + index + 1);
            }
        }

        while let Some(&line_start) = old_starts.get(old_start_index) {
            if line_start > edit.range.end.0 {
                break;
            }
            old_start_index += 1;
        }

        shift += edit.replacement.len() as isize - edit.range.len() as isize;
        previous_old_offset = edit.range.end.0;
    }

    while let Some(&line_start) = old_starts.get(old_start_index) {
        if line_start > previous_old_offset {
            push_line_start(&mut starts, apply_line_shift(line_start, shift));
        }
        old_start_index += 1;
    }

    starts
}

fn push_line_start(starts: &mut Vec<usize>, offset: usize) {
    if starts.last().copied() != Some(offset) {
        starts.push(offset);
    }
}

fn apply_line_shift(offset: usize, shift: isize) -> usize {
    if shift < 0 {
        offset.saturating_sub(shift.unsigned_abs())
    } else {
        offset.saturating_add(shift as usize)
    }
}
