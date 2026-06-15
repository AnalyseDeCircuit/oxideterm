// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{BufferOffset, Selection, TextBuffer};

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

    pub fn preferred_column_or(&mut self, current_column: usize) -> usize {
        // Vertical movement should keep the original screen column while moving
        // through shorter lines, matching browser editor caret behavior.
        *self.preferred_column.get_or_insert(current_column)
    }

    pub fn move_to_with_preferred_column(
        &mut self,
        offset: BufferOffset,
        extend: bool,
        preferred_column: usize,
    ) {
        self.selection = if extend {
            Selection::new(self.selection.anchor, offset)
        } else {
            Selection::caret(offset)
        };
        self.preferred_column = Some(preferred_column);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_move_preserves_preferred_column_until_horizontal_move() {
        let buffer = TextBuffer::new("alpha\nb\ncharlie");
        let mut cursor = Cursor::new(BufferOffset(4));

        assert_eq!(cursor.preferred_column_or(4), 4);
        cursor.move_to_with_preferred_column(BufferOffset(6), false, 4);
        assert_eq!(cursor.preferred_column_or(1), 4);

        cursor.move_left(&buffer, false);
        assert_eq!(cursor.preferred_column_or(0), 0);
    }

    #[test]
    fn vertical_move_can_extend_selection_with_original_anchor() {
        let mut cursor = Cursor::new(BufferOffset(2));

        cursor.move_to_with_preferred_column(BufferOffset(8), true, 2);

        assert_eq!(
            cursor.selection(),
            Selection::new(BufferOffset(2), BufferOffset(8))
        );
        assert_eq!(cursor.preferred_column_or(0), 2);
    }
}
