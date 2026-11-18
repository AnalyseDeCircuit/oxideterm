use oxideterm_gpui_ai::state::images::{AiImageSource, AiImageImportNotice};

impl WorkspaceApp {
    fn resume_ai_image_request_for_model(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self
            .ai
            .state
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
            .ai
            .state
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
            .ai
            .state
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
                    .ai
                    .state
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
        self.ai
            .state
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

    fn ai_image_input_available(&self, cx: &App) -> Result<(), String> {
        let config = self.resolve_ai_stream_config(cx)?;
        if config.execution_backend == AiExecutionBackend::Acp
            || oxideterm_ai::model_image_input(&config.provider_type, &config.model, config.image_input) != Some(false)
        {
            Ok(())
        } else {
            Err(self.i18n.t("ai.images.model_unavailable"))
        }
    }

    fn check_ai_image_input(&mut self, cx: &mut Context<Self>) -> bool {
        if let Err(error) = self.ai_image_input_available(cx) {
            self.push_ai_settings_toast(error, TerminalNoticeVariant::Warning, cx);
            cx.notify();
            false
        } else { true }
    }

    fn import_ai_images(&mut self, sources: Vec<AiImageSource>, cx: &mut Context<Self>) {
        if sources.is_empty() || self.ai.state.read(cx).chat_ui().image_import.is_some() || !self.check_ai_image_input(cx) {
            return;
        }
        let validation_owner = cx.weak_entity();
        let notice_owner = validation_owner.clone();
        let started = self.ai.state.update(cx, |ai, cx| ai.import_chat_images(
            sources,
            move |cx| validation_owner.upgrade().map_or(Ok(()), |owner| owner.read(cx).ai_image_input_available(cx)),
            move |notice, cx| {
                let _ = notice_owner.update(cx, |app, cx| {
                    let (message, variant) = match notice {
                        AiImageImportNotice::Unsupported(error) => (error, TerminalNoticeVariant::Warning),
                        AiImageImportNotice::Invalid => (app.i18n.t("ai.images.invalid"), TerminalNoticeVariant::Error),
                    };
                    app.push_ai_settings_toast(message, variant, cx);
                });
            },
            cx,
        ));
        if !started {
            self.push_ai_settings_toast(self.i18n.t("ai.images.limit"), TerminalNoticeVariant::Warning, cx);
        }
    }

    pub(in crate::workspace) fn continue_ai_image_text_only(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(pending) = self.ai.state.update(cx, |ai, _| ai.consume_text_only_image_request(id)) {
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
