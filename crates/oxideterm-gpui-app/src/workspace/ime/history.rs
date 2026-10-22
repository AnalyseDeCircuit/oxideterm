// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::VecDeque;

use zeroize::Zeroizing;

use super::{WorkspaceImeSelection, WorkspaceImeTarget};

const MAX_EDITS: usize = 128;
const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;

pub(super) struct InputSnapshot {
    pub text: Zeroizing<String>,
    pub selection: WorkspaceImeSelection,
}

struct InputEdit {
    target: WorkspaceImeTarget,
    before: InputSnapshot,
    after: InputSnapshot,
}

impl InputEdit {
    fn bytes(&self) -> usize {
        self.before.text.len() + self.after.text.len()
    }
}

/// Window-owned, bounded history. Drafts may contain credentials even in plain inputs.
#[derive(Default)]
pub(super) struct InputHistory {
    undo: VecDeque<InputEdit>,
    redo: VecDeque<InputEdit>,
}

impl InputHistory {
    pub fn record(&mut self, before: InputSnapshot, after: InputSnapshot) {
        if before.text == after.text {
            return;
        }
        let target = before.selection.target;
        // A programmatic replacement starts a new history for this input only.
        if self
            .undo
            .iter()
            .rfind(|edit| edit.target == target)
            .is_some_and(|edit| edit.after.text != before.text)
        {
            self.clear(target);
        }
        self.redo.retain(|edit| edit.target != target);
        self.undo.push_back(InputEdit {
            target,
            before,
            after,
        });
        while self.undo.len() + self.redo.len() > MAX_EDITS
            || self
                .undo
                .iter()
                .chain(&self.redo)
                .map(InputEdit::bytes)
                .sum::<usize>()
                > MAX_TEXT_BYTES
        {
            if self.undo.pop_front().is_none() {
                self.redo.pop_front();
            }
        }
    }

    pub fn restore(
        &mut self,
        target: WorkspaceImeTarget,
        current: &str,
        redo: bool,
    ) -> Option<InputSnapshot> {
        let source = if redo { &mut self.redo } else { &mut self.undo };
        let index = source.iter().rposition(|edit| edit.target == target)?;
        let edit = source.remove(index)?;
        let (expected, restored) = if redo {
            (&edit.before, &edit.after)
        } else {
            (&edit.after, &edit.before)
        };
        if expected.text.as_str() != current {
            self.clear(target);
            return None;
        }
        let snapshot = InputSnapshot {
            text: restored.text.clone(),
            selection: restored.selection.clone(),
        };
        if redo {
            self.undo.push_back(edit);
        } else {
            self.redo.push_back(edit);
        }
        Some(snapshot)
    }

    fn clear(&mut self, target: WorkspaceImeTarget) {
        self.clear_matching(|candidate| candidate == target);
    }

    pub fn clear_matching(&mut self, matches: impl Fn(WorkspaceImeTarget) -> bool) {
        self.undo.retain(|edit| !matches(edit.target));
        self.redo.retain(|edit| !matches(edit.target));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(
        target: WorkspaceImeTarget,
        text: &str,
        range: std::ops::Range<usize>,
    ) -> InputSnapshot {
        InputSnapshot {
            text: Zeroizing::new(text.to_owned()),
            selection: WorkspaceImeSelection {
                target,
                range,
                reversed: true,
            },
        }
    }

    #[test]
    fn undo_redo_preserves_each_inputs_text_and_selection() {
        let first = WorkspaceImeTarget::CommandPalette;
        let second = WorkspaceImeTarget::KnowledgeSearch;
        let mut history = InputHistory::default();
        history.record(snapshot(first, "a🔒b", 1..3), snapshot(first, "a中b", 2..2));
        history.record(snapshot(second, "old", 0..3), snapshot(second, "new", 3..3));
        let restored = history.restore(first, "a中b", false).unwrap();
        assert_eq!(restored.text.as_str(), "a🔒b");
        assert_eq!(restored.selection, snapshot(first, "", 1..3).selection);
        assert_eq!(
            history.restore(second, "new", false).unwrap().text.as_str(),
            "old"
        );
        history.record(
            snapshot(second, "old", 3..3),
            snapshot(second, "other", 5..5),
        );
        assert!(history.restore(second, "other", true).is_none());
        assert_eq!(
            history.restore(first, "a🔒b", true).unwrap().text.as_str(),
            "a中b"
        );
        history.clear_matching(|target| target == first);
        assert!(history.restore(first, "a中b", false).is_none());
        assert_eq!(
            history
                .restore(second, "other", false)
                .unwrap()
                .text
                .as_str(),
            "old"
        );
    }

    #[test]
    fn changed_owner_value_and_oversized_edits_discard_stale_history() {
        let target = WorkspaceImeTarget::CommandPalette;
        let mut history = InputHistory::default();
        history.record(snapshot(target, "", 0..0), snapshot(target, "query", 5..5));
        assert!(history.restore(target, "new dialog", false).is_none());
        assert!(history.restore(target, "query", false).is_none());
        let oversized = "x".repeat(MAX_TEXT_BYTES + 1);
        history.record(
            snapshot(target, "", 0..0),
            snapshot(target, &oversized, oversized.len()..oversized.len()),
        );
        assert!(history.restore(target, &oversized, false).is_none());
    }
}
