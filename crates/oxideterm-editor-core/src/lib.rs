// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! UI-independent editor core for OxideTerm.
//!
//! This crate owns text, cursor, selection, undo/redo, search, and other
//! byte-offset editor semantics. GPUI layers should adapt these primitives to
//! input, layout, and rendering instead of owning their own text model logic.

mod buffer;
mod cursor;
mod edit;
mod error;
mod line_index;
mod multicursor;
mod piece_table;
mod search;
mod selection;
mod types;
mod word;

pub use buffer::TextBuffer;
pub use cursor::Cursor;
pub use edit::{EditTransaction, TextEdit};
pub use error::EditorError;
pub use multicursor::CursorSet;
pub use search::{FindMatch, FindOptions, find_all, replace_all_transaction};
pub use selection::Selection;
pub use types::{BufferOffset, LineCol, TextRange};
pub use word::word_at;
