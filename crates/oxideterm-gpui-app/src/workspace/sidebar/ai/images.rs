use base64::Engine as _;
use oxideterm_ai::{AiImageAttachment, AiImageInput};

enum AiImageSource {
    File(std::path::PathBuf),
    Clipboard(Vec<u8>),
}

#[cfg(test)]
mod image_input_tests {
    use super::*;

    #[test]
    fn image_input_preparation_resizes_and_rejects_non_images() {
        let pixels = image::RgbImage::from_pixel(2400, 1200, image::Rgb([210, 100, 40]));
        let mut png = std::io::Cursor::new(Vec::new());
        pixels.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let prepared = prepare_ai_image(AiImageSource::Clipboard(png.into_inner())).unwrap();
        assert_eq!((prepared.width, prepared.height), (2000, 1000));
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(prepared.data.as_deref().unwrap())
            .unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgb8();
        assert_eq!(decoded.get_pixel(1000, 500).0, [210, 100, 40]);
        assert!(prepare_ai_image(AiImageSource::Clipboard(b"not an image".to_vec())).is_err());
    }
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
        self.ai_entity
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
            .ai_entity
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
            .ai_entity
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

    fn select_ai_images(&mut self, cx: &mut Context<Self>) {
        if self.ai_entity.read(cx).chat_ui().image_import.is_some() {
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
            .ai_entity
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone();
        let task = cx.spawn(async move |weak, cx| {
            let paths = receiver.await;
            let _ = weak.update(cx, |this, cx| {
                this.ai_entity.update(cx, |ai, _| {
                    ai.set_chat_image_import(None);
                });
                if this
                    .ai_entity
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
        self.ai_entity
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
        if sources.is_empty() || self.ai_entity.read(cx).chat_ui().image_import.is_some() {
            return;
        }
        if !self.check_ai_image_input(cx) {
            return;
        }
        if sources.len() + self.ai_entity.read(cx).chat_ui().images.len()
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
            .ai_entity
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
                this.ai_entity
                    .update(cx, |ai, _| ai.set_chat_image_import(None));
                if this
                    .ai_entity
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
                        .ai_entity
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
        self.ai_entity
            .update(cx, |ai, _| ai.set_chat_image_import(Some(task)));
        cx.notify();
    }

    fn preview_ai_image(&mut self, image: AiImageAttachment, cx: &mut Context<Self>) {
        let ai = self.ai_entity.read(cx);
        if ai
            .chat_ui()
            .image_preview
            .as_ref()
            .is_some_and(|(id, _)| id == &image.id)
        {
            self.ai_entity
                .update(cx, |ai, _| ai.set_chat_image_preview(None));
            cx.notify();
            return;
        }
        let conversation = ai.conversation_state().active_conversation_id.clone();
        let store = ai.history.store.clone();
        let target = conversation.clone();
        let work = self.forwarding_runtime.spawn_blocking(move || {
            let bytes = match image.data {
                Some(data) => base64::engine::general_purpose::STANDARD
                    .decode(data.as_bytes())
                    .ok()?,
                None => store?
                    .image_bytes(target.as_deref()?, &image.id)
                    .ok()?
                    .to_vec(),
            };
            let format = match image.mime_type.as_str() {
                "image/jpeg" => gpui::ImageFormat::Jpeg,
                "image/webp" => gpui::ImageFormat::Webp,
                _ => gpui::ImageFormat::Png,
            };
            Some((image.id, Arc::new(gpui::Image::from_bytes(format, bytes))))
        });
        let task = cx.spawn(async move |weak, cx| {
            let result = work.await;
            let _ = weak.update(cx, |this, cx| {
                if this
                    .ai_entity
                    .read(cx)
                    .conversation_state()
                    .active_conversation_id
                    != conversation
                {
                    return;
                }
                if let Ok(Some(preview)) = result {
                    this.ai_entity
                        .update(cx, |ai, _| ai.set_chat_image_preview(Some(preview)));
                } else {
                    this.push_ai_settings_toast(
                        this.i18n.t("ai.images.missing"),
                        TerminalNoticeVariant::Error,
                        cx,
                    );
                }
                cx.notify();
            });
        });
        self.ai_entity
            .update(cx, |ai, _| ai.set_chat_image_preview_task(task));
    }

    fn render_ai_images(
        &self,
        images: &[AiImageAttachment],
        draft: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut row = div()
            .w_full()
            .min_w_0()
            .flex()
            .when(!draft, |row| row.flex_wrap())
            .gap(px(8.0));
        for image in images {
            let thumbnail = {
                let mut cache = self
                    .ai_entity
                    .read(cx)
                    .chat_ui()
                    .image_thumbnails
                    .borrow_mut();
                if cache.len() >= 64 {
                    cache.clear();
                }
                cache
                    .entry(image.id.clone())
                    .or_insert_with(|| {
                        Arc::new(gpui::Image::from_bytes(
                            gpui::ImageFormat::Png,
                            base64::engine::general_purpose::STANDARD
                                .decode(image.thumbnail.as_bytes())
                                .unwrap_or_default(),
                        ))
                    })
                    .clone()
            };
            let preview = self
                .ai_entity
                .read(cx)
                .chat_ui()
                .image_preview
                .as_ref()
                .filter(|(id, _)| id == &image.id)
                .map(|(_, source)| source.clone());
            let expanded = preview.is_some();
            let image_copy = image.clone();
            let title = if image.name.is_empty() {
                self.i18n.t("ai.images.pasted")
            } else {
                image.name.clone()
            };
            let mut card = div()
                .relative()
                .flex_none()
                .w(px(if expanded {
                    280.0
                } else if draft {
                    80.0
                } else {
                    112.0
                }))
                .max_w_full()
                .child(
                    self.agent_control(
                        format!("ai-image-{}-{draft}", image.id),
                        title,
                        div()
                            .w_full()
                            .overflow_hidden()
                            .rounded(px(self.tokens.radii.md))
                            .child(
                                gpui::img(preview.unwrap_or(thumbnail))
                                    .w_full()
                                    .h(px(if expanded { 280.0 } else { 80.0 }))
                                    .object_fit(gpui::ObjectFit::Contain),
                            ),
                        move |this, _, cx| this.preview_ai_image(image_copy.clone(), cx),
                        cx,
                    ),
                );
            if draft {
                let id = image.id.clone();
                let label = self.i18n.t("ai.images.remove");
                let tokens = self.tokens;
                let remove = self
                    .agent_control(
                        format!("remove-ai-image-{id}"),
                        label.clone(),
                        oxideterm_gpui_ui::button::icon_button(
                            &self.tokens,
                            Self::render_lucide_icon(LucideIcon::X, 12.0, rgb(self.tokens.ui.text)),
                            IconButtonOptions {
                                background: Some(rgb(self.tokens.ui.bg_panel)),
                                ..IconButtonOptions::compact(20.0)
                            },
                        ),
                        move |this, _, cx| {
                            this.ai_entity.update(cx, |ai, _| {
                                ai.chat_images_mut().retain(|image| image.id != id);
                                ai.set_chat_image_preview(None);
                            });
                            cx.notify();
                        },
                        cx,
                    )
                    .absolute()
                    .top_0()
                    .right_0()
                    .tooltip(move |_, cx| {
                        oxideterm_gpui_ui::tooltip::tooltip_view(tokens, label.clone(), None, cx)
                    });
                card = card.child(remove);
            }
            row = row.child(card);
        }
        row.into_any_element()
    }

    fn render_ai_image_add_button(&self, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let importing = self.ai_entity.read(cx).chat_ui().image_import.is_some();
        let label = self.i18n.t(if importing {
            "ai.images.loading"
        } else {
            "ai.images.add"
        });
        let disabled = !enabled || importing;
        let body = oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(
                if importing {
                    LucideIcon::LoaderCircle
                } else {
                    LucideIcon::Image
                },
                16.0,
                rgb(self.tokens.ui.text_muted),
            ),
            IconButtonOptions {
                disabled,
                ..IconButtonOptions::compact(24.0)
            },
        );
        let tokens = self.tokens;
        self.agent_control(
            "add-ai-image".into(),
            label.clone(),
            body,
            move |this, _, cx| {
                if !disabled {
                    this.select_ai_images(cx);
                }
            },
            cx,
        )
        .tooltip(move |_, cx| {
            oxideterm_gpui_ui::tooltip::tooltip_view(tokens, label.clone(), None, cx)
        })
        .into_any_element()
    }

    fn render_ai_image_controls(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ai = self.ai_entity.read(cx);
        let pending = ai
            .conversation_state()
            .active_conversation_id
            .as_ref()
            .and_then(|id| {
                ai.pending_image_requests
                    .get(id)
                    .map(|pending| (id.clone(), pending.config.model.clone()))
            });
        let images = ai.chat_ui().images.clone();
        if images.is_empty() && pending.is_none() {
            return None;
        }
        let mut column = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(8.0));
        if !images.is_empty() {
            column = column.child(
                div()
                    .id("ai-draft-images")
                    .max_h(px(220.0))
                    .overflow_x_scroll()
                    .overflow_y_scroll()
                    .child(self.render_ai_images(&images, true, cx)),
            );
        }
        if let Some((id, model)) = pending {
            column = column
                .child(
                    div().text_size(px(self.tokens.metrics.ui_text_sm)).child(
                        self.i18n
                            .t("ai.images.history_prompt")
                            .replace("{{model}}", &model),
                    ),
                )
                .child(self.agent_button(
                    "ai-image-text-only".into(),
                    self.i18n.t("ai.images.continue_text"),
                    move |this, _, cx| {
                        let pending = this
                            .ai_entity
                            .update(cx, |ai, _| ai.pending_image_requests.remove(&id));
                        if let Some(mut pending) = pending {
                            pending.config.omit_history_images = true;
                            let key = ai_image_model_key(
                                pending.config.provider_id.as_deref().unwrap_or_default(),
                                &pending.config.base_url,
                                &pending.config.model,
                            );
                            this.ai_entity.update(cx, |ai, _| {
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
                            this.start_ai_chat_stream_after_budget_preflight(
                                pending.conversation_id,
                                pending.config,
                                pending.request_content,
                                pending.task_system_prompt,
                                pending.rag_system_prompt,
                                true,
                                cx,
                            );
                        }
                    },
                    cx,
                ));
        }
        Some(column.into_any_element())
    }
}
