use oxideterm_ai::{AiImageAttachment, AiImageInput};

enum AiImageSource {
    File(std::path::PathBuf),
    Clipboard(Vec<u8>),
}

fn ai_image_model_key(provider: &str, endpoint: &str, model: &str) -> String {
    serde_json::json!([provider, endpoint, model]).to_string()
}

fn prepare_ai_image(source: AiImageSource) -> anyhow::Result<AiImageAttachment> {
    use std::io::Read;
    let (name, bytes) = match source {
        AiImageSource::File(path) => {
            let mut bytes = Vec::new();
            std::fs::File::open(&path)?
                .take(oxideterm_ai::AI_IMAGE_MAX_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            (
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                bytes,
            )
        }
        AiImageSource::Clipboard(bytes) => (String::new(), bytes),
    };
    oxideterm_ai::prepare_image_attachment(name, bytes)
}

impl WorkspaceApp {
    fn ai_image_model_setting(&self, provider: &str, endpoint: &str, model: &str) -> AiImageInput {
        self.settings_store
            .settings()
            .ai
            .extra
            .get("imageModelOverrides")
            .and_then(|models| models.get(ai_image_model_key(provider, endpoint, model)))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default()
    }

    fn ai_image_text_only(&self, provider: &str, endpoint: &str, model: &str, cx: &App) -> bool {
        self.ai.state
            .read(cx)
            .conversation_state()
            .active_conversation()
            .and_then(|conversation| conversation.session_metadata.as_ref())
            .and_then(|metadata| metadata.get("textOnlyImageModels"))
            .and_then(|models| models.get(ai_image_model_key(provider, endpoint, model)))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    }

    fn resume_ai_image_request_for_model(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self
            .ai.state
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone()
        else {
            return;
        };
        let Ok(config) = self.resolve_ai_stream_config(cx) else {
            return;
        };
        let pending = self
            .ai.state
            .update(cx, |ai, _| ai.pending_image_requests.remove(&id));
        if let Some(pending) = pending {
            self.start_ai_chat_stream_after_api_key_lookup(
                id,
                config,
                pending.request_content,
                pending.task_system_prompt,
                cx,
            );
        }
    }

    pub(in crate::workspace) fn select_ai_images(&mut self, cx: &mut Context<Self>) {
        if self.ai.state.read(cx).chat_ui().image_import.is_some() {
            return;
        }
        if !self.check_ai_image_input(cx) {
            return;
        }
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(self.i18n.t("ai.images.add").into()),
        });
        let conversation = self
            .ai.state
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone();
        let task = cx.spawn(async move |weak, cx| {
            let paths = receiver.await;
            let _ = weak.update(cx, |this, cx| {
                this.ai.state.update(cx, |ai, _| {
                    ai.set_chat_image_import(None);
                });
                if this
                    .ai.state
                    .read(cx)
                    .conversation_state()
                    .active_conversation_id
                    != conversation
                {
                    return;
                }
                if let Ok(Ok(Some(paths))) = paths {
                    this.import_ai_image_paths(paths, cx);
                }
                cx.notify();
            });
        });
        self.ai.state
            .update(cx, |ai, _| ai.set_chat_image_import(Some(task)));
    }

    pub(in crate::workspace) fn import_ai_image_paths(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.import_ai_images(paths.into_iter().map(AiImageSource::File).collect(), cx);
    }

    pub(in crate::workspace) fn paste_ai_image(
        &mut self,
        item: &gpui::ClipboardItem,
        cx: &mut Context<Self>,
    ) -> bool {
        for entry in item.entries() {
            match entry {
                gpui::ClipboardEntry::Image(image) => {
                    self.import_ai_images(vec![AiImageSource::Clipboard(image.bytes.clone())], cx);
                    return true;
                }
                gpui::ClipboardEntry::ExternalPaths(paths) => {
                    self.import_ai_image_paths(paths.paths().to_vec(), cx);
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    fn check_ai_image_input(&mut self, cx: &mut Context<Self>) -> bool {
        let error = match self.resolve_ai_stream_config(cx) {
            Ok(config)
                if config.execution_backend == AiExecutionBackend::Acp
                    || oxideterm_ai::model_image_input(
                        &config.provider_type,
                        &config.model,
                        config.image_input,
                    ) != Some(false) =>
            {
                return true;
            }
            Ok(_) => self.i18n.t("ai.images.model_unavailable"),
            Err(error) => error,
        };
        self.push_ai_settings_toast(error, TerminalNoticeVariant::Warning, cx);
        cx.notify();
        false
    }

    fn import_ai_images(&mut self, sources: Vec<AiImageSource>, cx: &mut Context<Self>) {
        if sources.is_empty() || self.ai.state.read(cx).chat_ui().image_import.is_some() {
            return;
        }
        if !self.check_ai_image_input(cx) {
            return;
        }
        if sources.len() + self.ai.state.read(cx).chat_ui().images.len()
            > oxideterm_ai::AI_IMAGE_MAX_ATTACHMENTS
        {
            self.push_ai_settings_toast(
                self.i18n.t("ai.images.limit"),
                TerminalNoticeVariant::Warning,
                cx,
            );
            return;
        }
        let conversation = self
            .ai.state
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone();
        let work = self.forwarding_runtime.spawn_blocking(move || {
            sources
                .into_iter()
                .map(prepare_ai_image)
                .collect::<anyhow::Result<Vec<_>>>()
        });
        let task = cx.spawn(async move |weak, cx| {
            let result = work.await;
            let _ = weak.update(cx, |this, cx| {
                this.ai.state
                    .update(cx, |ai, _| ai.set_chat_image_import(None));
                if this
                    .ai.state
                    .read(cx)
                    .conversation_state()
                    .active_conversation_id
                    != conversation
                {
                    return;
                }
                // The selected model may change while image preparation runs.
                if !this.check_ai_image_input(cx) {
                    return;
                }
                match result {
                    Ok(Ok(images)) => this
                        .ai.state
                        .update(cx, |ai, _| ai.chat_images_mut().extend(images)),
                    _ => this.push_ai_settings_toast(
                        this.i18n.t("ai.images.invalid"),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
                cx.notify();
            });
        });
        self.ai.state
            .update(cx, |ai, _| ai.set_chat_image_import(Some(task)));
        cx.notify();
    }

    pub(in crate::workspace) fn continue_ai_image_text_only(&mut self, id: &str, cx: &mut Context<Self>) {
    let pending = self
        .ai.state
        .update(cx, |ai, _| ai.pending_image_requests.remove(id));
    if let Some(mut pending) = pending {
        pending.config.omit_history_images = true;
        let key = ai_image_model_key(
            pending.config.provider_id.as_deref().unwrap_or_default(),
            &pending.config.base_url,
            &pending.config.model,
        );
        self.ai.state.update(cx, |ai, _| {
            if let Some(conversation) = ai
                .conversation_state_mut()
                .conversations
                .iter_mut()
                .find(|conversation| conversation.id == id)
            {
                let metadata = conversation
                    .session_metadata
                    .get_or_insert_with(|| serde_json::json!({}));
                if let Some(fields) = metadata.as_object_mut() {
                    let models = fields
                        .entry("textOnlyImageModels")
                        .or_insert_with(|| serde_json::json!({}));
                    if let Some(models) = models.as_object_mut() {
                        models.insert(key, serde_json::json!(true));
                    }
                }
            }
            ai.persist_chat_state();
        });
        self.start_ai_chat_stream_after_budget_preflight(
            pending.conversation_id,
            pending.config,
            pending.request_content,
            pending.task_system_prompt,
            pending.rag_system_prompt,
            true,
            cx,
        );
    }
    }
}
