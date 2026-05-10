
impl WorkspaceApp {
    fn settings_local(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        let mut shell_rows = vec![self.local_shell_select_row(settings, cx)];
        if let Some(path_hint) = self.local_shell_path_hint(settings) {
            shell_rows.push(path_hint);
        }
        shell_rows.push(self.card_separator());
        shell_rows.push(
            self.setting_row(
                "settings_view.local_terminal.git_bash_path",
                "settings_view.local_terminal.git_bash_path_hint",
                self.settings_text_input_control(
                    SettingsInput::LocalGitBashPath,
                    settings
                        .local_terminal
                        .git_bash_path
                        .clone()
                        .unwrap_or_default(),
                    self.i18n
                        .t("settings_view.local_terminal.git_bash_path_placeholder"),
                    300.0,
                    cx,
                ),
            ),
        );
        shell_rows.push(self.card_separator());
        shell_rows.push(
            self.setting_row(
                "settings_view.local_terminal.default_cwd",
                "settings_view.local_terminal.default_cwd_hint",
                self.settings_text_input_control(
                    SettingsInput::LocalDefaultCwd,
                    settings
                        .local_terminal
                        .default_cwd
                        .clone()
                        .unwrap_or_default(),
                    "~".to_string(),
                    self.tokens.metrics.settings_select_width,
                    cx,
                ),
            ),
        );

        let mut oh_my_posh_rows = vec![self.checkbox_row(
            "settings_view.local_terminal.oh_my_posh_enable",
            "settings_view.local_terminal.oh_my_posh_enable_hint",
            settings.local_terminal.oh_my_posh_enabled,
            set_oh_my_posh,
            cx,
        )];
        if settings.local_terminal.oh_my_posh_enabled {
            oh_my_posh_rows.push(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(rgba((self.tokens.ui.info << 8) | 0x33))
                    .bg(rgba((self.tokens.ui.info << 8) | 0x1a))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.info))
                            .child(format!(
                                "💡 {}",
                                self.i18n.t("settings_view.local_terminal.oh_my_posh_note")
                            )),
                    )
                    .into_any_element(),
            );
            oh_my_posh_rows.push(self.card_separator());
            oh_my_posh_rows.push(
                self.setting_row(
                    "settings_view.local_terminal.oh_my_posh_theme",
                    "settings_view.local_terminal.oh_my_posh_theme_hint",
                    self.settings_text_input_control(
                        SettingsInput::LocalOhMyPoshTheme,
                        settings
                            .local_terminal
                            .oh_my_posh_theme
                            .clone()
                            .unwrap_or_default(),
                        self.i18n
                            .t("settings_view.local_terminal.oh_my_posh_theme_placeholder"),
                        300.0,
                        cx,
                    ),
                ),
            );
        }

        let shortcut_default = if cfg!(target_os = "macos") {
            "⌘T"
        } else {
            "Ctrl+T"
        };
        let shortcut_launcher = if cfg!(target_os = "macos") {
            "⌘⇧T"
        } else {
            "Ctrl+Shift+T"
        };

        let effective_shells = self.effective_local_shells_for_settings(settings);
        let shell_list = if effective_shells.is_empty() {
            vec![
                div()
                    .text_align(gpui::TextAlign::Center)
                    .py(px(32.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.local_terminal.loading_shells"))
                    .into_any_element(),
            ]
        } else {
            effective_shells
                .iter()
                .map(|shell| {
                    self.available_shell_row(
                        shell,
                        settings.local_terminal.default_shell_id.as_deref(),
                    )
                })
                .collect()
        };

        vec![
            self.settings_card(
                "settings_view.local_terminal.shell",
                "settings_view.local_terminal.default_shell_hint",
                shell_rows,
            ),
            self.settings_card(
                "settings_view.local_terminal.shell_profile",
                "settings_view.local_terminal.load_shell_profile_hint",
                vec![self.checkbox_row(
                    "settings_view.local_terminal.load_shell_profile",
                    "settings_view.local_terminal.load_shell_profile_hint",
                    settings.local_terminal.load_shell_profile,
                    set_load_shell_profile,
                    cx,
                )],
            ),
            self.settings_card(
                "settings_view.local_terminal.oh_my_posh",
                "settings_view.local_terminal.oh_my_posh_note",
                oh_my_posh_rows,
            ),
            self.settings_card(
                "settings_view.local_terminal.shortcuts",
                "settings_view.local_terminal.custom_env_hint",
                vec![
                    self.local_shortcut_row(
                        "settings_view.local_terminal.new_default_shell",
                        shortcut_default,
                    ),
                    self.card_separator(),
                    self.local_shortcut_row(
                        "settings_view.local_terminal.new_shell_launcher",
                        shortcut_launcher,
                    ),
                ],
            ),
            self.settings_card(
                "settings_view.local_terminal.available_shells",
                "settings_view.local_terminal.select_shell",
                shell_list,
            ),
        ]
    }

    fn settings_reconnect(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![
            self.bool_row(
                "settings_view.reconnect.enabled",
                "settings_view.reconnect.enabled_hint",
                settings.reconnect.enabled,
                set_reconnect_enabled,
                cx,
            ),
            separator(&self.tokens, SeparatorOrientation::Horizontal).into_any_element(),
            div()
                .flex()
                .flex_col()
                .gap(px(24.0))
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(self.tokens.ui.text_heading))
                        .child(self.i18n.t("settings_view.reconnect.strategy")),
                )
                .child(
                    div()
                        .w_full()
                        .max_w(px(672.0))
                        .grid()
                        .grid_cols(2)
                        .gap(px(32.0))
                        .child(self.number_row(
                            "settings_view.reconnect.max_attempts",
                            "settings_view.reconnect.max_attempts_hint",
                            settings.reconnect.max_attempts,
                            1,
                            1,
                            20,
                            set_reconnect_max_attempts,
                            cx,
                        ))
                        .child(self.number_row(
                            "settings_view.reconnect.base_delay",
                            "settings_view.reconnect.base_delay_hint",
                            settings.reconnect.base_delay_ms,
                            500,
                            500,
                            10000,
                            set_reconnect_base_delay,
                            cx,
                        )),
                )
                .child(
                    div()
                        .w_full()
                        .max_w(px(672.0))
                        .grid()
                        .grid_cols(2)
                        .gap(px(32.0))
                        .child(self.number_row(
                            "settings_view.reconnect.max_delay",
                            "settings_view.reconnect.max_delay_hint",
                            settings.reconnect.max_delay_ms,
                            5000,
                            5000,
                            60000,
                            set_reconnect_max_delay,
                            cx,
                        )),
                )
                .child(
                    div()
                        .max_w(px(672.0))
                        .p(px(16.0))
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
                        .bg(rgb(self.tokens.ui.bg_card))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.reconnect.formula_hint")),
                )
                .into_any_element(),
        ]
    }

    fn settings_ai(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![self.settings_card(
            "settings_view.ai.title",
            "settings_view.ai.description",
            vec![
                self.bool_row(
                    "settings_view.ai.enable",
                    "settings_view.ai.enable_hint",
                    settings.ai.enabled,
                    set_ai_enabled,
                    cx,
                ),
                self.bool_row(
                    "settings_view.ai.privacy_notice",
                    "settings_view.ai.privacy_text",
                    settings.ai.enabled_confirmed,
                    set_ai_enabled_confirmed,
                    cx,
                ),
                self.value_row(
                    "settings_view.ai.base_url",
                    "settings_view.ai.provider_settings_summary",
                    settings.ai.base_url.clone(),
                ),
                self.value_row(
                    "settings_view.ai.model",
                    "settings_view.ai.provider_settings_summary",
                    settings.ai.model.clone(),
                ),
                self.count_row(
                    "settings_view.ai.provider_settings",
                    "settings_view.ai.provider_settings_summary",
                    settings.ai.providers.len(),
                ),
                self.value_row(
                    "settings_view.ai.default_model",
                    "settings_view.ai.provider_settings_summary",
                    settings
                        .ai
                        .active_model
                        .clone()
                        .unwrap_or_else(|| settings.ai.model.clone()),
                ),
                self.number_row(
                    "settings_view.ai.max_context",
                    "settings_view.ai.max_context_hint",
                    settings.ai.context_max_chars,
                    2000,
                    2000,
                    32000,
                    set_ai_context_max_chars,
                    cx,
                ),
                self.number_row(
                    "settings_view.ai.buffer_history",
                    "settings_view.ai.buffer_history_hint",
                    settings.ai.context_visible_lines,
                    20,
                    20,
                    1000,
                        set_ai_context_lines,
                        cx,
                    ),
                self.bool_row(
                    "settings_view.ai.context_source_ide",
                    "settings_view.ai.context_source_ide_hint",
                    settings.ai.context_sources.ide,
                    set_ai_context_source_ide,
                    cx,
                ),
                self.bool_row(
                    "settings_view.ai.context_source_sftp",
                    "settings_view.ai.context_source_sftp_hint",
                    settings.ai.context_sources.sftp,
                    set_ai_context_source_sftp,
                    cx,
                ),
                self.cycle_row(
                    "settings_view.ai.reasoning_title",
                    "settings_view.ai.reasoning_hint",
                    ai_thinking_label(settings.ai.thinking_style),
                    cycle_ai_thinking,
                    cx,
                ),
                self.cycle_row(
                    "settings_view.ai.reasoning_title",
                    "settings_view.ai.reasoning_hint",
                    ai_reasoning_label(settings.ai.reasoning_effort),
                        cycle_ai_reasoning,
                        cx,
                    ),
                self.bool_row(
                    "settings_view.ai.memory_enabled",
                    "settings_view.ai.memory_enabled_hint",
                    settings.ai.memory.enabled,
                    set_ai_memory_enabled,
                    cx,
                ),
                self.value_row(
                    "settings_view.ai.custom_system_prompt",
                    "settings_view.ai.system_prompt_hint",
                    if settings.ai.custom_system_prompt.trim().is_empty() {
                        self.i18n.t("settings_view.ai.system_prompt_placeholder")
                    } else {
                        settings.ai.custom_system_prompt.clone()
                    },
                ),
                self.value_row(
                    "settings_view.ai.memory_title",
                    "settings_view.ai.memory_hint",
                    if settings.ai.memory.content.trim().is_empty() {
                        self.i18n.t("settings_view.ai.memory_placeholder")
                    } else {
                        settings.ai.memory.content.clone()
                    },
                ),
                self.bool_row(
                    "settings_view.ai.tool_use_enabled",
                    "settings_view.ai.tool_use_enabled_hint",
                    settings.ai.tool_use.enabled,
                    set_ai_tool_use_enabled,
                    cx,
                ),
                self.number_row(
                    "settings_view.ai.tool_use_max_rounds",
                    "settings_view.ai.tool_use_max_rounds_hint",
                    settings.ai.tool_use.max_rounds.unwrap_or(10),
                    1,
                    1,
                    30,
                    set_ai_tool_use_max_rounds,
                    cx,
                ),
                self.count_row(
                    "settings_view.ai.tool_use_policy_summary",
                    "settings_view.ai.tool_use_approve_hint",
                    settings.ai.tool_use.auto_approve_tools.len(),
                ),
                self.count_row(
                    "settings_view.mcp.title",
                    "settings_view.mcp.description",
                    settings.ai.mcp_servers.len(),
                ),
                self.value_row(
                    "settings_view.ai.embedding_title",
                    "settings_view.ai.embedding_description",
                    if settings.ai.embedding_config.is_some() {
                        self.i18n.t("settings_view.knowledge.semantic_search_using")
                    } else {
                        self.i18n
                            .t("settings_view.knowledge.semantic_search_not_configured")
                    },
                ),
                self.count_row(
                    "settings_view.ai.execution_profiles",
                    "settings_view.ai.execution_profiles_hint",
                    settings
                        .ai
                        .execution_profiles
                        .get("profiles")
                        .and_then(|profiles| profiles.as_array())
                        .map(Vec::len)
                        .unwrap_or(0),
                ),
            ],
        )]
    }

    fn settings_knowledge(&self) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![self.settings_card(
            "settings_view.knowledge.title",
            "settings_view.knowledge.description",
            vec![
                self.value_row(
                    "settings_view.knowledge.semantic_search",
                    "settings_view.knowledge.semantic_search_description",
                    if settings.ai.embedding_config.is_some() {
                        self.i18n.t("settings_view.knowledge.semantic_search_using")
                    } else {
                        self.i18n
                            .t("settings_view.knowledge.semantic_search_not_configured")
                    },
                ),
                self.value_row(
                    "settings_view.knowledge.keyword_search_ready",
                    "settings_view.knowledge.description",
                    self.i18n.t("common.enabled"),
                ),
                self.value_row(
                    "settings_view.knowledge.collections",
                    "settings_view.knowledge.create_description",
                    self.i18n.t("settings_view.knowledge.no_collections"),
                ),
                self.value_row(
                    "settings_view.knowledge.import_files",
                    "settings_view.knowledge.file_filter_documents",
                    self.i18n.t("common.disabled"),
                ),
                self.value_row(
                    "settings_view.knowledge.generate_embeddings",
                    "settings_view.knowledge.semantic_search_description",
                    self.i18n.t("common.disabled"),
                ),
                self.value_row(
                    "settings_view.knowledge.configure_embeddings",
                    "settings_view.ai.embedding_description",
                    if settings.ai.embedding_config.is_some() {
                        self.i18n.t("settings_view.knowledge.semantic_search_using")
                    } else {
                        self.i18n
                            .t("settings_view.knowledge.semantic_search_not_configured")
                    },
                ),
            ],
        )]
    }

    fn settings_keybindings(&self) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![self.settings_card(
            "settings_view.keybindings.title",
            "settings_view.keybindings.description",
            vec![
                self.value_row(
                    "settings_view.keybindings.modified",
                    "settings_view.keybindings.intl_keyboard_note",
                    settings.keybindings.overrides.len().to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.import",
                    "settings_view.keybindings.import_invalid",
                    self.i18n.t("settings_view.keybindings.default_value"),
                ),
                self.value_row(
                    "settings_view.keybindings.export",
                    "settings_view.keybindings.export_error",
                    self.i18n.t("common.disabled"),
                ),
                self.value_row(
                    "settings_view.keybindings.reset_all",
                    "settings_view.keybindings.reset_all_confirm",
                    self.i18n.t("settings_view.keybindings.default_value"),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.app.newTerminal",
                    "settings_view.keybindings.scope_global",
                    "Cmd+T".to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.app.closeTab",
                    "settings_view.keybindings.scope_global",
                    "Cmd+W".to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.app.settings",
                    "settings_view.keybindings.scope_global",
                    "Cmd+,".to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.split.horizontal",
                    "settings_view.keybindings.scope_split",
                    "Cmd+Shift+E".to_string(),
                ),
                self.value_row(
                    "settings_view.keybindings.actions.split.vertical",
                    "settings_view.keybindings.scope_split",
                    "Cmd+Shift+D".to_string(),
                ),
            ],
        )]
    }

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
