// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::TextEdit;

#[derive(Clone, Debug)]
pub(crate) enum LineIndex {
    Compact(Vec<u32>),
    Wide(Vec<usize>),
}

impl LineIndex {
    pub(crate) fn from_chunks<'a>(
        chunks: impl Iterator<Item = &'a str> + Clone,
        len: usize,
    ) -> Self {
        fn starts<'a, T: TryFrom<usize>>(chunks: impl Iterator<Item = &'a str> + Clone) -> Vec<T> {
            // Match contiguous opening: reserve exactly once instead of growing a full row index.
            let lines = 1 + chunks
                .clone()
                .map(|text| text.matches('\n').count())
                .sum::<usize>();
            let mut starts = Vec::with_capacity(lines);
            starts.push(stored_offset(0));
            let mut base = 0;
            for chunk in chunks {
                for (index, _) in chunk.match_indices('\n') {
                    starts.push(stored_offset(base + index + 1));
                }
                base += chunk.len();
            }
            starts
        }
        if u32::try_from(len).is_ok() {
            Self::Compact(starts(chunks))
        } else {
            Self::Wide(starts(chunks))
        }
    }

    pub(crate) fn new(text: &str) -> Self {
        if u32::try_from(text.len()).is_ok() {
            Self::Compact(compute_line_starts(text))
        } else {
            Self::Wide(compute_line_starts(text))
        }
    }

    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Compact(starts) => starts.len(),
            Self::Wide(starts) => starts.len(),
        }
    }

    pub(crate) fn get(&self, line: usize) -> Option<usize> {
        match self {
            Self::Compact(starts) => starts.get(line).map(|&offset| offset as usize),
            Self::Wide(starts) => starts.get(line).copied(),
        }
    }

    pub(crate) fn line_for_offset(&self, offset: usize) -> usize {
        let next = match self {
            Self::Compact(starts) => starts.partition_point(|&start| start as usize <= offset),
            Self::Wide(starts) => starts.partition_point(|&start| start <= offset),
        };
        next.saturating_sub(1)
    }

    pub(crate) fn update(&mut self, edits: &[TextEdit], new_len: usize) {
        if u32::try_from(new_len).is_err()
            && let Self::Compact(starts) = self
        {
            *self = Self::Wide(starts.iter().map(|&offset| offset as usize).collect());
        }
        match self {
            Self::Compact(starts) => update_line_starts_after_edits(starts, edits),
            Self::Wide(starts) => update_line_starts_after_edits(starts, edits),
        }
    }
}

fn offset<T: Copy + TryInto<usize>>(value: T) -> usize {
    value.try_into().ok().expect("line offset fits usize")
}

fn stored_offset<T: TryFrom<usize>>(value: usize) -> T {
    T::try_from(value)
        .ok()
        .expect("line index width covers document")
}

fn compute_line_starts<T: TryFrom<usize>>(text: &str) -> Vec<T> {
    // Opening a file knows its exact line count; avoid geometric spare capacity
    // on the largest per-line allocation. Subsequent edits can grow normally.
    let mut starts = Vec::with_capacity(1 + text.bytes().filter(|&byte| byte == b'\n').count());
    starts.push(stored_offset(0));
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(stored_offset(index + 1));
        }
    }
    starts
}

fn update_line_starts_after_edits<T: Copy + TryFrom<usize> + TryInto<usize>>(
    starts: &mut Vec<T>,
    edits: &[TextEdit],
) {
    let Some(first_edit) = edits.first() else {
        return;
    };
    let first = starts.partition_point(|&value| offset(value) <= first_edit.range.start.0);
    let last = starts.partition_point(|&value| offset(value) <= edits.last().unwrap().range.end.0);
    let mut middle = Vec::new();
    let mut old_index = first;
    let mut shift: isize = 0;
    for edit in edits {
        while old_index < last && offset(starts[old_index]) <= edit.range.start.0 {
            push_line_start(
                &mut middle,
                apply_line_shift(offset(starts[old_index]), shift),
            );
            old_index += 1;
        }
        let base = apply_line_shift(edit.range.start.0, shift);
        let mut replacement_offset = 0;
        for chunk in edit.replacement.chunks() {
            for (index, _) in chunk.match_indices('\n') {
                push_line_start(&mut middle, base + replacement_offset + index + 1);
            }
            replacement_offset += chunk.len();
        }
        while old_index < last && offset(starts[old_index]) <= edit.range.end.0 {
            old_index += 1;
        }
        shift += edit.replacement.len() as isize - edit.range.len() as isize;
    }
    let suffix = first + middle.len();
    // Preserve the unaffected prefix and reuse the allocation. Absolute offsets
    // after a byte-length change still need one shift, regardless of edit count.
    starts.splice(first..last, middle);
    if shift != 0 {
        for value in &mut starts[suffix..] {
            *value = stored_offset(apply_line_shift(offset(*value), shift));
        }
    }
    if starts.capacity() > starts.len().saturating_mul(2) {
        starts.shrink_to(starts.len());
    }
}

fn push_line_start<T: Copy + TryFrom<usize> + TryInto<usize>>(starts: &mut Vec<T>, value: usize) {
    if starts.last().copied().map(offset) != Some(value) {
        starts.push(stored_offset(value));
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
    fn chunked_opening_keeps_absolute_offsets_without_spare_line_capacity() {
        let LineIndex::Compact(starts) =
            LineIndex::from_chunks(["", "a\n中", "🙂\n", "tail\n"].into_iter(), 15)
        else {
            panic!("small source uses compact offsets");
        };
        assert_eq!(starts, [0, 2, 10, 15]);
        assert_eq!(starts.capacity(), 4);
    }

    #[test]
    fn crossing_the_compact_limit_keeps_absolute_line_offsets() {
        if usize::BITS <= 32 {
            return;
        }
        let limit = u32::MAX as usize;
        let mut starts = LineIndex::Compact(vec![0, u32::MAX - 2]);
        starts.update(&[TextEdit::insert(BufferOffset(1), "abc\n")], limit + 4);
        assert_eq!(
            (0..starts.len())
                .map(|line| starts.get(line).unwrap())
                .collect::<Vec<_>>(),
            [0, 5, limit + 2]
        );
        assert_eq!(starts.line_for_offset(limit + 3), 2);
        starts.update(
            &[TextEdit::new(
                TextRange::new(BufferOffset(1), BufferOffset(5)),
                "",
            )],
            limit,
        );
        assert_eq!(
            (0..starts.len())
                .map(|line| starts.get(line).unwrap())
                .collect::<Vec<_>>(),
            [0, limit - 2]
        );
    }

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
            let mut starts = compute_line_starts::<u32>(source);
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
        let mut starts = compute_line_starts::<u32>(&"row\n".repeat(100_000));
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
