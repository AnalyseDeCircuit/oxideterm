// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only
// Hallmark · pre-emit critique: P5 H5 E5 S5 R5 V4

use super::*;

use gpui::EventEmitter;
use oxideterm_editor_syntax::LanguageId;
use oxideterm_gpui_editor::{EditorContextMenuLabels, EditorPresentation, TextEditorView};
use oxideterm_gpui_ui::{
    EntityListRowOptions, IconButtonOptions, SegmentedControlOptions, ToolbarButtonOptions,
    button::{ButtonRadius, ButtonVariant},
    color_for_background, entity_list_row,
};

pub(in crate::workspace) const KNOWLEDGE_WORKSPACE_SECTION_COUNT: usize = 4;
pub(in crate::workspace) const KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT: f32 = 44.0;
pub(in crate::workspace) const KNOWLEDGE_WORKSPACE_SECTION_OVERSCAN: usize = 8;
const KNOWLEDGE_INSTANT_BLOCK_ESTIMATED_HEIGHT: f32 = 96.0;
const KNOWLEDGE_INSTANT_BLOCK_OVERSCAN: usize = 6;
const KNOWLEDGE_NAVIGATOR_ACTION_SIZE: f32 = 28.0;
const KNOWLEDGE_NAVIGATOR_ACTION_ICON_SIZE: f32 = 14.0;
const KNOWLEDGE_NAVIGATOR_ROW_ICON_SIZE: f32 = 16.0;
const KNOWLEDGE_NAVIGATOR_SEARCH_HEIGHT: f32 = 32.0;
const KNOWLEDGE_NAVIGATOR_SEARCH_VERTICAL_PADDING: f32 = 12.0;
const KNOWLEDGE_NAVIGATOR_COLLECTION_MAX_VISIBLE_ROWS: usize = 5;
const KNOWLEDGE_NARROW_VIEWPORT_WIDTH: f32 = 720.0;
const KNOWLEDGE_NARROW_NAVIGATOR_HEIGHT_RATIO: f32 = 0.38;
const KNOWLEDGE_COMPACT_EDITOR_HEADER_ROWS: f32 = 2.0;
const KNOWLEDGE_EDITOR_MODE_SWITCHER_WIDTH: f32 = 264.0;
const KNOWLEDGE_BACKGROUND_SURFACE_ALPHA: u32 = 0x66;
const KNOWLEDGE_PREVIEW_PADDING: f32 = 24.0;
const KNOWLEDGE_AUTOSAVE_DELAY: Duration = Duration::from_millis(1_200);
const KNOWLEDGE_INDEX_STATE_POLL_INTERVAL: Duration = Duration::from_secs(1);
const KNOWLEDGE_NAVIGATOR_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum KnowledgeWorkspaceLayout {
    MainWindow,
    DetachedWindow,
}

fn editable_source_blocks(source: &str) -> Vec<oxideterm_gpui_markdown::MarkdownSourceBlock> {
    let mut blocks = oxideterm_gpui_markdown::parse_source_blocks(source).blocks;
    if blocks.is_empty() {
        // Empty and whitespace-only documents still need a focusable block in Instant mode.
        blocks.push(oxideterm_gpui_markdown::MarkdownSourceBlock {
            range: 0..source.len(),
            kind: oxideterm_gpui_markdown::MarkdownSourceBlockKind::Other,
        });
    }
    blocks
}

fn knowledge_document_matches(document: &oxideterm_ai::RagDocumentResponse, query: &str) -> bool {
    let terms = query.split_whitespace().map(str::to_lowercase);
    let searchable = format!(
        "{} {} {}",
        document.title,
        document.format,
        document.source_path.as_deref().unwrap_or_default()
    )
    .to_lowercase();
    terms.into_iter().all(|term| searchable.contains(&term))
}

fn knowledge_workspace_available_width(
    viewport_width: f32,
    zen_mode: bool,
    activity_bar_width: f32,
    sidebar_collapsed: bool,
    sidebar_panel_width: f32,
    context_sidebar_visible: bool,
    context_sidebar_width: f32,
) -> f32 {
    if zen_mode {
        return viewport_width;
    }
    // Knowledge is rendered inside the center column, so both persistent sidebar regions must be
    // removed before choosing its horizontal or stacked layout.
    let left_width = activity_bar_width
        + if sidebar_collapsed {
            0.0
        } else {
            sidebar_panel_width
        };
    let right_width = if context_sidebar_visible {
        context_sidebar_width
    } else {
        0.0
    };
    (viewport_width - left_width - right_width).max(0.0)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KnowledgeEditorMode {
    Source,
    Instant,
    Preview,
}

#[derive(Clone, Copy)]
enum KnowledgeFormatAction {
    Undo,
    Redo,
    Heading(u8),
    Bold,
    Italic,
    Strike,
    InlineCode,
    CodeBlock,
    Link,
    Image,
    Table,
    HorizontalRule,
    Quote,
    BulletList,
    OrderedList,
    TaskList,
}

fn knowledge_format_wrap(action: KnowledgeFormatAction) -> Option<(&'static str, &'static str)> {
    // Keep paired Markdown markers in one mapping so toolbar semantics can be
    // verified without constructing a GPUI editor entity.
    match action {
        KnowledgeFormatAction::Bold => Some(("**", "**")),
        KnowledgeFormatAction::Italic => Some(("*", "*")),
        KnowledgeFormatAction::Strike => Some(("~~", "~~")),
        KnowledgeFormatAction::InlineCode => Some(("`", "`")),
        KnowledgeFormatAction::CodeBlock => Some(("```\n", "\n```")),
        KnowledgeFormatAction::Link => Some(("[", "](url)")),
        KnowledgeFormatAction::Image => Some(("![", "](url)")),
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum KnowledgeFormatGlyph {
    Text(&'static str),
    Icon(LucideIcon),
}

#[derive(Clone)]
struct KnowledgeEditorLabels {
    source: String,
    instant: String,
    preview: String,
    save: String,
    saved: String,
    saving: String,
    dirty: String,
    conflict: String,
    save_failed: String,
    load_failed: String,
    navigator_load_failed: String,
    keyword_pending: String,
    keyword_failed: String,
    semantic_pending: String,
    empty: String,
    loading: String,
    format_undo: String,
    format_redo: String,
    format_heading: String,
    format_bold: String,
    format_italic: String,
    format_strike: String,
    format_inline_code: String,
    format_code_block: String,
    format_link: String,
    format_image: String,
    format_table: String,
    format_horizontal_rule: String,
    format_quote: String,
    format_bullet_list: String,
    format_ordered_list: String,
    format_task_list: String,
    switch_title: String,
    switch_description: String,
    close_title: String,
    close_description: String,
    quit_title: String,
    quit_description: String,
    discard: String,
    cancel: String,
    reload: String,
    copy_draft: String,
    copy: String,
    cut: String,
    paste: String,
    select_all: String,
}

struct ActiveKnowledgeBlock {
    index: usize,
    range: std::ops::Range<usize>,
    editor: Entity<TextEditorView>,
    _observer: Subscription,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum KnowledgeDocumentSaveState {
    Saved,
    Dirty,
    Saving,
    Conflict,
    Failed(String),
}

fn knowledge_save_state_allows_autosave(state: &KnowledgeDocumentSaveState) -> bool {
    !matches!(
        state,
        KnowledgeDocumentSaveState::Saving | KnowledgeDocumentSaveState::Conflict
    )
}

enum KnowledgeDocumentEditorEvent {
    Saved,
}

#[derive(Clone, Default)]
struct KnowledgeNavigatorSnapshot {
    collections: Arc<Vec<oxideterm_ai::RagCollectionResponse>>,
    selected_collection_id: Option<String>,
    selected_collection: Option<oxideterm_ai::RagCollectionResponse>,
    documents: Arc<Vec<oxideterm_ai::RagDocumentResponse>>,
    error: Option<String>,
    loaded: bool,
}

/// Owns the currently selected document draft inside the single Knowledge workspace tab.
struct KnowledgeDocumentEditor {
    document_id: String,
    collection_id: String,
    title: String,
    version: u64,
    store: Arc<oxideterm_ai::RagStore>,
    tokens: ThemeTokens,
    labels: KnowledgeEditorLabels,
    editor: Entity<TextEditorView>,
    _editor_observer: Subscription,
    observed_buffer_version: u64,
    draft: Arc<str>,
    saved_draft: Arc<str>,
    mode: KnowledgeEditorMode,
    source_blocks: Vec<oxideterm_gpui_markdown::MarkdownSourceBlock>,
    document_context: oxideterm_gpui_markdown::MarkdownDocumentContext,
    active_block: Option<ActiveKnowledgeBlock>,
    instant_list_state: ListState,
    save_state: KnowledgeDocumentSaveState,
    keyword_index: oxideterm_ai::RagKeywordIndexState,
    semantic_index: oxideterm_ai::RagSemanticIndexState,
    save_generation: u64,
    autosave_generation: u64,
    autosave_task: Option<Task<()>>,
    index_state_task: Option<Task<()>>,
    preview_scroll: MarkdownVirtualListScrollHandle,
    compact_layout: bool,
    has_background_image: bool,
}

impl KnowledgeDocumentEditor {
    fn is_dirty(&self) -> bool {
        self.draft.as_ref() != self.saved_draft.as_ref()
    }

    fn save_current_draft(&mut self, cx: &mut Context<Self>) {
        self.request_save(self.draft.to_string(), cx);
    }

    fn new(
        loaded: oxideterm_ai::RagDocumentContentResponse,
        store: Arc<oxideterm_ai::RagStore>,
        tokens: ThemeTokens,
        labels: KnowledgeEditorLabels,
        has_background_image: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let content = loaded.content;
        let semantic_index = loaded.semantic_index;
        let keyword_index = oxideterm_ai::rag_keyword_index_state(&store);
        let editor_labels = EditorContextMenuLabels {
            copy: labels.copy.clone(),
            cut: labels.cut.clone(),
            paste: labels.paste.clone(),
            select_all: labels.select_all.clone(),
        };
        let editor = cx.new(|cx| {
            let mut editor = TextEditorView::new(content.clone(), &tokens, cx);
            editor.set_context_menu_labels(editor_labels);
            editor.set_language(Some(LanguageId::Markdown), cx);
            editor.set_transparent_background(has_background_image, cx);
            editor
        });
        let observed_buffer_version = editor.read(cx).buffer().version();
        let draft = Arc::<str>::from(content);
        let saved_draft = draft.clone();
        let source_blocks = editable_source_blocks(&draft);
        let document_context =
            oxideterm_gpui_markdown::MarkdownDocumentContext::from_source(&draft);
        let instant_list_state = ListState::new(
            source_blocks.len(),
            ListAlignment::Top,
            TauriVirtualListSpec::new(
                px(KNOWLEDGE_INSTANT_BLOCK_ESTIMATED_HEIGHT),
                KNOWLEDGE_INSTANT_BLOCK_OVERSCAN,
            )
            .overdraw(),
        )
        .measure_all();
        let editor_observer = cx.observe(&editor, |surface, editor, cx| {
            let editor = editor.read(cx);
            let buffer_version = editor.buffer().version();
            if buffer_version == surface.observed_buffer_version {
                return;
            }
            surface.observed_buffer_version = buffer_version;
            surface.draft = Arc::from(editor.buffer().text());
            if knowledge_save_state_allows_autosave(&surface.save_state) {
                surface.save_state = KnowledgeDocumentSaveState::Dirty;
                surface.schedule_autosave(cx);
            }
            cx.notify();
        });
        Self {
            document_id: loaded.document.id,
            collection_id: loaded.document.collection_id,
            title: loaded.document.title,
            version: loaded.document.version,
            store,
            tokens,
            labels,
            editor,
            _editor_observer: editor_observer,
            observed_buffer_version,
            draft,
            saved_draft,
            mode: KnowledgeEditorMode::Instant,
            source_blocks,
            document_context,
            active_block: None,
            instant_list_state,
            save_state: KnowledgeDocumentSaveState::Saved,
            keyword_index,
            semantic_index,
            save_generation: 0,
            autosave_generation: 0,
            autosave_task: None,
            index_state_task: None,
            preview_scroll: MarkdownVirtualListScrollHandle::new(),
            compact_layout: false,
            has_background_image,
        }
    }

    fn set_compact_layout(&mut self, compact_layout: bool) {
        // The parent workspace owns the sidebars, so it passes the actual remaining content width
        // into this child instead of making the editor guess from the full window viewport.
        self.compact_layout = compact_layout;
    }

    fn set_has_background_image(&mut self, has_background_image: bool, cx: &mut Context<Self>) {
        if self.has_background_image == has_background_image {
            return;
        }
        self.has_background_image = has_background_image;
        self.editor.update(cx, |editor, cx| {
            editor.set_transparent_background(has_background_image, cx);
        });
        if let Some(active_block) = self.active_block.as_ref() {
            active_block.editor.update(cx, |editor, cx| {
                editor.set_transparent_background(has_background_image, cx);
            });
        }
        cx.notify();
    }

    fn configure_save_callback(surface: &Entity<Self>, cx: &mut App) {
        let weak_surface = surface.downgrade();
        let editor = surface.read(cx).editor.clone();
        editor.update(cx, |editor, _cx| {
            editor.set_on_save(Box::new(move |content, _window, cx| {
                let content = content.to_string();
                weak_surface
                    .update(cx, |surface, cx| surface.request_save(content, cx))
                    .map_err(|_| "knowledge document is no longer open".to_string())?;
                Ok(())
            }));
        });
    }

    fn start_index_state_poll(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.keyword_index,
            oxideterm_ai::RagKeywordIndexState::Ready
                | oxideterm_ai::RagKeywordIndexState::Failed { .. }
        ) && matches!(
            self.semantic_index,
            oxideterm_ai::RagSemanticIndexState::Ready
        ) {
            self.index_state_task = None;
            return;
        }
        self.index_state_task = Some(cx.spawn(async move |surface, cx| {
            loop {
                Timer::after(KNOWLEDGE_INDEX_STATE_POLL_INTERVAL).await;
                let finished = surface
                    .update(cx, |surface, cx| {
                        surface.keyword_index =
                            oxideterm_ai::rag_keyword_index_state(&surface.store);
                        if let Ok(semantic_index) = oxideterm_ai::rag_document_semantic_index_state(
                            &surface.store,
                            &surface.document_id,
                        ) {
                            surface.semantic_index = semantic_index;
                        }
                        let keyword_finished = matches!(
                            surface.keyword_index,
                            oxideterm_ai::RagKeywordIndexState::Ready
                                | oxideterm_ai::RagKeywordIndexState::Failed { .. }
                        );
                        let semantic_finished = matches!(
                            surface.semantic_index,
                            oxideterm_ai::RagSemanticIndexState::Ready
                        );
                        cx.notify();
                        keyword_finished && semantic_finished
                    })
                    .unwrap_or(true);
                if finished {
                    break;
                }
            }
        }));
    }

    fn set_mode(&mut self, mode: KnowledgeEditorMode, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.finish_active_block();
            if mode == KnowledgeEditorMode::Source
                && self.editor.read(cx).buffer().text() != self.draft.as_ref()
            {
                let draft = self.draft.to_string();
                self.editor
                    .update(cx, |editor, cx| editor.replace_text_external(draft, cx));
                self.observed_buffer_version = self.editor.read(cx).buffer().version();
            }
            if mode == KnowledgeEditorMode::Instant {
                // Source edits do not need a block projection until Instant mode is visible.
                // Deferring this parse avoids scanning a large document on every keystroke.
                self.refresh_source_blocks();
            }
            self.mode = mode;
            cx.notify();
        }
    }

    fn refresh_source_blocks(&mut self) {
        self.source_blocks = editable_source_blocks(&self.draft);
        // Instant blocks resolve links against the complete document without reparsing
        // document-level definitions for every visible virtual-list row.
        self.document_context =
            oxideterm_gpui_markdown::MarkdownDocumentContext::from_source(&self.draft);
        self.instant_list_state = ListState::new(
            self.source_blocks.len(),
            ListAlignment::Top,
            TauriVirtualListSpec::new(
                px(KNOWLEDGE_INSTANT_BLOCK_ESTIMATED_HEIGHT),
                KNOWLEDGE_INSTANT_BLOCK_OVERSCAN,
            )
            .overdraw(),
        )
        .measure_all();
    }

    fn finish_active_block(&mut self) {
        if self.active_block.take().is_some() {
            self.refresh_source_blocks();
        }
    }

    fn activate_block(
        &mut self,
        index: usize,
        source_offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .active_block
            .as_ref()
            .is_some_and(|block| block.index == index)
        {
            return;
        }
        self.finish_active_block();
        let index = self
            .source_blocks
            .iter()
            .position(|block| {
                block.range.start == source_offset
                    || (block.range.start <= source_offset && source_offset < block.range.end)
            })
            .unwrap_or(index);
        let Some(block) = self.source_blocks.get(index).cloned() else {
            return;
        };
        let Some(source) = self.draft.get(block.range.clone()).map(str::to_string) else {
            self.refresh_source_blocks();
            return;
        };
        let editor_labels = EditorContextMenuLabels {
            copy: self.labels.copy.clone(),
            cut: self.labels.cut.clone(),
            paste: self.labels.paste.clone(),
            select_all: self.labels.select_all.clone(),
        };
        let editor = cx.new(|cx| {
            let mut editor = TextEditorView::new(source, &self.tokens, cx);
            editor.set_context_menu_labels(editor_labels);
            editor.set_language(Some(LanguageId::Markdown), cx);
            editor.set_presentation(EditorPresentation::Inline, cx);
            editor.set_transparent_background(self.has_background_image, cx);
            editor
        });
        let weak_surface = cx.entity().downgrade();
        editor.update(cx, |editor, _cx| {
            editor.set_on_save(Box::new(move |content, _window, cx| {
                let content = content.to_string();
                weak_surface
                    .update(cx, |surface, cx| {
                        // The save callback may run before an observer delivery for the final
                        // keystroke, so synchronize the active block explicitly first.
                        surface.apply_active_block_text(content, cx);
                        surface.request_save(surface.draft.to_string(), cx)
                    })
                    .map_err(|_| "knowledge document is no longer open".to_string())?;
                Ok(())
            }));
        });
        let observer = cx.observe(&editor, |surface, editor, cx| {
            let text = editor.read(cx).buffer().text();
            surface.apply_active_block_text(text, cx);
        });
        let focus_handle = editor.read(cx).focus_handle(cx);
        self.active_block = Some(ActiveKnowledgeBlock {
            index,
            range: block.range,
            editor,
            _observer: observer,
        });
        window.focus(&focus_handle, cx);
        cx.notify();
    }

    fn apply_active_block_text(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(active) = self.active_block.as_mut() else {
            return;
        };
        let Some(current) = self.draft.get(active.range.clone()) else {
            return;
        };
        if current == text {
            return;
        }
        let replacement_range = active.range.clone();
        let old_end = replacement_range.end;
        let old_len = active.range.len();
        // Mirror the inline editor through the canonical editor transaction log so Source mode,
        // save shortcuts, dirty tracking, and undo history all observe the same Markdown draft.
        self.editor.update(cx, |editor, cx| {
            editor.replace_range_external(replacement_range, text.clone(), cx);
        });
        active.range.end = active.range.start + text.len();
        let delta = text.len() as isize - old_len as isize;
        if delta != 0 {
            for block in self.source_blocks.iter_mut().skip(active.index + 1) {
                block.range.start = block.range.start.saturating_add_signed(delta);
                block.range.end = block.range.end.saturating_add_signed(delta);
            }
        }
        if let Some(block) = self.source_blocks.get_mut(active.index) {
            block.range.end = active.range.end;
        }
        debug_assert_eq!(old_end.saturating_add_signed(delta), active.range.end);
        self.observed_buffer_version = self.editor.read(cx).buffer().version();
        self.draft = Arc::from(self.editor.read(cx).buffer().text());
        if knowledge_save_state_allows_autosave(&self.save_state) {
            self.save_state = KnowledgeDocumentSaveState::Dirty;
            self.schedule_autosave(cx);
        }
        cx.notify();
    }

    fn schedule_autosave(&mut self, cx: &mut Context<Self>) {
        self.autosave_generation = self.autosave_generation.wrapping_add(1);
        let generation = self.autosave_generation;
        // Replacing the retained task cancels superseded idle timers, so bursts of edits produce
        // one document save and one index rebuild rather than one rebuild per keystroke.
        self.autosave_task = Some(cx.spawn(async move |surface, cx| {
            Timer::after(KNOWLEDGE_AUTOSAVE_DELAY).await;
            let _ = surface.update(cx, |surface, cx| {
                if generation == surface.autosave_generation
                    && !matches!(surface.save_state, KnowledgeDocumentSaveState::Saving)
                    && surface.is_dirty()
                {
                    surface.save_current_draft(cx);
                }
            });
        }));
    }

    fn request_save(&mut self, content: String, cx: &mut Context<Self>) {
        if matches!(
            self.save_state,
            KnowledgeDocumentSaveState::Saving | KnowledgeDocumentSaveState::Conflict
        ) {
            return;
        }
        if self.draft.as_ref() == self.saved_draft.as_ref() {
            self.save_state = KnowledgeDocumentSaveState::Saved;
            cx.notify();
            return;
        }
        self.save_generation = self.save_generation.wrapping_add(1);
        let generation = self.save_generation;
        let expected_version = self.version;
        let document_id = self.document_id.clone();
        let store = self.store.clone();
        let saved_content: Arc<str> = Arc::from(content.as_str());
        self.save_state = KnowledgeDocumentSaveState::Saving;
        cx.notify();
        cx.spawn(async move |surface, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    oxideterm_ai::rag_save_document(
                        &store,
                        &document_id,
                        content,
                        Some(expected_version),
                    )
                })
                .await;
            let _ = surface.update(cx, |surface, cx| {
                if generation != surface.save_generation {
                    return;
                }
                match result {
                    Ok(outcome) => {
                        surface.version = outcome.document.version;
                        surface.keyword_index = outcome.keyword_index;
                        surface.semantic_index = outcome.semantic_index;
                        surface.start_index_state_poll(cx);
                        if surface.draft.as_ref() == saved_content.as_ref() {
                            surface.saved_draft = saved_content.clone();
                            surface.editor.update(cx, |editor, cx| {
                                if editor.buffer().text() == saved_content.as_ref() {
                                    editor.mark_saved_external(cx);
                                }
                            });
                            if let Some(active) = surface.active_block.as_ref() {
                                active.editor.update(cx, |editor, cx| {
                                    editor.mark_saved_external(cx);
                                });
                            }
                            surface.save_state = KnowledgeDocumentSaveState::Saved;
                            cx.emit(KnowledgeDocumentEditorEvent::Saved);
                        } else {
                            surface.save_state = KnowledgeDocumentSaveState::Dirty;
                            surface.schedule_autosave(cx);
                        }
                    }
                    Err(oxideterm_ai::RagError::VersionConflict { .. }) => {
                        surface.save_state = KnowledgeDocumentSaveState::Conflict;
                    }
                    Err(error) => {
                        surface.save_state = KnowledgeDocumentSaveState::Failed(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save_status_label(&self) -> String {
        match &self.save_state {
            KnowledgeDocumentSaveState::Saved => {
                if let oxideterm_ai::RagKeywordIndexState::Failed { .. } = &self.keyword_index {
                    return self.labels.keyword_failed.clone();
                }
                if matches!(
                    self.keyword_index,
                    oxideterm_ai::RagKeywordIndexState::Pending
                        | oxideterm_ai::RagKeywordIndexState::Rebuilding
                ) {
                    return self.labels.keyword_pending.clone();
                }
                match self.semantic_index {
                    oxideterm_ai::RagSemanticIndexState::Pending { .. } => {
                        self.labels.semantic_pending.clone()
                    }
                    oxideterm_ai::RagSemanticIndexState::Ready => self.labels.saved.clone(),
                }
            }
            KnowledgeDocumentSaveState::Dirty => self.labels.dirty.clone(),
            KnowledgeDocumentSaveState::Saving => self.labels.saving.clone(),
            KnowledgeDocumentSaveState::Conflict => self.labels.conflict.clone(),
            KnowledgeDocumentSaveState::Failed(_error) => self.labels.save_failed.clone(),
        }
    }

    fn reload_after_conflict(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.save_state, KnowledgeDocumentSaveState::Conflict) {
            return;
        }
        self.save_generation = self.save_generation.wrapping_add(1);
        self.autosave_generation = self.autosave_generation.wrapping_add(1);
        self.autosave_task = None;
        let generation = self.save_generation;
        let document_id = self.document_id.clone();
        let store = self.store.clone();
        self.save_state = KnowledgeDocumentSaveState::Saving;
        cx.notify();
        cx.spawn(async move |surface, cx| {
            let load_store = store.clone();
            let load_document_id = document_id.clone();
            let result = cx
                .background_executor()
                .spawn(
                    async move { oxideterm_ai::rag_get_document(&load_store, &load_document_id) },
                )
                .await;
            let _ = surface.update(cx, |surface, cx| {
                if generation != surface.save_generation {
                    return;
                }
                match result {
                    Ok(loaded) => {
                        let content: Arc<str> = Arc::from(loaded.content);
                        surface.finish_active_block();
                        surface.version = loaded.document.version;
                        surface.semantic_index = loaded.semantic_index;
                        surface.draft = content.clone();
                        surface.saved_draft = content.clone();
                        surface.editor.update(cx, |editor, cx| {
                            editor.replace_text_external(content.to_string(), cx);
                            editor.mark_saved_external(cx);
                        });
                        surface.observed_buffer_version =
                            surface.editor.read(cx).buffer().version();
                        surface.refresh_source_blocks();
                        surface.keyword_index =
                            oxideterm_ai::rag_keyword_index_state(&surface.store);
                        surface.start_index_state_poll(cx);
                        surface.save_state = KnowledgeDocumentSaveState::Saved;
                    }
                    Err(error) => {
                        surface.save_state = KnowledgeDocumentSaveState::Failed(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn render_conflict_action(
        &self,
        id: &'static str,
        label: String,
        reload: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let options = ToolbarButtonOptions::compact_text(
            ButtonVariant::Outline,
            ButtonRadius::Sm,
            28.0,
            8.0,
            self.tokens.metrics.ui_text_xs,
        );
        oxideterm_gpui_ui::toolbar_button(&self.tokens, label, None, options)
            .id(id)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |surface, _event, _window, cx| {
                    if reload {
                        surface.reload_after_conflict(cx);
                    } else {
                        cx.write_to_clipboard(ClipboardItem::new_string(surface.draft.to_string()));
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_mode_button(
        &self,
        mode: KnowledgeEditorMode,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.mode == mode;
        oxideterm_gpui_ui::segmented_control_item(&self.tokens, label, active)
            .id(match mode {
                KnowledgeEditorMode::Source => "knowledge-editor-mode-source",
                KnowledgeEditorMode::Instant => "knowledge-editor-mode-instant",
                KnowledgeEditorMode::Preview => "knowledge-editor-mode-preview",
            })
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |surface, _event, _window, cx| {
                    surface.set_mode(mode, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_mode_switcher(&self, cx: &mut Context<Self>) -> AnyElement {
        let active_index = match self.mode {
            KnowledgeEditorMode::Source => 0,
            KnowledgeEditorMode::Instant => 1,
            KnowledgeEditorMode::Preview => 2,
        };
        oxideterm_gpui_ui::segmented_control(
            &self.tokens,
            "knowledge-editor-mode-switcher",
            SegmentedControlOptions::new(active_index, active_index, 3)
                .has_background_image(self.has_background_image)
                .compact(KNOWLEDGE_EDITOR_MODE_SWITCHER_WIDTH),
            vec![
                self.render_mode_button(
                    KnowledgeEditorMode::Source,
                    self.labels.source.clone(),
                    cx,
                ),
                self.render_mode_button(
                    KnowledgeEditorMode::Instant,
                    self.labels.instant.clone(),
                    cx,
                ),
                self.render_mode_button(
                    KnowledgeEditorMode::Preview,
                    self.labels.preview.clone(),
                    cx,
                ),
            ],
        )
        .into_any_element()
    }

    fn render_instant_block(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(block) = self.source_blocks.get(index) else {
            return div().into_any_element();
        };
        let source_offset = block.range.start;
        if let Some(active) = self
            .active_block
            .as_ref()
            .filter(|active| active.index == index)
        {
            let line_count = active.editor.read(cx).buffer().line_count().max(1) as f32;
            let editor_height = (line_count * self.tokens.metrics.markdown_body_font_size * 1.6)
                .max(KNOWLEDGE_INSTANT_BLOCK_ESTIMATED_HEIGHT);
            return div()
                .w_full()
                .h(px(editor_height))
                .px(px(20.0))
                .py(px(6.0))
                .on_key_down(cx.listener(|surface, event: &KeyDownEvent, _window, cx| {
                    if event.keystroke.key.as_str() == "escape" {
                        surface.finish_active_block();
                        cx.stop_propagation();
                        cx.notify();
                    }
                }))
                .child(active.editor.clone())
                .into_any_element();
        }
        let Some(source) = self.draft.get(block.range.clone()).map(str::to_string) else {
            return div().into_any_element();
        };
        let options = MarkdownOptions::from_theme(&self.tokens);
        let rendered_block =
            if block.kind == oxideterm_gpui_markdown::MarkdownSourceBlockKind::FootnoteDefinition {
                // The definition row owns its own rendered footnote body; other rows borrow only
                // document-level numbering and never duplicate the complete footnote section.
                oxideterm_gpui_markdown::markdown_with_options(&self.tokens, &source, &options)
            } else {
                oxideterm_gpui_markdown::markdown_block_with_document_context(
                    &self.tokens,
                    &source,
                    &self.document_context,
                    &options,
                )
            };
        div()
            .id(("knowledge-instant-block", index))
            .w_full()
            .min_h(px(32.0))
            .px(px(24.0))
            .py(px(4.0))
            .rounded(px(self.tokens.radii.sm))
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |surface, _event, window, cx| {
                    surface.activate_block(index, source_offset, window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(rendered_block)
            .into_any_element()
    }

    fn apply_format_action(&mut self, action: KnowledgeFormatAction, cx: &mut Context<Self>) {
        let editor = match self.mode {
            KnowledgeEditorMode::Source => Some(self.editor.clone()),
            KnowledgeEditorMode::Instant => self
                .active_block
                .as_ref()
                .map(|active| active.editor.clone()),
            KnowledgeEditorMode::Preview => None,
        };
        let Some(editor) = editor else {
            return;
        };
        editor.update(cx, |editor, cx| match action {
            KnowledgeFormatAction::Undo => editor.undo_external(cx),
            KnowledgeFormatAction::Redo => editor.redo_external(cx),
            KnowledgeFormatAction::Heading(level) => {
                let prefix = format!("{} ", "#".repeat(usize::from(level)));
                editor.prefix_selected_lines_external(&prefix, cx);
            }
            action @ (KnowledgeFormatAction::Bold
            | KnowledgeFormatAction::Italic
            | KnowledgeFormatAction::Strike
            | KnowledgeFormatAction::InlineCode
            | KnowledgeFormatAction::CodeBlock
            | KnowledgeFormatAction::Link
            | KnowledgeFormatAction::Image) => {
                if let Some((prefix, suffix)) = knowledge_format_wrap(action) {
                    editor.wrap_primary_selection_external(prefix, suffix, cx);
                }
            }
            KnowledgeFormatAction::Table => {
                editor.insert_text("\n|  |  |\n| --- | --- |\n|  |  |\n", cx)
            }
            KnowledgeFormatAction::HorizontalRule => editor.insert_text("\n---\n", cx),
            KnowledgeFormatAction::Quote => editor.prefix_selected_lines_external("> ", cx),
            KnowledgeFormatAction::BulletList => editor.prefix_selected_lines_external("- ", cx),
            KnowledgeFormatAction::OrderedList => editor.prefix_selected_lines_external("1. ", cx),
            KnowledgeFormatAction::TaskList => editor.prefix_selected_lines_external("- [ ] ", cx),
        });
    }

    fn render_format_button(
        &self,
        id: &'static str,
        glyph: KnowledgeFormatGlyph,
        tooltip: String,
        action: KnowledgeFormatAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = self.tokens;
        let (label, icon, show_label) = match glyph {
            KnowledgeFormatGlyph::Text(label) => (label.to_string(), None, true),
            KnowledgeFormatGlyph::Icon(icon) => (
                String::new(),
                Some(
                    svg()
                        .path(icon.path())
                        .size(px(15.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .into_any_element(),
                ),
                false,
            ),
        };
        let mut options = ToolbarButtonOptions::compact_text_min_width(
            ButtonVariant::Ghost,
            ButtonRadius::Sm,
            28.0,
            30.0,
            6.0,
            self.tokens.metrics.ui_text_sm,
        );
        options.show_label = show_label;
        options.button.disabled =
            self.mode == KnowledgeEditorMode::Instant && self.active_block.is_none();
        options.text_color = Some(rgb(self.tokens.ui.text_muted));
        options.hover_text_color = Some(rgb(self.tokens.ui.text));
        oxideterm_gpui_ui::toolbar_button(&self.tokens, label, icon, options)
            .id(id)
            .flex_none()
            .cursor_pointer()
            .tooltip(move |_window, cx| {
                oxideterm_gpui_ui::tooltip::tooltip_view(tokens, tooltip.clone(), None, cx)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |surface, _event, _window, cx| {
                    surface.apply_format_action(action, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_format_separator(&self) -> AnyElement {
        oxideterm_gpui_ui::separator::separator(
            &self.tokens,
            oxideterm_gpui_ui::separator::SeparatorOrientation::Vertical,
        )
        .h(px(18.0))
        .mx(px(4.0))
        .flex_none()
        .into_any_element()
    }
}

impl EventEmitter<KnowledgeDocumentEditorEvent> for KnowledgeDocumentEditor {}

impl Render for KnowledgeDocumentEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let compact_layout = self.compact_layout;
        let source = self.mode == KnowledgeEditorMode::Source;
        let instant = self.mode == KnowledgeEditorMode::Instant;
        let preview = self.mode == KnowledgeEditorMode::Preview;
        let conflict = matches!(self.save_state, KnowledgeDocumentSaveState::Conflict);
        let options = MarkdownOptions::from_theme(&self.tokens);
        let preview_id = format!("knowledge-document-preview-{}", self.document_id);
        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .h(px(self.tokens.metrics.ui_button_lg_height))
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .border_b_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .when(compact_layout, |header| {
                        header
                            .h(px(self.tokens.metrics.ui_button_lg_height
                                * KNOWLEDGE_COMPACT_EDITOR_HEADER_ROWS))
                            .flex_col()
                            .items_start()
                            .justify_center()
                            .py(px(6.0))
                    })
                    .when(!compact_layout, |header| {
                        header.child(
                            div().flex_1().min_w_0().child(
                                div()
                                    .max_w(px(320.0))
                                    .truncate()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.title.clone()),
                            ),
                        )
                    })
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_center()
                            .when(compact_layout, |modes| modes.w_full())
                            .child(self.render_mode_switcher(cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_end()
                            .when(compact_layout, |actions| actions.w_full().justify_between())
                            .gap(px(8.0))
                            .child(
                                div()
                                    .max_w(px(280.0))
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(self.save_status_label()),
                            )
                            .when(conflict, |actions| {
                                actions
                                    .child(self.render_conflict_action(
                                        "knowledge-conflict-copy",
                                        self.labels.copy_draft.clone(),
                                        false,
                                        cx,
                                    ))
                                    .child(self.render_conflict_action(
                                        "knowledge-conflict-reload",
                                        self.labels.reload.clone(),
                                        true,
                                        cx,
                                    ))
                            })
                            .child(
                                oxideterm_gpui_ui::toolbar_button(
                                    &self.tokens,
                                    self.labels.save.clone(),
                                    None,
                                    ToolbarButtonOptions::compact_text(
                                        ButtonVariant::Default,
                                        ButtonRadius::Sm,
                                        30.0,
                                        10.0,
                                        self.tokens.metrics.ui_text_sm,
                                    ),
                                )
                                .id("knowledge-editor-save")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|surface, _event, _window, cx| {
                                        let content = surface.draft.to_string();
                                        surface.request_save(content, cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                            ),
                    ),
            )
            .when(!preview, |surface| {
                surface.child(
                    div()
                        .w_full()
                        .h(px(self.tokens.metrics.ui_button_lg_height))
                        .flex_none()
                        .min_w_0()
                        .overflow_x_scrollbar()
                        .border_b_1()
                        .border_color(rgb(self.tokens.ui.border))
                        .child(
                            div()
                                .h_full()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(4.0))
                                .px(px(16.0))
                                .child(self.render_format_button(
                                    "knowledge-format-undo",
                                    KnowledgeFormatGlyph::Icon(LucideIcon::RotateCcw),
                                    self.labels.format_undo.clone(),
                                    KnowledgeFormatAction::Undo,
                                    cx,
                                ))
                                .child(self.render_format_button(
                                    "knowledge-format-redo",
                                    KnowledgeFormatGlyph::Icon(LucideIcon::RefreshCw),
                                    self.labels.format_redo.clone(),
                                    KnowledgeFormatAction::Redo,
                                    cx,
                                ))
                                .child(self.render_format_separator())
                                .child(self.render_format_button(
                                    "knowledge-format-heading-1",
                                    KnowledgeFormatGlyph::Text("H1"),
                                    format!("{} 1", self.labels.format_heading),
                                    KnowledgeFormatAction::Heading(1),
                                    cx,
                                ))
                                .child(self.render_format_button(
                                    "knowledge-format-heading-2",
                                    KnowledgeFormatGlyph::Text("H2"),
                                    format!("{} 2", self.labels.format_heading),
                                    KnowledgeFormatAction::Heading(2),
                                    cx,
                                ))
                                .child(self.render_format_button(
                                    "knowledge-format-heading-3",
                                    KnowledgeFormatGlyph::Text("H3"),
                                    format!("{} 3", self.labels.format_heading),
                                    KnowledgeFormatAction::Heading(3),
                                    cx,
                                ))
                                .child(self.render_format_separator())
                                .child(self.render_format_button(
                                    "knowledge-format-bold",
                                    KnowledgeFormatGlyph::Text("B"),
                                    self.labels.format_bold.clone(),
                                    KnowledgeFormatAction::Bold,
                                    cx,
                                ))
                                .child(self.render_format_button(
                                    "knowledge-format-italic",
                                    KnowledgeFormatGlyph::Text("I"),
                                    self.labels.format_italic.clone(),
                                    KnowledgeFormatAction::Italic,
                                    cx,
                                ))
                                .child(self.render_format_button(
                                    "knowledge-format-strike",
                                    KnowledgeFormatGlyph::Text("S"),
                                    self.labels.format_strike.clone(),
                                    KnowledgeFormatAction::Strike,
                                    cx,
                                ))
                                .child(self.render_format_button(
                                    "knowledge-format-code",
                                    KnowledgeFormatGlyph::Text("<>"),
                                    self.labels.format_inline_code.clone(),
                                    KnowledgeFormatAction::InlineCode,
                                    cx,
                                ))
                                .child(self.render_format_separator())
                                .child(self.render_format_button(
                                    "knowledge-format-quote",
                                    KnowledgeFormatGlyph::Text("”"),
                                    self.labels.format_quote.clone(),
                                    KnowledgeFormatAction::Quote,
                                    cx,
                                ))
                                .child(self.render_format_button(
                                    "knowledge-format-bullets",
                                    KnowledgeFormatGlyph::Text("•"),
                                    self.labels.format_bullet_list.clone(),
                                    KnowledgeFormatAction::BulletList,
                                    cx,
                                ))
                                .child(self.render_format_button(
                                    "knowledge-format-ordered",
                                    KnowledgeFormatGlyph::Text("1."),
                                    self.labels.format_ordered_list.clone(),
                                    KnowledgeFormatAction::OrderedList,
                                    cx,
                                ))
                                .child(self.render_format_button(
                                    "knowledge-format-task",
                                    KnowledgeFormatGlyph::Icon(LucideIcon::ListChecks),
                                    self.labels.format_task_list.clone(),
                                    KnowledgeFormatAction::TaskList,
                                    cx,
                                ))
                                .child(self.render_format_button(
                                    "knowledge-format-code-block",
                                    KnowledgeFormatGlyph::Icon(LucideIcon::Code2),
                                    self.labels.format_code_block.clone(),
                                    KnowledgeFormatAction::CodeBlock,
                                    cx,
                                ))
                                .child(self.render_format_separator())
                                .child(self.render_format_button(
                                    "knowledge-format-link",
                                    KnowledgeFormatGlyph::Icon(LucideIcon::Link2),
                                    self.labels.format_link.clone(),
                                    KnowledgeFormatAction::Link,
                                    cx,
                                ))
                                .child(self.render_format_button(
                                    "knowledge-format-image",
                                    KnowledgeFormatGlyph::Icon(LucideIcon::Image),
                                    self.labels.format_image.clone(),
                                    KnowledgeFormatAction::Image,
                                    cx,
                                ))
                                .child(self.render_format_button(
                                    "knowledge-format-table",
                                    KnowledgeFormatGlyph::Icon(LucideIcon::FileSpreadsheet),
                                    self.labels.format_table.clone(),
                                    KnowledgeFormatAction::Table,
                                    cx,
                                ))
                                .child(self.render_format_button(
                                    "knowledge-format-horizontal-rule",
                                    KnowledgeFormatGlyph::Text("—"),
                                    self.labels.format_horizontal_rule.clone(),
                                    KnowledgeFormatAction::HorizontalRule,
                                    cx,
                                )),
                        ),
                )
            })
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .when(source, |content| {
                        content.child(div().size_full().child(self.editor.clone()))
                    })
                    .when(instant, |content| {
                        let state = self.instant_list_state.clone();
                        let surface = cx.entity();
                        let spec = TauriVirtualListSpec::new(
                            px(KNOWLEDGE_INSTANT_BLOCK_ESTIMATED_HEIGHT),
                            KNOWLEDGE_INSTANT_BLOCK_OVERSCAN,
                        );
                        content.child(tauri_virtual_list(
                            state,
                            spec,
                            move |index, _window, app| {
                                surface.update(app, |surface, cx| {
                                    surface.render_instant_block(index, cx)
                                })
                            },
                        ))
                    })
                    .when(preview, |content| {
                        content.child(
                            div()
                                .size_full()
                                .min_h_0()
                                .p(px(KNOWLEDGE_PREVIEW_PADDING))
                                .child(oxideterm_gpui_markdown::markdown_virtual_with_options(
                                    preview_id,
                                    &self.tokens,
                                    &self.draft,
                                    &options,
                                    &self.preview_scroll,
                                )),
                        )
                    }),
            )
    }
}

/// Registry and async selection state for the single Knowledge workspace tab.
#[derive(Default)]
pub(super) struct KnowledgeWorkspaceEntity {
    tab_id: Option<TabId>,
    selected_document_id: Option<String>,
    editor: Option<Entity<KnowledgeDocumentEditor>>,
    load_generation: u64,
    loading: bool,
    load_error: Option<String>,
    pending_document_id: Option<String>,
    switch_after_save: bool,
    pending_close: Option<(TabId, AnyWindowHandle)>,
    close_after_save: bool,
    pending_app_quit: bool,
    app_quit_after_save: bool,
    _editor_subscription: Option<Subscription>,
    navigator_snapshot: KnowledgeNavigatorSnapshot,
    navigator_refresh_generation: u64,
    navigator_refresh_running: bool,
    navigator_refresh_requested: bool,
    navigator_last_refresh: Option<Instant>,
    navigator_query: Arc<str>,
    navigator_search_editor: Option<Entity<TextEditorView>>,
    _navigator_search_subscription: Option<Subscription>,
}

impl KnowledgeWorkspaceEntity {
    fn tab_id(&self) -> Option<TabId> {
        self.tab_id
    }

    fn register_tab(&mut self, tab_id: TabId) {
        self.tab_id = Some(tab_id);
    }

    pub(super) fn close_tab(&mut self, tab_id: TabId) {
        if self.tab_id == Some(tab_id) {
            self.tab_id = None;
            self.selected_document_id = None;
            self.editor = None;
            self.loading = false;
            self.pending_document_id = None;
            self.switch_after_save = false;
            self.pending_close = None;
            self.close_after_save = false;
            self.pending_app_quit = false;
            self.app_quit_after_save = false;
            self._editor_subscription = None;
            self.load_generation = self.load_generation.wrapping_add(1);
            self.navigator_query = Arc::from("");
            self.navigator_search_editor = None;
            self._navigator_search_subscription = None;
        }
    }

    fn begin_document_load(&mut self, document_id: String) -> u64 {
        self.load_generation = self.load_generation.wrapping_add(1);
        // A clean editor must disappear before the asynchronous replacement starts. Leaving it
        // interactive would allow a new dirty draft to be created and then overwritten by the
        // pending load result.
        self.editor = None;
        self._editor_subscription = None;
        self.selected_document_id = Some(document_id);
        self.loading = true;
        self.load_error = None;
        self.load_generation
    }

    fn request_document_switch(&mut self, document_id: String) {
        self.pending_document_id = Some(document_id);
        self.switch_after_save = false;
    }

    fn cancel_document_switch(&mut self) {
        self.pending_document_id = None;
        self.switch_after_save = false;
    }

    fn take_pending_document_after_save(&mut self) -> Option<String> {
        if !self.switch_after_save {
            return None;
        }
        self.switch_after_save = false;
        self.pending_document_id.take()
    }

    fn request_close(&mut self, tab_id: TabId, window_handle: AnyWindowHandle) {
        self.pending_close = Some((tab_id, window_handle));
        self.close_after_save = false;
    }

    fn cancel_close(&mut self) {
        self.pending_close = None;
        self.close_after_save = false;
    }

    fn request_app_quit(&mut self) {
        self.pending_app_quit = true;
        self.app_quit_after_save = false;
    }

    fn cancel_app_quit(&mut self) {
        self.pending_app_quit = false;
        self.app_quit_after_save = false;
    }

    fn take_pending_close_after_save(&mut self) -> Option<(TabId, AnyWindowHandle)> {
        if !self.close_after_save {
            return None;
        }
        self.close_after_save = false;
        self.pending_close.take()
    }

    fn confirm_save_before_leaving(&mut self) {
        self.switch_after_save = self.pending_document_id.is_some();
        self.close_after_save = self.pending_close.is_some();
        self.app_quit_after_save = self.pending_app_quit;
    }

    fn take_pending_document_now(&mut self) -> Option<String> {
        self.switch_after_save = false;
        self.pending_document_id.take()
    }

    fn take_pending_close_now(&mut self) -> Option<(TabId, AnyWindowHandle)> {
        self.close_after_save = false;
        self.pending_close.take()
    }

    fn take_pending_app_quit_after_save(&mut self) -> bool {
        if !self.app_quit_after_save {
            return false;
        }
        self.app_quit_after_save = false;
        std::mem::take(&mut self.pending_app_quit)
    }

    fn take_pending_app_quit_now(&mut self) -> bool {
        self.app_quit_after_save = false;
        std::mem::take(&mut self.pending_app_quit)
    }

    fn install_document(
        &mut self,
        generation: u64,
        document_id: &str,
        editor: Entity<KnowledgeDocumentEditor>,
        subscription: Subscription,
    ) -> bool {
        if generation != self.load_generation
            || self.selected_document_id.as_deref() != Some(document_id)
        {
            return false;
        }
        self.editor = Some(editor);
        self._editor_subscription = Some(subscription);
        self.loading = false;
        self.load_error = None;
        true
    }

    fn install_load_error(&mut self, generation: u64, error: String) {
        if generation == self.load_generation {
            self.editor = None;
            self.loading = false;
            self.load_error = Some(error);
        }
    }

    fn begin_navigator_refresh(&mut self, force: bool) -> Option<u64> {
        if self.navigator_refresh_running {
            // A mutation can land while the previous snapshot is still loading. Retain one
            // follow-up request so the older result cannot leave the navigator permanently stale.
            self.navigator_refresh_requested |= force;
            return None;
        }
        if !force
            && self
                .navigator_last_refresh
                .is_some_and(|updated| updated.elapsed() < KNOWLEDGE_NAVIGATOR_REFRESH_INTERVAL)
        {
            return None;
        }
        self.navigator_refresh_generation = self.navigator_refresh_generation.wrapping_add(1);
        self.navigator_refresh_running = true;
        Some(self.navigator_refresh_generation)
    }

    fn install_navigator_snapshot(
        &mut self,
        generation: u64,
        snapshot: KnowledgeNavigatorSnapshot,
    ) -> bool {
        if generation != self.navigator_refresh_generation {
            return false;
        }
        self.navigator_snapshot = snapshot;
        self.navigator_refresh_running = false;
        self.navigator_last_refresh = Some(Instant::now());
        std::mem::take(&mut self.navigator_refresh_requested)
    }

    pub(in crate::workspace) fn insert_created_document(
        &mut self,
        document: oxideterm_ai::RagDocumentResponse,
    ) {
        if self.navigator_refresh_running {
            // The in-flight snapshot was sampled before this committed mutation. Invalidate its
            // generation so it cannot briefly erase the optimistic row before the forced refresh.
            self.navigator_refresh_generation = self.navigator_refresh_generation.wrapping_add(1);
            self.navigator_refresh_running = false;
            self.navigator_refresh_requested = false;
        }
        if self.navigator_snapshot.selected_collection_id.as_deref()
            != Some(document.collection_id.as_str())
        {
            return;
        }
        // Reflect the committed document immediately; the forced refresh below remains the source
        // of truth for collection counts and any mutations that landed concurrently.
        let mut documents = self.navigator_snapshot.documents.as_ref().clone();
        documents.retain(|existing| existing.id != document.id);
        documents.push(document);
        self.navigator_snapshot.documents = Arc::new(documents);
    }

    pub(super) fn remove_document(&mut self, document_id: &str) {
        if self.selected_document_id.as_deref() == Some(document_id) {
            self.selected_document_id = None;
            self.editor = None;
            self._editor_subscription = None;
            self.pending_document_id = None;
            self.load_generation = self.load_generation.wrapping_add(1);
            self.loading = false;
        }
    }

    pub(super) fn remove_collection(&mut self, collection_id: &str, cx: &App) {
        if self
            .editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).collection_id.as_str() == collection_id)
        {
            self.selected_document_id = None;
            self.editor = None;
            self._editor_subscription = None;
            self.pending_document_id = None;
            self.load_generation = self.load_generation.wrapping_add(1);
            self.loading = false;
        }
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn knowledge_text_editor_focused(
        &self,
        window: &Window,
        cx: &App,
    ) -> bool {
        let knowledge = self.knowledge_workspace.read(cx);
        if knowledge
            .navigator_search_editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).focus_handle(cx).is_focused(window))
        {
            return true;
        }
        let Some(document) = knowledge.editor.as_ref() else {
            return false;
        };
        let document = document.read(cx);
        document.editor.read(cx).focus_handle(cx).is_focused(window)
            || document
                .active_block
                .as_ref()
                .is_some_and(|active| active.editor.read(cx).focus_handle(cx).is_focused(window))
    }

    pub(in crate::workspace) fn handle_knowledge_workspace_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .active_tab(cx)
            .is_some_and(|tab| tab.kind == TabKind::Knowledge)
            || self.knowledge_text_editor_focused(window, cx)
        {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let command = event.keystroke.modifiers.platform || event.keystroke.modifiers.control;
        if command && key == "f" {
            let search = self.ensure_knowledge_navigator_search(cx);
            window.focus(&search.read(cx).focus_handle(cx), cx);
            return true;
        }
        let direction = match key {
            "up" | "arrowup" => -1,
            "down" | "arrowdown" => 1,
            _ => return false,
        };
        let (documents, query, selected_document_id) = {
            let knowledge = self.knowledge_workspace.read(cx);
            (
                knowledge.navigator_snapshot.documents.clone(),
                knowledge.navigator_query.clone(),
                knowledge.selected_document_id.clone(),
            )
        };
        let visible = documents
            .iter()
            .filter(|document| knowledge_document_matches(document, &query))
            .collect::<Vec<_>>();
        if visible.is_empty() {
            return true;
        }
        let current = selected_document_id
            .as_deref()
            .and_then(|selected| visible.iter().position(|document| document.id == selected));
        let next = match (current, direction) {
            (Some(index), -1) => index.checked_sub(1).unwrap_or(visible.len() - 1),
            (Some(index), _) => (index + 1) % visible.len(),
            (None, -1) => visible.len() - 1,
            (None, _) => 0,
        };
        self.select_knowledge_document(visible[next].id.clone(), cx);
        true
    }

    fn ensure_knowledge_navigator_search(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Entity<TextEditorView> {
        if let Some(editor) = self
            .knowledge_workspace
            .read(cx)
            .navigator_search_editor
            .clone()
        {
            return editor;
        }
        let placeholder = self
            .i18n
            .t("settings_view.knowledge.navigator_search_placeholder");
        let has_background_image = self.background_surface_active("knowledge");
        let editor = cx.new(|cx| {
            let mut editor = TextEditorView::new("", &self.tokens, cx);
            editor.set_presentation(EditorPresentation::Inline, cx);
            editor.set_transparent_background(has_background_image, cx);
            editor.set_placeholder(Some(placeholder), cx);
            editor
        });
        let weak_workspace = cx.entity().downgrade();
        editor.update(cx, |editor, _cx| {
            editor.set_on_save(Box::new(move |_query, _window, cx| {
                weak_workspace
                    .update(cx, |workspace, cx| {
                        if let Some(document_editor) =
                            workspace.knowledge_workspace.read(cx).editor.clone()
                        {
                            document_editor.update(cx, |editor, cx| editor.save_current_draft(cx));
                        }
                    })
                    .map_err(|_| "knowledge workspace is no longer open".to_string())?;
                Ok(())
            }));
        });
        let observed_editor = editor.clone();
        let subscription = cx.observe(&editor, move |workspace, editor, cx| {
            let query: Arc<str> = Arc::from(editor.read(cx).buffer().text().trim());
            let changed = workspace.knowledge_workspace.update(cx, |knowledge, _cx| {
                if knowledge.navigator_query == query {
                    return false;
                }
                knowledge.navigator_query = query;
                true
            });
            if changed {
                cx.notify();
            }
        });
        self.knowledge_workspace.update(cx, |knowledge, _cx| {
            knowledge.navigator_search_editor = Some(observed_editor);
            knowledge._navigator_search_subscription = Some(subscription);
        });
        editor
    }

    fn knowledge_navigator_search(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let editor = self.ensure_knowledge_navigator_search(cx);
        let editor_line_height = editor.read(cx).line_height();
        let has_background_image = self.background_surface_active("knowledge");
        editor.update(cx, |editor, cx| {
            editor.set_transparent_background(has_background_image, cx);
        });
        div()
            .h(px(
                KNOWLEDGE_NAVIGATOR_SEARCH_HEIGHT + KNOWLEDGE_NAVIGATOR_SEARCH_VERTICAL_PADDING
            ))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .child(Self::render_lucide_icon(
                LucideIcon::Search,
                KNOWLEDGE_NAVIGATOR_ACTION_ICON_SIZE,
                rgb(self.tokens.ui.text_muted),
            ))
            .child(
                div()
                    .h(px(KNOWLEDGE_NAVIGATOR_SEARCH_HEIGHT))
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .child(
                        div()
                            .w_full()
                            .h(px(editor_line_height))
                            .flex_none()
                            .child(editor),
                    ),
            )
            .into_any_element()
    }

    fn knowledge_navigator_action(
        &self,
        id: &'static str,
        icon: LucideIcon,
        tooltip: String,
        disabled: bool,
        loading: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tooltip_tokens = self.tokens;
        let tooltip_label = tooltip.clone();
        self.workspace_icon_action_button(
            icon,
            KNOWLEDGE_NAVIGATOR_ACTION_ICON_SIZE,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                disabled,
                loading,
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(
                    KNOWLEDGE_NAVIGATOR_ACTION_SIZE,
                    ButtonRadius::Sm,
                )
            },
            listener,
            cx,
        )
        .id(id)
        .tooltip(move |_window, cx| {
            oxideterm_gpui_ui::tooltip::tooltip_view(
                tooltip_tokens,
                tooltip_label.clone(),
                None,
                cx,
            )
        })
        .into_any_element()
    }

    fn knowledge_navigator_toolbar(
        &self,
        selected_collection_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let collection_id = selected_collection_id.map(str::to_string);
        let import_collection_id = collection_id.clone();
        let import_running = self
            .ai_entity
            .read(cx)
            .knowledge_import_progress()
            .is_some();
        div()
            .h(px(self.tokens.metrics.ui_button_lg_height))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .px(px(12.0))
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.knowledge.title")),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(self.knowledge_navigator_action(
                        "knowledge-nav-create-collection",
                        LucideIcon::FolderPlus,
                        self.i18n.t("settings_view.knowledge.create_collection"),
                        false,
                        false,
                        |this, _event, _window, cx| {
                            this.open_knowledge_create_dialog(cx);
                            this.reset_standard_confirm_focus();
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.knowledge_navigator_action(
                        "knowledge-nav-import",
                        LucideIcon::Upload,
                        self.i18n.t("settings_view.knowledge.import_files"),
                        import_collection_id.is_none(),
                        import_running,
                        move |this, _event, window, cx| {
                            if let Some(collection_id) = import_collection_id.clone() {
                                this.knowledge_import_files(collection_id, window, cx);
                            }
                            cx.stop_propagation();
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn knowledge_navigator_section_label(&self, key: &'static str, count: usize) -> AnyElement {
        // Compact navigators keep the count at the far edge so localized titles
        // retain the full flexible slot instead of competing with a count pill.
        div()
            .h(px(32.0))
            .w_full()
            .min_w_0()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(self.tokens.spacing.two))
            .px(px(self.tokens.spacing.three))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(self.i18n.t(key)),
            )
            .child(div().flex_none().child(count.to_string()))
            .into_any_element()
    }

    fn knowledge_navigator_documents_header(
        &self,
        collection_id: &str,
        document_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let create_collection_id = collection_id.to_string();
        let embedding_collection_id = collection_id.to_string();
        let reindex_collection_id = collection_id.to_string();
        let embedding_running = self
            .ai_entity
            .read(cx)
            .knowledge_embedding_progress()
            .is_some();
        let reindex_running = self
            .ai_entity
            .read(cx)
            .knowledge_reindex_progress()
            .is_some();
        div()
            .h(px(36.0))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .px(px(12.0))
            .border_t_1()
            .border_color(rgb(self.tokens.ui.border))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.knowledge.navigator_documents"))
                    .child(document_count.to_string()),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(self.knowledge_navigator_action(
                        "knowledge-nav-new-document",
                        LucideIcon::FilePlus,
                        self.i18n.t("settings_view.knowledge.new_document"),
                        false,
                        false,
                        move |this, _event, window, cx| {
                            this.open_knowledge_document_dialog(
                                create_collection_id.clone(),
                                true,
                                window,
                                cx,
                            );
                            this.reset_standard_confirm_focus();
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.knowledge_navigator_action(
                        "knowledge-nav-embeddings",
                        LucideIcon::Sparkles,
                        self.i18n.t("settings_view.knowledge.generate_embeddings"),
                        false,
                        embedding_running,
                        move |this, _event, _window, cx| {
                            this.knowledge_generate_embeddings(embedding_collection_id.clone(), cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.knowledge_navigator_action(
                        "knowledge-nav-reindex",
                        if reindex_running {
                            LucideIcon::X
                        } else {
                            LucideIcon::RefreshCw
                        },
                        self.i18n.t("settings_view.knowledge.reindex"),
                        false,
                        false,
                        move |this, _event, _window, cx| {
                            if reindex_running {
                                this.knowledge_cancel_reindex(cx);
                            } else {
                                this.knowledge_reindex(reindex_collection_id.clone(), cx);
                            }
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.knowledge_navigator_action(
                        "knowledge-nav-settings",
                        LucideIcon::Settings,
                        self.i18n.t("settings_view.knowledge.configure_embeddings"),
                        false,
                        false,
                        |this, _event, window, cx| {
                            this.open_knowledge_settings(window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn knowledge_navigator_collection_row(
        &self,
        collection: oxideterm_ai::RagCollectionResponse,
        selected_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = selected_id == Some(collection.id.as_str());
        let select_id = collection.id.clone();
        let delete_id = collection.id.clone();
        let delete_name = collection.name.clone();
        div()
            .id(format!("knowledge-nav-collection-{}", collection.id))
            .w_full()
            .h(px(KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT))
            .flex_none()
            .px(px(4.0))
            .child(
                entity_list_row(
                    &self.tokens,
                    EntityListRowOptions::new()
                        .active(selected)
                        .compact()
                        .has_background_image(self.background_surface_active("knowledge")),
                    Some(
                        div()
                            .flex_none()
                            .child(Self::render_lucide_icon(
                                LucideIcon::BookOpen,
                                KNOWLEDGE_NAVIGATOR_ROW_ICON_SIZE,
                                rgb(if selected {
                                    self.tokens.ui.accent
                                } else {
                                    self.tokens.ui.text_muted
                                }),
                            ))
                            .into_any_element(),
                    ),
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(if selected {
                            self.tokens.ui.text
                        } else {
                            self.tokens.ui.text_muted
                        }))
                        .child(collection.name)
                        .into_any_element(),
                    None,
                    Vec::new(),
                    vec![
                        self.knowledge_icon_button(
                            LucideIcon::Trash2,
                            rgb(self.tokens.ui.text_muted),
                            Some(rgb(self.tokens.ui.error)),
                            move |this, _event, _window, cx| {
                                this.ai_entity.update(cx, |entity, cx| {
                                    entity.request_delete_knowledge_collection(
                                        delete_id.clone(),
                                        delete_name.clone(),
                                    );
                                    cx.notify();
                                });
                                this.reset_standard_confirm_focus();
                                cx.stop_propagation();
                            },
                            cx,
                        )
                        .into_any_element(),
                    ],
                )
                .h(px(40.0))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.ai_entity.update(cx, |entity, cx| {
                            entity.select_knowledge_collection(select_id.clone());
                            cx.notify();
                        });
                        this.refresh_knowledge_navigator(true, cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element()
    }

    /// Reports whether the Knowledge draft confirmation currently owns window input.
    pub(in crate::workspace) fn knowledge_leave_confirmation_open(&self, cx: &App) -> bool {
        let knowledge = self.knowledge_workspace.read(cx);
        knowledge.pending_document_id.is_some()
            || knowledge.pending_close.is_some()
            || knowledge.pending_app_quit
    }

    /// Keeps keyboard input out of the editor while a dirty-draft decision is pending.
    pub(in crate::workspace) fn handle_knowledge_leave_confirmation_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match event.keystroke.key.as_str() {
            "escape" => {
                let pending_close = self.knowledge_workspace.read(cx).pending_close.is_some();
                let pending_app_quit = self.knowledge_workspace.read(cx).pending_app_quit;
                if pending_app_quit {
                    self.cancel_knowledge_app_quit(cx);
                } else if pending_close {
                    self.cancel_knowledge_tab_close(cx);
                } else {
                    self.cancel_knowledge_document_switch(cx);
                }
            }
            "enter" => self.save_before_leaving_knowledge_document(window, cx),
            _ => {}
        }
        true
    }

    pub(in crate::workspace) fn is_knowledge_document_selected(
        &self,
        document_id: &str,
        cx: &App,
    ) -> bool {
        self.knowledge_workspace
            .read(cx)
            .selected_document_id
            .as_deref()
            == Some(document_id)
    }

    fn knowledge_editor_labels(&self) -> KnowledgeEditorLabels {
        KnowledgeEditorLabels {
            source: self.i18n.t("settings_view.knowledge.editor_source"),
            instant: self.i18n.t("settings_view.knowledge.editor_instant"),
            preview: self.i18n.t("settings_view.knowledge.editor_preview"),
            save: self.i18n.t("settings_view.knowledge.editor_save"),
            saved: self.i18n.t("settings_view.knowledge.editor_saved"),
            saving: self.i18n.t("settings_view.knowledge.editor_saving"),
            dirty: self.i18n.t("settings_view.knowledge.editor_dirty"),
            conflict: self.i18n.t("settings_view.knowledge.editor_conflict"),
            save_failed: self.i18n.t("settings_view.knowledge.editor_save_failed"),
            load_failed: self.i18n.t("settings_view.knowledge.editor_load_failed"),
            navigator_load_failed: self.i18n.t("settings_view.knowledge.navigator_load_failed"),
            keyword_pending: self
                .i18n
                .t("settings_view.knowledge.editor_keyword_pending"),
            keyword_failed: self.i18n.t("settings_view.knowledge.editor_keyword_failed"),
            semantic_pending: self
                .i18n
                .t("settings_view.knowledge.editor_semantic_pending"),
            empty: self.i18n.t("settings_view.knowledge.editor_empty"),
            loading: self.i18n.t("settings_view.knowledge.editor_loading"),
            format_undo: self.i18n.t("settings_view.knowledge.format_undo"),
            format_redo: self.i18n.t("settings_view.knowledge.format_redo"),
            format_heading: self.i18n.t("settings_view.knowledge.format_heading"),
            format_bold: self.i18n.t("settings_view.knowledge.format_bold"),
            format_italic: self.i18n.t("settings_view.knowledge.format_italic"),
            format_strike: self.i18n.t("settings_view.knowledge.format_strike"),
            format_inline_code: self.i18n.t("settings_view.knowledge.format_inline_code"),
            format_code_block: self.i18n.t("settings_view.knowledge.format_code_block"),
            format_link: self.i18n.t("settings_view.knowledge.format_link"),
            format_image: self.i18n.t("settings_view.knowledge.format_image"),
            format_table: self.i18n.t("settings_view.knowledge.format_table"),
            format_horizontal_rule: self
                .i18n
                .t("settings_view.knowledge.format_horizontal_rule"),
            format_quote: self.i18n.t("settings_view.knowledge.format_quote"),
            format_bullet_list: self.i18n.t("settings_view.knowledge.format_bullet_list"),
            format_ordered_list: self.i18n.t("settings_view.knowledge.format_ordered_list"),
            format_task_list: self.i18n.t("settings_view.knowledge.format_task_list"),
            switch_title: self.i18n.t("settings_view.knowledge.switch_title"),
            switch_description: self.i18n.t("settings_view.knowledge.switch_description"),
            close_title: self.i18n.t("settings_view.knowledge.close_title"),
            close_description: self.i18n.t("settings_view.knowledge.close_description"),
            quit_title: self.i18n.t("settings_view.knowledge.quit_title"),
            quit_description: self.i18n.t("settings_view.knowledge.quit_description"),
            discard: self.i18n.t("settings_view.knowledge.discard"),
            cancel: self.i18n.t("common.actions.cancel"),
            reload: self.i18n.t("settings_view.knowledge.reload"),
            copy_draft: self.i18n.t("settings_view.knowledge.copy_draft"),
            copy: self.i18n.t("menu.copy"),
            cut: self.i18n.t("fileManager.cut"),
            paste: self.i18n.t("menu.paste"),
            select_all: self.i18n.t("fileManager.selectAll"),
        }
    }

    pub(in crate::workspace) fn refresh_knowledge_navigator(
        &mut self,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        let generation = self.knowledge_workspace.update(cx, |knowledge, _cx| {
            knowledge.begin_navigator_refresh(force)
        });
        let Some(generation) = generation else {
            return;
        };
        let store = self.ai_entity.read(cx).rag_store();
        let preferred_collection_id = self
            .ai_entity
            .read(cx)
            .knowledge_selected_collection_id()
            .map(str::to_string);
        cx.spawn(async move |workspace, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let collections = oxideterm_ai::rag_list_collections(&store, None)?;
                    let selected_collection_id = preferred_collection_id
                        .filter(|id| collections.iter().any(|collection| collection.id == *id))
                        .or_else(|| collections.first().map(|collection| collection.id.clone()));
                    let selected_collection = selected_collection_id
                        .as_deref()
                        .and_then(|id| collections.iter().find(|collection| collection.id == id))
                        .cloned();
                    let documents = selected_collection_id
                        .as_deref()
                        .map(|id| oxideterm_ai::rag_list_documents(&store, id, None, None))
                        .transpose()?
                        .map(|page| page.documents)
                        .unwrap_or_default();
                    Ok::<_, String>(KnowledgeNavigatorSnapshot {
                        collections: Arc::new(collections),
                        selected_collection_id,
                        selected_collection,
                        documents: Arc::new(documents),
                        error: None,
                        loaded: true,
                    })
                })
                .await;
            let _ = workspace.update(cx, |workspace, cx| {
                let snapshot = result.unwrap_or_else(|error| KnowledgeNavigatorSnapshot {
                    error: Some(error),
                    loaded: true,
                    ..KnowledgeNavigatorSnapshot::default()
                });
                let refresh_again = workspace.knowledge_workspace.update(cx, |knowledge, _cx| {
                    knowledge.install_navigator_snapshot(generation, snapshot)
                });
                if refresh_again {
                    workspace.refresh_knowledge_navigator(true, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Opens the single Knowledge workspace tab without changing the global context sidebar.
    pub(in crate::workspace) fn open_knowledge_workspace_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab_id) = self.knowledge_workspace.read(cx).tab_id()
            && self.tabs(cx).iter().any(|tab| tab.id == tab_id)
        {
            self.set_active_tab(tab_id, window, cx);
            self.refresh_knowledge_navigator(true, cx);
            return;
        }
        let tab_id = self.alloc_tab_id(cx);
        self.knowledge_workspace
            .update(cx, |workspace, _cx| workspace.register_tab(tab_id));
        self.insert_tab(
            Tab {
                id: tab_id,
                kind: TabKind::Knowledge,
                title: self.i18n.t("sidebar.panels.knowledge"),
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            },
            cx,
        );
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        self.reveal_active_tab(window, cx);
        self.refresh_knowledge_navigator(true, cx);
        cx.notify();
    }

    /// Returns true when a dirty Knowledge draft takes ownership of the user close request.
    pub(in crate::workspace) fn guard_dirty_knowledge_tab_close(
        &mut self,
        tab_id: TabId,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let editor = self.knowledge_workspace.read(cx).editor.clone();
        if !editor.is_some_and(|editor| editor.read(cx).is_dirty()) {
            return false;
        }
        self.knowledge_workspace.update(cx, |knowledge, _cx| {
            knowledge.request_close(tab_id, window.window_handle().into());
        });
        cx.notify();
        true
    }

    /// Returns true when a dirty Knowledge draft takes ownership of the application quit action.
    pub(crate) fn guard_dirty_knowledge_app_quit(&mut self, cx: &mut Context<Self>) -> bool {
        let editor = self.knowledge_workspace.read(cx).editor.clone();
        if !editor.is_some_and(|editor| editor.read(cx).is_dirty()) {
            return false;
        }
        self.knowledge_workspace.update(cx, |knowledge, _cx| {
            knowledge.request_app_quit();
        });
        let knowledge_tab_id = self.knowledge_workspace.read(cx).tab_id();
        if let Some(knowledge_tab_id) = knowledge_tab_id
            && !self.focus_detached_tab_window(knowledge_tab_id, cx)
        {
            // The confirmation is owned by the Knowledge surface. Bring that surface into the
            // main window before blocking the global quit action so the decision is always visible.
            self.set_main_window_active_tab(Some(knowledge_tab_id), cx);
            self.sync_active_tab_surface(cx);
            // Tray quit can arrive while the main native window is hidden. Activating a hidden
            // AppKit or Win32 window does not make its confirmation visible.
            oxideterm_desktop_presence::show_main_window();
            if let Some(handle) = self
                .window_registry
                .handle_for_role(window_registry::WindowRole::Main)
            {
                let _ = handle.update(cx, |_root, window, _cx| window.activate_window());
            }
        }
        cx.notify();
        true
    }

    /// Protects the final detached Knowledge surface when no main window can receive its draft.
    pub(in crate::workspace) fn guard_last_detached_knowledge_window_close(
        &mut self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .window_registry
            .is_only_window_with_role(window_registry::WindowRole::Detached { tab_id })
        {
            return false;
        }
        self.guard_dirty_knowledge_app_quit(cx)
    }

    /// Applies the dirty-draft guard for quit intents that do not originate from a focused window.
    pub(in crate::workspace) fn request_application_quit(&mut self, cx: &mut Context<Self>) {
        if self.guard_dirty_knowledge_app_quit(cx) {
            return;
        }
        oxideterm_desktop_presence::request_quit();
        cx.quit();
    }

    /// Loads a document for the editor pane while keeping the Knowledge tab itself stable.
    pub(in crate::workspace) fn select_knowledge_document(
        &mut self,
        document_id: String,
        cx: &mut Context<Self>,
    ) {
        let (selected_document_id, current_editor, loading) = {
            let knowledge = self.knowledge_workspace.read(cx);
            (
                knowledge.selected_document_id.clone(),
                knowledge.editor.clone(),
                knowledge.loading,
            )
        };
        if selected_document_id.as_deref() == Some(document_id.as_str())
            && (current_editor.is_some() || loading)
        {
            return;
        }
        if current_editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).is_dirty())
        {
            self.knowledge_workspace.update(cx, |knowledge, _cx| {
                knowledge.request_document_switch(document_id);
            });
            cx.notify();
            return;
        }
        let generation = self.knowledge_workspace.update(cx, |workspace, _cx| {
            workspace.begin_document_load(document_id.clone())
        });
        let store = self.ai_entity.read(cx).rag_store();
        let labels = self.knowledge_editor_labels();
        let tokens = self.tokens;
        let has_background_image = self.background_surface_active("knowledge");
        cx.spawn(async move |workspace, cx| {
            let load_document_id = document_id.clone();
            let load_store = store.clone();
            let result = cx
                .background_executor()
                .spawn(
                    async move { oxideterm_ai::rag_get_document(&load_store, &load_document_id) },
                )
                .await;
            let _ = workspace.update(cx, |workspace, cx| {
                match result {
                    Ok(loaded) => {
                        let editor = cx.new(|cx| {
                            KnowledgeDocumentEditor::new(
                                loaded,
                                store,
                                tokens,
                                labels,
                                has_background_image,
                                cx,
                            )
                        });
                        KnowledgeDocumentEditor::configure_save_callback(&editor, cx);
                        editor.update(cx, |editor, cx| editor.start_index_state_poll(cx));
                        let subscription = cx.subscribe(
                            &editor,
                            |workspace, _editor, _event: &KnowledgeDocumentEditorEvent, cx| {
                                let pending =
                                    workspace.knowledge_workspace.update(cx, |knowledge, _cx| {
                                        knowledge.take_pending_document_after_save()
                                    });
                                if let Some(document_id) = pending {
                                    workspace.select_knowledge_document(document_id, cx);
                                }
                                let pending_close =
                                    workspace.knowledge_workspace.update(cx, |knowledge, _cx| {
                                        knowledge.take_pending_close_after_save()
                                    });
                                if let Some((tab_id, window_handle)) = pending_close {
                                    cx.spawn(async move |weak, cx| {
                                        let _ =
                                            cx.update_window(window_handle, |_root, window, cx| {
                                                weak.update(cx, |workspace, cx| {
                                                    workspace.close_tab_by_id(tab_id, window, cx);
                                                })
                                            });
                                    })
                                    .detach();
                                }
                                let quit_after_save =
                                    workspace.knowledge_workspace.update(cx, |knowledge, _cx| {
                                        knowledge.take_pending_app_quit_after_save()
                                    });
                                if quit_after_save {
                                    oxideterm_desktop_presence::request_quit();
                                    cx.quit();
                                }
                            },
                        );
                        workspace.knowledge_workspace.update(cx, |knowledge, _cx| {
                            knowledge.install_document(
                                generation,
                                &document_id,
                                editor,
                                subscription,
                            );
                        });
                    }
                    Err(error) => {
                        workspace.knowledge_workspace.update(cx, |knowledge, _cx| {
                            knowledge.install_load_error(generation, error.to_string());
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn cancel_knowledge_document_switch(&mut self, cx: &mut Context<Self>) {
        self.knowledge_workspace
            .update(cx, |knowledge, _cx| knowledge.cancel_document_switch());
        cx.notify();
    }

    fn discard_and_switch_knowledge_document(&mut self, cx: &mut Context<Self>) {
        let pending = self
            .knowledge_workspace
            .update(cx, |knowledge, _cx| knowledge.take_pending_document_now());
        if let Some(document_id) = pending {
            // Removing the current editor drops its draft before the selected identifier changes.
            self.knowledge_workspace.update(cx, |knowledge, _cx| {
                knowledge.editor = None;
                knowledge._editor_subscription = None;
                knowledge.selected_document_id = None;
            });
            self.select_knowledge_document(document_id, cx);
        }
    }

    fn save_before_leaving_knowledge_document(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = self.knowledge_workspace.read(cx).editor.clone();
        let is_dirty = editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).is_dirty());
        if !is_dirty {
            let pending_document = self
                .knowledge_workspace
                .update(cx, |knowledge, _cx| knowledge.take_pending_document_now());
            if let Some(document_id) = pending_document {
                self.select_knowledge_document(document_id, cx);
                return;
            }
            let pending_close = self
                .knowledge_workspace
                .update(cx, |knowledge, _cx| knowledge.take_pending_close_now());
            if let Some((tab_id, _window_handle)) = pending_close {
                self.close_tab_by_id(tab_id, window, cx);
                return;
            }
            let quit = self
                .knowledge_workspace
                .update(cx, |knowledge, _cx| knowledge.take_pending_app_quit_now());
            if quit {
                oxideterm_desktop_presence::request_quit();
                cx.quit();
            }
            return;
        }
        self.knowledge_workspace.update(cx, |knowledge, _cx| {
            knowledge.confirm_save_before_leaving();
        });
        if let Some(editor) = editor {
            editor.update(cx, |editor, cx| editor.save_current_draft(cx));
        }
    }

    fn cancel_knowledge_tab_close(&mut self, cx: &mut Context<Self>) {
        self.knowledge_workspace
            .update(cx, |knowledge, _cx| knowledge.cancel_close());
        cx.notify();
    }

    fn cancel_knowledge_app_quit(&mut self, cx: &mut Context<Self>) {
        self.knowledge_workspace
            .update(cx, |knowledge, _cx| knowledge.cancel_app_quit());
        cx.notify();
    }

    fn discard_and_close_knowledge_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pending = self
            .knowledge_workspace
            .update(cx, |knowledge, _cx| knowledge.take_pending_close_now());
        if let Some((tab_id, _window_handle)) = pending {
            self.knowledge_workspace.update(cx, |knowledge, _cx| {
                knowledge.editor = None;
                knowledge._editor_subscription = None;
            });
            self.close_tab_by_id(tab_id, window, cx);
        }
    }

    fn discard_and_quit_with_knowledge_draft(&mut self, cx: &mut Context<Self>) {
        let quit = self
            .knowledge_workspace
            .update(cx, |knowledge, _cx| knowledge.take_pending_app_quit_now());
        if quit {
            oxideterm_desktop_presence::request_quit();
            cx.quit();
        }
    }

    pub(in crate::workspace) fn render_knowledge_workspace_surface(
        &mut self,
        layout: KnowledgeWorkspaceLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport_width = f32::from(window.viewport_size().width);
        let available_width = match layout {
            KnowledgeWorkspaceLayout::MainWindow => knowledge_workspace_available_width(
                viewport_width,
                self.settings_store.settings().sidebar_ui.zen_mode,
                self.tokens.metrics.activity_bar_width,
                self.sidebar_collapsed,
                self.sidebar_panel_width(),
                self.context_sidebar_visible(),
                self.ai_entity.read(cx).chat_ui().sidebar_width,
            ),
            // A detached tab owns the whole native window and has no activity or context sidebars.
            KnowledgeWorkspaceLayout::DetachedWindow => viewport_width,
        };
        let narrow_layout = available_width < KNOWLEDGE_NARROW_VIEWPORT_WIDTH;
        let has_background_image = self.background_surface_active("knowledge");
        if let Some(editor) = self.knowledge_workspace.read(cx).editor.clone() {
            editor.update(cx, |editor, cx| {
                editor.set_has_background_image(has_background_image, cx);
            });
        }
        let labels = self.knowledge_editor_labels();
        self.refresh_knowledge_navigator(false, cx);
        let navigator_snapshot = self.knowledge_workspace.read(cx).navigator_snapshot.clone();
        let collections = navigator_snapshot.collections;
        let selected_collection_id = navigator_snapshot.selected_collection_id;
        let selected_collection = navigator_snapshot.selected_collection;
        let documents = navigator_snapshot.documents;
        let navigator_query = self.knowledge_workspace.read(cx).navigator_query.clone();
        let filtered_documents: Arc<Vec<_>> = if navigator_query.is_empty() {
            documents.clone()
        } else {
            Arc::new(
                documents
                    .iter()
                    .filter(|document| knowledge_document_matches(document, &navigator_query))
                    .cloned()
                    .collect(),
            )
        };
        let navigator_failed = self.ai_entity.read(cx).knowledge_error().is_some()
            || navigator_snapshot.error.is_some();
        let navigator_error = if navigator_failed {
            Some(labels.navigator_load_failed.clone())
        } else {
            (!navigator_snapshot.loaded).then(|| labels.loading.clone())
        };
        let (editor, loading, error, pending_switch, pending_close, pending_app_quit) = {
            let knowledge = self.knowledge_workspace.read(cx);
            (
                knowledge.editor.clone(),
                knowledge.loading,
                knowledge.load_error.clone(),
                knowledge.pending_document_id.is_some(),
                knowledge.pending_close.is_some(),
                knowledge.pending_app_quit,
            )
        };
        if let Some(editor) = editor.as_ref() {
            editor.update(cx, |editor, _cx| editor.set_compact_layout(narrow_layout));
        }
        let has_editor = editor.is_some();
        let collection_row_count = collections.len().max(1);
        if self.knowledge_workspace_collection_list_state.item_count() != collection_row_count {
            self.knowledge_workspace_collection_list_state
                .reset(collection_row_count);
        }
        let document_row_count = filtered_documents.len();
        if self.knowledge_workspace_list_state.item_count() != document_row_count {
            self.knowledge_workspace_list_state
                .reset(document_row_count);
        }
        let navigator_toolbar =
            self.knowledge_navigator_toolbar(selected_collection_id.as_deref(), cx);
        let navigator_search = self.knowledge_navigator_search(cx);
        let spec = TauriVirtualListSpec::new(
            px(KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT),
            KNOWLEDGE_WORKSPACE_SECTION_OVERSCAN,
        );
        let collection_list_state = self.knowledge_workspace_collection_list_state.clone();
        let collections_for_list = collections.clone();
        let selected_collection_for_list = selected_collection_id.clone();
        let workspace_for_collections = cx.entity();
        let collection_body = if collections.is_empty() {
            div()
                .w_full()
                .h(px(KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT * 2.0))
                .flex()
                .items_center()
                .justify_center()
                .child(self.knowledge_empty_row(
                    LucideIcon::BookOpen,
                    self.i18n.t("settings_view.knowledge.no_collections"),
                    cx,
                ))
                .into_any_element()
        } else {
            let visible_collection_rows = collections
                .len()
                .min(KNOWLEDGE_NAVIGATOR_COLLECTION_MAX_VISIBLE_ROWS);
            div()
                .w_full()
                .h(px(
                    KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT * visible_collection_rows as f32
                ))
                .min_h_0()
                .overflow_hidden()
                .child(tauri_virtual_list(
                    collection_list_state,
                    spec,
                    move |index, _window, app| {
                        workspace_for_collections.update(app, |workspace, cx| {
                            collections_for_list
                                .get(index)
                                .cloned()
                                .map(|collection| {
                                    workspace.knowledge_navigator_collection_row(
                                        collection,
                                        selected_collection_for_list.as_deref(),
                                        cx,
                                    )
                                })
                                .unwrap_or_else(|| div().w_full().into_any_element())
                        })
                    },
                ))
                .into_any_element()
        };
        let documents_section = if let Some(collection) = selected_collection.as_ref() {
            let documents_header = self.knowledge_navigator_documents_header(
                &collection.id,
                filtered_documents.len(),
                cx,
            );
            let document_body = if filtered_documents.is_empty() {
                let empty_key = if navigator_query.is_empty() {
                    "settings_view.knowledge.no_documents"
                } else {
                    "settings_view.knowledge.no_matching_documents"
                };
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(self.knowledge_empty_row(
                        LucideIcon::FileText,
                        self.i18n.t(empty_key),
                        cx,
                    ))
                    .into_any_element()
            } else {
                let document_list_state = self.knowledge_workspace_list_state.clone();
                let documents_for_list = filtered_documents.clone();
                let workspace_for_documents = cx.entity();
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        document_list_state,
                        spec,
                        move |index, _window, app| {
                            workspace_for_documents.update(app, |workspace, cx| {
                                documents_for_list
                                    .get(index)
                                    .cloned()
                                    .map(|document| {
                                        workspace.knowledge_document_row(document, true, cx)
                                    })
                                    .unwrap_or_else(|| div().w_full().into_any_element())
                            })
                        },
                    ))
                    .into_any_element()
            };
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .border_t_1()
                .border_color(rgb(self.tokens.ui.border))
                .child(documents_header)
                .child(document_body)
                .into_any_element()
        } else {
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .child(self.knowledge_empty_row(
                    LucideIcon::FileText,
                    self.i18n.t("settings_view.knowledge.no_collections"),
                    cx,
                ))
                .into_any_element()
        };
        let navigator = div()
            .min_h_0()
            .flex_none()
            .flex()
            .flex_col()
            .overflow_hidden()
            .when(narrow_layout, |navigator| {
                navigator
                    .w_full()
                    .h(relative(KNOWLEDGE_NARROW_NAVIGATOR_HEIGHT_RATIO))
                    .border_b_1()
            })
            .when(!narrow_layout, |navigator| {
                navigator
                    .w(px(self.tokens.metrics.sidebar_default_width))
                    .h_full()
                    .border_r_1()
            })
            .border_color(rgb(self.tokens.ui.border))
            .bg(color_for_background(
                self.tokens.ui.bg_secondary,
                has_background_image,
                KNOWLEDGE_BACKGROUND_SURFACE_ALPHA,
            ))
            .child(navigator_toolbar)
            .child(navigator_search)
            .when_some(navigator_error, |navigator, error| {
                navigator.child(self.knowledge_error_row(&error))
            })
            .child(self.knowledge_navigator_section_label(
                "settings_view.knowledge.collections",
                collections.len(),
            ))
            .child(collection_body)
            .child(documents_section);
        let editor_pane = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .when_some(editor, |pane, editor| pane.child(editor))
            .when(!has_editor, |pane| {
                let message = if error.is_some() {
                    labels.load_failed.clone()
                } else if loading {
                    labels.loading.clone()
                } else {
                    labels.empty.clone()
                };
                pane.flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(message)
            });
        let leave_labels = labels.clone();
        let leave_confirmation = (pending_switch || pending_close || pending_app_quit).then(|| {
            let title = if pending_app_quit {
                leave_labels.quit_title.clone()
            } else if pending_close {
                leave_labels.close_title.clone()
            } else {
                leave_labels.switch_title.clone()
            };
            let description = if pending_app_quit {
                leave_labels.quit_description.clone()
            } else if pending_close {
                leave_labels.close_description.clone()
            } else {
                leave_labels.switch_description.clone()
            };
            let dialog = oxideterm_gpui_ui::modal_container(&self.tokens)
                .w(px(440.0))
                .max_w(relative(0.92))
                .shadow(oxideterm_gpui_ui::theme_overlay_shadow(&self.tokens))
                .flex()
                .flex_col()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(oxideterm_gpui_ui::modal_header(
                    &self.tokens,
                    title,
                    description,
                ))
                .child(
                    oxideterm_gpui_ui::modal_footer(&self.tokens)
                        .child(self.knowledge_switch_dialog_button(
                            "knowledge-switch-cancel",
                            leave_labels.cancel.clone(),
                            false,
                            cx.listener(move |this, _event, _window, cx| {
                                if pending_app_quit {
                                    this.cancel_knowledge_app_quit(cx);
                                } else if pending_close {
                                    this.cancel_knowledge_tab_close(cx);
                                } else {
                                    this.cancel_knowledge_document_switch(cx);
                                }
                                cx.stop_propagation();
                            }),
                        ))
                        .child(self.knowledge_switch_dialog_button(
                            "knowledge-switch-discard",
                            leave_labels.discard.clone(),
                            false,
                            cx.listener(move |this, _event, window, cx| {
                                if pending_app_quit {
                                    this.discard_and_quit_with_knowledge_draft(cx);
                                } else if pending_close {
                                    this.discard_and_close_knowledge_tab(window, cx);
                                } else {
                                    this.discard_and_switch_knowledge_document(cx);
                                }
                                cx.stop_propagation();
                            }),
                        ))
                        .child(self.knowledge_switch_dialog_button(
                            "knowledge-switch-save",
                            leave_labels.save.clone(),
                            true,
                            cx.listener(|this, _event, window, cx| {
                                this.save_before_leaving_knowledge_document(window, cx);
                                cx.stop_propagation();
                            }),
                        )),
                );
            oxideterm_gpui_ui::modal_overlay(&self.tokens, dialog)
        });
        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .when(narrow_layout, |workspace| workspace.flex_col())
            .when(!narrow_layout, |workspace| workspace.flex_row())
            .relative()
            .overflow_hidden()
            .child(navigator)
            .child(editor_pane)
            .when_some(leave_confirmation, |workspace, dialog| {
                workspace.child(dialog)
            })
            .into_any_element()
    }

    fn knowledge_switch_dialog_button(
        &self,
        id: &'static str,
        label: String,
        primary: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        oxideterm_gpui_ui::toolbar_button(
            &self.tokens,
            label,
            None,
            ToolbarButtonOptions::compact_text(
                if primary {
                    ButtonVariant::Default
                } else {
                    ButtonVariant::Outline
                },
                ButtonRadius::Sm,
                30.0,
                12.0,
                self.tokens.metrics.ui_text_sm,
            ),
        )
        .id(id)
        .on_mouse_down(MouseButton::Left, listener)
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_toolbar_action_uses_double_asterisk_markers() {
        assert_eq!(
            knowledge_format_wrap(KnowledgeFormatAction::Bold),
            Some(("**", "**"))
        );
        assert_eq!(
            knowledge_format_wrap(KnowledgeFormatAction::Italic),
            Some(("*", "*"))
        );
    }

    fn navigator_document(
        title: &str,
        format: &str,
        source_path: Option<&str>,
    ) -> oxideterm_ai::RagDocumentResponse {
        oxideterm_ai::RagDocumentResponse {
            id: "doc".to_string(),
            collection_id: "collection".to_string(),
            title: title.to_string(),
            source_path: source_path.map(str::to_string),
            format: format.to_string(),
            chunk_count: 1,
            indexed_at: 0,
            version: 0,
        }
    }

    #[test]
    fn empty_document_keeps_one_editable_instant_block() {
        let blocks = editable_source_blocks("");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].range, 0..0);
    }

    #[test]
    fn whitespace_document_keeps_all_source_in_editable_block() {
        let source = "  \n\n";
        let blocks = editable_source_blocks(source);
        assert_eq!(blocks.len(), 1);
        assert_eq!(&source[blocks[0].range.clone()], source);
    }

    #[test]
    fn navigator_filter_matches_all_terms_across_document_metadata() {
        let document = navigator_document(
            "Production Runbook",
            "markdown",
            Some("/docs/operations/server.md"),
        );

        assert!(knowledge_document_matches(&document, "production markdown"));
        assert!(knowledge_document_matches(&document, "OPERATIONS server"));
        assert!(!knowledge_document_matches(
            &document,
            "production database"
        ));
        assert!(knowledge_document_matches(&document, "   "));
    }

    #[test]
    fn responsive_width_uses_center_workspace_after_sidebars() {
        let available =
            knowledge_workspace_available_width(1_200.0, false, 48.0, false, 260.0, true, 320.0);
        assert_eq!(available, 572.0);

        let zen_available =
            knowledge_workspace_available_width(1_200.0, true, 48.0, false, 260.0, true, 320.0);
        assert_eq!(zen_available, 1_200.0);
    }

    #[test]
    fn conflict_state_blocks_automatic_retry_until_user_resolves_it() {
        assert!(!knowledge_save_state_allows_autosave(
            &KnowledgeDocumentSaveState::Conflict
        ));
        assert!(!knowledge_save_state_allows_autosave(
            &KnowledgeDocumentSaveState::Saving
        ));
        assert!(knowledge_save_state_allows_autosave(
            &KnowledgeDocumentSaveState::Dirty
        ));
    }

    #[test]
    fn autosave_completion_does_not_accept_pending_switch_without_user_confirmation() {
        let mut workspace = KnowledgeWorkspaceEntity::default();
        workspace.request_document_switch("next".to_string());

        assert_eq!(workspace.take_pending_document_after_save(), None);
        assert_eq!(workspace.pending_document_id.as_deref(), Some("next"));

        workspace.confirm_save_before_leaving();
        assert_eq!(
            workspace.take_pending_document_after_save().as_deref(),
            Some("next")
        );
    }

    #[test]
    fn autosave_completion_does_not_quit_without_user_confirmation() {
        let mut workspace = KnowledgeWorkspaceEntity::default();
        workspace.request_app_quit();

        assert!(!workspace.take_pending_app_quit_after_save());
        assert!(workspace.pending_app_quit);

        workspace.confirm_save_before_leaving();
        assert!(workspace.take_pending_app_quit_after_save());
        assert!(!workspace.pending_app_quit);
    }

    #[test]
    fn closing_unrelated_tab_keeps_knowledge_workspace_registered() {
        let mut workspace = KnowledgeWorkspaceEntity::default();
        workspace.register_tab(TabId(7));
        workspace.close_tab(TabId(8));
        assert_eq!(workspace.tab_id(), Some(TabId(7)));
    }

    #[test]
    fn closing_knowledge_tab_invalidates_pending_document_load() {
        let mut workspace = KnowledgeWorkspaceEntity::default();
        workspace.register_tab(TabId(7));
        let generation = workspace.begin_document_load("doc".to_string());
        workspace.close_tab(TabId(7));
        assert_ne!(workspace.load_generation, generation);
        assert_eq!(workspace.tab_id(), None);
        assert_eq!(workspace.selected_document_id, None);
    }

    #[test]
    fn forced_navigator_refresh_during_load_schedules_one_follow_up() {
        let mut workspace = KnowledgeWorkspaceEntity::default();
        let generation = workspace.begin_navigator_refresh(true).unwrap();

        assert_eq!(workspace.begin_navigator_refresh(true), None);
        assert!(
            workspace
                .install_navigator_snapshot(generation, KnowledgeNavigatorSnapshot::default(),)
        );
        assert!(workspace.begin_navigator_refresh(true).is_some());
    }

    #[test]
    fn created_document_is_immediately_inserted_into_selected_collection_snapshot() {
        let mut existing = navigator_document("Existing", "markdown", None);
        existing.id = "existing".to_string();
        let mut created = navigator_document("Created", "markdown", None);
        created.id = "created".to_string();
        let mut workspace = KnowledgeWorkspaceEntity::default();
        workspace.navigator_snapshot.selected_collection_id = Some("collection".to_string());
        workspace.navigator_snapshot.documents = Arc::new(vec![existing]);

        workspace.insert_created_document(created.clone());
        workspace.insert_created_document(created);

        let document_ids = workspace
            .navigator_snapshot
            .documents
            .iter()
            .map(|document| document.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(document_ids, vec!["existing", "created"]);
    }

    #[test]
    fn stale_navigator_refresh_cannot_overwrite_a_created_document() {
        let mut created = navigator_document("Created", "markdown", None);
        created.id = "created".to_string();
        let mut workspace = KnowledgeWorkspaceEntity::default();
        workspace.navigator_snapshot.selected_collection_id = Some("collection".to_string());
        let stale_generation = workspace.begin_navigator_refresh(true).unwrap();

        workspace.insert_created_document(created);
        let current_generation = workspace.begin_navigator_refresh(true).unwrap();

        assert!(
            !workspace.install_navigator_snapshot(
                stale_generation,
                KnowledgeNavigatorSnapshot::default(),
            )
        );
        assert_eq!(
            workspace.navigator_snapshot.documents[0].id.as_str(),
            "created"
        );
        assert_ne!(stale_generation, current_generation);
    }
}
