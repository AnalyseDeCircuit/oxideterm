// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(in crate::workspace) mod navigation;
mod operations;
mod preview;

use gpui::EventEmitter;
use oxideterm_editor_syntax::LanguageId;
use oxideterm_gpui_editor::{EditorContextMenuLabels, TextEditorView};
use oxideterm_gpui_ui::{
    IconButtonOptions, SegmentedControlOptions, ToolbarButtonOptions,
    button::{ButtonRadius, ButtonVariant},
    color_for_background,
};

pub(in crate::workspace) const KNOWLEDGE_WORKSPACE_SECTION_COUNT: usize = 4;
pub(in crate::workspace) const KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT: f32 = 28.0;
pub(in crate::workspace) const KNOWLEDGE_WORKSPACE_SECTION_OVERSCAN: usize = 8;
const KNOWLEDGE_NAVIGATOR_ACTION_SIZE: f32 = 28.0;
const KNOWLEDGE_NAVIGATOR_ACTION_ICON_SIZE: f32 = 14.0;
const KNOWLEDGE_NAVIGATOR_DEFAULT_WIDTH: f32 = 200.0;
const KNOWLEDGE_NARROW_VIEWPORT_WIDTH: f32 = 720.0;
const KNOWLEDGE_EDITOR_MODE_SWITCHER_WIDTH: f32 = 176.0;
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

fn knowledge_document_matches(title: &str, content: &str, terms: &[String]) -> bool {
    let searchable = format!("{title}\n{content}").to_lowercase();
    terms.iter().all(|term| searchable.contains(term))
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

use oxideterm_settings::KnowledgeEditorMode;

#[derive(Clone, Copy)]
enum KnowledgeFormatAction {
    Undo,
    Redo,
    Heading(u8),
    Bold,
    Italic,
    Strike,
    InlineCode,
    InlineMath,
    DisplayMath,
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
        KnowledgeFormatAction::InlineMath => Some(("$", "$")),
        KnowledgeFormatAction::DisplayMath => Some(("\n$$\n", "\n$$\n")),
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
    preview: String,
    split: String,
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
    index_ready: String,
    empty: String,
    loading: String,
    format_undo: String,
    format_redo: String,
    format_heading: String,
    format_bold: String,
    format_italic: String,
    format_strike: String,
    format_inline_code: String,
    format_inline_math: String,
    format_display_math: String,
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

impl KnowledgeEditorLabels {
    fn new(i18n: &oxideterm_i18n::I18n) -> Self {
        KnowledgeEditorLabels {
            source: i18n.t("settings_view.knowledge.editor_source"),
            preview: i18n.t("settings_view.knowledge.editor_preview"),
            split: i18n.t("settings_view.knowledge.editor_split"),
            save: i18n.t("settings_view.knowledge.editor_save"),
            saved: i18n.t("settings_view.knowledge.editor_saved"),
            saving: i18n.t("settings_view.knowledge.editor_saving"),
            dirty: i18n.t("settings_view.knowledge.editor_dirty"),
            conflict: i18n.t("settings_view.knowledge.editor_conflict"),
            save_failed: i18n.t("settings_view.knowledge.editor_save_failed"),
            load_failed: i18n.t("settings_view.knowledge.editor_load_failed"),
            navigator_load_failed: i18n.t("settings_view.knowledge.navigator_load_failed"),
            keyword_pending: i18n.t("settings_view.knowledge.editor_keyword_pending"),
            keyword_failed: i18n.t("settings_view.knowledge.editor_keyword_failed"),
            semantic_pending: i18n.t("settings_view.knowledge.editor_semantic_pending"),
            index_ready: i18n.t("settings_view.knowledge.index_ready"),
            empty: i18n.t("settings_view.knowledge.editor_empty"),
            loading: i18n.t("settings_view.knowledge.editor_loading"),
            format_undo: i18n.t("settings_view.knowledge.format_undo"),
            format_redo: i18n.t("settings_view.knowledge.format_redo"),
            format_heading: i18n.t("settings_view.knowledge.format_heading"),
            format_bold: i18n.t("settings_view.knowledge.format_bold"),
            format_italic: i18n.t("settings_view.knowledge.format_italic"),
            format_strike: i18n.t("settings_view.knowledge.format_strike"),
            format_inline_code: i18n.t("settings_view.knowledge.format_inline_code"),
            format_inline_math: i18n.t("settings_view.knowledge.format_inline_math"),
            format_display_math: i18n.t("settings_view.knowledge.format_display_math"),
            format_code_block: i18n.t("settings_view.knowledge.format_code_block"),
            format_link: i18n.t("settings_view.knowledge.format_link"),
            format_image: i18n.t("settings_view.knowledge.format_image"),
            format_table: i18n.t("settings_view.knowledge.format_table"),
            format_horizontal_rule: i18n.t("settings_view.knowledge.format_horizontal_rule"),
            format_quote: i18n.t("settings_view.knowledge.format_quote"),
            format_bullet_list: i18n.t("settings_view.knowledge.format_bullet_list"),
            format_ordered_list: i18n.t("settings_view.knowledge.format_ordered_list"),
            format_task_list: i18n.t("settings_view.knowledge.format_task_list"),
            switch_title: i18n.t("settings_view.knowledge.switch_title"),
            switch_description: i18n.t("settings_view.knowledge.switch_description"),
            close_title: i18n.t("settings_view.knowledge.close_title"),
            close_description: i18n.t("settings_view.knowledge.close_description"),
            quit_title: i18n.t("settings_view.knowledge.quit_title"),
            quit_description: i18n.t("settings_view.knowledge.quit_description"),
            discard: i18n.t("settings_view.knowledge.discard"),
            cancel: i18n.t("common.actions.cancel"),
            reload: i18n.t("settings_view.knowledge.reload"),
            copy_draft: i18n.t("settings_view.knowledge.copy_draft"),
            copy: i18n.t("menu.copy"),
            cut: i18n.t("fileManager.cut"),
            paste: i18n.t("menu.paste"),
            select_all: i18n.t("fileManager.selectAll"),
        }
    }
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
    PreferencesChanged(oxideterm_settings::KnowledgeEditorUiState),
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
    previous_mode: KnowledgeEditorMode,
    mode_transition: Option<Task<()>>,
    save_state: KnowledgeDocumentSaveState,
    keyword_index: oxideterm_ai::RagKeywordIndexState,
    semantic_index: oxideterm_ai::RagSemanticIndexState,
    save_generation: u64,
    autosave_generation: u64,
    autosave_task: Option<Task<()>>,
    index_state_task: Option<Task<()>>,
    preview_scroll: MarkdownVirtualListScrollHandle,
    preview_navigation: oxideterm_gpui_markdown::navigation::MarkdownNavigation,
    preview_workspace: Option<gpui::WeakEntity<WorkspaceApp>>,
    preview_workspace_subscription: Option<Subscription>,
    has_background_image: bool,
    source_path: Option<std::path::PathBuf>,
    is_markdown: bool,
    preview_document: Option<oxideterm_gpui_markdown::MarkdownDocument>,
    preview_task: Option<Task<()>>,
    preview_timer: Option<Task<()>>,
    preview_version: u64,
    preview_running: bool,
    source_ratio: f32,
    split_bounds: Option<gpui::Bounds<gpui::Pixels>>,
    split_dragging: bool,
    preview_leads: bool,
    pending_preview_anchor: Option<oxideterm_gpui_markdown::scroll_sync::SourceAnchor>,
    _viewport_subscription: Option<Subscription>,
}

impl KnowledgeDocumentEditor {
    fn is_dirty(&self) -> bool {
        self.draft.as_ref() != self.saved_draft.as_ref()
    }

    fn save_current_draft(&mut self, cx: &mut Context<Self>) {
        let content = self.editor.read(cx).buffer().text();
        self.request_save(content, cx);
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
        let is_markdown = loaded.document.format == "markdown";
        let source_path = loaded
            .document
            .source_path
            .as_ref()
            .map(std::path::PathBuf::from);
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
            editor.set_language(is_markdown.then_some(LanguageId::Markdown), cx);
            editor.set_border_visible(false);
            editor.set_settings(
                oxideterm_gpui_editor::EditorSettings {
                    soft_wrap: true,
                    soft_wrap_column: None,
                    ..Default::default()
                },
                cx,
            );
            editor.set_transparent_background(has_background_image, cx);
            editor
        });
        editor.update(cx, |editor, _| editor.track_edit_changes());
        let preview_scroll = MarkdownVirtualListScrollHandle::new();
        let observed_buffer_version = editor.read(cx).buffer().version();
        let draft = Arc::<str>::from(content);
        let saved_draft = draft.clone();
        let editor_observer = cx.observe(&editor, |surface, editor, cx| {
            let buffer_version = editor.read(cx).buffer().version();
            if buffer_version == surface.observed_buffer_version {
                return;
            }
            surface.observed_buffer_version = buffer_version;
            let (draft, changes) = editor.update(cx, |editor, _| {
                (
                    Arc::from(editor.buffer().text()),
                    editor.take_edit_changes(),
                )
            });
            surface.draft = draft;
            surface.remap_preview_anchor(&changes);
            if surface.mode != KnowledgeEditorMode::Source && surface.is_markdown {
                surface.schedule_preview(cx);
            }
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
            mode: KnowledgeEditorMode::Source,
            previous_mode: KnowledgeEditorMode::Source,
            mode_transition: None,
            save_state: KnowledgeDocumentSaveState::Saved,
            keyword_index,
            semantic_index,
            save_generation: 0,
            autosave_generation: 0,
            autosave_task: None,
            index_state_task: None,
            preview_navigation: oxideterm_gpui_markdown::navigation::MarkdownNavigation::new(
                preview_scroll.scroll_handle().clone(),
            ),
            preview_scroll,
            preview_workspace: None,
            preview_workspace_subscription: None,
            has_background_image,
            source_path,
            is_markdown,
            preview_document: None,
            preview_task: None,
            preview_timer: None,
            preview_version: 0,
            preview_running: false,
            source_ratio: 0.5,
            split_bounds: None,
            split_dragging: false,
            preview_leads: false,
            pending_preview_anchor: None,
            _viewport_subscription: None,
        }
    }

    fn set_has_background_image(&mut self, has_background_image: bool, cx: &mut Context<Self>) {
        if self.has_background_image == has_background_image {
            return;
        }
        self.has_background_image = has_background_image;
        self.editor.update(cx, |editor, cx| {
            editor.set_transparent_background(has_background_image, cx);
        });
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
        let store = self.store.clone();
        let document_id = self.document_id.clone();
        self.index_state_task = Some(cx.spawn(async move |surface, cx| {
            loop {
                let store = store.clone();
                let document_id = document_id.clone();
                let (keyword, semantic) = cx
                    .background_executor()
                    .spawn(async move {
                        (
                            oxideterm_ai::rag_keyword_index_state(&store),
                            oxideterm_ai::rag_document_semantic_index_state(&store, &document_id),
                        )
                    })
                    .await;
                let finished = matches!(
                    keyword,
                    oxideterm_ai::RagKeywordIndexState::Ready
                        | oxideterm_ai::RagKeywordIndexState::Failed { .. }
                );
                if surface
                    .update(cx, |surface, cx| {
                        let mut changed = surface.keyword_index != keyword;
                        surface.keyword_index = keyword;
                        if let Ok(semantic) = semantic {
                            changed |= surface.semantic_index != semantic;
                            surface.semantic_index = semantic;
                        }
                        if changed {
                            cx.notify();
                        }
                    })
                    .is_err()
                    || finished
                {
                    break;
                }
                Timer::after(KNOWLEDGE_INDEX_STATE_POLL_INTERVAL).await;
            }
        }));
    }

    fn set_mode(&mut self, mode: KnowledgeEditorMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.previous_mode = self.mode;
            self.mode = mode;
            self.mode_transition = None;
            if let Some(motion) = oxideterm_gpui_ui::segmented_control_motion(&self.tokens) {
                self.mode_transition = Some(cx.spawn(async move |surface, cx| {
                    Timer::after(motion.duration).await;
                    let _ = surface.update(cx, |surface, cx| {
                        surface.previous_mode = surface.mode;
                        surface.mode_transition = None;
                        cx.notify();
                    });
                }));
            }
            self.editor.update(cx, |editor, _cx| {
                editor.set_read_only(mode == KnowledgeEditorMode::Preview);
            });
            if mode != KnowledgeEditorMode::Preview {
                window.focus(&self.editor.read(cx).focus_handle(cx), cx);
            }
            if mode != KnowledgeEditorMode::Source
                && self.is_markdown
                && (self.preview_document.is_none()
                    || self.preview_version != self.observed_buffer_version)
            {
                self.load_preview(cx);
            }
            if mode == KnowledgeEditorMode::Split {
                self.sync_from_editor(cx);
            }
            self.emit_preferences(cx);
            cx.notify();
        }
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
        self.draft = Arc::from(content.as_str());
        if content == self.saved_draft.as_ref() {
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
                        surface.saved_draft = saved_content.clone();
                        if surface.draft.as_ref() == saved_content.as_ref() {
                            surface.editor.update(cx, |editor, cx| {
                                if editor.buffer().text() == saved_content.as_ref() {
                                    editor.mark_saved_external(cx);
                                }
                            });
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
            KnowledgeDocumentSaveState::Saved => self.labels.saved.clone(),
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
                        surface.version = loaded.document.version;
                        surface.semantic_index = loaded.semantic_index;
                        surface.draft = content.clone();
                        surface.saved_draft = content.clone();
                        surface.preview_document = None;
                        surface.editor.update(cx, |editor, cx| {
                            editor.replace_text_external(content.to_string(), cx);
                            editor.mark_saved_external(cx);
                        });
                        surface.observed_buffer_version =
                            surface.editor.read(cx).buffer().version();
                        surface.keyword_index =
                            oxideterm_ai::rag_keyword_index_state(&surface.store);
                        surface.start_index_state_poll(cx);
                        surface.save_state = KnowledgeDocumentSaveState::Saved;
                        if surface.mode != KnowledgeEditorMode::Source && surface.is_markdown {
                            surface.load_preview(cx);
                        }
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
                KnowledgeEditorMode::Preview => "knowledge-editor-mode-preview",
                KnowledgeEditorMode::Split => "knowledge-editor-mode-split",
            })
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |surface, _event, window, cx| {
                    surface.set_mode(mode, window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_mode_switcher(&self, cx: &mut Context<Self>) -> AnyElement {
        let active_index = match self.mode {
            KnowledgeEditorMode::Source => 0,
            KnowledgeEditorMode::Preview => 1,
            KnowledgeEditorMode::Split => 2,
        };
        oxideterm_gpui_ui::segmented_control(
            &self.tokens,
            "knowledge-editor-mode-switcher",
            SegmentedControlOptions::new(
                active_index,
                match self.previous_mode {
                    KnowledgeEditorMode::Source => 0,
                    KnowledgeEditorMode::Preview => 1,
                    KnowledgeEditorMode::Split => 2,
                },
                if self.is_markdown { 3 } else { 2 },
            )
            .user_transition_active(self.mode_transition.is_some())
            .has_background_image(self.has_background_image)
            .compact(if self.is_markdown {
                KNOWLEDGE_EDITOR_MODE_SWITCHER_WIDTH * 1.5
            } else {
                KNOWLEDGE_EDITOR_MODE_SWITCHER_WIDTH
            }),
            vec![
                self.render_mode_button(
                    KnowledgeEditorMode::Source,
                    self.labels.source.clone(),
                    cx,
                ),
                self.render_mode_button(
                    KnowledgeEditorMode::Preview,
                    self.labels.preview.clone(),
                    cx,
                ),
            ]
            .into_iter()
            .chain(self.is_markdown.then(|| {
                self.render_mode_button(KnowledgeEditorMode::Split, self.labels.split.clone(), cx)
            }))
            .collect(),
        )
        .into_any_element()
    }

    fn apply_format_action(&mut self, action: KnowledgeFormatAction, cx: &mut Context<Self>) {
        let editor = match self.mode {
            KnowledgeEditorMode::Source | KnowledgeEditorMode::Split => Some(self.editor.clone()),
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
            | KnowledgeFormatAction::InlineMath
            | KnowledgeFormatAction::DisplayMath
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
        let source = self.mode != KnowledgeEditorMode::Preview;
        let preview = self.mode != KnowledgeEditorMode::Source;
        let conflict = matches!(self.save_state, KnowledgeDocumentSaveState::Conflict);
        let workspace = self
            .preview_workspace
            .as_ref()
            .and_then(gpui::WeakEntity::upgrade);
        let mut options = workspace
            .as_ref()
            .map(|workspace| workspace.read(cx).localized_markdown_options())
            .unwrap_or_else(|| MarkdownOptions::from_theme(&self.tokens));
        let selectable = workspace.as_ref().map(|workspace| {
            workspace
                .read(cx)
                .selectable_text_render_state_for_entity(workspace.clone(), cx)
        });
        let actions =
            workspace.map(
                |workspace| oxideterm_gpui_markdown::render::MarkdownCodeBlockActions {
                    on_run: None,
                    on_mermaid_zoom: Some(WorkspaceApp::mermaid_zoom_handler_for_workspace(
                        workspace,
                    )),
                },
            );
        options.background_surface_active = self.has_background_image;
        options.navigation = Some(self.preview_navigation.clone());
        options.scroll_sync = Some(self.preview_scroll.scroll_sync.clone());
        options.code_block_padding = self.tokens.spacing.two;
        if let Some(path) = self.source_path.as_ref() {
            options = options.with_source_path(path);
        }
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
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .px(px(self.tokens.spacing.three))
                    .border_b_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.title.clone()),
                    )
                    .child(self.render_mode_switcher(cx)),
            )
            .child({
                let source_pane = div().size_full().min_w_0().min_h_0().flex().flex_col()
                    .when(source && self.is_markdown, |pane| pane.child(self.render_source_toolbar(cx)))
                    .child(div().flex_1().min_h_0().child(self.editor.clone()));
                let preview_pane = if preview && self.is_markdown {
                    div()
                                .size_full()
                                .min_h_0()
                                .p(px(KNOWLEDGE_PREVIEW_PADDING))
                                .child(if let Some(document) = self.preview_document.as_ref() {
                                    let group_id = super::selectable_text::selectable_text_id("notes-preview", &self.document_id);
                                    let mut order = 0;
                                    oxideterm_gpui_markdown::render::render_document_virtual_selectable(
                                        preview_id,
                                        document,
                                        &self.tokens,
                                        &options,
                                        &self.preview_scroll,
                                        actions.as_ref(),
                                        &mut |key, text, runs, links| {
                                            let join_previous = key.join_previous;
                                            let index = order;
                                            order += 1;
                                            if let Some(state) = selectable.as_ref() {
                                                state.render_styled_text_in_group(
                                                    super::selectable_text::SelectableTextRole::PlainDocument,
                                                    group_id,
                                                    super::selectable_text::selectable_text_id("notes-preview-fragment", (group_id, key)),
                                                    index, text, runs, links, join_previous,
                                                )
                                            } else {
                                                gpui::StyledText::new(text).with_runs(runs).into_any_element()
                                            }
                                        },
                                    )
                                } else {
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_sm))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(self.labels.loading.clone())
                                        .into_any_element()
                                })
                } else { div() };
                if self.mode == KnowledgeEditorMode::Split && self.is_markdown {
                    let view = cx.entity();
                    div().id("knowledge-split").relative().w_full().flex_1().min_h_0().flex().overflow_hidden()
                        .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| this.resize_split(f32::from(event.position.x), cx)))
                        .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| this.finish_split_resize(cx)))
                        .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| this.finish_split_resize(cx)))
                        .child(div().id("knowledge-source-pane").relative().w(gpui::relative(self.source_ratio)).h_full().min_w_0().flex_none()
                            .child(source_pane)
                            .child(super::sidebar::sidebar_resize_hotzone_chrome("knowledge-split-divider", rgb(self.tokens.ui.border), true)
                                .right_0().top_0().bottom_0()
                                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                    this.split_dragging = true;
                                    window.prevent_default();
                                    cx.stop_propagation();
                                }))))
                        .child(div().id("knowledge-preview-pane").flex_1().min_w_0().h_full().child(preview_pane))
                        .child(gpui::canvas(move |bounds, _, app| {
                            view.update(app, |this, _| this.split_bounds = Some(bounds));
                        }, |_, _, _, _| {}).absolute().top_0().left_0().size_full()).into_any_element()
                } else {
                    div().w_full().flex_1().min_h_0().overflow_hidden()
                        .child(if source || !self.is_markdown { source_pane } else { preview_pane })
                        .into_any_element()
                }
            })
            .child(
                div()
                    .min_h(px(32.0))
                    .flex_none()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .px(px(self.tokens.spacing.three))
                    .py(px(self.tokens.spacing.one))
                    .border_t_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.save_status_label())
                    .child(div().flex_1())
                    .child(match &self.keyword_index {
                        oxideterm_ai::RagKeywordIndexState::Failed { .. } => {
                            self.labels.keyword_failed.clone()
                        }
                        oxideterm_ai::RagKeywordIndexState::Pending
                        | oxideterm_ai::RagKeywordIndexState::Rebuilding => {
                            self.labels.keyword_pending.clone()
                        }
                        _ => match self.semantic_index {
                            oxideterm_ai::RagSemanticIndexState::Ready => {
                                self.labels.index_ready.clone()
                            }
                            _ => self.labels.semantic_pending.clone(),
                        },
                    })
                    .when(conflict, |footer| {
                        footer
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
                                ButtonVariant::Ghost,
                                ButtonRadius::Sm,
                                28.0,
                                8.0,
                                self.tokens.metrics.ui_text_sm,
                            ),
                        )
                        .id("knowledge-editor-save")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.save_current_draft(cx);
                                cx.stop_propagation();
                            }),
                        ),
                    ),
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
    navigator_hidden: bool,
    mobile_navigator_open: bool,
    navigator_width: Option<f32>,
    navigator_resize: Option<(gpui::WindowId, f32, f32)>,
    navigator_closing: bool,
    navigator_motion_task: Option<Task<()>>,
    menu: Option<navigation::KnowledgeMenu>,
    pub(in crate::workspace) rename: Option<navigation::KnowledgeRename>,
    pub(in crate::workspace) metadata_task: Option<Task<()>>,
    metadata_error: Option<String>,
    clipboard: Option<operations::NoteClipboard>,
    search_task: Option<Task<()>>,
    last_embedding_running: bool,
    pending_collection_id: Option<String>,
    navigator_refresh_generation: u64,
    navigator_refresh_running: bool,
    navigator_refresh_requested: bool,
    navigator_last_refresh: Option<Instant>,
    pub(in crate::workspace) navigator_query: Arc<str>,
    pub(in crate::workspace) navigator_search_window: Option<gpui::WindowId>,
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
            self.menu = None;
            self.rename = None;
            self.metadata_task = None;
            self.metadata_error = None;
            self.search_task = None;
            self.navigator_resize = None;
            self.navigator_closing = false;
            self.navigator_motion_task = None;
            self.pending_collection_id = None;
            self.navigator_search_window = None;
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
        self.mobile_navigator_open = false;
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
        self.pending_collection_id = None;
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
        self.switch_after_save =
            self.pending_document_id.is_some() || self.pending_collection_id.is_some();
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
            .menu
            .as_ref()
            .is_some_and(|menu| menu.focus.is_focused(window))
        {
            return true;
        }
        if matches!(
            self.active_ime_target_for_window(window.window_handle().window_id(), cx),
            Some(
                ime::WorkspaceImeTarget::KnowledgeSearch | ime::WorkspaceImeTarget::KnowledgeRename
            )
        ) {
            return true;
        }
        let Some(document) = knowledge.editor.as_ref() else {
            return false;
        };
        let document = document.read(cx);
        document.mode != KnowledgeEditorMode::Preview
            && document.editor.read(cx).focus_handle(cx).is_focused(window)
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
        if command && matches!(key, "c" | "x" | "v") {
            if key == "v" {
                self.paste_knowledge_note(cx);
            } else if let Some(id) = self
                .knowledge_workspace
                .read(cx)
                .selected_document_id
                .clone()
            {
                self.copy_knowledge_note(id, key == "x", cx);
            }
            return true;
        }
        if command && key == "f" {
            self.knowledge_workspace.update(cx, |state, _| {
                state.navigator_hidden = false;
                state.mobile_navigator_open = true;
            });
            self.clear_ime_selection();
            self.selected_ime_target = Some(ime::WorkspaceImeTarget::KnowledgeSearch);
            window.focus(&self.focus_handle, cx);
            self.show_active_input_caret(cx);
            cx.notify();
            return true;
        }
        let direction = match key {
            "up" | "arrowup" => -1,
            "down" | "arrowdown" => 1,
            _ => return false,
        };
        let (visible, selected_document_id) = {
            let knowledge = self.knowledge_workspace.read(cx);
            (
                knowledge.navigator_snapshot.documents.clone(),
                knowledge.selected_document_id.clone(),
            )
        };
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

    fn knowledge_navigator_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = ime::WorkspaceImeTarget::KnowledgeSearch;
        let query = self.knowledge_workspace.read(cx).navigator_query.clone();
        let input = oxideterm_gpui_ui::text_input::text_input(
            &self.tokens,
            oxideterm_gpui_ui::text_input::TextInputView {
                value: &query,
                placeholder: self
                    .i18n
                    .t("settings_view.knowledge.navigator_search_placeholder"),
                focused: self.active_ime_target(cx) == Some(target),
                caret_visible: self.input_caret.visible(),
                secret: false,
                selected_all: false,
                selected_range: self.ime_selected_range_for_target(target, cx),
                marked_text: self.marked_text_for_target(target, cx),
            },
        )
        .flex_1()
        .min_w_0()
        .px_0()
        .border_0()
        .rounded_none()
        .bg(gpui::transparent_black());
        let input = self.text_input_with_workspace_ime(
            target,
            input,
            |this, cx| this.show_active_input_caret(cx),
            cx,
        );
        div()
            .h(px(self.tokens.metrics.ui_button_lg_height))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .px(px(self.tokens.spacing.two))
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .child(Self::render_lucide_icon(
                LucideIcon::Search,
                KNOWLEDGE_NAVIGATOR_ACTION_ICON_SIZE,
                rgb(self.tokens.ui.text_muted),
            ))
            .child(input)
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
        let tooltip_label = tooltip;
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

    /// Reports whether the Knowledge draft confirmation currently owns window input.
    pub(in crate::workspace) fn knowledge_leave_confirmation_open(&self, cx: &App) -> bool {
        let knowledge = self.knowledge_workspace.read(cx);
        knowledge.pending_document_id.is_some()
            || knowledge.pending_collection_id.is_some()
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
        KnowledgeEditorLabels::new(&self.i18n)
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
        let query = self.knowledge_workspace.read(cx).navigator_query.clone();
        let search_query = query.clone();
        let requested_collection = preferred_collection_id.clone();
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
                    let mut documents = selected_collection_id
                        .as_deref()
                        .map(|id| oxideterm_ai::rag_list_documents(&store, id, None, None))
                        .transpose()?
                        .map(|page| page.documents)
                        .unwrap_or_default();
                    if !search_query.is_empty() {
                        let terms: Vec<_> = search_query
                            .split_whitespace()
                            .map(str::to_lowercase)
                            .collect();
                        let mut matching = Vec::new();
                        for document in documents {
                            let content =
                                oxideterm_ai::rag_get_document_content(&store, &document.id)?;
                            if knowledge_document_matches(&document.title, &content, &terms) {
                                matching.push(document);
                            }
                        }
                        documents = matching;
                    }
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
                if workspace.knowledge_workspace.read(cx).navigator_query != query
                    || workspace
                        .ai_entity
                        .read(cx)
                        .knowledge_selected_collection_id()
                        != requested_collection.as_deref()
                {
                    workspace.knowledge_workspace.update(cx, |state, _| {
                        state.navigator_refresh_running = false;
                    });
                    workspace.refresh_knowledge_navigator(true, cx);
                    return;
                }
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
            knowledge.request_close(tab_id, window.window_handle());
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
    pub(in crate::workspace) fn guard_detached_knowledge_window_close(
        &mut self,
        tab_id: TabId,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .window_registry
            .is_only_window_with_role(window_registry::WindowRole::Detached { tab_id })
        {
            return self.guard_dirty_knowledge_app_quit(cx);
        }
        // Native detached-window release removes its tab on the current workspace host.
        // Resolve the draft before that release even when other windows remain open.
        self.tab_by_id(tab_id, cx)
            .is_some_and(|tab| tab.kind == TabKind::Knowledge)
            && self.guard_dirty_knowledge_tab_close(tab_id, window, cx)
    }

    /// Applies the dirty-draft guard for quit intents that do not originate from a focused window.
    pub(in crate::workspace) fn request_application_quit(&mut self, cx: &mut Context<Self>) {
        if self.guard_dirty_knowledge_app_quit(cx) {
            return;
        }
        super::request_app_quit(cx);
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
            self.knowledge_workspace
                .update(cx, |state, _| state.mobile_navigator_open = false);
            cx.notify();
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
                        let preview_workspace = cx.entity();
                        let preferences = workspace.settings_store.settings().window_ui.knowledge_editor.clone();
                        editor.update(cx, |editor, cx| {
                            editor.preview_workspace = Some(preview_workspace.downgrade());
                            editor.initialize_preview(preferences, cx);
                            editor.preview_workspace_subscription =
                                Some(cx.observe(&preview_workspace, |editor, _, cx| {
                                    if editor.mode != KnowledgeEditorMode::Source {
                                        cx.notify();
                                    }
                                }));
                        });
                        KnowledgeDocumentEditor::configure_save_callback(&editor, cx);
                        editor.update(cx, |editor, cx| editor.start_index_state_poll(cx));
                        let subscription = cx.subscribe(
                            &editor,
                            |workspace, _editor, event: &KnowledgeDocumentEditorEvent, cx| {
                                if let KnowledgeDocumentEditorEvent::PreferencesChanged(preferences) = event {
                                    workspace.settings_store.settings_mut().window_ui.knowledge_editor = preferences.clone();
                                    if let Err(error) = workspace.settings_store.save() {
                                        tracing::warn!(%error, "failed to save local note editor preferences");
                                    } else {
                                        workspace.settings_workspace.update(cx, |settings, _| settings.acknowledge_external_store_state());
                                    }
                                    return;
                                }
                                workspace.refresh_knowledge_navigator(true, cx);
                                let notebook =
                                    workspace.knowledge_workspace.update(cx, |state, _| {
                                        if state.switch_after_save {
                                            state.pending_collection_id.take()
                                        } else {
                                            None
                                        }
                                    });
                                if let Some(id) = notebook {
                                    workspace.change_knowledge_notebook(id, true, cx);
                                }
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
                                    super::request_app_quit(cx);
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
        let notebook = self
            .knowledge_workspace
            .update(cx, |state, _| state.pending_collection_id.take());
        if let Some(id) = notebook {
            self.change_knowledge_notebook(id, true, cx);
            return;
        }
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
                super::request_app_quit(cx);
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
            super::request_app_quit(cx);
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
        if !self.tokens.motion.enabled {
            self.knowledge_workspace.update(cx, |state, _| {
                state.navigator_closing = false;
                state.navigator_motion_task = None;
            });
        }
        let (show_navigator, navigator_width) = {
            let state = self.knowledge_workspace.read(cx);
            (
                if narrow_layout {
                    state.mobile_navigator_open || (state.editor.is_none() && !state.loading)
                } else {
                    !state.navigator_hidden
                },
                state
                    .navigator_width
                    .unwrap_or(KNOWLEDGE_NAVIGATOR_DEFAULT_WIDTH)
                    .clamp(
                        self.tokens.metrics.sidebar_min_width,
                        available_width
                            .min(self.tokens.metrics.sidebar_max_width)
                            .max(self.tokens.metrics.sidebar_min_width),
                    ),
            )
        };
        let has_background_image = self.background_surface_active("knowledge");
        let embedding_running = self
            .ai_entity
            .read(cx)
            .knowledge_embedding_progress()
            .is_some();
        let embedding_finished = self.knowledge_workspace.update(cx, |state, _| {
            let finished = state.last_embedding_running && !embedding_running;
            state.last_embedding_running = embedding_running;
            finished
        });
        if let Some(editor) = self.knowledge_workspace.read(cx).editor.clone() {
            editor.update(cx, |editor, cx| {
                if editor.tokens != self.tokens {
                    editor.tokens = self.tokens;
                    if !self.tokens.motion.enabled {
                        editor.mode_transition = None;
                        editor.previous_mode = editor.mode;
                    }
                    cx.notify();
                }
                editor.set_has_background_image(has_background_image, cx);
                if embedding_finished {
                    editor.start_index_state_poll(cx);
                }
            });
        }
        let labels = self.knowledge_editor_labels();
        let navigator_snapshot = self.knowledge_workspace.read(cx).navigator_snapshot.clone();
        let selected_collection = navigator_snapshot.selected_collection;
        let documents = navigator_snapshot.documents;
        let navigator_query = self.knowledge_workspace.read(cx).navigator_query.clone();
        let filtered_documents = documents;
        let navigator_error =
            if let Some(error) = self.knowledge_workspace.read(cx).metadata_error.clone() {
                Some(error)
            } else if let Some(error) = self.ai_entity.read(cx).knowledge_error() {
                Some(error.to_owned())
            } else if navigator_snapshot.error.is_some() {
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
                knowledge.pending_document_id.is_some()
                    || knowledge.pending_collection_id.is_some(),
                knowledge.pending_close.is_some(),
                knowledge.pending_app_quit,
            )
        };
        let has_editor = editor.is_some();
        let document_row_count = filtered_documents.len();
        if self.knowledge_workspace_list_state.item_count() != document_row_count {
            self.knowledge_workspace_list_state
                .reset(document_row_count);
        }
        let navigator_toolbar = self.knowledge_navigator_toolbar(selected_collection.as_ref(), cx);
        self.knowledge_workspace.update(cx, |state, _| {
            state.navigator_search_window =
                show_navigator.then_some(window.window_handle().window_id());
        });
        let navigator_search = self.knowledge_navigator_search(cx);
        let spec = TauriVirtualListSpec::new(
            px(KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT),
            KNOWLEDGE_WORKSPACE_SECTION_OVERSCAN,
        );
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
                let documents_for_list = filtered_documents;
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
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        window.focus(&this.focus_handle, cx);
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        this.open_knowledge_list_menu(event.position, window, cx);
                        cx.stop_propagation();
                    }),
                )
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
            .w(px(navigator_width))
            .when(narrow_layout, |navigator| navigator.w_full())
            .h_full()
            .when(narrow_layout, |navigator| {
                navigator
                    .border_r_1()
                    .border_color(rgb(self.tokens.ui.border))
            })
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
            .child(documents_section)
            .relative()
            .when(!narrow_layout, |navigator| {
                navigator.child(self.knowledge_resize_handle(navigator_width, cx))
            });
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
        let leave_labels = labels;
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
            .flex_col()
            .relative()
            .overflow_hidden()
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                let target = ime::WorkspaceImeTarget::KnowledgeSearch;
                if this.active_ime_target(cx) == Some(target)
                    && this
                        .text_input_anchors
                        .get(target.anchor_id())
                        .is_none_or(|anchor| !anchor.bounds.contains(&event.position))
                {
                    this.clear_ime_selection();
                    cx.notify();
                }
            }))
            .child(self.knowledge_workspace_header(narrow_layout, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(
                        show_navigator || self.knowledge_workspace.read(cx).navigator_closing,
                        |content| {
                            content.child(oxideterm_gpui_ui::motion::horizontal_reveal(
                                &self.tokens,
                                "notes-navigation-motion",
                                navigator,
                                if narrow_layout {
                                    available_width
                                } else {
                                    navigator_width
                                },
                                show_navigator,
                            ))
                        },
                    )
                    .when(!narrow_layout || !show_navigator, |content| {
                        content.child(editor_pane)
                    }),
            )
            .children(self.render_knowledge_menu(window, cx))
            .children(self.render_knowledge_rename(window, cx))
            .when(
                layout == KnowledgeWorkspaceLayout::DetachedWindow
                    && self.knowledge_resize_active(cx),
                |content| content.child(self.render_workspace_pointer_capture_overlay(cx)),
            )
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

    #[gpui::test]
    fn undo_during_save_keeps_the_returned_draft_dirty(cx: &mut gpui::TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(oxideterm_ai::RagStore::new(directory.path()).unwrap());
        let collection = oxideterm_ai::rag_create_collection(
            &store,
            oxideterm_ai::RagCreateCollectionRequest {
                name: "Notes".into(),
                scope: oxideterm_ai::RagDocScopeRequest::Global,
            },
        )
        .unwrap();
        let doc = oxideterm_ai::rag_create_blank_document(
            &store,
            oxideterm_ai::RagCreateBlankDocumentRequest {
                collection_id: collection.id,
                title: "Draft".into(),
                format: "markdown".into(),
            },
        )
        .unwrap();
        oxideterm_ai::rag_save_document(&store, &doc.id, "alpha".into(), Some(0)).unwrap();
        let loaded = oxideterm_ai::rag_get_document(&store, &doc.id).unwrap();
        let surface = cx.new(|cx| {
            KnowledgeDocumentEditor::new(
                loaded,
                store.clone(),
                oxideterm_theme::default_tokens(),
                KnowledgeEditorLabels::new(&oxideterm_i18n::I18n::default()),
                false,
                cx,
            )
        });
        surface.update(cx, |surface, cx| {
            surface
                .editor
                .update(cx, |editor, cx| editor.replace_text_external("beta", cx));
            surface.save_current_draft(cx);
            surface
                .editor
                .update(cx, |editor, cx| editor.replace_text_external("alpha", cx));
        });
        cx.run_until_parked();
        assert_eq!(
            oxideterm_ai::rag_get_document_content(&store, &doc.id).unwrap(),
            "beta"
        );
        surface.update(cx, |surface, cx| {
            assert_eq!(surface.draft.as_ref(), "alpha");
            assert!(surface.is_dirty());
            surface.save_current_draft(cx);
        });
        cx.run_until_parked();
        assert_eq!(
            oxideterm_ai::rag_get_document_content(&store, &doc.id).unwrap(),
            "alpha"
        );
        surface.read_with(cx, |surface, _| assert!(!surface.is_dirty()));
    }

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
    fn navigator_search_matches_title_and_body() {
        let terms = vec!["production".to_owned(), "数据库".to_owned()];
        assert!(knowledge_document_matches(
            "Production runbook",
            "数据库恢复步骤",
            &terms
        ));
        assert!(!knowledge_document_matches(
            "Production runbook",
            "Network recovery",
            &terms
        ));
        assert!(!knowledge_document_matches(
            "Local runbook",
            "数据库恢复步骤",
            &terms
        ));
        assert!(knowledge_document_matches("Untitled", "", &[]));
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
