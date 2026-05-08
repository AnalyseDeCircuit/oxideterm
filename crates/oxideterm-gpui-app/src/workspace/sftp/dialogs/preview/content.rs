impl WorkspaceApp {
    fn render_sftp_preview_content(
        &self,
        content: &PreviewContent,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match content {
            PreviewContent::Text {
                data,
                mime_type,
                language,
                ..
            } if sftp_preview_is_markdown(language.as_deref(), mime_type.as_deref()) => {
                if self.sftp_view.preview_markdown_source_mode {
                    self.render_sftp_preview_code(data, Some("markdown"))
                } else {
                    self.render_sftp_preview_markdown(data, cx)
                }
            }
            PreviewContent::Text { data, language, .. } => {
                self.render_sftp_preview_code(data, language.as_deref())
            }
            PreviewContent::Image { mime_type, data } => {
                let source = format!("data:{mime_type};base64,{data}");
                self.render_sftp_preview_image(source, mime_type.clone())
            }
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind: AssetFileKind::Image,
            } => self.render_sftp_preview_image(std::path::PathBuf::from(path), mime_type.clone()),
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind: AssetFileKind::Pdf,
            } => self.render_sftp_preview_pdf(path, mime_type),
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind: AssetFileKind::Audio,
            } => self.render_sftp_preview_audio(path, mime_type, cx),
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind: AssetFileKind::Video,
            } => self.render_sftp_preview_video(path, mime_type, cx),
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind: AssetFileKind::Office,
            } => self.render_sftp_preview_office(path, mime_type, cx),
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind: AssetFileKind::Font,
            } => self.render_sftp_preview_font(path, mime_type, cx),
            PreviewContent::Hex {
                data,
                total_size,
                offset,
                chunk_size,
                has_more,
            } => self.render_sftp_preview_hex(
                data,
                *total_size,
                *offset,
                *chunk_size,
                *has_more,
                cx,
            ),
            PreviewContent::TooLarge { .. } | PreviewContent::Unsupported { .. } => {
                self.render_sftp_preview_text(preview_content_text(content))
            }
        }
    }

    fn render_sftp_preview_hex(
        &self,
        data: &str,
        total_size: u64,
        offset: u64,
        chunk_size: u64,
        has_more: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let showing = offset.saturating_add(chunk_size).min(total_size);
        let loading_more = self.sftp_view.preview_hex_loading_more;
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .mb(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(SFTP_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("sftp.preview.hex_view"))
                    .child("•")
                    .child(
                        self.i18n
                            .t("sftp.preview.showing_first")
                            .replace("{{size}}", &format_file_size(showing)),
                    )
                    .when(total_size > 0, |header| {
                        header.child("•").child(
                            self.i18n
                                .t("sftp.preview.total_size")
                                .replace("{{size}}", &format_file_size(total_size)),
                        )
                    }),
            )
            .child(
                div()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(SFTP_TEXT_XS))
                    .line_height(px(20.0))
                    .text_color(rgb(theme.text))
                    .child(data.to_string()),
            )
            .when(has_more, |body| {
                let label = if loading_more {
                    self.i18n.t("sftp.preview.loading")
                } else {
                    self.i18n.t("sftp.preview.load_more")
                };
                body.child(
                    div().mt(px(16.0)).flex().justify_center().child(
                        self.render_sftp_text_button(
                            label,
                            false,
                            cx.listener(move |this, _event, _window, cx| {
                                if !loading_more {
                                    this.load_more_sftp_preview_hex();
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                    ),
                )
            })
            .into_any_element()
    }
}
