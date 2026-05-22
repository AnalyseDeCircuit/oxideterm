impl WorkspaceApp {
    fn render_sftp_preview_markdown(&self, source: &str, cx: &mut Context<Self>) -> AnyElement {
        let opts = MarkdownOptions::from_theme(&self.tokens);
        div()
            .size_full()
            .p(px(16.0))
            .child(markdown_virtual_with_options(
                cx.entity(),
                "sftp-preview-markdown-virtual",
                &self.tokens,
                source,
                &opts,
                &self.sftp_view.preview_markdown_scroll,
            ))
            .into_any_element()
    }

    fn render_sftp_preview_font(
        &self,
        path: &str,
        mime_type: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        if let Some(error) = self.sftp_view.preview_font_error.as_ref() {
            return self
                .render_sftp_native_asset_status("Font", path, mime_type, error, cx)
                .into_any_element();
        }
        let Some(font_family) = self.sftp_view.preview_font_family.clone() else {
            return self.render_sftp_preview_text(self.i18n.t("sftp.preview.loading"));
        };
        let font_size = self.sftp_view.preview_font_size;
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
                            this.sftp_view.preview_font_size =
                                (this.sftp_view.preview_font_size - 4.0).max(8.0);
                            cx.stop_propagation();
                            cx.notify();
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
                            this.sftp_view.preview_font_size =
                                (this.sftp_view.preview_font_size + 4.0).min(120.0);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ))
                    .children([16.0, 24.0, 32.0, 48.0, 72.0].into_iter().map(|size| {
                        self.render_sftp_font_size_button(
                            format!("{size:.0}"),
                            (font_size - size).abs() < f32::EPSILON,
                            cx.listener(move |this, _event, _window, cx| {
                                this.sftp_view.preview_font_size = size;
                                cx.stop_propagation();
                                cx.notify();
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
                            .child(font_family.clone()),
                    ),
            )
            .child(
                div()
                    .id("sftp-font-preview-scroll")
                    .flex_1()
                    .selectable_overflow_y_scroll(
                        &self.selectable_text_scroll_handle("sftp-font-preview-scroll"),
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
        toolbar_button(
            &self.tokens,
            label.into(),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Secondary,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Sm,
                    disabled: false,
                },
                show_label: true,
                height: Some(28.0),
                min_width: Some(28.0),
                padding_x: Some(8.0),
                font_size: Some(SFTP_TEXT_XS),
                background: Some(if active {
                    rgb(theme.bg_hover)
                } else {
                    rgb(theme.bg_panel)
                }),
                text_color: Some(text_color),
                hover_background: Some(rgb(theme.bg_hover)),
                hover_text_color: Some(rgb(theme.text)),
                ..ToolbarButtonOptions::default()
            },
        )
            .on_mouse_down(MouseButton::Left, on_click)
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
                        self.settings_store.settings().appearance.ui_font_family.as_str(),
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

    fn sftp_preview_uses_virtual_text(&self) -> bool {
        matches!(
            self.sftp_view.preview_content.as_ref(),
            Some(PreviewContent::Text { .. })
        )
    }

    fn render_sftp_preview_code(&self, source: &str, language: Option<&str>) -> AnyElement {
        let theme = self.tokens.ui;
        let opts = MarkdownOptions::from_theme(&self.tokens);
        let language = language
            .filter(|language| !language.trim().is_empty())
            .unwrap_or("text")
            .to_ascii_lowercase();
        let lines = std::sync::Arc::new(sftp_preview_visual_lines(source));
        let row_count = lines.len();
        let list_lines = lines.clone();
        let font_family = settings_mono_font_family(self.settings_store.settings());
        let scroll = self.sftp_view.preview_code_scroll.clone();
        div()
            .size_full()
            .bg(rgb(theme.bg_sunken))
            .child(
                tracked_uniform_list(
                    "sftp-preview-code-virtual",
                    row_count,
                    scroll,
                    move |range, _window, _cx| {
                        let opts = opts.clone();
                        let language = language.clone();
                        let font_family = font_family.clone();
                        range
                            .map(|index| {
                                let line = &list_lines[index];
                                let content: AnyElement = if language != "text"
                                    && let Some(runs) =
                                        highlight::highlight_code(&language, &line.content, &opts)
                                {
                                    let (text, text_runs) =
                                        highlight::highlighted_runs_to_text_runs(&runs);
                                    StyledText::new(text)
                                        .with_runs(text_runs)
                                        .into_any_element()
                                } else {
                                    SharedString::from(line.content.clone()).into_any_element()
                                };
                                div()
                                    .h(px(SFTP_PREVIEW_CODE_LINE_HEIGHT))
                                    .w_full()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .font_family(font_family.clone())
                                    .text_size(px(SFTP_TEXT_XS))
                                    .line_height(px(SFTP_PREVIEW_CODE_LINE_HEIGHT))
                                    .text_color(rgb(theme.text))
                                    .child(
                                        div()
                                            .w(px(SFTP_DIFF_LINE_NUMBER_COL))
                                            .flex_none()
                                            .pr(px(12.0))
                                            .text_align(gpui::TextAlign::Right)
                                            .text_color(rgba(
                                                (theme.text_muted << 8)
                                                    | SFTP_PREVIEW_CODE_GUTTER_ALPHA,
                                            ))
                                            .child(
                                                line.line_number
                                                    .map(|line_number| line_number.to_string())
                                                    .unwrap_or_default(),
                                            ),
                                    )
                                    .child(div().flex_1().min_w(px(0.0)).child(content))
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>()
                    },
                )
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
            )
            .into_any_element()
}

}
