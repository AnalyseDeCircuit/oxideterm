// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyntaxError {
    #[error("tree-sitter language error: {0}")]
    Language(#[from] tree_sitter::LanguageError),
    #[error("tree-sitter query error: {0}")]
    Query(#[from] tree_sitter::QueryError),
    #[error("tree-sitter parse was cancelled")]
    ParseCancelled,
}
