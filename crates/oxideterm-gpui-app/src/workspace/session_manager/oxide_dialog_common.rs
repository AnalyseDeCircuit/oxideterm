use super::*;

impl WorkspaceApp {
    pub(super) fn render_oxide_close_button(
        &self,
        import_dialog: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Tauri OxideImportModal/OxideExportModal use DialogClose with
        // opacity-70 hover:opacity-100. Keep that chrome in the shared icon
        // button primitive instead of hand-drawing another close control.
        self.workspace_icon_action_button(
            LucideIcon::X,
            16.0,
            rgb(self.tokens.ui.text_muted),
            IconButtonOptions {
                size: 24.0,
                radius: ButtonRadius::Sm,
                disabled: false,
                loading: false,
                has_background: false,
                background: None,
                border: None,
                hover_background: Some(rgba(0x00000000)),
                hover_opacity: Some(1.0),
                focus_visible: false,
                idle_opacity: 0.7,
                disabled_opacity: 0.35,
            },
            move |this, _event, _window, cx| {
                if import_dialog {
                    this.begin_oxide_import_dialog_exit(cx);
                } else {
                    this.begin_oxide_export_dialog_exit(cx);
                }
                cx.notify();
                cx.stop_propagation();
            },
            cx,
        )
        .into_any_element()
    }

    pub(super) fn render_oxide_labeled_input(
        &self,
        label: String,
        input: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.render_selectable_display_text(
                        "oxide-labeled-input",
                        &label,
                        label.clone(),
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .child(input)
            .into_any_element()
    }

    pub(super) fn render_oxide_card(
        &self,
        title: Option<(LucideIcon, String)>,
        children: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_oxide_padded_card(OXIDE_MODAL_CARD_P, title, children, cx)
    }

    pub(super) fn render_oxide_padded_card(
        &self,
        padding: f32,
        title: Option<(LucideIcon, String)>,
        children: Vec<AnyElement>,
        cx: &mut Context<Self>,
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
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "oxide-card-title",
                            label.clone(),
                            label,
                            theme.text,
                            cx,
                        )),
                )
            })
            .children(children)
            .into_any_element()
    }

    pub(super) fn render_oxide_option_row(
        &self,
        title: String,
        description: String,
        checked: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
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
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
                                "oxide-option-title",
                                title.clone(),
                                title,
                                self.tokens.ui.text,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .line_height(px(16.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
                                "oxide-option-description",
                                description.clone(),
                                description,
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_oxide_progress(
        &self,
        progress: OxideTransferProgress,
        export_embed_keys: Option<bool>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let percent = progress.percent();
        let label = if let Some(embed_keys) = export_embed_keys {
            oxide_export_progress_label(&progress.stage, embed_keys, &self.i18n)
        } else {
            oxide_import_progress_label(&progress.stage, progress.total, &self.i18n)
        };
        let summary = (progress.total > 0).then(|| {
            format!(
                "{}/{}",
                progress.current.min(progress.total),
                progress.total
            )
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
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "oxide-progress-label",
                                (&progress.stage, export_embed_keys),
                                label,
                                self.tokens.ui.text,
                                cx,
                            ))
                            .when_some(summary, |body, summary| {
                                body.child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(self.render_display_text_with_role(
                                            SelectableTextRole::PlainDocument,
                                            "oxide-progress-summary",
                                            (&progress.stage, export_embed_keys),
                                            summary,
                                            self.tokens.ui.text_muted,
                                            cx,
                                        )),
                                )
                            }),
                    )
                    .child(div().text_color(rgb(self.tokens.ui.text_muted)).child(
                        self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "oxide-progress-percent",
                            (&progress.stage, export_embed_keys),
                            format!("{percent}%"),
                            self.tokens.ui.text_muted,
                            cx,
                        ),
                    ))
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
            cx,
        )
    }

    pub(super) fn render_oxide_password_strength(
        &self,
        password: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let strength = oxide_password_strength(password);
        let text_color = match strength {
            OxidePasswordStrength::Weak => OXIDE_YELLOW_500,
            OxidePasswordStrength::Fair => self.tokens.ui.text_muted,
            OxidePasswordStrength::Strong => OXIDE_GREEN_500,
        };
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
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "oxide-password-strength",
                        strength as u8,
                        oxide_password_strength_label(strength, &self.i18n),
                        text_color,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_oxide_tone_notice(
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

    pub(super) fn render_oxide_settings_section_grid(
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
                        div().flex().flex_col().child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .text_color(rgb(self.tokens.ui.text))
                                .child(oxide_settings_section_label(&section, &self.i18n)),
                        ),
                    ),
            );
        }
        list.into_any_element()
    }

    pub(super) fn render_oxide_primary_button_label(&self, busy: bool, label: String) -> String {
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

    pub(super) fn render_oxide_cancel_button_label(&self, import_dialog: bool) -> String {
        if import_dialog {
            "取消".to_string()
        } else {
            "取消".to_string()
        }
    }

    pub(super) fn render_oxide_subcard_bg(&self, panel: bool) -> Rgba {
        rgba(
            ((if panel {
                self.tokens.ui.bg_panel
            } else {
                self.tokens.ui.bg
            }) << 8)
                | OXIDE_SUBCARD_BG_ALPHA,
        )
    }

    pub(super) fn render_oxide_section_empty_warning(
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

    pub(super) fn render_oxide_footer(
        &self,
        busy: bool,
        primary_disabled: bool,
        secondary_label: String,
        primary_label: String,
        focused_action: Option<OxideDialogFooterAction>,
        secondary_listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>)
        + 'static,
        primary_listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cancel_listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let primary_label = self.render_oxide_primary_button_label(busy, primary_label);
        let cancel_disabled = busy;
        let secondary_disabled = busy;
        let primary_disabled = busy || primary_disabled;
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(8.0))
            .pt(px(8.0))
            .child(self.render_oxide_footer_click_action(
                self.render_oxide_cancel_button_label(false),
                ButtonVariant::Outline,
                OxideDialogFooterAction::Cancel,
                focused_action,
                cancel_disabled,
                None,
                cancel_listener,
                cx,
            ))
            .when(!secondary_label.is_empty(), |footer| {
                footer.child(self.render_oxide_footer_click_action(
                    secondary_label,
                    ButtonVariant::Outline,
                    OxideDialogFooterAction::Secondary,
                    focused_action,
                    secondary_disabled,
                    None,
                    secondary_listener,
                    cx,
                ))
            })
            .child(self.render_oxide_footer_click_action(
                primary_label,
                ButtonVariant::Default,
                OxideDialogFooterAction::Primary,
                focused_action,
                primary_disabled,
                Some(140.0),
                primary_listener,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_oxide_footer_click_action(
        &self,
        label: String,
        variant: ButtonVariant,
        action: OxideDialogFooterAction,
        focused_action: Option<OxideDialogFooterAction>,
        disabled: bool,
        min_width: Option<f32>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        // Tauri DialogFooter buttons share the same disabled guard for mouse
        // activation and keyboard FocusCycle activation. Keep .oxide import
        // and export footers on one action wrapper instead of per-dialog
        // `when(!disabled)` blocks.
        self.workspace_modal_footer_action_button(
            label,
            variant,
            action,
            disabled,
            focused_action,
            min_width,
            listener,
            cx,
        )
    }

    pub(super) fn render_oxide_checkbox(
        &self,
        label: String,
        checked: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        checkbox(&self.tokens, label, checked).on_mouse_down(MouseButton::Left, listener)
    }

    pub(super) fn render_oxide_status_line(
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
            .child(self.render_selectable_text_scoped("oxide-status-line", error, text, color, cx))
            .into_any_element()
    }

    pub(super) fn render_oxide_error_banner(
        &self,
        text: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
