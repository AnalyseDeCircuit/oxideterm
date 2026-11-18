use std::{
    ops::Range,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    AnyElement, AnyWindowHandle, App, Context, Div, Entity, FocusHandle, Focusable, KeyDownEvent,
    ListAlignment, ListState, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Render,
    Subscription, Task, Window, div, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_gpui_ui::{
    EntityListRowOptions, IconButtonOptions, ToolbarButtonOptions,
    button::{ButtonRadius, ButtonVariant},
    color_for_background, entity_list_row,
    icons::LucideIcon,
    text_input::{
        TextInputAnchor, TextInputContentAlign, TextInputView, TextInputViewport,
        text_input_anchor_probe, text_input_with_viewport_and_content_align,
    },
    virtual_list::{TauriVirtualListSpec, tauri_virtual_list},
};
use oxideterm_i18n::I18n;
use oxideterm_settings::KnowledgeEditorUiState;
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::TabId;

use super::editor::{
    KnowledgeDocumentEditor, KnowledgeDocumentEditorEvent, KnowledgeEditorLabels,
    KnowledgePreviewSource,
};
use super::page::{KnowledgeDocumentsChanged, KnowledgePage};

mod navigation;
mod operations;
mod state;
use state::KnowledgeNavigatorSnapshot;
pub use state::KnowledgeWorkspaceEntity;

const KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT: f32 = 28.0;
const KNOWLEDGE_WORKSPACE_SECTION_OVERSCAN: usize = 8;
const KNOWLEDGE_NAVIGATOR_ACTION_SIZE: f32 = 28.0;
const KNOWLEDGE_NAVIGATOR_ACTION_ICON_SIZE: f32 = 14.0;
const KNOWLEDGE_NAVIGATOR_DEFAULT_WIDTH: f32 = 200.0;
const KNOWLEDGE_NARROW_VIEWPORT_WIDTH: f32 = 720.0;
const KNOWLEDGE_BACKGROUND_SURFACE_ALPHA: u32 = 0x66;
const KNOWLEDGE_NAVIGATOR_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnowledgeInput {
    Search,
    Rename,
}

pub struct KnowledgeInputFrame {
    pub focused: bool,
    pub caret_visible: bool,
    pub selected_range: Option<Range<usize>>,
    pub active_offset: Option<usize>,
    pub marked_text: Option<String>,
    pub anchor_id: oxideterm_gpui_ui::text_input::TextInputAnchorId,
    pub viewport: TextInputViewport,
    pub update_anchor: Rc<dyn Fn(TextInputAnchor)>,
}

pub enum KnowledgeInputRequest {
    Focus(KnowledgeInput),
    PointerDown(KnowledgeInput, MouseDownEvent),
    Drag(gpui::MouseMoveEvent),
    DismissSearchOutside(gpui::Point<Pixels>),
    Clear,
}

pub enum KnowledgeRequest {
    Close(TabId, AnyWindowHandle),
    Quit,
    SaveEditorPreferences(KnowledgeEditorUiState),
    CreateNotebook,
    CreateDocument(String),
    FocusDeleteConfirmation,
    Import(String),
    Settings,
}

#[derive(Default)]
struct KnowledgeStatus {
    pub collection: Option<String>,
    pub embedding: bool,
    pub reindex: bool,
    pub error: Option<String>,
}

pub struct KnowledgeEnvironment {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub background: bool,
    pub preferences: KnowledgeEditorUiState,
}

pub struct KnowledgeBindings {
    pub page: Entity<KnowledgePage>,
    pub environment: Rc<dyn Fn(&App) -> Option<KnowledgeEnvironment>>,
    pub preview: KnowledgePreviewSource,
    pub frame: Rc<dyn Fn(KnowledgeInput, &App) -> Option<KnowledgeInputFrame>>,
    pub input: Rc<dyn Fn(KnowledgeInputRequest, &mut Window, &mut App)>,
    pub request: Rc<dyn Fn(KnowledgeRequest, Option<&mut Window>, &mut App)>,
    pub embedding_job: Rc<dyn Fn(&App) -> Option<Result<super::page::EmbeddingJob, String>>>,
}

pub struct KnowledgeViews {
    pub view: Entity<KnowledgeWorkspaceView>,
    pub state: Entity<KnowledgeWorkspaceEntity>,
}

pub struct KnowledgeWorkspaceView {
    state: Entity<KnowledgeWorkspaceEntity>,
    bindings: KnowledgeBindings,
    focus_handle: FocusHandle,
    tokens: ThemeTokens,
    i18n: I18n,
    has_background: bool,
    knowledge_workspace_list_state: ListState,
}

impl Render for KnowledgeWorkspaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(environment) = (self.bindings.environment)(cx) else {
            return div().into_any_element();
        };
        self.tokens = environment.tokens;
        self.i18n = environment.i18n;
        self.has_background = environment.background;
        self.render_knowledge_workspace_surface(window, cx)
    }
}

impl KnowledgeWorkspaceEntity {
    pub fn input_owner(&self, input: KnowledgeInput) -> Option<gpui::WindowId> {
        match input {
            KnowledgeInput::Search => self.navigator_search_window,
            KnowledgeInput::Rename => self.rename.as_ref().map(|rename| rename.window_id),
        }
    }

    pub fn input_value(&self, input: KnowledgeInput) -> Option<&str> {
        match input {
            KnowledgeInput::Search => Some(&self.navigator_query),
            KnowledgeInput::Rename => self.rename.as_ref().map(|rename| rename.name.as_str()),
        }
    }
}

impl KnowledgeWorkspaceView {
    pub fn new(
        state: Entity<KnowledgeWorkspaceEntity>,
        bindings: KnowledgeBindings,
        focus_handle: FocusHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        cx.observe(&bindings.page, |_, _, cx| cx.notify()).detach();
        cx.subscribe(
            &bindings.page,
            |view, _, _: &KnowledgeDocumentsChanged, cx| {
                view.refresh_knowledge_navigator(true, cx);
            },
        )
        .detach();
        Self {
            state,
            bindings,
            focus_handle,
            tokens: oxideterm_theme::default_tokens(),
            i18n: I18n::default(),
            has_background: false,
            knowledge_workspace_list_state: ListState::new(
                0,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT),
                    KNOWLEDGE_WORKSPACE_SECTION_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
        }
    }

    fn status(&self, cx: &App) -> KnowledgeStatus {
        let page = self.bindings.page.read(cx);
        KnowledgeStatus {
            collection: page.knowledge_selected_collection_id().map(str::to_owned),
            embedding: page.knowledge_embedding_progress().is_some(),
            reindex: page.knowledge_reindex_progress().is_some(),
            error: page.knowledge_error().map(str::to_owned),
        }
    }

    fn select_collection(&self, id: String, cx: &mut App) {
        self.bindings.page.update(cx, |page, cx| {
            page.select_knowledge_collection(id);
            cx.notify();
        });
    }

    fn rag_store(&self, cx: &App) -> Arc<oxideterm_ai::RagStore> {
        self.bindings.page.read(cx).rag_store()
    }

    pub fn set_cache_visible(&self, visible: bool, cx: &mut App) {
        self.state.update(cx, |state, cx| {
            state.cache_visible = visible;
            if let Some(editor) = state.editor.as_ref() {
                editor.update(cx, |editor, cx| editor.set_cache_visible(visible, cx));
            }
        });
    }

    pub fn text_editor_focused(&self, window: &Window, cx: &App) -> bool {
        let state = self.state.read(cx);
        state
            .menu
            .as_ref()
            .is_some_and(|menu| menu.focus.is_focused(window))
            || state.editor.as_ref().is_some_and(|editor| {
                let editor = editor.read(cx);
                editor.source_editable()
                    && editor.editor.read(cx).focus_handle(cx).is_focused(window)
            })
    }

    pub fn set_input_value(
        &mut self,
        input: KnowledgeInput,
        value: String,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| match input {
            KnowledgeInput::Search => state.navigator_query = value.into(),
            KnowledgeInput::Rename if state.metadata_task.is_none() => {
                if let Some(rename) = state.rename.as_mut() {
                    rename.name = value;
                }
            }
            _ => {}
        });
        if input == KnowledgeInput::Search {
            self.queue_knowledge_search(cx);
        }
        cx.notify();
    }

    fn render_text_input(
        &self,
        input: KnowledgeInput,
        value: &str,
        placeholder: String,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(frame) = (self.bindings.frame)(input, cx) else {
            return div().into_any_element();
        };
        let field = text_input_with_viewport_and_content_align(
            &self.tokens,
            TextInputView {
                value,
                placeholder,
                focused: frame.focused,
                caret_visible: frame.caret_visible,
                secret: false,
                selected_all: false,
                selected_range: frame.selected_range,
                marked_text: frame.marked_text.as_deref(),
            },
            TextInputContentAlign::Start,
            &frame.viewport,
            frame.active_offset,
        )
        .when(input == KnowledgeInput::Search, |field| {
            field
                .flex_1()
                .min_w_0()
                .px_0()
                .border_0()
                .rounded_none()
                .bg(gpui::transparent_black())
        })
        .cursor(gpui::CursorStyle::IBeam)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                (this.bindings.input)(
                    KnowledgeInputRequest::PointerDown(input, event.clone()),
                    window,
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
            (this.bindings.input)(KnowledgeInputRequest::Drag(event.clone()), window, cx);
        }));
        text_input_anchor_probe(frame.anchor_id, field, move |anchor, _, _| {
            (frame.update_anchor)(anchor)
        })
        .into_any_element()
    }

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: gpui::Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    fn workspace_icon_action_button(
        &self,
        icon: LucideIcon,
        size: f32,
        color: gpui::Rgba,
        options: IconButtonOptions,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &Context<Self>,
    ) -> Div {
        let enabled = !options.disabled && !options.loading;
        oxideterm_gpui_ui::icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, size, color),
            options,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event, window, cx| {
                if enabled {
                    listener(this, event, window, cx);
                } else {
                    cx.stop_propagation();
                }
            }),
        )
    }

    fn knowledge_document_row(
        &self,
        document: oxideterm_ai::RagDocumentResponse,
        cx: &Context<Self>,
    ) -> AnyElement {
        let id = document.id.clone();
        let delete_id = id.clone();
        let note_menu = document.clone();
        let selected = self.is_knowledge_document_selected(&id, cx);
        let delete = self.knowledge_navigator_action(
            "knowledge-document-delete",
            LucideIcon::Trash2,
            self.i18n.t("common.actions.delete"),
            false,
            false,
            move |this, _, window, cx| {
                this.confirm_knowledge_delete(false, delete_id.clone(), document.title.clone(), window, cx);
                cx.stop_propagation();
            },
            cx,
        );
        entity_list_row(
            &self.tokens,
            EntityListRowOptions::new()
                .active(selected)
                .has_background_image(self.has_background)
                .compact()
                .hover_background(color_for_background(
                    self.tokens.ui.bg_hover,
                    self.has_background,
                    0x66,
                )),
            Some(
                oxideterm_gpui_ui::file_icons::file_icon(&format!("note.{}", document.format))
                    .render(14.0, &self.tokens),
            ),
            div()
                .min_w_0()
                .truncate()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .child(note_menu.title.clone())
                .into_any_element(),
            None,
            Vec::new(),
            vec![delete],
        )
        .id(format!("knowledge-document-row-{id}"))
        .h(px(KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT))
        .min_h(px(KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT))
        .py_0()
        .px(px(self.tokens.spacing.two))
        .rounded_none()
        .border_0()
        .bg(if selected {
            rgba((self.tokens.ui.accent << 8) | 0x33)
        } else {
            rgba(0)
        })
        .opacity(if self.knowledge_note_is_cut(&id, cx) {
            0.5
        } else {
            1.0
        })
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                this.open_knowledge_note_menu(note_menu.clone(), event.position, window, cx);
                cx.stop_propagation();
            }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                this.select_knowledge_document(id.clone(), cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn knowledge_empty_row(&self, icon: LucideIcon, label: String, _: &App) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .px(px(16.0))
            .py(px(32.0))
            .text_center()
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(Self::render_lucide_icon(
                icon,
                32.0,
                rgba((self.tokens.ui.text_muted << 8) | 0x66),
            ))
            .child(
                div()
                    .w_full()
                    .text_center()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .child(label),
            )
            .into_any_element()
    }

    fn knowledge_error_row(&self, message: &str) -> AnyElement {
        div()
            .p(px(self.tokens.spacing.two))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.error))
            .child(message.to_owned())
            .into_any_element()
    }

    fn resize_capture(&self, cx: &Context<Self>) -> AnyElement {
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .cursor(gpui::CursorStyle::ResizeColumn)
            .occlude()
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_knowledge_resize(event, window, cx);
                cx.stop_propagation();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.finish_knowledge_resize(cx);
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
    fn knowledge_navigator_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let query = self.state.read(cx).navigator_query.clone();
        let input = self.render_text_input(
            KnowledgeInput::Search,
            &query,
            self.i18n
                .t("settings_view.knowledge.navigator_search_placeholder"),
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
        cx: &Context<Self>,
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

    pub fn knowledge_leave_confirmation_open(&self, cx: &App) -> bool {
        let knowledge = self.state.read(cx);
        knowledge.pending_document_id.is_some()
            || knowledge.pending_collection_id.is_some()
            || knowledge.pending_close.is_some()
            || knowledge.pending_app_quit
    }

    pub fn handle_knowledge_leave_confirmation_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match event.keystroke.key.as_str() {
            "escape" => {
                let pending_close = self.state.read(cx).pending_close.is_some();
                let pending_app_quit = self.state.read(cx).pending_app_quit;
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

    pub fn is_knowledge_document_selected(&self, document_id: &str, cx: &App) -> bool {
        self.state.read(cx).selected_document_id.as_deref() == Some(document_id)
    }

    fn knowledge_editor_labels(&self) -> KnowledgeEditorLabels {
        KnowledgeEditorLabels::new(&self.i18n)
    }

    pub fn refresh_knowledge_navigator(&mut self, force: bool, cx: &mut Context<Self>) {
        let generation = self.state.update(cx, |knowledge, _cx| {
            knowledge.begin_navigator_refresh(force)
        });
        let Some(generation) = generation else {
            return;
        };
        let store = self.rag_store(cx);
        let preferred_collection_id = self.status(cx).collection;
        let query = self.state.read(cx).navigator_query.clone();
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
                if workspace.state.read(cx).navigator_query != query
                    || workspace.status(cx).collection.as_deref() != requested_collection.as_deref()
                {
                    workspace.state.update(cx, |state, _| {
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
                let refresh_again = workspace.state.update(cx, |knowledge, _cx| {
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

    pub fn select_knowledge_document(&mut self, document_id: String, cx: &mut Context<Self>) {
        let (selected_document_id, current_editor, loading) = {
            let knowledge = self.state.read(cx);
            (
                knowledge.selected_document_id.clone(),
                knowledge.editor.clone(),
                knowledge.loading,
            )
        };
        if selected_document_id.as_deref() == Some(document_id.as_str())
            && (current_editor.is_some() || loading)
        {
            self.state
                .update(cx, |state, _| state.mobile_navigator_open = false);
            cx.notify();
            return;
        }
        if current_editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).is_dirty())
        {
            self.state.update(cx, |knowledge, _cx| {
                knowledge.request_document_switch(document_id);
            });
            cx.notify();
            return;
        }
        let generation = self.state.update(cx, |workspace, _cx| {
            workspace.begin_document_load(document_id.clone())
        });
        let store = self.rag_store(cx);
        let labels = self.knowledge_editor_labels();
        let tokens = self.tokens;
        let has_background_image = self.has_background;
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
                        let Some(environment) = (workspace.bindings.environment)(cx) else {
                            return;
                        };
                        let preview = workspace.bindings.preview.clone();
                        editor.update(cx, |editor, cx| {
                            editor.initialize(environment.preferences, preview, cx)
                        });
                        KnowledgeDocumentEditor::configure_save_callback(&editor, cx);
                        let subscription = cx.subscribe(
                            &editor,
                            |workspace, _editor, event: &KnowledgeDocumentEditorEvent, cx| {
                                if let KnowledgeDocumentEditorEvent::PreferencesChanged(
                                    preferences,
                                ) = event
                                {
                                    (workspace.bindings.request)(
                                        KnowledgeRequest::SaveEditorPreferences(
                                            preferences.clone(),
                                        ),
                                        None,
                                        cx,
                                    );
                                    return;
                                }
                                workspace.refresh_knowledge_navigator(true, cx);
                                let notebook = workspace.state.update(cx, |state, _| {
                                    if state.switch_after_save {
                                        state.pending_collection_id.take()
                                    } else {
                                        None
                                    }
                                });
                                if let Some(id) = notebook {
                                    workspace.change_knowledge_notebook(id, true, cx);
                                }
                                let pending = workspace.state.update(cx, |knowledge, _cx| {
                                    knowledge.take_pending_document_after_save()
                                });
                                if let Some(document_id) = pending {
                                    workspace.select_knowledge_document(document_id, cx);
                                }
                                let pending_close = workspace.state.update(cx, |knowledge, _cx| {
                                    knowledge.take_pending_close_after_save()
                                });
                                if let Some((tab_id, window_handle)) = pending_close {
                                    (workspace.bindings.request)(
                                        KnowledgeRequest::Close(tab_id, window_handle),
                                        None,
                                        cx,
                                    );
                                }
                                let quit_after_save =
                                    workspace.state.update(cx, |knowledge, _cx| {
                                        knowledge.take_pending_app_quit_after_save()
                                    });
                                if quit_after_save {
                                    (workspace.bindings.request)(KnowledgeRequest::Quit, None, cx);
                                }
                            },
                        );
                        workspace.state.update(cx, |knowledge, _cx| {
                            knowledge.install_document(
                                generation,
                                &document_id,
                                editor,
                                subscription,
                            );
                        });
                        let visible = workspace.state.read(cx).cache_visible;
                        workspace.set_cache_visible(visible, cx);
                    }
                    Err(error) => {
                        workspace.state.update(cx, |knowledge, _cx| {
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
        self.state
            .update(cx, |knowledge, _cx| knowledge.cancel_document_switch());
        cx.notify();
    }

    fn discard_and_switch_knowledge_document(&mut self, cx: &mut Context<Self>) {
        let notebook = self
            .state
            .update(cx, |state, _| state.pending_collection_id.take());
        if let Some(id) = notebook {
            self.change_knowledge_notebook(id, true, cx);
            return;
        }
        let pending = self
            .state
            .update(cx, |knowledge, _cx| knowledge.take_pending_document_now());
        if let Some(document_id) = pending {
            // Removing the current editor drops its draft before the selected identifier changes.
            self.state.update(cx, |knowledge, _cx| {
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
        let editor = self.state.read(cx).editor.clone();
        let is_dirty = editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).is_dirty());
        if !is_dirty {
            let pending_document = self
                .state
                .update(cx, |knowledge, _cx| knowledge.take_pending_document_now());
            if let Some(document_id) = pending_document {
                self.select_knowledge_document(document_id, cx);
                return;
            }
            let pending_close = self
                .state
                .update(cx, |knowledge, _cx| knowledge.take_pending_close_now());
            if let Some((tab_id, _window_handle)) = pending_close {
                (self.bindings.request)(
                    KnowledgeRequest::Close(tab_id, window.window_handle()),
                    None,
                    cx,
                );
                return;
            }
            let quit = self
                .state
                .update(cx, |knowledge, _cx| knowledge.take_pending_app_quit_now());
            if quit {
                (self.bindings.request)(KnowledgeRequest::Quit, None, cx);
            }
            return;
        }
        self.state.update(cx, |knowledge, _cx| {
            knowledge.confirm_save_before_leaving();
        });
        if let Some(editor) = editor {
            editor.update(cx, |editor, cx| editor.save_current_draft(cx));
        }
    }

    fn cancel_knowledge_tab_close(&mut self, cx: &mut Context<Self>) {
        self.state
            .update(cx, |knowledge, _cx| knowledge.cancel_close());
        cx.notify();
    }

    fn cancel_knowledge_app_quit(&mut self, cx: &mut Context<Self>) {
        self.state
            .update(cx, |knowledge, _cx| knowledge.cancel_app_quit());
        cx.notify();
    }

    fn discard_and_close_knowledge_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pending = self
            .state
            .update(cx, |knowledge, _cx| knowledge.take_pending_close_now());
        if let Some((tab_id, _window_handle)) = pending {
            self.state.update(cx, |knowledge, _cx| {
                knowledge.editor = None;
                knowledge._editor_subscription = None;
            });
            (self.bindings.request)(
                KnowledgeRequest::Close(tab_id, window.window_handle()),
                None,
                cx,
            );
        }
    }

    fn discard_and_quit_with_knowledge_draft(&mut self, cx: &mut Context<Self>) {
        let quit = self
            .state
            .update(cx, |knowledge, _cx| knowledge.take_pending_app_quit_now());
        if quit {
            (self.bindings.request)(KnowledgeRequest::Quit, None, cx);
        }
    }

    pub fn render_knowledge_workspace_surface(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let available_width = self
            .state
            .read(cx)
            .surface_width
            .unwrap_or_else(|| f32::from(window.viewport_size().width));
        let narrow_layout = available_width < KNOWLEDGE_NARROW_VIEWPORT_WIDTH;
        if !self.tokens.motion.enabled {
            self.state.update(cx, |state, _| {
                state.navigator_closing = false;
                state.navigator_motion_task = None;
            });
        }
        let (show_navigator, navigator_width) = {
            let state = self.state.read(cx);
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
        let has_background_image = self.has_background;
        let embedding_running = self.status(cx).embedding;
        let embedding_finished = self.state.update(cx, |state, _| {
            let finished = state.last_embedding_running && !embedding_running;
            state.last_embedding_running = embedding_running;
            finished
        });
        if let Some(editor) = self.state.read(cx).editor.clone() {
            editor.update(cx, |editor, cx| {
                editor.set_theme(self.tokens, cx);
                editor.set_has_background_image(has_background_image, cx);
                if embedding_finished {
                    editor.start_index_state_poll(cx);
                }
            });
        }
        let labels = self.knowledge_editor_labels();
        let navigator_snapshot = self.state.read(cx).navigator_snapshot.clone();
        let selected_collection = navigator_snapshot.selected_collection;
        let documents = navigator_snapshot.documents;
        let navigator_query = self.state.read(cx).navigator_query.clone();
        let filtered_documents = documents;
        let navigator_error = if let Some(error) = self.state.read(cx).metadata_error.clone() {
            Some(error)
        } else if let Some(error) = self.status(cx).error {
            Some(error.to_owned())
        } else if navigator_snapshot.error.is_some() {
            Some(labels.navigator_load_failed.clone())
        } else {
            (!navigator_snapshot.loaded).then(|| labels.loading.clone())
        };
        let (editor, loading, error, pending_switch, pending_close, pending_app_quit) = {
            let knowledge = self.state.read(cx);
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
        self.state.update(cx, |state, _| {
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
                                    .map(|document| workspace.knowledge_document_row(document, cx))
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
                .debug_selector(|| "knowledge-leave-confirmation".into())
                .w(px(440.0))
                .max_w(gpui::relative(0.92))
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
            .bg(color_for_background(
                self.tokens.ui.bg,
                has_background_image,
                0x00,
            ))
            .on_children_prepainted({
                let knowledge = self.state.clone();
                move |bounds, _, cx| {
                    if let Some(bounds) = bounds.first() {
                        let width = f32::from(bounds.size.width);
                        if knowledge.read(cx).surface_width != Some(width) {
                            knowledge.update(cx, |state, cx| {
                                state.surface_width = Some(width);
                                cx.notify();
                            });
                        }
                    }
                }
            })
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                (this.bindings.input)(
                    KnowledgeInputRequest::DismissSearchOutside(event.position),
                    window,
                    cx,
                );
            }))
            .child(self.knowledge_workspace_header(narrow_layout, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(
                        show_navigator || self.state.read(cx).navigator_closing,
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
            .when(self.knowledge_resize_active(cx), |content| {
                content.child(self.resize_capture(cx))
            })
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

    pub fn handle_knowledge_workspace_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.text_editor_focused(window, cx) {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let command = event.keystroke.modifiers.platform || event.keystroke.modifiers.control;
        if command && matches!(key, "c" | "x" | "v") {
            if key == "v" {
                self.paste_knowledge_note(cx);
            } else if let Some(id) = self.state.read(cx).selected_document_id.clone() {
                self.copy_knowledge_note(id, key == "x", cx);
            }
            return true;
        }
        if command && key == "f" {
            self.state.update(cx, |state, _| {
                state.navigator_hidden = false;
                state.mobile_navigator_open = true;
            });
            (self.bindings.input)(
                KnowledgeInputRequest::Focus(KnowledgeInput::Search),
                window,
                cx,
            );
            cx.notify();
            return true;
        }
        let direction = match key {
            "up" | "arrowup" => -1,
            "down" | "arrowdown" => 1,
            _ => return false,
        };
        let (visible, selected_document_id) = {
            let knowledge = self.state.read(cx);
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
}

fn knowledge_document_matches(title: &str, content: &str, terms: &[String]) -> bool {
    let searchable = format!("{title}\n{content}").to_lowercase();
    terms.iter().all(|term| searchable.contains(term))
}

#[cfg(test)]
mod tests;

impl KnowledgeWorkspaceView {
    fn confirm_knowledge_delete(&mut self, notebook: bool, id: String, title: String, window: &mut Window, cx: &mut Context<Self>) {
        self.bindings.page.update(cx, |page, cx| {
            if notebook {
                page.request_delete_knowledge_collection(id, title);
            } else {
                page.request_delete_knowledge_document(id, title);
            }
            cx.notify();
        });
        (self.bindings.request)(KnowledgeRequest::FocusDeleteConfirmation, Some(window), cx);
    }
}
