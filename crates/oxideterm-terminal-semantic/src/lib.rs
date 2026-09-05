// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic semantic classification for plain terminal text.
//!
//! This crate returns byte ranges and semantic roles. It deliberately owns no
//! terminal snapshots, renderer colors, GPUI state, or user-defined rules.

mod classifier;
mod command;
mod compiler;
mod container;
mod document;
mod filesystem;
mod git;
mod import;
mod network;
mod permissions;
mod ps;
mod resources;
mod scheme;
#[cfg(feature = "shell-syntax")]
mod syntax;
mod systemd;
mod test_runner;
mod tokens;
mod types;

#[cfg(test)]
mod tests;

pub use classifier::{
    classify_line, classify_line_with_compiled_scheme,
    classify_line_with_compiled_scheme_and_shell, classify_line_with_scheme,
    semantic_line_emphasis, semantic_output_role_for_command,
};
pub use document::{
    MAX_SEMANTIC_PATTERN_LENGTH, MAX_SEMANTIC_RULES, SEMANTIC_SCHEME_FORMAT_VERSION,
    SemanticRuleContext, SemanticRuleDefinition, SemanticSchemeDocument, export_scheme_document,
    import_scheme_document, validate_scheme_document,
};
pub use import::import_external_scheme_document;
pub use scheme::{
    CompiledSemanticScheme, built_in_scheme_document, compile_scheme_document,
    compiled_builtin_scheme,
};
pub use types::{
    SEMANTIC_CLASSES, SemanticClass, SemanticLineRole, SemanticScheme, SemanticShellDialect,
    SemanticSpan,
};
