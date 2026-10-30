// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{BufferOffset, TextRange};
use std::{ops::Range, sync::Arc};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TextAllocation {
    Original(Arc<str>),
    Added(Arc<String>),
}

impl TextAllocation {
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Original(text) => text,
            Self::Added(text) => text,
        }
    }

    pub(crate) fn is_unique(&self) -> bool {
        match self {
            Self::Original(text) => Arc::strong_count(text) == 1,
            Self::Added(text) => Arc::strong_count(text) == 1,
        }
    }

    pub(crate) fn same_allocation(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Original(a), Self::Original(b)) => Arc::ptr_eq(a, b),
            (Self::Added(a), Self::Added(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    pub(crate) fn allocation_key(&self) -> (u8, usize) {
        match self {
            Self::Original(text) => (0, Arc::as_ptr(text) as *const u8 as usize),
            Self::Added(text) => (1, Arc::as_ptr(text) as usize),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TextSlice {
    pub(crate) allocation: TextAllocation,
    pub(crate) range: Range<usize>,
}

impl TextSlice {
    pub(crate) fn text(&self) -> &str {
        &self.allocation.text()[self.range.clone()]
    }
}

/// Replacement and undo text share their source allocations across piece boundaries.
#[derive(Clone, Debug)]
pub struct SharedText {
    storage: SharedTextStorage,
}

#[derive(Clone, Debug)]
enum SharedTextStorage {
    Empty,
    Single(TextSlice),
    Multiple {
        slices: Arc<[TextSlice]>,
        len: usize,
    },
}

impl PartialEq for SharedText {
    fn eq(&self, other: &Self) -> bool {
        if self.slices().len() == other.slices().len()
            && self
                .slices()
                .iter()
                .zip(other.slices())
                .all(|(a, b)| a.range == b.range && a.allocation.same_allocation(&b.allocation))
        {
            return true;
        }
        if self.len() != other.len() {
            return false;
        }
        let mut left = self.chunks();
        let mut right = other.chunks();
        let mut a = left.next().unwrap_or_default().as_bytes();
        let mut b = right.next().unwrap_or_default().as_bytes();
        while !a.is_empty() && !b.is_empty() {
            let len = a.len().min(b.len());
            if a[..len] != b[..len] {
                return false;
            }
            a = &a[len..];
            b = &b[len..];
            if a.is_empty() {
                a = left.next().unwrap_or_default().as_bytes();
            }
            if b.is_empty() {
                b = right.next().unwrap_or_default().as_bytes();
            }
        }
        true
    }
}

impl Eq for SharedText {}

impl From<Arc<str>> for SharedText {
    fn from(text: Arc<str>) -> Self {
        let len = text.len();
        Self::from_slices(if len == 0 {
            Vec::new()
        } else {
            vec![TextSlice {
                allocation: TextAllocation::Original(text),
                range: 0..len,
            }]
        })
    }
}

impl From<&str> for SharedText {
    fn from(text: &str) -> Self {
        Arc::<str>::from(text).into()
    }
}

impl std::fmt::Display for SharedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for text in self.chunks() {
            f.write_str(text)?;
        }
        Ok(())
    }
}

impl serde::Serialize for SharedText {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for SharedText {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <String as serde::Deserialize>::deserialize(deserializer).map(Self::from)
    }
}

impl From<String> for SharedText {
    fn from(text: String) -> Self {
        let len = text.len();
        let storage = if len == 0 {
            SharedTextStorage::Empty
        } else {
            SharedTextStorage::Single(TextSlice {
                allocation: TextAllocation::Added(Arc::new(text)),
                range: 0..len,
            })
        };
        Self { storage }
    }
}

impl SharedText {
    /// Preserve an existing whole-string allocation for consumers requiring `Arc<str>`.
    pub fn into_contiguous(self) -> Arc<str> {
        match self.storage {
            SharedTextStorage::Empty => Arc::from(""),
            SharedTextStorage::Single(slice) => match slice.allocation {
                TextAllocation::Original(text) if slice.range == (0..text.len()) => text,
                allocation => Arc::from(&allocation.text()[slice.range]),
            },
            SharedTextStorage::Multiple { slices, len } => {
                let mut text = String::with_capacity(len);
                for slice in slices.iter() {
                    text.push_str(slice.text());
                }
                text.into()
            }
        }
    }

    /// Join only at a boundary requiring contiguous text; never retain that copy.
    pub fn with_text<R>(&self, f: impl FnOnce(&str) -> R) -> R {
        match self.slices() {
            [] => f(""),
            [slice] => f(slice.text()),
            _ => {
                let mut text = String::with_capacity(self.len());
                for chunk in self.chunks() {
                    text.push_str(chunk);
                }
                f(&text)
            }
        }
    }

    pub fn len(&self) -> usize {
        match &self.storage {
            SharedTextStorage::Empty => 0,
            SharedTextStorage::Single(slice) => slice.range.len(),
            SharedTextStorage::Multiple { len, .. } => *len,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn chunks(&self) -> impl Iterator<Item = &str> + Clone {
        self.slices().iter().map(TextSlice::text)
    }

    pub fn bytes(&self) -> impl Iterator<Item = u8> {
        self.chunks().flat_map(str::bytes)
    }

    pub(crate) fn slices(&self) -> &[TextSlice] {
        match &self.storage {
            SharedTextStorage::Empty => &[],
            SharedTextStorage::Single(slice) => std::slice::from_ref(slice),
            SharedTextStorage::Multiple { slices, .. } => slices,
        }
    }

    pub(crate) fn from_slices(mut slices: Vec<TextSlice>) -> Self {
        let storage = match slices.len() {
            0 => SharedTextStorage::Empty,
            1 => SharedTextStorage::Single(slices.pop().unwrap()),
            _ => SharedTextStorage::Multiple {
                len: slices.iter().map(|slice| slice.range.len()).sum(),
                slices: slices.into(),
            },
        };
        Self { storage }
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
