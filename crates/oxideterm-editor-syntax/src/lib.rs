// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Syntax data layer for OxideTerm's native editor.
//!
//! This crate owns tree-sitter parsers and returns byte-range metadata. It does
//! not paint GPUI elements and does not mutate editor buffers.

mod brackets;
mod edit;
mod error;
mod folding;
mod highlight;
mod language;
mod queries;
mod session;
mod types;

#[cfg(test)]
mod tests;

pub use edit::SyntaxEdit;
pub use error::SyntaxError;
pub use language::{LanguageId, SUPPORTED_LANGUAGES};
pub use session::SyntaxSession;
pub use types::{BracketPair, FoldRange, HighlightSpan, SyntaxScope};
