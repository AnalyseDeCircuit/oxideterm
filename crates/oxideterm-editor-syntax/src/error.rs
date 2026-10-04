// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyntaxError {
    #[error("Language support is not installed")]
    LanguageUnavailable,
    #[error("Language plugin failed: {0}")]
    Plugin(String),
    #[error("tree-sitter language error: {0}")]
    Language(#[from] tree_sitter::LanguageError),
    #[error("tree-sitter query error: {0}")]
    Query(#[from] tree_sitter::QueryError),
    #[error("tree-sitter parse was cancelled")]
    ParseCancelled,
}
