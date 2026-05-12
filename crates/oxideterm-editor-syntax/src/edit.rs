// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_editor_core::{LineCol, TextRange};
use tree_sitter::{InputEdit, Point};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyntaxEdit {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
    pub start_position: LineCol,
    pub old_end_position: LineCol,
    pub new_end_position: LineCol,
}

impl SyntaxEdit {
    pub fn replace(source_before: &str, range: TextRange, replacement: &str) -> Self {
        let start_position = point_for_byte(source_before, range.start.0);
        let old_end_position = point_for_byte(source_before, range.end.0);
        let new_end_position = advance_position(start_position, replacement);
        Self {
            start_byte: range.start.0,
            old_end_byte: range.end.0,
            new_end_byte: range.start.0 + replacement.len(),
            start_position,
            old_end_position,
            new_end_position,
        }
    }

    pub(crate) fn as_input_edit(self) -> InputEdit {
        InputEdit {
            start_byte: self.start_byte,
            old_end_byte: self.old_end_byte,
            new_end_byte: self.new_end_byte,
            start_position: Point {
                row: self.start_position.line,
                column: self.start_position.column,
            },
            old_end_position: Point {
                row: self.old_end_position.line,
                column: self.old_end_position.column,
            },
            new_end_position: Point {
                row: self.new_end_position.line,
                column: self.new_end_position.column,
            },
        }
    }
}

fn point_for_byte(source: &str, byte: usize) -> LineCol {
    let mut line = 0;
    let mut line_start = 0;
    for (index, ch) in source.char_indices() {
        if index >= byte {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = index + 1;
        }
    }
    LineCol::new(line, byte.saturating_sub(line_start))
}

fn advance_position(start: LineCol, text: &str) -> LineCol {
    let mut line = start.line;
    let mut column = start.column;
    for ch in text.chars() {
        if ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += ch.len_utf8();
        }
    }
    LineCol::new(line, column)
}
