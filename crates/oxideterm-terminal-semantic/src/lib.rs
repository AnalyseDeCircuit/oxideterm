// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic semantic classification for plain terminal text.
//!
//! This crate returns byte ranges and semantic roles. It deliberately owns no
//! terminal snapshots, renderer colors, GPUI state, or user-defined rules.

mod classifier;
mod scheme;
mod types;

#[cfg(test)]
mod tests;

pub use classifier::{classify_line, classify_line_with_scheme};
pub use types::{SemanticClass, SemanticLineRole, SemanticScheme, SemanticSpan};
