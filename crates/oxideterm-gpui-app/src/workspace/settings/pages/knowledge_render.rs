use super::*;
use oxideterm_gpui_settings_view::knowledge_page::{
    KnowledgeSection, KnowledgeSettingsFrame, KnowledgeSettingsPage, KnowledgeSettingsRequest,
};

impl WorkspaceApp {
    pub(in crate::workspace::settings) fn prepare_knowledge_settings(
        &mut self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> KnowledgeSection {
        if section == SettingsSectionId::KnowledgeEmbeddingConfig {
            self.ensure_ai_provider_key_statuses(cx);
        }
        let ai = self.ai_entity.clone();
        let model = ai.read(cx);
        let store = model.rag_store();
        let selected = model.knowledge_selected_collection_id().map(str::to_owned);
        let error = model.knowledge_error().map(str::to_owned);
        KnowledgeSection::load(
            section,
            &store,
            selected.as_deref(),
            error.as_deref(),
            |collection| {
                let stats = ai.update(cx, |ai, cx| {
                    ai.request_knowledge_collection_stats(collection, cx)
                });
                (ai.read(cx).knowledge_document_page(collection), stats)
            },
        )
    }

    pub(in crate::workspace::settings) fn knowledge_settings_page_data<'a>(
        &'a self,
        owner: gpui::WeakEntity<Self>,
        cx: &'a App,
    ) -> oxideterm_gpui_settings_view::surface::KnowledgePageData<'a> {
        use oxideterm_gpui_settings_view::surface::KnowledgePageData;
        let ai = self.ai_entity.read(cx);
        KnowledgePageData {
            frame: KnowledgeSettingsFrame {
                page_size: crate::workspace::ai_state::KNOWLEDGE_DOCUMENT_PAGE_SIZE,
                compact_row_height: KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT,
                has_background: self.background_surface_active("knowledge"),
                import_progress: ai.knowledge_import_progress(),
                embedding_progress: ai.knowledge_embedding_progress(),
                reindex_progress: ai.knowledge_reindex_progress(),
                embedding_config_expanded: ai.knowledge_embedding_config_expanded(),
                external_edit_id: ai
                    .knowledge_external_edit()
                    .map(|edit| edit.doc_id.as_str()),
                embedding_model: "",
            },
            provider_has_key: Rc::new({
                let owner = owner.clone();
                move |id, cx| {
                    owner
                        .upgrade()
                        .is_some_and(|app| app.read(cx).ai_provider_has_key_cached(id, cx))
                }
            }),
            selected: Rc::new({
                let owner = owner.clone();
                move |id, cx| {
                    owner
                        .upgrade()
                        .is_some_and(|app| app.read(cx).is_knowledge_document_selected(id, cx))
                }
            }),
            cut: Rc::new({
                let owner = owner.clone();
                move |id, cx| {
                    owner
                        .upgrade()
                        .is_some_and(|app| app.read(cx).knowledge_note_is_cut(id, cx))
                }
            }),
            chevron: Rc::new(move |expanded, cx| {
                let Some(owner) = owner.upgrade() else {
                    return div().into_any_element();
                };
                let app = owner.read(cx);
                app.render_animated_chevron(
                    ("knowledge-embedding-chevron", expanded as usize),
                    expanded,
                    14.0,
                    rgb(app.tokens.ui.text_muted),
                )
                .into_any_element()
            }),
        }
    }

    fn with_knowledge_settings_page<R>(
        &self,
        cx: &Context<Self>,
        render: impl FnOnce(KnowledgeSettingsPage<'_>, &App) -> R,
    ) -> R {
        let owner = cx.entity().downgrade();
        let dispatch = Rc::new(
            move |request, position, window: &mut Window, cx: &mut App| {
                let _ = owner.update(cx, |app, cx| {
                    app.handle_knowledge_settings_request(request, position, window, cx)
                });
            },
        );
        let ai = self.ai_entity.read(cx);
        let model = self.current_settings_input_value(SettingsInput::AiEmbeddingModel, cx);
        let document_selected = |id: &str, cx: &App| self.is_knowledge_document_selected(id, cx);
        let document_cut = |id: &str, cx: &App| self.knowledge_note_is_cut(id, cx);
        let provider_has_key = |id: &str, cx: &App| self.ai_provider_has_key_cached(id, cx);
        let chevron = |expanded| {
            self.render_animated_chevron(
                ("knowledge-embedding-chevron", expanded as usize),
                expanded,
                14.0,
                rgb(self.tokens.ui.text_muted),
            )
            .into_any_element()
        };
        self.with_settings_controls(cx, |controls, cx| {
            render(
                KnowledgeSettingsPage {
                    controls,
                    dispatch,
                    provider_has_key: &provider_has_key,
                    document_selected: &document_selected,
                    document_cut: &document_cut,
                    chevron: &chevron,
                    frame: KnowledgeSettingsFrame {
                        page_size: crate::workspace::ai_state::KNOWLEDGE_DOCUMENT_PAGE_SIZE,
                        compact_row_height: KNOWLEDGE_WORKSPACE_SECTION_ESTIMATED_HEIGHT,
                        has_background: self.background_surface_active("knowledge"),
                        import_progress: ai.knowledge_import_progress(),
                        embedding_progress: ai.knowledge_embedding_progress(),
                        reindex_progress: ai.knowledge_reindex_progress(),
                        embedding_config_expanded: ai.knowledge_embedding_config_expanded(),
                        external_edit_id: ai
                            .knowledge_external_edit()
                            .map(|edit| edit.doc_id.as_str()),
                        embedding_model: &model,
                    },
                },
                cx,
            )
        })
    }

    pub(in crate::workspace::settings) fn handle_knowledge_settings_request(
        &mut self,
        request: KnowledgeSettingsRequest,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match request {
            KnowledgeSettingsRequest::CreateCollection => {
                self.open_knowledge_create_dialog(cx);
                self.reset_standard_confirm_focus();
            }
            KnowledgeSettingsRequest::ImportFiles(import_collection_id) => {
                self.knowledge_import_files(import_collection_id.clone(), window, cx);
            }
            KnowledgeSettingsRequest::NewDocument(create_document_collection_id) => {
                self.open_knowledge_document_dialog(
                    create_document_collection_id.clone(),
                    false,
                    window,
                    cx,
                );
                self.reset_standard_confirm_focus();
            }
            KnowledgeSettingsRequest::Generate(embedding_collection_id) => {
                self.knowledge_generate_embeddings(embedding_collection_id.clone(), cx);
            }
            KnowledgeSettingsRequest::ToggleReindex(reindex_collection_id) => {
                if self
                    .ai_entity
                    .read(cx)
                    .knowledge_reindex_progress()
                    .is_some()
                {
                    self.knowledge_cancel_reindex(cx);
                } else {
                    self.knowledge_reindex(reindex_collection_id.clone(), cx);
                }
            }
            KnowledgeSettingsRequest::ShowPage(collection, page) => {
                self.show_knowledge_document_page(collection, page, cx);
            }
            KnowledgeSettingsRequest::SelectCollection(collection_id) => {
                self.ai_entity.update(cx, |entity, cx| {
                    entity.select_knowledge_collection(collection_id.clone());
                    cx.notify();
                });
                self.refresh_knowledge_navigator(true, cx);
                self.settings_views.surface.update(cx, |_, cx| cx.notify());
            }
            KnowledgeSettingsRequest::DeleteCollection(delete_id, delete_name) => {
                self.ai_entity.update(cx, |entity, cx| {
                    entity.request_delete_knowledge_collection(
                        delete_id.clone(),
                        delete_name.clone(),
                    );
                    cx.notify();
                });
                self.reset_standard_confirm_focus();
                self.settings_views.surface.update(cx, |_, cx| cx.notify());
            }
            KnowledgeSettingsRequest::SyncExternal => {
                self.knowledge_sync_external_edit(true, cx);
            }
            KnowledgeSettingsRequest::OpenExternal(edit_id) => {
                self.knowledge_open_external(edit_id.clone(), cx);
            }
            KnowledgeSettingsRequest::DeleteDocument(delete_id, delete_name) => {
                self.ai_entity.update(cx, |entity, cx| {
                    entity
                        .request_delete_knowledge_document(delete_id.clone(), delete_name.clone());
                    cx.notify();
                });
                self.reset_standard_confirm_focus();
                self.settings_views.surface.update(cx, |_, cx| cx.notify());
            }
            KnowledgeSettingsRequest::NoteMenu(menu_document) => {
                self.open_knowledge_note_menu(menu_document.clone(), position, window, cx);
            }
            KnowledgeSettingsRequest::OpenInWorkspace(open_id) => {
                self.open_knowledge_workspace_tab(window, cx);
                self.select_knowledge_document(open_id.clone(), cx);
            }
            KnowledgeSettingsRequest::ToggleEmbedding => {
                self.ai_entity.update(cx, |entity, cx| {
                    entity.toggle_knowledge_embedding_config();
                    cx.notify();
                });
                self.settings_views.surface.update(cx, |_, cx| cx.notify());
            }
        }
    }

    pub(in crate::workspace) fn knowledge_document_row(
        &self,
        document: oxideterm_ai::RagDocumentResponse,
        open_in_workspace: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.with_knowledge_settings_page(cx, |page, cx| {
            page.knowledge_document_row(document, open_in_workspace, cx)
        })
    }

    pub(in crate::workspace) fn knowledge_empty_row(
        &self,
        icon: LucideIcon,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.with_knowledge_settings_page(cx, |page, cx| {
            page.knowledge_empty_row(icon.path(), label, cx)
        })
    }

    pub(in crate::workspace) fn knowledge_error_row(&self, error: &str) -> AnyElement {
        self.settings_callout(
            SettingsCalloutTone::Error,
            Some(LucideIcon::AlertCircle),
            div().child(error.to_string()).into_any_element(),
        )
    }

    pub(in crate::workspace) fn knowledge_document_format_label(
        &self,
        cx: &Context<Self>,
    ) -> String {
        match self.ai_entity.read(cx).knowledge_new_document_format() {
            "plaintext" => self.i18n.t("settings_view.knowledge.format_plain_text"),
            _ => self.i18n.t("settings_view.knowledge.format_markdown"),
        }
    }
}
