// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::Context;
use oxideterm_editor_core::{
    BufferOffset, FindOptions, Selection, find_all, replace_all_transaction, word_at,
};

use super::TextEditorView;

impl TextEditorView {
    pub fn set_find_query(&mut self, query: impl Into<String>, cx: &mut Context<Self>) {
        self.find_query = query.into();
        self.refresh_find_matches();
        self.active_find_index = (!self.find_matches.is_empty()).then_some(0);
        if let Some(hit) = self.active_find_match() {
            self.cursor
                .set_selection(Selection::new(hit.range.start, hit.range.end));
            self.secondary_selections.clear();
        }
        cx.notify();
    }

    pub fn find_query(&self) -> &str {
        &self.find_query
    }

    pub fn find_matches(&self) -> &[oxideterm_editor_core::FindMatch] {
        &self.find_matches
    }

    pub fn select_next_find_match(&mut self, cx: &mut Context<Self>) {
        if self.find_matches.is_empty() {
            return;
        }
        let next = self
            .active_find_index
            .map(|index| (index + 1) % self.find_matches.len())
            .unwrap_or(0);
        self.active_find_index = Some(next);
        if let Some(hit) = self.active_find_match() {
            self.cursor
                .set_selection(Selection::new(hit.range.start, hit.range.end));
            self.secondary_selections.clear();
        }
        cx.notify();
    }

    pub fn select_previous_find_match(&mut self, cx: &mut Context<Self>) {
        if self.find_matches.is_empty() {
            return;
        }
        let previous = self
            .active_find_index
            .map(|index| {
                if index == 0 {
                    self.find_matches.len() - 1
                } else {
                    index - 1
                }
            })
            .unwrap_or(0);
        self.active_find_index = Some(previous);
        if let Some(hit) = self.active_find_match() {
            self.cursor
                .set_selection(Selection::new(hit.range.start, hit.range.end));
            self.secondary_selections.clear();
        }
        cx.notify();
    }

    pub fn replace_current_find_match(
        &mut self,
        replacement: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(hit) = self.active_find_match() else {
            return;
        };
        self.replace_range_with_caret(hit.range, replacement, cx);
        self.refresh_find_matches();
    }

    pub fn replace_all_find_matches(
        &mut self,
        replacement: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        if self.find_query.is_empty() {
            return;
        }
        let replacement = replacement.into();
        let transaction = self.buffer.with_text(|text| {
            replace_all_transaction(text, &self.find_query, &replacement, self.find_options())
        });
        if transaction.is_empty() {
            return;
        }
        if self.buffer.apply_transaction(transaction).is_ok() {
            self.cursor
                .set_selection(Selection::caret(BufferOffset::ZERO));
            self.secondary_selections.clear();
            self.marked_text = None;
            self.save_status = super::EditorSaveStatus::Dirty;
            self.apply_syntax_edit(None);
            self.refresh_find_matches();
            self.viewport
                .clamp(self.document_row_count(), self.metrics.line_height);
            cx.notify();
        }
    }

    pub(super) fn refresh_find_matches(&mut self) {
        self.find_matches = self
            .buffer
            .with_text(|text| find_all(text, &self.find_query, self.find_options()));
        if self
            .active_find_index
            .is_some_and(|index| index >= self.find_matches.len())
        {
            self.active_find_index = self.find_matches.len().checked_sub(1);
        }
    }

    pub(super) fn active_find_match(&self) -> Option<oxideterm_editor_core::FindMatch> {
        self.active_find_index
            .and_then(|index| self.find_matches.get(index).cloned())
    }

    pub(super) fn select_current_word_for_find(&mut self, cx: &mut Context<Self>) {
        let selection = self.cursor.selection();
        let query = if selection.is_caret() {
            self.buffer.with_text(|text| word_at(text, selection.head))
        } else {
            self.buffer.slice(selection.range()).unwrap_or_default()
        };
        if !query.is_empty() {
            self.set_find_query(query, cx);
        }
    }

    pub(super) fn add_next_find_match_as_cursor(&mut self, cx: &mut Context<Self>) {
        if self.find_query.is_empty() {
            self.select_current_word_for_find(cx);
            return;
        }
        if self.find_matches.is_empty() {
            return;
        }
        let current_end = self.cursor.selection().range().end.0;
        let index = self
            .find_matches
            .iter()
            .position(|hit| hit.range.start.0 > current_end)
            .unwrap_or(0);
        self.active_find_index = Some(index);
        let hit = self.find_matches[index].clone();
        if self.cursor.selection().is_caret() {
            self.cursor
                .set_selection(Selection::new(hit.range.start, hit.range.end));
        } else if !self
            .secondary_selections
            .contains(&Selection::new(hit.range.start, hit.range.end))
        {
            self.secondary_selections
                .push(Selection::new(hit.range.start, hit.range.end));
        }
        cx.notify();
    }

    fn find_options(&self) -> FindOptions {
        FindOptions {
            case_sensitive: self.settings.find_case_sensitive,
            whole_word: self.settings.find_whole_word,
        }
    }
}
