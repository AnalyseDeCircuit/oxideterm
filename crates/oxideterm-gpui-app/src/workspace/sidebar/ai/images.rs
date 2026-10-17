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
    use image::ImageDecoder;
    use std::io::{Cursor, Read};
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
    anyhow::ensure!(
        !bytes.is_empty() && bytes.len() <= oxideterm_ai::AI_IMAGE_MAX_BYTES,
        "image size"
    );
    let bytes = zeroize::Zeroizing::new(bytes);
    let mut reader =
        image::ImageReader::new(Cursor::new(bytes.as_slice())).with_guessed_format()?;
    anyhow::ensure!(
        matches!(
            reader.format(),
            Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
        ),
        "image format"
    );
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(160 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    anyhow::ensure!(
        u64::from(width) * u64::from(height) <= oxideterm_ai::AI_IMAGE_MAX_PIXELS,
        "image dimensions"
    );
    let orientation = decoder.orientation()?;
    let mut decoded = image::DynamicImage::from_decoder(decoder)?;
    decoded.apply_orientation(orientation);
    // Normalize orientation and remove file metadata before an explicit send.
    if decoded.width().max(decoded.height()) > 2000 {
        decoded = decoded.resize(2000, 2000, image::imageops::FilterType::Lanczos3);
    }
    let mut full = Cursor::new(Vec::new());
    decoded.write_to(&mut full, image::ImageFormat::Png)?;
    let mut mime = "image/png";
    if full.get_ref().len() > 3 * 1024 * 1024 {
        full = Cursor::new(Vec::new());
        let mut rgba = decoded.to_rgba8();
        for pixel in rgba.pixels_mut() {
            let alpha = u16::from(pixel.0[3]);
            for channel in &mut pixel.0[..3] {
                *channel = ((u16::from(*channel) * alpha + 255 * (255 - alpha)) / 255) as u8;
            }
        }
        let rgb = image::DynamicImage::ImageRgba8(rgba).to_rgb8();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut full, 90).encode_image(&rgb)?;
        mime = "image/jpeg";
    }
    anyhow::ensure!(full.get_ref().len() <= 3 * 1024 * 1024, "image size");
    let mut thumb = Cursor::new(Vec::new());
    decoded
        .thumbnail(160, 160)
        .write_to(&mut thumb, image::ImageFormat::Png)?;
    AiImageAttachment::new(
        name,
        mime.into(),
        decoded.width(),
        decoded.height(),
        full.get_ref(),
        thumb.get_ref(),
    )
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

    fn set_ai_image_model_setting(&mut self, setting: AiImageInput, cx: &mut Context<Self>) {
        let Ok(config) = self.resolve_ai_stream_config(cx) else {
            return;
        };
        let key = ai_image_model_key(
            config.provider_id.as_deref().unwrap_or_default(),
            &config.base_url,
            &config.model,
        );
        self.edit_settings(
            move |settings| {
                let overrides = settings
                    .ai
                    .extra
                    .entry("imageModelOverrides")
                    .or_insert_with(|| serde_json::json!({}));
                if let Some(overrides) = overrides.as_object_mut() {
                    if setting == AiImageInput::Auto {
                        overrides.remove(&key);
                    } else {
                        overrides.insert(key.clone(), serde_json::json!(setting));
                    }
                }
            },
            cx,
        );
        self.resume_ai_image_request_for_model(cx);
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

    fn import_ai_images(&mut self, sources: Vec<AiImageSource>, cx: &mut Context<Self>) {
        if sources.is_empty() || self.ai_entity.read(cx).chat_ui().image_import.is_some() {
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
        let mut row = div().w_full().min_w_0().flex().flex_wrap().gap(px(8.0));
        let config = self.resolve_ai_stream_config(cx).ok();
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
                .flex()
                .flex_col()
                .gap(px(3.0))
                .w(px(if expanded { 280.0 } else { 112.0 }))
                .max_w_full()
                .child(
                    self.agent_control(
                        format!("ai-image-{}-{draft}", image.id),
                        title.clone(),
                        div().w_full().child(
                            gpui::img(preview.unwrap_or(thumbnail))
                                .w_full()
                                .h(px(if expanded { 280.0 } else { 80.0 }))
                                .object_fit(gpui::ObjectFit::Contain),
                        ),
                        move |this, _, cx| this.preview_ai_image(image_copy.clone(), cx),
                        cx,
                    ),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(format!("{} × {}", image.width, image.height)),
                );
            if draft {
                if let Some(config) = &config {
                    let cost = oxideterm_ai::estimate_image_tokens(
                        &config.provider_type,
                        &config.model,
                        image.width,
                        image.height,
                    );
                    card = card.child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(
                                self.i18n
                                    .t(if cost.known_rule {
                                        "ai.images.tokens"
                                    } else {
                                        "ai.images.tokens_uncertain"
                                    })
                                    .replace("{{count}}", &cost.tokens.to_string()),
                            ),
                    );
                }
                let id = image.id.clone();
                card = card.child(self.agent_button(
                    format!("remove-ai-image-{id}"),
                    self.i18n.t("ai.images.remove"),
                    move |this, _, cx| {
                        this.ai_entity.update(cx, |ai, _| {
                            ai.chat_images_mut().retain(|image| image.id != id);
                            ai.set_chat_image_preview(None);
                        });
                        cx.notify();
                    },
                    cx,
                ));
            }
            row = row.child(card);
        }
        row.into_any_element()
    }

    fn render_ai_image_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let ai = self.ai_entity.read(cx);
        let importing = ai.chat_ui().image_import.is_some();
        let settings_open = ai.chat_ui().image_settings_open;
        let pending = ai
            .conversation_state()
            .active_conversation_id
            .as_ref()
            .and_then(|id| {
                ai.pending_image_requests
                    .get(id)
                    .map(|pending| (id.clone(), pending.config.model.clone()))
            });
        let mut column = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(8.0));
        let images = ai.chat_ui().images.clone();
        if !images.is_empty() {
            column = column.child(
                div()
                    .id("ai-draft-images")
                    .max_h(px(220.0))
                    .overflow_y_scroll()
                    .child(self.render_ai_images(&images, true, cx)),
            );
        }
        let mut actions =
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.0))
                .child(self.agent_button(
                    "add-ai-image".into(),
                    self.i18n.t(if importing {
                        "ai.images.loading"
                    } else {
                        "ai.images.add"
                    }),
                    |this, _, cx| this.select_ai_images(cx),
                    cx,
                ));
        if let Ok(config) = self.resolve_ai_stream_config(cx) {
            if config.execution_backend != AiExecutionBackend::Acp {
                actions = actions.child(self.agent_button(
                    "ai-image-capability".into(),
                    self.i18n.t("ai.images.settings"),
                    |this, _, cx| {
                        this.ai_entity
                            .update(cx, |ai, _| ai.toggle_chat_image_settings());
                        cx.notify();
                    },
                    cx,
                ));
                if settings_open {
                    let mut options = div().flex().flex_wrap().gap(px(4.0));
                    for (setting, key) in [
                        (AiImageInput::Auto, "ai.images.auto"),
                        (AiImageInput::Enabled, "ai.images.enabled"),
                        (AiImageInput::Disabled, "ai.images.disabled"),
                    ] {
                        let label = self.i18n.t(key);
                        let body = oxideterm_gpui_ui::button::button_with(
                            &self.tokens,
                            label.clone(),
                            ButtonOptions {
                                variant: if config.image_input == setting {
                                    ButtonVariant::Default
                                } else {
                                    ButtonVariant::Ghost
                                },
                                size: ButtonSize::Sm,
                                ..Default::default()
                            },
                        );
                        options = options.child(self.agent_control(
                            key.into(),
                            label,
                            body,
                            move |this, _, cx| this.set_ai_image_model_setting(setting, cx),
                            cx,
                        ));
                    }
                    column = column
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .child(config.model.clone()),
                        )
                        .child(options)
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t("ai.images.invalid")),
                        );
                }
                if !images.is_empty()
                    && oxideterm_ai::model_image_input(
                        &config.provider_type,
                        &config.model,
                        config.image_input,
                    ) != Some(true)
                {
                    column = column.child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.warning))
                            .child(self.i18n.t("ai.images.model_unavailable")),
                    );
                }
            }
        }
        column = column.child(actions);
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
        column.into_any_element()
    }
}
