const IDE_SETTINGS_CARD_PADDING: f32 = 20.0; // Tauri p-5.
const IDE_SETTINGS_CARD_GAP: f32 = 16.0; // Tauri space-y-4.
const IDE_SETTINGS_TOGGLE_CARD_GAP: f32 = 16.0; // Tauri flex gap between copy and control.
const IDE_SETTINGS_INPUT_WIDTH: f32 = 80.0; // Tauri w-20.
const IDE_SETTINGS_AGENT_SELECT_WIDTH: f32 = 160.0; // Tauri w-40.
const IDE_SETTINGS_AGENT_DOT_SIZE: f32 = 4.0; // Tauri w-1 h-1.
const IDE_SETTINGS_AGENT_DOT_TOP_MARGIN: f32 = 6.0; // Tauri mt-1.5.
const IDE_SETTINGS_AGENT_PRIVACY_BORDER_ALPHA: u8 = 0x33; // Tauri blue-500/20.
const IDE_SETTINGS_AGENT_PRIVACY_BG_ALPHA: u8 = 0x0d; // Tauri blue-500/5.
const IDE_SETTINGS_AGENT_SUBTLE_BORDER_ALPHA: u8 = 0x80; // Tauri border/50.
const IDE_SETTINGS_EMERALD_400: u32 = 0x34d399;
const IDE_SETTINGS_BLUE_400: u32 = 0x60a5fa;
const IDE_SETTINGS_BLUE_500: u32 = 0x3b82f6;

impl WorkspaceApp {
    fn settings_ide(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![
            self.ide_toggle_card(
                "settings_view.ide.auto_save",
                "settings_view.ide.auto_save_hint",
                settings.ide.auto_save,
                set_ide_auto_save,
                cx,
            ),
            self.ide_toggle_card(
                "settings_view.ide.word_wrap",
                "settings_view.ide.word_wrap_hint",
                settings.ide.word_wrap,
                set_ide_word_wrap,
                cx,
            ),
            self.ide_typography_card(settings, cx),
            self.ide_agent_card(settings, cx),
            self.ide_agent_privacy_card(),
        ]
    }

    fn ide_card(&self) -> Div {
        let card = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .p(px(IDE_SETTINGS_CARD_PADDING));
        self.settings_card_surface(card, self.tokens.ui.bg_card)
    }

    fn ide_toggle_card(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ide_card()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(IDE_SETTINGS_TOGGLE_CARD_GAP))
            .child(self.ide_label_block(label_key, hint_key))
            .child(
                div().flex_none().child(
                    checkbox(&self.tokens, String::new(), checked).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.edit_settings(|settings| setter(settings, !checked), cx);
                        }),
                    ),
                ),
            )
            .into_any_element()
    }

    fn ide_typography_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ide_card()
            .flex()
            .flex_col()
            .gap(px(IDE_SETTINGS_CARD_GAP))
            .child(self.ide_card_title(
                self.i18n.t("settings_view.ide.editor_typography"),
            ))
            .child(self.ide_card_description(
                self.i18n.t("settings_view.ide.editor_typography_hint"),
            ))
            .child(self.ide_setting_row(
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
            ))
            .child(self.card_separator())
            .child(self.ide_setting_row(
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
            ))
            .into_any_element()
    }

    fn ide_agent_card(&self, settings: &PersistedSettings, cx: &mut Context<Self>) -> AnyElement {
        self.ide_card()
            .flex()
            .flex_col()
            .gap(px(IDE_SETTINGS_CARD_GAP))
            .child(self.ide_card_title(self.i18n.t("settings_view.ide.agent_title")))
            .child(self.ide_card_description(
                self.i18n.t("settings_view.ide.agent_description"),
            ))
            .child(
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
                    ]),
            )
            .child(
                div()
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(rgba(
                        (self.tokens.ui.border << 8) | IDE_SETTINGS_AGENT_SUBTLE_BORDER_ALPHA as u32,
                    ))
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
                            .child("~1 MB"),
                    ),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .italic()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ide.agent_auto_hint")),
            )
            .child(self.card_separator())
            .child(self.ide_setting_row(
                "settings_view.ide.agent_mode_label",
                "settings_view.ide.agent_mode_hint",
                self.ide_select_control(
                    SettingsSelect::IdeAgentMode,
                    ide_agent_label(settings.ide.agent_mode, &self.i18n),
                    IDE_SETTINGS_AGENT_SELECT_WIDTH,
                    cx,
                ),
            ))
            .into_any_element()
    }

    fn ide_agent_privacy_card(&self) -> AnyElement {
        self.ide_card()
            .border_color(rgba(
                (IDE_SETTINGS_BLUE_500 << 8) | IDE_SETTINGS_AGENT_PRIVACY_BORDER_ALPHA as u32,
            ))
            .bg(rgba(
                (IDE_SETTINGS_BLUE_500 << 8) | IDE_SETTINGS_AGENT_PRIVACY_BG_ALPHA as u32,
            ))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Shield,
                        16.0,
                        rgb(IDE_SETTINGS_BLUE_400),
                    ))
                    .child(self.i18n.t("settings_view.ide.agent_transparency_title")),
            )
            .child(
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
                    ]),
            )
            .into_any_element()
    }

    fn ide_card_title(&self, title: String) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(title.to_uppercase())
            .into_any_element()
    }

    fn ide_card_description(&self, description: String) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(description)
            .into_any_element()
    }

    fn ide_label_block(&self, label_key: &str, hint_key: &str) -> AnyElement {
        div()
            // Tauri flex rows leave this copy column to take the remaining
            // width. Without flex_1 GPUI can shrink CJK labels to one glyph.
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
                    .child(self.i18n.t(label_key)),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(hint_key)),
            )
            .into_any_element()
    }

    fn ide_setting_row(&self, label_key: &str, hint_key: &str, control: AnyElement) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(self.ide_label_block(label_key, hint_key))
            .child(div().flex_none().child(control))
            .into_any_element()
    }

    fn ide_number_input_with_suffix(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        suffix: Option<&'static str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut control = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(self.settings_text_input_control(
                input,
                value,
                placeholder,
                IDE_SETTINGS_INPUT_WIDTH,
                cx,
            ));
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

    fn ide_select_control(
        &self,
        select_id: SettingsSelect,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .relative()
            .w(px(width))
            .flex_none()
            .child(self.settings_select_control(select_id, value, false, Some(width), cx))
            .into_any_element()
    }

    fn ide_feature_row(&self, label_key: &str) -> AnyElement {
        self.ide_dot_row(
            IDE_SETTINGS_EMERALD_400,
            div().child(self.i18n.t(label_key)).into_any_element(),
        )
    }

    fn ide_privacy_row(&self, label_key: &str, detail_key: &str) -> AnyElement {
        self.ide_dot_row(
            IDE_SETTINGS_BLUE_400,
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
                .child(
                    div()
                        .w_full()
                        .min_w(px(0.0))
                        .child(self.i18n.t(detail_key)),
                )
                .into_any_element(),
        )
    }

    fn ide_dot_row(&self, color: u32, content: AnyElement) -> AnyElement {
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
