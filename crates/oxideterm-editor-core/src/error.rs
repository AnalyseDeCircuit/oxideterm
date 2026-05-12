// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use thiserror::Error;

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
