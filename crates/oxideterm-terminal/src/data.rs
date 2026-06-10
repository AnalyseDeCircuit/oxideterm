use std::{
    collections::HashMap,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

use alacritty_terminal::vte::ansi::CursorShape as AlacCursorShape;
pub use oxideterm_terminal_graphics::{
    GraphicsOptions, TerminalImageAnimationState, TerminalImageData, TerminalImageFrame,
    TerminalImageId, TerminalImageProtocol,
};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct TerminalCell {
    pub ch: char,
    pub zerowidth: String,
    pub wide: bool,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub attrs: TerminalAttrs,
    pub hyperlink: Option<String>,
    pub cursor: bool,
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
        // Snapshot rows can share unchanged cell buffers across frames. Any
        // writer must go through copy-on-write so older snapshots remain stable.
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
        if self.signature != previous.signature
            || self.absolute_line != previous.absolute_line
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

#[derive(Clone, Debug)]
pub struct TerminalSnapshot {
    pub generation: u64,
    pub cols: usize,
    pub rows: usize,
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub cursor_shape: TerminalCursorShape,
    pub display_offset: usize,
    pub scrollback_lines: usize,
    pub lines: Vec<TerminalRow>,
    pub images: Vec<TerminalImageSnapshot>,
}

impl TerminalSnapshot {
    pub fn with_generation(mut self, generation: u64) -> Self {
        self.generation = generation;
        self
    }

    pub fn reuse_unchanged_rows_from(&mut self, previous: &Self) -> usize {
        // Match by terminal grid line so scrolling can reuse rows that moved to
        // a different viewport index between frames.
        let previous_rows_by_line = previous
            .lines
            .iter()
            .map(|row| (row.absolute_line, row))
            .collect::<HashMap<_, _>>();
        let mut reused_rows = 0;
        for row in &mut self.lines {
            let Some(previous_row) = previous_rows_by_line.get(&row.absolute_line) else {
                continue;
            };
            if row.reuse_cells_from_if_equal(previous_row) {
                reused_rows += 1;
            }
        }
        reused_rows
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalImageSnapshot {
    pub id: TerminalImageId,
    pub protocol: TerminalImageProtocol,
    pub row: usize,
    pub col: usize,
    pub cols: usize,
    pub rows: usize,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub z_index: i32,
    pub placeholder: bool,
    pub version: u64,
    pub data: Option<Arc<TerminalImageData>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSearchRange {
    pub line: i32,
    pub start_col: usize,
    pub end_col: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSearchMatch {
    pub line: i32,
    pub start_col: usize,
    pub end_col: usize,
    pub ranges: Vec<TerminalSearchRange>,
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub enum TerminalCursorShape {
    #[default]
    Block,
    Underline,
    Bar,
    Hollow,
    Hidden,
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
            attrs: TerminalAttrs::default(),
            hyperlink: None,
            cursor: false,
        }
    }

    #[test]
    fn terminal_row_signature_tracks_paint_relevant_content() {
        let mut row = TerminalRow {
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

    #[test]
    fn terminal_snapshot_reuses_equal_row_cell_buffers() {
        let mut previous_row = TerminalRow {
            absolute_line: 0,
            cells: Arc::new(vec![test_cell('a')]),
            wrapped: false,
            active_input: false,
            signature: 0,
        };
        previous_row.refresh_signature();

        let mut next_row = TerminalRow {
            absolute_line: 0,
            cells: Arc::new(vec![test_cell('a')]),
            wrapped: false,
            active_input: false,
            signature: 0,
        };
        next_row.refresh_signature();

        let previous = TerminalSnapshot {
            generation: 1,
            cols: 1,
            rows: 1,
            cursor_col: 0,
            cursor_row: 0,
            cursor_shape: TerminalCursorShape::Block,
            display_offset: 0,
            scrollback_lines: 0,
            lines: vec![previous_row],
            images: Vec::new(),
        };
        let mut next = previous.clone().with_generation(0);
        next.lines = vec![next_row];

        assert_eq!(next.reuse_unchanged_rows_from(&previous), 1);
        assert!(Arc::ptr_eq(&next.lines[0].cells, &previous.lines[0].cells));
    }

    #[test]
    fn terminal_snapshot_keeps_changed_row_cell_buffers_separate() {
        let mut previous_row = TerminalRow {
            absolute_line: 0,
            cells: Arc::new(vec![test_cell('a')]),
            wrapped: false,
            active_input: false,
            signature: 0,
        };
        previous_row.refresh_signature();

        let mut next_row = TerminalRow {
            absolute_line: 0,
            cells: Arc::new(vec![test_cell('b')]),
            wrapped: false,
            active_input: false,
            signature: 0,
        };
        next_row.refresh_signature();

        let previous = TerminalSnapshot {
            generation: 1,
            cols: 1,
            rows: 1,
            cursor_col: 0,
            cursor_row: 0,
            cursor_shape: TerminalCursorShape::Block,
            display_offset: 0,
            scrollback_lines: 0,
            lines: vec![previous_row],
            images: Vec::new(),
        };
        let mut next = previous.clone().with_generation(0);
        next.lines = vec![next_row];

        assert_eq!(next.reuse_unchanged_rows_from(&previous), 0);
        assert!(!Arc::ptr_eq(&next.lines[0].cells, &previous.lines[0].cells));
    }

    #[test]
    fn terminal_snapshot_reuses_equal_rows_after_scroll_changes_viewport_index() {
        let mut first_row = TerminalRow {
            absolute_line: -1,
            cells: Arc::new(vec![test_cell('a')]),
            wrapped: false,
            active_input: false,
            signature: 0,
        };
        first_row.refresh_signature();
        let mut second_row = TerminalRow {
            absolute_line: 0,
            cells: Arc::new(vec![test_cell('b')]),
            wrapped: false,
            active_input: false,
            signature: 0,
        };
        second_row.refresh_signature();
        let previous = TerminalSnapshot {
            generation: 1,
            cols: 1,
            rows: 2,
            cursor_col: 0,
            cursor_row: 1,
            cursor_shape: TerminalCursorShape::Block,
            display_offset: 1,
            scrollback_lines: 1,
            lines: vec![first_row, second_row],
            images: Vec::new(),
        };

        let mut moved_row = TerminalRow {
            absolute_line: 0,
            cells: Arc::new(vec![test_cell('b')]),
            wrapped: false,
            active_input: false,
            signature: 0,
        };
        moved_row.refresh_signature();
        let mut new_bottom_row = TerminalRow {
            absolute_line: 1,
            cells: Arc::new(vec![test_cell('c')]),
            wrapped: false,
            active_input: false,
            signature: 0,
        };
        new_bottom_row.refresh_signature();
        let mut next = TerminalSnapshot {
            generation: 0,
            cols: 1,
            rows: 2,
            cursor_col: 0,
            cursor_row: 1,
            cursor_shape: TerminalCursorShape::Block,
            display_offset: 0,
            scrollback_lines: 1,
            lines: vec![moved_row, new_bottom_row],
            images: Vec::new(),
        };

        assert_eq!(next.reuse_unchanged_rows_from(&previous), 1);
        assert!(Arc::ptr_eq(&next.lines[0].cells, &previous.lines[1].cells));
        assert!(!Arc::ptr_eq(&next.lines[1].cells, &previous.lines[0].cells));
    }

    #[test]
    fn terminal_snapshot_generation_is_metadata_only() {
        let snapshot = TerminalSnapshot {
            generation: 0,
            cols: 1,
            rows: 1,
            cursor_col: 0,
            cursor_row: 0,
            cursor_shape: TerminalCursorShape::Block,
            display_offset: 0,
            scrollback_lines: 0,
            lines: Vec::new(),
            images: Vec::new(),
        }
        .with_generation(42);

        assert_eq!(snapshot.generation, 42);
    }
}

impl From<AlacCursorShape> for TerminalCursorShape {
    fn from(value: AlacCursorShape) -> Self {
        match value {
            AlacCursorShape::Block => TerminalCursorShape::Block,
            AlacCursorShape::Underline => TerminalCursorShape::Underline,
            AlacCursorShape::Beam => TerminalCursorShape::Bar,
            AlacCursorShape::HollowBlock => TerminalCursorShape::Hollow,
            AlacCursorShape::Hidden => TerminalCursorShape::Hidden,
        }
    }
}
