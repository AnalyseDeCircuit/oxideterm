use super::*;

pub(in crate::workspace) enum KnowledgeExternalSyncOutcome {
    NoEdit,
    NoChanges,
    Updated,
    Failed,
}

/// Page-local Knowledge state whose lifetime follows the AI workspace entity.
///
/// The type intentionally has no `Debug` implementation because drafts and
/// external-edit metadata can describe user-owned document content.
pub(in crate::workspace) struct KnowledgePageState {
    pub(super) selected_collection_id: Option<String>,
    pub(super) create_dialog_open: bool,
    pub(super) new_document_dialog_open: bool,
    pub(super) embedding_config_expanded: bool,
    pub(super) new_collection_name: String,
    pub(super) new_document_title: String,
    pub(super) new_document_format: String,
    pub(super) import_progress: Option<(usize, usize)>,
    pub(super) embedding_progress: Option<(usize, usize)>,
    pub(super) delete_confirm: Option<oxideterm_settings_model::KnowledgeDeleteConfirm>,
    pub(super) external_edit: Option<oxideterm_settings_model::KnowledgeExternalEdit>,
    pub(super) error: Option<String>,
    pub(super) create_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) document_presence: oxideterm_gpui_ui::motion::ExitPresence,
    pub(super) create_exit_task: Option<Task<()>>,
    pub(super) document_exit_task: Option<Task<()>>,
}

impl Default for KnowledgePageState {
    fn default() -> Self {
        Self {
            selected_collection_id: None,
            create_dialog_open: false,
            new_document_dialog_open: false,
            embedding_config_expanded: false,
            new_collection_name: String::new(),
            new_document_title: String::new(),
            new_document_format: "markdown".to_string(),
            import_progress: None,
            embedding_progress: None,
            delete_confirm: None,
            external_edit: None,
            error: None,
            create_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            document_presence: oxideterm_gpui_ui::motion::ExitPresence::visible(),
            create_exit_task: None,
            document_exit_task: None,
        }
    }
}

impl KnowledgePageState {
    pub(super) fn input_value_mut(&mut self, input: SettingsInput) -> Option<&mut String> {
        match input {
            SettingsInput::KnowledgeCollectionName => Some(&mut self.new_collection_name),
            SettingsInput::KnowledgeDocumentTitle => Some(&mut self.new_document_title),
            _ => None,
        }
    }
}

impl AiWorkspaceEntity {
    fn emit_knowledge_page_changed(&self, cx: &mut Context<Self>) {
        cx.emit(AiWorkspaceEvent::KnowledgePageChanged);
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_selected_collection_id(&self) -> Option<&str> {
        self.knowledge_page.selected_collection_id.as_deref()
    }

    pub(in crate::workspace) fn knowledge_create_dialog_open(&self) -> bool {
        self.knowledge_page.create_dialog_open
    }

    pub(in crate::workspace) fn knowledge_document_dialog_open(&self) -> bool {
        self.knowledge_page.new_document_dialog_open
    }

    pub(in crate::workspace) fn knowledge_embedding_config_expanded(&self) -> bool {
        self.knowledge_page.embedding_config_expanded
    }

    pub(in crate::workspace) fn knowledge_new_collection_name(&self) -> &str {
        &self.knowledge_page.new_collection_name
    }

    pub(in crate::workspace) fn knowledge_new_document_title(&self) -> &str {
        &self.knowledge_page.new_document_title
    }

    pub(in crate::workspace) fn knowledge_new_document_format(&self) -> &str {
        &self.knowledge_page.new_document_format
    }

    pub(in crate::workspace) fn knowledge_import_progress(&self) -> Option<(usize, usize)> {
        self.knowledge_page.import_progress
    }

    pub(in crate::workspace) fn knowledge_embedding_progress(&self) -> Option<(usize, usize)> {
        self.knowledge_page.embedding_progress
    }

    pub(in crate::workspace) fn knowledge_delete_confirm(
        &self,
    ) -> Option<&oxideterm_settings_model::KnowledgeDeleteConfirm> {
        self.knowledge_page.delete_confirm.as_ref()
    }

    pub(in crate::workspace) fn knowledge_external_edit(
        &self,
    ) -> Option<&oxideterm_settings_model::KnowledgeExternalEdit> {
        self.knowledge_page.external_edit.as_ref()
    }

    pub(in crate::workspace) fn knowledge_error(&self) -> Option<&str> {
        self.knowledge_page.error.as_deref()
    }

    pub(in crate::workspace) fn knowledge_create_presence(
        &self,
    ) -> oxideterm_gpui_ui::motion::ExitPresence {
        self.knowledge_page.create_presence
    }

    pub(in crate::workspace) fn knowledge_document_presence(
        &self,
    ) -> oxideterm_gpui_ui::motion::ExitPresence {
        self.knowledge_page.document_presence
    }

    pub(in crate::workspace) fn select_knowledge_collection(&mut self, collection_id: String) {
        self.knowledge_page.selected_collection_id = Some(collection_id);
    }

    pub(in crate::workspace) fn set_knowledge_document_format(&mut self, format: String) {
        self.knowledge_page.new_document_format = format;
    }

    pub(in crate::workspace) fn toggle_knowledge_embedding_config(&mut self) {
        self.knowledge_page.embedding_config_expanded =
            !self.knowledge_page.embedding_config_expanded;
    }

    pub(in crate::workspace) fn expand_knowledge_embedding_config(&mut self) {
        self.knowledge_page.embedding_config_expanded = true;
        self.knowledge_page.error = None;
    }

    pub(in crate::workspace) fn set_knowledge_error(&mut self, error: String) {
        self.knowledge_page.error = Some(error);
    }

    pub(in crate::workspace) fn clear_knowledge_error(&mut self) {
        self.knowledge_page.error = None;
    }

    pub(in crate::workspace) fn request_delete_knowledge_collection(
        &mut self,
        id: String,
        name: String,
    ) {
        self.knowledge_page.delete_confirm =
            Some(oxideterm_settings_model::KnowledgeDeleteConfirm {
                target: oxideterm_settings_model::KnowledgeDeleteTarget::Collection,
                id,
                name,
            });
    }

    pub(in crate::workspace) fn request_delete_knowledge_document(
        &mut self,
        id: String,
        name: String,
    ) {
        self.knowledge_page.delete_confirm =
            Some(oxideterm_settings_model::KnowledgeDeleteConfirm {
                target: oxideterm_settings_model::KnowledgeDeleteTarget::Document,
                id,
                name,
            });
    }

    pub(in crate::workspace) fn clear_knowledge_delete_confirm(&mut self) {
        self.knowledge_page.delete_confirm = None;
    }

    pub(in crate::workspace) fn take_knowledge_delete_confirm(
        &mut self,
    ) -> Option<oxideterm_settings_model::KnowledgeDeleteConfirm> {
        self.knowledge_page.delete_confirm.take()
    }

    pub(in crate::workspace) fn open_knowledge_create_dialog(&mut self) {
        self.knowledge_page.create_exit_task = None;
        self.knowledge_page.create_presence.reopen();
        self.knowledge_page.create_dialog_open = true;
    }

    pub(in crate::workspace) fn close_knowledge_create_dialog(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        let Some(generation) = self.knowledge_page.create_presence.begin_exit() else {
            return;
        };
        if delay.is_zero() {
            self.finish_knowledge_create_dialog_exit(generation);
            self.emit_knowledge_page_changed(cx);
            return;
        }
        let task = cx.spawn(async move |entity, cx| {
            gpui::Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                entity.finish_knowledge_create_dialog_exit(generation);
                entity.emit_knowledge_page_changed(cx);
            });
        });
        self.knowledge_page.create_exit_task = Some(task);
        self.emit_knowledge_page_changed(cx);
    }

    fn finish_knowledge_create_dialog_exit(&mut self, generation: u64) {
        if self.knowledge_page.create_presence.finish_exit(generation) {
            self.knowledge_page.create_dialog_open = false;
            self.knowledge_page.new_collection_name.clear();
            self.knowledge_page.create_presence.reopen();
            self.knowledge_page.create_exit_task = None;
        }
    }

    pub(in crate::workspace) fn open_knowledge_document_dialog(&mut self) {
        self.knowledge_page.document_exit_task = None;
        self.knowledge_page.document_presence.reopen();
        self.knowledge_page.new_document_dialog_open = true;
    }

    pub(in crate::workspace) fn close_knowledge_document_dialog(
        &mut self,
        delay: Duration,
        cx: &mut Context<Self>,
    ) {
        let Some(generation) = self.knowledge_page.document_presence.begin_exit() else {
            return;
        };
        if delay.is_zero() {
            self.finish_knowledge_document_dialog_exit(generation);
            self.emit_knowledge_page_changed(cx);
            return;
        }
        let task = cx.spawn(async move |entity, cx| {
            gpui::Timer::after(delay).await;
            let _ = entity.update(cx, |entity, cx| {
                entity.finish_knowledge_document_dialog_exit(generation);
                entity.emit_knowledge_page_changed(cx);
            });
        });
        self.knowledge_page.document_exit_task = Some(task);
        self.emit_knowledge_page_changed(cx);
    }

    fn finish_knowledge_document_dialog_exit(&mut self, generation: u64) {
        if self
            .knowledge_page
            .document_presence
            .finish_exit(generation)
        {
            self.knowledge_page.new_document_dialog_open = false;
            self.knowledge_page.new_document_title.clear();
            self.knowledge_page.document_presence.reopen();
            self.knowledge_page.document_exit_task = None;
        }
    }

    pub(in crate::workspace) fn create_knowledge_collection(
        &mut self,
        error_message: String,
    ) -> bool {
        let name = self.knowledge_page.new_collection_name.trim().to_string();
        if name.is_empty() {
            return false;
        }
        let store = self.rag_store();
        match oxideterm_ai::rag_create_collection(
            &store,
            oxideterm_ai::RagCreateCollectionRequest {
                name,
                scope: oxideterm_ai::RagDocScopeRequest::Global,
            },
        ) {
            Ok(collection) => {
                self.knowledge_page.selected_collection_id = Some(collection.id);
                self.knowledge_page.new_collection_name.clear();
                self.knowledge_page.error = None;
                true
            }
            Err(_) => {
                self.knowledge_page.error = Some(error_message);
                false
            }
        }
    }

    pub(in crate::workspace) fn create_blank_knowledge_document(
        &mut self,
        error_message: String,
    ) -> Option<String> {
        let store = self.rag_store();
        let collection_id = self
            .knowledge_page
            .selected_collection_id
            .clone()
            .or_else(|| {
                oxideterm_ai::rag_list_collections(&store, None)
                    .ok()
                    .and_then(|collections| {
                        collections.first().map(|collection| collection.id.clone())
                    })
            })?;
        let title = self.knowledge_page.new_document_title.trim().to_string();
        if title.is_empty() {
            return None;
        }
        match oxideterm_ai::rag_create_blank_document(
            &store,
            oxideterm_ai::RagCreateBlankDocumentRequest {
                collection_id,
                title,
                format: self.knowledge_page.new_document_format.clone(),
            },
        ) {
            Ok(document) => {
                self.knowledge_page.new_document_title.clear();
                self.knowledge_page.error = None;
                Some(document.id)
            }
            Err(_) => {
                self.knowledge_page.error = Some(error_message);
                None
            }
        }
    }

    pub(in crate::workspace) fn delete_knowledge_collection(
        &mut self,
        collection_id: &str,
        error_message: String,
    ) -> bool {
        if oxideterm_ai::rag_delete_collection(&self.rag_store(), collection_id).is_err() {
            self.knowledge_page.error = Some(error_message);
            return false;
        }
        if self.knowledge_page.selected_collection_id.as_deref() == Some(collection_id) {
            self.knowledge_page.selected_collection_id = None;
        }
        self.knowledge_page.external_edit = None;
        self.knowledge_page.error = None;
        true
    }

    pub(in crate::workspace) fn delete_knowledge_document(
        &mut self,
        document_id: &str,
        error_message: String,
    ) -> bool {
        if oxideterm_ai::rag_remove_document(&self.rag_store(), document_id).is_err() {
            self.knowledge_page.error = Some(error_message);
            return false;
        }
        if self
            .knowledge_page
            .external_edit
            .as_ref()
            .is_some_and(|edit| edit.doc_id == document_id)
        {
            self.knowledge_page.external_edit = None;
        }
        self.knowledge_page.error = None;
        true
    }

    pub(in crate::workspace) fn prepare_knowledge_external_edit(
        &mut self,
        document_id: &str,
        edit_dir: PathBuf,
        error_message: String,
    ) -> Option<(PathBuf, oxideterm_settings_model::KnowledgeExternalEdit)> {
        if uuid::Uuid::parse_str(document_id).is_err() {
            self.knowledge_page.error = Some(error_message);
            return None;
        }
        let store = self.rag_store();
        let document = oxideterm_ai::rag_list_collections(&store, None)
            .ok()
            .into_iter()
            .flatten()
            .find_map(|collection| {
                oxideterm_ai::rag_list_documents(&store, &collection.id, None, Some(500))
                    .ok()
                    .and_then(|page| {
                        page.documents
                            .into_iter()
                            .find(|document| document.id == document_id)
                    })
            });
        let Some(document) = document else {
            self.knowledge_page.error = Some(error_message);
            return None;
        };
        let Ok(content) = oxideterm_ai::rag_get_document_content(&store, document_id) else {
            self.knowledge_page.error = Some(error_message);
            return None;
        };
        if std::fs::create_dir_all(&edit_dir).is_err() {
            self.knowledge_page.error = Some(error_message);
            return None;
        }
        #[cfg(unix)]
        if set_private_permissions(&edit_dir, 0o700).is_err() {
            self.knowledge_page.error = Some(error_message);
            return None;
        }
        let extension = if document.format == "plaintext" {
            "txt"
        } else {
            "md"
        };
        let path = edit_dir.join(format!("{}.{}", document.id, extension));
        if std::fs::write(&path, content).is_err() {
            self.knowledge_page.error = Some(error_message);
            return None;
        }
        #[cfg(unix)]
        if set_private_permissions(&path, 0o600).is_err() {
            let _ = std::fs::remove_file(&path);
            self.knowledge_page.error = Some(error_message);
            return None;
        }
        Some((
            path.clone(),
            oxideterm_settings_model::KnowledgeExternalEdit {
                doc_id: document.id,
                path,
                version: document.version,
            },
        ))
    }

    pub(in crate::workspace) fn finish_knowledge_external_open(
        &mut self,
        edit: oxideterm_settings_model::KnowledgeExternalEdit,
        opened: bool,
        error_message: String,
    ) {
        if opened {
            self.knowledge_page.external_edit = Some(edit);
            self.knowledge_page.error = None;
        } else {
            let _ = std::fs::remove_file(&edit.path);
            self.knowledge_page.error = Some(error_message);
        }
    }

    pub(in crate::workspace) fn sync_knowledge_external_edit(
        &mut self,
        error_message: String,
    ) -> KnowledgeExternalSyncOutcome {
        let Some(edit) = self.knowledge_page.external_edit.clone() else {
            return KnowledgeExternalSyncOutcome::NoEdit;
        };
        let Ok(content) = std::fs::read_to_string(&edit.path) else {
            let _ = std::fs::remove_file(&edit.path);
            self.knowledge_page.external_edit = None;
            self.knowledge_page.error = Some(error_message);
            return KnowledgeExternalSyncOutcome::Failed;
        };
        let store = self.rag_store();
        match oxideterm_ai::rag_get_document_content(&store, &edit.doc_id) {
            Ok(current) if current == content => {
                let _ = std::fs::remove_file(&edit.path);
                self.knowledge_page.external_edit = None;
                self.knowledge_page.error = None;
                return KnowledgeExternalSyncOutcome::NoChanges;
            }
            Ok(_) => {}
            Err(_) => {
                self.knowledge_page.error = Some(error_message);
                return KnowledgeExternalSyncOutcome::Failed;
            }
        }
        match oxideterm_ai::rag_update_document(&store, &edit.doc_id, content, Some(edit.version)) {
            Ok(_) => {
                let _ = std::fs::remove_file(&edit.path);
                self.knowledge_page.external_edit = None;
                self.knowledge_page.error = None;
                KnowledgeExternalSyncOutcome::Updated
            }
            Err(error) => {
                if error.contains("Version conflict") {
                    self.knowledge_page.external_edit = None;
                }
                self.knowledge_page.error = Some(error_message);
                KnowledgeExternalSyncOutcome::Failed
            }
        }
    }

    pub(in crate::workspace) fn start_knowledge_import(
        &mut self,
        paths: impl std::future::Future<Output = Option<Vec<PathBuf>>> + 'static,
        collection_id: String,
        error_message: String,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.knowledge_import_task.is_some() {
            return false;
        }
        let store = self.rag_store();
        let task = cx.spawn(async move |entity, cx| {
            let Some(paths) = paths.await.filter(|paths| !paths.is_empty()) else {
                let _ = entity.update(cx, |entity, _cx| {
                    entity.knowledge_import_task = None;
                });
                return;
            };
            let total = paths.len();
            let _ = entity.update(cx, |entity, cx| {
                entity.knowledge_page.import_progress = Some((0, total));
                entity.knowledge_page.error = None;
                entity.emit_knowledge_page_changed(cx);
            });
            let mut failed = false;
            for (index, path) in paths.iter().enumerate() {
                if oxideterm_settings_model::import_knowledge_file(&store, &collection_id, path)
                    .is_err()
                {
                    failed = true;
                }
                let current = index + 1;
                let _ = entity.update(cx, |entity, cx| {
                    entity.knowledge_page.import_progress = Some((current, total));
                    entity.emit_knowledge_page_changed(cx);
                });
                if failed {
                    break;
                }
            }
            let _ = entity.update(cx, |entity, cx| {
                entity.knowledge_page.import_progress = None;
                entity.knowledge_page.error = failed.then_some(error_message);
                entity.knowledge_import_task = None;
                entity.emit_knowledge_page_changed(cx);
            });
        });
        self.knowledge_import_task = Some(task);
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::workspace) fn start_knowledge_embeddings(
        &mut self,
        collection_id: String,
        provider: oxideterm_ai::AiProviderView,
        model: String,
        requires_api_key: bool,
        missing_key_error: String,
        embedding_error: String,
        partial_failure_template: String,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.knowledge_embedding_task.is_some() {
            return false;
        }
        let store = self.rag_store();
        let key_store = self.key_store.clone();
        let provider_id = provider.id.clone();
        let task_runtime = self.task_runtime.clone();
        let task = cx.spawn(async move |entity, cx| {
            let api_key = if requires_api_key {
                let key = task_runtime
                    .spawn_blocking(move || key_store.get_provider_key(&provider_id).ok().flatten())
                    .await
                    .ok()
                    .flatten();
                match key {
                    Some(key) if !key.trim().is_empty() => {
                        Some(oxideterm_ai::SharedAiProviderKey::new(key))
                    }
                    _ => {
                        let _ = entity.update(cx, |entity, cx| {
                            entity.knowledge_page.embedding_config_expanded = true;
                            entity.knowledge_page.error = Some(missing_key_error);
                            entity.knowledge_embedding_task = None;
                            entity.emit_knowledge_page_changed(cx);
                        });
                        return;
                    }
                }
            } else {
                None
            };
            let mut pending =
                match oxideterm_ai::rag_get_pending_embeddings(&store, &collection_id, Some(500)) {
                    Ok(pending) => pending,
                    Err(_) => {
                        let _ = entity.update(cx, |entity, cx| {
                            entity.knowledge_page.error = Some(embedding_error);
                            entity.knowledge_embedding_task = None;
                            entity.emit_knowledge_page_changed(cx);
                        });
                        return;
                    }
                };
            if pending.is_empty() {
                let _ = entity.update(cx, |entity, _cx| {
                    entity.knowledge_embedding_task = None;
                });
                return;
            }
            let total = pending.len();
            let _ = entity.update(cx, |entity, cx| {
                entity.knowledge_page.embedding_config_expanded = true;
                entity.knowledge_page.embedding_progress = Some((0, total));
                entity.knowledge_page.error = None;
                entity.emit_knowledge_page_changed(cx);
            });
            let mut processed = 0usize;
            let mut failed_count = 0usize;
            while !pending.is_empty() {
                let batch_len = pending
                    .len()
                    .min(oxideterm_settings_model::KNOWLEDGE_EMBEDDING_BATCH_SIZE);
                let mut batch = pending.drain(..batch_len).collect::<Vec<_>>();
                // Move raw chunk allocations into the provider boundary; do
                // not clone or pre-sanitize user document content here.
                let texts = batch
                    .iter_mut()
                    .map(|pending| std::mem::take(&mut pending.content))
                    .collect::<Vec<_>>();
                match oxideterm_ai::embed_texts(&provider, api_key.as_ref(), &model, texts).await {
                    Ok(vectors) => {
                        let embeddings = batch
                            .into_iter()
                            .zip(vectors)
                            .map(|(pending, vector)| oxideterm_ai::RagEmbeddingInputRequest {
                                chunk_id: pending.chunk_id,
                                vector,
                            })
                            .collect::<Vec<_>>();
                        if oxideterm_ai::rag_store_embeddings(
                            &store,
                            oxideterm_ai::RagStoreEmbeddingsRequest {
                                embeddings,
                                model_name: model.clone(),
                            },
                        )
                        .is_err()
                        {
                            failed_count += batch_len;
                        }
                    }
                    Err(_) => failed_count += batch_len,
                }
                processed += batch_len;
                let _ = entity.update(cx, |entity, cx| {
                    entity.knowledge_page.embedding_progress = Some((processed, total));
                    entity.emit_knowledge_page_changed(cx);
                });
            }
            let _ = entity.update(cx, |entity, cx| {
                entity.knowledge_page.embedding_progress = None;
                entity.knowledge_page.error = if failed_count == 0 {
                    None
                } else {
                    Some(
                        partial_failure_template
                            .replace("{{failed}}", &failed_count.to_string())
                            .replace("{{total}}", &total.to_string()),
                    )
                };
                entity.knowledge_embedding_task = None;
                entity.emit_knowledge_page_changed(cx);
            });
        });
        self.knowledge_embedding_task = Some(task);
        true
    }
}

#[cfg(unix)]
fn set_private_permissions(path: &std::path::Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(mode);
    std::fs::set_permissions(path, permissions)
}
