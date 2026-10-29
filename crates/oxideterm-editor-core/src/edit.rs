// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{BufferOffset, TextRange};
use std::{
    ops::{Deref, Range},
    sync::Arc,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TextAllocation {
    Empty,
    Original(Arc<str>),
    Added(Arc<String>),
}

impl TextAllocation {
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Empty => "",
            Self::Original(text) => text,
            Self::Added(text) => text,
        }
    }

    pub(crate) fn is_unique(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Original(text) => Arc::strong_count(text) == 1,
            Self::Added(text) => Arc::strong_count(text) == 1,
        }
    }

    pub(crate) fn same_allocation(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty, Self::Empty) => true,
            (Self::Original(a), Self::Original(b)) => Arc::ptr_eq(a, b),
            (Self::Added(a), Self::Added(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// Replacement text can refer to an existing allocation, including a deleted
/// range retained by undo, without copying its bytes.
#[derive(Clone, Debug, Eq)]
pub struct SharedText {
    pub(crate) allocation: TextAllocation,
    pub(crate) range: Range<usize>,
}

impl PartialEq for SharedText {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl From<String> for SharedText {
    fn from(text: String) -> Self {
        let len = text.len();
        Self {
            allocation: if text.is_empty() {
                TextAllocation::Empty
            } else {
                TextAllocation::Added(Arc::new(text))
            },
            range: 0..len,
        }
    }
}

impl Deref for SharedText {
    type Target = str;
    fn deref(&self) -> &str {
        &self.allocation.text()[self.range.clone()]
    }
}

/// A single range replacement in byte coordinates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEdit {
    pub range: TextRange,
    pub replacement: SharedText,
}

impl TextEdit {
    pub fn new(range: TextRange, replacement: impl Into<String>) -> Self {
        Self {
            range,
            replacement: SharedText::from(replacement.into()),
        }
    }

    pub fn insert(offset: BufferOffset, text: impl Into<String>) -> Self {
        Self::new(TextRange::caret(offset), text)
    }

    pub fn with_shared_text(range: TextRange, replacement: SharedText) -> Self {
        Self { range, replacement }
    }
}

/// A logical editing action that should undo/redo as one unit.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EditTransaction {
    edits: Vec<TextEdit>,
}

impl EditTransaction {
    pub fn new(edits: Vec<TextEdit>) -> Self {
        Self { edits }
    }

    pub fn single(edit: TextEdit) -> Self {
        Self { edits: vec![edit] }
    }

    pub fn edits(&self) -> &[TextEdit] {
        &self.edits
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    pub(crate) fn into_edits(self) -> Vec<TextEdit> {
        self.edits
    }
}
