use super::*;

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn render_sftp_preview_markdown(
        &self,
        source: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut opts = self.localized_markdown_options();
        let (preview_pane, preview_path, markdown_scroll) = {
            let sftp_view = self.sftp_view.read(cx);
            (
                sftp_view.preview_pane,
                sftp_view.preview_path.clone(),
                sftp_view.preview_markdown_scroll.clone(),
            )
        };
        if preview_pane == Some(SftpPane::Local)
            && let Some(source_path) = preview_path.as_deref()
        {
            // Only local previews can resolve relative markdown images directly.
            // Remote SFTP markdown needs a separate asset cache before paths are
            // safe to hand to GPUI's local image renderer.
            opts = opts.with_source_path(source_path);
        }
        let code_actions = self.markdown_mermaid_actions(cx);
        div()
            .size_full()
            .p(px(16.0))
            .child(markdown_virtual_with_code_actions(
                "sftp-preview-markdown-virtual",
                &self.tokens,
                source,
                &opts,
                &markdown_scroll,
                &code_actions,
            ))
            .into_any_element()
    }

    pub(in crate::workspace::sftp) fn render_sftp_preview_font(
        &self,
        path: &str,
        mime_type: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (font_error, font_family, font_size) = {
            let sftp_view = self.sftp_view.read(cx);
            (
                sftp_view.preview_font_error.clone(),
                sftp_view.preview_font_family.clone(),
                sftp_view.preview_font_size,
            )
        };
        if let Some(error) = font_error.as_deref() {
            return self
                .render_sftp_native_asset_status("Font", path, mime_type, error, cx)
                .into_any_element();
        }
        let Some(font_family) = font_family else {
            return self.render_sftp_preview_text(self.i18n.t("sftp.preview.loading"));
        };
        let sample_font = SharedString::from(font_family.clone());
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .py(px(12.0))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .bg(rgba((theme.bg_panel << 8) | SFTP_PANEL_80_ALPHA))
                    .child(self.render_sftp_font_size_button(
                        "-",
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.sftp_view.update(cx, |sftp_view, cx| {
                                sftp_view.preview_font_size =
                                    (sftp_view.preview_font_size - 4.0).max(8.0);
                                cx.notify();
                            });
                            cx.stop_propagation();
                        }),
                    ))
                    .child(
                        div()
                            .w(px(52.0))
                            .text_center()
                            .text_size(px(SFTP_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(format!("{font_size:.0}px")),
                    )
                    .child(self.render_sftp_font_size_button(
                        "+",
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.sftp_view.update(cx, |sftp_view, cx| {
                                sftp_view.preview_font_size =
                                    (sftp_view.preview_font_size + 4.0).min(120.0);
                                cx.notify();
                            });
                            cx.stop_propagation();
                        }),
                    ))
                    .children([16.0, 24.0, 32.0, 48.0, 72.0].into_iter().map(|size| {
                        self.render_sftp_font_size_button(
                            format!("{size:.0}"),
                            (font_size - size).abs() < f32::EPSILON,
                            cx.listener(move |this, _event, _window, cx| {
                                this.sftp_view.update(cx, |sftp_view, cx| {
                                    sftp_view.preview_font_size = size;
                                    cx.notify();
                                });
                                cx.stop_propagation();
                            }),
                        )
                    }))
                    .child(
                        div()
                            .ml(px(8.0))
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(SFTP_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(font_family),
                    ),
            )
            .child(
                div()
                    .id("sftp-font-preview-scroll")
                    .flex_1()
                    .selectable_overflow_y_scroll(
                        &self.sftp_view.read(cx).font_preview_scroll,
                    )
                    .p(px(24.0))
                    .bg(rgb(theme.bg_sunken))
                    .font_family(sample_font.clone())
                    .text_color(rgb(theme.text))
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(32.0))
                            .child(self.render_sftp_font_sample_section(
                                self.i18n.t("sftp.preview.font_alphabet"),
                                "ABCDEFGHIJKLMNOPQRSTUVWXYZ\nabcdefghijklmnopqrstuvwxyz",
                                sample_font.clone(),
                                font_size,
                                1.4,
                            ))
                            .child(self.render_sftp_font_sample_section(
                                self.i18n.t("sftp.preview.font_numbers"),
                                "0123456789\n!@#$%^&*()_+-=[]{}|;:'\",.<>?/\\~`",
                                sample_font.clone(),
                                font_size,
                                1.4,
                            ))
                            .child(self.render_sftp_font_sample_section(
                                self.i18n.t("sftp.preview.font_pangram"),
                                "The quick brown fox jumps over the lazy dog.",
                                sample_font.clone(),
                                font_size,
                                1.4,
                            ))
                            .child(self.render_sftp_font_sample_section(
                                self.i18n.t("sftp.preview.font_cjk"),
                                "天地玄黄，宇宙洪荒。日月盈昃，辰宿列张。\nいろはにほへとちりぬるを\n키스의 고유조건은 입술끼리 만나는 것이다",
                                sample_font.clone(),
                                font_size,
                                1.6,
                            ))
                            .child(self.render_sftp_font_sample_section(
                                self.i18n.t("sftp.preview.font_nerd_icons"),
                                "       󰊤  󰇘  󱁤           ",
                                sample_font.clone(),
                                font_size,
                                1.4,
                            ))
                            .child(self.render_sftp_font_sample_section(
                                self.i18n.t("sftp.preview.font_code"),
                                "fn main() {\n    println!(\"Hello, 世界!\");\n    let x = 42;\n}",
                                sample_font.clone(),
                                (font_size * 0.75).max(12.0),
                                1.6,
                            ))
                            .child(self.render_sftp_font_sample_section(
                                self.i18n.t("sftp.preview.font_ligatures"),
                                "-> => == != <= >= && || :: ++ -- ** // /* */ <!-- -->",
                                sample_font,
                                font_size,
                                1.4,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_sftp_font_size_button(
        &self,
        label: impl Into<String>,
        active: bool,
        on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let text_color = if active {
            rgb(theme.text)
        } else {
            rgb(theme.text_muted)
        };
        self.workspace_toolbar_action_button(
            label.into(),
            None,
            ToolbarButtonOptions {
                background: Some(if active {
                    rgb(theme.bg_hover)
                } else {
                    rgb(theme.bg_panel)
                }),
                text_color: Some(text_color),
                hover_background: Some(rgb(theme.bg_hover)),
                hover_text_color: Some(rgb(theme.text)),
                ..ToolbarButtonOptions::compact_text_min_width(
                    ButtonVariant::Secondary,
                    ButtonRadius::Sm,
                    28.0,
                    28.0,
                    8.0,
                    SFTP_TEXT_XS,
                )
            },
            on_click,
        )
        .into_any_element()
    }

    fn render_sftp_font_sample_section(
        &self,
        title: String,
        sample: &'static str,
        font_family: SharedString,
        font_size: f32,
        line_height: f32,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .font_family(settings_ui_font_family(
                        self.settings_store
                            .settings()
                            .appearance
                            .ui_font_family
                            .as_str(),
                    ))
                    .text_size(px(SFTP_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .child(title),
            )
            .child(
                div()
                    .font_family(font_family)
                    .text_size(px(font_size))
                    .line_height(px(font_size * line_height))
                    .text_color(rgb(theme.text))
                    .child(sample),
            )
            .into_any_element()
    }

    pub(in crate::workspace::sftp) fn sftp_preview_uses_virtual_text(&self, cx: &App) -> bool {
        matches!(
            self.sftp_view.read(cx).preview_content.as_deref(),
            Some(PreviewContent::Text { .. })
        )
    }

    pub(in crate::workspace::sftp) fn render_sftp_preview_code(
        &self,
        source: &str,
        language: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let existing_editor = self.sftp_view.read(cx).preview_editor.clone();
        let editor = existing_editor.unwrap_or_else(|| {
            let tokens = self.tokens;
            let runtime_settings = self.ide_runtime_settings();
            let preview_path = self.sftp_view.read(cx).preview_path.clone();
            let name = preview_path
                .as_deref()
                .and_then(|path| std::path::Path::new(path).file_name())
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let syntax_language =
                sftp_editor_language_id(language, preview_path.as_deref(), name, source);
            let context_menu_labels = EditorContextMenuLabels {
                copy: self.i18n.t("menu.copy"),
                cut: self.i18n.t("fileManager.cut"),
                paste: self.i18n.t("menu.paste"),
                select_all: self.i18n.t("fileManager.selectAll"),
            };
            let (editor_text, _) = normalize_text_line_endings(source);
            let editor = cx.new(|cx| {
                let mut editor = TextEditorView::new(editor_text, &tokens, cx);
                editor.set_read_only(true);
                editor.set_context_menu_labels(context_menu_labels);
                editor.apply_ide_runtime_settings(
                    &tokens,
                    runtime_settings.editor_font_fallback.clone(),
                    runtime_settings.editor_font_size,
                    runtime_settings.editor_line_height,
                    runtime_settings.word_wrap,
                    runtime_settings.background_active,
                    cx,
                );
                editor.set_language(syntax_language, cx);
                editor
            });
            self.sftp_view.update(cx, |sftp, cx| {
                // The same editor entity becomes editable when the user chooses Edit.
                sftp.preview_editor = Some(editor.clone());
                cx.notify();
            });
            editor
        });
        div().size_full().child(editor).into_any_element()
    }
}
