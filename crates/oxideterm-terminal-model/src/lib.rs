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
    pub wide: bool,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub style_origin: TerminalStyleOrigin,
    pub attrs: TerminalAttrs,
    /// Rare text and link metadata is shared so ordinary cells carry only one optional pointer.
    pub extra: Option<Arc<TerminalCellExtra>>,
    pub cursor: bool,
}

/// Heap-backed metadata omitted from the common single-codepoint, non-hyperlink cell.
#[derive(Clone, Debug, Default, Hash, PartialEq, Eq)]
pub struct TerminalCellExtra {
    zerowidth: String,
    hyperlink: Option<String>,
}

impl TerminalCell {
    pub fn zerowidth(&self) -> &str {
        self.extra
            .as_deref()
            .map_or("", |extra| extra.zerowidth.as_str())
    }

    pub fn hyperlink(&self) -> Option<&str> {
        self.extra
            .as_deref()
            .and_then(|extra| extra.hyperlink.as_deref())
    }

    /// Replaces both rare fields at once while preserving the allocation-free empty state.
    pub fn set_extra(&mut self, zerowidth: String, hyperlink: Option<String>) {
        self.extra = if zerowidth.is_empty() && hyperlink.is_none() {
            None
        } else {
            Some(Arc::new(TerminalCellExtra {
                zerowidth,
                hyperlink,
            }))
        };
    }

    pub fn set_zerowidth(&mut self, zerowidth: String) {
        if zerowidth.is_empty() && self.hyperlink().is_none() {
            self.extra = None;
            return;
        }

        let extra = self
            .extra
            .get_or_insert_with(|| Arc::new(TerminalCellExtra::default()));
        Arc::make_mut(extra).zerowidth = zerowidth;
    }

    pub fn set_hyperlink(&mut self, hyperlink: Option<String>) {
        if hyperlink.is_none() && self.zerowidth().is_empty() {
            self.extra = None;
            return;
        }

        let extra = self
            .extra
            .get_or_insert_with(|| Arc::new(TerminalCellExtra::default()));
        Arc::make_mut(extra).hyperlink = hyperlink;
    }
}

/// Tracks explicit terminal colors in one byte so every snapshot cell stays compact.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub struct TerminalStyleOrigin(u8);

impl TerminalStyleOrigin {
    const FOREGROUND_EXPLICIT: u8 = 1 << 0;
    const BACKGROUND_EXPLICIT: u8 = 1 << 1;

    pub const fn new(foreground_explicit: bool, background_explicit: bool) -> Self {
        Self(
            (if foreground_explicit {
                Self::FOREGROUND_EXPLICIT
            } else {
                0
            }) | (if background_explicit {
                Self::BACKGROUND_EXPLICIT
            } else {
                0
            }),
        )
    }

    #[inline]
    pub const fn foreground_explicit(self) -> bool {
        self.0 & Self::FOREGROUND_EXPLICIT != 0
    }

    #[inline]
    pub const fn background_explicit(self) -> bool {
        self.0 & Self::BACKGROUND_EXPLICIT != 0
    }
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

/// Stores independent terminal text attributes as bits in snapshot hot storage.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub struct TerminalAttrs(u8);

impl TerminalAttrs {
    const BOLD: u8 = 1 << 0;
    const DIM: u8 = 1 << 1;
    const ITALIC: u8 = 1 << 2;
    const UNDERLINE: u8 = 1 << 3;
    const STRIKEOUT: u8 = 1 << 4;
    const INVERSE: u8 = 1 << 5;

    pub const fn new(
        bold: bool,
        dim: bool,
        italic: bool,
        underline: bool,
        strikeout: bool,
        inverse: bool,
    ) -> Self {
        Self(
            (if bold { Self::BOLD } else { 0 })
                | (if dim { Self::DIM } else { 0 })
                | (if italic { Self::ITALIC } else { 0 })
                | (if underline { Self::UNDERLINE } else { 0 })
                | (if strikeout { Self::STRIKEOUT } else { 0 })
                | (if inverse { Self::INVERSE } else { 0 }),
        )
    }

    #[inline]
    pub const fn bold(self) -> bool {
        self.0 & Self::BOLD != 0
    }

    #[inline]
    pub const fn dim(self) -> bool {
        self.0 & Self::DIM != 0
    }

    #[inline]
    pub const fn italic(self) -> bool {
        self.0 & Self::ITALIC != 0
    }

    #[inline]
    pub const fn underline(self) -> bool {
        self.0 & Self::UNDERLINE != 0
    }

    #[inline]
    pub const fn strikeout(self) -> bool {
        self.0 & Self::STRIKEOUT != 0
    }

    #[inline]
    pub const fn inverse(self) -> bool {
        self.0 & Self::INVERSE != 0
    }
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
            text.push_str(cell.zerowidth());
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
            wide: false,
            fg: TerminalColor::rgb(0xe6, 0xe8, 0xeb),
            bg: TerminalColor::rgb(0x0d, 0x0f, 0x12),
            style_origin: TerminalStyleOrigin::default(),
            attrs: TerminalAttrs::default(),
            extra: None,
            cursor: false,
        }
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn terminal_cell_keeps_common_case_compact() {
        assert_eq!(std::mem::size_of::<TerminalAttrs>(), 1);
        assert_eq!(std::mem::size_of::<TerminalStyleOrigin>(), 1);
        assert_eq!(std::mem::size_of::<TerminalCell>(), 24);
    }

    #[test]
    fn packed_style_fields_preserve_each_flag() {
        let alternating = TerminalAttrs::new(true, false, true, false, true, false);
        let complementary = TerminalAttrs::new(false, true, false, true, false, true);
        let foreground_origin = TerminalStyleOrigin::new(true, false);
        let background_origin = TerminalStyleOrigin::new(false, true);

        assert!(alternating.bold());
        assert!(!alternating.dim());
        assert!(alternating.italic());
        assert!(!alternating.underline());
        assert!(alternating.strikeout());
        assert!(!alternating.inverse());
        assert!(!complementary.bold());
        assert!(complementary.dim());
        assert!(!complementary.italic());
        assert!(complementary.underline());
        assert!(!complementary.strikeout());
        assert!(complementary.inverse());
        assert!(foreground_origin.foreground_explicit());
        assert!(!foreground_origin.background_explicit());
        assert!(!background_origin.foreground_explicit());
        assert!(background_origin.background_explicit());
    }

    #[test]
    fn terminal_cell_extra_uses_copy_on_write_and_clears_empty_state() {
        let mut original = test_cell('e');
        original.set_extra(
            "\u{301}".to_string(),
            Some("https://example.com".to_string()),
        );
        let mut changed = original.clone();

        changed.set_zerowidth(String::new());
        changed.set_hyperlink(None);

        assert_eq!(original.zerowidth(), "\u{301}");
        assert_eq!(original.hyperlink(), Some("https://example.com"));
        assert!(changed.extra.is_none());
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
