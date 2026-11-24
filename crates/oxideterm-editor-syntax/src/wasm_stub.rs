// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! WASM fallback for the syntax layer when native C tree-sitter grammars are unavailable.
//!
//! Preserves the public API surface of `oxideterm-editor-syntax` so editor and
//! semantic-highlighting callers degrade cleanly to plain text while keeping
//! pure-Rust language detection and bracket matching available.

use std::{ops::Range, path::PathBuf};

use oxideterm_editor_core::TextRange;

use crate::{
    BracketIndex, BracketPair, FoldRange, HighlightSpan, IndentGuide, LanguageId, SyntaxChange,
    SyntaxEdit, SyntaxError, SyntaxWork, TextSource, brackets,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginGrammarSource {
    pub language: LanguageId,
    pub grammar_name: String,
    pub parser: PathBuf,
    pub highlights: PathBuf,
    pub parser_sha256: String,
    pub highlights_sha256: String,
    pub injections: Vec<PluginGrammarInjectionSource>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginGrammarInjectionSource {
    pub query: PathBuf,
    pub query_sha256: String,
    pub grammar: Box<PluginGrammarSource>,
}

/// Placeholder handle on `wasm32` where native `wasmtime`-backed plugin grammars cannot run.
pub struct PluginGrammar {
    source: PluginGrammarSource,
}

impl PluginGrammar {
    pub fn new(source: PluginGrammarSource) -> Self {
        Self { source }
    }

    pub fn failed(&self) -> bool {
        true
    }

    pub fn source(&self) -> &PluginGrammarSource {
        &self.source
    }
}

pub struct SyntaxSession {
    language_id: LanguageId,
}

impl SyntaxSession {
    pub fn parse(
        language_id: LanguageId,
        source: &(impl TextSource + ?Sized),
    ) -> Result<Self, SyntaxError> {
        Self::parse_controlled(language_id, source, None)
    }

    pub fn parse_controlled(
        _language_id: LanguageId,
        _source: &(impl TextSource + ?Sized),
        work: Option<&SyntaxWork>,
    ) -> Result<Self, SyntaxError> {
        crate::work::checkpoint(work)?;
        Err(SyntaxError::LanguageUnavailable)
    }

    pub fn language_id(&self) -> LanguageId {
        self.language_id.clone()
    }

    pub fn parse_plugin(
        _grammar: &PluginGrammar,
        _source: &(impl TextSource + ?Sized),
        work: Option<&SyntaxWork>,
    ) -> Result<Self, SyntaxError> {
        crate::work::checkpoint(work)?;
        Err(SyntaxError::LanguageUnavailable)
    }

    pub fn root_has_error(&self) -> bool {
        false
    }

    pub fn apply_edit(
        &mut self,
        source_after: &(impl TextSource + ?Sized),
        edit: SyntaxEdit,
    ) -> Result<SyntaxChange, SyntaxError> {
        self.apply_edit_controlled(source_after, edit, None)
    }

    pub fn apply_edit_controlled(
        &mut self,
        _source_after: &(impl TextSource + ?Sized),
        edit: SyntaxEdit,
        work: Option<&SyntaxWork>,
    ) -> Result<SyntaxChange, SyntaxError> {
        crate::work::checkpoint(work)?;
        Ok(SyntaxChange { edit })
    }

    pub fn reparse(&mut self, _source: &(impl TextSource + ?Sized)) -> Result<(), SyntaxError> {
        Err(SyntaxError::LanguageUnavailable)
    }

    pub fn highlight_spans(&self, _source: &(impl TextSource + ?Sized)) -> Vec<HighlightSpan> {
        Vec::new()
    }

    pub fn highlight_spans_in_range(
        &self,
        _source: &(impl TextSource + ?Sized),
        _range: TextRange,
    ) -> Vec<HighlightSpan> {
        Vec::new()
    }

    pub fn highlights_controlled(
        &self,
        _source: &(impl TextSource + ?Sized),
        _range: TextRange,
        work: Option<&SyntaxWork>,
    ) -> Result<Vec<HighlightSpan>, SyntaxError> {
        crate::work::checkpoint(work)?;
        Ok(Vec::new())
    }

    pub fn bracket_pairs(&self, source: &(impl TextSource + ?Sized)) -> Vec<BracketPair> {
        brackets::bracket_pairs(source)
    }

    pub fn bracket_index_controlled(
        &self,
        source: &(impl TextSource + ?Sized),
        work: Option<&SyntaxWork>,
    ) -> Result<BracketIndex, SyntaxError> {
        BracketIndex::new(self.brackets_controlled(source, work)?, work)
    }

    pub fn brackets_controlled(
        &self,
        source: &(impl TextSource + ?Sized),
        work: Option<&SyntaxWork>,
    ) -> Result<Vec<BracketPair>, SyntaxError> {
        brackets::bracket_pairs_controlled(source, work)
    }

    pub fn fold_ranges(&self) -> Vec<FoldRange> {
        Vec::new()
    }

    pub fn indent_guides(
        &self,
        _source: &(impl TextSource + ?Sized),
        _tab_size: usize,
    ) -> Vec<IndentGuide> {
        Vec::new()
    }
}

#[derive(Debug, Default)]
pub struct HighlightCache;

impl HighlightCache {
    pub fn clear(&mut self) {}

    pub fn is_empty(&self) -> bool {
        true
    }

    pub fn spans_in_range(&self, _range: Range<usize>) -> impl Iterator<Item = HighlightSpan> + '_ {
        std::iter::empty()
    }

    pub fn update(
        &mut self,
        _session: &SyntaxSession,
        _source: &(impl TextSource + ?Sized),
        _change: Option<&SyntaxChange>,
    ) {
    }

    pub fn update_controlled(
        &mut self,
        _session: &SyntaxSession,
        _source: &(impl TextSource + ?Sized),
        _change: Option<&SyntaxChange>,
        work: Option<&SyntaxWork>,
    ) -> Result<(), SyntaxError> {
        crate::work::checkpoint(work)
    }
}

#[derive(Debug, Default)]
pub struct StructureCache;

impl StructureCache {
    pub fn clear(&mut self) {}

    pub fn fold_lines(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        std::iter::empty()
    }

    pub fn fold_at_line(&self, _line: usize) -> Option<(usize, usize)> {
        None
    }

    pub fn columns_for_line(&self, _line: usize) -> Vec<usize> {
        Vec::new()
    }

    pub fn update(
        &mut self,
        _session: &SyntaxSession,
        _source: &(impl TextSource + ?Sized),
        _tab_size: usize,
        _change: Option<&SyntaxChange>,
    ) {
    }

    pub fn update_controlled(
        &mut self,
        _session: &SyntaxSession,
        _source: &(impl TextSource + ?Sized),
        _tab_size: usize,
        _change: Option<&SyntaxChange>,
        work: Option<&SyntaxWork>,
    ) -> Result<(), SyntaxError> {
        crate::work::checkpoint(work)
    }
}
