impl WorkspaceApp {
    fn settings_help(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![
            self.settings_card(
                "settings_view.help.version_info",
                "settings_view.help.description",
                vec![
                    self.value_row(
                        "settings_view.help.app_name",
                        "settings_view.help.version_info",
                        "OxideTerm Native".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.version",
                        "settings_view.help.version_info",
                        env!("CARGO_PKG_VERSION").to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.license",
                        "settings_view.help.resources",
                        "GPL-3.0-only".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.portable_mode",
                        "settings_view.help.portable_mode_hint",
                        self.i18n.t("settings_view.help.updates_manual_only"),
                    ),
                    self.cycle_row(
                        "settings_view.help.update_channel",
                        "settings_view.help.update_channel_hint",
                        update_channel_label(settings.general.update_channel, &self.i18n),
                        cycle_update_channel,
                        cx,
                    ),
                ],
            ),
            self.settings_card(
                "settings_view.help.shortcuts",
                "settings_view.help.resources",
                vec![
                    self.value_row(
                        "settings_view.help.shortcut_new_tab",
                        "settings_view.help.category_app",
                        "Cmd+T".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_close_tab",
                        "settings_view.help.category_app",
                        "Cmd+W".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_find",
                        "settings_view.help.category_terminal",
                        "Cmd+F".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_split_h",
                        "settings_view.help.category_split",
                        "Cmd+Shift+E".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_split_v",
                        "settings_view.help.category_split",
                        "Cmd+Shift+D".to_string(),
                    ),
                    self.value_row(
                        "settings_view.help.shortcut_settings",
                        "settings_view.help.category_app",
                        "Cmd+,".to_string(),
                    ),
                ],
            ),
            self.settings_card(
                "settings_view.help.diagnostics",
                "settings_view.help.open_logs_hint",
                vec![
                    self.value_row(
                        "settings_view.help.open_logs",
                        "settings_view.help.open_logs_hint",
                        self.i18n.t("common.disabled"),
                    ),
                    self.value_row(
                        "settings_view.help.memory_diagnostics_title",
                        "settings_view.help.memory_diagnostics_hint",
                        self.i18n.t("common.disabled"),
                    ),
                    self.value_row(
                        "settings_view.help.check_update",
                        "settings_view.help.updates_manual_only_hint",
                        self.i18n.t("settings_view.help.updates_manual_only"),
                    ),
                ],
            ),
        ]
    }

    fn cycle_row(
        &self,
        label_key: &str,
        hint_key: &str,
        value: String,
        cycle: fn(&mut PersistedSettings),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = button(&self.tokens, value, oxideterm_gpui_ui::ButtonTone::Secondary)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(cycle, cx);
                }),
            )
            .into_any_element();
        self.setting_row(label_key, hint_key, control)
    }

    fn language_label(&self, language: Language) -> String {
        match language {
            Language::De => "Deutsch",
            Language::En => "English",
            Language::EsEs => "Español (España)",
            Language::FrFr => "Français (France)",
            Language::It => "Italiano",
            Language::Ko => "한국어",
            Language::PtBr => "Português (Brasil)",
            Language::Vi => "Tiếng Việt",
            Language::Ja => "日本語",
            Language::ZhCn => "简体中文",
            Language::ZhTw => "繁體中文",
        }
        .to_string()
    }
}
