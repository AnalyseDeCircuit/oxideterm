// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_editor_core::{BufferOffset, TextRange};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SyntaxScope {
    Attribute,
    Comment,
    Constant,
    Function,
    Keyword,
    Namespace,
    Number,
    Operator,
    Property,
    Punctuation,
    String,
    Type,
    Variable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HighlightSpan {
    pub range: TextRange,
    pub scope: SyntaxScope,
    pub capture: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BracketPair {
    pub open: BufferOffset,
    pub close: BufferOffset,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoldRange {
    pub range: TextRange,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndentGuide {
    pub start_line: usize,
    pub end_line: usize,
    pub column: usize,
}
