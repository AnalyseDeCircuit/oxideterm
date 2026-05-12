// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{BufferOffset, TextRange};

/// Anchor/head selection. A caret is represented by `anchor == head`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Selection {
    pub anchor: BufferOffset,
    pub head: BufferOffset,
}

impl Selection {
    pub fn caret(offset: BufferOffset) -> Self {
        Self {
            anchor: offset,
            head: offset,
        }
    }

    pub fn new(anchor: BufferOffset, head: BufferOffset) -> Self {
        Self { anchor, head }
    }

    pub fn is_caret(self) -> bool {
        self.anchor == self.head
    }

    pub fn range(self) -> TextRange {
        TextRange::new(self.anchor, self.head)
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self::caret(BufferOffset::ZERO)
    }
}
