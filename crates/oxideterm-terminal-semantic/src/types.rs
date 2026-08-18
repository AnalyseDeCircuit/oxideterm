// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SemanticClass {
    Command,
    Option,
    String,
    Link,
    Path,
    Address,
    Timestamp,
    Number,
    Error,
    Warning,
    Success,
    Info,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SemanticLineRole {
    Command,
    Output,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SemanticScheme {
    #[default]
    Balanced,
    Conservative,
}

impl SemanticScheme {
    pub(crate) const fn includes(self, class: SemanticClass) -> bool {
        match self {
            Self::Balanced => true,
            // Generic numbers and informational words are the two classes
            // most likely to make ordinary terminal output visually noisy.
            Self::Conservative => !matches!(class, SemanticClass::Number | SemanticClass::Info),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticSpan {
    pub range: Range<usize>,
    pub class: SemanticClass,
}

impl SemanticSpan {
    pub(crate) fn new(range: Range<usize>, class: SemanticClass) -> Self {
        Self { range, class }
    }
}
