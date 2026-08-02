use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn knowledge_create_collection(&mut self, cx: &mut Context<Self>) {
        let error_message = self
            .i18n
            .t("settings_view.knowledge.error_create_collection");
        self.ai_entity.update(cx, |entity, cx| {
            entity.create_knowledge_collection(error_message);
            cx.notify();
        });
        self.settings_input_draft.clear();
        self.refresh_knowledge_navigator(true, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_create_blank_document(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let error_message = self.i18n.t("settings_view.knowledge.error_create_document");
        let creation = self.ai_entity.update(cx, |entity, cx| {
            let creation = entity.create_blank_knowledge_document(error_message);
            cx.notify();
            creation
        });
        let Some((document, open_in_workspace)) = creation else {
            cx.notify();
            return false;
        };
        let document_id = document.id.clone();
        self.settings_input_draft.clear();
        self.knowledge_workspace.update(cx, |workspace, _cx| {
            workspace.insert_created_document(document);
        });
        self.refresh_knowledge_navigator(true, cx);
        {
            if open_in_workspace {
                self.select_knowledge_document(document_id, cx);
            } else {
                self.knowledge_open_external(document_id, cx);
            }
        }
        cx.notify();
        true
    }

    pub(in crate::workspace) fn knowledge_delete_collection(
        &mut self,
        collection_id: String,
        cx: &mut Context<Self>,
    ) {
        let error_message = self.i18n.t("settings_view.knowledge.error_delete");
        let deleted = self.ai_entity.update(cx, |entity, cx| {
            let deleted = entity.delete_knowledge_collection(&collection_id, error_message);
            cx.notify();
            deleted
        });
        if deleted {
            self.knowledge_workspace.update(cx, |workspace, entity_cx| {
                workspace.remove_collection(&collection_id, entity_cx);
            });
            self.refresh_knowledge_navigator(true, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_delete_document(
        &mut self,
        document_id: String,
        cx: &mut Context<Self>,
    ) {
        let error_message = self.i18n.t("settings_view.knowledge.error_delete");
        let deleted = self.ai_entity.update(cx, |entity, cx| {
            let deleted = entity.delete_knowledge_document(&document_id, error_message);
            cx.notify();
            deleted
        });
        if deleted {
            self.knowledge_workspace.update(cx, |workspace, _cx| {
                workspace.remove_document(&document_id);
            });
            self.refresh_knowledge_navigator(true, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_reindex(
        &mut self,
        collection_id: String,
        cx: &mut Context<Self>,
    ) {
        let store = self.ai_entity.read(cx).rag_store();
        self.ai_entity.update(cx, |entity, cx| {
            if entity.request_knowledge_reindex(store, collection_id) {
                entity.clear_knowledge_error();
            }
            cx.notify();
        });
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_cancel_reindex(&mut self, cx: &mut Context<Self>) {
        self.ai_entity.update(cx, |entity, cx| {
            entity.cancel_knowledge_reindex();
            cx.notify();
        });
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_import_files(
        &mut self,
        collection_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.knowledge.import_files"),
            )),
        });
        let paths = async move {
            match receiver.await {
                Ok(Ok(paths)) => paths,
                _ => None,
            }
        };
        let error_message = self.i18n.t("settings_view.knowledge.error_import");
        self.ai_entity.update(cx, |entity, cx| {
            entity.start_knowledge_import(paths, collection_id, error_message, cx);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_generate_embeddings(
        &mut self,
        collection_id: String,
        cx: &mut Context<Self>,
    ) {
        let settings = self.settings_store.settings();
        let resolved = oxideterm_ai::resolve_ai_embedding_provider(
            &settings.ai.providers,
            settings.ai.active_provider_id.as_deref(),
            settings.ai.embedding_config.as_ref(),
            None,
        );
        let Some(provider) = resolved.provider else {
            self.report_knowledge_embedding_configuration_error(
                "settings_view.knowledge.error_no_embedding_support",
                cx,
            );
            return;
        };
        match resolved.reason {
            oxideterm_ai::AiEmbeddingProviderReason::NoProvider
            | oxideterm_ai::AiEmbeddingProviderReason::UnsupportedProvider => {
                self.report_knowledge_embedding_configuration_error(
                    "settings_view.knowledge.error_no_embedding_support",
                    cx,
                );
                return;
            }
            oxideterm_ai::AiEmbeddingProviderReason::MissingModel => {
                self.report_knowledge_embedding_configuration_error(
                    "settings_view.knowledge.error_no_embedding_model",
                    cx,
                );
                return;
            }
            oxideterm_ai::AiEmbeddingProviderReason::Ready
            | oxideterm_ai::AiEmbeddingProviderReason::MissingApiKey => {}
        }
        let requires_api_key = oxideterm_ai::ai_embedding_requires_api_key(&provider);
        let missing_key_error = self
            .i18n
            .t("settings_view.knowledge.error_no_embedding_api_key");
        let embedding_error = self
            .i18n
            .t("settings_view.knowledge.error_generate_embeddings");
        let partial_failure_template = self
            .i18n
            .t("settings_view.knowledge.embedding_partial_failure");
        self.ai_entity.update(cx, |entity, cx| {
            entity.start_knowledge_embeddings(
                collection_id,
                provider,
                resolved.model,
                requires_api_key,
                missing_key_error,
                embedding_error,
                partial_failure_template,
                cx,
            );
        });
        cx.notify();
    }

    fn report_knowledge_embedding_configuration_error(
        &mut self,
        message_key: &str,
        cx: &mut Context<Self>,
    ) {
        let message = self.i18n.t(message_key);
        self.ai_entity.update(cx, |entity, cx| {
            entity.expand_knowledge_embedding_config();
            entity.set_knowledge_error(message);
            cx.notify();
        });
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_open_external(
        &mut self,
        document_id: String,
        cx: &mut Context<Self>,
    ) {
        let error_message = self.i18n.t("settings_view.knowledge.error_open_external");
        let edit_dir = self
            .settings_store
            .path()
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rag-edit");
        let prepared = self.ai_entity.update(cx, |entity, _cx| {
            entity.prepare_knowledge_external_edit(&document_id, edit_dir, error_message.clone())
        });
        let Some((path, edit)) = prepared else {
            cx.notify();
            return;
        };
        // Launching the platform editor is a window adapter; document content
        // and edit ownership remain inside the AI entity.
        let opened = open_path_external(&path).is_ok();
        self.ai_entity.update(cx, |entity, cx| {
            entity.finish_knowledge_external_open(edit, opened, error_message);
            cx.notify();
        });
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_sync_external_edit(
        &mut self,
        notify_no_changes: bool,
        cx: &mut Context<Self>,
    ) {
        let error_message = self.i18n.t("settings_view.knowledge.error_sync");
        let outcome = self.ai_entity.update(cx, |entity, cx| {
            let outcome = entity.sync_knowledge_external_edit(error_message);
            cx.notify();
            outcome
        });
        match outcome {
            ai_state::knowledge::KnowledgeExternalSyncOutcome::NoChanges if notify_no_changes => {
                self.push_ai_settings_toast(
                    self.i18n.t("settings_view.knowledge.doc_no_changes"),
                    TerminalNoticeVariant::Success,
                    cx,
                );
            }
            ai_state::knowledge::KnowledgeExternalSyncOutcome::Updated => {
                self.push_ai_settings_toast(
                    self.i18n.t("settings_view.knowledge.doc_updated"),
                    TerminalNoticeVariant::Success,
                    cx,
                );
            }
            ai_state::knowledge::KnowledgeExternalSyncOutcome::NoEdit
            | ai_state::knowledge::KnowledgeExternalSyncOutcome::NoChanges
            | ai_state::knowledge::KnowledgeExternalSyncOutcome::Failed => {}
        }
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_confirm_delete(&mut self, cx: &mut Context<Self>) {
        let confirm = self
            .ai_entity
            .update(cx, |entity, _cx| entity.take_knowledge_delete_confirm());
        let Some(confirm) = confirm else {
            cx.notify();
            return;
        };
        match confirm.target {
            KnowledgeDeleteTarget::Collection => {
                self.knowledge_delete_collection(confirm.id, cx);
            }
            KnowledgeDeleteTarget::Document => {
                self.knowledge_delete_document(confirm.id, cx);
            }
        }
    }
}
