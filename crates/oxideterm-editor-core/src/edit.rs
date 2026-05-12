// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{BufferOffset, TextRange};

/// A single range replacement in byte coordinates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEdit {
    pub range: TextRange,
    pub replacement: String,
}

impl TextEdit {
    pub fn new(range: TextRange, replacement: impl Into<String>) -> Self {
        Self {
            range,
            replacement: replacement.into(),
        }
    }

    pub fn insert(offset: BufferOffset, text: impl Into<String>) -> Self {
        Self::new(TextRange::caret(offset), text)
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
