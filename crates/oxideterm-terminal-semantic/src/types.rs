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
