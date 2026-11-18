use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn show_knowledge_document_page(
        &mut self,
        collection_id: String,
        page: usize,
        cx: &mut Context<Self>,
    ) {
        self.knowledge_page(cx).update(cx, |ai, cx| {
            ai.set_knowledge_document_page(collection_id, page);
            cx.notify();
        });
        self.sync_settings_section_list_state(cx);
        self.settings_workspace
            .read(cx)
            .section_list
            .scroll_to_section(
                SettingsTab::Knowledge,
                SettingsSectionId::KnowledgeDocuments,
                self.settings_dynamic_section_counts(cx),
            );
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_create_collection(&mut self, cx: &mut Context<Self>) {
        let error_message = self
            .i18n
            .t("settings_view.knowledge.error_create_collection");
        self.knowledge_page(cx).update(cx, |entity, cx| {
            entity.create_knowledge_collection(error_message);
            cx.notify();
        });
        self.settings_input_draft.clear();
        self.knowledge_workspace.view.update(cx, |view, cx| {
            view.refresh_knowledge_navigator(true, cx);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_create_blank_document(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let error_message = self.i18n.t("settings_view.knowledge.error_create_document");
        let creation = self.knowledge_page(cx).update(cx, |entity, cx| {
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
        self.knowledge_state().update(cx, |workspace, _cx| {
            workspace.insert_created_document(document);
        });
        self.knowledge_workspace.view.update(cx, |view, cx| {
            view.refresh_knowledge_navigator(true, cx);
        });
        {
            if open_in_workspace {
                self.knowledge_workspace.view.update(cx, |view, cx| {
                    view.select_knowledge_document(document_id, cx);
                });
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
        let deleted = self.knowledge_page(cx).update(cx, |entity, cx| {
            let deleted = entity.delete_knowledge_collection(&collection_id, error_message);
            cx.notify();
            deleted
        });
        if deleted {
            self.knowledge_state().update(cx, |workspace, entity_cx| {
                workspace.remove_collection(&collection_id, entity_cx);
            });
            self.knowledge_workspace.view.update(cx, |view, cx| {
                view.refresh_knowledge_navigator(true, cx);
            });
        }
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_delete_document(
        &mut self,
        document_id: String,
        cx: &mut Context<Self>,
    ) {
        let error_message = self.i18n.t("settings_view.knowledge.error_delete");
        let deleted = self.knowledge_page(cx).update(cx, |entity, cx| {
            let deleted = entity.delete_knowledge_document(&document_id, error_message);
            cx.notify();
            deleted
        });
        if deleted {
            self.knowledge_state().update(cx, |workspace, _cx| {
                workspace.remove_document(&document_id);
            });
            self.knowledge_workspace.view.update(cx, |view, cx| {
                view.refresh_knowledge_navigator(true, cx);
            });
        }
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_reindex(
        &mut self,
        collection_id: String,
        cx: &mut Context<Self>,
    ) {
        self.knowledge_page(cx).update(cx, |entity, cx| {
            entity.request_knowledge_reindex(collection_id, cx);
        });
    }

    pub(in crate::workspace) fn knowledge_cancel_reindex(&mut self, cx: &mut Context<Self>) {
        self.knowledge_page(cx).update(cx, |entity, _| {
            entity.cancel_knowledge_reindex();
        });
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
        self.knowledge_page(cx).update(cx, |entity, cx| {
            entity.start_knowledge_import(paths, collection_id, error_message, cx);
        });
    }

    pub(in crate::workspace) fn knowledge_generate_embeddings(
        &mut self,
        collection_id: String,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_settings_view::knowledge_page::EmbeddingJob;
        let job = EmbeddingJob::prepare(self.settings_store.settings(), &self.i18n);
        self.knowledge_page(cx).update(cx, |entity, cx| match job {
            Ok(job) => {
                entity.start_knowledge_embeddings(
                    collection_id,
                    job.provider,
                    job.model,
                    job.requires_api_key,
                    job.missing_key_error,
                    job.retrieval_error,
                    job.partial_failure,
                    cx,
                );
            }
            Err(message) => {
                entity.expand_knowledge_embedding_config();
                entity.set_knowledge_error(message);
                cx.notify();
            }
        });
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
        let prepared = self.knowledge_page(cx).update(cx, |entity, _cx| {
            entity.prepare_knowledge_external_edit(&document_id, edit_dir, error_message.clone())
        });
        let Some((path, edit)) = prepared else {
            cx.notify();
            return;
        };
        // Launching the platform editor is a window adapter; document content
        // and edit ownership remain inside the AI entity.
        let opened = open_path_external(&path).is_ok();
        self.knowledge_page(cx).update(cx, |entity, cx| {
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
        let outcome = self.knowledge_page(cx).update(cx, |entity, cx| {
            let outcome = entity.sync_knowledge_external_edit(error_message);
            cx.notify();
            outcome
        });
        match outcome {
            oxideterm_gpui_ai::knowledge::page::KnowledgeExternalSyncOutcome::NoChanges
                if notify_no_changes =>
            {
                self.push_ai_settings_toast(
                    self.i18n.t("settings_view.knowledge.doc_no_changes"),
                    TerminalNoticeVariant::Success,
                    cx,
                );
            }
            oxideterm_gpui_ai::knowledge::page::KnowledgeExternalSyncOutcome::Updated => {
                self.push_ai_settings_toast(
                    self.i18n.t("settings_view.knowledge.doc_updated"),
                    TerminalNoticeVariant::Success,
                    cx,
                );
            }
            oxideterm_gpui_ai::knowledge::page::KnowledgeExternalSyncOutcome::NoEdit
            | oxideterm_gpui_ai::knowledge::page::KnowledgeExternalSyncOutcome::NoChanges
            | oxideterm_gpui_ai::knowledge::page::KnowledgeExternalSyncOutcome::Failed => {}
        }
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_confirm_delete(&mut self, cx: &mut Context<Self>) {
        let confirm = self
            .knowledge_page(cx)
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
