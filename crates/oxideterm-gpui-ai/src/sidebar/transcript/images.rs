impl AiAssistantSidebarView {
    pub fn render_ai_images(
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
                let mut cache = self.state.read(cx).chat_ui().image_thumbnails.borrow_mut();
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
                .state
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
                                gpui::image_cache(gpui::retain_recent(
                                    format!("ai-image-cache-{}", image.id),
                                    0,
                                ))
                                .child(
                                    gpui::img(preview.unwrap_or(thumbnail))
                                        .w_full()
                                        .h(px(if expanded { 280.0 } else { 80.0 }))
                                        .object_fit(gpui::ObjectFit::Contain),
                                ),
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
                            this.state.update(cx, |ai, _| {
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

    fn preview_ai_image(&self, image: AiImageAttachment, cx: &mut App) {
        let runtime = &self.bindings.runtime;
        let request = self.bindings.request.clone();
        self.state.update(cx, |state, cx| {
            if state.chat_ui().image_preview.as_ref().is_some_and(|(id, _)| id == &image.id) {
                state.set_chat_image_preview(None);
                cx.notify();
                return;
            }
            let conversation = state.conversation_state().active_conversation_id.clone();
            let store = state.history.store.clone();
            let target = conversation.clone();
            let work = runtime.spawn_blocking(move || {
                let bytes = match image.data {
                    Some(data) => base64::engine::general_purpose::STANDARD.decode(data.as_bytes()).ok()?,
                    None => store?.image_bytes(target.as_deref()?, &image.id).ok()?.to_vec(),
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
                let _ = weak.update(cx, |state, cx| {
                    if state.conversation_state().active_conversation_id != conversation {
                        return;
                    }
                    if let Ok(Some(preview)) = result {
                        state.set_chat_image_preview(Some(preview));
                        cx.notify();
                    } else {
                        cx.defer(move |cx| request(AiTranscriptRequest::ImagePreviewFailed, None, cx));
                    }
                });
            });
            state.set_chat_image_preview_task(task);
        });
    }
}
