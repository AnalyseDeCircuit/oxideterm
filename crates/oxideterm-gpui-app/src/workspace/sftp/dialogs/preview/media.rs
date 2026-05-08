impl WorkspaceApp {
    fn render_sftp_preview_pdf(&self, path: &str, mime_type: &str) -> AnyElement {
        let backend = PdfiumPreviewBackend;
        let path_buf = std::path::PathBuf::from(path);
        match backend.render_page(&path_buf, 0, 900) {
            Ok(bitmap) => {
                if let Some(image) = bitmap.into_render_image() {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            gpui::img(image)
                                .w_full()
                                .h(px(456.0))
                                .object_fit(ObjectFit::Contain),
                        )
                        .child(
                            div()
                                .text_size(px(SFTP_TEXT_XS))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(format!("PDF · {mime_type} · page 1")),
                        )
                        .into_any_element()
                } else {
                    self.render_sftp_native_asset_status(
                        "PDF",
                        path,
                        mime_type,
                        "PDFium rendered a page but GPUI could not build a bitmap.",
                    )
                    .into_any_element()
                }
            }
            Err(error) => self.render_sftp_native_asset_status(
                "PDF",
                path,
                mime_type,
                &format!("{error}"),
            )
            .into_any_element(),
        }
    }

    fn render_sftp_preview_audio(
        &self,
        path: &str,
        mime_type: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let snapshot = self.sftp_view.preview_audio.snapshot();
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path)
            .to_string();
        let duration = snapshot.duration.unwrap_or_default();
        let position = snapshot.position.min(duration);
        let progress = if duration.is_zero() {
            0.0
        } else {
            (position.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
        };
        let play_icon = if snapshot.state == AudioPreviewState::Playing {
            LucideIcon::Pause
        } else {
            LucideIcon::Play
        };
        let can_seek = snapshot.duration.is_some() && snapshot.state != AudioPreviewState::Error;

        div()
            .w_full()
            .min_h(px(456.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p_4()
            .gap(px(16.0))
            .child(
                div()
                    .text_size(px(56.0))
                    .line_height(px(64.0))
                    .text_color(rgb(theme.text_muted))
                    .child("♪"),
            )
            .child(
                div()
                    .max_w(px(448.0))
                    .truncate()
                    .text_size(px(SFTP_TEXT_SM))
                    .text_color(rgb(theme.text_muted))
                    .child(name),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(448.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .w(px(32.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .bg(rgb(theme.bg))
                            .text_color(rgb(theme.text))
                            .when(snapshot.state != AudioPreviewState::Error, |button| {
                                button.cursor_pointer().hover(move |button| {
                                    button.bg(rgb(theme.bg_hover))
                                })
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.toggle_sftp_preview_audio(cx);
                                    cx.notify();
                                }),
                            )
                            .child(Self::render_lucide_icon(play_icon, 14.0, rgb(theme.text))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h(px(6.0))
                            .rounded(px(self.tokens.radii.sm))
                            .overflow_hidden()
                            .bg(rgb(theme.bg_sunken))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(progress))
                                    .rounded(px(self.tokens.radii.sm))
                                    .bg(rgb(theme.accent)),
                            ),
                    )
                    .child(
                        div()
                            .min_w(px(92.0))
                            .text_size(px(SFTP_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(format!(
                                "{} / {}",
                                format_sftp_media_time(position),
                                format_sftp_media_time(duration)
                            )),
                    )
                    .when(can_seek, |row| {
                        row.child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded(px(self.tokens.radii.sm))
                                .text_size(px(SFTP_TEXT_XS))
                                .text_color(rgb(theme.text_muted))
                                .cursor_pointer()
                                .hover(move |button| button.bg(rgb(theme.bg_hover)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        let now = this.sftp_view.preview_audio.snapshot().position;
                                        let next = now.saturating_sub(std::time::Duration::from_secs(15));
                                        this.seek_sftp_preview_audio(next, cx);
                                        cx.notify();
                                    }),
                                )
                                .child("-15s"),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded(px(self.tokens.radii.sm))
                                .text_size(px(SFTP_TEXT_XS))
                                .text_color(rgb(theme.text_muted))
                                .cursor_pointer()
                                .hover(move |button| button.bg(rgb(theme.bg_hover)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        let snapshot = this.sftp_view.preview_audio.snapshot();
                                        let Some(duration) = snapshot.duration else {
                                            return;
                                        };
                                        let next = (snapshot.position
                                            + std::time::Duration::from_secs(15))
                                        .min(duration);
                                        this.seek_sftp_preview_audio(next, cx);
                                        cx.notify();
                                    }),
                                )
                                .child("+15s"),
                        )
                    })
                    .when_some(snapshot.error, |row, error| {
                        row.child(
                            div()
                                .text_size(px(SFTP_TEXT_XS))
                                .text_color(rgb(SFTP_RED))
                                .child(error),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(SFTP_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .child(mime_type.to_string()),
            )
            .into_any_element()
    }

    fn render_sftp_preview_video(
        &self,
        path: &str,
        mime_type: &str,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let snapshot = self.sftp_view.preview_video_surface.snapshot();
            let detail = snapshot.error.unwrap_or_else(|| {
                "Native video playback is initializing.".to_string()
            });
            let fallback = self.render_sftp_native_asset_status_with_external(
                "Video",
                path,
                mime_type,
                &detail,
                _cx,
            );
            sftp_native_video_element(
                path.to_string(),
                self.sftp_view.preview_video_surface.clone(),
                fallback,
            )
            .into_any_element()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let snapshot = self.sftp_view.preview_video_surface.snapshot();
            let detail = snapshot.error.unwrap_or_else(|| {
                format!("{} backend is unavailable", snapshot.backend)
            });
            self.render_sftp_native_asset_status_with_external(
                "Video", path, mime_type, &detail, _cx,
            )
                .into_any_element()
        }
    }

    fn render_sftp_preview_office(
        &self,
        path: &str,
        mime_type: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_sftp_native_asset_status_with_external(
            "Office",
            path,
            mime_type,
            "Office preview requires the later Office -> PDF/image conversion pipeline.",
            cx,
        )
        .into_any_element()
    }

    fn render_sftp_native_asset_status_with_external(
        &self,
        title: &str,
        path: &str,
        mime_type: &str,
        detail: &str,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.render_sftp_native_asset_status(title, path, mime_type, detail)
            .child(self.render_sftp_external_open_button(path.to_string(), cx))
    }

    fn render_sftp_native_asset_status(
        &self,
        title: &str,
        path: &str,
        mime_type: &str,
        detail: &str,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_h(px(456.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .text_size(px(SFTP_TEXT_XS))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .text_size(px(SFTP_TEXT_SM))
                    .text_color(rgb(theme.text))
                    .child(title.to_string()),
            )
            .child(mime_type.to_string())
            .child(div().max_w(px(680.0)).child(detail.to_string()))
            .child(
                div()
                    .max_w(px(680.0))
                    .truncate()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .child(path.to_string()),
            )
    }

    fn render_sftp_external_open_button(
        &self,
        path: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .mt_2()
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg))
            .px_3()
            .text_size(px(SFTP_TEXT_XS))
            .text_color(rgb(theme.text))
            .cursor_pointer()
            .hover(move |button| button.bg(rgb(theme.bg_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.open_sftp_preview_external(&path);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::ExternalLink,
                SFTP_ICON_MD,
                rgb(theme.text),
            ))
            .child(self.i18n.t("sftp.preview.open_external"))
            .into_any_element()
    }

    fn render_sftp_preview_image(
        &self,
        source: impl Into<gpui::ImageSource>,
        fallback_label: String,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        gpui::img(source)
            .w_full()
            .h(px(456.0))
            .object_fit(ObjectFit::Contain)
            .with_fallback(move || {
                div()
                    .w_full()
                    .h(px(456.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(SFTP_TEXT_SM))
                    .text_color(rgb(theme.text_muted))
                    .child(fallback_label.clone())
                    .into_any_element()
            })
            .into_any_element()
    }

}
