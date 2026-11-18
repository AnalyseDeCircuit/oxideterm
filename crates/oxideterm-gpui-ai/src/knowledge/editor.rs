use std::{rc::Rc, sync::Arc, time::Duration};

use gpui::{
    AnyElement, App, ClipboardItem, Context, Entity, EventEmitter, Focusable, MouseButton, Render,
    Subscription, Task, Window, div, prelude::*, px, rgb, svg,
};
use oxideterm_editor_syntax::LanguageId;
use oxideterm_gpui_editor::{EditorContextMenuLabels, TextEditorView};
use oxideterm_gpui_markdown::{
    MarkdownOptions, MarkdownVirtualListScrollHandle,
    render::{MarkdownCodeBlockActions, MarkdownTextFragmentId, MarkdownTextLink},
};
use oxideterm_gpui_ui::{
    SegmentedControlOptions, ToolbarButtonOptions,
    button::{ButtonRadius, ButtonVariant},
    icons::LucideIcon,
};
use oxideterm_theme::ThemeTokens;

mod preview;

const KNOWLEDGE_EDITOR_MODE_SWITCHER_WIDTH: f32 = 176.0;
const KNOWLEDGE_PREVIEW_PADDING: f32 = 24.0;
const KNOWLEDGE_AUTOSAVE_DELAY: Duration = Duration::from_millis(1_200);
const KNOWLEDGE_INDEX_STATE_POLL_INTERVAL: Duration = Duration::from_secs(1);

pub type KnowledgePreviewText = Rc<
    dyn Fn(
        &str,
        MarkdownTextFragmentId,
        usize,
        gpui::SharedString,
        Vec<gpui::TextRun>,
        Vec<MarkdownTextLink>,
    ) -> AnyElement,
>;

pub struct KnowledgePreviewFrame {
    pub options: MarkdownOptions,
    pub text: Option<KnowledgePreviewText>,
    pub actions: Option<MarkdownCodeBlockActions>,
}

pub type KnowledgePreviewSource = Rc<dyn Fn(&App) -> Option<KnowledgePreviewFrame>>;

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
pub struct KnowledgeEditorLabels {
    source: String,
    preview: String,
    split: String,
    pub save: String,
    saved: String,
    saving: String,
    dirty: String,
    conflict: String,
    save_failed: String,
    pub load_failed: String,
    pub navigator_load_failed: String,
    keyword_pending: String,
    keyword_failed: String,
    semantic_pending: String,
    index_ready: String,
    pub empty: String,
    pub loading: String,
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
    pub switch_title: String,
    pub switch_description: String,
    pub close_title: String,
    pub close_description: String,
    pub quit_title: String,
    pub quit_description: String,
    pub discard: String,
    pub cancel: String,
    reload: String,
    copy_draft: String,
    copy: String,
    cut: String,
    paste: String,
    select_all: String,
}

impl KnowledgeEditorLabels {
    pub fn new(i18n: &oxideterm_i18n::I18n) -> Self {
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

pub enum KnowledgeDocumentEditorEvent {
    Saved,
    PreferencesChanged(oxideterm_settings::KnowledgeEditorUiState),
}

/// Owns the currently selected document draft inside the single Knowledge workspace tab.
pub struct KnowledgeDocumentEditor {
    pub document_id: String,
    pub collection_id: String,
    pub title: String,
    pub version: u64,
    store: Arc<oxideterm_ai::RagStore>,
    tokens: ThemeTokens,
    labels: KnowledgeEditorLabels,
    pub editor: Entity<TextEditorView>,
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
    preview_source: Option<KnowledgePreviewSource>,
    preview_subscriptions: Vec<Subscription>,
    has_background_image: bool,
    source_path: Option<std::path::PathBuf>,
    is_markdown: bool,
    preview_document: Option<oxideterm_gpui_markdown::MarkdownDocument>,
    preview_task: Option<Task<()>>,
    preview_timer: Option<Task<()>>,
    preview_version: u64,
    preview_running: bool,
    cache_visible: bool,
    source_ratio: f32,
    split_bounds: Option<gpui::Bounds<gpui::Pixels>>,
    split_dragging: bool,
    preview_leads: bool,
    pending_preview_anchor: Option<oxideterm_gpui_markdown::scroll_sync::SourceAnchor>,
    _viewport_subscription: Option<Subscription>,
}

impl KnowledgeDocumentEditor {
    pub(super) fn is_saved(&self) -> bool {
        matches!(self.save_state, KnowledgeDocumentSaveState::Saved)
    }
    pub fn source_editable(&self) -> bool {
        self.mode != KnowledgeEditorMode::Preview
    }

    pub fn initialize(
        &mut self,
        preferences: oxideterm_settings::KnowledgeEditorUiState,
        preview_source: KnowledgePreviewSource,
        cx: &mut Context<Self>,
    ) {
        self.preview_source = Some(preview_source);
        self.initialize_preview(preferences, cx);
        self.start_index_state_poll(cx);
    }

    pub fn observe_preview_source<T: 'static>(
        &mut self,
        source: &Entity<T>,
        cx: &mut Context<Self>,
    ) {
        self.preview_subscriptions
            .push(cx.observe(source, |editor, _, cx| {
                if editor.mode != KnowledgeEditorMode::Source {
                    cx.notify();
                }
            }));
    }

    pub fn set_theme(&mut self, tokens: ThemeTokens, cx: &mut Context<Self>) {
        if self.tokens != tokens {
            self.tokens = tokens;
            if !tokens.motion.enabled {
                self.mode_transition = None;
                self.previous_mode = self.mode;
            }
            cx.notify();
        }
    }

    pub fn draft(&self) -> &str {
        &self.draft
    }

    pub fn is_dirty(&self) -> bool {
        self.draft.as_ref() != self.saved_draft.as_ref()
    }

    pub fn save_current_draft(&mut self, cx: &mut Context<Self>) {
        let content = self.editor.read(cx).buffer().text();
        self.request_save(content, cx);
    }

    pub fn new(
        loaded: oxideterm_ai::RagDocumentContentResponse,
        store: Arc<oxideterm_ai::RagStore>,
        tokens: ThemeTokens,
        labels: KnowledgeEditorLabels,
        has_background_image: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let content: Arc<str> = loaded.content.into();
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
        let draft = content;
        let saved_draft = draft.clone();
        let editor_observer = cx.observe(&editor, |surface, editor, cx| {
            let buffer_version = editor.read(cx).buffer().version();
            if buffer_version == surface.observed_buffer_version {
                return;
            }
            surface.observed_buffer_version = buffer_version;
            let (draft, changes) = editor.update(cx, |editor, _| {
                (editor.buffer().text_snapshot(), editor.take_edit_changes())
            });
            surface.draft = draft;
            surface.remap_preview_anchor(&changes);
            if surface.cache_visible
                && surface.mode != KnowledgeEditorMode::Source
                && surface.is_markdown
            {
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
            preview_source: None,
            preview_subscriptions: Vec::new(),
            has_background_image,
            source_path,
            is_markdown,
            preview_document: None,
            preview_task: None,
            preview_timer: None,
            preview_version: 0,
            preview_running: false,
            cache_visible: true,
            source_ratio: 0.5,
            split_bounds: None,
            split_dragging: false,
            preview_leads: false,
            pending_preview_anchor: None,
            _viewport_subscription: None,
        }
    }

    pub fn set_has_background_image(&mut self, has_background_image: bool, cx: &mut Context<Self>) {
        if self.has_background_image == has_background_image {
            return;
        }
        self.has_background_image = has_background_image;
        self.editor.update(cx, |editor, cx| {
            editor.set_transparent_background(has_background_image, cx);
        });
        cx.notify();
    }

    pub fn configure_save_callback(surface: &Entity<Self>, cx: &mut App) {
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

    pub fn start_index_state_poll(&mut self, cx: &mut Context<Self>) {
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
                cx.background_executor()
                    .timer(KNOWLEDGE_INDEX_STATE_POLL_INTERVAL)
                    .await;
            }
        }));
    }

    fn set_mode(&mut self, mode: KnowledgeEditorMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.previous_mode = self.mode;
            self.mode = mode;
            self.sync_render_caches(cx);
            self.mode_transition = None;
            if let Some(motion) = oxideterm_gpui_ui::segmented_control_motion(&self.tokens) {
                self.mode_transition = Some(cx.spawn(async move |surface, cx| {
                    cx.background_executor().timer(motion.duration).await;
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
            if self.cache_visible
                && mode != KnowledgeEditorMode::Source
                && self.is_markdown
                && (self.preview_document.is_none()
                    || self.preview_version != self.observed_buffer_version)
            {
                self.load_preview(cx);
            }
            if self.cache_visible && mode == KnowledgeEditorMode::Split {
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
            cx.background_executor()
                .timer(KNOWLEDGE_AUTOSAVE_DELAY)
                .await;
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
        let frame = self.preview_source.as_ref().and_then(|source| source(cx));
        let (mut options, selectable, actions) = frame.map_or_else(
            || (MarkdownOptions::from_theme(&self.tokens), None, None),
            |frame| (frame.options, frame.text, frame.actions),
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
                let source_pane = div()
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .when(source && self.is_markdown, |pane| {
                        pane.child(self.render_source_toolbar(cx))
                    })
                    .child(div().flex_1().min_h_0().child(self.editor.clone()));
                let preview_pane = if preview && self.is_markdown {
                    div()
                        .size_full()
                        .min_h_0()
                        .p(px(KNOWLEDGE_PREVIEW_PADDING))
                        .child(if let Some(document) = self.preview_document.as_ref() {
                            let mut order = 0;
                            oxideterm_gpui_markdown::render::render_document_virtual_selectable(
                                preview_id,
                                document,
                                &self.tokens,
                                &options,
                                &self.preview_scroll,
                                actions.as_ref(),
                                &mut |key, text, runs, links| {
                                    let index = order;
                                    order += 1;
                                    if let Some(render) = selectable.as_ref() {
                                        render(&self.document_id, key, index, text, runs, links)
                                    } else {
                                        gpui::StyledText::new(text)
                                            .with_runs(runs)
                                            .into_any_element()
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
                } else {
                    div()
                };
                if self.mode == KnowledgeEditorMode::Split && self.is_markdown {
                    let view = cx.entity();
                    div()
                        .id("knowledge-split")
                        .relative()
                        .w_full()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .overflow_hidden()
                        .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                            this.resize_split(f32::from(event.position.x), cx)
                        }))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| this.finish_split_resize(cx)),
                        )
                        .on_mouse_up_out(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| this.finish_split_resize(cx)),
                        )
                        .child(
                            div()
                                .id("knowledge-source-pane")
                                .relative()
                                .w(gpui::relative(self.source_ratio))
                                .h_full()
                                .min_w_0()
                                .flex_none()
                                .child(source_pane)
                                .child(
                                    oxideterm_gpui_ui::separator::sidebar_resize_hotzone_chrome(
                                        "knowledge-split-divider",
                                        rgb(self.tokens.ui.border),
                                        true,
                                    )
                                    .right_0()
                                    .top_0()
                                    .bottom_0()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, window, cx| {
                                            this.split_dragging = true;
                                            window.prevent_default();
                                            cx.stop_propagation();
                                        }),
                                    ),
                                ),
                        )
                        .child(
                            div()
                                .id("knowledge-preview-pane")
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .child(preview_pane),
                        )
                        .child(
                            gpui::canvas(
                                move |bounds, _, app| {
                                    view.update(app, |this, _| this.split_bounds = Some(bounds));
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                        )
                        .into_any_element()
                } else {
                    div()
                        .w_full()
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .child(if source || !self.is_markdown {
                            source_pane
                        } else {
                            preview_pane
                        })
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

    #[gpui::test]
    fn background_note_releases_preview_and_restores_unsaved_history(
        cx: &mut gpui::TestAppContext,
    ) {
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
        let document = oxideterm_ai::rag_create_blank_document(
            &store,
            oxideterm_ai::RagCreateBlankDocumentRequest {
                collection_id: collection.id,
                title: "Memory".into(),
                format: "markdown".into(),
            },
        )
        .unwrap();
        let loaded = oxideterm_ai::rag_get_document(&store, &document.id).unwrap();
        let (surface, cx) = cx.add_window_view(|_, cx| {
            KnowledgeDocumentEditor::new(
                loaded,
                store,
                oxideterm_theme::default_tokens(),
                KnowledgeEditorLabels::new(&oxideterm_i18n::I18n::default()),
                false,
                cx,
            )
        });
        let text = "# Heading\n\nUnsaved **paragraph**.\n\n".repeat(2048);
        surface.update(cx, |surface, cx| {
            // Keep this draft unsaved without scheduling an unrelated index rebuild.
            surface.save_state = KnowledgeDocumentSaveState::Conflict;
            surface
                .editor
                .update(cx, |editor, cx| editor.insert_text(&text, cx));
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            surface.update(cx, |surface, cx| {
                surface.set_mode(KnowledgeEditorMode::Preview, window, cx)
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        surface.update(cx, |surface, cx| {
            let before = surface.preview_document.as_ref().unwrap().retained_bytes();
            surface
                .preview_scroll
                .set_offset(gpui::point(px(0.0), px(-400.0)));
            surface.set_cache_visible(false, cx);
            eprintln!(
                "KNOWLEDGE_PREVIEW_BYTES active={before} hidden={}",
                surface
                    .preview_document
                    .as_ref()
                    .map_or(0, |doc| doc.retained_bytes())
            );
            assert!(surface.preview_document.is_none());
            assert_eq!(surface.draft.as_ref(), text);
            assert!(surface.is_dirty());
            surface.set_cache_visible(true, cx);
        });
        cx.run_until_parked();
        surface.read_with(cx, |surface, _| {
            assert!(surface.preview_document.is_some());
            assert_eq!(surface.preview_scroll.offset().y, px(-400.0));
        });
        cx.update(|window, cx| {
            surface.update(cx, |surface, cx| {
                surface.set_mode(KnowledgeEditorMode::Source, window, cx);
                assert!(surface.preview_document.is_none());
                surface
                    .editor
                    .update(cx, |editor, cx| editor.undo_external(cx));
            })
        });
        cx.run_until_parked();
        surface.read_with(cx, |surface, _| assert_eq!(surface.draft.as_ref(), ""));
        cx.update(|window, cx| {
            surface.update(cx, |surface, cx| {
                surface.set_mode(KnowledgeEditorMode::Preview, window, cx);
                surface.set_cache_visible(false, cx);
            })
        });
        cx.run_until_parked();
        surface.read_with(cx, |surface, _| {
            assert!(
                surface.preview_document.is_none(),
                "late parse repopulated a hidden preview"
            )
        });
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
}
