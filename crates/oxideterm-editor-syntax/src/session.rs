// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;

use oxideterm_editor_core::{BufferOffset, TextRange};
use tree_sitter::{Language, Parser, Query, Tree};

use crate::{
    BracketPair, FoldRange, HighlightSpan, LanguageId, SyntaxChange, SyntaxEdit, SyntaxError,
    brackets, folding, highlight, indent,
};

pub struct SyntaxSession {
    pub(crate) language_id: LanguageId,
    language: Language,
    parser: Parser,
    pub(crate) highlight_query: Query,
    markdown_inline_query: Option<Query>,
    pub(crate) tree: Tree,
    pub(crate) cache_owner: Arc<()>,
    pub(crate) revision: u64,
}

impl SyntaxSession {
    pub fn parse(language_id: LanguageId, source: &str) -> Result<Self, SyntaxError> {
        let language = language_id.tree_sitter_language();
        let mut parser = Parser::new();
        parser.set_language(&language)?;
        let highlight_query = Query::new(&language, language_id.highlight_query())?;
        let markdown_inline_query = if language_id == LanguageId::Markdown {
            let inline_language: Language = tree_sitter_md::INLINE_LANGUAGE.into();
            Some(Query::new(
                &inline_language,
                tree_sitter_md::HIGHLIGHT_QUERY_INLINE,
            )?)
        } else {
            None
        };
        let tree = parser
            .parse(source, None)
            .ok_or(SyntaxError::ParseCancelled)?;

        Ok(Self {
            language_id,
            language,
            parser,
            highlight_query,
            markdown_inline_query,
            tree,
            cache_owner: Arc::new(()),
            revision: 0,
        })
    }

    pub fn language_id(&self) -> LanguageId {
        self.language_id
    }

    pub fn root_has_error(&self) -> bool {
        self.tree.root_node().has_error()
    }

    pub fn apply_edit(
        &mut self,
        source_after: &str,
        edit: SyntaxEdit,
    ) -> Result<SyntaxChange, SyntaxError> {
        // tree-sitter incremental parsing requires the old tree to be edited
        // with the same byte/point delta before it is passed back as a hint.
        self.tree.edit(&edit.as_input_edit());
        let tree = self
            .parser
            .parse(source_after, Some(&self.tree))
            .ok_or(SyntaxError::ParseCancelled)?;
        let old_tree = std::mem::replace(&mut self.tree, tree);
        self.revision += 1;
        Ok(SyntaxChange {
            edit,
            owner: self.cache_owner.clone(),
            revision: self.revision,
            structural_ranges: Default::default(),
            old_tree,
            new_tree: self.tree.clone(),
        })
    }

    pub fn reparse(&mut self, source: &str) -> Result<(), SyntaxError> {
        self.cache_owner = Arc::new(());
        self.revision = 0;
        self.parser.set_language(&self.language)?;
        self.tree = self
            .parser
            .parse(source, None)
            .ok_or(SyntaxError::ParseCancelled)?;
        Ok(())
    }

    pub fn highlight_spans(&self, source: &str) -> Vec<HighlightSpan> {
        self.highlight_spans_in_range(
            source,
            TextRange::new(BufferOffset(0), BufferOffset(source.len())),
        )
    }

    /// Return full, absolute spans intersecting a half-open byte range, without
    /// clipping captures that cross its edges. Source must match this session's tree.
    pub fn highlight_spans_in_range(&self, source: &str, range: TextRange) -> Vec<HighlightSpan> {
        highlight::highlight_spans(
            self.language_id,
            &self.tree,
            &self.highlight_query,
            self.markdown_inline_query.as_ref(),
            source,
            range.start.0..range.end.0,
        )
    }

    pub fn bracket_pairs(&self, source: &str) -> Vec<BracketPair> {
        brackets::bracket_pairs(source)
    }

    pub fn fold_ranges(&self) -> Vec<FoldRange> {
        folding::fold_ranges(self.tree.root_node())
    }

    pub fn indent_guides(&self, source: &str, tab_size: usize) -> Vec<crate::IndentGuide> {
        indent::indent_guides(self.tree.root_node(), source, tab_size)
    }
}
