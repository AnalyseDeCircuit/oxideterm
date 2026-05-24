impl WorkspaceApp {
    const HIGHLIGHT_PREVIEW_WRAP_CHARS: usize = 32;

    fn highlight_rules_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rules = &settings.terminal.highlight_rules;
        let limit_text = self
            .i18n
            .t("settings_view.terminal.highlight_rules.limit")
            .replace("{{count}}", &MAX_HIGHLIGHT_RULES.to_string());
        let add_disabled = rules.len() >= MAX_HIGHLIGHT_RULES;

        let mut body = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
            .shadow(oxideterm_gpui_ui::tauri_card_shadow(
                self.tokens.ui.bg_card,
            ))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_start()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(
                                        self.i18n
                                            .t("settings_view.terminal.highlight_rules.title")
                                            .to_uppercase(),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(
                                        self.i18n.t(
                                            "settings_view.terminal.highlight_rules.description",
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap(px(8.0))
                            .child(self.settings_select_control(
                                SettingsSelect::HighlightPreset,
                                self.i18n
                                    .t("settings_view.terminal.highlight_rules.add_preset"),
                                add_disabled,
                                Some(168.0),
                                cx,
                            ))
                            .child(
                                // Tauri uses the shared shadcn Button for this action. Route it
                                // through the workspace wrapper so max-rule disabled state cannot
                                // dispatch while preserving the existing Button chrome.
                                self.workspace_toolbar_action_button(
                                    self.i18n
                                        .t("settings_view.terminal.highlight_rules.add_rule"),
                                    Some(Self::render_lucide_icon(
                                        LucideIcon::Plus,
                                        14.0,
                                        rgb(self.tokens.ui.accent_text),
                                    )),
                                    ToolbarButtonOptions {
                                        button: ButtonOptions {
                                            variant: ButtonVariant::Default,
                                            size: ButtonSize::Sm,
                                            radius: ButtonRadius::Md,
                                            disabled: add_disabled,
                                        },
                                        ..ToolbarButtonOptions::default()
                                    },
                                    cx.listener(move |this, _event, _window, cx| {
                                        if this
                                            .settings_store
                                            .settings()
                                            .terminal
                                            .highlight_rules
                                            .len()
                                            < MAX_HIGHLIGHT_RULES
                                        {
                                            this.add_highlight_rule(cx);
                                        }
                                        cx.stop_propagation();
                                    }),
                                ),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .gap(px(12.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(limit_text)
                    .child(
                        self.i18n
                            .t("settings_view.terminal.highlight_rules.priority_hint"),
                    ),
            )
            .child(self.card_separator());

        if rules.is_empty() {
            body = body.child(
                div()
                    .w_full()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgba((self.tokens.ui.bg_sunken << 8) | 0x99))
                    .px(px(16.0))
                    .py(px(32.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.terminal.highlight_rules.empty")),
            );
        } else {
            for (index, rule) in rules.iter().enumerate() {
                body = body.child(self.highlight_rule_row(index, rule, rules.len(), cx));
            }
        }

        body.child(self.card_separator())
            .child(self.highlight_preview(rules))
            .into_any_element()
    }

    fn highlight_rule_row(
        &self,
        index: usize,
        rule: &HighlightRule,
        total: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status_color = if rule.enabled {
            self.tokens.ui.accent
        } else {
            self.tokens.ui.text_muted
        };
        let title = if rule.label.trim().is_empty() {
            self.i18n
                .t("settings_view.terminal.highlight_rules.untitled_rule")
        } else {
            rule.label.clone()
        };
        let mode_label = highlight_render_mode_label(rule.render_mode, &self.i18n);
        let validation_error = highlight_rule_validation_error(rule);

        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_sunken))
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(self.tokens.metrics.ui_text_sm))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(self.tokens.ui.text))
                                            .child(title),
                                    )
                                    .child(self.text_badge(
                                        if rule.enabled {
                                            self.i18n
                                                .t("settings_view.terminal.highlight_rules.enabled")
                                        } else {
                                            self.i18n.t(
                                                "settings_view.terminal.highlight_rules.disabled",
                                            )
                                        },
                                        status_color,
                                    ))
                                    .when(rule.is_regex, |row| {
                                        row.child(
                                            self.text_badge(
                                                self.i18n.t(
                                                    "settings_view.terminal.highlight_rules.regex",
                                                ),
                                                self.tokens.ui.text_muted,
                                            ),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .font_family("JetBrainsMono Nerd Font")
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(summarize_highlight_pattern(&rule.pattern)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap(px(6.0))
                            .child(self.highlight_small_button(
                                "↑".to_string(),
                                index > 0,
                                move |this, cx| this.move_highlight_rule(index, -1, cx),
                                cx,
                            ))
                            .child(self.highlight_small_button(
                                "↓".to_string(),
                                index + 1 < total,
                                move |this, cx| this.move_highlight_rule(index, 1, cx),
                                cx,
                            ))
                            .child(self.highlight_small_button(
                                self.i18n.t("settings_view.terminal.highlight_rules.delete"),
                                true,
                                move |this, cx| this.remove_highlight_rule(index, cx),
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .grid()
                    .gap(px(12.0))
                    .child(
                        self.highlight_input_block(
                            self.i18n.t("settings_view.terminal.highlight_rules.label"),
                            SettingsInput::HighlightLabel(index),
                            rule.label.clone(),
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.label_placeholder"),
                            220.0,
                            cx,
                        ),
                    )
                    .child(
                        self.highlight_input_block(
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.pattern"),
                            SettingsInput::HighlightPattern(index),
                            rule.pattern.clone(),
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.pattern_placeholder"),
                            360.0,
                            cx,
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_end()
                    .gap(px(12.0))
                    .child(
                        self.highlight_input_block(
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.foreground"),
                            SettingsInput::HighlightForeground(index),
                            rule.foreground.clone().unwrap_or_default(),
                            "#f8fafc".to_string(),
                            150.0,
                            cx,
                        ),
                    )
                    .child(
                        self.highlight_input_block(
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.background"),
                            SettingsInput::HighlightBackground(index),
                            rule.background.clone().unwrap_or_default(),
                            "#991b1b".to_string(),
                            150.0,
                            cx,
                        ),
                    )
                    .child(self.highlight_render_mode_control(index, mode_label, cx))
                    .child(
                        self.highlight_checkbox(
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.enabled"),
                            rule.enabled,
                            move |settings, value| {
                                if let Some(rule) = settings.terminal.highlight_rules.get_mut(index)
                                {
                                    rule.enabled = value;
                                }
                            },
                            cx,
                        ),
                    )
                    .child(self.highlight_checkbox(
                        self.i18n.t("settings_view.terminal.highlight_rules.regex"),
                        rule.is_regex,
                        move |settings, value| {
                            if let Some(rule) = settings.terminal.highlight_rules.get_mut(index) {
                                rule.is_regex = value;
                            }
                        },
                        cx,
                    ))
                    .child(
                        self.highlight_checkbox(
                            self.i18n
                                .t("settings_view.terminal.highlight_rules.case_sensitive"),
                            rule.case_sensitive,
                            move |settings, value| {
                                if let Some(rule) = settings.terminal.highlight_rules.get_mut(index)
                                {
                                    rule.case_sensitive = value;
                                }
                            },
                            cx,
                        ),
                    ),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(if validation_error.is_some() {
                        rgb(self.tokens.ui.warning)
                    } else {
                        rgb(self.tokens.ui.text_muted)
                    })
                    .child(validation_error.map(|reason| {
                        self.i18n.t(&format!(
                            "settings_view.terminal.highlight_rules.validation.{reason}"
                        ))
                    }).unwrap_or_else(|| {
                        self.i18n.t(if rule.is_regex {
                            "settings_view.terminal.highlight_rules.mode_hint.regex"
                        } else {
                            "settings_view.terminal.highlight_rules.mode_hint.literal"
                        })
                    })),
            )
            .into_any_element()
    }

    fn highlight_small_button(
        &self,
        label: String,
        enabled: bool,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Rule-row ghost Buttons mirror TerminalHighlightRulesSection.tsx and
        // share the same disabled action guard as other settings Buttons.
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: !enabled,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, _window, cx| {
                action(this, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn highlight_input_block(
        &self,
        label: String,
        input: SettingsInput,
        value: String,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(label),
            )
            .child(self.settings_text_input_control(input, value, placeholder, width, cx))
            .into_any_element()
    }

    fn highlight_render_mode_control(
        &self,
        index: usize,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let select_id = SettingsSelect::HighlightRenderMode(index);
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(
                        self.i18n
                            .t("settings_view.terminal.highlight_rules.render_mode"),
                    ),
            )
            .child(self.settings_select_control(select_id, value, false, Some(148.0), cx))
            .into_any_element()
    }

    fn highlight_checkbox(
        &self,
        label: String,
        checked: bool,
        setter: impl Fn(&mut PersistedSettings, bool) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        checkbox(&self.tokens, label, checked)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(
                        |settings| {
                            setter(settings, !checked);
                            settings.terminal.highlight_rules =
                                reindex_highlight_rules(settings.terminal.highlight_rules.clone());
                        },
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn highlight_preview(&self, rules: &[HighlightRule]) -> AnyElement {
        let lines = [
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_error"),
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_warning"),
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_ok"),
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_trace"),
            self.i18n
                .t("settings_view.terminal.highlight_rules.preview_line_audit"),
        ];
        let mut preview = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(0x071018))
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(
                                self.i18n
                                    .t("settings_view.terminal.highlight_rules.preview"),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_align(gpui::TextAlign::Right)
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(
                                self.i18n
                                    .t("settings_view.terminal.highlight_rules.preview_hint"),
                            ),
                    ),
            );
        let mut sample = div()
            .w_full()
            .min_w(px(0.0))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(0xffffff0d))
            .bg(rgb(0x020617))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .font_family("JetBrainsMono Nerd Font")
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .line_height(px(24.0))
            .text_color(rgb(0xe2e8f0));
        for line in lines {
            sample = sample.child(self.highlight_preview_line(&line, rules));
        }
        preview = preview.child(sample);
        preview.into_any_element()
    }

    fn highlight_preview_line(&self, line: &str, rules: &[HighlightRule]) -> AnyElement {
        let matches = accepted_highlight_preview_matches(line, rules);
        let mut row = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .flex_wrap()
            .overflow_hidden();
        let mut cursor = 0;
        for matched in matches {
            if matched.start > cursor {
                row = self.highlight_preview_plain_chunks(row, &line[cursor..matched.start]);
            }
            for chunk in Self::highlight_preview_wrapping_chunks(&line[matched.start..matched.end])
            {
                row = row.child(highlight_preview_segment(&self.tokens, &chunk, matched.rule));
            }
            cursor = matched.end;
        }
        if cursor < line.len() {
            row = self.highlight_preview_plain_chunks(row, &line[cursor..]);
        }
        row.into_any_element()
    }

    fn highlight_preview_plain_chunks(&self, mut row: Div, text: &str) -> Div {
        for chunk in Self::highlight_preview_wrapping_chunks(text) {
            row = row.child(chunk);
        }
        row
    }

    fn highlight_preview_wrapping_chunks(text: &str) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut current_chars = 0usize;

        for ch in text.chars() {
            current.push(ch);
            current_chars += 1;
            if ch.is_whitespace() || current_chars >= Self::HIGHLIGHT_PREVIEW_WRAP_CHARS {
                chunks.push(std::mem::take(&mut current));
                current_chars = 0;
            }
        }

        if !current.is_empty() {
            chunks.push(current);
        }

        chunks
    }
}
