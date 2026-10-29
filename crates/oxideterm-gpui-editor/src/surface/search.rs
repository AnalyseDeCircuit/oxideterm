// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::Context;
use oxideterm_editor_core::{
    BufferOffset, EditTransaction, FindMatch, FindMatches, FindOptions, Selection, TextEdit,
    TextRange, TextSnapshot,
};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use super::TextEditorView;

pub(super) struct FindPatternCache {
    query: String,
    case_sensitive: bool,
    whole_word: bool,
    pattern: Result<regex::Regex, regex::Error>,
}

// Small buffers complete inline to keep navigation and inline inputs immediate.
// Larger scans belong to one cancellable worker per editor, not the UI executor.
const INLINE_SEARCH_BYTES: usize = 64 * 1024;

#[derive(Default)]
pub(super) struct FindWork {
    generation: Arc<AtomicU64>,
    task: Option<gpui::Task<()>>,
    pub(super) pending: bool,
    select_first: bool,
    reveal_pending: bool,
    actions: VecDeque<FindAction>,
}

impl Drop for FindWork {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

enum FindAction {
    Next,
    Previous,
    AddCursor,
    Replace(String),
    ReplaceAll(String),
}

fn search_snapshot(
    text: &TextSnapshot,
    query: &str,
    options: FindOptions,
    pattern: Option<&regex::Regex>,
    cancelled: impl Fn() -> bool,
) -> Option<FindMatches> {
    if cancelled() {
        return None;
    }
    if let Some(pattern) = pattern {
        text.with_text(|text| {
            let mut matches = FindMatches::default();
            for hit in pattern.find_iter(text) {
                if cancelled() {
                    return None;
                }
                if !hit.is_empty() {
                    matches.push(FindMatch {
                        range: TextRange::new(BufferOffset(hit.start()), BufferOffset(hit.end())),
                    });
                }
            }
            (!cancelled()).then_some(matches)
        })
    } else {
        text.find_all_controlled(query, options, cancelled)
    }
}

impl TextEditorView {
    pub fn set_find_query(&mut self, query: impl Into<String>, cx: &mut Context<Self>) {
        self.find_query = query.into();
        self.sync_search_input(cx);
        self.find_work.actions.clear();
        self.request_find(true, cx);
        cx.notify();
    }

    pub fn find_query(&self) -> &str {
        &self.find_query
    }

    pub fn find_matches(&self) -> &FindMatches {
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
        self.find_work.actions.clear();
        self.request_find(true, cx);
        cx.notify();
    }

    pub fn set_find_regex(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.find_regex == enabled {
            return;
        }
        self.find_regex = enabled;
        self.find_work.actions.clear();
        self.request_find(true, cx);
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
        if self.find_work.pending {
            self.find_work.actions.push_back(FindAction::Next);
            return;
        }
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
        if self.find_work.pending {
            self.find_work.actions.push_back(FindAction::Previous);
            return;
        }
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
        let replacement = replacement.into();
        if self.find_work.pending {
            self.find_work
                .actions
                .push_back(FindAction::Replace(replacement));
            return;
        }
        let Some(hit) = self.active_find_match() else {
            return;
        };
        let mut replacement = replacement;
        if self.find_regex {
            let Ok(pattern) = self.find_pattern() else {
                return;
            };
            let expanded = self.buffer.with_temporary_text(|text| {
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
        if self.find_work.pending {
            self.find_work
                .actions
                .push_back(FindAction::ReplaceAll(replacement));
            return;
        }
        let transaction = if self.find_regex {
            let Ok(pattern) = self.find_pattern() else {
                return;
            };
            self.buffer.with_temporary_text(|text| {
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
            let replacement = oxideterm_editor_core::SharedText::from(replacement);
            EditTransaction::new(
                self.find_matches
                    .iter()
                    .map(|hit| TextEdit::with_shared_text(hit.range, replacement.clone()))
                    .collect(),
            )
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
            self.refresh_find_matches(cx);
            self.viewport
                .clamp(self.document_row_count(), self.metrics.line_height);
            cx.notify();
        }
    }

    pub(super) fn refresh_find_matches(&mut self, cx: &mut Context<Self>) {
        self.request_find(false, cx);
    }

    fn request_find(&mut self, select_first: bool, cx: &mut Context<Self>) {
        self.find_work.select_first = select_first;
        self.find_work.generation.fetch_add(1, Ordering::AcqRel);
        self.find_regex_error = false;
        self.find_matches = FindMatches::default();
        self.find_work.pending = !self.find_query.is_empty();
        if self.find_query.is_empty() {
            self.find_pattern_cache.borrow_mut().take();
            self.active_find_index = None;
            self.find_work.select_first = false;
            self.find_work.reveal_pending = false;
            self.find_work.actions.clear();
            return;
        }
        if self.buffer.len() <= INLINE_SEARCH_BYTES || self.find_work.task.is_none() {
            self.start_find(cx);
        }
    }

    fn start_find(&mut self, cx: &mut Context<Self>) {
        let pattern = if self.find_regex {
            match self.find_pattern() {
                Ok(pattern) => Some(pattern),
                Err(_) => {
                    self.find_regex_error = true;
                    self.publish_find(FindMatches::default(), cx);
                    return;
                }
            }
        } else {
            None
        };
        let snapshot = self.buffer.shared_snapshot();
        let options = self.find_options();
        if snapshot.len() <= INLINE_SEARCH_BYTES {
            let matches = search_snapshot(
                &snapshot,
                &self.find_query,
                options,
                pattern.as_ref(),
                || false,
            )
            .unwrap();
            self.publish_find(matches, cx);
            return;
        }
        let query = self.find_query.clone();
        let token = self.find_work.generation.clone();
        let generation = token.load(Ordering::Acquire);
        let version = self.buffer.version();
        let background = cx.background_executor().spawn(async move {
            search_snapshot(&snapshot, &query, options, pattern.as_ref(), || {
                token.load(Ordering::Acquire) != generation
            })
        });
        self.find_work.task = Some(cx.spawn(async move |weak, cx| {
            let result = background.await;
            let _ = weak.update(cx, |this, cx| {
                this.find_work.task = None;
                if !this.find_work.pending {
                    return;
                }
                if this.find_work.generation.load(Ordering::Acquire) != generation
                    || this.buffer.version() != version
                {
                    this.start_find(cx);
                } else if let Some(matches) = result {
                    this.publish_find(matches, cx);
                }
            });
        }));
    }

    fn publish_find(&mut self, matches: FindMatches, cx: &mut Context<Self>) {
        self.find_matches = matches;
        self.find_work.pending = false;
        let select_first = std::mem::take(&mut self.find_work.select_first);
        if select_first {
            self.active_find_index = (!self.find_matches.is_empty()).then_some(0);
            self.select_active_find_match();
        }
        if self
            .active_find_index
            .is_some_and(|index| index >= self.find_matches.len())
        {
            self.active_find_index = self.find_matches.len().checked_sub(1);
        }
        for action in std::mem::take(&mut self.find_work.actions) {
            match action {
                FindAction::Next => self.select_next_find_match(cx),
                FindAction::Previous => self.select_previous_find_match(cx),
                FindAction::AddCursor => self.add_next_find_match_as_cursor(cx),
                FindAction::Replace(text) => self.replace_current_find_match(text, cx),
                FindAction::ReplaceAll(text) => self.replace_all_find_matches(text, cx),
            }
        }
        cx.notify();
    }

    pub(super) fn active_find_match(&self) -> Option<oxideterm_editor_core::FindMatch> {
        self.active_find_index
            .and_then(|index| self.find_matches.get(index))
    }

    pub(super) fn select_current_word_for_find(&mut self, cx: &mut Context<Self>) {
        let selection = self.cursor.selection();
        let query = if selection.is_caret() {
            self.buffer.word_at(selection.head)
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
        if self.find_work.pending {
            self.find_work.actions.push_back(FindAction::AddCursor);
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
        let hit = self.find_matches.get(index).unwrap();
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
        if !self.render_cache_active {
            self.find_work.reveal_pending = true;
            return;
        }
        self.find_work.reveal_pending = false;
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

    pub(super) fn reveal_pending_find_match(&mut self) {
        if self.find_work.reveal_pending {
            self.find_work.reveal_pending = false;
            self.select_active_find_match();
        }
    }

    fn find_options(&self) -> FindOptions {
        FindOptions {
            case_sensitive: self.settings.find_case_sensitive,
            whole_word: self.settings.find_whole_word,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext;

    #[gpui::test]
    fn pending_search_uses_latest_query_and_version_before_navigation_or_replacement(
        cx: &mut gpui::TestAppContext,
    ) {
        let source = format!("hit\n{}hit", "x\n".repeat(35_000));
        let editor =
            cx.new(|cx| TextEditorView::new(source, &oxideterm_theme::default_tokens(), cx));
        editor.update(cx, |editor, cx| {
            editor.set_find_query("x", cx);
            editor.set_find_query("hit", cx);
            assert!(editor.find_work.pending);
            editor.select_next_find_match(cx);
            editor.set_render_cache_active(false, cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            assert_eq!(editor.active_find_position(), Some((2, 2)));
            assert!(editor.display_rows_cache.borrow().is_none());
            editor.set_render_cache_active(true, cx);
            assert_eq!(
                editor.cursor.selection().range(),
                TextRange::new(BufferOffset(70_004), BufferOffset(70_007))
            );
            editor.replace_range_with_caret(TextRange::caret(BufferOffset(0)), "z", cx);
            editor.replace_current_find_match("done", cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            assert_eq!(editor.buffer.line_text(0).as_deref(), Some("zhit"));
            assert_eq!(editor.buffer.line_text(35_001).as_deref(), Some("done"));
            assert_eq!(
                editor
                    .find_matches
                    .iter()
                    .map(|hit| hit.range.start.0..hit.range.end.0)
                    .collect::<Vec<_>>(),
                [1..4]
            );
            editor.undo(cx);
            editor.set_find_regex(true, cx);
            editor.set_find_query("hit$", cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            assert_eq!(
                editor
                    .find_matches
                    .iter()
                    .map(|hit| hit.range.start.0..hit.range.end.0)
                    .collect::<Vec<_>>(),
                [1..4, 70_005..70_008]
            );
            editor.set_find_query("x", cx);
            editor.set_find_query("", cx);
        });
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(!editor.find_work.pending);
            assert!(editor.find_matches.is_empty());
            assert_eq!(editor.active_find_position(), None);
        });
    }
}
