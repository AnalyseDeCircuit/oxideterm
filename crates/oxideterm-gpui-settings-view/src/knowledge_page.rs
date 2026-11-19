use crate::{
    SettingsInput, SettingsSelect,
    components::{SettingsCalloutTone, SettingsControls},
    surface::nav::render_icon,
};
use gpui::{
    AnyElement, App, Div, MouseButton, MouseDownEvent, Pixels, Point, Rgba, Window, div,
    prelude::*, px, rgb, rgba,
};
use oxideterm_ai::RagDocumentResponse;
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
    ToolbarButtonOptions, icon_button, toolbar_button,
};
use oxideterm_i18n::Locale;
use oxideterm_settings_model::ai_provider_views;
use std::{ops::Deref, rc::Rc};

pub mod dialog;
mod section;
pub use section::{KnowledgeSection, selected_collection};

const KNOWLEDGE_ACTION_BUTTON_HEIGHT: f32 = 28.0; // Tauri size="sm" outline action buttons.
const KNOWLEDGE_DOCUMENT_HEADER_INFO_MIN_WIDTH: f32 = 160.0; // Keep collection identity readable before action chips wrap.
const KNOWLEDGE_DOCUMENT_ACTION_GROUP_MIN_WIDTH: f32 = 280.0; // Wrap document actions inside the card before they overflow.
const KNOWLEDGE_ICON_BUTTON_SIZE: f32 = 28.0; // Tauri h-7 w-7 document row buttons.
const KNOWLEDGE_INLINE_ICON_SIZE: f32 = 14.0; // Tauri h-3.5 w-3.5 action icons.
const KNOWLEDGE_ROW_ICON_SIZE: f32 = 16.0; // Tauri h-4 w-4 row icons.
const KNOWLEDGE_EMBEDDING_ICON_BOX: f32 = 32.0; // Tauri h-8 w-8 semantic search icon box.
const KNOWLEDGE_EMBEDDING_CONFIG_BUTTON_HEIGHT: f32 = 32.0; // Tauri h-8 configure button.
const KNOWLEDGE_STATUS_BORDER_ALPHA: u32 = 0x33; // Tauri border-current/20.
const KNOWLEDGE_STATUS_BG_ALPHA: u32 = 0x1a; // Tauri bg-current/10.
const KNOWLEDGE_ICON_BUTTON_HOVER_ALPHA: u32 = 0x0d; // Tauri hover:bg-theme-bg-hover/5.

#[derive(Clone)]
pub enum KnowledgeSettingsRequest {
    CreateCollection,
    ImportFiles(String),
    NewDocument(String),
    Generate(String),
    ToggleReindex(String),
    ShowPage(String, usize),
    SelectCollection(String),
    DeleteCollection(String, String),
    SyncExternal,
    OpenExternal(String),
    DeleteDocument(String, String),
    NoteMenu(RagDocumentResponse),
    OpenInWorkspace(String),
    ToggleEmbedding,
}

pub struct KnowledgeSettingsFrame<'a> {
    pub page_size: usize,
    pub compact_row_height: f32,
    pub has_background: bool,
    pub import_progress: Option<(usize, usize)>,
    pub embedding_progress: Option<(usize, usize)>,
    pub reindex_progress: Option<(usize, usize)>,
    pub embedding_config_expanded: bool,
    pub external_edit_id: Option<&'a str>,
    pub embedding_model: &'a str,
}

pub struct KnowledgeSettingsPage<'a> {
    pub controls: SettingsControls<'a>,
    pub frame: KnowledgeSettingsFrame<'a>,
    pub dispatch: Rc<dyn Fn(KnowledgeSettingsRequest, Point<Pixels>, &mut Window, &mut App)>,
    pub provider_has_key: &'a dyn Fn(&str, &App) -> bool,
    pub document_selected: &'a dyn Fn(&str, &App) -> bool,
    pub document_cut: &'a dyn Fn(&str, &App) -> bool,
    pub chevron: &'a dyn Fn(bool) -> AnyElement,
}

impl<'a> Deref for KnowledgeSettingsPage<'a> {
    type Target = SettingsControls<'a>;
    fn deref(&self) -> &Self::Target {
        &self.controls
    }
}

impl KnowledgeSettingsPage<'_> {
    fn press(
        &self,
        request: KnowledgeSettingsRequest,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |event, window, cx| {
            dispatch(request.clone(), event.position, window, cx);
            cx.stop_propagation();
        }
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let enabled = !(options.button.disabled || options.loading);
        toolbar_button(self.tokens, label, icon, options).on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                if enabled {
                    press(event, window, cx);
                } else {
                    cx.stop_propagation();
                }
            },
        )
    }

    fn workspace_icon_action_button(
        &self,
        icon: &'static str,
        size: f32,
        color: Rgba,
        options: IconButtonOptions,
        press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _: &App,
    ) -> Div {
        let enabled = !(options.disabled || options.loading);
        icon_button(self.tokens, render_icon(icon, size, color), options).on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                if enabled {
                    press(event, window, cx);
                } else {
                    cx.stop_propagation();
                }
            },
        )
    }

    pub fn knowledge_error_row(&self, error: &str) -> AnyElement {
        self.settings_callout(
            SettingsCalloutTone::Error,
            Some("lucide/alert-circle.svg"),
            div().child(error.to_string()).into_any_element(),
        )
    }

    pub fn knowledge_collections_card(
        &self,
        collections: &[oxideterm_ai::RagCollectionResponse],
        selected_id: Option<&str>,
        cx: &App,
    ) -> AnyElement {
        let mut rows = vec![
            div()
                .flex()
                .items_center()
                .justify_end()
                .child(
                    // Tauri DocumentManager renders this as an outline small
                    // Button with a leading Plus icon. Route activation through
                    // the workspace wrapper so collection creation shares the
                    // same guarded Button path as document actions.
                    self.workspace_toolbar_action_button(
                        self.i18n.t("settings_view.knowledge.create_collection"),
                        Some(render_icon(
                            "lucide/plus.svg",
                            KNOWLEDGE_INLINE_ICON_SIZE,
                            rgb(self.tokens.ui.text),
                        )),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        self.press(KnowledgeSettingsRequest::CreateCollection),
                    )
                    .into_any_element(),
                )
                .into_any_element(),
        ];
        if collections.is_empty() {
            rows.push(self.knowledge_empty_row(
                "lucide/book-open.svg",
                self.i18n.t("settings_view.knowledge.no_collections"),
                cx,
            ));
        } else {
            for collection in collections {
                rows.push(self.knowledge_collection_row(collection, selected_id, cx));
            }
        }
        self.settings_card(
            "settings_view.knowledge.collections",
            "settings_view.knowledge.create_description",
            rows,
        )
    }

    pub fn knowledge_documents_card(
        &self,
        collection: &oxideterm_ai::RagCollectionResponse,
        documents: oxideterm_ai::RagPaginatedDocuments,
        page_index: usize,
        stats: Option<oxideterm_ai::RagStatsResponse>,
        include_document_rows: bool,
        cx: &App,
    ) -> AnyElement {
        let reindex_collection_id = collection.id.clone();
        let import_collection_id = collection.id.clone();
        let embedding_collection_id = collection.id.clone();
        let total = documents.total;
        let page_size = self.frame.page_size;
        let page_count = total.div_ceil(page_size).max(1);
        let documents = documents.documents;
        let create_document_collection_id = collection.id.clone();
        let import_progress = self.frame.import_progress;
        let embedding_progress = self.frame.embedding_progress;
        let import_label = import_progress
            .map(|(current, total)| format!("{current}/{total}"))
            .unwrap_or_else(|| self.i18n.t("settings_view.knowledge.import_files"));
        let embedding_label = embedding_progress
            .map(|(current, total)| format!("{current}/{total}"))
            .unwrap_or_else(|| self.i18n.t("settings_view.knowledge.generate_embeddings"));
        let reindex_progress = self.frame.reindex_progress;
        let reindex_label = reindex_progress
            .map(|(current, total)| {
                if total == 0 {
                    self.i18n.t("settings_view.knowledge.reindex")
                } else {
                    format!("{current}/{total}")
                }
            })
            .unwrap_or_else(|| self.i18n.t("settings_view.knowledge.reindex"));
        let mut rows = vec![
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex_basis(px(KNOWLEDGE_DOCUMENT_HEADER_INFO_MIN_WIDTH))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(self.tokens.ui.text))
                                .child(collection.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.knowledge_scope_label(&collection.scope)),
                        ),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .max_w_full()
                        .flex_1()
                        .flex_basis(px(KNOWLEDGE_DOCUMENT_ACTION_GROUP_MIN_WIDTH))
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_end()
                        .gap(px(8.0))
                        .child({
                            let import_disabled = import_progress.is_some();
                            self.knowledge_text_icon_button(
                                "lucide/folder-open.svg",
                                import_label,
                                import_disabled,
                                self.press(KnowledgeSettingsRequest::ImportFiles(
                                    import_collection_id,
                                )),
                            )
                        })
                        .child(self.knowledge_text_icon_button(
                            "lucide/file-plus.svg",
                            self.i18n.t("settings_view.knowledge.new_document"),
                            false,
                            self.press(KnowledgeSettingsRequest::NewDocument(
                                create_document_collection_id,
                            )),
                        ))
                        .child({
                            let embedding_disabled = embedding_progress.is_some();
                            self.knowledge_text_icon_button(
                                "lucide/sparkles.svg",
                                embedding_label,
                                embedding_disabled,
                                self.press(KnowledgeSettingsRequest::Generate(
                                    embedding_collection_id,
                                )),
                            )
                        })
                        .child({
                            let reindex_disabled = matches!(reindex_progress, Some((_current, 0)));
                            self.knowledge_text_icon_button(
                                if reindex_progress.is_some() {
                                    "lucide/x.svg"
                                } else {
                                    "lucide/refresh-cw.svg"
                                },
                                reindex_label,
                                reindex_disabled,
                                self.press(KnowledgeSettingsRequest::ToggleReindex(
                                    reindex_collection_id,
                                )),
                            )
                        }),
                )
                .into_any_element(),
        ];
        if let Some(stats) = stats {
            rows.push(self.knowledge_stats_row(stats, cx));
        }
        if include_document_rows {
            rows.push(self.card_separator());
            if documents.is_empty() {
                rows.push(self.knowledge_empty_row(
                    "lucide/file-text.svg",
                    self.i18n.t("settings_view.knowledge.no_documents"),
                    cx,
                ));
            } else {
                for document in documents {
                    rows.push(self.knowledge_document_row(document, false, cx));
                }
            }
        }
        if page_count > 1 {
            let previous_collection = collection.id.clone();
            let next_collection = collection.id.clone();
            rows.push(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.0))
                    .child(self.knowledge_text_icon_button(
                        "lucide/chevron-left.svg",
                        self.i18n.t("settings_view.knowledge.previous_page"),
                        page_index == 0,
                        self.press(KnowledgeSettingsRequest::ShowPage(
                            previous_collection,
                            page_index.saturating_sub(1),
                        )),
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(
                                self.i18n
                                    .t("settings_view.knowledge.page_summary")
                                    .replace("{{page}}", &(page_index + 1).to_string())
                                    .replace("{{pages}}", &page_count.to_string())
                                    .replace("{{total}}", &total.to_string()),
                            ),
                    )
                    .child(self.knowledge_text_icon_button(
                        "lucide/chevron-right.svg",
                        self.i18n.t("settings_view.knowledge.next_page"),
                        page_index + 1 >= page_count,
                        self.press(KnowledgeSettingsRequest::ShowPage(
                            next_collection,
                            page_index + 1,
                        )),
                    ))
                    .into_any_element(),
            );
        }
        self.settings_card("settings_view.knowledge.navigator_documents", "", rows)
    }

    pub fn knowledge_collection_row(
        &self,
        collection: &oxideterm_ai::RagCollectionResponse,
        selected_id: Option<&str>,
        cx: &App,
    ) -> AnyElement {
        let selected = selected_id == Some(collection.id.as_str());
        let collection_id = collection.id.clone();
        let delete_id = collection.id.clone();
        let delete_name = collection.name.clone();
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if selected {
                rgba((self.tokens.ui.accent << 8) | 0x4d)
            } else {
                rgba(0x00000000)
            })
            .bg(if selected {
                rgba((self.tokens.ui.accent << 8) | 0x1a)
            } else {
                rgba(0x00000000)
            })
            .px(px(12.0))
            .py(px(8.0))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .on_mouse_down(
                MouseButton::Left,
                self.press(KnowledgeSettingsRequest::SelectCollection(collection_id)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(div().flex_none().child(render_icon(
                        "lucide/book-open.svg",
                        KNOWLEDGE_ROW_ICON_SIZE,
                        rgb(self.tokens.ui.text_muted),
                    )))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .truncate()
                                    .child(collection.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .truncate()
                                    .child(format!(
                                        "{} · {}",
                                        self.knowledge_scope_label(&collection.scope),
                                        self.knowledge_format_date(collection.updated_at)
                                    )),
                            ),
                    ),
            )
            .child(div().flex_none().child(self.knowledge_icon_button(
                "lucide/trash-2.svg",
                rgb(self.tokens.ui.text_muted),
                Some(rgb(self.tokens.ui.error)),
                self.press(KnowledgeSettingsRequest::DeleteCollection(
                    delete_id,
                    delete_name,
                )),
                cx,
            )))
            .into_any_element()
    }

    pub fn knowledge_text_icon_button(
        &self,
        icon: &'static str,
        label: String,
        disabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        // Knowledge action chips match Tauri's small outline buttons. Use the
        // workspace action wrapper so disabled cursor/loading/focus-visible
        // behavior stays aligned with the rest of settings.
        self.workspace_toolbar_action_button(
            label,
            Some(render_icon(
                icon,
                KNOWLEDGE_INLINE_ICON_SIZE,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                height: Some(KNOWLEDGE_ACTION_BUTTON_HEIGHT),
                padding_x: Some(10.0),
                font_size: Some(self.tokens.metrics.ui_text_xs),
                background: Some(rgb(self.tokens.ui.bg)),
                border: Some(rgb(self.tokens.ui.border)),
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
    }

    pub fn knowledge_icon_button(
        &self,
        icon: &'static str,
        color: gpui::Rgba,
        hover_color: Option<gpui::Rgba>,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &App,
    ) -> Div {
        // The original local helper accepted a hover text color, but the icon
        // SVG is rendered with an explicit color. Keep the parameter until the
        // shared icon primitive grows a real hover-icon-color slot.
        let _ = hover_color;
        self.workspace_icon_action_button(
            icon,
            KNOWLEDGE_INLINE_ICON_SIZE,
            color,
            IconButtonOptions {
                hover_background: Some(rgba((0xffffff << 8) | KNOWLEDGE_ICON_BUTTON_HOVER_ALPHA)),
                ..IconButtonOptions::opaque_toolbar(KNOWLEDGE_ICON_BUTTON_SIZE, ButtonRadius::Sm)
            },
            listener,
            cx,
        )
    }

    pub fn knowledge_document_row(
        &self,
        document: oxideterm_ai::RagDocumentResponse,
        open_in_workspace: bool,
        cx: &App,
    ) -> AnyElement {
        let menu_document = document.clone();
        let delete_id = document.id.clone();
        let delete_name = document.title.clone();
        let edit_id = document.id.clone();
        let open_id = document.id.clone();
        let selected = open_in_workspace && (self.document_selected)(&document.id, cx);
        let metadata = format!(
            "{} · {} {} · {}",
            document.format,
            document.chunk_count,
            self.i18n.t("settings_view.knowledge.chunks"),
            self.knowledge_format_date(document.indexed_at)
        );
        let editing_this =
            !open_in_workspace && self.frame.external_edit_id == Some(document.id.as_str());
        let mut options = oxideterm_gpui_ui::EntityListRowOptions::new()
            .active(selected)
            .has_background_image(self.frame.has_background);
        if open_in_workspace {
            options = options
                .compact()
                .hover_background(oxideterm_gpui_ui::color_for_background(
                    self.tokens.ui.bg_hover,
                    self.frame.has_background,
                    0x66,
                ));
        }
        let mut trailing = Vec::with_capacity(2);
        if !open_in_workspace {
            trailing.push(
                if editing_this {
                    self.knowledge_icon_button(
                        "lucide/refresh-cw.svg",
                        rgb(self.tokens.ui.accent),
                        Some(rgb(self.tokens.ui.accent)),
                        self.press(KnowledgeSettingsRequest::SyncExternal),
                        cx,
                    )
                } else {
                    self.knowledge_icon_button(
                        "lucide/pencil.svg",
                        rgb(self.tokens.ui.text_muted),
                        Some(rgb(self.tokens.ui.text)),
                        self.press(KnowledgeSettingsRequest::OpenExternal(edit_id)),
                        cx,
                    )
                }
                .into_any_element(),
            );
        }
        trailing.push(
            self.knowledge_icon_button(
                "lucide/trash-2.svg",
                rgb(self.tokens.ui.text_muted),
                Some(rgb(self.tokens.ui.error)),
                self.press(KnowledgeSettingsRequest::DeleteDocument(
                    delete_id,
                    delete_name,
                )),
                cx,
            )
            .into_any_element(),
        );
        let row = oxideterm_gpui_ui::entity_list_row(
            &self.tokens,
            options,
            Some(
                oxideterm_gpui_ui::file_icons::file_icon(&format!("note.{}", document.format))
                    .render(
                        if open_in_workspace {
                            KNOWLEDGE_INLINE_ICON_SIZE
                        } else {
                            KNOWLEDGE_ROW_ICON_SIZE
                        },
                        &self.tokens,
                    ),
            ),
            div()
                .min_w(px(0.0))
                .truncate()
                .text_size(px(if open_in_workspace {
                    self.tokens.metrics.ui_text_xs
                } else {
                    self.tokens.metrics.ui_text_sm
                }))
                .text_color(rgb(self.tokens.ui.text))
                .child(document.title)
                .into_any_element(),
            (!open_in_workspace).then(|| {
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(metadata)
                    .into_any_element()
            }),
            Vec::new(),
            trailing,
        )
        .id(format!("knowledge-document-row-{}", document.id))
        .when(open_in_workspace, |row| {
            row.h(px(self.frame.compact_row_height))
                .min_h(px(self.frame.compact_row_height))
                .py_0()
                .px(px(self.tokens.spacing.two))
                .rounded_none()
                .border_0()
                .bg(if selected {
                    rgba((self.tokens.ui.accent << 8) | 0x33)
                } else {
                    rgba(0x00000000)
                })
                .opacity(if (self.document_cut)(&menu_document.id, cx) {
                    0.5
                } else {
                    1.0
                })
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Right,
                    self.press(KnowledgeSettingsRequest::NoteMenu(menu_document)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    self.press(KnowledgeSettingsRequest::OpenInWorkspace(open_id)),
                )
        });
        row.into_any_element()
    }

    pub fn knowledge_embedding_config_section(&self, cx: &App) -> AnyElement {
        let settings = self.settings;
        let preliminary = oxideterm_ai::resolve_ai_embedding_provider(
            &settings.ai.providers,
            settings.ai.active_provider_id.as_deref(),
            settings.ai.embedding_config.as_ref(),
            None,
        );
        let has_api_key = preliminary.provider.as_ref().and_then(|provider| {
            oxideterm_ai::ai_embedding_requires_api_key(provider)
                .then(|| (self.provider_has_key)(&provider.id, cx))
        });
        let resolved = oxideterm_ai::resolve_ai_embedding_provider(
            &settings.ai.providers,
            settings.ai.active_provider_id.as_deref(),
            settings.ai.embedding_config.as_ref(),
            has_api_key,
        );
        let provider_label = settings
            .ai
            .embedding_config
            .as_ref()
            .and_then(|config| config.get("providerId"))
            .and_then(serde_json::Value::as_str)
            .and_then(|provider_id| {
                ai_provider_views(settings)
                    .into_iter()
                    .find(|provider| provider.id == provider_id)
                    .map(|provider| provider.name)
            })
            .unwrap_or_else(|| {
                self.i18n
                    .t("settings_view.knowledge.auto_embedding_provider")
            });
        let model_value = self.frame.embedding_model;
        let status = match resolved.reason {
            oxideterm_ai::AiEmbeddingProviderReason::Ready => resolved
                .provider
                .as_ref()
                .map(|provider| {
                    self.i18n
                        .t("settings_view.knowledge.semantic_search_using")
                        .replace("{{provider}}", &provider.name)
                        .replace("{{model}}", &resolved.model)
                })
                .unwrap_or_else(|| {
                    self.i18n
                        .t("settings_view.knowledge.semantic_search_not_configured")
                }),
            oxideterm_ai::AiEmbeddingProviderReason::MissingModel => self
                .i18n
                .t("settings_view.knowledge.semantic_search_missing_model"),
            oxideterm_ai::AiEmbeddingProviderReason::MissingApiKey => self
                .i18n
                .t("settings_view.knowledge.embedding_api_key_missing"),
            oxideterm_ai::AiEmbeddingProviderReason::UnsupportedProvider => self
                .i18n
                .t("settings_view.knowledge.embedding_provider_unsupported"),
            oxideterm_ai::AiEmbeddingProviderReason::NoProvider => self
                .i18n
                .t("settings_view.knowledge.semantic_search_not_configured"),
        };
        let status_color = if resolved.reason == oxideterm_ai::AiEmbeddingProviderReason::Ready {
            self.tokens.ui.success
        } else {
            self.tokens.ui.text_muted
        };
        let embeddings_expanded = self.frame.embedding_config_expanded;

        let card = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .p(px(self.tokens.metrics.settings_card_padding))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .items_start()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .size(px(KNOWLEDGE_EMBEDDING_ICON_BOX))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.md))
                                    .bg(rgba(
                                        (self.tokens.ui.accent << 8) | KNOWLEDGE_STATUS_BG_ALPHA,
                                    ))
                                    .child(render_icon(
                                        "lucide/sparkles.svg",
                                        KNOWLEDGE_ROW_ICON_SIZE,
                                        rgb(self.tokens.ui.accent),
                                    )),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(self.tokens.ui.text))
                                            .child(self.i18n.t("settings_view.knowledge.semantic_search")),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .flex_wrap()
                                            .child(
                                                div()
                                                    .rounded_full()
                                                    .border_1()
                                                    .border_color(rgba(
                                                        (status_color << 8)
                                                            | KNOWLEDGE_STATUS_BORDER_ALPHA,
                                                    ))
                                                    .bg(rgba(
                                                        (status_color << 8)
                                                            | KNOWLEDGE_STATUS_BG_ALPHA,
                                                    ))
                                                    .px(px(8.0))
                                                    .py(px(2.0))
                                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                                    .text_color(rgb(status_color))
                                                    .child(status),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                                    .text_color(rgb(self.tokens.ui.text_muted))
                                                    .child(self.i18n.t("settings_view.knowledge.keyword_search_ready")),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .h(px(KNOWLEDGE_EMBEDDING_CONFIG_BUTTON_HEIGHT))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .bg(rgb(self.tokens.ui.bg))
                            .px(px(10.0))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                            .child(render_icon(
                                "lucide/wrench.svg",
                                KNOWLEDGE_INLINE_ICON_SIZE,
                                rgb(self.tokens.ui.text),
                            ))
                            .child(self.i18n.t("settings_view.knowledge.configure_embeddings"))
                            .child((self.chevron)(embeddings_expanded))
                            .on_mouse_down(
                                MouseButton::Left,
                                self.press(KnowledgeSettingsRequest::ToggleEmbedding),
                            ),
                    ),
            )
            .when(embeddings_expanded, |section| {
                section
                    .child(self.card_separator())
                    .child(
                        div()
                            .p(px(self.tokens.metrics.settings_card_padding))
                            .flex()
                            .flex_col()
                            .gap(px(self.tokens.metrics.settings_card_gap))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(
                                        self.i18n
                                            .t("settings_view.knowledge.semantic_search_description"),
                                    ),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(self.tokens.metrics.settings_row_gap))
                                    .child(self.select_setting_row(
                                        "settings_view.ai.embedding_provider",
                                        "",
                                        SettingsSelect::AiEmbeddingProvider,
                                        provider_label,
                                        self.tokens.metrics.settings_select_width,
                                        cx,
                                    ))
                                    .child(self.card_separator())
                                    .child(self.setting_row(
                                        "settings_view.ai.embedding_model",
                                        "",
                                        self.settings_text_input_control(
                                            SettingsInput::AiEmbeddingModel,
                                            model_value,
                                            self.i18n.t("settings_view.ai.embedding_model"),
                                            224.0,
                                            cx,
                                        ),
                                        cx,
                                    )),
                            ),
                    )
            });
        self.settings_card_surface(card, self.tokens.ui.bg_card)
            .into_any_element()
    }

    pub fn knowledge_stats_row(
        &self,
        stats: oxideterm_ai::RagStatsResponse,
        cx: &App,
    ) -> AnyElement {
        let embedded_pct = if stats.chunk_count > 0 {
            ((stats.embedded_chunk_count as f64 / stats.chunk_count as f64) * 100.0).round() as i64
        } else {
            0
        };
        let mut row = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap(px(24.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.knowledge_stat_item(
                stats.doc_count.to_string(),
                self.i18n.t("settings_view.knowledge.stat_docs"),
                cx,
            ))
            .child(self.knowledge_stat_item(
                stats.chunk_count.to_string(),
                self.i18n.t("settings_view.knowledge.stat_chunks"),
                cx,
            ))
            .child(self.knowledge_stat_item(
                format!("{embedded_pct}%"),
                self.i18n.t("settings_view.knowledge.stat_embedded"),
                cx,
            ));
        if stats.last_updated > 0 {
            row = row.child(self.knowledge_stat_item(
                self.knowledge_format_date(stats.last_updated),
                self.i18n.t("settings_view.knowledge.stat_updated"),
                cx,
            ));
        }
        row.into_any_element()
    }

    pub fn knowledge_stat_item(&self, value: String, label: String, cx: &App) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .child((self.label)(
                        "knowledge-stat-value",
                        &label,
                        value.clone(),
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .child((self.label)(
                "knowledge-stat-label",
                &value,
                label,
                self.tokens.ui.text_muted,
                cx,
            ))
            .into_any_element()
    }

    pub fn knowledge_format_date(&self, timestamp_millis: i64) -> String {
        let Some(datetime) = chrono::DateTime::from_timestamp_millis(timestamp_millis) else {
            return "-".to_string();
        };
        let datetime = datetime.with_timezone(&chrono::Local);
        match self.i18n.locale() {
            Locale::ZhCn | Locale::ZhTw => datetime.format("%Y年%-m月%-d日").to_string(),
            _ => datetime.format("%b %-d, %Y").to_string(),
        }
    }

    pub fn knowledge_empty_row(&self, icon: &'static str, label: String, _cx: &App) -> AnyElement {
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
            .child(render_icon(
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

    pub fn knowledge_scope_label(&self, scope: &oxideterm_ai::DocScope) -> String {
        match scope {
            oxideterm_ai::DocScope::Global => self.i18n.t("settings_view.knowledge.scope_global"),
            oxideterm_ai::DocScope::Connection(_) => {
                self.i18n.t("settings_view.knowledge.scope_connection")
            }
        }
    }
}
