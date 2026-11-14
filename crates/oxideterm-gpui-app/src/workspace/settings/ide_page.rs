use super::*;

pub(in crate::workspace) const IDE_SETTINGS_AGENT_DOT_SIZE: f32 = 4.0; // Tauri w-1 h-1.
pub(in crate::workspace) const IDE_SETTINGS_AGENT_DOT_TOP_MARGIN: f32 = 6.0; // Tauri mt-1.5.
pub(in crate::workspace) const IDE_SETTINGS_AGENT_SUBTLE_BORDER_ALPHA: u8 = 0x80; // Tauri border/50.

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_ide_section(
        &self,
        section: SettingsSectionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        match section {
            SettingsSectionId::IdeEditing => self.settings_card(
                "settings_view.ide.editing",
                "settings_view.ide.editing_hint",
                self.separated_settings_rows(vec![
                    self.bool_row(
                        "settings_view.ide.auto_save",
                        "settings_view.ide.auto_save_hint",
                        settings.ide.auto_save,
                        set_ide_auto_save,
                        cx,
                    ),
                    self.bool_row(
                        "settings_view.ide.word_wrap",
                        "settings_view.ide.word_wrap_hint",
                        settings.ide.word_wrap,
                        set_ide_word_wrap,
                        cx,
                    ),
                ]),
            ),
            SettingsSectionId::IdeTypography => self.ide_typography_card(settings, cx),
            SettingsSectionId::IdeAgent => self.ide_agent_card(settings, cx),
            SettingsSectionId::IdeAgentPrivacy => self.ide_agent_privacy_card(),
            _ => div().into_any_element(),
        }
    }

    pub(in crate::workspace) fn ide_typography_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let custom_cjk = self.settings_workspace.read(cx).ide_cjk_font_custom
            || settings
                .ide
                .cjk_font_family
                .as_deref()
                .is_some_and(cjk_font_is_custom);
        let mut font_group = div().w_full().min_w(px(0.0)).flex().flex_col().gap(px(12.0)).child(
            self.select_setting_row(
                "settings_view.ide.font_family",
                "settings_view.ide.font_family_hint",
                SettingsSelect::IdeFontFamily,
                settings
                    .ide
                    .font_family
                    .map(font_family_label)
                    .unwrap_or_else(|| self.i18n.t("settings_view.ide.follow_terminal")),
                self.tokens.metrics.settings_select_width,
                cx,
            ),
        );
        if settings.ide.font_family == Some(oxideterm_settings::FontFamily::Custom) {
            font_group = font_group.child(self.setting_row(
                "settings_view.terminal.custom_font_stack",
                "settings_view.terminal.custom_font_stack_hint",
                self.settings_text_input_control(
                    SettingsInput::IdeCustomFontFamily,
                    settings.ide.custom_font_family.clone(),
                    "'Sarasa Fixed SC', 'Fira Code', monospace".to_string(),
                    SETTINGS_TERMINAL_CUSTOM_FONT_INPUT_WIDTH,
                    cx,
                ),
                cx,
            ));
        }

        let mut cjk_group = div().w_full().min_w(px(0.0)).flex().flex_col().gap(px(12.0)).child(
            self.select_setting_row(
                "settings_view.ide.cjk_font_family",
                "settings_view.ide.cjk_font_family_hint",
                SettingsSelect::IdeCjkFontFamily,
                if custom_cjk {
                    self.i18n.t("settings_view.terminal.cjk_font_custom")
                } else {
                    settings
                        .ide
                        .cjk_font_family
                        .as_deref()
                        .map(|family| terminal_cjk_font_label(family, &self.i18n))
                        .unwrap_or_else(|| self.i18n.t("settings_view.ide.follow_terminal"))
                },
                self.tokens.metrics.settings_select_width,
                cx,
            ),
        );
        if custom_cjk {
            cjk_group = cjk_group.child(self.setting_row(
                "settings_view.terminal.custom_cjk_font",
                "settings_view.terminal.custom_cjk_font_hint",
                self.settings_text_input_control(
                    SettingsInput::IdeCjkFontFamily,
                    settings.ide.cjk_font_family.clone().unwrap_or_default(),
                    "PingFang SC".to_string(),
                    SETTINGS_TERMINAL_CUSTOM_FONT_INPUT_WIDTH,
                    cx,
                ),
                cx,
            ));
        }

        let rows = self.separated_settings_rows(vec![
            font_group.into_any_element(),
            cjk_group.into_any_element(),
            self.setting_row(
                "settings_view.ide.font_weight",
                "settings_view.ide.font_weight_hint",
                self.ide_number_input_with_suffix(
                    SettingsInput::IdeFontWeight,
                    settings
                        .ide
                        .font_weight
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    settings.terminal.font_weight.to_string(),
                    None,
                    cx,
                ),
                cx,
            ),
            self.setting_row(
                "settings_view.ide.font_size",
                "settings_view.ide.font_size_hint",
                self.ide_number_input_with_suffix(
                    SettingsInput::IdeFontSize,
                    settings
                        .ide
                        .font_size
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    settings.terminal.font_size.to_string(),
                    Some("px"),
                    cx,
                ),
                cx,
            ),
            self.setting_row(
                "settings_view.ide.line_height",
                "settings_view.ide.line_height_hint",
                self.ide_number_input_with_suffix(
                    SettingsInput::IdeLineHeight,
                    settings
                        .ide
                        .line_height
                        .map(compact_decimal)
                        .unwrap_or_default(),
                    compact_decimal(settings.terminal.line_height),
                    None,
                    cx,
                ),
                cx,
            ),
        ]);
        self.settings_card(
            "settings_view.ide.editor_typography",
            "settings_view.ide.editor_typography_hint",
            rows,
        )
    }

    pub(in crate::workspace) fn ide_agent_card(
        &self,
        _settings: &PersistedSettings,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_card(
            "settings_view.ide.agent_title",
            "settings_view.ide.agent_description",
            vec![
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .children([
                        self.ide_feature_row("settings_view.ide.agent_feature_atomic"),
                        self.ide_feature_row("settings_view.ide.agent_feature_watch"),
                        self.ide_feature_row("settings_view.ide.agent_feature_hash"),
                        self.ide_feature_row("settings_view.ide.agent_feature_search"),
                    ])
                    .into_any_element(),
                self.card_separator(),
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t("settings_view.ide.agent_supported")),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child("x86_64, aarch64 (Linux)"),
                            ),
                    )
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded(px(self.tokens.radii.sm))
                            .border_1()
                            .border_color(rgba(
                                (self.tokens.ui.border << 8)
                                    | IDE_SETTINGS_AGENT_SUBTLE_BORDER_ALPHA as u32,
                            ))
                            .bg(rgb(self.tokens.ui.bg_panel))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.ide.agent_binary_size")),
                    )
                    .into_any_element(),
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .italic()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ide.agent_auto_hint"))
                    .into_any_element(),
            ],
        )
    }

    pub(in crate::workspace) fn ide_agent_privacy_card(&self) -> AnyElement {
        self.settings_card_with_header(
            "settings_view.ide.agent_transparency_title",
            "",
            Some(Self::render_lucide_icon(
                LucideIcon::Shield,
                16.0,
                rgb(self.tokens.ui.info),
            )),
            None,
            vec![self.settings_callout(
                SettingsCalloutTone::Info,
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .children([
                        self.ide_privacy_row(
                            "settings_view.ide.agent_path_label",
                            "settings_view.ide.agent_path_detail",
                        ),
                        self.ide_privacy_row(
                            "settings_view.ide.agent_lifecycle_label",
                            "settings_view.ide.agent_lifecycle_detail",
                        ),
                        self.ide_privacy_row(
                            "settings_view.ide.agent_privacy_label",
                            "settings_view.ide.agent_privacy_detail",
                        ),
                    ])
                    .into_any_element(),
            )],
        )
    }

    pub(in crate::workspace) fn ide_number_input_with_suffix(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        suffix: Option<&'static str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut control = div().flex_none().flex().items_center().gap(px(4.0)).child(
            self.settings_text_input_control_with_align(
                input,
                value,
                placeholder,
                self.tokens.metrics.settings_number_input_width,
                // Tauri renders IDE typography overrides as number inputs. Keep
                // their placeholder/value centered like the terminal numeric
                // controls while ordinary text settings remain start-aligned.
                TextInputContentAlign::Center,
                cx,
            ),
        );
        if let Some(suffix) = suffix {
            control = control.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(suffix),
            );
        }
        control.into_any_element()
    }

    pub(in crate::workspace) fn ide_feature_row(&self, label_key: &str) -> AnyElement {
        self.ide_dot_row(
            self.tokens.ui.success,
            div().child(self.i18n.t(label_key)).into_any_element(),
        )
    }

    pub(in crate::workspace) fn ide_privacy_row(
        &self,
        label_key: &str,
        detail_key: &str,
    ) -> AnyElement {
        self.ide_dot_row(
            self.tokens.ui.info,
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_color(rgb(self.tokens.ui.text))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(format!("{}:", self.i18n.t(label_key))),
                )
                .child(div().w_full().min_w(px(0.0)).child(self.i18n.t(detail_key)))
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn ide_dot_row(&self, color: u32, content: AnyElement) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(8.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                div()
                    .mt(px(IDE_SETTINGS_AGENT_DOT_TOP_MARGIN))
                    .size(px(IDE_SETTINGS_AGENT_DOT_SIZE))
                    .rounded_full()
                    .bg(rgb(color))
                    .flex_none(),
            )
            .child(div().flex_1().min_w(px(0.0)).child(content))
            .into_any_element()
    }
}
