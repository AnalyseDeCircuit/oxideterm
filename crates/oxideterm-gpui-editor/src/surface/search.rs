// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::Context;
use oxideterm_editor_core::{
    BufferOffset, EditTransaction, FindMatch, FindOptions, Selection, TextEdit, TextRange,
    find_all, replace_all_transaction, word_at,
};

use super::TextEditorView;

pub(super) struct FindPatternCache {
    query: String,
    case_sensitive: bool,
    whole_word: bool,
    pattern: Result<regex::Regex, regex::Error>,
}

impl TextEditorView {
    pub fn set_find_query(&mut self, query: impl Into<String>, cx: &mut Context<Self>) {
        self.find_query = query.into();
        self.sync_search_input(cx);
        self.refresh_find_matches();
        self.active_find_index = (!self.find_matches.is_empty()).then_some(0);
        self.select_active_find_match();
        cx.notify();
    }

    pub fn find_query(&self) -> &str {
        &self.find_query
    }

    pub fn find_matches(&self) -> &[oxideterm_editor_core::FindMatch] {
        &self.find_matches
    }

    pub fn active_find_position(&self) -> Option<(usize, usize)> {
        self.active_find_index
            .map(|index| (index + 1, self.find_matches.len()))
            .filter(|(_, total)| *total > 0)
    }

    pub fn set_find_case_sensitive(&mut self, case_sensitive: bool, cx: &mut Context<Self>) {
        if self.settings.find_case_sensitive == case_sensitive {
            return;
        }
        self.settings.find_case_sensitive = case_sensitive;
        self.refresh_find_matches();
        self.active_find_index = (!self.find_matches.is_empty()).then_some(0);
        self.select_active_find_match();
        cx.notify();
    }

    pub fn set_find_regex(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.find_regex == enabled {
            return;
        }
        self.find_regex = enabled;
        self.refresh_find_matches();
        self.active_find_index = (!self.find_matches.is_empty()).then_some(0);
        self.select_active_find_match();
        cx.notify();
    }

    fn find_pattern(&self) -> Result<regex::Regex, regex::Error> {
        let mut cache = self.find_pattern_cache.borrow_mut();
        if let Some(cached) = cache.as_ref()
            && cached.query == self.find_query
            && cached.case_sensitive == self.settings.find_case_sensitive
            && cached.whole_word == self.settings.find_whole_word
        {
            return cached.pattern.clone();
        }
        let pattern = if self.settings.find_whole_word {
            format!(r"\b(?:{})\b", self.find_query)
        } else {
            self.find_query.clone()
        };
        let pattern = regex::RegexBuilder::new(&pattern)
            .case_insensitive(!self.settings.find_case_sensitive)
            .multi_line(true)
            .build();
        *cache = Some(FindPatternCache {
            query: self.find_query.clone(),
            case_sensitive: self.settings.find_case_sensitive,
            whole_word: self.settings.find_whole_word,
            pattern: pattern.clone(),
        });
        pattern
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
        self.select_active_find_match();
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
        self.select_active_find_match();
        cx.notify();
    }

    pub fn replace_current_find_match(
        &mut self,
        replacement: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let Some(hit) = self.active_find_match() else {
            return;
        };
        let mut replacement = replacement.into();
        if self.find_regex {
            let Ok(pattern) = self.find_pattern() else {
                return;
            };
            let expanded = self.buffer.with_text(|text| {
                let captures = pattern.captures_at(text, hit.range.start.0)?;
                let matched = captures.get(0)?;
                if matched.start() != hit.range.start.0 || matched.end() != hit.range.end.0 {
                    return None;
                }
                let mut expanded = String::new();
                captures.expand(&replacement, &mut expanded);
                Some(expanded)
            });
            let Some(expanded) = expanded else {
                return;
            };
            replacement = expanded;
        }
        self.replace_range_with_caret(hit.range, replacement, cx);
        self.refresh_find_matches();
    }

    pub fn replace_all_find_matches(
        &mut self,
        replacement: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        if self.read_only || self.find_query.is_empty() {
            return;
        }
        let replacement = replacement.into();
        let transaction = if self.find_regex {
            let Ok(pattern) = self.find_pattern() else {
                return;
            };
            self.buffer.with_text(|text| {
                let edits = pattern
                    .captures_iter(text)
                    .filter_map(|captures| {
                        let matched = captures.get(0)?;
                        if matched.is_empty() {
                            return None;
                        }
                        let mut expanded = String::new();
                        captures.expand(&replacement, &mut expanded);
                        Some(TextEdit::new(
                            TextRange::new(
                                BufferOffset(matched.start()),
                                BufferOffset(matched.end()),
                            ),
                            expanded,
                        ))
                    })
                    .collect();
                EditTransaction::new(edits)
            })
        } else {
            self.buffer.with_text(|text| {
                replace_all_transaction(text, &self.find_query, &replacement, self.find_options())
            })
        };
        if transaction.is_empty() {
            return;
        }
        if self.buffer.apply_transaction(transaction).is_ok() {
            self.cursor
                .set_selection(Selection::caret(BufferOffset::ZERO));
            self.secondary_selections.clear();
            self.marked_text = None;
            self.save_status = super::EditorSaveStatus::Dirty;
            // Replace-all can touch many ranges, so rebuild tree-sitter state
            // instead of pretending a single incremental edit exists.
            self.request_syntax(None, true, cx);
            self.clear_folds_after_buffer_change();
            self.refresh_find_matches();
            self.viewport
                .clamp(self.document_row_count(), self.metrics.line_height);
            cx.notify();
        }
    }

    pub(super) fn refresh_find_matches(&mut self) {
        self.find_regex_error = false;
        if self.find_query.is_empty() {
            self.find_pattern_cache.borrow_mut().take();
            self.find_matches.clear();
            self.find_line_matches.clear();
            self.active_find_index = None;
            return;
        }
        self.find_matches = if self.find_regex {
            match self.find_pattern() {
                Ok(pattern) => self.buffer.with_text(|text| {
                    pattern
                        .find_iter(text)
                        .filter(|matched| !matched.is_empty())
                        .map(|matched| FindMatch {
                            range: TextRange::new(
                                BufferOffset(matched.start()),
                                BufferOffset(matched.end()),
                            ),
                        })
                        .collect()
                }),
                Err(_) => {
                    self.find_regex_error = true;
                    Vec::new()
                }
            }
        } else {
            self.buffer
                .with_text(|text| find_all(text, &self.find_query, self.find_options()))
        };
        self.find_line_matches = self.build_find_line_matches();
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

    fn select_active_find_match(&mut self) {
        let Some(hit) = self.active_find_match() else {
            return;
        };
        self.cursor
            .set_selection(Selection::new(hit.range.start, hit.range.end));
        self.secondary_selections.clear();
        if let Ok(position) = self.buffer.offset_to_line_col(hit.range.start) {
            self.unfold_line_if_hidden(position.line);
            let text = self.buffer.line_text(position.line).unwrap_or_default();
            let column = super::coords::visual_column_for_byte_column(&text, position.column);
            if let Some((index, _, _)) = super::wrap::display_row_for_visual_column(
                &self.display_rows(),
                position.line,
                column,
            ) {
                self.reveal_display_row(index);
            }
        }
    }

    fn find_options(&self) -> FindOptions {
        FindOptions {
            case_sensitive: self.settings.find_case_sensitive,
            whole_word: self.settings.find_whole_word,
        }
    }
}
