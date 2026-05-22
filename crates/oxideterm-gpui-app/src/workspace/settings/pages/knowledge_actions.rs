impl WorkspaceApp {
    pub(in crate::workspace) fn knowledge_create_collection(&mut self, cx: &mut Context<Self>) {
        let name = self.knowledge_new_collection_name.trim().to_string();
        if name.is_empty() {
            cx.notify();
            return;
        }
        match oxideterm_ai::rag_create_collection(
            &self.ai_rag_store,
            oxideterm_ai::RagCreateCollectionRequest {
                name,
                scope: oxideterm_ai::RagDocScopeRequest::Global,
            },
        ) {
            Ok(collection) => {
                self.knowledge_selected_collection_id = Some(collection.id);
                self.knowledge_new_collection_name.clear();
                self.settings_input_draft.clear();
                self.knowledge_error = None;
            }
            Err(error) => {
                self.knowledge_error = Some(error);
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_create_blank_document(&mut self, cx: &mut Context<Self>) {
        let Some(collection_id) = self.knowledge_selected_collection_id.clone().or_else(|| {
            oxideterm_ai::rag_list_collections(&self.ai_rag_store, None)
                .ok()
                .and_then(|collections| collections.first().map(|collection| collection.id.clone()))
        }) else {
            cx.notify();
            return;
        };
        let title = self.knowledge_new_document_title.trim().to_string();
        if title.is_empty() {
            cx.notify();
            return;
        }
        match oxideterm_ai::rag_create_blank_document(
            &self.ai_rag_store,
            oxideterm_ai::RagCreateBlankDocumentRequest {
                collection_id,
                title,
                format: self.knowledge_new_document_format.clone(),
            },
        ) {
            Ok(document) => {
                self.knowledge_new_document_title.clear();
                self.settings_input_draft.clear();
                self.knowledge_error = None;
                self.knowledge_open_external(document.id, cx);
            }
            Err(error) => {
                self.knowledge_error = Some(error);
            }
        }
        cx.notify();
    }

    fn knowledge_delete_collection(&mut self, collection_id: String, cx: &mut Context<Self>) {
        match oxideterm_ai::rag_delete_collection(&self.ai_rag_store, &collection_id) {
            Ok(()) => {
                if self.knowledge_selected_collection_id.as_deref() == Some(collection_id.as_str())
                {
                    self.knowledge_selected_collection_id = None;
                }
                self.knowledge_external_edit = None;
                self.knowledge_error = None;
            }
            Err(error) => {
                self.knowledge_error = Some(error);
            }
        }
        cx.notify();
    }

    fn knowledge_delete_document(&mut self, document_id: String, cx: &mut Context<Self>) {
        match oxideterm_ai::rag_remove_document(&self.ai_rag_store, &document_id) {
            Ok(()) => {
                if self
                    .knowledge_external_edit
                    .as_ref()
                    .is_some_and(|edit| edit.doc_id == document_id)
                {
                    self.knowledge_external_edit = None;
                }
                self.knowledge_error = None;
            }
            Err(error) => {
                self.knowledge_error = Some(error);
            }
        }
        cx.notify();
    }

    fn knowledge_reindex(&mut self, collection_id: String, cx: &mut Context<Self>) {
        if self.knowledge_reindex_progress.is_some() {
            cx.notify();
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_task = cancel.clone();
        let store = self.ai_rag_store.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.knowledge_reindex_progress = Some((0, 0));
        self.knowledge_reindex_cancel = Some(cancel);
        self.knowledge_reindex_rx = Some(rx);
        self.knowledge_error = None;
        self.schedule_knowledge_reindex_poll(cx);
        self.forwarding_runtime.spawn(async move {
            let mut last_emitted = 0usize;
            let mut on_progress = |current: usize, total: usize| {
                if current == total || current.saturating_sub(last_emitted) >= 10 {
                    let _ = tx.send(KnowledgeReindexDelivery::Progress { current, total });
                    last_emitted = current;
                }
            };
            let result = oxideterm_ai::rag_reindex_collection_with_progress(
                &store,
                &collection_id,
                Some(cancel_for_task.as_ref()),
                Some(&mut on_progress),
            );
            let _ = tx.send(KnowledgeReindexDelivery::Finished(result));
        });
        cx.notify();
    }

    fn knowledge_cancel_reindex(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.knowledge_reindex_cancel.as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
        cx.notify();
    }

    fn poll_knowledge_reindex_results(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = self.knowledge_reindex_rx.take() else {
            return;
        };
        let mut keep_rx = true;
        while let Ok(delivery) = rx.try_recv() {
            match delivery {
                KnowledgeReindexDelivery::Progress { current, total } => {
                    self.knowledge_reindex_progress = Some((current, total));
                }
                KnowledgeReindexDelivery::Finished(result) => {
                    keep_rx = false;
                    self.knowledge_reindex_progress = None;
                    self.knowledge_reindex_cancel = None;
                    if let Err(error) = result {
                        let message = format!(
                            "{}: {error}",
                            self.i18n.t("settings_view.knowledge.error_reindex")
                        );
                        self.knowledge_error = None;
                        self.push_ai_settings_toast(message, TerminalNoticeVariant::Error);
                    } else {
                        self.knowledge_error = None;
                    }
                }
            }
        }
        if keep_rx {
            self.knowledge_reindex_rx = Some(rx);
        }
        cx.notify();
    }

    fn schedule_knowledge_reindex_poll(&mut self, cx: &mut Context<Self>) {
        if self.knowledge_reindex_polling {
            return;
        }
        self.knowledge_reindex_polling = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(33)).await;
            let _ = weak.update(cx, |this, cx| {
                this.knowledge_reindex_polling = false;
                if this.knowledge_reindex_rx.is_some() {
                    this.poll_knowledge_reindex_results(cx);
                    this.schedule_knowledge_reindex_poll(cx);
                }
            });
        })
        .detach();
    }

    fn knowledge_import_files(
        &mut self,
        collection_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.knowledge_import_progress.is_some() {
            return;
        }
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.knowledge.import_files"),
            )),
        });
        let store = self.ai_rag_store.clone();
        let error_title = self.i18n.t("settings_view.knowledge.error_import");
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let total = paths.len();
            if total == 0 {
                return;
            }
            let _ = weak.update(cx, |this, cx| {
                this.knowledge_import_progress = Some((0, total));
                this.knowledge_error = None;
                cx.notify();
            });
            let mut result = Ok(());
            for (index, path) in paths.iter().enumerate() {
                result = import_knowledge_file(&store, &collection_id, path).map(|_| ());
                let current = index + 1;
                let failed = result.is_err();
                let _ = weak.update(cx, |this, cx| {
                    this.knowledge_import_progress = Some((current, total));
                    cx.notify();
                });
                if failed {
                    break;
                }
            }
            let _ = weak.update(cx, |this, cx| {
                this.knowledge_import_progress = None;
                if let Err(error) = result {
                    let message = format!("{error_title}: {error}");
                    this.knowledge_error = None;
                    this.push_ai_settings_toast(message, TerminalNoticeVariant::Error);
                } else {
                    this.knowledge_error = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn knowledge_generate_embeddings(&mut self, collection_id: String, cx: &mut Context<Self>) {
        if self.knowledge_embedding_progress.is_some() {
            return;
        }
        let settings = self.settings_store.settings().clone();
        let resolved = oxideterm_ai::resolve_ai_embedding_provider(
            &settings.ai.providers,
            settings.ai.active_provider_id.as_deref(),
            settings.ai.embedding_config.as_ref(),
            None,
        );
        let Some(provider) = resolved.provider.clone() else {
            let message = self
                .i18n
                .t("settings_view.knowledge.error_no_embedding_support");
            self.knowledge_embedding_config_expanded = true;
            self.knowledge_error = None;
            self.push_ai_settings_toast(message, TerminalNoticeVariant::Error);
            cx.notify();
            return;
        };
        if resolved.reason == oxideterm_ai::AiEmbeddingProviderReason::UnsupportedProvider
            || resolved.reason == oxideterm_ai::AiEmbeddingProviderReason::NoProvider
        {
            let message = self
                .i18n
                .t("settings_view.knowledge.error_no_embedding_support");
            self.knowledge_embedding_config_expanded = true;
            self.knowledge_error = None;
            self.push_ai_settings_toast(message, TerminalNoticeVariant::Error);
            cx.notify();
            return;
        }
        if resolved.reason == oxideterm_ai::AiEmbeddingProviderReason::MissingModel {
            let message = self
                .i18n
                .t("settings_view.knowledge.error_no_embedding_model");
            self.knowledge_embedding_config_expanded = true;
            self.knowledge_error = None;
            self.push_ai_settings_toast(message, TerminalNoticeVariant::Error);
            cx.notify();
            return;
        }
        let store = self.ai_rag_store.clone();
        let key_store = self.ai_key_store.clone();
        let key_provider_id = provider.id.clone();
        let key_lookup_runtime = self.forwarding_runtime.clone();
        let requires_api_key = oxideterm_ai::ai_embedding_requires_api_key(&provider);
        let api_key_error = self
            .i18n
            .t("settings_view.knowledge.error_no_embedding_api_key");
        let error_title = self
            .i18n
            .t("settings_view.knowledge.error_generate_embeddings");
        let partial_template = self
            .i18n
            .t("settings_view.knowledge.embedding_partial_failure");
        let model = resolved.model.clone();
        cx.spawn(async move |weak, cx| {
            let api_key = if requires_api_key {
                let key_lookup = key_lookup_runtime
                    .spawn_blocking(move || key_store.get_provider_key(&key_provider_id).ok().flatten())
                    .await
                    .ok()
                    .flatten();
                match key_lookup {
                    Some(key) if !key.trim().is_empty() => Some(key),
                    _ => {
                        let _ = weak.update(cx, |this, cx| {
                            this.knowledge_embedding_config_expanded = true;
                            this.knowledge_error = None;
                            this.push_ai_settings_toast(
                                api_key_error,
                                TerminalNoticeVariant::Error,
                            );
                            cx.notify();
                        });
                        return;
                    }
                }
            } else {
                None
            };
            let pending =
                match oxideterm_ai::rag_get_pending_embeddings(&store, &collection_id, Some(500))
                {
                    Ok(pending) => pending,
                    Err(error) => {
                        let _ = weak.update(cx, |this, cx| {
                            let message = format!("{error_title}: {error}");
                            this.knowledge_error = None;
                            this.push_ai_settings_toast(message, TerminalNoticeVariant::Error);
                            cx.notify();
                        });
                        return;
                    }
                };
            if pending.is_empty() {
                return;
            }
            let total = pending.len();
            let _ = weak.update(cx, |this, cx| {
                this.knowledge_embedding_progress = Some((0, total));
                this.knowledge_error = None;
                cx.notify();
            });
            let mut processed = 0usize;
            let mut failed_count = 0usize;
            for batch in pending.chunks(KNOWLEDGE_EMBEDDING_BATCH_SIZE) {
                let texts = batch
                    .iter()
                    .map(|pending| pending.content.clone())
                    .collect::<Vec<_>>();
                match oxideterm_ai::embed_texts(&provider, api_key.clone(), &model, texts).await {
                    Ok(vectors) => {
                        let embeddings = batch
                            .iter()
                            .zip(vectors.into_iter())
                            .map(|(pending, vector)| oxideterm_ai::RagEmbeddingInputRequest {
                                chunk_id: pending.chunk_id.clone(),
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
                            failed_count += batch.len();
                        }
                    }
                    Err(_) => {
                        failed_count += batch.len();
                    }
                }
                processed += batch.len();
                let _ = weak.update(cx, |this, cx| {
                    this.knowledge_embedding_progress = Some((processed, total));
                    cx.notify();
                });
            }
            let _ = weak.update(cx, |this, cx| {
                this.knowledge_embedding_progress = None;
                if failed_count > 0 {
                    let detail = partial_template
                            .replace("{{failed}}", &failed_count.to_string())
                            .replace("{{total}}", &total.to_string());
                    this.knowledge_error = None;
                    this.push_ai_settings_toast(
                        format!("{error_title}: {detail}"),
                        TerminalNoticeVariant::Error,
                    );
                } else {
                    this.knowledge_error = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn knowledge_open_external(&mut self, document_id: String, cx: &mut Context<Self>) {
        if uuid::Uuid::parse_str(&document_id).is_err() {
            self.knowledge_error = Some(self.i18n.t("settings_view.knowledge.error_open_external"));
            cx.notify();
            return;
        }
        let docs = oxideterm_ai::rag_list_collections(&self.ai_rag_store, None)
            .ok()
            .into_iter()
            .flatten()
            .find_map(|collection| {
                oxideterm_ai::rag_list_documents(
                    &self.ai_rag_store,
                    &collection.id,
                    None,
                    Some(500),
                )
                .ok()
                .and_then(|page| {
                    page.documents
                        .into_iter()
                        .find(|document| document.id == document_id)
                })
            });
        let Some(document) = docs else {
            self.knowledge_error = Some(self.i18n.t("settings_view.knowledge.error_open_external"));
            cx.notify();
            return;
        };
        let content = match oxideterm_ai::rag_get_document_content(&self.ai_rag_store, &document_id)
        {
            Ok(content) => content,
            Err(error) => {
                self.knowledge_error = Some(format!(
                    "{}: {error}",
                    self.i18n.t("settings_view.knowledge.error_open_external")
                ));
                cx.notify();
                return;
            }
        };
        let edit_dir = self
            .settings_store
            .path()
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rag-edit");
        if let Err(error) = fs::create_dir_all(&edit_dir) {
            self.knowledge_error = Some(format!(
                "{}: {error}",
                self.i18n.t("settings_view.knowledge.error_open_external")
            ));
            cx.notify();
            return;
        }
        #[cfg(unix)]
        {
            let permissions_result = fs::metadata(&edit_dir).and_then(|metadata| {
                let mut permissions = metadata.permissions();
                std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o700);
                fs::set_permissions(&edit_dir, permissions)
            });
            if let Err(error) = permissions_result {
                self.knowledge_error = Some(format!(
                    "{}: {error}",
                    self.i18n.t("settings_view.knowledge.error_open_external")
                ));
                cx.notify();
                return;
            }
        }
        let extension = if document.format == "plaintext" {
            "txt"
        } else {
            "md"
        };
        let path = edit_dir.join(format!("{}.{}", document.id, extension));
        if let Err(error) = fs::write(&path, content) {
            self.knowledge_error = Some(format!(
                "{}: {error}",
                self.i18n.t("settings_view.knowledge.error_open_external")
            ));
            cx.notify();
            return;
        }
        #[cfg(unix)]
        {
            let permissions_result = fs::metadata(&path).and_then(|metadata| {
                let mut permissions = metadata.permissions();
                std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o600);
                fs::set_permissions(&path, permissions)
            });
            if let Err(error) = permissions_result {
                self.knowledge_error = Some(format!(
                    "{}: {error}",
                    self.i18n.t("settings_view.knowledge.error_open_external")
                ));
                cx.notify();
                return;
            }
        }
        let opened = open_path_external(&path).map_err(|error| error.to_string());
        match opened {
            Ok(()) => {
                self.knowledge_external_edit = Some(KnowledgeExternalEdit {
                    doc_id: document.id,
                    path,
                    version: document.version,
                });
                self.knowledge_error = None;
            }
            Err(error) => {
                self.knowledge_error = Some(format!(
                    "{}: {error}",
                    self.i18n.t("settings_view.knowledge.error_open_external")
                ));
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_sync_external_edit(
        &mut self,
        notify_no_changes: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self.knowledge_external_edit.clone() else {
            return;
        };
        let content = match fs::read_to_string(&edit.path) {
            Ok(content) => content,
            Err(error) => {
                let _ = fs::remove_file(&edit.path);
                self.knowledge_external_edit = None;
                self.knowledge_error = Some(format!(
                    "{}: {error}",
                    self.i18n.t("settings_view.knowledge.error_sync")
                ));
                cx.notify();
                return;
            }
        };
        match oxideterm_ai::rag_get_document_content(&self.ai_rag_store, &edit.doc_id) {
            Ok(current) if current == content => {
                let _ = fs::remove_file(&edit.path);
                self.knowledge_external_edit = None;
                if notify_no_changes {
                    self.push_ai_settings_toast(
                        self.i18n.t("settings_view.knowledge.doc_no_changes"),
                        TerminalNoticeVariant::Success,
                    );
                }
                cx.notify();
                return;
            }
            Ok(_) => {}
            Err(error) => {
                self.knowledge_error = Some(format!(
                    "{}: {error}",
                    self.i18n.t("settings_view.knowledge.error_sync")
                ));
                cx.notify();
                return;
            }
        }
        match oxideterm_ai::rag_update_document(
            &self.ai_rag_store,
            &edit.doc_id,
            content,
            Some(edit.version),
        ) {
            Ok(_document) => {
                let _ = fs::remove_file(&edit.path);
                self.knowledge_external_edit = None;
                self.knowledge_error = None;
                self.push_ai_settings_toast(
                    self.i18n.t("settings_view.knowledge.doc_updated"),
                    TerminalNoticeVariant::Success,
                );
            }
            Err(error) => {
                if error.contains("Version conflict") {
                    self.knowledge_external_edit = None;
                }
                self.knowledge_error = Some(format!(
                    "{}: {error}",
                    self.i18n.t("settings_view.knowledge.error_sync")
                ));
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn knowledge_confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.knowledge_delete_confirm.take() else {
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
