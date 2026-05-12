// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

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

    pub(crate) fn as_range(self) -> Range<usize> {
        self.start.0..self.end.0
    }
}
