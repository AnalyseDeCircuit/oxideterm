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

    fn set_head(&mut self, head: BufferOffset, extend: bool) {
        self.selection = if extend {
            Selection::new(self.selection.anchor, head)
        } else {
            Selection::caret(head)
        };
        self.preferred_column = None;
    }
}
