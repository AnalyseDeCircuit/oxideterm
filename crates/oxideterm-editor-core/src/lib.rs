// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! UI-independent editor core for OxideTerm.
//!
//! This crate deliberately owns the public text, cursor, selection, and undo
//! types used by later GPUI/SFTP/IDE layers. The public API stays byte-offset
//! based while the internal storage uses a piece table so edits do not mutate a
//! single monolithic `String`.

use std::{cell::RefCell, cmp::Ordering, ops::Range};

use thiserror::Error;
use unicode_segmentation::UnicodeSegmentation;

mod multicursor;
mod search;

pub use multicursor::CursorSet;
pub use search::{FindMatch, FindOptions, find_all, replace_all_transaction};

/// Byte offset inside the buffer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BufferOffset(pub usize);

impl BufferOffset {
    pub const ZERO: Self = Self(0);

    pub fn as_usize(self) -> usize {
        self.0
    }
}

/// Zero-based line and byte-column position.
///
/// The column is a UTF-8 byte offset within the line, not a displayed cell or
/// UTF-16 column. Keeping this explicit prevents IME, SFTP, and syntax layers
/// from silently mixing coordinate systems.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct LineCol {
    pub line: usize,
    pub column: usize,
}

impl LineCol {
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// Half-open byte range `[start, end)` inside the buffer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct TextRange {
    pub start: BufferOffset,
    pub end: BufferOffset,
}

impl TextRange {
    pub fn new(start: BufferOffset, end: BufferOffset) -> Self {
        if start <= end {
            Self { start, end }
        } else {
            Self {
                start: end,
                end: start,
            }
        }
    }

    pub fn caret(offset: BufferOffset) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    pub fn len(self) -> usize {
        self.end.0 - self.start.0
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    fn as_range(self) -> Range<usize> {
        self.start.0..self.end.0
    }
}

/// A single range replacement in byte coordinates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEdit {
    pub range: TextRange,
    pub replacement: String,
}

impl TextEdit {
    pub fn new(range: TextRange, replacement: impl Into<String>) -> Self {
        Self {
            range,
            replacement: replacement.into(),
        }
    }

    pub fn insert(offset: BufferOffset, text: impl Into<String>) -> Self {
        Self::new(TextRange::caret(offset), text)
    }
}

/// A logical editing action that should undo/redo as one unit.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EditTransaction {
    edits: Vec<TextEdit>,
}

impl EditTransaction {
    pub fn new(edits: Vec<TextEdit>) -> Self {
        Self { edits }
    }

    pub fn single(edit: TextEdit) -> Self {
        Self { edits: vec![edit] }
    }

    pub fn edits(&self) -> &[TextEdit] {
        &self.edits
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}

/// Anchor/head selection. A caret is represented by `anchor == head`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Selection {
    pub anchor: BufferOffset,
    pub head: BufferOffset,
}

impl Selection {
    pub fn caret(offset: BufferOffset) -> Self {
        Self {
            anchor: offset,
            head: offset,
        }
    }

    pub fn new(anchor: BufferOffset, head: BufferOffset) -> Self {
        Self { anchor, head }
    }

    pub fn is_caret(self) -> bool {
        self.anchor == self.head
    }

    pub fn range(self) -> TextRange {
        TextRange::new(self.anchor, self.head)
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self::caret(BufferOffset::ZERO)
    }
}

/// Cursor state with an optional visual column for future vertical movement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cursor {
    selection: Selection,
    preferred_column: Option<usize>,
}

impl Cursor {
    pub fn new(offset: BufferOffset) -> Self {
        Self {
            selection: Selection::caret(offset),
            preferred_column: None,
        }
    }

    pub fn selection(&self) -> Selection {
        self.selection
    }

    pub fn set_selection(&mut self, selection: Selection) {
        self.selection = selection;
        self.preferred_column = None;
    }

    pub fn move_left(&mut self, buffer: &TextBuffer, extend: bool) {
        let next = if !extend && !self.selection.is_caret() {
            self.selection.range().start
        } else {
            buffer.previous_grapheme_offset(self.selection.head)
        };
        self.set_head(next, extend);
    }

    pub fn move_right(&mut self, buffer: &TextBuffer, extend: bool) {
        let next = if !extend && !self.selection.is_caret() {
            self.selection.range().end
        } else {
            buffer.next_grapheme_offset(self.selection.head)
        };
        self.set_head(next, extend);
    }

    pub fn move_to(&mut self, offset: BufferOffset, extend: bool) {
        self.set_head(offset, extend);
    }

    fn set_head(&mut self, head: BufferOffset, extend: bool) {
        self.selection = if extend {
            Selection::new(self.selection.anchor, head)
        } else {
            Selection::caret(head)
        };
        self.preferred_column = None;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HistoryEntry {
    edits: Vec<TextEdit>,
    before_revision: u64,
    after_revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PieceSource {
    Original,
    Add,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Piece {
    source: PieceSource,
    start: usize,
    len: usize,
}

impl Piece {
    fn new(source: PieceSource, start: usize, len: usize) -> Option<Self> {
        (len > 0).then_some(Self { source, start, len })
    }

    fn slice(self, offset: usize, len: usize) -> Option<Self> {
        debug_assert!(offset <= self.len);
        debug_assert!(offset + len <= self.len);
        Self::new(self.source, self.start + offset, len)
    }

    fn end(self) -> usize {
        self.start + self.len
    }
}

/// Piece-table storage inspired by Monaco/VS Code's buffer model.
///
/// `original` never changes after construction. Inserted text is appended to
/// `add`, and `pieces` describes the visible document as byte spans into those
/// two buffers. `TextBuffer` now materializes the full document only for
/// boundary APIs such as save, syntax, search, and IME; edits themselves keep
/// the piece table as the source of truth.
#[derive(Clone, Debug, Eq, PartialEq)]
struct PieceTableTextBuffer {
    original: String,
    add: String,
    pieces: Vec<Piece>,
    len: usize,
}

impl PieceTableTextBuffer {
    fn new(original: String) -> Self {
        let len = original.len();
        let pieces = Piece::new(PieceSource::Original, 0, len)
            .into_iter()
            .collect();
        Self {
            original,
            add: String::new(),
            pieces,
            len,
        }
    }

    fn len(&self) -> usize {
        self.len
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn source_text(&self, source: PieceSource) -> &str {
        match source {
            PieceSource::Original => &self.original,
            PieceSource::Add => &self.add,
        }
    }

    fn to_text(&self) -> String {
        let mut text = String::with_capacity(self.len);
        for piece in self.pieces.iter().copied() {
            let source = self.source_text(piece.source);
            text.push_str(&source[piece.start..piece.end()]);
        }
        text
    }

    fn slice_to_string(&self, range: Range<usize>) -> String {
        debug_assert!(range.start <= range.end);
        debug_assert!(range.end <= self.len);
        if range.is_empty() {
            return String::new();
        }

        let mut text = String::with_capacity(range.end - range.start);
        let mut position = 0;
        for piece in self.pieces.iter().copied() {
            let piece_start = position;
            let piece_end = position + piece.len;
            position = piece_end;
            if piece_end <= range.start {
                continue;
            }
            if piece_start >= range.end {
                break;
            }
            let local_start = range.start.max(piece_start) - piece_start;
            let local_end = range.end.min(piece_end) - piece_start;
            let source = self.source_text(piece.source);
            text.push_str(&source[piece.start + local_start..piece.start + local_end]);
        }
        text
    }

    fn is_char_boundary(&self, offset: usize) -> bool {
        if offset > self.len {
            return false;
        }
        if offset == self.len {
            return true;
        }

        let mut position = 0;
        for piece in self.pieces.iter().copied() {
            let piece_start = position;
            let piece_end = position + piece.len;
            position = piece_end;
            if offset < piece_start {
                break;
            }
            if offset == piece_start || offset == piece_end {
                return true;
            }
            if offset < piece_end {
                let source_offset = piece.start + (offset - piece_start);
                return self
                    .source_text(piece.source)
                    .is_char_boundary(source_offset);
            }
        }
        false
    }

    fn replace(&mut self, range: Range<usize>, replacement: &str) {
        debug_assert!(range.start <= range.end);
        debug_assert!(range.end <= self.len);

        let replacement_piece = self.append_add_piece(replacement);
        let mut next =
            Vec::with_capacity(self.pieces.len() + usize::from(replacement_piece.is_some()));
        let mut position = 0;
        let mut inserted = false;

        for piece in self.pieces.iter().copied() {
            let piece_start = position;
            let piece_end = position + piece.len;
            position = piece_end;

            if piece_end <= range.start {
                push_piece(&mut next, piece);
                continue;
            }

            if piece_start >= range.end {
                if !inserted {
                    if let Some(piece) = replacement_piece {
                        push_piece(&mut next, piece);
                    }
                    inserted = true;
                }
                push_piece(&mut next, piece);
                continue;
            }

            if !inserted {
                if range.start > piece_start {
                    let left_len = range.start - piece_start;
                    if let Some(left) = piece.slice(0, left_len) {
                        push_piece(&mut next, left);
                    }
                }
                if let Some(piece) = replacement_piece {
                    push_piece(&mut next, piece);
                }
                inserted = true;
            }

            if range.end < piece_end {
                let right_offset = range.end - piece_start;
                let right_len = piece_end - range.end;
                if let Some(right) = piece.slice(right_offset, right_len) {
                    push_piece(&mut next, right);
                }
            }
        }

        if !inserted {
            if let Some(piece) = replacement_piece {
                push_piece(&mut next, piece);
            }
        }

        self.pieces = next;
        self.len = self.len - (range.end - range.start) + replacement.len();
    }

    fn append_add_piece(&mut self, text: &str) -> Option<Piece> {
        let start = self.add.len();
        self.add.push_str(text);
        Piece::new(PieceSource::Add, start, text.len())
    }
}

fn push_piece(pieces: &mut Vec<Piece>, piece: Piece) {
    if let Some(previous) = pieces.last_mut()
        && previous.source == piece.source
        && previous.end() == piece.start
    {
        previous.len += piece.len;
        return;
    }
    pieces.push(piece);
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
        let inverse = self.apply_edits_internal(transaction.edits)?;
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

#[derive(Debug, Error, Eq, PartialEq)]
pub enum EditorError {
    #[error("buffer offset {offset} is out of bounds for length {len}")]
    OffsetOutOfBounds { offset: usize, len: usize },
    #[error("buffer offset {offset} is not a UTF-8 character boundary")]
    InvalidUtf8Boundary { offset: usize },
    #[error("invalid text range {start}..{end}")]
    InvalidRange { start: usize, end: usize },
    #[error("line {line} is out of bounds for {line_count} lines")]
    InvalidLine { line: usize, line_count: usize },
    #[error("column {column} is out of bounds for line {line} length {line_len}")]
    InvalidColumn {
        line: usize,
        column: usize,
        line_len: usize,
    },
    #[error("transaction edits overlap at offset {offset}")]
    OverlappingEdits { offset: usize },
    #[error("edit delta overflow")]
    EditDeltaOverflow,
}

fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

fn update_line_starts_after_edits(old_starts: &[usize], edits: &[TextEdit]) -> Vec<usize> {
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
