// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::ops::Range;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticClass {
    Command,
    Keyword,
    Option,
    Operator,
    String,
    Variable,
    Comment,
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

pub const SEMANTIC_CLASSES: &[SemanticClass] = &[
    SemanticClass::Command,
    SemanticClass::Keyword,
    SemanticClass::Option,
    SemanticClass::Operator,
    SemanticClass::String,
    SemanticClass::Variable,
    SemanticClass::Comment,
    SemanticClass::Link,
    SemanticClass::Path,
    SemanticClass::Address,
    SemanticClass::Timestamp,
    SemanticClass::Number,
    SemanticClass::Error,
    SemanticClass::Warning,
    SemanticClass::Success,
    SemanticClass::Info,
];

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticShellDialect {
    #[default]
    Auto,
    Bash,
    Zsh,
    Fish,
    PowerShell,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SemanticLineRole {
    Command,
    Output,
    PsAuxOutput,
    PsFullOutput,
    #[default]
    Unknown,
}

impl SemanticLineRole {
    pub(crate) fn is_output(self) -> bool {
        matches!(self, Self::Output | Self::PsAuxOutput | Self::PsFullOutput)
    }
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
    /// Optional presentation variant for semantic peers such as nested brackets.
    pub style_variant: Option<u8>,
}

impl SemanticSpan {
    pub(crate) fn new(range: Range<usize>, class: SemanticClass) -> Self {
        Self {
            range,
            class,
            style_variant: None,
        }
    }
}
