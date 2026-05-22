impl WorkspaceApp {
    fn settings_ai(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        self.ensure_ai_provider_key_statuses(cx);
        if ai_execution_profiles_need_normalization(self.settings_store.settings()) {
            self.edit_settings(ai_normalize_execution_profiles, cx);
        }
        vec![self.ai_settings_surface(cx)]
    }

    fn settings_knowledge(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        self.ensure_ai_provider_key_statuses(cx);
        let collections = oxideterm_ai::rag_list_collections(&self.ai_rag_store, None)
            .unwrap_or_default();
        let selected_id = self
            .knowledge_selected_collection_id
            .as_deref()
            .filter(|id| collections.iter().any(|collection| collection.id == *id))
            .map(str::to_string)
            .or_else(|| collections.first().map(|collection| collection.id.clone()));
        let selected_collection = selected_id
            .as_deref()
            .and_then(|id| collections.iter().find(|collection| collection.id == id));
        let selected_documents = selected_id
            .as_deref()
            .and_then(|id| {
                oxideterm_ai::rag_list_documents(&self.ai_rag_store, id, None, Some(100)).ok()
            });
        let selected_stats = selected_id
            .as_deref()
            .and_then(|id| oxideterm_ai::rag_get_collection_stats(&self.ai_rag_store, id).ok());

        let mut rows = vec![self.knowledge_collections_card(&collections, selected_id.as_deref(), cx)];
        if let Some(error) = self.knowledge_error.as_ref() {
            rows.insert(0, self.knowledge_error_row(error));
        }
        if let Some(collection) = selected_collection {
            rows.push(self.knowledge_documents_card(
                collection,
                selected_documents,
                selected_stats,
                cx,
            ));
        }
        rows
    }

    fn knowledge_error_row(&self, error: &str) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba((self.tokens.ui.error << 8) | 0x4d))
            .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
            .p(px(12.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(self.tokens.ui.error))
            .child(error.to_string())
            .into_any_element()
    }

    fn knowledge_collections_card(
        &self,
        collections: &[oxideterm_ai::RagCollectionResponse],
        selected_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut rows = vec![
            div()
                .flex()
                .items_center()
                .justify_end()
                .child(
                    button_with(
                        &self.tokens,
                        self.i18n.t("settings_view.knowledge.create_collection"),
                        ButtonOptions {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: false,
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.knowledge_create_dialog_open = true;
                            this.reset_standard_confirm_focus();
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .into_any_element(),
                )
                .into_any_element(),
        ];
        if collections.is_empty() {
            rows.push(self.knowledge_empty_row(
                LucideIcon::BookOpen,
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

    fn knowledge_documents_card(
        &self,
        collection: &oxideterm_ai::RagCollectionResponse,
        documents: Option<oxideterm_ai::RagPaginatedDocuments>,
        stats: Option<oxideterm_ai::RagStatsResponse>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let reindex_collection_id = collection.id.clone();
        let import_collection_id = collection.id.clone();
        let embedding_collection_id = collection.id.clone();
        let documents = documents.map(|page| page.documents).unwrap_or_default();
        let import_label = self
            .knowledge_import_progress
            .map(|(current, total)| format!("{current}/{total}"))
            .unwrap_or_else(|| self.i18n.t("settings_view.knowledge.import_files"));
        let embedding_label = self
            .knowledge_embedding_progress
            .map(|(current, total)| format!("{current}/{total}"))
            .unwrap_or_else(|| self.i18n.t("settings_view.knowledge.generate_embeddings"));
        let reindex_label = self
            .knowledge_reindex_progress
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
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
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
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child({
                            let import_disabled = self.knowledge_import_progress.is_some();
                            let import_button = self.knowledge_text_icon_button(
                                LucideIcon::FolderOpen,
                                import_label,
                                import_disabled,
                            );
                            if import_disabled {
                                import_button
                            } else {
                                import_button.on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, window, cx| {
                                        this.knowledge_import_files(
                                            import_collection_id.clone(),
                                            window,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }),
                                )
                            }
                        })
                        .child(
                            self.knowledge_text_icon_button(
                                LucideIcon::FilePlus,
                                self.i18n.t("settings_view.knowledge.new_document"),
                                false,
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.knowledge_new_document_dialog_open = true;
                                    this.reset_standard_confirm_focus();
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                        )
                        .child({
                            let embedding_disabled = self.knowledge_embedding_progress.is_some();
                            let embedding_button = self.knowledge_text_icon_button(
                                LucideIcon::Sparkles,
                                embedding_label,
                                embedding_disabled,
                            );
                            if embedding_disabled {
                                embedding_button
                            } else {
                                embedding_button.on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        this.knowledge_generate_embeddings(
                                            embedding_collection_id.clone(),
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }),
                                )
                            }
                        })
                        .child({
                            let reindex_disabled =
                                matches!(self.knowledge_reindex_progress, Some((_current, 0)));
                            let reindex_button = self.knowledge_text_icon_button(
                                if self.knowledge_reindex_progress.is_some() {
                                    LucideIcon::X
                                } else {
                                    LucideIcon::RefreshCw
                                },
                                reindex_label,
                                reindex_disabled,
                            );
                            if reindex_disabled {
                                reindex_button
                            } else {
                                reindex_button.on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        if this.knowledge_reindex_progress.is_some() {
                                            this.knowledge_cancel_reindex(cx);
                                        } else {
                                            this.knowledge_reindex(
                                                reindex_collection_id.clone(),
                                                cx,
                                            );
                                        }
                                        cx.stop_propagation();
                                    }),
                                )
                            }
                        }),
                )
                .into_any_element(),
        ];
        rows.push(self.knowledge_embedding_config_section(cx));
        if let Some(stats) = stats {
            rows.push(self.knowledge_stats_row(stats, cx));
        }
        rows.push(self.card_separator());
        if documents.is_empty() {
            rows.push(self.knowledge_empty_row(
                LucideIcon::FileText,
                self.i18n.t("settings_view.knowledge.no_documents"),
                cx,
            ));
        } else {
            for document in documents {
                rows.push(self.knowledge_document_row(document, cx));
            }
        }
        self.settings_card(
            "settings_view.knowledge.title",
            "settings_view.knowledge.description",
            rows,
        )
    }

    fn knowledge_collection_row(
        &self,
        collection: &oxideterm_ai::RagCollectionResponse,
        selected_id: Option<&str>,
        cx: &mut Context<Self>,
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
                cx.listener(move |this, _event, _window, cx| {
                    this.knowledge_selected_collection_id = Some(collection_id.clone());
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(div().flex_none().child(Self::render_lucide_icon(
                        LucideIcon::BookOpen,
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
            .child(
                div().flex_none().child(
                    self.knowledge_icon_button(
                        LucideIcon::Trash2,
                        rgb(self.tokens.ui.text_muted),
                        Some(rgb(self.tokens.ui.error)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.knowledge_delete_confirm = Some(KnowledgeDeleteConfirm {
                                target: KnowledgeDeleteTarget::Collection,
                                id: delete_id.clone(),
                                name: delete_name.clone(),
                            });
                            this.reset_standard_confirm_focus();
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
                ),
            )
            .into_any_element()
    }

    fn knowledge_text_icon_button(&self, icon: LucideIcon, label: String, disabled: bool) -> Div {
        // Knowledge action chips match Tauri's small outline buttons. Use the
        // shared primitive so disabled cursor/loading/focus-visible behavior
        // stays aligned with the rest of settings.
        toolbar_button(
            &self.tokens,
            label,
            Some(Self::render_lucide_icon(
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
        )
    }

    fn knowledge_icon_button(
        &self,
        icon: LucideIcon,
        color: gpui::Rgba,
        hover_color: Option<gpui::Rgba>,
    ) -> Div {
        // The original local helper accepted a hover text color, but the icon
        // SVG is rendered with an explicit color. Keep the parameter until the
        // shared icon primitive grows a real hover-icon-color slot.
        let _ = hover_color;
        icon_button(
            &self.tokens,
            Self::render_lucide_icon(
                icon,
                KNOWLEDGE_INLINE_ICON_SIZE,
                color,
            ),
            IconButtonOptions {
                size: KNOWLEDGE_ICON_BUTTON_SIZE,
                radius: ButtonRadius::Sm,
                hover_background: Some(rgba(
                    (0xffffff << 8) | KNOWLEDGE_ICON_BUTTON_HOVER_ALPHA,
                )),
                idle_opacity: 1.0,
                ..IconButtonOptions::compact(KNOWLEDGE_ICON_BUTTON_SIZE)
            },
        )
    }

    fn knowledge_document_row(
        &self,
        document: oxideterm_ai::RagDocumentResponse,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let delete_id = document.id.clone();
        let delete_name = document.title.clone();
        let edit_id = document.id.clone();
        let editing_this = self
            .knowledge_external_edit
            .as_ref()
            .is_some_and(|edit| edit.doc_id == document.id);
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(12.0))
            .py(px(8.0))
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(div().flex_none().child(Self::render_lucide_icon(
                        LucideIcon::FileText,
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
                                    .child(document.title),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .truncate()
                                    .child(format!(
                                        "{} · {} {} · {}",
                                        document.format,
                                        document.chunk_count,
                                        self.i18n.t("settings_view.knowledge.chunks"),
                                        self.knowledge_format_date(document.indexed_at)
                                    )),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(if editing_this {
                        self.knowledge_icon_button(
                            LucideIcon::RefreshCw,
                            rgb(self.tokens.ui.accent),
                            Some(rgb(self.tokens.ui.accent)),
                        )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.knowledge_sync_external_edit(true, cx);
                                    cx.stop_propagation();
                                }),
                            )
                    } else {
                        self.knowledge_icon_button(
                            LucideIcon::Pencil,
                            rgb(self.tokens.ui.text_muted),
                            Some(rgb(self.tokens.ui.text)),
                        )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.knowledge_open_external(edit_id.clone(), cx);
                                    cx.stop_propagation();
                                }),
                            )
                    })
                    .child(
                        self.knowledge_icon_button(
                            LucideIcon::Trash2,
                            rgb(self.tokens.ui.text_muted),
                            Some(rgb(self.tokens.ui.error)),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.knowledge_delete_confirm = Some(KnowledgeDeleteConfirm {
                                    target: KnowledgeDeleteTarget::Document,
                                    id: delete_id.clone(),
                                    name: delete_name.clone(),
                                });
                                this.reset_standard_confirm_focus();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .into_any_element()
    }

    fn knowledge_embedding_config_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.settings_store.settings();
        let preliminary = oxideterm_ai::resolve_ai_embedding_provider(
            &settings.ai.providers,
            settings.ai.active_provider_id.as_deref(),
            settings.ai.embedding_config.as_ref(),
            None,
        );
        let has_api_key = preliminary.provider.as_ref().and_then(|provider| {
            oxideterm_ai::ai_embedding_requires_api_key(provider)
                .then(|| self.ai_provider_has_key_cached(&provider.id))
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
            .unwrap_or_else(|| self.i18n.t("settings_view.knowledge.auto_embedding_provider"));
        let model_value = self.current_settings_input_value(SettingsInput::AiEmbeddingModel);
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
                .unwrap_or_else(|| self.i18n.t("settings_view.knowledge.semantic_search_not_configured")),
            oxideterm_ai::AiEmbeddingProviderReason::MissingModel => {
                self.i18n.t("settings_view.knowledge.semantic_search_missing_model")
            }
            oxideterm_ai::AiEmbeddingProviderReason::MissingApiKey => {
                self.i18n.t("settings_view.knowledge.embedding_api_key_missing")
            }
            oxideterm_ai::AiEmbeddingProviderReason::UnsupportedProvider => {
                self.i18n.t("settings_view.knowledge.embedding_provider_unsupported")
            }
            oxideterm_ai::AiEmbeddingProviderReason::NoProvider => {
                self.i18n.t("settings_view.knowledge.semantic_search_not_configured")
            }
        };
        let status_color = if resolved.reason == oxideterm_ai::AiEmbeddingProviderReason::Ready {
            self.tokens.ui.success
        } else {
            self.tokens.ui.text_muted
        };
        let chevron = if self.knowledge_embedding_config_expanded {
            LucideIcon::ChevronDown
        } else {
            LucideIcon::ChevronRight
        };

        div()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.border << 8) | KNOWLEDGE_SECTION_BORDER_ALPHA,
            ))
            .bg(rgba((self.tokens.ui.bg << 8) | KNOWLEDGE_SECTION_BG_ALPHA))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .p(px(12.0))
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
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::Sparkles,
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
                            .child(Self::render_lucide_icon(
                                LucideIcon::Wrench,
                                KNOWLEDGE_INLINE_ICON_SIZE,
                                rgb(self.tokens.ui.text),
                            ))
                            .child(self.i18n.t("settings_view.knowledge.configure_embeddings"))
                            .child(Self::render_lucide_icon(
                                chevron,
                                KNOWLEDGE_INLINE_ICON_SIZE,
                                rgb(self.tokens.ui.text_muted),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.knowledge_embedding_config_expanded =
                                        !this.knowledge_embedding_config_expanded;
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .when(self.knowledge_embedding_config_expanded, |section| {
                section.child(
                    div()
                        .border_t_1()
                        .border_color(rgba(
                            (self.tokens.ui.border << 8) | KNOWLEDGE_SECTION_DIVIDER_ALPHA,
                        ))
                        .p(px(12.0))
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t("settings_view.knowledge.semantic_search_description")),
                        )
                        .child(
                            div()
                                .grid()
                                .grid_cols(2)
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.0))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(self.tokens.ui.text_muted))
                                                .child(self.i18n.t("settings_view.ai.embedding_provider")),
                                        )
                                        .child(self.ai_settings_select_control(
                                            SettingsSelect::AiEmbeddingProvider,
                                            provider_label,
                                            224.0,
                                            cx,
                                        )),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.0))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(self.tokens.ui.text_muted))
                                                .child(self.i18n.t("settings_view.ai.embedding_model")),
                                        )
                                        .child(self.settings_text_input_control(
                                            SettingsInput::AiEmbeddingModel,
                                            model_value,
                                            self.i18n.t("settings_view.ai.embedding_model"),
                                            224.0,
                                            cx,
                                        )),
                                ),
                        ),
                )
            })
            .into_any_element()
    }

    fn knowledge_stats_row(
        &self,
        stats: oxideterm_ai::RagStatsResponse,
        cx: &mut Context<Self>,
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

    fn knowledge_stat_item(
        &self,
        value: String,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.render_selectable_display_text(
                        "knowledge-stat-value",
                        &label,
                        value.clone(),
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .child(self.render_selectable_display_text(
                "knowledge-stat-label",
                &value,
                label,
                self.tokens.ui.text_muted,
                cx,
            ))
            .into_any_element()
    }

    fn knowledge_format_date(&self, timestamp_millis: i64) -> String {
        let Some(datetime) = chrono::DateTime::from_timestamp_millis(timestamp_millis) else {
            return "-".to_string();
        };
        let datetime = datetime.with_timezone(&chrono::Local);
        match self.i18n.locale() {
            Locale::ZhCn | Locale::ZhTw => datetime.format("%Y年%-m月%-d日").to_string(),
            _ => datetime.format("%b %-d, %Y").to_string(),
        }
    }

    fn knowledge_empty_row(
        &self,
        icon: LucideIcon,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .py(px(32.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(Self::render_lucide_icon(
                icon,
                32.0,
                rgba((self.tokens.ui.text_muted << 8) | 0x66),
            ))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .child(self.render_selectable_display_text(
                        "knowledge-empty-row",
                        &label,
                        label.clone(),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn knowledge_scope_label(&self, scope: &oxideterm_ai::DocScope) -> String {
        match scope {
            oxideterm_ai::DocScope::Global => self.i18n.t("settings_view.knowledge.scope_global"),
            oxideterm_ai::DocScope::Connection(_) => {
                self.i18n.t("settings_view.knowledge.scope_connection")
            }
        }
    }

    fn knowledge_document_format_label(&self) -> String {
        match self.knowledge_new_document_format.as_str() {
            "plaintext" => "Plain Text".to_string(),
            _ => "Markdown".to_string(),
        }
    }

}
