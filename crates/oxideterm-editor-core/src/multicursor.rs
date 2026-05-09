// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{BufferOffset, Selection};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorSet {
    selections: Vec<Selection>,
}

impl CursorSet {
    pub fn new(primary: Selection) -> Self {
        Self {
            selections: vec![primary],
        }
    }

    pub fn primary(&self) -> Selection {
        self.selections
            .first()
            .copied()
            .unwrap_or_else(|| Selection::caret(BufferOffset::ZERO))
    }

    pub fn selections(&self) -> &[Selection] {
        &self.selections
    }

    pub fn set_primary(&mut self, selection: Selection) {
        self.selections.clear();
        self.selections.push(selection);
    }

    pub fn add_selection(&mut self, selection: Selection) {
        if !self.selections.contains(&selection) {
            self.selections.push(selection);
            normalize_selections(&mut self.selections);
        }
    }

    pub fn clear_secondary(&mut self) {
        let primary = self.primary();
        self.selections.clear();
        self.selections.push(primary);
    }
}

fn normalize_selections(selections: &mut Vec<Selection>) {
    selections.sort_by_key(|selection| {
        let range = selection.range();
        (range.start.0, range.end.0)
    });
    selections.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_primary_when_clearing_secondary_cursors() {
        let mut cursors = CursorSet::new(Selection::caret(BufferOffset(3)));
        cursors.add_selection(Selection::caret(BufferOffset(7)));

        cursors.clear_secondary();

        assert_eq!(cursors.selections(), &[Selection::caret(BufferOffset(3))]);
    }
}
