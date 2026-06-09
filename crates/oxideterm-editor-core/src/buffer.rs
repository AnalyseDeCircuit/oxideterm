// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{cell::RefCell, cmp::Ordering};

use unicode_segmentation::UnicodeSegmentation;

use crate::{
    BufferOffset, EditTransaction, EditorError, LineCol, Selection, TextEdit, TextRange,
    line_index::{compute_line_starts, update_line_starts_after_edits},
    piece_table::PieceTableTextBuffer,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct HistoryEntry {
    edits: Vec<TextEdit>,
    before_revision: u64,
    after_revision: u64,
}

/// Editable text buffer with line lookup, transactions, undo/redo, and dirty state.
#[derive(Clone, Debug)]
pub struct TextBuffer {
    storage: PieceTableTextBuffer,
    text_cache: RefCell<Option<String>>,
    line_starts: Vec<usize>,
    version: u64,
    content_revision: u64,
    saved_revision: u64,
    next_content_revision: u64,
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
}

impl TextBuffer {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let line_starts = compute_line_starts(&text);
        let storage = PieceTableTextBuffer::new(text.clone());
        Self {
            storage,
            text_cache: RefCell::new(Some(text)),
            line_starts,
            version: 0,
            content_revision: 0,
            saved_revision: 0,
            next_content_revision: 1,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn text(&self) -> String {
        self.with_text(str::to_string)
    }

    pub fn with_text<R>(&self, f: impl FnOnce(&str) -> R) -> R {
        if self.text_cache.borrow().is_none() {
            *self.text_cache.borrow_mut() = Some(self.storage.to_text());
        }
        let cache = self.text_cache.borrow();
        f(cache
            .as_deref()
            .expect("text cache should be materialized before callback"))
    }

    pub fn len(&self) -> usize {
        self.storage.len()
    }

    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    pub fn is_dirty(&self) -> bool {
        self.content_revision != self.saved_revision
    }

    pub fn mark_saved(&mut self) {
        self.saved_revision = self.content_revision;
    }

    pub fn slice(&self, range: TextRange) -> Result<String, EditorError> {
        self.validate_range(range)?;
        Ok(self.storage.slice_to_string(range.as_range()))
    }

    pub fn line_text(&self, line: usize) -> Option<String> {
        let start = *self.line_starts.get(line)?;
        let end = self.line_end_offset(line)?.0;
        Some(self.storage.slice_to_string(start..end))
    }

    pub fn with_line_text<R>(&self, line: usize, f: impl FnOnce(&str) -> R) -> Option<R> {
        let start = *self.line_starts.get(line)?;
        let end = self.line_end_offset(line)?.0;
        // Rendering hot paths need a borrowed line view. Materialize the shared
        // text cache once and slice it instead of allocating a fresh line string.
        self.with_text(|text| text.get(start..end).map(f))
    }

    pub fn line_char_counts(&self) -> Vec<usize> {
        // Soft-wrap layout may scan every line after width changes. Count from
        // the materialized text cache so layout does not allocate one string per
        // line before it can rebuild display rows.
        self.with_text(|text| {
            (0..self.line_starts.len())
                .map(|line| {
                    let start = self.line_starts[line];
                    let end = self
                        .line_starts
                        .get(line + 1)
                        .copied()
                        .map(|next| next.saturating_sub(1))
                        .unwrap_or_else(|| text.len())
                        .max(start);
                    text.get(start..end)
                        .map(|line_text| line_text.chars().count())
                        .unwrap_or_default()
                })
                .collect()
        })
    }

    pub fn offset_to_line_col(&self, offset: BufferOffset) -> Result<LineCol, EditorError> {
        self.validate_offset(offset)?;
        let line = match self.line_starts.binary_search(&offset.0) {
            Ok(line) => line,
            Err(next_line) => next_line.saturating_sub(1),
        };
        Ok(LineCol::new(line, offset.0 - self.line_starts[line]))
    }

    pub fn line_col_to_offset(&self, position: LineCol) -> Result<BufferOffset, EditorError> {
        let start = *self
            .line_starts
            .get(position.line)
            .ok_or(EditorError::InvalidLine {
                line: position.line,
                line_count: self.line_count(),
            })?;
        let line_end = self
            .line_end_offset(position.line)
            .ok_or(EditorError::InvalidLine {
                line: position.line,
                line_count: self.line_count(),
            })?
            .0;
        let offset = start + position.column;
        if offset > line_end {
            return Err(EditorError::InvalidColumn {
                line: position.line,
                column: position.column,
                line_len: line_end - start,
            });
        }
        self.validate_offset(BufferOffset(offset))?;
        Ok(BufferOffset(offset))
    }

    pub fn line_start_offset(&self, line: usize) -> Option<BufferOffset> {
        self.line_starts.get(line).copied().map(BufferOffset)
    }

    pub fn line_end_offset(&self, line: usize) -> Option<BufferOffset> {
        let start = *self.line_starts.get(line)?;
        let next_start = self.line_starts.get(line + 1).copied();
        let end = next_start
            .map(|next| next.saturating_sub(1))
            .unwrap_or_else(|| self.storage.len());
        Some(BufferOffset(end.max(start)))
    }

    pub fn next_grapheme_offset(&self, offset: BufferOffset) -> BufferOffset {
        if self.validate_offset(offset).is_err() || offset.0 >= self.storage.len() {
            return BufferOffset(self.storage.len());
        }
        self.with_text(|text| {
            let remaining = &text[offset.0..];
            let next_len = remaining.graphemes(true).next().map(str::len).unwrap_or(0);
            BufferOffset((offset.0 + next_len).min(text.len()))
        })
    }

    pub fn previous_grapheme_offset(&self, offset: BufferOffset) -> BufferOffset {
        if self.validate_offset(offset).is_err() || offset.0 == 0 {
            return BufferOffset::ZERO;
        }
        self.with_text(|text| {
            let prefix = &text[..offset.0];
            let previous_len = prefix
                .graphemes(true)
                .next_back()
                .map(str::len)
                .unwrap_or(0);
            BufferOffset(offset.0.saturating_sub(previous_len))
        })
    }

    pub fn apply_transaction(&mut self, transaction: EditTransaction) -> Result<(), EditorError> {
        if transaction.is_empty() {
            return Ok(());
        }
        let before_revision = self.content_revision;
        let after_revision = self.allocate_content_revision();
        let inverse = self.apply_edits_internal(transaction.into_edits())?;
        // Dirty tracking follows content revisions instead of the notification
        // version so undoing back to the saved content can clear the dirty flag.
        self.content_revision = after_revision;
        self.undo_stack.push(HistoryEntry {
            edits: inverse,
            before_revision,
            after_revision,
        });
        self.redo_stack.clear();
        Ok(())
    }

    pub fn replace_selection(
        &mut self,
        selection: Selection,
        replacement: impl Into<String>,
    ) -> Result<Selection, EditorError> {
        let range = selection.range();
        let replacement = replacement.into();
        let caret = BufferOffset(range.start.0 + replacement.len());
        self.apply_transaction(EditTransaction::single(TextEdit::new(range, replacement)))?;
        Ok(Selection::caret(caret))
    }

    pub fn undo(&mut self) -> Result<bool, EditorError> {
        let Some(entry) = self.undo_stack.pop() else {
            return Ok(false);
        };
        let redo = self.apply_edits_internal(entry.edits)?;
        // Undo restores the exact content revision that existed before the
        // transaction. This keeps dirty state independent from stack movement.
        self.content_revision = entry.before_revision;
        self.redo_stack.push(HistoryEntry {
            edits: redo,
            before_revision: entry.before_revision,
            after_revision: entry.after_revision,
        });
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, EditorError> {
        let Some(entry) = self.redo_stack.pop() else {
            return Ok(false);
        };
        let undo = self.apply_edits_internal(entry.edits)?;
        // Redo returns to the transaction's original after-revision instead of
        // minting a new one, preserving save/dirty parity with the first edit.
        self.content_revision = entry.after_revision;
        self.undo_stack.push(HistoryEntry {
            edits: undo,
            before_revision: entry.before_revision,
            after_revision: entry.after_revision,
        });
        Ok(true)
    }

    fn allocate_content_revision(&mut self) -> u64 {
        let revision = self.next_content_revision;
        self.next_content_revision = self.next_content_revision.saturating_add(1);
        revision
    }

    fn apply_edits_internal(&mut self, edits: Vec<TextEdit>) -> Result<Vec<TextEdit>, EditorError> {
        let edits = normalize_edits_for_storage(edits, &self.storage)?;
        let mut inverse = Vec::with_capacity(edits.len());
        let mut delta: isize = 0;

        for edit in &edits {
            let original = self.storage.slice_to_string(edit.range.as_range());
            let start_after = apply_delta(edit.range.start.0, delta)?;
            let end_after = start_after + edit.replacement.len();
            inverse.push(TextEdit::new(
                TextRange::new(BufferOffset(start_after), BufferOffset(end_after)),
                original,
            ));
            delta += edit.replacement.len() as isize - edit.range.len() as isize;
        }

        let next_line_starts = update_line_starts_after_edits(&self.line_starts, &edits);

        for edit in edits.iter().rev() {
            self.storage
                .replace(edit.range.as_range(), &edit.replacement);
        }
        // Syntax, save, search, and IME still require contiguous text at their
        // API boundary. Keep that as an explicit on-demand cache instead of
        // forcing every edit through full-document materialization.
        *self.text_cache.borrow_mut() = None;
        self.line_starts = next_line_starts;
        self.version = self.version.saturating_add(1);
        Ok(inverse)
    }

    fn validate_offset(&self, offset: BufferOffset) -> Result<(), EditorError> {
        if offset.0 > self.storage.len() {
            return Err(EditorError::OffsetOutOfBounds {
                offset: offset.0,
                len: self.storage.len(),
            });
        }
        if !self.storage.is_char_boundary(offset.0) {
            return Err(EditorError::InvalidUtf8Boundary { offset: offset.0 });
        }
        Ok(())
    }

    fn validate_range(&self, range: TextRange) -> Result<(), EditorError> {
        self.validate_offset(range.start)?;
        self.validate_offset(range.end)?;
        if range.start > range.end {
            return Err(EditorError::InvalidRange {
                start: range.start.0,
                end: range.end.0,
            });
        }
        Ok(())
    }
}

fn normalize_edits_for_storage(
    edits: Vec<TextEdit>,
    storage: &PieceTableTextBuffer,
) -> Result<Vec<TextEdit>, EditorError> {
    let mut edits = edits;
    edits.sort_by(|left, right| {
        left.range
            .start
            .cmp(&right.range.start)
            .then_with(|| left.range.end.cmp(&right.range.end))
    });

    let mut previous_end = BufferOffset::ZERO;
    for edit in &edits {
        validate_offset_for_storage(storage, edit.range.start)?;
        validate_offset_for_storage(storage, edit.range.end)?;
        if edit.range.start > edit.range.end {
            return Err(EditorError::InvalidRange {
                start: edit.range.start.0,
                end: edit.range.end.0,
            });
        }
        if edit.range.start < previous_end {
            return Err(EditorError::OverlappingEdits {
                offset: edit.range.start.0,
            });
        }
        previous_end = edit.range.end;
    }

    Ok(edits)
}

fn validate_offset_for_storage(
    storage: &PieceTableTextBuffer,
    offset: BufferOffset,
) -> Result<(), EditorError> {
    if offset.0 > storage.len() {
        return Err(EditorError::OffsetOutOfBounds {
            offset: offset.0,
            len: storage.len(),
        });
    }
    if !storage.is_char_boundary(offset.0) {
        return Err(EditorError::InvalidUtf8Boundary { offset: offset.0 });
    }
    Ok(())
}

fn apply_delta(offset: usize, delta: isize) -> Result<usize, EditorError> {
    match delta.cmp(&0) {
        Ordering::Less => offset
            .checked_sub(delta.unsigned_abs())
            .ok_or(EditorError::EditDeltaOverflow),
        Ordering::Equal => Ok(offset),
        Ordering::Greater => offset
            .checked_add(delta as usize)
            .ok_or(EditorError::EditDeltaOverflow),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cursor, piece_table::PieceSource};

    #[test]
    fn maps_unicode_offsets_to_line_columns() {
        let buffer = TextBuffer::new("aé\n你b\nlast");

        assert_eq!(
            buffer.offset_to_line_col(BufferOffset(3)).unwrap(),
            LineCol::new(0, 3)
        );
        assert_eq!(
            buffer.offset_to_line_col(BufferOffset(4)).unwrap(),
            LineCol::new(1, 0)
        );
        assert_eq!(
            buffer.line_col_to_offset(LineCol::new(1, 3)).unwrap(),
            BufferOffset(7)
        );
        assert_eq!(buffer.line_count(), 3);
        assert_eq!(buffer.line_text(1), Some("你b".to_string()));
    }

    #[test]
    fn borrowed_line_text_matches_owned_line_text() {
        let buffer = TextBuffer::new("alpha\n你b\nlast");

        assert_eq!(
            buffer.with_line_text(1, str::to_string),
            buffer.line_text(1)
        );
        assert_eq!(buffer.with_line_text(99, str::len), None);
    }

    #[test]
    fn line_char_counts_match_visible_line_text() {
        let buffer = TextBuffer::new("aé\n你b\n");

        assert_eq!(buffer.line_char_counts(), vec![2, 2, 0]);
    }

    #[test]
    fn trailing_newline_creates_empty_final_line() {
        let buffer = TextBuffer::new("one\n");

        assert_eq!(buffer.line_count(), 2);
        assert_eq!(buffer.line_text(0), Some("one".to_string()));
        assert_eq!(buffer.line_text(1), Some(String::new()));
        assert_eq!(
            buffer.line_col_to_offset(LineCol::new(1, 0)).unwrap(),
            BufferOffset(4)
        );
    }

    #[test]
    fn moves_by_grapheme_boundaries() {
        let buffer = TextBuffer::new("a🇨🇳é");
        let after_a = buffer.next_grapheme_offset(BufferOffset(0));
        let after_flag = buffer.next_grapheme_offset(after_a);
        let end = buffer.next_grapheme_offset(after_flag);

        assert_eq!(after_a, BufferOffset(1));
        let text = buffer.text();
        assert_eq!(&text[after_a.0..after_flag.0], "🇨🇳");
        assert_eq!(&text[after_flag.0..end.0], "é");
        assert_eq!(buffer.previous_grapheme_offset(end), after_flag);
    }

    #[test]
    fn applies_multiline_range_edits_and_updates_line_index() {
        let mut buffer = TextBuffer::new("one\ntwo\nthree");

        buffer
            .apply_transaction(EditTransaction::single(TextEdit::new(
                TextRange::new(BufferOffset(4), BufferOffset(7)),
                "2\nII",
            )))
            .unwrap();

        assert_eq!(buffer.text(), "one\n2\nII\nthree");
        assert_eq!(buffer.line_count(), 4);
        assert_eq!(buffer.line_text(2), Some("II".to_string()));
        assert!(buffer.is_dirty());
    }

    #[test]
    fn updates_line_index_across_deleted_and_inserted_newlines() {
        let mut buffer = TextBuffer::new("alpha\nbravo\ncharlie\ndelta");

        buffer
            .apply_transaction(EditTransaction::single(TextEdit::new(
                TextRange::new(BufferOffset(8), BufferOffset(20)),
                "R\nS\nT\n",
            )))
            .unwrap();

        assert_eq!(buffer.text(), "alpha\nbrR\nS\nT\ndelta");
        assert_eq!(buffer.line_count(), 5);
        assert_eq!(buffer.line_text(1), Some("brR".to_string()));
        assert_eq!(buffer.line_text(2), Some("S".to_string()));
        assert_eq!(buffer.line_text(3), Some("T".to_string()));
        assert_eq!(buffer.line_text(4), Some("delta".to_string()));
        assert_eq!(
            buffer
                .offset_to_line_col(BufferOffset(buffer.len()))
                .unwrap(),
            LineCol::new(4, 5)
        );
    }

    #[test]
    fn stores_edits_as_piece_table_appends() {
        let mut buffer = TextBuffer::new("hello world");

        buffer
            .apply_transaction(EditTransaction::new(vec![
                TextEdit::new(TextRange::new(BufferOffset(0), BufferOffset(5)), "hi"),
                TextEdit::insert(BufferOffset(11), "!"),
            ]))
            .unwrap();

        assert_eq!(buffer.text(), "hi world!");
        assert_eq!(buffer.storage.original, "hello world");
        assert!(buffer.storage.add.contains("hi"));
        assert!(buffer.storage.add.contains('!'));
        assert!(
            buffer
                .storage
                .pieces
                .iter()
                .any(|piece| piece.source == PieceSource::Add)
        );
    }

    #[test]
    fn piece_table_handles_middle_replacement_without_touching_original() {
        let mut buffer = TextBuffer::new("alpha\nbeta\ngamma");

        buffer
            .apply_transaction(EditTransaction::single(TextEdit::new(
                TextRange::new(BufferOffset(6), BufferOffset(10)),
                "BETA\nextra",
            )))
            .unwrap();

        assert_eq!(buffer.text(), "alpha\nBETA\nextra\ngamma");
        assert_eq!(buffer.storage.original, "alpha\nbeta\ngamma");
        assert_eq!(buffer.line_count(), 4);
        assert_eq!(buffer.line_text(2), Some("extra".to_string()));
    }

    #[test]
    fn rejects_overlapping_or_invalid_edits() {
        let mut buffer = TextBuffer::new("abcdef");

        let error = buffer
            .apply_transaction(EditTransaction::new(vec![
                TextEdit::new(TextRange::new(BufferOffset(1), BufferOffset(4)), "x"),
                TextEdit::new(TextRange::new(BufferOffset(3), BufferOffset(5)), "y"),
            ]))
            .unwrap_err();

        assert_eq!(error, EditorError::OverlappingEdits { offset: 3 });
        assert_eq!(buffer.text(), "abcdef");

        let mut unicode_buffer = TextBuffer::new("éx");
        let error = unicode_buffer
            .apply_transaction(EditTransaction::single(TextEdit::insert(
                BufferOffset(1),
                "x",
            )))
            .unwrap_err();
        assert_eq!(error, EditorError::InvalidUtf8Boundary { offset: 1 });
    }

    #[test]
    fn replaces_selection_and_returns_new_caret() {
        let mut buffer = TextBuffer::new("hello world");
        let selection = Selection::new(BufferOffset(6), BufferOffset(11));

        let next_selection = buffer.replace_selection(selection, "OxideTerm").unwrap();

        assert_eq!(buffer.text(), "hello OxideTerm");
        assert_eq!(next_selection, Selection::caret(BufferOffset(15)));
    }

    #[test]
    fn undo_redo_restores_multiline_transaction() {
        let mut buffer = TextBuffer::new("alpha\nbeta\ngamma");
        buffer.mark_saved();

        buffer
            .apply_transaction(EditTransaction::new(vec![
                TextEdit::new(TextRange::new(BufferOffset(0), BufferOffset(5)), "A"),
                TextEdit::insert(BufferOffset(11), "B2\n"),
            ]))
            .unwrap();
        assert_eq!(buffer.text(), "A\nbeta\nB2\ngamma");
        assert!(buffer.is_dirty());

        assert!(buffer.undo().unwrap());
        assert_eq!(buffer.text(), "alpha\nbeta\ngamma");
        assert!(!buffer.is_dirty());

        assert!(buffer.redo().unwrap());
        assert_eq!(buffer.text(), "A\nbeta\nB2\ngamma");
        assert!(buffer.is_dirty());
    }

    #[test]
    fn cursor_extends_and_collapses_selection() {
        let buffer = TextBuffer::new("你a");
        let mut cursor = Cursor::new(BufferOffset(0));

        cursor.move_right(&buffer, true);
        assert_eq!(
            cursor.selection(),
            Selection::new(BufferOffset(0), BufferOffset(3))
        );

        cursor.move_right(&buffer, false);
        assert_eq!(cursor.selection(), Selection::caret(BufferOffset(3)));

        cursor.move_left(&buffer, false);
        assert_eq!(cursor.selection(), Selection::caret(BufferOffset(0)));

        cursor.set_selection(Selection::new(BufferOffset(4), BufferOffset(0)));
        cursor.move_right(&buffer, false);
        assert_eq!(cursor.selection(), Selection::caret(BufferOffset(4)));
    }
}
