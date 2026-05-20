impl WorkspaceApp {
    fn render_sftp_preview_body(&self, _has_background: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let body = if self.sftp_view.preview_loading {
            self.render_sftp_preview_text(self.i18n.t("sftp.preview.loading"))
        } else if let Some(error) = &self.sftp_view.preview_error {
            self.render_sftp_preview_text(error.clone())
        } else if let Some(content) = &self.sftp_view.preview_content {
            self.render_sftp_preview_content(content, cx)
        } else {
            self.render_sftp_preview_text(String::new())
        };
        let uses_virtual_text = self.sftp_preview_uses_virtual_text();
        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .bg(rgb(theme.bg_sunken))
            .child(
                div()
                    .id("sftp-preview-scroll")
                    .flex_1()
                    .when(!uses_virtual_text, |scroll| {
                        scroll.overflow_y_scroll().p(px(16.0))
                    })
                    .text_color(rgb(theme.text))
                    .child(body),
            )
            .into_any_element()
    }

    fn render_sftp_editor_body(&self, _has_background: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let language = self
            .sftp_view
            .preview_editor_language
            .clone()
            .unwrap_or_else(|| "text".to_string());
        let encoding = self.sftp_view.preview_editor_encoding.clone();
        let (line, column) = self
            .sftp_view
            .preview_editor_input
            .as_ref()
            .map(|input| {
                let pos = input.read(cx).cursor_position();
                (pos.line + 1, pos.character + 1)
            })
            .unwrap_or((1, 1));
        let status = if self.sftp_view.preview_editor_saving {
            Some((self.i18n.t("sftp.preview.saving"), rgb(theme.text_muted)))
        } else if self.sftp_view.preview_editor_dirty {
            Some((self.i18n.t("sftp.preview.modified"), rgb(SFTP_YELLOW)))
        } else if let Some(atomic) = self.sftp_view.preview_editor_last_atomic_write {
            let key = if atomic {
                "sftp.preview.saved_atomic"
            } else {
                "sftp.preview.saved_direct"
            };
            Some((self.i18n.t(key), rgb(SFTP_GREEN)))
        } else {
            None
        };

        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .bg(rgb(theme.bg_sunken))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .when_some(self.sftp_view.preview_editor_input.clone(), |body, input| {
                        body.child(
                            CodeEditorInput::new(&input)
                                .appearance(false)
                                .h_full()
                                .font_family(settings_mono_font_family(
                                    self.settings_store.settings(),
                                ))
                                .text_size(px(SFTP_TEXT_SM))
                                .text_color(rgb(theme.text))
                                .bg(rgb(theme.bg_sunken)),
                        )
                    })
                    .when(self.sftp_view.preview_editor_input.is_none(), |body| {
                        body.child(self.render_sftp_preview_text(String::new()))
                    }),
            )
            .child(
                div()
                    .h(px(32.0))
                    .flex_none()
                    .px(px(16.0))
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(SFTP_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .child(
                        div()
                            .flex()
                            .gap(px(16.0))
                            .child(self.render_selectable_text_scoped(
                                "sftp-editor-cursor-position",
                                (),
                                format!(
                                    "{} {}, {} {}",
                                    self.i18n.t("sftp.preview.line"),
                                    line,
                                    self.i18n.t("sftp.preview.column"),
                                    column
                                ),
                                theme.text_muted,
                                cx,
                            ))
                            .child(self.render_selectable_text_scoped(
                                "sftp-editor-language",
                                (),
                                language,
                                theme.text_muted,
                                cx,
                            ))
                            .child(self.render_selectable_text_scoped(
                                "sftp-editor-encoding",
                                (),
                                format!("{} {}", self.i18n.t("sftp.preview.encoding"), encoding),
                                theme.text_muted,
                                cx,
                            )),
                    )
                .child(self.render_sftp_editor_status(status, cx)),
            )
            .into_any_element()
    }

    fn render_sftp_editor_status(
        &self,
        status: Option<(String, gpui::Rgba)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(error) = &self.sftp_view.preview_editor_save_error {
            let message = error.clone();
            if self.sftp_view.preview_editor_network_error {
                let retry_count = self.sftp_view.preview_editor_retry_count;
                let label = if retry_count > 0 {
                    format!("{} ({retry_count})", self.i18n.t("sftp.preview.retry"))
                } else {
                    self.i18n.t("sftp.preview.retry")
                };
                return div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_color(rgb(SFTP_ORANGE))
                            .child(Self::render_lucide_icon(
                                LucideIcon::WifiOff,
                                SFTP_ICON_MD,
                                rgb(SFTP_ORANGE),
                            ))
                            .child(div().max_w(px(320.0)).truncate().child(
                                self.render_selectable_text_scoped(
                                    "sftp-editor-save-error",
                                    (),
                                    message,
                                    SFTP_ORANGE,
                                    cx,
                                ),
                            )),
                    )
                    .child(
                        div()
                            .h(px(20.0))
                            .px(px(8.0))
                            .rounded(px(self.tokens.radii.sm))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .text_size(px(SFTP_TEXT_XS))
                            .text_color(rgb(SFTP_ORANGE))
                            .hover(|style| {
                                style.bg(rgba(
                                    (SFTP_ORANGE << 8) | SFTP_EDITOR_RETRY_HOVER_ALPHA,
                                ))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.retry_sftp_preview_editor_save(cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )
                            .child(Self::render_lucide_icon(
                                LucideIcon::RefreshCcw,
                                SFTP_ICON_SM,
                                rgb(SFTP_ORANGE),
                            ))
                            .child(label),
                    )
                    .into_any_element();
            }
            return div()
                .max_w(px(360.0))
                .truncate()
                .text_color(rgb(SFTP_RED))
                .child(message)
                .into_any_element();
        }

        if let Some((message, color)) = status {
            div()
                .max_w(px(360.0))
                .truncate()
                .text_color(color)
                .child(message)
                .into_any_element()
        } else {
            div().into_any_element()
        }
    }

    fn render_sftp_preview_text(&self, text: String) -> AnyElement {
        div()
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_size(px(SFTP_TEXT_XS))
            .child(text)
            .into_any_element()
    }
}
