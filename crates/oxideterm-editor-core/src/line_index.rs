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

pub(crate) fn update_line_starts_after_edits(starts: &mut Vec<usize>, edits: &[TextEdit]) {
    let Some(first_edit) = edits.first() else {
        return;
    };
    let first = starts.partition_point(|&offset| offset <= first_edit.range.start.0);
    let last = starts.partition_point(|&offset| offset <= edits.last().unwrap().range.end.0);
    let mut middle = Vec::new();
    let mut old_index = first;
    let mut shift: isize = 0;
    for edit in edits {
        while old_index < last && starts[old_index] <= edit.range.start.0 {
            push_line_start(&mut middle, apply_line_shift(starts[old_index], shift));
            old_index += 1;
        }
        let base = apply_line_shift(edit.range.start.0, shift);
        for (index, byte) in edit.replacement.bytes().enumerate() {
            if byte == b'\n' {
                push_line_start(&mut middle, base + index + 1);
            }
        }
        while old_index < last && starts[old_index] <= edit.range.end.0 {
            old_index += 1;
        }
        shift += edit.replacement.len() as isize - edit.range.len() as isize;
    }
    let suffix = first + middle.len();
    // Preserve the unaffected prefix and reuse the allocation. Absolute offsets
    // after a byte-length change still need one shift, regardless of edit count.
    starts.splice(first..last, middle);
    if shift != 0 {
        for offset in &mut starts[suffix..] {
            *offset = apply_line_shift(*offset, shift);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BufferOffset, TextRange};

    #[test]
    fn local_updates_preserve_line_boundaries_across_batched_edits() {
        for (source, changes, expected) in [
            (
                "a\n中\nz",
                vec![(0, 0, "\n"), (2, 5, "X\nY"), (6, 7, "")],
                vec![0, 1, 3, 5, 7],
            ),
            ("a\nb\n", vec![(0, 4, "")], vec![0]),
            (
                "a\nb\nc",
                vec![(0, 2, "x\n"), (2, 4, ""), (4, 5, "z\n")],
                vec![0, 2, 4],
            ),
            ("a\nb", vec![(1, 2, "")], vec![0]),
            ("a\nb", vec![(2, 2, "\n")], vec![0, 2, 3]),
            ("a\nb", vec![(0, 0, ""), (3, 3, "\n")], vec![0, 2, 4]),
            ("", vec![(0, 0, "中\n\n")], vec![0, 4, 5]),
        ] {
            let mut starts = compute_line_starts(source);
            let edits: Vec<_> = changes
                .into_iter()
                .map(|(start, end, text)| {
                    TextEdit::new(TextRange::new(BufferOffset(start), BufferOffset(end)), text)
                })
                .collect();
            update_line_starts_after_edits(&mut starts, &edits);
            assert_eq!(starts, expected, "{source:?}: {edits:?}");
        }
    }

    #[test]
    fn edits_without_newlines_reuse_the_line_index_allocation() {
        let mut starts = compute_line_starts(&"row\n".repeat(100_000));
        let allocation = starts.as_ptr();
        update_line_starts_after_edits(&mut starts, &[TextEdit::insert(BufferOffset(0), "中")]);
        assert_eq!(starts.as_ptr(), allocation);
        assert_eq!(starts[1], 7);
        assert_eq!(starts.last(), Some(&400003));
        update_line_starts_after_edits(
            &mut starts,
            &[TextEdit::new(
                TextRange::new(BufferOffset(4), BufferOffset(5)),
                "z",
            )],
        );
        assert_eq!(starts.as_ptr(), allocation);
        assert_eq!(starts[1], 7);
        assert_eq!(starts.last(), Some(&400003));
    }
}
