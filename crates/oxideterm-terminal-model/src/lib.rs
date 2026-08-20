// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Paint-ready terminal row data shared by runtime and presentation crates.
//!
//! This crate deliberately contains no terminal transport or PTY behavior so
//! Unicode layout and other readers can consume snapshots without importing
//! the complete terminal runtime dependency graph.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct TerminalCell {
    pub ch: char,
    pub zerowidth: String,
    pub wide: bool,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub style_origin: TerminalStyleOrigin,
    pub attrs: TerminalAttrs,
    pub hyperlink: Option<String>,
    pub cursor: bool,
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub struct TerminalStyleOrigin {
    pub foreground_explicit: bool,
    pub background_explicit: bool,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct TerminalColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl TerminalColor {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub struct TerminalAttrs {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub inverse: bool,
}

#[derive(Clone, Debug)]
pub struct TerminalRow {
    /// Stable presentation identity assigned by the pane. Zero means unassigned.
    pub line_id: u64,
    /// Opaque identity of the backing emulator row used for adjacent-snapshot reuse.
    pub source_id: usize,
    pub absolute_line: i64,
    pub cells: Arc<Vec<TerminalCell>>,
    pub wrapped: bool,
    pub active_input: bool,
    pub signature: u64,
}

impl TerminalRow {
    pub fn text(&self) -> String {
        let mut text = String::new();
        for cell in self.cells.iter() {
            text.push(cell.ch);
            text.push_str(&cell.zerowidth);
        }
        text
    }

    pub fn cells_mut(&mut self) -> &mut Vec<TerminalCell> {
        // Snapshot rows can share unchanged cell buffers across frames. Writers
        // use copy-on-write so older snapshots remain stable.
        Arc::make_mut(&mut self.cells)
    }

    pub fn refresh_signature(&mut self) {
        self.signature = self.compute_signature();
    }

    pub fn compute_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.wrapped.hash(&mut hasher);
        self.active_input.hash(&mut hasher);
        self.cells.hash(&mut hasher);
        hasher.finish()
    }

    pub fn reuse_cells_from_if_equal(&mut self, previous: &Self) -> bool {
        let same_line = if self.line_id != 0 && previous.line_id != 0 {
            self.line_id == previous.line_id
        } else {
            self.absolute_line == previous.absolute_line
        };
        if self.signature != previous.signature
            || !same_line
            || self.wrapped != previous.wrapped
            || self.active_input != previous.active_input
            || self.cells.as_ref() != previous.cells.as_ref()
        {
            return false;
        }

        self.cells = previous.cells.clone();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cell(ch: char) -> TerminalCell {
        TerminalCell {
            ch,
            zerowidth: String::new(),
            wide: false,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            style_origin: TerminalStyleOrigin::default(),
            attrs: TerminalAttrs::default(),
            hyperlink: None,
            cursor: false,
        }
    }

    #[test]
    fn row_signature_tracks_paint_relevant_content() {
        let mut row = TerminalRow {
            line_id: 0,
            source_id: 0,
            absolute_line: 0,
            cells: Arc::new(vec![test_cell('a')]),
            wrapped: false,
            active_input: false,
            signature: 0,
        };
        row.refresh_signature();
        let first = row.signature;

        row.cells_mut()[0].ch = 'b';
        row.refresh_signature();

        assert_ne!(first, row.signature);
    }
}
