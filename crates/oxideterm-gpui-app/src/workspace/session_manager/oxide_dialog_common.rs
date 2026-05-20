impl WorkspaceApp {
    fn render_oxide_close_button(
        &self,
        import_dialog: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .size(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .text_color(rgb(self.tokens.ui.text_muted))
            .hover(|button| button.text_color(rgb(self.tokens.ui.text)))
            .cursor_pointer()
            .child(Self::render_lucide_icon(
                LucideIcon::X,
                16.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if import_dialog {
                        this.session_manager.oxide_import_dialog = None;
                    } else {
                        this.session_manager.oxide_export_dialog = None;
                    }
                    this.session_manager.focused_input = None;
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_oxide_labeled_input(&self, label: String, input: AnyElement) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(label),
            )
            .child(input)
            .into_any_element()
    }


    fn render_oxide_card(
        &self,
        title: Option<(LucideIcon, String)>,
        children: Vec<AnyElement>,
    ) -> AnyElement {
        self.render_oxide_padded_card(OXIDE_MODAL_CARD_P, title, children)
    }

    fn render_oxide_padded_card(
        &self,
        padding: f32,
        title: Option<(LucideIcon, String)>,
        children: Vec<AnyElement>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg))
            .p(px(padding))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .when_some(title, |card, (icon, label)| {
                card.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text))
                        .child(Self::render_lucide_icon(icon, 16.0, rgb(theme.text)))
                        .child(label),
                )
            })
            .children(children)
            .into_any_element()
    }

    fn render_oxide_option_row(
        &self,
        title: String,
        description: String,
        checked: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(8.0))
            .child(self.render_oxide_checkbox(String::new(), checked, listener))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .line_height(px(16.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(description),
                    ),
            )
            .into_any_element()
    }


    fn render_oxide_progress(
        &self,
        progress: OxideTransferProgress,
        export_embed_keys: Option<bool>,
    ) -> AnyElement {
        let percent = progress.percent();
        let label = if let Some(embed_keys) = export_embed_keys {
            oxide_export_progress_label(&progress.stage, embed_keys)
        } else {
            oxide_import_progress_label(&progress.stage, progress.total)
        };
        let summary = (progress.total > 0).then(|| {
            format!("{}/{}", progress.current.min(progress.total), progress.total)
        });
        let padding = if export_embed_keys.is_some() {
            OXIDE_MODAL_CARD_P
        } else {
            16.0
        };
        self.render_oxide_padded_card(
            padding,
            None,
            vec![
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(label)
                            .when_some(summary, |body, summary| {
                                body.child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(summary),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(format!("{percent}%")),
                    )
                    .into_any_element(),
                div()
                    .h(px(8.0))
                    .w_full()
                    .overflow_hidden()
                    .rounded_full()
                    .bg(rgb(self.tokens.ui.bg_hover))
                    .child(
                        div()
                            .h_full()
                            .w(relative(percent.clamp(0, 100) as f32 / 100.0))
                            .rounded_full()
                            .bg(rgb(self.tokens.ui.accent)),
                    )
                    .into_any_element(),
            ],
        )
    }

    fn render_oxide_password_strength(&self, password: &str) -> AnyElement {
        let strength = oxide_password_strength(password);
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .gap(px(6.0))
                    .children((0..3).map(|index| {
                        div()
                            .h(px(6.0))
                            .rounded_full()
                            .bg(oxide_password_strength_bar_color(
                                strength,
                                index,
                                self.tokens.ui.border,
                                self.tokens.ui.accent,
                            ))
                    })),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(oxide_password_strength_text_color(
                        strength,
                        self.tokens.ui.text_muted,
                    ))
                    .child(oxide_password_strength_label(strength)),
            )
            .into_any_element()
    }


    fn render_oxide_tone_notice(
        &self,
        color: u32,
        title: String,
        lines: Vec<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .px_4()
            .py_3()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((color << 8) | OXIDE_TONE_BORDER_ALPHA))
            .bg(rgba((color << 8) | OXIDE_TONE_BG_ALPHA))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .text_color(rgb(color))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(self.render_selectable_text_scoped(
                        "oxide-tone-title",
                        (title.clone(), color),
                        title.clone(),
                        color,
                        cx,
                    )),
            )
            .children(lines.into_iter().enumerate().map(|(index, line)| {
                let text = format!("• {line}");
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(16.0))
                    .child(self.render_selectable_text_scoped(
                        "oxide-tone-line",
                        (color, index),
                        text,
                        color,
                        cx,
                    ))
            }))
            .into_any_element()
    }

    fn render_oxide_settings_section_grid(
        &self,
        selected: &HashSet<String>,
        import_dialog: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div().flex().flex_col().gap(px(8.0));
        let sections = if import_dialog {
            self.session_manager
                .oxide_import_dialog
                .as_ref()
                .and_then(|dialog| dialog.preview.as_ref())
                .map(|preview| preview.app_settings_section_ids.clone())
                .filter(|ids| !ids.is_empty())
                .unwrap_or_else(|| {
                    OXIDE_APP_SETTINGS_SECTIONS
                        .iter()
                        .map(|section| (*section).to_string())
                        .collect()
                })
        } else {
            OXIDE_APP_SETTINGS_SECTIONS
                .iter()
                .map(|section| (*section).to_string())
                .collect()
        };
        for section in sections {
            let id = section.clone();
            let checked = selected.contains(section.as_str());
            list = list.child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .child(self.render_oxide_checkbox(
                        String::new(),
                        checked,
                        cx.listener(move |this, _event, _window, cx| {
                            let selected = if import_dialog {
                                this.session_manager
                                    .oxide_import_dialog
                                    .as_mut()
                                    .map(|dialog| &mut dialog.selected_app_settings_sections)
                            } else {
                                this.session_manager
                                    .oxide_export_dialog
                                    .as_mut()
                                    .map(|dialog| &mut dialog.selected_app_settings_sections)
                            };
                            if let Some(selected) = selected {
                                if selected.contains(&id) {
                                    selected.remove(&id);
                                } else {
                                    selected.insert(id.clone());
                                }
                            }
                            if !import_dialog {
                                this.refresh_oxide_export_preflight();
                            }
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(oxide_settings_section_label(&section).to_string()),
                            ),
                    ),
            );
        }
        list.into_any_element()
    }

    fn render_oxide_primary_button_label(&self, busy: bool, label: String) -> String {
        if !busy {
            return label;
        }
        match label.as_str() {
            "预览" => "加载中...".to_string(),
            "确认导入" => "导入中...".to_string(),
            "导出" => "导出中...".to_string(),
            _ => label,
        }
    }

    fn render_oxide_cancel_button_label(&self, import_dialog: bool) -> String {
        if import_dialog {
            "取消".to_string()
        } else {
            "取消".to_string()
        }
    }

    fn render_oxide_subcard_bg(&self, panel: bool) -> Rgba {
        rgba(
            ((if panel {
                self.tokens.ui.bg_panel
            } else {
                self.tokens.ui.bg
            }) << 8)
                | OXIDE_SUBCARD_BG_ALPHA,
        )
    }

    fn render_oxide_section_empty_warning(
        &self,
        text: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(OXIDE_YELLOW_500))
            .child(self.render_selectable_text_scoped(
                "oxide-empty-warning",
                (),
                text,
                OXIDE_YELLOW_500,
                cx,
            ))
            .into_any_element()
    }

    fn render_oxide_footer(
        &self,
        busy: bool,
        primary_disabled: bool,
        secondary_label: String,
        primary_label: String,
        secondary_listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        primary_listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cancel_listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let primary_label = self.render_oxide_primary_button_label(busy, primary_label);
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(8.0))
            .pt(px(8.0))
            .child(
                button_with(
                    &self.tokens,
                    self.render_oxide_cancel_button_label(false),
                    ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: busy,
                    },
                )
                .on_mouse_down(MouseButton::Left, cancel_listener),
            )
            .when(!secondary_label.is_empty(), |footer| {
                footer.child(
                    button_with(
                        &self.tokens,
                        secondary_label,
                        ButtonOptions {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: busy,
                        },
                    )
                    .on_mouse_down(MouseButton::Left, secondary_listener),
                )
            })
            .child(
                button_with(
                    &self.tokens,
                    primary_label,
                    ButtonOptions {
                        variant: ButtonVariant::Default,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: busy || primary_disabled,
                    },
                )
                .min_w(px(140.0))
                .on_mouse_down(MouseButton::Left, primary_listener),
            )
            .into_any_element()
    }


    fn render_oxide_checkbox(
        &self,
        label: String,
        checked: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        checkbox(&self.tokens, label, checked).on_mouse_down(MouseButton::Left, listener)
    }

    fn render_oxide_status_line(
        &self,
        text: String,
        error: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = if error {
            self.tokens.ui.error
        } else {
            self.tokens.ui.text_muted
        };
        div()
            .px_3()
            .py_2()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(if error {
                self.tokens.ui.error
            } else {
                self.tokens.ui.border
            }))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(color))
            .child(self.render_selectable_text_scoped(
                "oxide-status-line",
                error,
                text,
                color,
                cx,
            ))
            .into_any_element()
    }

    fn render_oxide_error_banner(&self, text: String, cx: &mut Context<Self>) -> AnyElement {
        div()
            .px_3()
            .py_2()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((OXIDE_RED_500 << 8) | OXIDE_TONE_BORDER_ALPHA))
            .bg(rgba((OXIDE_RED_500 << 8) | OXIDE_TONE_BG_ALPHA))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(OXIDE_RED_500))
            .child(self.render_selectable_text_scoped(
                "oxide-error-banner",
                (),
                text,
                OXIDE_RED_500,
                cx,
            ))
            .into_any_element()
    }

}
