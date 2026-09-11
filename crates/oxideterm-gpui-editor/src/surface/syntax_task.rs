// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_editor_syntax::{SyntaxError, SyntaxWork};
use std::sync::atomic::Ordering;

// Probe measurements establish cooperative slices, not a hard lexer deadline.
const SYNTAX_SLICE: Duration = Duration::from_millis(2);

pub(super) struct SyntaxRequest {
    generation: u64,
    version: u64,
    language: LanguageId,
    text: Arc<String>,
    edit: Option<SyntaxEdit>,
    reset: bool,
    tab_size: usize,
}

struct SyntaxState {
    version: Option<u64>,
    syntax: Option<SyntaxSession>,
    highlights: HighlightCache,
    structure: StructureCache,
    brackets: HashMap<usize, BracketPair>,
}

// A completion can be dropped by a closing view or a cancelled foreground
// receiver. Release large trees and indexes on the background executor either way.
struct OwnedSyntax {
    state: Option<SyntaxState>,
    executor: gpui::BackgroundExecutor,
}

impl Drop for OwnedSyntax {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            if state.syntax.is_some() {
                self.executor
                    .spawn_dedicated(move |_| async move {
                        drop(state);
                    })
                    .detach();
            }
        }
    }
}

impl TextEditorView {
    fn take_syntax_state(&mut self) -> OwnedSyntax {
        OwnedSyntax {
            state: Some(SyntaxState {
                version: self.syntax_version.take(),
                syntax: self.syntax.take(),
                highlights: std::mem::take(&mut self.highlight_spans),
                structure: std::mem::take(&mut self.structure_cache),
                brackets: std::mem::take(&mut self.bracket_pair_by_caret),
            }),
            executor: self.syntax_executor.clone(),
        }
    }

    pub(super) fn request_syntax(
        &mut self,
        edit: Option<SyntaxEdit>,
        reset: bool,
        cx: &mut Context<Self>,
    ) {
        let generation = self.syntax_generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.highlight_chunk_cache.borrow_mut().clear();
        let Some(language) = self.language.filter(|_| !self.is_large_file()) else {
            self.pending_syntax = None;
            drop(self.take_syntax_state());
            return;
        };
        let request = SyntaxRequest {
            generation,
            version: self.buffer.version(),
            language,
            text: self.buffer.text_snapshot(),
            edit,
            reset,
            tab_size: self.settings.tab_size,
        };
        if self.syntax_task.is_some() {
            // Text transactions remain in TextBuffer. Only derived work is coalesced.
            self.pending_syntax = Some(request);
        } else {
            let runtime = self.take_syntax_state();
            self.start_syntax(request, runtime, cx);
        }
    }

    fn start_syntax(
        &mut self,
        request: SyntaxRequest,
        runtime: OwnedSyntax,
        cx: &mut Context<Self>,
    ) {
        let generation = request.generation;
        let version = request.version;
        let language = request.language;
        let token = self.syntax_generation.clone();
        let background = self.syntax_executor.spawn_dedicated(move |_| async move {
            let work = SyntaxWork::new(token, generation, SYNTAX_SLICE);
            compute(runtime, &request, &work)
        });
        self.syntax_task = Some(cx.spawn(async move |weak, cx| {
            let result = background.await;
            let _ = weak.update(cx, |this, cx| {
                this.syntax_task = None;
                if let Err(error) = &result {
                    if !matches!(error, SyntaxError::ParseCancelled) {
                        tracing::warn!(%error, "Editor syntax calculation failed");
                    }
                }
                if let Some(pending) = this.pending_syntax.take() {
                    let runtime = result.unwrap_or_else(|_| this.take_syntax_state());
                    this.start_syntax(pending, runtime, cx);
                    return;
                }
                if let Ok(runtime) = result {
                    this.publish_syntax(generation, version, language, runtime, cx);
                }
            });
        }));
    }

    fn publish_syntax(
        &mut self,
        generation: u64,
        version: u64,
        language: LanguageId,
        mut runtime: OwnedSyntax,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.syntax_generation.load(Ordering::Acquire) != generation
            || self.buffer.version() != version
            || self.language != Some(language)
        {
            return false;
        }
        let state = runtime
            .state
            .take()
            .expect("completed syntax owns its state");
        self.syntax = state.syntax;
        self.syntax_version = state.version;
        self.highlight_spans = state.highlights;
        self.structure_cache = state.structure;
        self.bracket_pair_by_caret = state.brackets;
        self.highlight_chunk_cache.borrow_mut().clear();
        if !self.folded_ranges.is_empty() {
            self.refresh_foldable_ranges();
        }
        // Completion must repaint even when the user has stopped typing.
        cx.notify();
        true
    }
}

impl Drop for TextEditorView {
    fn drop(&mut self) {
        self.syntax_generation.fetch_add(1, Ordering::AcqRel);
        drop(self.take_syntax_state());
    }
}

fn compute(
    mut runtime: OwnedSyntax,
    request: &SyntaxRequest,
    work: &SyntaxWork,
) -> Result<OwnedSyntax, SyntaxError> {
    let result = (|| -> Result<(), SyntaxError> {
        work.checkpoint()?;
        let state = runtime.state.as_mut().expect("worker owns syntax state");
        let same_language = state
            .syntax
            .as_ref()
            .is_some_and(|syntax| syntax.language_id() == request.language);
        let change = if !request.reset && same_language && state.version == Some(request.version) {
            None
        } else if !request.reset
            && same_language
            && state.version.and_then(|version| version.checked_add(1)) == Some(request.version)
            && let Some(edit) = request.edit
        {
            Some(state.syntax.as_mut().unwrap().apply_edit_controlled(
                &request.text,
                edit,
                Some(work),
            )?)
        } else {
            state.syntax = Some(SyntaxSession::parse_controlled(
                request.language,
                &request.text,
                Some(work),
            )?);
            None
        };
        let syntax = state.syntax.as_ref().unwrap();
        state
            .highlights
            .update_controlled(syntax, &request.text, change.as_ref(), Some(work))?;
        state.structure.update_controlled(
            syntax,
            &request.text,
            request.tab_size,
            change.as_ref(),
            Some(work),
        )?;
        let pairs = syntax.brackets_controlled(&request.text, Some(work))?;
        let mut brackets = HashMap::with_capacity(pairs.len().saturating_mul(4));
        for pair in pairs {
            work.checkpoint()?;
            for caret in [
                pair.open.0,
                pair.open.0.saturating_add(1),
                pair.close.0,
                pair.close.0.saturating_add(1),
            ] {
                brackets.entry(caret).or_insert_with(|| pair.clone());
            }
        }
        state.brackets = brackets;
        state.version = Some(request.version);
        work.checkpoint()
    })();
    // A cancelled parse may have edited its tree or partially moved cache blocks.
    // Discard that private state rather than reusing it for a different input.
    match result {
        Ok(()) => Ok(runtime),
        Err(error) => {
            // Already on the dedicated worker: finish teardown before starting
            // another request, so cancelled jobs cannot accumulate cleanup work.
            drop(runtime.state.take());
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};

    #[gpui::test]
    fn tab_width_recalculation_preserves_an_existing_fold(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| {
            TextEditorView::new(
                "fn main() {\n\tcall();\n\t}\n",
                &oxideterm_theme::default_tokens(),
                cx,
            )
        });
        editor.update(cx, |editor, cx| {
            editor.set_language(Some(LanguageId::Rust), cx)
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            assert!(editor.toggle_fold_at_line(0, cx));
            let mut settings = editor.settings.clone();
            settings.tab_size = 8;
            editor.set_settings(settings, cx);
            assert_eq!(
                editor
                    .folded_ranges
                    .iter()
                    .map(|fold| (fold.start_line, fold.end_line))
                    .collect::<Vec<_>>(),
                [(0, 2)]
            );
        });
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert_eq!(
                editor
                    .folded_ranges
                    .iter()
                    .map(|fold| (fold.start_line, fold.end_line))
                    .collect::<Vec<_>>(),
                [(0, 2)]
            );
            assert_eq!(editor.structure_cache.columns_for_line(1), [8]);
        });
    }

    #[gpui::test]
    fn a_completed_old_result_cannot_overwrite_a_new_document(cx: &mut TestAppContext) {
        let editor =
            cx.new(|cx| TextEditorView::new("fn old() {}", &oxideterm_theme::default_tokens(), cx));
        editor.update(cx, |editor, cx| {
            editor.set_language(Some(LanguageId::Rust), cx)
        });
        cx.run_until_parked();
        let (generation, version, completed) = editor.update(cx, |editor, _| {
            (
                editor.syntax_generation.load(Ordering::Acquire),
                editor.buffer.version(),
                editor.take_syntax_state(),
            )
        });
        editor.update(cx, |editor, cx| {
            editor.replace_text_external("{\"value\": 9}", cx);
            editor.set_language(Some(LanguageId::Json), cx);
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            assert!(!editor.publish_syntax(generation, version, LanguageId::Rust, completed, cx));
            assert_eq!(editor.buffer.text(), "{\"value\": 9}");
            assert_eq!(
                editor.syntax.as_ref().unwrap().language_id(),
                LanguageId::Json
            );
            assert_eq!(editor.syntax_version, Some(editor.buffer.version()));
        });
        cx.run_until_parked();
    }

    #[gpui::test]
    fn queued_edits_coalesce_and_complete_without_more_input(cx: &mut TestAppContext) {
        let editor = cx.new(|cx| {
            TextEditorView::new("fn main() {}\n", &oxideterm_theme::default_tokens(), cx)
        });
        editor.update(cx, |editor, cx| {
            editor.set_language(Some(LanguageId::Rust), cx)
        });
        cx.run_until_parked();
        editor.update(cx, |editor, cx| {
            editor
                .cursor
                .set_selection(Selection::caret(BufferOffset(3)));
            for text in ["a", "b", "c"] {
                editor.insert_text(text, cx);
            }
            assert_eq!(editor.buffer.text(), "fn abcmain() {}\n");
            assert_eq!(
                editor.pending_syntax.as_ref().unwrap().text.as_str(),
                "fn abcmain() {}\n"
            );
            assert!(
                editor.highlight_spans.is_empty(),
                "old coordinates remained visible"
            );
        });
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(editor.pending_syntax.is_none());
            assert!(editor.syntax_task.is_none());
            assert_eq!(editor.syntax_version, Some(editor.buffer.version()));
            assert_eq!(
                editor
                    .highlight_spans
                    .spans_in_range(0..editor.buffer.len())
                    .collect::<Vec<_>>(),
                SyntaxSession::parse(LanguageId::Rust, "fn abcmain() {}\n")
                    .unwrap()
                    .highlight_spans("fn abcmain() {}\n")
            );
        });
    }

    #[gpui::test]
    fn replacing_document_and_language_rejects_the_queued_result(cx: &mut TestAppContext) {
        let editor =
            cx.new(|cx| TextEditorView::new("fn old() {}", &oxideterm_theme::default_tokens(), cx));
        editor.update(cx, |editor, cx| {
            editor.set_language(Some(LanguageId::Rust), cx);
            editor.replace_text_external("{\"value\": 7}", cx);
            editor.set_language(Some(LanguageId::Json), cx);
        });
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.buffer.text(), "{\"value\": 7}");
            assert_eq!(
                editor.syntax.as_ref().unwrap().language_id(),
                LanguageId::Json
            );
            assert_eq!(editor.syntax_version, Some(editor.buffer.version()));
            assert_eq!(
                editor
                    .highlight_spans
                    .spans_in_range(0..12)
                    .collect::<Vec<_>>(),
                SyntaxSession::parse(LanguageId::Json, "{\"value\": 7}")
                    .unwrap()
                    .highlight_spans("{\"value\": 7}")
            );
        });
    }

    #[gpui::test]
    fn closing_a_view_invalidates_queued_syntax_work(cx: &mut TestAppContext) {
        let (editor, cx) = cx.add_window_view(|_, cx| {
            TextEditorView::new("fn main() {}", &oxideterm_theme::default_tokens(), cx)
        });
        let (token, generation) = editor.update(cx, |editor, cx| {
            editor.set_language(Some(LanguageId::Rust), cx);
            (
                editor.syntax_generation.clone(),
                editor.syntax_generation.load(Ordering::Acquire),
            )
        });
        let weak = editor.downgrade();
        cx.update(|window, _| window.remove_window());
        drop(editor);
        cx.cx.update(|_| {});
        cx.run_until_parked();
        assert!(weak.upgrade().is_none());
        assert_ne!(token.load(Ordering::Acquire), generation);
    }
}
