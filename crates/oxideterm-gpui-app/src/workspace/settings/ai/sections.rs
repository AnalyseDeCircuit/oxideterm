const AI_TEXTAREA_SYSTEM_PROMPT_MIN_H: f32 = 80.0; // Tauri rows=4 min-h-[80px].
const AI_TEXTAREA_MEMORY_MIN_H: f32 = 120.0; // Tauri rows=5 min-h-[120px].
const AI_TOOL_POLICY_CARD_BG_ALPHA: u32 = 0x4d; // Tauri bg-theme-bg-panel/30.
const AI_TOOL_POLICY_ROW_BG_ALPHA: u32 = 0x40; // Tauri bg-theme-bg/25.
const AI_TOOL_POLICY_BORDER_ALPHA: u32 = 0x99; // Tauri border-theme-border/60.

struct AiToolPolicyItem {
    key: Option<&'static str>,
    label_key: &'static str,
    checked: bool,
    locked: bool,
}

struct AiToolPolicyGroup {
    title_key: &'static str,
    description_key: &'static str,
    items: Vec<AiToolPolicyItem>,
}

impl WorkspaceApp {
    fn ai_execution_profiles_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let profiles = settings
            .ai
            .execution_profiles
            .get("profiles")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let default_profile_id = ai_default_execution_profile(settings).unwrap_or_else(|| "default".to_string());

        let mut profile_list = div().flex().flex_col().gap(px(8.0));
        for (index, profile) in profiles.iter().enumerate() {
            profile_list = profile_list.child(self.ai_execution_profile_card(
                index,
                profile,
                &default_profile_id,
                profiles.len(),
                settings,
                cx,
            ));
        }

        div()
            .mb(px(24.0))
            .max_w(px(AI_PROVIDER_MAX_W))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.border << 8) | AI_PROVIDER_SECTION_BORDER_ALPHA,
            ))
            .bg(rgba((self.tokens.ui.bg << 8) | AI_PROVIDER_SECTION_BG_ALPHA))
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(self.ai_section_heading(
                        "settings_view.ai.execution_profiles",
                        "settings_view.ai.execution_profiles_hint",
                    ))
                    .child(
                        button_with(
                            &self.tokens,
                            format!("+ {}", self.i18n.t("settings_view.ai.profile_add")),
                            ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.edit_settings(ai_add_execution_profile, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element(),
                    ),
            )
            .child(profile_list)
            .into_any_element()
    }

    fn ai_execution_profile_card(
        &self,
        index: usize,
        profile: &serde_json::Value,
        default_profile_id: &str,
        profile_count: usize,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let profile_id = ai_execution_profile_id(profile).unwrap_or_else(|| format!("profile-{index}"));
        let is_default = profile_id == default_profile_id;
        let provider_id = profile
            .get("providerId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let provider_label = provider_id
            .as_deref()
            .and_then(|id| {
                settings
                    .ai
                    .providers
                    .iter()
                    .find(|provider| ai_provider_id(provider).as_deref() == Some(id))
                    .and_then(|provider| ai_provider_string(provider, "name"))
            })
            .unwrap_or_else(|| self.i18n.t("settings_view.ai.profile_inherit_provider"));
        let reasoning = profile
            .get("reasoningEffort")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("auto");

        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x73))
            .bg(rgba((self.tokens.ui.bg_card << 8) | 0x73))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.settings_text_input_control(
                        SettingsInput::AiProfileName(index),
                        self.current_settings_input_value(SettingsInput::AiProfileName(index)),
                        self.i18n.t("settings_view.ai.profile_add"),
                        180.0,
                        cx,
                    ))
                    .child(self.ai_profile_default_button(index, profile_id.clone(), is_default, cx))
                    .child(self.ai_icon_button(
                        LucideIcon::Copy,
                        false,
                        move |this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| ai_duplicate_execution_profile(settings, index),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.ai_icon_button(
                        LucideIcon::Trash2,
                        profile_count <= 1,
                        move |this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| ai_delete_execution_profile(settings, index),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    )),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .gap(px(8.0))
                    .child(self.ai_settings_select_control(
                        SettingsSelect::AiProfileProvider(index),
                        provider_label,
                        180.0,
                        cx,
                    ))
                    .child(self.settings_text_input_control(
                        SettingsInput::AiProfileModel(index),
                        self.current_settings_input_value(SettingsInput::AiProfileModel(index)),
                        self.i18n.t("settings_view.ai.profile_inherit_model"),
                        180.0,
                        cx,
                    ))
                    .child(self.ai_settings_select_control(
                        SettingsSelect::AiProfileReasoning(index),
                        self.ai_reasoning_display(reasoning),
                        160.0,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn ai_provider_settings_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let providers = ai_provider_views(self.settings_store.settings());
        let mut provider_list = div()
            .w_full()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .gap(px(12.0));
        for (index, provider) in providers.into_iter().enumerate() {
            provider_list = provider_list.child(self.ai_provider_card(index, provider, cx));
        }

        let expanded = self.ai_provider_settings_expanded;
        let summary = self.i18n_count(
            "settings_view.ai.provider_settings_summary",
            self.settings_store.settings().ai.providers.len(),
        );

        div()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .child(self.ai_collapsible_header(
                "settings_view.ai.provider_settings",
                summary,
                expanded,
                |this, _event, _window, cx| {
                    this.ai_provider_settings_expanded = !this.ai_provider_settings_expanded;
                    cx.stop_propagation();
                    cx.notify();
                },
                cx,
            ))
            .when(expanded, |section| {
                section.child(
                    div()
                        .mb(px(24.0))
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(provider_list)
                        .child(self.ai_provider_add_controls(cx)),
                )
            })
            .into_any_element()
    }

    fn ai_context_controls_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.ai_section_title("settings_view.ai.context_controls"))
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(24.0))
                    .child(self.ai_context_select_field(
                        "settings_view.ai.max_context",
                        "settings_view.ai.max_context_hint",
                        SettingsSelect::AiContextMaxChars,
                        self.ai_context_max_chars_label(settings.ai.context_max_chars),
                        cx,
                    ))
                    .child(self.ai_context_select_field(
                        "settings_view.ai.buffer_history",
                        "settings_view.ai.buffer_history_hint",
                        SettingsSelect::AiContextVisibleLines,
                        self.ai_context_visible_lines_label(settings.ai.context_visible_lines),
                        cx,
                    )),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.ai.context_sources").to_uppercase()),
                    )
                    .child(self.ai_context_source_row(
                        "settings_view.ai.context_source_ide",
                        "settings_view.ai.context_source_ide_hint",
                        settings.ai.context_sources.ide,
                        set_ai_context_source_ide,
                        cx,
                    ))
                    .child(self.ai_context_source_row(
                        "settings_view.ai.context_source_sftp",
                        "settings_view.ai.context_source_sftp_hint",
                        settings.ai.context_sources.sftp,
                        set_ai_context_source_sftp,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn ai_context_select_field(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.ai_context_select_control(select_id, label, cx))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(hint_key)),
            )
            .into_any_element()
    }

    fn ai_context_select_control(
        &self,
        select_id: SettingsSelect,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = select_id.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, label, false, false)
            .w_full()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select = if this.open_settings_select == Some(select_id) {
                        None
                    } else {
                        Some(select_id)
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        div()
            .relative()
            .w_full()
            .child(select_anchor_probe(anchor_id, trigger, move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            }))
            .into_any_element()
    }

    fn ai_context_source_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(AI_CONTEXT_SOURCE_ROW_GAP))
            .cursor_pointer()
            .child(checkbox(&self.tokens, String::new(), checked))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(label_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(|settings| setter(settings, !checked), cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn ai_context_max_chars_label(&self, value: i64) -> String {
        match value {
            2_000 => self.i18n.t("settings_view.ai.chars_2000"),
            4_000 => self.i18n.t("settings_view.ai.chars_4000"),
            8_000 => self.i18n.t("settings_view.ai.chars_8000"),
            16_000 => self.i18n.t("settings_view.ai.chars_16000"),
            32_000 => self.i18n.t("settings_view.ai.chars_32000"),
            other => other.to_string(),
        }
    }

    fn ai_context_visible_lines_label(&self, value: i64) -> String {
        match value {
            50 => self.i18n.t("settings_view.ai.lines_50"),
            100 => self.i18n.t("settings_view.ai.lines_100"),
            200 => self.i18n.t("settings_view.ai.lines_200"),
            400 => self.i18n.t("settings_view.ai.lines_400"),
            other => other.to_string(),
        }
    }

    fn ai_system_prompt_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.ai_section_title("settings_view.ai.system_prompt_title"))
            .child(self.ai_textarea_row(
                SettingsInput::AiSystemPrompt,
                self.i18n.t("settings_view.ai.custom_system_prompt"),
                self.i18n.t("settings_view.ai.system_prompt_hint"),
                self.i18n.t("settings_view.ai.system_prompt_placeholder"),
                settings.ai.custom_system_prompt.clone(),
                AI_TEXTAREA_SYSTEM_PROMPT_MIN_H,
                cx,
            ))
            .child(self.ai_separator())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Brain,
                        16.0,
                        rgb(self.tokens.ui.text),
                    ))
                    .child(self.ai_section_title("settings_view.ai.memory_title")),
            )
            .child(self.bool_row(
                "settings_view.ai.memory_enabled",
                "settings_view.ai.memory_enabled_hint",
                settings.ai.memory.enabled,
                set_ai_memory_enabled,
                cx,
            ))
            .child(self.ai_textarea_row(
                SettingsInput::AiMemoryContent,
                String::new(),
                self.i18n.t("settings_view.ai.memory_hint"),
                self.i18n.t("settings_view.ai.memory_placeholder"),
                settings.ai.memory.content.clone(),
                AI_TEXTAREA_MEMORY_MIN_H,
                cx,
            ))
            .child(
                button_with(
                    &self.tokens,
                    self.i18n.t("settings_view.ai.memory_clear"),
                    ButtonOptions {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: settings.ai.memory.content.trim().is_empty(),
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.edit_settings(|settings| settings.ai.memory.content.clear(), cx);
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
            )
            .child(self.ai_separator())
            .child(self.ai_global_reasoning_section(settings, cx))
            .child(self.ai_model_reasoning_overrides_section(settings, cx))
            .child(self.ai_active_model_max_response_tokens_row(settings, cx))
            .child(self.ai_separator())
            .child(self.ai_model_context_windows_section(settings, cx))
            .into_any_element()
    }

    fn ai_global_reasoning_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .mb(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.ai.reasoning_title").to_uppercase()),
            )
            .child(self.ai_context_select_control(
                SettingsSelect::AiGlobalReasoning,
                self.ai_reasoning_display(ai_reasoning_profile_value(settings.ai.reasoning_effort)),
                cx,
            ))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ai.reasoning_hint")),
            )
            .into_any_element()
    }

    fn ai_tool_use_section(&self, settings: &PersistedSettings, cx: &mut Context<Self>) -> AnyElement {
        let approved_count = settings
            .ai
            .tool_use
            .auto_approve_tools
            .values()
            .filter(|value| value.as_bool() == Some(true))
            .count();
        let total_count = settings.ai.tool_use.auto_approve_tools.len();
        let mut policy_groups = div().grid().grid_cols(2).gap(px(12.0));
        for group in self.ai_tool_policy_groups(settings) {
            policy_groups = policy_groups.child(self.ai_tool_policy_group(group, cx));
        }

        div()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Wrench,
                                16.0,
                                rgb(self.tokens.ui.text),
                            ))
                            .child(self.ai_section_title("settings_view.ai.tool_use")),
                    )
                    .child(self.ai_tool_expand_button(cx)),
            )
            .child(self.bool_row(
                "settings_view.ai.tool_use_enabled",
                "settings_view.ai.tool_use_enabled_hint",
                settings.ai.tool_use.enabled,
                set_ai_tool_use_enabled,
                cx,
            ))
            .when(!self.ai_tool_use_expanded, |section| {
                section.child(
                    div()
                        .ml(px(16.0))
                        .pl(px(16.0))
                        .border_l_1()
                        .border_color(rgba((self.tokens.ui.border << 8) | 0x4d))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(format!(
                            "{} · {}",
                            self.i18n.t("settings_view.ai.tool_use_policy_summary"),
                            self.i18n
                                .t("settings_view.ai.tool_use_collapsed_summary")
                                .replace("{{approved}}", &approved_count.to_string())
                                .replace("{{total}}", &total_count.to_string())
                        )),
                )
            })
            .when(self.ai_tool_use_expanded, |section| {
                section.child(
                    div()
                        .ml(px(16.0))
                        .pl(px(16.0))
                        .border_l_1()
                        .border_color(rgba((self.tokens.ui.border << 8) | 0x4d))
                        .flex()
                        .flex_col()
                        .gap(px(20.0))
                        .opacity(if settings.ai.tool_use.enabled { 1.0 } else { 0.4 })
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t("settings_view.ai.tool_use_approve_hint")),
                        )
                        .child(self.number_row(
                            "settings_view.ai.tool_use_max_rounds",
                            "settings_view.ai.tool_use_max_rounds_hint",
                            settings.ai.tool_use.max_rounds.unwrap_or(10),
                            1,
                            1,
                            30,
                            set_ai_tool_use_max_rounds,
                            cx,
                        ))
                        .child(policy_groups)
                        .child(self.ai_disabled_tools_notice(settings, cx))
                        .child(self.ai_policy_warning()),
                )
            })
            .child(self.ai_separator())
            .child(self.ai_mcp_summary_section(settings, cx))
            .child(self.ai_embedding_config_section(settings, cx))
            .into_any_element()
    }

    fn ai_tool_policy_groups(&self, settings: &PersistedSettings) -> Vec<AiToolPolicyGroup> {
        let auto = &settings.ai.tool_use.auto_approve_tools;
        let checked = |key: &str| auto.get(key).and_then(serde_json::Value::as_bool) == Some(true);
        vec![
            AiToolPolicyGroup {
                title_key: "settings_view.ai.tool_policy_read_title",
                description_key: "settings_view.ai.tool_policy_read_desc",
                items: vec![AiToolPolicyItem {
                    key: None,
                    label_key: "settings_view.ai.tool_policy_read_auto",
                    checked: true,
                    locked: true,
                }],
            },
            AiToolPolicyGroup {
                title_key: "settings_view.ai.tool_policy_execute_title",
                description_key: "settings_view.ai.tool_policy_execute_desc",
                items: vec![AiToolPolicyItem {
                    key: Some("run_command"),
                    label_key: "settings_view.ai.tool_policy_execute_run_command",
                    checked: checked("run_command"),
                    locked: false,
                }],
            },
            AiToolPolicyGroup {
                title_key: "settings_view.ai.tool_policy_interactive_title",
                description_key: "settings_view.ai.tool_policy_interactive_desc",
                items: vec![AiToolPolicyItem {
                    key: Some("send_terminal_input"),
                    label_key: "settings_view.ai.tool_policy_interactive_send_input",
                    checked: checked("send_terminal_input"),
                    locked: false,
                }],
            },
            AiToolPolicyGroup {
                title_key: "settings_view.ai.tool_policy_navigation_title",
                description_key: "settings_view.ai.tool_policy_navigation_desc",
                items: vec![
                    AiToolPolicyItem {
                        key: Some("connect_target"),
                        label_key: "settings_view.ai.tool_policy_connect_target",
                        checked: checked("connect_target"),
                        locked: false,
                    },
                    AiToolPolicyItem {
                        key: Some("open_app_surface"),
                        label_key: "settings_view.ai.tool_policy_open_surface",
                        checked: checked("open_app_surface"),
                        locked: false,
                    },
                ],
            },
            AiToolPolicyGroup {
                title_key: "settings_view.ai.tool_policy_write_title",
                description_key: "settings_view.ai.tool_policy_write_desc",
                items: vec![
                    AiToolPolicyItem {
                        key: Some("write_resource:settings"),
                        label_key: "settings_view.ai.tool_policy_write_settings",
                        checked: checked("write_resource:settings"),
                        locked: false,
                    },
                    AiToolPolicyItem {
                        key: Some("write_resource:file"),
                        label_key: "settings_view.ai.tool_policy_write_file",
                        checked: checked("write_resource:file"),
                        locked: false,
                    },
                    AiToolPolicyItem {
                        key: Some("transfer_resource"),
                        label_key: "settings_view.ai.tool_policy_transfer_resource",
                        checked: checked("transfer_resource"),
                        locked: false,
                    },
                    AiToolPolicyItem {
                        key: Some("remember_preference"),
                        label_key: "settings_view.ai.tool_policy_remember_preference",
                        checked: checked("remember_preference"),
                        locked: false,
                    },
                ],
            },
        ]
    }

    fn ai_section_heading(&self, title_key: &str, hint_key: &str) -> AnyElement {
        div()
            .min_w(px(0.0))
            .flex_1()
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .whitespace_nowrap()
                    .child(self.i18n.t(title_key).to_uppercase()),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(hint_key)),
            )
            .into_any_element()
    }

    fn ai_collapsible_header(
        &self,
        title_key: &str,
        summary: String,
        expanded: bool,
        on_click: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .mb(px(16.0))
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .px(px(4.0))
            .py(px(4.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .cursor_pointer()
            .hover(|style| {
                style
                    .bg(rgba((self.tokens.ui.bg_hover << 8) | 0x66))
                    .text_color(rgb(self.tokens.ui.text))
            })
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(title_key).to_uppercase()),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(summary),
                    ),
            )
            .child(Self::render_lucide_icon(
                if expanded {
                    LucideIcon::ChevronDown
                } else {
                    LucideIcon::ChevronRight
                },
                16.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .on_mouse_down(MouseButton::Left, cx.listener(on_click))
            .into_any_element()
    }

    fn ai_icon_button(
        &self,
        icon: LucideIcon,
        disabled: bool,
        on_click: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w(px(30.0))
            .h(px(30.0))
            .rounded(px(self.tokens.radii.md))
            .flex()
            .items_center()
            .justify_center()
            .opacity(if disabled { 0.35 } else { 1.0 })
            .text_color(rgb(self.tokens.ui.text_muted))
            .when(!disabled, |button| {
                button
                    .cursor_pointer()
                    .hover(|style| style.bg(rgba((self.tokens.ui.bg_hover << 8) | 0x80)))
                    .on_mouse_down(MouseButton::Left, cx.listener(on_click))
            })
            .child(Self::render_lucide_icon(
                icon,
                15.0,
                if matches!(icon, LucideIcon::Trash2) {
                    rgb(self.tokens.ui.error)
                } else {
                    rgb(self.tokens.ui.text_muted)
                },
            ))
            .into_any_element()
    }

    fn ai_profile_default_button(
        &self,
        _index: usize,
        profile_id: String,
        is_default: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        button_with(
            &self.tokens,
            if is_default {
                self.i18n.t("settings_view.ai.profile_default")
            } else {
                self.i18n.t("settings_view.ai.profile_set_default")
            },
            ButtonOptions {
                variant: if is_default {
                    ButtonVariant::Default
                } else {
                    ButtonVariant::Outline
                },
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: false,
            },
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.edit_settings(
                    |settings| ai_set_default_execution_profile(settings, profile_id.clone()),
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn ai_settings_select_control(
        &self,
        select_id: SettingsSelect,
        label: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = select_id.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, label, false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select = if this.open_settings_select == Some(select_id) {
                        None
                    } else {
                        Some(select_id)
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        div()
            .relative()
            .w(px(width))
            .child(select_anchor_probe(anchor_id, trigger, move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            }))
            .into_any_element()
    }

    fn ai_reasoning_display(&self, value: &str) -> String {
        let key = match value {
            "off" | "none" => "settings_view.ai.reasoning_off",
            "low" | "minimal" => "settings_view.ai.reasoning_low",
            "medium" => "settings_view.ai.reasoning_medium",
            "high" => "settings_view.ai.reasoning_high",
            "max" | "xhigh" => "settings_view.ai.reasoning_max",
            _ => "settings_view.ai.reasoning_auto",
        };
        self.i18n.t(key)
    }

    fn ai_textarea_row(
        &self,
        input: SettingsInput,
        label: String,
        hint: String,
        placeholder: String,
        value: String,
        min_height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let mut textarea = div()
            .w_full()
            .min_h(px(min_height))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if focused {
                rgba((theme.accent << 8) | 0x66)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg))
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(2.0))
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(20.0))
            .text_color(rgb(theme.text))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                }),
            );

        if display_value.is_empty() {
            for line in placeholder.split('\n') {
                textarea = textarea.child(
                    div()
                        .min_h(px(20.0))
                        .text_color(rgba((theme.text_muted << 8) | 0x66))
                        .child(line.to_string()),
                );
            }
        } else {
            for line in display_value.split('\n') {
                textarea = textarea.child(div().min_h(px(20.0)).child(line.to_string()));
            }
        }

        if let Some(marked) = self.marked_text_for_target(target) {
            textarea = textarea.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }
        if focused {
            textarea = textarea.child(text_caret(
                &self.tokens,
                self.new_connection_caret_visible,
            ));
        }

        let control = text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.update_text_input_anchor(anchor, cx);
            });
        });

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .when(!label.is_empty(), |row| {
                row.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .child(label),
                )
            })
            .child(control)
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .line_height(px(18.0))
                    .child(hint),
            )
            .into_any_element()
    }

    fn ai_model_reasoning_overrides_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let providers_with_models: Vec<_> = ai_provider_views(settings)
            .into_iter()
            .enumerate()
            .filter(|(_, provider)| !provider.models.is_empty())
            .collect();
        div()
            .mt(px(8.0))
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .child(self.ai_model_reasoning_header(cx))
            .when(self.ai_model_reasoning_expanded, |section| {
                if providers_with_models.is_empty() {
                    section.child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .italic()
                            .child(self.i18n.t("settings_view.ai.model_reasoning_overrides_empty")),
                    )
                } else {
                    let mut list = div().flex().flex_col().gap(px(16.0));
                    for (provider_index, provider) in providers_with_models {
                        list = list.child(self.ai_model_reasoning_provider(
                            provider_index,
                            settings,
                            provider,
                            cx,
                        ));
                    }
                    section.child(list)
                }
            })
            .into_any_element()
    }

    fn ai_model_reasoning_header(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .mb(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(4.0))
            .py(px(4.0))
            .flex()
            .items_start()
            .justify_between()
            .gap(px(12.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .cursor_pointer()
            .hover(|style| {
                style
                    .bg(rgba(
                        (self.tokens.ui.bg_hover << 8) | AI_CONTEXT_PROVIDER_HOVER_ALPHA,
                    ))
                    .text_color(rgb(self.tokens.ui.text))
            })
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(
                                self.i18n
                                    .t("settings_view.ai.model_reasoning_overrides")
                                    .to_uppercase(),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.ai.model_reasoning_overrides_hint")),
                    ),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .child(Self::render_lucide_icon(
                        if self.ai_model_reasoning_expanded {
                            LucideIcon::ChevronDown
                        } else {
                            LucideIcon::ChevronRight
                        },
                        16.0,
                        rgb(self.tokens.ui.text_muted),
                    )),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.ai_model_reasoning_expanded = !this.ai_model_reasoning_expanded;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn ai_model_reasoning_provider(
        &self,
        provider_index: usize,
        settings: &PersistedSettings,
        provider: AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = provider.id.clone();
        let expanded = self
            .expanded_ai_model_reasoning_providers
            .contains(&provider_id);
        let override_count = provider
            .models
            .iter()
            .filter(|model| {
                settings
                    .ai
                    .reasoning_model_overrides
                    .get(&provider_id)
                    .and_then(|models| models.get(model.as_str()))
                    .is_some()
            })
            .count();
        let mut section = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .mb(px(4.0))
                    .rounded(px(self.tokens.radii.sm))
                    .px(px(4.0))
                    .py(px(4.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .cursor_pointer()
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(provider.name.to_uppercase()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                self.i18n
                                    .t("settings_view.ai.model_reasoning_provider_summary")
                                    .replace("{{count}}", &provider.models.len().to_string())
                                    .replace("{{overrides}}", &override_count.to_string()),
                            )
                            .child(Self::render_lucide_icon(
                                if expanded {
                                    LucideIcon::ChevronDown
                                } else {
                                    LucideIcon::ChevronRight
                                },
                                14.0,
                                rgb(self.tokens.ui.text_muted),
                            )),
                    )
                    .hover(|style| {
                        style
                            .bg(rgba(
                                (self.tokens.ui.bg_hover << 8) | AI_CONTEXT_PROVIDER_HOVER_ALPHA,
                            ))
                            .text_color(rgb(self.tokens.ui.text))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if this
                                .expanded_ai_model_reasoning_providers
                                .contains(&provider_id)
                            {
                                this.expanded_ai_model_reasoning_providers.remove(&provider_id);
                            } else {
                                this.expanded_ai_model_reasoning_providers
                                    .insert(provider_id.clone());
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        if expanded {
            let mut rows = div()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgba(
                    (self.tokens.ui.border << 8) | AI_CONTEXT_PROVIDER_ROW_BORDER_ALPHA,
                ))
                .overflow_hidden();
            for (index, model) in provider.models.iter().enumerate() {
                rows = rows.child(self.ai_model_reasoning_row(
                    provider_index,
                    index,
                    settings,
                    &provider.id,
                    model,
                    cx,
                ));
            }
            section = section.child(rows);
        }
        section.into_any_element()
    }

    fn ai_model_reasoning_row(
        &self,
        provider_index: usize,
        model_index: usize,
        settings: &PersistedSettings,
        provider_id: &str,
        model: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = settings
            .ai
            .reasoning_model_overrides
            .get(provider_id)
            .and_then(|models| models.get(model))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("__inherit__")
            .to_string();
        let label = if current == "__inherit__" {
            self.i18n.t("settings_view.ai.reasoning_inherit_provider")
        } else {
            self.ai_reasoning_display(&current)
        };
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(6.0))
            .when(model_index > 0, |row| {
                row.border_t_1().border_color(rgba(
                    (self.tokens.ui.border << 8) | AI_CONTEXT_PROVIDER_ROW_TOP_BORDER_ALPHA,
                ))
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_family(settings_mono_font_family(settings))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .overflow_hidden()
                    .child(model.to_string()),
            )
            .child(self.ai_settings_select_control(
                SettingsSelect::AiModelReasoning(provider_index, model_index),
                label,
                160.0,
                cx,
            ))
            .into_any_element()
    }

    fn ai_model_context_windows_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let providers_with_models: Vec<_> = ai_provider_views(settings)
            .into_iter()
            .enumerate()
            .filter(|(_, provider)| !provider.models.is_empty())
            .collect();
        div()
            .opacity(if settings.ai.enabled { 1.0 } else { 0.5 })
            .flex()
            .flex_col()
            .child(self.ai_context_windows_header(cx))
            .when(self.ai_context_windows_expanded, |section| {
                if providers_with_models.is_empty() {
                    section.child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .italic()
                            .child(self.i18n.t("settings_view.ai.model_context_windows_empty")),
                    )
                } else {
                    let mut list = div()
                        .max_w(px(AI_PROVIDER_MAX_W))
                        .flex()
                        .flex_col()
                        .gap(px(16.0));
                    for (provider_index, provider) in providers_with_models {
                        list = list.child(self.ai_context_window_provider(
                            provider_index,
                            settings,
                            provider,
                            cx,
                        ));
                    }
                    section.child(list)
                }
            })
            .into_any_element()
    }

    fn ai_context_windows_header(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .mb(px(16.0))
            .w_full()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .items_start()
            .justify_between()
            .gap(px(12.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(
                                self.i18n
                                    .t("settings_view.ai.model_context_windows")
                                    .to_uppercase(),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.ai.model_context_windows_hint")),
                    ),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .child(Self::render_lucide_icon(
                        if self.ai_context_windows_expanded {
                            LucideIcon::ChevronDown
                        } else {
                            LucideIcon::ChevronRight
                        },
                        16.0,
                        rgb(self.tokens.ui.text_muted),
                    )),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.ai_context_windows_expanded = !this.ai_context_windows_expanded;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn ai_context_window_provider(
        &self,
        provider_index: usize,
        settings: &PersistedSettings,
        provider: AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = provider.id.clone();
        let expanded = self.expanded_ai_context_providers.contains(&provider_id);
        let override_count = provider
            .models
            .iter()
            .filter(|model| {
                settings
                    .ai
                    .user_context_windows
                    .get(&provider.id)
                    .and_then(|windows| windows.get(model.as_str()))
                    .is_some()
            })
            .count();
        let mut section = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .mb(px(4.0))
                    .rounded(px(self.tokens.radii.sm))
                    .px(px(4.0))
                    .py(px(4.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .cursor_pointer()
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(provider.name.to_uppercase()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                self.i18n
                                    .t("settings_view.ai.ctx_provider_summary")
                                    .replace("{{count}}", &provider.models.len().to_string())
                                    .replace("{{overrides}}", &override_count.to_string()),
                            )
                            .child(Self::render_lucide_icon(
                                if expanded {
                                    LucideIcon::ChevronDown
                                } else {
                                    LucideIcon::ChevronRight
                                },
                                14.0,
                                rgb(self.tokens.ui.text_muted),
                            )),
                    )
                    .hover(|style| {
                        style
                            .bg(rgba(
                                (self.tokens.ui.bg_hover << 8) | AI_CONTEXT_PROVIDER_HOVER_ALPHA,
                            ))
                            .text_color(rgb(self.tokens.ui.text))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if this.expanded_ai_context_providers.contains(&provider_id) {
                                this.expanded_ai_context_providers.remove(&provider_id);
                            } else {
                                this.expanded_ai_context_providers.insert(provider_id.clone());
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        if expanded {
            let mut rows = div()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgba(
                    (self.tokens.ui.border << 8) | AI_CONTEXT_PROVIDER_ROW_BORDER_ALPHA,
                ))
                .overflow_hidden();
            for (model_index, model) in provider.models.iter().enumerate() {
                rows = rows.child(self.ai_context_window_row(
                    provider_index,
                    model_index,
                    settings,
                    &provider.id,
                    model,
                    cx,
                ));
            }
            section = section.child(rows);
        }
        section.into_any_element()
    }

    fn ai_context_window_row(
        &self,
        provider_index: usize,
        model_index: usize,
        settings: &PersistedSettings,
        provider_id: &str,
        model: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_override = settings
            .ai
            .user_context_windows
            .get(provider_id)
            .and_then(|windows| windows.get(model))
            .is_some();
        let info = ai_model_context_window_info(
            model,
            &settings.ai.model_context_windows,
            Some(provider_id),
            &settings.ai.user_context_windows,
        );
        let input = SettingsInput::AiModelContextWindow(provider_index, model_index);
        let reset_provider_id = provider_id.to_string();
        let reset_model = model.to_string();
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(6.0))
            .bg(if has_override {
                rgba((self.tokens.ui.accent << 8) | AI_CONTEXT_USER_OVERRIDE_BG_ALPHA)
            } else {
                rgba((self.tokens.ui.bg << 8) | 0x00)
            })
            .when(model_index > 0, |row| {
                row.border_t_1().border_color(rgba(
                    (self.tokens.ui.border << 8) | AI_CONTEXT_PROVIDER_ROW_TOP_BORDER_ALPHA,
                ))
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_family(settings_mono_font_family(settings))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .overflow_hidden()
                    .child(model.to_string()),
            )
            .child(self.ai_context_source_badge(info.source))
            .child(self.settings_text_input_control(
                input,
                self.current_settings_input_value(input),
                "Auto".to_string(),
                AI_CONTEXT_NUMBER_W,
                cx,
            ))
            .child(
                div()
                    .w(px(AI_CONTEXT_RESET_SLOT_W))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(has_override, |slot| {
                        slot.child(
                            div()
                                .cursor_pointer()
                                .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x99))
                                .hover(|style| style.text_color(rgb(self.tokens.ui.text)))
                                .child(Self::render_lucide_icon(
                                    LucideIcon::X,
                                    12.0,
                                    rgb(self.tokens.ui.text_muted),
                                ))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        let provider_id = reset_provider_id.clone();
                                        let model = reset_model.clone();
                                        this.edit_settings(
                                            move |settings| {
                                                set_ai_user_context_window(
                                                    settings,
                                                    &provider_id,
                                                    &model,
                                                    None,
                                                );
                                            },
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }),
                                ),
                        )
                    }),
            )
            .into_any_element()
    }

    fn ai_context_source_badge(&self, source: ContextWindowSource) -> AnyElement {
        let (text_color, bg_color) = self.ai_context_source_badge_colors(source);
        div()
            .rounded(px(self.tokens.radii.sm))
            .px(px(AI_CONTEXT_SOURCE_BADGE_PX))
            .py(px(AI_CONTEXT_SOURCE_BADGE_PY))
            .text_size(px(AI_CONTEXT_SOURCE_BADGE_TEXT_SIZE))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(text_color)
            .bg(bg_color)
            .child(self.i18n.t(source.i18n_key()))
            .into_any_element()
    }

    fn ai_context_source_badge_colors(&self, source: ContextWindowSource) -> (Rgba, Rgba) {
        match source {
            ContextWindowSource::User => (
                rgb(AI_CONTEXT_SOURCE_USER_COLOR),
                rgba((AI_CONTEXT_SOURCE_USER_COLOR << 8) | AI_CONTEXT_SOURCE_BADGE_BG_ALPHA),
            ),
            ContextWindowSource::Api => (
                rgb(AI_CONTEXT_SOURCE_API_COLOR),
                rgba((AI_CONTEXT_SOURCE_API_COLOR << 8) | AI_CONTEXT_SOURCE_BADGE_BG_ALPHA),
            ),
            ContextWindowSource::Name => (
                rgb(AI_CONTEXT_SOURCE_NAME_COLOR),
                rgba((AI_CONTEXT_SOURCE_NAME_COLOR << 8) | AI_CONTEXT_SOURCE_BADGE_BG_ALPHA),
            ),
            ContextWindowSource::Pattern | ContextWindowSource::Default => (
                rgba((self.tokens.ui.text_muted << 8) | AI_CONTEXT_SOURCE_DEFAULT_TEXT_ALPHA),
                rgba((self.tokens.ui.border << 8) | AI_CONTEXT_SOURCE_DEFAULT_BG_ALPHA),
            ),
        }
    }

    fn ai_active_model_max_response_tokens_row(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(model) = settings.ai.active_model.clone() else {
            return div().into_any_element();
        };
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.ai_section_title("settings_view.ai.max_response_tokens"))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ai.max_response_tokens_hint")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .font_family(settings_mono_font_family(settings))
                            .child(format!("{model}:")),
                    )
                    .child(self.settings_text_input_control(
                        SettingsInput::AiActiveModelMaxResponseTokens,
                        self.current_settings_input_value(SettingsInput::AiActiveModelMaxResponseTokens),
                        "Auto".to_string(),
                        128.0,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn ai_tool_expand_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.ai_tool_use_expanded;
        button_with(
            &self.tokens,
            if expanded {
                self.i18n.t("settings_view.ai.tool_use_collapse")
            } else {
                self.i18n.t("settings_view.ai.tool_use_expand")
            },
            ButtonOptions {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: false,
            },
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                this.ai_tool_use_expanded = !this.ai_tool_use_expanded;
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn ai_tool_policy_group(
        &self,
        group: AiToolPolicyGroup,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut items = div().mt(px(12.0)).flex().flex_col().gap(px(8.0));
        for item in group.items {
            let tool_key = item.key.map(str::to_string);
            let checked = item.checked;
            let locked = item.locked;
            items = items.child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x4d))
                    .bg(rgba((self.tokens.ui.bg << 8) | AI_TOOL_POLICY_ROW_BG_ALPHA))
                    .px(px(10.0))
                    .py(px(8.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(item.label_key))
                    .child(
                        checkbox(&self.tokens, String::new(), checked)
                            .opacity(if locked { 0.5 } else { 1.0 })
                            .when(!locked, |checkbox| {
                                checkbox.on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        let Some(tool_key) = tool_key.clone() else {
                                            return;
                                        };
                                        this.edit_settings(
                                            move |settings| {
                                                settings.ai.tool_use.auto_approve_tools.insert(
                                                    tool_key.clone(),
                                                    serde_json::json!(!checked),
                                                );
                                            },
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }),
                                )
                            })
                            .into_any_element(),
                    ),
            );
        }

        div()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.border << 8) | AI_TOOL_POLICY_BORDER_ALPHA,
            ))
            .bg(rgba(
                (self.tokens.ui.bg_panel << 8) | AI_TOOL_POLICY_CARD_BG_ALPHA,
            ))
            .p(px(12.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(group.title_key)),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(18.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(group.description_key)),
            )
            .child(items)
            .into_any_element()
    }

    fn ai_disabled_tools_notice(&self, settings: &PersistedSettings, cx: &mut Context<Self>) -> AnyElement {
        let count = settings.ai.tool_use.disabled_tools.len();
        if count == 0 {
            return div().into_any_element();
        }
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.warning << 8) | 0x33))
            .bg(rgba((self.tokens.ui.warning << 8) | 0x1a))
            .p(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.warning))
                    .child(
                        self.i18n
                            .t("settings_view.ai.tool_use_disabled_tools_title")
                            .replace("{{count}}", &count.to_string()),
                    ),
            )
            .child(
                button_with(
                    &self.tokens,
                    self.i18n.t("settings_view.ai.tool_use_restore_disabled_tools"),
                    ButtonOptions {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.edit_settings(|settings| settings.ai.tool_use.disabled_tools.clear(), cx);
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
            )
            .into_any_element()
    }

    fn ai_policy_warning(&self) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((self.tokens.ui.warning << 8) | 0x33))
            .bg(rgba((self.tokens.ui.warning << 8) | 0x1a))
            .p(px(12.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .line_height(px(18.0))
            .text_color(rgb(self.tokens.ui.warning))
            .child(self.i18n.t("settings_view.ai.tool_policy_warning"))
            .into_any_element()
    }

    fn ai_mcp_summary_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ai_mcp_servers_section(settings, cx)
    }

    fn ai_embedding_config_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_label = settings
            .ai
            .embedding_config
            .as_ref()
            .and_then(|config| config.get("providerId"))
            .and_then(serde_json::Value::as_str)
            .and_then(|provider_id| {
                settings
                    .ai
                    .providers
                    .iter()
                    .find(|provider| ai_provider_id(provider).as_deref() == Some(provider_id))
                    .and_then(|provider| ai_provider_string(provider, "name"))
            })
            .unwrap_or_else(|| self.i18n.t("settings_view.knowledge.auto_embedding_provider"));
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.ai_section_heading(
                "settings_view.ai.embedding_title",
                "settings_view.ai.embedding_description",
            ))
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(12.0))
                    .child(self.ai_settings_select_control(
                        SettingsSelect::AiEmbeddingProvider,
                        provider_label,
                        224.0,
                        cx,
                    ))
                    .child(self.settings_text_input_control(
                        SettingsInput::AiEmbeddingModel,
                        self.current_settings_input_value(SettingsInput::AiEmbeddingModel),
                        self.i18n.t("settings_view.ai.embedding_model"),
                        224.0,
                        cx,
                    )),
            )
            .into_any_element()
    }

}
